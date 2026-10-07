//! Graph Editor and Dope Sheet.

use crate::app::{App, LogKind};
use egui::{Color32, Pos2, Rect, RichText, Sense, Stroke};
use rg_core::anim::{Infinity, Tangent};
use rg_core::NodeId;
use rg_script::value::fmt_float;
use std::collections::HashSet;

type KeyRef = (NodeId, String, usize);

pub struct GraphState {
    pub t0: f64,
    pub t1: f64,
    pub v0: f64,
    pub v1: f64,
    pub selected: Vec<KeyRef>,
    pub hidden: HashSet<(NodeId, String)>,
    drag: Option<(Pos2, Vec<(KeyRef, f64, f64)>)>,
    boxsel: Option<(Pos2, Pos2)>,
    pub framed_once: bool,
    pub snap_frames: bool,
}

impl Default for GraphState {
    fn default() -> Self {
        Self { t0: 0.0, t1: 120.0, v0: -10.0, v1: 10.0, selected: Vec::new(), hidden: HashSet::new(), drag: None, boxsel: None, framed_once: false, snap_frames: true }
    }
}

fn attr_color(a: &str) -> Color32 {
    if a.ends_with('X') {
        Color32::from_rgb(230, 70, 70)
    } else if a.ends_with('Y') {
        Color32::from_rgb(90, 220, 90)
    } else if a.ends_with('Z') {
        Color32::from_rgb(90, 140, 255)
    } else {
        Color32::from_rgb(230, 200, 110)
    }
}

fn curves(app: &App) -> Vec<(NodeId, String)> {
    let mut out = Vec::new();
    for id in app.scene.selected() {
        if let Some(n) = app.scene.nodes.get(&id) {
            for a in n.anim.keys() {
                out.push((id, a.clone()));
            }
        }
    }
    out
}

fn frame(app: &mut App, only_selected: bool) {
    let mut tmin = f64::INFINITY;
    let mut tmax = f64::NEG_INFINITY;
    let mut vmin = f64::INFINITY;
    let mut vmax = f64::NEG_INFINITY;
    let sel: HashSet<KeyRef> = app.graph.selected.iter().cloned().collect();
    for (id, a) in curves(app) {
        if app.graph.hidden.contains(&(id, a.clone())) {
            continue;
        }
        let c = &app.scene.nodes[&id].anim[&a];
        for (i, k) in c.keys.iter().enumerate() {
            if only_selected && !sel.contains(&(id, a.clone(), i)) {
                continue;
            }
            tmin = tmin.min(k.time);
            tmax = tmax.max(k.time);
            vmin = vmin.min(k.value);
            vmax = vmax.max(k.value);
        }
        if !only_selected {
            // include in-between samples for overshoot
            if let (Some(a0), Some(a1)) = (c.first_time(), c.last_time()) {
                for s in 0..=40 {
                    let v = c.evaluate(a0 + (a1 - a0) * s as f64 / 40.0);
                    vmin = vmin.min(v);
                    vmax = vmax.max(v);
                }
            }
        }
    }
    if !tmin.is_finite() {
        if only_selected {
            frame(app, false);
        }
        return;
    }
    let tp = ((tmax - tmin) * 0.1).max(2.0);
    let vp = ((vmax - vmin) * 0.15).max(0.5);
    app.graph.t0 = tmin - tp;
    app.graph.t1 = tmax + tp;
    app.graph.v0 = vmin - vp;
    app.graph.v1 = vmax + vp;
}

fn nice_step(range: f64, px: f32, min_px: f32) -> f64 {
    let raw = range * min_px as f64 / px.max(1.0) as f64;
    let p = 10f64.powf(raw.log10().floor());
    for m in [1.0, 2.0, 5.0, 10.0] {
        if p * m >= raw {
            return p * m;
        }
    }
    p * 10.0
}

