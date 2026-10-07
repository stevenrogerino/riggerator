//! Floating editor windows.

mod editors;
mod graph;
mod hypershade;
mod render;

pub use graph::GraphState;
pub use render::RenderJob;

use crate::app::{App, LogKind};
use egui::RichText;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileAction {
    OpenScene,
    SaveScene,
    Import,
    ExportAll,
    ExportSelection,
    SourceScript,
    TexturePath,
    SaveRender,
}

pub struct FileDialog {
    pub title: String,
    pub dir: PathBuf,
    pub name: String,
    pub exts: Vec<String>,
    pub save: bool,
    pub action: FileAction,
    pub error: Option<String>,
    pub show_all: bool,
}

impl FileDialog {
    fn new(title: &str, exts: &[&str], action: FileAction, save: bool) -> Self {
        let dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self { title: title.into(), dir, name: String::new(), exts: exts.iter().map(|s| s.to_string()).collect(), save, action, error: None, show_all: false }
    }
    pub fn open(title: &str, exts: &[&str], action: FileAction) -> Self {
        Self::new(title, exts, action, false)
    }
    pub fn save(title: &str, exts: &[&str], action: FileAction) -> Self {
        let mut d = Self::new(title, exts, action, true);
        d.name = format!("untitled.{}", exts.first().unwrap_or(&"rgs"));
        d
    }
}

fn matches_ext(p: &Path, exts: &[String]) -> bool {
    p.extension().and_then(|e| e.to_str()).is_some_and(|e| exts.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

pub fn file_dialog(app: &mut App, ctx: &egui::Context) {
    let Some(mut d) = app.file_dialog.take() else { return };
    let mut open = true;
    let mut chosen: Option<PathBuf> = None;
    egui::Window::new(d.title.clone()).open(&mut open).collapsible(false).default_size([560.0, 420.0]).show(ctx, |ui| {
        ui.horizontal(|ui| {
            if ui.button("⬆").on_hover_text("Parent folder").clicked() {
                if let Some(p) = d.dir.parent() {
                    d.dir = p.to_path_buf();
                }
            }
            if ui.button("🏠").on_hover_text("Home").clicked() {
                if let Some(h) = std::env::var_os("HOME") {
                    d.dir = PathBuf::from(h);
                }
            }
            let mut s = d.dir.display().to_string();
            if ui.add(egui::TextEdit::singleline(&mut s).desired_width(f32::INFINITY)).lost_focus() {
                let p = PathBuf::from(&s);
                if p.is_dir() {
                    d.dir = p;
                }
            }
        });
        ui.separator();
        let mut entries: Vec<(bool, PathBuf)> = std::fs::read_dir(&d.dir)
            .map(|rd| rd.filter_map(|e| e.ok()).map(|e| (e.path().is_dir(), e.path())).collect())
            .unwrap_or_default();
        entries.retain(|(is_dir, p)| {
            let hidden = p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with('.'));
            !hidden && (*is_dir || d.show_all || matches_ext(p, &d.exts))
        });
        entries.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        egui::ScrollArea::vertical().max_height(280.0).auto_shrink([false, false]).show(ui, |ui| {
            for (is_dir, p) in entries {
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let label = if is_dir { format!("📁 {name}") } else { format!("   {name}") };
                let r = ui.selectable_label(d.name == name && !is_dir, label);
                if r.clicked() {
                    if is_dir {
                        d.dir = p.clone();
                    } else {
                        d.name = name.clone();
                    }
                }
                if r.double_clicked() && !is_dir {
                    chosen = Some(p.clone());
                }
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("File name:");
            let te = ui.add(egui::TextEdit::singleline(&mut d.name).desired_width(300.0));
            if te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !d.name.is_empty() {
                chosen = Some(d.dir.join(&d.name));
            }
            ui.checkbox(&mut d.show_all, "all files");
        });
        ui.label(RichText::new(format!("Types: {}", d.exts.iter().map(|e| format!("*.{e}")).collect::<Vec<_>>().join(", "))).weak());
        if let Some(e) = &d.error {
            ui.colored_label(egui::Color32::from_rgb(255, 120, 110), e);
        }
        ui.horizontal(|ui| {
            if ui.button(if d.save { "Save" } else { "Open" }).clicked() && !d.name.is_empty() {
                chosen = Some(d.dir.join(&d.name));
            }
            if ui.button("Cancel").clicked() {
                d.error = Some("__cancel".into());
            }
        });
    });
    if d.error.as_deref() == Some("__cancel") || !open {
        return;
    }
    let Some(path) = chosen else {
        app.file_dialog = Some(d);
        return;
    };
    let path = if d.save && path.extension().is_none() { path.with_extension(&d.exts[0]) } else { path };
    if !d.save && !path.exists() {
        d.error = Some(format!("{} does not exist", path.display()));
        app.file_dialog = Some(d);
        return;
    }
    let p = path.display().to_string().replace('\\', "/");
    match d.action {
        FileAction::OpenScene => app.open_path(path),
        FileAction::SaveScene => app.save_to(path),
        FileAction::Import => {
            let _ = app.mel(&format!("file -i \"{p}\";"));
        }
        FileAction::ExportAll => {
            let _ = app.mel(&format!("file -ea \"{p}\";"));
        }
        FileAction::ExportSelection => {
            let _ = app.mel(&format!("file -es \"{p}\";"));
        }
        FileAction::SourceScript => {
            let _ = app.mel(&format!("source \"{p}\";"));
        }
        FileAction::TexturePath => {
            if let Some(m) = app.hypershade_sel.clone() {
                app.checkpoint("texture");
                if let Some(mat) = app.scene.materials.get_mut(&m) {
                    mat.texture = rg_core::Texture::File { path: p };
                }
                app.scene.touch();
            }
        }
        FileAction::SaveRender => {
            if let Some(job) = &app.render_job {
                let img = job.progress.image.lock().unwrap().clone();
                match img.save_png(&path) {
                    Ok(()) => app.flash(format!("Saved {p}")),
                    Err(e) => app.log(LogKind::Error, format!("// Error: {e}")),
                }
            }
        }
    }
}

#[derive(Default)]
pub struct SdkState {
    pub driver: Option<rg_core::NodeId>,
    pub driver_attr: Option<String>,
    pub driven: Vec<rg_core::NodeId>,
    pub driven_attrs: Vec<String>,
}

pub fn show_windows(app: &mut App, ctx: &egui::Context) {
    editors::script_editor(app, ctx);
    graph::graph_editor(app, ctx);
    graph::dope_sheet(app, ctx);
    hypershade::hypershade(app, ctx);
    render::render_view(app, ctx);
    render::render_settings(app, ctx);
    editors::set_driven_key(app, ctx);
    editors::preferences(app, ctx);
    editors::hotkeys(app, ctx);
    editors::about(app, ctx);
    editors::component_editor(app, ctx);
    editors::node_editor(app, ctx);
    editors::uv_editor(app, ctx);
    file_dialog(app, ctx);
    editors::confirm(app, ctx);
}
