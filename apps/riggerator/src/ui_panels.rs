//! Outliner, Channel Box + Layer Editor, Attribute Editor and Tool Settings.

use crate::app::{App, LogKind, RightTab, Tool};
use crate::theme;
use egui::{Color32, RichText};
use glam::Vec3;
use rg_core::*;
use rg_script::value::fmt_float;

fn type_glyph(k: &NodeKind) -> (&'static str, Color32) {
    match k {
        NodeKind::Transform => ("⬜", Color32::from_rgb(190, 190, 190)),
        NodeKind::Mesh(_) => ("🔷", Color32::from_rgb(110, 180, 255)),
        NodeKind::Camera(_) => ("🎥", Color32::from_rgb(200, 200, 200)),
        NodeKind::Light(_) => ("💡", Color32::from_rgb(255, 220, 90)),
        NodeKind::Joint(_) => ("⚪", Color32::from_rgb(120, 200, 255)),
        NodeKind::Locator => ("✚", Color32::from_rgb(190, 190, 255)),
        NodeKind::Curve(_) => ("〰", Color32::from_rgb(150, 230, 150)),
        NodeKind::IkHandle(_) => ("⛓", Color32::from_rgb(230, 200, 140)),
    }
}

// --------------------------------------------------------------- outliner

pub fn outliner(app: &mut App, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Outliner").strong());
        ui.add(egui::TextEdit::singleline(&mut app.outliner_filter).hint_text("search…").desired_width(f32::INFINITY));
    });
    ui.separator();
    let roots = app.scene.roots.clone();
    let filter = app.outliner_filter.to_lowercase();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for r in roots {
            outliner_node(app, ui, r, 0, &filter);
        }
        // drop on empty space -> parent to world
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), ui.available_height().max(30.0)), egui::Sense::click());
        if let Some(payload) = resp.dnd_release_payload::<NodeId>() {
            let name = app.scene.name_of(*payload);
            let _ = app.mel(&format!("parent -w {name};"));
        }
        if resp.clicked() {
            let _ = app.mel("select -cl;");
        }
        let _ = rect;
        ui.separator();
        ui.label(RichText::new(format!("Materials ({})", app.scene.materials.len())).weak());
        let mats: Vec<String> = app.scene.materials.keys().cloned().collect();
        for m in mats {
            if ui.selectable_label(app.hypershade_sel.as_deref() == Some(m.as_str()), format!("● {m}")).clicked() {
                app.hypershade_sel = Some(m);
                app.win.hypershade = true;
            }
        }
    });
}

fn matches_filter(app: &App, id: NodeId, filter: &str) -> bool {
    if filter.is_empty() {
        return true;
    }
    if app.scene.name_of(id).to_lowercase().contains(filter) {
        return true;
    }
    app.scene.descendants(id).iter().any(|d| app.scene.name_of(*d).to_lowercase().contains(filter))
}

