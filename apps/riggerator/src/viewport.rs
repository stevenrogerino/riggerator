//! 3D viewport panes: navigation, picking, manipulators and drawing.

use crate::app::{App, LogKind, NavMode, Tool};
use crate::camera::*;
use crate::gl::{self, CpuMesh, DrawItem, DrawList, GlLight, MeshKey, TexMode};
use crate::theme;
use egui::{Color32, Pos2, Rect, Sense, Stroke};
use glam::{Mat4, Quat, Vec3};
use rg_core::mesh::{Edge, Mesh};
use rg_core::*;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShadeMode {
    Wireframe,
    Smooth,
    Textured,
}

#[derive(Clone, Debug)]
pub struct ShowFlags {
    pub polygons: bool,
    pub joints: bool,
    pub curves: bool,
    pub cameras: bool,
    pub lights: bool,
    pub locators: bool,
    pub ik: bool,
    pub grid: bool,
    pub manips: bool,
    pub hud: bool,
}

impl Default for ShowFlags {
    fn default() -> Self {
        Self { polygons: true, joints: true, curves: true, cameras: true, lights: true, locators: true, ik: true, grid: true, manips: true, hud: true }
    }
}

#[derive(Clone, Debug)]
pub struct ViewPane {
    pub cam: ViewCamera,
    pub shade: ShadeMode,
    pub wire_on_shaded: bool,
    pub xray: bool,
    pub xray_joints: bool,
    pub backface: bool,
    pub use_lights: bool,
    pub show: ShowFlags,
    pub isolate: Option<Vec<NodeId>>,
    nav: Option<(egui::PointerButton, Pos2)>,
    marquee: Option<(Pos2, Pos2)>,
    vslider: Option<(f32, Vec<(NodeId, String, f64)>)>,
    pub rect: Rect,
    picks: Vec<(NodeId, Pos2, Pos2)>,
}

