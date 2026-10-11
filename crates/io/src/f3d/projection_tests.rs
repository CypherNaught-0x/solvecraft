use super::import_tests::{Fixture, common, fixture, volume};
use super::relationship_tests::{reference, refs, string, u32, u64, wide};

pub(super) fn projected_circle() -> Fixture {
    let mut f = fixture(false);
    let mut b = vec![0; 6];
    reference(&mut b, 4);
    refs(&mut b, &[8, 18, 9]);
    f.0.get_mut(&5).unwrap().2 = b;
    let mut b = vec![0; 7];
    refs(&mut b, &[]);
    u32(&mut b, 1);
    b.push(0);
    reference(&mut b, 170);
    reference(&mut b, 6);
    u32(&mut b, 0);
    refs(&mut b, &[180]);
    u32(&mut b, 0);
    common(&mut b, "Projected", false);
    f.put(18, "8DA771B7-52ED-42FC-94E2-2353FE141373", 5, b, None);
    let mut b = vec![0; 4];
    b.extend([0, 0]);
    wide(&mut b, "original-projected-circle");
    u64(&mut b, 0);
    b.push(1);
    refs(&mut b, &[]);
    b.push(1);
    refs(&mut b, &[]);
    b.extend([0; 17]);
    reference(&mut b, 18);
    b.extend([0; 4]);
    let mut secondary = vec![0; 4];
    reference(&mut secondary, 6);
    secondary.extend([0; 17]);
    wide(&mut secondary, "Projected circle");
    secondary.push(0);
    refs(&mut secondary, &[142, 141, 131]);
    reference(&mut secondary, 3);
    f.put(170, "44A64366-4BD3-4B24-881A-F94C206E8F2D", 18, b, Some(secondary));
    let mut b = vec![0; 4];
    b.extend([0, 1]);
    u32(&mut b, 2);
    for (key, value) in [("crv_primary_id", 17), ("crv_secondary_id", 2)] {
        string(&mut b, key);
        string(&mut b, "IntrinsicMetaTypeuint64");
        u64(&mut b, value);
    }
    for n in [0_f64, 0., 0., 0., 0., 1., 1., 0., 0., 3., 0., std::f64::consts::TAU] {
        b.extend(n.to_le_bytes());
    }
    reference(&mut b, 141);
    b.extend([0; 6]);
    b.push(1); // reference
    b.extend([0; 37]);
    reference(&mut b, 170);
    f.put(131, "F0130424-8B7E-4092-93C9-1CA807482534", 0, b, None);
    for (id, connection, origin) in [(141, 143, false), (142, 144, true)] {
        let mut b = vec![0; 6];
        reference(&mut b, connection);
        b.extend([0; 3]);
        b.extend([u8::from(origin), 1, u8::from(origin), u8::from(!origin), 0]);
        b.extend([0; 54]);
        refs(&mut b, &[connection]);
        reference(&mut b, 170);
        f.put(id, "C2CEDAE7-1716-47C1-B7B1-07B70081D0FB", 11, b, None);
        f.put(connection, "362B7EC3-0F09-47C8-A3BE-DC066715CDAE", 0, vec![], None);
    }
    let mut b = vec![0; 6];
    reference(&mut b, 101);
    u32(&mut b, 1);
    wide(&mut b, "original-source");
    wide(&mut b, "170");
    b.extend([0; 20]);
    reference(&mut b, 9);
    f.0.get_mut(&100).unwrap().2 = b;
    let mut b = vec![0; 9];
    b.push(1);
    reference(&mut b, 181);
    b.push(0);
    b.extend(0.001_f64.to_le_bytes());
    u32(&mut b, 3);
    reference(&mut b, 18);
    reference(&mut b, 182);
    b.extend([0; 42]);
    reference(&mut b, 18);
    f.put(180, super::projections::CLASS, 4, b, None);
    let mut b = vec![0; 6];
    u32(&mut b, 1);
    reference(&mut b, 184);
    u64(&mut b, 131);
    u64(&mut b, 1);
    f.put(181, "CA72DD8E-2861-4DDD-9721-03039B5FB113", 0, b, None);
    let mut b = vec![0; 6];
    refs(&mut b, &[183]);
    b.extend([0; 20]);
    reference(&mut b, 18);
    f.put(182, "2CA5A1CD-C99B-4C9B-91AF-57989148E841", 1, b, None);
    let mut b = vec![0; 6];
    reference(&mut b, 185);
    b.extend([0; 20]);
    reference(&mut b, 18);
    f.put(183, "5A1BF548-241F-46FD-9FB5-E4B05126EB9D", 1, b, None);
    let mut b = vec![0; 6];
    reference(&mut b, 185);
    b.push(0);
    f.put(184, "CB4F4CC2-9D6F-48BA-9B1F-20C1992C9C9C", 2, b, None);
    let mut b = vec![0; 14];
    u64(&mut b, 70);
    u64(&mut b, 17);
    f.put(185, "E2CEFD18-D755-4E09-8E7F-953A2F6D43F8", 0, b, None);
    f
}

#[test]
fn projected_circle_keeps_source_dependency_and_curve_ids_through_edits() {
    let mut doc = projected_circle().design().to_document("Original projection").unwrap();
    volume(&doc, 900. * std::f64::consts::PI * 17.);
    let solvecraft_doc::FeatureKind::Sketch { sketch, .. } = &doc.features[1].kind else { panic!("sketch") };
    assert_eq!(sketch.curves[0].id, "c131");
    assert_eq!(sketch.links.len(), 1);
    assert!(matches!(&sketch.links[0].source, solvecraft_doc::sketch::LinkSource::SketchCurve { sketch: 1, curve } if curve == "c31"));
    doc.change_param("d7", "40 mm", None, None).unwrap();
    volume(&doc, 400. * std::f64::consts::PI * 17.);
    let mut reopened = crate::read_design(&crate::write_design(&doc)).unwrap();
    assert_eq!(reopened, doc);
    reopened.change_param("d7", "80 mm", None, None).unwrap();
    volume(&reopened, 1600. * std::f64::consts::PI * 17.);
}

#[test]
fn damaged_projection_relationships_never_fall_back_to_cached_geometry() {
    for (id, at, replacement) in [(180, 35, 8_u64), (181, 21, 31), (185, 14, 170), (185, 22, 999)] {
        let mut f = projected_circle();
        f.0.get_mut(&id).unwrap().2[at..at + 8].copy_from_slice(&replacement.to_le_bytes());
        assert!(f.design().to_document("Damaged projection").is_err(), "record {id}");
    }
    for id in [180, 181, 182, 183, 184, 185] {
        let n = projected_circle().0[&id].2.len();
        for keep in 0..n {
            let mut f = projected_circle();
            f.0.get_mut(&id).unwrap().2.truncate(keep);
            assert!(f.design().to_document("Truncated projection").is_err(), "record {id}, prefix {keep}");
        }
    }
}
