//! Reconstruct verified native feature schemas as editable SolveCraft history.
//! Unsupported schemas fail before a caller replaces its current document.
use super::history::{empty, finish, payload, references, require_class, zeros};
use super::*;
use solvecraft_doc::sketch::{ConstraintKind, Sketch};
use solvecraft_doc::{Direction, Document, Extent, FeatureKind, Operation, Param, PlaneRef, ProfileSel};
use solvecraft_geom::{Plane, Vec2, Vec3};

pub fn import_f3d(bytes: &[u8], name: &str) -> Result<Document> {
    let designs = read_f3d(bytes)?;
    if designs.len() != 1 {
        return Err(invalid("multiple design streams are not supported yet"));
    }
    designs.first().ok_or_else(|| invalid("missing design"))?.to_document(name)
}

impl Design {
    pub fn to_document(&self, name: &str) -> Result<Document> {
        let history = self.history()?;
        let has_offset_planes = history.iter().any(|f| matches!(f.kind, HistoryKind::OffsetPlane { .. }));
        if history.len() > solvecraft_doc::MAX_FEATURES {
            return Err(invalid("too many imported features"));
        }
        let components = self.classes.iter().find(|c| c.id == "E03784ED-5E19-4E14-B9F2-3B07017018CD");
        let root = components
            .filter(|c| c.version == 23 && c.objects.len() == 1)
            .and_then(|c| c.objects.first())
            .copied()
            .ok_or_else(|| invalid("component/assembly reconstruction is not supported yet"))?;
        let records = Records::new(self);
        let classes: BTreeMap<_, _> = self.classes.iter().flat_map(|c| c.objects.iter().map(move |id| (*id, c))).collect();
        let parameters = self.parameters()?;
        if parameters.len() > solvecraft_doc::MAX_PARAMS {
            return Err(invalid("too many imported parameters"));
        }
        let bindings: BTreeMap<_, _> = parameters.iter().map(|p| (p.binding, p)).collect();
        let named_parameters: BTreeMap<_, _> = parameters.iter().map(|p| (p.name.as_str(), p)).collect();
        let mut internal_offsets = BTreeSet::new();
        for f in &history {
            if let HistoryKind::OffsetPlane { extra_offset, .. } = &f.kind {
                let p = length_parameter(&bindings, *extra_offset)?;
                let mut expression = p.expression.split_whitespace();
                if p.owner != f.id
                    || p.label != "ExtraOffset"
                    || p.value != 0.
                    || expression.next().and_then(|s| s.parse::<f64>().ok()) != Some(0.)
                    || expression.next() != Some(p.display_unit.as_str())
                    || expression.next().is_some()
                {
                    return Err(invalid("unsupported internal plane offset"));
                }
                // Fusion does not expose this zero internal slot as a model
                // parameter. It is verified neutral, never a guessed input.
                internal_offsets.insert(p.id);
            }
        }
        let points: BTreeMap<_, _> = self.sketch_points()?.into_iter().map(|p| (p.id, p)).collect();
        let circles: BTreeMap<_, _> = self.sketch_circles()?.into_iter().map(|c| (c.id, c)).collect();
        let arcs: BTreeMap<_, _> = self.sketch_arcs()?.into_iter().map(|a| (a.circle.id, a)).collect();
        let lines: BTreeMap<_, _> = self.sketch_lines()?.into_iter().map(|l| (l.id, l)).collect();
        let constraints: BTreeMap<_, _> = self.sketch_constraints()?.into_iter().map(|c| (c.geometry, c)).collect();
        let profiles: BTreeMap<_, _> = self.sketch_profiles()?.into_iter().map(|p| (p.id, p)).collect();
        let mut doc = Document::new(name);
        let mut names = BTreeSet::new();
        for p in &parameters {
            if internal_offsets.contains(&p.id) {
                continue;
            }
            if solvecraft_doc::expr::unit_info(&p.display_unit).is_none() {
                return Err(invalid(&format!("unsupported parameter unit: {}", p.display_unit)));
            }
            if p.name.is_empty() || !names.insert(p.name.clone()) {
                return Err(invalid("duplicate or empty parameter name"));
            }
            doc.params.push(Param {
                name: p.name.clone(),
                expr: p.expression.clone(),
                unit: p.display_unit.clone(),
                comment: p.label.clone(),
                model: true,
            });
        }
        let mut sketches = BTreeMap::new();
        let mut planes: BTreeMap<u64, (String, Plane)> = BTreeMap::new();
        let mut active_bodies = Vec::new();
        let mut body_number = 0;
        for f in history {
            let mut new_body = None;
            let mut new_plane = None;
            let kind = match f.kind {
                HistoryKind::OffsetPlane { geometry, definition, extra_offset } => {
                    let definition = super::planes::definition(&records, &classes, definition, f.id)?;
                    if definition.extra != Some(extra_offset) {
                        return Err(invalid("inconsistent internal plane offset binding"));
                    }
                    let p = length_parameter(&bindings, definition.distance.ok_or_else(|| invalid("missing plane distance"))?)?;
                    if p.owner != f.id || p.label != "AlongDistance" {
                        return Err(invalid("plane distance belongs to another feature"));
                    }
                    let (base, cached) = plane_base(&records, &classes, &planes, definition.base, root)?;
                    let cached = cached.offset(p.value * 10.);
                    if !super::planes::same_surface(cached, definition.plane) {
                        return Err(invalid("plane definition disagrees with its dependencies"));
                    }
                    new_plane = Some((geometry, cached));
                    FeatureKind::ConstructionPlane { plane: PlaneRef::Offset { base: Box::new(base), distance: p.name.clone() } }
                }
                HistoryKind::Sketch { geometry, placement } => {
                    if placement.owner != root {
                        return Err(invalid("non-root sketch placement is not supported yet"));
                    }
                    let plane = plane(&placement.matrix)?;
                    let plane_ref = if !has_offset_planes {
                        PlaneRef::Custom { plane }
                    } else {
                        let definition = super::planes::definition(&records, &classes, placement.support, f.id)?;
                        if definition.extra.is_some() || definition.distance.is_some() {
                            return Err(invalid("unsupported sketch support offset"));
                        }
                        let (base, cached) = plane_base(&records, &classes, &planes, definition.base, root)?;
                        if !super::planes::same_surface(cached, definition.plane)
                            || !super::planes::same_surface(cached, plane)
                            || (cached.x - plane.x).len() > 1e-9
                            || (cached.y - plane.y).len() > 1e-9
                        {
                            return Err(invalid("unsupported sketch frame relative to its construction plane"));
                        }
                        base
                    };
                    let relations =
                        constraints.get(&geometry).filter(|c| c.feature == f.id).ok_or_else(|| invalid("missing sketch relationships"))?;
                    let mut sketch = Sketch::new();
                    let mut point_indices = BTreeMap::new();
                    let mut curve_indices = BTreeMap::new();
                    let mut consumed = BTreeSet::new();
                    let mut has_origin = false;
                    for id in &placement.entities {
                        if let Some(p) = points.get(id) {
                            if p.sketch != geometry {
                                return Err(invalid("point belongs to another sketch"));
                            }
                            let [x, y, z] = p.position;
                            if z.abs() > 1e-10 {
                                return Err(invalid("3D sketch points are not supported yet"));
                            }
                            let i = if p.origin {
                                if has_origin {
                                    return Err(invalid("duplicate sketch origin"));
                                }
                                has_origin = true;
                                0
                            } else {
                                sketch.add_point(Vec2::new(x * 10., y * 10.), Some(&format!("p{id}"))).map_err(|e| invalid(&e.to_string()))?
                            };
                            let point = sketch.points.get_mut(i).ok_or_else(|| invalid("missing imported point"))?;
                            point.fixed = p.fixed;
                            point_indices.insert(*id, i);
                            if !consumed.insert(*id) {
                                return Err(invalid("duplicate sketch entity"));
                            }
                        }
                    }
                    for id in &placement.entities {
                        if let Some(c) = circles.get(id) {
                            if c.sketch != geometry || c.center.get(2).is_none_or(|z| z.abs() > 1e-10) || c.normal != [0., 0., 1.] {
                                return Err(invalid("unsupported circle placement"));
                            }
                            let center = points.get(&c.center_point).ok_or_else(|| invalid("missing circle center"))?;
                            if c.center.iter().zip(center.position).any(|(a, b)| (a - b).abs() > 1e-10) {
                                return Err(invalid("circle and center point disagree"));
                            }
                            let pi = point_indices.get(&c.center_point).copied().ok_or_else(|| invalid("circle center is outside its sketch"))?;
                            let [x, y, _] = c.center;
                            let i = sketch
                                .add_circle(Vec2::new(x * 10., y * 10.), c.radius * 10., Some(pi), Some(&format!("c{id}")))
                                .map_err(|e| invalid(&e.to_string()))?;
                            let curve = sketch.curves.get_mut(i).ok_or_else(|| invalid("missing imported curve"))?;
                            curve.construction = c.construction;
                            curve.fixed = c.fixed;
                            curve_indices.insert(*id, i);
                            if !consumed.insert(*id) {
                                return Err(invalid("duplicate sketch entity"));
                            }
                        }
                    }
                    for id in &placement.entities {
                        if let Some(arc) = arcs.get(id) {
                            let c = &arc.circle;
                            if c.sketch != geometry || c.normal != [0., 0., 1.] {
                                return Err(invalid("unsupported arc placement"));
                            }
                            let center = Vec3::new(c.center[0], c.center[1], c.center[2]);
                            let x = Vec3::new(c.x_axis[0], c.x_axis[1], c.x_axis[2]);
                            let y = Vec3::new(0., 0., 1.).cross(x);
                            let position = |angle: f64| center + (x * angle.cos() + y * angle.sin()) * c.radius;
                            let mut indices = [None; 3];
                            let mut coords = [Vec2::ZERO; 3];
                            for (slot, (id, expected)) in
                                [(c.center_point, center), (arc.start_point, position(arc.start_angle)), (arc.end_point, position(arc.end_angle))]
                                    .into_iter()
                                    .enumerate()
                            {
                                let point = points.get(&id).filter(|p| p.sketch == geometry).ok_or_else(|| invalid("missing arc point"))?;
                                let [px, py, pz] = point.position;
                                if (Vec3::new(px, py, pz) - expected).len() > 1e-9 || pz.abs() > 1e-10 {
                                    return Err(invalid("arc and point geometry disagree"));
                                }
                                let index = point_indices.get(&id).copied().ok_or_else(|| invalid("arc point outside sketch"))?;
                                if let Some(target) = indices.get_mut(slot) {
                                    *target = Some(index);
                                }
                                if let Some(target) = coords.get_mut(slot) {
                                    *target = Vec2::new(px * 10., py * 10.);
                                }
                            }
                            let [center, start, end] = coords;
                            let i = sketch.add_arc(center, start, end, indices, Some(&format!("c{id}"))).map_err(|e| invalid(&e.to_string()))?;
                            let curve = sketch.curves.get_mut(i).ok_or_else(|| invalid("missing imported arc"))?;
                            curve.fixed = c.fixed;
                            curve.construction = c.construction;
                            curve_indices.insert(*id, i);
                            if !consumed.insert(*id) {
                                return Err(invalid("duplicate sketch entity"));
                            }
                        }
                    }
                    for id in &placement.entities {
                        if let Some(line) = lines.get(id) {
                            if line.sketch != geometry {
                                return Err(invalid("line belongs to another sketch"));
                            }
                            for (point, position) in [(line.start_point, line.start), (line.end_point, line.end)] {
                                let p = points.get(&point).filter(|p| p.sketch == geometry).ok_or_else(|| invalid("missing line endpoint"))?;
                                if p.position.iter().zip(position).any(|(a, b)| (a - b).abs() > 1e-10) {
                                    return Err(invalid("line and endpoint disagree"));
                                }
                            }
                            let a = point_indices.get(&line.start_point).copied().ok_or_else(|| invalid("line start outside sketch"))?;
                            let b = point_indices.get(&line.end_point).copied().ok_or_else(|| invalid("line end outside sketch"))?;
                            let i = sketch.add_line_pts(a, b, Some(&format!("c{id}"))).map_err(|e| invalid(&e.to_string()))?;
                            let c = sketch.curves.get_mut(i).ok_or_else(|| invalid("missing imported line"))?;
                            c.fixed = line.fixed;
                            c.construction = line.construction;
                            curve_indices.insert(*id, i);
                            if !consumed.insert(*id) {
                                return Err(invalid("duplicate sketch entity"));
                            }
                        }
                    }
                    for constraint in &relations.constraints {
                        let (kind, param) = match constraint {
                            SketchConstraint::Coincident { points: [a, b], .. } => {
                                let p = point_indices.get(a).copied().ok_or_else(|| invalid("missing coincidence point"))?;
                                let q = point_indices.get(b).copied().ok_or_else(|| invalid("missing coincidence point"))?;
                                (ConstraintKind::Coincident { p, q }, None)
                            }
                            SketchConstraint::AlignedPoints { points: [a, b], vertical, .. } => {
                                let p = point_indices.get(a).copied().ok_or_else(|| invalid("missing aligned point"))?;
                                let q = point_indices.get(b).copied().ok_or_else(|| invalid("missing aligned point"))?;
                                (if *vertical { ConstraintKind::VerticalPoints { p, q } } else { ConstraintKind::HorizontalPoints { p, q } }, None)
                            }
                            SketchConstraint::PointCurve { point, curve, midpoint, .. } => {
                                let p = point_indices.get(point).copied().ok_or_else(|| invalid("missing constrained point"))?;
                                let c = curve_indices.get(curve).copied().ok_or_else(|| invalid("missing constrained curve"))?;
                                (if *midpoint { ConstraintKind::Midpoint { p, l: c } } else { ConstraintKind::PointOnCurve { p, c } }, None)
                            }
                            SketchConstraint::Curves { curves: [a, b], relation, .. } => {
                                let a = curve_indices.get(a).copied().ok_or_else(|| invalid("missing constrained curve"))?;
                                let b = curve_indices.get(b).copied().ok_or_else(|| invalid("missing constrained curve"))?;
                                (
                                    match relation {
                                        CurveRelation::Collinear => ConstraintKind::Collinear { a, b },
                                        CurveRelation::Concentric => ConstraintKind::Concentric { a, b },
                                        CurveRelation::Parallel => ConstraintKind::Parallel { a, b },
                                        CurveRelation::Perpendicular => ConstraintKind::Perpendicular { a, b },
                                        CurveRelation::Tangent => ConstraintKind::Tangent { a, b },
                                        CurveRelation::Equal => ConstraintKind::Equal { a, b },
                                    },
                                    None,
                                )
                            }
                            SketchConstraint::Diameter { id, circle, binding } | SketchConstraint::Radius { id, circle, binding } => {
                                let c = curve_indices.get(circle).copied().ok_or_else(|| invalid("missing dimensioned curve"))?;
                                let parameter = length_parameter(&bindings, *binding)?;
                                consumed.insert(*id);
                                let value = parameter.value * 10.;
                                (
                                    if matches!(constraint, SketchConstraint::Radius { .. }) {
                                        ConstraintKind::Radius { c, value }
                                    } else {
                                        ConstraintKind::Diameter { c, value }
                                    },
                                    Some(parameter.name.clone()),
                                )
                            }
                            SketchConstraint::Horizontal { line, .. } | SketchConstraint::Vertical { line, .. } => {
                                let l = curve_indices.get(line).copied().ok_or_else(|| invalid("missing constrained line"))?;
                                (
                                    if matches!(constraint, SketchConstraint::Horizontal { .. }) {
                                        ConstraintKind::Horizontal { l }
                                    } else {
                                        ConstraintKind::Vertical { l }
                                    },
                                    None,
                                )
                            }
                            SketchConstraint::Linear { id, points: [a, b], vertical, binding } => {
                                let p = point_indices.get(a).copied().ok_or_else(|| invalid("missing dimension point"))?;
                                let q = point_indices.get(b).copied().ok_or_else(|| invalid("missing dimension point"))?;
                                let parameter = length_parameter(&bindings, *binding)?;
                                consumed.insert(*id);
                                let value = parameter.value * 10.;
                                (
                                    if *vertical { ConstraintKind::DistanceY { p, q, value } } else { ConstraintKind::DistanceX { p, q, value } },
                                    Some(parameter.name.clone()),
                                )
                            }
                        };
                        sketch.add_constraint(kind, param).map_err(|e| invalid(&e.to_string()))?;
                    }
                    if placement.entities.iter().any(|id| !consumed.contains(id)) {
                        return Err(invalid("unsupported sketch entity; import would lose history"));
                    }
                    sketches.insert(geometry, (doc.next_id, plane));
                    FeatureKind::Sketch { plane: plane_ref, sketch }
                }
                HistoryKind::Extrude { operation, profile_containers } => {
                    let selected = selected(&records, &profiles, f.id, &profile_containers)?;
                    let [profile] = selected.as_slice() else {
                        return Err(invalid("multiple-profile extrusion reconstruction is not supported yet"));
                    };
                    let (sketch, plane) = sketches.get(&profile.geometry).copied().ok_or_else(|| invalid("extrusion precedes its source sketch"))?;
                    let extent = extent(&records, &classes, &bindings, f.id, &plane)?;
                    let targets = if operation == Operation::NewBody {
                        body_number += 1;
                        let body = format!("Body{body_number}");
                        if !f.suppressed {
                            active_bodies.push(body.clone());
                        }
                        new_body = Some(body);
                        vec![]
                    } else {
                        if active_bodies.len() != 1 {
                            return Err(invalid("multi-body boolean target reconstruction is not supported yet"));
                        }
                        active_bodies.clone()
                    };
                    FeatureKind::Extrude { sketch, profiles: profile_selection(profile)?, extent, operation, targets }
                }
            };
            let id = doc.add_feature(kind, Some(&f.name))?;
            let feature = doc.features.iter_mut().find(|f| f.id == id).ok_or_else(|| invalid("missing imported feature"))?;
            feature.suppressed = f.suppressed;
            if let Some((geometry, plane)) = new_plane
                && planes.insert(geometry, (feature.name.clone(), plane)).is_some()
            {
                return Err(invalid("duplicate construction-plane geometry"));
            }
            // Feature inputs own their native parameter identity. Keeping a second
            // stored parameter would leave downstream expressions stale after a
            // timeline edit. Reserve all names above, then transfer these inputs.
            let mut transferred = BTreeSet::new();
            for (name, (_, expression, kind)) in feature.param_names.iter_mut().zip(feature.kind.inputs_mut()) {
                let Some(p) = named_parameters.get(expression.as_str()).filter(|p| p.owner == f.id) else { continue };
                // SolveCraft feature inputs use mm/degrees. Preserve the existing
                // unit-bearing parameter indirection for other display units until
                // feature inputs support their own unit contexts.
                if solvecraft_doc::expr::unit_info(&p.display_unit) != solvecraft_doc::expr::unit_info(kind.default_unit()) {
                    continue;
                }
                *name = p.name.clone();
                *expression = p.expression.clone();
                transferred.insert(p.name.clone());
                doc.param_comments.insert(p.name.clone(), p.label.clone());
            }
            if let Some(body) = new_body {
                feature.body_names = vec![body];
            }
            doc.params.retain(|p| !transferred.contains(&p.name));
        }
        // Preserve expressions and reject a translation that changes their value.
        // A cached native value is a check, never a substitute for an expression.
        let (values, errors) = doc.param_values();
        if !errors.is_empty() {
            return Err(invalid(&format!("parameter expressions cannot be reconstructed: {errors:?}")));
        }
        for p in &parameters {
            if internal_offsets.contains(&p.id) {
                continue;
            }
            let kind = solvecraft_doc::expr::Kind::from_unit(&p.display_unit);
            let value = values.get(&p.name).ok_or_else(|| invalid("missing imported parameter"))?.to_kind(kind)?;
            let expected = p.value * if kind == solvecraft_doc::expr::Kind::Length { 10. } else { 1. };
            if (value - expected).abs() > 1e-8 * expected.abs().max(1.) {
                return Err(invalid(&format!("parameter {} changed value during import", p.name)));
            }
        }
        Ok(doc)
    }
}

