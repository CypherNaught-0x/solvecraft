//! Native chamfer sets verified by controlled distance, angle, flip and chain edits.
use super::history::{empty, finish, payload, references, require_class, zeros};
use super::*;

pub(super) const CLASS: &str = "F757D611-217B-4B72-9C41-B617BDCE43DA";
const SET: &str = "0D383DB2-A6E5-48C7-82D5-ED3DD0C275C5";
const BINDING: &str = "D91D429C-1BB5-4640-B49F-69AF14F463E9";

#[derive(Debug, PartialEq)]
pub struct ChamferSet {
    pub container: u64,
    pub distance: u64,
    pub distance2: Option<u64>,
    pub angle: Option<u64>,
    pub flip: bool,
    pub tangent_chain: bool,
    pub suppressed: bool,
}

fn flag(r: &mut Reader<'_>) -> Result<bool> {
    match r.take(1)? {
        [0] => Ok(false),
        [1] => Ok(true),
        _ => Err(invalid("invalid chamfer boolean")),
    }
}
fn optional(r: &mut Reader<'_>, classes: &BTreeMap<u64, &Class>) -> Result<Option<u64>> {
    if r.bytes.get(r.at) == Some(&0) {
        r.take(1)?;
        Ok(None)
    } else {
        let id = r.reference()?;
        require_class(classes, id, BINDING, 0)?;
        Ok(Some(id))
    }
}

