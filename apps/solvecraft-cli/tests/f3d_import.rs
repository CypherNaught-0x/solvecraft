//! Independent public-API measurements are local evidence, never committed assets.
use serde_json::{Value, json};
use solvecraft_engine::Session;

fn volume(s: &mut Session, want_cm3: f64, label: &str) -> Value {
    for r in &s.model.results {
        assert!(r.error.is_none(), "{label}: {r:?}");
    }
    let measure = s.execute("inspect.measure", &json!({})).unwrap();
    let got = measure["total"]["volume_mm3"].as_f64().unwrap();
    let want = want_cm3 * 1000.;
    assert!((got - want).abs() < want.abs().max(1.) * 1e-3, "{label}: volume {got} != {want}");
    measure
}

#[test]
#[ignore = "requires SOLVECRAFT_F3D_PROBES pointing to controlled local Fusion exports"]
fn native_import_reconstructs_and_regenerates_controlled_history() {
    let dir = std::path::PathBuf::from(std::env::var_os("SOLVECRAFT_F3D_PROBES").expect("set SOLVECRAFT_F3D_PROBES"));
    for source in [
        "fusion-measurements.json",
        "fusion-feature-measurements.json",
        "fusion-circle-measurements.json",
        "fusion-history-measurements.json",
        "fusion-extent-measurements.json",
        "fusion-polygon-measurements.json",
    ] {
        let cases: Value = serde_json::from_slice(&std::fs::read(dir.join(source)).unwrap()).unwrap();
        for (name, api) in cases.as_object().unwrap() {
            if name == "extent-two-sided" {
                continue;
            } // Dedicated unsupported-schema atomicity check below.
            let mut s = Session::default();
            let result = s.execute("doc.open", &json!({"path":dir.join(format!("{name}.f3d"))})).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(result["editable_history_imported"], true);
            assert!(s.path.is_none(), "the source F3D must not become a JSON save target");
            assert!(s.is_dirty());
            let measure = volume(&mut s, api["volume"].as_f64().unwrap(), name);
            if let Some(bounds) = api["bounds"].as_array() {
                for (end, key) in [(0, "min"), (1, "max")] {
                    for axis in 0..3 {
                        let actual = measure["bodies"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|b| b["bbox"][key][axis].as_f64().unwrap())
                            .reduce(|a, b| if end == 0 { a.min(b) } else { a.max(b) })
                            .unwrap();
                        assert!((actual - bounds[end][axis].as_f64().unwrap() * 10.).abs() < 1e-5, "{name}: bounds {end}/{axis}");
                    }
                }
            }
            if let Some(timeline) = api["timeline"].as_array() {
                assert_eq!(s.doc.features.len(), timeline.len(), "{name}");
                for (f, t) in s.doc.features.iter().zip(timeline) {
                    assert_eq!(f.name, t["name"].as_str().unwrap(), "{name}");
                }
            }
            // Expressions are retained as dependencies, not replaced by cached values.
            for p in api["parameters"].as_array().unwrap() {
                let imported = s.doc.params.iter().find(|i| i.name == p["name"].as_str().unwrap()).unwrap();
                assert_eq!(imported.expr, p["expression"].as_str().unwrap(), "{name}");
            }
            let doc = solvecraft_engine::io::read_design(&solvecraft_engine::io::write_design(&s.doc)).unwrap();
            assert_eq!(doc, *s.doc, "{name}: round trip");
            volume(&mut Session::new(doc), api["volume"].as_f64().unwrap(), name);
        }
    }
}

#[test]
#[ignore = "requires SOLVECRAFT_F3D_PROBES pointing to controlled local Fusion exports"]
fn native_fixation_matches_independent_point_and_circle_controls() {
    let dir = std::path::PathBuf::from(std::env::var_os("SOLVECRAFT_F3D_PROBES").expect("set SOLVECRAFT_F3D_PROBES"));
    let cases: Value = serde_json::from_slice(&std::fs::read(dir.join("fusion-point-measurements.json")).unwrap()).unwrap();
    for (name, api) in cases.as_object().unwrap() {
        let mut s = Session::default();
        s.execute("doc.open", &json!({"path":dir.join(format!("{name}.f3d"))})).unwrap_or_else(|e| panic!("{name}: {e}"));
        let solvecraft_engine::doc::FeatureKind::Sketch { sketch, .. } = &s.doc.features[0].kind else { panic!("sketch") };
        assert_eq!(sketch.curves.len(), 1);
        assert_eq!(sketch.curves[0].fixed, api["circle_fixed"].as_bool().unwrap(), "{name}");
        assert_eq!(sketch.points.len(), api["points"].as_array().unwrap().len(), "{name}");
        assert_eq!(sketch.points[0].fixed, api["origin_fixed"].as_bool().unwrap(), "{name}");
        assert_eq!(sketch.points[1].fixed, api["center_fixed"].as_bool().unwrap(), "{name}");
    }
}

#[test]
#[ignore = "requires SOLVECRAFT_F3D_CORPUS pointing to the supplied archives"]
fn supplied_minimal_imports_without_a_fusion_roundtrip() {
    let dir = std::path::PathBuf::from(std::env::var_os("SOLVECRAFT_F3D_CORPUS").expect("set SOLVECRAFT_F3D_CORPUS"));
    let mut s = Session::default();
    s.execute("doc.open", &json!({"path":dir.join("Minimal.f3d")})).unwrap();
    assert_eq!(s.doc.features.len(), 2);
    volume(&mut s, std::f64::consts::PI * 5. * 5. * 10., "supplied Minimal");
}

