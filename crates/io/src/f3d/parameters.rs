//! Parameter schemas verified with controlled edits and legacy/resaved pairs.
use super::*;

#[derive(Debug, PartialEq)]
pub struct Parameter {
    pub id: u64,
    /// User parameters have no feature binding or owner.
    pub binding: Option<u64>,
    pub owner: Option<u64>,
    pub name: String,
    pub expression: String,
    pub label: String,
    pub display_unit: String,
    pub comment: String,
    pub driven: bool,
    pub favorite: bool,
    /// Native numeric value: length is centimetres, angle is radians.
    pub value: f64,
}

impl Design {
    /// Decoding a driven parameter does not reconstruct its measurement dependency.
    /// Importers must reject it until they can preserve that dependency.
    pub fn parameters(&self) -> Result<Vec<Parameter>> {
        let Some(class) = self.classes.iter().find(|c| c.id == "7A6A3D31-BE74-4E19-9115-20642944C2E4") else {
            return Ok(Vec::new());
        };
        if !matches!((self.layout, class.version), (4, 5) | (5, 6 | 7) | (8, 9)) {
            return Err(invalid("unsupported parameter layout and class version"));
        }
        let records = Records::new(self);
        let bindings: BTreeMap<_, _> = self
            .classes
            .iter()
            .filter_map(|c| match (c.id.as_str(), c.version) {
                ("D91D429C-1BB5-4640-B49F-69AF14F463E9", 0) => Some((c, false)),
                ("A085449A-5144-4B2B-B455-7F7035A40559", 1) => Some((c, true)),
                _ => None,
            })
            .flat_map(|(c, integer)| c.objects.iter().map(move |id| (*id, integer)))
            .collect();
        class
            .objects
            .iter()
            .map(|id| {
                let mut p = decode(records.primary(*id)?, class.version)?;
                if p.id != *id {
                    return Err(invalid("invalid parameter identity"));
                }
                let Some(binding) = p.binding else { return Ok(p) };
                let integer = *bindings.get(&binding).ok_or_else(|| invalid("invalid parameter binding"))?;
                let mut r = Reader { bytes: records.primary(binding)?, at: 0 };
                r.string()?;
                if r.u64()? != binding || !r.string()?.is_empty() || r.take(1)? != [1] || r.u32()? != 1 {
                    return Err(invalid("unsupported parameter binding payload"));
                }
                let owner = r.reference()?;
                p.owner = Some(owner);
                r.take(5)?; // property ordinal and flags
                let value = if integer {
                    if r.take(1)? != [1] || solvecraft_doc::expr::Kind::from_unit(&p.display_unit) != solvecraft_doc::expr::Kind::Unitless {
                        return Err(invalid("unsupported integer parameter binding"));
                    }
                    f64::from(r.u32()?)
                } else {
                    f64::from_bits(r.u64()?)
                };
                if value != p.value || r.reference()? != p.id || !records.by_id.contains_key(&owner) {
                    return Err(invalid("inconsistent parameter owner or value"));
                }
                Ok(p)
            })
            .collect()
    }
}

