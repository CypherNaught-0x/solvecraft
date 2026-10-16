use solidcraft_cam::{Motion, ToolKind, corpus, generate, nc, verify};

fn fixture(slope: f64) -> solidcraft_cam::JobInput {
    corpus::fixture(if slope == 0. { "parallel-flat" } else { "parallel-slope" }).unwrap()
}

#[test]
fn flat_parallel_paths_post_and_independently_certify_cusp_and_finished_surface() {
    let input = fixture(0.);
    let path = generate::generate(input.clone()).unwrap();
    let text = nc::post(&path, &input.setup).unwrap();
    let parsed = nc::parse(&text, &input.tools).unwrap();
    let report = verify::simulate(&input, &parsed, Default::default()).unwrap();
    assert_eq!(report.status, verify::Status::Pass, "{:?}", report.findings);
    assert_eq!(report.finished_dimensions, verify::Status::Pass, "{:?}", report.findings);
    assert!(report.stock_complete);
    let z = verify::probe_height(&input, &parsed, [0.5, 0.45]).unwrap();
    let expected = 5.00025 + 1. - (1. - 0.05_f64.powi(2)).sqrt();
    assert!((z - expected).abs() < 1e-6, "{z} vs {expected}");
    // Missed paths and a height fault must not become dimensional successes.
    let missing = nc::parse("G21 G90 G17 G94 G54\nG0 X0 Y0 Z15\nM30", &input.tools).unwrap();
    assert_eq!(verify::simulate(&input, &missing, Default::default()).unwrap().finished_dimensions, verify::Status::Fail);
    let wrong = text.replace("Z5.000250", "Z5.050250");
    let parsed = nc::parse(&wrong, &input.tools).unwrap();
    assert_eq!(verify::simulate(&input, &parsed, Default::default()).unwrap().finished_dimensions, verify::Status::Fail);
}

#[test]
fn sloped_parallel_moves_follow_sphere_plane_contact_and_remain_deterministic() {
    let input = fixture(0.2);
    let first = generate::generate(input.clone()).unwrap();
    let second = generate::generate(input.clone()).unwrap();
    assert_eq!(first.blocks, second.blocks);
    let text = nc::post(&first, &input.setup).unwrap();
    let parsed = nc::parse(&text, &input.tools).unwrap();
    for x in [0.1, 0.4, 0.7, 0.9] {
        let z = verify::probe_height(&input, &parsed, [x, 0.5]).unwrap();
        assert!((z - (5. + 0.2 * x)).abs() < 0.001, "x={x}, z={z}");
    }
    let report = verify::simulate(&input, &parsed, Default::default()).unwrap();
    assert_eq!(
        (report.status, report.finished_dimensions),
        (verify::Status::Pass, verify::Status::Pass),
        "cells={} work={} {:?}",
        report.cells.len(),
        report.work,
        report.findings
    );
    assert!(first.blocks.iter().any(|b| matches!(b.motion, Motion::Linear { to, .. } if to.z > 5.1 && to.z < 5.3)));
}

#[test]
fn parallel_rejects_bad_tools_cusps_meshes_and_inaccessible_heights() {
    let mut input = fixture(0.);
    input.tools[0].kind = ToolKind::FlatEndMill;
    assert!(generate::generate(input.clone()).unwrap_err().0.contains("ball end mill"));
    input.tools[0].kind = ToolKind::BallEndMill;
    input.operations[0].stepover = 1.;
    assert!(generate::generate(input.clone()).unwrap_err().0.contains("cusp"));
    input.operations[0].stepover = 0.1;
    input.geometry.target.pop();
    assert!(generate::generate(input).unwrap_err().0.contains("closed"));
    let mut input = fixture(0.);
    input.operations[0].bottom = 5.1;
    assert!(generate::generate(input).unwrap_err().0.contains("bottom/retract"));
    let mut input = fixture(0.);
    // Tilt the bottom upward: this is an underside the fixed-axis strategy
    // cannot finish, even though a top-only height field could hide it.
    for p in input.geometry.target.iter_mut().flatten() {
        if p.z == 0. {
            p.z = p.x * 0.1;
        }
    }
    assert!(generate::generate(input).unwrap_err().0.contains("undercuts"));
}

#[test]
fn cancellation_during_contact_refinement_discards_every_partial_path() {
    let mut generator = generate::Generator::new(fixture(0.2)).unwrap();
    for _ in 0..20 {
        assert!(!generator.step().unwrap());
    }
    generator.cancel();
    assert!(generator.step().is_err());
    assert!(generator.finish().is_err());
}

#[test]
fn raw_cam_mesh_cracks_are_rejected_without_silent_repair() {
    for crack in [1e-14, 1e-5] {
        let mut input = fixture(0.2);
        input.geometry.target[2][0].x += crack;
        assert!(generate::generate(input).unwrap_err().0.contains("closed"));
    }
}

#[test]
fn declared_top_cannot_hide_uncut_stock_from_a_short_tool() {
    let mut input = fixture(0.);
    input.tools[0].cutting_length = 1.; // R=1: a geometrically valid ball tool.
    input.operations[0].top = 5.;
    // The requested 0.5 mm depth fits the tool, but stock still reaches Z=6.
    // No preceding removal was supplied to justify the shorter cutting reach.
    assert!(generate::generate(input).unwrap_err().0.contains("original stock top"));
}

#[test]
fn curved_dome_bowl_and_saddle_finish_against_independent_analytic_heights() {
    for name in ["parallel-dome", "parallel-bowl", "parallel-saddle"] {
        let input = corpus::fixture(name).unwrap();
        let path = generate::generate(input.clone()).unwrap();
        let parsed = nc::round_trip(&path, &input.setup, &input.tools).unwrap();
        let report = verify::simulate(&input, &parsed, Default::default()).unwrap();
        assert_eq!((report.status, report.finished_dimensions), (verify::Status::Pass, verify::Status::Pass), "{name}: {:?}", report.findings);
        assert!(report.stock_complete);
        let stl = verify::stock_stl(&report, input.setup.stock.min.z).unwrap();
        let mesh = solidcraft_cam::compare::read_stl(stl.as_bytes()).unwrap();
        let exported = solidcraft_cam::compare::stock_mesh(&report, &mesh, input.setup.stock.min.z, input.operations[0].tolerance).unwrap();
        assert_eq!(exported.status, verify::Status::Pass, "{name}: {exported:?}");
        for (xy, expected) in corpus::probes(name, 1).unwrap() {
            let actual = verify::probe_height(&input, &parsed, xy).unwrap();
            assert!((actual - expected).abs() < 0.004, "{name} {xy:?}: {actual} vs {expected}");
        }
    }
}
