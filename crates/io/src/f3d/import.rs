//! Reconstruct verified native feature schemas as editable SolveCraft history.
//! Unsupported schemas fail before a caller replaces its current document.
use super::history::{empty, finish, payload, references, zeros};
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
        if parameters.iter().any(|p| p.driven) {
            return Err(invalid("driven parameter measurement dependencies are not supported yet"));
        }
        if parameters.len() > solvecraft_doc::MAX_PARAMS {
            return Err(invalid("too many imported parameters"));
        }
        let bindings: BTreeMap<_, _> = parameters.iter().filter_map(|p| p.binding.map(|binding| (binding, p))).collect();
        let named_parameters: BTreeMap<_, _> = parameters.iter().map(|p| (p.name.as_str(), p)).collect();
        let mut internal_offsets = BTreeSet::new();
        for f in &history {
            if let HistoryKind::OffsetPlane { extra_offset: Some(extra_offset), .. } = &f.kind {
                let p = length_parameter(&bindings, *extra_offset)?;
                let mut expression = p.expression.split_whitespace();
                if p.owner != Some(f.id)
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
        super::offset_import::metadata(&projections::Geometry { points: &points, lines: &lines, circles: &circles, arcs: &arcs }, &constraints)?;
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
            if p.favorite {
                doc.favorites.insert(p.name.clone());
            }
            doc.params.push(Param {
                name: p.name.clone(),
                expr: p.expression.clone(),
                unit: p.display_unit.clone(),
                comment: p.comment.clone(),
                model: p.binding.is_some(),
            });
        }
        let mut sketches = BTreeMap::new();
        let mut planes: BTreeMap<u64, (String, Plane)> = BTreeMap::new();
        let mut active_bodies = Vec::new();
        let mut body_number = 0;
        let mut feature_ids = BTreeMap::new();
        let mut source_profiles = BTreeMap::new();
        let mut operation_owners = None;
        let mut projection_model = solvecraft_doc::Model::new();
        for f in history {
            let mut new_body = None;
            let mut new_plane = None;
            let kind = match f.kind {
                HistoryKind::OffsetPlane { geometry, definition, extra_offset } => {
                    let definition = super::planes::definition(&records, &classes, definition, f.id)?;
                    if definition.extra != extra_offset {
                        return Err(invalid("inconsistent internal plane offset binding"));
                    }
                    let p = length_parameter(&bindings, definition.distance.ok_or_else(|| invalid("missing plane distance"))?)?;
                    if p.owner != Some(f.id) || p.label != "AlongDistance" {
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
                    if !placement.legacy_entity_metadata.is_empty() {
                        return Err(invalid("legacy sketch entity metadata reconstruction is not supported yet"));
                    }
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
                        if !super::planes::same_surface(cached, definition.plane) {
                            return Err(invalid("sketch support disagrees with its construction plane"));
                        }
                        sketch_plane_reference(base, cached, plane)?
                    };
                    let relations =
                        constraints.get(&geometry).filter(|c| c.feature == f.id).ok_or_else(|| invalid("missing sketch relationships"))?;
                    let mut sketch = Sketch::new();
                    let mut point_indices = BTreeMap::new();
                    let mut curve_indices = BTreeMap::new();
                    let mut consumed = BTreeSet::new();
                    let native = projections::Geometry { points: &points, lines: &lines, circles: &circles, arcs: &arcs };
                    let mut projected_sources = BTreeMap::new();
                    let members: BTreeSet<_> = placement.entities.iter().copied().collect();
                    if members.len() != placement.entities.len() {
                        return Err(invalid("duplicate sketch entity"));
                    }
                    for id in &placement.projections {
                        let projection = projections::decode(&records, &classes, *id, f.id)?;
                        let (entities, namespace) = match &projection.source {
                            projections::Source::Sketch { geometry: source_geometry, .. } => {
                                let (source_feature, source_plane) =
                                    sketches.get(source_geometry).copied().ok_or_else(|| invalid("projection precedes its source sketch"))?;
                                (native.attach(&projection, geometry, source_feature, (source_plane, plane), &mut sketch)?, (0_u8, *source_geometry))
                            }
                            projections::Source::Body(edge) => {
                                if operation_owners.is_none() {
                                    operation_owners = Some(self.operation_owners()?);
                                }
                                projection_model.evaluate(&doc);
                                if projection_model.results.iter().any(|r| r.error.is_some()) {
                                    return Err(invalid("cannot regenerate body projection dependencies"));
                                }
                                let state = projection_model.state();
                                let context = super::body_projections::Context {
                                    native: &native,
                                    profiles: &source_profiles,
                                    doc: &doc,
                                    model: &state,
                                    features: &feature_ids,
                                    operations: operation_owners.as_ref().ok_or_else(|| invalid("missing topology operation ownership"))?,
                                };
                                (context.attach(&projection, geometry, plane, &mut sketch)?, (if edge.recipe.end { 2_u8 } else { 1_u8 }, edge.body))
                            }
                        };
                        for (id, index, source_point) in entities.points {
                            if !members.contains(&id) {
                                return Err(invalid("foreign projected point"));
                            }
                            let source = (namespace, source_point);
                            if let Some(previous) = projected_sources.insert(id, source) {
                                if previous != source {
                                    return Err(invalid("shared projected point has conflicting sources"));
                                }
                                // A link owns its points. Adjacent one-curve links use
                                // aliases of the same source vertex; constraints bind
                                // the canonical point and every alias follows that source.
                                sketch.points.get_mut(index).ok_or_else(|| invalid("missing projected alias"))?.id =
                                    format!("p{id}:projection{}", projection.target);
                            } else {
                                consumed.insert(id);
                                point_indices.insert(id, index);
                            }
                        }
                        let (id, index) = entities.curve;
                        if !members.contains(&id) || !consumed.insert(id) {
                            return Err(invalid("duplicate or foreign projected curve"));
                        }
                        curve_indices.insert(id, index);
                    }
                    let projected = consumed.clone();
                    let mut has_origin = false;
                    for id in &placement.entities {
                        if projected.contains(id) {
                            continue;
                        }
                        if let Some(p) = points.get(id) {
                            if p.linked {
                                return Err(invalid("linked sketch source dependencies are not supported yet"));
                            }
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
                        if projected.contains(id) {
                            continue;
                        }
                        if let Some(c) = circles.get(id) {
                            if c.reference {
                                return Err(invalid("linked sketch source dependencies are not supported yet"));
                            }
                            if c.sketch != geometry || c.center.get(2).is_none_or(|z| z.abs() > 1e-10) || !xy_normal(&c.normal) {
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
                        if projected.contains(id) {
                            continue;
                        }
                        if let Some(arc) = arcs.get(id) {
                            let c = &arc.circle;
                            if c.reference {
                                return Err(invalid("linked sketch source dependencies are not supported yet"));
                            }
                            if c.sketch != geometry || !xy_normal(&c.normal) {
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
                                // Rotated public-API controls save an angle about 2e-8 rad
                                // from its exact endpoint; retain the independently saved point.
                                if (Vec3::new(px, py, pz) - expected).len() > 1e-7 * c.radius.max(1.) || pz.abs() > 1e-10 {
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
                            curve.construction = c.construction;
                            if c.fixed {
                                // Native fixation holds the supporting circle. Endpoint
                                // fixation is independent; curve.fixed would freeze both.
                                let [center, _, _] = indices;
                                let p = center.ok_or_else(|| invalid("missing fixed arc center"))?;
                                for kind in [ConstraintKind::Fix { p }, ConstraintKind::Radius { c: i, value: c.radius * 10. }] {
                                    sketch.add_constraint(kind, None).map_err(|e| invalid(&e.to_string()))?;
                                }
                            }
                            curve_indices.insert(*id, i);
                            if !consumed.insert(*id) {
                                return Err(invalid("duplicate sketch entity"));
                            }
                        }
                    }
                    for id in &placement.entities {
                        if projected.contains(id) {
                            continue;
                        }
                        if let Some(line) = lines.get(id) {
                            if line.reference {
                                return Err(invalid("linked sketch source dependencies are not supported yet"));
                            }
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
                            c.construction = line.construction;
                            c.centerline = line.centerline;
                            if line.fixed {
                                fix_line_support(&mut sketch, i, *id)?;
                            }
                            curve_indices.insert(*id, i);
                            if !consumed.insert(*id) {
                                return Err(invalid("duplicate sketch entity"));
                            }
                        }
                    }
                    for constraint in &relations.constraints {
                        let (kind, param) = match constraint {
                            SketchConstraint::OffsetCurves(offset) => {
                                let parameter = length_parameter(&bindings, offset.binding)?;
                                super::offset_import::attach(&mut sketch, offset, parameter)?;
                                consumed.insert(offset.dimension);
                                continue;
                            }
                            SketchConstraint::LineMidpoints { id, lines: [a, b] } => {
                                let a = curve_indices.get(a).copied().ok_or_else(|| invalid("missing midpoint line"))?;
                                let b = curve_indices.get(b).copied().ok_or_else(|| invalid("missing midpoint line"))?;
                                line_midpoints(&mut sketch, [a, b], *id)?;
                                continue;
                            }
                            SketchConstraint::Symmetry { axis, entities, .. } => {
                                for kind in symmetry(&sketch, &point_indices, &curve_indices, *axis, *entities)? {
                                    sketch.add_constraint(kind, None).map_err(|e| invalid(&e.to_string()))?;
                                }
                                continue;
                            }
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
                            SketchConstraint::Linear { id, points: [a, b], orientation, binding } => {
                                let p = point_indices.get(a).copied().ok_or_else(|| invalid("missing dimension point"))?;
                                let q = point_indices.get(b).copied().ok_or_else(|| invalid("missing dimension point"))?;
                                let parameter = length_parameter(&bindings, *binding)?;
                                consumed.insert(*id);
                                let value = parameter.value * 10.;
                                (
                                    match orientation {
                                        LinearOrientation::Horizontal => ConstraintKind::DistanceX { p, q, value },
                                        LinearOrientation::Vertical => ConstraintKind::DistanceY { p, q, value },
                                        LinearOrientation::Aligned => ConstraintKind::Distance { p, q, value },
                                    },
                                    Some(parameter.name.clone()),
                                )
                            }
                            SketchConstraint::Offset { id, line, entity, binding } => {
                                let l = curve_indices.get(line).copied().ok_or_else(|| invalid("missing offset dimension line"))?;
                                let p = if let Some(p) = point_indices.get(entity) {
                                    *p
                                } else {
                                    let c = curve_indices.get(entity).copied().ok_or_else(|| invalid("missing offset dimension entity"))?;
                                    let Some(solvecraft_doc::sketch::CurveKind::Line { a, .. }) = sketch.curves.get(c).map(|c| &c.kind) else {
                                        return Err(invalid("offset dimension requires a line or point"));
                                    };
                                    let p = *a;
                                    sketch.add_constraint(ConstraintKind::Parallel { a: l, b: c }, None).map_err(|e| invalid(&e.to_string()))?;
                                    p
                                };
                                let parameter = length_parameter(&bindings, *binding)?;
                                consumed.insert(*id);
                                (ConstraintKind::PointLineDistance { p, l, value: parameter.value * 10. }, Some(parameter.name.clone()))
                            }
                            SketchConstraint::Angle { id, lines: [a, b], quadrants, binding, cached_radians } => {
                                let a = curve_indices.get(a).copied().ok_or_else(|| invalid("missing angular dimension line"))?;
                                let b = curve_indices.get(b).copied().ok_or_else(|| invalid("missing angular dimension line"))?;
                                let p = bindings.get(binding).ok_or_else(|| invalid("missing angular dimension parameter"))?;
                                if solvecraft_doc::expr::Kind::from_unit(&p.display_unit) != solvecraft_doc::expr::Kind::Angle
                                    || (p.value - cached_radians).abs() > 1e-10
                                {
                                    return Err(invalid("angular dimension parameter disagrees with its cached value or units"));
                                }
                                let [quadrant, _] = *quadrants;
                                consumed.insert(*id);
                                (angle_constraint(&sketch, a, b, p.value, quadrant)?, Some(p.name.clone()))
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
                HistoryKind::Loft { operation, section_containers, target_containers } => {
                    if !(2..=3).contains(&section_containers.len()) {
                        return Err(invalid("multi-section loft interpolation is not supported yet"));
                    }
                    if !matches!(operation, Operation::NewBody | Operation::Join) {
                        return Err(invalid("loft cut/intersect history is not supported yet"));
                    }
                    let three_sections = section_containers.len() == 3;
                    let mut sections = Vec::new();
                    for container in section_containers {
                        let objects = super::loft_faces::objects(&records, &classes, container, f.id)?;
                        let [object] = objects.as_slice() else {
                            return Err(invalid("loft section requires one source"));
                        };
                        if classes.get(object).is_some_and(|c| c.id == super::body_projections::SELECTOR) {
                            let face = super::loft_faces::face(&records, &classes, *object, f.id)?;
                            if operation_owners.is_none() {
                                operation_owners = Some(self.operation_owners()?);
                            }
                            let owners = operation_owners.as_ref().ok_or_else(|| invalid("missing loft face operations"))?;
                            let id = *feature_ids.get(&face.feature).ok_or_else(|| invalid("missing imported loft body creator"))?;
                            let creator = doc.feature(id).ok_or_else(|| invalid("missing imported loft body creator"))?;
                            let [body_name] = creator.body_names.as_slice() else { return Err(invalid("loft face requires one created body")) };
                            let mut contributions = Vec::new();
                            let mut producers = BTreeSet::new();
                            for operation in &face.all_operations {
                                let native = *owners.get(operation).ok_or_else(|| invalid("missing loft face operation owner"))?;
                                let imported = *feature_ids.get(&native).ok_or_else(|| invalid("loft face operation follows its selector"))?;
                                let current =
                                    doc.feature(imported).filter(|f| !f.suppressed).ok_or_else(|| invalid("loft face operation is suppressed"))?;
                                if native != face.feature
                                    && !matches!(&current.kind, FeatureKind::Extrude { operation: Operation::Join, targets, .. } if targets.as_slice() == [body_name.clone()])
                                {
                                    return Err(invalid("loft face contributor belongs to another body or operation"));
                                }
                                let source = *source_profiles.get(&native).ok_or_else(|| invalid("missing loft face contributor profile"))?;
                                if face.contributions.contains(operation) {
                                    face.validate_profile(source, &circles, &lines)?;
                                    if !producers.insert(imported) {
                                        return Err(invalid("duplicate loft face contribution producer"));
                                    }
                                    let curve = face
                                        .side_roles
                                        .get(operation)
                                        .copied()
                                        .map(|label| {
                                            super::side_roles::curve(&records, &classes, native, label, source, &lines).map(|id| format!("c{id}"))
                                        })
                                        .transpose()?;
                                    contributions.push((imported, curve));
                                }
                            }
                            if face.side.is_none() && contributions.len() != 1 {
                                return Err(invalid("composite cap sections are not supported yet"));
                            }
                            projection_model.evaluate(&doc);
                            if projection_model.results.iter().any(|r| r.error.is_some()) {
                                return Err(invalid("cannot regenerate loft face dependencies"));
                            }
                            let anchor = face.anchor(&doc, &projection_model.state(), id, &contributions)?;
                            sections.push(solvecraft_doc::LoftSection {
                                sketch: 0,
                                profiles: ProfileSel::All,
                                point: None,
                                face: Some(Box::new(anchor)),
                            });
                            continue;
                        }
                        let chosen = selected(&records, &profiles, f.id, &[container])?;
                        let [profile] = chosen.as_slice() else {
                            return Err(invalid("loft section requires one selected sketch profile"));
                        };
                        let [loops] = profile.regions.as_slice() else {
                            return Err(invalid("loft section requires one region"));
                        };
                        if loops.len() != 1 {
                            return Err(invalid("loft sections with holes are not supported yet"));
                        }
                        if three_sections {
                            let [boundary] = loops.as_slice() else { return Err(invalid("missing circular loft boundary")) };
                            let [circle] = boundary.curves.as_slice() else {
                                return Err(invalid("three-section native loft requires complete circular sketch profiles"));
                            };
                            if !boundary.outer || boundary.pieces != [(1, 1)] || !circles.get(circle).is_some_and(|c| c.sketch == profile.geometry) {
                                return Err(invalid("three-section native loft requires complete circular sketch profiles"));
                            }
                        }
                        let (sketch, _) = sketches.get(&profile.geometry).ok_or_else(|| invalid("loft section must precede its feature"))?;
                        sections.push(solvecraft_doc::LoftSection {
                            sketch: *sketch,
                            profiles: profile_selection(profile, &circles)?,
                            point: None,
                            face: None,
                        });
                    }
                    let targets = if operation == Operation::NewBody {
                        if !target_containers.is_empty() {
                            return Err(invalid("new-body loft cannot have boolean targets"));
                        }
                        body_number += 1;
                        let name = format!("Body{body_number}");
                        if !f.suppressed {
                            active_bodies.push(name.clone());
                        }
                        new_body = Some(vec![name]);
                        vec![]
                    } else {
                        let [container] = target_containers.as_slice() else {
                            return Err(invalid("loft join requires one explicit target"));
                        };
                        let target = super::loft_faces::target(&records, &classes, *container, f.id)?;
                        if operation_owners.is_none() {
                            operation_owners = Some(self.operation_owners()?);
                        }
                        let owners = operation_owners.as_ref().ok_or_else(|| invalid("missing loft target operations"))?;
                        let source = feature_ids
                            .get(&target.feature)
                            .and_then(|id| doc.feature(*id))
                            .filter(|f| !f.suppressed)
                            .ok_or_else(|| invalid("loft target precedes its producer or the producer is suppressed"))?;
                        let [body] = source.body_names.as_slice() else {
                            return Err(invalid("loft target must identify one body"));
                        };
                        if !active_bodies.contains(body) {
                            return Err(invalid("loft target body is no longer active"));
                        }
                        let mut producers = BTreeSet::new();
                        for (i, operation) in target.operations.iter().enumerate() {
                            let producer = *owners.get(&operation.operation).ok_or_else(|| invalid("missing loft target operation owner"))?;
                            let feature = feature_ids
                                .get(&producer)
                                .and_then(|id| doc.feature(*id))
                                .filter(|f| !f.suppressed)
                                .ok_or_else(|| invalid("loft target operation follows its selector or is suppressed"))?;
                            if !producers.insert(producer)
                                || (i == 0 && producer != target.feature)
                                || (i != 0
                                    && !matches!(&feature.kind, FeatureKind::Extrude { operation: Operation::Join, targets, .. } if targets.as_slice() == [body.clone()]))
                            {
                                return Err(invalid("loft target operations disagree with their body ownership"));
                            }
                            super::body_labels::validate(&records, &classes, producer, operation.operation, u64::from(operation.label))?;
                        }
                        vec![body.clone()]
                    };
                    let interpolation = if sections.len() == 3 {
                        if sections.iter().any(|s| s.face.is_some()) {
                            return Err(invalid("three-section native loft requires circular sketch profiles"));
                        }
                        // The kernel validates coaxial complete circles on every
                        // evaluation, including after parameter/plane edits.
                        solvecraft_doc::LoftInterpolation::CircularChord
                    } else {
                        solvecraft_doc::LoftInterpolation::Uniform
                    };
                    FeatureKind::Loft { sections, interpolation, operation, targets }
                }
                HistoryKind::Extrude { operation, profile_containers } => {
                    let selected = selected(&records, &profiles, f.id, &profile_containers)?;
                    let [profile] = selected.as_slice() else {
                        return Err(invalid("multiple-profile extrusion reconstruction is not supported yet"));
                    };
                    let (sketch, plane) = sketches.get(&profile.geometry).copied().ok_or_else(|| invalid("extrusion precedes its source sketch"))?;
                    let extent = extent(&records, &classes, &bindings, f.id, &plane)?;
                    source_profiles.insert(f.id, *profile);
                    let targets = if operation == Operation::NewBody {
                        let bodies: Vec<_> = profile
                            .regions
                            .iter()
                            .map(|_| {
                                body_number += 1;
                                format!("Body{body_number}")
                            })
                            .collect();
                        if !f.suppressed {
                            active_bodies.extend(bodies.iter().cloned());
                        }
                        new_body = Some(bodies);
                        vec![]
                    } else {
                        if active_bodies.len() != 1 {
                            return Err(invalid("multi-body boolean target reconstruction is not supported yet"));
                        }
                        active_bodies.clone()
                    };
                    FeatureKind::Extrude { sketch, profiles: profile_selection(profile, &circles)?, extent, operation, targets }
                }
            };
            let id = doc.add_feature(kind, Some(&f.name))?;
            feature_ids.insert(f.id, id);
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
                let Some(p) = named_parameters.get(expression.as_str()).filter(|p| p.owner == Some(f.id)) else { continue };
                // SolveCraft feature inputs use mm/degrees. Preserve the existing
                // unit-bearing parameter indirection for other display units until
                // feature inputs support their own unit contexts.
                if solvecraft_doc::expr::unit_info(&p.display_unit) != solvecraft_doc::expr::unit_info(kind.default_unit()) {
                    continue;
                }
                *name = p.name.clone();
                *expression = p.expression.clone();
                transferred.insert(p.name.clone());
                if !p.comment.is_empty() {
                    doc.param_comments.insert(p.name.clone(), p.comment.clone());
                }
            }
            if let Some(body) = new_body {
                feature.body_names = body;
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

pub(super) fn angle_constraint(sketch: &Sketch, a: usize, b: usize, value: f64, quadrant: u32) -> Result<ConstraintKind> {
    let direction = |i: usize| {
        let Some(solvecraft_doc::sketch::CurveKind::Line { a, b }) = sketch.curves.get(i).map(|c| &c.kind) else {
            return Err(invalid("angular dimension requires finite lines"));
        };
        let start = sketch.point(*a).ok_or_else(|| invalid("missing angular line endpoint"))?;
        let end = sketch.point(*b).ok_or_else(|| invalid("missing angular line endpoint"))?;
        (end - start).normalized().ok_or_else(|| invalid("degenerate angular dimension line"))
    };
    let da = direction(a)?;
    let db = direction(b)?;
    let cross = da.cross(db);
    let direct = cross.abs().atan2(da.dot(db));
    let flip = quadrant % 2 == 1;
    let expected = if flip { std::f64::consts::PI - value } else { value };
    if !direct.is_finite() || (direct - expected).abs() > 1e-7 {
        return Err(invalid("angular dimension quadrant disagrees with native line geometry"));
    }
    // Odd quadrants measure the supplement. Swapping the directed lines retains
    // clockwise dimensions, including at 90 degrees where cached angles coincide.
    let (a, b) = if (cross < 0.) != flip { (b, a) } else { (a, b) };
    Ok(ConstraintKind::Angle { a, b, value, flip })
}

fn fix_line_support(sketch: &mut Sketch, line: usize, id: u64) -> Result<()> {
    let Some(solvecraft_doc::sketch::CurveKind::Line { a, b }) = sketch.curves.get(line).map(|c| &c.kind) else {
        return Err(invalid("fixed support requires a line"));
    };
    let start = sketch.point(*a).ok_or_else(|| invalid("missing fixed line endpoint"))?;
    let end = sketch.point(*b).ok_or_else(|| invalid("missing fixed line endpoint"))?;
    // Native curve fixation holds the infinite support, independently of endpoint
    // fixation. SolidCraft's curve.fixed holds both endpoints and would lose DOFs.
    let anchor = sketch.add_point(start, Some(&format!("line{id}_support"))).map_err(|e| invalid(&e.to_string()))?;
    sketch.points.get_mut(anchor).ok_or_else(|| invalid("missing fixed support anchor"))?.fixed = true;
    for kind in [ConstraintKind::PointOnCurve { p: anchor, c: line }, ConstraintKind::LineAngle { l: line, value: (end - start).angle() }] {
        sketch.add_constraint(kind, None).map_err(|e| invalid(&e.to_string()))?;
    }
    Ok(())
}

pub(super) fn line_midpoints(sketch: &mut Sketch, lines: [usize; 2], id: u64) -> Result<()> {
    let center = |line: usize| {
        let Some(solvecraft_doc::sketch::CurveKind::Line { a, b }) = sketch.curves.get(line).map(|c| &c.kind) else {
            return Err(invalid("common midpoint requires two lines"));
        };
        let a = sketch.point(*a).ok_or_else(|| invalid("missing midpoint endpoint"))?;
        let b = sketch.point(*b).ok_or_else(|| invalid("missing midpoint endpoint"))?;
        Ok((a + b) * 0.5)
    };
    let [a, b] = lines;
    let (ma, mb) = (center(a)?, center(b)?);
    if !ma.is_finite() || !mb.is_finite() || (ma - mb).len() > 1e-7 * ma.len().max(mb.len()).max(1.) {
        return Err(invalid("line midpoints disagree with native geometry"));
    }
    // A shared, unfixed point equates the centers without constraining line
    // angle or length. Native line-to-line midpoint allows independent rotation.
    let p = sketch.add_point(ma, Some(&format!("midpoint{id}"))).map_err(|e| invalid(&e.to_string()))?;
    for l in lines {
        sketch.add_constraint(ConstraintKind::Midpoint { p, l }, None).map_err(|e| invalid(&e.to_string()))?;
    }
    Ok(())
}

fn xy_normal(normal: &[f64; 3]) -> bool {
    normal.iter().zip([0., 0., 1.]).all(|(value, expected)| value.is_finite() && (value - expected).abs() <= 1e-10)
}

fn profile_selection(profile: &SketchProfile, circles: &BTreeMap<u64, SketchCircle>) -> Result<ProfileSel> {
    if profile.regions.iter().flatten().flat_map(|boundary| &boundary.pieces).any(|(_, count)| *count != 1) {
        let boundary = |boundary: &ProfileLoop| -> Result<Vec<solvecraft_doc::sketch::ProfilePiece>> {
            let mut pieces = Vec::new();
            for ((id, (piece, count)), reversed) in boundary.curves.iter().zip(&boundary.pieces).zip(&boundary.reversed) {
                pieces.push(solvecraft_doc::sketch::ProfilePiece { curve: format!("c{id}"), piece: *piece, count: *count, reversed: *reversed });
            }
            pieces.sort();
            Ok(pieces)
        };
        let mut profiles = Vec::new();
        for loops in &profile.regions {
            let outer = loops.iter().find(|l| l.outer).ok_or_else(|| invalid("missing outer profile boundary"))?;
            let mut holes = loops.iter().filter(|l| !l.outer).map(boundary).collect::<Result<Vec<_>>>()?;
            holes.sort();
            profiles.push(solvecraft_doc::PieceProfileBoundary { outer: boundary(outer)?, holes });
        }
        let mut circle_seams = BTreeMap::new();
        for id in profile.regions.iter().flatten().flat_map(|boundary| &boundary.curves) {
            if let Some(circle) = circles.get(id) {
                let angle = circle.x_axis[1].atan2(circle.x_axis[0]).rem_euclid(std::f64::consts::TAU);
                if angle > 1e-12 && angle < std::f64::consts::TAU - 1e-12 {
                    circle_seams.insert(format!("c{id}"), angle);
                }
            }
        }
        return Ok(ProfileSel::PieceBoundaries { profiles, circle_seams });
    }
    let ids = |boundary: &ProfileLoop| {
        let mut ids: Vec<_> = boundary.curves.iter().map(|id| format!("c{id}")).collect();
        ids.sort();
        ids
    };
    let mut profiles = Vec::new();
    for loops in &profile.regions {
        let outer = loops.iter().find(|l| l.outer).ok_or_else(|| invalid("missing outer profile boundary"))?;
        let mut holes: Vec<_> = loops.iter().filter(|l| !l.outer).map(ids).collect();
        holes.sort();
        profiles.push(solvecraft_doc::WholeProfileBoundary { outer: ids(outer), holes });
    }
    Ok(ProfileSel::WholeBoundaries { profiles })
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

/// Offset-only origin-plane chains keep a fixed normal. An in-plane rotation
/// about that normal preserves their editable position and native sketch basis.
pub(super) fn sketch_plane_reference(base: PlaneRef, cached: Plane, sketch: Plane) -> Result<PlaneRef> {
    if !super::planes::same_surface(cached, sketch) {
        return Err(invalid("unsupported sketch frame relative to its construction plane"));
    }
    if (cached.x - sketch.x).len() <= 1e-9 && (cached.y - sketch.y).len() <= 1e-9 {
        return Ok(base);
    }
    let normal = cached.normal();
    if cached.origin.cross(normal).len() > 1e-7 {
        return Err(invalid("rotated sketch requires an origin-plane offset chain"));
    }
    let angle = sketch.x.dot(cached.y).atan2(sketch.x.dot(cached.x));
    let (sin, cos) = angle.sin_cos();
    if (cached.x * cos + cached.y * sin - sketch.x).len() > 1e-9 || (cached.y * cos - cached.x * sin - sketch.y).len() > 1e-9 {
        return Err(invalid("unsupported sketch basis rotation"));
    }
    Ok(PlaneRef::AtAngle { base: Box::new(base), axis_origin: Vec3::default(), axis_dir: normal, angle: format!("{} deg", angle.to_degrees()) })
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
    let input = super::history::extrude_inputs(&mut r, classes)?;
    if (input.axis - plane.normal()).len() > 1e-9 {
        return Err(invalid("extrusion axis differs from its sketch normal"));
    }
    let start_offset = input
        .start_offset
        .map(|binding| {
            let p = parameters
                .get(&binding)
                .filter(|p| p.owner == Some(id) && p.label == "ProfileOffset")
                .ok_or_else(|| invalid("missing extrusion start-offset binding"))?;
            if solvecraft_doc::expr::Kind::from_unit(&p.display_unit) != solvecraft_doc::expr::Kind::Length {
                return Err(invalid("extrusion start offset requires a length parameter"));
            }
            Ok(p.name.clone())
        })
        .transpose()?;
    let distance = parameters
        .get(&input.distance)
        .filter(|p| p.owner == Some(id) && p.label == "AlongDistance")
        .ok_or_else(|| invalid("missing extrusion distance binding"))?;
    let taper = parameters
        .get(&input.taper)
        .filter(|p| p.owner == Some(id) && p.label == "TaperAngle")
        .ok_or_else(|| invalid("missing extrusion taper binding"))?;
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
        start_offset,
        through_all: false,
        taper: Some(taper.name.clone()),
        to: None,
        to_offset: None,
        from: None,
    })
}

pub(super) fn symmetry(
    sketch: &Sketch,
    points: &BTreeMap<u64, usize>,
    curves: &BTreeMap<u64, usize>,
    axis: u64,
    entities: [u64; 2],
) -> Result<Vec<ConstraintKind>> {
    use solvecraft_doc::sketch::CurveKind;
    let [a, b] = entities;
    let l = curves.get(&axis).copied().ok_or_else(|| invalid("missing symmetry axis"))?;
    if let (Some(p), Some(q)) = (points.get(&a), points.get(&b)) {
        return Ok(vec![ConstraintKind::Symmetric { p: *p, q: *q, l }]);
    }
    let ca = curves.get(&a).copied().ok_or_else(|| invalid("missing symmetric curve"))?;
    let cb = curves.get(&b).copied().ok_or_else(|| invalid("missing symmetric curve"))?;
    let kind = |i| sketch.curves.get(i).map(|c: &solvecraft_doc::sketch::Curve| &c.kind).ok_or_else(|| invalid("missing symmetry curve"));
    match (kind(ca)?, kind(cb)?) {
        (CurveKind::Circle { c: p, .. }, CurveKind::Circle { c: q, .. }) => {
            Ok(vec![ConstraintKind::Symmetric { p: *p, q: *q, l }, ConstraintKind::Equal { a: ca, b: cb }])
        }
        (CurveKind::Line { a: p0, b: p1 }, CurveKind::Line { a: q0, b: q1 }) => {
            let CurveKind::Line { a: la, b: lb } = kind(l)? else { return Err(invalid("symmetry axis is not a line")) };
            let point = |i| sketch.points.get(i).map(|p: &solvecraft_doc::sketch::SPoint| p.pos).ok_or_else(|| invalid("missing symmetry point"));
            let start = point(*la)?;
            let direction = point(*lb)? - start;
            let length2 = direction.len2();
            if !length2.is_finite() || length2 < 1e-20 {
                return Err(invalid("degenerate symmetry axis"));
            }
            let reflect = |p: Vec2| start + direction * (2. * (p - start).dot(direction) / length2) - (p - start);
            let [r0, r1] = [reflect(point(*p0)?), reflect(point(*p1)?)];
            let close = |p: Vec2, q: Vec2| (p - q).len() <= 1e-7 * p.len().max(q.len()).max(1.);
            let (q0, q1) = if close(r0, point(*q0)?) && close(r1, point(*q1)?) {
                (*q0, *q1)
            } else if close(r0, point(*q1)?) && close(r1, point(*q0)?) {
                (*q1, *q0)
            } else {
                return Err(invalid("symmetric line endpoints disagree with native geometry"));
            };
            Ok(vec![ConstraintKind::Symmetric { p: *p0, q: q0, l }, ConstraintKind::Symmetric { p: *p1, q: q1, l }])
        }
        _ => Err(invalid("unsupported symmetric entity types")),
    }
}
