//! Native topology-operation ownership, verified with top/bottom edge and
//! overlapping independent-body controls. Tags are not bulk record identities.
use super::history::{empty, finish, payload, require_class, zeros};
use super::*;

const CLASS: &str = "5D89B935-3F04-4CAB-81F5-E7B309E7E13F";

impl Design {
    /// Map kernel operation tags to native feature records. Both native
    /// directions must agree; no ownership is inferred from geometry or order.
    pub fn operation_owners(&self) -> Result<BTreeMap<u64, u64>> {
        let class = self.classes.iter().find(|c| c.id == CLASS).ok_or_else(|| invalid("missing topology operation map"))?;
        if class.version != 1 || class.objects.len() != 1 {
            return Err(invalid("unsupported topology operation map version or count"));
        }
        let id = class.objects.first().copied().ok_or_else(|| invalid("missing operation map identity"))?;
        let records = Records::new(self);
        let classes: BTreeMap<_, _> = self.classes.iter().flat_map(|c| c.objects.iter().map(move |id| (*id, c))).collect();
        let mut r = payload(records.primary(id)?, id)?;
        empty(&mut r)?;
        zeros(&mut r, 2)?;
        let mut features = BTreeSet::new();
        let mut owners = BTreeMap::new();
        for _ in 0..r.count(12)? {
            let feature = r.u64()?;
            if !classes.contains_key(&feature) || !features.insert(feature) {
                return Err(invalid("missing or duplicate topology operation feature"));
            }
            for _ in 0..r.count(8)? {
                let tag = r.u64()?;
                if tag == 0 || owners.insert(tag, feature).is_some() {
                    return Err(invalid("duplicate or empty topology operation tag"));
                }
            }
        }
        let mut reverse = BTreeMap::new();
        for _ in 0..r.count(16)? {
            if reverse.insert(r.u64()?, r.u64()?).is_some() {
                return Err(invalid("duplicate reverse topology operation tag"));
            }
        }
        if reverse != owners {
            return Err(invalid("inconsistent topology operation ownership"));
        }
        zeros(&mut r, 4)?;
        let owner = r.reference()?;
        require_class(&classes, owner, "C3423E04-7E85-4B44-8E31-A5FA9C100052", 2)?;
        finish(&r)?;
        Ok(owners)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::f3d::import_tests::{Fixture, fixture};
    use crate::f3d::relationship_tests::{reference, u32, u64};

    fn mapped() -> Fixture {
        let mut f = fixture(false);
        let mut b = vec![0; 6];
        u32(&mut b, 2);
        for (feature, tags) in [(9, vec![8001, 8002]), (8, vec![8000])] {
            u64(&mut b, feature);
            u32(&mut b, tags.len() as u32);
            for tag in tags {
                u64(&mut b, tag);
            }
        }
        u32(&mut b, 3);
        for (tag, feature) in [(8000, 8), (8001, 9), (8002, 9)] {
            u64(&mut b, tag);
            u64(&mut b, feature);
        }
        u32(&mut b, 0);
        reference(&mut b, 4);
        f.put(180, CLASS, 1, b, None);
        f
    }

    #[test]
    fn operation_tags_are_owned_by_features_not_records_with_similar_ids() {
        assert_eq!(mapped().design().operation_owners().unwrap(), BTreeMap::from([(8000, 8), (8001, 9), (8002, 9)]));
    }

    #[test]
    fn damaged_operation_maps_fail_without_partial_ownership() {
        let n = mapped().0[&180].2.len();
        for keep in 0..n {
            let mut f = mapped();
            f.0.get_mut(&180).unwrap().2.truncate(keep);
            assert!(f.design().operation_owners().is_err(), "prefix {keep}");
        }
        // Missing/duplicate feature, duplicate/zero tag, reverse disagreement,
        // duplicate reverse tag and wrong owner class.
        for (at, value) in [(10, 999), (38, 9), (30, 8001), (22, 0), (70, 9), (78, 8000), (115, 9)] {
            let mut f = mapped();
            f.0.get_mut(&180).unwrap().2[at..at + 8].copy_from_slice(&u64::to_le_bytes(value));
            assert!(f.design().operation_owners().is_err(), "offset {at}");
        }
        let mut f = mapped();
        f.0.get_mut(&180).unwrap().1 = 0;
        assert!(f.design().operation_owners().is_err());
    }
}
