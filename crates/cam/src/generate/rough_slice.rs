//! Protected cutter-center intervals for horizontal rows of 3D roughing.
//! Slice the target ABOVE the cutting plane, project its convex triangle pieces,
//! and expand them by the flat cutter plus radial allowance. This is generator
//! geometry, not an independent stock or finished-surface certificate.
use crate::{Result, invalid, model, spatial::Index};
use solidcraft_geom::Vec3;

pub struct Slices<'a> {
    triangles: &'a [[Vec3; 3]],
    index: Index,
    guard: f64,
}
#[derive(Clone, Copy)]
pub struct Query {
    pub y: f64,
    pub tip_z: f64,
    pub radius: f64,
    pub radial_allowance: f64,
    pub axial_allowance: f64,
}
impl<'a> Slices<'a> {
    pub fn new(triangles: &'a [[Vec3; 3]]) -> Result<Self> {
        if triangles.is_empty() || triangles.len() > 200_000 {
            return invalid("roughing needs a bounded target mesh");
        }
        let mut extent = 1_f64;
        let mut items = Vec::with_capacity(triangles.len());
        for (id, triangle) in triangles.iter().enumerate() {
            let mut min = [f64::INFINITY; 2];
            let mut max = [f64::NEG_INFINITY; 2];
            for p in triangle {
                model::point(*p)?;
                extent = extent.max(p.x.abs()).max(p.y.abs()).max(p.z.abs());
                min[0] = min[0].min(p.x);
                min[1] = min[1].min(p.y);
                max[0] = max[0].max(p.x);
                max[1] = max[1].max(p.y);
            }
            items.push((id, min, max));
        }
        Ok(Self { triangles, index: Index::new(items), guard: 1024. * f64::EPSILON * extent + 1e-8 })
    }

    /// Ordered, disjoint intervals that must NOT contain a cutting tool center.
    /// The work budget is shared across levels/rows. Polls yield after at most
    /// 128 triangle clips; no host thread or browser dependency is required.
    pub async fn blocked(&self, query: Query, work: &mut usize) -> Result<Vec<[f64; 2]>> {
        let Query { y, tip_z, radius, radial_allowance, axial_allowance } = query;
        if !model::positive(radius)
            || [y, tip_z, radial_allowance, axial_allowance].into_iter().any(|x| !model::finite(x))
            || radial_allowance < 0.
            || axial_allowance < 0.
        {
            return invalid("invalid roughing slice height, tool or allowance");
        }
        let reach = radius + radial_allowance + self.guard;
        let plane = tip_z - axial_allowance;
        let mut ids = Vec::new();
        self.index.query([-1e6, y - reach], [1e6, y + reach], &mut ids);
        let mut intervals = Vec::new();
        for (n, id) in ids.into_iter().enumerate() {
            if n.is_multiple_of(128) {
                super::YieldOnce::default().await;
            }
            *work = work.checked_sub(1).ok_or_else(|| crate::Error("roughing slice work budget exhausted".into()))?;
            let triangle = self.triangles.get(id).ok_or_else(|| crate::Error("invalid roughing triangle index".into()))?;
            // A horizontal face coincident with the cutting plane is tangent,
            // not an obstruction. Vertical walls above it remain protected.
            if triangle.iter().all(|p| p.z <= plane) {
                continue;
            }
            let mut polygon = Vec::with_capacity(4);
            for (&a, &b) in triangle.iter().zip(triangle.iter().cycle().skip(1)).take(3) {
                if a.z >= plane {
                    polygon.push(a);
                }
                if (a.z >= plane) != (b.z >= plane) {
                    polygon.push(a + (b - a) * ((plane - a.z) / (b.z - a.z)));
                }
            }
            if let Some([lo, hi]) = expanded_row(&polygon, y, reach) {
                intervals.push([lo - self.guard, hi + self.guard]);
            }
        }
        intervals.sort_unstable_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
        let mut merged: Vec<[f64; 2]> = Vec::new();
        for interval in intervals {
            if let Some(last) = merged.last_mut()
                && interval[0] <= last[1]
            {
                last[1] = last[1].max(interval[1]);
            } else {
                merged.push(interval);
            }
        }
        Ok(merged)
    }
}

/// Intersect a convex polygon Minkowski-summed with a disk and a horizontal row.
/// Every extreme lies on a vertex circle or an offset edge. Degenerate projected
/// lines/points are intentional (vertical target walls), not discarded contours.
fn expanded_row(polygon: &[Vec3], y: f64, radius: f64) -> Option<[f64; 2]> {
    let mut range = [f64::INFINITY, f64::NEG_INFINITY];
    let mut add = |x: f64| {
        range[0] = range[0].min(x);
        range[1] = range[1].max(x);
    };
    for (&a, &b) in polygon.iter().zip(polygon.iter().cycle().skip(1)).take(polygon.len()) {
        let distance = (y - a.y).abs();
        if distance <= radius {
            let dx = ((radius - distance) * (radius + distance)).max(0.).sqrt();
            add(a.x - dx);
            add(a.x + dx);
        }
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let length = dx.hypot(dy);
        if length == 0. || dy == 0. {
            continue;
        }
        for sign in [-1., 1.] {
            let x0 = a.x - sign * radius * dy / length;
            let y0 = a.y + sign * radius * dx / length;
            let t = (y - y0) / dy;
            if (0.0..=1.0).contains(&t) {
                add(x0 + t * dx);
            }
        }
    }
    (range[0] <= range[1]).then_some(range)
}

