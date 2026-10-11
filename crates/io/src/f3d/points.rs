//! Sketch-local point coordinates and fixation verified with independent API edits.
use super::history::{empty, finish, payload, references, require_class, zeros};
use super::*;

#[derive(Debug, PartialEq)]
pub struct SketchPoint {
    pub id: u64,
    pub sketch: u64,
    pub position: [f64; 3],
    pub fixed: bool,
    pub origin: bool,
}

pub(super) fn fixed(r: &mut Reader<'_>) -> Result<bool> {
    match r.take(2)? {
        // The second flag is derived constraint state. A dimensioned, coincident
        // circle is constrained but remains editable: never turn that into Fix.
        [0, 0 | 1] => Ok(false),
        [1, 0 | 1] => Ok(true),
        _ => Err(invalid("unsupported fixation flags")),
    }
}

impl Design {
    pub fn sketch_points(&self) -> Result<Vec<SketchPoint>> {
        if self.layout != 8 {
            return Err(invalid("point decoding requires metadata layout 8"));
        }
        let Some(class) = self.classes.iter().find(|c| c.id == "C2CEDAE7-1716-47C1-B7B1-07B70081D0FB") else { return Ok(vec![]) };
        if class.version != 11 {
            return Err(invalid("unsupported sketch point version"));
        }
        let records = Records::new(self);
        let classes: BTreeMap<_, _> = self.classes.iter().flat_map(|c| c.objects.iter().map(move |id| (*id, c))).collect();
        class
            .objects
            .iter()
            .map(|id| {
                let mut r = payload(records.primary(*id)?, *id)?;
                empty(&mut r)?;
                if r.take(2)? != [0, 1] {
                    return Err(invalid("unsupported point attributes"));
                }
                for _ in 0..r.count(16)? {
                    r.string()?;
                    if r.string()? != "IntrinsicMetaTypeuint64" {
                        return Err(invalid("unsupported point attribute type"));
                    }
                    r.u64()?;
                }
                let connections = r.reference()?;
                require_class(&classes, connections, "362B7EC3-0F09-47C8-A3BE-DC066715CDAE", 0)?;
                zeros(&mut r, 3)?;
                let fixed = fixed(&mut r)?;
                let origin = match r.take(1)? {
                    [0] => false,
                    [1] => true,
                    _ => return Err(invalid("unsupported point origin flag")),
                };
                zeros(&mut r, 2)?;
                let mut number = || -> Result<f64> {
                    let n = f64::from_bits(r.u64()?);
                    if !n.is_finite() {
                        return Err(invalid("non-finite sketch point"));
                    }
                    Ok(n)
                };
                let position = [number()?, number()?, number()?];
                if origin && (!fixed || position != [0.; 3]) {
                    return Err(invalid("inconsistent sketch origin"));
                }
                r.take(30)?; // presentation fields
                if references(&mut r)? != [connections] {
                    return Err(invalid("unsupported point connection list"));
                }
                let sketch = r.reference()?;
                require_class(&classes, sketch, "44A64366-4BD3-4B24-881A-F94C206E8F2D", 18)?;
                finish(&r)?;
                Ok(SketchPoint { id: *id, sketch, position, fixed, origin })
            })
            .collect()
    }
}
