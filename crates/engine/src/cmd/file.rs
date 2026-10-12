//! Documents and files: new, open (designs and STEP), insert STEP, save, export.

use serde_json::{Value, json};
use solvecraft_doc::Document;
use solvecraft_io::Format;

use super::CommandSpec;
use crate::params::{bad, bool_, str_, string_list};
use crate::{EngineError, Result, Session};

pub static COMMANDS: &[CommandSpec] = &[
    CommandSpec::new("file.new", "New Design", new_doc).icon("new").key("Ctrl+N").noundo().params("name?"),
    CommandSpec::new("doc.open", "Open", open)
        .icon("open")
        .key("Ctrl+O")
        .noundo()
        .params("path: .solvecraft design, or a .step/.stp file (opens as a new design)"),
    CommandSpec::new("file.insert_step", "Insert STEP", insert_step)
        .at("SOLID", "INSERT")
        .icon("import")
        .params("path: .step/.stp file (its bodies join the design as an Import base feature); name?"),
    CommandSpec::new("file.insert_mesh", "Insert Mesh", insert_mesh)
        .at("SOLID", "INSERT")
        .icon("import")
        .params("path: .3mf or .stl file (its meshes join the design as mesh bodies); name?"),
    CommandSpec::new("file.inspect_f3d", "Inspect F3D Archive", inspect_f3d)
        .noundo()
        .params("path: native .f3d archive → supported index, parameter, timeline, placement, circle, profile and constraint records; does not import editable history"),
    CommandSpec::new("file.save", "Save", save).icon("save").key("Ctrl+S").noundo().params("path? (default: current file)"),
    CommandSpec::new("file.save_as", "Save As", save_as).icon("save").noundo().params("path"),
    CommandSpec::new("doc.recovery_list", "Recoverable Designs", recovery_list)
        .noundo()
        .params("dir? (default: the recovery folder) → designs autosaved by apps that crashed or closed with unsaved changes"),
    CommandSpec::new("doc.recover", "Recover Design", recover)
        .noundo()
        .params("id (from doc.recovery_list); dir?; discard?: bool (delete the entry once open) — opens it with its file path, unsaved"),
    CommandSpec::new("doc.recovery_discard", "Discard Recovered Design", recovery_discard).noundo().params("id, or all: true; dir?"),
    CommandSpec::new("file.export", "Export", export)
        .icon("export")
        .noundo()
        .params("path; format?: stl|stla|obj|step|3mf (default from extension); bodies?: [names]"),
    CommandSpec::new("file.save_mesh", "Save As Mesh", save_stl).icon("export").noundo().params("path; bodies?: [names]; ascii?: bool"),
];

fn new_doc(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_(p, "name").unwrap_or("Untitled");
    *s = Session::new(Document::new(name));
    Ok(json!({"name": name}))
}

fn path_arg<'a>(p: &'a Value, cmd: &str) -> Result<&'a str> {
    str_(p, "path").filter(|x| !x.trim().is_empty() && x.len() < 4096).ok_or_else(|| bad(cmd, "`path` is required"))
}

/// Read a STEP file as an Import feature.
fn read_step(path: &str) -> Result<solvecraft_io::StepFeature> {
    let bytes = solvecraft_io::vfs::read(path, solvecraft_io::MAX_STEP_BYTES).map_err(|e| EngineError::Other(format!("{path}: {e}")))?;
    solvecraft_io::step_import_feature(&bytes, path).map_err(|e| EngineError::Other(format!("{path}: {e}")))
}

/// Add an Import feature for a STEP file to the session's design.
fn add_step(s: &mut Session, path: &str, name: Option<&str>) -> Result<Value> {
    let sf = read_step(path)?;
    let fname = name.filter(|n| !n.trim().is_empty()).unwrap_or(&sf.name).to_string();
    let id = s.doc_mut().add_feature(sf.kind, Some(&fname))?;
    if let Some(f) = s.doc_mut().feature_mut(id) {
        f.body_names = sf.body_names;
    }
    s.active_sketch = None;
    s.refresh();
    if let Some(e) = s.model.result(id).and_then(|r| r.error.clone()) {
        return Err(EngineError::Other(e));
    }
    let st = s.model.state();
    let bodies: Vec<&str> = st.bodies.iter().filter(|b| b.feature == id).map(|b| b.name.as_str()).collect();
    let fname = s.doc.feature(id).map(|f| f.name.clone()).unwrap_or_default();
    Ok(json!({"path": path, "feature": id, "name": fname, "bodies": bodies, "warnings": sf.warnings}))
}

