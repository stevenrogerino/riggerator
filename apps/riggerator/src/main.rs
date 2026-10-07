//! Riggerator — an open-source, Maya-style 3D application.

mod app;
mod camera;
mod gl;
mod theme;
mod ui_bottom;
mod ui_panels;
mod ui_top;
mod viewport;
mod windows;

use app::{App, Layout, PendingAction, RightTab, Tool};
use egui::Key;
use rg_core::ComponentMode;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

struct Runner {
    app: App,
    initialized: bool,
}

#[derive(Clone, Default)]
struct Config {
    open: Option<PathBuf>,
    script: Option<String>,
    screenshot: Option<PathBuf>,
    quad: bool,
    window: Option<String>,
}

fn build_app(cfg: &Config) -> App {
    let mut app = App::new();
    if cfg.quad {
        app.layout = Layout::Quad;
    }
    match cfg.window.as_deref() {
        Some("graph") => app.win.graph_editor = true,
        Some("hypershade") => app.win.hypershade = true,
        Some("script") => app.win.script_editor = true,
        Some("render") => app.win.render_view = true,
        Some("dope") => app.win.dope_sheet = true,
        Some("attr") => app.right_tab = RightTab::AttributeEditor,
        _ => {}
    }
    if let Some(p) = &cfg.screenshot {
        app.screenshot = Some((p.clone(), 30));
        app.quit_after_screenshot = true;
    }
    if let Some(p) = &cfg.open {
        app.open_path(p.clone());
    }
    if let Some(s) = &cfg.script {
        let _ = app.mel(s);
        app.saved_rev = app.scene.revision;
    }
    app
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut cfg = Config::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--screenshot" => {
                cfg.screenshot = args.get(i + 1).map(PathBuf::from);
                i += 1;
            }
            "--script" | "-s" => {
                cfg.script = args.get(i + 1).and_then(|p| std::fs::read_to_string(p).ok());
                i += 1;
            }
            "--command" | "-c" => {
                cfg.script = args.get(i + 1).cloned();
                i += 1;
            }
            "--layout" => {
                cfg.quad = args.get(i + 1).map(|s| s.as_str()) == Some("quad");
                i += 1;
            }
            "--window" => {
                cfg.window = args.get(i + 1).cloned();
                i += 1;
            }
            "-h" | "--help" => {
                println!("riggerator [scene.rgs|.ma|.obj] [--script file.mel] [--command \"mel\"] [--layout quad] [--window graph|hypershade|script|render|dope|attr] [--screenshot out.png]");
                return Ok(());
            }
            other => cfg.open = Some(PathBuf::from(other)),
        }
        i += 1;
    }
    let msaa: u16 = std::env::var("RIGGERATOR_MSAA").ok().and_then(|v| v.parse().ok()).unwrap_or(4);
    let res = run(cfg.clone(), msaa);
    if res.is_err() && msaa > 0 {
        eprintln!("riggerator: retrying without multisampling ({:?})", res.as_ref().err());
        return run(cfg, 0);
    }
    res
}

fn run(cfg: Config, msaa: u16) -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1600.0, 960.0]).with_min_inner_size([900.0, 600.0]).with_title("Riggerator"),
        depth_buffer: 24,
        multisampling: msaa,
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "Riggerator",
        options,
        Box::new(move |cc| {
            theme::apply(&cc.egui_ctx);
            let mut app = build_app(&cfg);
            if let Some(gl) = cc.gl.as_ref() {
                match gl::GlRenderer::new(gl) {
                    Ok(r) => app.gl = Some(Arc::new(Mutex::new(r))),
                    Err(e) => app.log(app::LogKind::Error, format!("// Error: OpenGL init failed: {e}")),
                }
            }
            Ok(Box::new(Runner { app, initialized: false }))
        }),
    )
}

impl eframe::App for Runner {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let app = &mut self.app;
        let now = ctx.input(|i| i.time);
        let dt = (now - app.last_frame_time).clamp(0.0, 0.5);
        app.last_frame_time = now;
        if dt > 0.0 {
            app.fps_display = app.fps_display * 0.9 + (1.0 / dt as f32) * 0.1;
        }
        app.frame_counter += 1;
        if !self.initialized {
            self.initialized = true;
            app.frame_all_panes();
            app.panes[0].cam = camera::ViewCamera::new(camera::CamKind::Persp);
            if !app.scene.nodes.is_empty() {
                let save = app.active_pane;
                app.active_pane = 0;
                app.frame_selected(true);
                app.active_pane = save;
            }
        }
        if let Some(gl) = frame.gl() {
            if let Some(r) = &app.gl {
                r.lock().unwrap().begin_frame(gl);
            }
        }
        app.tick_playback(dt);
        hotkeys(app, &ctx);
        app.smooth_used.clear();