fn outliner_node(app: &mut App, ui: &mut egui::Ui, id: NodeId, depth: usize, filter: &str) {
    let Some(n) = app.scene.nodes.get(&id) else { return };
    if !matches_filter(app, id, filter) {
        return;
    }
    let children = n.children.clone();
    let (glyph, gcol) = type_glyph(&n.kind);
    let name = n.name.clone();
    let hidden = !app.scene.is_visible(id);
    let selected = app.scene.selection.contains(&id);
    let expanded = app.outliner_expanded.contains(&id) || !filter.is_empty();
    let row = ui.horizontal(|ui| {
        ui.add_space(depth as f32 * 14.0);
        if !children.is_empty() {
            if ui.add(egui::Button::new(if expanded { "⬇" } else { "➡" }).frame(false).small()).clicked() {
                if expanded {
                    app.outliner_expanded.remove(&id);
                } else {
                    app.outliner_expanded.insert(id);
                }
            }
        } else {
            ui.add_space(14.0);
        }
        ui.label(RichText::new(glyph).color(gcol));
        if app.outliner_rename.as_ref().is_some_and(|(r, _)| *r == id) {
            let mut text = app.outliner_rename.as_ref().unwrap().1.clone();
            let te = ui.add(egui::TextEdit::singleline(&mut text).desired_width(120.0));
            te.request_focus();
            app.outliner_rename = Some((id, text.clone()));
            if te.lost_focus() {
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) && !text.trim().is_empty() && text != name {
                    let _ = app.mel(&format!("rename {name} {};", text.trim()));
                }
                app.outliner_rename = None;
            }
            return None;
        }
        let mut txt = RichText::new(&name);
        if hidden {
            txt = txt.color(Color32::from_gray(120)).italics();
        }
        let label = egui::Button::selectable(selected, txt).frame_when_inactive(selected);
        let r = ui.add(label.sense(egui::Sense::click_and_drag()));
        Some(r)
    });
    if let Some(r) = row.inner {
        r.dnd_set_drag_payload(id);
        if let Some(p) = r.dnd_release_payload::<NodeId>() {
            if *p != id {
                let child = app.scene.name_of(*p);
                let _ = app.mel(&format!("parent {child} {name};"));
                app.outliner_expanded.insert(id);
            }
        }
        if r.dnd_hover_payload::<NodeId>().is_some() {
            ui.painter().rect_stroke(r.rect, 2.0, egui::Stroke::new(1.0, theme::ACCENT), egui::StrokeKind::Outside);
        }
        if r.clicked() {
            let m = ui.input(|i| i.modifiers);
            let flag = if m.ctrl && m.shift {
                "-add"
            } else if m.ctrl {
                "-tgl"
            } else if m.shift {
                "-add"
            } else {
                "-r"
            };
            let _ = app.mel(&format!("select {flag} {name};"));
        }
        if r.double_clicked() {
            app.outliner_rename = Some((id, name.clone()));
        }
        r.context_menu(|ui| {
            if !selected {
                let _ = app.mel(&format!("select -r {name};"));
            }
            if ui.button("Rename").clicked() {
                app.outliner_rename = Some((id, name.clone()));
                ui.close();
            }
            for (l, c) in [("Select Hierarchy", format!("select -hi {name};")), ("Duplicate", "duplicate;".into()), ("Group", "group;".into()), ("Unparent", "parent -w;".into()), ("Hide", "hide;".into()), ("Show", format!("showHidden {name};")), ("Delete", "delete;".into())] {
                if ui.button(l).clicked() {
                    let _ = app.mel(&c);
                    ui.close();
                }
            }
            if ui.button("Frame").clicked() {
                app.frame_selected(false);
                ui.close();
            }
        });
    }
    if expanded {
        for c in children {
            outliner_node(app, ui, c, depth + 1, filter);
        }
    }
}

// ------------------------------------------------------------ right panel

pub fn right_panel(app: &mut App, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        for (t, l) in [(RightTab::ChannelBox, "Channel Box"), (RightTab::AttributeEditor, "Attribute Editor"), (RightTab::ToolSettings, "Tool Settings")] {
            ui.selectable_value(&mut app.right_tab, t, l);
        }
    });
    ui.separator();
    match app.right_tab {
        RightTab::ChannelBox => channel_box(app, ui),
        RightTab::AttributeEditor => attribute_editor(app, ui),
        RightTab::ToolSettings => tool_settings(app, ui),
    }
}

fn channel_state(app: &App, ev: &Evaluated, id: NodeId, attr: &str) -> Option<Color32> {
    let n = app.scene.nodes.get(&id)?;
    if n.locked.contains(attr) {
        return Some(theme::LOCKED);
    }
    if let Some(c) = n.anim.get(attr) {
        return Some(if c.key_at(app.scene.time.current).is_some() { theme::KEYED } else { theme::ANIMATED });
    }
    if app.scene.driven_keys.iter().any(|d| d.driven == id && d.driven_attr == attr) {
        return Some(theme::DRIVEN);
    }
    let constrained = app.scene.constraints.iter().any(|c| {
        c.driven == id
            && match c.kind {
                ConstraintKind::Point => attr.starts_with("translate"),
                ConstraintKind::Orient | ConstraintKind::Aim => attr.starts_with("rotate"),
                ConstraintKind::Scale => attr.starts_with("scale"),
                ConstraintKind::Parent => attr.starts_with("translate") || attr.starts_with("rotate"),
                ConstraintKind::PoleVector => false,
            }
    });
    let ik = n.is_joint() && app.scene.nodes.values().any(|h| matches!(&h.kind, NodeKind::IkHandle(ik) if app.scene.joint_chain(ik.start_joint, ik.end_joint).is_some_and(|c| c.contains(&id)))) && attr.starts_with("rotate");
    let _ = ev;
    if constrained || ik {
        return Some(theme::CONSTRAINED);
    }
    None
}

