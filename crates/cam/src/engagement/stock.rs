//! Conservative material history for independent engagement checks.
//! Updates are transactional: dropping the future never publishes partial stock.
use super::Material;
use crate::{Result, Stock, invalid, model};
use solidcraft_geom::Vec3;

pub struct MaterialStock {
    cells: Vec<Material>,
    stock: Stock,
    revision: usize,
    fingerprint: String,
}
impl MaterialStock {
    /// `spacing` is the maximum XY column width, not an uncertainty claim.
    /// Each column retains lower/upper material heights; unresolved edge columns
    /// remain possible material regardless of their center's height.
    pub fn new(stock: Stock, spacing: f64, fingerprint: String) -> Result<Self> {
        stock.validate()?;
        if !model::positive(spacing) || fingerprint.is_empty() || fingerprint.len() > 256 {
            return invalid("material tracking needs a positive spacing and input fingerprint");
        }
        let nx = ((stock.max.x - stock.min.x) / spacing).ceil() as usize;
        let ny = ((stock.max.y - stock.min.y) / spacing).ceil() as usize;
        let count = nx.checked_mul(ny).filter(|n| *n > 0 && *n <= 250_000).ok_or_else(|| crate::Error("material grid exceeds cell budget".into()))?;
        let mut cells = Vec::with_capacity(count);
        for y in 0..ny {
            for x in 0..nx {
                let point = |x: usize, y: usize| {
                    [
                        stock.min.x + (stock.max.x - stock.min.x) * x as f64 / nx as f64,
                        stock.min.y + (stock.max.y - stock.min.y) * y as f64 / ny as f64,
                    ]
                };
                cells.push(Material { min: point(x, y), max: point(x + 1, y + 1), bottom: stock.min.z, low: stock.max.z, high: stock.max.z });
            }
        }
        Ok(Self { cells, stock, revision: 0, fingerprint })
    }

    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
    pub fn revision(&self) -> usize {
        self.revision
    }
    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    /// Include every column that can intersect the swept cylinder. A long move
    /// can exceed the local limit; its verifier must split it, never truncate.
    pub fn local(&self, from: Vec3, to: Vec3, radius: f64, error: f64) -> Result<Vec<Material>> {
        validate_cut(self.stock, from, to, radius, error)?;
        let padding = error + guard(from, to, radius);
        let min = [from.x.min(to.x) - radius - padding, from.y.min(to.y) - radius - padding];
        let max = [from.x.max(to.x) + radius + padding, from.y.max(to.y) + radius + padding];
        let mut cells = Vec::new();
        for cell in &self.cells {
            if cell.high + padding <= from.z || (0..2).any(|k| cell.min[k] > max[k] || cell.max[k] < min[k]) {
                continue;
            }
            if cells.len() == 4096 {
                return invalid("local engagement material exceeds budget; subdivide the motion or refine the stock representation");
            }
            cells.push(*cell);
        }
        Ok(cells)
    }

    /// Apply one constant-Z flat-cutter sweep with declared XYZ uncertainty.
    /// Possible overlap lowers only the lower material bound; complete coverage
    /// lowers the upper bound. The highest possible tip governs definite removal.
    pub async fn cut<F: std::future::Future<Output = ()>>(
        &mut self,
        from: Vec3,
        to: Vec3,
        radius: f64,
        error: f64,
        budget: usize,
        mut yield_control: impl FnMut() -> F,
    ) -> Result<()> {
        self.apply(from, to, radius, [error, error], budget, &mut yield_control).await
    }

    /// Constant-Z interpreted NC: lateral chord uncertainty does not imply a Z
    /// error. Assigning the exact parsed tip height preserves the fact that a
    /// preceding pass already removed material down to the same Z.
    pub(super) async fn cut_planar<F: std::future::Future<Output = ()>>(
        &mut self,
        from: Vec3,
        to: Vec3,
        radius: f64,
        lateral_error: f64,
        budget: usize,
        yield_control: impl FnMut() -> F,
    ) -> Result<()> {
        self.apply(from, to, radius, [lateral_error, 0.], budget, yield_control).await
    }
    async fn apply<F: std::future::Future<Output = ()>>(
        &mut self,
        from: Vec3,
        to: Vec3,
        radius: f64,
        errors: [f64; 2],
        budget: usize,
        mut yield_control: impl FnMut() -> F,
    ) -> Result<()> {
        let [error, height_error] = errors;
        validate_cut(self.stock, from, to, radius, error)?;
        if self.cells.len() > budget || self.revision >= crate::MAX_MOVES {
            return invalid("material history work budget exhausted");
        }
        let padding = error + guard(from, to, radius);
        let mut next = Vec::with_capacity(self.cells.len());
        for (index, cell) in self.cells.iter().enumerate() {
            if index.is_multiple_of(128) {
                yield_control().await;
            }
            let mut cell = *cell;
            let center = [(cell.min[0] + cell.max[0]) / 2., (cell.min[1] + cell.max[1]) / 2.];
            let half = (cell.max[0] - cell.min[0]).hypot(cell.max[1] - cell.min[1]) / 2.;
            // Distance to a convex segment is 1-Lipschitz. Corner maxima are
            // exact for a convex rectangle; center-minus-radius is a safe min.
            let near = (distance(center, from, to) - half).max(0.);
            let far = [cell.min, [cell.max[0], cell.min[1]], cell.max, [cell.min[0], cell.max[1]]]
                .into_iter()
                .map(|p| distance(p, from, to))
                .fold(0_f64, f64::max);
            if near <= radius + padding {
                cell.low = cell.low.min((from.z - height_error).max(self.stock.min.z));
            }
            if far + padding < radius {
                cell.high = cell.high.min((from.z + height_error).max(self.stock.min.z));
            }
            next.push(cell);
        }
        self.cells = next;
        self.revision += 1;
        Ok(())
    }
}

