//! A single trimmed graph surface for a multi-sided setback patch. The trimming
//! curves lie inside a rectangular parameter domain, so a six-sided boundary
//! does not require a full-multiplicity knot or a crease across the surface.

use super::*;
mod bounds;
pub(crate) mod partition;
mod reference;
use crate::body::from_p3;
use mt::{BoundedCurve, ParametricCurve, ParametricSurface, ParametricSurface3D};

#[derive(Clone)]
pub(crate) enum Normal {
    Hermite { start: Vec3, end: Vec3, start_der: Vec3, end_der: Vec3 },
    Free,
    InwardCylinder { origin: Vec3, axis: Vec3 },
    Plane(Vec3),
    Cylinder { origin: Vec3, axis: Vec3 },
}

pub(crate) struct Boundary {
    pub curve: mt::Curve,
    pub normal: Normal,
}

impl Boundary {
    pub(crate) fn sample(&self, t: f64) -> Result<(Vec3, Vec3)> {
        let p = from_p3(self.curve.subs(t));
        let n = match self.normal {
            Normal::Free => Vec3::ZERO,
            Normal::Hermite { start, end, start_der, end_der } => {
                let (lo, hi) = self.curve.range_tuple();
                let u = (t - lo) / (hi - lo);
                let n = start * (2.0 * u * u * u - 3.0 * u * u + 1.0)
                    + end * (-2.0 * u * u * u + 3.0 * u * u)
                    + start_der * (u * u * u - 2.0 * u * u + u)
                    + end_der * (u * u * u - u * u);
                let d = self.curve.der(t);
                let tangent = Vec3::new(d.x, d.y, d.z).normalized().ok_or_else(|| unsupported("singular transition seam tangent"))?;
                (n - tangent * n.dot(tangent)).normalized().ok_or_else(|| unsupported("singular transition seam normal"))?
            }
            Normal::Plane(n) => n,
            Normal::InwardCylinder { origin, axis } => {
                let d = origin - p;
                (d - axis * d.dot(axis)).normalized().ok_or_else(|| unsupported("singular transition normal"))?
            }
            Normal::Cylinder { origin, axis } => {
                let d = p - origin;
                (d - axis * d.dot(axis)).normalized().ok_or_else(|| unsupported("singular setback normal"))?
            }
        };
        if !p.is_finite() || !n.is_finite() {
            return Err(unsupported("invalid setback boundary"));
        }
        Ok((p, n))
    }
}

#[derive(Clone)]
struct Row {
    entries: Vec<(usize, f64)>,
    rhs: f64,
}

