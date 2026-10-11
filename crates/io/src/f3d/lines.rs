//! Finite sketch lines, verified with rectangle edits and isolated flag controls.
use super::history::{empty, finish, payload, require_class, zeros};
use super::*;
use solvecraft_geom::Vec3;

#[derive(Debug, PartialEq)]
pub struct SketchLine {
    pub id: u64,
    pub sketch: u64,
    pub start_point: u64,
    pub end_point: u64,
    pub start: [f64; 3],
    pub end: [f64; 3],
    pub fixed: bool,
    pub construction: bool,
    pub centerline: bool,
    pub reference: bool,
    pub profile_id: Option<(u64, u64)>,
}

impl Design {
    pub fn sketch_lines(&self) -> Result<Vec<SketchLine>> {
        if self.layout != 8 {
            return Err(invalid("line decoding requires metadata layout 8"));
        }
        let Some(class) = self.classes.iter().find(|c| c.id == "DCA267ED-D615-4934-B64F-AD805E8003E2") else { return Ok(vec![]) };
        if class.version != 2 {
            return Err(invalid("unsupported sketch line version"));
        }
        let records = Records::new(self);
        let classes: BTreeMap<_, _> = self.classes.iter().flat_map(|c| c.objects.iter().map(move |id| (*id, c))).collect();
        class
            .objects
            .iter()
            .map(|id| {
                let mut r = payload(records.primary(*id)?, *id)?;
                empty(&mut r)?;
                zeros(&mut r, 1)?;
                let attributes = attributes::read(&mut r)?;
                let mut number = || -> Result<f64> {
                    let n = f64::from_bits(r.u64()?);
                    if !n.is_finite() {
                        return Err(invalid("non-finite line geometry"));
                    }
                    Ok(n)
                };
                let start = Vec3::new(number()?, number()?, number()?);
                let delta = Vec3::new(number()?, number()?, number()?);
                let direction = Vec3::new(number()?, number()?, number()?);
                for _ in 0..3 {
                    number()?;
                } // auxiliary frame; endpoints establish the line
                let length = delta.len();
                if !length.is_finite() || length < 1e-12 || (direction.len() - 1.).abs() > 1e-9 || (delta - direction * length).len() > length * 1e-9
                {
                    return Err(invalid("inconsistent finite line direction"));
                }
                let end = start + delta;
                if !end.is_finite() {
                    return Err(invalid("non-finite line endpoint"));
                }
                let end_point = r.reference()?;
                let start_point = r.reference()?;
                if start_point == end_point {
                    return Err(invalid("coincident line endpoints"));
                }
                for p in [start_point, end_point] {
                    require_class(&classes, p, "C2CEDAE7-1716-47C1-B7B1-07B70081D0FB", 11)?;
                }
                r.take(2)?;
                let fixed = points::fixed(&mut r)?;
                let reference = points::boolean(&mut r)?;
                r.take(2)?;
                let (construction, centerline) = match r.take(1)? {
                    [0] => (false, false),
                    [1] => (true, false),
                    [2] => (false, true),
                    _ => return Err(invalid("unsupported line construction flag")),
                };
                r.take(34)?;
                let sketch = r.reference()?;
                require_class(&classes, sketch, "44A64366-4BD3-4B24-881A-F94C206E8F2D", 18)?;
                finish(&r)?;
                let profile_id = match (attributes.get("crv_primary_id"), attributes.get("crv_secondary_id")) {
                    (Some(a), Some(b)) => Some((*a, *b)),
                    (None, None) => None,
                    _ => return Err(invalid("incomplete line profile identity")),
                };
                Ok(SketchLine {
                    id: *id,
                    sketch,
                    start_point,
                    end_point,
                    start: [start.x, start.y, start.z],
                    end: [end.x, end.y, end.z],
                    fixed,
                    construction,
                    centerline,
                    reference,
                    profile_id,
                })
            })
            .collect()
    }
}
