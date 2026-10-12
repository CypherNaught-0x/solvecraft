//! Explicit circular extrusion-cap identities observed in loft face controls.
//! Persistent producer/operation references are authoritative; cached geometry
//! is never used to choose a nearby face.
use super::history::{empty, finish, payload, references, require_class, zeros};
use super::*;

const IDENTITY: &str = "7ACC2A03-0261-4879-A14A-A93D661A5BDC";
const HANDLE: &str = "D26351F0-5940-4D23-AA20-2C35475A6D9E";

pub(super) struct Face {
    pub feature: u64,
    pub operation: u64,
    pub high: bool,
}

impl Face {
    pub fn anchor(&self, doc: &solvecraft_doc::Document, state: &solvecraft_doc::ModelState, id: u64) -> Result<solvecraft_doc::FaceAnchor> {
        let feature = doc.feature(id).filter(|f| !f.suppressed).ok_or_else(|| invalid("missing loft face producer"))?;
        let [body] = feature.body_names.as_slice() else {
            return Err(invalid("loft face requires one explicitly produced body"));
        };
        let body = state.body(body).ok_or_else(|| invalid("loft face body no longer exists"))?;
        let sources = body.body.face_sources().ok_or_else(|| invalid("loft face has no construction identities"))?;
        let indices: Vec<_> = sources.iter().enumerate().filter_map(|(index, source)| {
            matches!(source, Some(solvecraft_kernel::FaceSource::CertifiedExtrudeCap { producer, high, .. }) if *producer == id && *high == self.high).then_some(index)
        }).collect();
        let [index] = indices.as_slice() else {
            return Err(invalid("native loft face is missing or ambiguous"));
        };
        let plane = body.body.face_plane(*index).ok_or_else(|| invalid("loft face has no exact plane"))?;
        doc.capture_face_anchor(body, *index, &plane, &[], 0, &[]).map_err(|e| invalid(&e.to_string()))
    }
}

pub(super) fn objects(records: &Records<'_>, classes: &BTreeMap<u64, &Class>, id: u64, owner: u64) -> Result<Vec<u64>> {
    require_class(classes, id, "2CA5A1CD-C99B-4C9B-91AF-57989148E841", 1)?;
    let bytes = records.primary(id)?;
    let mut r = payload(bytes, id)?;
    empty(&mut r)?;
    zeros(&mut r, 2)?;
    let objects = references(&mut r)?;
    if objects.iter().any(|id| !classes.contains_key(id)) {
        return Err(invalid("missing loft container source"));
    }
    r.at = bytes.len().checked_sub(11).filter(|at| *at >= r.at).ok_or_else(|| invalid("truncated loft container"))?;
    if r.reference()? != owner {
        return Err(invalid("inconsistent loft container owner"));
    }
    finish(&r)?;
    Ok(objects)
}

