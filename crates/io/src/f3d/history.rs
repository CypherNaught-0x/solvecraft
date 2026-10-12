//! Schemas verified with local reorder, rename, suppression and operation controls.
//! These are inspected relationships, not a complete feature reconstruction.
use super::*;
use solvecraft_doc::Operation;

const TIMELINE: &str = "2F4C1849-1A5A-4F6C-A086-8DD445CBF94B";
const SKETCH: &str = "8DA771B7-52ED-42FC-94E2-2353FE141373";
const EXTRUDE: &str = "DD405BC2-D673-44F0-8833-5CB2A1C186C7";
const GEOMETRY: &str = "44A64366-4BD3-4B24-881A-F94C206E8F2D";
const TRANSFORM: &str = "F47A46FB-DA27-4AFA-8A72-1B38FA596E23";

#[derive(Debug, PartialEq)]
pub struct HistoryFeature {
    pub id: u64,
    pub name: String,
    pub suppressed: bool,
    pub dependencies: Vec<u64>,
    pub kind: HistoryKind,
}

#[derive(Debug, PartialEq)]
pub enum HistoryKind {
    Sketch { geometry: u64, placement: SketchPlacement },
    OffsetPlane { geometry: u64, definition: u64, extra_offset: Option<u64> },
    Extrude { operation: Operation, profile_containers: Vec<u64> },
}

#[derive(Debug, PartialEq)]
pub struct SketchPlacement {
    pub support: u64,
    /// Row-major local-to-world matrix; translations are native centimetres.
    pub matrix: [f64; 16],
    /// The placement's owning object. Assembly placement is not yet reconstructed.
    pub owner: u64,
    pub entities: Vec<u64>,
    pub projections: Vec<u64>,
    /// Version-17 secondary entity metadata, retained for investigation rather
    /// than discarded. Reconstruction rejects it until its semantics are known.
    pub legacy_entity_metadata: Vec<(u64, u32)>,
}

#[derive(Debug)]
pub struct TimelineEntry {
    pub id: u64,
    pub class: String,
    pub version: u32,
    /// A failure remains attached to its actual timeline position. Inspection
    /// can expose later supported records without treating the history as complete.
    pub feature: Result<HistoryFeature>,
}

impl Design {
    /// The actual timeline, including suppressed features. Neither registration
    /// order nor the evaluation dependency graph is a substitute for this list.
    pub fn history(&self) -> Result<Vec<HistoryFeature>> {
        self.timeline()?.into_iter().map(|entry| entry.feature).collect()
    }

