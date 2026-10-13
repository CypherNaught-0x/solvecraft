//! Sketch-curve projection relationships verified with linked/broken API controls.
use super::history::{empty, finish, payload, references, require_class, zeros};
use super::*;

pub(super) const CLASS: &str = "F1451EC8-11A4-4952-84ED-5AB7C13E2E83";

pub(super) struct Projection {
    pub target: u64,
    pub source: Source,
}

pub(super) enum Source {
    Sketch { geometry: u64, tag: u64 },
    Body(super::body_projections::Edge),
}

pub(super) struct Geometry<'a> {
    pub points: &'a BTreeMap<u64, SketchPoint>,
    pub lines: &'a BTreeMap<u64, SketchLine>,
    pub circles: &'a BTreeMap<u64, SketchCircle>,
    pub arcs: &'a BTreeMap<u64, SketchArc>,
}

pub(super) struct Entities {
    pub points: Vec<(u64, usize, u64)>,
    pub curve: (u64, usize),
}

impl Geometry<'_> {
    pub fn source(&self, p: &Projection) -> Result<u64> {
        let Source::Sketch { geometry, tag } = &p.source else { return Err(invalid("expected projected sketch source")) };
        let mut matching = self
            .lines
            .values()
            .map(|c| (c.id, c.sketch, c.profile_id))
            .chain(self.circles.values().map(|c| (c.id, c.sketch, c.profile_id)))
            .chain(self.arcs.values().map(|a| (a.circle.id, a.circle.sketch, a.circle.profile_id)))
            .filter(|(_, sketch, candidate)| sketch == geometry && candidate.is_some_and(|(candidate, _)| candidate == *tag));
        let source = matching.next().ok_or_else(|| invalid("missing projected source curve"))?.0;
        if matching.next().is_some() {
            return Err(invalid("ambiguous projected source curve tag"));
        }
        Ok(source)
    }

    pub fn attach(
        &self,
        p: &Projection,
        geometry: u64,
        source_feature: u64,
        planes: (solidcraft_geom::Plane, solidcraft_geom::Plane),
        sketch: &mut solidcraft_doc::sketch::Sketch,
    ) -> Result<Entities> {
        use solidcraft_doc::sketch::{LinkGeom, LinkKind, LinkSource};
        use solidcraft_geom::Vec2;
        let Source::Sketch { geometry: source_geometry, .. } = &p.source else { return Err(invalid("expected projected sketch source")) };
        let source = self.source(p)?;
        let point = |id, expected: [f64; 3]| -> Result<Vec2> {
            let v = self
                .points
                .get(&id)
                .filter(|v| v.sketch == geometry && v.linked && !v.origin)
                .ok_or_else(|| invalid("missing linked projection point"))?;
            if v.position[2].abs() > 1e-10 || v.position.iter().zip(expected).any(|(a, b)| (a - b).abs() > 1e-9) {
                return Err(invalid("inconsistent projected point geometry"));
            }
            Ok(Vec2::new(v.position[0] * 10., v.position[1] * 10.))
        };
        let (geom, point_ids, construction, centerline, fixed) = if let Some(c) = self.circles.get(&p.target) {
            if c.sketch != geometry || !c.reference || c.normal != [0., 0., 1.] {
                return Err(invalid("unsupported projected circle placement"));
            }
            (LinkGeom::Circle(point(c.center_point, c.center)?, c.radius * 10.), vec![c.center_point], c.construction, false, c.fixed)
        } else if let Some(l) = self.lines.get(&p.target) {
            if l.sketch != geometry || !l.reference {
                return Err(invalid("unsupported projected line placement"));
            }
            (
                LinkGeom::Line(point(l.start_point, l.start)?, point(l.end_point, l.end)?),
                vec![l.start_point, l.end_point],
                l.construction,
                l.centerline,
                l.fixed,
            )
        } else {
            return Err(invalid("unsupported projected curve type"));
        };
        let source_points = if let Some(c) = self.circles.get(&source) {
            if !self.circles.contains_key(&p.target) {
                return Err(invalid("projected curve changed type"));
            }
            let target = self.circles.get(&p.target).ok_or_else(|| invalid("missing projected circle"))?;
            if (c.radius - target.radius).abs() > 1e-9 || (planes.0.normal().dot(planes.1.normal()).abs() - 1.).abs() > 1e-9 {
                return Err(invalid("inconsistent projected circle support"));
            }
            vec![c.center_point]
        } else if let Some(l) = self.lines.get(&source) {
            if !self.lines.contains_key(&p.target) {
                return Err(invalid("projected curve changed type"));
            }
            vec![l.start_point, l.end_point]
        } else {
            return Err(invalid("unsupported projected source curve type"));
        };
        for (target, source) in point_ids.iter().zip(&source_points) {
            let source =
                self.points.get(source).filter(|v| v.sketch == *source_geometry).ok_or_else(|| invalid("missing projection source point"))?;
            let target = self.points.get(target).ok_or_else(|| invalid("missing projection target point"))?;
            let world = planes.0.to_world(Vec2::new(source.position[0] * 10., source.position[1] * 10.));
            let expected = planes.1.to_local(world);
            if source.position[2].abs() > 1e-10 || expected.dist(Vec2::new(target.position[0] * 10., target.position[1] * 10.)) > 1e-7 {
                return Err(invalid("projected geometry disagrees with its source"));
            }
        }
        let link = sketch
            .add_link(LinkKind::Project, LinkSource::SketchCurve { sketch: source_feature, curve: format!("c{source}") }, &[geom])
            .map_err(|e| invalid(&e.to_string()))?;
        let indices = sketch.link_points(&link);
        let curves = sketch.link_curves(&link);
        let [curve] = curves.as_slice() else { return Err(invalid("unexpected projected curve count")) };
        if indices.len() != point_ids.len() {
            return Err(invalid("unexpected projected point count"));
        }
        for (id, index) in point_ids.iter().zip(&indices) {
            let point = sketch.points.get_mut(*index).ok_or_else(|| invalid("missing projected point"))?;
            point.id = format!("p{id}");
        }
        let c = sketch.curves.get_mut(*curve).ok_or_else(|| invalid("missing projected curve"))?;
        c.id = format!("c{}", p.target);
        c.construction = construction;
        c.centerline = centerline;
        c.fixed = fixed;
        Ok(Entities {
            points: point_ids.into_iter().zip(indices).zip(source_points).map(|((id, index), source)| (id, index, source)).collect(),
            curve: (p.target, *curve),
        })
    }
}

