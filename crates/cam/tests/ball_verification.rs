//! Hand-authored NC and analytic solids, independent of toolpath generation.
use solidcraft_cam::{ToolKind, corpus, nc, verify};

fn input() -> solidcraft_cam::JobInput {
    let mut input = corpus::fixture("face").unwrap();
    // Protect a 0.5-mm plate; no generator or reference product supplies it.
    for p in input.geometry.target.iter_mut().flatten() {
        p.z *= 0.1;
    }
    let mut ball = input.tools[0].clone();
    ball.id = 2;
    ball.number = 2;
    ball.kind = ToolKind::BallEndMill;
    ball.diameter = 10.;
    input.tools.push(ball);
    input
}

fn program(motion: &str) -> String {
    format!("G21 G90 G17 G94 G54\nG0 X0 Y0 Z15\nT2 M6\nS6000 M3\nG0 X-6 Y10\nG1 Z1 F100\n{motion}\nG0 Z15\nM5\nM30")
}

#[test]
fn hand_authored_ball_pass_has_spherical_cross_section_and_line_linked_rapid_failure() {
    let input = input();
    let nc = nc::parse(&program("G1 X26 F300"), &input.tools).unwrap();
    assert!((verify::probe_height(&input, &nc, [10., 10.]).unwrap() - 1.).abs() < 1e-10);
    assert!((verify::probe_height(&input, &nc, [10., 13.]).unwrap() - 2.).abs() < 1e-10);
    assert!((verify::probe_height(&input, &nc, [10., 14.]).unwrap() - 3.).abs() < 1e-10);
    assert_eq!(verify::probe_height(&input, &nc, [10., 16.]).unwrap(), 6.);
    // Qualify whole-cell intervals on a small physical region around the bottom
    // of the groove. A full steep-sided groove can exhaust the height-field
    // budget and must remain inconclusive rather than weakening its intervals.
    let mut input = input;
    input.setup.stock.min.x = 9.9;
    input.setup.stock.min.y = 9.9;
    input.setup.stock.max.x = 10.1;
    input.setup.stock.max.y = 10.1;
    for p in input.geometry.target.iter_mut().flatten() {
        p.x = 9.9 + p.x / 100.;
        p.y = 9.9 + p.y / 100.;
    }
    let report = verify::simulate(&input, &nc, Default::default()).unwrap();
    assert_eq!(report.status, verify::Status::Pass, "{:?}", report.findings);
    assert!(report.stock_complete);
    for cell in &report.cells {
        let y = (cell.min[1] + cell.max[1]) / 2.;
        let analytic = 6. - (25. - (y - 10.).powi(2)).sqrt();
        assert!(cell.low <= analytic && cell.high >= analytic);
    }
    // A single ball pass is not a finished face; adding a tool must not weaken
    // existing dimensional acceptance or make incomplete milling postable.
    assert_eq!(report.finished_dimensions, verify::Status::Fail);
    let rapid = nc::parse(&program("G0 X26"), &input.tools).unwrap();
    let report = verify::simulate(&input, &rapid, Default::default()).unwrap();
    assert_eq!(report.status, verify::Status::Fail);
    assert!(report.findings.iter().any(|f| f.kind == "rapid_through_stock" && f.line == Some(7)));
}

#[test]
fn wrong_ball_radius_and_sloping_program_are_independently_measurable() {
    let mut input = input();
    let text = program("G1 X26 F300");
    let parsed = nc::parse(&text, &input.tools).unwrap();
    input.tools[1].diameter = 8.;
    let wrong = verify::probe_height(&input, &parsed, [10., 13.]).unwrap();
    assert!((wrong - (5. - 7_f64.sqrt())).abs() < 1e-10);
    assert!((wrong - 2.).abs() > 0.01);
    input.tools[1].diameter = 10.;
    let slope = "G21 G90 G17 G94 G54\nG0 X0 Y0 Z15\nT2 M6\nS6000 M3\nG0 X0 Y10\nG1 Z-5 F100\nG1 X20 Z15 F300\nM5\nM30";
    let parsed = nc::parse(slope, &input.tools).unwrap();
    // Sphere centers trace z=x; the lower swept surface at x=10 is 10-5√2.
    let z = verify::probe_height(&input, &parsed, [10., 10.]).unwrap();
    assert!((z - (10. - 5. * 2_f64.sqrt())).abs() < 1e-10);
}

#[test]
fn ball_dimensions_serialize_and_unsupported_strategies_fail_explicitly() {
    let mut input = input();
    let ball = &mut input.tools[1];
    ball.cutting_length = ball.diameter / 2.;
    assert!(ball.validate().is_ok());
    let json = serde_json::to_value(&ball).unwrap();
    assert_eq!(json["kind"], "ball_end_mill");
    let round_trip: solidcraft_cam::Tool = serde_json::from_value(json).unwrap();
    assert_eq!(&round_trip, ball);
    ball.cutting_length -= 0.01;
    assert!(ball.validate().is_err());
    ball.cutting_length = 20.;
    input.operations[0].tool = ball.id;
    assert!(input.validate().unwrap_err().0.contains("flat end mill"));
}

#[test]
fn cancelled_and_exhausted_ball_verification_cannot_pass() {
    let input = input();
    let parsed = nc::parse(&program("G1 X26 F300"), &input.tools).unwrap();
    assert!(verify::simulate_cancellable(&input, &parsed, Default::default(), || true).is_err());
    let report = verify::simulate(&input, &parsed, verify::Limits { work: 1, ..Default::default() }).unwrap();
    assert_eq!(report.status, verify::Status::Inconclusive);
    assert!(!report.stock_complete);
    assert_ne!(report.finished_dimensions, verify::Status::Pass);
}
