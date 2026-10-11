//! Observed version-9 parameter records. These describe parameters, not a timeline.
use super::*;

#[derive(Debug, PartialEq)]
pub struct Parameter {
    pub id: u64,
    pub binding: u64,
    pub owner: u64,
    pub name: String,
    pub expression: String,
    pub label: String,
    pub display_unit: String,
    /// Native numeric value: length is centimetres, angle is radians.
    pub value: f64,
}

impl Design {
    /// Decode the parameter schema observed in layout 8. Other versions fail
    /// explicitly; absence of decoded parameters never implies absence of history.
    pub fn parameters(&self) -> Result<Vec<Parameter>> {
        if self.layout != 8 {
            return Err(invalid("parameter decoding requires metadata layout 8"));
        }
        let Some(class) = self.classes.iter().find(|c| c.id == "7A6A3D31-BE74-4E19-9115-20642944C2E4") else {
            return Ok(Vec::new());
        };
        if class.version != 9 {
            return Err(invalid("unsupported parameter class version"));
        }
        let records = Records::new(self);
        let bindings: BTreeSet<_> = self
            .classes
            .iter()
            .filter(|c| c.id == "D91D429C-1BB5-4640-B49F-69AF14F463E9" && c.version == 0)
            .flat_map(|c| c.objects.iter().copied())
            .collect();
        class
            .objects
            .iter()
            .map(|id| {
                let mut p = decode(records.primary(*id)?)?;
                if p.id != *id || !bindings.contains(&p.binding) {
                    return Err(invalid("invalid parameter binding"));
                }
                let mut r = Reader { bytes: records.primary(p.binding)?, at: 0 };
                r.string()?;
                if r.u64()? != p.binding || !r.string()?.is_empty() || r.take(1)? != [1] || r.u32()? != 1 {
                    return Err(invalid("unsupported parameter binding payload"));
                }
                p.owner = r.reference()?;
                r.take(5)?; // property ordinal and flags
                let value = f64::from_bits(r.u64()?);
                if value != p.value || r.reference()? != p.id || !records.by_id.contains_key(&p.owner) {
                    return Err(invalid("inconsistent parameter owner or value"));
                }
                Ok(p)
            })
            .collect()
    }
}

fn decode(bytes: &[u8]) -> Result<Parameter> {
    let mut r = Reader { bytes, at: 0 };
    r.string()?; // per-file numeric tag
    let id = r.u64()?;
    if !r.string()?.is_empty() {
        return Err(invalid("unsupported parameter attributes"));
    }
    r.take(16)?; // inherited flags and unit identity; retained in the bulk stream
    r.u32()?; // parameter sequence number
    let binding = r.reference()?;
    let expression = r.wide_string()?;
    if r.take(10)?.iter().any(|b| *b != 0) {
        return Err(invalid("unsupported parameter expression metadata"));
    }
    let label = r.wide_string()?;
    if !r.wide_string()?.is_empty() {
        return Err(invalid("unsupported parameter comment"));
    }
    let display_unit = r.wide_string()?;
    let name = r.wide_string()?;
    let value = f64::from_bits(r.u64()?);
    if !value.is_finite() {
        return Err(invalid("non-finite parameter value"));
    }
    r.take(12)?; // inherited trailing flags; retained, not interpreted
    if r.at != bytes.len() {
        return Err(invalid("unsupported parameter payload tail"));
    }
    Ok(Parameter { id, binding, owner: 0, name, expression, label, display_unit, value })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wide(out: &mut Vec<u8>, s: &str) {
        let chars: Vec<_> = s.encode_utf16().collect();
        out.extend((chars.len() as u32).to_le_bytes());
        for c in chars {
            out.extend(c.to_le_bytes());
        }
    }
    fn parameter(value: f64) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend(2u32.to_le_bytes());
        out.extend(b"42");
        out.extend(123u64.to_le_bytes());
        out.extend(0u32.to_le_bytes());
        out.extend([0; 16]);
        out.extend(7u32.to_le_bytes());
        out.push(1);
        out.extend(124u64.to_le_bytes());
        out.extend([0; 2]);
        wide(&mut out, "130 mm");
        out.extend([0; 10]);
        wide(&mut out, "Length — Δ");
        wide(&mut out, "");
        wide(&mut out, "mm");
        wide(&mut out, "d7");
        out.extend(value.to_le_bytes());
        out.extend([0; 12]);
        out
    }

    #[test]
    fn parameter_values_and_utf16_survive_variable_tags() {
        let bytes = parameter(13.0);
        let p = decode(&bytes).unwrap();
        assert_eq!(p.id, 123);
        assert_eq!(p.binding, 124);
        assert_eq!(p.expression, "130 mm");
        assert_eq!(p.label, "Length — Δ");
        assert_eq!(p.name, "d7");
        assert_eq!(p.value, 13.0);
        for n in 0..bytes.len() {
            assert!(decode(&bytes[..n]).is_err(), "prefix {n}");
        }
        assert!(decode(&parameter(f64::NAN)).is_err());
        assert!(decode(&parameter(f64::INFINITY)).is_err());
        let mut extra = bytes;
        extra.push(0);
        assert!(decode(&extra).is_err());

        // Reference/driven dimensions carry nonzero expression metadata.
        // Until measurement dependencies are reconstructed, do not freeze them
        // into independent driving parameters.
        let mut driven = parameter(13.0);
        let metadata = 4 + 2 + 8 + 4 + 16 + 4 + 11 + 4 + "130 mm".len() * 2;
        driven[metadata + 6] = 1;
        assert!(decode(&driven).is_err());
    }
}