pub(super) fn decode(records: &Records<'_>, classes: &BTreeMap<u64, &Class>, id: u64, owner: u64) -> Result<Projection> {
    require_class(classes, id, CLASS, 4)?;
    let mut r = payload(records.primary(id)?, id)?;
    empty(&mut r)?;
    zeros(&mut r, 2)?;
    r.take(3)?; // evaluation state
    if r.take(1)? != [1] {
        return Err(invalid("unsupported projection mapping flag"));
    }
    let mapping = r.reference()?;
    zeros(&mut r, 1)?;
    let tolerance = f64::from_bits(r.u64()?);
    if !tolerance.is_finite() || tolerance <= 0. || r.u32()? != 3 || r.reference()? != owner {
        return Err(invalid("unsupported sketch projection mode or owner"));
    }
    let container = r.reference()?;
    r.take(42)?; // evaluation metadata, distinct from geometric relationships
    if r.reference()? != owner {
        return Err(invalid("inconsistent projection owner"));
    }
    finish(&r)?;
    require_class(classes, mapping, "CA72DD8E-2861-4DDD-9721-03039B5FB113", 0)?;
    let mut r = payload(records.primary(mapping)?, mapping)?;
    empty(&mut r)?;
    zeros(&mut r, 2)?;
    if r.u32()? != 1 {
        return Err(invalid("multi-entity projection mapping is not supported yet"));
    }
    let tracking = r.reference()?;
    let target = r.u64()?;
    if r.u64()? != 1 {
        return Err(invalid("unsupported projection mapping state"));
    }
    finish(&r)?;
    require_class(classes, container, "2CA5A1CD-C99B-4C9B-91AF-57989148E841", 1)?;
    let mut r = payload(records.primary(container)?, container)?;
    empty(&mut r)?;
    zeros(&mut r, 2)?;
    let selected = references(&mut r)?;
    let [selected] = selected.as_slice() else { return Err(invalid("projection requires one source")) };
    // Both selectors carry their owning feature in the final reference.
    let tail = r.bytes.len().checked_sub(11).filter(|at| *at >= r.at).ok_or_else(|| invalid("truncated projection container"))?;
    r.at = tail;
    if r.reference()? != owner {
        return Err(invalid("projection container belongs to another feature"));
    }
    finish(&r)?;
    let source = if classes.get(selected).is_some_and(|c| c.id == super::body_projections::SELECTOR) {
        Source::Body(super::body_projections::selector(records, classes, *selected, owner)?)
    } else {
        require_class(classes, *selected, "5A1BF548-241F-46FD-9FB5-E4B05126EB9D", 1)?;
        let mut r = payload(records.primary(*selected)?, *selected)?;
        empty(&mut r)?;
        zeros(&mut r, 2)?;
        let identity = r.reference()?;
        let tail = r.bytes.len().checked_sub(11).filter(|at| *at >= r.at).ok_or_else(|| invalid("truncated projection selector"))?;
        r.at = tail;
        if r.reference()? != owner {
            return Err(invalid("projection selector belongs to another feature"));
        }
        finish(&r)?;
        let (geometry, tag) = source_identity(records, classes, identity)?;
        Source::Sketch { geometry, tag }
    };
    let (tracking_class, tracking_version) = match &source {
        Source::Sketch { .. } => ("CB4F4CC2-9D6F-48BA-9B1F-20C1992C9C9C", 2),
        Source::Body(_) => ("4B31DA1B-FCD6-4527-BC3E-BE1FBAF867FE", 1),
    };
    let tracked_identity = tracking_identity(records, classes, tracking, tracking_class, tracking_version)?;
    let agrees = match &source {
        Source::Sketch { geometry, tag } => source_identity(records, classes, tracked_identity)? == (*geometry, *tag),
        Source::Body(edge) => super::body_projections::identity(records, classes, tracked_identity, true)? == edge.recipe,
    };
    if !agrees {
        return Err(invalid("projection mapping disagrees with its source selector"));
    }
    Ok(Projection { target, source })
}

