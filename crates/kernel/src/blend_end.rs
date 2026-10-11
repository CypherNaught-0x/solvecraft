//! Straight fillets trimmed by the fan of faces at each endpoint.
//! The cylinder stays circular; oblique planes cut ellipses, not tilted circles.

use crate::blend::{plane_normal, rebuild_face, unsupported, vtx};
use crate::body::{Solid, from_p3, p3};
use crate::{KernelError, Result};
use solvecraft_geom::Vec3;
use std::collections::HashMap;
use truck_modeling::{self as mt, BoundedCurve, Cut, ParameterDivision1D, ParametricCurve, ParametricSurface3D, SearchNearestParameter, builder};

fn h(p: Vec3, w: f64) -> mt::Vector4 {
    mt::Vector4::new(p.x * w, p.y * w, p.z * w, w)
}
fn conic(a: Vec3, m: Vec3, b: Vec3, w: f64) -> mt::Curve {
    mt::Curve::NurbsCurve(mt::NurbsCurve::new(mt::BSplineCurve::new(mt::KnotVec::bezier_knot(2), vec![h(a, 1.0), h(m, w), h(b, 1.0)])))
}
fn trim(mut c: mt::Curve, lo: f64, hi: f64) -> mt::Curve {
    let (a, b) = c.range_tuple();
    if hi < b {
        let _ = c.cut(hi);
    }
    if lo > a {
        c = c.cut(lo);
    }
    c
}

