//! Exact convex support for a ball mill's lower hemisphere. The remaining
//! cutting length, shank and holder are separate cylinders in the parent adapter.
//! In Parry coordinates +Y points toward the tool tip; the part's AABB center is
//! the origin, the sphere center is at Y=-R/2 and its flat cap is there too.
use super::{Pose, ShapeCastOptions, ShapeCastStatus, Status, Triangle, Vector};
use parry3d_f64::{
    query::details::{cast_shapes_support_map_support_map, distance_support_map_support_map},
    shape::SupportMap,
};

struct Hemisphere(f64);
impl SupportMap for Hemisphere {
    fn local_support_point(&self, dir: Vector) -> Vector {
        let direction = if dir.y >= 0. { dir } else { Vector::new(dir.x, 0., dir.z) };
        let length = direction.length();
        let point = if length > 0. { direction * (self.0 / length) } else { Vector::ZERO };
        point + Vector::new(0., -self.0 / 2., 0.)
    }
}

pub(super) fn cast(position: &Pose, velocity: Vector, radius: f64, triangle: &Triangle, margin: f64) -> Status {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let shape = Hemisphere(radius);
        let relative = position.inv_mul(&Pose::IDENTITY);
        let relative_velocity = position.rotation.inverse() * -velocity;
        let initial = distance_support_map_support_map(&relative, &shape, triangle);
        // This API returns zero for both intersection and failed convergence.
        // A zero distance alone therefore cannot prove an actual collision.
        let clear_initial = initial.is_finite() && initial > margin;
        let query = |distance| {
            cast_shapes_support_map_support_map(
                &relative,
                relative_velocity,
                &shape,
                triangle,
                ShapeCastOptions {
                    max_time_of_impact: 1.,
                    target_distance: distance,
                    stop_at_penetration: true,
                    compute_impact_geometry_on_penetration: true,
                },
            )
        };
        match query(margin) {
            None if clear_initial => Status::Pass,
            None => Status::Inconclusive,
            Some(_) => match query(0.) {
                Some(hit)
                    if matches!(hit.status, ShapeCastStatus::Converged | ShapeCastStatus::PenetratingOrWithinTargetDist)
                        && hit.time_of_impact.is_finite()
                        && (0.0..=1.0).contains(&hit.time_of_impact) =>
                {
                    Status::Fail
                }
                _ => Status::Inconclusive,
            },
        }
    }))
    .unwrap_or(Status::Inconclusive)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn support_is_on_the_hemisphere_or_cap_and_maximizes_each_direction() {
        let shape = Hemisphere(2.);
        for direction in [Vector::X, Vector::Y, -Vector::Y, Vector::Z, Vector::new(3., 4., 0.), Vector::new(3., -4., 0.)] {
            let support = shape.local_support_point(direction);
            let center = Vector::new(0., -1., 0.);
            assert!(support.y >= -1. && (support - center).length() <= 2. + 1e-12);
            for a in 0..32 {
                for b in 0..32 {
                    let polar = std::f64::consts::FRAC_PI_2 * a as f64 / 31.;
                    let angle = std::f64::consts::TAU * b as f64 / 32.;
                    let point = center + 2. * Vector::new(polar.sin() * angle.cos(), polar.cos(), polar.sin() * angle.sin());
                    assert!(point.dot(direction) <= support.dot(direction) + 1e-12);
                }
            }
        }
        assert_eq!(shape.local_support_point(-Vector::Y).y, -1.);
        assert_eq!(shape.local_support_point(Vector::Y).y, 1.);
    }

    #[test]
    fn hemisphere_clears_its_bounding_box_corners_and_detects_swept_contact() {
        // Radius 1: tool tip at world Z=0, cap Z=1. Parry Y=-Z.
        let pose = Pose::translation(0., -0.5, 0.);
        let triangle = |x: f64, z: f64| Triangle::new(Vector::new(x, -z, 0.), Vector::new(x + 0.02, -z, 0.), Vector::new(x, -z, 0.02));
        assert_eq!(cast(&pose, Vector::ZERO, 1., &triangle(0.9, 0.1), 0.0035), Status::Pass);
        assert_ne!(cast(&pose, Vector::ZERO, 1., &triangle(0.2, 0.1), 0.0035), Status::Pass);
        // A narrow face midway along a long travel must not be missed.
        assert_ne!(cast(&pose, Vector::new(10., 0., 0.), 1., &triangle(5., 0.1), 0.0035), Status::Pass);
        // No phantom upper hemisphere beyond a short cutter's flat cap.
        assert_eq!(cast(&pose, Vector::ZERO, 1., &triangle(0.5, 1.2), 0.0035), Status::Pass);
    }
}
