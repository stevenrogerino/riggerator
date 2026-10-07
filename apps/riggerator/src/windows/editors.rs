use crate::app::{App, LogKind, NavMode, PendingAction};
use egui::{Color32, RichText};
use rg_core::*;

pub fn script_editor(app: &mut App, ctx: &egui::Context) {
    let mut open = app.win.script_editor;
    egui::Window::new("Script Editor").open(&mut open).default_size([640.0, 520.0]).show(ctx, |ui| {
        ui.horizontal(|ui| {
            if ui.button("⏵ Execute All").on_hover_text("Ctrl+Enter").clicked() {
                run_script_buffer(app, false);
            }
            if ui.button("Clear History").clicked() {
                app.log.clear();
            }
            if ui.button("Save to Shelf").clicked() {
                let label = app.script_text.lines().find(|l| !l.trim().is_empty() && !l.trim_start().starts_with("//")).unwrap_or("script").chars().take(8).collect::<String>();
                app.shelf_custom.push((label, app.script_text.clone()));
                app.shelf_tab = 4;
                app.flash("Added to the Custom shelf");
            }
            if ui.button("Load…").clicked() {
                app.file_dialog = Some(super::FileDialog::open("Source Script", &["mel", "txt"], super::FileAction::SourceScript));
            }
            if ui.button("Help").clicked() {
                let _ = app.mel("help;");
            }
        });
        let h = ui.available_height();
        egui::ScrollArea::vertical().id_salt("se_hist").max_height(h * 0.45).stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
            egui::Frame::new().fill(Color32::from_rgb(38, 38, 38)).inner_margin(4).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                for (k, s) in &app.log {
                    let col = match k {
                        LogKind::Command => Color32::from_rgb(170, 210, 255),
                        LogKind::Output => Color32::from_gray(200),
                        LogKind::Result => Color32::from_rgb(160, 230, 160),
                        LogKind::Warning => Color32::from_rgb(240, 210, 120),
                        LogKind::Error => Color32::from_rgb(255, 120, 110),
                    };
                    ui.label(RichText::new(s).monospace().size(12.0).color(col));
                }
            });
        });
        ui.separator();
        ui.label(RichText::new("MEL").weak());
        let id = egui::Id::new("script_edit");
        egui::ScrollArea::vertical().id_salt("se_input").auto_shrink([false, false]).show(ui, |ui| {
            let out = egui::TextEdit::multiline(&mut app.script_text).id(id).font(egui::TextStyle::Monospace).code_editor().desired_width(f32::INFINITY).desired_rows(14).show(ui);
            if out.response.has_focus() && ui.input(|i| (i.modifiers.ctrl || i.modifiers.command) && i.key_pressed(egui::Key::Enter)) {
                let sel = out.cursor_range.map(|c| {
                    let (a, b) = (c.primary.index.0.min(c.secondary.index.0), c.primary.index.0.max(c.secondary.index.0));
                    (a, b)
                });
                let text = match sel {
                    Some((a, b)) if b > a => app.script_text.chars().skip(a).take(b - a).collect::<String>(),
                    _ => app.script_text.clone(),
                };
                let _ = app.mel(&text);
            }
        });
    });
    app.win.script_editor = open;
}

fn run_script_buffer(app: &mut App, _sel_only: bool) {
    let text = app.script_text.clone();
    let _ = app.mel(&text);
}