        ctx.send_viewport_cmd(egui::ViewportCommand::Title(app.title()));

        egui::Panel::top("menubar").frame(egui::Frame::new().fill(theme::PANEL).inner_margin(egui::Margin::symmetric(4, 2))).show(ui, |ui| {
            ui_top::menu_bar(app, ui);
        });
        egui::Panel::top("status").frame(egui::Frame::new().fill(theme::PANEL).inner_margin(egui::Margin::symmetric(4, 2))).show(ui, |ui| {
            ui_top::status_line(app, ui);
        });
        egui::Panel::top("shelf").frame(egui::Frame::new().fill(egui::Color32::from_rgb(60, 60, 60)).inner_margin(egui::Margin::symmetric(4, 2))).show(ui, |ui| {
            ui_top::shelf(app, ui);
        });
        egui::Panel::bottom("helpline").frame(egui::Frame::new().fill(theme::PANEL).inner_margin(egui::Margin::symmetric(4, 1))).show(ui, |ui| {
            ui_bottom::help_line(app, ui);
        });
        egui::Panel::bottom("cmdline").frame(egui::Frame::new().fill(theme::PANEL).inner_margin(egui::Margin::symmetric(4, 2))).show(ui, |ui| {
            ui_bottom::command_line(app, ui);
        });
        egui::Panel::bottom("range").frame(egui::Frame::new().fill(theme::PANEL).inner_margin(egui::Margin::symmetric(4, 2))).show(ui, |ui| {
            ui_bottom::range_slider(app, ui);
        });
        egui::Panel::bottom("timeslider").frame(egui::Frame::new().fill(theme::PANEL).inner_margin(egui::Margin::symmetric(4, 2))).show(ui, |ui| {
            ui_bottom::time_slider(app, ui);
        });
        egui::Panel::left("toolbox").exact_size(40.0).resizable(false).frame(egui::Frame::new().fill(theme::PANEL).inner_margin(egui::Margin::symmetric(2, 6))).show(ui, |ui| {
            ui_top::toolbox(app, ui);
        });
        if app.show_outliner {
            egui::Panel::left("outliner").default_size(210.0).size_range(140.0..=500.0).frame(egui::Frame::new().fill(theme::PANEL).inner_margin(4)).show(ui, |ui| {
                ui_panels::outliner(app, ui);
            });
        }
        if app.show_right {
            egui::Panel::right("rightpanel").default_size(290.0).size_range(220.0..=600.0).frame(egui::Frame::new().fill(theme::PANEL).inner_margin(4)).show(ui, |ui| {
                ui_panels::right_panel(app, ui);
            });
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(egui::Color32::from_rgb(40, 40, 40))).show(ui, |ui| {
            let full = ui.max_rect();
            let panes = app.visible_panes();
            let rects: Vec<egui::Rect> = match (app.layout, panes.len()) {
                (Layout::Quad, 4) => {
                    let c = full.center();
                    vec![
                        egui::Rect::from_min_max(full.min, c - egui::vec2(1.0, 1.0)),
                        egui::Rect::from_min_max(egui::pos2(c.x + 1.0, full.top()), egui::pos2(full.right(), c.y - 1.0)),
                        egui::Rect::from_min_max(egui::pos2(full.left(), c.y + 1.0), egui::pos2(c.x - 1.0, full.bottom())),
                        egui::Rect::from_min_max(c + egui::vec2(1.0, 1.0), full.max),
                    ]
                }
                (Layout::TwoSide, 2) => {
                    let c = full.center().x;
                    vec![egui::Rect::from_min_max(full.min, egui::pos2(c - 1.0, full.bottom())), egui::Rect::from_min_max(egui::pos2(c + 1.0, full.top()), full.max)]
                }
                _ => vec![full],
            };
            for (r, idx) in rects.into_iter().zip(panes) {
                viewport::pane_ui(app, ui, r, idx);
            }
        });

