//! Circular extrusion-edge selectors verified with independent top, bottom and
//! overlapping-body controls. Unsupported recipes are never selected by position.
use super::history::{empty, finish, payload, references, require_class, zeros};
use super::*;

pub(super) const SELECTOR: &str = "5662F619-2281-4AD9-93DC-1FCFDA1062E7";
const IDENTITY: &str = "7ACC2A03-0261-4879-A14A-A93D661A5BDC";
const HANDLE: &str = "D26351F0-5940-4D23-AA20-2C35475A6D9E";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Recipe {
    pub operation: u64,
    pub end: bool,
}

#[derive(Debug)]
pub(super) struct Edge {
    pub body: u64,
    pub feature: u64,
    pub recipe: Recipe,
}

pub(super) struct Context<'a> {
    pub native: &'a super::projections::Geometry<'a>,
    pub doc: &'a solvecraft_doc::Document,
    pub model: &'a solvecraft_doc::ModelState,
    pub features: &'a BTreeMap<u64, u64>,
    pub operations: &'a BTreeMap<u64, u64>,
}

impl Context<'_> {
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
        let [curve] = profile.outer.as_slice() else { return Err(invalid("circular edge requires one source circle")) };
        if !profile.holes.is_empty()
            || extent.direction != Direction::Positive
            || self.doc.eval(&extent.distance, Kind::Length)? <= 0.
            || extent.taper.as_ref().map(|e| self.doc.eval(e, Kind::Angle)).transpose()?.is_some_and(|angle| angle.abs() > 1e-12)
        {
            return Err(invalid("unsupported projected circular extrusion extent"));
        }
        let source = self.model.sketch(*source_sketch).ok_or_else(|| invalid("missing evaluated source sketch"))?;
        let curve_index = source.sketch.curve_index(curve).ok_or_else(|| invalid("missing projected source profile curve"))?;
        if !matches!(source.sketch.curves.get(curve_index).map(|c| &c.kind), Some(CurveKind::Circle { .. }))
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
    if [r.u32()?, r.u32()?, r.u32()?] != [1, 3, 3] {
        return Err(invalid("unsupported body-edge identity structure"));
    }
    let mut terms = Vec::new();
    for _ in 0..3 {
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
    let [(first, operation), (side, same), (cap, also)] = terms.as_slice() else { return Err(invalid("missing body-edge terms")) };
    if *operation == 0 || operation != same || operation != also || side != "3" {
        return Err(invalid("unsupported body-edge ownership or side"));
    }
    let end = match (first.as_str(), cap.as_str()) {
        ("-1", "2") => true,
        ("-102", "1") => false,
        _ => return Err(invalid("unsupported circular extrusion edge role")),
    };
    zeros(&mut r, 4)?;
    if r.string()? != "edge_recipe_data" {
        return Err(invalid("missing body-edge recipe"));
    }
    zeros(&mut r, 4)?;
    // Verified recipe grammar for the two circular cap edges. Other operation
    // trees need their own interpretation, not a prefix match to these labels.
    let common: [i32; 21] = [-1, 2, 1, -1, 2, -1, 1, -1, 2, 1, -1, 2, -1, 1, 0, -1, 0, 0, -1, 1, 0];
    let role: [i32; 7] = if end { [2, 2, 2, 2, 1, 0, 1] } else { [2, 1, 1, 1, 2, 1, 2] };
    for expected in common.into_iter().chain(role) {
        if r.u32()? != expected as u32 {
            return Err(invalid("unsupported body-edge recipe tree"));
        }
    }
    if r.u32()? != if tracked { u32::MAX } else { 0 } {
        return Err(invalid("unsupported body-edge tracking role"));
    }
    finish(&r)?;
    Ok(Recipe { operation: *operation, end })
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
    require_class(classes, *cached, "B0B3FA59-C374-444F-B424-24AA5DE41645", 0)?;
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
    empty(&mut r)?;
    if r.take(2)? != [0, 1] {
        return Err(invalid("unsupported projected extrusion attributes"));
    }
    r.take(7)?;
    if r.u32()? != 4 {
        return Err(invalid("body-edge import requires an independent extrusion"));
    }
    r.take(115)?;
    references(&mut r)?;
    if !references(&mut r)?.is_empty() || references(&mut r)? != handles || !references(&mut r)?.is_empty() || !references(&mut r)?.is_empty() {
        return Err(invalid("body handle disagrees with extrusion outputs"));
    }
    Ok(Edge { body, feature, recipe })
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
