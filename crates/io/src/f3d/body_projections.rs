//! Circular and rectangular extrusion-edge selectors verified with independent top, bottom and
//! overlapping-body controls. Unsupported recipes never use a nearest-edge fallback.
use super::history::{empty, finish, payload, references, require_class, zeros};
use super::*;

pub(super) const SELECTOR: &str = "5662F619-2281-4AD9-93DC-1FCFDA1062E7";
const IDENTITY: &str = "7ACC2A03-0261-4879-A14A-A93D661A5BDC";
const HANDLE: &str = "D26351F0-5940-4D23-AA20-2C35475A6D9E";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Recipe {
    pub operation: u64,
    pub end: bool,
    pub side: Option<u32>,
}

#[derive(Debug)]
pub(super) struct Edge {
    pub body: u64,
    pub feature: u64,
    pub recipe: Recipe,
    pub line: Option<[solvecraft_geom::Vec3; 2]>,
}

pub(super) struct Context<'a> {
    pub native: &'a super::projections::Geometry<'a>,
    pub profiles: &'a BTreeMap<u64, &'a SketchProfile>,
    pub doc: &'a solvecraft_doc::Document,
    pub model: &'a solvecraft_doc::ModelState,
    pub features: &'a BTreeMap<u64, u64>,
    pub operations: &'a BTreeMap<u64, u64>,
}