fn nice_attr(a: &str) -> String {
    let mut out = String::new();
    for (i, c) in a.chars().enumerate() {
        if i == 0 {
            out.extend(c.to_uppercase());
        } else if c.is_uppercase() {
            out.push(' ');
            out.push(c);
        } else if c == '.' {
            out.push_str(" › ");
        } else {
            out.push(c);
        }
    }
    out
}

fn channel_row(app: &mut App, ui: &mut egui::Ui, ev: &Evaluated, ids: &[NodeId], lead: NodeId, attr: &str) {
    let val = ev.attr(&app.scene, lead, attr).unwrap_or(0.0);
    let state = channel_state(app, ev, lead, attr);
    let selected = app.channel_sel.iter().any(|a| a == attr);
    ui.horizontal(|ui| {
        let lw = ui.available_width() * 0.55;
        let label = egui::Button::selectable(selected, RichText::new(nice_attr(attr)).size(12.0)).frame_when_inactive(selected);
        let r = ui.add_sized([lw, 18.0], label);
        if r.clicked() {
            let m = ui.input(|i| i.modifiers);
            if m.ctrl || m.shift {
                if selected {
                    app.channel_sel.retain(|a| a != attr);
                } else {
                    app.channel_sel.push(attr.to_string());
                }
            } else {
                app.channel_sel = vec![attr.to_string()];
            }
        }
        r.context_menu(|ui| channel_menu(app, ui, ids, attr));
        let mut v = val;
        let bool_attr = attr == "visibility";
        let resp = if bool_attr {
            let mut b = v >= 0.5;
            let label = if b { "on" } else { "off" };
            let r = ui.checkbox(&mut b, label);
            v = if b { 1.0 } else { 0.0 };
            r
        } else {
            let speed = if attr.starts_with("rotate") { 0.5 } else { 0.02 };
            let mut dv = egui::DragValue::new(&mut v).speed(speed).max_decimals(3);
            if attr.starts_with("scale") {
                dv = dv.min_decimals(3);
            }
            ui.add_sized([ui.available_width(), 18.0], dv)
        };
        if let Some(c) = state {
            ui.painter().rect_filled(resp.rect.shrink(1.0), 1.0, c.gamma_multiply(0.45));
        }
        if resp.drag_started() {
            app.checkpoint("setAttr");
        }
        if resp.changed() && (v - val).abs() > 1e-12 {
            if !resp.dragged() && !resp.drag_stopped() {
                // typed value: one MEL command per object
                let cmds: Vec<String> = ids.iter().map(|id| format!("setAttr {}.{attr} {};", app.scene.name_of(*id), fmt_float(v))).collect();
                let _ = app.mel(&cmds.join(" "));
            } else {
                for id in ids {
                    let _ = app.scene.set_attr(*id, attr, v);
                }
            }
            for id in ids {
                app.auto_key_attrs(*id, &[attr]);
            }
        }
        if resp.drag_stopped() {
            let cmds: Vec<String> = ids.iter().map(|id| format!("setAttr {}.{attr} {};", app.scene.name_of(*id), fmt_float(app.scene.get_attr(*id, attr).unwrap_or(0.0)))).collect();
            app.log(LogKind::Command, cmds.join(" "));
        }
    });
}

