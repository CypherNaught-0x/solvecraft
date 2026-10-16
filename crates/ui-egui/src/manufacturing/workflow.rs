//! UI availability is advisory. The commands still perform all posting checks.
use super::*;

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum Page {
    #[default]
    Toolpath,
    Display,
    Output,
}

#[derive(Default)]
pub(super) struct Availability {
    pub busy: bool,
    pub path: bool,
    pub verified: bool,
    pub post: bool,
    pub stock: bool,
    pub stale: bool,
}
impl Availability {
    pub fn read(app: &SolveApp) -> Self {
        let Some(id) = app.manufacturing.job else { return Self::default() };
        let Ok(job) = solidcraft_engine::manufacturing::job(&app.session, id) else { return Self::default() };
        let stale = solidcraft_engine::manufacturing::current(&app.session, &job).is_err();
        let mut a = Self { stale, ..Self::default() };
        match job.state() {
            Ok(JobState::Preparing | JobState::Running | JobState::Verifying(_)) => a.busy = true,
            Ok(JobState::Generated(_)) => a.path = !stale,
            Ok(JobState::Verified(_, r)) if !stale => {
                a.path = true;
                a.verified = true;
                a.stock = r.stock_complete;
                a.post = r.status == cam::verify::Status::Pass && r.finished_dimensions == cam::verify::Status::Pass && r.stock_complete;
            }
            _ => {}
        }
        a
    }
    pub fn export(&self, format: &str) -> bool {
        match format {
            "gcode" => self.post,
            "stock" => self.stock,
            "validation" => self.verified,
            _ => self.path,
        }
    }
    pub fn hint(&self, format: &str) -> &'static str {
        if self.stale {
            "Inputs changed. Generate and simulate again."
        } else if !self.path {
            "Generate the active NC program first."
        } else if format == "gcode" && self.verified {
            "Resolve failed or inconclusive checks before posting."
        } else {
            "Simulate the current toolpath first."
        }
    }
}

pub(super) fn actions(app: &mut SolveApp, ui: &mut egui::Ui, a: &Availability, action: &mut Option<&'static str>) {
    let program = app.session.doc.manufacturing.programs.iter().find(|p| p.id == app.manufacturing.program);
    let generate = program.is_some_and(|p| !p.operations.is_empty()) && !a.busy && app.manufacturing.pending_export.is_none();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(generate, egui::Button::new("Generate"))
            .on_disabled_hover_text("Choose a program with operations; wait for the current job to finish.")
            .clicked()
        {
            *action = Some("cam.generate");
        }
        if ui.add_enabled(a.path && !a.busy, egui::Button::new("Simulate")).on_disabled_hover_text(a.hint("validation")).clicked() {
            *action = Some("cam.simulate");
            app.manufacturing.page = Page::Toolpath;
        }
        if ui
            .add_enabled(a.post && app.manufacturing.pending_export.is_none(), egui::Button::new("Post G-code"))
            .on_disabled_hover_text(a.hint("gcode"))
            .clicked()
        {
            *action = Some("cam.post");
        }
    });
    let (text, color) = if a.stale {
        ("Changed inputs · regenerate", Tokens::get().warning)
    } else if a.busy {
        ("Job in progress", Tokens::get().accent)
    } else if a.post {
        ("Verified · ready for reference post", Tokens::get().accent)
    } else if a.verified {
        ("Review findings · posting blocked", Tokens::get().warning)
    } else if a.path {
        ("Toolpath ready · simulate next", Tokens::get().accent)
    } else {
        ("Choose a program, then generate", Tokens::get().text_dim)
    };
    ui.colored_label(color, text);
    if a.busy || app.manufacturing.pending_export.is_some() {
        ui.horizontal(|ui| {
            ui.spinner();
            if let Some(job) = app.manufacturing.job.or(app.manufacturing.pending_export)
                && ui.button("Cancel job").clicked()
            {
                let _ = app.run("cam.job.cancel", json!({"job":job}));
            }
        });
    }
}

/// Schematic levels, deliberately not to scale; the numeric Z values are authoritative.
pub(super) fn heights(ui: &mut egui::Ui, s: &cam::Setup, o: &cam::Operation) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 112.), egui::Sense::hover());
    let t = Tokens::get();
    for (i, (label, z, color)) in
        [("Clearance", s.clearance, t.cam_rapid), ("Retract", s.retract, t.cam_tool), ("Top", o.top, t.cam_stock), ("Bottom", o.bottom, t.cam_cut)]
            .into_iter()
            .enumerate()
    {
        let y = rect.top() + 14. + i as f32 * 26.;
        ui.painter().line_segment([egui::pos2(rect.left(), y), egui::pos2(rect.left() + 44., y)], Stroke::new(2., color));
        ui.painter().text(
            egui::pos2(rect.left() + 54., y),
            egui::Align2::LEFT_CENTER,
            format!("{label}   {z:.3} mm"),
            egui::FontId::proportional(13.),
            t.text,
        );
    }
    ui.weak("Height schematic · not to scale");
}
