//! Original small designs with analytic expectations, not captured kernel output.
use serde_json::{Value, json};
use solvecraft_engine::{Session, doc::Document};
use std::path::{Path, PathBuf};

fn recipes() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/history");
    let mut paths: Vec<_> =
        std::fs::read_dir(dir).unwrap().map(|p| p.unwrap().path()).filter(|p| p.extension().is_some_and(|e| e == "json")).collect();
    paths.sort();
    assert_eq!(paths.len(), 21);
    paths
}

fn recipe(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn replay(value: &Value) -> Session {
    let mut s = Session::default();
    for command in value["commands"].as_array().unwrap() {
        s.execute(command["command"].as_str().unwrap(), &command["params"]).unwrap_or_else(|e| panic!("{command}: {e}"));
    }
    s
}

fn expected_volume(value: &Value) -> f64 {
    value["volume_mm3"].as_f64().unwrap_or_else(|| value["volume_pi_mm3"].as_f64().unwrap() * std::f64::consts::PI)
}

fn volume(s: &mut Session, want: f64, label: &str) -> Value {
    for result in &s.model.results {
        assert!(result.error.is_none(), "{label}: {result:?}");
    }
    let measure = s.execute("inspect.measure", &json!({})).unwrap();
    let got = measure["total"]["volume_mm3"].as_f64().unwrap();
    // inspect.measure integrates a tessellation of curved faces. The 0.1%
    // tolerance is the existing design/oracle tolerance, not a saved result.
    assert!((got - want).abs() < want.abs().max(1.0) * 1e-3, "{label}: volume {got} != {want}");
    measure
}

fn bounds(measure: &Value, expected: &Value, label: &str) {
    let Some(bounds) = expected["bounds"].as_array() else { return };
    for (end, key) in [(0, "min"), (1, "max")] {
        for axis in 0..3 {
            let value = measure["bodies"]
                .as_array()
                .unwrap()
                .iter()
                .map(|b| b["bbox"][key][axis].as_f64().unwrap())
                .reduce(|a, b| if end == 0 { a.min(b) } else { a.max(b) })
                .unwrap();
            assert!((value - bounds[end][axis].as_f64().unwrap()).abs() < 1e-5, "{label}: bounds {end}/{axis}: {value}");
        }
    }
}

#[test]
fn simple_history_files_replay_reopen_edit_and_undo() {
    for path in recipes() {
        let label = path.display().to_string();
        let recipe = recipe(&path);
        let native = path.with_extension("solvecraft");
        let doc = solvecraft_engine::io::read_design(&std::fs::read(&native).unwrap()).unwrap();
        let rebuilt = replay(&recipe);
        assert_eq!(doc, *rebuilt.doc, "{label}: authored recipe differs from saved fixture");
        let mut s = Session::default();
        s.execute("doc.open", &json!({"path":native})).unwrap();
        let expected = &recipe["expected"];
        let original_volume = expected_volume(expected);
        let measure = volume(&mut s, original_volume, &label);
        assert_eq!(measure["body_count"], expected["body_count"], "{label}");
        assert_eq!(json!(s.doc.features.iter().map(|f| &f.name).collect::<Vec<_>>()), expected["features"], "{label}");
        // Aggregate world bounds catch incorrect sketch placement and direction.
        bounds(&measure, expected, &label);
        for edit in recipe["edits"].as_array().unwrap() {
            s.execute(edit["command"].as_str().unwrap(), &edit["params"]).unwrap();
            let edited_volume = expected_volume(edit);
            bounds(&volume(&mut s, edited_volume, &label), edit, &label);
            let reopened = Document::from_json(&s.doc.to_json()).unwrap();
            assert_eq!(reopened, *s.doc);
            bounds(&volume(&mut Session::new(reopened), edited_volume, &label), edit, &label);
            s.execute("edit.undo", &json!({})).unwrap();
            bounds(&volume(&mut s, original_volume, &label), expected, &label);
        }
    }
}

#[test]
#[ignore = "explicitly author original SolveCraft fixtures from the checked-in recipes"]
fn write_original_history_fixtures() {
    for path in recipes() {
        let s = replay(&recipe(&path));
        std::fs::write(path.with_extension("solvecraft"), solvecraft_engine::io::write_design(&s.doc)).unwrap();
    }
}