/// Cubic spline basis and its first two derivatives, including repeated knots.
fn basis(knots: &[f64], t: f64, n: usize) -> Result<(Vec<f64>, Vec<f64>, Vec<f64>)> {
    let mut values = vec![0.0; knots.len().saturating_sub(1)];
    for (i, span) in knots.windows(2).enumerate() {
        let [a, b] = span else { continue };
        if ((*a <= t && t < *b) || (t == 1.0 && i + 1 == n))
            && let Some(x) = values.get_mut(i)
        {
            *x = 1.0;
        }
    }
    let mut degree1 = Vec::new();
    let mut degree2 = Vec::new();
    for degree in 1..=3 {
        let mut next = vec![0.0; values.len().saturating_sub(1)];
        for (i, value) in next.iter_mut().enumerate() {
            let get = |k| knots.get(k).copied().ok_or_else(|| unsupported("setback basis"));
            let (a, b, c, d) = (get(i)?, get(i + degree)?, get(i + 1)?, get(i + degree + 1)?);
            if b > a {
                *value += (t - a) / (b - a) * values.get(i).copied().unwrap_or(0.0);
            }
            if d > c {
                *value += (d - t) / (d - c) * values.get(i + 1).copied().unwrap_or(0.0);
            }
        }
        values = next;
        if degree == 1 {
            degree1 = values.clone();
        }
        if degree == 2 {
            degree2 = values.clone();
        }
    }
    let mut derivative = vec![0.0; n];
    for (i, value) in derivative.iter_mut().enumerate() {
        let den1 = knots.get(i + 3).copied().unwrap_or(0.0) - knots.get(i).copied().unwrap_or(0.0);
        let den2 = knots.get(i + 4).copied().unwrap_or(0.0) - knots.get(i + 1).copied().unwrap_or(0.0);
        if den1 > 0.0 {
            *value += 3.0 / den1 * degree2.get(i).copied().unwrap_or(0.0);
        }
        if den2 > 0.0 {
            *value -= 3.0 / den2 * degree2.get(i + 1).copied().unwrap_or(0.0);
        }
    }
    let mut lower = vec![0.0; n + 1];
    for (i, value) in lower.iter_mut().enumerate() {
        let a = knots.get(i + 2).copied().unwrap_or(0.0) - knots.get(i).copied().unwrap_or(0.0);
        let b = knots.get(i + 3).copied().unwrap_or(0.0) - knots.get(i + 1).copied().unwrap_or(0.0);
        if a > 0.0 {
            *value += 2.0 / a * degree1.get(i).copied().unwrap_or(0.0);
        }
        if b > 0.0 {
            *value -= 2.0 / b * degree1.get(i + 1).copied().unwrap_or(0.0);
        }
    }
    let mut second = vec![0.0; n];
    for (i, value) in second.iter_mut().enumerate() {
        let a = knots.get(i + 3).copied().unwrap_or(0.0) - knots.get(i).copied().unwrap_or(0.0);
        let b = knots.get(i + 4).copied().unwrap_or(0.0) - knots.get(i + 1).copied().unwrap_or(0.0);
        if a > 0.0 {
            *value += 3.0 / a * lower.get(i).copied().unwrap_or(0.0);
        }
        if b > 0.0 {
            *value -= 3.0 / b * lower.get(i + 1).copied().unwrap_or(0.0);
        }
    }
    Ok((values, derivative, second))
}

fn tensor(a: &[f64], b: &[f64], weight: f64, rhs: f64) -> Row {
    let mut entries = Vec::new();
    for (i, x) in a.iter().enumerate().filter(|(_, x)| x.abs() > 1e-14) {
        for (j, y) in b.iter().enumerate().filter(|(_, y)| y.abs() > 1e-14) {
            entries.push((i * b.len() + j, x * y * weight));
        }
    }
    Row { entries, rhs: rhs * weight }
}

