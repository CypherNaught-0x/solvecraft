//! Trace physical paths through both endpoint fills and the retained cylinder.
//! Checking only boundary tangency missed waves inside otherwise valid patches.

use super::*;
use crate::Body;
use mt::{InnerSpace, ParametricSurface};

#[test]
fn sloped_corner_flow_stays_smooth_from_floor_to_rim() {
    let rim = [Vec3::new(45.0, 1.8, 16.0), Vec3::new(45.0, 45.0, 16.0), Vec3::new(15.0, 23.4, 16.0), Vec3::new(75.0, 23.4, 16.0)];
    let mut body = crate::fillet(&super::tests::shallow_basin(), &rim, 0.3).unwrap();
    let inset = 12.0 * 10.0_f64.to_radians().tan();
    for edge in [
        Vec3::new(15.0 + inset, 40.0 - inset, 4.0),
        Vec3::new(75.0 - inset, 40.0 - inset, 4.0),
        Vec3::new(45.0, 45.0 - inset, 4.0),
        Vec3::new(45.0, 35.0 - inset, 4.0),
        Vec3::new(15.0 + inset * 0.5, 45.0 - inset * 0.5, 10.0),
    ] {
        body = crate::fillet(&body, &[edge], 1.5).unwrap();
    }
    check_flow(&body, true);
}

#[test]
fn perpendicular_corner_flow_stays_smooth_at_rim() {
    let slab = crate::box_solid(Vec3::ZERO, Vec3::new(90.0, 48.0, 16.0)).unwrap();
    let tool = crate::box_solid(Vec3::new(15.0, 10.0, 4.0), Vec3::new(75.0, 40.0, 18.0)).unwrap();
    let body = crate::boolean(&slab, &tool, crate::BoolOp::Cut).unwrap().unwrap();
    let rim = [Vec3::new(45.0, 10.0, 16.0), Vec3::new(45.0, 40.0, 16.0), Vec3::new(15.0, 25.0, 16.0), Vec3::new(75.0, 25.0, 16.0)];
    let body = crate::fillet(&body, &rim, 0.3).unwrap();
    let body = crate::fillet(&body, &[Vec3::new(15.0, 40.0, 10.0)], 1.5).unwrap();
    check_flow(&body, false);
}