fn validate_cut(stock: Stock, from: Vec3, to: Vec3, radius: f64, error: f64) -> Result<()> {
    model::point(from)?;
    model::point(to)?;
    if from.z != to.z || from.z < stock.min.z || !model::positive(radius) || !model::finite(error) || error < 0. || error >= radius {
        return invalid("material history supports horizontal flat-cutter sweeps above the stock bottom with bounded uncertainty");
    }
    Ok(())
}
fn guard(a: Vec3, b: Vec3, radius: f64) -> f64 {
    [a.x, a.y, a.z, b.x, b.y, b.z, radius].into_iter().map(f64::abs).fold(1_f64, f64::max) * 1024. * f64::EPSILON
}
fn distance(p: [f64; 2], a: Vec3, b: Vec3) -> f64 {
    let length = (b.x - a.x).hypot(b.y - a.y);
    if length == 0. {
        return (p[0] - a.x).hypot(p[1] - a.y);
    }
    let (ux, uy) = ((b.x - a.x) / length, (b.y - a.y) / length);
    let along = ((p[0] - a.x) * ux + (p[1] - a.y) * uy).clamp(0., length);
    (p[0] - a.x - along * ux).hypot(p[1] - a.y - along * uy)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    fn stock() -> MaterialStock {
        MaterialStock::new(Stock { min: Vec3::new(-2., -2., 0.), max: Vec3::new(2., 2., 5.) }, 0.1, "original-input".into()).unwrap()
    }
    fn ready<T>(f: impl std::future::Future<Output = T>) -> T {
        let mut f = std::pin::pin!(f);
        loop {
            if let std::task::Poll::Ready(value) = f.as_mut().poll(&mut std::task::Context::from_waker(std::task::Waker::noop())) {
                return value;
            }
        }
    }
    #[test]
    fn previous_removal_reduces_engagement_but_edge_uncertainty_remains_material() {
        let mut s = stock();
        let a = Vec3::new(-3., 0., 1.);
        let b = Vec3::new(3., 0., 1.);
        let before = super::super::horizontal(&s.local(a, b, 1., 0.).unwrap(), a, b, 1., 0., 4096).unwrap();
        assert!((before.load_high - 2.).abs() < 1e-8);
        ready(s.cut(a, b, 1., 0., 250_000, || std::future::ready(()))).unwrap();
        assert_eq!(s.revision(), 1);
        assert_eq!(s.fingerprint(), "original-input");
        // A repeated cut above the previous tip uncertainty sees only unresolved
        // edge stock. At precisely the same Z a thin residual layer is possible.
        let (a, b) = (a + Vec3::Z * 0.001, b + Vec3::Z * 0.001);
        let after = super::super::horizontal(&s.local(a, b, 1., 0.).unwrap(), a, b, 1., 0., 4096).unwrap();
        assert!(after.load_high < 0.5, "{after:?}");
        assert!(s.cells.iter().filter(|c| c.min[1] < 1. && c.max[1] >= 1.).any(|c| c.high == 5.));
        assert!(s.cells.iter().filter(|c| c.min[1] >= -0.5 && c.max[1] <= 0.5).all(|c| c.high < 1.00001));
    }
    #[test]
    fn dropping_incremental_update_or_rejecting_a_cut_preserves_history() {
        let mut s = stock();
        let mut f = Box::pin(s.cut(Vec3::new(-3., 0., 1.), Vec3::new(3., 0., 1.), 1., 0., 250_000, std::future::pending));
        assert!(f.as_mut().poll(&mut std::task::Context::from_waker(std::task::Waker::noop())).is_pending());
        drop(f);
        assert_eq!(s.revision(), 0);
        assert!(s.cells.iter().all(|c| c.low == 5. && c.high == 5.));
        assert!(ready(s.cut(Vec3::ZERO, Vec3::X, 1., 0., 0, || std::future::ready(()))).is_err());
        assert_eq!(s.revision(), 0);
    }
    #[test]
    fn diagonal_endcaps_and_error_bounds_never_remove_unreached_material() {
        let mut s = stock();
        let a = Vec3::new(-0.5, -0.5, 2.);
        let b = Vec3::new(0.5, 0.5, 2.);
        ready(s.cut(a, b, 0.5, 0.01, 250_000, || std::future::ready(()))).unwrap();
        for c in &s.cells {
            for i in 0..=4 {
                for j in 0..=4 {
                    let p = [c.min[0] + (c.max[0] - c.min[0]) * i as f64 / 4., c.min[1] + (c.max[1] - c.min[1]) * j as f64 / 4.];
                    let exact = if distance(p, a, b) <= 0.5 { 2. } else { 5. };
                    assert!(c.low <= exact && c.high >= exact);
                }
            }
        }
        assert!(s.local(a, b, 0.5, f64::NAN).is_err());
        assert!(MaterialStock::new(s.stock, 1e-6, "id".into()).is_err());
    }
}
