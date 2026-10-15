//! Conservative artifact comparison, separate from toolpath generation.
use crate::spatial::Index;
use crate::{
    verify::{Report, Status},
    *,
};
use serde::{Deserialize, Serialize};
use solvecraft_geom::Vec3;

#[derive(Debug, Serialize, Deserialize)]
pub struct Comparison {
    pub status: Status,
    pub maximum_possible_error: f64,
    pub maximum_definite_error: f64,
    pub checked_regions: usize,
    pub detail: String,
}
fn gaps(a: [f64; 2], b: [f64; 2]) -> (f64, f64) {
    ((a[0] - b[1]).max(b[0] - a[1]).max(0.0), (a[0] - b[1]).abs().max((a[1] - b[0]).abs()))
}
pub fn stock_reports(a: &Report, b: &Report, tolerance: f64) -> Result<Comparison> {
    let mut forward = compare_covered(a, b, tolerance)?;
    let reverse = compare_covered(b, a, tolerance)?;
    forward.status = match (forward.status, reverse.status) {
        (Status::Fail, _) | (_, Status::Fail) => Status::Fail,
        (Status::Inconclusive, _) | (_, Status::Inconclusive) => Status::Inconclusive,
        _ => Status::Pass,
    };
    forward.maximum_possible_error = forward.maximum_possible_error.max(reverse.maximum_possible_error);
    forward.maximum_definite_error = forward.maximum_definite_error.max(reverse.maximum_definite_error);
    forward.checked_regions += reverse.checked_regions;
    Ok(forward)
}
fn compare_covered(a: &Report, b: &Report, tolerance: f64) -> Result<Comparison> {
    if !tolerance.is_finite() || tolerance <= 0.0 {
        return invalid("comparison tolerance must be positive");
    }
    let mut result = Comparison {
        status: Status::Pass,
        maximum_possible_error: 0.0,
        maximum_definite_error: 0.0,
        checked_regions: 0,
        detail: "Common-coordinate height intervals; no geometry alignment or rescaling".into(),
    };
    if a.status != Status::Pass || b.status != Status::Pass || !a.stock_complete || !b.stock_complete || a.cells.is_empty() || b.cells.is_empty() {
        result.status = Status::Inconclusive;
    }
    let index = Index::new(b.cells.iter().enumerate().map(|(id, c)| (id, c.min, c.max)).collect());
    let mut work = 0usize;
    for x in &a.cells {
        let mut covered = 0.0;
        let mut candidates = Vec::new();
        index.query(x.min, x.max, &mut candidates);
        for id in candidates {
            let y = &b.cells[id];
            work += 1;
            if work > 10_000_000 {
                result.status = Status::Inconclusive;
                result.detail = "stock comparison work budget exhausted".into();
                return Ok(result);
            }
            let w = x.max[0].min(y.max[0]) - x.min[0].max(y.min[0]);
            let h = x.max[1].min(y.max[1]) - x.min[1].max(y.min[1]);
            if w <= 0.0 || h <= 0.0 {
                continue;
            }
            covered += w * h;
            result.checked_regions += 1;
            let (definite, possible) = gaps([x.low, x.high], [y.low, y.high]);
            result.maximum_definite_error = result.maximum_definite_error.max(definite);
            result.maximum_possible_error = result.maximum_possible_error.max(possible);
        }
        if covered < (x.max[0] - x.min[0]) * (x.max[1] - x.min[1]) * (1.0 - 1e-8) {
            result.status = Status::Inconclusive;
        }
    }
    if result.maximum_definite_error > tolerance {
        result.status = Status::Fail;
    } else if result.maximum_possible_error > tolerance {
        result.status = Status::Inconclusive;
    }
    Ok(result)
}
pub fn stock_mesh(report: &Report, mesh: &[[Vec3; 3]], bottom: f64, tolerance: f64) -> Result<Comparison> {
    if report.cells.is_empty() || mesh.is_empty() || mesh.len() > 500_000 {
        return invalid("reference stock mesh empty or too large");
    }
    if !tolerance.is_finite() || tolerance <= 0.0 {
        return invalid("comparison tolerance must be positive");
    }
    let mut r = Comparison {
        status: if report.status == Status::Pass && report.stock_complete { Status::Pass } else { Status::Inconclusive },
        maximum_possible_error: 0.0,
        maximum_definite_error: 0.0,
        checked_regions: 0,
        detail: "Reference stock STL in setup millimetres; conservative upper-surface intervals".into(),
    };
    let index = Index::new(
        mesh.iter()
            .enumerate()
            .map(|(id, t)| {
                (
                    id,
                    [t.iter().map(|v| v.x).fold(f64::INFINITY, f64::min), t.iter().map(|v| v.y).fold(f64::INFINITY, f64::min)],
                    [t.iter().map(|v| v.x).fold(f64::NEG_INFINITY, f64::max), t.iter().map(|v| v.y).fold(f64::NEG_INFINITY, f64::max)],
                )
            })
            .collect(),
    );
    let mut work = 0;
    for cell in &report.cells {
        let p = [(cell.min[0] + cell.max[0]) / 2.0, (cell.min[1] + cell.max[1]) / 2.0];
        let half = (cell.max[0] - cell.min[0]).max(cell.max[1] - cell.min[1]) / 2.0;
        let mut candidates = Vec::new();
        index.query([p[0] - half, p[1] - half], [p[0] + half, p[1] + half], &mut candidates);
        let local: Vec<_> = candidates.iter().map(|id| mesh[*id]).collect();
        let (lo, hi, uncertain) = crate::verify::target_height(p, half, &local, bottom, &mut work, 10_000_000);
        if work > 10_000_000 {
            r.status = Status::Inconclusive;
            r.detail = "STL comparison budget exhausted".into();
            return Ok(r);
        }
        let (definite, possible) = gaps([cell.low, cell.high], [lo, hi]);
        r.maximum_definite_error = r.maximum_definite_error.max(definite);
        r.maximum_possible_error = r.maximum_possible_error.max(possible);
        r.checked_regions += 1;
        if uncertain {
            r.status = Status::Inconclusive;
        }
    }
    if r.maximum_definite_error > tolerance {
        r.status = Status::Fail;
    } else if r.maximum_possible_error > tolerance {
        r.status = Status::Inconclusive;
    }
    Ok(r)
}
/// Strict STL decoding: malformed triangles are rejected, never silently dropped.
pub fn read_stl(bytes: &[u8]) -> Result<Vec<[Vec3; 3]>> {
    if bytes.len() > 128 * 1024 * 1024 {
        return invalid("STL exceeds 128 MiB");
    }
    let mut triangles = Vec::new();
    let count = bytes.get(80..84).and_then(|v| <[u8; 4]>::try_from(v).ok()).map(u32::from_le_bytes).map(|n| n as usize);
    if let Some(count) = count.filter(|n| 84usize.checked_add(n.saturating_mul(50)) == Some(bytes.len())) {
        if count > 500_000 {
            return invalid("STL triangle limit");
        }
        for chunk in bytes.get(84..).unwrap_or_default().as_chunks::<50>().0 {
            let mut pts = [Vec3::ZERO; 3];
            for (k, p) in pts.iter_mut().enumerate() {
                let mut xyz = [0.0; 3];
                for (j, v) in xyz.iter_mut().enumerate() {
                    let start = 12 + k * 12 + j * 4;
                    let b =
                        chunk.get(start..start + 4).and_then(|b| <[u8; 4]>::try_from(b).ok()).ok_or_else(|| Error("truncated STL vertex".into()))?;
                    *v = f32::from_le_bytes(b) as f64;
                }
                *p = Vec3::new(xyz[0], xyz[1], xyz[2]);
                crate::model::point(*p)?;
            }
            triangles.push(pts);
        }
    } else {
        let text = std::str::from_utf8(bytes).map_err(|_| Error("invalid STL encoding/length".into()))?;
        let mut lines = text.lines().map(|line| line.split_whitespace().collect::<Vec<_>>()).filter(|line| !line.is_empty());
        if lines.next().and_then(|line| line.first().copied()) != Some("solid") {
            return invalid("invalid ASCII STL header");
        }
        loop {
            let header = lines.next().ok_or_else(|| Error("missing STL terminator".into()))?;
            if header.first() == Some(&"endsolid") {
                if lines.next().is_some() {
                    return invalid("data after STL terminator");
                }
                break;
            }
            if header.len() != 5 || header[..2] != ["facet", "normal"] || header[2..].iter().any(|v| !v.parse::<f64>().is_ok_and(f64::is_finite)) {
                return invalid("invalid STL facet header");
            }
            if lines.next().as_deref() != Some(&["outer", "loop"]) {
                return invalid("missing STL outer loop");
            }
            let mut points = [Vec3::ZERO; 3];
            for point in &mut points {
                let words = lines.next().ok_or_else(|| Error("incomplete STL triangle".into()))?;
                if words.len() != 4 || words[0] != "vertex" {
                    return invalid("invalid STL vertex");
                }
                let mut xyz = [0.0; 3];
                for (i, word) in words[1..].iter().enumerate() {
                    xyz[i] = word.parse::<f64>().map_err(|_| Error("invalid STL number".into()))?;
                }
                *point = Vec3::new(xyz[0], xyz[1], xyz[2]);
                crate::model::point(*point)?;
            }
            if lines.next().as_deref() != Some(&["endloop"]) || lines.next().as_deref() != Some(&["endfacet"]) {
                return invalid("incomplete STL facet");
            }
            triangles.push(points);
            if triangles.len() > 500_000 {
                return invalid("STL triangle limit");
            }
        }
    }
    if triangles.is_empty() {
        return invalid("empty STL");
    }
    Ok(triangles)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn invalid_stl_is_not_partially_accepted() {
        assert!(read_stl(b"solid x\nvertex NaN 0 0\nvertex 1 0 0\nvertex 0 1 0\nendsolid").is_err());
        assert!(read_stl(&[0; 84]).is_err());
    }
    #[test]
    fn interval_difference_catches_local_gouge() {
        assert!((gaps([4.0, 4.001], [5.0, 5.001]).0 - 0.999).abs() < 1e-10);
        assert!(gaps([4.99, 5.01], [5.0, 5.0]).1 > 0.009);
    }
    #[test]
    fn incomplete_coverage_is_symmetric_and_stl_is_bounded() {
        let input = corpus::fixture("face").unwrap();
        let path = generate::generate(input.clone()).unwrap();
        let parsed = nc::round_trip(&path, &input.setup, &input.tools).unwrap();
        let full = verify::simulate(&input, &parsed, Default::default()).unwrap();
        let mut partial = full.clone();
        partial.cells.truncate(1);
        assert_eq!(stock_reports(&partial, &full, TOLERANCE).unwrap().status, Status::Inconclusive);
        assert_eq!(stock_reports(&full, &partial, TOLERANCE).unwrap().status, Status::Inconclusive);
        let stl = verify::stock_stl(&full, 0.0).unwrap();
        let mesh = read_stl(stl.as_bytes()).unwrap();
        assert_eq!(mesh.len(), 12); // exact coplanar merge, conforming boundary
        let volume: f64 = mesh.iter().map(|t| t[0].dot(t[1].cross(t[2])) / 6.0).sum();
        assert!((volume - 2000.0).abs() < 1e-9);
        assert!(read_stl(stl.replace("endfacet", "unknown").as_bytes()).is_err());
    }
}
