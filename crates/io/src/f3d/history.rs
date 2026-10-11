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
    Extrude { operation: Operation, profile_containers: Vec<u64> },
}

#[derive(Debug, PartialEq)]
pub struct SketchPlacement {
    /// Row-major local-to-world matrix; translations are native centimetres.
    pub matrix: [f64; 16],
    /// The placement's owning object. Assembly placement is not yet reconstructed.
    pub owner: u64,
    pub entities: Vec<u64>,
}

impl Design {
    /// The actual timeline, including suppressed features. Neither registration
    /// order nor the evaluation dependency graph is a substitute for this list.
    pub fn history(&self) -> Result<Vec<HistoryFeature>> {
        if self.layout != 8 {
            return Err(invalid("history decoding requires metadata layout 8"));
        }
        let class = self.classes.iter().find(|c| c.id == TIMELINE).ok_or_else(|| invalid("missing timeline"))?;
        if class.version != 3 || class.objects.len() != 1 {
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
        let mut seen = BTreeSet::new();
        ids.into_iter()
            .map(|id| {
                if !seen.insert(id) {
                    return Err(invalid("duplicate timeline feature"));
                }
                let class = classes.get(&id).ok_or_else(|| invalid("missing timeline feature"))?;
                let mut r = payload(records.primary(id)?, id)?;
                let (kind, custom) = match (class.id.as_str(), class.version) {
                    (SKETCH, 5) => {
                        empty(&mut r)?;
                        zeros(&mut r, 3)?;
                        let bindings = references(&mut r)?;
                        validate_refs(&classes, &bindings)?;
                        r.u32()?; // evaluation state, not timeline position
                        zeros(&mut r, 1)?;
                        let geometry = r.reference()?;
                        require_class(&classes, geometry, GEOMETRY, 18)?;
                        let plane = r.reference()?;
                        validate_refs(&classes, &[plane])?;
                        r.take(12)?;
                        let (placement, name) = placement(&records, &classes, geometry)?;
                        (HistoryKind::Sketch { geometry, placement }, name)
                    }
                    (EXTRUDE, 8) => {
                        empty(&mut r)?;
                        if r.take(2)? != [0, 1] {
                            return Err(invalid("unsupported extrusion attributes"));
                        }
                        r.take(7)?;
                        let operation = operation(r.u32()?)?;
                        if r.u32()? != 1 {
                            return Err(invalid("unsupported extrusion extent layout (observed one-sided schema required)"));
                        }
                        // Extent/start/direction fields remain opaque. Do not use
                        // this inspector to synthesize a guessed distance extrusion.
                        r.take(111)?;
                        let profile_containers = references(&mut r)?;
                        for p in &profile_containers {
                            require_class(&classes, *p, "2CA5A1CD-C99B-4C9B-91AF-57989148E841", 1)?;
                        }
                        // Four role-specific object lists: the populated list
                        // changes for booleans. Their roles remain uninterpreted.
                        for _ in 0..4 {
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
                let tail = r.take(66)?;
                let suppressed = match (tail.get(5), tail.get(51)) {
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
                    format!("{base}{sequence}")
                };
                Ok(HistoryFeature { id, name, suppressed, dependencies, kind })
            })
            .collect()
    }
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

fn placement(records: &Records<'_>, classes: &BTreeMap<u64, &Class>, geometry: u64) -> Result<(SketchPlacement, String)> {
    let mut r = payload(records.secondary(geometry)?, geometry)?;
    empty(&mut r)?;
    let transform = r.reference()?;
    require_class(classes, transform, TRANSFORM, 0)?;
    r.take(17)?;
    let name = r.wide_string()?;
    zeros(&mut r, 1)?;
    let entities = references(&mut r)?;
    validate_refs(classes, &entities)?;
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
    Ok((SketchPlacement { matrix, owner, entities }, name))
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
    if r.take(n)?.iter().any(|b| *b != 0) {
        return Err(invalid("unsupported history flags"));
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
