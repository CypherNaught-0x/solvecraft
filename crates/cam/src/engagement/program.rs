//! Independent NC engagement analysis. This supplements stock/clearance checks;
//! its result alone must never authorize posting or qualify a physical machine.
use super::{horizontal_errors, stock::MaterialStock};
use crate::{JobInput, Result, ToolKind, invalid, model, nc, verify::Status};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy)]
pub struct Limits {
    pub maximum_load: f64,
    pub spacing: f64,
    pub motion_error: f64,
    pub work: usize,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Finding {
    pub line: usize,
    pub source: Option<nc::SourceLocation>,
    pub status: Status,
    pub load_low: Option<f64>,
    pub load_high: Option<f64>,
    pub detail: String,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Report {
    pub fingerprint: String,
    pub program_fingerprint: String,
    pub status: Status,
    pub complete: bool,
    pub maximum_load: f64,
    pub column_spacing: f64,
    pub motion_error: f64,
    pub work: usize,
    pub findings: Vec<Finding>,
}

pub async fn verify<F: std::future::Future<Output = ()>>(
    input: &JobInput,
    parsed: &nc::Parsed,
    limits: Limits,
    cancelled: impl Fn() -> bool,
    mut yield_control: impl FnMut() -> F,
) -> Result<Report> {
    input.validate()?;
    if parsed.work_offset != input.setup.work_offset
        || !model::positive(limits.maximum_load)
        || !model::positive(limits.spacing)
        || !model::finite(limits.motion_error)
        || limits.motion_error < crate::POST_ERROR
        || limits.motion_error > crate::VERIFY_ERROR
        || limits.work == 0
        || limits.work > 100_000_000
    {
        return invalid("invalid engagement limits or NC work offset");
    }
    let mut stock = MaterialStock::new(input.setup.stock, limits.spacing, input.fingerprint.clone())?;
    let segments = nc::segments(parsed, &input.setup)?;
    let mut report = Report {
        fingerprint: input.fingerprint.clone(),
        program_fingerprint: crate::fingerprint(parsed)?,
        status: Status::Pass,
        complete: true,
        maximum_load: limits.maximum_load,
        column_spacing: limits.spacing,
        motion_error: limits.motion_error,
        work: 0,
        findings: Vec::new(),
    };
    for segment in segments {
        yield_control().await;
        if cancelled() {
            return invalid("engagement verification cancelled");
        }
        let tool = input.tools.iter().find(|t| t.id == segment.tool).ok_or_else(|| crate::Error("missing engagement tool".into()))?;
        let radius = tool.diameter / 2.;
        let z = segment.from.z.min(segment.to.z);
        if z > input.setup.stock.max.z + limits.motion_error {
            continue;
        }
        let mut from = segment.from;
        let mut to = segment.to;
        from.z = z;
        to.z = z;
        let unresolved = |detail: String| Finding {
            line: segment.line,
            source: parsed.source_for_line(segment.line),
            status: Status::Inconclusive,
            load_low: None,
            load_high: None,
            detail,
        };
        // Charge each bounded history scan, including air-motion queries.
        let cost = stock.cell_count().saturating_mul(2);
        if cost > limits.work.saturating_sub(report.work) {
            record(&mut report, unresolved("engagement material-history work limit reached".into()));
            report.complete = false;
            break;
        }
        report.work += cost;
        let local = match stock.local(from, to, radius, limits.motion_error) {
            Ok(local) => local,
            Err(error) => {
                record(&mut report, unresolved(error.0));
                report.complete = false;
                break;
            }
        };
        // Empty conservative material at the lowest tip height proves this
        // motion cannot remove stock, including vertical entry outside stock.
        if local.is_empty() {
            continue;
        }
        if segment.rapid || segment.from.z != segment.to.z || tool.kind != ToolKind::FlatEndMill {
            record(
                &mut report,
                unresolved(
                    "potential material contact during a rapid, changing-depth entry or non-flat cut; this engagement model cannot qualify it".into(),
                ),
            );
            report.complete = false;
            break;
        }
        if from.x == to.x && from.y == to.y {
            continue;
        }
        let bounds = horizontal_errors(&local, from, to, radius, limits.motion_error, 0., 4096)?;
        let status = if bounds.load_low > limits.maximum_load {
            Status::Fail
        } else if bounds.load_high > limits.maximum_load {
            Status::Inconclusive
        } else {
            Status::Pass
        };
        if status != Status::Pass {
            record(
                &mut report,
                Finding {
                    line: segment.line,
                    source: parsed.source_for_line(segment.line),
                    status,
                    load_low: Some(bounds.load_low),
                    load_high: Some(bounds.load_high),
                    detail: "independent equivalent radial immersion exceeds or straddles the declared maximum load".into(),
                },
            );
        }
        // Drop/cancellation cannot publish a partially updated stock revision.
        stock.cut_planar(from, to, radius, limits.motion_error, stock.cell_count(), &mut yield_control).await?;
        if cancelled() {
            return invalid("engagement verification cancelled");
        }
    }
    Ok(report)
}
fn record(report: &mut Report, finding: Finding) {
    if finding.status == Status::Fail || report.status == Status::Pass {
        report.status = finding.status;
    }
    if report.findings.len() < 200 {
        report.findings.push(finding);
    } else if finding.status == Status::Fail
        && let Some(old) = report.findings.iter_mut().find(|old| old.status != Status::Fail)
    {
        *old = finding;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    fn ready<T>(f: impl std::future::Future<Output = T>) -> T {
        let mut f = std::pin::pin!(f);
        loop {
            if let std::task::Poll::Ready(v) = f.as_mut().poll(&mut std::task::Context::from_waker(std::task::Waker::noop())) {
                return v;
            }
        }
    }
    fn input() -> JobInput {
        let mut input = crate::corpus::fixture("face").unwrap();
        input.setup.stock.min = solidcraft_geom::Vec3::new(-2., -2., 0.);
        input.setup.stock.max = solidcraft_geom::Vec3::new(2., 2., 6.);
        // Geometry is not used for engagement, but retains a valid selected model.
        input.geometry.target = crate::corpus::fixture("parallel-flat").unwrap().geometry.target;
        input.tools[0].diameter = 2.;
        input.operations[0].stepover = 0.4;
        input.fingerprint = crate::fingerprint(&(&input.setup, &input.tools, &input.operations, &input.geometry)).unwrap();
        input
    }
    fn limits(maximum_load: f64) -> Limits {
        Limits { maximum_load, spacing: 0.1, motion_error: crate::POST_ERROR, work: 1_000_000 }
    }
    #[test]
    fn independently_parsed_slot_exceeds_load_and_links_the_real_nc_line() {
        let input = input();
        let text = "G21 G90 G17 G94 G54\nG0 X-4 Y0 Z15\nT1 M6\nS6000 M3\nG1 Z5 F100\nG1 X4 F300\nG1 X-4\nM5\nM30";
        let parsed = nc::parse(text, &input.tools).unwrap();
        let report = ready(verify(&input, &parsed, limits(0.4), || false, || std::future::ready(()))).unwrap();
        assert_eq!(report.status, Status::Fail);
        assert!(report.complete);
        // The return cut sees the actual preceding removal at identical Z.
        assert_eq!(report.findings.len(), 1, "{report:?}");
        assert_eq!(report.findings[0].line, 6);
        assert!(report.findings[0].source.is_none());
        // Eroding each grid column by the declared XY uncertainty introduces
        // conservative gaps at shared edges. Its lower certificate need not
        // approach one full diameter to prove overload; the analytic full-slot
        // immersion (2R = 2 mm) must remain inside the reported interval.
        let first = &report.findings[0];
        assert!(first.load_low.unwrap() > 0.4 && first.load_low.unwrap() <= 2.);
        assert_eq!(first.load_high, Some(2.));
        let allowed = ready(verify(&input, &parsed, limits(2.), || false, || std::future::ready(()))).unwrap();
        assert_eq!(allowed.status, Status::Pass);
    }
    #[test]
    fn unsupported_plunge_and_exhaustion_cannot_pass_and_cancellation_returns_no_report() {
        let input = input();
        let parsed = nc::parse("G21 G90 G17 G94 G54\nG0 X0 Y0 Z15\nT1 M6\nS6000 M3\nG1 Z5 F100\nM5\nM30", &input.tools).unwrap();
        let report = ready(verify(&input, &parsed, limits(2.), || false, || std::future::ready(()))).unwrap();
        assert_eq!(report.status, Status::Inconclusive);
        assert!(!report.complete);
        assert_eq!(report.findings[0].line, 5);
        let mut small = limits(2.);
        small.work = 1;
        assert_eq!(ready(verify(&input, &parsed, small, || false, || std::future::ready(()))).unwrap().status, Status::Inconclusive);
        assert!(ready(verify(&input, &parsed, limits(2.), || true, || std::future::ready(()))).is_err());
    }
}
