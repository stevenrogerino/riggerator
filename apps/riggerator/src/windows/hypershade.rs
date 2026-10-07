//! Hypershade: material browser with rendered swatches and a property editor.

use crate::app::App;
use egui::{Color32, RichText};
use glam::Vec3;
use rg_core::{mesh::primitives, LightShape, LightType, Material, NodeKind, Scene, ShaderType, Texture};

fn swatch_key(m: &Material) -> String {
    format!("{:?}", m)
}

/// Render a small material preview sphere with the path tracer.
fn render_swatch(m: &Material) -> egui::ColorImage {
    let mut s = Scene::new();
    let id = s.add_mesh("ball", primitives::sphere(1.0, 24, 16));
    let mut mm = m.clone();
    mm.name = "swatch".into();
    s.materials.insert("swatch".into(), mm);
    s.assign_material(id, "swatch").unwrap();
    let l = s.add_node("key", NodeKind::Light(LightShape::new(LightType::Directional)), None);
    s.set_attr(l, "rx", -40.0).unwrap();
    s.set_attr(l, "ry", -35.0).unwrap();
    s.render.sky = true;
    s.render.sky_intensity = 0.5;
    let cam = rg_render::RenderCamera::look_at(Vec3::new(0.0, 0.0, 3.6), Vec3::ZERO, 38.0);
    let opts = rg_render::RenderOptions { width: 64, height: 64, samples: 6, max_bounces: 2, threads: 2 };
    let img = rg_render::render(&s, &cam, &opts, None);
    let rgba = img.to_rgba8();
    // checkerboard behind transparent areas
    let mut px = Vec::with_capacity(64 * 64);
    for (i, c) in rgba.chunks(4).enumerate() {
        let (x, y) = (i % 64, i / 64);
        let bg = if ((x / 8) + (y / 8)) % 2 == 0 { 70 } else { 50 };
        let a = c[3] as f32 / 255.0;
        let mix = |v: u8| (v as f32 * a + bg as f32 * (1.0 - a)) as u8;
        px.push(Color32::from_rgb(mix(c[0]), mix(c[1]), mix(c[2])));
    }
    egui::ColorImage { size: [64, 64], source_size: egui::vec2(64.0, 64.0), pixels: px }
}

fn swatch(app: &mut App, ctx: &egui::Context, name: &str) -> Option<egui::TextureHandle> {
    let m = app.scene.materials.get(name)?.clone();
    let key = swatch_key(&m);
    if let Some((k, tex)) = app.mat_previews.get(name) {
        if *k == key {
            return Some(tex.clone());
        }
    }
    let img = render_swatch(&m);
    let tex = ctx.load_texture(format!("swatch_{name}"), img, egui::TextureOptions::LINEAR);
    app.mat_previews.insert(name.to_string(), (key, tex.clone()));
    Some(tex)
}

