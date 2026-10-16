//! Generator-only ball/triangle contact queries. Verification must not use these
//! routines. Each query checks faces, edges and vertices analytically, including
//! vertical and degenerate projected triangles; no surface sampling is involved.
use crate::{Result, invalid, model, spatial::Index};
use solidcraft_geom::Vec3;

pub struct BallContact<'a> {
    triangles: &'a [[Vec3; 3]],
    index: Index,
    guard: f64,
}

/// Height errors of an entire linear tip motion relative to the contact surface.
/// A missing excess bound cannot certify interpolation accuracy.
#[derive(Clone, Copy, Debug)]
pub struct ChordBounds {
    pub gouge: f64,
    pub excess: Option<f64>,
}

impl<'a> BallContact<'a> {
    pub fn new(triangles: &'a [[Vec3; 3]]) -> Result<Self> {
        if triangles.is_empty() || triangles.len() > 200_000 {
            return invalid("contact mesh must contain 1–200000 triangles");
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
        Ok(Self { triangles, index: Index::new(items), guard: 2048. * f64::EPSILON * extent })
    }

    /// Minimum non-gouging ball-tip height above the mesh at XY. None means no
    /// contact, not zero height or a certificate of solid topology/accessibility.
    /// `work` is a shared remaining budget; exhaustion is an error.
    pub async fn height(&self, xy: [f64; 2], radius: f64, work: &mut usize) -> Result<Option<f64>> {
        self.validate(xy, radius)?;
        let r = radius + self.guard;
        let mut candidates = Vec::new();
        self.index.query([xy[0] - r, xy[1] - r], [xy[0] + r, xy[1] + r], &mut candidates);
        let mut high: Option<f64> = None;
        for (n, id) in candidates.into_iter().enumerate() {
            self.charge(work, 1, n).await?;
            let triangle = self.triangles.get(id).ok_or_else(|| crate::Error("invalid contact index".into()))?;
            if let Some(z) = triangle_height(triangle, xy, r) {
                let z = z + self.guard * 2.;
                high = Some(high.map_or(z, |old| old.max(z)));
            }
        }
        Ok(high)
    }

    /// Certify the WHOLE chord. Subtracting the moving tip displacement from a
    /// triangle produces a convex triangular prism. Dropping the stationary ball
    /// onto its eight boundary triangles bounds contact at every intermediate
    /// pose, including a narrow feature missed by endpoints and midpoint.
    ///
    /// For excess stock, each triangle's contact height is concave on its convex
    /// projected domain. If both endpoints contact that triangle, its chord is a
    /// lower bound on the mesh envelope, so the larger endpoint gap bounds excess.
    pub async fn chord(&self, from: Vec3, to: Vec3, radius: f64, work: &mut usize) -> Result<ChordBounds> {
        model::point(from)?;
        model::point(to)?;
        self.validate([from.x, from.y], radius)?;
        let r = radius + self.guard;
        let mut candidates = Vec::new();
        self.index.query([from.x.min(to.x) - r, from.y.min(to.y) - r], [from.x.max(to.x) + r, from.y.max(to.y) + r], &mut candidates);
        let delta = to - from;
        let mut gouge = f64::NEG_INFINITY;
        let mut excess: Option<f64> = None;
        for (n, id) in candidates.into_iter().enumerate() {
            self.charge(work, 10, n).await?;
            let triangle = self.triangles.get(id).ok_or_else(|| crate::Error("invalid contact index".into()))?;
            let shifted = triangle.map(|p| p - delta);
            let faces = [
                *triangle,
                shifted,
                [triangle[0], triangle[1], shifted[1]],
                [triangle[0], shifted[1], shifted[0]],
                [triangle[1], triangle[2], shifted[2]],
                [triangle[1], shifted[2], shifted[1]],
                [triangle[2], triangle[0], shifted[0]],
                [triangle[2], shifted[0], shifted[2]],
            ];
            for face in faces {
                if let Some(z) = triangle_height(&face, [from.x, from.y], r) {
                    gouge = gouge.max(z + self.guard * 4. - from.z);
                }
            }
            // A smaller ball's contact tip height is a lower bound: its lower
            // surface is higher at every fixed lateral distance from its tip.
            let eroded = radius - self.guard;
            if eroded > 0. {
                let a = triangle_height(triangle, [from.x, from.y], eroded);
                let b = triangle_height(triangle, [to.x, to.y], eroded);
                if let (Some(a), Some(b)) = (a, b) {
                    let gap = (from.z - a).max(to.z - b) + self.guard * 4.;
                    excess = Some(excess.map_or(gap, |old| old.min(gap)));
                }
            }
        }
        Ok(ChordBounds { gouge, excess })
    }

    fn validate(&self, xy: [f64; 2], radius: f64) -> Result<()> {
        model::point(Vec3::new(xy[0], xy[1], 0.))?;
        if !model::positive(radius) || radius <= self.guard * 16. {
            return invalid("invalid or numerically unresolved ball radius");
        }
        Ok(())
    }
    async fn charge(&self, work: &mut usize, amount: usize, n: usize) -> Result<()> {
        *work = work.checked_sub(amount).ok_or_else(|| crate::Error("cutter-contact work budget exhausted".into()))?;
        if n.is_multiple_of(128) {
            super::YieldOnce::default().await;
        }
        Ok(())
    }
}

fn triangle_height(tri: &[Vec3; 3], p: [f64; 2], radius: f64) -> Option<f64> {
    // Translate before products to retain accuracy far from the setup origin.
    let [a, b, c] = tri.map(|v| Vec3::new(v.x - p[0], v.y - p[1], v.z));
    let mut high: Option<f64> = None;
    let mut add = |z: f64| high = Some(high.map_or(z, |old| old.max(z)));
    for (a, b) in [(a, b), (b, c), (c, a)] {
        let d = b - a;
        let length = d.x.hypot(d.y);
        if length == 0. {
            let distance = a.x.hypot(a.y);
            if distance <= radius {
                add(a.z.max(b.z) + ((radius - distance) * (radius + distance)).max(0.).sqrt() - radius);
            }
            continue;
        }
        let ux = d.x / length;
        let uy = d.y / length;
        let along = -(a.x * ux + a.y * uy);
        let across = (a.x * uy - a.y * ux).abs();
        if across > radius {
            continue;
        }
        let cross_radius = ((radius - across) * (radius + across)).max(0.).sqrt();
        let lo = 0_f64.max(along - cross_radius);
        let hi = length.min(along + cross_radius);
        if lo > hi {
            continue;
        }
        let s = (along + d.z * cross_radius / length.hypot(d.z)).clamp(lo, hi);
        let z = a.z + d.z * (s / length) + ((cross_radius - (s - along).abs()) * (cross_radius + (s - along).abs())).max(0.).sqrt() - radius;
        add(z);
    }
    let normal = (b - a).cross(c - a);
    let length = normal.len();
    if normal.z != 0. && length > 0. {
        let sign = normal.z.signum();
        let qx = -radius * sign * normal.x / length;
        let qy = -radius * sign * normal.y / length;
        let u = ((qx - a.x) * (c.y - a.y) - (qy - a.y) * (c.x - a.x)) / normal.z;
        let v = ((b.x - a.x) * (qy - a.y) - (b.y - a.y) * (qx - a.x)) / normal.z;
        if u >= 0. && v >= 0. && u + v <= 1. {
            add(a.z + u * (b.z - a.z) + v * (c.z - a.z) + radius * normal.z.abs() / length - radius);
        }
    }
    high
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    fn ready<T>(f: impl std::future::Future<Output = T>) -> T {
        let mut f = std::pin::pin!(f);
        loop {
            if let std::task::Poll::Ready(value) = f.as_mut().poll(&mut std::task::Context::from_waker(std::task::Waker::noop())) {
                return value;
            }
        }
    }
    fn plane(slope: f64) -> [[Vec3; 3]; 2] {
        let a = Vec3::new(-20., -20., -20. * slope);
        let b = Vec3::new(20., -20., 20. * slope);
        let c = Vec3::new(20., 20., 20. * slope);
        let d = Vec3::new(-20., 20., -20. * slope);
        [[a, b, c], [a, c, d]]
    }
    #[test]
    fn face_contact_matches_analytic_plane_offsets_and_reversed_winding() {
        for slope in [0., 0.5, -1., 4.] {
            for reverse in [false, true] {
                let tris = plane(slope).map(|t| if reverse { [t[2], t[1], t[0]] } else { t });
                let mesh = BallContact::new(&tris).unwrap();
                let z = ready(mesh.height([5., 0.], 3., &mut 100)).unwrap().unwrap();
                let expected = 5. * slope + 3. * ((1. + slope * slope).sqrt() - 1.);
                assert!((z - expected).abs() < 1e-8, "{slope}: {z} vs {expected}");
                let a = Vec3::new(5., 0., z);
                let b = Vec3::new(7., 0., z + 2. * slope);
                let bounds = ready(mesh.chord(a, b, 3., &mut 100)).unwrap();
                assert!(bounds.gouge.abs() < 1e-8 && bounds.excess.unwrap().abs() < 1e-8, "{bounds:?}");
            }
        }
    }
    #[test]
    fn edges_vertices_vertical_faces_and_no_contact() {
        let vertex = [Vec3::new(0., 0., 7.); 3];
        assert!((triangle_height(&vertex, [3., 0.], 5.).unwrap() - 6.).abs() < 1e-12);
        assert!(triangle_height(&vertex, [6., 0.], 5.).is_none());
        let wall = [Vec3::ZERO, Vec3::new(0., 0., 10.), Vec3::new(0., 10., 10.)];
        assert!((triangle_height(&wall, [3., 4.], 5.).unwrap() - 9.).abs() < 1e-12);
        let slope_edge = [Vec3::new(-10., 0., -10.), Vec3::new(10., 0., 10.), Vec3::new(-10., 0., -10.)];
        assert!((triangle_height(&slope_edge, [0., 0.], 5.).unwrap() - 5. * (2_f64.sqrt() - 1.)).abs() < 1e-12);
    }
    #[test]
    fn swept_chord_finds_a_narrow_peak_between_all_three_usual_samples() {
        let triangles = [[Vec3::new(2., -0.01, 4.), Vec3::new(2.01, -0.01, 4.), Vec3::new(2., 0.01, 4.)]];
        let mesh = BallContact::new(&triangles).unwrap();
        for x in [0., 5., 10.] {
            assert!(ready(mesh.height([x, 0.], 0.1, &mut 100)).unwrap().is_none());
        }
        let bounds = ready(mesh.chord(Vec3::ZERO, Vec3::new(10., 0., 0.), 0.1, &mut 100)).unwrap();
        assert!((bounds.gouge - 4.).abs() < 1e-8);
        assert!(bounds.excess.is_none());
    }
    #[test]
    fn translated_mesh_budget_and_hostile_inputs() {
        let triangles = plane(1.).map(|t| t.map(|p| p + Vec3::new(100_000., -200_000., 300_000.)));
        let mesh = BallContact::new(&triangles).unwrap();
        let z = ready(mesh.height([100_000., -200_000.], 5., &mut 100)).unwrap().unwrap();
        assert!((z - 300_000. - 5. * (2_f64.sqrt() - 1.)).abs() < 1e-6);
        assert!(ready(mesh.height([100_000., -200_000.], 5., &mut 0)).is_err());
        for radius in [0., -1., f64::NAN, f64::INFINITY, 1e7] {
            assert!(ready(mesh.height([0., 0.], radius, &mut 100)).is_err());
        }
        assert!(BallContact::new(&[[Vec3::new(f64::NAN, 0., 0.); 3]]).is_err());
        assert!(BallContact::new(&[]).is_err());
    }
}
