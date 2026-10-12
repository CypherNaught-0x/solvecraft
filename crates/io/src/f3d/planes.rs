//! Offset-plane dependencies verified by signed, chained and XZ Fusion controls.
use super::history::{empty, finish, payload, references, require_class, zeros};
use super::*;
use solvecraft_geom::{Plane, Vec3};

pub(super) const GEOMETRY: &str = "8D424E29-DE23-4B1F-A3AD-84C52A497D1A";
pub(super) const DEFINITION: &str = "7B6C3C2D-4096-4E6D-B225-699E8C2D9355";

#[derive(Debug, PartialEq)]
pub struct OffsetPlaneDefinition {
    pub feature: u64,
    pub geometry: u64,
    pub base: u64,
    pub distance: u64,
    pub extra: Option<u64>,
    /// Cached placement in millimetres, checked separately against dependencies on import.
    pub plane: Plane,
}

impl Design {
    /// Inspect verified offset-plane dependencies even when unrelated timeline
    /// features remain unsupported. Unknown plane methods still fail explicitly.
    pub fn offset_planes(&self) -> Result<Vec<OffsetPlaneDefinition>> {
        let records = Records::new(self);
        let classes = self.classes.iter().flat_map(|c| c.objects.iter().map(move |id| (*id, c))).collect();
        let mut out = Vec::new();
        for entry in self.timeline()? {
            if entry.class != "D869265F-F339-4751-A066-91D30F81FF08" {
                continue;
            }
            let feature = entry.feature?;
            let HistoryKind::OffsetPlane { geometry, definition: id, extra_offset } = feature.kind else {
                return Err(invalid("unsupported plane history"));
            };
            let d = definition(&records, &classes, id, feature.id)?;
            if d.extra != extra_offset {
                return Err(invalid("inconsistent internal plane offset binding"));
            }
            out.push(OffsetPlaneDefinition {
                feature: feature.id,
                geometry,
                base: d.base,
                distance: d.distance.ok_or_else(|| invalid("missing plane distance"))?,
                extra: d.extra,
                plane: d.plane,
            });
        }
        Ok(out)
    }
}

pub(super) struct Definition {
    pub base: u64,
    pub distance: Option<u64>,
    pub extra: Option<u64>,
    pub plane: Plane,
}

pub(super) fn definition(records: &Records<'_>, classes: &BTreeMap<u64, &Class>, id: u64, owner: u64) -> Result<Definition> {
    let version = classes
        .get(&id)
        .filter(|class| class.id == DEFINITION && matches!(class.version, 8 | 15))
        .map(|class| class.version)
        .ok_or_else(|| invalid("unsupported plane definition class"))?;
    let mut r = payload(records.primary(id)?, id)?;
    empty(&mut r)?;
    let extra = if version == 8 {
        // Original layout-5 offset planes have no extra-offset slot; the
        // current public-API resave adds the version-15 header and null slot.
        zeros(&mut r, 24)?;
        None
    } else {
        zeros(&mut r, 36)?;
        if r.take(2)? != [1, 0] {
            return Err(invalid("unsupported plane definition header"));
        }
        match r.bytes.get(r.at) {
            Some(0) => {
                r.take(1)?;
                None
            }
            Some(1) => Some(r.reference()?),
            _ => return Err(invalid("unsupported plane extra offset")),
        }
    };
    zeros(&mut r, 7)?;
    let mut matrix = [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.];
    match r.take(1)? {
        [1] => {}
        [0] => {
            for n in &mut matrix {
                *n = f64::from_bits(r.u64()?);
            }
        }
        _ => return Err(invalid("unsupported plane transform encoding")),
    }
    let plane = super::import::plane(&matrix)?;
    zeros(&mut r, 35)?;
    if r.u32()? != 2 {
        return Err(invalid("unsupported construction-plane method"));
    }
    if version >= 15 {
        zeros(&mut r, 1)?;
    }
    let distance = match references(&mut r)?.as_slice() {
        [] => None,
        [id] => Some(*id),
        _ => return Err(invalid("unsupported plane distance bindings")),
    };
    let bases = references(&mut r)?;
    let [base] = bases.as_slice() else { return Err(invalid("unsupported plane base references")) };
    zeros(&mut r, 2)?;
    evaluation_tail(&mut r, classes, owner)?;
    let base = tracked_plane(records, classes, *base, owner)?;
    Ok(Definition { base, distance, extra, plane })
}

