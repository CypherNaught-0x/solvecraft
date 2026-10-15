//! Independent bounded stock verifier. Adaptive cells carry height intervals;
//! unresolved intervals and exhausted work budgets never become a pass.
use crate::{
    nc::{Parsed, Segment},
    *,
};
use serde::{Deserialize, Serialize};
use solvecraft_geom::Vec3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Pass,
    Fail,
    Inconclusive,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Finding {
    pub kind: String,
    pub line: Option<usize>,
    pub at: Vec3,
    pub detail: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cell {
    pub min: [f64; 2],
    pub max: [f64; 2],
    pub low: f64,
    pub high: f64,
    pub target_low: f64,
    pub target_high: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Report {
    pub version: String,
    pub fingerprint: String,
    pub status: Status,
    /// Separate from path clearance. Unsupported finished surfaces stay inconclusive.
    pub finished_dimensions: Status,
    pub post_version: String,
    pub tolerance: f64,
    /// Requested spatial refinement budget, not a claim about height uncertainty.
    pub uncertainty: f64,
    pub maximum_height_interval: f64,
    pub cells: Vec<Cell>,
    pub findings: Vec<Finding>,
    pub cutting_distance: f64,
    pub rapid_distance: f64,
    pub cutting_minutes: f64,
    pub remaining_volume_low: f64,
    pub remaining_volume_high: f64,
    pub work: usize,
}
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub cells: usize,
    pub work: usize,
    pub uncertainty: f64,
}
impl Default for Limits {
    fn default() -> Self {
        Self { cells: 250_000, work: 30_000_000, uncertainty: VERIFY_ERROR }
    }
}

pub fn simulate(input: &JobInput, nc: &Parsed, limits: Limits) -> Result<Report> {
    simulate_cancellable(input, nc, limits, || false)
}
pub fn simulate_cancellable(input: &JobInput, nc: &Parsed, limits: Limits, cancelled: impl Fn() -> bool) -> Result<Report> {
    use std::future::Future;
    let mut future = std::pin::pin!(simulate_cooperative(input, nc, limits, cancelled, || std::future::ready(())));
    match future.as_mut().poll(&mut std::task::Context::from_waker(std::task::Waker::noop())) {
        std::task::Poll::Ready(result) => result,
        std::task::Poll::Pending => invalid("synchronous verifier unexpectedly yielded"),
    }
}
/// Portable host-driven verification. A host supplies a yield future to bound a poll's work.
/// This contains no executor, OS thread, browser or generator dependency.
pub async fn simulate_cooperative<F: std::future::Future<Output = ()>>(
    input: &JobInput,
    nc: &Parsed,
    limits: Limits,
    cancelled: impl Fn() -> bool,
    mut yield_control: impl FnMut() -> F,
) -> Result<Report> {
    input.validate()?;
    if limits.cells == 0
        || limits.cells > 1_000_000
        || limits.work == 0
        || limits.work > 100_000_000
        || !limits.uncertainty.is_finite()
        || limits.uncertainty <= 0.0
        || limits.uncertainty > VERIFY_ERROR
    {
        return invalid("invalid verifier limits");
    }
    if nc.work_offset != input.setup.work_offset {
        return invalid("NC/setup work-offset mismatch");
    }
    if input.geometry.target.is_empty() {
        return invalid("no target geometry to verify");
    }
    if input.geometry.target.len() + input.geometry.fixtures.len() > 200_000 {
        return invalid("verification mesh limit");
    }
    for t in input.geometry.target.iter().chain(&input.geometry.fixtures) {
        for p in t {
            crate::model::point(*p)?;
        }
    }
    let segments = nc::segments(nc, &input.setup)?;
    let tol = input.operations.iter().map(|o| o.tolerance).fold(TOLERANCE, f64::min);
    let stock = input.setup.stock;
    let mut r = Report {
        version: VERSION.into(),
        fingerprint: input.fingerprint.clone(),
        status: Status::Pass,
        finished_dimensions: Status::Inconclusive,
        post_version: "reference-three-axis/1".into(),
        tolerance: tol,
        uncertainty: limits.uncertainty,
        maximum_height_interval: 0.0,
        cells: Vec::new(),
        findings: Vec::new(),
        cutting_distance: 0.0,
        rapid_distance: 0.0,
        cutting_minutes: 0.0,
        remaining_volume_low: 0.0,
        remaining_volume_high: 0.0,
        work: 0,
    };
    for s in &segments {
        let d = s.from.dist(s.to);
        if s.rapid {
            r.rapid_distance += d;
        } else {
            r.cutting_distance += d;
            r.cutting_minutes += d / s.feed;
        }
    }
    crate::collision::fixtures(input, &segments, limits, &cancelled, &mut yield_control, &mut r).await?;
    let mut pending = vec![([stock.min.x, stock.min.y], [stock.max.x, stock.max.y], 0u8)];
    let mut next_yield = 0;
    while let Some((min, max, depth)) = pending.pop() {
        if r.work >= next_yield {
            next_yield = r.work.saturating_add(4096);
            yield_control().await;
        }
        if cancelled() {
            return invalid("simulation cancelled");
        }
        if r.work > limits.work || r.cells.len() + pending.len() >= limits.cells {
            r.status = if r.status == Status::Fail { Status::Fail } else { Status::Inconclusive };
            r.findings.push(Finding {
                kind: "budget_exhausted".into(),
                line: None,
                at: stock.min,
                detail: "Adaptive verification incomplete; increase resource budget or simplify the job".into(),
            });
            break;
        }
        let center = [(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0];
        let half = ((max[0] - min[0]).hypot(max[1] - min[1])) / 2.0;
        let mut low = stock.max.z;
        let mut high = stock.max.z;
        let mut collision = None;
        let mut removal_line = None;
        for (segment_index, seg) in segments.iter().enumerate() {
            r.work += 1;
            if segment_index % 4096 == 4095 {
                yield_control().await;
                if cancelled() {
                    return invalid("simulation cancelled");
                }
            }
            if r.work > limits.work {
                break;
            }
            let Some(tool) = input.tools.iter().find(|t| t.id == seg.tool) else {
                continue;
            };
            let radius = tool.diameter / 2.0;
            let (distance, zlo, zhi) = sweep(center, half, seg, radius);
            let cone_low = if tool.kind == ToolKind::TwistDrill { (distance - half).max(0.0) / (tool.tip_angle / 2.0).tan() } else { 0.0 };
            let cone_high = if tool.kind == ToolKind::TwistDrill { (distance + half) / (tool.tip_angle / 2.0).tan() } else { 0.0 };
            let overlaps = distance <= radius + half + POST_ERROR;
            // A vertical withdrawal starts at the endpoint of the preceding feed
            // with the same tool; this exact geometric fact needs no raster estimate.
            let withdrawal = segment_index.checked_sub(1).and_then(|i| segments.get(i)).is_some_and(|previous| {
                !previous.rapid
                    && previous.tool == seg.tool
                    && previous.to == seg.from
                    && seg.from.x == seg.to.x
                    && seg.from.y == seg.to.y
                    && seg.to.z >= seg.from.z
            });
            if seg.rapid && overlaps && !withdrawal {
                if zhi + cone_high < low - tol && distance + half <= radius {
                    collision = Some(("rapid_through_stock", seg.line));
                } else if zlo + cone_low < high - tol {
                    collision = Some(("rapid_clearance_unresolved", seg.line));
                }
            } else if !seg.rapid && overlaps {
                low = low.min(zlo + cone_low).max(stock.min.z);
                if distance + half <= radius {
                    let next = high.min(zhi + cone_high).max(stock.min.z);
                    if next < high {
                        removal_line = Some(seg.line);
                    }
                    high = next;
                }
            }
            // Shank and holder sweeps against remaining stock, separate from cutter removal.
            for (rad, lift, label) in [
                (tool.shank_diameter / 2.0, tool.cutting_length, "shank_collision"),
                (tool.holder_diameter / 2.0, tool.overall_length, "holder_collision"),
            ] {
                let (d, z, _) = sweep(center, half, seg, rad);
                if d + half < rad && z + lift < low - tol {
                    collision = Some((label, seg.line));
                } else if d < rad + half && z + lift < high - tol {
                    collision = Some(("holder_clearance_unresolved", seg.line));
                }
            }
        }
        let (target_low, target_high, uncertain_target) =
            target_height(center, (max[0] - min[0]).max(max[1] - min[1]) / 2.0, &input.geometry.target, stock.min.z, &mut r.work, limits.work);
        let gouge = target_low - high > tol;
        let rest = low - target_high > tol;
        // Target boundary cells refine spatially; interior planar cells can certify early.
        let unresolved =
            (high - low > tol / 2.0 || (uncertain_target && low < target_high - tol) || collision.is_some()) && half > limits.uncertainty;
        if unresolved && depth < 24 && r.work <= limits.work {
            pending.extend([
                (min, center, depth + 1),
                ([center[0], min[1]], [max[0], center[1]], depth + 1),
                ([min[0], center[1]], [center[0], max[1]], depth + 1),
                (center, max, depth + 1),
            ]);
            continue;
        }
        let at = Vec3::new(center[0], center[1], low);
        if let Some((kind, line)) = collision {
            if !kind.ends_with("unresolved") {
                r.status = Status::Fail;
            } else if r.status == Status::Pass {
                r.status = Status::Inconclusive;
            }
            if r.findings.len() < 200 {
                r.findings.push(Finding {
                    kind: kind.into(),
                    line: Some(line),
                    at,
                    detail: "Swept cutter or non-cutting tool envelope intersects remaining stock".into(),
                });
            }
        }
        if gouge {
            r.status = Status::Fail;
            if r.findings.len() < 200 {
                r.findings.push(Finding {
                    kind: "gouge".into(),
                    line: removal_line,
                    at,
                    detail: format!("Protected target removed by at least {:.6} mm", target_low - high),
                });
            }
        }
        // Rest is reported separately: intermediate roughing is not required to finish a part.
        if rest && !r.findings.iter().any(|f| f.kind == "remaining_material") {
            r.findings.push(Finding {
                kind: "remaining_material".into(),
                line: None,
                at,
                detail: format!("At least {:.6} mm above target; may be intentional outside selected finished regions", low - target_high),
            });
        }
        if low < target_high - tol && !gouge && r.status == Status::Pass {
            r.status = Status::Inconclusive;
        }
        let area = (max[0] - min[0]) * (max[1] - min[1]);
        r.remaining_volume_low += area * (low - stock.min.z);
        r.remaining_volume_high += area * (high - stock.min.z);
        r.maximum_height_interval = r.maximum_height_interval.max(high - low);
        r.cells.push(Cell { min, max, low, high, target_low, target_high });
    }
    if r.work > limits.work && r.status == Status::Pass {
        r.status = Status::Inconclusive;
    }
    // Facing has an independently specified finished plane over the entire stock.
    // Wall and hole dimensional checks remain explicitly unimplemented, rather
    // than inheriting a pass from the absence of detected gouges.
    if input.operations.iter().all(|o| matches!(o.geometry, Strategy::Face)) && r.status == Status::Pass {
        let plane = input.operations.iter().map(|o| o.bottom + o.allowance).fold(stock.max.z, f64::min);
        r.finished_dimensions = Status::Pass;
        for c in &r.cells {
            let definite = (c.low - plane).max(plane - c.high).max(0.0);
            let possible = (c.low - plane).abs().max((c.high - plane).abs());
            if definite > tol {
                r.finished_dimensions = Status::Fail;
                r.findings.push(Finding {
                    kind: "finished_plane_error".into(),
                    line: None,
                    at: Vec3::new(c.min[0], c.min[1], c.low),
                    detail: format!("Finished plane differs by at least {definite:.6} mm"),
                });
                break;
            } else if possible > tol {
                r.finished_dimensions = Status::Inconclusive;
            }
        }
    }
    Ok(r)
}

/// Distance to XY sweep and conservative height range over the part of a segment
/// intersecting a disk expanded by the cell radius.
fn sweep(p: [f64; 2], half: f64, s: &Segment, radius: f64) -> (f64, f64, f64) {
    let dx = s.to.x - s.from.x;
    let dy = s.to.y - s.from.y;
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 1e-20 { ((p[0] - s.from.x) * dx + (p[1] - s.from.y) * dy) / l2 } else { 0.0 };
    let c = t.clamp(0.0, 1.0);
    let d = (p[0] - s.from.x - c * dx).hypot(p[1] - s.from.y - c * dy);
    let minimum = |r: f64| {
        if l2 <= 1e-20 {
            return s.from.z.min(s.to.z);
        }
        let perpendicular = ((p[0] - s.from.x).powi(2) + (p[1] - s.from.y).powi(2) - t * t * l2).max(0.0);
        let dt = ((r * r - perpendicular).max(0.0) / l2).sqrt();
        let z0 = s.from.z + (s.to.z - s.from.z) * (t - dt).clamp(0.0, 1.0);
        let z1 = s.from.z + (s.to.z - s.from.z) * (t + dt).clamp(0.0, 1.0);
        z0.min(z1)
    };
    (d, minimum(radius + half), minimum((radius - half).max(0.0)))
}
pub(crate) fn target_height(p: [f64; 2], half: f64, tris: &[[Vec3; 3]], bottom: f64, work: &mut usize, budget: usize) -> (f64, f64, bool) {
    let mut lo = f64::INFINITY;
    let mut hi = bottom;
    let mut area = 0.0;
    for tri in tris {
        *work += 1;
        if *work > budget {
            return (bottom, hi, true);
        }
        let [a, b, c] = *tri;
        // Positive-Z faces form the top envelope of supported height-field solids.
        if (b - a).cross(c - a).z <= 1e-12 {
            continue;
        }
        let mut poly = tri.to_vec();
        for (axis, bound, greater) in [(0, p[0] - half, true), (0, p[0] + half, false), (1, p[1] - half, true), (1, p[1] + half, false)] {
            let old = std::mem::take(&mut poly);
            if old.is_empty() {
                break;
            }
            let value = |v: Vec3| if axis == 0 { v.x } else { v.y };
            let inside = |v: Vec3| if greater { value(v) >= bound } else { value(v) <= bound };
            for (a, b) in old.iter().zip(old.iter().cycle().skip(1)).take(old.len()) {
                if inside(*a) {
                    poly.push(*a);
                }
                if inside(*a) != inside(*b) {
                    let t = (bound - value(*a)) / (value(*b) - value(*a));
                    poly.push(*a + (*b - *a) * t);
                }
            }
        }
        if poly.len() < 3 {
            continue;
        }
        let covered = poly.iter().zip(poly.iter().cycle().skip(1)).take(poly.len()).map(|(a, b)| a.x * b.y - a.y * b.x).sum::<f64>().abs() / 2.0;
        if covered < 1e-18 {
            continue;
        }
        area += covered;
        for v in poly {
            lo = lo.min(v.z);
            hi = hi.max(v.z);
        }
    }
    if area == 0.0 {
        return (bottom, bottom, false);
    }
    let complete = area >= 4.0 * half * half * (1.0 - 1e-8);
    (if complete { lo } else { bottom }, hi, !complete)
}
pub fn stock_stl(report: &Report, bottom: f64) -> Result<String> {
    if !bottom.is_finite() || report.cells.is_empty() || report.cells.len() > 1_000_000 {
        return invalid("invalid stock export bounds");
    }
    // Merge exactly coplanar adjacent columns before serialization. No resampling
    // or rounding changes the certified upper envelope.
    let mut cells = report.cells.clone();
    for _ in 0..24 {
        let before = cells.len();
        for axis in 0..2 {
            let other = 1 - axis;
            cells.sort_unstable_by(|a, b| {
                a.min[other]
                    .total_cmp(&b.min[other])
                    .then(a.max[other].total_cmp(&b.max[other]))
                    .then(a.high.total_cmp(&b.high))
                    .then(a.min[axis].total_cmp(&b.min[axis]))
            });
            let mut merged: Vec<Cell> = Vec::new();
            for c in cells {
                if let Some(last) = merged.last_mut()
                    && last.min[other] == c.min[other]
                    && last.max[other] == c.max[other]
                    && last.high == c.high
                    && last.max[axis] == c.min[axis]
                {
                    last.max[axis] = c.max[axis];
                    continue;
                }
                merged.push(c);
            }
            cells = merged;
        }
        if cells.len() == before {
            break;
        }
    }
    if cells.len() > 40_000 {
        return invalid("stock STL exceeds 40000 merged columns; use validation JSON for this resolution");
    }
    let mut out = String::from("solid cam_remaining_stock\n");
    for c in &cells {
        if c.high <= bottom {
            continue;
        }
        let [x, y] = c.min;
        let [u, v] = c.max;
        let z = c.high;
        let p = [
            Vec3::new(x, y, bottom),
            Vec3::new(u, y, bottom),
            Vec3::new(u, v, bottom),
            Vec3::new(x, v, bottom),
            Vec3::new(x, y, z),
            Vec3::new(u, y, z),
            Vec3::new(u, v, z),
            Vec3::new(x, v, z),
        ];
        for q in [[0, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4], [1, 2, 6, 5], [2, 3, 7, 6], [3, 0, 4, 7]] {
            for ids in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
                out.push_str("facet normal 0 0 0\nouter loop\n");
                for i in ids {
                    let a = p[i];
                    out.push_str(&format!("vertex {} {} {}\n", a.x, a.y, a.z));
                }
                out.push_str("endloop\nendfacet\n");
            }
        }
    }
    if out.len() > 128 * 1024 * 1024 {
        return invalid("stock STL exceeds 128 MiB");
    }
    out.push_str("endsolid cam_remaining_stock\n");
    Ok(out)
}

/// Independently evaluate a point on the upper stock surface. Used for analytic
/// acceptance probes, never as a substitute for whole-region verification.
pub fn probe_height(input: &JobInput, nc: &Parsed, p: [f64; 2]) -> Result<f64> {
    let mut z = input.setup.stock.max.z;
    for s in nc::segments(nc, &input.setup)? {
        if s.rapid {
            continue;
        }
        let t = input.tools.iter().find(|t| t.id == s.tool).ok_or_else(|| Error("unknown probe tool".into()))?;
        let (d, low, _) = sweep(p, 0.0, &s, t.diameter / 2.0);
        if d <= t.diameter / 2.0 {
            let point = if t.kind == ToolKind::TwistDrill { d / (t.tip_angle / 2.0).tan() } else { 0.0 };
            z = z.min(low + point);
        }
    }
    Ok(z.max(input.setup.stock.min.z))
}
