//! Time slider, range slider with playback controls, command line, help line.

use crate::app::{App, LogKind, PlayMode};
use crate::theme;
use egui::{Color32, Pos2, RichText, Sense, Stroke};
use rg_script::value::fmt_float;

pub fn time_slider(app: &mut App, ui: &mut egui::Ui) {
    let (min, max) = (app.scene.time.min, app.scene.time.max);
    let span = (max - min).max(1.0);
    let ctrl_w = 330.0;
    ui.horizontal(|ui| {
        let w = (ui.available_width() - ctrl_w).max(100.0);
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 34.0), Sense::click_and_drag());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 0.0, Color32::from_rgb(56, 56, 56));
        let x_of = |t: f64| rect.left() + 8.0 + ((t - min) / span) as f32 * (rect.width() - 16.0);
        // ticks
        let px_per_frame = (rect.width() - 16.0) / span as f32;
        let step: f64 = [1.0f64, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0].into_iter().find(|s| *s * px_per_frame as f64 >= 40.0).unwrap_or(1000.0);
        let mut t = (min / step).ceil() * step;
        while t <= max {
            let x = x_of(t);
            p.line_segment([Pos2::new(x, rect.top() + 14.0), Pos2::new(x, rect.bottom())], Stroke::new(1.0, Color32::from_gray(130)));
            p.text(Pos2::new(x + 2.0, rect.top() + 2.0), egui::Align2::LEFT_TOP, fmt_float(t), egui::FontId::proportional(10.0), Color32::from_gray(190));
            t += step;
        }
        if px_per_frame >= 6.0 {
            let mut f = min.ceil();
            while f <= max {
                let x = x_of(f);
                p.line_segment([Pos2::new(x, rect.bottom() - 6.0), Pos2::new(x, rect.bottom())], Stroke::new(1.0, Color32::from_gray(100)));
                f += 1.0;
            }
        }
        // key ticks for selection
        let sel = app.scene.selected();
        for k in app.scene.key_times(&sel) {
            if k >= min && k <= max {
                let x = x_of(k);
                p.rect_filled(egui::Rect::from_min_max(Pos2::new(x - 1.5, rect.top() + 14.0), Pos2::new(x + 1.5, rect.bottom())), 0.0, theme::KEYED);
            }
        }
        // current time
        let cur = app.scene.time.current;
        let x = x_of(cur);
        let cw = px_per_frame.max(3.0);
        p.rect_filled(egui::Rect::from_min_max(Pos2::new(x - cw / 2.0, rect.top() + 12.0), Pos2::new(x + cw / 2.0, rect.bottom())), 0.0, Color32::from_rgba_unmultiplied(170, 210, 255, 140));
        p.text(Pos2::new(x + cw / 2.0 + 2.0, rect.top() + 14.0), egui::Align2::LEFT_TOP, fmt_float(cur), egui::FontId::proportional(11.0), Color32::WHITE);
        if (resp.dragged() || resp.clicked()) && !ui.input(|i| i.pointer.secondary_down()) {
            if let Some(pos) = resp.interact_pointer_pos() {
                let t = min + ((pos.x - rect.left() - 8.0) / (rect.width() - 16.0)) as f64 * span;
                let t = t.round().clamp(min, max);
                app.set_time(t);
            }
        }
        resp.context_menu(|ui| {
            let names = app.selection_names();
            let t = app.scene.time.current;
            if ui.button("Set Key").clicked() {
                let _ = app.mel("setKeyframe;");
                ui.close();
            }
            if ui.button(format!("Delete Key at {}", fmt_float(t))).clicked() {
                let _ = app.mel(&format!("cutKey -t {t} {names};"));
                ui.close();
            }
            for tt in ["auto", "spline", "linear", "flat", "step"] {
                if ui.button(format!("Tangents → {tt}")).clicked() {
                    let _ = app.mel(&format!("keyTangent -t {t} -itt {tt} -ott {tt} {names};"));
                    ui.close();
                }
            }
        });
        // controls
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                let mut cur = app.scene.time.current;
                if ui.add(egui::DragValue::new(&mut cur).speed(0.2).max_decimals(2)).changed() {
                    app.set_time(cur);
                }
                ui.spacing_mut().item_spacing.x = 1.0;
                let b = |ui: &mut egui::Ui, s: &str, tip: &str| ui.add(egui::Button::new(s).min_size(egui::vec2(22.0, 18.0))).on_hover_text(tip).clicked();
                if b(ui, "⏮", "Go to start") {
                    let t = app.scene.time.min;
                    app.set_time(t);
                }
                if b(ui, "◀", "Step back one frame (Alt+,)") {
                    app.step_time(-1.0);
                }
                if b(ui, "⏪", "Previous key (,)") {
                    app.next_key(false);
                }
                if b(ui, if app.playing && app.play_backwards { "⏹" } else { "⏴" }, "Play backwards") {
                    app.play_backwards = true;
                    app.playing = !app.playing;
                }
                if b(ui, if app.playing && !app.play_backwards { "⏹" } else { "⏵" }, "Play forwards (Alt+V)") {
                    app.play_backwards = false;
                    app.playing = !app.playing;
                }
                if b(ui, "⏩", "Next key (.)") {
                    app.next_key(true);
                }
                if b(ui, "▶", "Step forward one frame (Alt+.)") {
                    app.step_time(1.0);
                }
                if b(ui, "⏭", "Go to end") {
                    let t = app.scene.time.max;
                    app.set_time(t);
                }
            });
            ui.horizontal(|ui| {
                let key = egui::Button::new(RichText::new("🔑 Auto Key").color(if app.auto_key { Color32::WHITE } else { Color32::from_gray(180) })).fill(if app.auto_key { theme::KEYED } else { Color32::from_rgb(80, 80, 80) });
                if ui.add(key).on_hover_text("Auto keyframe: key animated channels when you change them").clicked() {
                    app.auto_key = !app.auto_key;
                }
                egui::ComboBox::from_id_salt("fps").selected_text(format!("{} fps", fmt_float(app.scene.time.fps))).width(70.0).show_ui(ui, |ui| {
                    for f in [12.0, 15.0, 24.0, 25.0, 30.0, 48.0, 50.0, 60.0] {
                        if ui.selectable_label(app.scene.time.fps == f, format!("{f} fps")).clicked() {
                            app.scene.time.fps = f;
                        }
                    }
                });
                egui::ComboBox::from_id_salt("loop").selected_text(match app.prefs.play_mode {
                    PlayMode::Loop => "loop",
                    PlayMode::Once => "once",
                    PlayMode::Oscillate => "osc",
                }).width(55.0).show_ui(ui, |ui| {
                    ui.selectable_value(&mut app.prefs.play_mode, PlayMode::Loop, "loop");
                    ui.selectable_value(&mut app.prefs.play_mode, PlayMode::Once, "once");
                    ui.selectable_value(&mut app.prefs.play_mode, PlayMode::Oscillate, "oscillate");
                });
            });
        });
    });
}