impl Context<'_> {
    fn rectangle_curve(&self, edge: &Edge, plane: solvecraft_geom::Plane, height: f64) -> Result<u64> {
        use solvecraft_geom::{Vec2, Vec3};
        let profile = self.profiles.get(&edge.feature).ok_or_else(|| invalid("missing native body profile"))?;
        let [boundary] = profile.loops.as_slice() else { return Err(invalid("projected rectangle cannot contain holes")) };
        if !boundary.outer || boundary.curves.len() != 4 {
            return Err(invalid("unsupported projected rectangle boundary"));
        }
        let cached = edge.line.ok_or_else(|| invalid("missing native finite edge geometry"))?;
        let world = |p: [f64; 3]| plane.to_world(Vec2::new(p[0] * 10., p[1] * 10.)) + plane.normal() * height;
        let same = |a: Vec3, b: Vec3| (a - b).len() < 1e-5;
        let mut matching = Vec::new();
        let mut sides = Vec::new();
        for id in &boundary.curves {
            let line = self.native.lines.get(id).filter(|l| l.sketch == profile.geometry).ok_or_else(|| invalid("missing rectangle side"))?;
            if line.start[2].abs() > 1e-10 || line.end[2].abs() > 1e-10 {
                return Err(invalid("nonplanar rectangle"));
            }
            let a = world(line.start);
            let b = world(line.end);
            if (a - b).len() < 1e-5 {
                return Err(invalid("degenerate rectangle side"));
            }
            if (same(a, cached[0]) && same(b, cached[1])) || (same(a, cached[1]) && same(b, cached[0])) {
                matching.push(*id);
            }
            sides.push((a, b));
        }
        // Profile traversal is not stable across archives. Match the complete
        // finite 3D edge to exactly one side of this explicitly owned profile;
        // never select a nearest edge or use the cached curve as imported geometry.
        for (i, (a, b)) in sides.iter().enumerate() {
            for vertex in [a, b] {
                let adjacent: Vec<_> = sides.iter().enumerate().filter(|(j, (c, d))| *j != i && (same(*vertex, *c) || same(*vertex, *d))).collect();
                let [(_, (c, d))] = adjacent.as_slice() else { return Err(invalid("rectangle endpoint incidence is ambiguous")) };
                if (*b - *a).dot(*d - *c).abs() > 1e-9 * (*b - *a).len() * (*d - *c).len() {
                    return Err(invalid("body profile is not a rectangle"));
                }
            }
        }
        let [id] = matching.as_slice() else { return Err(invalid("native edge does not uniquely match its source rectangle")) };
        Ok(*id)
    }

    fn attach_line(
        &self,
        projection: &super::projections::Projection,
        geometry: u64,
        plane: solvecraft_geom::Plane,
        sketch: &mut solvecraft_doc::sketch::Sketch,
        selection: (&str, &[String], u64),
    ) -> Result<super::projections::Entities> {
        let (body, selected, source_id) = selection;
        use solvecraft_doc::sketch::{LinkGeom, LinkKind, LinkSource};
        use solvecraft_geom::{Vec2, Vec3};
        let [name] = selected else { return Err(invalid("rectangular side requires exactly one named edge")) };
        let resolved = solvecraft_doc::project::resolve_named_edge(
            self.model,
            &plane,
            LinkKind::Project,
            &LinkSource::Edge { body: body.into(), at: Vec3::ZERO },
            name,
        )?;
        let [LinkGeom::Line(a, b)] = resolved.geom.as_slice() else { return Err(invalid("projected side is not a line")) };
        let line = self
            .native
            .lines
            .get(&projection.target)
            .filter(|l| l.sketch == geometry && l.reference)
            .ok_or_else(|| invalid("missing native projected line"))?;
        let super::projections::Source::Body(edge) = &projection.source else { return Err(invalid("missing projected body")) };
        let source = self.native.lines.get(&source_id).ok_or_else(|| invalid("missing native source line"))?;
        let feature = self.features.get(&edge.feature).and_then(|id| self.doc.feature(*id)).ok_or_else(|| invalid("missing producer"))?;
        let solvecraft_doc::FeatureKind::Extrude { sketch: source_sketch, .. } = feature.kind else { return Err(invalid("missing source sketch")) };
        let source_plane = self.model.sketch(source_sketch).ok_or_else(|| invalid("missing source plane"))?.plane;
        let projected = |p: [f64; 3]| plane.to_local(source_plane.to_world(Vec2::new(p[0] * 10., p[1] * 10.)));
        let source_points = [projected(source.start), projected(source.end)];
        let target_points = [Vec2::new(line.start[0] * 10., line.start[1] * 10.), Vec2::new(line.end[0] * 10., line.end[1] * 10.)];
        let native_ids = [line.start_point, line.end_point];
        for (id, expected) in native_ids.iter().zip([line.start, line.end]) {
            let p = self
                .native
                .points
                .get(id)
                .filter(|p| p.sketch == geometry && p.linked && !p.origin)
                .ok_or_else(|| invalid("missing linked rectangle endpoint"))?;
            if expected[2].abs() > 1e-10 || p.position.iter().zip(expected).any(|(a, b)| (a - b).abs() > 1e-9) {
                return Err(invalid("inconsistent projected line endpoint"));
            }
        }
        let mut endpoints = Vec::new();
        for point in [a, b] {
            let matching = |values: &[Vec2]| -> Result<usize> {
                let mut matches = values.iter().enumerate().filter(|(_, p)| point.dist(**p) < 1e-5);
                let i = matches
                    .next()
                    .ok_or_else(|| invalid(&format!("projected line disagrees with source body: {name}, {point:?}, expected {values:?}")))?
                    .0;
                if matches.next().is_some() {
                    return Err(invalid("ambiguous projected endpoint"));
                }
                Ok(i)
            };
            let target = matching(&target_points)?;
            let source_index = matching(&source_points)?;
            let id = *native_ids.get(target).ok_or_else(|| invalid("missing target endpoint"))?;
            let source_ids = [source.start_point, source.end_point];
            endpoints.push((id, *source_ids.get(source_index).ok_or_else(|| invalid("missing source endpoint"))?));
        }
        if endpoints.first() == endpoints.get(1) {
            return Err(invalid("degenerate projected line"));
        }
        let link = sketch.add_link(LinkKind::Project, resolved.source, &resolved.geom).map_err(|e| invalid(&e.to_string()))?;
        let points = sketch.link_points(&link);
        let curves = sketch.link_curves(&link);
        let [curve] = curves.as_slice() else { return Err(invalid("unexpected projected side entities")) };
        if points.len() != 2 {
            return Err(invalid("unexpected projected endpoints"));
        }
        sketch.links.iter_mut().find(|l| l.id == link).ok_or_else(|| invalid("missing edge link"))?.edge_name = Some(name.clone());
        let mut entities = Vec::new();
        for ((id, source), index) in endpoints.into_iter().zip(points) {
            sketch.points.get_mut(index).ok_or_else(|| invalid("missing projected endpoint"))?.id = format!("p{id}");
            entities.push((id, index, source));
        }
        let c = sketch.curves.get_mut(*curve).ok_or_else(|| invalid("missing projected line"))?;
        c.id = format!("c{}", projection.target);
        c.construction = line.construction;
        c.centerline = line.centerline;
        c.fixed = line.fixed;
        Ok(super::projections::Entities { points: entities, curve: (projection.target, *curve) })
    }

    pub fn attach(
        &self,
        projection: &super::projections::Projection,
        geometry: u64,
        plane: solvecraft_geom::Plane,
        sketch: &mut solvecraft_doc::sketch::Sketch,
    ) -> Result<super::projections::Entities> {
        use solvecraft_doc::sketch::{CurveKind, LinkGeom, LinkKind};
        use solvecraft_doc::{Direction, FeatureKind, Operation, ProfileSel, expr::Kind};
        let super::projections::Source::Body(edge) = &projection.source else { return Err(invalid("expected body-edge projection")) };
        if self.operations.get(&edge.recipe.operation) != Some(&edge.feature) {
            return Err(invalid("body-edge operation disagrees with its producer"));
        }
        let id = self.features.get(&edge.feature).copied().ok_or_else(|| invalid("projection precedes its source extrusion"))?;
        let feature = self.doc.feature(id).filter(|f| !f.suppressed).ok_or_else(|| invalid("missing projected extrusion"))?;
        let FeatureKind::Extrude {
            sketch: source_sketch,
            profiles: ProfileSel::WholeBoundaries { profiles },
            extent,
            operation: Operation::NewBody,
            ..
        } = &feature.kind
        else {
            return Err(invalid("unsupported projected body producer"));
        };
        let [profile] = profiles.as_slice() else { return Err(invalid("circular edge requires one source profile")) };
        let source = self.model.sketch(*source_sketch).ok_or_else(|| invalid("missing evaluated source sketch"))?;
        let native_curve = if edge.recipe.side.is_some() {
            Some(self.rectangle_curve(edge, source.plane, if edge.recipe.end { self.doc.eval(&extent.distance, Kind::Length)? } else { 0. })?)
        } else {
            None
        };
        let curve = if let Some(curve) = native_curve {
            format!("c{curve}")
        } else {
            let [curve] = profile.outer.as_slice() else { return Err(invalid("circular edge requires one source circle")) };
            curve.clone()
        };
        if !profile.holes.is_empty()
            || extent.start_offset.is_some()
            || extent.direction != Direction::Positive
            || self.doc.eval(&extent.distance, Kind::Length)? <= 0.
            || extent.taper.as_ref().map(|e| self.doc.eval(e, Kind::Angle)).transpose()?.is_some_and(|angle| angle.abs() > 1e-12)
        {
            return Err(invalid("unsupported projected circular extrusion extent"));
        }
        let curve_index = source.sketch.curve_index(&curve).ok_or_else(|| invalid("missing projected source profile curve"))?;
        let kind = source.sketch.curves.get(curve_index).map(|c| &c.kind);
        if !matches!((kind, edge.recipe.side), (Some(CurveKind::Circle { .. }), None) | (Some(CurveKind::Line { .. }), Some(_)))
            || source.plane.normal().cross(plane.normal()).len() > 1e-9
        {
            return Err(invalid("unsupported projected circle frame or source"));
        }
        let [body] = feature.body_names.as_slice() else { return Err(invalid("circular edge requires one produced body")) };
        let target = self.model.body(body).filter(|b| b.feature == id).ok_or_else(|| invalid("projected body has changed producer"))?;
        let side = format!("F{id}:side:{curve}");
        let cap = format!("F{id}:{}", if edge.recipe.end { "top" } else { "start" });
        let selected: Vec<_> = solvecraft_doc::naming::edge_names(target)
            .into_iter()
            .filter(|name| {
                let faces: BTreeSet<_> = name.split('|').map(solvecraft_doc::naming::strip_piece).collect();
                faces == BTreeSet::from([side.as_str(), cap.as_str()])
            })
            .collect();
        if let Some(curve) = native_curve {
            return self.attach_line(projection, geometry, plane, sketch, (body, &selected, curve));
        }
        let resolved = solvecraft_doc::project::resolve_edge_circle(self.model, &plane, body, &selected)?;
        let [LinkGeom::Circle(center, radius)] = resolved.geom.as_slice() else { return Err(invalid("projected cap is not a circle")) };
        let circle = self
            .native
            .circles
            .get(&projection.target)
            .filter(|c| c.sketch == geometry && c.reference && c.normal == [0., 0., 1.])
            .ok_or_else(|| invalid("missing native projected circle"))?;
        let point = self
            .native
            .points
            .get(&circle.center_point)
            .filter(|p| p.sketch == geometry && p.linked && !p.origin)
            .ok_or_else(|| invalid("missing linked circle center"))?;
        let expected = solvecraft_geom::Vec2::new(circle.center[0] * 10., circle.center[1] * 10.);
        if center.dist(expected) > 1e-5
            || (radius - circle.radius * 10.).abs() > 1e-5
            || circle.center[2].abs() > 1e-10
            || point.position.iter().zip(circle.center).any(|(a, b)| (a - b).abs() > 1e-9)
        {
            return Err(invalid("native projected circle disagrees with its source body"));
        }
        let link = sketch.add_link(LinkKind::Project, resolved.source, &resolved.geom).map_err(|e| invalid(&e.to_string()))?;
        let points = sketch.link_points(&link);
        let curves = sketch.link_curves(&link);
        let ([point], [curve]) = (points.as_slice(), curves.as_slice()) else { return Err(invalid("unexpected projected circle entities")) };
        sketch.links.iter_mut().find(|l| l.id == link).ok_or_else(|| invalid("missing circular edge link"))?.edge_circle = selected;
        sketch.points.get_mut(*point).ok_or_else(|| invalid("missing projected center"))?.id = format!("p{}", circle.center_point);
        let c = sketch.curves.get_mut(*curve).ok_or_else(|| invalid("missing projected circle"))?;
        c.id = format!("c{}", projection.target);
        c.construction = circle.construction;
        c.fixed = circle.fixed;
        Ok(super::projections::Entities {
            points: vec![(circle.center_point, *point, if edge.recipe.end { 2 } else { 1 })],
            curve: (projection.target, *curve),
        })
    }
}

