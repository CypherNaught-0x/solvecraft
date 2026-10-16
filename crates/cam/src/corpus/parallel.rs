//! Original closed plate/wedge solids for ball finishing. All coordinates and
//! expected heights are analytic; no reference-product artifacts enter this file.
use crate::*;
use solidcraft_geom::Vec3;

pub(super) fn fixture(slope: f64) -> Result<JobInput> {
    let mut input = super::fixture("face")?;
    input.setup.name = if slope == 0. { "Parallel plane" } else { "Parallel slope" }.into();
    input.setup.stock.max.x = 1.;
    input.setup.stock.max.y = 1.;
    let p = [
        Vec3::new(0., 0., 0.),
        Vec3::new(1., 0., 0.),
        Vec3::new(1., 1., 0.),
        Vec3::new(0., 1., 0.),
        Vec3::new(0., 0., 5.),
        Vec3::new(1., 0., 5. + slope),
        Vec3::new(1., 1., 5. + slope),
        Vec3::new(0., 1., 5.),
    ];
    input.geometry.target = [[0, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4], [1, 2, 6, 5], [2, 3, 7, 6], [3, 0, 4, 7]]
        .into_iter()
        .flat_map(|q| [[p[q[0]], p[q[1]], p[q[2]]], [p[q[0]], p[q[2]], p[q[3]]]])
        .collect();
    input.tools[0].kind = ToolKind::BallEndMill;
    input.tools[0].name = "2 mm ball end mill".into();
    input.tools[0].diameter = 2.;
    input.tools[0].shank_diameter = 2.;
    input.operations[0].name = input.setup.name.clone();
    input.operations[0].geometry = Strategy::Parallel;
    input.operations[0].stepover = 0.1;
    input.operations[0].bottom = 4.5;
    // This original reference recipe travels toward +X on every cutting row.
    input.operations[0].climb = false;
    input.fingerprint.clear();
    input.fingerprint = crate::fingerprint(&input)?;
    Ok(input)
}

/// Original shallow quadratic patches, represented by a closed faceted solid.
/// For h=1/8 and |second derivative|=.2, each triangle's interpolation error
/// against the analytic quadratic is at most .1*h*h/2 = .00078125 mm.
pub(super) fn curved(name: &str) -> Result<JobInput> {
    let mut input = fixture(0.)?;
    input.setup.name = format!("Original {name}");
    input.operations[0].name = input.setup.name.clone();
    let mut triangles = Vec::new();
    let vertex = |x: usize, y: usize, top: bool| -> Result<Vec3> {
        let x = x as f64 / 8.;
        let y = y as f64 / 8.;
        Ok(Vec3::new(x, y, if top { height(name, [x, y])? } else { 0. }))
    };
    for y in 0..8 {
        for x in 0..8 {
            for top in [false, true] {
                let [a, b, c, d] = [vertex(x, y, top)?, vertex(x + 1, y, top)?, vertex(x + 1, y + 1, top)?, vertex(x, y + 1, top)?];
                triangles.extend(if top { [[a, b, c], [a, c, d]] } else { [[a, c, b], [a, d, c]] });
            }
        }
    }
    for i in 0..8 {
        // Counterclockwise top perimeter; side faces point outward.
        for (a, b) in [((i, 0), (i + 1, 0)), ((8, i), (8, i + 1)), ((8 - i, 8), (7 - i, 8)), ((0, 8 - i), (0, 7 - i))] {
            let lo_a = vertex(a.0, a.1, false)?;
            let lo_b = vertex(b.0, b.1, false)?;
            let hi_a = vertex(a.0, a.1, true)?;
            let hi_b = vertex(b.0, b.1, true)?;
            triangles.extend([[lo_a, lo_b, hi_b], [lo_a, hi_b, hi_a]]);
        }
    }
    input.geometry.target = triangles;
    input.fingerprint.clear();
    input.fingerprint = crate::fingerprint(&input)?;
    Ok(input)
}
pub(super) fn height(name: &str, p: [f64; 2]) -> Result<f64> {
    let x = (p[0] - 0.5).powi(2);
    let y = (p[1] - 0.5).powi(2);
    Ok(5.
        + 0.1
            * match name {
                "parallel-dome" => -x - y,
                "parallel-bowl" => x + y,
                "parallel-saddle" => x - y,
                _ => return invalid("unknown quadratic milling fixture"),
            })
}
