//! Bounded unidirectional X raster with analytically certified ball-tip chords.
use super::{Writer, YieldOnce, contact::BallContact};
use crate::*;
use solidcraft_geom::Vec3;

pub(super) async fn generate(input: &JobInput, op: &Operation, tool: &Tool, w: &mut Writer) -> Result<()> {
    let stock = input.setup.stock;
    height_field(&input.geometry.target).await?;
    let radius = tool.diameter / 2.;
    let lowest_surface = input
        .geometry
        .target
        .iter()
        .filter(|t| (t[1] - t[0]).cross(t[2] - t[0]).z > 0.)
        .flat_map(|t| t.iter())
        .map(|p| p.z)
        .fold(f64::INFINITY, f64::min);
    if op.bottom > lowest_surface {
        return invalid("parallel bottom/retract heights exclude the finished surface");
    }
    // Contact centers must extend outside the stock footprint on slopes. A
    // generator-only floor at Bottom Z bounds this overscan's depth; the stock
    // verifier still uses only the original target solid.
    let mut triangles = input.geometry.target.clone();
    let a = Vec3::new(stock.min.x - 2. * radius, stock.min.y - 2. * radius, op.bottom);
    let b = Vec3::new(stock.max.x + 2. * radius, a.y, op.bottom);
    let c = Vec3::new(b.x, stock.max.y + 2. * radius, op.bottom);
    let d = Vec3::new(a.x, c.y, op.bottom);
    triangles.extend([[a, b, c], [a, c, d]]);
    let mesh = BallContact::new(&triangles)?;
    // Generator, post and independent verifier have separate error budgets.
    let error = (op.tolerance / 32.).min(POST_ERROR / 4.);
    let count = ((stock.max.y - stock.min.y + 2. * radius) / op.stepover).ceil() as usize;
    if count == 0 || count > 10_000 {
        return invalid("parallel finishing exceeds raster pass limit");
    }
    let mut work = 30_000_000;
    let mut points = 0;
    for row in 0..=count {
        YieldOnce::default().await;
        w.progress.set(0.1 + 0.85 * row as f64 / (count + 1) as f64);
        let y = stock.min.y - radius + (stock.max.y - stock.min.y + 2. * radius) * row as f64 / count as f64;
        let (x0, x1) = if op.climb { (stock.max.x + radius, stock.min.x - radius) } else { (stock.min.x - radius, stock.max.x + radius) };
        let a = Vec3::new(x0, y, mesh.height([x0, y], radius, &mut work).await?.ok_or_else(|| Error("parallel row leaves target surface".into()))?);
        let b = Vec3::new(x1, y, mesh.height([x1, y], radius, &mut work).await?.ok_or_else(|| Error("parallel row leaves target surface".into()))?);
        let mut stack = vec![(a, b, 0_u8)];
        let mut row_points = vec![a];
        while let Some((a, b, depth)) = stack.pop() {
            YieldOnce::default().await;
            let bounds = mesh.chord(a, b, radius, &mut work).await?;
            if bounds.gouge <= error && bounds.excess.is_some_and(|gap| gap <= error) {
                points += 1;
                if points > MAX_MOVES / 2 {
                    return invalid("parallel finishing exceeds chord budget");
                }
                row_points.push(b);
            } else {
                if depth >= 24 || a.dist(b) < 1e-9 {
                    return invalid("parallel chord could not be certified; reduce the region or refine target geometry");
                }
                let x = (a.x + b.x) / 2.;
                let z = mesh.height([x, y], radius, &mut work).await?.ok_or_else(|| Error("parallel row crosses a gap in target surface".into()))?;
                let mid = Vec3::new(x, y, z);
                stack.push((mid, b, depth + 1));
                stack.push((a, mid, depth + 1));
            }
        }
        // Lift the entire row by the certified maximum undercut error, retaining
        // continuous shared vertices. Allowance is explicitly vertical here.
        for point in &mut row_points {
            point.z += error + op.allowance;
            if point.z < op.bottom || point.z >= input.setup.retract {
                return invalid("parallel contact is outside bottom/retract heights; adjust the setup or operation");
            }
        }
        if let Some(first) = row_points.first() {
            w.enter(&input.setup, *first, op).await?;
        }
        for point in row_points.into_iter().skip(1) {
            w.feed(point, op.feed).await?;
        }
    }
    Ok(())
}

async fn height_field(triangles: &[[Vec3; 3]]) -> Result<()> {
    // Conservative first scope: an oriented closed solid with a planar bottom
    // and no other downward faces. Unsupported undercuts must not be silently
    // treated as machinable by the vertical height-field verifier.
    let bottom = triangles.iter().flatten().map(|p| p.z).fold(f64::INFINITY, f64::min);
    let mut edges = std::collections::BTreeMap::new();
    // The CAD adapter canonicalizes numerical seams before fingerprinting.
    // Raw CAM inputs must already be closed; never repair their geometry here.
    let key = |p: Vec3| [p.x, p.y, p.z].map(|v| if v == 0. { 0 } else { v.to_bits() });
    for (index, triangle) in triangles.iter().enumerate() {
        if index.is_multiple_of(128) {
            YieldOnce::default().await;
        }
        let [a, b, c] = *triangle;
        let normal = (b - a).cross(c - a);
        if normal.len() == 0. || (normal.z < 0. && triangle.iter().any(|p| (p.z - bottom).abs() > 1e-8)) {
            return invalid("parallel finishing requires a nondegenerate height-field solid with a planar bottom; undercuts are unsupported");
        }
        let [a, b, c] = [key(a), key(b), key(c)];
        for (a, b) in [(a, b), (b, c), (c, a)] {
            let (edge, direction) = if a < b { ((a, b), 1_i32) } else { ((b, a), -1) };
            let entry = edges.entry(edge).or_insert((0_u32, 0_i32));
            entry.0 += 1;
            entry.1 += direction;
        }
    }
    if edges.values().any(|&(count, direction)| count != 2 || direction != 0) {
        return invalid("parallel finishing requires a closed, consistently oriented target mesh");
    }
    Ok(())
}