pub(super) fn identity(records: &Records<'_>, classes: &BTreeMap<u64, &Class>, id: u64, tracked: bool) -> Result<Recipe> {
    require_class(classes, id, IDENTITY, 3)?;
    let mut r = payload(records.primary(id)?, id)?;
    empty(&mut r)?;
    zeros(&mut r, 2)?;
    if [r.u32()?, r.u32()?] != [1, 3] {
        return Err(invalid("unsupported body-edge identity structure"));
    }
    let count = r.u32()?;
    if count != 3 && count != 4 {
        return Err(invalid("unsupported body-edge term count"));
    }
    let mut terms = Vec::new();
    for _ in 0..count {
        if r.u32()? != 1 {
            return Err(invalid("unsupported body-edge term count"));
        }
        let label = r.string()?;
        zeros(&mut r, 4)?;
        if r.u32()? != 1 {
            return Err(invalid("unsupported body-edge operation count"));
        }
        terms.push((label, r.u64()?));
    }
    let operation = terms.first().ok_or_else(|| invalid("missing body-edge terms"))?.1;
    if operation == 0 || terms.iter().any(|(_, tag)| *tag != operation) {
        return Err(invalid("unsupported body-edge ownership"));
    }
    let labels: Vec<_> = terms.iter().map(|(label, _)| label.as_str()).collect();
    let (end, side) = match labels.as_slice() {
        ["-1", "3", "2"] => (true, None),
        ["-102", "3", "1"] => (false, None),
        [side, cap @ ("1" | "2"), previous, next] => {
            let side = side.parse::<u32>().map_err(|_| invalid("invalid side-face label"))?;
            if !(3..=6).contains(&side) || *previous != (3 + (side - 3 + 3) % 4).to_string() || *next != (3 + (side - 3 + 1) % 4).to_string() {
                return Err(invalid("unsupported rectangular edge adjacency"));
            }
            (*cap == "2", Some(side))
        }
        _ => return Err(invalid("unsupported extrusion edge role")),
    };
    zeros(&mut r, 4)?;
    if r.string()? != "edge_recipe_data" {
        return Err(invalid("missing body-edge recipe"));
    }
    if r.u32()? != if side.is_some() { u32::MAX } else { 0 } {
        return Err(invalid("unsupported body-edge recipe framing"));
    }
    let expected = recipe_tree(end, side);
    for expected in expected {
        if r.u32()? != expected as u32 {
            return Err(invalid("unsupported body-edge recipe tree"));
        }
    }
    if r.u32()? != if tracked { u32::MAX } else { 0 } {
        return Err(invalid("unsupported body-edge tracking role"));
    }
    finish(&r)?;
    Ok(Recipe { operation, end, side })
}

