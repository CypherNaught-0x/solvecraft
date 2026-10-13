//! Constant-radius edge sets and explicit edge incidence, observed independently.
use super::history::{empty, finish, payload, references, require_class, zeros};
use super::*;

pub(super) const CLASS: &str = "A07D5F17-68CB-464D-9935-BF68E98A865F";
const SET: &str = "4A557CE2-375C-4F69-AA76-2403109F1493";
const CONTAINER: &str = "2CA5A1CD-C99B-4C9B-91AF-57989148E841";

#[derive(Debug, PartialEq)]
pub struct FilletSet {
    pub container: u64,
    pub radius: u64,
    pub weight: u64,
    pub tangent_chain: bool,
}

pub(super) fn inputs(r: &mut Reader<'_>, records: &Records<'_>, classes: &BTreeMap<u64, &Class>, version: u32) -> Result<Vec<FilletSet>> {
    empty(r)?;
    zeros(r, 1)?;
    if !super::attributes::read(r)?.is_empty() {
        return Err(invalid("unsupported fillet attributes"));
    }
    let helpers = references(r)?;
    if !(1..=64).contains(&helpers.len()) || helpers.iter().collect::<BTreeSet<_>>().len() != helpers.len() {
        return Err(invalid("invalid fillet edge-set count"));
    }
    zeros(r, 4)?;
    if r.take(1)? != [1] {
        return Err(invalid("unsupported fillet feature mode"));
    }
    if version == 3 {
        zeros(r, 1)?;
    }
    if r.take(1)? != [1] {
        return Err(invalid("unsupported fillet corner mode"));
    }
    zeros(r, 9)?;
    let containers = references(r)?;
    let mut sets = Vec::new();
    for (index, helper) in helpers.into_iter().enumerate() {
        let version = classes
            .get(&helper)
            .filter(|c| c.id == SET && matches!(c.version, 9 | 10))
            .map(|c| c.version)
            .ok_or_else(|| invalid("unsupported fillet edge-set schema"))?;
        let mut h = payload(records.primary(helper)?, helper)?;
        zeros(&mut h, if version == 10 { 8 } else { 7 })?;
        if h.take(2)? != [1, 0] {
            return Err(invalid("unsupported fillet radius mode"));
        }
        if version == 10 {
            zeros(&mut h, 1)?;
        }
        let tangent_chain = match h.take(1)? {
            [0] => false,
            [1] => true,
            _ => return Err(invalid("invalid fillet tangent-chain flag")),
        };
        zeros(&mut h, 2)?;
        let container = h.reference()?;
        require_class(classes, container, CONTAINER, 1)?;
        zeros(&mut h, if version == 10 { 23 } else { 21 })?;
        let radius = h.reference()?;
        zeros(&mut h, 4)?;
        if h.u32()? as usize != index {
            return Err(invalid("inconsistent fillet edge-set order"));
        }
        let weight = h.reference()?;
        for id in [radius, weight] {
            require_class(classes, id, "D91D429C-1BB5-4640-B49F-69AF14F463E9", 0)?;
        }
        zeros(&mut h, 4)?;
        finish(&h)?;
        sets.push(FilletSet { container, radius, weight, tangent_chain });
    }
    if containers != sets.iter().map(|s| s.container).collect::<Vec<_>>() || containers.iter().collect::<BTreeSet<_>>().len() != containers.len() {
        return Err(invalid("fillet feature and edge-set selections disagree"));
    }
    Ok(sets)
}

#[derive(Debug, PartialEq)]
pub(super) struct Edge {
    pub feature: u64,
    pub operation: u64,
    pub faces: [u32; 2],
    pub labels: Vec<u32>,
    pub endpoint_cycles: [[u32; 3]; 2],
}

