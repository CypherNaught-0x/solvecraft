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
pub(super) fn ring() -> Fixture {
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
    assert!(!p[0].regions[0][0].outer);
    assert!(p[0].regions[0][0].reversed[0]);
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
fn rotated_arc_roundoff_preserves_independent_endpoints_but_rejects_real_disagreement() {
    let mut f = arc(PI, PI / 3.);
    round(&mut f, 31, 17, 3., PI - 2.1e-8, PI / 3.);
    let mut prefix = Vec::new();
    attributes(&mut prefix, 17);
    let normal_z = prefix.len() + 5 * 8;
    f.0.get_mut(&31).unwrap().2[normal_z..normal_z + 8].copy_from_slice(&(1. + f64::EPSILON).to_le_bytes());
    volume(&f.design().to_document("Rounded semicircle").unwrap(), 450. * PI * 17.);
    let mut f = arc(PI, PI / 3.);
    round(&mut f, 31, 17, 3., PI - 1e-3, PI / 3.);
    assert!(f.design().to_document("Disagreeing endpoints").unwrap_err().to_string().contains("geometry disagree"));
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
fn diameter_graph_slots_do_not_change_dimension_semantics() {
    for slot in [0_u32, 4, 5, 6, u32::MAX] {
        let mut f = fixture(false);
        for at in [20, 35] {
            f.0.get_mut(&56).unwrap().2[at..at + 4].copy_from_slice(&slot.to_le_bytes());
        }
        let mut doc = f.design().to_document("Diameter connection slots").unwrap();
        doc.change_param("d7", "40 mm", None, None).unwrap();
        volume(&doc, 400. * PI * 17.);
    }
}

#[test]
fn fixed_arc_support_preserves_free_endpoint_motion() {
    use solvecraft_doc::sketch::{ConstraintKind, CurveKind};
    let mut f = arc((1_f64 / 3.).acos(), 0.);
    let b = &mut f.0.get_mut(&31).unwrap().2;
    let n = b.len();
    b[n - 51] = 1; // Original field-authored support fixation.
    f.0.get_mut(&71).unwrap().2[24] = 1; // Start point independently fixed.
    let mut doc = f.design().to_document("Fixed arc support").unwrap();
    let FeatureKind::Sketch { sketch, .. } = &mut doc.features[0].kind else { panic!("sketch") };
    let ci = sketch.curve_index("c31").unwrap();
    let CurveKind::Arc { c, a, b } = sketch.curves[ci].kind else { panic!("arc") };
    assert!(!sketch.curves[ci].fixed);
    assert!(sketch.points[a].fixed);
    assert!(!sketch.points[b].fixed);
    let dimension = sketch.add_constraint(ConstraintKind::DistanceX { p: c, q: b, value: 10. }, None).unwrap();
    for x in [10_f64, 20., 5.] {
        let FeatureKind::Sketch { sketch, .. } = &mut doc.features[0].kind else { panic!("sketch") };
        sketch.constraints.iter_mut().find(|c| c.id == dimension).unwrap().kind = ConstraintKind::DistanceX { p: c, q: b, value: x };
        let angle = (x / 30.).acos();
        volume(&doc, 450. * (angle - angle.sin()) * 17.);
        let mut model = solvecraft_doc::Model::new();
        model.evaluate(&doc);
        let state = model.state();
        let solved = &state.sketches[0].sketch;
        for (index, expected) in [(c, (0., 0.)), (a, (30., 0.)), (b, (x, (900. - x * x).sqrt()))] {
            let p = solved.points[index].pos;
            assert!((p.x - expected.0).abs() < 1e-5);
            assert!((p.y - expected.1).abs() < 1e-5);
        }
        volume(&crate::read_design(&crate::write_design(&doc)).unwrap(), 450. * (angle - angle.sin()) * 17.);
    }
}

fn multiple_regions(duplicate: bool) -> Fixture {
    let mut f = ring();
    let mut b = vec![];
    attributes(&mut b, 18);
    for x in [8_f64, 0., 0., 0., 0., 1., 1., 0., 0., 1., 0., TAU] {
        b.extend(x.to_le_bytes());
    }
    reference(&mut b, 75);
    b.extend([0; 44]);
    reference(&mut b, 70);
    f.put(35, "F0130424-8B7E-4092-93C9-1CA807482534", 0, b, None);
    let mut b = vec![0; 6];
    reference(&mut b, 76);
    b.extend([0, 0, 0, 1, 1, 0, 0, 0]);
    for x in [8_f64, 0., 0.] {
        b.extend(x.to_le_bytes());
    }
    b.extend([0; 30]);
    refs(&mut b, &[76]);
    reference(&mut b, 70);
    f.put(75, "C2CEDAE7-1716-47C1-B7B1-07B70081D0FB", 11, b, None);
    f.put(76, "362B7EC3-0F09-47C8-A3BE-DC066715CDAE", 0, vec![], None);
    geometry(&mut f, &[42, 41, 31, 35, 75, 56], &[56]);
    let mut b = vec![0; 6];
    reference(&mut b, 100);
    u32(&mut b, 2);
    for tag in [17, if duplicate { 17 } else { 18 }] {
        u32(&mut b, 1);
        u32(&mut b, 1);
        u32(&mut b, 3);
        u64(&mut b, tag);
        u64(&mut b, 2);
        u32(&mut b, 0);
        u32(&mut b, 1);
        u32(&mut b, 1);
        u64(&mut b, 0);
        b.push(1);
        u32(&mut b, 0);
    }
    f.0.get_mut(&101).unwrap().2 = b;
    f
}

#[test]
fn multiple_profile_regions_keep_separate_bodies_and_editable_dimensions() {
    let mut doc = multiple_regions(false).design().to_document("Two original disks").unwrap();
    assert_eq!(doc.features[1].body_names, ["Body1", "Body2"]);
    volume(&doc, 1000. * PI * 17.);
    doc.change_param("d7", "80 mm", None, None).unwrap();
    volume(&doc, 1700. * PI * 17.);
    let mut model = solvecraft_doc::Model::new();
    model.evaluate(&doc);
    assert_eq!(model.state().bodies.len(), 2);
    volume(&crate::read_design(&crate::write_design(&doc)).unwrap(), 1700. * PI * 17.);
}

#[test]
fn multiple_profile_regions_reject_duplicates_and_truncation() {
    assert!(multiple_regions(true).design().sketch_profiles().is_err());
    let length = multiple_regions(false).0[&101].2.len();
    for keep in 0..length {
        let mut f = multiple_regions(false);
        f.0.get_mut(&101).unwrap().2.truncate(keep);
        assert!(f.design().sketch_profiles().is_err(), "prefix {keep}");
    }
}

fn partial_circles(first: (u32, bool), second: (u32, bool)) -> Fixture {
    let mut f = multiple_regions(false);
    let mut b = vec![];
    attributes(&mut b, 18);
    for x in [4_f64, 0., 0., 0., 0., 1., 1., 0., 0., 3., 0., TAU] {
        b.extend(x.to_le_bytes());
    }
    reference(&mut b, 75);
    b.extend([0; 44]);
    reference(&mut b, 70);
    f.put(35, "F0130424-8B7E-4092-93C9-1CA807482534", 0, b, None);
    f.0.get_mut(&75).unwrap().2[25..33].copy_from_slice(&4_f64.to_le_bytes());
    let mut b = vec![0; 6];
    reference(&mut b, 100);
    for n in [1, 1, 2] {
        u32(&mut b, n);
    }
    for (tag, (piece, reversed)) in [(17, first), (18, second)] {
        u32(&mut b, 3);
        u64(&mut b, tag);
        u64(&mut b, 2);
        u32(&mut b, u32::from(reversed));
        u32(&mut b, piece);
        u32(&mut b, 2);
        u64(&mut b, 0);
    }
    b.push(1);
    u32(&mut b, 0);
    f.0.get_mut(&101).unwrap().2 = b;
    f
}

fn overlap(r: f64, s: f64, d: f64) -> f64 {
    r * r * ((d * d + r * r - s * s) / (2. * d * r)).acos() + s * s * ((d * d + s * s - r * r) / (2. * d * s)).acos()
        - ((-d + r + s) * (d + r - s) * (d - r + s) * (d + r + s)).sqrt() / 2.
}

#[test]
fn partial_circles_distinguish_lens_and_crescents_through_edits() {
    for (first, second, side) in [((1, false), (2, false), 0), ((2, false), (2, true), 1), ((1, true), (1, false), 2)] {
        let mut doc = partial_circles(first, second).design().to_document("Original split circles").unwrap();
        let area = |r: f64| {
            let lens = overlap(r, 30., 40.);
            match side {
                0 => lens,
                1 => PI * r * r - lens,
                _ => PI * 900. - lens,
            }
        };
        volume(&doc, area(30.) * 17.);
        doc.change_param("d7", "70 mm", None, None).unwrap();
        volume(&doc, area(35.) * 17.);
        volume(&crate::read_design(&crate::write_design(&doc)).unwrap(), area(35.) * 17.);
        doc.change_param("d7", "10 mm", None, None).unwrap();
        let mut model = solvecraft_doc::Model::new();
        model.evaluate(&doc);
        assert!(model.results.last().unwrap().error.is_some());
        assert!(model.state().bodies.is_empty());
    }
}

#[test]
fn profile_piece_records_reject_invalid_counts_and_truncation() {
    for (piece, count) in [(0, 2), (3, 2), (1, 0), (1, 50_001)] {
        let mut f = partial_circles((1, false), (2, false));
        f.0.get_mut(&101).unwrap().2[53..57].copy_from_slice(&u32::to_le_bytes(piece));
        f.0.get_mut(&101).unwrap().2[57..61].copy_from_slice(&u32::to_le_bytes(count));
        assert!(f.design().sketch_profiles().is_err());
    }
    let length = partial_circles((1, false), (2, false)).0[&101].2.len();
    for keep in 0..length {
        let mut f = partial_circles((1, false), (2, false));
        f.0.get_mut(&101).unwrap().2.truncate(keep);
        assert!(f.design().sketch_profiles().is_err(), "prefix {keep}");
    }
    assert!(partial_circles((1, false), (2, false)).design().circle_profiles().is_err());
}

#[test]
fn rotated_circle_seams_survive_import_edit_and_reopening() {
    for phase in [PI / 3., PI / 2., PI] {
        // Rotating only the left source axis moves its lens span to piece two.
        let mut f = partial_circles((2, false), (2, false));
        round(&mut f, 31, 17, 3., TAU, phase);
        let mut doc = f.design().to_document("Rotated source circle").unwrap();
        volume(&doc, overlap(30., 30., 40.) * 17.);
        doc.change_param("d7", "70 mm", None, None).unwrap();
        doc.change_param("height", "23 mm", None, None).unwrap();
        volume(&doc, overlap(35., 30., 40.) * 23.);
        let reopened = crate::read_design(&crate::write_design(&doc)).unwrap();
        assert_eq!(doc, reopened);
        volume(&reopened, overlap(35., 30., 40.) * 23.);
    }
}
