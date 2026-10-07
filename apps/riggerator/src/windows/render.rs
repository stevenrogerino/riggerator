//! Render View (progressive background rendering) and Render Settings.

use crate::app::App;
use egui::RichText;
use rg_core::{NodeKind, Scene};
use rg_render::{Progress, RenderCamera, RenderOptions};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

pub struct RenderJob {
    pub progress: Arc<Progress>,
    pub started: Instant,
    pub finished: Option<f32>,
    handle: Option<std::thread::JoinHandle<()>>,
    pub width: u32,
    pub height: u32,
}

impl RenderJob {
    pub fn start(scene: Scene, cam: RenderCamera, opts: RenderOptions) -> Self {
        let progress = Progress::new(opts.width, opts.height);
        let p2 = progress.clone();
        let (w, h) = (opts.width, opts.height);
        let handle = std::thread::spawn(move || {
            // quick low-sample pass first for fast feedback, then the full render
            if opts.samples > 4 {
                let mut quick = opts.clone();
                quick.samples = 1;
                quick.max_bounces = 1;
                let img = rg_render::render(&scene, &cam, &quick, None);
                *p2.image.lock().unwrap() = img;
            }
            p2.rows_done.store(0, Ordering::Relaxed);
            rg_render::render(&scene, &cam, &opts, Some(&p2));
        });
        Self { progress, started: Instant::now(), finished: None, handle: Some(handle), width: w, height: h }
    }

    pub fn is_done(&mut self) -> bool {
        if let Some(h) = &self.handle {
            if h.is_finished() {
                let _ = self.handle.take().unwrap().join();
                self.finished = Some(self.started.elapsed().as_secs_f32());
            }
        }
        self.handle.is_none()
    }

    pub fn cancel(&self) {
        self.progress.cancel.store(true, Ordering::Relaxed);
    }
}

pub fn render_view(app: &mut App, ctx: &egui::Context) {
    // update texture from the running job
    let mut needs_repaint = false;
    if let Some(job) = &mut app.render_job {
        let done = job.is_done();
        let img = job.progress.image.lock().unwrap().clone();
        let rgba = img.to_rgba8();
        let ci = egui::ColorImage::from_rgba_unmultiplied([img.width as usize, img.height as usize], &rgba);
        match &mut app.render_tex {
            Some(t) if t.size() == [img.width as usize, img.height as usize] => {
                if !done || job.finished.is_some() {
                    t.set(ci, egui::TextureOptions::LINEAR);
                }
            }
            _ => app.render_tex = Some(ctx.load_texture("render", ci, egui::TextureOptions::LINEAR)),
        }
        let rows = job.progress.rows_done.load(Ordering::Relaxed);
        if done {
            app.render_status = format!("Rendered {}x{} in {:.2}s", job.width, job.height, job.finished.unwrap_or(0.0));
        } else {
            app.render_status = format!("Rendering… {:.0}%", rows as f32 / job.height.max(1) as f32 * 100.0);
            needs_repaint = true;
        }
    }
    if needs_repaint {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
    let mut open = app.win.render_view;
    if !open {
        return;
    }
    egui::Window::new("Render View").open(&mut open).default_size([700.0, 480.0]).show(ctx, |ui| {
        ui.horizontal(|ui| {
            if ui.button("Render").clicked() {
                app.start_render();
            }
            let running = app.render_job.as_mut().is_some_and(|j| !j.is_done());
            if ui.add_enabled(running, egui::Button::new("⏹ Stop")).clicked() {
                if let Some(j) = &app.render_job {
                    j.cancel();
                }
            }
            let cams: Vec<_> = app.scene.walk().into_iter().filter(|id| matches!(app.scene.nodes[id].kind, NodeKind::Camera(_))).collect();
            let label = app.render_camera.filter(|c| app.scene.nodes.contains_key(c)).map(|c| app.scene.name_of(c)).unwrap_or_else(|| "active viewport".into());
            egui::ComboBox::from_label("camera").selected_text(label).show_ui(ui, |ui| {
                if ui.selectable_label(app.render_camera.is_none(), "active viewport").clicked() {
                    app.render_camera = None;
                }
                for c in cams {
                    if ui.selectable_label(app.render_camera == Some(c), app.scene.name_of(c)).clicked() {
                        app.render_camera = Some(c);
                    }
                }
            });
            if ui.add_enabled(app.render_job.is_some(), egui::Button::new("Save Image…")).clicked() {
                app.file_dialog = Some(super::FileDialog::save("Save Image", &["png"], super::FileAction::SaveRender));
            }
            if ui.button("Settings").clicked() {
                app.win.render_settings = true;
            }
        });
        ui.label(RichText::new(&app.render_status).weak());
        ui.separator();
        if let Some(t) = &app.render_tex {
            let avail = ui.available_size();
            let [w, h] = t.size();
            let s = (avail.x / w as f32).min(avail.y / h as f32).min(2.0);
            ui.centered_and_justified(|ui| {
                ui.add(egui::Image::new(t).fit_to_exact_size(egui::vec2(w as f32 * s, h as f32 * s)));
            });
        } else {
            ui.label(RichText::new("Press Render to render the current frame.").weak());
        }
    });
    app.win.render_view = open;
}

pub fn render_settings(app: &mut App, ctx: &egui::Context) {
    let mut open = app.win.render_settings;
    if !open {
        return;
    }
    egui::Window::new("Render Settings").open(&mut open).show(ctx, |ui| {
        let r = &mut app.scene.render;
        let mut changed = false;
        ui.label(RichText::new("Riggerator Software Renderer (path tracer)").strong());
        egui::Grid::new("rs").num_columns(2).show(ui, |ui| {
            ui.label("Presets");
            ui.horizontal(|ui| {
                for (l, w, h) in [("HD 720", 1280, 720), ("HD 1080", 1920, 1080), ("640x480", 640, 480), ("Square 1k", 1024, 1024)] {
                    if ui.small_button(l).clicked() {
                        r.width = w;
                        r.height = h;
                        changed = true;
                    }
                }
            });
            ui.end_row();
            ui.label("Width");
            changed |= ui.add(egui::DragValue::new(&mut r.width).range(16..=8192)).changed();
            ui.end_row();
            ui.label("Height");
            changed |= ui.add(egui::DragValue::new(&mut r.height).range(16..=8192)).changed();
            ui.end_row();
            ui.label("Samples / pixel");
            changed |= ui.add(egui::Slider::new(&mut r.samples, 1..=1024).logarithmic(true)).changed();
            ui.end_row();
            ui.label("Max bounces");
            changed |= ui.add(egui::Slider::new(&mut r.max_bounces, 0..=12)).changed();
            ui.end_row();
            ui.label("Sky dome");
            changed |= ui.checkbox(&mut r.sky, "").changed();
            ui.end_row();
            ui.label("Sky intensity");
            changed |= ui.add(egui::Slider::new(&mut r.sky_intensity, 0.0..=4.0)).changed();
            ui.end_row();
            ui.label("Background");
            let mut c = [r.background.x, r.background.y, r.background.z];
            if ui.color_edit_button_rgb(&mut c).changed() {
                r.background = glam::Vec3::from(c);
                changed = true;
            }
            ui.end_row();
            ui.label("Default light");
            changed |= ui.checkbox(&mut r.default_light, "when the scene has no lights").changed();
            ui.end_row();
        });
        if changed {
            app.scene.touch();
        }
        ui.separator();
        if ui.button("Render").clicked() {
            app.start_render();
        }
    });
    app.win.render_settings = open;
}
