use serde_json::json;
use solvecraft_engine::Session;

#[test]
fn oversized_f3d_reads_fail_before_archive_decoding() {
    let path = std::env::temp_dir().join(format!("solvecraft-f3d-cap-{}.f3d", std::process::id()));
    let file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).unwrap();
    file.set_len(solvecraft_engine::io::f3d::MAX_F3D_BYTES as u64 + 1).unwrap();
    drop(file);
    let mut s = Session::default();
    let doc = s.doc.clone();
    let errors: Vec<_> = ["doc.open", "file.inspect_f3d"].into_iter().map(|command| s.execute(command, &json!({"path":path}))).collect();
    let cli = std::process::Command::new(env!("CARGO_BIN_EXE_solvecraft-cli")).arg("bench-open").arg(&path).output().unwrap();
    std::fs::remove_file(path).unwrap();
    for result in errors {
        assert!(result.unwrap_err().to_string().contains("file too large"));
    }
    assert!(!cli.status.success());
    assert!(String::from_utf8_lossy(&cli.stderr).contains("file too large"));
    assert!(std::sync::Arc::ptr_eq(&doc, &s.doc));
}

#[cfg(unix)]
#[test]
fn f3d_entry_points_refuse_non_regular_files() {
    let path = std::env::temp_dir().join(format!("solvecraft-f3d-device-{}.f3d", std::process::id()));
    // /dev/null is finite, so this regression itself cannot hang even if the
    // regular-file guard is accidentally removed in a future change.
    std::os::unix::fs::symlink("/dev/null", &path).unwrap();
    let mut s = Session::default();
    let errors: Vec<_> = ["doc.open", "file.inspect_f3d"].into_iter().map(|command| s.execute(command, &json!({"path":path}))).collect();
    let cli = std::process::Command::new(env!("CARGO_BIN_EXE_solvecraft-cli")).arg("bench-open").arg(&path).output().unwrap();
    std::fs::remove_file(path).unwrap();
    for result in errors {
        assert!(result.unwrap_err().to_string().contains("not a regular file"));
    }
    assert!(!cli.status.success());
    assert!(String::from_utf8_lossy(&cli.stderr).contains("not a regular file"));
}

#[test]
fn unsupported_f3d_open_preserves_the_current_design() {
    use std::io::Write;
    let path = std::env::temp_dir().join(format!("solvecraft-invalid-{}.F3D", std::process::id()));
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).unwrap();
    file.write_all(b"invalid native archive").unwrap();
    drop(file);
    let mut session = Session::default();
    session.execute("solid.box", &json!({"length": 10, "width": 20, "height": 30})).unwrap();
    let doc = session.doc.clone();
    let model = session.model.state();
    let error = session.execute("doc.open", &json!({"path": path})).unwrap_err().to_string();
    std::fs::remove_file(path).unwrap();
    assert!(error.contains("F3D"), "{error}");
    assert!(std::sync::Arc::ptr_eq(&doc, &session.doc));
    assert!(std::sync::Arc::ptr_eq(&model, &session.model.state()));
    assert!(session.is_dirty());
    session.execute("edit.undo", &json!({})).unwrap();
    assert!(session.model.state().bodies.is_empty());
}

