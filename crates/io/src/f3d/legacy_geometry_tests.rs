//! Original field-authored geometry records, never captured Fusion payloads.
use super::import_tests::Fixture;
use super::polygon_tests::rectangle;
use super::relationship_tests::{reference, refs};
use super::*;

fn legacy(mut f: Fixture, geometry: u32) -> Design {
    for (guid, version, bytes, _) in f.0.values_mut() {
        match guid.as_str() {
            entity::LINE | "F0130424-8B7E-4092-93C9-1CA807482534" => {
                // Curve base v8 lacks one byte preceding the construction flag.
                let n = bytes.len();
                bytes.remove(n - 48);
            }
            "44A64366-4BD3-4B24-881A-F94C206E8F2D" => *version = geometry,
            _ => continue,
        }
    }
    if geometry == 17 {
        for (guid, _, bytes, _) in f.0.values_mut() {
            if matches!(guid.as_str(), entity::POINT | entity::LINE | "F0130424-8B7E-4092-93C9-1CA807482534") {
                let n = bytes.len();
                bytes.splice(n - 11..n - 11, [0; 4]);
            }
        }
    }
    f.put(999, "9FF71F46-205F-484B-A2B5-A245D0D8FE33", 8, vec![], None);
    let mut d = f.design();
    d.layout = if geometry == 16 { 4 } else { 5 };
    d
}

#[test]
fn legacy_geometry_versions_preserve_construction_and_coordinates() {
    for polygon in [true, false] {
        let fixture = || if polygon { rectangle() } else { super::import_tests::fixture(false) };
        for geometry in [16, 17] {
            let mut original = fixture();
            let curve = &mut original.0.get_mut(&31).unwrap().2;
            let at = curve.len() - 46;
            curve[at] = 1;
            let modern = original.design();
            let mut old = fixture();
            let curve = &mut old.0.get_mut(&31).unwrap().2;
            let at = curve.len() - 46;
            curve[at] = 1;
            let old = legacy(old, geometry);
            assert_eq!(old.sketch_points().unwrap(), modern.sketch_points().unwrap());
            assert_eq!(old.sketch_lines().unwrap(), modern.sketch_lines().unwrap());
            assert_eq!(old.sketch_circles().unwrap(), modern.sketch_circles().unwrap());
        }
    }
}

fn specialized() -> Fixture {
    let mut f = rectangle();
    for id in [41, 31] {
        let record = f.0.get_mut(&id).unwrap();
        let mut r = Reader { bytes: &record.2, at: 4 };
        r.take(1).unwrap();
        attributes::read(&mut r).unwrap();
        let at = r.at;
        let mut primary = record.2[..at].to_vec();
        let mut secondary = vec![0; 4];
        secondary.extend(&record.2[at..]);
        if id == 31 {
            r.take(96).unwrap();
            let end = r.reference().unwrap();
            let start = r.reference().unwrap();
            reference(&mut primary, end);
            reference(&mut primary, 70); // opaque definition retained explicitly
            reference(&mut primary, start);
            record.0 = entity::SPECIAL_LINE.into();
        } else {
            record.0 = entity::SPECIAL_POINT.into();
        }
        record.1 = 0;
        record.2 = primary;
        record.3 = Some(secondary);
    }
    f
}

fn design(f: Fixture) -> Design {
    let mut d = f.design();
    for class in &mut d.classes {
        match class.id.as_str() {
            entity::SPECIAL_LINE => class.parent = entity::LINE.into(),
            entity::SPECIAL_POINT => class.parent = entity::POINT.into(),
            _ => {}
        }
    }
    d
}