pub(super) fn edge(records: &Records<'_>, classes: &BTreeMap<u64, &Class>, id: u64, owner: u64) -> Result<Edge> {
    let version = super::loft_faces::selector_version(classes, id)?;
    let mut r = payload(records.primary(id)?, id)?;
    zeros(&mut r, if version == 3 { 8 } else { 7 })?;
    let identity = r.reference()?;
    require_class(classes, identity, "7ACC2A03-0261-4879-A14A-A93D661A5BDC", 3)?;
    let mut recipe = payload(records.primary(identity)?, identity)?;
    let (operation, labels, endpoint_cycles) = edge_recipe(&mut recipe)?;
    if r.u32()? != 1 {
        return Err(invalid("unsupported fillet edge source count"));
    }
    if r.wide_string()?.is_empty() || r.wide_string()?.is_empty() {
        return Err(invalid("empty fillet edge source identity"));
    }
    if r.u32()? != 2 {
        return Err(invalid("unsupported fillet edge selection"));
    }
    zeros(&mut r, 4)?;
    for cached in references(&mut r)? {
        // Fillet selections retain a point cache. Its coordinates never choose
        // the edge: the explicit recipe and producer handle do that below.
        require_class(classes, cached, "50AEE8B4-9456-4DD7-847D-6F6C909BBB9D", 0)?;
    }
    let handle = super::loft_faces::selector_tail(&mut r, records, owner)?;
    let feature = super::loft_faces::producer(records, classes, handle)?;
    let faces = [*labels.first().ok_or_else(|| invalid("missing fillet face"))?, *labels.get(1).ok_or_else(|| invalid("missing fillet face"))?];
    Ok(Edge { feature, operation, faces, labels, endpoint_cycles })
}