fn check_flow(body: &Body, rounded_floor: bool) {
    assert!(body.validity().is_empty());
    assert_eq!(body.unmeshed_faces(), 0);
    let mesh = body.display_mesh(body.size() * 0.002).unwrap();
    let faces: Vec<_> = body.solid.face_iter().collect();
    let (origin, axis) = faces
        .iter()
        .find_map(|f| match crate::offset::surf_of(f, 1e-6) {
            Some(crate::offset::Surf::Cylinder { o, a, r, .. }) if a.z.abs() > 0.9 && (r - 1.5).abs() < 1e-6 => {
                Some((o, if a.z > 0.0 { a } else { -a }))
            }
            _ => None,
        })
        .unwrap();
    let regions: Vec<_> = faces
        .iter()
        .map(|f| {
            // Fan fills occupy a complete unit square. Their singular corners
            // can defeat a generic trim inversion, so verify the four sides and
            // use the exact parameter bounds for these surfaces.
            if matches!(f.surface(), mt::Surface::BSplineSurface(s) if s.degrees()==(5,5)) {
                assert_eq!(f.boundaries().len(), 1);
                assert_eq!(f.edge_iter().count(), 4);
                let surface = crate::body::evaluation::Surface(f.oriented_surface());
                for edge in f.edge_iter() {
                    let curve = edge.oriented_curve();
                    let (lo, hi) = curve.range_tuple();
                    for t in [0.25, 0.5, 0.75] {
                        let (u, v) = surface.search_nearest_parameter(curve.subs(lo + (hi - lo) * t), None, 100).unwrap();
                        assert!(u.abs().min(v.abs()).min((1.0 - u).abs()).min((1.0 - v).abs()) < 1e-4);
                    }
                }
                None
            } else {
                Some(crate::blend_trim::FaceRegion::new(f, 1e-4).unwrap())
            }
        })
        .collect();
    let normal_at = |z: f64, direction: Vec3| {
        let across = Vec3::Z.cross(direction);
        let a = mt::Vector3::unit_z();
        let b = mt::Vector3::new(across.x, across.y, across.z);
        let o = origin + axis * ((z - origin.z) / axis.z);
        let (distance, index) = mesh.raycast(o, direction).expect("the complete corner path must be covered");
        let picked = mesh.tri_face[index] as usize;
        let p = o + direction * if distance < 4.0 { distance } else { 1.5 };
        // A triangle can straddle a curved trim by its chord tolerance.
        // Fall back to the other faces, accepting only a forward exact hit
        // inside its trim and within the corner. Extrapolating the first
        // surface invents normal jumps, and a coarse triangle may miss a trim.
        let mut candidates = std::iter::once(picked).chain((0..faces.len()).filter(|j| *j != picked));
        let n = candidates
            .find_map(|index| {
                let s = crate::body::evaluation::Surface(faces[index].oriented_surface());
                let (mut u, mut v) = s.search_nearest_parameter(p3(p), None, 100)?;
                for _ in 0..20 {
                    let q = s.subs(u, v) - p3(o);
                    let (su, sv) = (s.uder(u, v), s.vder(u, v));
                    let (aa, ab, ba, bb) = (su.dot(a), sv.dot(a), su.dot(b), sv.dot(b));
                    let det = aa * bb - ab * ba;
                    if !det.is_finite() || det.abs() < 1e-14 {
                        return None;
                    }
                    let du = (q.dot(a) * bb - q.dot(b) * ab) / det;
                    let dv = (q.dot(b) * aa - q.dot(a) * ba) / det;
                    u -= du;
                    v -= dv;
                    if du.abs() + dv.abs() < 1e-12 {
                        break;
                    }
                }
                if regions[index].is_none() && (-1e-8..=1.0 + 1e-8).contains(&u) && (-1e-8..=1.0 + 1e-8).contains(&v) {
                    u = u.clamp(0.0, 1.0);
                    v = v.clamp(0.0, 1.0);
                }
                let q = s.subs(u, v) - p3(o);
                (q.dot(a).abs() < 1e-6
                    && q.dot(b).abs() < 1e-6
                    && (0.0..4.0).contains(&(from_p3(s.subs(u, v)) - o).dot(direction))
                    && regions[index]
                        .as_ref()
                        .map_or((0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v), |region| region.contains(s.subs(u, v), 1e-4)))
                .then(|| s.normal(u, v))
            })
            .unwrap_or_else(|| panic!("ray {direction:?}, z {z}: no exact hit inside the corner"));
        assert!((n.magnitude() - 1.0).abs() < 1e-9);
        n
    };
    let radial = (-Vec3::X + Vec3::Y).normalized().unwrap();
    let mut failures = Vec::new();
    // Nine paths span the corner, including its shoulders. Sample cell centers
    // to avoid exactly coplanar ray/edge hits at rational construction heights.
    // Leave the original sharp floor vertex and rim apex out of this regular
    // surface test; their direction-dependent limits have separate coverage.
    for lane in -4..=4 {
        let angle = lane as f64 * 0.15;
        let direction = radial * angle.cos() + Vec3::Z.cross(radial) * angle.sin();
        let radial_normal = (direction - axis * direction.dot(axis)).normalized().unwrap();
        let radial_normal = mt::Vector3::new(radial_normal.x, radial_normal.y, radial_normal.z);
        let corner_axis = mt::Vector3::new(axis.x, axis.y, axis.z);
        let mut lower = Vec::new();
        let mut upper = Vec::new();
        let mut previous: Option<mt::Vector3> = None;
        for i in (if rounded_floor { 0 } else { 232 })..470 {
            let z = 4.2125 + i as f64 * 0.025;
            let n = normal_at(z, direction);
            if let Some(previous) = previous {
                let turn = previous.dot(n).clamp(-1.0, 1.0).acos();
                assert!(turn < 10.0_f64.to_radians(), "abrupt normal turn at lane {lane}, z {z}: {} degrees", turn.to_degrees());
            }
            previous = Some(n);
            // Project into this radial section: asin(n.axis) would confuse
            // legitimate azimuthal turning with reversal of the axial profile.
            assert!(-n.dot(radial_normal) > 1e-3, "corner must face the cavity");
            let tilt = n.dot(corner_axis).atan2(-n.dot(radial_normal));
            if z < 10.0 {
                lower.push(tilt);
            } else {
                upper.push(tilt);
            }
        }
        for (part, values) in [("floor", lower), ("rim", upper)] {
            if values.is_empty() {
                continue;
            }
            // A smooth approach to the axial cylinder normal should not turn
            // back on itself. Allow one degree of total numerical/fit reversal,
            // substantially below the old broad bands (over twenty degrees).
            let travel: f64 = values.windows(2).map(|n| (n[1] - n[0]).abs()).sum();
            let net = (values.last().unwrap() - values[0]).abs();
            let reversal = (travel - net).to_degrees();
            if reversal > 1.0 {
                failures.push(format!("{part}, lane {lane}: normal reverses {reversal} degrees"));
            }
        }
    }
    // A transverse section may have one broad peak at the corner. Extra
    // normal travel beyond that peak reveals small alternating rim ripples.
    let corner_axis = mt::Vector3::new(axis.x, axis.y, axis.z);
    for z in [15.6, 15.7, 15.8, 15.9, 15.95] {
        let values: Vec<_> = (-32..=32)
            .map(|lane| {
                let angle = lane as f64 * 0.0225;
                let direction = radial * angle.cos() + Vec3::Z.cross(radial) * angle.sin();
                normal_at(z, direction).dot(corner_axis).clamp(-1.0, 1.0).asin()
            })
            .collect();
        let travel: f64 = values.windows(2).map(|n| (n[1] - n[0]).abs()).sum();
        let min = values.iter().copied().fold(f64::INFINITY, f64::min);
        let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let ends = values[0] + values.last().unwrap();
        let excess = (travel - (2.0 * max - ends).max(ends - 2.0 * min)).to_degrees();
        if excess > 1.0 {
            failures.push(format!("transverse rim at z {z}: extra normal travel {excess} degrees"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("; "));
}