pub fn hypershade(app: &mut App, ctx: &egui::Context) {
    let mut open = app.win.hypershade;
    if !open {
        return;
    }
    egui::Window::new("Hypershade").open(&mut open).default_size([700.0, 480.0]).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label("Create:");
            for s in ShaderType::ALL {
                if ui.button(s.name()).clicked() {
                    if let Ok(v) = app.mel(&format!("shadingNode -asShader {};", s.name())) {
                        app.hypershade_sel = Some(v.as_string());
                    }
                }
            }
        });
        ui.separator();
        ui.horizontal_top(|ui| {
            // browser
            ui.vertical(|ui| {
                ui.set_width(260.0);
                egui::ScrollArea::vertical().id_salt("hs_list").max_height(400.0).show(ui, |ui| {
                    let names: Vec<String> = app.scene.materials.keys().cloned().collect();
                    egui::Grid::new("hs_grid").spacing([6.0, 6.0]).show(ui, |ui| {
                        for (i, n) in names.iter().enumerate() {
                            ui.vertical(|ui| {
                                let selected = app.hypershade_sel.as_deref() == Some(n.as_str());
                                if let Some(tex) = swatch(app, ui.ctx(), n) {
                                    let img = egui::Image::new(&tex).fit_to_exact_size(egui::vec2(64.0, 64.0));
                                    let r = ui.add(egui::Button::image(img).selected(selected));
                                    if r.clicked() {
                                        app.hypershade_sel = Some(n.clone());
                                    }
                                    r.context_menu(|ui| {
                                        if ui.button("Assign to selection").clicked() {
                                            let _ = app.mel(&format!("hyperShade -assign {n};"));
                                            ui.close();
                                        }
                                        if ui.button("Select objects with material").clicked() {
                                            let ids: Vec<_> = app.scene.nodes.iter().filter(|(_, x)| x.mesh().is_some_and(|m| m.material.as_deref().unwrap_or("lambert1") == n)).map(|(id, _)| *id).collect();
                                            let names = app.names(&ids);
                                            let _ = app.mel(&format!("select -r {names};"));
                                            ui.close();
                                        }
                                        if n != "lambert1" && ui.button("Delete").clicked() {
                                            app.checkpoint("delete material");
                                            app.scene.materials.remove(n);
                                            for node in app.scene.nodes.values_mut() {
                                                if let Some(ms) = node.mesh_mut() {
                                                    if ms.material.as_deref() == Some(n.as_str()) {
                                                        ms.material = None;
                                                    }
                                                }
                                            }
                                            app.scene.touch();
                                            ui.close();
                                        }
                                    });
                                }
                                ui.label(RichText::new(n).size(10.0));
                            });
                            if i % 3 == 2 {
                                ui.end_row();
                            }
                        }
                    });
                });
            });
            ui.separator();
            // property editor
            ui.vertical(|ui| {
                let Some(name) = app.hypershade_sel.clone().filter(|n| app.scene.materials.contains_key(n)) else {
                    ui.label(RichText::new("Select a material swatch.").weak());
                    return;
                };
                let orig = app.scene.materials[&name].clone();
                let mut m = orig.clone();
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&name).strong());
                    ui.label(RichText::new(m.shader.name()).weak());
                    if ui.button("Assign to Selection").clicked() {
                        let _ = app.mel(&format!("hyperShade -assign {name};"));
                    }
                });
                egui::ComboBox::from_label("Type").selected_text(m.shader.name()).show_ui(ui, |ui| {
                    for s in ShaderType::ALL {
                        ui.selectable_value(&mut m.shader, s, s.name());
                    }
                });
                let row = |ui: &mut egui::Ui, label: &str, v: &mut f32, r: std::ops::RangeInclusive<f32>| {
                    ui.horizontal(|ui| {
                        ui.add_sized([100.0, 18.0], egui::Label::new(label));
                        ui.add(egui::Slider::new(v, r));
                    });
                };
                let color = |ui: &mut egui::Ui, label: &str, c: &mut Vec3| {
                    ui.horizontal(|ui| {
                        ui.add_sized([100.0, 18.0], egui::Label::new(label));
                        let mut a = [c.x, c.y, c.z];
                        if ui.color_edit_button_rgb(&mut a).changed() {
                            *c = Vec3::from(a);
                        }
                    });
                };
                egui::CollapsingHeader::new("Common").default_open(true).show(ui, |ui| {
                    color(ui, "Color", &mut m.color);
                    row(ui, "Diffuse", &mut m.diffuse, 0.0..=1.0);
                    row(ui, "Opacity", &mut m.opacity, 0.0..=1.0);
                });
                if m.shader != ShaderType::Lambert && m.shader != ShaderType::SurfaceShader {
                    egui::CollapsingHeader::new("Specular").default_open(true).show(ui, |ui| {
                        color(ui, "Specular Color", &mut m.specular_color);
                        row(ui, "Specular", &mut m.specular, 0.0..=1.0);
                        row(ui, "Roughness", &mut m.roughness, 0.0..=1.0);
                        if m.shader == ShaderType::StandardSurface {
                            row(ui, "Metalness", &mut m.metalness, 0.0..=1.0);
                            row(ui, "Transmission", &mut m.transmission, 0.0..=1.0);
                            row(ui, "IOR", &mut m.ior, 1.0..=3.0);
                        }
                    });
                }
                egui::CollapsingHeader::new("Emission").default_open(false).show(ui, |ui| {
                    color(ui, "Emission Color", &mut m.emission_color);
                    row(ui, "Emission", &mut m.emission, 0.0..=10.0);
                });
                egui::CollapsingHeader::new("Texture").default_open(true).show(ui, |ui| {
                    let kind = match &m.texture {
                        Texture::None => 0,
                        Texture::Checker { .. } => 1,
                        Texture::Ramp { .. } => 2,
                        Texture::Noise { .. } => 3,
                        Texture::File { .. } => 4,
                    };
                    let mut k = kind;
                    ui.horizontal(|ui| {
                        for (i, l) in ["none", "checker", "ramp", "noise", "file"].iter().enumerate() {
                            ui.radio_value(&mut k, i, *l);
                        }
                    });
                    if k != kind {
                        m.texture = match k {
                            1 => Texture::Checker { color2: Vec3::splat(0.1), repeats: 8.0 },
                            2 => Texture::Ramp { color2: Vec3::new(1.0, 1.0, 1.0) },
                            3 => Texture::Noise { color2: Vec3::splat(0.1), scale: 3.0 },
                            4 => Texture::File { path: String::new() },
                            _ => Texture::None,
                        };
                    }
                    match &mut m.texture {
                        Texture::Checker { color2, repeats } => {
                            color(ui, "Color 2", color2);
                            row(ui, "Repeats", repeats, 1.0..=64.0);
                        }
                        Texture::Ramp { color2 } => color(ui, "Top Color", color2),
                        Texture::Noise { color2, scale } => {
                            color(ui, "Color 2", color2);
                            row(ui, "Scale", scale, 0.1..=20.0);
                        }
                        Texture::File { path } => {
                            ui.horizontal(|ui| {
                                ui.add(egui::TextEdit::singleline(path).desired_width(220.0));
                                if ui.button("…").clicked() {
                                    app.file_dialog = Some(super::FileDialog::open("Texture", &["png", "jpg", "jpeg"], super::FileAction::TexturePath));
                                }
                            });
                        }
                        Texture::None => {}
                    }
                });
                if let Some(tex) = swatch(app, ui.ctx(), &name) {
                    ui.add(egui::Image::new(&tex).fit_to_exact_size(egui::vec2(128.0, 128.0)));
                }
                if m != orig {
                    let edit_id = ui.id().with(("hs_edit", &name));
                    let editing = ui.ctx().input(|i| i.pointer.any_down());
                    let started: bool = ui.data_mut(|d| d.get_temp(edit_id).unwrap_or(false));
                    if !started {
                        app.checkpoint("edit material");
                    }
                    ui.data_mut(|d| d.insert_temp(edit_id, editing));
                    app.scene.materials.insert(name.clone(), m);
                    app.scene.touch();
                } else {
                    let edit_id = ui.id().with(("hs_edit", &name));
                    if !ui.ctx().input(|i| i.pointer.any_down()) {
                        ui.data_mut(|d| d.insert_temp(edit_id, false));
                    }
                }
            });
        });
    });
    app.win.hypershade = open;
}
