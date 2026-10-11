use serde_json::json;
use solvecraft_engine::Session;

#[test]
fn unsupported_f3d_open_preserves_the_current_design() {
    let mut session = Session::default();
    session.execute("solid.box", &json!({"length": 10, "width": 20, "height": 30})).unwrap();
    let doc = session.doc.clone();
    let model = session.model.state();
    let error = session.execute("doc.open", &json!({"path": "unsupported.F3D"})).unwrap_err().to_string();
    assert!(error.contains("editable-history import is not implemented"), "{error}");
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
        }
    }
}