/// Add a MeshImport feature for a 3MF or STL file.
fn add_mesh(s: &mut Session, path: &str, name: Option<&str>) -> Result<Value> {
    let bytes = solvecraft_io::vfs::read(path, solvecraft_io::MAX_3MF_BYTES).map_err(|e| EngineError::Other(format!("{path}: {e}")))?;
    let mf = solvecraft_io::mesh_import_feature(&bytes, path).map_err(|e| EngineError::Other(format!("{path}: {e}")))?;
    let fname = name.filter(|n| !n.trim().is_empty()).unwrap_or(&mf.name).to_string();
    let id = s.doc_mut().add_feature(mf.kind, Some(&fname))?;
    if let Some(f) = s.doc_mut().feature_mut(id) {
        f.body_names = mf.body_names;
    }
    s.active_sketch = None;
    s.refresh();
    if let Some(e) = s.model.result(id).and_then(|r| r.error.clone()) {
        return Err(EngineError::Other(e));
    }
    let st = s.model.state();
    let bodies: Vec<&str> = st.bodies.iter().filter(|b| b.feature == id).map(|b| b.name.as_str()).collect();
    let fname = s.doc.feature(id).map(|f| f.name.clone()).unwrap_or_default();
    Ok(json!({"path": path, "feature": id, "name": fname, "bodies": bodies, "warnings": mf.warnings}))
}

fn insert_mesh(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "file.insert_mesh";
    let path = path_arg(p, cmd)?;
    if !solvecraft_io::is_mesh_path(path) {
        return Err(bad(cmd, "only 3MF and STL files (.3mf, .stl) can be inserted as meshes"));
    }
    add_mesh(s, path, str_(p, "name"))
}

fn insert_step(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "file.insert_step";
    let path = path_arg(p, cmd)?;
    if !solvecraft_io::is_step_path(path) {
        return Err(bad(cmd, "only STEP files (.step, .stp) can be inserted"));
    }
    add_step(s, path, str_(p, "name"))
}

fn open(s: &mut Session, p: &Value) -> Result<Value> {
    let path = path_arg(p, "doc.open")?;
    if solvecraft_io::f3d::is_f3d_path(path) {
        let bytes = solvecraft_io::vfs::read(path, solvecraft_io::f3d::MAX_F3D_BYTES).map_err(|e| EngineError::Other(format!("{path}: {e}")))?;
        let stem = std::path::Path::new(path).file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| "Imported".into());
        let doc = solvecraft_io::f3d::import_f3d(&bytes, &stem)?;
        let mut fresh = Session::new(doc);
        let errors: Vec<_> = fresh.model.results.iter().filter_map(|r| r.error.as_ref().map(|e| format!("feature {}: {e}", r.id))).collect();
        if !errors.is_empty() {
            return Err(bad("doc.open", format!("F3D history could not regenerate: {}", errors.join("; "))));
        }
        fresh.mark_unsaved();
        let result = json!({"path":path,"features":fresh.doc.features.len(),"bodies":fresh.model.state().bodies.len(),"errors":0,"editable_history_imported":true});
        *s = fresh;
        return Ok(result);
    }
    if solvecraft_io::is_step_path(path) || solvecraft_io::is_mesh_path(path) {
        // A new, unsaved design holding the file's bodies.
        let stem = std::path::Path::new(path).file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| "Untitled".into());
        let mut fresh = Session::new(Document::new(&stem));
        let r = if solvecraft_io::is_step_path(path) { add_step(&mut fresh, path, None)? } else { add_mesh(&mut fresh, path, None)? };
        fresh.undo.clear();
        *s = fresh;
        let errors = s.model.results.iter().filter(|r| r.error.is_some()).count();
        return Ok(json!({"path": path, "features": s.doc.features.len(), "errors": errors, "bodies": r["bodies"], "warnings": r["warnings"]}));
    }
    let bytes = solvecraft_io::vfs::read(path, solvecraft_io::MAX_DESIGN_BYTES).map_err(|e| EngineError::Other(format!("{path}: {e}")))?;
    let doc = solvecraft_io::read_design(&bytes)?;
    *s = Session::new(doc);
    s.path = Some(path.to_string());
    let errors = s.model.results.iter().filter(|r| r.error.is_some()).count();
    Ok(json!({"path": path, "features": s.doc.features.len(), "errors": errors}))
}