pub fn set_driven_key(app: &mut App, ctx: &egui::Context) {
    let mut open = app.win.sdk;
    egui::Window::new("Set Driven Key").open(&mut open).default_size([520.0, 360.0]).show(ctx, |ui| {
        ui.columns(2, |cols| {
            let ui = &mut cols[0];
            ui.label(RichText::new("Driver").strong());
            if ui.button("Load Driver").clicked() {
                app.sdk.driver = app.scene.selection.last().copied();
                app.sdk.driver_attr = None;
            }
            if let Some(d) = app.sdk.driver.filter(|d| app.scene.nodes.contains_key(d)) {
                ui.label(app.scene.name_of(d));
                egui::ScrollArea::vertical().id_salt("sdk_drv").max_height(220.0).show(ui, |ui| {
                    let mut attrs = app.scene.keyable_attrs(d);
                    attrs.extend(app.scene.shape_attrs(d));
                    for a in attrs {
                        let on = app.sdk.driver_attr.as_deref() == Some(a.as_str());
                        if ui.selectable_label(on, &a).clicked() {
                            app.sdk.driver_attr = Some(a);
                        }
                    }
                });
            }
            let ui = &mut cols[1];
            ui.label(RichText::new("Driven").strong());
            if ui.button("Load Driven").clicked() {
                app.sdk.driven = app.scene.selected();
                app.sdk.driven_attrs.clear();
            }
            let driven: Vec<NodeId> = app.sdk.driven.iter().copied().filter(|d| app.scene.nodes.contains_key(d)).collect();
            if let Some(&first) = driven.first() {
                ui.label(app.names(&driven));
                egui::ScrollArea::vertical().id_salt("sdk_dvn").max_height(220.0).show(ui, |ui| {
                    let mut attrs = app.scene.keyable_attrs(first);
                    attrs.extend(app.scene.shape_attrs(first));
                    for a in attrs {
                        let on = app.sdk.driven_attrs.contains(&a);
                        if ui.selectable_label(on, &a).clicked() {
                            if on {
                                app.sdk.driven_attrs.retain(|x| *x != a);
                            } else {
                                app.sdk.driven_attrs.push(a);
                            }
                        }
                    }
                });
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            let ready = app.sdk.driver.is_some() && app.sdk.driver_attr.is_some() && !app.sdk.driven.is_empty() && !app.sdk.driven_attrs.is_empty();
            if ui.add_enabled(ready, egui::Button::new("Key")).clicked() {
                let d = app.scene.name_of(app.sdk.driver.unwrap());
                let da = app.sdk.driver_attr.clone().unwrap();
                let targets: Vec<String> = app.sdk.driven.iter().flat_map(|n| app.sdk.driven_attrs.iter().map(move |a| (n, a))).map(|(n, a)| format!("{}.{a}", app.scene.name_of(*n))).collect();
                let _ = app.mel(&format!("setDrivenKeyframe -cd {d}.{da} {};", targets.join(" ")));
            }
            ui.label(RichText::new("Pose the driver, pose the driven, press Key. Repeat for each pose.").weak());
        });
        if !app.scene.driven_keys.is_empty() {
            ui.separator();
            ui.label(RichText::new("Existing driven keys").weak());
            let mut remove = None;
            for (i, dk) in app.scene.driven_keys.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("{}.{} → {}.{}  ({} keys)", app.scene.name_of(dk.driver), dk.driver_attr, app.scene.name_of(dk.driven), dk.driven_attr, dk.curve.len()));
                    if ui.small_button("✖").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                app.checkpoint("delete driven key");
                app.scene.driven_keys.remove(i);
                app.scene.touch();
            }
        }
    });
    app.win.sdk = open;
}

pub fn preferences(app: &mut App, ctx: &egui::Context) {
    let mut open = app.win.prefs;
    egui::Window::new("Preferences").open(&mut open).show(ctx, |ui| {
        ui.label(RichText::new("Viewport navigation").strong());
        ui.radio_value(&mut app.prefs.nav, NavMode::Maya, "Maya: Alt+LMB tumble, Alt+MMB track, Alt+RMB dolly");
        ui.radio_value(&mut app.prefs.nav, NavMode::Simple, "Simple: RMB tumble, MMB track, wheel zoom (no Alt)");
        ui.separator();
        ui.label(RichText::new("Grid").strong());
        ui.add(egui::Slider::new(&mut app.prefs.grid_size, 1.0..=100.0).text("size"));
        ui.add(egui::Slider::new(&mut app.prefs.grid_spacing, 0.1..=10.0).text("spacing"));
        ui.separator();
        ui.label(RichText::new("Undo").strong());
        ui.add(egui::Slider::new(&mut app.prefs.max_undo, 10..=1000).text("queue size"));
        ui.separator();
        ui.label(RichText::new("Playback").strong());
        ui.checkbox(&mut app.prefs.play_every_frame, "Play every frame");
    });
    app.win.prefs = open;
}

