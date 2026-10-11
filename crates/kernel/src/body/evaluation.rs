//! Meshing adapter: spline bases have compact support. Skipping exactly zero
//! weights preserves the surface while avoiding work proportional to its entire
//! control net at every interior grid point.
use truck_modeling::{self as mt, *};

#[derive(Clone)]
pub(super) struct Surface(pub mt::Surface);

fn derivative(s: &mt::BSplineSurface<mt::Point3>, u: f64, v: f64, along_u: bool) -> mt::Vector3 {
    let (ku, kv) = s.knot_vecs();
    let (pu, pv) = s.degrees();
    let (knots, degree, t, other_knots, other_degree, other_t) = if along_u { (ku, pu, u, kv, pv, v) } else { (kv, pv, v, ku, pu, u) };
    let mut result = mt::Vector3::new(0.0, 0.0, 0.0);
    let Some(lower_degree) = degree.checked_sub(1) else { return result };
    let other: Vec<_> = other_knots.bspline_basis_functions(other_degree, other_t).into_iter().enumerate().filter(|(_, b)| *b != 0.0).collect();
    let point = |i: usize, j: usize| {
        let (i, j) = if along_u { (i, j) } else { (j, i) };
        s.control_points().get(i).and_then(|row| row.get(j)).copied().unwrap_or(mt::Point3::new(0.0, 0.0, 0.0))
    };
    for (i, b) in knots.bspline_basis_functions(lower_degree, t).into_iter().enumerate() {
        if b == 0.0 {
            continue;
        }
        let (Some(a), Some(end)) = (knots.get(i), knots.get(i + degree)) else { continue };
        let den = end - a;
        // Match truck's derivative convention at numerically collapsed spans.
        if den.so_small() {
            continue;
        }
        for (j, other) in &other {
            let previous = i.checked_sub(1).map(|i| point(i, *j)).unwrap_or(mt::Point3::new(0.0, 0.0, 0.0));
            result += (point(i, *j) - previous) * (b * other / den);
        }
    }
    result * degree as f64
}

impl ParametricSurface for Surface {
    type Point = mt::Point3;
    type Vector = mt::Vector3;
    fn subs(&self, u: f64, v: f64) -> mt::Point3 {
        let mt::Surface::BSplineSurface(s) = &self.0 else { return self.0.subs(u, v) };
        let (ku, kv) = s.knot_vecs();
        let (pu, pv) = s.degrees();
        let bu = ku.bspline_basis_functions(pu, u);
        let bv: Vec<_> = kv.bspline_basis_functions(pv, v).into_iter().enumerate().filter(|(_, b)| *b != 0.0).collect();
        let mut point = mt::Point3::new(0.0, 0.0, 0.0);
        for (row, a) in s.control_points().iter().zip(bu) {
            if a == 0.0 {
                continue;
            }
            for (j, b) in &bv {
                if let Some(p) = row.get(*j) {
                    point.x += p.x * (a * b);
                    point.y += p.y * (a * b);
                    point.z += p.z * (a * b);
                }
            }
        }
        point
    }
    fn uder(&self, u: f64, v: f64) -> mt::Vector3 {
        match &self.0 {
            mt::Surface::BSplineSurface(s) => derivative(s, u, v, true),
            _ => self.0.uder(u, v),
        }
    }
    fn vder(&self, u: f64, v: f64) -> mt::Vector3 {
        match &self.0 {
            mt::Surface::BSplineSurface(s) => derivative(s, u, v, false),
            _ => self.0.vder(u, v),
        }
    }
    fn uuder(&self, u: f64, v: f64) -> mt::Vector3 {
        self.0.uuder(u, v)
    }
    fn uvder(&self, u: f64, v: f64) -> mt::Vector3 {
        self.0.uvder(u, v)
    }
    fn vvder(&self, u: f64, v: f64) -> mt::Vector3 {
        self.0.vvder(u, v)
    }
    fn parameter_range(&self) -> (ParameterRange, ParameterRange) {
        self.0.parameter_range()
    }
    fn u_period(&self) -> Option<f64> {
        self.0.u_period()
    }
    fn v_period(&self) -> Option<f64> {
        self.0.v_period()
    }
}
impl ParametricSurface3D for Surface {
    fn normal(&self, u: f64, v: f64) -> mt::Vector3 {
        if matches!(self.0, mt::Surface::BSplineSurface(_)) { self.uder(u, v).cross(self.vder(u, v)).normalize() } else { self.0.normal(u, v) }
    }
}
impl ParameterDivision2D for Surface {
    fn parameter_division(&self, range: ((f64, f64), (f64, f64)), tol: f64) -> (Vec<f64>, Vec<f64>) {
        self.0.parameter_division(range, tol)
    }
}
impl SearchParameter<D2> for Surface {
    type Point = mt::Point3;
    fn search_parameter<H: Into<SPHint2D>>(&self, p: mt::Point3, hint: H, trials: usize) -> Option<(f64, f64)> {
        self.0.search_parameter(p, hint, trials)
    }
}
impl SearchNearestParameter<D2> for Surface {
    type Point = mt::Point3;
    fn search_nearest_parameter<H: Into<SPHint2D>>(&self, p: mt::Point3, hint: H, trials: usize) -> Option<(f64, f64)> {
        self.0.search_nearest_parameter(p, hint, trials)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sparse_evaluation_preserves_knots_and_domain_endpoints() {
        let knots = mt::KnotVec::from(vec![0.0, 0.0, 0.0, 0.0, 0.2, 0.4, 0.4, 0.7, 1.0, 1.0, 1.0, 1.0]);
        let control = (0..8).map(|i| (0..8).map(|j| mt::Point3::new(i as f64, j as f64, ((i * 7 + j) as f64).sin())).collect()).collect();
        let surface = Surface(mt::Surface::BSplineSurface(mt::BSplineSurface::new((knots.clone(), knots), control)));
        for u in [0.0, 0.13, 0.2, 0.4, 0.57, 0.7, 1.0] {
            for v in [0.0, 0.13, 0.2, 0.4, 0.57, 0.7, 1.0] {
                assert_eq!(surface.subs(u, v), surface.0.subs(u, v));
                assert!((surface.uder(u, v) - surface.0.uder(u, v)).magnitude() < 1e-12);
                assert!((surface.vder(u, v) - surface.0.vder(u, v)).magnitude() < 1e-12);
                assert!((surface.normal(u, v) - surface.0.normal(u, v)).magnitude() < 1e-12);
            }
        }
    }

    #[test]
    fn collapsed_knot_spans_keep_the_original_derivative_convention() {
        let knots = mt::KnotVec::from(vec![0.0, 0.0, 0.0, 0.0, 1e-8, 0.4, 0.4, 0.7, 1.0, 1.0, 1.0, 1.0]);
        let control = (0..8).map(|i| (0..8).map(|j| mt::Point3::new(i as f64, j as f64, ((i * 7 + j) as f64).sin())).collect()).collect();
        let surface = Surface(mt::Surface::BSplineSurface(mt::BSplineSurface::new((knots.clone(), knots), control)));
        for u in [0.0, 0.5e-8, 1e-8, 0.2, 1.0] {
            for v in [0.0, 0.5e-8, 1e-8, 0.2, 1.0] {
                assert_eq!(surface.subs(u, v), surface.0.subs(u, v));
                assert!((surface.uder(u, v) - surface.0.uder(u, v)).magnitude() < 1e-12);
                assert!((surface.vder(u, v) - surface.0.vder(u, v)).magnitude() < 1e-12);
            }
        }
    }
}
