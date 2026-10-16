//! Meshing adapter: spline bases have compact support. Skipping exactly zero
//! weights preserves the surface while avoiding work proportional to its entire
//! control net at every interior grid point.
use solvecraft_geom::Vec3;
use truck_modeling::{self as mt, *};

#[derive(Clone)]
pub(crate) struct Surface(pub mt::Surface);

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
        let mt::Surface::BSplineSurface(s) = &self.0 else { return self.0.normal(u, v) };
        let (du, dv) = (self.uder(u, v), self.vder(u, v));
        let normal = du.cross(dv);
        let (ur, vr) = s.range_tuple();
        let boundary = u.is_finite() && v.is_finite() && (u == ur.0 || u == ur.1 || v == vr.0 || v == vr.1);
        // Coincident endpoint controls can differ by rounding after placement.
        // Their tiny nonzero derivative is not a meaningful limiting normal.
        let roundoff = if boundary { self.subs(u, v).to_vec().magnitude().max(1.0) * f64::EPSILON * 8192.0 } else { 0.0 };
        let singular = du.magnitude2() <= roundoff * roundoff || dv.magnitude2() <= roundoff * roundoff;
        if !singular && normal.magnitude2() > 0.0 && normal.magnitude2().is_finite() {
            return normal.normalize();
        }
        // At an isolated singular corner the surface has directional limiting
        // normals. Use the interior limit for its display vertex; a surface
        // that remains degenerate still returns the original invalid normal.
        if boundary {
            let inward = |t: f64, (a, b): (f64, f64)| (t + if t < (a + b) * 0.5 { 1e-3 * (b - a) } else { -1e-3 * (b - a) }).clamp(a, b);
            let (u, v) = (inward(u, ur), inward(v, vr));
            let limit = self.uder(u, v).cross(self.vder(u, v));
            if limit.magnitude2() > 0.0 && limit.magnitude2().is_finite() {
                return limit.normalize();
            }
        }
        normal.normalize()
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
        if matches!(&self.0, mt::Surface::BSplineSurface(s) if s.degrees() == (5, 5)) {
            // Robust triangulation tries this exact search before the nearest
            // search. Both must use the bounded solver: an unconstrained root
            // outside a singular patch can corrupt its boundary triangulation.
            let (u, v) = self.search_nearest_parameter(p, hint, trials)?;
            return self.subs(u, v).near(&p).then_some((u, v));
        }
        self.0.search_parameter(p, hint, trials)
    }
}
impl SearchNearestParameter<D2> for Surface {
    type Point = mt::Point3;
    fn search_nearest_parameter<H: Into<SPHint2D>>(&self, p: mt::Point3, hint: H, trials: usize) -> Option<(f64, f64)> {
        let hint = hint.into();
        let mt::Surface::BSplineSurface(s) = &self.0 else { return self.0.search_nearest_parameter(p, hint, trials) };
        if s.degrees() != (5, 5) {
            return self.0.search_nearest_parameter(p, hint, trials);
        }
        if !p.x.is_finite() || !p.y.is_finite() || !p.z.is_finite() {
            return None;
        }
        let (ur, vr) = s.range_tuple();
        let boundary_tol = (p.to_vec().magnitude().max(1.0) * f64::EPSILON * 128.0).max(1e-12);
        // At a cubic pole, indistinguishable points can have very different
        // parameters. Prefer the exact endpoint and its stable limiting normal.
        for u in [ur.0, ur.1] {
            for v in [vr.0, vr.1] {
                if self.subs(u, v).distance2(p) <= boundary_tol * boundary_tol {
                    return Some((u, v));
                }
            }
        }
        let clamp = |(u, v): (f64, f64)| (u.clamp(ur.0, ur.1), v.clamp(vr.0, vr.1));
        let improve = |(mut u, mut v): (f64, f64)| {
            for _ in 0..trials.min(100) {
                let q = self.subs(u, v) - p;
                let du = self.uder(u, v);
                let dv = self.vder(u, v);
                let a = du.magnitude2();
                let b = du.dot(dv);
                let c = dv.magnitude2();
                let det = a * c - b * b;
                if !det.is_finite() || det <= 1e-24 * a * c {
                    break;
                }
                let su = (c * du.dot(q) - b * dv.dot(q)) / det;
                let sv = (a * dv.dot(q) - b * du.dot(q)) / det;
                if !su.is_finite() || !sv.is_finite() {
                    break;
                }
                let old = q.magnitude2();
                let mut scale = 1.0;
                let mut next = (u, v);
                for _ in 0..20 {
                    let candidate = clamp((u - scale * su, v - scale * sv));
                    if self.subs(candidate.0, candidate.1).distance2(p) < old {
                        next = candidate;
                        break;
                    }
                    scale *= 0.5;
                }
                let delta = (next.0 - u).abs() + (next.1 - v).abs();
                (u, v) = next;
                if delta < 1e-13 {
                    break;
                }
            }
            (u, v)
        };
        let seed = match hint {
            SPHint2D::Parameter(u, v) if u.is_finite() && v.is_finite() => clamp((u, v)),
            _ => mt::algo::surface::presearch(self, p, (ur, vr), 12),
        };
        let first = Some(improve(seed));
        let valid = |(u, v): (f64, f64)| u.is_finite() && v.is_finite() && u >= ur.0 && u <= ur.1 && v >= vr.0 && v <= vr.1;
        let distance = |(u, v)| self.subs(u, v).distance2(p);
        let mut best = first.filter(|uv| valid(*uv));
        let mut consider = |uv| {
            if valid(uv) && distance(uv).is_finite() && best.is_none_or(|old| distance(uv) < distance(old)) {
                best = Some(uv);
            }
        };
        if !matches!(hint, SPHint2D::None) {
            consider(improve(mt::algo::surface::presearch(self, p, (ur, vr), 12)));
        }
        // Prefer an incident boundary over a numerically equivalent interior
        // root. Otherwise roundoff can turn a straight parameter boundary into
        // a thin zigzag, producing overlapping sliver triangles along a seam.
        let mut boundary_best = None;
        let mut boundary = |uv| {
            consider(uv);
            if valid(uv) && distance(uv) <= boundary_tol * boundary_tol && boundary_best.is_none_or(|old| distance(uv) < distance(old)) {
                boundary_best = Some(uv);
            }
        };
        // A singular boundary can trap Newton's iteration. Search its four
        // regular one-dimensional boundary curves as independent candidates.
        if s.is_clamped() {
            let rows = s.control_points().len();
            let columns = s.control_points().first().map_or(0, Vec::len);
            if let (Some(last_u), Some(last_v)) = (rows.checked_sub(1), columns.checked_sub(1)) {
                for (curve, along_u, fixed) in [
                    (s.row_curve(0), true, vr.0),
                    (s.row_curve(last_v), true, vr.1),
                    (s.column_curve(0), false, ur.0),
                    (s.column_curve(last_u), false, ur.1),
                ] {
                    let (a, b) = curve.range_tuple();
                    let uv = |t| if along_u { (t, fixed) } else { (fixed, t) };
                    boundary(uv(a));
                    boundary(uv(b));
                    if let Some(mut t) = curve.search_nearest_parameter(p, None, trials) {
                        // Polish the one-dimensional boundary solve: the generic
                        // Newton stopping threshold is too coarse at a flat pole.
                        for _ in 0..16 {
                            let d = curve.der(t);
                            let norm = d.magnitude2();
                            if !norm.is_finite() || norm <= 0.0 {
                                break;
                            }
                            let next = (t - (curve.subs(t) - p).dot(d) / norm).clamp(a, b);
                            if !next.is_finite() {
                                break;
                            }
                            let delta = (next - t).abs();
                            t = next;
                            if delta <= 1e-14 * (b - a) {
                                break;
                            }
                        }
                        boundary(uv(t));
                    }
                }
            }
        }
        boundary_best.or(best)
    }
}