fn channel_menu(app: &mut App, ui: &mut egui::Ui, ids: &[NodeId], attr: &str) {
    let attrs = if app.channel_sel.is_empty() || !app.channel_sel.iter().any(|a| a == attr) { vec![attr.to_string()] } else { app.channel_sel.clone() };
    let names = app.names(ids);
    let at = attrs.join(" -at ");
    if ui.button("Key Selected").clicked() {
        let _ = app.mel(&format!("setKeyframe -at {at} {names};"));
        ui.close();
    }
    if ui.button("Key All").clicked() {
        let _ = app.mel(&format!("setKeyframe {names};"));
        ui.close();
    }
    if ui.button("Delete Keys (Break Connections)").clicked() {
        let _ = app.mel(&format!("cutKey -cl -at {at} {names};"));
        ui.close();
    }
    ui.separator();
    if ui.button("Lock Selected").clicked() {
        let cmds: Vec<String> = ids.iter().flat_map(|id| attrs.iter().map(move |a| (id, a))).map(|(id, a)| format!("setAttr -l true {}.{a};", app.scene.name_of(*id))).collect();
        let _ = app.mel(&cmds.join(" "));
        ui.close();
    }
    if ui.button("Unlock Selected").clicked() {
        let cmds: Vec<String> = ids.iter().flat_map(|id| attrs.iter().map(move |a| (id, a))).map(|(id, a)| format!("setAttr -l false {}.{a};", app.scene.name_of(*id))).collect();
        let _ = app.mel(&cmds.join(" "));
        ui.close();
    }
    if ui.button("Copy value to all selected").clicked() {
        if let Some(&lead) = ids.last() {
            let v = app.scene.get_attr(lead, attr).unwrap_or(0.0);
            let cmds: Vec<String> = ids.iter().map(|id| format!("setAttr {}.{attr} {};", app.scene.name_of(*id), fmt_float(v))).collect();
            let _ = app.mel(&cmds.join(" "));
        }
        ui.close();
    }
    ui.separator();
    if ui.button("Set Driven Key…").clicked() {
        app.win.sdk = true;
        ui.close();
    }
}

pub fn channel_box(app: &mut App, ui: &mut egui::Ui) {
    let ev = app.evaluated();
    let sel = app.scene.selected();
    let avail = ui.available_height();
    egui::ScrollArea::vertical().id_salt("cb").max_height(avail * 0.62).auto_shrink([false, false]).show(ui, |ui| {
        let Some(&lead) = sel.last() else {
            ui.label(RichText::new("Nothing selected").weak());
            return;
        };
        let name = app.scene.name_of(lead);
        ui.horizontal(|ui| {
            ui.label(RichText::new(&name).strong().size(13.0));
            if sel.len() > 1 {
                ui.label(RichText::new(format!("(+{})", sel.len() - 1)).weak());
            }
        });
        let ids: Vec<NodeId> = sel.clone();
        for a in app.scene.keyable_attrs(lead) {
            channel_row(app, ui, &ev, &ids, lead, &a);
        }
        let shape = app.scene.shape_attrs(lead);
        if !shape.is_empty() {
            ui.separator();
            ui.label(RichText::new("SHAPES / INPUTS").weak().size(11.0));
            for a in shape {
                channel_row(app, ui, &ev, &[lead], lead, &a);
            }
        }
        // constraints driving this node
        let cons: Vec<(usize, String)> = app.scene.constraints.iter().enumerate().filter(|(_, c)| c.driven == lead).map(|(i, c)| (i, c.name.clone())).collect();
        if !cons.is_empty() {
            ui.separator();
            ui.label(RichText::new("CONSTRAINTS").weak().size(11.0));
            for (i, n) in cons {
                ui.horizontal(|ui| {
                    ui.label(n);
                    let mut w = app.scene.constraints[i].enabled;
                    if ui.add(egui::Slider::new(&mut w, 0.0..=1.0).text("blend")).changed() {
                        app.scene.constraints[i].enabled = w;
                        app.scene.touch();
                    }
                });
            }
        }
    });
    ui.separator();
    layer_editor(app, ui);
}

fn layer_editor(app: &mut App, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Display Layers").strong());
        if ui.small_button("+ new from selection").clicked() {
            let _ = app.mel("createDisplayLayer -n layer1;");
        }
    });
    egui::ScrollArea::vertical().id_salt("layers").auto_shrink([false, false]).show(ui, |ui| {
        let mut remove = None;
        let n = app.scene.layers.len();
        for i in 0..n {
            ui.horizontal(|ui| {
                let l = &mut app.scene.layers[i];
                let mut changed = false;
                changed |= ui.selectable_label(l.visible, "V").on_hover_text("Visible").clicked().then(|| l.visible = !l.visible).is_some();
                if ui.selectable_label(l.reference, "R").on_hover_text("Reference (unselectable)").clicked() {
                    l.reference = !l.reference;
                    changed = true;
                }
                let mut col = l.color.map(|c| [c.x, c.y, c.z]).unwrap_or([0.3, 0.3, 0.8]);
                if ui.color_edit_button_rgb(&mut col).changed() {
                    l.color = Some(Vec3::from(col));
                    changed = true;
                }
                ui.label(format!("{} ({})", l.name, l.members.len()));
                if changed {
                    app.scene.touch();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("✖").on_hover_text("Delete layer").clicked() {
                        remove = Some(i);
                    }
                    if ui.small_button("+sel").on_hover_text("Add selected objects").clicked() {
                        let sel = app.scene.selected();
                        let l = &mut app.scene.layers[i];
                        for s in sel {
                            if !l.members.contains(&s) {
                                l.members.push(s);
                            }
                        }
                        app.scene.touch();
                    }
                    if ui.small_button("sel").on_hover_text("Select members").clicked() {
                        let m = app.scene.layers[i].members.clone();
                        let names = app.names(&m);
                        let _ = app.mel(&format!("select -r {names};"));
                    }
                });
            });
        }
        if let Some(i) = remove {
            app.checkpoint("delete layer");
            app.scene.layers.remove(i);
            app.scene.touch();
        }
    });
}

