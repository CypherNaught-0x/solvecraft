//! Toolpath construction. No verifier or NC interpreter algorithms live here.
use crate::*;
use cavalier_contours::polyline::{PlineSource, PlineSourceMut, Polyline, seg_arc_radius_and_center, seg_length};
use cavalier_contours::shape_algorithms::{Shape, ShapeOffsetOptions};
use solvecraft_geom::Vec3;

use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, Waker},
};

/// A portable job advances a bounded emission/validation chunk or one bounded
/// library offset query per poll. No executor, OS thread or browser API is used.
pub struct Generator {
    pending: Option<Pin<Box<dyn Future<Output = Result<Toolpath>> + Send>>>,
    path: Option<Toolpath>,
    progress: Arc<AtomicUsize>,
    failed: Option<String>,
    cancelled: bool,
}
impl Generator {
    pub fn new(input: JobInput) -> Result<Self> {
        input.validate()?;
        if input.operations.is_empty() || input.operations.len() > MAX_ITEMS {
            return invalid("empty or excessive operation list");
        }
        let progress = Arc::new(AtomicUsize::new(0));
        let pending = Box::pin(generate_incremental(input, progress.clone()));
        Ok(Self { pending: Some(pending), path: None, progress, failed: None, cancelled: false })
    }
    pub fn cancel(&mut self) {
        self.cancelled = true;
        self.pending = None;
        self.path = None;
    }
    pub fn progress(&self) -> f64 {
        self.progress.load(Ordering::Relaxed) as f64 / 1_000_000.
    }
    pub fn step(&mut self) -> Result<bool> {
        if self.cancelled {
            return invalid("generation cancelled");
        }
        if let Some(error) = &self.failed {
            return invalid(error.clone());
        }
        if self.path.is_some() {
            return Ok(true);
        }
        let future = self.pending.as_mut().ok_or_else(|| Error("generation has no pending work".into()))?;
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| future.as_mut().poll(&mut Context::from_waker(Waker::noop()))));
        match outcome {
            Ok(Poll::Pending) => Ok(false),
            Ok(Poll::Ready(Ok(path))) => {
                self.pending = None;
                self.path = Some(path);
                self.progress.store(1_000_000, Ordering::Relaxed);
                Ok(true)
            }
            result => {
                let error = match result {
                    Ok(Poll::Ready(Err(e))) => e,
                    _ => Error("contour library failed; no path published".into()),
                };
                self.pending = None;
                self.failed = Some(error.0.clone());
                Err(error)
            }
        }
    }
    pub fn finish(self) -> Result<Toolpath> {
        if self.cancelled {
            return invalid("generation cancelled");
        }
        self.path.ok_or_else(|| Error(self.failed.unwrap_or_else(|| "generation incomplete".into())))
    }
}
#[derive(Default)]
struct YieldOnce(bool);
impl Future for YieldOnce {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<()> {
        if self.0 {
            Poll::Ready(())
        } else {
            self.0 = true;
            Poll::Pending
        }
    }
}
#[derive(Clone)]
struct Progress {
    value: Arc<AtomicUsize>,
    operation: usize,
    count: usize,
}
impl Progress {
    fn set(&self, fraction: f64) {
        self.value.fetch_max(((self.operation as f64 + fraction.clamp(0., 1.)) / self.count as f64 * 1_000_000.) as usize, Ordering::Relaxed);
    }
}
async fn generate_incremental(input: JobInput, value: Arc<AtomicUsize>) -> Result<Toolpath> {
    let mut path = Toolpath { fingerprint: input.fingerprint.clone(), blocks: Vec::new() };
    for (index, o) in input.operations.iter().enumerate() {
        YieldOnce::default().await;
        let progress = Progress { value: value.clone(), operation: index, count: input.operations.len() };
        let blocks = operation(&input, o, progress.clone()).await?;
        if path.blocks.len() + blocks.len() > MAX_MOVES {
            return invalid("toolpath exceeds motion limit");
        }
        path.blocks.extend(blocks);
        progress.set(1.);
    }
    Ok(path)
}
pub fn generate(input: JobInput) -> Result<Toolpath> {
    let mut g = Generator::new(input)?;
    while !g.step()? {}
    g.finish()
}
fn tool(i: &JobInput, id: u64) -> Result<&Tool> {
    i.tools.iter().find(|t| t.id == id).ok_or_else(|| Error("unknown tool".into()))
}