pub fn range_slider(app: &mut App, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        let ts = &mut app.scene.time;
        let mut changed = false;
        changed |= ui.add(egui::DragValue::new(&mut ts.anim_start).speed(0.5).max_decimals(1)).on_hover_text("Animation start").changed();
        changed |= ui.add(egui::DragValue::new(&mut ts.min).speed(0.5).max_decimals(1)).on_hover_text("Playback start").changed();
        let w = (ui.available_width() - 160.0).max(80.0);
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 16.0), Sense::click_and_drag());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 3.0, Color32::from_rgb(48, 48, 48));
        let aspan = (ts.anim_end - ts.anim_start).max(1.0);
        let x_of = |t: f64| rect.left() + ((t - ts.anim_start) / aspan) as f32 * rect.width();
        let bar = egui::Rect::from_min_max(Pos2::new(x_of(ts.min), rect.top() + 2.0), Pos2::new(x_of(ts.max), rect.bottom() - 2.0));
        p.rect_filled(bar, 3.0, Color32::from_rgb(120, 120, 120));
        p.text(bar.left_center() + egui::vec2(4.0, 0.0), egui::Align2::LEFT_CENTER, fmt_float(ts.min), egui::FontId::proportional(10.0), Color32::BLACK);
        p.text(bar.right_center() - egui::vec2(4.0, 0.0), egui::Align2::RIGHT_CENTER, fmt_float(ts.max), egui::FontId::proportional(10.0), Color32::BLACK);
        if resp.dragged() {
            let d = resp.drag_delta().x as f64 / rect.width() as f64 * aspan;
            let pos = resp.interact_pointer_pos().unwrap_or_default();
            let near_left = (pos.x - bar.left()).abs() < 8.0;
            let near_right = (pos.x - bar.right()).abs() < 8.0;
            if near_left && !near_right {
                ts.min = (ts.min + d).min(ts.max - 1.0);
            } else if near_right && !near_left {
                ts.max = (ts.max + d).max(ts.min + 1.0);
            } else {
                ts.min += d;
                ts.max += d;
            }
            changed = true;
        }
        if resp.drag_stopped() {
            ts.min = ts.min.round();
            ts.max = ts.max.round();
        }
        changed |= ui.add(egui::DragValue::new(&mut ts.max).speed(0.5).max_decimals(1)).on_hover_text("Playback end").changed();
        changed |= ui.add(egui::DragValue::new(&mut ts.anim_end).speed(0.5).max_decimals(1)).on_hover_text("Animation end").changed();
        if changed {
            ts.anim_start = ts.anim_start.min(ts.min);
            ts.anim_end = ts.anim_end.max(ts.max);
            app.scene.touch();
        }
    });
}