fn length_parameter<'a>(bindings: &BTreeMap<u64, &'a Parameter>, binding: u64) -> Result<&'a Parameter> {
    let p = bindings.get(&binding).copied().ok_or_else(|| invalid("missing dimension parameter"))?;
    if solvecraft_doc::expr::Kind::from_unit(&p.display_unit) != solvecraft_doc::expr::Kind::Length {
        return Err(invalid("dimension requires a length parameter"));
    }
    Ok(p)
}

fn profile_selection(profile: &SketchProfile) -> Result<ProfileSel> {
    let ids = |boundary: &ProfileLoop| {
        let mut ids: Vec<_> = boundary.curves.iter().map(|id| format!("c{id}")).collect();
        ids.sort();
        ids
    };
    let outer = profile.loops.iter().find(|l| l.outer).ok_or_else(|| invalid("missing outer profile boundary"))?;
    let mut holes: Vec<_> = profile.loops.iter().filter(|l| !l.outer).map(ids).collect();
    holes.sort();
    Ok(ProfileSel::WholeBoundaries { profiles: vec![solvecraft_doc::WholeProfileBoundary { outer: ids(outer), holes }] })
}

pub(super) fn plane(matrix: &[f64; 16]) -> Result<Plane> {
    if matrix.iter().any(|n| !n.is_finite()) {
        return Err(invalid("non-finite plane placement"));
    }
    let [xx, yx, nx, ox, xy, yy, ny, oy, xz, yz, nz, oz, a, b, c, d] = *matrix;
    if [a, b, c, d] != [0., 0., 0., 1.] {
        return Err(invalid("non-affine sketch placement"));
    }
    let x = Vec3::new(xx, xy, xz);
    let y = Vec3::new(yx, yy, yz);
    let n = Vec3::new(nx, ny, nz);
    if (x.len() - 1.).abs() > 1e-9 || (y.len() - 1.).abs() > 1e-9 || x.dot(y).abs() > 1e-9 || (x.cross(y) - n).len() > 1e-9 {
        return Err(invalid("non-rigid sketch placement"));
    }
    Plane::new(Vec3::new(ox, oy, oz) * 10., x, y).ok_or_else(|| invalid("invalid sketch placement"))
}