// The finite rectangular cap edges have two endpoint incidence branches.
// At the first/last side the cyclic boundary wraps, changing the term indices.
fn recipe_tree(end: bool, side: Option<u32>) -> Vec<i32> {
    if let Some(side) = side {
        let (first, last) = (side == 3, side == 6);
        let a = if first { 0 } else { 1 };
        let b = if first { 1 } else { 2 };
        let c = if last { 3 } else { 1 };
        let d = if last { 1 } else { 0 };
        let (a, b) = if end { (a, b) } else { (b, a) };
        let (c, d) = if end {
            (c, d)
        } else if last {
            (1, 3)
        } else {
            (d, c)
        };
        vec![
            -1,
            2,
            0,
            -1,
            1,
            -1,
            2,
            -1,
            3,
            if first { 2 } else { 0 },
            -1,
            a,
            -1,
            b,
            -1,
            0,
            0,
            -1,
            3,
            if last { 0 } else { 3 },
            -1,
            c,
            -1,
            d,
            -1,
            0,
            0,
        ]
    } else {
        let mut values = vec![-1, 2, 1, -1, 2, -1, 1, -1, 2, 1, -1, 2, -1, 1, 0, -1, 0, 0, -1, 1, 0];
        values.extend(if end { [2, 2, 2, 2, 1, 0, 1] } else { [2, 1, 1, 1, 2, 1, 2] });
        values
    }
}

fn cached_line(records: &Records<'_>, classes: &BTreeMap<u64, &Class>, id: u64, recipe: Recipe) -> Result<[solvecraft_geom::Vec3; 2]> {
    use solvecraft_geom::Vec3;
    let mut r = payload(records.primary(id)?, id)?;
    empty(&mut r)?;
    zeros(&mut r, 2)?;
    let mut vector = || -> Result<Vec3> {
        let v = Vec3::new(f64::from_bits(r.u64()?), f64::from_bits(r.u64()?), f64::from_bits(r.u64()?)) * 10.;
        if !v.is_finite() {
            return Err(invalid("non-finite native edge"));
        }
        Ok(v)
    };
    let start = vector()?;
    let delta = vector()?;
    if delta.len() < 1e-5 || !(start + delta).is_finite() {
        return Err(invalid("degenerate native edge"));
    }
    zeros(&mut r, 1)?;
    require_class(classes, r.reference()?, "C46D3EEB-40E1-43FA-83DC-9CB204335417", 1)?;
    if r.take(2)? != [0, 1] {
        return Err(invalid("unsupported native finite-edge tracking"));
    }
    let tracker = r.reference()?;
    finish(&r)?;
    let identity_id = super::projections::tracking_identity(records, classes, tracker, "4B31DA1B-FCD6-4527-BC3E-BE1FBAF867FE", 1)?;
    if identity(records, classes, identity_id, true)? != recipe {
        return Err(invalid("cached edge tracking disagrees with its selector"));
    }
    Ok([start, start + delta])
}