pub fn command_line(app: &mut App, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("MEL").strong().color(Color32::from_rgb(160, 200, 255)));
        let id = egui::Id::new("cmdline");
        let w = ui.available_width() * 0.6;
        let te = ui.add(egui::TextEdit::singleline(&mut app.cmd_line).id(id).desired_width(w).font(egui::TextStyle::Monospace).hint_text("type a MEL command, e.g. polyCube -w 2; move -r 0 1 0;"));
        if te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            let cmd = app.cmd_line.trim().to_string();
            if !cmd.is_empty() {
                let cmd = if cmd.ends_with(';') || cmd.ends_with('}') { cmd } else { format!("{cmd};") };
                let _ = app.mel(&cmd);
                app.cmd_line.clear();
            }
            te.request_focus();
        }
        // history recall with up/down
        if te.has_focus() {
            let (up, down) = ui.input(|i| (i.key_pressed(egui::Key::ArrowUp), i.key_pressed(egui::Key::ArrowDown)));
            if up || down {
                let cmds: Vec<String> = app.log.iter().filter(|(k, _)| *k == LogKind::Command).map(|(_, s)| s.clone()).collect();
                let idx_id = egui::Id::new("cmd_hist_idx");
                let mut idx: usize = ui.data_mut(|d| d.get_temp(idx_id).unwrap_or(0));
                if up {
                    idx = (idx + 1).min(cmds.len());
                } else {
                    idx = idx.saturating_sub(1);
                }
                ui.data_mut(|d| d.insert_temp(idx_id, idx));
                app.cmd_line = if idx == 0 { String::new() } else { cmds[cmds.len() - idx].clone() };
            }
        }
        let res_col = if app.cmd_result.starts_with("Error") { Color32::from_rgb(255, 120, 110) } else { Color32::from_gray(210) };
        egui::Frame::new().fill(theme::FIELD).inner_margin(egui::Margin::symmetric(4, 2)).show(ui, |ui| {
            ui.set_min_width(ui.available_width() - 30.0);
            ui.label(RichText::new(&app.cmd_result).monospace().color(res_col));
        });
        if ui.button("{}").on_hover_text("Script Editor").clicked() {
            app.win.script_editor = !app.win.script_editor;
        }
    });
}

pub fn help_line(app: &mut App, ui: &mut egui::Ui) {
    let help = if !app.help_line.is_empty() {
        app.help_line.clone()
    } else {
        match app.tool {
            crate::app::Tool::Select => "Select Tool: select an object".into(),
            crate::app::Tool::Move => "Move Tool: use manipulator to move object(s). Ctrl+drag/hold X to snap. D edits pivot.".into(),
            crate::app::Tool::Rotate => "Rotate Tool: use manipulator to rotate object(s). Hold J for discrete steps.".into(),
            crate::app::Tool::Scale => "Scale Tool: use manipulator to scale object(s). Center handle scales uniformly.".into(),
        }
    };
    ui.horizontal(|ui| {
        ui.label(RichText::new(help).size(11.0).color(Color32::from_gray(185)));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(format!("{:.0} fps", app.fps_display)).size(10.0).weak());
            ui.label(RichText::new(app.title()).size(10.0).weak());
        });
    });
}