pub(crate) fn display_projection(surface: &mt::BSplineSurface<mt::Point3>, mesh_normals: &[mt::Vector3]) -> Option<Vec3> {
    if surface.degrees() != (5, 5) || surface.control_points().len() > 64 || surface.control_points().iter().any(|r| r.len() > 64) {
        return None;
    }
    let origin = *surface.control_points().first()?.first()?;
    let local = mt::BSplineSurface::try_new(
        (surface.uknot_vec().clone(), surface.vknot_vec().clone()),
        surface
            .control_points()
            .iter()
            .map(|row| row.iter().map(|p| mt::Point3::new(p.x - origin.x, p.y - origin.y, p.z - origin.z)).collect())
            .collect(),
    )
    .ok()?;
    let (ur, vr) = local.range_tuple();
    if ur != (0.0, 1.0) || vr != (0.0, 1.0) {
        return None;
    }
    let evaluated = crate::body::evaluation::Surface(mt::Surface::BSplineSurface(local));
    let mut normals = Vec::new();
    for i in 0..=16 {
        for j in 0..=16 {
            if (i == 0 || i == 16) && (j == 0 || j == 16) {
                continue;
            }
            let n = evaluated.normal(i as f64 / 16.0, j as f64 / 16.0);
            if let Some(n) = Vec3::new(n.x, n.y, n.z).normalized() {
                normals.push(n);
            }
        }
    }
    normals.extend(mesh_normals.iter().filter_map(|n| Vec3::new(n.x, n.y, n.z).normalized()));
    let mut direction = normals.iter().copied().fold(Vec3::ZERO, |a, b| a + b).normalized()?;
    // This only chooses how to connect existing display vertices; construction
    // still requires its independent Jacobian and containment certificates.
    for _ in 0..1024 {
        let worst = normals.iter().min_by(|a, b| a.dot(direction).total_cmp(&b.dot(direction)))?;
        let dot = worst.dot(direction);
        if dot >= 0.01 {
            break;
        }
        direction = (direction + *worst * (0.011 - dot)).normalized()?;
    }
    normals.iter().all(|n| n.dot(direction) >= 0.01).then_some(direction)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projected_display_mesh_removes_crossing_diagonals_without_moving_vertices() {
        use truck_meshalgo::prelude::{Faces, PolygonMesh, StandardAttributes, retriangulate_projected};
        let positions = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0), (0.8, 0.7), (0.2, 0.3)]
            .into_iter()
            .map(|(x, y)| mt::Point3::new(x, y, 0.0))
            .collect::<Vec<_>>();
        // Two interior vertices exchange sides in projection. The original
        // parameter diagonals overlap, although the surface itself is flat.
        let triangles = [[0, 1, 5], [0, 5, 4], [0, 4, 3], [1, 2, 5], [2, 4, 5], [2, 3, 4]];
        let mesh = PolygonMesh::new(
            StandardAttributes { positions, normals: vec![mt::Vector3::unit_z(); 6], uv_coords: vec![mt::Vector2::new(0.0, 0.0); 6] },
            Faces::from_tri_and_quad_faces(triangles.into_iter().map(|t| t.map(|i| [i, i, i].into())).collect(), Vec::new()),
        );
        let fixed = retriangulate_projected(&mesh, mt::Vector3::unit_z()).unwrap();
        assert_eq!(fixed.positions(), mesh.positions());
        assert_eq!(fixed.normals(), mesh.normals());
        let mut area = 0.0;
        for triangle in fixed.faces().triangle_iter() {
            let [a, b, c] = triangle.map(|v| fixed.positions()[v.pos]);
            let signed = (b - a).cross(c - a).z * 0.5;
            assert!(signed > 0.0);
            area += signed;
        }
        assert!((area - 1.0).abs() < 1e-12);
        assert!(retriangulate_projected(&mesh, -mt::Vector3::unit_z()).is_none());
        assert!(retriangulate_projected(&mesh, mt::Vector3::zero()).is_none());
    }

    #[test]
    fn bounded_projection_recovers_from_a_singular_or_outside_hint() {
        // x=u³, y=v³: the origin is stationary in both parameters.
        let cubic = [0.0, 0.0, 0.0, 0.1, 0.4, 1.0];
        let ctrl = cubic.iter().map(|x| cubic.iter().map(|y| mt::Point3::new(*x, *y, 0.0)).collect()).collect();
        let knots = mt::KnotVec::bezier_knot(5);
        let surface = Surface(mt::Surface::BSplineSurface(mt::BSplineSurface::new((knots.clone(), knots), ctrl)));
        for p in [mt::Point3::new(0.0, 0.5, 0.0), mt::Point3::new(0.125, 0.343, 0.0), mt::Point3::new(0.0, 0.0, 0.0)] {
            for hint in [(0.0, 0.0), (9.0, -9.0)] {
                let (u, v) = surface.search_nearest_parameter(p, hint, 100).unwrap();
                assert!((0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v));
                assert!(surface.subs(u, v).distance(p) < 1e-10);
                let (u, v) = surface.search_parameter(p, hint, 100).unwrap();
                assert!((0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v));
                assert!(surface.subs(u, v).distance(p) < 1e-10);
            }
        }
        assert!(surface.search_parameter(mt::Point3::new(-0.125, 0.5, 0.0), None, 100).is_none());
        assert_eq!(surface.search_nearest_parameter(mt::Point3::new(1e-14, 1e-14, 0.0), (0.001, 0.001), 100), Some((0.0, 0.0)));
        assert!(surface.search_nearest_parameter(mt::Point3::new(f64::NAN, 0.0, 0.0), None, 100).is_none());
    }

    #[test]
    fn interior_fold_does_not_acquire_a_fallback_normal() {
        let knots = mt::KnotVec::bezier_knot(2);
        let ctrl = [0.25, -0.25, 0.25].into_iter().map(|x| (0..3).map(|j| mt::Point3::new(x, j as f64 / 2.0, 0.0)).collect()).collect();
        let s = Surface(mt::Surface::BSplineSurface(mt::BSplineSurface::new((knots.clone(), knots), ctrl)));
        assert!(!s.normal(0.5, 0.5).magnitude2().is_finite());
    }

    #[test]
    fn singular_corner_uses_a_finite_interior_normal() {
        let knots = mt::KnotVec::bezier_knot(2);
        let ctrl =
            (0..3).map(|i| (0..3).map(|j| mt::Point3::new(if i == 2 { 1.0 } else { 0.0 }, if j == 2 { 1.0 } else { 0.0 }, 0.0)).collect()).collect();
        let surface = Surface(mt::Surface::BSplineSurface(mt::BSplineSurface::new((knots.clone(), knots), ctrl)));
        assert_eq!(surface.normal(0.0, 0.0), mt::Vector3::new(0.0, 0.0, 1.0));
        assert_eq!(surface.normal(0.0, 0.5), mt::Vector3::new(0.0, 0.0, 1.0));
    }

    #[test]
    fn placed_singular_corner_ignores_control_roundoff() {
        let cubic = [0.0, 0.0, 0.0, 0.1, 0.4, 1.0];
        let mut ctrl: Vec<Vec<_>> = cubic.iter().map(|x| cubic.iter().map(|y| mt::Point3::new(100.0 + x, 200.0 + y, 700.0)).collect()).collect();
        ctrl[1][0].x += 1e-12;
        ctrl[0][1].z += 1e-12;
        let knots = mt::KnotVec::bezier_knot(5);
        let surface = Surface(mt::Surface::BSplineSurface(mt::BSplineSurface::new((knots.clone(), knots), ctrl)));
        assert!(surface.normal(0.0, 0.0).dot(mt::Vector3::unit_z()) > 0.99999);
    }
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
