//! Explicit extrusion face labels linked to complete sketch curves. Labels are
//! never inferred from registration order, coordinates, or cached B-rep faces.
use super::history::{finish, payload, require_class, zeros};
use super::*;

const LABELS: &str = "B7F34D7B-DAC4-41A5-B67D-D672537C3BFA";
const WRAPPER: &str = "335CC7BD-BBE5-40AA-9E8C-5EC99D427A3A";
const CURVE: &str = "2DEE34BE-FA70-4D54-979C-164D86FDA786";

fn curve_identity(records: &Records<'_>, classes: &BTreeMap<u64, &Class>, feature: u64, label: u64) -> Result<(u64, [u64; 2])> {
    require_class(classes, feature, "DD405BC2-D673-44F0-8833-5CB2A1C186C7", 8)?;
    // This slot is part of the version-2 inherited feature tail consumed by
    // history::timeline. Older tails remain unsupported for side attachments.
    if classes.values().any(|c| c.id == "A0A15D26-1F3B-4120-A3F1-9CDDA189AB74" && c.version != 2) {
        return Err(invalid("unsupported side-role inherited feature version"));
    }
    let bytes = records.primary(feature)?;
    let mut r = payload(bytes, feature)?;
    r.at = bytes.len().checked_sub(59).filter(|at| *at >= r.at).ok_or_else(|| invalid("truncated side-role feature tail"))?;
    let labels = r.reference()?;
    require_class(classes, labels, LABELS, 2)?;
    let mut r = payload(records.primary(labels)?, labels)?;
    zeros(&mut r, 6)?;
    let count = r.count(19)?;
    if count > 1024 {
        return Err(invalid("extrusion label table exceeds budget"));
    }
    let mut entries = BTreeMap::new();
    let mut ids = BTreeSet::new();
    for _ in 0..count {
        let id = r.reference()?;
        let role = r.u64()?;
        if role < 3 || !classes.contains_key(&id) || !ids.insert(id) || entries.insert(role, id).is_some() {
            return Err(invalid("invalid or duplicate extrusion label"));
        }
    }
    let next = r.u64()?;
    if entries.keys().any(|role| *role >= next) {
        return Err(invalid("inconsistent extrusion label counter"));
    }
    finish(&r)?;
    let mut r = payload(records.secondary(labels)?, labels)?;
    zeros(&mut r, 5)?;
    finish(&r)?;
    let id = *entries.get(&label).ok_or_else(|| invalid("missing extrusion side label"))?;
    require_class(classes, id, WRAPPER, 0)?;
    let mut r = payload(records.primary(id)?, id)?;
    zeros(&mut r, 5)?;
    if r.u32()? != 1 {
        return Err(invalid("unsupported side-role source count"));
    }
    zeros(&mut r, 1)?;
    let curve = r.reference()?;
    zeros(&mut r, 1)?;
    finish(&r)?;
    require_class(classes, curve, CURVE, 2)?;
    let mut r = payload(records.primary(curve)?, curve)?;
    zeros(&mut r, 6)?;
    for value in [1, 0, 1, 3] {
        if r.u32()? != value {
            return Err(invalid("unsupported side-role curve header"));
        }
    }
    let profile_id = r.u64()?;
    if profile_id == 0 || r.u64()? != 0 {
        return Err(invalid("unsupported side-role curve piece"));
    }
    for value in [1, 1, 1, 0, 0] {
        if r.u32()? != value {
            return Err(invalid("unsupported side-role curve flags"));
        }
    }
    let mut adjacent = [0; 2];
    for id in &mut adjacent {
        if r.u32()? != 1 {
            return Err(invalid("unsupported side-role adjacency count"));
        }
        *id = r.u64()?;
        if *id == 0 || *id == profile_id {
            return Err(invalid("invalid side-role adjacency"));
        }
    }
    if adjacent[0] == adjacent[1] {
        return Err(invalid("duplicate side-role adjacency"));
    }
    finish(&r)?;
    Ok((profile_id, adjacent))
}