// ------------------------------------------------------- attribute editor

fn vec3_edit(ui: &mut egui::Ui, label: &str, v: &mut Vec3, speed: f64) -> egui::Response {
    ui.horizontal(|ui| {
        ui.add_sized([90.0, 18.0], egui::Label::new(label));
        let a = ui.add(egui::DragValue::new(&mut v.x).speed(speed).max_decimals(3));
        let b = ui.add(egui::DragValue::new(&mut v.y).speed(speed).max_decimals(3));
        let c = ui.add(egui::DragValue::new(&mut v.z).speed(speed).max_decimals(3));
        a.union(b).union(c)
    })
    .inner
}

fn f32_edit(ui: &mut egui::Ui, label: &str, v: &mut f32, range: std::ops::RangeInclusive<f32>) -> egui::Response {
    ui.horizontal(|ui| {
        ui.add_sized([90.0, 18.0], egui::Label::new(label));
        ui.add(egui::Slider::new(v, range).clamping(egui::SliderClamping::Never))
    })
    .inner
}

/// Track edits to a node clone, committing with one undo step.
pub fn attribute_editor(app: &mut App, ui: &mut egui::Ui) {
    let Some(&lead) = app.scene.selection.last() else {
        ui.label(RichText::new("Select an object to edit its attributes.").weak());
        return;
    };
    let Some(orig) = app.scene.nodes.get(&lead).cloned() else { return };
    let mut n = orig.clone();
    let mut pending_cmds: Vec<String> = Vec::new();
    egui::ScrollArea::vertical().id_salt("ae").auto_shrink([false, false]).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(n.kind.type_name()).weak());
            let mut name = n.name.clone();
            if ui.add(egui::TextEdit::singleline(&mut name).desired_width(160.0)).lost_focus() && name != n.name && !name.trim().is_empty() {
                pending_cmds.push(format!("rename {} {};", n.name, name.trim()));
            }
        });
        egui::CollapsingHeader::new("Transform Attributes").default_open(true).show(ui, |ui| {
            vec3_edit(ui, "Translate", &mut n.transform.translate, 0.02);
            vec3_edit(ui, "Rotate", &mut n.transform.rotate, 0.5);
            vec3_edit(ui, "Scale", &mut n.transform.scale, 0.01);
            vec3_edit(ui, "Pivot", &mut n.transform.pivot, 0.02);
            ui.horizontal(|ui| {
                ui.add_sized([90.0, 18.0], egui::Label::new("Rotate Order"));
                egui::ComboBox::from_id_salt("ro").selected_text(n.transform.rotate_order.name()).show_ui(ui, |ui| {
                    for o in RotateOrder::ALL {
                        ui.selectable_value(&mut n.transform.rotate_order, o, o.name());
                    }
                });
            });
            ui.checkbox(&mut n.visible, "Visibility");
            ui.checkbox(&mut n.template, "Template");
            let mut has_color = n.color.is_some();
            ui.horizontal(|ui| {
                if ui.checkbox(&mut has_color, "Wireframe color override").changed() {
                    n.color = if has_color { Some(Vec3::new(1.0, 0.8, 0.1)) } else { None };
                }
                if let Some(c) = &mut n.color {
                    let mut a = [c.x, c.y, c.z];
                    if ui.color_edit_button_rgb(&mut a).changed() {
                        *c = Vec3::from(a);
                    }
                }
            });
        });
        let ev = app.ev.clone();
        match &mut n.kind {
            NodeKind::Mesh(ms) => {
                egui::CollapsingHeader::new("Mesh").default_open(true).show(ui, |ui| {
                    let m = ms.mesh.clone();
                    ui.label(format!("{} verts · {} edges · {} faces · {} uvs", m.positions.len(), m.edges().len(), m.faces.len(), m.uvs.len()));
                    let mut lvl = ms.smooth_level as i32;
                    ui.horizontal(|ui| {
                        ui.add_sized([90.0, 18.0], egui::Label::new("Smooth Preview"));
                        if ui.add(egui::Slider::new(&mut lvl, 0..=3)).changed() {
                            ms.smooth_level = lvl as u32;
                        }
                    });
                    let mut ang = ms.mesh.normal_angle;
                    if f32_edit(ui, "Normal Angle", &mut ang, 0.0..=180.0).changed() {
                        std::sync::Arc::make_mut(&mut ms.mesh).normal_angle = ang;
                    }
                    let problems = m.validate();
                    if !problems.is_empty() {
                        ui.colored_label(Color32::from_rgb(255, 160, 120), format!("⚠ {} topology issue(s): {}", problems.len(), problems[0]));
                    }
                });
                egui::CollapsingHeader::new("Material").default_open(true).show(ui, |ui| {
                    let cur = ms.material.clone().unwrap_or_else(|| "lambert1".into());
                    egui::ComboBox::from_id_salt("mat").selected_text(&cur).show_ui(ui, |ui| {
                        for name in app.scene.materials.keys() {
                            if ui.selectable_label(cur == *name, name).clicked() {
                                ms.material = if name == "lambert1" { None } else { Some(name.clone()) };
                            }
                        }
                    });
                    if ui.button("Edit in Hypershade").clicked() {
                        app.hypershade_sel = Some(cur);
                        app.win.hypershade = true;
                    }
                });
                if let Some(skin) = ms.skin.clone() {
                    egui::CollapsingHeader::new("Skin Cluster").default_open(true).show(ui, |ui| {
                        ui.label(format!("{} influences", skin.joints.len()));
                        let mut env = skin.envelope;
                        if f32_edit(ui, "Envelope", &mut env, 0.0..=1.0).changed() {
                            std::sync::Arc::make_mut(ms.skin.as_mut().unwrap()).envelope = env;
                        }
                        let names: Vec<String> = ms.skin.as_ref().unwrap().joints.iter().map(|j| app.scene.name_of(*j)).collect();
                        ui.label(RichText::new(names.join(", ")).weak().size(11.0));
                        if ui.button("Unbind").clicked() {
                            ms.skin = None;
                        }
                    });
                }
                if !ms.blend_shapes.is_empty() {
                    egui::CollapsingHeader::new("Blend Shapes").default_open(true).show(ui, |ui| {
                        let mut remove = None;
                        for (i, b) in ms.blend_shapes.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                ui.add_sized([90.0, 18.0], egui::Label::new(&b.name));
                                ui.add(egui::Slider::new(&mut b.weight, 0.0..=1.0));
                                if ui.small_button("✖").clicked() {
                                    remove = Some(i);
                                }
                            });
                        }
                        if let Some(i) = remove {
                            ms.blend_shapes.remove(i);
                        }
                    });
                }
                if !ms.deformers.is_empty() {
                    egui::CollapsingHeader::new("Deformers").default_open(true).show(ui, |ui| {
                        let mut remove = None;
                        for (i, d) in ms.deformers.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&d.name).strong());
                                if ui.small_button("✖").clicked() {
                                    remove = Some(i);
                                }
                            });
                            f32_edit(ui, "Envelope", &mut d.envelope, 0.0..=1.0);
                            for (k, v) in d.params.iter_mut() {
                                let range = match k.as_str() {
                                    "curvature" => -4.0..=4.0,
                                    "startAngle" | "endAngle" => -720.0..=720.0,
                                    "wavelength" => 0.1..=10.0,
                                    _ => -2.0..=2.0,
                                };
                                f32_edit(ui, k, v, range);
                            }
                            ui.horizontal(|ui| {
                                ui.add_sized([90.0, 18.0], egui::Label::new("Bounds Y"));
                                ui.add(egui::DragValue::new(&mut d.low_bound).speed(0.05));
                                ui.add(egui::DragValue::new(&mut d.high_bound).speed(0.05));
                            });
                        }
                        if let Some(i) = remove {
                            ms.deformers.remove(i);
                        }
                    });
                }
            }
            NodeKind::Camera(c) => {
                egui::CollapsingHeader::new("Camera Attributes").default_open(true).show(ui, |ui| {
                    f32_edit(ui, "Focal Length", &mut c.focal_length, 2.5..=300.0);
                    f32_edit(ui, "Near Clip", &mut c.near, 0.001..=10.0);
                    f32_edit(ui, "Far Clip", &mut c.far, 10.0..=100000.0);
                    ui.checkbox(&mut c.orthographic, "Orthographic");
                    f32_edit(ui, "Ortho Width", &mut c.ortho_width, 0.1..=1000.0);
                    ui.label(format!("Angle of view: {:.1}°", c.hfov().to_degrees()));
                    if ui.button("Use as render camera").clicked() {
                        app.render_camera = Some(lead);
                    }
                });
            }
            NodeKind::Light(l) => {
                egui::CollapsingHeader::new(format!("{} Attributes", l.kind.name())).default_open(true).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add_sized([90.0, 18.0], egui::Label::new("Color"));
                        let mut a = [l.color.x, l.color.y, l.color.z];
                        if ui.color_edit_button_rgb(&mut a).changed() {
                            l.color = Vec3::from(a);
                        }
                    });
                    f32_edit(ui, "Intensity", &mut l.intensity, 0.0..=10.0);
                    if l.kind == LightType::Spot {
                        f32_edit(ui, "Cone Angle", &mut l.cone_angle, 1.0..=179.0);
                        f32_edit(ui, "Penumbra", &mut l.penumbra, -10.0..=10.0);
                    }
                    if matches!(l.kind, LightType::Point | LightType::Spot) {
                        let mut d = l.decay as i32;
                        ui.horizontal(|ui| {
                            ui.add_sized([90.0, 18.0], egui::Label::new("Decay Rate"));
                            egui::ComboBox::from_id_salt("decay").selected_text(["No Decay", "Linear", "Quadratic"][d.clamp(0, 2) as usize]).show_ui(ui, |ui| {
                                ui.selectable_value(&mut d, 0, "No Decay");
                                ui.selectable_value(&mut d, 1, "Linear");
                                ui.selectable_value(&mut d, 2, "Quadratic");
                            });
                        });
                        l.decay = d as u32;
                    }
                    ui.checkbox(&mut l.cast_shadows, "Cast Shadows");
                });
            }
            NodeKind::Joint(j) => {
                egui::CollapsingHeader::new("Joint").default_open(true).show(ui, |ui| {
                    vec3_edit(ui, "Joint Orient", &mut j.orient, 0.5);
                    f32_edit(ui, "Radius", &mut j.radius, 0.05..=5.0);
                });
            }
            NodeKind::IkHandle(ik) => {
                egui::CollapsingHeader::new("IK Handle").default_open(true).show(ui, |ui| {
                    ui.label(format!("Chain: {} → {}", app.scene.name_of(ik.start_joint), app.scene.name_of(ik.end_joint)));
                    ui.horizontal(|ui| {
                        ui.label("Solver");
                        ui.radio_value(&mut ik.solver, IkSolver::RotatePlane, "ikRPsolver");
                        ui.radio_value(&mut ik.solver, IkSolver::SingleChain, "ikSCsolver");
                    });
                    f32_edit(ui, "Twist", &mut ik.twist, -180.0..=180.0);
                    f32_edit(ui, "IK Blend", &mut ik.ik_blend, 0.0..=1.0);
                    vec3_edit(ui, "Pole Vector", &mut ik.pole_vector, 0.02);
                    if let Some(p) = ik.pole_target {
                        ui.label(format!("Pole target: {}", app.scene.name_of(p)));
                    }
                });
            }
            NodeKind::Curve(c) => {
                egui::CollapsingHeader::new("NURBS Curve").default_open(true).show(ui, |ui| {
                    ui.label(format!("{} CVs, degree {}, {}", c.cvs.len(), c.degree, if c.periodic { "periodic" } else { "open" }));
                });
            }
            _ => {}
        }
        let _ = ev;
        egui::CollapsingHeader::new("Extra Attributes").default_open(true).show(ui, |ui| {
            let keys: Vec<String> = n.user_attrs.keys().cloned().collect();
            for k in keys {
                ui.horizontal(|ui| {
                    ui.add_sized([90.0, 18.0], egui::Label::new(&k));
                    let v = n.user_attrs.get_mut(&k).unwrap();
                    ui.add(egui::DragValue::new(v).speed(0.05));
                    if ui.small_button("✖").on_hover_text("Delete attribute").clicked() {
                        n.user_attrs.remove(&k);
                    }
                });
            }
            ui.horizontal(|ui| {
                let id = ui.id().with("newattr");
                let mut s: String = ui.data_mut(|d| d.get_temp::<String>(id).unwrap_or_default());
                ui.add(egui::TextEdit::singleline(&mut s).hint_text("new attribute").desired_width(110.0));
                if ui.button("Add").clicked() && !s.trim().is_empty() {
                    pending_cmds.push(format!("addAttr -ln \"{}\" -dv 0 {};", s.trim(), n.name));
                    s.clear();
                }
                ui.data_mut(|d| d.insert_temp(id, s));
            });
        });
        egui::CollapsingHeader::new("Notes").default_open(false).show(ui, |ui| {
            ui.add(egui::TextEdit::multiline(&mut n.notes).desired_rows(3).desired_width(f32::INFINITY));
        });
    });
    if n != orig {
        app.checkpoint("attribute edit");
        let mut changed_attrs: Vec<&str> = Vec::new();
        if n.transform.translate != orig.transform.translate {
            changed_attrs.extend(["translateX", "translateY", "translateZ"]);
        }
        if n.transform.rotate != orig.transform.rotate {
            changed_attrs.extend(["rotateX", "rotateY", "rotateZ"]);
        }
        if n.transform.scale != orig.transform.scale {
            changed_attrs.extend(["scaleX", "scaleY", "scaleZ"]);
        }
        let attrs: Vec<String> = changed_attrs.iter().map(|s| s.to_string()).collect();
        // keep name and hierarchy unchanged here (renames go through MEL)
        n.name = orig.name.clone();
        n.children = orig.children.clone();
        n.parent = orig.parent;
        *app.scene.node_mut(lead).unwrap() = n;
        let refs: Vec<&str> = attrs.iter().map(|s| s.as_str()).collect();
        app.auto_key_attrs(lead, &refs);
    }
    for c in pending_cmds {
        let _ = app.mel(&c);
    }
}

