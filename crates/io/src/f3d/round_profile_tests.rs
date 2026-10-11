use super::import_tests::{Fixture, common, fixture, volume};
use super::relationship_tests::{reference, refs, string, u32, u64, wide};
use solvecraft_doc::FeatureKind;
use std::f64::consts::{PI, TAU};

// Original field-authored geometry; no native exported byte sequences.
fn attributes(b: &mut Vec<u8>, tag: u64) {
    b.extend([0; 4]);
    b.extend([0, 1]);
    u32(b, 2);
    for (key, value) in [("crv_primary_id", tag), ("crv_secondary_id", 2)] {
        string(b, key);
        string(b, "IntrinsicMetaTypeuint64");
        u64(b, value);
    }
}
fn round(f: &mut Fixture, id: u64, tag: u64, radius: f64, sweep: f64, axis: f64) {
    let mut b = vec![];
    attributes(&mut b, tag);
    for x in [0., 0., 0., 0., 0., 1., axis.cos(), axis.sin(), 0., radius, 0., sweep] {
        b.extend(x.to_le_bytes());
    }
    reference(&mut b, 41);
    if sweep != TAU {
        reference(&mut b, 72);
        reference(&mut b, 71);
    }
    b.extend(vec![0; if sweep == TAU { 44 } else { 42 }]);
    reference(&mut b, 70);
    f.put(id, "F0130424-8B7E-4092-93C9-1CA807482534", 0, b, None);
}
pub(super) fn geometry(f: &mut Fixture, entities: &[u64], dimensions: &[u64]) {
    let mut b = vec![0; 4];
    b.extend([0, 1]);
    u32(&mut b, 0);
    wide(&mut b, "original-round-profile");
    u64(&mut b, 0);
    b.push(1);
    refs(&mut b, &[55]);
    b.push(1);
    refs(&mut b, dimensions);
    b.extend([0; 17]);
    reference(&mut b, 8);
    b.extend([0; 4]);
    let mut secondary = vec![0; 4];
    reference(&mut secondary, 6);
    secondary.extend([0; 17]);
    wide(&mut secondary, "Round profile");
    secondary.push(0);
    refs(&mut secondary, entities);
    reference(&mut secondary, 3);
    f.put(70, "44A64366-4BD3-4B24-881A-F94C206E8F2D", 18, b, Some(secondary));
}
fn tracking(f: &mut Fixture, loops: &[(Vec<(u64, bool)>, bool)]) {
    let mut b = vec![0; 6];
    reference(&mut b, 100);
    u32(&mut b, 1);
    u32(&mut b, loops.len() as u32);
    for (curves, outer) in loops {
        u32(&mut b, curves.len() as u32);
        for (tag, reversed) in curves {
            u32(&mut b, 3);
            u64(&mut b, *tag);
            u64(&mut b, 2);
            u32(&mut b, u32::from(*reversed));
            u32(&mut b, 1);
            u32(&mut b, 1);
            u64(&mut b, 0);
        }
        b.push(u8::from(*outer));
    }
    u32(&mut b, 0);
    f.put(101, "0D57BD2F-D09B-43FC-AD57-1E89A118C453", 1, b, None);
}
fn ring() -> Fixture {
    let mut f = fixture(false);
    round(&mut f, 35, 18, 1., TAU, 0.);
    geometry(&mut f, &[42, 41, 31, 35, 56], &[56]);
    // Inner loop first deliberately proves registration order is not outer identity.
    tracking(&mut f, &[(vec![(18, true)], false), (vec![(17, false)], true)]);
    f
}
pub(super) fn arc(sweep: f64, axis: f64) -> Fixture {
    let mut f = fixture(false);
    for id in [56, 61, 62] {
        f.0.remove(&id);
    }
    let mut b = vec![0; 7];
    refs(&mut b, &[]);
    u32(&mut b, 1);
    b.push(0);
    reference(&mut b, 70);
    reference(&mut b, 6);
    b.extend([0; 12]);
    common(&mut b, "Sketch", false);
    f.put(8, "8DA771B7-52ED-42FC-94E2-2353FE141373", 5, b, None);
    let start = [3. * axis.cos(), 3. * axis.sin(), 0.];
    let end = [3. * (axis + sweep).cos(), 3. * (axis + sweep).sin(), 0.];
    for (id, connection, position) in [(71, 73, start), (72, 74, end)] {
        let mut b = vec![0; 4];
        b.extend([0, 1]);
        u32(&mut b, 0);
        reference(&mut b, connection);
        b.extend([0; 8]);
        for n in position {
            b.extend(n.to_le_bytes());
        }
        b.extend([0; 30]);
        refs(&mut b, &[connection]);
        reference(&mut b, 70);
        f.put(id, "C2CEDAE7-1716-47C1-B7B1-07B70081D0FB", 11, b, None);
        f.put(connection, "362B7EC3-0F09-47C8-A3BE-DC066715CDAE", 0, vec![], None);
    }
    round(&mut f, 31, 17, 3., sweep, axis);
    let delta = [start[0] - end[0], start[1] - end[1], 0.];
    let length = delta.iter().map(|v| v * v).sum::<f64>().sqrt();
    let mut b = vec![];
    attributes(&mut b, 18);
    for n in end.into_iter().chain(delta).chain(delta.map(|n| n / length)).chain([0., 0., 1.]) {
        b.extend(n.to_le_bytes());
    }
    reference(&mut b, 71);
    reference(&mut b, 72);
    b.extend([0; 42]);
    reference(&mut b, 70);
    f.put(35, "DCA267ED-D615-4934-B64F-AD805E8003E2", 2, b, None);
    geometry(&mut f, &[42, 41, 71, 72, 31, 35], &[]);
    tracking(&mut f, &[(vec![(17, false), (18, false)], true)]);
    f
}