/// Banded Cholesky on the sparse normal equations. Cubic tensor-product rows
/// couple only neighboring control points, so a direct solve remains bounded
/// and resolves the weak interior fairness constraints as well as the boundary.
fn solve(rows: &[Row], count: usize) -> Result<Vec<f64>> {
    let width = rows
        .iter()
        .filter_map(|r| {
            let lo = r.entries.iter().map(|(i, _)| *i).min()?;
            let hi = r.entries.iter().map(|(i, _)| *i).max()?;
            Some(hi - lo)
        })
        .max()
        .unwrap_or(0);
    if count > 8192 || width > 320 {
        return Err(unsupported("setback fit exceeds its budget"));
    }
    let stride = width + 1;
    let mut matrix = vec![0.0; count * stride];
    let mut rhs = vec![0.0; count];
    for row in rows {
        for (i, a) in &row.entries {
            let b = rhs.get_mut(*i).ok_or_else(|| unsupported("setback fit index"))?;
            *b += a * row.rhs;
            for (j, b) in &row.entries {
                if i >= j {
                    let value = matrix.get_mut(i * stride + i - j).ok_or_else(|| unsupported("setback fit index"))?;
                    *value += a * b;
                }
            }
        }
    }
    // Equilibrate the columns before factorization. Boundary derivatives can
    // outweigh weak controls outside the trim by many orders of magnitude;
    // factoring that unscaled matrix loses the small tangent constraints.
    let mut scale = Vec::with_capacity(count);
    for i in 0..count {
        let diagonal = matrix.get(i * stride).copied().ok_or_else(|| unsupported("patch fit diagonal"))?;
        if !diagonal.is_finite() || diagonal <= 0.0 {
            return Err(unsupported("invalid patch fit diagonal"));
        }
        scale.push(diagonal.sqrt());
    }
    for (i, row) in matrix.chunks_mut(stride).enumerate() {
        let si = scale.get(i).copied().ok_or_else(|| unsupported("patch fit scale"))?;
        for (offset, value) in row.iter_mut().enumerate() {
            let Some(j) = i.checked_sub(offset) else { continue };
            let sj = scale.get(j).copied().ok_or_else(|| unsupported("patch fit scale"))?;
            *value /= si * sj;
        }
        if let Some(value) = rhs.get_mut(i) {
            *value /= si;
        }
    }
    let at = |matrix: &[f64], i: usize, j: usize| {
        if i < j || i - j > width { 0.0 } else { matrix.get(i * stride + i - j).copied().unwrap_or(0.0) }
    };
    for i in 0..count {
        for j in i.saturating_sub(width)..=i {
            let mut value = at(&matrix, i, j);
            for k in i.saturating_sub(width)..j {
                value -= at(&matrix, i, k) * at(&matrix, j, k);
            }
            value = if i == j {
                if !(value.is_finite() && value > 0.0) {
                    return Err(unsupported("singular setback fit"));
                }
                value.sqrt()
            } else {
                value / at(&matrix, j, j)
            };
            let slot = matrix.get_mut(i * stride + i - j).ok_or_else(|| unsupported("setback fit index"))?;
            *slot = value;
        }
    }
    let mut solution = vec![0.0; count];
    for i in 0..count {
        let mut value = rhs.get(i).copied().ok_or_else(|| unsupported("setback fit index"))?;
        for j in i.saturating_sub(width)..i {
            value -= at(&matrix, i, j) * solution.get(j).copied().unwrap_or(0.0);
        }
        if let Some(x) = solution.get_mut(i) {
            *x = value / at(&matrix, i, i);
        }
    }
    for i in (0..count).rev() {
        let mut value = solution.get(i).copied().unwrap_or(0.0);
        for j in i + 1..count.min(i + width + 1) {
            value -= at(&matrix, j, i) * solution.get(j).copied().unwrap_or(0.0);
        }
        if let Some(x) = solution.get_mut(i) {
            *x = value / at(&matrix, i, i);
        }
    }
    for (value, scale) in solution.iter_mut().zip(scale) {
        *value /= scale;
    }
    if solution.iter().any(|x| !x.is_finite()) {
        return Err(unsupported("invalid setback fit"));
    }
    Ok(solution)
}

pub(super) fn fit(boundaries: &[Boundary], out: Vec3, size: f64, planes: &[(Vec3, Vec3)], radii: [f64; 3]) -> Result<mt::Surface> {
    fit_impl(boundaries, out, size, planes, Some(radii))
}

/// A bounded transition keeps its prescribed cylinder/face tangent planes while
/// unconstrained boundaries may retain an existing sharp meeting with an end face.
pub(crate) fn transition(boundaries: &[Boundary], out: Vec3, size: f64, planes: &[(Vec3, Vec3)]) -> Result<mt::Surface> {
    fit_impl(boundaries, out, size, planes, None)
}