    pub fn timeline(&self) -> Result<Vec<TimelineEntry>> {
        if !matches!(self.layout, 4 | 5 | 8) {
            return Err(invalid("unsupported history metadata layout"));
        }
        let class = self.classes.iter().find(|c| c.id == TIMELINE).ok_or_else(|| invalid("missing timeline"))?;
        if !matches!(class.version, 2 | 3) || class.objects.len() != 1 {
            return Err(invalid("unsupported timeline class version or count"));
        }
        let records = Records::new(self);
        let classes: BTreeMap<_, _> = self.classes.iter().flat_map(|c| c.objects.iter().map(move |id| (*id, c))).collect();
        let timeline_id = class.objects.first().copied().ok_or_else(|| invalid("missing timeline identity"))?;
        let mut r = payload(records.primary(timeline_id)?, timeline_id)?;
        empty(&mut r)?;
        zeros(&mut r, 2)?;
        let owner = r.reference()?;
        require_class(&classes, owner, "C3423E04-7E85-4B44-8E31-A5FA9C100052", 2)?;
        let ids = references(&mut r)?;
        finish(&r)?;
        if ids.len() > solvecraft_doc::MAX_FEATURES {
            return Err(invalid("too many native timeline features"));
        }
        let inherited_version = self
            .classes
            .iter()
            .find(|c| c.id == "A0A15D26-1F3B-4120-A3F1-9CDDA189AB74")
            .map(|c| c.version)
            .unwrap_or(if self.layout == 8 { 2 } else { 0 });
        let body_feature_version = self
            .classes
            .iter()
            .find(|c| c.id == "2EDC2536-D95B-4469-8ACA-DC07124D9C43")
            .map(|c| c.version)
            .unwrap_or(if self.layout == 8 { 2 } else { 0 });
        let mut seen = BTreeSet::new();
        ids.into_iter()
            .map(|id| {
                if !seen.insert(id) {
                    return Err(invalid("duplicate timeline feature"));
                }
                let class = classes.get(&id).ok_or_else(|| invalid("missing timeline feature"))?;
                let feature = (|| {
                    let mut r = payload(records.primary(id)?, id)?;
                    let (kind, custom) = match (class.id.as_str(), class.version) {
                        (SKETCH, version @ 3..=5) => {
                            empty(&mut r)?;
                            zeros(&mut r, 1)?;
                            if !super::attributes::read(&mut r)?.is_empty() {
                                return Err(invalid("unsupported sketch feature attributes"));
                            }
                            zeros(&mut r, 1)?;
                            let bindings = references(&mut r)?;
                            validate_refs(&classes, &bindings)?;
                            r.u32()?; // evaluation state, not timeline position
                            zeros(&mut r, 1)?;
                            let geometry = r.reference()?;
                            require_class(&classes, geometry, GEOMETRY, version + 13)?;
                            let plane = r.reference()?;
                            validate_refs(&classes, &[plane])?;
                            if version >= 5 {
                                zeros(&mut r, 4)?;
                            }
                            let projections = references(&mut r)?;
                            for projection in &projections {
                                require_class(
                                    &classes,
                                    *projection,
                                    super::projections::CLASS,
                                    match version {
                                        3 => 1,
                                        4 => 3,
                                        _ => 4,
                                    },
                                )?;
                            }
                            if version >= 4 {
                                zeros(&mut r, 4)?;
                            }
                            let (mut placement, name) = placement(&records, &classes, geometry, version + 13)?;
                            placement.support = plane;
                            placement.projections = projections;
                            (HistoryKind::Sketch { geometry, placement }, name)
                        }
                        ("D869265F-F339-4751-A066-91D30F81FF08", version @ (2 | 4)) => {
                            empty(&mut r)?;
                            zeros(&mut r, 1)?;
                            if !super::attributes::read(&mut r)?.is_empty() {
                                return Err(invalid("unsupported plane feature attributes"));
                            }
                            if r.u32()? != 1 {
                                return Err(invalid("unsupported plane binding flags"));
                            }
                            if version >= 4 {
                                zeros(&mut r, 1)?;
                            }
                            let extra_offset = match r.bytes.get(r.at) {
                                Some(0) => {
                                    r.take(1)?;
                                    None
                                }
                                Some(1) => {
                                    let binding = r.reference()?;
                                    require_class(&classes, binding, "D91D429C-1BB5-4640-B49F-69AF14F463E9", 0)?;
                                    Some(binding)
                                }
                                _ => return Err(invalid("unsupported plane extra offset")),
                            };
                            if r.take(1)? != [1] {
                                return Err(invalid("unsupported plane flags"));
                            }
                            let geometry = r.reference()?;
                            require_class(&classes, geometry, super::planes::GEOMETRY, 1)?;
                            let definition = r.reference()?;
                            require_class(&classes, definition, super::planes::DEFINITION, if version == 2 { 8 } else { 15 })?;
                            if r.take(1)? != [u8::from(extra_offset.is_some())] {
                                return Err(invalid("unsupported plane definition flags"));
                            }
                            (HistoryKind::OffsetPlane { geometry, definition, extra_offset }, String::new())
                        }
                        (EXTRUDE, 8) => {
                            let input = extrude_inputs(&mut r, &classes)?;
                            let operation = input.operation;
                            let profile_containers = references(&mut r)?;
                            for p in &profile_containers {
                                require_class(&classes, *p, "2CA5A1CD-C99B-4C9B-91AF-57989148E841", 1)?;
                            }
                            // Versioned role-specific object lists: the populated list
                            // changes for booleans. Their roles remain uninterpreted.
                            let role_count = match body_feature_version {
                                1 => 3,
                                2 => 4,
                                _ => return Err(invalid("unsupported inherited extrusion schema")),
                            };
                            for _ in 0..role_count {
                                validate_refs(&classes, &references(&mut r)?)?;
                            }
                            (HistoryKind::Extrude { operation, profile_containers }, String::new())
                        }
                        _ => return Err(invalid(&format!("unsupported timeline feature {id}: class {} version {}", class.id, class.version))),
                    };
                    r.wide_string()?; // inherited external identity
                    r.take(3)?;
                    let dependencies = references(&mut r)?;
                    validate_refs(&classes, &dependencies)?;
                    r.u32()?; // evaluation cache status
                    let base = r.wide_string()?;
                    let sequence = r.u32()?;
                    r.u32()?;
                    let feature_name = r.wide_string()?;
                    let (length, suppression) = match inherited_version {
                        1 => (65, 50),
                        2 => (66, 51),
                        _ => return Err(invalid("unsupported inherited feature schema")),
                    };
                    let tail = r.take(length)?;
                    let suppressed = match (tail.get(5), tail.get(suppression)) {
                        (Some(0), Some(0)) => false,
                        (Some(1), Some(1)) => true,
                        _ => return Err(invalid("inconsistent feature suppression flags")),
                    };
                    finish(&r)?;
                    let name = if !custom.is_empty() {
                        custom
                    } else if !feature_name.is_empty() {
                        feature_name
                    } else {
                        format!("{}{sequence}", if base == "WorkPlane" { "Plane" } else { &base })
                    };
                    Ok(HistoryFeature { id, name, suppressed, dependencies, kind })
                })();
                Ok(TimelineEntry { id, class: class.id.clone(), version: class.version, feature })
            })
            .collect()
    }
}