fn plane_base(
    records: &Records<'_>,
    classes: &BTreeMap<u64, &Class>,
    planes: &BTreeMap<u64, (String, Plane)>,
    id: u64,
    root: u64,
) -> Result<(PlaneRef, Plane)> {
    if let Some((name, plane)) = planes.get(&id) {
        return Ok((PlaneRef::Construction { name: name.clone() }, *plane));
    }
    let (name, plane) = super::planes::origin(records, classes, id, root)?;
    Ok((PlaneRef::Origin { name }, plane))
}

fn selected<'a>(
    records: &Records<'_>,
    profiles: &'a BTreeMap<u64, SketchProfile>,
    feature: u64,
    containers: &[u64],
) -> Result<Vec<&'a SketchProfile>> {
    let mut selected = Vec::new();
    let mut seen = BTreeSet::new();
    for id in containers {
        let bytes = records.primary(*id)?;
        let mut r = payload(bytes, *id)?;
        empty(&mut r)?;
        zeros(&mut r, 2)?;
        for id in references(&mut r)? {
            if !seen.insert(id) {
                return Err(invalid("duplicate selected profile"));
            }
            selected.push(profiles.get(&id).filter(|p| p.feature == feature).ok_or_else(|| invalid("missing selected profile or wrong owner"))?);
        }
        let tail = bytes.len().checked_sub(11).filter(|at| *at >= r.at).ok_or_else(|| invalid("truncated profile container"))?;
        r.at = tail;
        if r.reference()? != feature {
            return Err(invalid("inconsistent profile container owner"));
        }
        finish(&r)?;
    }
    Ok(selected)
}