/// First crossing of zero along a seam, short of either original endpoint.
fn crossing(c: &mt::Curve, f: impl Fn(Vec3) -> f64, tol: f64) -> Result<(f64, Vec3)> {
    let (a, b) = c.range_tuple();
    let mut prev = (a, f(from_p3(c.subs(a))));
    for i in 1..=128 {
        let t = a + (b - a) * i as f64 / 128.0;
        let y = f(from_p3(c.subs(t)));
        if !y.is_finite() {
            return Err(unsupported("an invalid fillet intersection"));
        }
        if prev.1 * y <= 0.0 {
            let (mut lo, mut hi) = (prev.0, t);
            for _ in 0..60 {
                let mid = (lo + hi) * 0.5;
                if f(from_p3(c.subs(mid))) * prev.1 > 0.0 {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let t = (lo + hi) * 0.5;
            let q = from_p3(c.subs(t));
            if q.dist(from_p3(c.subs(a))) <= tol || q.dist(from_p3(c.subs(b))) <= tol {
                return Err(unsupported("a fillet reaching the next vertex"));
            }
            return Ok((t, q));
        }
        prev = (t, y);
    }
    Err(unsupported("the fillet is larger than its neighbouring faces"))
}

/// A plane cuts an exact elliptical section out of the circular cylinder.
fn on_end(f: &mt::Face, q: Vec3, d: Vec3, p: Vec3) -> Result<Vec3> {
    let n = plane_normal(f).ok_or_else(|| unsupported("this fillet needs rebuilding where it meets an earlier curved blend"))?;
    let den = n.dot(d);
    if den.abs() < 1e-8 {
        return Err(unsupported("an end face parallel to the fillet"));
    }
    Ok(q - d * (n.dot(q - p) / den))
}

/// Topological closure alone does not detect a cap crossing another boundary or a hole.
/// Check the rebuilt planar loops, including boundaries away from the old endpoint.
fn uncrossed(f: &mt::Face, tol: f64) -> Result<()> {
    let Some(n) = plane_normal(f) else { return Ok(()) };
    let mut segments = Vec::new();
    for (wi, wire) in f.boundaries().iter().enumerate() {
        let mut points = Vec::new();
        for e in wire.edge_iter() {
            let c = e.oriented_curve();
            let mut ps = c.parameter_division(c.range_tuple(), tol.max(1e-6)).1;
            ps.pop(); // the next edge supplies the shared endpoint
            points.extend(ps.into_iter().map(from_p3));
        }
        if points.len() < 3 || points.len() + segments.len() > 8192 {
            return Err(unsupported("cannot validate this fillet boundary within the segment limit"));
        }
        for (i, a) in points.iter().enumerate() {
            let b = points.get((i + 1) % points.len()).ok_or_else(|| unsupported("empty fillet boundary"))?;
            segments.push((wi, i, points.len(), *a, *b));
        }
    }
    for (i, (wi, ei, count, a, b)) in segments.iter().enumerate() {
        for (wj, ej, _, c, d) in segments.iter().skip(i + 1) {
            if wi == wj && (ej - ei == 1 || ej - ei + 1 == *count) {
                continue;
            }
            let (ab, cd) = (*b - *a, *d - *c);
            let (sa, sb) = (ab.cross(*c - *a).dot(n), ab.cross(*d - *a).dot(n));
            let (sc, sd) = (cd.cross(*a - *c).dot(n), cd.cross(*b - *c).dot(n));
            let crosses = sa * sb < 0.0 && sc * sd < 0.0;
            let touches =
                a.dist_to_segment(*c, *d).min(b.dist_to_segment(*c, *d)).min(c.dist_to_segment(*a, *b)).min(d.dist_to_segment(*a, *b)) <= tol;
            if crosses || touches {
                return Err(unsupported("the fillet crosses another face boundary"));
            }
        }
    }
    Ok(())
}

pub(crate) fn can_trim(solid: &Solid, edge: &mt::Edge) -> bool {
    matches!(edge.curve(), mt::Curve::Line(_))
        && solid.face_iter().filter(|f| f.edge_iter().any(|e| e.id() == edge.id())).all(|f| plane_normal(f).is_some())
}

pub(crate) fn fillet_one(solid: &Solid, edge: &mt::Edge, size: f64, r: f64) -> Result<Solid> {
    if !matches!(edge.curve(), mt::Curve::Line(_)) {
        return Err(unsupported("an endpoint-trimmed fillet needs a straight edge"));
    }
    let (si, sh) = solid
        .boundaries()
        .iter()
        .enumerate()
        .find(|(_, s)| s.edge_iter().any(|e| e.id() == edge.id()))
        .ok_or_else(|| unsupported("missing fillet edge"))?;
    let faces: Vec<_> = sh.face_iter().collect();
    let on =
        |id: mt::EdgeID| -> Vec<usize> { faces.iter().enumerate().filter(|(_, f)| f.edge_iter().any(|e| e.id() == id)).map(|(i, _)| i).collect() };
    let fs = on(edge.id());
    let [fa, fb] = fs[..] else {
        return Err(unsupported("a non-manifold edge"));
    };
    let (Some(f1), Some(f2)) = (faces.get(fa), faces.get(fb)) else {
        return Err(unsupported("missing side faces"));
    };
    let (Some(n1), Some(n2)) = (plane_normal(f1), plane_normal(f2)) else {
        return Err(unsupported("fillet side faces must be planar"));
    };
    let into = |f: &mt::Face, n: Vec3| -> Option<Vec3> {
        let e = f.boundary_iters().into_iter().flatten().find(|e| e.id() == edge.id())?;
        n.cross(vtx(e.back()) - vtx(e.front())).normalized()
    };
    let (Some(t1), Some(t2)) = (into(f1, n1), into(f2, n2)) else {
        return Err(unsupported("fillet side directions"));
    };
    let sign = if n1.dot(t2) > 1e-6 && n2.dot(t1) > 1e-6 {
        1.0
    } else if n1.dot(t2) < -1e-6 && n2.dot(t1) < -1e-6 {
        -1.0
    } else {
        return Err(unsupported("tangent or inconsistent fillet side faces"));
    };
    let (v0, v1) = (edge.absolute_front(), edge.absolute_back());
    let (p0, p1) = (vtx(v0), vtx(v1));
    let d = (p1 - p0).normalized().ok_or_else(|| unsupported("zero length fillet"))?;
    let alpha = t1.dot(t2).clamp(-1.0, 1.0).acos();
    if !(alpha > 1e-3 && alpha < std::f64::consts::PI - 1e-3) {
        return Err(unsupported("the faces at the edge are tangent"));
    }
    let s = r / (alpha * 0.5).tan();
    let w = (alpha * 0.5).sin();
    let tol = (size * 1e-8).max(1e-7);
    // Extensions through a reflex endpoint must not escape the part. The
    // original vertex envelope is conservative for curved outer boundaries;
    // refusing those cases is preferable to creating an exterior protrusion.
    let mut bounds = solvecraft_geom::Aabb3::EMPTY;
    for v in solid.vertex_iter() {
        bounds.add(vtx(&v));
    }
    let in_bounds = |q: Vec3| {
        q.is_finite()
            && q.x >= bounds.min.x - tol
            && q.x <= bounds.max.x + tol
            && q.y >= bounds.min.y - tol
            && q.y <= bounds.max.y + tol
            && q.z >= bounds.min.z - tol
            && q.z <= bounds.max.z + tol
    };
    let mut subst = HashMap::new();
    let mut ends = Vec::new();
    for (v, p) in [(v0, p0), (v1, p1)] {
        // Walk the endpoint fan from side A to side B, retaining every intervening face.
        let mut fan = Vec::new();
        let mut fi = fa;
        let mut incoming = edge.id();
        for _ in 0..32 {
            let f = faces.get(fi).ok_or_else(|| unsupported("missing endpoint face"))?;
            let seam =
                f.edge_iter().find(|e| e.id() != incoming && (e.front() == v || e.back() == v)).ok_or_else(|| unsupported("open endpoint fan"))?;
            let next = on(seam.id()).into_iter().find(|f| *f != fi).ok_or_else(|| unsupported("open endpoint fan"))?;
            if next == fa || fan.iter().any(|(e, _): &(mt::Edge, usize)| e.id() == seam.id()) {
                return Err(unsupported("cyclic endpoint fan"));
            }
            incoming = seam.id();
            fan.push((seam, next));
            fi = next;
            if fi == fb {
                break;
            }
        }
        if fi != fb {
            return Err(unsupported("too many faces at the fillet endpoint"));
        }
        if fan.iter().take(fan.len().saturating_sub(1)).any(|(_, i)| faces.get(*i).is_none_or(|f| plane_normal(f).is_none())) {
            return Err(unsupported("this fillet needs rebuilding where it meets an earlier curved blend"));
        }
        let base = conic(p + t1 * s, p, p + t2 * s, w);
        let center = p + t1 * s + n1 * (sign * r);
        let mut points = Vec::new();
        let count = fan.len();
        for (i, (seam, _)) in fan.iter().enumerate() {
            let oriented = if seam.front() == v { seam.clone() } else { seam.inverse() };
            let c = oriented.oriented_curve();
            let residual = |q: Vec3| {
                if i == 0 {
                    (q - p).dot(t1) - s
                } else if i + 1 == count {
                    (q - p).dot(t2) - s
                } else {
                    let x = q - center;
                    (x - d * x.dot(d)).len() - r
                }
            };
            let hit = crossing(&c, residual, tol);
            let (t, q) = match hit {
                Ok(hit) => hit,
                Err(_) if i > 0 && i + 1 < count && matches!(c, mt::Curve::Line(_)) => {
                    // A reflex joint in the end-face fan may extend its shared seam.
                    // The rebuilt face loops below must remain free of crossings.
                    let back = vtx(oriented.back());
                    let away = (back - p).normalized().ok_or_else(|| unsupported("zero length endpoint seam"))?;
                    let ext = mt::Curve::Line(mt::Line(p3(p), p3(p - away * size)));
                    let (_, q) =
                        crossing(&ext, residual, tol).map_err(|_| unsupported("this fillet needs a different arrangement of faces at its end"))?;
                    (0.0, q)
                }
                Err(e) => return Err(e),
            };
            let q0 = q - d * (q - p).dot(d);
            if !in_bounds(q) {
                return Err(unsupported("the fillet endpoint leaves the part's bounds"));
            }
            let u = if i == 0 {
                0.0
            } else if i + 1 == count {
                1.0
            } else {
                base.search_nearest_parameter(p3(q0), None, 100).ok_or_else(|| unsupported("endpoint seam misses the fillet"))?
            };
            if !(0.0..=1.0).contains(&u) || from_p3(base.subs(u)).dist(q0) > tol * 10.0 {
                return Err(unsupported("endpoint seam misses the fillet arc"));
            }
            if points.last().is_some_and(|(prev, _): &(f64, mt::Vertex)| u <= *prev + 1e-8) {
                return Err(unsupported("fillet crosses the endpoint faces out of order"));
            }
            let nv = builder::vertex(p3(q));
            let (_, last) = c.range_tuple();
            let ne = if matches!(c, mt::Curve::Line(_)) {
                builder::line(&nv, oriented.back())
            } else {
                mt::Edge::try_new(&nv, oriented.back(), trim(c, t, last)).map_err(|e| KernelError::Failed(format!("fillet seam: {e}")))?
            };
            subst.insert(seam.id(), if oriented.front() == seam.absolute_front() { ne } else { ne.inverse() });
            points.push((u, nv));
        }
        let mut caps = Vec::new();
        for (i, pair) in points.windows(2).enumerate() {
            let [(lo, a), (hi, b)] = pair else {
                return Err(unsupported("fillet endpoint"));
            };
            let (_, g) = fan.get(i).ok_or_else(|| unsupported("fillet endpoint face"))?;
            let f = faces.get(*g).ok_or_else(|| unsupported("fillet endpoint face"))?;
            let ca = on_end(f, p + t1 * s, d, p)?;
            let cb = on_end(f, p + t2 * s, d, p)?;
            let curve = trim(conic(ca, p, cb, w), *lo, *hi);
            // A nearly parallel end plane can give an ellipse with a large
            // excursion between its endpoints, even when those endpoints fit.
            let samples = curve.parameter_division(curve.range_tuple(), tol.max(1e-6)).1;
            if samples.len() > 8192 || samples.into_iter().any(|q| !in_bounds(from_p3(q))) {
                return Err(unsupported("the fillet end section leaves the part's bounds"));
            }
            let cap = mt::Edge::try_new(a, b, curve).map_err(|e| KernelError::Failed(format!("fillet cap: {e}")))?;
            caps.push((*g, cap));
        }
        let a = points.first().map(|x| x.1.clone()).ok_or_else(|| unsupported("fillet endpoint"))?;
        let b = points.last().map(|x| x.1.clone()).ok_or_else(|| unsupported("fillet endpoint"))?;
        ends.push((a, b, caps));
    }
    let [(a0, b0, c0), (a1, b1, c1)] = &ends[..] else {
        return Err(unsupported("fillet endpoints"));
    };
    for (f, a, b) in [(f1, a0, a1), (f2, b0, b1)] {
        let (a, b) = (vtx(a), vtx(b));
        if (b - a).dot(d) <= tol * 100.0 || !crate::blend::extension_fits(f, a + d * tol * 20.0, b - d * tol * 20.0, tol) {
            return Err(unsupported("fillet contact leaves the side face"));
        }
    }
    let (la, lb) = (builder::line(a0, a1), builder::line(b0, b1));
    let mut out = Vec::new();
    for (i, f) in faces.iter().enumerate() {
        let mut sub = subst.clone();
        if i == fa {
            sub.insert(edge.id(), la.clone());
        }
        if i == fb {
            sub.insert(edge.id(), lb.clone());
        }
        let caps: Vec<_> = c0.iter().chain(c1).filter(|(g, _)| *g == i).map(|(_, e)| e.clone()).collect();
        let rebuilt = rebuild_face(f, &sub, &caps)?;
        if i == fa || i == fb || !caps.is_empty() {
            uncrossed(&rebuilt, tol)?;
        }
        out.push(rebuilt);
    }
    let mut es: Vec<_> = c0.iter().map(|(_, e)| e.clone()).collect();
    es.push(lb);
    es.extend(c1.iter().rev().map(|(_, e)| e.inverse()));
    es.push(la.inverse());
    let mut wire: mt::Wire = es.into();
    let forward = f1.boundary_iters().into_iter().flatten().any(|e| e.id() == edge.id() && e.front() == v0);
    if !forward {
        wire = wire.inverse();
    }
    let (q0, q1) = (p0 - d * size, p1 + d * size);
    let ctrl = vec![vec![h(q0 + t1 * s, 1.0), h(q1 + t1 * s, 1.0)], vec![h(q0, w), h(q1, w)], vec![h(q0 + t2 * s, 1.0), h(q1 + t2 * s, 1.0)]];
    let mut surf =
        mt::Surface::NurbsSurface(mt::NurbsSurface::new(mt::BSplineSurface::new((mt::KnotVec::bezier_knot(2), mt::KnotVec::bezier_knot(1)), ctrl)));
    let mid = from_p3(conic(p0 + t1 * s, p0, p0 + t2 * s, w).subs(0.5));
    let center = p0 + t1 * s + n1 * (sign * r);
    if let Some((u, v)) = surf.search_nearest_parameter(p3(mid), None, 100) {
        let n = surf.normal(u, v);
        if Vec3::new(n.x, n.y, n.z).dot((center - mid) * sign) < 0.0 {
            surf = mt::Invertible::inverse(&surf);
        }
    }
    out.push(mt::Face::try_new(vec![wire], surf).map_err(|e| KernelError::Failed(format!("fillet cylinder: {e}")))?);
    let mut shells = solid.boundaries().clone();
    if let Some(sh) = shells.get_mut(si) {
        *sh = out.into();
    }
    Solid::try_new(shells).map_err(|e| KernelError::Failed(format!("fillet endpoints: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Body, BoolOp};
    use mt::ParametricSurface;
    use solvecraft_geom::{Loop2, Plane, Vec2};

    fn basin(fall: bool) -> Body {
        let rect = |x0, y0, x1, y1| Loop2::polygon(&[Vec2::new(x0, y0), Vec2::new(x1, y0), Vec2::new(x1, y1), Vec2::new(x0, y1)]);
        let slab = crate::box_solid(Vec3::ZERO, Vec3::new(100.0, 60.0, 30.0)).unwrap();
        let pocket =
            crate::loft(&[(Plane::XY.offset(30.0), rect(20.0, 4.0, 80.0, 50.0)), (Plane::XY.offset(10.0), rect(30.0, 20.0, 70.0, 40.0))]).unwrap();
        let mut body = crate::boolean(&slab, &pocket, BoolOp::Cut).unwrap().unwrap();
        if fall {
            let floor = crate::loft(&[(Plane::XY.offset(10.0), rect(30.0, 20.0, 70.0, 40.0)), (Plane::XY.offset(6.0), rect(45.0, 26.0, 55.0, 34.0))])
                .unwrap();
            body = crate::boolean(&body, &floor, BoolOp::Cut).unwrap().unwrap();
        }
        body
    }
    fn check(body: &Body, p: Vec3, r: f64) -> Body {
        let out = crate::fillet(body, &[p], r).unwrap();
        assert!(out.validity().is_empty());
        assert_eq!(out.unmeshed_faces(), 0);
        let delta = crate::measure(&out).unwrap().volume - crate::measure(body).unwrap().volume;
        assert!(delta.abs() > 1e-6 && delta.abs() < 500.0, "volume delta {delta}");
        for f in out.solid.face_iter() {
            let s = f.surface();
            for e in f.edge_iter() {
                let c = e.curve();
                let (a, b) = c.range_tuple();
                for j in 0..=16 {
                    let q = c.subs(a + (b - a) * j as f64 / 16.0);
                    let (u, v) = s.search_nearest_parameter(q, None, 100).unwrap();
                    assert!(from_p3(s.subs(u, v)).dist(from_p3(q)) < 1e-4, "boundary off face");
                }
            }
        }
        out
    }
    #[test]
    fn concave_oblique_end_faces() {
        check(&basin(false), Vec3::new(25.0, 12.0, 20.0), 2.0);
    }
    #[test]
    fn concave_four_face_endpoints() {
        let b = basin(true);
        check(&b, Vec3::new(25.0, 12.0, 20.0), 2.0);
        check(&b, Vec3::new(50.0, 20.0, 10.0), 2.0);
    }

    /// One tenth of the reported basin: 10-degree side/back walls, a shallow front
    /// wall, and a floor dropping 0.4 mm to an 8 mm square drain seat.
    fn shallow_basin() -> Body {
        let rect = |x0, y0, x1, y1| Loop2::polygon(&[Vec2::new(x0, y0), Vec2::new(x1, y0), Vec2::new(x1, y1), Vec2::new(x0, y1)]);
        let inset = 12.0 * 10.0_f64.to_radians().tan();
        let floor = rect(15.0 + inset, 35.0 - inset, 75.0 - inset, 45.0 - inset);
        let slab = crate::box_solid(Vec3::ZERO, Vec3::new(90.0, 48.0, 16.0)).unwrap();
        let wall_tool = crate::loft(&[(Plane::XY.offset(16.0), rect(15.0, 1.8, 75.0, 45.0)), (Plane::XY.offset(4.0), floor.clone())]).unwrap();
        let body = crate::boolean(&slab, &wall_tool, BoolOp::Cut).unwrap().unwrap();
        let drain_tool =
            crate::loft(&[(Plane::XY.offset(4.0), floor), (Plane::XY.offset(3.6), rect(41.0, 36.0 - inset, 49.0, 45.0 - inset - 1.0))]).unwrap();
        crate::boolean(&body, &drain_tool, BoolOp::Cut).unwrap().unwrap()
    }

    #[test]
    fn basin_floor_edges_and_back_corners() {
        let b = shallow_basin();
        let inset = 12.0 * 10.0_f64.to_radians().tan();
        for p in [
            Vec3::new(45.0, 35.0 - inset, 4.0),
            Vec3::new(45.0, 45.0 - inset, 4.0),
            Vec3::new(15.0 + inset, 40.0 - inset, 4.0),
            Vec3::new(75.0 - inset, 40.0 - inset, 4.0),
            Vec3::new(15.0 + inset * 0.5, 45.0 - inset * 0.5, 10.0),
            Vec3::new(75.0 - inset * 0.5, 45.0 - inset * 0.5, 10.0),
        ] {
            check(&b, p, 1.5);
        }
    }

    #[test]
    fn oversized_endpoint_trim_preserves_input() {
        let b = basin(true);
        let before: Vec<_> = b.solid.vertex_iter().map(|v| vtx(&v)).collect();
        for r in [100.0, 1000.0] {
            assert!(crate::fillet(&b, &[Vec3::new(25.0, 12.0, 20.0)], r).is_err(), "radius {r}");
        }
        let after: Vec<_> = b.solid.vertex_iter().map(|v| vtx(&v)).collect();
        assert_eq!(before, after);
        assert!(b.validity().is_empty());
    }

    #[test]
    fn endpoint_section_cannot_cross_a_hole() {
        let b = basin(false);
        let pick = Vec3::new(25.0, 12.0, 20.0);
        let rounded = crate::fillet(&b, &[pick], 2.0).unwrap();
        // Put a small through-hole in the floor exactly where the new elliptical
        // endpoint would pass. The rails still fit their side faces.
        let q = rounded
            .solid
            .face_iter()
            .filter(|f| plane_normal(f).is_none())
            .flat_map(|f| f.edge_iter())
            .filter(|e| !matches!(e.curve(), mt::Curve::Line(_)))
            .map(|e| {
                let c = e.curve();
                let (a, b) = c.range_tuple();
                from_p3(c.subs((a + b) * 0.5))
            })
            .find(|p| (p.z - 10.0).abs() < 1e-6)
            .unwrap();
        let hole = crate::cylinder(Vec3::new(q.x, q.y, -1.0), Vec3::Z, 0.15, 12.0).unwrap();
        let drilled = crate::boolean(&b, &hole, BoolOp::Cut).unwrap().unwrap();
        assert!(crate::fillet(&drilled, &[pick], 2.0).is_err());
        assert!(drilled.validity().is_empty());
    }
}
