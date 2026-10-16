//! Roundoff-only vertex association at the CAD/CAM input boundary.
//!
//! Independently tessellated BRep faces can differ by a few ulps at their
//! shared endpoints. Canonicalize before hashing, so generation and independent
//! verification consume exactly the same solid. Reference STL is never repaired.
use crate::{EngineError, Result, cam, geom::Vec3};
use std::collections::BTreeMap;

pub(super) struct Vertices {
    guard: f64,
    axes: [Axis; 3],
}
#[derive(Default)]
struct Axis {
    points: Vec<f64>,
    buckets: BTreeMap<i64, Vec<usize>>,
}
fn error(message: &str) -> EngineError {
    EngineError::Other(message.into())
}
impl Vertices {
    pub(super) fn new(extent: f64) -> Result<Self> {
        // Each axis moves at most guard, so the 3D displacement is <= sqrt(3)*guard.
        let guard = extent.max(1.) * 4096. * f64::EPSILON;
        if !extent.is_finite() || 3_f64.sqrt() * guard > cam::VERIFY_ERROR / 128. {
            return Err(error("CAM coordinate roundoff exceeds the geometry budget; move the work origin closer"));
        }
        Ok(Self { guard, axes: Default::default() })
    }
    pub(super) fn anchor_stock(&mut self, stock: cam::Stock) -> Result<()> {
        // Exact declared stock planes take precedence over roundoff-spelled CAD
        // endpoints. This cannot move any point farther than the same error bound.
        self.point(stock.min)?;
        self.point(stock.max)?;
        Ok(())
    }
    fn point(&mut self, p: Vec3) -> Result<Vec3> {
        let x = self.axes[0].coordinate(p.x, self.guard)?;
        let y = self.axes[1].coordinate(p.y, self.guard)?;
        let z = self.axes[2].coordinate(p.z, self.guard)?;
        Ok(Vec3::new(x, y, z))
    }
    pub(super) fn triangle(&mut self, triangle: [Vec3; 3]) -> Result<[Vec3; 3]> {
        let [a, b, c] = triangle;
        let normalized = [self.point(a)?, self.point(b)?, self.point(c)?];
        let [p, q, r] = normalized;
        if (q - p).cross(r - p).dot((b - a).cross(c - a)) <= 0. {
            return Err(error("CAM mesh has degenerate features at numerical resolution"));
        }
        Ok(normalized)
    }
}
impl Axis {
    fn coordinate(&mut self, value: f64, guard: f64) -> Result<f64> {
        if !value.is_finite() || value.abs() / guard >= 1e16 {
            return Err(error("CAM mesh vertex exceeds its finite coordinate bound"));
        }
        let key = (value / guard).floor() as i64;
        let mut found = None;
        for offset in -1..=1 {
            if let Some(ids) = self.buckets.get(&(key + offset)) {
                for &id in ids {
                    if self.points.get(id).is_some_and(|q| (value - q).abs() <= guard) {
                        if found.is_some_and(|old| old != id) {
                            return Err(error("CAM mesh vertex association is ambiguous at numerical resolution"));
                        }
                        found = Some(id);
                    }
                }
            }
        }
        if let Some(id) = found {
            return self.points.get(id).copied().ok_or_else(|| error("CAM mesh vertex association failed"));
        }
        if self.points.len() >= 600_002 {
            return Err(error("CAM mesh exceeds vertex budget"));
        }
        let bucket = self.buckets.entry(key).or_default();
        if bucket.len() >= 4 {
            return Err(error("CAM mesh exceeds local vertex association budget"));
        }
        bucket.push(self.points.len());
        self.points.push(value);
        Ok(value)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn numerical_seams_join_but_real_cracks_remain() {
        let mut vertices = Vertices::new(2.).unwrap();
        let p = Vec3::new(1., 0., 1.2);
        assert_eq!(vertices.point(p).unwrap(), p);
        assert_eq!(vertices.point(p + Vec3::X * 1e-14).unwrap(), p);
        let crack = p + Vec3::X * 1e-5;
        assert_eq!(vertices.point(crack).unwrap(), crack);
    }
    #[test]
    fn coordinate_roundoff_keeps_vertical_walls_vertical() {
        let mut vertices = Vertices::new(2.).unwrap();
        let a = vertices.point(Vec3::new(1., 0., 0.)).unwrap();
        let b = vertices.point(Vec3::new(1. + 1e-14, 1., 0.)).unwrap();
        let c = vertices.point(Vec3::new(1. - 1e-14, 1., 1.2)).unwrap();
        assert_eq!((b - a).cross(c - a).z, 0.);
        assert_eq!(a.x, b.x);
        assert_eq!(b.x, c.x);
    }
    #[test]
    fn ambiguous_associations_collapsed_faces_and_excessive_error_are_rejected() {
        let mut vertices = Vertices::new(2.).unwrap();
        let guard = vertices.guard;
        vertices.point(Vec3::ZERO).unwrap();
        vertices.point(Vec3::X * guard * 1.5).unwrap();
        assert!(vertices.point(Vec3::X * guard * 0.75).is_err());
        assert!(vertices.triangle([Vec3::ZERO, Vec3::X * guard * 0.1, Vec3::Y]).is_err());
        assert!(Vertices::new(1e12).is_err());
        assert!(Vertices::new(f64::NAN).is_err());
    }
}
