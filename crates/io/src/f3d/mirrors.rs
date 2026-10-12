//! Separate body mirrors observed with source, plane, suppression and mode controls.
//! Source recipes and tracked planes select geometry; evaluation caches do not.
use super::history::{empty, finish, payload, references, require_class, zeros};
use super::*;

pub(super) const CLASS: &str = "D1728651-3640-4CCF-8083-AF7703013978";
const HELPER: &str = "C444E6AD-33DC-4877-B037-A3A5518856B1";
const SOURCE: &str = "8DB90DAE-3FF5-4E01-BACC-00AAA3EBB3EF";
const INSTANCE: &str = "AD719C1A-11AF-4537-BB41-030A9091D36B";

pub(super) fn inputs(
    r: &mut Reader<'_>,
    records: &Records<'_>,
    classes: &BTreeMap<u64, &Class>,
    owner: u64,
    body_version: u32,
) -> Result<(u64, u64, usize, bool)> {
    empty(r)?;
    zeros(r, 1)?;
    if !super::attributes::read(r)?.is_empty() {
        return Err(invalid("unsupported mirror attributes"));
    }
    let helper = r.reference()?;
    require_class(classes, helper, HELPER, 1)?;
    let sources = r.reference()?;
    require_class(classes, sources, SOURCE, 1)?;
    let roles = super::history::body_roles(r, classes, body_version)?;
    let outputs = roles.get(1).ok_or_else(|| invalid("missing mirror outputs"))?;
    if !(1..=64).contains(&outputs.len()) || roles.iter().enumerate().any(|(i, ids)| i != 1 && !ids.is_empty()) {
        return Err(invalid("unsupported mirror body roles"));
    }
    let mut unique = BTreeSet::new();
    let mut suppressed = None;
    for output in outputs {
        if !unique.insert(*output) {
            return Err(invalid("duplicate mirror output"));
        }
        require_class(classes, *output, "D26351F0-5940-4D23-AA20-2C35475A6D9E", 2)?;
        let mut o = payload(records.primary(*output)?, *output)?;
        zeros(&mut o, 6)?;
        if o.wide_string()?.is_empty() {
            return Err(invalid("unsupported mirror output identity"));
        }
        let inactive = match o.take(2)? {
            [0, 1] => false,
            [1, 1] => true,
            _ => return Err(invalid("unsupported mirror output state")),
        };
        if suppressed.replace(inactive).is_some_and(|previous| previous != inactive) {
            return Err(invalid("inconsistent mirror output suppression"));
        }
        let body = o.reference()?;
        if !classes.get(&body).is_some_and(|c| c.id == "D3937028-C20C-4E65-B010-94AD418A5C20" && matches!(c.version, 18 | 19))
            || o.reference()? != owner
        {
            return Err(invalid("mirror output belongs to another feature"));
        }
        finish(&o)?;
    }
    let mut h = payload(records.primary(helper)?, helper)?;
    zeros(&mut h, 6)?;
    if h.take(1)? != [0] {
        return Err(invalid("combined mirror history is not supported yet"));
    }
    require_class(classes, h.reference()?, "D91D429C-1BB5-4640-B49F-69AF14F463E9", 0)?;
    let plane_container = h.reference()?;
    zeros(&mut h, 12)?;
    if h.u32()? != 2 || h.u32()? != 2 || h.u32()? != u32::MAX || h.take(5)? != [1, 1, 0, 0, 0] || h.u32()? != 2 {
        return Err(invalid("unsupported mirror selection or computation mode"));
    }
    zeros(&mut h, 4)?;
    if h.u64()? == 0 {
        return Err(invalid("empty mirror evaluation operation"));
    }
    let instances = references(&mut h)?;
    if instances.len() != 2 {
        return Err(invalid("unsupported mirror instance count"));
    }
    for (i, instance) in instances.iter().enumerate() {
        require_class(classes, *instance, INSTANCE, 0)?;
        let mut instance = payload(records.primary(*instance)?, *instance)?;
        zeros(&mut instance, 6)?;
        if instance.u32()? != i as u32 + 1 || instance.reference()? != helper {
            return Err(invalid("inconsistent mirror instance ownership"));
        }
        finish(&instance)?;
    }
    let cache = h.reference()?;
    require_class(classes, cache, "A085449A-5144-4B2B-B455-7F7035A40559", 1)?;
    let bytes = records.primary(cache)?;
    let mut cache = payload(bytes, cache)?;
    zeros(&mut cache, 4)?;
    if cache.take(5)? != [1, 1, 0, 0, 0] || cache.reference()? != owner {
        return Err(invalid("inconsistent mirror cache owner"));
    }
    owner_tail(&mut cache, owner)?;
    if h.reference()? != owner {
        return Err(invalid("inconsistent mirror helper owner"));
    }
    let source_container = h.reference()?;
    zeros(&mut h, 4)?;
    finish(&h)?;
    let objects = super::loft_faces::objects(records, classes, source_container, owner)?;
    if objects.len() != outputs.len() || objects.iter().collect::<BTreeSet<_>>().len() != objects.len() {
        return Err(invalid("inconsistent mirror source count"));
    }
    let mut s = payload(records.primary(sources)?, sources)?;
    zeros(&mut s, 10)?;
    if s.count(15)? != objects.len() {
        return Err(invalid("inconsistent mirror source relationship count"));
    }
    for object in objects {
        require_class(classes, object, "9716F783-676D-42E0-93F9-EBF273E7C035", 1)?;
        if s.reference()? != object {
            return Err(invalid("mirror source relationships disagree"));
        }
        for cached in references(&mut s)? {
            require_class(classes, cached, "C46D3EEB-40E1-43FA-83DC-9CB204335417", 1)?;
        }
    }
    if s.reference()? != owner {
        return Err(invalid("inconsistent mirror source owner"));
    }
    owner_tail(&mut s, owner)?;
    let planes = super::loft_faces::objects(records, classes, plane_container, owner)?;
    let [plane] = planes.as_slice() else { return Err(invalid("mirror requires one tracked plane")) };
    let plane = super::planes::tracked_plane(records, classes, *plane, owner)?;
    Ok((plane, source_container, outputs.len(), suppressed.ok_or_else(|| invalid("missing mirror output state"))?))
}