struct Writer {
    blocks: Vec<Block>,
    op: u64,
    pos: Vec3,
    progress: Progress,
}
impl Writer {
    async fn push(&mut self, motion: Motion) -> Result<()> {
        if self.blocks.len() >= MAX_MOVES {
            return invalid("operation exceeds motion limit");
        }
        if let Motion::Rapid { to } | Motion::Linear { to, .. } | Motion::Arc { to, .. } = &motion {
            crate::model::point(*to)?;
            self.pos = *to;
        }
        self.blocks.push(Block { operation: self.op, motion });
        if self.blocks.len().is_multiple_of(256) {
            self.progress.set(0.3 + 0.65 * self.blocks.len() as f64 / (self.blocks.len() as f64 + 1024.));
            YieldOnce::default().await;
        }
        Ok(())
    }
    async fn rapid(&mut self, p: Vec3) -> Result<()> {
        self.push(Motion::Rapid { to: p }).await
    }
    async fn feed(&mut self, p: Vec3, f: f64) -> Result<()> {
        self.push(Motion::Linear { to: p, feed: f }).await
    }
    async fn enter(&mut self, s: &Setup, p: Vec3, o: &Operation) -> Result<()> {
        self.rapid(Vec3::new(self.pos.x, self.pos.y, s.clearance)).await?;
        self.rapid(Vec3::new(p.x, p.y, s.clearance)).await?;
        self.rapid(Vec3::new(p.x, p.y, s.retract)).await?;
        self.feed(p, o.plunge_feed).await
    }
}
async fn operation(input: &JobInput, o: &Operation, progress: Progress) -> Result<Vec<Block>> {
    let s = &input.setup;
    let t = tool(input, o.tool)?;
    let mut w = Writer { blocks: Vec::new(), op: o.id, pos: Vec3::new(0.0, 0.0, s.clearance), progress: progress.clone() };
    w.push(Motion::Tool { tool: t.id, number: t.number }).await?;
    w.push(Motion::Spindle { rpm: o.spindle }).await?;
    w.push(Motion::Coolant { on: o.coolant }).await?;
    let levels = depths(o.top, o.bottom + o.allowance, o.stepdown)?;
    match &o.geometry {
        Strategy::Face => {
            let (a, b) = (s.stock.min, s.stock.max);
            let radius = t.diameter / 2.0;
            let n = ((b.y - a.y) / o.stepover).ceil() as usize;
            if n > 10_000 {
                return invalid("facing exceeds pass limit");
            }
            for z in levels {
                for j in 0..=n {
                    let y = a.y + (b.y - a.y) * j as f64 / n.max(1) as f64;
                    let (x0, x1) = if o.climb { (a.x - radius, b.x + radius) } else { (b.x + radius, a.x - radius) };
                    w.enter(s, Vec3::new(x0, y, z), o).await?;
                    w.feed(Vec3::new(x1, y, z), o.feed).await?;
                }
            }
        }
        Strategy::Contour { boundary, inside } => {
            if !boundary.islands.is_empty() {
                return invalid("contour islands: select each contour as its own operation");
            }
            let shape = shape(boundary, progress.clone()).await?;
            let d = (t.diameter / 2.0 + o.allowance) * if *inside { 1.0 } else { -1.0 };
            YieldOnce::default().await;
            let loops = shape.parallel_offset(d, &offset_options());
            if loops.ccw_plines.is_empty() {
                return invalid("tool cannot fit contour");
            }
            for z in levels {
                for lp in &loops.ccw_plines {
                    trace(&mut w, s, o, &lp.polyline, z, o.climb == *inside).await?;
                }
            }
        }
        Strategy::Pocket { boundary } => {
            let original = shape(boundary, progress.clone()).await?;
            let mut loops = Vec::new();
            let mut vertices = 0usize;
            let mut distance = t.diameter / 2.0 + o.allowance;
            for pass in 0..10_000 {
                progress.set(0.1 + 0.15 * pass as f64 / (pass as f64 + 8.));
                YieldOnce::default().await;
                let offset = original.parallel_offset(distance, &offset_options());
                if offset.ccw_plines.is_empty() {
                    break;
                }
                for lp in offset.ccw_plines.iter().chain(&offset.cw_plines) {
                    vertices = vertices.checked_add(lp.polyline.vertex_count()).ok_or_else(|| Error("pocket vertex budget exhausted".into()))?;
                    if vertices > MAX_MOVES {
                        return invalid("pocket vertex budget exhausted");
                    }
                    loops.push(lp.polyline.clone());
                }
                if loops.len() > 10_000 {
                    return invalid("pocket exceeds pass limit");
                }
                distance += o.stepover;
                if pass == 9999 {
                    return invalid("pocket offset budget exhausted");
                }
            }
            if loops.is_empty() {
                return invalid("tool cannot fit pocket");
            }
            // Innermost passes first; independent clearance retracts avoid island crossings.
            for z in levels {
                for lp in loops.iter().rev() {
                    trace(&mut w, s, o, lp, z, o.climb).await?;
                }
            }
        }
        Strategy::Drill { holes, peck, full_diameter } => {
            let bottom = o.bottom - if *full_diameter { t.tip_length() } else { 0.0 };
            let zs = depths(o.top, bottom, peck.unwrap_or(o.top - bottom))?;
            for p in holes {
                w.enter(s, Vec3::new(p[0], p[1], o.top), o).await?;
                let mut previous = o.top;
                for z in &zs {
                    if previous != o.top {
                        w.feed(Vec3::new(p[0], p[1], previous), o.plunge_feed).await?;
                    }
                    w.feed(Vec3::new(p[0], p[1], *z), o.plunge_feed).await?;
                    w.rapid(Vec3::new(p[0], p[1], s.retract)).await?;
                    previous = *z;
                }
            }
        }
    }
    w.rapid(Vec3::new(w.pos.x, w.pos.y, s.clearance)).await?;
    w.push(Motion::Coolant { on: false }).await?;
    w.push(Motion::Spindle { rpm: 0.0 }).await?;
    w.push(Motion::Stop).await?;
    Ok(w.blocks)
}
fn depths(top: f64, bottom: f64, step: f64) -> Result<Vec<f64>> {
    if bottom >= top {
        return invalid("allowance leaves no cutting depth");
    }
    let n = ((top - bottom) / step).ceil() as usize;
    if n > 10_000 {
        return invalid("depth pass limit");
    }
    Ok((1..=n).map(|i| (top - step * i as f64).max(bottom)).collect())
}
async fn trace(w: &mut Writer, s: &Setup, o: &Operation, lp: &Polyline<f64>, z: f64, reverse: bool) -> Result<()> {
    let mut lp = lp.clone();
    if reverse {
        lp.invert_direction_mut();
    }
    let start = lp.iter_vertexes().next().ok_or_else(|| Error("empty offset".into()))?;
    let entry_z = if o.ramp_angle.is_some() { o.top } else { z };
    let first = Vec3::new(start.x, start.y, entry_z);
    let tangent = lp.iter_segments().next().map(|(a, b)| Vec3::new(b.x - a.x, b.y - a.y, 0.0)).and_then(|v| v.normalized()).unwrap_or(Vec3::X);
    w.enter(s, first - tangent * o.lead_distance, o).await?;
    if o.lead_distance > 0.0 {
        w.feed(first, o.feed).await?;
    }
    if let Some(angle) = o.ramp_angle {
        let perimeter = lp.iter_segments().map(|(a, b)| seg_length(a, b)).sum::<f64>();
        if perimeter <= 1e-8 {
            return invalid("ramp contour has no length");
        }
        let laps = ((o.top - z) / (perimeter * angle.tan())).ceil() as usize;
        if laps > 1000 {
            return invalid("entry ramp exceeds lap budget");
        }
        let mut height = o.top;
        for _ in 0..laps {
            for (a, b) in lp.iter_segments() {
                let length = seg_length(a, b);
                let bottom = (height - length * angle.tan()).max(z);
                if a.bulge.abs() < 1e-12 {
                    w.feed(Vec3::new(b.x, b.y, bottom), o.plunge_feed).await?;
                } else {
                    let (radius, c) = seg_arc_radius_and_center(a, b);
                    let sweep = 4.0 * a.bulge.atan();
                    let start = (a.y - c.y).atan2(a.x - c.x);
                    let step = (2.0 * (1.0 - POST_ERROR / radius).clamp(-1.0, 1.0).acos()).max(1e-6);
                    let n = (sweep.abs() / step).ceil() as usize;
                    if n > MAX_MOVES {
                        return invalid("ramp arc exceeds motion budget");
                    }
                    for k in 1..=n {
                        let t = k as f64 / n as f64;
                        let theta = start + sweep * t;
                        w.feed(Vec3::new(c.x + radius * theta.cos(), c.y + radius * theta.sin(), height + (bottom - height) * t), o.plunge_feed)
                            .await?;
                    }
                }
                height = bottom;
            }
        }
    }
    for (a, b) in lp.iter_segments() {
        let to = Vec3::new(b.x, b.y, z);
        if a.bulge.abs() < 1e-12 {
            w.feed(to, o.feed).await?;
        } else {
            let (_, c) = seg_arc_radius_and_center(a, b);
            w.push(Motion::Arc { to, center: Vec3::new(c.x, c.y, z), clockwise: a.bulge < 0.0, feed: o.feed }).await?;
        }
    }
    if o.lead_distance > 0.0 {
        w.feed(Vec3::new(start.x, start.y, z) - tangent * o.lead_distance, o.feed).await?;
    }
    Ok(())
}
// Work in setup millimetres with predicate/stitch tolerances comfortably below
// the post budget. Upstream defaults can merge distinct small offset slices.
fn offset_options() -> ShapeOffsetOptions<f64> {
    ShapeOffsetOptions { pos_equal_eps: 1e-8, offset_dist_eps: 1e-7, slice_join_eps: 1e-8 }
}
async fn shape(b: &Boundary, progress: Progress) -> Result<Shape<f64>> {
    b.validate()?;
    // Reject all crossings/touches, not just those the offset library happens to tolerate.
    let rings: Vec<_> = std::iter::once(&b.outer).chain(&b.islands).collect();
    let mut comparisons = 0usize;
    for (r, a) in rings.iter().enumerate() {
        for i in 0..a.len() {
            let p = a[i];
            let q = a[(i + 1) % a.len()];
            if distance(p, q) < 1e-7 {
                return invalid("zero-length boundary edge");
            }
            for (s, c) in rings.iter().enumerate().skip(r) {
                for j in 0..c.len() {
                    if r == s && (i == j || (i + 1) % a.len() == j || (j + 1) % a.len() == i) {
                        continue;
                    }
                    comparisons += 1;
                    if comparisons.is_multiple_of(4096) {
                        progress.set(0.1 * comparisons as f64 / 2_000_000.);
                        YieldOnce::default().await;
                    }
                    if comparisons > 2_000_000 {
                        return invalid("boundary validation budget exceeded");
                    }
                    if intersects(p, q, c[j], c[(j + 1) % c.len()]) {
                        return invalid("boundary intersections or touching loops");
                    }
                }
            }
        }
    }
    for (i, hole) in b.islands.iter().enumerate() {
        if !inside(hole[0], &b.outer) || b.islands.iter().enumerate().any(|(j, h)| i != j && inside(hole[0], h)) {
            return invalid("islands must be disjoint and inside the outer boundary");
        }
    }
    let mut plines = Vec::new();
    for (i, ring) in rings.iter().enumerate() {
        YieldOnce::default().await;
        let mut p = Polyline::new_closed();
        for xy in ring.iter() {
            p.add(xy[0], xy[1], 0.0);
        }
        if p.area().abs() < 1e-8 {
            return invalid("zero-area boundary");
        }
        if (p.area() > 0.0) != (i == 0) {
            p.invert_direction_mut();
        }
        plines.push(p);
    }
    Ok(Shape::from_plines(plines))
}
fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
fn cross(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}
fn intersects(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let eps = 1e-8;
    let overlap = (0..2).all(|k| a[k].min(b[k]) <= c[k].max(d[k]) + eps && c[k].min(d[k]) <= a[k].max(b[k]) + eps);
    overlap && cross(a, b, c) * cross(a, b, d) <= eps && cross(c, d, a) * cross(c, d, b) <= eps
}
fn inside(p: [f64; 2], ring: &[[f64; 2]]) -> bool {
    let mut yes = false;
    for (a, b) in ring.iter().zip(ring.iter().cycle().skip(1)).take(ring.len()) {
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
            yes = !yes;
        }
    }
    yes
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod incremental_tests {
    use super::*;
    #[test]
    fn large_single_operations_yield_and_remain_deterministic() {
        for name in ["face", "contour", "pocket", "drill"] {
            let mut input = corpus::fixture(name).unwrap();
            input.operations[0].stepdown = 0.02;
            if name == "drill" {
                input.operations[0].geometry = Strategy::Drill {
                    holes: (0..300).map(|n| [1. + (n % 18) as f64, 1. + (n / 18) as f64]).collect(),
                    peck: Some(0.05),
                    full_diameter: false,
                };
            }
            let mut generator = Generator::new(input.clone()).unwrap();
            let mut polls = 0;
            let mut previous = 0.;
            let mut intermediate = false;
            while !generator.step().unwrap() {
                polls += 1;
                let progress = generator.progress();
                assert!(progress >= previous && progress < 1.);
                intermediate |= progress > 0.;
                previous = progress;
                assert!(polls < 100_000);
            }
            assert!(polls > 10 && intermediate, "{name}: {polls}");
            assert_eq!(generator.progress(), 1.);
            let path = generator.finish().unwrap();
            assert_eq!(serde_json::to_value(path).unwrap(), serde_json::to_value(generate(input).unwrap()).unwrap());
        }
    }
    #[test]
    fn cancellation_during_one_operation_discards_partial_output() {
        let mut input = corpus::fixture("face").unwrap();
        input.operations[0].stepdown = 0.001;
        let mut generator = Generator::new(input).unwrap();
        for _ in 0..4 {
            assert!(!generator.step().unwrap());
        }
        assert!(generator.progress() > 0. && generator.progress() < 1.);
        generator.cancel();
        assert!(generator.step().is_err());
        assert!(generator.finish().is_err());
    }
    #[test]
    fn incomplete_and_failed_jobs_never_publish_partial_paths() {
        let mut generator = Generator::new(corpus::fixture("face").unwrap()).unwrap();
        assert!(!generator.step().unwrap());
        assert!(generator.finish().is_err());
        let mut input = corpus::fixture("face").unwrap();
        let mut bad = input.operations[0].clone();
        bad.id = 2;
        bad.allowance = 2.;
        input.operations.push(bad);
        let mut generator = Generator::new(input).unwrap();
        let failed = loop {
            match generator.step() {
                Ok(false) => {}
                result => break result,
            }
        };
        assert!(failed.is_err());
        assert!(generator.step().is_err());
        assert!(generator.finish().is_err());
    }
}