/// These user-supplied Fusion files are local evidence and are never committed.
#[test]
#[ignore = "requires SOLVECRAFT_F3D_CORPUS pointing to the supplied test_files directory"]
fn supplied_f3d_indexes() {
    let dir = std::env::var_os("SOLVECRAFT_F3D_CORPUS").expect("set SOLVECRAFT_F3D_CORPUS");
    let mut session = Session::default();
    let doc = session.doc.clone();
    for (name, layout, classes, objects, secondary) in [
        ("adapter-x20-makita.f3d", 4, 213, 5538, 849),
        ("R36S Grip.f3d", 5, 207, 3952, 629),
        ("Gridfinity Storage Box Shelves (Parametric) V1.0.f3d", 5, 223, 4899, 784),
        ("Minimal.f3d", 8, 176, 259, 36),
    ] {
        let path = std::path::Path::new(&dir).join(name);
        let start = std::time::Instant::now();
        let result = session.execute("file.inspect_f3d", &json!({"path": path})).unwrap();
        assert_eq!(result["editable_history_imported"], false);
        let designs = result["designs"].as_array().unwrap();
        assert_eq!(designs.len(), 1);
        let d = &designs[0];
        assert_eq!(d["layout"], layout);
        assert_eq!(d["classes"], classes);
        assert_eq!(d["objects"], objects);
        assert_eq!(d["secondary_records"], secondary);
        if name == "Minimal.f3d" {
            let params = d["parameters"]["records"].as_array().unwrap();
            assert_eq!(params.len(), 3);
            assert_eq!(params[0]["name"], "d1");
            assert_eq!(params[0]["expression"], "100 mm");
            assert_eq!(params[0]["native_value"], 10.0);
            assert_eq!(params[1]["label"], "AlongDistance");
            assert_eq!(params[1]["expression"], "100.00 mm");
            assert_eq!(params[1]["native_value"], 10.0);
            assert_eq!(params[2]["label"], "TaperAngle");
            assert_eq!(params[2]["native_value"], 0.0);
            assert_ne!(params[0]["owner"], params[1]["owner"]);
            assert_eq!(params[1]["owner"], params[2]["owner"]);
        }
        assert!(std::sync::Arc::ptr_eq(&doc, &session.doc));
        assert!(!session.is_dirty());
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
        eprintln!("{name}: {objects} indexed objects, {:?}", start.elapsed());
    }
}

#[test]
#[ignore = "requires SOLVECRAFT_F3D_PROBES pointing to local controlled Fusion exports"]
fn controlled_parameter_edits_match_fusion_api() {
    let dir = std::path::PathBuf::from(std::env::var_os("SOLVECRAFT_F3D_PROBES").expect("set SOLVECRAFT_F3D_PROBES"));
    let mut s = Session::default();
    for measurements_file in ["fusion-measurements.json", "fusion-feature-measurements.json", "fusion-circle-measurements.json"] {
        let measurements: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join(measurements_file)).unwrap()).unwrap();
        for (name, measurement) in measurements.as_object().unwrap() {
            let result = s.execute("file.inspect_f3d", &json!({"path":dir.join(format!("{name}.f3d"))})).unwrap();
            let decoded = result["designs"][0]["parameters"]["records"].as_array().unwrap();
            let api = measurement["parameters"].as_array().unwrap();
            assert_eq!(decoded.len(), api.len());
            for p in api {
                let actual = decoded.iter().find(|r| r["name"] == p["name"]).unwrap();
                assert_eq!(actual["expression"], p["expression"]);
                assert_eq!(actual["display_unit"], p["unit"]);
                assert_eq!(actual["native_value"], p["value"]);
            }
            let circles = &result["designs"][0]["sketch_circles"];
            assert_eq!(circles["coordinate_frame"], "sketch_local");
            assert_eq!(circles["length_unit"], "cm");
            let circles = circles["records"].as_array().unwrap();
            assert_eq!(circles.len(), 1);
            assert_eq!(circles[0]["center"], measurement["circle"]["center"]);
            assert_eq!(circles[0]["radius"], measurement["circle"]["radius"]);
            let features =
                result["designs"][0]["history"]["records"].as_array().unwrap_or_else(|| panic!("{name}: {}", result["designs"][0]["history"]));
            for (f, api) in features.iter().zip(measurement["timeline"].as_array().unwrap()) {
                assert_eq!(f["name"], api["name"], "{name}");
            }
            assert_eq!(features[0]["kind"]["transform"], measurement["transform"], "{name}");
        }
    }
}