pub(super) fn inputs(
    r: &mut Reader<'_>,
    records: &Records<'_>,
    classes: &BTreeMap<u64, &Class>,
    owner: u64,
    version: u32,
) -> Result<Vec<ChamferSet>> {
    empty(r)?;
    zeros(r, 1)?;
    if !super::attributes::read(r)?.is_empty() || r.u32()? != 1 {
        return Err(invalid("unsupported chamfer attributes or mode"));
    }
    if version == 3 {
        zeros(r, 1)?;
    }
    if !flag(r)? {
        return Err(invalid("unsupported chamfer edge table"));
    }
    // Output topology labels are not input edge selectors. Retain structural
    // validation without treating a label as an ordinal in regenerated geometry.
    let count = r.count(10)?;
    if count == 0 || count > 1000 {
        return Err(invalid("invalid chamfer label count"));
    }
    let mut labels = BTreeSet::new();
    let mut group_labels = Vec::new();
    for _ in 0..count {
        let label = r.u64()?;
        if label == 0 || !labels.insert(label) {
            return Err(invalid("duplicate or empty chamfer label"));
        }
        let [a, b] = r.take(2)? else {
            return Err(invalid("missing chamfer group label"));
        };
        let group = u16::from_le_bytes([*a, *b]);
        group_labels.push(usize::from(group));
    }
    let helpers = references(r)?;
    if !(1..=64).contains(&helpers.len())
        || helpers.iter().collect::<BTreeSet<_>>().len() != helpers.len()
        || group_labels.iter().any(|g| *g == 0 || *g > helpers.len())
    {
        return Err(invalid("invalid chamfer edge-set table"));
    }
    let containers = references(r)?;
    let mut sets = Vec::new();
    for (index, helper) in helpers.into_iter().enumerate() {
        require_class(classes, helper, SET, 2)?;
        let mut h = payload(records.primary(helper)?, helper)?;
        zeros(&mut h, 6)?;
        let tangent_chain = flag(&mut h)?;
        let flip = flag(&mut h)?;
        let angle = optional(&mut h, classes)?;
        let distance = h.reference()?;
        require_class(classes, distance, BINDING, 0)?;
        let distance2 = optional(&mut h, classes)?;
        let mode = h.u32()?;
        if !matches!((mode, angle.is_some(), distance2.is_some()), (1, false, false) | (2, false, true) | (3, true, false)) || (mode == 1 && flip) {
            return Err(invalid("inconsistent chamfer mode and dimensions"));
        }
        let container = h.reference()?;
        require_class(classes, container, "2CA5A1CD-C99B-4C9B-91AF-57989148E841", 1)?;
        zeros(&mut h, 2)?;
        if h.u32()? as usize != index + 1 || !flag(&mut h)? {
            return Err(invalid("invalid chamfer group order"));
        }
        let suppressed = flag(&mut h)?;
        zeros(&mut h, 1)?;
        require_class(classes, h.reference()?, "6CCF41D5-40BE-48ED-A834-18F3EAED6C57", 1)?;
        zeros(&mut h, 1)?;
        if h.reference()? != owner {
            return Err(invalid("chamfer group belongs to another feature"));
        }
        finish(&h)?;
        sets.push(ChamferSet { container, distance, distance2, angle, flip, tangent_chain, suppressed });
    }
    if containers != sets.iter().map(|s| s.container).collect::<Vec<_>>() || containers.iter().collect::<BTreeSet<_>>().len() != containers.len() {
        return Err(invalid("chamfer feature and group selections disagree"));
    }
    Ok(sets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::f3d::import_tests::{Fixture, common, fixture};
    use crate::f3d::relationship_tests::{reference, refs, u32, u64};

    fn authored(version: u32, mode: u32, flip: bool, chain: bool, suppressed: bool) -> Fixture {
        let mut f = fixture(false);
        let mut timeline = vec![0; 6];
        reference(&mut timeline, 4);
        refs(&mut timeline, &[8, 9, 19]);
        f.0.get_mut(&5).unwrap().2 = timeline;
        let mut feature = vec![0; 6];
        u32(&mut feature, 1);
        if version == 3 {
            feature.push(0);
        }
        feature.push(1);
        u32(&mut feature, 1);
        u64(&mut feature, 71);
        feature.extend([1, 0]);
        refs(&mut feature, &[900]);
        refs(&mut feature, &[901]);
        common(&mut feature, "Chamfer", suppressed);
        f.put(19, CLASS, version, feature, None);
        let mut set = vec![0; 6];
        set.extend([u8::from(chain), u8::from(flip)]);
        if mode == 3 {
            reference(&mut set, 904);
        } else {
            set.push(0);
        }
        reference(&mut set, 902);
        if mode == 2 {
            reference(&mut set, 903);
        } else {
            set.push(0);
        }
        u32(&mut set, mode);
        reference(&mut set, 901);
        set.extend([0; 2]);
        u32(&mut set, 1);
        set.extend([1, u8::from(suppressed), 0]);
        reference(&mut set, 905);
        set.push(0);
        reference(&mut set, 19);
        f.put(900, SET, 2, set, None);
        f.put(901, "2CA5A1CD-C99B-4C9B-91AF-57989148E841", 1, vec![], None);
        for id in [902, 903, 904] {
            f.put(id, BINDING, 0, vec![], None);
        }
        f.put(905, "6CCF41D5-40BE-48ED-A834-18F3EAED6C57", 1, vec![], None);
        f
    }

    #[test]
    fn chamfer_modes_keep_bindings_flip_chain_and_suppression() {
        for version in [2, 3] {
            for mode in 1..=3 {
                for flip in [false, true] {
                    for chain in [false, true] {
                        for suppressed in [false, true] {
                            let decoded = authored(version, mode, flip, chain, suppressed).design().history();
                            if mode == 1 && flip {
                                assert!(decoded.is_err());
                                continue;
                            }
                            let h = decoded.unwrap();
                            assert_eq!(
                                h.last().unwrap().kind,
                                HistoryKind::Chamfer {
                                    sets: vec![ChamferSet {
                                        container: 901,
                                        distance: 902,
                                        distance2: (mode == 2).then_some(903),
                                        angle: (mode == 3).then_some(904),
                                        flip,
                                        tangent_chain: chain,
                                        suppressed
                                    }]
                                }
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn malformed_chamfers_do_not_drop_dimensions_or_relationships() {
        for mode in 1..=3 {
            for id in [19, 900] {
                for keep in 0..authored(3, mode, false, false, false).0[&id].2.len() {
                    let mut f = authored(3, mode, false, false, false);
                    f.0.get_mut(&id).unwrap().2.truncate(keep);
                    assert!(f.design().history().is_err(), "{mode}/{id}/{keep}");
                }
            }
            for offset in [6, 7] {
                let mut f = authored(3, mode, false, false, false);
                f.0.get_mut(&900).unwrap().2[offset] = 2;
                assert!(f.design().history().is_err());
            }
            let mut f = authored(3, mode, false, false, false);
            let b = &mut f.0.get_mut(&900).unwrap().2;
            let at = b.len() - 10;
            b[at] = 9;
            assert!(f.design().history().is_err(), "wrong group owner");
            let mut f = authored(3, mode, false, false, false);
            f.0.get_mut(&900).unwrap().2[if mode == 1 { 43 } else { 53 }] = 1;
            assert!(f.design().history().is_err(), "suppression mismatch");
            let mut f = authored(3, mode, false, false, false);
            f.0.get_mut(&902).unwrap().0 = SET.into();
            assert!(f.design().history().is_err(), "wrong dimension class");
        }
    }
}