pub const HOTKEYS: &[(&str, &str)] = &[
    ("Q / W / E / R", "Select / Move / Rotate / Scale tool"),
    ("Alt + LMB / MMB / RMB", "Tumble / Track / Dolly (Maya navigation)"),
    ("Mouse wheel", "Zoom"),
    ("F / A", "Frame selection / frame all"),
    ("Space (tap)", "Toggle single / four view"),
    ("1 / 2 / 3", "Smooth mesh preview off / cage / smooth"),
    ("4 / 5 / 6 / 7", "Wireframe / shaded / textured / use all lights"),
    ("F8 / F9 / F10 / F11", "Object / vertex / edge / face mode"),
    ("Right click", "Marking menu (component modes, tools)"),
    ("Z or Ctrl+Z", "Undo"),
    ("Shift+Z or Ctrl+Y", "Redo"),
    ("G", "Repeat last command"),
    ("Ctrl+D", "Duplicate"),
    ("Ctrl+G", "Group"),
    ("P / Shift+P", "Parent / unparent"),
    ("Delete / Backspace", "Delete selection or components"),
    ("Ctrl+H / Shift+H", "Hide / show all"),
    ("Up / Down arrow", "Pick-walk to parent / child"),
    ("S", "Set key"),
    ("Shift+W / Shift+E / Shift+R", "Key translate / rotate / scale"),
    (", / .", "Previous / next key"),
    ("Alt+, / Alt+.", "Step one frame"),
    ("Alt+V", "Play / stop"),
    ("Hold X", "Snap to grid while moving"),
    ("Hold J", "Discrete rotate steps"),
    ("D (toggle)", "Pivot edit mode"),
    ("+ / -", "Manipulator size"),
    ("Insert", "Toggle pivot edit"),
    ("Ctrl+A", "Toggle Attribute Editor"),
    ("Ctrl+N / Ctrl+O / Ctrl+S", "New / open / save"),
    ("Alt+A", "X-ray toggle"),
    ("MMB drag (channel selected)", "Virtual slider for selected channels"),
    ("F1", "This window"),
];

pub fn hotkeys(app: &mut App, ctx: &egui::Context) {
    let mut open = app.win.hotkeys;
    egui::Window::new("Hotkeys").open(&mut open).default_size([460.0, 520.0]).show(ctx, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("hk").striped(true).show(ui, |ui| {
                for (k, d) in HOTKEYS {
                    ui.label(RichText::new(*k).monospace().color(Color32::from_rgb(170, 210, 255)));
                    ui.label(*d);
                    ui.end_row();
                }
            });
        });
    });
    app.win.hotkeys = open;
}

pub fn about(app: &mut App, ctx: &egui::Context) {
    let mut open = app.win.about;
    egui::Window::new("About Riggerator").open(&mut open).collapsible(false).show(ctx, |ui| {
        ui.heading("Riggerator");
        ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
        ui.label("An open-source, Maya-style 3D modeling, rigging, animation and rendering application written in Rust.");
        ui.label("Not affiliated with Autodesk. Maya is a trademark of Autodesk, Inc.");
        ui.label("Apache-2.0 licensed.");
    });
    app.win.about = open;
}