fn extent(records: &Records<'_>, classes: &BTreeMap<u64, &Class>, parameters: &BTreeMap<u64, &Parameter>, id: u64, plane: &Plane) -> Result<Extent> {
    let mut r = payload(records.primary(id)?, id)?;
    empty(&mut r)?;
    if r.take(2)? != [0, 1] {
        return Err(invalid("unsupported extrusion attributes"));
    }
    r.take(7)?;
    r.u32()?;
    if r.u32()? != 1 || r.u32()? != 2 {
        return Err(invalid("unsupported extrusion extent type"));
    }
    zeros(&mut r, 1)?;
    if r.u32()? != 1 {
        return Err(invalid("unsupported extrusion start type"));
    }
    zeros(&mut r, 1)?;
    let axis = Vec3::new(f64::from_bits(r.u64()?), f64::from_bits(r.u64()?), f64::from_bits(r.u64()?));
    if !axis.is_finite() || (axis - plane.normal()).len() > 1e-9 {
        return Err(invalid("extrusion axis differs from its sketch normal"));
    }
    zeros(&mut r, 2)?;
    let distance = r.reference()?;
    let extent_helper = r.reference()?;
    require_class(classes, extent_helper, "0897AF07-EF1B-42E5-BDD9-1C746A085CBC", 0)?;
    zeros(&mut r, 1)?;
    let taper = r.reference()?;
    zeros(&mut r, 1)?;
    if r.u32()? != 1 {
        return Err(invalid("unsupported extrusion extent flags"));
    }
    zeros(&mut r, 36)?;
    let distance = parameters
        .get(&distance)
        .filter(|p| p.owner == id && p.label == "AlongDistance")
        .ok_or_else(|| invalid("missing extrusion distance binding"))?;
    let taper =
        parameters.get(&taper).filter(|p| p.owner == id && p.label == "TaperAngle").ok_or_else(|| invalid("missing extrusion taper binding"))?;
    if solvecraft_doc::expr::Kind::from_unit(&distance.display_unit) != solvecraft_doc::expr::Kind::Length
        || solvecraft_doc::expr::Kind::from_unit(&taper.display_unit) != solvecraft_doc::expr::Kind::Angle
    {
        return Err(invalid("extrusion parameter units do not match their input roles"));
    }
    // Controlled volume measurements establish the same positive taper sign.
    Ok(Extent {
        distance: distance.name.clone(),
        direction: Direction::Positive,
        distance2: None,
        start_offset: None,
        through_all: false,
        taper: Some(taper.name.clone()),
        to: None,
        to_offset: None,
        from: None,
    })
}