#[test]
#[ignore = "requires SOLVECRAFT_F3D_PROBES pointing to local controlled Fusion exports"]
fn controlled_history_matches_fusion_api() {
    let dir = std::path::PathBuf::from(std::env::var_os("SOLVECRAFT_F3D_PROBES").expect("set SOLVECRAFT_F3D_PROBES"));
    let measurements: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("fusion-history-measurements.json")).unwrap()).unwrap();
    let mut s = Session::default();
    for (name, measurement) in measurements.as_object().unwrap() {
        let result = s.execute("file.inspect_f3d", &json!({"path":dir.join(format!("{name}.f3d"))})).unwrap();
        let history = &result["designs"][0]["history"];
        let d = &result["designs"][0];
        let constraints = d["sketch_constraints"]["records"].as_array().unwrap_or_else(|| panic!("{name}: {}", d["sketch_constraints"]));
        let profiles = d["circle_profiles"]["records"].as_array().unwrap_or_else(|| panic!("{name}: {}", d["circle_profiles"]));
        let records = history["records"].as_array().unwrap_or_else(|| panic!("{name}: {history}"));
        let timeline = measurement["timeline"].as_array().unwrap();
        assert_eq!(records.len(), timeline.len(), "{name}");
        for (f, api) in records.iter().zip(timeline) {
            assert_eq!(f["name"], api["name"], "{name}");
            assert_eq!(f["suppressed"], api["suppressed"], "{name}");
            if f["kind"]["type"] == "sketch" {
                let sketch = measurement["sketches"].as_array().unwrap().iter().find(|x| x["name"] == api["name"]).unwrap();
                assert_eq!(f["kind"]["transform"], sketch["transform"], "{name}");
                let decoded = constraints.iter().find(|s| s["geometry"] == f["kind"]["geometry"]).unwrap()["constraints"].as_array().unwrap();
                assert_eq!(decoded.iter().filter(|c| c["type"] == "coincident").count(), sketch["constraints"].as_array().unwrap().len());
                assert_eq!(decoded.iter().filter(|c| c["type"] == "diameter").count(), sketch["dimensions"].as_array().unwrap().len());
            } else {
                let extrude = measurement["extrudes"].as_array().unwrap().iter().find(|x| x["name"] == api["name"]).unwrap();
                let operation = ["join", "cut", "intersect", "new_body"][extrude["operation"].as_u64().unwrap() as usize];
                assert_eq!(f["kind"]["operation"], operation, "{name}");
                let profile = profiles.iter().find(|p| p["feature"] == f["id"]).unwrap();
                let circle = d["sketch_circles"]["records"].as_array().unwrap().iter().find(|c| c["id"] == profile["circle"]).unwrap();
                assert_eq!(circle["sketch"], profile["geometry"]);
            }
        }
    }
}

#[test]
#[ignore = "requires SOLVECRAFT_F3D_PROBES pointing to local controlled Fusion exports"]
fn controlled_construction_and_constraint_removal() {
    let dir = std::path::PathBuf::from(std::env::var_os("SOLVECRAFT_F3D_PROBES").expect("set SOLVECRAFT_F3D_PROBES"));
    let measurements: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("fusion-extent-measurements.json")).unwrap()).unwrap();
    for (name, measurement) in measurements.as_object().unwrap() {
        let designs = solvecraft_engine::io::f3d::read_f3d(&std::fs::read(dir.join(format!("{name}.f3d"))).unwrap()).unwrap();
        let d = &designs[0];
        assert_eq!(d.sketch_circles().unwrap()[0].construction, measurement["construction"].as_bool().unwrap(), "{name}");
        let constraints = d.sketch_constraints().unwrap_or_else(|e| panic!("{name}: {e}"));
        let want = if name == "circle-unconstrained" {
            0
        } else if name == "circle-undimensioned" {
            1
        } else {
            2
        };
        assert_eq!(constraints[0].constraints.len(), want, "{name}");
        if name == "extent-two-sided" {
            assert!(d.history().unwrap_err().to_string().contains("unsupported extrusion extent layout"));
        } else {
            for f in d.history().unwrap() {
                if let solvecraft_engine::io::f3d::HistoryKind::Extrude { profile_containers, .. } = f.kind {
                    assert_eq!(d.selected_circle_profiles(f.id, &profile_containers).unwrap().len(), 1, "{name}");
                }
            }
        }
    }
}
