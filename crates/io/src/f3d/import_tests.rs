use super::relationship_tests::{reference, refs, string, u32, u64, wide};
use super::*;
use solvecraft_doc::{Document, FeatureKind, Model};

// Original, field-authored records extending the relationship fixture. Values,
// identities, strings and archive framing are authored here, not copied exports.
type TestRecord = (String, u32, Vec<u8>, Option<Vec<u8>>);
struct Fixture(BTreeMap<u64, TestRecord>);
impl Fixture {
    fn put(&mut self, id: u64, guid: &str, version: u32, b: Vec<u8>, secondary: Option<Vec<u8>>) {
        self.0.insert(id, (guid.into(), version, b, secondary));
    }
    fn design(self) -> Design {
        let mut d =
            Design { path: "original".into(), build: 1, layout: 8, classes: vec![], roots: vec![3], records: vec![], metadata: vec![], bulk: vec![] };
        for (id, (guid, version, b, secondary)) in self.0 {
            let mut write = |b: Vec<u8>| {
                let at = d.bulk.len();
                string(&mut d.bulk, "5");
                u64(&mut d.bulk, id);
                d.bulk.extend(b);
                at
            };
            let offset = write(b);
            let secondary_offset = secondary.map(write);
            d.records.push(Record { id, offset, secondary_offset });
            if let Some(c) = d.classes.iter_mut().find(|c| c.id == guid) {
                c.objects.push(id);
            } else {
                d.classes.push(Class { id: guid, version, parent: String::new(), group: String::new(), objects: vec![id] });
            }
        }
        d
    }
}
fn common(b: &mut Vec<u8>, name: &str, suppressed: bool) {
    wide(b, "");
    b.extend([0; 3]);
    refs(b, &[]);
    u32(b, 0);
    wide(b, name);
    u32(b, 1);
    u32(b, 0);
    wide(b, "");
    let mut tail = [0; 66];
    tail[5] = u8::from(suppressed);
    tail[51] = tail[5];
    b.extend(tail);
}
fn fixture(suppressed: bool) -> Fixture {
    let d = super::relationship_tests::fixture(false, 17);
    let r = Records::new(&d);
    let payload = |b: &[u8]| {
        let mut r = Reader { bytes: b, at: 0 };
        r.string().unwrap();
        r.u64().unwrap();
        b[r.at..].to_vec()
    };
    let mut f = Fixture(
        d.classes
            .iter()
            .flat_map(|c| {
                c.objects.iter().map(|id| (*id, (c.id.clone(), c.version, payload(r.primary(*id).unwrap()), r.secondary(*id).ok().map(payload))))
            })
            .collect(),
    );
    f.put(3, "E03784ED-5E19-4E14-B9F2-3B07017018CD", 23, vec![], None);
    f.put(4, "C3423E04-7E85-4B44-8E31-A5FA9C100052", 2, vec![], None);
    let mut b = vec![0; 6];
    reference(&mut b, 4);
    refs(&mut b, &[8, 9]);
    f.put(5, "2F4C1849-1A5A-4F6C-A086-8DD445CBF94B", 3, b, None);
    let mut b = vec![0; 7];
    refs(&mut b, &[61]);
    u32(&mut b, 1);
    b.push(0);
    reference(&mut b, 70);
    reference(&mut b, 6);
    b.extend([0; 12]);
    common(&mut b, "Sketch", false);
    f.put(8, "8DA771B7-52ED-42FC-94E2-2353FE141373", 5, b, None);
    let mut b = vec![0; 6];
    b.extend([1, 0]);
    reference(&mut b, 3);
    f.put(6, "F47A46FB-DA27-4AFA-8A72-1B38FA596E23", 0, b, None);
    let mut b = vec![0; 4];
    reference(&mut b, 6);
    b.extend([0; 17]);
    wide(&mut b, "Editable circle");
    b.push(0);
    refs(&mut b, &[42, 41, 31, 56]);
    reference(&mut b, 3);
    f.0.get_mut(&70).unwrap().3 = Some(b);
    // The fully-constrained bit must not make a driving diameter into a Fix.
    let circle = &mut f.0.get_mut(&31).unwrap().2;
    let mut r = Reader { bytes: circle, at: 6 };
    for _ in 0..r.u32().unwrap() {
        r.string().unwrap();
        r.string().unwrap();
        r.u64().unwrap();
    }
    let at = r.at;
    circle[at..at + 24].fill(0);
    circle[at + 96 + 11 + 5] = 1;
    for (id, connection, origin) in [(41, 43, false), (42, 44, true)] {
        let mut b = vec![0; 4];
        b.extend([0, 1]);
        u32(&mut b, 0);
        reference(&mut b, connection);
        b.extend([0; 3]);
        b.extend([u8::from(origin), 1, u8::from(origin), 0, 0]);
        b.extend([0; 24]);
        b.extend([0; 30]);
        refs(&mut b, &[connection]);
        reference(&mut b, 70);
        f.put(id, "C2CEDAE7-1716-47C1-B7B1-07B70081D0FB", 11, b, None);
        let mut b = vec![0; 6];
        refs(&mut b, if origin { &[] } else { &[31] });
        reference(&mut b, id);
        f.put(connection, "362B7EC3-0F09-47C8-A3BE-DC066715CDAE", 0, b, None);
    }
    for (id, binding, name, unit, label, expr, value) in
        [(63, 64, "height", "mm", "AlongDistance", "17 mm", 1.7_f64), (65, 66, "draft", "deg", "TaperAngle", "0 deg", 0.)]
    {
        let mut b = vec![0; 20];
        u32(&mut b, 0);
        reference(&mut b, binding);
        wide(&mut b, expr);
        b.extend([0; 10]);
        for s in [label, "", unit, name] {
            wide(&mut b, s);
        }
        b.extend(value.to_le_bytes());
        b.extend([0; 12]);
        f.put(id, "7A6A3D31-BE74-4E19-9115-20642944C2E4", 9, b, None);
        let mut b = vec![0; 4];
        b.push(1);
        refs(&mut b, &[9]);
        b.extend([0; 5]);
        b.extend(value.to_le_bytes());
        reference(&mut b, id);
        f.put(binding, "D91D429C-1BB5-4640-B49F-69AF14F463E9", 0, b, None);
    }
    f.put(67, "0897AF07-EF1B-42E5-BDD9-1C746A085CBC", 0, vec![], None);
    let mut b = vec![0; 4];
    b.extend([0, 1]);
    b.extend([0; 7]);
    u32(&mut b, 4);
    u32(&mut b, 1);
    u32(&mut b, 2);
    b.push(0);
    u32(&mut b, 1);
    b.push(0);
    for n in [0_f64, 0., 1.] {
        b.extend(n.to_le_bytes());
    }
    b.extend([0; 2]);
    reference(&mut b, 64);
    reference(&mut b, 67);
    b.push(0);
    reference(&mut b, 66);
    b.push(0);
    u32(&mut b, 1);
    b.extend([0; 36]);
    refs(&mut b, &[102]);
    for _ in 0..4 {
        refs(&mut b, &[]);
    }
    common(&mut b, "Extrude", suppressed);
    f.put(9, "DD405BC2-D673-44F0-8833-5CB2A1C186C7", 8, b, None);
    f
}
fn volume(doc: &Document, want: f64) {
    let mut model = Model::new();
    model.evaluate(doc);
    for result in &model.results {
        assert!(result.error.is_none(), "{result:?}");
    }
    let got = model.state().bodies.iter().map(|b| solvecraft_kernel::measure(&b.body).unwrap().volume).sum::<f64>();
    assert!((got - want).abs() < want.max(1.) * 1e-3, "volume {got} != {want}");
}
#[test]
fn original_native_history_regenerates_and_driving_parameters_stay_editable() {
    let d = fixture(false).design();
    let mut doc = d.to_document("Original cylinder").unwrap();
    assert_eq!(doc.features.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["Editable circle", "Extrude1"]);
    let FeatureKind::Sketch { sketch, .. } = &doc.features[0].kind else { panic!("sketch") };
    assert_eq!(sketch.points.len(), 2);
    assert!(!sketch.curves[0].fixed);
    volume(&doc, std::f64::consts::PI * 30. * 30. * 17.);
    doc.params.iter_mut().find(|p| p.name == "d7").unwrap().expr = "40 mm".into();
    doc.params.iter_mut().find(|p| p.name == "height").unwrap().expr = "d7 / 2".into();
    volume(&doc, std::f64::consts::PI * 20. * 20. * 20.);
    let reopened = crate::read_design(&crate::write_design(&doc)).unwrap();
    assert_eq!(doc, reopened);
    volume(&reopened, std::f64::consts::PI * 20. * 20. * 20.);
    volume(&fixture(true).design().to_document("Suppressed").unwrap(), 0.);
}
#[test]
fn fixation_is_distinct_from_constraint_state_and_truncated_points_fail() {
    for fixed in [false, true] {
        let mut f = fixture(false);
        f.0.get_mut(&41).unwrap().2[24] = u8::from(fixed);
        let d = f.design();
        let points = d.sketch_points().unwrap();
        assert_eq!(points.iter().find(|p| p.id == 41).unwrap().fixed, fixed);
        assert!(points.iter().find(|p| p.id == 42).unwrap().origin);
    }
    let len = fixture(false).0[&41].2.len();
    for n in 0..len {
        let mut f = fixture(false);
        f.0.get_mut(&41).unwrap().2.truncate(n);
        assert!(f.design().to_document("Truncated").is_err(), "prefix {n}");
    }
    let mut f = fixture(false);
    f.0.get_mut(&9).unwrap().1 = 999;
    assert!(f.design().to_document("Unknown").is_err());
    let mut f = fixture(false);
    f.0.get_mut(&41).unwrap().2[24] = 2;
    assert!(f.design().sketch_points().is_err());
}