fn owner_tail(r: &mut Reader<'_>, owner: u64) -> Result<()> {
    r.at = r.bytes.len().checked_sub(11).filter(|at| *at >= r.at).ok_or_else(|| invalid("truncated mirror evaluation data"))?;
    if r.reference()? != owner {
        return Err(invalid("inconsistent mirror evaluation back-reference"));
    }
    finish(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::f3d::import_tests::{Fixture, common, fixture};
    use crate::f3d::relationship_tests::{reference, refs, u32, u64, wide};

    // Original, field-authored relationships with arbitrary IDs and identities.
    // Actual reconstruction is independently checked against local API controls.
    fn authored(suppressed: bool) -> Fixture {
        let mut f = fixture(false);
        let mut timeline = vec![0; 6];
        reference(&mut timeline, 4);
        refs(&mut timeline, &[8, 9, 19]);
        f.0.get_mut(&5).unwrap().2 = timeline;
        let mut b = vec![0; 6];
        reference(&mut b, 910);
        reference(&mut b, 930);
        for outputs in [&[][..], &[950][..], &[], &[]] {
            refs(&mut b, outputs);
        }
        common(&mut b, "Mirror", suppressed);
        f.put(19, CLASS, 0, b, None);
        let mut b = vec![0; 6];
        wide(&mut b, "original-mirror-output");
        b.extend([u8::from(suppressed), 1]);
        reference(&mut b, 951);
        reference(&mut b, 19);
        f.put(950, "D26351F0-5940-4D23-AA20-2C35475A6D9E", 2, b, None);
        f.put(951, "D3937028-C20C-4E65-B010-94AD418A5C20", 19, vec![], None);
        let mut b = vec![0; 7];
        reference(&mut b, 914);
        reference(&mut b, 903);
        b.extend([0; 12]);
        for n in [2, 2, u32::MAX] {
            u32(&mut b, n);
        }
        b.push(1);
        u32(&mut b, 1);
        u32(&mut b, 2);
        u32(&mut b, 0);
        u64(&mut b, 8007);
        refs(&mut b, &[911, 912]);
        reference(&mut b, 913);
        reference(&mut b, 19);
        reference(&mut b, 921);
        u32(&mut b, 0);
        f.put(910, HELPER, 1, b, None);
        f.put(914, "D91D429C-1BB5-4640-B49F-69AF14F463E9", 0, vec![], None);
        for (id, role) in [(911, 1), (912, 2)] {
            let mut b = vec![0; 6];
            u32(&mut b, role);
            reference(&mut b, 910);
            f.put(id, INSTANCE, 0, b, None);
        }
        let mut b = vec![0; 4];
        b.extend([1, 1, 0, 0, 0]);
        reference(&mut b, 19);
        b.extend([0; 17]);
        reference(&mut b, 19);
        f.put(913, "A085449A-5144-4B2B-B455-7F7035A40559", 1, b, None);
        for (id, object) in [(921, 920), (903, 901)] {
            let mut b = vec![0; 6];
            refs(&mut b, &[object]);
            reference(&mut b, 19);
            f.put(id, "2CA5A1CD-C99B-4C9B-91AF-57989148E841", 1, b, None);
        }
        f.put(920, "9716F783-676D-42E0-93F9-EBF273E7C035", 1, vec![], None);
        let mut b = vec![0; 10];
        u32(&mut b, 1);
        reference(&mut b, 920);
        refs(&mut b, &[]);
        reference(&mut b, 19);
        b.extend([0; 17]);
        reference(&mut b, 19);
        f.put(930, SOURCE, 1, b, None);
        let mut b = vec![0; 6];
        reference(&mut b, 902);
        u32(&mut b, 1);
        wide(&mut b, "original-plane");
        wide(&mut b, "original-plane-owner");
        u32(&mut b, 2);
        b.extend([0; 4]);
        refs(&mut b, &[]);
        b.extend([0; 60]);
        reference(&mut b, 19);
        f.put(901, "5A1BF548-241F-46FD-9FB5-E4B05126EB9D", 1, b, None);
        let mut b = vec![0; 6];
        u64(&mut b, 900);
        f.put(902, "90055C05-546C-4EE7-B3C9-3DD922AD0C9C", 0, b, None);
        f.put(900, super::super::planes::GEOMETRY, 1, vec![], None);
        f
    }

    #[test]
    fn mirror_history_retains_plane_source_and_suppression() {
        for suppressed in [false, true] {
            let history = authored(suppressed).design().history().unwrap();
            let mirror = history.last().unwrap();
            assert_eq!(mirror.name, "Mirror1");
            assert_eq!(mirror.suppressed, suppressed);
            assert_eq!(mirror.kind, HistoryKind::Mirror { plane: 900, source_container: 921, output_count: 1 });
        }
        let mut inconsistent = authored(false);
        let output = &mut inconsistent.0.get_mut(&950).unwrap().2;
        let at = output.len() - 24;
        output[at] = 1;
        assert!(inconsistent.design().history().is_err());
    }

    #[test]
    fn mirror_modes_and_broken_ownership_fail_without_cached_geometry_fallback() {
        for (id, offset, value) in [
            (910, 6, 1),   // combine
            (910, 41, 0),  // compute mode
            (910, 45, 1),  // feature rather than body
            (910, 78, 0),  // broken instance reference
            (910, 112, 9), // helper owner
            (911, 6, 2),   // instance role
            (912, 11, 9),  // instance owner
            (930, 15, 9),  // source disagreement
            (921, 22, 9),  // source container owner
            (913, 10, 9),  // cache owner
            (902, 6, 9),   // plane geometry identity
        ] {
            let mut f = authored(false);
            f.0.get_mut(&id).unwrap().2[offset] = value;
            assert!(f.design().history().is_err(), "{id}/{offset}");
        }
        for id in [19, 901, 902, 903, 910, 911, 912, 913, 921, 930, 950] {
            for keep in 0..authored(false).0[&id].2.len() {
                let mut f = authored(false);
                f.0.get_mut(&id).unwrap().2.truncate(keep);
                assert!(f.design().history().is_err(), "{id}/{keep}");
            }
        }
    }
}