pub fn component_editor(app: &mut App, ctx: &egui::Context) {
    let mut open = app.win.component_editor;
    egui::Window::new("Component Editor").open(&mut open).default_size([520.0, 380.0]).show(ctx, |ui| {
        let Some(node) = app.scene.components.node.or(app.scene.selection.last().copied()) else {
            ui.label("Select a mesh (and vertices) to edit values.");
            return;
        };
        let Some(ms) = app.scene.nodes.get(&node).and_then(|n| n.mesh()).cloned() else {
            ui.label("Not a mesh.");
            return;
        };
        let verts: Vec<u32> = if app.scene.components.node == Some(node) && !app.scene.components.is_empty() {
            app.scene.components.affected_verts(&ms.mesh).into_iter().collect()
        } else {
            (0..ms.mesh.positions.len().min(200) as u32).collect()
        };
        ui.label(format!("{} — {} vertices shown", app.scene.name_of(node), verts.len()));
        let tabs_id = ui.id().with("ce_tab");
        let mut tab: u8 = ui.data_mut(|d| d.get_temp(tabs_id).unwrap_or(0));
        ui.horizontal(|ui| {
            ui.selectable_value(&mut tab, 0, "Positions");
            ui.add_enabled_ui(ms.skin.is_some(), |ui| {
                ui.selectable_value(&mut tab, 1, "Skin Weights");
            });
        });
        ui.data_mut(|d| d.insert_temp(tabs_id, tab));
        egui::ScrollArea::both().show(ui, |ui| {
            if tab == 0 {
                let mut mesh = (*ms.mesh).clone();
                let mut changed = false;
                egui::Grid::new("ce_pos").striped(true).show(ui, |ui| {
                    ui.label("vtx");
                    ui.label("X");
                    ui.label("Y");
                    ui.label("Z");
                    ui.end_row();
                    for &v in &verts {
                        ui.label(v.to_string());
                        let p = &mut mesh.positions[v as usize];
                        changed |= ui.add(egui::DragValue::new(&mut p.x).speed(0.01).max_decimals(3)).changed();
                        changed |= ui.add(egui::DragValue::new(&mut p.y).speed(0.01).max_decimals(3)).changed();
                        changed |= ui.add(egui::DragValue::new(&mut p.z).speed(0.01).max_decimals(3)).changed();
                        ui.end_row();
                    }
                });
                if changed {
                    app.checkpoint("component edit");
                    app.scene.edit_mesh(node, |m| *m = mesh);
                }
            } else if let Some(skin) = &ms.skin {
                let mut skin = (**skin).clone();
                let mut changed = false;
                egui::Grid::new("ce_w").striped(true).show(ui, |ui| {
                    ui.label("vtx");
                    for j in &skin.joints {
                        ui.label(app.scene.name_of(*j));
                    }
                    ui.end_row();
                    for &v in &verts {
                        ui.label(v.to_string());
                        let ws = &mut skin.weights[v as usize];
                        for ji in 0..skin.joints.len() as u32 {
                            let mut w = ws.iter().find(|x| x.0 == ji).map(|x| x.1).unwrap_or(0.0);
                            if ui.add(egui::DragValue::new(&mut w).speed(0.01).range(0.0..=1.0).max_decimals(3)).changed() {
                                ws.retain(|x| x.0 != ji);
                                if w > 0.0 {
                                    ws.push((ji, w));
                                }
                                // renormalize the others
                                let others: f32 = ws.iter().filter(|x| x.0 != ji).map(|x| x.1).sum();
                                let rest = (1.0 - w).max(0.0);
                                if others > 0.0 {
                                    for x in ws.iter_mut().filter(|x| x.0 != ji) {
                                        x.1 *= rest / others;
                                    }
                                }
                                changed = true;
                            }
                        }
                        ui.end_row();
                    }
                });
                if changed {
                    app.checkpoint("skin weights");
                    if let Some(m) = app.scene.node_mut(node).and_then(|n| n.mesh_mut()) {
                        m.skin = Some(std::sync::Arc::new(skin));
                    }
                }
            }
        });
    });
    app.win.component_editor = open;
}

pub fn node_editor(app: &mut App, ctx: &egui::Context) {
    let mut open = app.win.node_editor;
    egui::Window::new("Node Editor — relationships").open(&mut open).default_size([520.0, 380.0]).show(ctx, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            let sel = app.scene.selected();
            let relevant = |id: NodeId| sel.is_empty() || sel.contains(&id);
            ui.label(RichText::new("Constraints").strong());
            for c in &app.scene.constraints {
                if relevant(c.driven) || c.targets.iter().any(|t| relevant(*t)) {
                    ui.label(format!("{}  ⟶  {}   [{}]", app.names(&c.targets), app.scene.name_of(c.driven), c.kind.name()));
                }
            }
            ui.separator();
            ui.label(RichText::new("Driven keys").strong());
            for d in &app.scene.driven_keys {
                if relevant(d.driver) || relevant(d.driven) {
                    ui.label(format!("{}.{}  ⟶  {}.{}", app.scene.name_of(d.driver), d.driver_attr, app.scene.name_of(d.driven), d.driven_attr));
                }
            }
            ui.separator();
            ui.label(RichText::new("Skin clusters / IK").strong());
            for (id, n) in &app.scene.nodes {
                match &n.kind {
                    NodeKind::Mesh(ms) => {
                        if let Some(s) = &ms.skin {
                            if relevant(*id) || s.joints.iter().any(|j| relevant(*j)) {
                                ui.label(format!("{} ⟵ skin ({} joints)", n.name, s.joints.len()));
                            }
                        }
                        for b in &ms.blend_shapes {
                            ui.label(format!("{} ⟵ blendShape {} ({:.2})", n.name, b.name, b.weight));
                        }
                        for d in &ms.deformers {
                            ui.label(format!("{} ⟵ {}", n.name, d.name));
                        }
                    }
                    NodeKind::IkHandle(ik) if relevant(*id) || relevant(ik.start_joint) || relevant(ik.end_joint) => {
                        ui.label(format!("{}: {} → {}", n.name, app.scene.name_of(ik.start_joint), app.scene.name_of(ik.end_joint)));
                    }
                    _ => {}
                }
            }
        });
    });
    app.win.node_editor = open;
}

