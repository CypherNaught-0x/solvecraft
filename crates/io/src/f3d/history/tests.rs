use super::*;

// Original field-by-field records with deliberately non-monotonic identities.
// No bytes from an Autodesk-produced archive are embedded here.
fn u32(b: &mut Vec<u8>, n: u32) {
    b.extend(n.to_le_bytes());
}
fn wide(b: &mut Vec<u8>, s: &str) {
    u32(b, s.encode_utf16().count() as u32);
    for c in s.encode_utf16() {
        b.extend(c.to_le_bytes());
    }
}
fn reference(b: &mut Vec<u8>, id: u64) {
    b.push(1);
    b.extend(id.to_le_bytes());
    b.extend([0, 0]);
}
fn refs(b: &mut Vec<u8>, ids: &[u64]) {
    u32(b, ids.len() as u32);
    for id in ids {
        reference(b, *id);
    }
}
fn header(id: u64) -> Vec<u8> {
    let mut b = 1u32.to_le_bytes().to_vec();
    b.push(b'9');
    b.extend(id.to_le_bytes());
    b
}
fn common(b: &mut Vec<u8>, base: &str, number: u32, name: &str, suppressed: bool) {
    wide(b, "");
    b.extend([0; 3]);
    refs(b, &[76]);
    u32(b, u32::MAX);
    wide(b, base);
    u32(b, number);
    u32(b, 0);
    wide(b, name);
    let mut tail = [0; 66];
    tail[5] = suppressed as u8;
    tail[51] = suppressed as u8;
    b.extend(tail);
}
fn fixture(order: &[u64], op: u32, suppressed: bool, matrix: Option<[f64; 16]>) -> Design {
    let mut design =
        Design { path: "synthetic".into(), build: 1, layout: 8, classes: Vec::new(), roots: vec![], records: vec![], metadata: vec![], bulk: vec![] };
    let mut add = |id, guid: &str, version, primary: Vec<u8>, secondary: Option<Vec<u8>>| {
        let offset = design.bulk.len();
        design.bulk.extend(primary);
        let secondary_offset = secondary.map(|bytes| {
            let at = design.bulk.len();
            design.bulk.extend(bytes);
            at
        });
        design.records.push(Record { id, offset, secondary_offset });
        design.classes.push(Class { id: guid.into(), parent: String::new(), version, group: String::new(), objects: vec![id] });
    };
    let mut b = header(7);
    u32(&mut b, 0);
    b.extend([0; 2]);
    reference(&mut b, 4);
    refs(&mut b, order);
    add(7, TIMELINE, 3, b, None);
    let mut b = header(90);
    u32(&mut b, 0);
    b.extend([0; 3]);
    refs(&mut b, &[]);
    u32(&mut b, 1);
    b.push(0);
    reference(&mut b, 60);
    reference(&mut b, 76);
    b.extend([0; 12]);
    common(&mut b, "Sketch", 12, "", false);
    add(90, SKETCH, 5, b, None);
    let mut b = header(60);
    u32(&mut b, 0);
    reference(&mut b, 20);
    b.extend([0; 17]);
    wide(&mut b, "Round Δ");
    b.push(0);
    refs(&mut b, &[78]);
    reference(&mut b, 8);
    add(60, GEOMETRY, 18, header(60), Some(b));
    let mut b = header(20);
    u32(&mut b, 0);
    b.extend([0; 2]);
    b.push(u8::from(matrix.is_none()));
    if let Some(matrix) = matrix {
        for n in matrix {
            b.extend(n.to_le_bytes());
        }
    }
    b.push(0);
    reference(&mut b, 9);
    add(20, TRANSFORM, 0, b, None);
    let mut b = header(10);
    u32(&mut b, 0);
    b.extend([0, 1]);
    b.extend([0; 7]);
    u32(&mut b, op);
    u32(&mut b, 1);
    b.extend([0; 111]);
    refs(&mut b, &[44]);
    for ids in [&[][..], &[76][..], &[][..], &[][..]] {
        refs(&mut b, ids);
    }
    common(&mut b, "Extrude", 3, "Depth", suppressed);
    add(10, EXTRUDE, 8, b, None);
    add(4, "C3423E04-7E85-4B44-8E31-A5FA9C100052", 2, header(4), None);
    add(44, "2CA5A1CD-C99B-4C9B-91AF-57989148E841", 1, header(44), None);
    for id in [8, 9, 76, 78] {
        add(id, &format!("opaque-{id}"), 0, header(id), None);
    }
    design
}

#[test]
fn history_uses_timeline_order_names_suppression_and_placement() {
    for (op, expected) in [(1, Operation::Join), (2, Operation::Cut), (3, Operation::Intersect), (4, Operation::NewBody)] {
        let d = fixture(&[90, 10], op, true, None);
        let f = d.history().unwrap();
        assert_eq!(f.iter().map(|f| f.id).collect::<Vec<_>>(), [90, 10]);
        assert_eq!(f[0].name, "Round Δ");
        assert_eq!(f[1].name, "Depth");
        assert!(!f[0].suppressed);
        assert!(f[1].suppressed);
        assert_eq!(f[1].kind, HistoryKind::Extrude { operation: expected, profile_containers: vec![44] });
        let HistoryKind::Sketch { geometry, placement } = &f[0].kind else { panic!("sketch") };
        assert_eq!(*geometry, 60);
        assert_eq!(placement.entities, [78]);
        assert_eq!(placement.matrix, [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.]);
    }
    let matrix = [1., 0., 0., 2., 0., 0., 1., 3., 0., -1., 0., 4., 0., 0., 0., 1.];
    let f = fixture(&[10, 90], 4, false, Some(matrix)).history().unwrap();
    assert_eq!(f.iter().map(|f| f.id).collect::<Vec<_>>(), [10, 90]);
    let HistoryKind::Sketch { placement, .. } = &f[1].kind else { panic!("sketch") };
    assert_eq!(placement.matrix, matrix);
}

#[test]
fn malformed_or_unrecognized_history_is_not_silently_partial() {
    for order in [&[90, 90][..], &[90, 999][..], &[90, 78][..]] {
        assert!(fixture(order, 4, false, None).history().is_err());
    }
    assert!(fixture(&[90, 10], 5, false, None).history().is_err());
    assert!(fixture(&[90, 10], 4, false, Some([f64::NAN; 16])).history().is_err());
    let mut d = fixture(&[90, 10], 4, false, None);
    d.classes.iter_mut().find(|c| c.id == EXTRUDE).unwrap().version += 1;
    assert!(d.history().is_err());
    // Truncate every interpreted primary/secondary payload while preserving the
    // rest of the index. This exercises nested counts, strings and references.
    for (id, secondary) in [(7, false), (90, false), (60, true), (20, false), (10, false)] {
        let d = fixture(&[90, 10], 4, false, None);
        let records = Records::new(&d);
        let bytes = if secondary { records.secondary(id) } else { records.primary(id) }.unwrap();
        for keep in 0..bytes.len() {
            let mut d = fixture(&[90, 10], 4, false, None);
            let rec = d.records.iter().find(|r| r.id == id).unwrap();
            let offset = if secondary { rec.secondary_offset.unwrap() } else { rec.offset };
            let removed = bytes.len() - keep;
            d.bulk.drain(offset + keep..offset + bytes.len());
            for r in &mut d.records {
                if r.offset > offset {
                    r.offset -= removed;
                }
                if let Some(at) = &mut r.secondary_offset
                    && *at > offset
                {
                    *at -= removed;
                }
            }
            assert!(d.history().is_err(), "record {id}, secondary {secondary}, prefix {keep}");
        }
    }
}
