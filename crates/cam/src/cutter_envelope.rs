//! Independent ball-cutter sweep bounds for the next milling milestone.
//!
//! The tool tip is the lowest point of the ball; its center is one radius above
//! that tip. These bounds describe removal by one straight motion, not machine
//! clearance or a finished-part certificate. They contain no path-generation code.
use crate::{Result, invalid, model};
use solidcraft_geom::Vec3;

/// Bounds on the lowest swept cutting surface over a disk in the XY plane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeightBounds {
    /// No point in the disk can be cut below this height.
    pub low: f64,
    /// Every point in the disk is reached at or below this height. `None` means
    /// full coverage is not proved, so a stock verifier must retain old stock.
    pub high: Option<f64>,
}

/// Bound a ball-end cutter moving from `from` to `to` over a planar cell enclosed
/// by a disk centered at `xy` with radius `half`. `None` proves no disk overlap.
/// This constant-work query is portable and rejects non-finite/excessive inputs.
///
/// Inflating the swept sphere by `half` bounds every possible point in the cell.
/// Eroding it by `half` gives a witness that covers the entire cell at once. The
/// triangle inequality proves both inclusions, including sloped and vertical
/// motion. A small outward floating-point guard prevents tangency rounding from
/// certifying clearance; it is included in the returned interval.
pub fn ball_sweep(from: Vec3, to: Vec3, radius: f64, xy: [f64; 2], half: f64) -> Result<Option<HeightBounds>> {
    model::point(from)?;
    model::point(to)?;
    model::point(Vec3::new(xy[0], xy[1], 0.))?;
    if !model::positive(radius) || !model::finite(half) || half < 0. {
        return invalid("ball sweep needs a positive radius and nonnegative cell extent");
    }
    let scale = [from.x, from.y, from.z, to.x, to.y, to.z, xy[0], xy[1], radius, half].into_iter().map(f64::abs).fold(1_f64, f64::max);
    let guard = 512. * f64::EPSILON * scale;
    let Some(low) = sphere_sweep(from, to, radius, radius + half + guard, xy) else {
        return Ok(None);
    };
    let inner = radius - half - guard;
    let high = if inner > 0. { sphere_sweep(from, to, radius, inner, xy).map(|z| z + guard) } else { None };
    Ok(Some(HeightBounds { low: low - guard, high }))
}

/// Exact minimizer of the convex lower sphere envelope on a finite line segment.
/// `lift` remains the real tool radius when the bounding sphere is inflated.
fn sphere_sweep(from: Vec3, to: Vec3, lift: f64, radius: f64, xy: [f64; 2]) -> Option<f64> {
    sphere_contact(from, to, lift, radius, xy).map(|c| c.height)
}
struct Contact {
    height: f64,
    gradient: Option<[f64; 2]>,
    drop: f64,
}
fn sphere_contact(from: Vec3, to: Vec3, lift: f64, radius: f64, xy: [f64; 2]) -> Option<Contact> {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    let dz = to.z - from.z;
    let length = dx.hypot(dy);
    let px = xy[0] - from.x;
    let py = xy[1] - from.y;
    if length == 0. {
        let d = px.hypot(py);
        return (d <= radius).then(|| {
            let drop = ((radius - d) * (radius + d)).max(0.).sqrt();
            Contact { height: from.z.min(to.z) + lift - drop, gradient: (drop > 0.).then(|| [px / drop, py / drop]), drop }
        });
    }
    let ux = dx / length;
    let uy = dy / length;
    let along = px * ux + py * uy;
    let across = (px * uy - py * ux).abs();
    if across > radius {
        return None;
    }
    let reach = ((radius - across) * (radius + across)).max(0.).sqrt();
    let start = (along - reach).max(0.);
    let end = (along + reach).min(length);
    if start > end {
        return None;
    }
    // Setting f'(s)=0 gives s = along - dz*reach/hypot(length,dz).
    // This form avoids overflow from dividing by a nearly vertical XY length.
    let s = (along - (dz / length.hypot(dz)) * reach).clamp(start, end);
    let distance = (s - along).abs();
    let drop = ((reach - distance) * (reach + distance)).max(0.).sqrt();
    Some(Contact {
        height: from.z + dz * (s / length) + lift - drop,
        gradient: (drop > 0.).then(|| [(px - ux * s) / drop, (py - uy * s) / drop]),
        drop,
    })
}

/// Analytic point probes are diagnostics; whole-cell certification uses intervals.
pub(crate) fn ball_point_height(from: Vec3, to: Vec3, radius: f64, xy: [f64; 2]) -> Result<Option<f64>> {
    ball_sweep(from, to, radius, xy, 0.)?;
    Ok(sphere_sweep(from, to, radius, radius, xy))
}