        windows::show_windows(app, &ctx);

        // smooth-preview cache housekeeping
        let used = std::mem::take(&mut app.smooth_used);
        app.smooth_cache.retain(|k, _| used.contains(k));
        app.smooth_used = used;

        if app.playing || app.manip.is_some() || app.render_job.as_mut().is_some_and(|j| !j.is_done()) {
            ctx.request_repaint();
        }
        // headless screenshot support (for docs/CI)
        if let Some((path, n)) = app.screenshot.clone() {
            if n > 1 {
                app.screenshot = Some((path, n - 1));
                ctx.request_repaint();
            } else if n == 1 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
                app.screenshot = Some((path, 0));
                ctx.request_repaint();
            } else {
                let shot = ctx.input(|i| {
                    i.raw.events.iter().find_map(|e| match e {
                        egui::Event::Screenshot { image, .. } => Some(image.clone()),
                        _ => None,
                    })
                });
                if let Some(img) = shot {
                    let [w, h] = img.size;
                    let mut buf = Vec::with_capacity(w * h * 4);
                    for p in &img.pixels {
                        buf.extend_from_slice(&p.to_array());
                    }
                    let _ = image::save_buffer(&path, &buf, w as u32, h as u32, image::ExtendedColorType::Rgba8);
                    app.screenshot = None;
                    if app.quit_after_screenshot {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                } else {
                    ctx.request_repaint();
                }
            }
        }
    }
}