impl ViewPane {
    pub fn new(kind: CamKind) -> Self {
        Self {
            cam: ViewCamera::new(kind),
            shade: ShadeMode::Smooth,
            wire_on_shaded: false,
            xray: false,
            xray_joints: true,
            backface: false,
            use_lights: false,
            show: ShowFlags::default(),
            isolate: None,
            nav: None,
            marquee: None,
            vslider: None,
            rect: Rect::NOTHING,
            picks: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Handle {
    Axis(usize),
    Plane(usize),
    Screen,
    Ring(usize),
    ScreenRing,
    Uniform,
}

#[derive(Clone, Debug)]
pub struct ManipDrag {
    pub handle: Handle,
    pub tool: Tool,
    pub pane: usize,
    pub start_mouse: Pos2,
    pub center: Vec3,
    pub axes: [Vec3; 3],
    pub t0: f32,
    pub hit0: Vec3,
    pub objects: Vec<(NodeId, Transform, Mat4)>,
    pub comp: Option<(NodeId, Vec<(u32, Vec3)>, Mat4)>,
    pub last: Vec3,
    pub angle: f32,
    pub prev_screen_angle: f32,
    pub moved: bool,
    pub pivot: bool,
}

// -------------------------------------------------------------- helpers --

fn rgba(c: [f32; 4]) -> [f32; 4] {
    c
}

const AXIS_COL: [[f32; 4]; 3] = [[0.9, 0.15, 0.15, 1.0], [0.25, 0.85, 0.2, 1.0], [0.2, 0.35, 1.0, 1.0]];
const HOVER_COL: [f32; 4] = [1.0, 1.0, 0.1, 1.0];

fn c32(c: [f32; 4]) -> Color32 {
    theme::c32(c)
}

fn node_wire_color(app: &App, id: NodeId, default: [f32; 4]) -> [f32; 4] {
    let sel = &app.scene.selection;
    if sel.last() == Some(&id) {
        return theme::SEL_GREEN;
    }
    if sel.contains(&id) {
        return theme::LEAD_WHITE;
    }
    let n = &app.scene.nodes[&id];
    if n.template {
        return [0.55, 0.55, 0.55, 1.0];
    }
    if let Some(c) = n.color {
        return [c.x, c.y, c.z, 1.0];
    }
    for l in &app.scene.layers {
        if l.members.contains(&id) {
            if let Some(c) = l.color {
                return [c.x, c.y, c.z, 1.0];
            }
            if l.reference {
                return [0.1, 0.1, 0.1, 1.0];
            }
        }
    }
    // parent selected -> highlight children like Maya (purple-ish)
    let mut cur = n.parent;
    while let Some(p) = cur {
        if sel.contains(&p) {
            return [0.62, 0.86, 0.52, 1.0];
        }
        cur = app.scene.nodes.get(&p).and_then(|x| x.parent);
    }
    default
}

fn smooth_mesh(app: &mut App, mesh: &Arc<Mesh>, level: u32) -> Arc<Mesh> {
    if level == 0 {
        return mesh.clone();
    }
    let key = (Arc::as_ptr(mesh) as usize, level);
    if let Some((_, r)) = app.smooth_cache.get(&key) {
        app.smooth_used.insert(key);
        return r.clone();
    }
    let r = Arc::new(rg_core::mesh::ops::smooth(mesh, level.min(3)));
    app.smooth_cache.insert(key, (mesh.clone(), r.clone()));
    app.smooth_used.insert(key);
    r
}

fn visible(app: &App, pane: &ViewPane, id: NodeId) -> bool {
    if !app.scene.is_visible(id) {
        return false;
    }
    if let Some(iso) = &pane.isolate {
        let mut cur = Some(id);
        while let Some(c) = cur {
            if iso.contains(&c) {
                return true;
            }
            cur = app.scene.nodes.get(&c).and_then(|n| n.parent);
        }
        return false;
    }
    true
}

// ------------------------------------------------------------------ UI --

pub fn pane_ui(app: &mut App, ui: &mut egui::Ui, rect: Rect, idx: usize) {
    let ev = app.evaluated();
    // ---- header bar
    let bar = Rect::from_min_size(rect.min, egui::vec2(rect.width(), 20.0));
    let view_rect = Rect::from_min_max(egui::pos2(rect.left(), bar.bottom()), rect.max);
    header_bar(app, ui, bar, idx);
    app.panes[idx].rect = view_rect;

    let resp = ui.interact(view_rect, ui.id().with(("viewport", idx)), Sense::click_and_drag());
    if resp.hovered() || resp.is_pointer_button_down_on() {
        if ui.input(|i| i.pointer.any_pressed()) {
            app.active_pane = idx;
        }
    }
    let painter = ui.painter_at(view_rect);
    // background gradient (Maya style)
    let top = Color32::from_rgb(105, 120, 134);
    let bottom = Color32::from_rgb(38, 40, 44);
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(view_rect.left_top(), top);
    mesh.colored_vertex(view_rect.right_top(), top);
    mesh.colored_vertex(view_rect.right_bottom(), bottom);
    mesh.colored_vertex(view_rect.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));

    let aspect = view_rect.width() / view_rect.height().max(1.0);
    let pane = app.panes[idx].clone();
    let view = pane.cam.view(&app.scene, &ev);
    let proj = pane.cam.proj(&app.scene, aspect);
    let projector = Projector::new(view, proj, view_rect, pane.cam.is_ortho(&app.scene));

    handle_input(app, ui, &resp, idx, &projector, &ev);

    // ---- build draw list
    let dl = build_draw_list(app, idx, &projector, view, proj, &ev);
    if let Some(gl) = app.gl.clone() {
        let cb = egui_glow::CallbackFn::new(move |_info, painter| {
            let mut r = gl.lock().unwrap();
            r.paint(painter.gl(), &dl);
        });
        painter.add(egui::PaintCallback { rect: view_rect, callback: Arc::new(cb) });
    } else {
        painter.text(view_rect.center(), egui::Align2::CENTER_CENTER, "OpenGL unavailable", egui::FontId::proportional(16.0), Color32::WHITE);
    }

    draw_overlays(app, &painter, idx, &projector, &ev);

    // context (marking) menu
    if !ui.input(|i| i.modifiers.alt) {
        resp.context_menu(|ui| marking_menu(app, ui));
    }
    // active pane border
    if app.layout != crate::app::Layout::Single && app.active_pane == idx {
        painter.rect_stroke(view_rect.shrink(1.0), 0.0, Stroke::new(2.0, Color32::from_rgb(120, 180, 230)), egui::StrokeKind::Inside);
    }
}

fn header_bar(app: &mut App, ui: &mut egui::Ui, bar: Rect, idx: usize) {
    ui.painter().rect_filled(bar, 0.0, Color32::from_rgb(58, 58, 58));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(bar.shrink2(egui::vec2(4.0, 0.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
    let ui = &mut child;
    let cams: Vec<NodeId> = app.scene.walk().into_iter().filter(|id| matches!(app.scene.nodes[id].kind, NodeKind::Camera(_))).collect();
    ui.menu_button("View", |ui| {
        for (k, label) in [(CamKind::Persp, "persp"), (CamKind::Top, "top"), (CamKind::Front, "front"), (CamKind::Side, "side")] {
            if ui.selectable_label(app.panes[idx].cam.kind == k, label).clicked() {
                let keep = app.panes[idx].cam.clone();
                app.panes[idx].cam = ViewCamera::new(k);
                if k == CamKind::Persp && keep.kind == CamKind::Persp {
                    app.panes[idx].cam = keep;
                }
                ui.close();
            }
        }
        if !cams.is_empty() {
            ui.separator();
            for c in &cams {
                if ui.selectable_label(app.panes[idx].cam.kind == CamKind::Scene(*c), app.scene.name_of(*c)).clicked() {
                    app.panes[idx].cam.kind = CamKind::Scene(*c);
                    ui.close();
                }
            }
        }
        ui.separator();
        if ui.button("Look Through Selected").clicked() {
            if let Some(c) = app.scene.selected().into_iter().find(|id| matches!(app.scene.nodes[id].kind, NodeKind::Camera(_))) {
                app.panes[idx].cam.kind = CamKind::Scene(c);
            } else {
                app.flash("Select a camera to look through");
            }
            ui.close();
        }
        if ui.button("Frame All  (A)").clicked() {
            app.active_pane = idx;
            app.frame_selected(true);
            ui.close();
        }
        if ui.button("Frame Selection  (F)").clicked() {
            app.active_pane = idx;
            app.frame_selected(false);
            ui.close();
        }
        if ui.button("Default Home").clicked() {
            let k = app.panes[idx].cam.kind;
            app.panes[idx].cam = ViewCamera::new(if matches!(k, CamKind::Scene(_)) { CamKind::Persp } else { k });
            ui.close();
        }
        if ui.button("Create Camera From View").clicked() {
            let ev = app.evaluated();
            let w = app.panes[idx].cam.world(&app.scene, &ev);
            let (_, r, t) = w.to_scale_rotation_translation();
            app.checkpoint("camera");
            let id = app.scene.add_node("camera", NodeKind::Camera(CameraShape::default()), None);
            let n = app.scene.node_mut(id).unwrap();
            n.transform.translate = t;
            n.transform.rotate = RotateOrder::XYZ.from_quat(r, None);
            app.scene.select(&[id], SelectMode::Replace);
            ui.close();
        }
    });
    ui.menu_button("Shading", |ui| {
        let p = &mut app.panes[idx];
        ui.radio_value(&mut p.shade, ShadeMode::Wireframe, "Wireframe  (4)");
        ui.radio_value(&mut p.shade, ShadeMode::Smooth, "Smooth Shade All  (5)");
        ui.radio_value(&mut p.shade, ShadeMode::Textured, "Textured  (6)");
        ui.separator();
        ui.checkbox(&mut p.wire_on_shaded, "Wireframe on Shaded");
        ui.checkbox(&mut p.xray, "X-Ray  (Alt+A)");
        ui.checkbox(&mut p.xray_joints, "X-Ray Joints");
        ui.checkbox(&mut p.backface, "Backface Culling");
    });
    ui.menu_button("Lighting", |ui| {
        let p = &mut app.panes[idx];
        ui.radio_value(&mut p.use_lights, false, "Use Default Lighting");
        ui.radio_value(&mut p.use_lights, true, "Use All Lights  (7)");
    });
    ui.menu_button("Show", |ui| {
        let s = &mut app.panes[idx].show;
        if ui.button("All").clicked() {
            *s = ShowFlags::default();
        }
        ui.separator();
        ui.checkbox(&mut s.polygons, "Polygons");
        ui.checkbox(&mut s.curves, "NURBS Curves");
        ui.checkbox(&mut s.joints, "Joints");
        ui.checkbox(&mut s.ik, "IK Handles");
        ui.checkbox(&mut s.locators, "Locators");
        ui.checkbox(&mut s.cameras, "Cameras");
        ui.checkbox(&mut s.lights, "Lights");
        ui.separator();
        ui.checkbox(&mut s.grid, "Grid");
        ui.checkbox(&mut s.manips, "Manipulators");
        ui.checkbox(&mut s.hud, "Heads Up Display");
        ui.separator();
        let iso = app.panes[idx].isolate.is_some();
        if ui.selectable_label(iso, "Isolate Select").clicked() {
            app.panes[idx].isolate = if iso { None } else { Some(app.scene.selected()) };
            ui.close();
        }
    });
    ui.menu_button("Panels", |ui| {
        if ui.button("Single Pane").clicked() {
            app.layout = crate::app::Layout::Single;
            app.active_pane = idx;
            ui.close();
        }
        if ui.button("Four Panes").clicked() {
            app.layout = crate::app::Layout::Quad;
            ui.close();
        }
        if ui.button("Two Panes Side by Side").clicked() {
            app.layout = crate::app::Layout::TwoSide;
            ui.close();
        }
    });
    ui.separator();
    let p = &mut app.panes[idx];
    let tog = |ui: &mut egui::Ui, on: bool, label: &str, tip: &str| ui.selectable_label(on, label).on_hover_text(tip).clicked();
    if tog(ui, p.shade == ShadeMode::Wireframe, "Wire", "Wireframe (4)") {
        p.shade = ShadeMode::Wireframe;
    }
    if tog(ui, p.shade == ShadeMode::Smooth, "Shade", "Smooth shade (5)") {
        p.shade = ShadeMode::Smooth;
    }
    if tog(ui, p.shade == ShadeMode::Textured, "Tex", "Textured (6)") {
        p.shade = ShadeMode::Textured;
    }
    if tog(ui, p.wire_on_shaded, "W/S", "Wireframe on shaded") {
        p.wire_on_shaded = !p.wire_on_shaded;
    }
    if tog(ui, p.use_lights, "Lights", "Use all lights (7)") {
        p.use_lights = !p.use_lights;
    }
    if tog(ui, p.xray, "X-Ray", "X-Ray (Alt+A)") {
        p.xray = !p.xray;
    }
    if tog(ui, p.show.grid, "Grid", "Grid") {
        p.show.grid = !p.show.grid;
    }
    if tog(ui, p.isolate.is_some(), "Iso", "Isolate select") {
        p.isolate = if p.isolate.is_some() { None } else { Some(app.scene.selected()) };
    }
}

// --------------------------------------------------------------- input --

fn handle_input(app: &mut App, ui: &egui::Ui, resp: &egui::Response, idx: usize, pr: &Projector, ev: &Arc<Evaluated>) {
    let (mods, pointer, scroll) = ui.input(|i| (i.modifiers, i.pointer.clone(), i.smooth_scroll_delta));
    let Some(pos) = pointer.interact_pos().or(pointer.hover_pos()) else { return };
    let inside = pr.rect.contains(pos);
    let size = glam::Vec2::new(pr.rect.width(), pr.rect.height());

    // wheel zoom
    if inside && resp.hovered() && scroll.y.abs() > 0.0 {
        let s = app.scene.clone_light();
        app.panes[idx].cam.dolly(scroll.y * 0.0015, &s);
        sync_scene_camera(app, idx, ev);
    }

    // navigation
    let nav_mode = app.prefs.nav;
    let pressed_btn = [egui::PointerButton::Primary, egui::PointerButton::Middle, egui::PointerButton::Secondary]
        .into_iter()
        .find(|b| pointer.button_pressed(*b));
    if let Some(b) = pressed_btn {
        if inside && resp.hovered() {
            let nav_start = match nav_mode {
                NavMode::Maya => mods.alt,
                NavMode::Simple => b != egui::PointerButton::Primary || mods.alt,
            };
            if nav_start && !(nav_mode == NavMode::Simple && b == egui::PointerButton::Middle && !app.channel_sel.is_empty() && !mods.alt) {
                app.panes[idx].nav = Some((b, pos));
            }
        }
    }
    if let Some((b, last)) = app.panes[idx].nav {
        if !pointer.button_down(b) {
            app.panes[idx].nav = None;
        } else {
            let d = pos - last;
            let d = glam::Vec2::new(d.x, d.y);
            let scene = app.scene.clone_light();
            let simple = nav_mode == NavMode::Simple && !mods.alt;
            let cam = &mut app.panes[idx].cam;
            match (b, simple) {
                (egui::PointerButton::Primary, false) | (egui::PointerButton::Secondary, true) => {
                    if !cam.is_ortho(&scene) {
                        cam.tumble(d);
                    } else {
                        cam.track(d, size, &scene);
                    }
                }
                (egui::PointerButton::Middle, _) => cam.track(d, size, &scene),
                (egui::PointerButton::Secondary, false) => cam.dolly((d.x - d.y) * 0.004, &scene),
                _ => {}
            }
            app.panes[idx].nav = Some((b, pos));
            sync_scene_camera(app, idx, ev);
            return;
        }
    }
    if app.panes[idx].nav.is_some() {
        return;
    }

    // virtual slider (MMB with channel selection)
    if pointer.button_pressed(egui::PointerButton::Middle) && inside && !app.channel_sel.is_empty() {
        let mut start = Vec::new();
        for id in app.scene.selected() {
            for a in &app.channel_sel {
                if let Some(v) = app.scene.get_attr(id, a) {
                    start.push((id, a.clone(), v));
                }
            }
        }
        if !start.is_empty() {
            app.checkpoint("virtual slider");
            app.panes[idx].vslider = Some((pos.x, start));
        }
    }
    if let Some((x0, start)) = app.panes[idx].vslider.clone() {
        if pointer.button_down(egui::PointerButton::Middle) {
            let speed = if mods.ctrl { 0.01 } else if mods.shift { 1.0 } else { 0.1 };
            let d = (pos.x - x0) as f64 * speed;
            for (id, a, v) in &start {
                let _ = app.scene.set_attr(*id, a, v + d);
            }
        } else {
            app.panes[idx].vslider = None;
            let cmds: Vec<String> = start.iter().map(|(id, a, _)| format!("setAttr {}.{} {};", app.scene.name_of(*id), a, rg_script::value::fmt_float(app.scene.get_attr(*id, a).unwrap_or(0.0)))).collect();
            for (id, a, _) in &start {
                app.auto_key_attrs(*id, &[a.as_str()]);
            }
            app.log(LogKind::Command, cmds.join(" "));
        }
        return;
    }

    // manipulator drag in progress
    if let Some(drag) = app.manip.clone() {
        if drag.pane == idx {
            if pointer.button_down(egui::PointerButton::Primary) {
                drag_manip(app, drag, pos, pr, mods);
            } else {
                end_manip(app);
            }
            return;
        }
    }

    // manipulator hover / start
    let hover = if inside && app.panes[idx].show.manips && app.tool != Tool::Select { manip_hit(app, pr, pos, ev) } else { None };
    app.panes[idx].vslider = None;
    if let Some(h) = hover {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        if pointer.button_pressed(egui::PointerButton::Primary) && !mods.alt && resp.hovered() {
            start_manip(app, idx, h, pos, pr, ev);
            return;
        }
    }
    store_hover(ui, idx, hover);

    // selection: click & marquee
    let primary_pressed = pointer.button_pressed(egui::PointerButton::Primary);
    if primary_pressed && inside && resp.hovered() && !mods.alt {
        app.panes[idx].marquee = Some((pos, pos));
    }
    if let Some((a, _)) = app.panes[idx].marquee {
        if pointer.button_down(egui::PointerButton::Primary) {
            app.panes[idx].marquee = Some((a, pos));
        } else {
            app.panes[idx].marquee = None;
            let r = Rect::from_two_pos(a, pos);
            let mode = if mods.shift && mods.ctrl {
                SelectMode::Add
            } else if mods.shift {
                SelectMode::Toggle
            } else if mods.ctrl {
                SelectMode::Remove
            } else {
                SelectMode::Replace
            };
            if r.width() < 4.0 && r.height() < 4.0 {
                click_select(app, idx, pr, pos, mode, ev);
            } else {
                marquee_select(app, idx, pr, r, mode, ev);
            }
        }
    }
}

thread_local! {
    static HOVER: std::cell::RefCell<HashMap<usize, Option<Handle>>> = std::cell::RefCell::new(HashMap::new());
}

fn store_hover(_ui: &egui::Ui, idx: usize, h: Option<Handle>) {
    HOVER.with(|m| m.borrow_mut().insert(idx, h));
}

fn get_hover(idx: usize) -> Option<Handle> {
    HOVER.with(|m| m.borrow().get(&idx).copied().flatten())
}

/// When looking through a scene camera, navigation moves the camera node.
fn sync_scene_camera(app: &mut App, idx: usize, ev: &Arc<Evaluated>) {
    let cam = app.panes[idx].cam.clone();
    if let CamKind::Scene(id) = cam.kind {
        if app.scene.nodes.contains_key(&id) {
            let _ = ev;
            // derive camera node transform from orbit parameters
            let w = rg_core::look_at(cam.eye(), cam.target, Vec3::Y).inverse();
            let (_, r, t) = w.to_scale_rotation_translation();
            let pw = app.scene.parent_world(id);
            let local = pw.inverse() * Mat4::from_rotation_translation(r, t);
            if let Some(n) = app.scene.node_mut(id) {
                let jo = n.joint_orient();
                n.transform.set_from_matrix(local, jo);
            }
        }
    }
}

// ------------------------------------------------------------- picking --

fn object_ray_hit(app: &App, id: NodeId, ev: &Evaluated, ro: Vec3, rd: Vec3) -> Option<(f32, u32)> {
    let mesh = ev.mesh(id)?;
    let w = ev.world(id);
    let inv = w.inverse();
    let o = inv.transform_point3(ro);
    let d = inv.transform_vector3(rd);
    let _ = app;
    let hit = mesh.raycast(o, d.normalize())?;
    let wp = w.transform_point3(hit.point);
    Some((wp.distance(ro), hit.face))
}

fn pick_object(app: &App, idx: usize, pr: &Projector, pos: Pos2, ev: &Evaluated) -> Option<NodeId> {
    // non-mesh shapes: closest drawn segment
    let mut best: Option<(f32, NodeId)> = None;
    for (id, a, b) in &app.panes[idx].picks {
        let d = seg_dist(pos, *a, *b);
        if d < 7.0 && best.is_none_or(|x| d < x.0) {
            best = Some((d, *id));
        }
    }
    if let Some((_, id)) = best {
        return Some(id);
    }
    let (ro, rd) = pr.ray(pos);
    let mut hit: Option<(f32, NodeId)> = None;
    if app.panes[idx].show.polygons {
        for (&id, n) in &app.scene.nodes {
            if n.mesh().is_none() || !visible(app, &app.panes[idx], id) || !app.scene.is_selectable(id) {
                continue;
            }
            if let Some((t, _)) = object_ray_hit(app, id, ev, ro, rd) {
                if hit.is_none_or(|h| t < h.0) {
                    hit = Some((t, id));
                }
            }
        }
    }
    hit.map(|h| h.1)
}

fn click_select(app: &mut App, idx: usize, pr: &Projector, pos: Pos2, mode: SelectMode, ev: &Arc<Evaluated>) {
    let comp_mode = app.scene.components.mode;
    if comp_mode != ComponentMode::Object {
        if let Some(node) = app.scene.components.node {
            if let Some(i) = pick_component(app, node, comp_mode, pr, pos, ev) {
                let suffix = match comp_mode {
                    ComponentMode::Vertex => "vtx",
                    ComponentMode::Edge => "e",
                    _ => "f",
                };
                let flag = mode_flag(mode);
                let _ = app.mel(&format!("select {flag}{}.{suffix}[{i}];", app.scene.name_of(node)));
                return;
            }
            if mode == SelectMode::Replace {
                // clicked empty space: keep component mode, clear components
                app.checkpoint("select");
                app.scene.components.clear();
                app.scene.touch();
                app.log(LogKind::Command, "select -cl;");
            }
            return;
        }
    }
    match pick_object(app, idx, pr, pos, ev) {
        Some(id) => {
            // Maya selects the top transform? It selects the picked object.
            let flag = mode_flag(mode);
            let name = app.scene.name_of(id);
            let _ = app.mel(&format!("select {flag}{name};"));
        }
        None => {
            if mode == SelectMode::Replace && !app.scene.selection.is_empty() {
                let _ = app.mel("select -cl;");
            }
        }
    }
}

fn mode_flag(mode: SelectMode) -> &'static str {
    match mode {
        SelectMode::Replace => "-r ",
        SelectMode::Add => "-add ",
        SelectMode::Toggle => "-tgl ",
        SelectMode::Remove => "-d ",
    }
}

fn pick_component(app: &App, node: NodeId, mode: ComponentMode, pr: &Projector, pos: Pos2, ev: &Evaluated) -> Option<u32> {
    let mesh = ev.mesh(node)?;
    let w = ev.world(node);
    match mode {
        ComponentMode::Vertex => {
            let mut best: Option<(f32, u32)> = None;
            let (ro, rd) = pr.ray(pos);
            let front = object_ray_hit(app, node, ev, ro, rd).map(|h| h.0);
            for (i, p) in mesh.positions.iter().enumerate() {
                let wp = w.transform_point3(*p);
                if let Some(s) = pr.project(wp) {
                    let d = s.distance(pos);
                    // prefer visible vertices: skip ones far behind the hit surface
                    if let Some(t) = front {
                        if wp.distance(ro) > t * 1.02 + 0.05 && d > 3.0 {
                            continue;
                        }
                    }
                    if d < 12.0 && best.is_none_or(|b| d < b.0) {
                        best = Some((d, i as u32));
                    }
                }
            }
            best.map(|b| b.1)
        }
        ComponentMode::Edge => {
            let mut best: Option<(f32, u32)> = None;
            for (i, e) in mesh.edges().iter().enumerate() {
                let (Some(a), Some(b)) = (pr.project(w.transform_point3(mesh.positions[e.0 as usize])), pr.project(w.transform_point3(mesh.positions[e.1 as usize]))) else { continue };
                let d = seg_dist(pos, a, b);
                if d < 8.0 && best.is_none_or(|x| d < x.0) {
                    best = Some((d, i as u32));
                }
            }
            best.map(|b| b.1)
        }
        ComponentMode::Face => {
            let (ro, rd) = pr.ray(pos);
            object_ray_hit(app, node, ev, ro, rd).map(|h| h.1)
        }
        ComponentMode::Object => None,
    }
}

fn marquee_select(app: &mut App, idx: usize, pr: &Projector, r: Rect, mode: SelectMode, ev: &Arc<Evaluated>) {
    let comp_mode = app.scene.components.mode;
    if comp_mode != ComponentMode::Object {
        if let Some(node) = app.scene.components.node {
            let Some(mesh) = ev.mesh(node).cloned() else { return };
            let w = ev.world(node);
            let inside = |p: Vec3| pr.project(w.transform_point3(p)).is_some_and(|s| r.contains(s));
            let picked: Vec<u32> = match comp_mode {
                ComponentMode::Vertex => (0..mesh.positions.len() as u32).filter(|&i| inside(mesh.positions[i as usize])).collect(),
                ComponentMode::Edge => mesh
                    .edges()
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| inside(mesh.positions[e.0 as usize]) && inside(mesh.positions[e.1 as usize]))
                    .map(|(i, _)| i as u32)
                    .collect(),
                _ => (0..mesh.faces.len()).filter(|&f| inside(mesh.face_center(f))).map(|f| f as u32).collect(),
            };
            app.checkpoint("select");
            let edges = mesh.edges();
            let c = &mut app.scene.components;
            if mode == SelectMode::Replace {
                c.clear();
            }
            for i in picked {
                match comp_mode {
                    ComponentMode::Vertex => toggle(&mut c.verts, i, mode),
                    ComponentMode::Edge => toggle(&mut c.edges, edges[i as usize], mode),
                    _ => toggle(&mut c.faces, i, mode),
                }
            }
            app.scene.touch();
            let strs = rg_script::commands::component_strings(&app.scene);
            app.log(LogKind::Command, format!("select {}{};", mode_flag(mode), compress_components(&strs)));
            return;
        }
    }
    let mut ids: Vec<NodeId> = Vec::new();
    for (id, a, b) in &app.panes[idx].picks {
        if (r.contains(*a) || r.contains(*b)) && !ids.contains(id) {
            ids.push(*id);
        }
    }
    if app.panes[idx].show.polygons {
        for (&id, n) in &app.scene.nodes {
            if n.mesh().is_none() || !visible(app, &app.panes[idx], id) || !app.scene.is_selectable(id) {
                continue;
            }
            let Some(m) = ev.mesh(id) else { continue };
            let w = ev.world(id);
            let step = (m.positions.len() / 4000).max(1);
            if m.positions.iter().step_by(step).any(|p| pr.project(w.transform_point3(*p)).is_some_and(|s| r.contains(s))) {
                ids.push(id);
            }
        }
    }
    if ids.is_empty() && mode == SelectMode::Replace {
        let _ = app.mel("select -cl;");
        return;
    }
    if ids.is_empty() {
        return;
    }
    let names = app.names(&ids);
    let _ = app.mel(&format!("select {}{};", mode_flag(mode), names));
}

fn toggle<T: Ord + Copy>(set: &mut std::collections::BTreeSet<T>, v: T, mode: SelectMode) {
    match mode {
        SelectMode::Remove => {
            set.remove(&v);
        }
        SelectMode::Toggle => {
            if !set.remove(&v) {
                set.insert(v);
            }
        }
        _ => {
            set.insert(v);
        }
    }
}

/// "a.vtx[1] a.vtx[2] a.vtx[3]" -> "a.vtx[1:3]"
pub fn compress_components(strs: &[String]) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < strs.len() {
        let s = &strs[i];
        let (prefix, idx) = match s.rfind('[') {
            Some(p) => (&s[..p], s[p + 1..s.len() - 1].parse::<u32>().ok()),
            None => (s.as_str(), None),
        };
        let Some(start) = idx else {
            out.push(s.clone());
            i += 1;
            continue;
        };
        let mut end = start;
        let mut j = i + 1;
        while j < strs.len() {
            let t = &strs[j];
            if t.starts_with(prefix) && t[prefix.len()..].starts_with('[') {
                if let Ok(n) = t[prefix.len() + 1..t.len() - 1].parse::<u32>() {
                    if n == end + 1 {
                        end = n;
                        j += 1;
                        continue;
                    }
                }
            }
            break;
        }
        out.push(if end > start { format!("{prefix}[{start}:{end}]") } else { s.clone() });
        i = j;
    }
    out.join(" ")
}

// -------------------------------------------------------- manipulators --

/// Center and axes of the manipulator for the current selection.
pub fn manip_frame(app: &App, ev: &Evaluated, tool: Tool) -> Option<(Vec3, [Vec3; 3])> {
    let c = &app.scene.components;
    if c.mode != ComponentMode::Object && !c.is_empty() {
        let node = c.node?;
        let mesh = ev.mesh(node)?;
        let w = ev.world(node);
        let verts = c.affected_verts(mesh);
        if verts.is_empty() {
            return None;
        }
        let center = verts.iter().map(|&v| w.transform_point3(mesh.positions[v as usize])).sum::<Vec3>() / verts.len() as f32;
        let axes = if tool == Tool::Move && app.prefs.move_world {
            [Vec3::X, Vec3::Y, Vec3::Z]
        } else {
            let (_, r, _) = w.to_scale_rotation_translation();
            [r * Vec3::X, r * Vec3::Y, r * Vec3::Z]
        };
        return Some((center, axes));
    }
    let lead = *app.scene.selection.last()?;
    app.scene.nodes.get(&lead)?;
    let w = ev.world(lead);
    let pivot = ev.node(&app.scene, lead).map(|n| n.transform.pivot).unwrap_or(Vec3::ZERO);
    let center = w.transform_point3(pivot);
    let world_axes = match tool {
        Tool::Move => app.prefs.move_world,
        Tool::Rotate => app.prefs.rotate_world,
        _ => false,
    };
    let axes = if world_axes {
        [Vec3::X, Vec3::Y, Vec3::Z]
    } else {
        let (_, r, _) = w.to_scale_rotation_translation();
        [r * Vec3::X, r * Vec3::Y, r * Vec3::Z]
    };
    Some((center, axes))
}

fn manip_len(app: &App, pr: &Projector, center: Vec3) -> f32 {
    pr.pixel_size(center) * 95.0 * app.prefs.manip_size
}

fn ring_points(center: Vec3, axis: Vec3, radius: f32) -> Vec<Vec3> {
    let (u, v) = {
        let a = axis.normalize();
        let t = a.any_orthonormal_vector();
        (t, a.cross(t))
    };
    (0..=48).map(|i| {
        let th = i as f32 / 48.0 * std::f32::consts::TAU;
        center + (u * th.cos() + v * th.sin()) * radius
    }).collect()
}

fn manip_hit(app: &App, pr: &Projector, pos: Pos2, ev: &Evaluated) -> Option<Handle> {
    let (c, axes) = manip_frame(app, ev, app.tool)?;
    let len = manip_len(app, pr, c);
    let cs = pr.project(c)?;
    match app.tool {
        Tool::Move | Tool::Scale => {
            if cs.distance(pos) < 9.0 {
                return Some(if app.tool == Tool::Move { Handle::Screen } else { Handle::Uniform });
            }
            if app.tool == Tool::Move {
                for i in 0..3 {
                    let (a, b) = (axes[(i + 1) % 3], axes[(i + 2) % 3]);
                    let q = [c + (a + b) * len * 0.2, c + (a + b) * len * 0.35];
                    if let (Some(p0), Some(p1)) = (pr.project(q[0]), pr.project(q[1])) {
                        if Rect::from_two_pos(p0, p1).expand(2.0).contains(pos) {
                            return Some(Handle::Plane(i));
                        }
                    }
                }
            }
            let mut best: Option<(f32, usize)> = None;
            for (i, ax) in axes.iter().enumerate() {
                if let Some(e) = pr.project(c + *ax * len) {
                    let d = seg_dist(pos, cs, e);
                    if d < 8.0 && best.is_none_or(|b| d < b.0) {
                        best = Some((d, i));
                    }
                }
            }
            best.map(|b| Handle::Axis(b.1))
        }
        Tool::Rotate => {
            let mut best: Option<(f32, Handle)> = None;
            for (i, ax) in axes.iter().enumerate() {
                let pts = ring_points(c, *ax, len);
                let to_eye = (pr.eye - c).normalize();
                for w in pts.windows(2) {
                    // only the front half of each ring is grabbable
                    if (w[0] - c).normalize().dot(to_eye) < -0.15 && !pr.ortho {
                        continue;
                    }
                    if let (Some(a), Some(b)) = (pr.project(w[0]), pr.project(w[1])) {
                        let d = seg_dist(pos, a, b);
                        if d < 7.0 && best.is_none_or(|x| d < x.0) {
                            best = Some((d, Handle::Ring(i)));
                        }
                    }
                }
            }
            if best.is_none() {
                let r = cs.distance(pos);
                let screen_r = len / pr.pixel_size(c) * 1.2;
                if (r - screen_r).abs() < 7.0 {
                    return Some(Handle::ScreenRing);
                }
            }
            best.map(|b| b.1)
        }
        Tool::Select => None,
    }
}

fn top_level_selection(app: &App) -> Vec<NodeId> {
    let sel = app.scene.selected();
    sel.iter().copied().filter(|id| !sel.iter().any(|o| o != id && app.scene.is_ancestor(*o, *id))).collect()
}

fn start_manip(app: &mut App, idx: usize, h: Handle, pos: Pos2, pr: &Projector, ev: &Arc<Evaluated>) {
    let Some((center, axes)) = manip_frame(app, ev, app.tool) else { return };
    let (ro, rd) = pr.ray(pos);
    let t0 = match h {
        Handle::Axis(i) => closest_on_line(center, axes[i], ro, rd).unwrap_or(0.0),
        _ => 0.0,
    };
    let plane_n = match h {
        Handle::Plane(i) => axes[i],
        _ => -pr.cam_forward,
    };
    let hit0 = ray_plane(ro, rd, center, plane_n).unwrap_or(center);
    let comp = {
        let c = &app.scene.components;
        if c.mode != ComponentMode::Object && !c.is_empty() {
            c.node.and_then(|n| {
                let m = app.scene.nodes.get(&n)?.mesh()?.mesh.clone();
                let verts = c.affected_verts(&m);
                Some((n, verts.into_iter().map(|v| (v, m.positions[v as usize])).collect(), ev.world(n)))
            })
        } else {
            None
        }
    };
    let objects = if comp.is_none() {
        top_level_selection(app).into_iter().filter_map(|id| app.scene.nodes.get(&id).map(|n| (id, n.transform.clone(), app.scene.parent_world(id)))).collect()
    } else {
        Vec::new()
    };
    app.checkpoint(match app.tool {
        Tool::Move => "move",
        Tool::Rotate => "rotate",
        _ => "scale",
    });
    let cs = pr.project(center).unwrap_or(pos);
    app.manip = Some(ManipDrag {
        handle: h,
        tool: app.tool,
        pane: idx,
        start_mouse: pos,
        center,
        axes,
        t0,
        hit0,
        objects,
        comp,
        last: Vec3::ZERO,
        angle: 0.0,
        prev_screen_angle: (pos.y - cs.y).atan2(pos.x - cs.x),
        moved: false,
        pivot: app.pivot_mode,
    });
}

fn drag_manip(app: &mut App, mut d: ManipDrag, pos: Pos2, pr: &Projector, mods: egui::Modifiers) {
    let (ro, rd) = pr.ray(pos);
    let snap = app.prefs.snap_grid || app.held_x;
    let step = app.prefs.snap_step.max(1e-4);
    if pos.distance(d.start_mouse) > 1.0 {
        d.moved = true;
    }
    match d.tool {
        Tool::Move => {
            let mut delta = match d.handle {
                Handle::Axis(i) => {
                    let t = closest_on_line(d.center, d.axes[i], ro, rd).unwrap_or(d.t0);
                    d.axes[i] * (t - d.t0)
                }
                Handle::Plane(i) => ray_plane(ro, rd, d.center, d.axes[i]).map(|h| h - d.hit0).unwrap_or(Vec3::ZERO),
                _ => ray_plane(ro, rd, d.center, -pr.cam_forward).map(|h| h - d.hit0).unwrap_or(Vec3::ZERO),
            };
            if snap {
                let target = d.center + delta;
                let snapped = (target / step).round() * step;
                delta = match d.handle {
                    Handle::Axis(i) => d.axes[i] * (snapped - d.center).dot(d.axes[i]),
                    _ => snapped - d.center,
                };
            }
            d.last = delta;
            apply_move(app, &d, delta);
        }
        Tool::Rotate => {
            let cs = pr.project(d.center).unwrap_or(pos);
            let a = (pos.y - cs.y).atan2(pos.x - cs.x);
            let mut da = a - d.prev_screen_angle;
            while da > std::f32::consts::PI {
                da -= std::f32::consts::TAU;
            }
            while da < -std::f32::consts::PI {
                da += std::f32::consts::TAU;
            }
            d.prev_screen_angle = a;
            let axis = match d.handle {
                Handle::Ring(i) => d.axes[i],
                _ => -pr.cam_forward,
            };
            // screen y is down: flip so counter-clockwise on screen is positive about an axis facing the viewer
            let facing = axis.dot(pr.eye - d.center) >= 0.0 || pr.ortho && axis.dot(-pr.cam_forward) >= 0.0;
            d.angle += if facing { -da } else { da };
            let mut ang = d.angle;
            if snap || app.held_j {
                let s = app.prefs.rotate_step.to_radians();
                ang = (ang / s).round() * s;
            }
            d.last = axis * ang.to_degrees();
            apply_rotate(app, &d, axis, ang);
        }
        Tool::Scale => {
            let s = match d.handle {
                Handle::Axis(i) => {
                    let t = closest_on_line(d.center, d.axes[i], ro, rd).unwrap_or(d.t0);
                    let f = if d.t0.abs() > 1e-6 { t / d.t0 } else { 1.0 };
                    let mut v = Vec3::ONE;
                    v[i] = f;
                    v
                }
                _ => Vec3::splat((1.0 + (pos.x - d.start_mouse.x) / 100.0).max(0.01)),
            };
            let s = if mods.ctrl { Vec3::ONE + (s - Vec3::ONE) * 0.1 } else { s };
            d.last = s;
            apply_scale(app, &d, s);
        }
        Tool::Select => {}
    }
    app.manip = Some(d);
}

fn apply_move(app: &mut App, d: &ManipDrag, delta: Vec3) {
    if let Some((node, verts, w)) = &d.comp {
        let inv = w.inverse();
        let dl = inv.transform_vector3(delta);
        let verts = verts.clone();
        app.scene.edit_mesh(*node, |m| {
            for (i, p) in &verts {
                m.positions[*i as usize] = *p + dl;
            }
        });
        return;
    }
    for (id, start, pw) in &d.objects {
        let local = pw.inverse().transform_vector3(delta);
        if let Some(n) = app.scene.node_mut(*id) {
            if d.pivot {
                // move the pivot, keep the object in place
                let rs = n.transform.matrix(n.joint_orient()) * Mat4::from_translation(-n.transform.translate);
                let _ = rs;
                let mut t = start.clone();
                let world_to_obj = Mat4::from_quat(t.rotation()) * Mat4::from_scale(t.scale);
                let dp = world_to_obj.inverse().transform_vector3(local);
                let before = t.matrix(n.joint_orient());
                t.pivot = start.pivot + dp;
                let after = t.matrix(n.joint_orient());
                t.translate += before.transform_point3(Vec3::ZERO) - after.transform_point3(Vec3::ZERO);
                n.transform = t;
            } else {
                n.transform.translate = start.translate + local;
            }
        }
    }
}

fn apply_rotate(app: &mut App, d: &ManipDrag, axis: Vec3, ang: f32) {
    let q = Quat::from_axis_angle(axis.normalize(), ang);
    if let Some((node, verts, w)) = &d.comp {
        let inv = w.inverse();
        let c = d.center;
        let w = *w;
        let verts = verts.clone();
        app.scene.edit_mesh(*node, |m| {
            for (i, p) in &verts {
                let wp = w.transform_point3(*p);
                m.positions[*i as usize] = inv.transform_point3(c + q * (wp - c));
            }
        });
        return;
    }
    for (id, start, pw) in &d.objects {
        let Some(n) = app.scene.node_mut(*id) else { continue };
        let jo = n.joint_orient().map(|o| RotateOrder::XYZ.to_quat(o)).unwrap_or(Quat::IDENTITY);
        let parent_rot = pw.to_scale_rotation_translation().1;
        let start_local = jo * start.rotation();
        let start_world = parent_rot * start_local;
        let new_world = q * start_world;
        let new_local = jo.inverse() * parent_rot.inverse() * new_world;
        n.transform.rotate = start.rotate_order.from_quat(new_local, Some(start.rotate));
    }
}

fn apply_scale(app: &mut App, d: &ManipDrag, s: Vec3) {
    if let Some((node, verts, w)) = &d.comp {
        let inv = w.inverse();
        let c = d.center;
        let w = *w;
        let axes = d.axes;
        let verts = verts.clone();
        app.scene.edit_mesh(*node, |m| {
            for (i, p) in &verts {
                let wp = w.transform_point3(*p) - c;
                let local = Vec3::new(wp.dot(axes[0]), wp.dot(axes[1]), wp.dot(axes[2])) * s;
                let np = c + axes[0] * local.x + axes[1] * local.y + axes[2] * local.z;
                m.positions[*i as usize] = inv.transform_point3(np);
            }
        });
        return;
    }
    for (id, start, _) in &d.objects {
        if let Some(n) = app.scene.node_mut(*id) {
            n.transform.scale = start.scale * s;
        }
    }
}

fn end_manip(app: &mut App) {
    let Some(d) = app.manip.take() else { return };
    if !d.moved {
        return;
    }
    let f = |v: f32| rg_script::value::fmt_float(v as f64);
    let target = if let Some((node, verts, _)) = &d.comp {
        let strs: Vec<String> = verts.iter().map(|(i, _)| format!("{}.vtx[{i}]", app.scene.name_of(*node))).collect();
        compress_components(&strs)
    } else {
        app.names(&d.objects.iter().map(|o| o.0).collect::<Vec<_>>())
    };
    let cmd = match d.tool {
        Tool::Move => format!("move -r {} {} {} {target};", f(d.last.x), f(d.last.y), f(d.last.z)),
        Tool::Rotate => format!("rotate -r -ws {} {} {} {target};", f(d.last.x), f(d.last.y), f(d.last.z)),
        _ => format!("scale -r {} {} {} {target};", f(d.last.x), f(d.last.y), f(d.last.z)),
    };
    app.log(LogKind::Command, cmd);
    if app.auto_key {
        let attrs: &[&str] = match d.tool {
            Tool::Move => &["translateX", "translateY", "translateZ"],
            Tool::Rotate => &["rotateX", "rotateY", "rotateZ"],
            _ => &["scaleX", "scaleY", "scaleZ"],
        };
        for (id, _, _) in &d.objects {
            app.auto_key_attrs(*id, attrs);
        }
    }
}

// ------------------------------------------------------------- drawing --

fn build_draw_list(app: &mut App, idx: usize, pr: &Projector, view: Mat4, proj: Mat4, ev: &Arc<Evaluated>) -> DrawList {
    let pane = app.panes[idx].clone();
    let mut dl = DrawList {
        view,
        proj,
        eye: pr.eye,
        xray: pane.xray,
        backface_cull: pane.backface,
        use_lights: pane.use_lights,
        ..Default::default()
    };
    // grid
    if pane.show.grid {
        let n = (app.prefs.grid_size / app.prefs.grid_spacing).round().max(1.0) as i32;
        let s = app.prefs.grid_spacing;
        let ext = n as f32 * s;
        let (u, v) = match pane.cam.kind {
            CamKind::Front => (Vec3::X, Vec3::Y),
            CamKind::Side => (Vec3::Z, Vec3::Y),
            _ => (Vec3::X, Vec3::Z),
        };
        let grid_c = [0.42, 0.42, 0.42, 1.0];
        let axis_c = [0.1, 0.1, 0.1, 1.0];
        for i in -n..=n {
            let c = if i == 0 { axis_c } else { grid_c };
            let o = i as f32 * s;
            gl::push_line(&mut dl.lines, u * o - v * ext, u * o + v * ext, c);
            gl::push_line(&mut dl.lines, v * o - u * ext, v * o + u * ext, c);
        }
    }
    // lights for "use all lights"
    if pane.use_lights {
        for (&id, n) in &app.scene.nodes {
            let NodeKind::Light(l) = &n.kind else { continue };
            if !app.scene.is_visible(id) {
                continue;
            }
            let w = ev.world(id);
            let p = w.transform_point3(Vec3::ZERO);
            let dir = w.transform_vector3(Vec3::NEG_Z).normalize_or_zero();
            let col = l.color * l.intensity;
            let (pos, cone) = match l.kind {
                LightType::Directional => ([dir.x, dir.y, dir.z, 0.0], [-2.0, -2.0]),
                LightType::Spot => {
                    let half = (l.cone_angle * 0.5).to_radians();
                    let pen = l.penumbra.to_radians();
                    ([p.x, p.y, p.z, 1.0], [(half + pen.max(0.0)).cos(), (half + pen.min(0.0)).cos()])
                }
                LightType::Ambient => continue,
                _ => ([p.x, p.y, p.z, 1.0], [-2.0, -2.0]),
            };
            dl.lights.push(GlLight { pos, dir: [dir.x, dir.y, dir.z], color: [col.x, col.y, col.z], cone });
        }
    }

    let comp_node = if app.scene.components.mode != ComponentMode::Object { app.scene.components.node } else { None };
    let ids: Vec<NodeId> = app.scene.nodes.keys().copied().collect();
    let mut uploads: Vec<(MeshKey, Arc<Mesh>)> = Vec::new();
    for id in ids {
        if !visible(app, &pane, id) {
            continue;
        }
        let Some(n) = ev.node(&app.scene, id) else { continue };
        let NodeKind::Mesh(ms) = &n.kind else { continue };
        if !pane.show.polygons {
            continue;
        }
        let smooth_level = ms.smooth_level;
        let template = n.template;
        let Some(m) = ev.mesh(id).cloned() else { continue };
        let display = smooth_mesh(app, &m, smooth_level);
        let key = MeshKey { ptr: Arc::as_ptr(&display) as usize, smooth: smooth_level };
        uploads.push((key, display.clone()));
        let mat = app.scene.material_for(id).clone();
        let model = ev.world(id);
        let selected = app.scene.selection.contains(&id);
        let in_comp = comp_node == Some(id);
        let shaded = pane.shade != ShadeMode::Wireframe;
        let wire_default = if shaded { [0.0, 0.0, 0.0, 0.6] } else { theme::WIRE_DEFAULT };
        let mut wire = None;
        if !shaded || pane.wire_on_shaded || selected || in_comp {
            wire = Some(node_wire_color(app, id, wire_default));
        }
        if in_comp {
            wire = Some([0.15, 0.25, 0.85, 1.0]);
        } else if shaded && selected && !pane.wire_on_shaded {
            let c = node_wire_color(app, id, wire_default);
            wire = Some([c[0], c[1], c[2], 0.9]);
        }
        let tex = if pane.shade == ShadeMode::Textured {
            match &mat.texture {
                Texture::Checker { color2, repeats } => TexMode::Checker { color2: [color2.x, color2.y, color2.z], repeats: *repeats },
                Texture::Ramp { color2 } => TexMode::Ramp { color2: [color2.x, color2.y, color2.z] },
                Texture::File { .. } => TexMode::File,
                _ => TexMode::None,
            }
        } else {
            TexMode::None
        };
        let tex_path = match &mat.texture {
            Texture::File { path } => Some(path.clone()),
            _ => None,
        };
        dl.items.push(DrawItem {
            key,
            model,
            color: [mat.color.x * mat.diffuse.max(0.2), mat.color.y * mat.diffuse.max(0.2), mat.color.z * mat.diffuse.max(0.2), if template { 0.3 } else { mat.opacity.max(0.15) }],
            shaded: shaded && !template,
            wire: if template { Some([0.5, 0.5, 0.5, 1.0]) } else { wire },
            spec: if mat.shader == ShaderType::Lambert { 0.0 } else { mat.specular },
            rough: mat.roughness,
            emission: [mat.emission_color.x * mat.emission, mat.emission_color.y * mat.emission, mat.emission_color.z * mat.emission],
            tex,
            tex_path,
        });
        // smooth preview: also show the cage
        if smooth_level > 0 && (selected || in_comp) {
            let ck = MeshKey { ptr: Arc::as_ptr(&m) as usize, smooth: 0 };
            uploads.push((ck, m.clone()));
            dl.items.push(DrawItem {
                key: ck,
                model,
                color: [0.0; 4],
                shaded: false,
                wire: Some([0.6, 0.6, 0.9, 0.8]),
                spec: 0.0,
                rough: 1.0,
                emission: [0.0; 3],
                tex: TexMode::None,
                tex_path: None,
            });
        }
        // component highlights
        if in_comp {
            let c = &app.scene.components;
            match c.mode {
                ComponentMode::Face => {
                    for &f in &c.faces {
                        let Some(face) = m.faces.get(f as usize) else { continue };
                        let pts: Vec<Vec3> = face.verts.iter().map(|&v| model.transform_point3(m.positions[v as usize])).collect();
                        for t in rg_core::mesh::triangulate_polygon(&pts, rg_core::mesh::newell_normal(pts.iter().copied())) {
                            gl::push_tri(&mut dl.tris_overlay, pts[t[0] as usize], pts[t[1] as usize], pts[t[2] as usize], [1.0, 0.45, 0.1, 0.45]);
                        }
                    }
                }
                ComponentMode::Edge => {
                    for e in &c.edges {
                        if (e.1 as usize) < m.positions.len() {
                            let a = model.transform_point3(m.positions[e.0 as usize]);
                            let b = model.transform_point3(m.positions[e.1 as usize]);
                            gl::push_line(&mut dl.lines_top, a, b, [1.0, 0.55, 0.1, 1.0]);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    // queue uploads for meshes not on the GPU yet
    if let Some(glr) = &app.gl {
        let mut r = glr.lock().unwrap();
        for (key, mesh) in uploads {
            if !r.has(&key) {
                r.queue_upload(key, CpuMesh::build(&mesh), mesh);
            }
        }
    }

    // non-mesh shapes as lines (+ pick segments)
    let mut picks: Vec<(NodeId, Pos2, Pos2)> = Vec::new();
    let mut add = |dl: &mut DrawList, picks: &mut Vec<(NodeId, Pos2, Pos2)>, id: NodeId, a: Vec3, b: Vec3, c: [f32; 4], top: bool| {
        if top {
            gl::push_line(&mut dl.lines_top, a, b, c);
        } else {
            gl::push_line(&mut dl.lines, a, b, c);
        }
        if let (Some(pa), Some(pb)) = (pr.project(a), pr.project(b)) {
            picks.push((id, pa, pb));
        }
    };
    let ids: Vec<NodeId> = app.scene.walk();
    for id in ids {
        if !visible(app, &pane, id) || !app.scene.is_selectable(id) && !app.scene.nodes[&id].template {
            continue;
        }
        let Some(n) = ev.node(&app.scene, id) else { continue };
        let w = ev.world(id);
        let px = pr.pixel_size(w.transform_point3(Vec3::ZERO));
        match &n.kind {
            NodeKind::Joint(j) if pane.show.joints => {
                let col = rgba(node_wire_color(app, id, [0.0, 0.2, 0.9, 1.0]));
                let p = w.transform_point3(Vec3::ZERO);
                let r = j.radius * 0.5;
                for ax in [Vec3::X, Vec3::Y, Vec3::Z] {
                    let pts = ring_points(p, ax, r);
                    for s in pts.windows(2) {
                        add(&mut dl, &mut picks, id, s[0], s[1], col, pane.xray_joints);
                    }
                }
                // bones to child joints
                for &c in &n.children {
                    if !app.scene.nodes.get(&c).is_some_and(|x| x.is_joint()) {
                        continue;
                    }
                    let q = ev.world(c).transform_point3(Vec3::ZERO);
                    let dir = q - p;
                    let len = dir.length();
                    if len < 1e-5 {
                        continue;
                    }
                    let d = dir / len;
                    let (u, v) = (d.any_orthonormal_vector(), d.cross(d.any_orthonormal_vector()));
                    let base = p + d * r;
                    let wdt = r.min(len * 0.15);
                    let ring = [base + u * wdt, base + v * wdt, base - u * wdt, base - v * wdt];
                    for k in 0..4 {
                        add(&mut dl, &mut picks, id, ring[k], ring[(k + 1) % 4], col, pane.xray_joints);
                        add(&mut dl, &mut picks, id, ring[k], q, col, pane.xray_joints);
                    }
                }
            }
            NodeKind::Locator if pane.show.locators => {
                let col = node_wire_color(app, id, [0.1, 0.1, 0.6, 1.0]);
                let p = w.transform_point3(Vec3::ZERO);
                for ax in [Vec3::X, Vec3::Y, Vec3::Z] {
                    let a = w.transform_vector3(ax);
                    add(&mut dl, &mut picks, id, p - a, p + a, col, false);
                }
            }
            NodeKind::Curve(c) if pane.show.curves => {
                let col = node_wire_color(app, id, [0.05, 0.1, 0.55, 1.0]);
                let pts: Vec<Vec3> = c.points(10).into_iter().map(|q| w.transform_point3(q)).collect();
                for s in pts.windows(2) {
                    add(&mut dl, &mut picks, id, s[0], s[1], col, false);
                }
            }
            NodeKind::Camera(_) if pane.show.cameras => {
                if pane.cam.kind == CamKind::Scene(id) {
                    continue;
                }
                let col = node_wire_color(app, id, [0.0, 0.0, 0.0, 1.0]);
                let s = 0.5;
                let corners = [Vec3::new(-s, -s * 0.7, 0.4), Vec3::new(s, -s * 0.7, 0.4), Vec3::new(s, s * 0.7, 0.4), Vec3::new(-s, s * 0.7, 0.4)];
                let back = corners.map(|c| w.transform_point3(c + Vec3::Z * 0.8));
                let front = corners.map(|c| w.transform_point3(c));
                for k in 0..4 {
                    add(&mut dl, &mut picks, id, front[k], front[(k + 1) % 4], col, false);
                    add(&mut dl, &mut picks, id, back[k], back[(k + 1) % 4], col, false);
                    add(&mut dl, &mut picks, id, front[k], back[k], col, false);
                }
                // lens
                let tip = w.transform_point3(Vec3::new(0.0, 0.0, -0.4));
                for k in 0..4 {
                    add(&mut dl, &mut picks, id, front[k], tip, col, false);
                }
            }
            NodeKind::Light(l) if pane.show.lights => {
                let col = node_wire_color(app, id, [0.95, 0.85, 0.3, 1.0]);
                let p = w.transform_point3(Vec3::ZERO);
                let dir = w.transform_vector3(Vec3::NEG_Z).normalize_or_zero();
                let sz = px * 30.0;
                match l.kind {
                    LightType::Directional => {
                        for off in [Vec3::ZERO, w.transform_vector3(Vec3::X).normalize() * sz * 0.4, -w.transform_vector3(Vec3::X).normalize() * sz * 0.4] {
                            add(&mut dl, &mut picks, id, p + off, p + off + dir * sz * 2.0, col, false);
                        }
                    }
                    LightType::Spot => {
                        let len = sz * 3.0;
                        let half = (l.cone_angle * 0.5).to_radians();
                        let rad = len * half.tan();
                        let ring = ring_points(p + dir * len, dir, rad);
                        for s in ring.windows(2) {
                            add(&mut dl, &mut picks, id, s[0], s[1], col, false);
                        }
                        for k in [0, 12, 24, 36] {
                            add(&mut dl, &mut picks, id, p, ring[k], col, false);
                        }
                    }
                    LightType::Area => {
                        let c = [Vec3::new(-1., -1., 0.), Vec3::new(1., -1., 0.), Vec3::new(1., 1., 0.), Vec3::new(-1., 1., 0.)].map(|q| w.transform_point3(q));
                        for k in 0..4 {
                            add(&mut dl, &mut picks, id, c[k], c[(k + 1) % 4], col, false);
                        }
                        add(&mut dl, &mut picks, id, c[0], c[2], col, false);
                        add(&mut dl, &mut picks, id, p, p + dir * sz * 1.5, col, false);
                    }
                    _ => {
                        for ax in [Vec3::X, Vec3::Y, Vec3::Z, Vec3::new(1., 1., 0.).normalize(), Vec3::new(1., -1., 0.).normalize()] {
                            add(&mut dl, &mut picks, id, p - ax * sz * 0.5, p + ax * sz * 0.5, col, false);
                        }
                    }
                }
            }
            NodeKind::IkHandle(ik) if pane.show.ik => {
                let col = node_wire_color(app, id, [0.85, 0.75, 0.55, 1.0]);
                let p = w.transform_point3(Vec3::ZERO);
                if app.scene.nodes.contains_key(&ik.start_joint) {
                    let s = ev.world(ik.start_joint).transform_point3(Vec3::ZERO);
                    add(&mut dl, &mut picks, id, s, p, col, true);
                    // pole vector
                    let pole = match ik.pole_target {
                        Some(t) if app.scene.nodes.contains_key(&t) => ev.world(t).transform_point3(Vec3::ZERO),
                        _ => s + ik.pole_vector,
                    };
                    gl::push_line(&mut dl.lines_top, s, pole, [0.6, 0.6, 0.6, 0.6]);
                }
                let sz = px * 8.0;
                add(&mut dl, &mut picks, id, p - Vec3::X * sz, p + Vec3::X * sz, col, true);
                add(&mut dl, &mut picks, id, p - Vec3::Y * sz, p + Vec3::Y * sz, col, true);
                add(&mut dl, &mut picks, id, p - Vec3::Z * sz, p + Vec3::Z * sz, col, true);
            }
            _ => {}
        }
    }
    app.panes[idx].picks = picks;
    dl
}

fn draw_overlays(app: &mut App, painter: &egui::Painter, idx: usize, pr: &Projector, ev: &Arc<Evaluated>) {
    let pane = app.panes[idx].clone();
    // component points
    if let (ComponentMode::Vertex | ComponentMode::Face, Some(node)) = (app.scene.components.mode, app.scene.components.node) {
        if let Some(m) = ev.mesh(node) {
            let w = ev.world(node);
            let c = &app.scene.components;
            if c.mode == ComponentMode::Vertex {
                for (i, p) in m.positions.iter().enumerate() {
                    if let Some(s) = pr.project(w.transform_point3(*p)) {
                        if pr.rect.contains(s) {
                            let sel = c.verts.contains(&(i as u32));
                            painter.circle_filled(s, if sel { 3.0 } else { 2.2 }, if sel { Color32::from_rgb(255, 230, 0) } else { Color32::from_rgb(170, 0, 170) });
                        }
                    }
                }
            } else {
                for f in 0..m.faces.len() {
                    if let Some(s) = pr.project(w.transform_point3(m.face_center(f))) {
                        if pr.rect.contains(s) {
                            let sel = c.faces.contains(&(f as u32));
                            painter.rect_filled(Rect::from_center_size(s, egui::vec2(4.0, 4.0)), 0.0, if sel { Color32::from_rgb(255, 140, 20) } else { Color32::from_rgb(40, 60, 180) });
                        }
                    }
                }
            }
        }
    }

    // manipulator
    if pane.show.manips && app.tool != Tool::Select {
        if let Some((c, axes)) = manip_frame(app, ev, app.tool) {
            draw_manip(app, painter, idx, pr, c, axes);
        }
    } else if pane.show.manips {
        // select tool: small pivot marker
        if let Some((c, _)) = manip_frame(app, ev, Tool::Move) {
            if let Some(s) = pr.project(c) {
                painter.circle_stroke(s, 4.0, Stroke::new(1.0, Color32::from_gray(220)));
            }
        }
    }

    // marquee
    if let Some((a, b)) = pane.marquee {
        let r = Rect::from_two_pos(a, b);
        if r.width() > 2.0 || r.height() > 2.0 {
            painter.rect_stroke(r, 0.0, Stroke::new(1.0, Color32::WHITE), egui::StrokeKind::Inside);
            painter.rect_filled(r, 0.0, Color32::from_white_alpha(12));
        }
    }

    // HUD
    if pane.show.hud && app.prefs.show_hud {
        let font = egui::FontId::proportional(12.0);
        let r = pr.rect;
        painter.text(egui::pos2(r.center().x, r.bottom() - 12.0), egui::Align2::CENTER_CENTER, pane.cam.kind.label(&app.scene), font.clone(), Color32::from_gray(225));
        // poly count
        let (mut v, mut e, mut f, mut t) = (0usize, 0usize, 0usize, 0usize);
        let (mut sv, mut sf) = (0usize, 0usize);
        for (&id, n) in &app.scene.nodes {
            if let NodeKind::Mesh(ms) = &n.kind {
                if !app.scene.is_visible(id) {
                    continue;
                }
                let m = &ms.mesh;
                v += m.positions.len();
                f += m.faces.len();
                e += m.faces.iter().map(|f| f.len()).sum::<usize>() / 2;
                t += m.faces.iter().map(|f| f.len().saturating_sub(2)).sum::<usize>();
                if app.scene.selection.contains(&id) {
                    sv += m.positions.len();
                    sf += m.faces.len();
                }
            }
        }
        let lines = [format!("Verts: {v:>8}  {sv:>6}"), format!("Edges: {e:>8}"), format!("Faces: {f:>8}  {sf:>6}"), format!("Tris:  {t:>8}")];
        for (i, l) in lines.iter().enumerate() {
            painter.text(egui::pos2(r.left() + 8.0, r.top() + 8.0 + i as f32 * 14.0), egui::Align2::LEFT_TOP, l, egui::FontId::monospace(11.0), Color32::from_gray(225));
        }
        let frame_txt = format!("Frame {}", rg_script::value::fmt_float(app.scene.time.current));
        painter.text(egui::pos2(r.right() - 8.0, r.top() + 8.0), egui::Align2::RIGHT_TOP, frame_txt, font.clone(), Color32::from_gray(225));
        if app.pivot_mode {
            painter.text(egui::pos2(r.right() - 8.0, r.top() + 24.0), egui::Align2::RIGHT_TOP, "PIVOT EDIT (D)", font.clone(), Color32::YELLOW);
        }
        if app.scene.components.mode != ComponentMode::Object {
            let m = match app.scene.components.mode {
                ComponentMode::Vertex => "Vertex mode",
                ComponentMode::Edge => "Edge mode",
                _ => "Face mode",
            };
            painter.text(egui::pos2(r.right() - 8.0, r.top() + 40.0), egui::Align2::RIGHT_TOP, m, font.clone(), Color32::from_rgb(255, 200, 120));
        }
        if pane.isolate.is_some() {
            painter.text(egui::pos2(r.center().x, r.top() + 10.0), egui::Align2::CENTER_TOP, "Isolate Select", font, Color32::from_rgb(255, 200, 120));
        }
    }
    // axis triad
    let o = egui::pos2(pr.rect.left() + 30.0, pr.rect.bottom() - 30.0);
    let rot = pr.view_proj;
    let _ = rot;
    let inv = pr.inv_view_proj;
    let _ = inv;
    for (i, (ax, label)) in [(Vec3::X, "x"), (Vec3::Y, "y"), (Vec3::Z, "z")].into_iter().enumerate() {
        let a = pr.project(pr.eye + pr.cam_forward * 10.0);
        let b = pr.project(pr.eye + pr.cam_forward * 10.0 + ax * 1.0);
        if let (Some(a), Some(b)) = (a, b) {
            let d = (b - a).normalized() * 20.0;
            let d = if d.x.is_nan() { egui::Vec2::ZERO } else { d };
            painter.line_segment([o, o + d], Stroke::new(2.0, c32(AXIS_COL[i])));
            painter.text(o + d * 1.25, egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(11.0), c32(AXIS_COL[i]));
        }
    }
}

fn draw_manip(app: &App, painter: &egui::Painter, idx: usize, pr: &Projector, c: Vec3, axes: [Vec3; 3]) {
    let Some(cs) = pr.project(c) else { return };
    let len = manip_len(app, pr, c);
    let active = app.manip.as_ref().filter(|m| m.pane == idx).map(|m| m.handle);
    let hover = active.or(get_hover(idx));
    let col = |h: Handle, base: [f32; 4]| if hover == Some(h) { c32(HOVER_COL) } else { c32(base) };
    match app.tool {
        Tool::Move | Tool::Scale => {
            for (i, ax) in axes.iter().enumerate() {
                let Some(e) = pr.project(c + *ax * len) else { continue };
                let cc = col(Handle::Axis(i), AXIS_COL[i]);
                painter.line_segment([cs, e], Stroke::new(2.0, cc));
                if app.tool == Tool::Move {
                    let dir = (e - cs).normalized();
                    if dir.x.is_finite() {
                        let perp = egui::vec2(-dir.y, dir.x);
                        painter.add(egui::Shape::convex_polygon(vec![e + dir * 12.0, e + perp * 5.0, e - perp * 5.0], cc, Stroke::NONE));
                    }
                } else {
                    painter.rect_filled(Rect::from_center_size(e, egui::vec2(9.0, 9.0)), 0.0, cc);
                }
            }
            if app.tool == Tool::Move {
                for i in 0..3 {
                    let (a, b) = (axes[(i + 1) % 3], axes[(i + 2) % 3]);
                    let q = [c + (a) * len * 0.2 + b * len * 0.2, c + a * len * 0.35 + b * len * 0.2, c + (a + b) * len * 0.35, c + a * len * 0.2 + b * len * 0.35];
                    let pts: Vec<Pos2> = q.iter().filter_map(|p| pr.project(*p)).collect();
                    if pts.len() == 4 {
                        let base = AXIS_COL[i];
                        let fill = if hover == Some(Handle::Plane(i)) { Color32::from_rgba_unmultiplied(255, 255, 0, 120) } else { Color32::from_rgba_unmultiplied((base[0] * 255.0) as u8, (base[1] * 255.0) as u8, (base[2] * 255.0) as u8, 70) };
                        painter.add(egui::Shape::convex_polygon(pts, fill, Stroke::NONE));
                    }
                }
                let cc = col(Handle::Screen, [0.95, 0.95, 0.3, 1.0]);
                painter.rect_stroke(Rect::from_center_size(cs, egui::vec2(10.0, 10.0)), 0.0, Stroke::new(1.5, cc), egui::StrokeKind::Middle);
            } else {
                painter.rect_filled(Rect::from_center_size(cs, egui::vec2(10.0, 10.0)), 0.0, col(Handle::Uniform, [0.9, 0.9, 0.2, 1.0]));
            }
        }
        Tool::Rotate => {
            let to_eye = (pr.eye - c).normalize();
            for (i, ax) in axes.iter().enumerate() {
                let pts = ring_points(c, *ax, len);
                let cc = col(Handle::Ring(i), AXIS_COL[i]);
                for w in pts.windows(2) {
                    let front = pr.ortho || (w[0] - c).normalize().dot(to_eye) > -0.15;
                    if let (Some(a), Some(b)) = (pr.project(w[0]), pr.project(w[1])) {
                        let s = if front { Stroke::new(2.0, cc) } else { Stroke::new(1.0, cc.gamma_multiply(0.25)) };
                        painter.line_segment([a, b], s);
                    }
                }
            }
            let screen_r = len / pr.pixel_size(c) * 1.2;
            painter.circle_stroke(cs, screen_r, Stroke::new(1.5, col(Handle::ScreenRing, [0.2, 0.9, 0.9, 1.0])));
            painter.circle_stroke(cs, len / pr.pixel_size(c), Stroke::new(1.0, Color32::from_gray(150)));
            if let Some(d) = app.manip.as_ref().filter(|m| m.pane == idx) {
                let deg = d.angle.to_degrees();
                painter.text(cs + egui::vec2(0.0, -screen_r - 12.0), egui::Align2::CENTER_CENTER, format!("{deg:.1}°"), egui::FontId::proportional(12.0), Color32::YELLOW);
            }
        }
        Tool::Select => {}
    }
}

// -------------------------------------------------------- marking menu --

fn marking_menu(app: &mut App, ui: &mut egui::Ui) {
    let has_mesh = app.scene.selected().iter().any(|id| app.scene.nodes.get(id).is_some_and(|n| n.mesh().is_some())) || app.scene.components.node.is_some();
    if has_mesh {
        ui.label(egui::RichText::new("Component Mode").weak());
        for (label, m) in [("Vertex  (F9)", ComponentMode::Vertex), ("Edge  (F10)", ComponentMode::Edge), ("Face  (F11)", ComponentMode::Face), ("Object Mode  (F8)", ComponentMode::Object)] {
            if ui.selectable_label(app.scene.components.mode == m, label).clicked() {
                app.checkpoint("component mode");
                app.set_component_mode(m);
                ui.close();
            }
        }
        ui.separator();
    }
    if app.scene.components.mode != ComponentMode::Object {
        let cmds: &[(&str, &str)] = match app.scene.components.mode {
            ComponentMode::Face => &[("Extrude Face", "polyExtrudeFacet -ltz 0.5;"), ("Inset", "polyInset -off 0.2;"), ("Poke Face", "polyPoke;"), ("Extract", "polyChipOff -dup 0;"), ("Duplicate Face", "polyChipOff -dup 1;"), ("Delete Face", "delete;"), ("Reverse Normal", "polyNormal -nm 0;")],
            ComponentMode::Edge => &[("Insert Edge Loop", "polySplitRing -wt 0.5;"), ("Select Edge Loop", "polySelectEdgeLoop;"), ("Select Edge Ring", "polySelectEdgeRing;"), ("Bridge", "polyBridgeEdge;"), ("Extrude Edge", "polyExtrudeEdge;"), ("Delete Edge", "polyDelEdge;"), ("Fill Hole", "polyCloseBorder;")],
            _ => &[("Merge (0.01)", "polyMergeVertex -d 0.01;"), ("Merge To Center", "polyMergeToCenter;"), ("Average Vertices", "polyAverageVertex -i 3;"), ("Delete Vertex", "delete;"), ("Fill Hole", "polyCloseBorder;")],
        };
        for (label, cmd) in cmds {
            if ui.button(*label).clicked() {
                let _ = app.mel(cmd);
                ui.close();
            }
        }
        ui.separator();
    }
    for (label, cmd) in [("Select All", "select -all;"), ("Frame Selection", "viewFit;"), ("Duplicate", "duplicate;"), ("Delete", "delete;"), ("Group", "group;"), ("Center Pivot", "centerPivot;"), ("Freeze Transformations", "makeIdentity -apply true;")] {
        if ui.button(label).clicked() {
            let _ = app.mel(cmd);
            ui.close();
        }
    }
    if ui.button("Assign New Material…").clicked() {
        app.win.hypershade = true;
        ui.close();
    }
}

trait CloneLight {
    fn clone_light(&self) -> Scene;
}

impl CloneLight for Scene {
    /// Cheap scene stand-in for camera queries (only camera nodes matter).
    fn clone_light(&self) -> Scene {
        let mut s = Scene::new();
        for (id, n) in &self.nodes {
            if matches!(n.kind, NodeKind::Camera(_)) {
                s.nodes.insert(*id, n.clone());
            }
        }
        s
    }
}

pub fn edges_of(m: &Mesh) -> Vec<Edge> {
    m.edges()
}