// ---------------------------------------------------------- tool settings

pub fn tool_settings(app: &mut App, ui: &mut egui::Ui) {
    let t = app.tool;
    ui.label(RichText::new(match t {
        Tool::Select => "Select Tool",
        Tool::Move => "Move Tool",
        Tool::Rotate => "Rotate Tool",
        Tool::Scale => "Scale Tool",
    }).strong());
    ui.separator();
    match t {
        Tool::Move => {
            ui.label("Axis orientation");
            ui.radio_value(&mut app.prefs.move_world, true, "World");
            ui.radio_value(&mut app.prefs.move_world, false, "Object");
            ui.checkbox(&mut app.prefs.snap_grid, "Snap to grid");
            ui.add(egui::Slider::new(&mut app.prefs.snap_step, 0.05..=10.0).text("Snap step"));
            ui.checkbox(&mut app.pivot_mode, "Edit pivot (D)");
        }
        Tool::Rotate => {
            ui.label("Axis orientation");
            ui.radio_value(&mut app.prefs.rotate_world, false, "Object");
            ui.radio_value(&mut app.prefs.rotate_world, true, "World");
            ui.add(egui::Slider::new(&mut app.prefs.rotate_step, 1.0..=90.0).text("Discrete step (hold J)"));
        }
        Tool::Scale => {
            ui.label("Scales in object space. Drag the center for uniform scale; hold Ctrl for fine control.");
        }
        Tool::Select => {
            ui.label("Click to select, Shift toggles, Ctrl deselects, Ctrl+Shift adds. Drag for marquee.");
        }
    }
    ui.separator();
    ui.add(egui::Slider::new(&mut app.prefs.manip_size, 0.3..=3.0).text("Manipulator size (+/-)"));
}
