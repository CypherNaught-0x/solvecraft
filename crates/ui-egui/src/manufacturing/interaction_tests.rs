//! Regression scenarios for context changes, parameter drafts and posting controls.
#![allow(clippy::unwrap_used)]
use super::*;
use crate::scenario::Harness;

fn prepared() -> Harness {
    let mut h = Harness::new();
    h.call("ui.resize", json!({"width":1400,"height":1100}));
    h.app.run("solid.box", json!({"length":20,"width":20,"height":5})).unwrap();
    for command in ["cam.setup.put", "cam.tool.put", "cam.operation.put", "cam.program.put"] {
        h.step(&json!({"start":command})).unwrap();
        h.frames(3);
        h.step(&json!({"widget":"Save"})).unwrap();
    }
    h.frames(3);
    h
}

#[test]
fn switching_setup_clears_job_and_creates_operations_in_the_selected_context() {
    let mut h = prepared();
    let mut second = h.app.session.doc.manufacturing.setups[0].clone();
    second.id = 2;
    second.name = "Second setup".into();
    second.stock.max.z = 8.;
    h.app.run("cam.setup.put", json!({"setup":second})).unwrap();
    let job = h.app.run("cam.generate", json!({"program":1})).unwrap()["job"].as_u64().unwrap();
    h.app.manufacturing.job = Some(job);
    h.frames(3);
    h.step(&json!({"widget":"Milling setup"})).unwrap();
    h.step(&json!({"widget":"Second setup"})).unwrap();
    h.frames(3);
    assert_eq!(h.app.manufacturing.setup, 2);
    assert_eq!(h.app.manufacturing.program, 0);
    assert!(h.app.manufacturing.job.is_none());
    assert_eq!(view::setup(&h.app).unwrap().id, 2);
    h.step(&json!({"start":"cam.generate"})).unwrap();
    assert!(h.app.manufacturing.job.is_none(), "a setup without a program must not generate another setup");
    h.step(&json!({"widget":"Add operation"})).unwrap();
    let context = match &h.app.manufacturing.editor {
        Some(Editor::Operation(o)) => Some((o.setup, o.top)),
        _ => None,
    };
    assert_eq!(context, Some((2, 8.)));
    h.step(&json!({"widget":"Save"})).unwrap();
    h.step(&json!({"widget":"New program"})).unwrap();
    h.frames(3);
    h.step(&json!({"widget":"Save"})).unwrap();
    assert_eq!(h.app.manufacturing.program, 2);
    assert_eq!(h.app.session.doc.manufacturing.programs[1].operations, vec![2]);
    h.app.run("cam.delete", json!({"kind":"program","id":2})).unwrap();
    h.frames(3);
    assert_eq!(h.app.manufacturing.program, 0);
    assert!(!workflow::Availability::read(&h.app).post);
}

#[test]
fn operation_tabs_preserve_draft_and_cancel_leaves_document_unchanged() {
    let mut h = prepared();
    let original = h.app.session.doc.manufacturing.operations[0].clone();
    h.step(&json!({"widget":"Facing"})).unwrap();
    h.frames(20);
    assert!(h.widget("Feed mm/min").is_some());
    assert!(h.widget("Bottom Z").is_none());
    h.step(&json!({"widget":"Heights"})).unwrap();
    h.frames(20);
    assert!(h.widget("Bottom Z").is_some());
    assert!(h.widget("Feed mm/min").is_none());
    let at = h.widget_near(&format!("{:.1}", original.bottom), "Bottom Z").unwrap();
    h.step(&json!({"click":at,"double":true})).unwrap();
    h.step(&json!({"key":"A","cmd":true})).unwrap();
    h.step(&json!({"text":"4.5"})).unwrap();
    h.step(&json!({"key":"Enter"})).unwrap();
    h.step(&json!({"widget":"Passes"})).unwrap();
    h.frames(20);
    assert!(h.widget("Stepover").is_some());
    h.step(&json!({"widget":"Heights"})).unwrap();
    h.frames(20);
    assert!(matches!(&h.app.manufacturing.editor, Some(Editor::Operation(o)) if o.bottom == 4.5));
    h.step(&json!({"widget":"Cancel"})).unwrap();
    assert!(h.app.manufacturing.editor.is_none());
    assert_eq!(h.app.session.doc.manufacturing.operations[0], original);
}

#[test]
fn posting_controls_follow_generation_verification_and_staleness() {
    let mut h = prepared();
    assert!(!workflow::Availability::read(&h.app).path);
    let job = h.app.run("cam.generate", json!({"program":1})).unwrap()["job"].as_u64().unwrap();
    h.app.manufacturing.job = Some(job);
    let a = workflow::Availability::read(&h.app);
    assert!(a.path && !a.post && !a.verified);
    h.app.run("cam.simulate", json!({"job":job})).unwrap();
    let a = workflow::Availability::read(&h.app);
    assert!(a.post && a.stock && a.verified);
    let mut operation = h.app.session.doc.manufacturing.operations[0].clone();
    operation.bottom = 4.;
    h.app.run("cam.operation.put", json!({"operation":operation})).unwrap();
    let a = workflow::Availability::read(&h.app);
    assert!(a.stale && !a.path && !a.post && !a.stock);
    let job = h.app.run("cam.generate", json!({"program":1})).unwrap()["job"].as_u64().unwrap();
    h.app.manufacturing.job = Some(job);
    h.app.run("cam.simulate", json!({"job":job})).unwrap();
    let a = workflow::Availability::read(&h.app);
    assert!(a.verified && !a.post);
    assert!(a.export("validation") && !a.export("gcode"));
    let called = std::rc::Rc::new(std::cell::Cell::new(false));
    let picker_called = called.clone();
    h.app.services.pick_save = Some(Box::new(move |_, _| {
        picker_called.set(true);
        Box::pin(async { None })
    }));
    h.frames(3);
    h.step(&json!({"widget":"Post G-code"})).unwrap();
    h.frames(3);
    assert!(!called.get());
}