fn inspect_f3d(_: &mut Session, p: &Value) -> Result<Value> {
    let path = path_arg(p, "file.inspect_f3d")?;
    let bytes = solvecraft_io::vfs::read(path, solvecraft_io::f3d::MAX_F3D_BYTES).map_err(|e| EngineError::Other(format!("{path}: {e}")))?;
    let designs = solvecraft_io::f3d::read_f3d(&bytes)?;
    let summaries: Vec<_> = designs
        .iter()
        .map(|d| {
            let mut groups = std::collections::BTreeMap::<&str, usize>::new();
            for class in &d.classes {
                *groups.entry(&class.group).or_default() += class.objects.len();
            }
            let parameters = match d.parameters() {
                Ok(parameters) => json!({"native_units":{"length":"cm","angle":"rad"},"records": parameters.iter().map(|p| json!({"id":p.id,"binding":p.binding,"owner":p.owner,"name":p.name,"expression":p.expression,"label":p.label,"display_unit":p.display_unit,"native_value":p.value,"comment":p.comment,"driven":p.driven,"favorite":p.favorite,"user":p.binding.is_none()})).collect::<Vec<_>>()}),
                Err(e) => json!({"unsupported":e.to_string()}),
            };
            let metadata = |m: &solvecraft_io::f3d::EntityMetadata| json!({"specialized":m.specialized,"point_auxiliary":m.point_auxiliary,"point_prefix":m.point_prefix,"definition":m.definition,"relationships":m.relationships,"legacy":m.legacy});
            let points = match d.sketch_points() {
                Ok(points) => json!({"coordinate_frame":"sketch_local","length_unit":"cm","records":points.iter().map(|p| json!({"id":p.id,"sketch":p.sketch,"position":p.position,"fixed":p.fixed,"origin":p.origin,"linked":p.linked,"metadata":metadata(&p.metadata)})).collect::<Vec<_>>()}),
                Err(e) => json!({"unsupported":e.to_string()}),
            };
            let circles = match d.sketch_circles() {
                Ok(circles) => json!({"coordinate_frame":"sketch_local","length_unit":"cm","records":circles.iter().map(|c| json!({"id":c.id,"sketch":c.sketch,"center_point":c.center_point,"center":c.center,"normal":c.normal,"x_axis":c.x_axis,"radius":c.radius,"construction":c.construction,"reference":c.reference,"metadata":metadata(&c.metadata),"profile_id":c.profile_id})).collect::<Vec<_>>()}),
                Err(e) => json!({"unsupported":e.to_string()}),
            };
            let operation_owners = match d.operation_owners() {
                Ok(owners) => json!({"records":owners.into_iter().map(|(tag,feature)|json!({"tag":tag,"feature":feature})).collect::<Vec<_>>()}),
                Err(e) => json!({"unsupported":e.to_string()}),
            };
            let timeline = match d.timeline() {
                Ok(entries) => json!({"records":entries.into_iter().map(|entry| {
                    let decoded=match entry.feature {
                        Ok(f) => json!({"name":f.name,"suppressed":f.suppressed,"dependencies":f.dependencies,
                            "sketch":match f.kind { solvecraft_io::f3d::HistoryKind::Sketch {geometry,placement} =>
                                json!({"geometry":geometry,"matrix":placement.matrix,"owner":placement.owner,"entities":placement.entities,"projections":placement.projections,"legacy_entity_metadata":placement.legacy_entity_metadata}), _=>Value::Null}}),
                        Err(e) => json!({"unsupported":e.to_string()}),
                    };
                    json!({"id":entry.id,"class":entry.class,"version":entry.version,"decoded":decoded})
                }).collect::<Vec<_>>()}),
                Err(e) => json!({"unsupported":e.to_string()}),
            };
            let offset_planes = match d.offset_planes() {
                Ok(planes) => json!({"length_unit":"mm","records":planes.iter().map(|p|json!({"feature":p.feature,"geometry":p.geometry,"base":p.base,"distance_binding":p.distance,"extra_offset_binding":p.extra,"plane":p.plane})).collect::<Vec<_>>()}),
                Err(e) => json!({"unsupported":e.to_string()}),
            };
            let history = match d.history() {
                Ok(features) => json!({"records":features.iter().map(|f| {
                    let kind = match &f.kind {
                        solvecraft_io::f3d::HistoryKind::Sketch { geometry, placement } => json!({"type":"sketch","geometry":geometry,"transform":placement.matrix,"transform_length_unit":"cm","placement_owner":placement.owner,"entities":placement.entities}),
                        solvecraft_io::f3d::HistoryKind::Extrude { operation, profile_containers } => json!({"type":"extrude","operation":operation,"profile_containers":profile_containers}),
                        solvecraft_io::f3d::HistoryKind::Loft { operation, section_containers, target_containers } => json!({"type":"loft","operation":operation,"section_containers":section_containers,"target_containers":target_containers}),
                        solvecraft_io::f3d::HistoryKind::OffsetPlane { geometry, definition, extra_offset } => json!({"type":"offset_plane","geometry":geometry,"definition":definition,"extra_offset_binding":extra_offset}),
                    };
                    json!({"id":f.id,"name":f.name,"suppressed":f.suppressed,"dependencies":f.dependencies,"kind":kind})
                }).collect::<Vec<_>>()}),
                Err(e) => json!({"unsupported":e.to_string()}),
            };
            let lines = match d.sketch_lines() {
                Ok(lines) => json!({"coordinate_frame":"sketch_local","length_unit":"cm","records":lines.iter().map(|l|json!({"id":l.id,"sketch":l.sketch,"start_point":l.start_point,"end_point":l.end_point,"start":l.start,"end":l.end,"fixed":l.fixed,"construction":l.construction,"centerline":l.centerline,"reference":l.reference,"metadata":metadata(&l.metadata),"profile_id":l.profile_id})).collect::<Vec<_>>()}),
                Err(e) => json!({"unsupported":e.to_string()}),
            };
            let arcs = match d.sketch_arcs() {
                Ok(arcs) => json!({"coordinate_frame":"sketch_local","length_unit":"cm","angle_unit":"rad","records":arcs.iter().map(|a| {
                    let c=&a.circle;
                    json!({"id":c.id,"sketch":c.sketch,"center_point":c.center_point,"start_point":a.start_point,"end_point":a.end_point,"center":c.center,"normal":c.normal,"x_axis":c.x_axis,"radius":c.radius,"start_angle":a.start_angle,"end_angle":a.end_angle,"fixed":c.fixed,"construction":c.construction,"reference":c.reference,"metadata":metadata(&c.metadata),"profile_id":c.profile_id})
                }).collect::<Vec<_>>()}),
                Err(e) => json!({"unsupported":e.to_string()}),
            };
            let profiles = match d.circle_profiles() {
                Ok(profiles) => json!({"records":profiles.iter().map(|p|json!({"id":p.id,"feature":p.feature,"geometry":p.geometry,"circle":p.circle})).collect::<Vec<_>>()}),
                Err(e) => json!({"unsupported":e.to_string()}),
            };
            let constraint_graphs = match d.sketch_constraint_graphs() {
                Ok(graphs) => json!({"records":graphs.iter().map(|g| json!({"geometry":g.geometry,"feature":g.feature,"relations":g.relations,"dimensions":g.dimensions,"auxiliary":g.auxiliary,"support":g.support})).collect::<Vec<_>>()}),
                Err(e) => json!({"unsupported":e.to_string()}),
            };
            let constraints = match d.sketch_constraints() {
                Ok(sketches) => json!({"records":sketches.iter().map(|s|json!({"geometry":s.geometry,"feature":s.feature,"constraints":s.constraints.iter().map(|c|match c {
                    solvecraft_io::f3d::SketchConstraint::OffsetCurves(o) => json!({"type":"offset_curves","id":o.id,"dimension":o.dimension,"binding":o.binding,"distance":o.distance,"sources":o.sources,"outputs":o.outputs}),
                    solvecraft_io::f3d::SketchConstraint::Symmetry {id,axis,entities} => json!({"type":"symmetry","id":id,"axis":axis,"entities":entities}),
                    solvecraft_io::f3d::SketchConstraint::Offset {id,line,entity,binding} => json!({"type":"offset_distance","id":id,"line":line,"entity":entity,"binding":binding}),
                    solvecraft_io::f3d::SketchConstraint::Angle {id,lines,quadrants,binding,cached_radians} => json!({"type":"angle","id":id,"lines":lines,"quadrants":quadrants,"binding":binding,"radians":cached_radians}),
                    solvecraft_io::f3d::SketchConstraint::Coincident {id,points} => json!({"type":"coincident","id":id,"points":points}),
                    solvecraft_io::f3d::SketchConstraint::Diameter {id,circle,binding} => json!({"type":"diameter","id":id,"circle":circle,"binding":binding}),
                    solvecraft_io::f3d::SketchConstraint::Radius {id,circle,binding} => json!({"type":"radius","id":id,"circle":circle,"binding":binding}),
                    solvecraft_io::f3d::SketchConstraint::AlignedPoints {id,points,vertical} => json!({"type":if *vertical{"vertical_points"}else{"horizontal_points"},"id":id,"points":points}),
                    solvecraft_io::f3d::SketchConstraint::PointCurve {id,point,curve,midpoint} => json!({"type":if *midpoint{"midpoint"}else{"point_on_curve"},"id":id,"point":point,"curve":curve}),
                    solvecraft_io::f3d::SketchConstraint::LineMidpoints {id,lines} => json!({"type":"line_midpoints","id":id,"lines":lines}),
                    solvecraft_io::f3d::SketchConstraint::Curves {id,curves,relation} => json!({"type":format!("{relation:?}").to_lowercase(),"id":id,"curves":curves}),
                    solvecraft_io::f3d::SketchConstraint::Horizontal{id,line}=>json!({"type":"horizontal","id":id,"line":line}),
                    solvecraft_io::f3d::SketchConstraint::Vertical{id,line}=>json!({"type":"vertical","id":id,"line":line}),
                    solvecraft_io::f3d::SketchConstraint::Linear{id,points,orientation,binding}=>json!({"type":match orientation {
                        solvecraft_io::f3d::LinearOrientation::Horizontal => "distance_x",
                        solvecraft_io::f3d::LinearOrientation::Vertical => "distance_y",
                        solvecraft_io::f3d::LinearOrientation::Aligned => "distance",
                    },"id":id,"points":points,"binding":binding}),
                }).collect::<Vec<_>>()})).collect::<Vec<_>>()}),
                Err(e) => json!({"unsupported":e.to_string()}),
            };
            let sketch_profiles=match d.sketch_profiles() {
                Ok(profiles)=>json!({"records":profiles.iter().map(|p|json!({"id":p.id,"feature":p.feature,"geometry":p.geometry,"regions":p.regions.iter().map(|loops|loops.iter().map(|l|json!({"curves":l.curves,"reversed":l.reversed,"pieces":l.pieces,"outer":l.outer})).collect::<Vec<_>>()).collect::<Vec<_>>()})).collect::<Vec<_>>()}),
                Err(e)=>json!({"unsupported":e.to_string()}),
            };
            json!({"path": d.path, "build": d.build, "layout": d.layout, "classes": d.classes.len(),
            "objects": d.records.len(), "secondary_records": d.records.iter().filter(|r| r.secondary_offset.is_some()).count(),
            "roots": d.roots, "object_groups": groups, "metadata_bytes": d.metadata.len(), "bulk_bytes": d.bulk.len(), "parameters":parameters,"sketch_points":points,"sketch_circles":circles,"sketch_arcs":arcs,"sketch_lines":lines,"history":history,"timeline":timeline,"offset_planes":offset_planes,"operation_owners":operation_owners,"circle_profiles":profiles,"sketch_profiles":sketch_profiles,"sketch_constraints":constraints,"constraint_graphs":constraint_graphs})
        })
        .collect();
    Ok(json!({"path": path, "editable_history_imported": false, "designs": summaries,
        "warnings": ["Inspection does not regenerate or import a document. Use doc.open for supported editable history; general history and native B-reps remain unsupported."]}))
}