pub fn confirm(app: &mut App, ctx: &egui::Context) {
    let Some(action) = app.confirm_new.clone() else { return };
    if !app.is_dirty() {
        app.confirm_new = None;
        do_pending(app, ctx, action);
        return;
    }
    egui::Window::new("Save changes?").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
        ui.label("The scene has unsaved changes.");
        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                app.save();
                app.confirm_new = None;
                if !app.is_dirty() {
                    do_pending(app, ctx, action.clone());
                }
            }
            if ui.button("Don't Save").clicked() {
                app.confirm_new = None;
                do_pending(app, ctx, action.clone());
            }
            if ui.button("Cancel").clicked() {
                app.confirm_new = None;
            }
        });
    });
}

fn do_pending(app: &mut App, ctx: &egui::Context, a: PendingAction) {
    match a {
        PendingAction::New => app.new_scene(),
        PendingAction::Open => app.file_dialog = Some(super::FileDialog::open("Open Scene", &["rgs", "ma", "obj"], super::FileAction::OpenScene)),
        PendingAction::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
    }
}

pub fn uv_editor(app: &mut App, ctx: &egui::Context) {
    let mut open = app.win.uv_editor;
    if !open {
        return;
    }
    egui::Window::new("UV Editor").open(&mut open).default_size([460.0, 500.0]).show(ctx, |ui| {
        let node = app.scene.components.node.or(app.scene.selection.last().copied());
        let Some(node) = node.filter(|n| app.scene.nodes.get(n).is_some_and(|x| x.mesh().is_some())) else {
            ui.label("Select a polygon mesh.");
            return;
        };
        ui.horizontal(|ui| {
            ui.label(RichText::new(app.scene.name_of(node)).strong());
            for (l, c) in [("Planar Y", "polyPlanarProjection -md y;"), ("Planar X", "polyPlanarProjection -md x;"), ("Planar Z", "polyPlanarProjection -md z;"), ("Cylindrical", "polyCylindricalProjection;")] {
                if ui.button(l).clicked() {
                    let _ = app.mel(c);
                }
            }
        });
        let m = app.scene.nodes[&node].mesh().unwrap().mesh.clone();
        let size = ui.available_width().min(ui.available_height()).max(100.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
        let p = ui.painter_at(rect);
        // checker background for 0..1 tile
        let n = 8;
        for i in 0..n {
            for j in 0..n {
                let c = if (i + j) % 2 == 0 { Color32::from_gray(70) } else { Color32::from_gray(58) };
                let cell = rect.width() / n as f32;
                p.rect_filled(egui::Rect::from_min_size(rect.min + egui::vec2(i as f32 * cell, j as f32 * cell), egui::vec2(cell, cell)), 0.0, c);
            }
        }
        let to = |u: glam::Vec2| egui::pos2(rect.left() + u.x * rect.width(), rect.bottom() - u.y * rect.height());
        let sel_faces = if app.scene.components.node == Some(node) && app.scene.components.mode == ComponentMode::Face { app.scene.components.faces.clone() } else { Default::default() };
        let mut drawn = 0usize;
        for (fi, f) in m.faces.iter().enumerate() {
            if !f.has_uvs() {
                continue;
            }
            let pts: Vec<egui::Pos2> = f.uvs.iter().filter_map(|&u| m.uvs.get(u as usize)).map(|u| to(*u)).collect();
            if pts.len() != f.len() {
                continue;
            }
            if sel_faces.contains(&(fi as u32)) {
                p.add(egui::Shape::convex_polygon(pts.clone(), Color32::from_rgba_unmultiplied(255, 140, 20, 80), egui::Stroke::NONE));
            }
            let mut closed = pts.clone();
            closed.push(pts[0]);
            p.add(egui::Shape::line(closed, egui::Stroke::new(1.0, Color32::from_rgb(120, 200, 255))));
            drawn += 1;
        }
        p.rect_stroke(rect, 0.0, egui::Stroke::new(1.0, Color32::from_gray(150)), egui::StrokeKind::Inside);
        if drawn == 0 {
            p.text(rect.center(), egui::Align2::CENTER_CENTER, "No UVs — use a projection above", egui::FontId::proportional(14.0), Color32::WHITE);
        }
    });
    app.win.uv_editor = open;
}