pub fn graph_editor(app: &mut App, ctx: &egui::Context) {
    let mut open = app.win.graph_editor;
    if !open {
        return;
    }
    egui::Window::new("Graph Editor").open(&mut open).default_size([820.0, 420.0]).show(ctx, |ui| {
        if !app.graph.framed_once {
            frame(app, false);
            app.graph.framed_once = true;
        }
        // toolbar
        ui.horizontal(|ui| {
            for (label, t) in [("auto", Tangent::Auto), ("spline", Tangent::Spline), ("clamped", Tangent::Clamped), ("linear", Tangent::Linear), ("flat", Tangent::Flat), ("step", Tangent::Stepped)] {
                if ui.button(label).on_hover_text(format!("Set selected key tangents to {label}")).clicked() {
                    set_tangents(app, t);
                }
            }
            ui.separator();
            ui.menu_button("Curves ▾", |ui| {
                for (label, inf) in Infinity::ALL.iter().map(|i| (i.name(), *i)) {
                    if ui.button(format!("Pre infinity: {label}")).clicked() {
                        set_infinity(app, Some(inf), None);
                        ui.close();
                    }
                }
                ui.separator();
                for (label, inf) in Infinity::ALL.iter().map(|i| (i.name(), *i)) {
                    if ui.button(format!("Post infinity: {label}")).clicked() {
                        set_infinity(app, None, Some(inf));
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button("Simplify curves").clicked() {
                    app.checkpoint("simplify");
                    for (id, a) in curves(app) {
                        if let Some(c) = app.scene.node_mut(id).and_then(|n| n.anim.get_mut(&a)) {
                            c.simplify(0.01);
                        }
                    }
                    app.graph.selected.clear();
                    ui.close();
                }
            });
            if ui.button("Frame All (A)").clicked() {
                frame(app, false);
            }
            if ui.button("Frame Sel (F)").clicked() {
                frame(app, true);
            }
            ui.checkbox(&mut app.graph.snap_frames, "snap frames");
            ui.separator();
            // selected key stats (editable)
            if let Some((id, a, i)) = app.graph.selected.first().cloned() {
                if let Some(k) = app.scene.nodes.get(&id).and_then(|n| n.anim.get(&a)).and_then(|c| c.keys.get(i)).copied() {
                    ui.label("Stats");
                    let mut t = k.time;
                    let mut v = k.value;
                    let tr = ui.add(egui::DragValue::new(&mut t).speed(0.1).max_decimals(2));
                    let vr = ui.add(egui::DragValue::new(&mut v).speed(0.01).max_decimals(3));
                    if tr.changed() || vr.changed() {
                        if tr.drag_started() || vr.drag_started() || !(tr.dragged() || vr.dragged()) {
                            app.checkpoint("key stats");
                        }
                        let dt = t - k.time;
                        let dv = v - k.value;
                        move_selected(app, dt, dv);
                    }
                }
            }
        });
        ui.separator();
        let avail = ui.available_size();
        ui.horizontal_top(|ui| {
            // curve list
            ui.allocate_ui(egui::vec2(170.0, avail.y - 6.0), |ui| {
                ui.set_min_size(egui::vec2(170.0, avail.y - 6.0));
                egui::ScrollArea::vertical().id_salt("gl").max_height(avail.y - 6.0).show(ui, |ui| {
                    let mut last_node = None;
                    for (id, a) in curves(app) {
                        if last_node != Some(id) {
                            ui.label(RichText::new(app.scene.name_of(id)).strong());
                            last_node = Some(id);
                        }
                        let vis = !app.graph.hidden.contains(&(id, a.clone()));
                        let text = if vis { RichText::new(format!("⚫ {a}")).color(attr_color(&a)) } else { RichText::new(format!("⚪ {a}")).color(Color32::from_gray(120)) };
                        let r = ui.add(egui::Button::new(text).frame(false));
                        if r.clicked() {
                            if ui.input(|i| i.modifiers.ctrl || i.modifiers.shift) {
                                if vis {
                                    app.graph.hidden.insert((id, a.clone()));
                                } else {
                                    app.graph.hidden.remove(&(id, a.clone()));
                                }
                            } else {
                                // solo
                                for (j, b) in curves(app) {
                                    if j == id && b == a {
                                        app.graph.hidden.remove(&(j, b));
                                    } else {
                                        app.graph.hidden.insert((j, b));
                                    }
                                }
                            }
                        }
                        if r.double_clicked() {
                            app.graph.hidden.clear();
                        }
                    }
                    if curves(app).is_empty() {
                        ui.label(RichText::new("Select animated objects.").weak());
                    }
                });
            });
            canvas(app, ui, avail.y - 6.0);
        });
    });
    app.win.graph_editor = open;
}

fn canvas(app: &mut App, ui: &mut egui::Ui, h: f32) {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), h.max(100.0)), Sense::click_and_drag());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, Color32::from_rgb(52, 52, 52));
    let g = &app.graph;
    let (t0, t1, v0, v1) = (g.t0, g.t1, g.v0, g.v1);
    let tx = |t: f64| rect.left() + ((t - t0) / (t1 - t0)) as f32 * rect.width();
    let vy = |v: f64| rect.bottom() - ((v - v0) / (v1 - v0)) as f32 * rect.height();
    let inv_t = |x: f32| t0 + (x - rect.left()) as f64 / rect.width() as f64 * (t1 - t0);
    let inv_v = |y: f32| v0 + (rect.bottom() - y) as f64 / rect.height() as f64 * (v1 - v0);
    // grid
    let ts = nice_step(t1 - t0, rect.width(), 60.0);
    let mut t = (t0 / ts).ceil() * ts;
    while t <= t1 {
        let x = tx(t);
        p.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], Stroke::new(1.0, Color32::from_gray(64)));
        p.text(Pos2::new(x + 2.0, rect.bottom() - 12.0), egui::Align2::LEFT_TOP, fmt_float(t), egui::FontId::proportional(10.0), Color32::from_gray(160));
        t += ts;
    }
    let vs = nice_step(v1 - v0, rect.height(), 40.0);
    let mut v = (v0 / vs).ceil() * vs;
    while v <= v1 {
        let y = vy(v);
        let col = if v.abs() < vs * 0.01 { Color32::from_gray(90) } else { Color32::from_gray(64) };
        p.line_segment([Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)], Stroke::new(1.0, col));
        p.text(Pos2::new(rect.left() + 2.0, y - 12.0), egui::Align2::LEFT_TOP, fmt_float((v * 1000.0).round() / 1000.0), egui::FontId::proportional(10.0), Color32::from_gray(160));
        v += vs;
    }
    // current time
    let cx = tx(app.scene.time.current);
    p.line_segment([Pos2::new(cx, rect.top()), Pos2::new(cx, rect.bottom())], Stroke::new(1.5, Color32::from_rgb(220, 60, 60)));

    let sel: HashSet<KeyRef> = app.graph.selected.iter().cloned().collect();
    let mut key_pts: Vec<(KeyRef, Pos2)> = Vec::new();
    for (id, a) in curves(app) {
        if app.graph.hidden.contains(&(id, a.clone())) {
            continue;
        }
        let c = app.scene.nodes[&id].anim[&a].clone();
        let col = attr_color(&a);
        let mut pts = Vec::with_capacity(rect.width() as usize / 2 + 2);
        let mut x = rect.left();
        while x <= rect.right() {
            pts.push(Pos2::new(x, vy(c.evaluate(inv_t(x)))));
            x += 2.0;
        }
        p.add(egui::Shape::line(pts, Stroke::new(1.5, col)));
        for (i, k) in c.keys.iter().enumerate() {
            let kp = Pos2::new(tx(k.time), vy(k.value));
            let r: KeyRef = (id, a.clone(), i);
            let is_sel = sel.contains(&r);
            if is_sel {
                // tangent handles
                let (ins, outs) = c.slopes(i);
                let len = 40.0;
                for (s, dir) in [(ins, -1.0f32), (outs, 1.0)] {
                    let dx = (t1 - t0) / rect.width() as f64;
                    let dy = (v1 - v0) / rect.height() as f64;
                    let vx = 1.0 / dx;
                    let vyv = s / dy;
                    let l = (vx * vx + vyv * vyv).sqrt();
                    let d = egui::vec2((vx / l) as f32, -(vyv / l) as f32) * len * dir;
                    p.line_segment([kp, kp + d], Stroke::new(1.0, Color32::from_rgb(160, 120, 230)));
                    p.circle_filled(kp + d, 2.5, Color32::from_rgb(160, 120, 230));
                }
            }
            p.rect_filled(Rect::from_center_size(kp, egui::vec2(6.0, 6.0)), 0.0, if is_sel { Color32::from_rgb(255, 230, 40) } else { Color32::BLACK });
            key_pts.push((r, kp));
        }
    }

    // interaction
    let mods = ui.input(|i| i.modifiers);
    if resp.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll.abs() > 0.0 {
            let f = (1.0 - scroll as f64 * 0.002).clamp(0.5, 2.0);
            if let Some(pos) = resp.hover_pos() {
                let (ct, cv) = (inv_t(pos.x), inv_v(pos.y));
                let g = &mut app.graph;
                if !mods.ctrl {
                    g.t0 = ct + (g.t0 - ct) * f;
                    g.t1 = ct + (g.t1 - ct) * f;
                }
                if !mods.shift {
                    g.v0 = cv + (g.v0 - cv) * f;
                    g.v1 = cv + (g.v1 - cv) * f;
                }
            }
        }
        let (fk, ak, del) = ui.input(|i| (i.key_pressed(egui::Key::F), i.key_pressed(egui::Key::A), i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)));
        if fk && !ui.ctx().egui_wants_keyboard_input() {
            frame(app, true);
        }
        if ak && !ui.ctx().egui_wants_keyboard_input() {
            frame(app, false);
        }
        if del && !app.graph.selected.is_empty() {
            delete_selected(app);
        }
    }
    // pan with alt+drag or middle drag
    if resp.dragged_by(egui::PointerButton::Middle) || (resp.dragged_by(egui::PointerButton::Primary) && mods.alt) {
        let d = resp.drag_delta();
        let g = &mut app.graph;
        let dt = -(d.x as f64) / rect.width() as f64 * (g.t1 - g.t0);
        let dv = (d.y as f64) / rect.height() as f64 * (g.v1 - g.v0);
        g.t0 += dt;
        g.t1 += dt;
        g.v0 += dv;
        g.v1 += dv;
        return;
    }
    if resp.drag_started_by(egui::PointerButton::Primary) {
        let pos = resp.interact_pointer_pos().unwrap_or_default();
        let hit = key_pts.iter().find(|(_, kp)| kp.distance(pos) < 6.0).map(|(r, _)| r.clone());
        match hit {
            Some(r) => {
                if !sel.contains(&r) {
                    if mods.shift {
                        app.graph.selected.push(r);
                    } else {
                        app.graph.selected = vec![r];
                    }
                }
                app.checkpoint("move keys");
                let originals: Vec<(KeyRef, f64, f64)> = app
                    .graph
                    .selected
                    .iter()
                    .filter_map(|r| {
                        let k = app.scene.nodes.get(&r.0)?.anim.get(&r.1)?.keys.get(r.2)?;
                        Some((r.clone(), k.time, k.value))
                    })
                    .collect();
                app.graph.drag = Some((pos, originals));
            }
            None => app.graph.boxsel = Some((pos, pos)),
        }
    }
    if let Some((start, originals)) = app.graph.drag.clone() {
        if resp.dragged_by(egui::PointerButton::Primary) {
            let pos = resp.interact_pointer_pos().unwrap_or(start);
            let mut dt = (pos.x - start.x) as f64 / rect.width() as f64 * (t1 - t0);
            let mut dv = -(pos.y - start.y) as f64 / rect.height() as f64 * (v1 - v0);
            if mods.shift {
                // constrain to the dominant axis
                if (pos.x - start.x).abs() > (pos.y - start.y).abs() {
                    dv = 0.0;
                } else {
                    dt = 0.0;
                }
            }
            for ((id, a, i), ot, ov) in &originals {
                if let Some(k) = app.scene.node_mut(*id).and_then(|n| n.anim.get_mut(a)).and_then(|c| c.keys.get_mut(*i)) {
                    let mut nt = ot + dt;
                    if app.graph.snap_frames {
                        nt = nt.round();
                    }
                    k.time = nt;
                    k.value = ov + dv;
                }
            }
            let t = app.scene.time.current;
            app.scene.set_time(t);
        } else {
            app.graph.drag = None;
            // resort curves and fix selection indices
            let mut new_sel = Vec::new();
            let keyset: Vec<(NodeId, String, f64)> = app
                .graph
                .selected
                .iter()
                .filter_map(|r| app.scene.nodes.get(&r.0)?.anim.get(&r.1)?.keys.get(r.2).map(|k| (r.0, r.1.clone(), k.time)))
                .collect();
            for (id, a) in curves(app) {
                if let Some(c) = app.scene.node_mut(id).and_then(|n| n.anim.get_mut(&a)) {
                    c.sort();
                }
            }
            for (id, a, t) in keyset {
                if let Some(i) = app.scene.nodes.get(&id).and_then(|n| n.anim.get(&a)).and_then(|c| c.key_at(t)) {
                    new_sel.push((id, a, i));
                }
            }
            app.graph.selected = new_sel;
            app.log(LogKind::Command, "keyframe -e -r -tc … ;");
        }
    }
    if let Some((a, _)) = app.graph.boxsel {
        if resp.dragged_by(egui::PointerButton::Primary) || resp.is_pointer_button_down_on() {
            let pos = resp.interact_pointer_pos().unwrap_or(a);
            app.graph.boxsel = Some((a, pos));
            p.rect_stroke(Rect::from_two_pos(a, pos), 0.0, Stroke::new(1.0, Color32::WHITE), egui::StrokeKind::Inside);
        } else {
            let (a, b) = app.graph.boxsel.take().unwrap();
            let r = Rect::from_two_pos(a, b);
            let hits: Vec<KeyRef> = key_pts.iter().filter(|(_, kp)| r.expand(3.0).contains(*kp)).map(|(k, _)| k.clone()).collect();
            if mods.shift {
                for h in hits {
                    if !app.graph.selected.contains(&h) {
                        app.graph.selected.push(h);
                    }
                }
            } else {
                app.graph.selected = hits;
            }
        }
    }
    // click on empty space with right button: set time
    if resp.secondary_clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            app.set_time(inv_t(pos.x).round());
        }
    }
}