fn decode(bytes: &[u8], version: u32) -> Result<Parameter> {
    let (flags, metadata_prefix, has_extra) = match version {
        5 => (3, 0, false),
        6 => (7, 0, false),
        7 => (8, 4, true),
        9 => (12, 5, true),
        _ => return Err(invalid("unsupported parameter class version")),
    };
    let mut r = Reader { bytes, at: 0 };
    r.string()?; // per-file numeric tag
    let id = r.u64()?;
    if !r.string()?.is_empty() {
        return Err(invalid("unsupported parameter attributes"));
    }
    r.take(flags)?; // inherited flags and unit identity
    let comment = r.wide_string()?;
    r.u32()?; // parameter sequence number
    let binding = match r.bytes.get(r.at) {
        Some(0) => {
            r.take(1)?;
            None
        }
        Some(1) => Some(r.reference()?),
        _ => return Err(invalid("unsupported nullable parameter binding")),
    };
    let expression = r.wide_string()?;
    if r.take(metadata_prefix)?.iter().any(|b| *b != 0) {
        return Err(invalid("unsupported parameter expression metadata"));
    }
    let (driven, favorite, user) = match r.take(5)? {
        [0, driven @ (0 | 1), favorite @ (0 | 1), 0, user @ (0 | 1)] => (*driven != 0, *favorite != 0, *user != 0),
        _ => return Err(invalid("unsupported parameter expression metadata")),
    };
    if user != binding.is_none() || (user && driven) {
        return Err(invalid("inconsistent parameter role and binding"));
    }
    let label = r.wide_string()?;
    if has_extra && !r.wide_string()?.is_empty() {
        return Err(invalid("unsupported parameter extra metadata"));
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
    Ok(Parameter { id, binding, owner: None, name, expression, label, display_unit, comment, driven, favorite, value })
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
        let p = decode(&bytes, 9).unwrap();
        assert_eq!(p.id, 123);
        assert_eq!(p.binding, Some(124));
        assert_eq!(p.expression, "130 mm");
        assert_eq!(p.label, "Length — Δ");
        assert_eq!(p.name, "d7");
        assert_eq!(p.value, 13.0);
        for n in 0..bytes.len() {
            assert!(decode(&bytes[..n], 9).is_err(), "prefix {n}");
        }
        assert!(decode(&parameter(f64::NAN), 9).is_err());
        assert!(decode(&parameter(f64::INFINITY), 9).is_err());
        let mut extra = bytes;
        extra.push(0);
        assert!(decode(&extra, 9).is_err());

        // Inspection distinguishes driven dimensions. Reconstruction rejects
        // them until their measurement dependencies are implemented.
        let mut driven = parameter(13.0);
        let metadata = 4 + 2 + 8 + 4 + 16 + 4 + 11 + 4 + "130 mm".len() * 2;
        driven[metadata + 6] = 1;
        assert!(decode(&driven, 9).unwrap().driven);
    }

    fn authored(version: u32, user: bool, driven: bool, favorite: bool) -> Vec<u8> {
        let mut b = 2_u32.to_le_bytes().to_vec();
        b.extend(b"42");
        b.extend(123_u64.to_le_bytes());
        b.extend([0; 4]);
        b.extend(vec![
            0;
            match version {
                5 => 3,
                6 => 7,
                7 => 8,
                _ => 12,
            }
        ]);
        wide(&mut b, "Description — Δ\nSecond line");
        b.extend(17_u32.to_le_bytes());
        if user {
            b.push(0);
        } else {
            super::super::relationship_tests::reference(&mut b, 124);
        }
        wide(&mut b, "60 mm");
        b.extend(vec![
            0;
            match version {
                7 => 4,
                9 => 5,
                _ => 0,
            }
        ]);
        b.extend([0, u8::from(driven), u8::from(favorite), 0, u8::from(user)]);
        wide(&mut b, if user { "User Parameter" } else { "Diameter" });
        if version >= 7 {
            wide(&mut b, "");
        }
        wide(&mut b, "mm");
        wide(&mut b, if user { "base" } else { "d7" });
        b.extend(6_f64.to_le_bytes());
        b.extend([0; 12]);
        b
    }

    #[test]
    fn legacy_versions_preserve_comments_roles_and_expression_flags() {
        for version in [5, 6, 7, 9] {
            for (user, driven, favorite) in [(false, false, false), (false, true, false), (false, false, true), (true, false, true)] {
                let bytes = authored(version, user, driven, favorite);
                let p = decode(&bytes, version).unwrap();
                assert_eq!(p, decode(&authored(9, user, driven, favorite), 9).unwrap());
                assert_eq!(p.binding.is_none(), user);
                assert_eq!(p.driven, driven);
                assert_eq!(p.favorite, favorite);
                assert_eq!(p.comment, "Description — Δ\nSecond line");
                for keep in 0..bytes.len() {
                    assert!(decode(&bytes[..keep], version).is_err(), "version {version}, prefix {keep}");
                }
            }
            assert!(decode(&authored(version, true, true, false), version).is_err());
        }
        assert!(decode(&authored(9, false, false, false), 8).is_err());
        let mut bytes = authored(9, true, false, false);
        let marker = 14 + 4 + 12 + 4 + "Description — Δ\nSecond line".encode_utf16().count() * 2 + 4;
        bytes[marker] = 2;
        assert!(decode(&bytes, 9).is_err());
    }

    #[test]
    fn only_observed_layout_and_parameter_version_pairs_are_accepted() {
        use super::super::import_tests::Fixture;
        for (layout, version, accepted) in [(4, 5, true), (5, 6, true), (5, 7, true), (8, 9, true), (4, 9, false), (8, 5, false), (6, 7, false)] {
            let mut f = Fixture(BTreeMap::new());
            f.put(123, "7A6A3D31-BE74-4E19-9115-20642944C2E4", version, authored(version, true, false, false)[14..].to_vec(), None);
            let mut d = f.design();
            d.layout = layout;
            assert_eq!(d.parameters().is_ok(), accepted, "layout {layout}, version {version}");
        }
    }

    #[test]
    fn integer_count_bindings_validate_units_values_and_ownership() {
        use super::super::import_tests::Fixture;
        use super::super::relationship_tests::{reference, refs, u32};
        let make = |value: u32, unit: &str, owner: u64, keep: Option<usize>| {
            let mut f = Fixture(BTreeMap::new());
            let mut b = vec![0; 20];
            u32(&mut b, 1);
            reference(&mut b, 124);
            wide(&mut b, "3");
            b.extend([0; 10]);
            for s in ["CountU", "", unit, "count"] {
                wide(&mut b, s);
            }
            b.extend(3_f64.to_le_bytes());
            b.extend([0; 12]);
            f.put(123, "7A6A3D31-BE74-4E19-9115-20642944C2E4", 9, b, None);
            let mut b = vec![0; 4];
            b.push(1);
            refs(&mut b, &[owner]);
            b.extend([0; 5]);
            b.push(1);
            u32(&mut b, value);
            reference(&mut b, 123);
            if let Some(keep) = keep {
                b.truncate(keep);
            }
            f.put(124, "A085449A-5144-4B2B-B455-7F7035A40559", 1, b, None);
            f.put(130, "original-count-owner", 0, vec![], None);
            f.design().parameters()
        };
        let params = make(3, "", 130, None).unwrap();
        assert_eq!(params[0].value, 3.);
        assert_eq!(params[0].owner, Some(130));
        assert!(make(4, "", 130, None).is_err());
        assert!(make(3, "mm", 130, None).is_err());
        assert!(make(3, "", 999, None).is_err());
        for keep in 0..41 {
            assert!(make(3, "", 130, Some(keep)).is_err(), "prefix {keep}");
        }
    }

    #[test]
    fn user_parameter_dependencies_and_comments_survive_import_edits_and_reopen() {
        use super::super::import_tests::{fixture, volume};
        let mut f = fixture(false);
        f.put(123, "7A6A3D31-BE74-4E19-9115-20642944C2E4", 9, authored(9, true, false, true)[14..].to_vec(), None);
        let b = &mut f.0.get_mut(&62).unwrap().2;
        let mut r = Reader { bytes: b, at: 35 };
        r.wide_string().unwrap();
        let end = r.at;
        let mut expression = Vec::new();
        wide(&mut expression, "base");
        b.splice(35..end, expression);
        let mut comment = Vec::new();
        wide(&mut comment, "Extrusion height note");
        f.0.get_mut(&63).unwrap().2.splice(16..20, comment);
        let mut doc = f.design().to_document("User parameters").unwrap();
        let p = doc.params.iter().find(|p| p.name == "base").unwrap();
        assert!(!p.model);
        assert_eq!(p.comment, "Description — Δ\nSecond line");
        assert!(doc.favorites.contains("base"));
        assert_eq!(doc.param_comments["height"], "Extrusion height note");
        volume(&doc, std::f64::consts::PI * 30. * 30. * 17.);
        doc.change_param("base", "40 mm", None, None).unwrap();
        volume(&doc, std::f64::consts::PI * 20. * 20. * 17.);
        let reopened = crate::read_design(&crate::write_design(&doc)).unwrap();
        assert_eq!(doc, reopened);
        volume(&reopened, std::f64::consts::PI * 20. * 20. * 17.);
    }
}