#[test]
fn split_geometry_retains_definitions_and_rejects_truncated_or_inconsistent_records() {
    let ordinary = rectangle().design();
    let d = design(specialized());
    let points = d.sketch_points().unwrap();
    let p = points.iter().find(|p| p.id == 41).unwrap();
    assert!(p.metadata.specialized);
    assert_eq!(p.position, ordinary.sketch_points().unwrap().iter().find(|p| p.id == 41).unwrap().position);
    let lines = d.sketch_lines().unwrap();
    let l = lines.iter().find(|l| l.id == 31).unwrap();
    assert!(l.metadata.specialized);
    assert_eq!(l.metadata.definition, [70]);
    let base = ordinary.sketch_lines().unwrap();
    assert_eq!((l.start, l.end), (base[0].start, base[0].end));
    assert!(d.to_document("Keep dependencies").unwrap_err().to_string().contains("entity relationship"));
    for id in [31, 41] {
        for secondary in [false, true] {
            let f = specialized();
            let record = &f.0[&id];
            let length = if secondary { record.3.as_ref().unwrap().len() } else { record.2.len() };
            for n in 0..length {
                let mut f = specialized();
                let record = f.0.get_mut(&id).unwrap();
                if secondary {
                    record.3.as_mut().unwrap().truncate(n);
                } else {
                    record.2.truncate(n);
                }
                let d = design(f);
                assert!(if id == 31 { d.sketch_lines().is_err() } else { d.sketch_points().is_err() }, "{id}/{secondary}/{n}");
            }
        }
    }
    let mut f = specialized();
    let primary = &mut f.0.get_mut(&31).unwrap().2;
    let at = primary.len() - 10;
    primary[at..at + 8].copy_from_slice(&72_u64.to_le_bytes());
    assert!(design(f).sketch_lines().unwrap_err().to_string().contains("endpoint references disagree"));
}

#[test]
fn nonempty_curve_relationships_remain_visible_and_cannot_be_flattened() {
    let mut f = rectangle();
    let bytes = &mut f.0.get_mut(&31).unwrap().2;
    let mut related = Vec::new();
    refs(&mut related, &[70]);
    let at = bytes.len() - 41;
    bytes.splice(at..at + 4, related);
    let d = f.design();
    assert_eq!(d.sketch_lines().unwrap()[0].metadata.relationships, [70]);
    assert!(d.to_document("Preserve dependencies").unwrap_err().to_string().contains("entity relationship"));
}

#[test]
fn unknown_point_flags_are_retained_without_inventing_constraints() {
    let mut f = rectangle();
    let point = &mut f.0.get_mut(&41).unwrap().2;
    let at = point.len() - 81;
    point[at] = 1;
    point[at - 6] = 1;
    let d = f.design();
    assert_eq!(d.sketch_points().unwrap().iter().find(|p| p.id == 41).unwrap().metadata.point_prefix, [0, 1, 0]);
    assert!(d.sketch_points().unwrap().iter().find(|p| p.id == 41).unwrap().metadata.point_auxiliary);
    assert!(d.to_document("Keep point semantics").unwrap_err().to_string().contains("entity relationship"));
}

#[test]
fn reversed_support_line_axis_preserves_endpoint_order_and_editable_profile() {
    let mut f = rectangle();
    let bytes = &mut f.0.get_mut(&31).unwrap().2;
    let mut r = Reader { bytes, at: 5 };
    attributes::read(&mut r).unwrap();
    let at = r.at + 48;
    for i in 0..3 {
        let range = at + i * 8..at + (i + 1) * 8;
        let value = f64::from_le_bytes(bytes[range.clone()].try_into().unwrap());
        bytes[range].copy_from_slice(&(-value).to_le_bytes());
    }
    let mut doc = f.design().to_document("Reversed support axis").unwrap();
    super::import_tests::volume(&doc, 30. * 20. * 17.);
    doc.change_param("width", "45 mm", None, None).unwrap();
    super::import_tests::volume(&doc, 45. * 20. * 17.);
    let mut f = rectangle();
    let bytes = &mut f.0.get_mut(&31).unwrap().2;
    for (i, value) in [0_f64, 0., 1.].iter().enumerate() {
        bytes[at + i * 8..at + (i + 1) * 8].copy_from_slice(&value.to_le_bytes());
    }
    assert!(f.design().sketch_lines().is_err());
}