pub(super) fn tracking_identity(
    records: &Records<'_>,
    classes: &BTreeMap<u64, &Class>,
    tracking: u64,
    tracking_class: &str,
    tracking_version: u32,
) -> Result<u64> {
    require_class(classes, tracking, tracking_class, tracking_version)?;
    let mut r = payload(records.primary(tracking)?, tracking)?;
    empty(&mut r)?;
    zeros(&mut r, 1)?;
    match r.take(1)? {
        [0] => {}
        [1] => {
            let mut names = BTreeSet::new();
            for _ in 0..r.count(19)? {
                if !names.insert(r.string()?) || r.string()? != "StrongRefMetaType" {
                    return Err(invalid("unsupported projection tracking attributes"));
                }
                let related = r.reference()?;
                require_class(classes, related, tracking_class, tracking_version)?;
            }
        }
        _ => return Err(invalid("unsupported projection tracking metadata")),
    }
    let tracked_identity = r.reference()?;
    zeros(&mut r, 1)?;
    finish(&r)?;
    Ok(tracked_identity)
}

pub(super) fn source_identity(records: &Records<'_>, classes: &BTreeMap<u64, &Class>, id: u64) -> Result<(u64, u64)> {
    require_class(classes, id, "E2CEFD18-D755-4E09-8E7F-953A2F6D43F8", 0)?;
    let mut r = payload(records.primary(id)?, id)?;
    empty(&mut r)?;
    zeros(&mut r, 10)?;
    let source_geometry = r.u64()?;
    let source_tag = r.u64()?;
    require_class(classes, source_geometry, "44A64366-4BD3-4B24-881A-F94C206E8F2D", 18)?;
    finish(&r)?;
    Ok((source_geometry, source_tag))
}