fn set_tangents(app: &mut App, t: Tangent) {
    app.checkpoint("keyTangent");
    let sel = app.graph.selected.clone();
    let targets: Vec<KeyRef> = if sel.is_empty() {
        curves(app).into_iter().flat_map(|(id, a)| (0..app.scene.nodes[&id].anim[&a].len()).map(move |i| (id, a.clone(), i))).collect()
    } else {
        sel
    };
    for (id, a, i) in targets {
        if let Some(c) = app.scene.node_mut(id).and_then(|n| n.anim.get_mut(&a)) {
            c.set_tangents(i, t);
        }
    }
    let now = app.scene.time.current;
    app.scene.set_time(now);
    app.log(LogKind::Command, format!("keyTangent -itt {0} -ott {0};", t.name()));
}

fn set_infinity(app: &mut App, pre: Option<Infinity>, post: Option<Infinity>) {
    app.checkpoint("setInfinity");
    for (id, a) in curves(app) {
        if app.graph.hidden.contains(&(id, a.clone())) {
            continue;
        }
        if let Some(c) = app.scene.node_mut(id).and_then(|n| n.anim.get_mut(&a)) {
            if let Some(p) = pre {
                c.pre_infinity = p;
            }
            if let Some(p) = post {
                c.post_infinity = p;
            }
        }
    }
    let now = app.scene.time.current;
    app.scene.set_time(now);
}