fn write(s: &mut Session, path: &str) -> Result<Value> {
    let bytes = solvecraft_io::write_design(&s.doc);
    // Atomic, keeping the version being replaced as `<file>.bak`.
    solvecraft_io::write_atomic(std::path::Path::new(path), &bytes, true).map_err(|e| EngineError::Other(format!("{path}: {e}")))?;
    s.path = Some(path.to_string());
    s.mark_saved();
    Ok(json!({"path": path, "bytes": bytes.len()}))
}

fn save(s: &mut Session, p: &Value) -> Result<Value> {
    let path = match str_(p, "path") {
        Some(x) if !x.trim().is_empty() => x.to_string(),
        _ => s.path.clone().ok_or_else(|| bad("file.save", "the design has no file yet: give a `path`"))?,
    };
    write(s, &path)
}

fn save_as(s: &mut Session, p: &Value) -> Result<Value> {
    let path = path_arg(p, "file.save_as")?.to_string();
    write(s, &path)
}

fn export_to(s: &Session, path: &str, format: Format, bodies: &[String]) -> Result<Value> {
    let name = std::path::Path::new(path).file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| s.doc.name.clone());
    // A design with components exports as a STEP assembly; otherwise bodies as placed.
    let bytes = if format == Format::Step && bodies.is_empty() && !s.doc.occurrences.is_empty() {
        solvecraft_io::step_assembly(&s.doc, &s.doc.painted(&s.model.state()), &name)?
    } else {
        // Bodies carry the colour they are shown in (appearance, material or imported).
        solvecraft_io::export(&s.doc.painted(&s.world_state()), bodies, format, &name)?
    };
    solvecraft_io::vfs::write(path, &bytes).map_err(|e| EngineError::Other(format!("{path}: {e}")))?;
    Ok(json!({"path": path, "bytes": bytes.len(), "format": format!("{format:?}")}))
}