fn hotkeys(app: &mut App, ctx: &egui::Context) {
    // modifier-held state for snapping
    let (x_down, j_down) = ctx.input(|i| (i.key_down(Key::X), i.key_down(Key::J)));
    let typing = ctx.egui_wants_keyboard_input();
    app.held_x = x_down && !typing;
    app.held_j = j_down && !typing;
    if typing {
        return;
    }
    let events: Vec<(Key, egui::Modifiers)> = ctx.input(|i| {
        i.events
            .iter()
            .filter_map(|e| match e {
                egui::Event::Key { key, pressed: true, repeat: false, modifiers, .. } => Some((*key, *modifiers)),
                _ => None,
            })
            .collect()
    });
    for (key, m) in events {
        let ctrl = m.ctrl || m.command;
        match (key, ctrl, m.shift, m.alt) {
            (Key::Q, false, false, false) => app.tool = Tool::Select,
            (Key::W, false, false, false) => app.tool = Tool::Move,
            (Key::E, false, false, false) => app.tool = Tool::Rotate,
            (Key::R, false, false, false) => app.tool = Tool::Scale,
            (Key::Y, false, false, false) => app.tool = app.last_tool,
            (Key::Z, false, false, false) | (Key::Z, true, false, _) => app.undo(),
            (Key::Z, _, true, _) | (Key::Y, true, _, _) => app.redo(),
            (Key::G, false, false, false) => {
                if let Some(c) = app.last_command.clone() {
                    let _ = app.mel(&c);
                }
            }
            (Key::G, true, false, _) => {
                let _ = app.mel("group;");
            }
            (Key::D, true, false, _) => {
                let _ = app.mel("duplicate;");
            }
            (Key::D, false, false, false) | (Key::Insert, _, _, _) => app.pivot_mode = !app.pivot_mode,
            (Key::P, false, false, false) => {
                let _ = app.mel("parent;");
            }
            (Key::P, false, true, false) => {
                let _ = app.mel("parent -w;");
            }
            (Key::Delete, _, _, _) | (Key::Backspace, _, _, _) => {
                if !app.scene.selection.is_empty() || !app.scene.components.is_empty() {
                    let _ = app.mel("delete;");
                }
            }
            (Key::H, true, false, _) => {
                let _ = app.mel("hide;");
            }
            (Key::H, false, true, false) | (Key::H, true, true, _) => {
                let _ = app.mel("showHidden -all;");
            }
            (Key::S, false, false, false) => {
                if !app.scene.selection.is_empty() {
                    let _ = app.mel("setKeyframe;");
                }
            }
            (Key::W, false, true, false) => {
                let _ = app.mel("setKeyframe -at translate;");
            }
            (Key::E, false, true, false) => {
                let _ = app.mel("setKeyframe -at rotate;");
            }
            (Key::R, false, true, false) => {
                let _ = app.mel("setKeyframe -at scale;");
            }
            (Key::S, true, true, _) => app.file_dialog = Some(windows::FileDialog::save("Save Scene As", &["rgs"], windows::FileAction::SaveScene)),
            (Key::S, true, false, _) => app.save(),
            (Key::N, true, _, _) => app.confirm_new = Some(PendingAction::New),
            (Key::O, true, _, _) => app.confirm_new = Some(PendingAction::Open),
            (Key::A, true, false, _) => {
                if app.show_right && app.right_tab == RightTab::AttributeEditor {
                    app.right_tab = RightTab::ChannelBox;
                } else {
                    app.show_right = true;
                    app.right_tab = RightTab::AttributeEditor;
                }
            }
            (Key::A, false, false, true) => {
                let i = app.active_pane;
                app.panes[i].xray = !app.panes[i].xray;
            }
            (Key::F, false, false, false) => app.frame_selected(false),
            (Key::A, false, false, false) => app.frame_selected(true),
            (Key::F, false, true, false) => {
                let save = app.active_pane;
                for p in app.visible_panes() {
                    app.active_pane = p;
                    app.frame_selected(false);
                }
                app.active_pane = save;
            }
            (Key::Space, false, false, false) => {
                app.layout = if app.layout == Layout::Single { Layout::Quad } else { Layout::Single };
            }
            (Key::Num1, false, false, false) => ui_top::set_smooth_preview(app, 0),
            (Key::Num2, false, false, false) => ui_top::set_smooth_preview(app, 1),
            (Key::Num3, false, false, false) => ui_top::set_smooth_preview(app, 2),
            (Key::Num4, false, false, false) => {
                let i = app.active_pane;
                app.panes[i].shade = viewport::ShadeMode::Wireframe;
            }
            (Key::Num5, false, false, false) => {
                let i = app.active_pane;
                app.panes[i].shade = viewport::ShadeMode::Smooth;
            }
            (Key::Num6, false, false, false) => {
                let i = app.active_pane;
                app.panes[i].shade = viewport::ShadeMode::Textured;
            }
            (Key::Num7, false, false, false) => {
                let i = app.active_pane;
                app.panes[i].use_lights = !app.panes[i].use_lights;
            }
            (Key::F8, _, _, _) => {
                app.checkpoint("component mode");
                let m = if app.scene.components.mode == ComponentMode::Object { ComponentMode::Vertex } else { ComponentMode::Object };
                app.set_component_mode(m);
            }
            (Key::F9, _, _, _) => {
                app.checkpoint("component mode");
                app.set_component_mode(ComponentMode::Vertex);
            }
            (Key::F10, _, _, _) => {
                app.checkpoint("component mode");
                app.set_component_mode(ComponentMode::Edge);
            }
            (Key::F11, _, _, _) => {
                app.checkpoint("component mode");
                app.set_component_mode(ComponentMode::Face);
            }
            (Key::F1, _, _, _) => app.win.hotkeys = !app.win.hotkeys,
            (Key::V, false, false, true) => app.playing = !app.playing,
            (Key::Comma, false, false, true) => app.step_time(-1.0),
            (Key::Period, false, false, true) => app.step_time(1.0),
            (Key::Comma, false, false, false) => app.next_key(false),
            (Key::Period, false, false, false) => app.next_key(true),
            (Key::Plus | Key::Equals, false, _, false) => app.prefs.manip_size = (app.prefs.manip_size * 1.15).min(4.0),
            (Key::Minus, false, false, false) => app.prefs.manip_size = (app.prefs.manip_size / 1.15).max(0.2),
            (Key::ArrowUp, false, false, false) => {
                let parents: Vec<_> = app.scene.selected().iter().filter_map(|id| app.scene.nodes.get(id).and_then(|n| n.parent)).collect();
                if !parents.is_empty() {
                    let names = app.names(&parents);
                    let _ = app.mel(&format!("select -r {names};"));
                }
            }
            (Key::ArrowDown, false, false, false) => {
                let kids: Vec<_> = app.scene.selected().iter().filter_map(|id| app.scene.nodes.get(id).and_then(|n| n.children.first().copied())).collect();
                if !kids.is_empty() {
                    let names = app.names(&kids);
                    let _ = app.mel(&format!("select -r {names};"));
                }
            }
            (Key::Escape, _, _, _) => {
                app.manip = None;
                app.playing = false;
            }
            _ => {}
        }
    }
}