fn evaluation_tail(r: &mut Reader<'_>, classes: &BTreeMap<u64, &Class>, owner: u64) -> Result<()> {
    for id in references(r)? {
        if !classes.contains_key(&id) {
            return Err(invalid("missing plane evaluation reference"));
        }
    }
    r.take(60)?; // inherited evaluation state, stamps and presentation references
    if r.reference()? != owner {
        return Err(invalid("plane definition belongs to another feature"));
    }
    finish(r)
}

pub(super) fn tracked_plane(records: &Records<'_>, classes: &BTreeMap<u64, &Class>, id: u64, owner: u64) -> Result<u64> {
    require_class(classes, id, "5A1BF548-241F-46FD-9FB5-E4B05126EB9D", 1)?;
    let mut r = payload(records.primary(id)?, id)?;
    empty(&mut r)?;
    zeros(&mut r, 2)?;
    let tracker = r.reference()?;
    require_class(classes, tracker, "90055C05-546C-4EE7-B3C9-3DD922AD0C9C", 0)?;
    if r.u32()? != 1 {
        return Err(invalid("unsupported plane tracking layout"));
    }
    r.wide_string()?;
    r.wide_string()?;
    if r.u32()? != 2 {
        return Err(invalid("unsupported plane tracking kind"));
    }
    zeros(&mut r, 4)?;
    evaluation_tail(&mut r, classes, owner)?;
    let mut r = payload(records.primary(tracker)?, tracker)?;
    empty(&mut r)?;
    zeros(&mut r, 2)?;
    let entity = r.u64()?;
    finish(&r)?;
    require_class(classes, entity, GEOMETRY, 1)?;
    Ok(entity)
}

/// Only root origin planes may terminate an imported offset chain. A cached face
/// or a later construction plane must never become a frozen custom base plane.
pub(super) fn origin(records: &Records<'_>, classes: &BTreeMap<u64, &Class>, id: u64, root: u64) -> Result<(String, Plane)> {
    require_class(classes, id, GEOMETRY, 1)?;
    let mut r = payload(records.primary(id)?, id)?;
    empty(&mut r)?;
    if r.take(2)? != [0, 1] {
        return Err(invalid("unsupported origin-plane attributes"));
    }
    for _ in 0..r.count(16)? {
        r.string()?;
        if r.string()? != "IntrinsicMetaTypeuint64" {
            return Err(invalid("unsupported origin-plane attribute type"));
        }
        r.u64()?;
    }
    zeros(&mut r, 4)?; // origin role, distinct from generated plane geometry
    r.take(17)?; // display extents and flags
    let mut vector = || -> Result<Vec3> { Ok(Vec3::new(f64::from_bits(r.u64()?), f64::from_bits(r.u64()?), f64::from_bits(r.u64()?))) };
    let (o, x, y) = (vector()?, vector()?, vector()?);
    if !o.is_finite() || !x.is_finite() || !y.is_finite() {
        return Err(invalid("non-finite origin plane"));
    }
    r.take(26)?; // presentation extents and origin identity flags
    let name = r.wide_string()?;
    let plane = match name.as_str() {
        "XY" => Plane::XY,
        "XZ" => Plane::XZ,
        "YZ" => Plane::YZ,
        _ => return Err(invalid("unsupported origin-plane name")),
    };
    if o.len() > 1e-10 || (x.cross(y) - plane.normal()).len() > 1e-10 {
        return Err(invalid("origin plane name and geometry disagree"));
    }
    r.take(37)?;
    let group = r.reference()?;
    finish(&r)?;
    require_class(classes, group, "2972A3B8-3627-49BC-8C3D-18707605EC44", 1)?;
    let mut r = payload(records.primary(group)?, group)?;
    empty(&mut r)?;
    zeros(&mut r, 4)?;
    if !references(&mut r)?.contains(&id) || r.reference()? != root {
        return Err(invalid("plane is not part of the root origin"));
    }
    finish(&r)?;
    Ok((name, plane))
}

pub(super) fn same_surface(a: Plane, b: Plane) -> bool {
    (a.origin - b.origin).len() < 1e-7 && (a.normal() - b.normal()).len() < 1e-9
}
