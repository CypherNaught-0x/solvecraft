//! Continue an existing crease through a local patch fan. Both sides meet the
//! primary cylinder smoothly; each patch meets its own old end face smoothly.

use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn build(
    faces: &[&mt::Face],
    fan: &[(mt::Edge, usize)],
    points: &[mt::Vertex],
    wire: &[mt::Edge],
    boundaries: &[Boundary],
    section: &mt::Edge,
    center: Vec3,
    axis: Vec3,
    sign: f64,
    size: f64,
    planes: &[(Vec3, Vec3)],
    out: Vec3,
) -> Result<(Vec<mt::Edge>, Vec<mt::Face>)> {
    let curve = section.curve();
    let (lo, hi) = curve.range_tuple();
    let mut parameters = vec![lo];
    let mut arc = vec![section.front().clone()];
    for p in points.iter().skip(1).take(points.len().saturating_sub(2)) {
        let u = curve.search_nearest_parameter(p.point(), None, 100).ok_or_else(|| unsupported("transition fan angular parameter"))?;
        if !u.is_finite() || u <= parameters.last().copied().unwrap_or(lo) + 1e-5 || u >= hi - 1e-5 {
            return Err(unsupported("transition fan folds across the cylinder"));
        }
        parameters.push(u);
        arc.push(builder::vertex(curve.subs(u)));
    }
    parameters.push(hi);
    arc.push(section.back().clone());
    let mut seams = vec![wire.get(1).cloned().ok_or_else(|| unsupported("transition wall rail"))?];
    for (i, point) in points.iter().enumerate().skip(1).take(points.len().saturating_sub(2)) {
        let a = arc.get(i).ok_or_else(|| unsupported("transition fan vertex"))?;
        let (edge, _) = fan.get(i).ok_or_else(|| unsupported("transition original seam"))?;
        let curve = edge.oriented_curve();
        let (_, t) = curve.range_tuple();
        let d = curve.der(t);
        let d = Vec3::new(d.x, d.y, d.z);
        let speed = d.len();
        let length = vtx(a).dist(vtx(point));
        if !speed.is_finite() || speed < 1e-10 || !length.is_finite() || length < size * 1e-8 {
            return Err(unsupported("degenerate transition seam"));
        }
        let start = vtx(a);
        let end = vtx(point);
        let ds = -axis * length;
        // Use the actual adjacent surface normals, not the derivative of an
        // approximated intersection spline, for the crease's terminal frame.
        let normal = |index: usize| -> Result<(Vec3, crate::offset::Surf)> {
            let (_, fi) = fan.get(index).ok_or_else(|| unsupported("transition seam face"))?;
            let face = faces.get(*fi).ok_or_else(|| unsupported("transition seam face"))?;
            let surface = face.oriented_surface();
            let (u, v) = surface.search_nearest_parameter(point.point(), None, 100).ok_or_else(|| unsupported("transition seam parameters"))?;
            let n = surface.normal(u, v);
            let analytic = crate::offset::surf_of(face, size * 1e-8).ok_or_else(|| unsupported("transition seam curvature"))?;
            Ok((Vec3::new(n.x, n.y, n.z), analytic))
        };
        let (na, first) = normal(i - 1)?;
        let (nb, second) = normal(i)?;
        let mut tangent = na.cross(nb).normalized().ok_or_else(|| unsupported("tangent transition crease faces"))?;
        if tangent.dot(d) < 0.0 {
            tangent = -tangent;
        }
        let normal_curvature = |surface: crate::offset::Surf| -> Result<f64> {
            match surface {
                crate::offset::Surf::Plane { .. } => Ok(0.0),
                crate::offset::Surf::Cylinder { a, r, convex, .. } => Ok(-(if convex { 1.0 } else { -1.0 }) * (1.0 - tangent.dot(a).powi(2)) / r),
                _ => Err(unsupported("transition seam curvature")),
            }
        };
        let (ka, kb) = (normal_curvature(first)?, normal_curvature(second)?);
        let dot = na.dot(nb);
        let denominator = 1.0 - dot * dot;
        if denominator <= 1e-8 {
            return Err(unsupported("nearly tangent transition crease faces"));
        }
        let curvature = na * ((ka - dot * kb) / denominator) + nb * ((kb - dot * ka) / denominator);
        let mut end_length = length;
        let mut controls = [start; 6];
        let mut bounded = false;
        for _ in 0..16 {
            let de = tangent * end_length;
            let dde = curvature * (end_length * end_length);
            controls = [start, start + ds / 5.0, start + ds * 0.4, end - de * 0.4 + dde / 20.0, end - de / 5.0, end];
            if controls.iter().all(|p| p.is_finite() && planes.iter().all(|(n, q)| (*p - *q).dot(*n) <= size * 1e-8)) {
                bounded = true;
                break;
            }
            end_length *= 0.5;
        }
        if !bounded || end_length <= size * 1e-9 {
            return Err(unsupported("transition seam cannot stay within its local bounds"));
        }
        let curve = mt::Curve::BSplineCurve(mt::BSplineCurve::new(mt::KnotVec::bezier_knot(5), controls.into_iter().map(p3).collect()));
        seams.push(mt::Edge::try_new(a, point, curve).map_err(|e| KernelError::Failed(format!("transition shared seam: {e}")))?);
    }
    seams.push(wire.last().cloned().ok_or_else(|| unsupported("transition wall rail"))?.inverse());
    let cylinder = if sign > 0.0 { Normal::InwardCylinder { origin: center, axis } } else { Normal::Cylinder { origin: center, axis } };
    let mut sections = Vec::new();
    let mut patches = Vec::new();
    let mut pieces = Vec::new();
    for (i, span) in parameters.windows(2).enumerate() {
        let [lo, hi] = span else {
            return Err(unsupported("transition section span"));
        };
        let (Some(a), Some(b), Some(sa), Some(sb)) = (arc.get(i), arc.get(i + 1), seams.get(i), seams.get(i + 1)) else {
            return Err(unsupported("transition fan section"));
        };
        let edge = mt::Edge::try_new(a, b, trim(curve.clone(), *lo, *hi)).map_err(|e| KernelError::Failed(format!("transition arc: {e}")))?;
        let end = wire.get(i + 2).ok_or_else(|| unsupported("transition end boundary"))?;
        let end_normal = boundaries.get(i + 2).ok_or_else(|| unsupported("transition end normal"))?.normal.clone();
        let (_, fi) = fan.get(i).ok_or_else(|| unsupported("transition fan end face"))?;
        let face = faces.get(*fi).ok_or_else(|| unsupported("transition fan end face"))?;
        let seam_normal = |seam: &mt::Edge, index: usize, reverse: bool| -> Result<Normal> {
            if index == 0 {
                return Ok(boundaries.get(1).ok_or_else(|| unsupported("transition rail normal"))?.normal.clone());
            }
            if index + 1 == points.len() {
                return Ok(boundaries.last().ok_or_else(|| unsupported("transition rail normal"))?.normal.clone());
            }
            let c = seam.oriented_curve();
            let (a, b) = c.range_tuple();
            let boundary = Boundary { curve: c.clone(), normal: cylinder.clone() };
            let n0 = boundary.sample(a)?.1;
            let p = from_p3(c.subs(b));
            let d = c.der(b);
            let d = Vec3::new(d.x, d.y, d.z) * (b - a);
            let surface = face.oriented_surface();
            let (u, v) = surface.search_nearest_parameter(p3(p), None, 100).ok_or_else(|| unsupported("transition fan normal"))?;
            let n = surface.normal(u, v);
            let n1 = Vec3::new(n.x, n.y, n.z);
            let derivative = match crate::offset::surf_of(face, size * 1e-8) {
                Some(crate::offset::Surf::Plane { .. }) => Vec3::ZERO,
                Some(crate::offset::Surf::Cylinder { a, r, convex, .. }) => (d - a * d.dot(a)) * (if convex { 1.0 } else { -1.0 } / r),
                _ => return Err(unsupported("transition fan end curvature")),
            };
            Ok(if reverse {
                Normal::Hermite { start: n1, end: n0, start_der: -derivative, end_der: Vec3::ZERO }
            } else {
                Normal::Hermite { start: n0, end: n1, start_der: Vec3::ZERO, end_der: derivative }
            })
        };
        let conditions = [
            Boundary { curve: edge.oriented_curve().inverse(), normal: cylinder.clone() },
            Boundary { curve: sa.oriented_curve(), normal: seam_normal(sa, i, false)? },
            Boundary { curve: end.oriented_curve(), normal: end_normal },
            Boundary { curve: sb.oriented_curve().inverse(), normal: seam_normal(sb, i + 1, true)? },
        ];
        pieces.push((vec![edge.inverse(), sa.clone(), end.clone(), sb.inverse()], conditions));
        sections.push(edge);
    }
    crate::blend_corner::setback::partition::check(&pieces.iter().map(|(wire, _)| wire.clone()).collect::<Vec<_>>(), out, size)?;
    for (wire, conditions) in pieces {
        let surface = crate::blend_corner::setback::transition(&conditions, out, size, planes)?;
        patches.push(mt::Face::try_new(vec![wire.into()], surface).map_err(|e| KernelError::Failed(format!("transition fan patch: {e}")))?);
    }
    Ok((sections, patches))
}