fn fit_impl(boundaries: &[Boundary], out: Vec3, size: f64, planes: &[(Vec3, Vec3)], radii: Option<[f64; 3]>) -> Result<mt::Surface> {
    if boundaries.len() < 3
        || boundaries.len() > 16
        || planes.len() > 32
        || !size.is_finite()
        || size <= 0.0
        || planes.iter().any(|(n, p)| !n.is_finite() || !p.is_finite() || (n.len() - 1.0).abs() > 1e-6)
    {
        return Err(unsupported("invalid transition patch input"));
    }
    let z = out.normalized().ok_or_else(|| unsupported("setback patch direction"))?;
    let x = z.any_perp();
    let y = z.cross(x);
    let mut samples = Vec::new();
    for boundary in boundaries {
        let (a, b) = boundary.curve.range_tuple();
        let count = if radii.is_some() { 256 } else { 1024 };
        for i in 0..=count {
            let (p, n) = boundary.sample(a + (b - a) * i as f64 / count as f64)?;
            if n != Vec3::ZERO && n.dot(z) < 1e-3 {
                return Err(unsupported("setback boundary folds behind its patch"));
            }
            samples.push((p, n));
        }
    }
    if samples.is_empty() {
        return Err(unsupported("empty setback boundary"));
    }
    let origin = samples.iter().fold(Vec3::ZERO, |a, (p, _)| a + *p) * (1.0 / samples.len() as f64);
    let (mut umin, mut umax, mut vmin, mut vmax) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
    for (p, _) in &samples {
        let p = *p - origin;
        umin = umin.min(p.dot(x));
        umax = umax.max(p.dot(x));
        vmin = vmin.min(p.dot(y));
        vmax = vmax.max(p.dot(y));
    }
    let (du, dv) = ((umax - umin) * 1.1, (vmax - vmin) * 1.1);
    if !(du.is_finite() && dv.is_finite() && du > size * 1e-9 && dv > size * 1e-9) {
        return Err(unsupported("degenerate setback patch"));
    }
    let origin = origin + x * ((umin + umax - du) * 0.5) + y * ((vmin + vmax - dv) * 0.5);
    let polygon: Vec<_> = samples
        .iter()
        .map(|(p, _)| {
            let p = *p - origin;
            (p.dot(x) / du, p.dot(y) / dv)
        })
        .collect();
    let inside = |u: f64, v: f64| {
        let mut result = false;
        for i in 0..polygon.len() {
            let (Some((a, b)), Some((c, d))) = (polygon.get(i), polygon.get((i + 1) % polygon.len())) else { continue };
            if (b > &v) != (d > &v) && u < (c - a) * (v - b) / (d - b) + a {
                result = !result;
            }
        }
        result
    };
    // A convex reference made from the Minkowski sum of three circular disks
    // and the inward octant. Its support function gives the specified circle
    // on every edge, the original planes outside their setbacks, and a rounded
    // interior. Unlike an unconstrained thin plate it cannot leave the part.
    let reference = if radii.is_none() { Some(reference::Reference::new(&samples, x, y)?) } else { None };
    let guide = |u: f64, v: f64| -> f64 {
        let base = origin + x * (du * u) + y * (dv * v);
        if let Some(reference) = &reference {
            let h = reference.height(base, z).unwrap_or(0.0);
            let (mut lower, mut upper) = (f64::NEG_INFINITY, f64::INFINITY);
            for (n, p) in planes {
                let den = n.dot(z);
                if den > 1e-8 {
                    upper = upper.min((*p - base).dot(*n) / den);
                } else if den < -1e-8 {
                    lower = lower.max((*p - base).dot(*n) / den);
                }
            }
            return if lower <= upper { h.clamp(lower, upper) } else { h };
        }
        let [pa, pb, pc] = planes else { return f64::NAN };
        let offsets = [(base - pa.1).dot(pa.0), (base - pb.1).dot(pb.0), (base - pc.1).dot(pc.0)];
        let direction = [z.dot(pa.0), z.dot(pb.0), z.dot(pc.0)];
        let Some([ra, rb, rc]) = radii else { return 0.0 };
        let height = |w: [f64; 3]| {
            let [a, b, c] = w;
            let mut h = ra * (a.hypot(c) - a - c) + rb * (a.hypot(b) - a - b) + rc * (b.hypot(c) - b - c);
            let dominant = if rc > ra + rb {
                Some((c, b, ra, rb))
            } else if rb > ra + rc {
                Some((b, a, rc, ra))
            } else if ra > rb + rc {
                Some((a, c, rb, rc))
            } else {
                None
            };
            if let Some((left, right, start, end)) = dominant {
                let den = left.hypot(right) + left;
                let q = if den > 0.0 { right / den } else { 0.0 };
                let t = 2.0_f64.sqrt() * q / (1.0 + (2.0_f64.sqrt() - 1.0) * q);
                let shift = end + (start - end) * t * t * (3.0 - 2.0 * t);
                let den = a * b + b * c + c * a;
                if den > 0.0 {
                    h += shift * a * b * c / den;
                }
            }
            (h - w.iter().zip(offsets).map(|(w, p)| w * p).sum::<f64>()) / w.iter().zip(direction).map(|(w, n)| w * n).sum::<f64>()
        };
        let mut best = [1.0, 0.0, 0.0];
        let mut value = height(best);
        for i in 0..=8 {
            for j in 0..=8 - i {
                let w = [i as f64 / 8.0, j as f64 / 8.0, (8 - i - j) as f64 / 8.0];
                let h = height(w);
                if h < value {
                    value = h;
                    best = w;
                }
            }
        }
        let mut step = 0.125;
        for _ in 0..14 {
            let current = best;
            for (i, j) in [(0, 1), (1, 0), (0, 2), (2, 0), (1, 2), (2, 1)] {
                let mut w = current;
                let Some(a) = w.get(i).copied() else { continue };
                if a < step {
                    continue;
                }
                if let Some(a) = w.get_mut(i) {
                    *a -= step;
                }
                if let Some(b) = w.get_mut(j) {
                    *b += step;
                }
                let h = height(w);
                if h < value {
                    value = h;
                    best = w;
                }
            }
            step *= 0.5;
        }
        value
    };
    // One part per million in position, with a separate 0.01-degree normal
    // bound. Validate between the fitting samples as well as at the corners.
    let tolerance = (size * 1e-6).max(1e-6);
    let trim = bounds::Trim::new(boundaries, origin, x, y, du, dv, tolerance)?;
    let densities: &[usize] = if radii.is_some() { &[20, 32, 48, 64] } else { &[20, 32, 48, 64, 80] };
    for &density in densities {
        let knot_vec = |axis: Vec3, length: f64| -> Result<Vec<f64>> {
            let mut corners = Vec::new();
            for b in boundaries {
                let (t, _) = b.curve.range_tuple();
                corners.push((b.sample(t)?.0 - origin).dot(axis) / length);
            }
            corners.sort_by(f64::total_cmp);
            corners.dedup_by(|a, b| (*a - *b).abs() < 1e-8);
            let mut interior: Vec<_> =
                (1..density - 3).map(|i| i as f64 / (density - 3) as f64).filter(|u| corners.iter().all(|c| (u - c).abs() > 1e-5)).collect();
            // C1 lines through the six corners permit the boundary's change of
            // curvature while retaining a continuous tangent plane everywhere.
            for c in corners {
                interior.extend([c, c]);
            }
            interior.sort_by(f64::total_cmp);
            let mut knots = vec![0.0; 4];
            knots.extend(interior);
            knots.extend([1.0; 4]);
            Ok(knots)
        };
        let ku = knot_vec(x, du)?;
        let kv = knot_vec(y, dv)?;
        let (nu, nv) = (ku.len() - 4, kv.len() - 4);
        let mut rows = Vec::new();
        for (p, normal) in &samples {
            let p = *p - origin;
            let (u, v) = (p.dot(x) / du, p.dot(y) / dv);
            let (bu, dbu, _) = basis(&ku, u, nu)?;
            let (bv, dbv, _) = basis(&kv, v, nv)?;
            rows.push(tensor(&bu, &bv, 1.0, p.dot(z)));
            if *normal != Vec3::ZERO {
                let weight = if radii.is_some() { 0.1 } else { 1.0 };
                rows.push(tensor(&dbu, &bv, weight, -du * normal.dot(x) / normal.dot(z)));
                rows.push(tensor(&bu, &dbv, weight, -dv * normal.dot(y) / normal.dot(z)));
            }
        }
        let greville = |knots: &[f64], i| -> f64 { knots.iter().skip(i + 1).take(3).sum::<f64>() / 3.0 };
        // Integrate actual spline bending energy in physical coordinates;
        // control-polygon differences bias nonuniform knots and repeated knots.
        let quadrature = |knots: &[f64], n: usize| -> Result<Vec<_>> {
            let mut out = Vec::new();
            for span in knots.windows(2) {
                let [a, b] = span else { continue };
                if b <= a {
                    continue;
                }
                for (t, w) in [
                    (-0.8611363115940526, 0.3478548451374538),
                    (-0.3399810435848563, 0.6521451548625461),
                    (0.3399810435848563, 0.6521451548625461),
                    (0.8611363115940526, 0.3478548451374538),
                ] {
                    let q = (a + b) * 0.5 + (b - a) * 0.5 * t;
                    out.push((basis(knots, q, n)?, (b - a) * 0.5 * w, q));
                }
            }
            Ok(out)
        };
        let constraints = |base: Vec3, bu: &[f64], bv: &[f64]| {
            if radii.is_some() {
                let upper = planes.iter().map(|(n, p)| (*p - base).dot(*n) / n.dot(z)).fold(f64::INFINITY, f64::min);
                return vec![tensor(bu, bv, 10.0, upper)];
            }
            planes
                .iter()
                .filter_map(|(n, p)| {
                    let den = n.dot(z);
                    // A graph cannot alter its projection. Such a plane is checked
                    // by the final hull certificate instead of dividing by zero.
                    (den.abs() > 1e-8).then(|| {
                        let mut row = tensor(bu, bv, 100.0 * den.signum(), (*p - base).dot(*n) / den);
                        row.rhs += 100.0 * tolerance * 0.25 / den.abs();
                        // tensor scales rhs by the signed weight as well: the
                        // inequality is still n·(surface - p) <= 0 on either side.
                        row
                    })
                })
                .collect::<Vec<_>>()
        };
        let mut limits = Vec::new();
        for ((bu, dbu, ddbu), wu, u) in &quadrature(&ku, nu)? {
            for ((bv, dbv, ddbv), wv, v) in &quadrature(&kv, nv)? {
                if inside(*u, *v) {
                    if radii.is_none() || boundaries.len() == 6 {
                        rows.push(tensor(bu, bv, 0.1 * (wu * wv).sqrt(), guide(*u, *v)));
                    }
                    let base = origin + x * (du * u) + y * (dv * v);
                    limits.extend(constraints(base, bu, bv));
                }
                let w = 1e-6 * size * (wu * wv * du * dv).sqrt();
                rows.push(tensor(ddbu, bv, w / (du * du), 0.0));
                rows.push(tensor(dbu, dbv, 2.0_f64.sqrt() * w / (du * dv), 0.0));
                rows.push(tensor(bu, ddbv, w / (dv * dv), 0.0));
            }
        }
        // Pin otherwise weakly constrained controls to a bounded reference,
        // rather than letting the normal equations amplify roundoff outside the trim.
        for i in 0..nu {
            for j in 0..nv {
                rows.push(Row { entries: vec![(i * nv + j, 1e-4)], rhs: 1e-4 * guide(greville(&ku, i), greville(&kv, j)) });
            }
        }
        let mut heights = solve(&rows, nu * nv)?;
        // Add plane constraints where the unconstrained fit protrudes. These
        // collocation rows guide the fit; only the control-hull certificate below
        // can accept a surface, including regions between the collocation points.
        let mut active = vec![false; limits.len()];
        for _ in 0..8 {
            // Transition boundaries are fitted first; their control-hull
            // certificate adds only the constraints actually needed below.
            if radii.is_none() {
                break;
            }
            let mut added = 0;
            for (limit, active) in limits.iter().zip(&mut active) {
                let value = limit.entries.iter().map(|(i, w)| heights.get(*i).copied().unwrap_or(0.0) * w).sum::<f64>();
                if !*active && value > limit.rhs + tolerance * 0.01 {
                    *active = true;
                    rows.push(limit.clone());
                    added += 1;
                }
            }
            if added == 0 {
                break;
            }
            heights = solve(&rows, nu * nv)?;
        }
        for _ in 0..16 {
            // Greville abscissae reproduce the affine x/y coordinates exactly.
            let mut ctrl = Vec::new();
            for i in 0..nu {
                let mut row = Vec::new();
                for j in 0..nv {
                    let h = heights.get(i * nv + j).copied().ok_or_else(|| unsupported("setback control point"))?;
                    row.push(p3(origin + x * (du * greville(&ku, i)) + y * (dv * greville(&kv, j)) + z * h));
                }
                ctrl.push(row);
            }
            let surface = mt::Surface::BSplineSurface(
                mt::BSplineSurface::try_new((mt::KnotVec::from(ku.clone()), mt::KnotVec::from(kv.clone())), ctrl)
                    .map_err(|e| KernelError::Failed(format!("setback surface: {e}")))?,
            );
            let mut worst: f64 = 0.0;
            let mut kink: f64 = 0.0;
            for boundary in boundaries {
                let (a, b) = boundary.curve.range_tuple();
                let count = if radii.is_some() { 513 } else { 1537 };
                for i in 0..=count {
                    let (p, normal) = boundary.sample(a + (b - a) * i as f64 / count as f64)?;
                    let q = p - origin;
                    let (u, v) = (q.dot(x) / du, q.dot(y) / dv);
                    let point = from_p3(surface.subs(u, v));
                    let nn = surface.normal(u, v);
                    let nn = Vec3::new(nn.x, nn.y, nn.z);
                    if !point.is_finite() || !nn.is_finite() {
                        return Err(unsupported("non-finite setback surface"));
                    }
                    worst = worst.max(point.dist(p));
                    if normal != Vec3::ZERO {
                        let error = 1.0 - nn.dot(normal);
                        kink = kink.max(error);
                    }
                }
            }
            if worst >= tolerance || kink >= 1.0 - 0.01_f64.to_radians().cos() {
                break;
            }
            let mt::Surface::BSplineSurface(s) = &surface else {
                break;
            };
            let violations = match trim.violations(s, planes, tolerance, if radii.is_some() { 0.5 } else { 0.95 }) {
                Ok(points) => points,
                Err(_) => break,
            };
            if violations.is_empty() {
                return Ok(surface);
            }
            for (u, v) in violations {
                let (bu, _, _) = basis(&ku, u, nu)?;
                let (bv, _, _) = basis(&kv, v, nv)?;
                let base = origin + x * (du * u) + y * (dv * v);
                for row in constraints(base, &bu, &bv) {
                    let value = row.entries.iter().map(|(i, w)| heights.get(*i).copied().unwrap_or(0.0) * w).sum::<f64>();
                    if value > row.rhs {
                        rows.push(row);
                    }
                }
            }
            heights = solve(&rows, nu * nv)?;
        }
    }
    Err(unsupported("cannot fit the setback patch within position and tangency tolerances"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cubic_basis_reproduces_affine_coordinates() {
        let knots = [0.0, 0.0, 0.0, 0.0, 0.25, 0.5, 0.75, 1.0, 1.0, 1.0, 1.0];
        for t in [0.0, 0.03, 0.25, 0.51, 0.99, 1.0] {
            let (b, d, _) = basis(&knots, t, 7).unwrap();
            assert!((b.iter().sum::<f64>() - 1.0).abs() < 1e-12);
            assert!(d.iter().sum::<f64>().abs() < 1e-12);
            let g: Vec<_> = (0..7).map(|i| knots[i + 1..i + 4].iter().sum::<f64>() / 3.0).collect();
            assert!((b.iter().zip(&g).map(|(b, g)| b * g).sum::<f64>() - t).abs() < 1e-12);
            assert!((d.iter().zip(&g).map(|(b, g)| b * g).sum::<f64>() - 1.0).abs() < 1e-12);
        }
    }
}
