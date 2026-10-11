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
        "fusion-loop-measurements.json",
        "fusion-arc-measurements.json",
        "fusion-plane-measurements.json",
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
            let definitions = s.doc.all_param_exprs();
            for p in api["parameters"].as_array().unwrap() {
                let imported = definitions.iter().find(|i| i.0 == p["name"].as_str().unwrap()).unwrap();
                assert_eq!(imported.1, p["expression"].as_str().unwrap(), "{name}");
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
fn imported_plane_dependencies_move_bodies_after_parameter_and_feature_edits() {
    let dir = std::path::PathBuf::from(std::env::var_os("SOLVECRAFT_F3D_PROBES").expect("set SOLVECRAFT_F3D_PROBES"));
    for (name, axis, child) in [("plane-xy-baseline", 2, 0.), ("plane-xz-baseline", 1, 0.), ("plane-chained-baseline", 2, 10.)] {
        let mut s = Session::default();
        s.execute("doc.open", &json!({"path":dir.join(format!("{name}.f3d"))})).unwrap_or_else(|e| panic!("{name}: {e}"));
        let original = s.doc.clone();
        assert_eq!(s.doc.features[0].param_names, ["d1"]);
        for (command, params, start) in [
            ("parameters.change", json!({"name":"d1","expression":"35 mm"}), 35. + child),
            ("timeline.edit", json!({"feature":s.doc.features[0].id,"set":{"plane":{"distance":"-20 mm"}}}), -20. + child),
        ] {
            s.execute(command, &params).unwrap();
            let measure = volume(&mut s, 1.5 * std::f64::consts::PI, name);
            assert!((measure["bodies"][0]["bbox"]["min"][axis].as_f64().unwrap() - start).abs() < 1e-5, "{name}");
            assert!((measure["bodies"][0]["bbox"]["max"][axis].as_f64().unwrap() - start - 15.).abs() < 1e-5, "{name}");
            let doc = solvecraft_engine::io::read_design(&solvecraft_engine::io::write_design(&s.doc)).unwrap();
            assert_eq!(doc, *s.doc);
            let measure = volume(&mut Session::new(doc), 1.5 * std::f64::consts::PI, name);
            assert!((measure["bodies"][0]["bbox"]["min"][axis].as_f64().unwrap() - start).abs() < 1e-5, "{name}: reopen");
            s.execute("edit.undo", &json!({})).unwrap();
            assert_eq!(s.doc, original);
        }
    }
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
    assert_eq!(s.doc.features[1].param_names, ["d3", "d4"]);
    s.execute("parameters.change", &json!({"name":"d1","expression":"d3 / 2"})).unwrap();
    let before = s.doc.clone();
    let extrude = s.doc.features[1].id;
    s.execute("timeline.edit", &json!({"feature":extrude,"set":{"extent":{"distance":"60 mm"}}})).unwrap();
    volume(&mut s, 36., "native feature input drives width");
    let rows = s.doc.param_rows();
    assert_eq!(rows.len(), 4);
    let height = rows.iter().find(|p| p.name == "d3").unwrap();
    assert_eq!(height.source, "feature");
    assert_eq!(height.expression, "60 mm");
    let reopened = solvecraft_engine::io::read_design(&solvecraft_engine::io::write_design(&s.doc)).unwrap();
    volume(&mut Session::new(reopened), 36., "reopened native feature input");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(*s.doc, *before);
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
fn imported_arcs_preserve_api_endpoints_and_independent_flags() {
    let dir = std::path::PathBuf::from(std::env::var_os("SOLVECRAFT_F3D_PROBES").expect("set SOLVECRAFT_F3D_PROBES"));
    let cases: Value = serde_json::from_slice(&std::fs::read(dir.join("fusion-arc-measurements.json")).unwrap()).unwrap();
    for (name, api) in cases.as_object().unwrap() {
        let mut s = Session::default();
        s.execute("doc.open", &json!({"path":dir.join(format!("{name}.f3d"))})).unwrap_or_else(|e| panic!("{name}: {e}"));
        let solvecraft_engine::doc::FeatureKind::Sketch { sketch, .. } = &s.doc.features[0].kind else { panic!("sketch") };
        let (curve, indices) = sketch
            .curves
            .iter()
            .find_map(
                |curve| {
                    if let solvecraft_engine::doc::sketch::CurveKind::Arc { c, a, b } = curve.kind { Some((curve, [c, a, b])) } else { None }
                },
            )
            .unwrap();
        assert_eq!(curve.fixed, api["arc"]["fixed"].as_bool().unwrap(), "{name}");
        assert_eq!(curve.construction, api["arc"]["construction"].as_bool().unwrap(), "{name}");
        for (index, key) in indices.into_iter().zip(["center", "start", "end"]) {
            let p = sketch.points[index].pos;
            assert!((p.x - api["arc"][key][0].as_f64().unwrap() * 10.).abs() < 1e-8, "{name}: {key} x");
            assert!((p.y - api["arc"][key][1].as_f64().unwrap() * 10.).abs() < 1e-8, "{name}: {key} y");
        }
    }
}

#[test]
#[ignore = "requires SOLVECRAFT_F3D_PROBES pointing to controlled local Fusion exports"]
fn imported_hole_dimensions_regenerate_reopen_and_undo() {
    let dir = std::path::PathBuf::from(std::env::var_os("SOLVECRAFT_F3D_PROBES").expect("set SOLVECRAFT_F3D_PROBES"));
    let mut s = Session::default();
    s.execute("doc.open", &json!({"path":dir.join("ring-baseline.f3d")})).unwrap();
    let original = s.doc.clone();
    for (name, expression, pi) in [("d1", "70 mm", 22.5), ("d2", "30 mm", 13.5), ("d3", "35 mm", 28.)] {
        s.execute("parameters.change", &json!({"name":name,"expression":expression})).unwrap();
        volume(&mut s, pi * std::f64::consts::PI, "edited annulus");
        let reopened = solvecraft_engine::io::read_design(&solvecraft_engine::io::write_design(&s.doc)).unwrap();
        volume(&mut Session::new(reopened), pi * std::f64::consts::PI, "reopened annulus");
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(*s.doc, *original);
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

#[test]
#[ignore = "requires SOLVECRAFT_F3D_PROBES pointing to controlled local Fusion exports"]
fn radius_and_arc_diameter_controls_preserve_values_edits_and_reopening() {
    let dir = std::path::PathBuf::from(std::env::var_os("SOLVECRAFT_F3D_PROBES").expect("set SOLVECRAFT_F3D_PROBES"));
    let cases: Value = serde_json::from_slice(&std::fs::read(dir.join("fusion-radial-measurements.json")).unwrap()).unwrap();
    for (name, api) in cases.as_object().unwrap() {
        let mut s = Session::default();
        let before = s.doc.clone();
        let model = s.model.state();
        let result = s.execute("doc.open", &json!({"path":dir.join(format!("{name}.f3d"))}));
        if !api["driving"].as_bool().unwrap() {
            assert!(result.unwrap_err().to_string().contains("parameter expression metadata"), "{name}");
            assert!(std::sync::Arc::ptr_eq(&before, &s.doc));
            assert!(std::sync::Arc::ptr_eq(&model, &s.model.state()));
            continue;
        }
        result.unwrap_or_else(|e| panic!("{name}: {e}"));
        volume(&mut s, api["volume"].as_f64().unwrap(), name);
        let radial = !name.starts_with("arc-diameter");
        let solvecraft_engine::doc::FeatureKind::Sketch { sketch, .. } = &s.doc.features[0].kind else { panic!("sketch") };
        let dimension = sketch.constraints.iter().find(|c| c.param.as_deref() == Some("d1")).unwrap();
        assert_eq!(matches!(dimension.kind, solvecraft_engine::doc::sketch::ConstraintKind::Radius { .. }), radial, "{name}");
        let original = s.doc.clone();
        s.execute("parameters.change", &json!({"name":"d1","expression":if radial {"40 mm"} else {"80 mm"}})).unwrap();
        let want = if name.starts_with("circle") { 32. } else { 16. } * std::f64::consts::PI;
        volume(&mut s, want, name);
        let reopened = solvecraft_engine::io::read_design(&solvecraft_engine::io::write_design(&s.doc)).unwrap();
        volume(&mut Session::new(reopened), want, name);
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(*s.doc, *original);
    }
}

#[test]
#[ignore = "requires SOLVECRAFT_F3D_PROBES pointing to controlled local Fusion exports"]
fn geometric_controls_preserve_relationships_and_remain_solvable_after_edits() {
    let dir = std::path::PathBuf::from(std::env::var_os("SOLVECRAFT_F3D_PROBES").expect("set SOLVECRAFT_F3D_PROBES"));
    let cases: Value = serde_json::from_slice(&std::fs::read(dir.join("fusion-constraint-measurements.json")).unwrap()).unwrap();
    for (name, api) in cases.as_object().unwrap() {
        let mut s = Session::default();
        s.execute("doc.open", &json!({"path":dir.join(format!("{name}.f3d"))})).unwrap_or_else(|e| panic!("{name}: {e}"));
        let solvecraft_engine::doc::FeatureKind::Sketch { sketch, .. } = &s.doc.features[0].kind else { panic!("sketch") };
        assert_eq!(sketch.constraints.len(), api["constraints"].as_array().unwrap().len(), "{name}");
        assert_eq!(sketch.points.len(), api["points"].as_array().unwrap().len(), "{name}");
        assert_eq!(sketch.curves.len(), api["curves"].as_array().unwrap().len(), "{name}");
        if name.ends_with("-constrained") {
            let native = name.strip_prefix("constraint-").unwrap().strip_suffix("-constrained").unwrap();
            let expected = match native {
                "equal-lines" | "equal-circles" => "equal",
                "point-on-line" => "point_on_curve",
                "horizontal-points" => "horizontal_points",
                "vertical-points" => "vertical_points",
                other => other,
            };
            assert_eq!(serde_json::to_value(&sketch.constraints[0]).unwrap()["type"], expected, "{name}");
            let mut edited = sketch.clone();
            for (i, point) in edited.points.iter_mut().enumerate().filter(|(_, p)| !p.fixed) {
                point.pos.x += (i % 3) as f64 * 0.3;
                point.pos.y -= (i % 2) as f64 * 0.2;
            }
            let report = solvecraft_engine::doc::sketch::solve(&mut edited);
            assert!(report.ok(), "{name}: {report:?}");
            assert!(report.max_residual < 1e-6, "{name}");
        }
        let reopened = solvecraft_engine::io::read_design(&solvecraft_engine::io::write_design(&s.doc)).unwrap();
        assert_eq!(*s.doc, reopened, "{name}");
    }
}
