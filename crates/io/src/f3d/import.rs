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
        let points: BTreeMap<_, _> = self.sketch_points()?.into_iter().map(|p| (p.id, p)).collect();
        let circles: BTreeMap<_, _> = self.sketch_circles()?.into_iter().map(|c| (c.id, c)).collect();
        let lines: BTreeMap<_, _> = self.sketch_lines()?.into_iter().map(|l| (l.id, l)).collect();
        let constraints: BTreeMap<_, _> = self.sketch_constraints()?.into_iter().map(|c| (c.geometry, c)).collect();
        let profiles: BTreeMap<_, _> = self.single_loop_profiles()?.into_iter().map(|p| (p.id, p)).collect();
        let mut doc = Document::new(name);
        let mut names = BTreeSet::new();
        for p in &parameters {
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
        let mut active_bodies = Vec::new();
        let mut body_number = 0;
        for f in history {
            let mut new_body = None;
            let kind = match f.kind {
                HistoryKind::Sketch { geometry, placement } => {
                    if placement.owner != root {
                        return Err(invalid("non-root sketch placement is not supported yet"));
                    }
                    let plane = plane(&placement.matrix)?;
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
                            SketchConstraint::Diameter { id, circle, binding } => {
                                let c = curve_indices.get(circle).copied().ok_or_else(|| invalid("missing dimensioned circle"))?;
                                let parameter = bindings.get(binding).ok_or_else(|| invalid("missing diameter parameter"))?;
                                consumed.insert(*id);
                                (ConstraintKind::Diameter { c, value: parameter.value * 10. }, Some(parameter.name.clone()))
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
                                let parameter = bindings.get(binding).ok_or_else(|| invalid("missing linear dimension parameter"))?;
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
                    FeatureKind::Sketch { plane: PlaneRef::Custom { plane }, sketch }
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
                    FeatureKind::Extrude {
                        sketch,
                        profiles: ProfileSel::Curves { loops: vec![profile.curves.iter().map(|id| format!("c{id}")).collect()] },
                        extent,
                        operation,
                        targets,
                    }
                }
            };
            let id = doc.add_feature(kind, Some(&f.name))?;
            let feature = doc.features.iter_mut().find(|f| f.id == id).ok_or_else(|| invalid("missing imported feature"))?;
            feature.suppressed = f.suppressed;
            if let Some(body) = new_body {
                feature.body_names = vec![body];
            }
        }
        // Preserve expressions and reject a translation that changes their value.
        // A cached native value is a check, never a substitute for an expression.
        let (values, errors) = doc.param_values();
        if !errors.is_empty() {
            return Err(invalid(&format!("parameter expressions cannot be reconstructed: {errors:?}")));
        }
        for p in &parameters {
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

fn plane(matrix: &[f64; 16]) -> Result<Plane> {
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
