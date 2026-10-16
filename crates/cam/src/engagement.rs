//! Independent radial-engagement certificates for horizontal straight cutting.
//! This primitive does not plan paths or assume a preceding cut removed stock.
//! Callers must supply material intervals BEFORE the motion. Curved motion must
//! be segmented with conservative stock/motion error bounds by its caller.
pub mod program;
pub mod stock;
use crate::{Result, invalid, model};
use solidcraft_geom::Vec3;

/// A conservative height-field material column. Material from `bottom` to `low` is certain;
/// material above `high` is absent. XY coverage outside supplied columns is empty.
#[derive(Clone, Copy, Debug)]
pub struct Material {
    pub min: [f64; 2],
    pub max: [f64; 2],
    pub bottom: f64,
    pub low: f64,
    pub high: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    /// Witnessed engagement at the midpoint of the motion, radians.
    pub angle_low: f64,
    /// Upper bound at EVERY pose in the motion, radians.
    pub angle_high: f64,
    /// Equivalent radial immersion R*(1-cos(angle)), in millimetres.
    pub load_low: f64,
    pub load_high: f64,
}

/// Bound total engaged angle on the forward semicircle of a flat cutter. A full
/// slot is pi radians / one diameter; a straight side cut of width a is
/// acos(1-a/R). Disjoint engaged arcs are summed, never reduced to the largest.
///
/// `error` expands possible material and erodes definite material in XY and Z.
/// Vertical entry, changing depth and zero-length motion require different
/// engagement models and are explicitly rejected. Budget exhaustion is an error.
/// A call accepts at most 4096 local material columns. A spatial index may omit
/// columns proved outside the swept cutter, but unknown material must be included.
pub fn horizontal(stock: &[Material], from: Vec3, to: Vec3, radius: f64, error: f64, budget: usize) -> Result<Bounds> {
    horizontal_errors(stock, from, to, radius, error, error, budget)
}
// Parsed horizontal NC has exact constant Z. Arc chord uncertainty is lateral;
// manufacturing/machine uncertainty is deliberately outside this reference post.
pub(super) fn horizontal_errors(
    stock: &[Material],
    from: Vec3,
    to: Vec3,
    radius: f64,
    error: f64,
    height_error: f64,
    budget: usize,
) -> Result<Bounds> {
    model::point(from)?;
    model::point(to)?;
    if stock.len() > 4096
        || stock.len() > budget
        || !model::positive(radius)
        || !model::finite(error)
        || error < 0.
        || !model::finite(height_error)
        || height_error < 0.
        || from.z != to.z
        || from.x == to.x && from.y == to.y
    {
        return invalid("engagement needs bounded material, a horizontal nonzero motion, positive radius and a sufficient work budget");
    }
    let direction = (to.y - from.y).atan2(to.x - from.x);
    let mid = [(from.x + to.x) / 2., (from.y + to.y) / 2.];
    let mut upper = Vec::new();
    let mut lower = Vec::new();
    for cell in stock {
        model::point(Vec3::new(cell.min[0], cell.min[1], cell.low))?;
        model::point(Vec3::new(cell.max[0], cell.max[1], cell.high))?;
        if cell.min[0] >= cell.max[0]
            || cell.min[1] >= cell.max[1]
            || cell.low > cell.high
            || !model::finite(cell.bottom)
            || cell.bottom > cell.low
            || from.z < cell.bottom
        {
            return invalid("invalid engagement material column");
        }
        let extent = [from.x, from.y, from.z, to.x, to.y, radius, cell.low, cell.high]
            .into_iter()
            .chain(cell.min)
            .chain(cell.max)
            .map(f64::abs)
            .fold(1_f64, f64::max);
        let padding = error + 1024. * f64::EPSILON * extent;
        if cell.high + height_error > from.z {
            // The rectangle minus every tool-center position is contained in
            // this rectangle. Exact for X/Y moves, conservative on diagonals.
            let min = [cell.min[0] - from.x.max(to.x) - padding, cell.min[1] - from.y.max(to.y) - padding];
            let max = [cell.max[0] - from.x.min(to.x) + padding, cell.max[1] - from.y.min(to.y) + padding];
            angular_intervals(min, max, radius, direction, &mut upper);
        }
        if cell.low - height_error > from.z {
            let min = [cell.min[0] - mid[0] + padding, cell.min[1] - mid[1] + padding];
            let max = [cell.max[0] - mid[0] - padding, cell.max[1] - mid[1] - padding];
            if min[0] < max[0] && min[1] < max[1] {
                // The leading circumference has not been swept by an earlier
                // pose on this same straight motion, so midpoint stock is a
                // valid lower witness despite removal behind the cutter.
                angular_intervals(min, max, radius, direction, &mut lower);
            }
        }
    }
    let angle_error = 8192. * f64::EPSILON * (stock.len() + 1) as f64;
    let no_possible_material = upper.is_empty();
    let angle_low = (union_length(lower) - angle_error).max(0.);
    let angle_high = if no_possible_material { 0. } else { (union_length(upper) + angle_error).min(std::f64::consts::PI) };
    if angle_low > angle_high + 1e-10 {
        return invalid("engagement interval is numerically unresolved");
    }
    Ok(Bounds { angle_low, angle_high, load_low: radius * (1. - angle_low.cos()), load_high: radius * (1. - angle_high.cos()) })
}

