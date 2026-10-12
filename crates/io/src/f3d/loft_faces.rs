//! Explicit extrusion-cap and side identities observed in local loft controls.
//! Persistent producer/operation references are authoritative; cached geometry
//! is never used to choose a nearby face.
use super::history::{empty, finish, payload, references, require_class, zeros};
use super::*;

const IDENTITY: &str = "7ACC2A03-0261-4879-A14A-A93D661A5BDC";
const HANDLE: &str = "D26351F0-5940-4D23-AA20-2C35475A6D9E";

pub(super) struct Face {
    pub feature: u64,
    pub high: bool,
    pub rectangular: bool,
    pub side: Option<u64>,
    pub contributions: Vec<u64>,
    pub all_operations: Vec<u64>,
}

struct Recipe {
    operations: Vec<u64>,
    all_operations: Vec<u64>,
    high: bool,
    rectangular: bool,
    side: Option<u64>,
}

impl Face {
    pub fn cap(feature: u64, operation: u64, high: bool, rectangular: bool) -> Self {
        Self { feature, high, rectangular, side: None, contributions: vec![operation], all_operations: vec![operation] }
    }
    pub fn validate_profile(&self, profile: &SketchProfile, circles: &BTreeMap<u64, SketchCircle>, lines: &BTreeMap<u64, SketchLine>) -> Result<()> {
        let [loops] = profile.regions.as_slice() else { return Err(invalid("unsupported loft face producer regions")) };
        let [boundary] = loops.as_slice() else { return Err(invalid("unsupported loft face producer holes")) };
        if !boundary.outer || boundary.pieces.iter().any(|piece| *piece != (1, 1)) {
            return Err(invalid("unsupported loft cap boundary pieces"));
        }
        if !self.rectangular {
            let [circle] = boundary.curves.as_slice() else { return Err(invalid("loft face recipe requires a circular producer")) };
            if !circles.get(circle).is_some_and(|c| c.sketch == profile.geometry) {
                return Err(invalid("missing loft cap circle"));
            }
            return Ok(());
        }
        if boundary.curves.len() != 4 {
            return Err(invalid("loft face recipe requires four rectangle sides"));
        }
        use solvecraft_geom::Vec3;
        let mut sides = Vec::new();
        for id in &boundary.curves {
            let line = lines.get(id).filter(|l| l.sketch == profile.geometry).ok_or_else(|| invalid("missing loft rectangle side"))?;
            let [x, y, z] = line.start;
            let [u, v, w] = line.end;
            let (a, b) = (Vec3::new(x, y, z), Vec3::new(u, v, w));
            if z.abs() > 1e-10 || w.abs() > 1e-10 || (b - a).len() < 1e-9 {
                return Err(invalid("degenerate or nonplanar loft rectangle"));
            }
            sides.push((a, b));
        }
        for (i, (a, b)) in sides.iter().enumerate() {
            for vertex in [a, b] {
                let adjacent: Vec<_> =
                    sides.iter().enumerate().filter(|(j, (c, d))| *j != i && ((*vertex - *c).len() < 1e-9 || (*vertex - *d).len() < 1e-9)).collect();
                let [(_, (c, d))] = adjacent.as_slice() else { return Err(invalid("ambiguous loft rectangle incidence")) };
                if (*b - *a).dot(*d - *c).abs() > 1e-9 * (*b - *a).len() * (*d - *c).len() {
                    return Err(invalid("loft cap producer is not a rectangle"));
                }
            }
        }
        Ok(())
    }