/// The swept sphere is convex. Its lower height function is convex over its XY
/// projection: corner heights bound the maximum, and a supporting tangent at
/// the center bounds the minimum. This retains XY/Z correlation on slopes and
/// endcaps without paying an artificial +/- cell-radius error everywhere.
/// Tangencies or incomplete coverage retain the enclosing-disk certificate.
pub(crate) fn ball_rectangle(from: Vec3, to: Vec3, radius: f64, min: [f64; 2], max: [f64; 2], padding: f64) -> Result<Option<HeightBounds>> {
    for p in [min, max] {
        model::point(Vec3::new(p[0], p[1], 0.))?;
    }
    if min[0] > max[0] || min[1] > max[1] || !model::finite(padding) || padding < 0. {
        return invalid("invalid ball-sweep rectangle or padding");
    }
    let center = [(min[0] + max[0]) / 2., (min[1] + max[1]) / 2.];
    let half = (max[0] - min[0]).hypot(max[1] - min[1]) / 2. + padding;
    let Some(mut bounds) = ball_sweep(from, to, radius, center, half)? else { return Ok(None) };
    let scale = [from.x, from.y, from.z, to.x, to.y, to.z, radius].into_iter().chain(min).chain(max).map(f64::abs).fold(1_f64, f64::max);
    let guard = 512. * f64::EPSILON * scale;
    let hx = (max[0] - min[0]) / 2. + padding + guard;
    let hy = (max[1] - min[1]) / 2. + padding + guard;
    if let Some(c) = sphere_contact(from, to, radius, radius, center)
        && let Some(g) = c.gradient
        && c.drop > radius * f64::EPSILON.sqrt()
    {
        // Derivative conditioning grows near the lateral tangent. If it is
        // unresolved, the looser enclosing-disk bound remains authoritative.
        let error = guard * (1. + g[0].abs() + g[1].abs()) * (1. + (hx + hy) * radius / (c.drop * c.drop));
        let low = c.height - g[0].abs() * hx - g[1].abs() * hy - error;
        if low.is_finite() {
            bounds.low = bounds.low.max(low);
        }
    }
    let mut upper = Some(f64::NEG_INFINITY);
    for (x, y) in [(-hx, -hy), (hx, -hy), (hx, hy), (-hx, hy)] {
        upper = upper.zip(sphere_contact(from, to, radius, radius, [center[0] + x, center[1] + y])).and_then(|(old, contact)| {
            let g = contact.gradient?;
            (contact.drop > radius * f64::EPSILON.sqrt()).then(|| old.max(contact.height + guard * (1. + g[0].abs() + g[1].abs())))
        });
    }
    if let Some(upper) = upper {
        bounds.high = Some(bounds.high.map_or(upper, |old| old.min(upper)));
    }
    if bounds.high.is_some_and(|high| high < bounds.low) {
        return invalid("ball rectangle interval could not be resolved numerically");
    }
    Ok(Some(bounds))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn contains(bounds: HeightBounds, expected: f64) {
        assert!(bounds.low <= expected, "{bounds:?} excludes {expected} below");
        assert!(bounds.high.is_some_and(|h| h >= expected), "{bounds:?} excludes {expected} above");
    }

    #[test]
    fn stationary_tip_and_three_four_five_contact() {
        let tip = Vec3::new(10., 20., 3.);
        contains(ball_sweep(tip, tip, 5., [10., 20.], 0.).unwrap().unwrap(), 3.);
        contains(ball_sweep(tip, tip, 5., [13., 20.], 0.).unwrap().unwrap(), 4.);
        contains(ball_sweep(tip, tip, 5., [14., 20.], 0.).unwrap().unwrap(), 5.);
        assert!(ball_sweep(tip, tip, 5., [16., 20.], 0.).unwrap().is_none());
    }

    #[test]
    fn rectangular_sloped_bounds_preserve_surface_correlation_and_endcaps() {
        let from = Vec3::ZERO;
        let to = Vec3::new(20., 0., 4.);
        let min = [9.99, -0.01];
        let max = [10.01, 0.01];
        let slope = 0.2_f64;
        let expected_low = slope * min[0] + 5. - slope.hypot(1.) * 5.;
        let expected_high = slope * max[0] + 5. - slope.hypot(1.) * (25. - 0.01_f64.powi(2)).sqrt();
        for (a, b) in [(from, to), (to, from)] {
            let bounds = ball_rectangle(a, b, 5., min, max, 0.).unwrap().unwrap();
            assert!(bounds.low <= expected_low && bounds.high.unwrap() >= expected_high);
            assert!(bounds.high.unwrap() - bounds.low < 0.0041, "{bounds:?}");
        }
        // Endcap region falls back without claiming an interior stationary pose.
        let cap = ball_rectangle(from, to, 5., [22.99, -0.01], [23.01, 0.01], 0.).unwrap().unwrap();
        contains(cap, 5.);
        let padded = ball_rectangle(from, to, 5., min, max, 0.001).unwrap().unwrap();
        let unpadded = ball_rectangle(from, to, 5., min, max, 0.).unwrap().unwrap();
        assert!(padded.low < unpadded.low && padded.high.unwrap() > unpadded.high.unwrap());
        assert!(ball_rectangle(from, to, 5., max, min, 0.).is_err());
    }

    #[test]
    fn horizontal_vertical_and_finite_endcaps() {
        let from = Vec3::new(0., 0., 2.);
        let to = Vec3::new(20., 0., 2.);
        for reverse in [false, true] {
            let (a, b) = if reverse { (to, from) } else { (from, to) };
            contains(ball_sweep(a, b, 5., [10., 3.], 0.).unwrap().unwrap(), 3.);
            contains(ball_sweep(a, b, 5., [23., 0.], 0.).unwrap().unwrap(), 3.);
            assert!(ball_sweep(a, b, 5., [26., 0.], 0.).unwrap().is_none());
        }
        contains(ball_sweep(from, Vec3::new(0., 0., -3.), 5., [3., 0.], 0.).unwrap().unwrap(), -2.);
    }

    #[test]
    fn sloped_contact_uses_interior_pose_not_nearest_xy_or_lowest_endpoint() {
        // The lower envelope of a radius-5 sphere on z=x+5 is the parallel
        // plane z=x+5-5*sqrt(2), away from the finite segment's endcaps.
        let a = Vec3::ZERO;
        let b = Vec3::new(20., 0., 20.);
        let expected = 15. - 5. * 2_f64.sqrt();
        for (from, to) in [(a, b), (b, a)] {
            let bounds = ball_sweep(from, to, 5., [10., 0.], 0.).unwrap().unwrap();
            contains(bounds, expected);
            assert!(bounds.high.unwrap() - bounds.low < 1e-8);
        }
    }

    #[test]
    fn cusp_between_parallel_passes_has_analytic_height() {
        // Two parallel passes four mm apart, R=5: midpoint scallop 5-sqrt(21).
        let left = ball_sweep(Vec3::new(0., -2., 0.), Vec3::new(20., -2., 0.), 5., [10., 0.], 0.).unwrap().unwrap();
        let right = ball_sweep(Vec3::new(0., 2., 0.), Vec3::new(20., 2., 0.), 5., [10., 0.], 0.).unwrap().unwrap();
        contains(left, 5. - 21_f64.sqrt());
        contains(right, 5. - 21_f64.sqrt());
        // A wrong radius is a dimensionally detectable fault, not a harmless
        // change to path ordering or NC formatting.
        let wrong = ball_sweep(Vec3::new(0., -2., 0.), Vec3::new(20., -2., 0.), 4., [10., 0.], 0.).unwrap().unwrap();
        assert!(wrong.low - left.high.unwrap() > 0.1);
    }

    #[test]
    fn cell_intervals_cover_sloped_surface_and_refine() {
        let a = Vec3::ZERO;
        let b = Vec3::new(20., 0., 20.);
        let mut last_width = f64::INFINITY;
        for half in [0.1, 0.01, 0.001, 0.0001] {
            let bounds = ball_sweep(a, b, 5., [10., 0.], half).unwrap().unwrap();
            for k in 0..64 {
                let angle = std::f64::consts::TAU * k as f64 / 64.;
                let x = 10. + half * angle.cos();
                let y = half * angle.sin();
                // Cross-section perpendicular to travel has radius sqrt(25-y²).
                let exact = x + 5. - (2. * (25. - y * y)).sqrt();
                contains(bounds, exact);
            }
            let width = bounds.high.unwrap() - bounds.low;
            assert!(width < last_width);
            last_width = width;
        }
        assert!(last_width < crate::VERIFY_ERROR);
    }

    #[test]
    fn partial_coverage_and_tangencies_never_certify_whole_cell() {
        let bounds = ball_sweep(Vec3::ZERO, Vec3::ZERO, 5., [5., 0.], 0.1).unwrap().unwrap();
        assert!(bounds.high.is_none());
        let point = ball_sweep(Vec3::ZERO, Vec3::ZERO, 5., [5., 0.], 0.).unwrap().unwrap();
        assert!(point.high.is_none());
        let huge = ball_sweep(Vec3::ZERO, Vec3::ZERO, 5., [0., 0.], 10.).unwrap().unwrap();
        assert!(huge.high.is_none());
    }

    #[test]
    fn translations_nearly_vertical_moves_and_hostile_inputs() {
        let o = Vec3::new(900_000., -900_000., 900_000.);
        let bounds = ball_sweep(o, o + Vec3::new(20., 0., 20.), 5., [o.x + 10., o.y], 0.).unwrap().unwrap();
        contains(bounds, o.z + 15. - 5. * 2_f64.sqrt());
        assert!(bounds.high.unwrap() - bounds.low < crate::POST_ERROR);
        let near_vertical = ball_sweep(Vec3::ZERO, Vec3::new(1e-150, 0., -10.), 5., [0., 0.], 0.).unwrap().unwrap();
        contains(near_vertical, -10.);
        for bad in [f64::NAN, f64::INFINITY, -1., 0., 1e7] {
            assert!(ball_sweep(Vec3::ZERO, Vec3::ZERO, bad, [0., 0.], 0.).is_err());
        }
        assert!(ball_sweep(Vec3::ZERO, Vec3::ZERO, 5., [f64::NAN, 0.], 0.).is_err());
        assert!(ball_sweep(Vec3::ZERO, Vec3::ZERO, 5., [0., 0.], -0.1).is_err());
        assert!(ball_sweep(Vec3::new(f64::INFINITY, 0., 0.), Vec3::ZERO, 5., [0., 0.], 0.).is_err());
    }
}
