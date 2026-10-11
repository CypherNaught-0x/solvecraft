//! Observed full-circle geometry. Coordinates are sketch-local centimetres;
//! placement, construction flags and constraints are deliberately not inferred.
use super::*;

#[derive(Debug, PartialEq)]
pub struct SketchCircle {
    pub id: u64,
    pub sketch: u64,
    pub center_point: u64,
    pub center: [f64; 3],
    pub normal: [f64; 3],
    pub x_axis: [f64; 3],
    pub radius: f64,
}

impl Design {
    pub fn sketch_circles(&self) -> Result<Vec<SketchCircle>> {
        if self.layout != 8 {
            return Err(invalid("circle decoding requires metadata layout 8"));
        }
        let Some(class) = self.classes.iter().find(|c| c.id == "F0130424-8B7E-4092-93C9-1CA807482534") else {
            return Ok(Vec::new());
        };
        if class.version != 0 {
            return Err(invalid("unsupported circle class version"));
        }
        let records = Records::new(self);
        let objects = |guid: &str, version| -> BTreeSet<_> {
            self.classes.iter().filter(|c| c.id == guid && c.version == version).flat_map(|c| c.objects.iter().copied()).collect()
        };
        let sketches = objects("44A64366-4BD3-4B24-881A-F94C206E8F2D", 18);
        let points = objects("C2CEDAE7-1716-47C1-B7B1-07B70081D0FB", 11);
        class
            .objects
            .iter()
            .map(|id| {
                let c = decode(records.primary(*id)?)?;
                if c.id != *id || !sketches.contains(&c.sketch) || !points.contains(&c.center_point) {
                    return Err(invalid("inconsistent circle references"));
                }
                Ok(c)
            })
            .collect()
    }
}

fn decode(bytes: &[u8]) -> Result<SketchCircle> {
    let mut r = Reader { bytes, at: 0 };
    r.string()?;
    let id = r.u64()?;
    if !r.string()?.is_empty() || r.take(2)? != [0, 1] {
        return Err(invalid("unsupported circle attributes"));
    }
    for _ in 0..r.count(16)? {
        r.string()?;
        if r.string()? != "IntrinsicMetaTypeuint64" {
            return Err(invalid("unsupported circle attribute type"));
        }
        r.u64()?;
    }
    let mut number = || -> Result<f64> {
        let x = f64::from_bits(r.u64()?);
        if !x.is_finite() {
            return Err(invalid("non-finite circle geometry"));
        }
        Ok(x)
    };
    let center = [number()?, number()?, number()?];
    let normal = [number()?, number()?, number()?];
    let x_axis = [number()?, number()?, number()?];
    let radius = number()?;
    let start = number()?;
    let end = number()?;
    let dot = |a: &[f64; 3], b: &[f64; 3]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f64>();
    if radius <= 0.0
        || start != 0.0
        || (end - std::f64::consts::TAU).abs() > 1e-12
        || (dot(&normal, &normal) - 1.0).abs() > 1e-10
        || (dot(&x_axis, &x_axis) - 1.0).abs() > 1e-10
        || dot(&normal, &x_axis).abs() > 1e-10
    {
        return Err(invalid("unsupported full-circle geometry"));
    }
    let center_point = r.reference()?;
    r.take(44)?; // Uninterpreted flags and presentation fields, retained in bulk.
    let sketch = r.reference()?;
    if r.at != bytes.len() {
        return Err(invalid("unsupported circle payload tail"));
    }
    Ok(SketchCircle { id, sketch, center_point, center, normal, x_axis, radius })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn circle(radius: f64) -> Vec<u8> {
        // Original synthetic data, deliberately not a copy of a Fusion record.
        let mut b = Vec::new();
        b.extend(1u32.to_le_bytes());
        b.push(b'7');
        b.extend(81u64.to_le_bytes());
        b.extend(0u32.to_le_bytes());
        b.extend([0, 1]);
        b.extend(0u32.to_le_bytes());
        for x in [1.5, -2.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, radius, 0.0, std::f64::consts::TAU] {
            b.extend(x.to_le_bytes());
        }
        b.push(1);
        b.extend(82u64.to_le_bytes());
        b.extend([0; 2]);
        b.extend([0; 44]);
        b.push(1);
        b.extend(83u64.to_le_bytes());
        b.extend([0; 2]);
        b
    }

    #[test]
    fn full_circle_keeps_local_coordinates_and_rejects_bad_payloads() {
        let b = circle(3.0);
        let c = decode(&b).unwrap();
        assert_eq!(c.center, [1.5, -2.0, 0.0]);
        assert_eq!((c.id, c.center_point, c.sketch), (81, 82, 83));
        assert_eq!(c.radius, 3.0);
        for n in 0..b.len() {
            assert!(decode(&b[..n]).is_err(), "prefix {n}");
        }
        for radius in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(decode(&circle(radius)).is_err());
        }
        let mut bad = b.clone();
        bad[23..31].copy_from_slice(&f64::NAN.to_le_bytes());
        assert!(decode(&bad).is_err());
        let mut bad = b.clone();
        bad[47..55].copy_from_slice(&2f64.to_le_bytes());
        assert!(decode(&bad).is_err());
        let mut bad = b;
        bad.push(0);
        assert!(decode(&bad).is_err());
    }
}
