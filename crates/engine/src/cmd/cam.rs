//! Manufacturing commands shared by UI, CLI and MCP.
use super::CommandSpec;
use crate::{EngineError, Result, Session, cam, manufacturing as jobs};
use serde_json::{Value, json};
use std::sync::atomic::Ordering;

pub static COMMANDS: &[CommandSpec] = &[
    CommandSpec::new("cam.boundary.from_profile", "Use Sketch Profile", from_profile)
        .noundo()
        .params("setup: ID, sketch: feature ID, profile: index, occurrence?: [IDs]; returns boundary and durable source reference"),
    CommandSpec::new("cam.setup.put", "Milling Setup", setup)
        .at("MILLING", "SETUP")
        .icon("box")
        .params("setup: complete Setup object (create or replace by ID)"),
    CommandSpec::new("cam.tool.put", "Tool Library", tool).at("MILLING", "SETUP").icon("hole").params("tool: complete Tool object"),
    CommandSpec::new("cam.operation.put", "Milling Operation", operation)
        .at("MILLING", "TOOLPATHS")
        .icon("cam_path")
        .params("operation: complete Operation object; strategy face, contour, pocket or drill"),
    CommandSpec::new("cam.program.put", "NC Program", program)
        .at("MILLING", "SETUP")
        .icon("save")
        .params("program: {id,name,setup,operations:[ordered IDs]}"),
    CommandSpec::new("cam.operation.order", "Reorder Operations", order).params("program: ID, operations: permutation of existing operation IDs"),
    CommandSpec::new("cam.delete", "Delete Manufacturing Item", delete)
        .params("kind: setup|tool|operation|program, id; referenced items cannot be deleted"),
    CommandSpec::new("cam.inspect", "Inspect Manufacturing", inspect).noundo().params("job?: ID; include_path?: bool"),
    CommandSpec::new("cam.generate", "Generate Toolpaths", generate)
        .at("MILLING", "TOOLPATHS")
        .icon("cam_generate")
        .noundo()
        .params("program: ID, background?: bool (default false); returns job ID"),
    CommandSpec::new("cam.job.status", "CAM Job Status", status).noundo().params("job: ID"),
    CommandSpec::new("cam.job.cancel", "Cancel CAM Job", cancel).noundo().params("job: ID"),
    CommandSpec::new("cam.simulate", "Simulate Toolpaths", simulate)
        .at("MILLING", "VERIFY")
        .icon("cam_verify")
        .noundo()
        .params("job: ID, background?: bool"),
    CommandSpec::new("cam.post", "Post Reference G-code", post)
        .at("MILLING", "EXPORT")
        .icon("save")
        .noundo()
        .params("job: ID, path?: string; requires current passing verification"),
    CommandSpec::new("cam.export", "Export CAM Artifact", export)
        .noundo()
        .params("job: ID, format: toolpath|setup_sheet|stock|validation, path?: string"),
];
fn from_profile(s: &mut Session, p: &Value) -> Result<Value> {
    let setup = id(p, "setup")?;
    let sketch = id(p, "sketch")?;
    let profile = id(p, "profile")? as usize;
    let state = s.model.state();
    let profile = state
        .sketches
        .iter()
        .find(|x| x.feature == sketch)
        .and_then(|s| s.profiles.get(profile))
        .ok_or_else(|| EngineError::Other("selected profile not found".into()))?;
    let source = cam::ProfileRef {
        sketch,
        curves: profile.curve_ids(),
        occurrence: p.get("occurrence").map(|_| decode(p, "occurrence")).transpose()?.unwrap_or_default(),
    };
    let setup = s.doc.manufacturing.setups.iter().find(|x| x.id == setup).ok_or_else(|| EngineError::Other("setup not found".into()))?;
    let boundary = jobs::profile_boundary(s, setup, &source, cam::TOLERANCE)?;
    Ok(json!({"boundary":boundary,"source":source}))
}
fn decode<T: serde::de::DeserializeOwned>(p: &Value, key: &str) -> Result<T> {
    serde_json::from_value(p.get(key).cloned().ok_or_else(|| EngineError::Other(format!("missing {key}")))?)
        .map_err(|e| EngineError::Other(e.to_string()))
}
fn id(p: &Value, key: &str) -> Result<u64> {
    p.get(key).and_then(Value::as_u64).ok_or_else(|| EngineError::Other(format!("{key} must be an ID")))
}
macro_rules! put {($fn:ident,$key:literal,$ty:ty,$field:ident)=>{fn $fn(s:&mut Session,p:&Value)->Result<Value>{
    let item:$ty=decode(p,$key)?;let id=item.id;let mut m=s.doc.manufacturing.clone();
    if let Some(old)=m.$field.iter_mut().find(|x|x.id==id){*old=item;}else{m.$field.push(item);}
    m.validate().map_err(jobs::error)?;s.doc_mut().manufacturing=m;Ok(json!({"id":id}))
}};}
put!(setup, "setup", cam::Setup, setups);
put!(tool, "tool", cam::Tool, tools);
put!(operation, "operation", cam::Operation, operations);
put!(program, "program", cam::Program, programs);
fn order(s: &mut Session, p: &Value) -> Result<Value> {
    let program = id(p, "program")?;
    let ids: Vec<u64> = decode(p, "operations")?;
    let mut m = s.doc.manufacturing.clone();
    let target = m.programs.iter_mut().find(|x| x.id == program).ok_or_else(|| EngineError::Other("program not found".into()))?;
    let mut a = target.operations.clone();
    let mut b = ids.clone();
    a.sort();
    b.sort();
    if a != b {
        return Err(EngineError::Other("order must be a permutation of the program operations".into()));
    }
    target.operations = ids;
    m.validate().map_err(jobs::error)?;
    s.doc_mut().manufacturing = m;
    Ok(json!({"program":program}))
}
fn delete(s: &mut Session, p: &Value) -> Result<Value> {
    let id = id(p, "id")?;
    let mut m = s.doc.manufacturing.clone();
    let removed = match p.get("kind").and_then(Value::as_str) {
        Some("setup") => {
            let n = m.setups.len();
            m.setups.retain(|x| x.id != id);
            n != m.setups.len()
        }
        Some("tool") => {
            let n = m.tools.len();
            m.tools.retain(|x| x.id != id);
            n != m.tools.len()
        }
        Some("operation") => {
            let n = m.operations.len();
            m.operations.retain(|x| x.id != id);
            n != m.operations.len()
        }
        Some("program") => {
            let n = m.programs.len();
            m.programs.retain(|x| x.id != id);
            n != m.programs.len()
        }
        _ => return Err(EngineError::Other("unknown CAM item kind".into())),
    };
    if !removed {
        return Err(EngineError::Other("CAM item not found".into()));
    }
    m.validate().map_err(jobs::error)?;
    s.doc_mut().manufacturing = m;
    Ok(json!({"deleted":id}))
}
fn inspect(s: &mut Session, p: &Value) -> Result<Value> {
    if p.get("job").is_none() {
        return Ok(json!({"manufacturing":s.doc.manufacturing,"jobs":s.cam_jobs.jobs.keys().collect::<Vec<_>>()}));
    }
    let job = jobs::job(s, id(p, "job")?)?;
    let mut result = status(s, p)?;
    match job.state()? {
        jobs::JobState::Generated(path) | jobs::JobState::Verified(path, _) if p.get("include_path").and_then(Value::as_bool) == Some(true) => {
            result["toolpath"] = json!(path);
        }
        _ => {}
    }
    Ok(result)
}
fn generate(s: &mut Session, p: &Value) -> Result<Value> {
    let id = jobs::start(s, id(p, "program")?, p.get("background").and_then(Value::as_bool).unwrap_or(false))?;
    status(s, &json!({"job":id}))
}
fn status(s: &mut Session, p: &Value) -> Result<Value> {
    let id = id(p, "job")?;
    let job = jobs::job(s, id)?;
    #[cfg(target_arch = "wasm32")]
    jobs::advance(&job);
    let stale = jobs::current(s, &job).is_err();
    let mut v = json!({"job":id,"stale":stale,"progress":job.progress.load(Ordering::Relaxed),"fingerprint":job.input.fingerprint});
    match job.state()? {
        jobs::JobState::Verifying(_) => v["state"] = json!("simulating"),
        jobs::JobState::Running => v["state"] = json!("running"),
        jobs::JobState::Cancelled => v["state"] = json!("cancelled"),
        jobs::JobState::Failed(e) => {
            v["state"] = json!("failed");
            v["error"] = json!(e);
        }
        jobs::JobState::Generated(path) => {
            v["state"] = json!("generated");
            v["blocks"] = json!(path.blocks.len());
        }
        jobs::JobState::Verified(path, r) => {
            v["state"] = json!("verified");
            v["blocks"] = json!(path.blocks.len());
            v["verification"] = json!({"status":r.status,"finished_dimensions":r.finished_dimensions,"stock_complete":r.stock_complete,
                "findings":r.findings,"cells":r.cells.len(),"work":r.work});
        }
    }
    Ok(v)
}
fn cancel(s: &mut Session, p: &Value) -> Result<Value> {
    let job = jobs::job(s, id(p, "job")?)?;
    job.cancelled.store(true, Ordering::Relaxed);
    job.set(jobs::JobState::Cancelled);
    status(s, p)
}
fn simulate(s: &mut Session, p: &Value) -> Result<Value> {
    let job = jobs::job(s, id(p, "job")?)?;
    jobs::current(s, &job)?;
    let path = match job.state()? {
        jobs::JobState::Generated(p) | jobs::JobState::Verified(p, _) => p,
        _ => return Err(EngineError::Other("generate a path first; another job may still be running".into())),
    };
    job.set(jobs::JobState::Verifying(path));
    jobs::dispatch(job, p.get("background").and_then(Value::as_bool).unwrap_or(false), true)?;
    status(s, p)
}
fn output(p: &Value, content: String) -> Result<Value> {
    if let Some(path) = p.get("path").and_then(Value::as_str) {
        crate::io::write_atomic(std::path::Path::new(path), content.as_bytes(), false).map_err(|e| EngineError::Other(e.to_string()))?;
        Ok(json!({"path":path,"bytes":content.len()}))
    } else {
        Ok(json!({"content":content}))
    }
}
fn post(s: &mut Session, p: &Value) -> Result<Value> {
    let job = jobs::job(s, id(p, "job")?)?;
    jobs::current(s, &job)?;
    let jobs::JobState::Verified(path, report) = job.state()? else {
        return Err(EngineError::Other("simulate before posting".into()));
    };
    if report.status != cam::verify::Status::Pass || report.finished_dimensions != cam::verify::Status::Pass || !report.stock_complete {
        return Err(EngineError::Other("posting blocked by failed or inconclusive verification".into()));
    }
    cam::nc::round_trip(&path, &job.input.setup, &job.input.tools).map_err(jobs::error)?;
    output(p, cam::nc::post(&path, &job.input.setup).map_err(jobs::error)?)
}
fn export(s: &mut Session, p: &Value) -> Result<Value> {
    let job = jobs::job(s, id(p, "job")?)?;
    jobs::current(s, &job)?;
    let (path, report) = match job.state()? {
        jobs::JobState::Generated(p) => (p, None),
        jobs::JobState::Verified(p, r) => (p, Some(r)),
        _ => return Err(EngineError::Other("no generated result".into())),
    };
    let format = p.get("format").and_then(Value::as_str).unwrap_or("validation");
    output(p, crate::io::cam::artifact(&job.input, &path, report.as_deref(), format)?)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    fn configured() -> Session {
        let mut s = Session::default();
        s.execute("solid.box", &json!({"length":20,"width":20,"height":4})).unwrap();
        let mut i = cam::corpus::fixture("face").unwrap();
        i.setup.models[0].body = s.model.state().bodies[0].name.clone();
        let bounds = s.model.state().bodies[0].mesh().bounds();
        i.setup.stock.min = bounds.min;
        i.setup.stock.max = bounds.max + crate::geom::Vec3::Z;
        i.operations[0].top = bounds.max.z + 1.0;
        i.operations[0].bottom = bounds.max.z;
        s.execute("cam.setup.put", &json!({"setup":i.setup})).unwrap();
        s.execute("cam.tool.put", &json!({"tool":i.tools[0]})).unwrap();
        s.execute("cam.operation.put", &json!({"operation":i.operations[0]})).unwrap();
        s.execute("cam.program.put", &json!({"program":{"id":1,"name":"Plate","setup":1,"operations":[1]}})).unwrap();
        s
    }
    #[test]
    fn save_undo_and_stale_generation() {
        let mut s = configured();
        let saved = crate::io::write_design(&s.doc);
        assert_eq!(crate::io::read_design(&saved).unwrap().manufacturing, s.doc.manufacturing);
        let r = s.execute("cam.generate", &json!({"program":1})).unwrap();
        assert_eq!(r["state"], "generated");
        let id = r["job"].clone();
        s.execute("cam.operation.order", &json!({"program":1,"operations":[1]})).unwrap();
        s.execute("solid.box", &json!({"length":2,"width":2,"height":2})).unwrap();
        assert!(s.execute("cam.post", &json!({"job":id})).is_err());
        assert_eq!(s.execute("cam.job.status", &json!({"job":id})).unwrap()["stale"], true);
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(s.execute("cam.job.status", &json!({"job":id})).unwrap()["stale"], false);
    }
    #[test]
    fn referenced_tool_delete_rolls_back() {
        let mut s = configured();
        let d = s.doc.clone();
        assert!(s.execute("cam.delete", &json!({"kind":"tool","id":1})).is_err());
        assert_eq!(s.doc, d);
    }
    #[test]
    fn missing_body_is_not_guessed() {
        let mut s = configured();
        s.doc_mut().manufacturing.setups[0].models[0].body = "missing".into();
        assert!(s.execute("cam.generate", &json!({"program":1})).is_err());
    }
    #[test]
    fn generate_simulate_post_and_reopen() {
        let mut s = configured();
        let r = s.execute("cam.generate", &json!({"program":1})).unwrap();
        let id = r["job"].clone();
        let v = s.execute("cam.simulate", &json!({"job":id})).unwrap();
        assert_eq!(v["verification"]["status"], "pass", "{v}");
        let posted = s.execute("cam.post", &json!({"job":id})).unwrap();
        assert!(posted["content"].as_str().unwrap().contains("G21 G90 G17 G94 G54"));
        for format in ["toolpath", "setup_sheet", "validation", "stock"] {
            assert!(!s.execute("cam.export", &json!({"job":id,"format":format})).unwrap()["content"].as_str().unwrap().is_empty());
        }
        let doc = crate::io::read_design(&crate::io::write_design(&s.doc)).unwrap();
        let mut reopened = Session::new(doc);
        assert!(reopened.execute("cam.post", &json!({"job":id})).is_err());
        assert_eq!(reopened.execute("cam.generate", &json!({"program":1})).unwrap()["state"], "generated");
        s.execute("cam.job.cancel", &json!({"job":id})).unwrap();
        assert!(s.execute("cam.post", &json!({"job":id})).is_err());
    }
    #[test]
    fn legacy_document_has_no_manufacturing() {
        let mut value: Value = serde_json::from_slice(&crate::io::write_design(&solvecraft_doc::Document::default())).unwrap();
        value["format"] = json!("solvecraft/11");
        value.as_object_mut().unwrap().remove("manufacturing");
        let migrated = crate::io::read_design(value.to_string().as_bytes()).unwrap();
        assert!(migrated.manufacturing.is_empty());
    }
    #[test]
    fn inconclusive_dimensions_cannot_be_posted() {
        let mut s = configured();
        let r = s.execute("cam.generate", &json!({"program":1})).unwrap();
        let id = r["job"].as_u64().unwrap();
        s.execute("cam.simulate", &json!({"job":id})).unwrap();
        let job = jobs::job(&s, id).unwrap();
        if let jobs::JobState::Verified(path, report) = job.state().unwrap() {
            let mut report = (*report).clone();
            report.finished_dimensions = cam::verify::Status::Inconclusive;
            job.set(jobs::JobState::Verified(path, std::sync::Arc::new(report)));
        }
        assert!(s.execute("cam.post", &json!({"job":id})).is_err());
    }
    #[test]
    fn drilled_cad_body_generates_verifies_posts_and_reopens() {
        let mut s = Session::default();
        s.execute("solid.box", &json!({"length":20,"width":20,"height":6})).unwrap();
        s.execute("solid.hole", &json!({"position":[10,10,6],"direction":[0,0,-1],"diameter":1,"depth":1,"type":"drilled","tip_angle":118})).unwrap();
        let mut input = cam::corpus::fixture("drill").unwrap();
        input.setup.models[0].body = s.model.state().bodies[0].name.clone();
        input.operations[0].geometry = cam::Strategy::Drill { holes: vec![[10., 10.]], peck: Some(0.3), full_diameter: true };
        s.execute("cam.setup.put", &json!({"setup":input.setup})).unwrap();
        s.execute("cam.tool.put", &json!({"tool":input.tools[0]})).unwrap();
        s.execute("cam.operation.put", &json!({"operation":input.operations[0]})).unwrap();
        s.execute("cam.program.put", &json!({"program":{"id":1,"name":"Drilled plate","setup":1,"operations":[1]}})).unwrap();
        let generated = s.execute("cam.generate", &json!({"program":1})).unwrap();
        let id = generated["job"].clone();
        let verified = s.execute("cam.simulate", &json!({"job":id})).unwrap();
        assert_eq!(verified["verification"]["status"], "pass", "{verified}");
        assert_eq!(verified["verification"]["finished_dimensions"], "pass", "{verified}");
        assert!(s.execute("cam.post", &json!({"job":id})).unwrap()["content"].as_str().unwrap().contains("M30"));
        let saved = crate::io::write_design(&s.doc);
        let mut reopened = Session::new(crate::io::read_design(&saved).unwrap());
        let generated = reopened.execute("cam.generate", &json!({"program":1})).unwrap();
        let verified = reopened.execute("cam.simulate", &json!({"job":generated["job"]})).unwrap();
        assert_eq!(verified["verification"]["finished_dimensions"], "pass");
    }
}