pub(super) struct ExtrudeInputs {
    pub operation: Operation,
    pub axis: solvecraft_geom::Vec3,
    pub distance: u64,
    pub taper: u64,
    pub start_offset: Option<u64>,
}

/// Consume the verified one-sided, profile-plane/offset-start schema, stopping
/// at the profile container lists. Shared by inspection and reconstruction.
pub(super) fn extrude_inputs(r: &mut Reader<'_>, classes: &BTreeMap<u64, &Class>) -> Result<ExtrudeInputs> {
    empty(r)?;
    zeros(r, 1)?;
    if !super::attributes::read(r)?.is_empty() {
        return Err(invalid("unsupported extrusion attributes"));
    }
    r.take(3)?; // evaluation metadata
    let operation = operation(r.u32()?)?;
    if r.u32()? != 1 {
        return Err(invalid("unsupported extrusion extent layout (observed one-sided schema required)"));
    }
    if r.u32()? != 2 {
        return Err(invalid("unsupported extrusion distance extent"));
    }
    zeros(r, 1)?;
    if r.take(1)? != [1] {
        return Err(invalid("unsupported extrusion start type"));
    }
    let offset = match r.take(1)? {
        [0] => false,
        [1] => true,
        _ => return Err(invalid("unsupported extrusion start-offset flag")),
    };
    zeros(r, 3)?;
    let axis = solvecraft_geom::Vec3::new(f64::from_bits(r.u64()?), f64::from_bits(r.u64()?), f64::from_bits(r.u64()?));
    if !axis.is_finite() {
        return Err(invalid("non-finite extrusion axis"));
    }
    let start_offset = if offset {
        let binding = r.reference()?;
        require_class(classes, binding, "D91D429C-1BB5-4640-B49F-69AF14F463E9", 0)?;
        zeros(r, 1)?;
        Some(binding)
    } else {
        zeros(r, 2)?;
        None
    };
    let distance = r.reference()?;
    require_class(classes, distance, "D91D429C-1BB5-4640-B49F-69AF14F463E9", 0)?;
    require_class(classes, r.reference()?, "0897AF07-EF1B-42E5-BDD9-1C746A085CBC", 0)?;
    zeros(r, 1)?;
    let taper = r.reference()?;
    require_class(classes, taper, "D91D429C-1BB5-4640-B49F-69AF14F463E9", 0)?;
    zeros(r, 1)?;
    if r.u32()? != 1 {
        return Err(invalid("unsupported extrusion extent flags"));
    }
    zeros(r, 36)?;
    Ok(ExtrudeInputs { operation, axis, distance, taper, start_offset })
}