/// Complement of the certified protected intervals within a bounded row span.
/// The caller still has to prove entry, linking, engagement and remaining stock.
pub fn free_intervals(blocked: &[[f64; 2]], span: [f64; 2]) -> Result<Vec<[f64; 2]>> {
    if blocked.len() > 200_000 || !model::finite(span[0]) || !model::finite(span[1]) || span[0] >= span[1] {
        return invalid("invalid roughing row span");
    }
    let mut previous = f64::NEG_INFINITY;
    let mut cursor = span[0];
    let mut free = Vec::new();
    for &[lo, hi] in blocked {
        if !lo.is_finite() || !hi.is_finite() || lo > hi || lo < previous {
            return invalid("roughing intervals must be finite, disjoint and ordered");
        }
        previous = hi;
        if lo > cursor && cursor < span[1] {
            free.push([cursor, lo.min(span[1])]);
        }
        cursor = cursor.max(hi);
    }
    if cursor < span[1] {
        free.push([cursor, span[1]]);
    }
    Ok(free)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    fn ready<T>(f: impl std::future::Future<Output = T>) -> T {
        let mut f = std::pin::pin!(f);
        loop {
            if let std::task::Poll::Ready(v) = f.as_mut().poll(&mut std::task::Context::from_waker(std::task::Waker::noop())) {
                return v;
            }
        }
    }
    fn square(x0: f64, x1: f64, slope: f64) -> [[Vec3; 3]; 2] {
        let a = Vec3::new(x0, 0., 5. + slope * x0);
        let b = Vec3::new(x1, 0., 5. + slope * x1);
        let c = Vec3::new(x1, 10., 5. + slope * x1);
        let d = Vec3::new(x0, 10., 5. + slope * x0);
        [[a, b, c], [a, c, d]]
    }
    fn query(y: f64, z: f64) -> Query {
        Query { y, tip_z: z, radius: 1., radial_allowance: 0., axial_allowance: 0. }
    }
    #[test]
    fn horizontal_faces_and_rounded_corners_have_analytic_slice_extents() {
        let triangles = square(0., 10., 0.);
        let slices = Slices::new(&triangles).unwrap();
        let b = ready(slices.blocked(query(5., 4.), &mut 100)).unwrap();
        assert_eq!(b.len(), 1);
        assert!((b[0][0] + 1.).abs() < 1e-6 && (b[0][1] - 11.).abs() < 1e-6);
        assert!(ready(slices.blocked(query(5., 5.), &mut 100)).unwrap().is_empty());
        let corner = ready(slices.blocked(query(-0.5, 4.), &mut 100)).unwrap();
        assert!((corner[0][0] + 0.75_f64.sqrt()).abs() < 1e-6);
        assert!((corner[0][1] - 10. - 0.75_f64.sqrt()).abs() < 1e-6);
    }
    #[test]
    fn sloping_surface_separates_axial_and_radial_allowances() {
        let triangles = square(0., 10., 1.);
        let slices = Slices::new(&triangles).unwrap();
        let mut q = query(5., 10.);
        q.axial_allowance = 0.2;
        q.radial_allowance = 0.3;
        let b = ready(slices.blocked(q, &mut 100)).unwrap();
        // Plane z=5+x: clip x>=4.8, then expand by radius 1.3.
        assert!((b[0][0] - 3.5).abs() < 1e-6 && (b[0][1] - 11.3).abs() < 1e-6, "{b:?}");
    }
    #[test]
    fn islands_vertical_walls_winding_and_oversized_tools_preserve_protection() {
        let mut triangles = square(0., 2., 0.).to_vec();
        triangles.extend(square(6., 8., 0.));
        triangles[0].swap(0, 2);
        let slices = Slices::new(&triangles).unwrap();
        let blocked = ready(slices.blocked(query(5., 4.), &mut 100)).unwrap();
        assert_eq!(blocked.len(), 2);
        let free = free_intervals(&blocked, [-2., 10.]).unwrap();
        assert_eq!(free.len(), 3);
        assert!((free[1][0] - 3.).abs() < 1e-6 && (free[1][1] - 5.).abs() < 1e-6);
        let mut q = query(5., 4.);
        q.radius = 3.;
        assert!(free_intervals(&ready(slices.blocked(q, &mut 100)).unwrap(), [0., 8.]).unwrap().is_empty());
        let wall = [[Vec3::new(5., 0., 0.), Vec3::new(5., 0., 8.), Vec3::new(5., 10., 8.)]];
        let b = ready(Slices::new(&wall).unwrap().blocked(query(5., 4.), &mut 100)).unwrap();
        assert!((b[0][0] - 4.).abs() < 1e-6 && (b[0][1] - 6.).abs() < 1e-6);
    }
    #[test]
    fn malformed_inputs_and_work_exhaustion_are_explicit_errors() {
        assert!(Slices::new(&[]).is_err());
        assert!(Slices::new(&[[Vec3::new(f64::NAN, 0., 0.); 3]]).is_err());
        let triangles = square(0., 10., 0.);
        let slices = Slices::new(&triangles).unwrap();
        assert!(ready(slices.blocked(query(5., 4.), &mut 0)).is_err());
        let mut q = query(5., 4.);
        q.radial_allowance = -1.;
        assert!(ready(slices.blocked(q, &mut 100)).is_err());
        assert!(free_intervals(&[[2., 4.], [3., 5.]], [0., 10.]).is_err());
    }
}