fn angular_intervals(min: [f64; 2], max: [f64; 2], radius: f64, direction: f64, out: &mut Vec<[f64; 2]>) {
    use std::f64::consts::{FRAC_PI_2, PI, TAU};
    let mut cuts = vec![0., TAU];
    let mut add = |a: f64| cuts.push(a.rem_euclid(TAU));
    add(direction - FRAC_PI_2);
    add(direction + FRAC_PI_2);
    for x in [min[0], max[0]] {
        if x.abs() <= radius {
            let a = (x / radius).acos();
            add(a);
            add(-a);
        }
    }
    for y in [min[1], max[1]] {
        if y.abs() <= radius {
            let a = (y / radius).asin();
            add(a);
            add(PI - a);
        }
    }
    cuts.sort_unstable_by(f64::total_cmp);
    cuts.dedup();
    for pair in cuts.windows(2) {
        let middle = (pair[0] + pair[1]) / 2.;
        let (x, y) = (radius * middle.cos(), radius * middle.sin());
        if x >= min[0] && x <= max[0] && y >= min[1] && y <= max[1] && (middle - direction).cos() >= 0. {
            out.push([pair[0], pair[1]]);
        }
    }
}

fn union_length(mut intervals: Vec<[f64; 2]>) -> f64 {
    intervals.sort_unstable_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    let mut end = 0_f64;
    let mut total = 0.;
    for [a, b] in intervals {
        total += (b - a.max(end)).max(0.);
        end = end.max(b);
    }
    total.min(std::f64::consts::PI)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    fn column(min: [f64; 2], max: [f64; 2]) -> Material {
        Material { min, max, bottom: 0., low: 5., high: 5. }
    }
    #[test]
    fn straight_side_cut_and_full_slot_match_hand_calculated_immersion() {
        let from = Vec3::new(-5., 0., 1.);
        let to = Vec3::new(5., 0., 1.);
        for immersion in [0.1, 0.4, 1., 2.] {
            let material = [column([-20., 1. - immersion], [20., 20.])];
            let b = horizontal(&material, from, to, 1., 0., 10).unwrap();
            assert!((b.load_low - immersion).abs() < 1e-8 && (b.load_high - immersion).abs() < 1e-8, "{b:?}");
            let reversed = horizontal(&material, to, from, 1., 0., 10).unwrap();
            assert!((reversed.load_high - b.load_high).abs() < 1e-8);
        }
    }
    #[test]
    fn separate_cutting_arcs_accumulate_without_double_counting_overlaps() {
        let left = column([-20., 0.8], [20., 20.]);
        let right = column([-20., -20.], [20., -0.8]);
        let from = Vec3::new(-2., 0., 1.);
        let to = Vec3::new(2., 0., 1.);
        let b = horizontal(&[left, right, left], from, to, 1., 0., 10).unwrap();
        let expected = 2. * 0.8_f64.acos();
        assert!((b.angle_low - expected).abs() < 1e-8 && (b.angle_high - expected).abs() < 1e-8);
    }
    #[test]
    fn a_thin_island_between_endpoints_is_included_in_the_whole_motion_bound() {
        let material = [column([4., -20.], [4.01, 20.])];
        let bounds = horizontal(&material, Vec3::new(0., 0., 1.), Vec3::new(10., 0., 1.), 1., 0., 10).unwrap();
        assert_eq!(bounds.load_low, 0.); // Midpoint has already passed the island.
        assert!((bounds.load_high - 2.).abs() < 1e-10);
    }
    #[test]
    fn uncertainty_and_missing_material_proofs_never_reduce_the_upper_bound() {
        let mut material = column([-20., 0.6], [20., 20.]);
        let from = Vec3::new(-2., 0., 1.);
        let to = Vec3::new(2., 0., 1.);
        let exact = horizontal(&[material], from, to, 1., 0., 10).unwrap();
        let uncertain = horizontal(&[material], from, to, 1., 0.0025, 10).unwrap();
        assert!(uncertain.load_low < exact.load_low && uncertain.load_high > exact.load_high);
        material.low = 0.;
        let unknown = horizontal(&[material], from, to, 1., 0., 10).unwrap();
        assert_eq!(unknown.load_low, 0.);
        assert!(unknown.load_high >= 0.4);
        assert!(horizontal(&[material], from, to, 1., 0., 0).is_err());
        assert!(horizontal(&[material], from, to + Vec3::Z, 1., 0., 10).is_err());
        assert!(horizontal(&[material], from, from, 1., 0., 10).is_err());
        assert!(horizontal(&[material], from, to, f64::NAN, 0., 10).is_err());
    }
}