// Two incident faces and two endpoint faces. The ordered incidence branches
// validate the selected edge; face labels are resolved through the producer table.
type EdgeRecipe = (u64, Vec<u32>, [[u32; 3]; 2]);
fn edge_recipe(r: &mut Reader<'_>) -> Result<EdgeRecipe> {
    zeros(r, 6)?;
    for n in [1, 3, 4] {
        if r.u32()? != n {
            return Err(invalid("unsupported fillet edge identity"));
        }
    }
    let mut operation = None;
    let mut labels = Vec::new();
    for _ in 0..4 {
        if r.u32()? != 1 {
            return Err(invalid("unsupported composite fillet edge term"));
        }
        let text = r.string()?;
        let label = text.parse::<u32>().ok().filter(|n| *n > 0 && n.to_string() == text).ok_or_else(|| invalid("invalid fillet face label"))?;
        if labels.contains(&label) {
            return Err(invalid("duplicate fillet face label"));
        }
        labels.push(label);
        zeros(r, 4)?;
        if r.u32()? != 1 {
            return Err(invalid("unsupported fillet edge operation count"));
        }
        let tag = r.u64()?;
        if tag == 0 || operation.is_some_and(|operation| operation != tag) {
            return Err(invalid("fillet edge has inconsistent operation ownership"));
        }
        operation = Some(tag);
    }
    zeros(r, 4)?;
    if r.string()? != "edge_recipe_data" || r.u32()? != u32::MAX {
        return Err(invalid("unsupported fillet edge recipe"));
    }
    for n in [u32::MAX, 2, 0, u32::MAX, 1, u32::MAX, 2] {
        if r.u32()? != n {
            return Err(invalid("invalid fillet edge face incidence"));
        }
    }
    let mut endpoint_cycles = [[0; 3]; 2];
    for (end, cycle) in [2, 3].into_iter().zip(&mut endpoint_cycles) {
        if r.u32()? != u32::MAX || r.u32()? != 3 {
            return Err(invalid("invalid fillet edge endpoint"));
        }
        let mut terms = BTreeSet::new();
        for term in cycle {
            *term = r.u32()?;
            if !terms.insert(*term) || r.u32()? != u32::MAX {
                return Err(invalid("invalid fillet edge endpoint incidence"));
            }
        }
        if terms != BTreeSet::from([0, 1, end]) {
            return Err(invalid("fillet edge endpoints disagree with its faces"));
        }
        zeros(r, 8)?;
    }
    if r.u32()? != u32::MAX {
        return Err(invalid("unsupported fillet edge tracking"));
    }
    finish(r)?;
    Ok((operation.ok_or_else(|| invalid("missing fillet operation"))?, labels, endpoint_cycles))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::f3d::import_tests::{Fixture, common, fixture};
    use crate::f3d::relationship_tests::{reference, refs, string, u32, u64, wide};

    fn authored(version: u32, tangent: bool, suppressed: bool) -> Fixture {
        let mut f = fixture(false);
        let mut timeline = vec![0; 6];
        reference(&mut timeline, 4);
        refs(&mut timeline, &[8, 9, 19]);
        f.0.get_mut(&5).unwrap().2 = timeline;
        let mut feature = vec![0; 6];
        refs(&mut feature, &[900]);
        feature.extend([0; 4]);
        feature.push(1);
        if version == 3 {
            feature.push(0);
        }
        feature.push(1);
        feature.extend([0; 9]);
        refs(&mut feature, &[901]);
        common(&mut feature, "Fillet", suppressed);
        f.put(19, CLASS, version, feature, None);
        let modern = version == 3;
        let mut set = vec![0; if modern { 8 } else { 7 }];
        set.extend([1, 0]);
        if modern {
            set.push(0);
        }
        set.push(u8::from(tangent));
        set.extend([0; 2]);
        reference(&mut set, 901);
        set.extend(vec![0; if modern { 23 } else { 21 }]);
        reference(&mut set, 902);
        set.extend([0; 8]);
        reference(&mut set, 903);
        set.extend([0; 4]);
        f.put(900, SET, if modern { 10 } else { 9 }, set, None);
        f.put(901, CONTAINER, 1, vec![], None);
        for id in [902, 903] {
            f.put(id, "D91D429C-1BB5-4640-B49F-69AF14F463E9", 0, vec![], None);
        }
        f
    }

    #[test]
    fn fillet_sets_preserve_versions_bindings_chain_and_suppression() {
        for version in [2, 3] {
            for tangent in [false, true] {
                for suppressed in [false, true] {
                    let history = authored(version, tangent, suppressed).design().history().unwrap();
                    let feature = history.last().unwrap();
                    assert_eq!(feature.suppressed, suppressed);
                    assert_eq!(
                        feature.kind,
                        HistoryKind::Fillet { sets: vec![FilletSet { container: 901, radius: 902, weight: 903, tangent_chain: tangent }] }
                    );
                }
            }
        }
    }

    #[test]
    fn malformed_fillet_sets_never_drop_modes_or_bindings() {
        for version in [2, 3] {
            for id in [19, 900] {
                for keep in 0..authored(version, false, false).0[&id].2.len() {
                    let mut f = authored(version, false, false);
                    f.0.get_mut(&id).unwrap().2.truncate(keep);
                    assert!(f.design().history().is_err(), "{version}/{id}/{keep}");
                }
            }
            for offset in if version == 3 { [8, 11, 63] } else { [7, 9, 59] } {
                let mut f = authored(version, false, false);
                f.0.get_mut(&900).unwrap().2[offset] = 2;
                assert!(f.design().history().is_err());
            }
            let mut f = authored(version, false, false);
            f.0.get_mut(&902).unwrap().0 = CONTAINER.into();
            assert!(f.design().history().is_err());
        }
    }

    fn recipe() -> Vec<u8> {
        let mut b = vec![0; 6];
        for n in [1, 3, 4] {
            u32(&mut b, n);
        }
        for label in ["19", "42", "1", "2"] {
            u32(&mut b, 1);
            string(&mut b, label);
            u32(&mut b, 0);
            u32(&mut b, 1);
            u64(&mut b, 8001);
        }
        u32(&mut b, 0);
        string(&mut b, "edge_recipe_data");
        u32(&mut b, u32::MAX);
        for n in [-1_i32, 2, 0, -1, 1, -1, 2, -1, 3, 1, -1, 0, -1, 2, -1, 0, 0, -1, 3, 1, -1, 3, -1, 0, -1, 0, 0, -1] {
            u32(&mut b, n as u32);
        }
        b
    }

    #[test]
    fn edge_incidence_accepts_explicit_nonordinal_labels_and_rejects_wrong_ownership() {
        let parse = |b: &[u8]| edge_recipe(&mut Reader { bytes: b, at: 0 });
        let b = recipe();
        assert_eq!(parse(&b).unwrap(), (8001, vec![19, 42, 1, 2], [[1, 0, 2], [1, 3, 0]]));
        for keep in 0..b.len() {
            assert!(parse(&b[..keep]).is_err(), "{keep}");
        }
        let mut wrong = recipe();
        let at = wrong.windows(8).rposition(|b| b == 8001_u64.to_le_bytes()).unwrap();
        wrong[at..at + 8].copy_from_slice(&8002_u64.to_le_bytes());
        assert!(parse(&wrong).is_err());
        let mut wrong = recipe();
        let at = wrong.len() - 9 * 4;
        wrong[at..at + 4].copy_from_slice(&2_u32.to_le_bytes());
        assert!(parse(&wrong).is_err());
    }

    fn authored_edge() -> Fixture {
        let mut f = fixture(false);
        let mut outputs = Vec::new();
        refs(&mut outputs, &[80]);
        f.0.get_mut(&9).unwrap().2.splice(151..155, outputs);
        let mut h = vec![0; 6];
        wide(&mut h, "original-body");
        h.extend([0, 1]);
        reference(&mut h, 81);
        reference(&mut h, 9);
        let mut secondary = vec![0; 4];
        reference(&mut secondary, 4);
        f.put(80, "D26351F0-5940-4D23-AA20-2C35475A6D9E", 2, h, Some(secondary));
        f.put(81, "D3937028-C20C-4E65-B010-94AD418A5C20", 19, vec![], None);
        let mut s = vec![0; 8];
        reference(&mut s, 302);
        u32(&mut s, 1);
        wide(&mut s, "original-selected-body");
        wide(&mut s, "original-selected-owner");
        u32(&mut s, 2);
        u32(&mut s, 0);
        refs(&mut s, &[303]);
        refs(&mut s, &[80]);
        s.extend([0; 56]);
        reference(&mut s, 19);
        f.put(301, super::super::body_projections::SELECTOR, 3, s, None);
        f.put(302, "7ACC2A03-0261-4879-A14A-A93D661A5BDC", 3, recipe(), None);
        f.put(303, "50AEE8B4-9456-4DD7-847D-6F6C909BBB9D", 0, vec![], None);
        f
    }

    #[test]
    fn selected_fillet_edge_uses_its_recipe_and_owned_producer_not_cache_coordinates() {
        let resolve = |f: Fixture, owner| {
            let d = f.design();
            let classes = d.classes.iter().flat_map(|c| c.objects.iter().map(move |id| (*id, c))).collect();
            edge(&Records::new(&d), &classes, 301, owner)
        };
        assert_eq!(
            resolve(authored_edge(), 19).unwrap(),
            Edge { feature: 9, operation: 8001, faces: [19, 42], labels: vec![19, 42, 1, 2], endpoint_cycles: [[1, 0, 2], [1, 3, 0]] }
        );
        assert!(resolve(authored_edge(), 9).is_err());
        for id in [80, 301, 302] {
            for keep in 0..authored_edge().0[&id].2.len() {
                let mut f = authored_edge();
                f.0.get_mut(&id).unwrap().2.truncate(keep);
                assert!(resolve(f, 19).is_err(), "{id}/{keep}");
            }
        }
        let mut f = authored_edge();
        f.0.get_mut(&303).unwrap().0 = CONTAINER.into();
        assert!(resolve(f, 19).is_err());
    }
}