#[test]
#[ignore = "requires SOLVECRAFT_F3D_PROBES pointing to controlled local Fusion exports"]
fn imported_rectangle_dimensions_regenerate_and_undo() {
    let dir = std::path::PathBuf::from(std::env::var_os("SOLVECRAFT_F3D_PROBES").expect("set SOLVECRAFT_F3D_PROBES"));
    let mut s = Session::default();
    s.execute("doc.open", &json!({"path":dir.join("rectangle-baseline.f3d")})).unwrap();
    let original = s.doc.clone();
    for (name, expr, want) in [("d1", "45 mm", 36.), ("d2", "35 mm", 42.), ("d3", "55 mm", 33.), ("d1", "d2 * 2", 32.)] {
        s.execute("parameters.change", &json!({"name":name,"expression":expr})).unwrap();
        volume(&mut s, want, name);
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(*s.doc, *original);
    }
}

#[test]
#[ignore = "requires SOLVECRAFT_F3D_PROBES pointing to controlled local Fusion exports"]
fn isolated_lines_keep_endpoints_fixation_and_construction() {
    let dir = std::path::PathBuf::from(std::env::var_os("SOLVECRAFT_F3D_PROBES").expect("set SOLVECRAFT_F3D_PROBES"));
    let cases: Value = serde_json::from_slice(&std::fs::read(dir.join("fusion-line-measurements.json")).unwrap()).unwrap();
    for (name, api) in cases.as_object().unwrap() {
        let mut s = Session::default();
        s.execute("doc.open", &json!({"path":dir.join(format!("{name}.f3d"))})).unwrap();
        let solvecraft_engine::doc::FeatureKind::Sketch { sketch, .. } = &s.doc.features[0].kind else { panic!("sketch") };
        let line = &sketch.curves[0];
        assert_eq!(line.fixed, api["fixed"].as_bool().unwrap());
        assert_eq!(line.construction, api["construction"].as_bool().unwrap());
        let solvecraft_engine::doc::sketch::CurveKind::Line { a, b } = line.kind else { panic!("line") };
        for (p, key) in [(a, "start"), (b, "end")] {
            let point = sketch.points[p].pos;
            assert!((point.x - api[key][0].as_f64().unwrap() * 10.).abs() < 1e-8);
            assert!((point.y - api[key][1].as_f64().unwrap() * 10.).abs() < 1e-8);
        }
    }
}

#[test]
#[ignore = "requires SOLVECRAFT_F3D_PROBES pointing to controlled local Fusion exports"]
fn driven_dimensions_cannot_silently_become_driving_constraints() {
    let dir = std::path::PathBuf::from(std::env::var_os("SOLVECRAFT_F3D_PROBES").expect("set SOLVECRAFT_F3D_PROBES"));
    let cases: Value = serde_json::from_slice(&std::fs::read(dir.join("fusion-dimension-measurements.json")).unwrap()).unwrap();
    for (name, api) in cases.as_object().unwrap() {
        let mut s = Session::default();
        let before = s.doc.clone();
        let model = s.model.state();
        let result = s.execute("doc.open", &json!({"path":dir.join(format!("{name}.f3d"))}));
        if api["driving"].as_bool().unwrap() {
            result.unwrap_or_else(|e| panic!("{name}: {e}"));
        } else {
            assert!(result.unwrap_err().to_string().contains("parameter expression metadata"), "{name}");
            assert!(std::sync::Arc::ptr_eq(&before, &s.doc));
            assert!(std::sync::Arc::ptr_eq(&model, &s.model.state()));
        }
    }
}

#[test]
#[ignore = "requires SOLVECRAFT_F3D_PROBES pointing to controlled local Fusion exports"]
fn native_parameters_remain_editable_and_failed_import_is_atomic() {
    let dir = std::path::PathBuf::from(std::env::var_os("SOLVECRAFT_F3D_PROBES").expect("set SOLVECRAFT_F3D_PROBES"));
    let mut s = Session::default();
    s.execute("doc.open", &json!({"path":dir.join("minimal-roundtrip.f3d")})).unwrap();
    let original = s.doc.clone();
    for (name, expression, want) in [
        ("d1", "80 mm", std::f64::consts::PI * 4. * 4. * 10.),
        ("d2", "125 mm", std::f64::consts::PI * 5. * 5. * 12.5),
        ("d2", "d1 / 2", std::f64::consts::PI * 5. * 5. * 5.),
    ] {
        s.execute("parameters.change", &json!({"name":name,"expression":expression})).unwrap();
        volume(&mut s, want, "edit");
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(*s.doc, *original);
    }
    let before = s.doc.clone();
    let model = s.model.state();
    assert!(s.execute("doc.open", &json!({"path":dir.join("extent-two-sided.f3d")})).is_err());
    assert!(std::sync::Arc::ptr_eq(&before, &s.doc));
    assert!(std::sync::Arc::ptr_eq(&model, &s.model.state()));
}