#[test]
fn holes_preserve_selected_boundaries_across_edits_and_reopening() {
    let d = ring().design();
    let p = d.sketch_profiles().unwrap();
    assert!(!p[0].loops[0].outer);
    assert!(p[0].loops[0].reversed[0]);
    let mut doc = d.to_document("Original annulus").unwrap();
    volume(&doc, 800. * PI * 17.);
    doc.change_param("d7", "80 mm", None, None).unwrap();
    doc.change_param("height", "23 mm", None, None).unwrap();
    volume(&doc, 1500. * PI * 23.);
    let reopened = crate::read_design(&crate::write_design(&doc)).unwrap();
    assert_eq!(doc, reopened);
    volume(&reopened, 1500. * PI * 23.);
    // Removing an explicitly selected hole must not silently turn it into a disk.
    let FeatureKind::Sketch { sketch, .. } = &mut doc.features[0].kind else { panic!("sketch") };
    sketch.curves.iter_mut().find(|c| c.id == "c35").unwrap().construction = true;
    let mut model = solvecraft_doc::Model::new();
    model.evaluate(&doc);
    assert!(model.results.iter().any(|r| r.error.is_some()));
}

#[test]
fn arcs_preserve_major_and_rotated_sweeps_and_shared_chord_points() {
    for (sweep, axis) in [(PI / 2., 0.), (PI, 0.), (PI * 1.5, 0.), (PI, PI)] {
        let d = arc(sweep, axis).design();
        assert!(d.sketch_circles().unwrap().is_empty());
        assert_eq!(d.sketch_arcs().unwrap().len(), 1);
        let mut doc = d.to_document("Original arc segment").unwrap();
        let area = 900. * (sweep - sweep.sin()) / 2.;
        volume(&doc, area * 17.);
        doc.change_param("height", "23 mm", None, None).unwrap();
        volume(&doc, area * 23.);
        let reopened = crate::read_design(&crate::write_design(&doc)).unwrap();
        assert_eq!(doc, reopened);
        volume(&reopened, area * 23.);
    }
}

#[test]
fn malformed_arc_and_hole_records_are_rejected() {
    for id in [31, 101] {
        let length = arc(PI, 0.).0[&id].2.len();
        for keep in 0..length {
            let mut f = arc(PI, 0.);
            f.0.get_mut(&id).unwrap().2.truncate(keep);
            assert!(f.design().to_document("Truncated").is_err(), "{id}/{keep}");
        }
    }
    for loops in [
        vec![(vec![(17, false)], false)],
        vec![(vec![(17, false)], true), (vec![(18, true)], true)],
        vec![(vec![(17, false)], true), (vec![(17, true)], false)],
    ] {
        let mut f = ring();
        tracking(&mut f, &loops);
        assert!(f.design().sketch_profiles().is_err());
    }
}

#[test]
fn verified_diameter_axes_remain_diameters() {
    for axis in [4_u32, 5, 6] {
        let mut f = fixture(false);
        f.0.get_mut(&56).unwrap().2[20..24].copy_from_slice(&axis.to_le_bytes());
        let result = f.design().to_document("Diameter axis");
        if axis == 6 {
            assert!(result.is_err());
        } else {
            let mut doc = result.unwrap();
            doc.change_param("d7", "40 mm", None, None).unwrap();
            volume(&doc, 400. * PI * 17.);
        }
    }
}