fn cap_recipe(records: &Records<'_>, classes: &BTreeMap<u64, &Class>, id: u64) -> Result<(u64, bool)> {
    require_class(classes, id, IDENTITY, 3)?;
    let mut r = payload(records.primary(id)?, id)?;
    zeros(&mut r, 6)?;
    for value in [1, 3, 3] {
        if r.u32()? != value {
            return Err(invalid("unsupported loft face recipe header"));
        }
    }
    let mut terms = Vec::new();
    for _ in 0..3 {
        if r.u32()? != 1 {
            return Err(invalid("unsupported loft face term count"));
        }
        let label = r.string()?;
        zeros(&mut r, 4)?;
        if r.u32()? != 1 {
            return Err(invalid("unsupported loft face operation count"));
        }
        terms.push((label, r.u64()?));
    }
    let labels: Vec<_> = terms.iter().map(|t| t.0.as_str()).collect();
    let high = match labels.as_slice() {
        ["2", "-1", "3"] => true,
        ["1", "-102", "3"] => false,
        _ => return Err(invalid("unsupported loft face topology recipe")),
    };
    let operation = terms.first().ok_or_else(|| invalid("missing face terms"))?.1;
    if operation == 0 || terms.iter().any(|t| t.1 != operation) {
        return Err(invalid("inconsistent loft face operation ownership"));
    }
    zeros(&mut r, 4)?;
    if r.string()? != "bounded_face_recipe_data" {
        return Err(invalid("missing bounded loft face recipe"));
    }
    zeros(&mut r, 4)?;
    // Boundary incidence and cap role, independently checked with top, bottom
    // and overlapping second-body controls. Unknown trees are not label matches.
    for value in [-1_i32, 1, 1, -1, 2, 2, -1, 0, -1, 1, -1, 2, 2, -1, 0, -1, 1, 1, -1, 0, 0, -1, 1, 0].into_iter().chain(if high {
        [2, 2, 2, 2, 1, 0, 1, -1]
    } else {
        [2, 1, 1, 1, 2, 1, 2, -1]
    }) {
        if r.u32()? != value as u32 {
            return Err(invalid("unsupported loft face incidence tree"));
        }
    }
    finish(&r)?;
    Ok((operation, high))
}

fn producer(records: &Records<'_>, classes: &BTreeMap<u64, &Class>, handle: u64) -> Result<u64> {
    require_class(classes, handle, HANDLE, 2)?;
    let mut r = payload(records.primary(handle)?, handle)?;
    zeros(&mut r, 6)?;
    if r.wide_string()?.is_empty() || r.take(2)? != [0, 1] {
        return Err(invalid("unsupported loft body identity"));
    }
    let body = r.reference()?;
    let feature = r.reference()?;
    finish(&r)?;
    require_class(classes, body, "D3937028-C20C-4E65-B010-94AD418A5C20", 19)?;
    require_class(classes, feature, "DD405BC2-D673-44F0-8833-5CB2A1C186C7", 8)?;
    let mut r = payload(records.secondary(handle)?, handle)?;
    empty(&mut r)?;
    require_class(classes, r.reference()?, "C3423E04-7E85-4B44-8E31-A5FA9C100052", 2)?;
    finish(&r)?;
    let mut r = payload(records.primary(feature)?, feature)?;
    let input = super::history::extrude_inputs(&mut r, classes)?;
    if input.operation != solvecraft_doc::Operation::NewBody || input.start_offset.is_some() {
        return Err(invalid("loft face requires an independent profile-plane extrusion"));
    }
    references(&mut r)?;
    if !references(&mut r)?.is_empty() || references(&mut r)? != [handle] || !references(&mut r)?.is_empty() || !references(&mut r)?.is_empty() {
        return Err(invalid("loft body handle disagrees with its producer"));
    }
    Ok(feature)
}

fn selector_tail(r: &mut Reader<'_>, owner: u64) -> Result<u64> {
    let handles = references(r)?;
    let [handle] = handles.as_slice() else {
        return Err(invalid("loft selector requires one body"));
    };
    r.take(56)?; // inherited evaluation/cache state
    if r.reference()? != owner {
        return Err(invalid("loft selector belongs to another feature"));
    }
    finish(r)?;
    Ok(*handle)
}

pub(super) fn face(records: &Records<'_>, classes: &BTreeMap<u64, &Class>, id: u64, owner: u64) -> Result<Face> {
    require_class(classes, id, super::body_projections::SELECTOR, 3)?;
    let mut r = payload(records.primary(id)?, id)?;
    zeros(&mut r, 8)?;
    let (operation, high) = cap_recipe(records, classes, r.reference()?)?;
    if r.u32()? != 1 {
        return Err(invalid("unsupported loft face source count"));
    }
    r.wide_string()?;
    r.wide_string()?;
    if r.u32()? != 2 {
        return Err(invalid("unsupported loft face selection type"));
    }
    zeros(&mut r, 4)?;
    let cached = references(&mut r)?;
    let [cached] = cached.as_slice() else {
        return Err(invalid("loft face requires one cached identity"));
    };
    require_class(classes, *cached, "C46D3EEB-40E1-43FA-83DC-9CB204335417", 1)?;
    let handle = selector_tail(&mut r, owner)?;
    Ok(Face { feature: producer(records, classes, handle)?, operation, high })
}

