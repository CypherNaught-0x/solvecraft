//! Cold project opening, including display preparation. Run in a child so a
//! performance regression has a deadline even when geometry evaluation hangs.
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::json;
use solvecraft_engine::Session;

const CHILD: &str = "SOLVECRAFT_OPEN_PERF_CHILD";

#[test]
fn hard_edge_basin_open_performance() {
    if std::env::var_os(CHILD).is_some() {
        check_basin();
        return;
    }
    // Optimized CI baseline is ~2 seconds on an M-series Mac. Leave generous
    // headroom for shared CI hosts; the reported regression took ~90 seconds.
    let budget = Duration::from_secs(20);
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "hard_edge_basin_open_performance", "--nocapture"])
        .env(CHILD, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "HardEdgeBasin opening checks failed: {status}");
            break;
        }
        if start.elapsed() >= budget {
            let _ = child.kill();
            let _ = child.wait();
            panic!("HardEdgeBasin cold open exceeded {budget:?}; run solvecraft-cli bench-open on tests/performance/HardEdgeBasin.solvecraft");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn check_basin() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/performance/HardEdgeBasin.solvecraft");
    let original = std::fs::read(&path).unwrap();
    let start = Instant::now();
    let mut session = Session::default();
    let result = session.execute("doc.open", &json!({"path": path})).unwrap();
    assert_eq!(result["features"], 58);
    assert_eq!(session.path.as_deref(), path.to_str());
    assert!(!session.is_dirty());
    assert!(session.model.param_errors.is_empty());
    assert_eq!(session.model.results.len(), 58);
    assert!(session.model.results.iter().all(|r| !r.skipped));
    // Upstream does not yet support these endpoint fillets. Pin their identities
    // and diagnostics, rather than mistaking fast failed evaluation for success.
    let errors: Vec<_> = session.model.results.iter().filter_map(|r| r.error.as_ref().map(|e| (r.id, e.as_str()))).collect();
    assert_eq!(errors.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [102, 103, 104, 105, 106, 107, 108, 109]);
    assert!(
        errors.iter().all(|(_, e)| {
            *e == "not supported yet: each end of the edge must touch exactly one more face"
                || *e == "not supported yet: the faces at the edge ends must be planar"
        }),
        "unexpected feature failures: {errors:?}"
    );
    let world = session.world_state();
    assert_eq!(world.bodies.len(), 14);
    let bounds = world.body("Basin").unwrap().mesh().bounds();
    assert!((bounds.max.x - bounds.min.x - 900.0).abs() < 0.01);
    assert!((bounds.max.y - bounds.min.y - 480.0).abs() < 0.01);
    assert!((bounds.max.z - 850.0).abs() < 0.01);
    let meshes: Vec<_> = world.bodies.iter().map(|body| body.mesh()).collect();
    assert!(meshes.iter().all(|mesh| !mesh.triangles.is_empty()));
    let triangles: usize = meshes.iter().map(|mesh| mesh.triangles.len()).sum();
    assert!(triangles > 10_000, "display geometry was lost: {triangles} triangles");
    eprintln!("HardEdgeBasin: open + display {:?}, {} bodies, {triangles} triangles", start.elapsed(), world.bodies.len());

    // Redraw/refresh after adoption must reuse the evaluated timeline and meshes.
    let warm = Instant::now();
    session.refresh();
    assert_eq!(session.model.last_recomputed, 0);
    let again = session.world_state();
    for (old, body) in meshes.iter().zip(&again.bodies) {
        assert!(std::sync::Arc::ptr_eq(old, &body.mesh()), "refresh remeshed {}", body.name);
    }
    assert!(warm.elapsed() < Duration::from_secs(2), "warm refresh took {:?}", warm.elapsed());
    assert_eq!(std::fs::read(path).unwrap(), original, "opening modified the source design");
}