pub(super) fn selector(records: &Records<'_>, classes: &BTreeMap<u64, &Class>, id: u64, owner: u64) -> Result<Edge> {
    require_class(classes, id, SELECTOR, 3)?;
    let mut r = payload(records.primary(id)?, id)?;
    empty(&mut r)?;
    zeros(&mut r, 3)?;
    let end = match r.take(1)? {
        [0] => false,
        [1] => true,
        _ => return Err(invalid("unsupported body-edge selector orientation")),
    };
    let recipe = identity(records, classes, r.reference()?, false)?;
    if end != recipe.end || r.u32()? != 1 {
        return Err(invalid("inconsistent body-edge selector role"));
    }
    r.wide_string()?;
    r.wide_string()?;
    if r.u32()? != 2 {
        return Err(invalid("unsupported body-edge selection type"));
    }
    zeros(&mut r, 4)?;
    let cached = references(&mut r)?;
    let [cached] = cached.as_slice() else { return Err(invalid("circular edge requires one cached curve")) };
    require_class(
        classes,
        *cached,
        if recipe.side.is_some() { "50AEE8B4-9456-4DD7-847D-6F6C909BBB9D" } else { "B0B3FA59-C374-444F-B424-24AA5DE41645" },
        0,
    )?;
    let line = if recipe.side.is_some() { Some(cached_line(records, classes, *cached, recipe)?) } else { None };
    let handles = references(&mut r)?;
    let [handle] = handles.as_slice() else { return Err(invalid("body edge requires one body handle")) };
    r.take(56)?; // inherited evaluation metadata
    if r.reference()? != owner {
        return Err(invalid("body-edge selector belongs to another feature"));
    }
    finish(&r)?;
    require_class(classes, *handle, HANDLE, 2)?;
    let mut r = payload(records.primary(*handle)?, *handle)?;
    empty(&mut r)?;
    zeros(&mut r, 2)?;
    if r.wide_string()?.is_empty() || r.take(2)? != [0, 1] {
        return Err(invalid("unsupported body-handle identity"));
    }
    let body = r.reference()?;
    let feature = r.reference()?;
    finish(&r)?;
    require_class(classes, body, "D3937028-C20C-4E65-B010-94AD418A5C20", 19)?;
    require_class(classes, feature, "DD405BC2-D673-44F0-8833-5CB2A1C186C7", 8)?;
    let mut r = payload(records.secondary(*handle)?, *handle)?;
    empty(&mut r)?;
    let root = r.reference()?;
    require_class(classes, root, "C3423E04-7E85-4B44-8E31-A5FA9C100052", 2)?;
    finish(&r)?;
    // Verify the reverse relationship on the producing extrusion. The second
    // role list is the single new-body output handle in the observed schema.
    let mut r = payload(records.primary(feature)?, feature)?;
    let input = super::history::extrude_inputs(&mut r, classes)?;
    if input.operation != solvecraft_doc::Operation::NewBody || input.start_offset.is_some() {
        return Err(invalid("body-edge import requires an independent profile-plane extrusion"));
    }
    references(&mut r)?;
    if !references(&mut r)?.is_empty() || references(&mut r)? != handles || !references(&mut r)?.is_empty() || !references(&mut r)?.is_empty() {
        return Err(invalid("body handle disagrees with extrusion outputs"));
    }
    Ok(Edge { body, feature, recipe, line })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::f3d::import_tests::{Fixture, fixture, volume};
    use crate::f3d::relationship_tests::{reference, refs, string, u32, u64, wide};

    fn body_circle(end: bool) -> Fixture {
        let mut f = super::super::projection_tests::projected_circle();
        let tracking = f.0.get_mut(&184).unwrap();
        tracking.0 = "4B31DA1B-FCD6-4527-BC3E-BE1FBAF867FE".into();
        tracking.1 = 1;
        let mut b = vec![0; 6];
        reference(&mut b, 4);
        refs(&mut b, &[8, 9, 18]);
        f.0.get_mut(&5).unwrap().2 = b;
        f.0.insert(100, fixture(false).0.remove(&100).unwrap());
        let mut output = Vec::new();
        refs(&mut output, &[186]);
        f.0.get_mut(&9).unwrap().2.splice(151..155, output);
        let mut b = vec![0; 7];
        b.push(u8::from(end));
        reference(&mut b, 185);
        u32(&mut b, 1);
        wide(&mut b, "original-selector");
        wide(&mut b, "original-context");
        u32(&mut b, 2);
        u32(&mut b, 0);
        refs(&mut b, &[187]);
        refs(&mut b, &[186]);
        b.extend([0; 56]);
        reference(&mut b, 18);
        f.put(183, SELECTOR, 3, b, None);
        let mut b = vec![0; 6];
        for n in [1, 3, 3] {
            u32(&mut b, n);
        }
        for label in if end { ["-1", "3", "2"] } else { ["-102", "3", "1"] } {
            u32(&mut b, 1);
            string(&mut b, label);
            u32(&mut b, 0);
            u32(&mut b, 1);
            u64(&mut b, 8001);
        }
        u32(&mut b, 0);
        string(&mut b, "edge_recipe_data");
        u32(&mut b, 0);
        for n in [-1_i32, 2, 1, -1, 2, -1, 1, -1, 2, 1, -1, 2, -1, 1, 0, -1, 0, 0, -1, 1, 0].into_iter().chain(if end {
            [2, 2, 2, 2, 1, 0, 1, 0]
        } else {
            [2, 1, 1, 1, 2, 1, 2, 0]
        }) {
            u32(&mut b, n as u32);
        }
        f.put(185, IDENTITY, 3, b.clone(), None);
        let n = b.len();
        b[n - 4..].copy_from_slice(&u32::MAX.to_le_bytes());
        f.put(188, IDENTITY, 3, b, None);
        let mut b = vec![0; 6];
        reference(&mut b, 188);
        b.push(0);
        f.0.get_mut(&184).unwrap().2 = b;
        let mut b = vec![0; 6];
        wide(&mut b, "original-body");
        b.extend([0, 1]);
        reference(&mut b, 209);
        reference(&mut b, 9);
        let mut secondary = vec![0; 4];
        reference(&mut secondary, 4);
        f.put(186, HANDLE, 2, b, Some(secondary));
        f.put(187, "B0B3FA59-C374-444F-B424-24AA5DE41645", 0, vec![], None);
        f.put(209, "D3937028-C20C-4E65-B010-94AD418A5C20", 19, vec![], None);
        let mut b = vec![0; 6];
        u32(&mut b, 1);
        u64(&mut b, 9);
        u32(&mut b, 1);
        u64(&mut b, 8001);
        u32(&mut b, 1);
        u64(&mut b, 8001);
        u64(&mut b, 9);
        u32(&mut b, 0);
        reference(&mut b, 4);
        f.put(280, "5D89B935-3F04-4CAB-81F5-E7B309E7E13F", 1, b, None);
        f
    }

    #[test]
    fn rectangular_recipe_roles_validate_full_records_and_reject_damage() {
        for end in [false, true] {
            for side in 3..=6 {
                for tracked in [false, true] {
                    let mut b = vec![0; 6];
                    for n in [1, 3, 4] {
                        u32(&mut b, n);
                    }
                    for label in [side, if end { 2 } else { 1 }, 3 + (side - 3 + 3) % 4, 3 + (side - 3 + 1) % 4] {
                        u32(&mut b, 1);
                        string(&mut b, &label.to_string());
                        u32(&mut b, 0);
                        u32(&mut b, 1);
                        u64(&mut b, 9001);
                    }
                    u32(&mut b, 0);
                    string(&mut b, "edge_recipe_data");
                    u32(&mut b, u32::MAX);
                    // Independently authored forms for the two endpoints. A
                    // malformed tree must never be accepted as a label-only match.
                    let first = match (end, side) {
                        (true, 3) => [2, 0, 1],
                        (false, 3) => [2, 1, 0],
                        (true, _) => [0, 1, 2],
                        (false, _) => [0, 2, 1],
                    };
                    let last = match (end, side) {
                        (true, 6) => [0, 3, 1],
                        (false, 6) => [0, 1, 3],
                        (true, _) => [3, 1, 0],
                        (false, _) => [3, 0, 1],
                    };
                    for n in [
                        -1, 2, 0, -1, 1, -1, 2, -1, 3, first[0], -1, first[1], -1, first[2], -1, 0, 0, -1, 3, last[0], -1, last[1], -1, last[2], -1,
                        0, 0,
                    ] {
                        u32(&mut b, n as u32);
                    }
                    u32(&mut b, if tracked { u32::MAX } else { 0 });
                    let check = |b: Vec<u8>| {
                        let mut f = Fixture(BTreeMap::new());
                        f.put(1, IDENTITY, 3, b, None);
                        let d = f.design();
                        let records = Records::new(&d);
                        let classes = d.classes.iter().flat_map(|c| c.objects.iter().map(move |i| (*i, c))).collect();
                        identity(&records, &classes, 1, tracked)
                    };
                    assert_eq!(check(b.clone()).unwrap(), Recipe { operation: 9001, end, side: Some(side) });
                    for keep in 0..b.len() {
                        assert!(check(b[..keep].to_vec()).is_err(), "side {side} prefix {keep}");
                    }
                    // Header, adjacency, ownership, framing, tree and role sentinel.
                    for at in [14, 26, 36, 51, 61, 76, 86, 101, 111, 142, 146, 182, 254] {
                        let mut bad = b.clone();
                        bad[at] ^= 0x40;
                        assert!(check(bad).is_err(), "side {side}, byte {at}");
                    }
                }
            }
        }
    }

    #[test]
    fn finite_edge_cache_requires_matching_tracking_and_finite_complete_geometry() {
        let mut records = Fixture(BTreeMap::new());
        let mut identity = vec![0; 6];
        for n in [1, 3, 4] {
            u32(&mut identity, n);
        }
        for label in ["3", "2", "6", "4"] {
            u32(&mut identity, 1);
            string(&mut identity, label);
            u32(&mut identity, 0);
            u32(&mut identity, 1);
            u64(&mut identity, 9001);
        }
        u32(&mut identity, 0);
        string(&mut identity, "edge_recipe_data");
        u32(&mut identity, u32::MAX);
        for n in [-1, 2, 0, -1, 1, -1, 2, -1, 3, 2, -1, 0, -1, 1, -1, 0, 0, -1, 3, 3, -1, 1, -1, 0, -1, 0, 0, -1] {
            u32(&mut identity, n as u32);
        }
        records.put(3, IDENTITY, 3, identity, None);
        let mut tracker = vec![0; 6];
        reference(&mut tracker, 3);
        tracker.push(0);
        records.put(2, "4B31DA1B-FCD6-4527-BC3E-BE1FBAF867FE", 1, tracker, None);
        records.put(4, "C46D3EEB-40E1-43FA-83DC-9CB204335417", 1, vec![], None);
        let mut b = vec![0; 6];
        for v in [0_f64, 2., 1.7, 0., -2., 0.] {
            b.extend(v.to_le_bytes());
        }
        b.push(0);
        reference(&mut b, 4);
        b.extend([0, 1]);
        reference(&mut b, 2);
        records.put(1, "50AEE8B4-9456-4DD7-847D-6F6C909BBB9D", 0, b.clone(), None);
        let check = |bytes: Vec<u8>, operation: u64| {
            let mut f = Fixture(records.0.clone());
            f.0.get_mut(&1).unwrap().2 = bytes;
            let d = f.design();
            let rs = Records::new(&d);
            let classes = d.classes.iter().flat_map(|c| c.objects.iter().map(move |id| (*id, c))).collect();
            cached_line(&rs, &classes, 1, Recipe { operation, end: true, side: Some(3) })
        };
        let points = check(b.clone(), 9001).unwrap();
        assert_eq!(points, [solvecraft_geom::Vec3::new(0., 20., 17.), solvecraft_geom::Vec3::new(0., 0., 17.)]);
        assert!(check(b.clone(), 9002).is_err());
        for keep in 0..b.len() {
            assert!(check(b[..keep].to_vec(), 9001).is_err(), "prefix {keep}");
        }
        for at in [6, 14, 22, 30, 38, 46] {
            let mut bad = b.clone();
            bad[at..at + 8].copy_from_slice(&f64::NAN.to_le_bytes());
            assert!(check(bad, 9001).is_err());
        }
        for at in [54, 55, 66, 67, 68] {
            let mut bad = b.clone();
            bad[at] ^= 0x40;
            assert!(check(bad, 9001).is_err());
        }
    }

    #[test]
    fn rectangular_edges_follow_native_profile_order_and_shared_corners() {
        use solvecraft_doc::{FeatureKind, PlaneRef, sketch::Sketch};
        for end in [false, true] {
            let design = super::super::polygon_tests::rectangle().design();
            let mut doc = design.to_document("Original projected rectangle").unwrap();
            let mut model = solvecraft_doc::Model::new();
            model.evaluate(&doc);
            let state = model.state();
            let mut profiles = design.sketch_profiles().unwrap();
            // A cyclic order different from both creation and sorted id order.
            profiles[0].loops[0].curves = vec![32, 31, 34, 33];
            let profiles = BTreeMap::from([(9, &profiles[0])]);
            let mut points: BTreeMap<_, _> = design.sketch_points().unwrap().into_iter().map(|p| (p.id, p)).collect();
            for mut p in design.sketch_points().unwrap() {
                p.id += 1000;
                p.sketch = 170;
                p.linked = true;
                p.origin = false;
                points.insert(p.id, p);
            }
            let mut lines: BTreeMap<_, _> = design.sketch_lines().unwrap().into_iter().map(|l| (l.id, l)).collect();
            for mut l in design.sketch_lines().unwrap() {
                l.id += 1000;
                l.sketch = 170;
                l.start_point += 1000;
                l.end_point += 1000;
                l.reference = true;
                lines.insert(l.id, l);
            }
            let native = super::super::projections::Geometry { points: &points, lines: &lines, circles: &BTreeMap::new(), arcs: &BTreeMap::new() };
            let context = Context {
                native: &native,
                profiles: &profiles,
                doc: &doc,
                model: &state,
                features: &BTreeMap::from([(9, 2)]),
                operations: &BTreeMap::from([(9001, 9)]),
            };
            let mut sketch = Sketch::new();
            let mut corners = BTreeMap::new();
            for (i, curve) in [34, 31, 32, 33].into_iter().enumerate() {
                let projection = super::super::projections::Projection {
                    target: curve + 1000,
                    source: super::super::projections::Source::Body(Edge {
                        body: 999,
                        feature: 9,
                        recipe: Recipe { operation: 9001, end, side: Some(3 + i as u32) },
                        line: Some({
                            let line = &lines[&curve];
                            let world = |p: [f64; 3]| solvecraft_geom::Vec3::new(p[0] * 10., p[1] * 10., if end { 17. } else { 0. });
                            [world(line.start), world(line.end)]
                        }),
                    }),
                };
                let entities = context.attach(&projection, 170, solvecraft_geom::Plane::XY, &mut sketch).unwrap();
                for (id, index, key) in entities.points {
                    if let Some(previous) = corners.insert(id, key) {
                        assert_eq!(previous, key);
                        sketch.points[index].id = format!("p{id}:alias{curve}");
                    }
                }
            }
            assert_eq!(corners.len(), 4);
            let id =
                doc.add_feature(FeatureKind::Sketch { plane: PlaneRef::Origin { name: "XY".into() }, sketch }, Some("Projected rectangle")).unwrap();
            let verify = |doc: &solvecraft_doc::Document, width: f64| {
                let mut m = solvecraft_doc::Model::new();
                m.evaluate(doc);
                assert!(m.results.iter().all(|r| r.error.is_none()), "{:?}", m.results);
                let st = m.state();
                let s = &st.sketch(id).unwrap().sketch;
                assert!(s.links.iter().all(|l| !l.lost));
                let max = s.points.iter().map(|p| p.pos.x).reduce(f64::max).unwrap();
                assert!((max - width).abs() < 1e-6);
            };
            verify(&doc, 30.);
            doc.change_param("width", "90 mm", None, None).unwrap();
            verify(&doc, 90.);
            let reopened = crate::read_design(&crate::write_design(&doc)).unwrap();
            verify(&reopened, 90.);
        }
    }

    #[test]
    fn native_body_circle_remains_one_curve_across_source_edits_and_reopening() {
        for end in [false, true] {
            let mut doc = body_circle(end).design().to_document("Original body projection").unwrap();
            let verify = |doc: &solvecraft_doc::Document, radius: f64| {
                let mut model = solvecraft_doc::Model::new();
                model.evaluate(doc);
                assert!(model.results.iter().all(|r| r.error.is_none()), "{:?}", model.results);
                let state = model.state();
                let projected = state.sketch(3).unwrap();
                assert_eq!(projected.sketch.curves.len(), 1);
                assert_eq!(projected.sketch.curves[0].id, "c131");
                assert!(matches!(projected.sketch.curves[0].kind, solvecraft_doc::sketch::CurveKind::Circle {r,..} if (r-radius).abs()<1e-6));
                assert!(!projected.sketch.links[0].lost);
                assert_eq!(projected.sketch.links[0].edge_circle.len(), 2);
                assert!(projected.sketch.links[0].edge_circle.iter().all(|n| n.contains(if end { "F2:top" } else { "F2:start" })));
            };
            verify(&doc, 30.);
            doc.change_param("d7", "120 mm", None, None).unwrap();
            verify(&doc, 60.);
            volume(&doc, 3600. * std::f64::consts::PI * 17.);
            let mut reopened = crate::read_design(&crate::write_design(&doc)).unwrap();
            assert_eq!(reopened, doc);
            reopened.change_param("d7", "40 mm", None, None).unwrap();
            verify(&reopened, 20.);
        }
    }

    #[test]
    fn invalid_body_selectors_never_use_the_cached_circle() {
        for id in [183, 184, 185, 186, 188, 280] {
            let n = body_circle(true).0[&id].2.len();
            for keep in 0..n {
                let mut f = body_circle(true);
                f.0.get_mut(&id).unwrap().2.truncate(keep);
                assert!(f.design().to_document("Truncated body projection").is_err(), "record {id}, prefix {keep}");
            }
        }
        for (id, at, value) in [(185, 36, 8002_u64), (185, 61, 8002), (185, 86, 8002), (280, 42, 8)] {
            let mut f = body_circle(true);
            f.0.get_mut(&id).unwrap().2[at..at + 8].copy_from_slice(&value.to_le_bytes());
            assert!(f.design().to_document("Conflicting body projection").is_err());
        }
    }
}