fn export(s: &mut Session, p: &Value) -> Result<Value> {
    let path = path_arg(p, "file.export")?;
    let format = Format::from_name(str_(p, "format").unwrap_or(path))?;
    export_to(s, path, format, &string_list(p, "bodies"))
}

fn save_stl(s: &mut Session, p: &Value) -> Result<Value> {
    let path = path_arg(p, "file.save_mesh")?;
    let f = if bool_(p, "ascii").unwrap_or(false) { Format::StlAscii } else { Format::StlBinary };
    export_to(s, path, f, &string_list(p, "bodies"))
}

fn recovery_dir(p: &Value, cmd: &str) -> Result<std::path::PathBuf> {
    match str_(p, "dir").filter(|d| !d.trim().is_empty() && d.len() < 4096) {
        Some(d) => Ok(d.into()),
        None => crate::recovery::default_dir().ok_or_else(|| bad(cmd, "no recovery folder on this system (give `dir`)")),
    }
}

fn recovery_list(_s: &mut Session, p: &Value) -> Result<Value> {
    let dir = recovery_dir(p, "doc.recovery_list")?;
    Ok(json!({"dir": dir, "designs": crate::recovery::orphans(&dir)}))
}

fn recover(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "doc.recover";
    let dir = recovery_dir(p, cmd)?;
    let id = str_(p, "id").ok_or_else(|| bad(cmd, "`id` is required (see doc.recovery_list)"))?;
    let (doc, entry) = crate::recovery::load(&dir, id)?;
    *s = Session::new(doc);
    s.path = entry.path.clone();
    s.mark_unsaved();
    if bool_(p, "discard").unwrap_or(false) {
        crate::recovery::remove(&dir, id);
    }
    let errors = s.model.results.iter().filter(|r| r.error.is_some()).count();
    Ok(json!({"id": id, "path": entry.path, "name": entry.name, "features": s.doc.features.len(), "errors": errors}))
}

fn recovery_discard(_s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "doc.recovery_discard";
    let dir = recovery_dir(p, cmd)?;
    let ids: Vec<String> = if bool_(p, "all").unwrap_or(false) {
        crate::recovery::orphans(&dir).into_iter().map(|e| e.id).collect()
    } else {
        vec![str_(p, "id").ok_or_else(|| bad(cmd, "`id` or `all` is required"))?.to_string()]
    };
    for id in &ids {
        crate::recovery::remove(&dir, id);
    }
    Ok(json!({"discarded": ids}))
}