fn operation(value: u32) -> Result<Operation> {
    match value {
        1 => Ok(Operation::Join),
        2 => Ok(Operation::Cut),
        3 => Ok(Operation::Intersect),
        4 => Ok(Operation::NewBody),
        _ => Err(invalid("unsupported extrusion operation")),
    }
}

fn placement(records: &Records<'_>, classes: &BTreeMap<u64, &Class>, geometry: u64, version: u32) -> Result<(SketchPlacement, String)> {
    let mut r = payload(records.secondary(geometry)?, geometry)?;
    empty(&mut r)?;
    let transform = r.reference()?;
    require_class(classes, transform, TRANSFORM, 0)?;
    r.take(if version == 18 { 17 } else { 6 })?;
    let name = r.wide_string()?;
    zeros(&mut r, 1)?;
    let entities = references(&mut r)?;
    validate_refs(classes, &entities)?;
    let mut legacy_entity_metadata = Vec::new();
    if version == 17 {
        for _ in 0..r.count(15)? {
            let id = r.reference()?;
            validate_refs(classes, &[id])?;
            let flags = r.u32()?;
            if flags != 1 {
                return Err(invalid("unsupported legacy entity metadata"));
            }
            legacy_entity_metadata.push((id, flags));
        }
    }
    let parent = r.reference()?;
    validate_refs(classes, &[parent])?;
    finish(&r)?;
    let mut r = payload(records.primary(transform)?, transform)?;
    empty(&mut r)?;
    zeros(&mut r, 2)?;
    let mut matrix = [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.];
    match r.take(1)? {
        [1] => {}
        [0] => {
            for x in &mut matrix {
                *x = f64::from_bits(r.u64()?);
                if !x.is_finite() {
                    return Err(invalid("non-finite sketch transform"));
                }
            }
        }
        _ => return Err(invalid("unsupported sketch transform encoding")),
    }
    zeros(&mut r, 1)?;
    let owner = r.reference()?;
    validate_refs(classes, &[owner])?;
    finish(&r)?;
    Ok((SketchPlacement { support: 0, matrix, owner, entities, projections: vec![], legacy_entity_metadata }, name))
}

pub(super) fn payload(bytes: &[u8], id: u64) -> Result<Reader<'_>> {
    let mut r = Reader { bytes, at: 0 };
    r.string()?;
    if r.u64()? != id {
        return Err(invalid("history record identity mismatch"));
    }
    Ok(r)
}
pub(super) fn empty(r: &mut Reader<'_>) -> Result<()> {
    if !r.string()?.is_empty() {
        return Err(invalid("unsupported history attributes"));
    }
    Ok(())
}
pub(super) fn zeros(r: &mut Reader<'_>, n: usize) -> Result<()> {
    let at = r.at;
    if r.take(n)?.iter().any(|b| *b != 0) {
        return Err(invalid(&format!("unsupported history flags at record byte {at} ({n} bytes)")));
    }
    Ok(())
}
pub(super) fn references(r: &mut Reader<'_>) -> Result<Vec<u64>> {
    (0..r.count(11)?).map(|_| r.reference()).collect()
}
pub(super) fn finish(r: &Reader<'_>) -> Result<()> {
    if r.at != r.bytes.len() {
        return Err(invalid("unsupported history payload tail"));
    }
    Ok(())
}
pub(super) fn require_class(classes: &BTreeMap<u64, &Class>, id: u64, guid: &str, version: u32) -> Result<()> {
    if !classes.get(&id).is_some_and(|c| c.id == guid && c.version == version) {
        return Err(invalid("unsupported history reference class or version"));
    }
    Ok(())
}
fn validate_refs(classes: &BTreeMap<u64, &Class>, ids: &[u64]) -> Result<()> {
    if ids.iter().any(|id| !classes.contains_key(id)) {
        return Err(invalid("missing history reference"));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