pub(super) fn target(records: &Records<'_>, classes: &BTreeMap<u64, &Class>, container: u64, owner: u64) -> Result<(u64, u64)> {
    let objects = objects(records, classes, container, owner)?;
    let [id] = objects.as_slice() else {
        return Err(invalid("loft target requires one body"));
    };
    require_class(classes, *id, "9716F783-676D-42E0-93F9-EBF273E7C035", 1)?;
    let mut r = payload(records.primary(*id)?, *id)?;
    zeros(&mut r, 6)?;
    if r.u32()? != 1 {
        return Err(invalid("unsupported loft target operation count"));
    }
    let operation = r.u64()?;
    if operation == 0 || r.u32()? != 33 {
        return Err(invalid("unsupported loft target flags"));
    }
    let identity = r.reference()?;
    require_class(classes, identity, IDENTITY, 3)?;
    let mut identity = payload(records.primary(identity)?, identity)?;
    zeros(&mut identity, 6)?;
    for value in [1, 3, 1, 1] {
        if identity.u32()? != value {
            return Err(invalid("unsupported loft body recipe header"));
        }
    }
    if identity.string()?.parse::<u64>().ok() != Some(operation) || identity.u32()? != 4 {
        return Err(invalid("loft target recipe disagrees with operation"));
    }
    zeros(&mut identity, 12)?;
    if identity.string()? != "body_recipe_data" || identity.u32()? != 0 || identity.u32()? != u32::MAX {
        return Err(invalid("unsupported loft body recipe"));
    }
    finish(&identity)?;
    if r.u32()? != 1 {
        return Err(invalid("unsupported loft target source count"));
    }
    r.wide_string()?;
    r.wide_string()?;
    if r.u32()? != 2 {
        return Err(invalid("unsupported loft target selection type"));
    }
    zeros(&mut r, 8)?;
    let handle = selector_tail(&mut r, owner)?;
    Ok((producer(records, classes, handle)?, operation))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::f3d::import_tests::{Fixture, fixture};
    use crate::f3d::relationship_tests::{reference, refs, string, u32, u64, wide};

    // Original records with arbitrary IDs, strings and operation values.
    // No exported binary payload or cached geometry is copied into the tests.
    fn authored(high: bool) -> Fixture {
        let mut f = fixture(false);
        let mut output = Vec::new();
        refs(&mut output, &[80]);
        f.0.get_mut(&9).unwrap().2.splice(151..155, output);
        let mut handle = vec![0; 6];
        wide(&mut handle, "authored-body");
        handle.extend([0, 1]);
        reference(&mut handle, 81);
        reference(&mut handle, 9);
        let mut owner = vec![0; 4];
        reference(&mut owner, 4);
        f.put(80, HANDLE, 2, handle, Some(owner));
        f.put(81, "D3937028-C20C-4E65-B010-94AD418A5C20", 19, vec![], None);
        f.put(182, "C46D3EEB-40E1-43FA-83DC-9CB204335417", 1, vec![], None);
        let mut identity = vec![0; 6];
        for n in [1, 3, 3] {
            u32(&mut identity, n);
        }
        for label in if high { ["2", "-1", "3"] } else { ["1", "-102", "3"] } {
            u32(&mut identity, 1);
            string(&mut identity, label);
            u32(&mut identity, 0);
            u32(&mut identity, 1);
            u64(&mut identity, 8001);
        }
        u32(&mut identity, 0);
        string(&mut identity, "bounded_face_recipe_data");
        u32(&mut identity, 0);
        let tree = [-1_i32, 1, 1, -1, 2, 2, -1, 0, -1, 1, -1, 2, 2, -1, 0, -1, 1, 1, -1, 0, 0, -1, 1, 0];
        for n in tree.into_iter().chain(if high { [2, 2, 2, 2, 1, 0, 1, -1] } else { [2, 1, 1, 1, 2, 1, 2, -1] }) {
            u32(&mut identity, n as u32);
        }
        f.put(181, IDENTITY, 3, identity, None);
        let mut selector = vec![0; 8];
        reference(&mut selector, 181);
        u32(&mut selector, 1);
        wide(&mut selector, "authored-source");
        wide(&mut selector, "authored-face");
        u32(&mut selector, 2);
        u32(&mut selector, 0);
        refs(&mut selector, &[182]);
        tail(&mut selector);
        f.put(180, super::super::body_projections::SELECTOR, 3, selector, None);
        let mut identity = vec![0; 6];
        for n in [1, 3, 1, 1] {
            u32(&mut identity, n);
        }
        string(&mut identity, "8001");
        u32(&mut identity, 4);
        identity.extend([0; 12]);
        string(&mut identity, "body_recipe_data");
        u32(&mut identity, 0);
        u32(&mut identity, u32::MAX);
        f.put(191, IDENTITY, 3, identity, None);
        let mut selector = vec![0; 6];
        u32(&mut selector, 1);
        u64(&mut selector, 8001);
        u32(&mut selector, 33);
        reference(&mut selector, 191);
        u32(&mut selector, 1);
        wide(&mut selector, "authored-target");
        wide(&mut selector, "authored-body");
        u32(&mut selector, 2);
        selector.extend([0; 8]);
        tail(&mut selector);
        f.put(190, "9716F783-676D-42E0-93F9-EBF273E7C035", 1, selector, None);
        let mut container = vec![0; 6];
        refs(&mut container, &[190]);
        reference(&mut container, 19);
        f.put(192, "2CA5A1CD-C99B-4C9B-91AF-57989148E841", 1, container, None);
        f
    }

    fn tail(b: &mut Vec<u8>) {
        refs(b, &[80]);
        b.extend([0; 56]);
        reference(b, 19);
    }

    fn decode(f: Fixture, body: bool) -> Result<(u64, u64, bool)> {
        let d = f.design();
        let classes = d.classes.iter().flat_map(|c| c.objects.iter().map(move |id| (*id, c))).collect();
        let records = Records::new(&d);
        if body {
            let (producer, operation) = target(&records, &classes, 192, 19)?;
            Ok((producer, operation, false))
        } else {
            let face = face(&records, &classes, 180, 19)?;
            Ok((face.feature, face.operation, face.high))
        }
    }

    #[test]
    fn authored_cap_and_join_target_preserve_producer_operation_and_side() {
        for high in [false, true] {
            assert_eq!(decode(authored(high), false).unwrap(), (9, 8001, high));
            assert_eq!(decode(authored(high), true).unwrap(), (9, 8001, false));
        }
    }

    #[test]
    fn malformed_face_and_target_identities_fail_without_geometry_fallback() {
        for (body, ids) in [(false, &[80, 180, 181][..]), (true, &[80, 190, 191, 192][..])] {
            for id in ids {
                for keep in 0..authored(true).0[id].2.len() {
                    let mut f = authored(true);
                    f.0.get_mut(id).unwrap().2.truncate(keep);
                    assert!(decode(f, body).is_err(), "{body}/{id}/{keep}");
                }
            }
        }
        for (body, id, offset) in [(false, 180, 0), (false, 181, 6), (true, 190, 10), (true, 191, 6)] {
            let mut f = authored(true);
            f.0.get_mut(&id).unwrap().2[offset] ^= 1;
            assert!(decode(f, body).is_err());
        }
    }
}