fn move_selected(app: &mut App, dt: f64, dv: f64) {
    for (id, a, i) in app.graph.selected.clone() {
        if let Some(k) = app.scene.node_mut(id).and_then(|n| n.anim.get_mut(&a)).and_then(|c| c.keys.get_mut(i)) {
            k.time += dt;
            k.value += dv;
        }
    }
    let now = app.scene.time.current;
    app.scene.set_time(now);
}

fn delete_selected(app: &mut App) {
    app.checkpoint("delete keys");
    let mut by_curve: std::collections::BTreeMap<(NodeId, String), Vec<usize>> = Default::default();
    for (id, a, i) in app.graph.selected.drain(..) {
        by_curve.entry((id, a)).or_default().push(i);
    }
    for ((id, a), mut idx) in by_curve {
        idx.sort_unstable_by(|x, y| y.cmp(x));
        if let Some(n) = app.scene.node_mut(id) {
            if let Some(c) = n.anim.get_mut(&a) {
                for i in idx {
                    if i < c.keys.len() {
                        c.keys.remove(i);
                    }
                }
            }
            n.anim.retain(|_, c| !c.is_empty());
        }
    }
    let now = app.scene.time.current;
    app.scene.set_time(now);
}

// --------------------------------------------------------------- dope sheet

pub fn dope_sheet(app: &mut App, ctx: &egui::Context) {
    let mut open = app.win.dope_sheet;
    if !open {
        return;
    }
    egui::Window::new("Dope Sheet").open(&mut open).default_size([760.0, 300.0]).show(ctx, |ui| {
        let (min, max) = (app.scene.time.min, app.scene.time.max);
        let span = (max - min).max(1.0);
        let rows: Vec<(NodeId, Option<String>)> = app
            .scene
            .selected()
            .into_iter()
            .flat_map(|id| {
                let mut v = vec![(id, None)];
                if let Some(n) = app.scene.nodes.get(&id) {
                    v.extend(n.anim.keys().map(|a| (id, Some(a.clone()))));
                }
                v
            })
            .collect();
        if rows.is_empty() {
            ui.label(RichText::new("Select animated objects.").weak());
            return;
        }
        let state_id = ui.id().with("ds_drag");
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (id, attr) in rows {
                ui.horizontal(|ui| {
                    let label = match &attr {
                        None => RichText::new(app.scene.name_of(id)).strong(),
                        Some(a) => RichText::new(format!("   {a}")).color(attr_color(a)),
                    };
                    ui.add_sized([150.0, 16.0], egui::Label::new(label));
                    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 16.0), Sense::click_and_drag());
                    let p = ui.painter_at(rect);
                    p.rect_filled(rect, 0.0, if attr.is_none() { Color32::from_rgb(60, 60, 66) } else { Color32::from_rgb(52, 52, 52) });
                    let tx = |t: f64| rect.left() + ((t - min) / span) as f32 * rect.width();
                    let cx = tx(app.scene.time.current);
                    p.line_segment([Pos2::new(cx, rect.top()), Pos2::new(cx, rect.bottom())], Stroke::new(1.0, Color32::from_rgb(220, 60, 60)));
                    let times: Vec<f64> = match &attr {
                        None => app.scene.key_times(&[id]),
                        Some(a) => app.scene.nodes[&id].anim[a].keys.iter().map(|k| k.time).collect(),
                    };
                    for t in &times {
                        let x = tx(*t);
                        let c = Pos2::new(x, rect.center().y);
                        p.add(egui::Shape::convex_polygon(vec![c + egui::vec2(0.0, -5.0), c + egui::vec2(5.0, 0.0), c + egui::vec2(0.0, 5.0), c + egui::vec2(-5.0, 0.0)], Color32::from_rgb(200, 200, 200), Stroke::new(1.0, Color32::BLACK)));
                    }
                    // drag a key horizontally to retime (all channels on the summary row)
                    if resp.drag_started() {
                        if let Some(pos) = resp.interact_pointer_pos() {
                            if let Some(t) = times.iter().copied().find(|t| (tx(*t) - pos.x).abs() < 6.0) {
                                app.checkpoint("retime key");
                                ui.data_mut(|d| d.insert_temp(state_id, (id, attr.clone(), t, pos.x)));
                            }
                        }
                    }
                    if resp.dragged() {
                        if let Some((did, dattr, t, x0)) = ui.data_mut(|d| d.get_temp::<(NodeId, Option<String>, f64, f32)>(state_id)) {
                            if did == id && dattr == attr {
                                let pos = resp.interact_pointer_pos().unwrap_or_default();
                                let nt = (t + ((pos.x - x0) as f64 / rect.width() as f64 * span)).round();
                                if (nt - t).abs() >= 1.0 {
                                    if let Some(n) = app.scene.node_mut(id) {
                                        for (a, c) in n.anim.iter_mut() {
                                            if attr.as_ref().is_none_or(|x| x == a) {
                                                if let Some(i) = c.key_at(t) {
                                                    if c.key_at(nt).is_none() {
                                                        c.keys[i].time = nt;
                                                        c.sort();
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    ui.data_mut(|d| d.insert_temp(state_id, (id, attr.clone(), nt, pos.x)));
                                }
                            }
                        }
                    }
                    if resp.drag_stopped() {
                        ui.data_mut(|d| d.remove::<(NodeId, Option<String>, f64, f32)>(state_id));
                        let now = app.scene.time.current;
                        app.scene.set_time(now);
                    }
                    if resp.clicked() {
                        if let Some(pos) = resp.interact_pointer_pos() {
                            let t = (min + (pos.x - rect.left()) as f64 / rect.width() as f64 * span).round();
                            app.set_time(t);
                        }
                    }
                });
            }
        });
    });
    app.win.dope_sheet = open;
}