pub(super) fn curve(
    records: &Records<'_>,
    classes: &BTreeMap<u64, &Class>,
    feature: u64,
    label: u64,
    profile: &SketchProfile,
    lines: &BTreeMap<u64, SketchLine>,
) -> Result<u64> {
    let (identity, neighbors) = curve_identity(records, classes, feature, label)?;
    let [loops] = profile.regions.as_slice() else { return Err(invalid("side-role requires one profile region")) };
    let [boundary] = loops.as_slice() else { return Err(invalid("side-role requires one profile boundary")) };
    let mut owned = BTreeMap::new();
    for id in &boundary.curves {
        let line = lines.get(id).filter(|l| l.sketch == profile.geometry).ok_or_else(|| invalid("missing side-role sketch line"))?;
        let Some((key, 0)) = line.profile_id else { return Err(invalid("unsupported side-role sketch curve piece")) };
        if owned.insert(key, line).is_some() {
            return Err(invalid("ambiguous side-role sketch curve"));
        }
    }
    let line = owned.get(&identity).ok_or_else(|| invalid("side-role curve outside selected profile"))?;
    let mut shared = BTreeSet::new();
    for neighbor in neighbors {
        let other = owned.get(&neighbor).ok_or_else(|| invalid("side-role neighbor outside selected profile"))?;
        let common: Vec<_> = [line.start_point, line.end_point].into_iter().filter(|p| [other.start_point, other.end_point].contains(p)).collect();
        let [point] = common.as_slice() else { return Err(invalid("side-role curves are not incident")) };
        if !shared.insert(*point) {
            return Err(invalid("side-role neighbors share the same endpoint"));
        }
    }
    Ok(line.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::f3d::import_tests::Fixture;
    use crate::f3d::relationship_tests::{reference, u32, u64};

    // Independently authored label numbers and profile identities. The label
    // sequence intentionally differs from curve registration and loop order.
    fn authored() -> Fixture {
        let mut f = crate::f3d::polygon_tests::rectangle();
        let bytes = &mut f.0.get_mut(&9).unwrap().2;
        let at = bytes.len() - 59;
        let mut slot = Vec::new();
        reference(&mut slot, 600);
        bytes[at..at + 11].copy_from_slice(&slot);
        let mut labels = vec![0; 6];
        u32(&mut labels, 4);
        for (i, (label, profile, neighbors)) in
            [(23, 19, [18, 20]), (13, 17, [20, 18]), (47, 20, [19, 17]), (17, 18, [17, 19])].into_iter().enumerate()
        {
            let id = 701 + i as u64 * 2;
            reference(&mut labels, id);
            u64(&mut labels, label);
            let mut wrapper = vec![0; 5];
            u32(&mut wrapper, 1);
            wrapper.push(0);
            reference(&mut wrapper, id + 1);
            wrapper.push(0);
            f.put(id, WRAPPER, 0, wrapper, None);
            let mut curve = vec![0; 6];
            for n in [1, 0, 1, 3] {
                u32(&mut curve, n);
            }
            u64(&mut curve, profile);
            u64(&mut curve, 0);
            for n in [1, 1, 1, 0, 0] {
                u32(&mut curve, n);
            }
            for neighbor in neighbors {
                u32(&mut curve, 1);
                u64(&mut curve, neighbor);
            }
            f.put(id + 1, CURVE, 2, curve, None);
        }
        u64(&mut labels, 48);
        f.put(600, LABELS, 2, labels, Some(vec![0; 5]));
        f
    }

    fn resolve(f: Fixture, label: u64, reverse: bool) -> Result<u64> {
        let d = f.design();
        let classes = d.classes.iter().flat_map(|c| c.objects.iter().map(move |id| (*id, c))).collect();
        let profile = &d.sketch_profiles()?[0];
        let mut lines: BTreeMap<_, _> = d
            .sketch_lines()?
            .into_iter()
            .map(|mut l| {
                l.profile_id = l.profile_id.map(|(id, _)| (id, 0));
                if reverse {
                    std::mem::swap(&mut l.start, &mut l.end);
                    std::mem::swap(&mut l.start_point, &mut l.end_point);
                }
                (l.id, l)
            })
            .collect();
        if label == 99 {
            lines.get_mut(&31).unwrap().start_point = 999;
        }
        curve(&Records::new(&d), &classes, 9, if label == 99 { 13 } else { label }, profile, &lines)
    }

    #[test]
    fn side_labels_follow_explicit_curve_and_endpoint_references() {
        for reverse in [false, true] {
            for (label, expected) in [(13, 31), (17, 32), (23, 33), (47, 34)] {
                assert_eq!(resolve(authored(), label, reverse).unwrap(), expected);
            }
        }
        assert!(resolve(authored(), 99, false).is_err(), "a nonincident curve cannot be rescued by coordinates");
        assert!(resolve(authored(), 3, false).is_err(), "labels are not ordinal curve positions");
    }

    #[test]
    fn malformed_side_role_records_fail_before_attachment() {
        for id in [600, 701, 702] {
            for keep in 0..authored().0[&id].2.len() {
                let mut f = authored();
                f.0.get_mut(&id).unwrap().2.truncate(keep);
                assert!(resolve(f, 23, false).is_err(), "{id}/{keep}");
            }
        }
        for (id, at, bytes) in [
            (600, 6, u32::MAX.to_le_bytes().to_vec()),
            (600, 40, 23_u64.to_le_bytes().to_vec()),
            (701, 5, 2_u32.to_le_bytes().to_vec()),
            (702, 22, 999_u64.to_le_bytes().to_vec()),
            (702, 30, 1_u64.to_le_bytes().to_vec()),
            (702, 62, 999_u64.to_le_bytes().to_vec()),
        ] {
            let mut f = authored();
            f.0.get_mut(&id).unwrap().2[at..at + bytes.len()].copy_from_slice(&bytes);
            assert!(resolve(f, 23, false).is_err(), "{id}/{at}");
        }
    }
}
