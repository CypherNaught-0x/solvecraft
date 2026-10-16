//! Setup context and a compact operation browser. Selection is transient UI state.
use super::*;

fn clear_job(app: &mut SolveApp) {
    app.manufacturing.job = None;
    app.manufacturing.playing = false;
    app.manufacturing.playback = 0;
    app.manufacturing.view.loaded_job = None;
    app.manufacturing.finding = None;
}

pub(super) fn select_setup(app: &mut SolveApp, id: u64) {
    if app.manufacturing.setup != id {
        app.manufacturing.setup = id;
        app.manufacturing.operation = None;
        let program = app.session.doc.manufacturing.programs.iter().find(|p| p.setup == id).map_or(0, |p| p.id);
        app.manufacturing.program = program;
        clear_job(app);
    }
}

pub(super) fn select_program(app: &mut SolveApp, id: u64) {
    if let Some(p) = app.session.doc.manufacturing.programs.iter().find(|p| p.id == id) {
        let setup = p.setup;
        if app.manufacturing.program != id || app.manufacturing.setup != setup {
            clear_job(app);
            app.manufacturing.program = id;
            app.manufacturing.setup = setup;
            app.manufacturing.operation = None;
        }
    }
}

/// Recover from command edits, deletion and undo without retaining another setup's job.
pub(super) fn reconcile(app: &mut SolveApp) {
    let m = &app.session.doc.manufacturing;
    if !m.setups.iter().any(|s| s.id == app.manufacturing.setup) {
        let setup =
            m.programs.iter().find(|p| p.id == app.manufacturing.program).map(|p| p.setup).or_else(|| m.setups.first().map(|s| s.id)).unwrap_or(0);
        // First layout can follow an explicit command-generated job. Only clear an
        // existing context when it actually disappears.
        if app.manufacturing.setup == 0 {
            app.manufacturing.setup = setup;
        } else {
            select_setup(app, setup);
        }
    }
    let m = &app.session.doc.manufacturing;
    if !m.programs.iter().any(|p| p.id == app.manufacturing.program && p.setup == app.manufacturing.setup) {
        let program = m.programs.iter().find(|p| p.setup == app.manufacturing.setup).map_or(0, |p| p.id);
        if app.manufacturing.program != 0 {
            clear_job(app);
        }
        app.manufacturing.program = program;
    }
    if !app.session.doc.manufacturing.operations.iter().any(|o| Some(o.id) == app.manufacturing.operation && o.setup == app.manufacturing.setup) {
        app.manufacturing.operation = None;
    }
}

fn edit(app: &mut SolveApp, editor: Editor) {
    app.manufacturing.error.clear();
    app.manufacturing.operation_tab = OperationTab::default();
    app.manufacturing.editor = Some(editor);
}

pub(super) fn show(app: &mut SolveApp, ui: &mut egui::Ui, action: &mut Option<&'static str>) {
    let m = app.session.doc.manufacturing.clone();
    ui.horizontal(|ui| {
        ui.menu_button("Create", |ui| {
            for (label, command) in
                [("Setup", "cam.setup.put"), ("Tool", "cam.tool.put"), ("Operation", "cam.operation.put"), ("NC program", "cam.program.put")]
            {
                if ui.button(label).clicked() {
                    *action = Some(command);
                    ui.close();
                }
            }
        });
        ui.weak(format!("{} setups · {} tools", m.setups.len(), m.tools.len()));
    });
    if m.setups.is_empty() {
        ui.label("Create a setup to define stock, model and work coordinates.");
        return;
    }
    let mut setup = app.manufacturing.setup;
    egui::ComboBox::from_id_salt("cam_active_setup")
        .width(ui.available_width() - 8.)
        .selected_text(m.setups.iter().find(|s| s.id == setup).map_or("Choose setup", |s| s.name.as_str()))
        .show_ui(ui, |ui| {
            for s in &m.setups {
                ui.selectable_value(&mut setup, s.id, &s.name);
            }
        });
    select_setup(app, setup);
    if let Some(s) = m.setups.iter().find(|s| s.id == setup) {
        ui.horizontal(|ui| {
            ui.weak(format!("G{} · {} models · {} fixtures", s.work_offset, s.models.len(), s.fixtures.len()));
            if ui.small_button("Edit setup").clicked() {
                edit(app, Editor::Setup(s.clone()));
            }
        });
    }
    ui.horizontal(|ui| {
        ui.strong("Operations");
        if ui.small_button("Add operation").clicked() {
            *action = Some("cam.operation.put");
        }
    });
    egui::ScrollArea::vertical().id_salt("cam_operation_tree").max_height(150.).show(ui, |ui| {
        let program = m.programs.iter().find(|p| p.id == app.manufacturing.program);
        let mut operations: Vec<_> = m.operations.iter().filter(|o| o.setup == setup).collect();
        operations.sort_by_key(|o| program.and_then(|p| p.operations.iter().position(|id| *id == o.id)).unwrap_or(usize::MAX));
        for o in operations {
            ui.push_id(o.id, |ui| {
                let order = program.and_then(|p| p.operations.iter().position(|id| *id == o.id));
                ui.horizontal(|ui| {
                    ui.weak(order.map_or("—".into(), |i| format!("{:02}", i + 1)));
                    if ui.selectable_label(app.manufacturing.operation == Some(o.id), &o.name).on_hover_text("Edit machining parameters").clicked() {
                        app.manufacturing.operation = Some(o.id);
                        edit(app, Editor::Operation(o.clone()));
                    }
                });
                let tool = m.tools.iter().find(|t| t.id == o.tool);
                ui.indent("tool", |ui| {
                    ui.weak(tool.map_or("Missing tool".into(), |t| format!("T{} · Ø {:.3} mm", t.number, t.diameter)));
                    if order.is_none() {
                        ui.weak("Not in active NC program");
                    }
                });
            });
        }
        if !m.operations.iter().any(|o| o.setup == setup) {
            ui.weak("Add a machining operation to this setup.");
        }
    });
    ui.collapsing("Document tools", |ui| {
        for tool in &m.tools {
            if ui.button(format!("T{} · {}", tool.number, tool.name)).clicked() {
                edit(app, Editor::Tool(tool.clone()));
            }
        }
        if ui.button("New tool").clicked() {
            *action = Some("cam.tool.put");
        }
    });
    ui.separator();
    ui.strong("Active NC program");
    let mut program = app.manufacturing.program;
    egui::ComboBox::from_id_salt("cam_active_program")
        .width(ui.available_width() - 8.)
        .selected_text(m.programs.iter().find(|p| p.id == program).map_or("No program in this setup", |p| p.name.as_str()))
        .show_ui(ui, |ui| {
            for p in m.programs.iter().filter(|p| p.setup == setup) {
                ui.selectable_value(&mut program, p.id, &p.name);
            }
        });
    select_program(app, program);
    ui.horizontal(|ui| {
        if let Some(p) = m.programs.iter().find(|p| p.id == program)
            && ui.small_button("Edit program / order").clicked()
        {
            edit(app, Editor::Program(p.clone()));
        }
        if ui.small_button("New program").clicked() {
            *action = Some("cam.program.put");
        }
    });
}
