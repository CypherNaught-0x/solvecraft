//! What surface a face lies on (plane, cylinder, cone, sphere, torus), for construction
//! geometry: axes through round faces, centres of spheres and tori.

use solvecraft_geom::Vec3;

use crate::Body;
use crate::offset::{Surf, surf_of};

/// A face's analytic surface. Axes are unit vectors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FaceSurface {
    /// Outward unit normal and a point on the plane.
    Plane {
        normal: Vec3,
        point: Vec3,
    },
    Cylinder {
        origin: Vec3,
        axis: Vec3,
        radius: f64,
    },
    /// Apex, axis pointing into the cone, half-angle (radians).
    Cone {
        apex: Vec3,
        axis: Vec3,
        half_angle: f64,
    },
    Sphere {
        center: Vec3,
        radius: f64,
    },
    /// Centre of the ring, its axis, the ring (major) and tube (minor) radii.
    Torus {
        center: Vec3,
        axis: Vec3,
        major: f64,
        minor: f64,
    },
}

/// The surface of face `face` (in face order), if it is one of the analytic kinds.
pub fn face_surface(b: &Body, face: usize) -> Option<FaceSurface> {
    let f = b.solid.face_iter().nth(face)?;
    let tol = (b.size() * 1e-7).max(1e-9) * 100.0;
    Some(match surf_of(f, tol)? {
        Surf::Plane { n, d } => FaceSurface::Plane { normal: n, point: n * d },
        Surf::Cylinder { o, a, r, .. } => FaceSurface::Cylinder { origin: o, axis: a, radius: r },
        Surf::Cone { v, a, half, .. } => FaceSurface::Cone { apex: v, axis: a, half_angle: half },
        Surf::Sphere { c, r, .. } => FaceSurface::Sphere { center: c, radius: r },
        Surf::Torus { o, a, big, h, r, .. } => FaceSurface::Torus { center: o + a * h, axis: a, major: big, minor: r },
    })
}

/// An inward cylindrical face's axis and radius. The caller still verifies the
/// machined result against the complete target, including trimmed face bounds.
pub fn hole_axis(b: &Body, face: usize, tolerance: f64) -> crate::Result<(Vec3, Vec3, f64)> {
    if !tolerance.is_finite() || tolerance <= 0. {
        return Err(crate::KernelError::Invalid("invalid hole tolerance".into()));
    }
    crate::guard("hole face axis", || {
        let f = b.solid.face_iter().nth(face).ok_or_else(|| crate::KernelError::Invalid("hole face no longer exists".into()))?;
        match surf_of(f, tolerance) {
            Some(Surf::Cylinder { o, a, r, convex: false }) => Ok((o, a, r)),
            _ => Err(crate::KernelError::Invalid("select an inward cylindrical hole face".into())),
        }
    })
}