    pub fn anchor(
        &self,
        doc: &solvecraft_doc::Document,
        state: &solvecraft_doc::ModelState,
        id: u64,
        contributions: &[(u64, Option<String>)],
    ) -> Result<solvecraft_doc::FaceAnchor> {
        let feature = doc.feature(id).filter(|f| !f.suppressed).ok_or_else(|| invalid("missing loft face producer"))?;
        let [body] = feature.body_names.as_slice() else {
            return Err(invalid("loft face requires one explicitly produced body"));
        };
        let body = state.body(body).ok_or_else(|| invalid("loft face body no longer exists"))?;
        let sources = body.body.face_sources().ok_or_else(|| invalid("loft face has no construction identities"))?;
        let matches = |source: &solvecraft_kernel::FaceSource, (id, curve): &(u64, Option<String>)| match (source, curve) {
            (solvecraft_kernel::FaceSource::CertifiedExtrudeCap { producer, high, .. }, None) => producer == id && *high == self.high,
            (solvecraft_kernel::FaceSource::CertifiedExtrudeSide { producer, curve: current, plane: Some(_), .. }, Some(curve)) => {
                producer == id && current == curve
            }
            _ => false,
        };
        let indices: Vec<_> = sources
            .iter()
            .enumerate()
            .filter_map(|(index, source)| {
                let valid = match (source, contributions) {
                    (Some(source), [only]) => matches(source, only),
                    (Some(solvecraft_kernel::FaceSource::PlanarUnion { sources, .. }), parts) if parts.len() >= 2 => {
                        sources.len() == parts.len() && parts.iter().all(|part| sources.iter().any(|source| matches(source, part)))
                    }
                    _ => false,
                };
                valid.then_some(index)
            })
            .collect();
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

fn cap_recipe(records: &Records<'_>, classes: &BTreeMap<u64, &Class>, id: u64) -> Result<Recipe> {
    require_class(classes, id, IDENTITY, 3)?;
    let mut r = payload(records.primary(id)?, id)?;
    zeros(&mut r, 6)?;
    for value in [1, 3] {
        if r.u32()? != value {
            return Err(invalid("unsupported loft face recipe header"));
        }
    }
    let count = r.u32()?;
    if count != 3 && count != 5 {
        return Err(invalid("unsupported loft face term count"));
    }
    let mut terms = Vec::new();
    let mut all_operations = BTreeSet::new();
    for _ in 0..count {
        let n = r.u32()?;
        if !(1..=64).contains(&n) {
            return Err(invalid("unsupported loft face alternative count"));
        }
        let mut label = None;
        let mut operations = BTreeSet::new();
        for _ in 0..n {
            let current = r.string()?;
            if label.as_ref().is_some_and(|label| label != &current) {
                return Err(invalid("mixed loft face alternative roles"));
            }
            label = Some(current);
            zeros(&mut r, 4)?;
            if r.u32()? != 1 {
                return Err(invalid("unsupported loft face operation count"));
            }
            let operation = r.u64()?;
            if operation == 0 || !operations.insert(operation) {
                return Err(invalid("invalid or duplicate loft face operation"));
            }
            all_operations.insert(operation);
        }
        terms.push((label.ok_or_else(|| invalid("missing loft face label"))?, operations));
    }
    if all_operations.len() > 64 {
        return Err(invalid("too many loft face contributors"));
    }
    let labels: Vec<_> = terms.iter().map(|t| t.0.as_str()).collect();
    let (high, rectangular, side) = match labels.as_slice() {
        ["2", "-1", "3"] => (true, false, None),
        ["1", "-102", "3"] => (false, false, None),
        ["2", "3", "6", "4", "5"] => (true, true, None),
        ["1", "3", "6", "4", "5"] => (false, true, None),
        ["5", "1", "4", "6", "2"] => (false, true, Some(5)),
        _ => return Err(invalid("unsupported loft face topology recipe")),
    };
    let operations = &terms.first().ok_or_else(|| invalid("missing face terms"))?.1;
    if !rectangular && (operations.len() != 1 || terms.iter().any(|t| &t.1 != operations)) {
        return Err(invalid("unsupported composite circular loft face"));
    }
    zeros(&mut r, 4)?;
    if r.string()? != "bounded_face_recipe_data" {
        return Err(invalid("missing bounded loft face recipe"));
    }
    zeros(&mut r, 4)?;
    // Exact bounded incidence is required in addition to construction labels.
    let tree = if side.is_some() {
        side_tree()
    } else if rectangular {
        rectangle_tree(high)
    } else {
        let mut tree = vec![-1_i32, 1, 1, -1, 2, 2, -1, 0, -1, 1, -1, 2, 2, -1, 0, -1, 1, 1, -1, 0, 0, -1, 1, 0];
        tree.extend(if high { [2, 2, 2, 2, 1, 0, 1, -1] } else { [2, 1, 1, 1, 2, 1, 2, -1] });
        tree
    };
    for value in tree {
        if r.u32()? != value as u32 {
            return Err(invalid("unsupported loft face incidence tree"));
        }
    }
    finish(&r)?;
    Ok(Recipe { operations: operations.iter().copied().collect(), all_operations: all_operations.into_iter().collect(), high, rectangular, side })
}

/// Four edges of the observed side-5 rectangle. Terms identify the selected
/// side, lower cap, previous side, next side, and upper cap respectively.
fn side_tree() -> Vec<i32> {
    let mut tree = vec![-1, 4];
    for [a, b, c, d, e, f, g, h] in [[0, 1, 0, 2, 1, 3, 0, 1], [0, 3, 3, 0, 1, 3, 4, 0], [0, 4, 0, 4, 2, 3, 4, 0], [2, 0, 0, 2, 1, 0, 4, 2]] {
        tree.extend([-1, -1, 2, a, -1, b, -1, 2, -1, 3, c, -1, d, -1, e, -1, 0, 0, -1, 3, f, -1, g, -1, h, -1, 0, 0, -1]);
    }
    tree
}

/// Four cyclic cap edges, each incident to the cap, a side and two vertices.
/// Term indices follow [cap, side3, side6, side4, side5]. Bottom caps traverse
/// the boundary in the opposite direction. Vertex incidence changes at the seam.
fn rectangle_tree(high: bool) -> Vec<i32> {
    let index = |side| match side {
        3 => 1,
        4 => 3,
        5 => 4,
        6 => 2,
        _ => 0,
    };
    let mut tree = vec![-1, 4];
    for side in if high { [3, 4, 5, 6] } else { [3, 6, 5, 4] } {
        let this = index(side);
        let previous = index(if side == 3 { 6 } else { side - 1 });
        let next = index(if side == 6 { 3 } else { side + 1 });
        let (a, b) = if side == 3 { (this, 0) } else { (0, previous) };
        let (a, b) = if high { (a, b) } else { (b, a) };
        // The last side's next vertex uses the first side; other next vertices
        // use the current side and cap, in the cap's orientation.
        let (c, d) = if side == 6 {
            if high { (next, 0) } else { (0, next) }
        } else if high {
            (0, this)
        } else {
            (this, 0)
        };
        tree.extend([
            -1,
            -1,
            2,
            this,
            -1,
            0,
            -1,
            2,
            -1,
            3,
            if side == 3 { previous } else { this },
            -1,
            a,
            -1,
            b,
            -1,
            0,
            0,
            -1,
            3,
            if side == 6 { this } else { next },
            -1,
            c,
            -1,
            d,
            -1,
            0,
            0,
            -1,
        ]);
    }
    tree
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
    let recipe = cap_recipe(records, classes, r.reference()?)?;
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
    Ok(Face {
        feature: producer(records, classes, handle)?,
        high: recipe.high,
        rectangular: recipe.rectangular,
        side: recipe.side,
        contributions: recipe.operations,
        all_operations: recipe.all_operations,
    })
}

pub(super) fn target(records: &Records<'_>, classes: &BTreeMap<u64, &Class>, container: u64, owner: u64) -> Result<(u64, u64, bool)> {
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
    if identity.string()?.parse::<u64>().ok() != Some(operation) {
        return Err(invalid("loft target recipe disagrees with operation"));
    }
    // Observed producer-shape markers; reconstruction also validates the
    // explicitly owned profile, so these are not accepted as generic body IDs.
    let rectangular = match identity.u32()? {
        4 => false,
        7 => true,
        _ => return Err(invalid("unsupported loft target shape marker")),
    };
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
    Ok((producer(records, classes, handle)?, operation, rectangular))
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

    fn authored_rectangle(high: bool) -> Fixture {
        let mut f = authored(high);
        let mut b = vec![0; 6];
        for n in [1, 3, 5] {
            u32(&mut b, n);
        }
        for label in [if high { "2" } else { "1" }, "3", "6", "4", "5"] {
            u32(&mut b, 1);
            string(&mut b, label);
            u32(&mut b, 0);
            u32(&mut b, 1);
            u64(&mut b, 8001);
        }
        u32(&mut b, 0);
        string(&mut b, "bounded_face_recipe_data");
        u32(&mut b, 0);
        u32(&mut b, u32::MAX);
        u32(&mut b, 4);
        // Field-authored edge/corner incidence, independent of rectangle_tree.
        let edges = if high {
            [[1, 2, 1, 0, 3, 0, 1], [3, 3, 0, 1, 4, 0, 3], [4, 4, 0, 3, 2, 0, 4], [2, 2, 0, 4, 2, 1, 0]]
        } else {
            [[1, 2, 0, 1, 3, 1, 0], [2, 2, 4, 0, 2, 0, 1], [4, 4, 3, 0, 2, 4, 0], [3, 3, 1, 0, 4, 3, 0]]
        };
        for [side, a, b0, c, d, e, g] in edges {
            for n in [-1_i32, -1, 2, side, -1, 0, -1, 2, -1, 3, a, -1, b0, -1, c, -1, 0, 0, -1, 3, d, -1, e, -1, g, -1, 0, 0, -1] {
                u32(&mut b, n as u32);
            }
        }
        f.put(181, IDENTITY, 3, b, None);
        f.0.get_mut(&191).unwrap().2[30..34].copy_from_slice(&7_u32.to_le_bytes());
        f
    }

    fn authored_joined_side() -> Fixture {
        let mut f = authored(true);
        let mut b = vec![0; 6];
        for n in [1, 3, 5] {
            u32(&mut b, n);
        }
        for (label, operations) in [("5", &[8001, 9001][..]), ("1", &[8001, 9001]), ("4", &[8001, 9001]), ("6", &[8001, 9001]), ("2", &[9001])] {
            u32(&mut b, operations.len() as u32);
            for operation in operations {
                string(&mut b, label);
                u32(&mut b, 0);
                u32(&mut b, 1);
                u64(&mut b, *operation);
            }
        }
        u32(&mut b, 0);
        string(&mut b, "bounded_face_recipe_data");
        u32(&mut b, 0);
        u32(&mut b, u32::MAX);
        u32(&mut b, 4);
        // Each edge joins two incident surfaces and two three-surface corners.
        let edges =
            [([0, 1], [[0, 2, 1], [3, 0, 1]]), ([0, 3], [[3, 0, 1], [3, 4, 0]]), ([0, 4], [[0, 4, 2], [3, 4, 0]]), ([2, 0], [[0, 2, 1], [0, 4, 2]])];
        for ([a, b0], vertices) in edges {
            for n in [-1_i32, -1, 2, a, -1, b0, -1, 2] {
                u32(&mut b, n as u32);
            }
            for [a, b0, c] in vertices {
                for n in [-1_i32, 3, a, -1, b0, -1, c, -1, 0, 0] {
                    u32(&mut b, n as u32);
                }
            }
            u32(&mut b, u32::MAX);
        }
        f.put(181, IDENTITY, 3, b, None);
        f
    }

    #[test]
    fn joined_side_recipe_keeps_body_creator_separate_from_contributors() {
        let d = authored_joined_side().design();
        let classes = d.classes.iter().flat_map(|c| c.objects.iter().map(move |id| (*id, c))).collect();
        let selected = face(&Records::new(&d), &classes, 180, 19).unwrap();
        assert_eq!(selected.feature, 9);
        assert_eq!(selected.side, Some(5));
        assert_eq!(selected.contributions, [8001, 9001]);
        assert_eq!(selected.all_operations, [8001, 9001]);
        assert!(selected.rectangular);
    }

    #[test]
    fn composite_side_recipe_rejects_ambiguous_roles_operations_and_incidence() {
        for keep in 0..authored_joined_side().0[&181].2.len() {
            let mut f = authored_joined_side();
            f.0.get_mut(&181).unwrap().2.truncate(keep);
            assert!(decode(f, false).is_err(), "prefix {keep}");
        }
        for (at, bytes) in [(18, 65_u32.to_le_bytes().to_vec()), (47, vec![b'6']), (56, 8001_u64.to_le_bytes().to_vec())] {
            let mut f = authored_joined_side();
            f.0.get_mut(&181).unwrap().2[at..at + bytes.len()].copy_from_slice(&bytes);
            assert!(decode(f, false).is_err(), "offset {at}");
        }
        let mut f = authored_joined_side();
        *f.0.get_mut(&181).unwrap().2.last_mut().unwrap() = 0;
        assert!(decode(f, false).is_err());
    }

    #[test]
    fn rectangular_cap_incidence_and_target_markers_are_not_circular_aliases() {
        for high in [false, true] {
            let d = authored_rectangle(high).design();
            let classes = d.classes.iter().flat_map(|c| c.objects.iter().map(move |id| (*id, c))).collect();
            let records = Records::new(&d);
            let selected = face(&records, &classes, 180, 19).unwrap();
            assert!(selected.rectangular);
            assert_eq!((selected.feature, selected.contributions[0], selected.high), (9, 8001, high));
            assert_eq!(target(&records, &classes, 192, 19).unwrap(), (9, 8001, true));
            for keep in 0..authored_rectangle(high).0[&181].2.len() {
                let mut f = authored_rectangle(high);
                f.0.get_mut(&181).unwrap().2.truncate(keep);
                assert!(decode(f, false).is_err(), "{high}/{keep}");
            }
            let mut f = authored_rectangle(high);
            let bytes = &mut f.0.get_mut(&181).unwrap().2;
            let end = bytes.len();
            bytes[end - 1] ^= 1;
            assert!(decode(f, false).is_err());
            let mut f = authored_rectangle(high);
            f.0.get_mut(&191).unwrap().2[30..34].copy_from_slice(&6_u32.to_le_bytes());
            assert!(decode(f, true).is_err());
        }
    }

    #[test]
    fn rectangular_cap_profile_requires_owned_whole_connected_orthogonal_sides() {
        let design = super::super::polygon_tests::rectangle().design();
        let mut profiles = design.sketch_profiles().unwrap();
        let profile = &mut profiles[0];
        let mut lines: BTreeMap<_, _> = design.sketch_lines().unwrap().into_iter().map(|l| (l.id, l)).collect();
        let face = Face::cap(9, 8001, true, true);
        face.validate_profile(profile, &BTreeMap::new(), &lines).unwrap();
        profile.regions[0][0].curves.reverse();
        face.validate_profile(profile, &BTreeMap::new(), &lines).unwrap();
        profile.regions[0][0].pieces[0] = (1, 2);
        assert!(face.validate_profile(profile, &BTreeMap::new(), &lines).is_err());
        profile.regions[0][0].pieces[0] = (1, 1);
        let line = *lines.keys().next().unwrap();
        lines.get_mut(&line).unwrap().sketch += 1;
        assert!(face.validate_profile(profile, &BTreeMap::new(), &lines).is_err());
        lines.get_mut(&line).unwrap().sketch -= 1;
        lines.get_mut(&line).unwrap().end[1] += 0.5;
        assert!(face.validate_profile(profile, &BTreeMap::new(), &lines).is_err());
    }

    fn decode(f: Fixture, body: bool) -> Result<(u64, u64, bool)> {
        let d = f.design();
        let classes = d.classes.iter().flat_map(|c| c.objects.iter().map(move |id| (*id, c))).collect();
        let records = Records::new(&d);
        if body {
            let (producer, operation, _) = target(&records, &classes, 192, 19)?;
            Ok((producer, operation, false))
        } else {
            let face = face(&records, &classes, 180, 19)?;
            Ok((face.feature, face.contributions[0], face.high))
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
