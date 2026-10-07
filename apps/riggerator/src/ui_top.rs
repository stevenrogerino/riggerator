//! Main menu bar (Maya menu sets), status line and shelves.

use crate::app::{App, Layout, MenuSet, PendingAction, RightTab, Tool};
use crate::windows::{FileAction, FileDialog};
use egui::{Color32, RichText};
use rg_core::ComponentMode;

fn item(app: &mut App, ui: &mut egui::Ui, label: &str, cmd: &str) {
    if ui.button(label).on_hover_text(cmd).clicked() {
        let _ = app.mel(cmd);
        ui.close();
    }
}

fn act(ui: &mut egui::Ui, label: &str) -> bool {
    let r = ui.button(label).clicked();
    if r {
        ui.close();
    }
    r
}

pub fn menu_bar(app: &mut App, ui: &mut egui::Ui) {
    egui::MenuBar::new().ui(ui, |ui| {
        ui.menu_button("File", |ui| file_menu(app, ui));
        ui.menu_button("Edit", |ui| edit_menu(app, ui));
        ui.menu_button("Create", |ui| create_menu(app, ui));
        ui.menu_button("Select", |ui| select_menu(app, ui));
        ui.menu_button("Modify", |ui| modify_menu(app, ui));
        ui.menu_button("Display", |ui| display_menu(app, ui));
        ui.menu_button("Windows", |ui| windows_menu(app, ui));
        ui.separator();
        match app.menu_set {
            MenuSet::Modeling => {
                ui.menu_button("Mesh", |ui| mesh_menu(app, ui));
                ui.menu_button("Edit Mesh", |ui| edit_mesh_menu(app, ui));
                ui.menu_button("Mesh Display", |ui| mesh_display_menu(app, ui));
                ui.menu_button("UV", |ui| uv_menu(app, ui));
            }
            MenuSet::Rigging => {
                ui.menu_button("Skeleton", |ui| skeleton_menu(app, ui));
                ui.menu_button("Skin", |ui| skin_menu(app, ui));
                ui.menu_button("Deform", |ui| deform_menu(app, ui));
                ui.menu_button("Constrain", |ui| constrain_menu(app, ui));
                ui.menu_button("Control", |ui| control_menu(app, ui));
            }
            MenuSet::Animation => {
                ui.menu_button("Key", |ui| key_menu(app, ui));
                ui.menu_button("Playback", |ui| playback_menu(app, ui));
                ui.menu_button("Deform", |ui| deform_menu(app, ui));
                ui.menu_button("Constrain", |ui| constrain_menu(app, ui));
            }
            MenuSet::Rendering => {
                ui.menu_button("Lighting/Shading", |ui| shading_menu(app, ui));
                ui.menu_button("Texturing", |ui| texturing_menu(app, ui));
                ui.menu_button("Render", |ui| render_menu(app, ui));
            }
        }
        ui.menu_button("Help", |ui| {
            if act(ui, "Hotkeys…  (F1)") {
                app.win.hotkeys = true;
            }
            if act(ui, "MEL Command Reference") {
                app.win.script_editor = true;
                let _ = app.mel("help;");
            }
            if act(ui, "About Riggerator") {
                app.win.about = true;
            }
        });
    });
}

fn file_menu(app: &mut App, ui: &mut egui::Ui) {
    if act(ui, "New Scene  (Ctrl+N)") {
        app.confirm_new = Some(PendingAction::New);
    }
    if act(ui, "Open Scene…  (Ctrl+O)") {
        app.file_dialog = Some(FileDialog::open("Open Scene", &["rgs", "ma", "obj"], FileAction::OpenScene));
    }
    if act(ui, "Save Scene  (Ctrl+S)") {
        app.save();
    }
    if act(ui, "Save Scene As…  (Ctrl+Shift+S)") {
        app.file_dialog = Some(FileDialog::save("Save Scene As", &["rgs"], FileAction::SaveScene));
    }
    ui.separator();
    if act(ui, "Import…") {
        app.file_dialog = Some(FileDialog::open("Import", &["rgs", "ma", "obj"], FileAction::Import));
    }
    if act(ui, "Export All…") {
        app.file_dialog = Some(FileDialog::save("Export All", &["ma", "obj", "gltf", "rgs"], FileAction::ExportAll));
    }
    if act(ui, "Export Selection…") {
        app.file_dialog = Some(FileDialog::save("Export Selection", &["ma", "obj", "gltf", "rgs"], FileAction::ExportSelection));
    }
    ui.separator();
    if act(ui, "Source Script…") {
        app.file_dialog = Some(FileDialog::open("Source Script", &["mel", "txt"], FileAction::SourceScript));
    }
    ui.separator();
    if act(ui, "Exit") {
        app.confirm_new = Some(PendingAction::Quit);
    }
}

fn edit_menu(app: &mut App, ui: &mut egui::Ui) {
    let undo_label = format!("Undo {}  (Z)", app.history.undo_labels().first().copied().unwrap_or(""));
    if ui.add_enabled(app.history.can_undo(), egui::Button::new(undo_label)).clicked() {
        app.undo();
        ui.close();
    }
    if ui.add_enabled(app.history.can_redo(), egui::Button::new("Redo  (Shift+Z)")).clicked() {
        app.redo();
        ui.close();
    }
    if let Some(c) = app.last_command.clone() {
        let short: String = c.chars().take(30).collect();
        if act(ui, &format!("Repeat \"{short}\"  (G)")) {
            let _ = app.mel(&c);
        }
    }
    ui.separator();
    item(app, ui, "Delete  (Del)", "delete;");
    item(app, ui, "Delete All by Type: History", "delete -ch;");
    ui.separator();
    item(app, ui, "Duplicate  (Ctrl+D)", "duplicate;");
    item(app, ui, "Group  (Ctrl+G)", "group;");
    item(app, ui, "Ungroup → Parent to World", "parent -w;");
    item(app, ui, "Parent  (P)", "parent;");
    item(app, ui, "Unparent  (Shift+P)", "parent -w;");
}

fn create_menu(app: &mut App, ui: &mut egui::Ui) {
    ui.menu_button("Polygon Primitives", |ui| {
        item(app, ui, "Sphere", "polySphere;");
        item(app, ui, "Cube", "polyCube;");
        item(app, ui, "Cylinder", "polyCylinder;");
        item(app, ui, "Cone", "polyCone;");
        item(app, ui, "Torus", "polyTorus;");
        item(app, ui, "Plane", "polyPlane;");
        item(app, ui, "Pipe", "polyPipe;");
        item(app, ui, "Platonic Solid", "polyPlatonicSolid;");
        item(app, ui, "Octahedron", "polyPlatonicSolid -st 1;");
        item(app, ui, "Tetrahedron", "polyPlatonicSolid -st 0;");
    });
    ui.menu_button("Curve Tools", |ui| {
        item(app, ui, "NURBS Circle", "circle -nr 0 1 0;");
        item(app, ui, "Square", "curve -d 1 -p -1 0 -1 -p 1 0 -1 -p 1 0 1 -p -1 0 1 -p -1 0 -1 -n square;");
        item(app, ui, "Arrow", "curve -d 1 -p 0 0 -2 -p 1 0 -1 -p 0.4 0 -1 -p 0.4 0 1 -p -0.4 0 1 -p -0.4 0 -1 -p -1 0 -1 -p 0 0 -2 -n arrow;");
    });
    ui.menu_button("Lights", |ui| {
        item(app, ui, "Ambient Light", "ambientLight -i 0.3;");
        item(app, ui, "Directional Light", "directionalLight;");
        item(app, ui, "Point Light", "pointLight; move 0 4 0;");
        item(app, ui, "Spot Light", "spotLight; move 0 6 0; rotate -90 0 0;");
        item(app, ui, "Area Light", "areaLight -i 2; move 0 5 0; rotate -90 0 0;");
    });
    ui.menu_button("Cameras", |ui| {
        item(app, ui, "Camera", "camera;");
    });
    item(app, ui, "Locator", "spaceLocator;");
    item(app, ui, "Empty Group", "group -em;");
}

fn select_menu(app: &mut App, ui: &mut egui::Ui) {
    item(app, ui, "All", "select -all;");
    item(app, ui, "Deselect All", "select -cl;");
    item(app, ui, "Hierarchy", &format!("select -hi {};", app.selection_names()));
    if act(ui, "Inverse") {
        let sel = app.scene.selected();
        let inv: Vec<_> = app.scene.roots.iter().copied().filter(|r| !sel.contains(r)).collect();
        let names = app.names(&inv);
        let _ = app.mel(&format!("select -r {names};"));
    }
    ui.separator();
    for (label, m) in [("Object Mode  (F8)", ComponentMode::Object), ("Vertex  (F9)", ComponentMode::Vertex), ("Edge  (F10)", ComponentMode::Edge), ("Face  (F11)", ComponentMode::Face)] {
        if act(ui, label) {
            app.checkpoint("component mode");
            app.set_component_mode(m);
        }
    }
    ui.separator();
    item(app, ui, "Select Edge Loop", "polySelectEdgeLoop;");
    item(app, ui, "Select Edge Ring", "polySelectEdgeRing;");
    ui.separator();
    item(app, ui, "All Joints", "select -r `ls -type joint`;");
    item(app, ui, "All Meshes", "select -r `ls -type mesh`;");
    item(app, ui, "All Curves", "select -r `ls -type nurbsCurve`;");
    item(app, ui, "All Lights", "select -r `ls -lights`;");
}

fn modify_menu(app: &mut App, ui: &mut egui::Ui) {
    item(app, ui, "Freeze Transformations", "makeIdentity -apply true -t 1 -r 1 -s 1;");
    item(app, ui, "Reset Transformations", "makeIdentity;");
    item(app, ui, "Center Pivot", "centerPivot;");
    ui.separator();
    if act(ui, "Add Attribute…") {
        app.right_tab = RightTab::AttributeEditor;
        app.show_right = true;
    }
    item(app, ui, "Lock Transform Channels", &format!("setAttr -l true {0}.t; setAttr -l true {0}.r; setAttr -l true {0}.s;", app.scene.selected().last().map(|i| app.scene.name_of(*i)).unwrap_or_default()));
    item(app, ui, "Unlock Transform Channels", &format!("setAttr -l false {0}.t; setAttr -l false {0}.r; setAttr -l false {0}.s;", app.scene.selected().last().map(|i| app.scene.name_of(*i)).unwrap_or_default()));
    ui.separator();
    if act(ui, "Edit Pivot (toggle, D)") {
        app.pivot_mode = !app.pivot_mode;
    }
}

fn display_menu(app: &mut App, ui: &mut egui::Ui) {
    item(app, ui, "Hide Selection  (Ctrl+H)", "hide;");
    item(app, ui, "Show All", "showHidden -all;");
    ui.separator();
    if act(ui, "Template Selected") {
        app.checkpoint("template");
        for id in app.scene.selected() {
            if let Some(n) = app.scene.node_mut(id) {
                n.template = true;
            }
        }
        app.scene.selection.clear();
    }
    if act(ui, "Untemplate All") {
        app.checkpoint("untemplate");
        let ids: Vec<_> = app.scene.nodes.keys().copied().collect();
        for id in ids {
            app.scene.node_mut(id).unwrap().template = false;
        }
    }
    ui.separator();
    ui.checkbox(&mut app.prefs.show_hud, "Heads Up Display");
    ui.add(egui::Slider::new(&mut app.prefs.grid_size, 1.0..=100.0).text("Grid size"));
    ui.add(egui::Slider::new(&mut app.prefs.grid_spacing, 0.1..=10.0).text("Grid spacing"));
    ui.add(egui::Slider::new(&mut app.prefs.manip_size, 0.3..=3.0).text("Manipulator size"));
}

fn windows_menu(app: &mut App, ui: &mut egui::Ui) {
    ui.checkbox(&mut app.show_outliner, "Outliner");
    if act(ui, "Attribute Editor  (Ctrl+A)") {
        app.show_right = true;
        app.right_tab = RightTab::AttributeEditor;
    }
    if act(ui, "Channel Box / Layer Editor") {
        app.show_right = true;
        app.right_tab = RightTab::ChannelBox;
    }
    ui.separator();
    ui.checkbox(&mut app.win.graph_editor, "Graph Editor");
    ui.checkbox(&mut app.win.dope_sheet, "Dope Sheet");
    ui.checkbox(&mut app.win.hypershade, "Hypershade");
    ui.checkbox(&mut app.win.node_editor, "Node Editor (relationships)");
    ui.checkbox(&mut app.win.component_editor, "Component Editor");
    ui.checkbox(&mut app.win.sdk, "Set Driven Key");
    ui.separator();
    ui.checkbox(&mut app.win.render_view, "Render View");
    ui.checkbox(&mut app.win.render_settings, "Render Settings");
    ui.separator();
    ui.checkbox(&mut app.win.script_editor, "Script Editor");
    ui.checkbox(&mut app.win.prefs, "Preferences");
    ui.separator();
    ui.label(RichText::new("Layouts").weak());
    if act(ui, "Single Perspective View") {
        app.layout = Layout::Single;
        app.active_pane = 0;
    }
    if act(ui, "Four View") {
        app.layout = Layout::Quad;
    }
    if act(ui, "Two Panes Side by Side") {
        app.layout = Layout::TwoSide;
    }
}

fn mesh_menu(app: &mut App, ui: &mut egui::Ui) {
    item(app, ui, "Combine", "polyUnite;");
    item(app, ui, "Separate", "polySeparate;");
    item(app, ui, "Extract", "polyChipOff -dup 0;");
    ui.separator();
    item(app, ui, "Smooth", "polySmooth -dv 1;");
    item(app, ui, "Average Vertices", "polyAverageVertex -i 3;");
    item(app, ui, "Fill Hole", "polyCloseBorder;");
    item(app, ui, "Mirror (X)", "polyMirrorFace -d 0;");
    item(app, ui, "Mirror (Y)", "polyMirrorFace -d 1;");
    item(app, ui, "Mirror (Z)", "polyMirrorFace -d 2;");
    ui.separator();
    item(app, ui, "Triangulate", "polyTriangulate;");
    item(app, ui, "Quadrangulate", "polyQuad -a 30;");
    item(app, ui, "Merge All Vertices (0.001)", "polyMergeVertex -d 0.001;");
}

fn edit_mesh_menu(app: &mut App, ui: &mut egui::Ui) {
    ui.label(RichText::new("Components").weak());
    item(app, ui, "Extrude", "polyExtrudeFacet -ltz 0.5;");
    item(app, ui, "Inset", "polyInset -off 0.2;");
    item(app, ui, "Bridge", "polyBridgeEdge;");
    item(app, ui, "Insert Edge Loop", "polySplitRing -wt 0.5;");
    item(app, ui, "Poke", "polyPoke;");
    ui.separator();
    item(app, ui, "Merge", "polyMergeVertex -d 0.01;");
    item(app, ui, "Merge To Center", "polyMergeToCenter;");
    item(app, ui, "Delete Edge/Vertex", "polyDelEdge;");
    ui.separator();
    item(app, ui, "Extract Faces", "polyChipOff -dup 0;");
    item(app, ui, "Duplicate Faces", "polyChipOff -dup 1;");
}

fn mesh_display_menu(app: &mut App, ui: &mut egui::Ui) {
    item(app, ui, "Reverse Normals", "polyNormal -nm 0;");
    item(app, ui, "Soften Edge", "polySoftEdge -a 180;");
    item(app, ui, "Harden Edge", "polySoftEdge -a 0;");
    item(app, ui, "Set Normal Angle 30°", "polySoftEdge -a 30;");
    ui.separator();
    ui.label(RichText::new("Smooth Mesh Preview").weak());
    for (label, lvl) in [("Off  (1)", 0), ("Level 1", 1), ("Level 2  (3)", 2), ("Level 3", 3)] {
        if act(ui, label) {
            set_smooth_preview(app, lvl);
        }
    }
}

pub fn set_smooth_preview(app: &mut App, lvl: u32) {
    let names = app.selection_names();
    if names.is_empty() {
        return;
    }
    let cmds: Vec<String> = app
        .scene
        .selected()
        .iter()
        .filter(|id| app.scene.nodes.get(id).is_some_and(|n| n.mesh().is_some()))
        .map(|id| format!("setAttr {}.smoothLevel {lvl};", app.scene.name_of(*id)))
        .collect();
    if !cmds.is_empty() {
        let _ = app.mel(&cmds.join(" "));
    }
}

fn uv_menu(app: &mut App, ui: &mut egui::Ui) {
    item(app, ui, "Planar (Y)", "polyPlanarProjection -md y;");
    item(app, ui, "Planar (X)", "polyPlanarProjection -md x;");
    item(app, ui, "Planar (Z)", "polyPlanarProjection -md z;");
    item(app, ui, "Cylindrical", "polyCylindricalProjection;");
}

fn skeleton_menu(app: &mut App, ui: &mut egui::Ui) {
    item(app, ui, "Create Joint at Origin", "joint -p 0 0 0;");
    if act(ui, "Create Joint Chain (3)…") {
        let _ = app.mel("select -cl; joint -p 0 0 0; joint -p 0 3 0.5; joint -p 0 6 0;");
    }
    if act(ui, "Create Spine/Arm Template") {
        let _ = app.mel(
            "select -cl; joint -p 0 9 0 -n hips; joint -p 0 11 0 -n spine; joint -p 0 13 0 -n chest; joint -p 0 15 0 -n neck; joint -p 0 16.5 0 -n head;\n\
             select -r chest; joint -p 1.5 14 0 -n l_shoulder; joint -p 4 14 -0.4 -n l_elbow; joint -p 6.5 14 0 -n l_wrist;\n\
             select -r hips; joint -p 1 8 0 -n l_hip; joint -p 1 4.5 0.4 -n l_knee; joint -p 1 0.8 0 -n l_ankle;",
        );
    }
    ui.separator();
    item(app, ui, "Orient Joint", "joint_orient;");
    item(app, ui, "Create IK Handle (RP)", "ikHandle -sol ikRPsolver;");
    item(app, ui, "Create IK Handle (SC)", "ikHandle -sol ikSCsolver;");
}

fn skin_menu(app: &mut App, ui: &mut egui::Ui) {
    item(app, ui, "Bind Skin", "skinCluster -mi 4 -dr 4;");
    item(app, ui, "Unbind Skin", "unbindSkin;");
    ui.separator();
    item(app, ui, "Add Blend Shape (targets…, base)", "blendShape;");
    if act(ui, "Component Editor (weights)") {
        app.win.component_editor = true;
    }
}

fn deform_menu(app: &mut App, ui: &mut egui::Ui) {
    ui.label(RichText::new("Nonlinear").weak());
    for d in ["bend", "twist", "squash", "flare", "sine", "wave"] {
        item(app, ui, &format!("{}{}", d[..1].to_uppercase(), &d[1..]), &format!("nonLinear -type {d};"));
    }
    ui.separator();
    item(app, ui, "Blend Shape", "blendShape;");
}

fn constrain_menu(app: &mut App, ui: &mut egui::Ui) {
    item(app, ui, "Parent", "parentConstraint -mo;");
    item(app, ui, "Point", "pointConstraint -mo;");
    item(app, ui, "Orient", "orientConstraint -mo;");
    item(app, ui, "Scale", "scaleConstraint -mo;");
    item(app, ui, "Aim", "aimConstraint;");
    item(app, ui, "Pole Vector", "poleVectorConstraint;");
    ui.separator();
    if act(ui, "Remove Constraints on Selected") {
        app.checkpoint("remove constraints");
        let sel = app.scene.selected();
        app.scene.constraints.retain(|c| !sel.contains(&c.driven));
        app.scene.touch();
    }
}

fn control_menu(app: &mut App, ui: &mut egui::Ui) {
    item(app, ui, "Circle Control", "circle -nr 0 1 0 -r 1.5 -n ctrl;");
    item(app, ui, "Square Control", "curve -d 1 -p -1 0 -1 -p 1 0 -1 -p 1 0 1 -p -1 0 1 -p -1 0 -1 -n ctrl;");
    if act(ui, "Control at Selected (parent constraint)") {
        let sel = app.scene.selected();
        if let Some(&t) = sel.last() {
            let name = app.scene.name_of(t);
            let _ = app.mel(&format!(
                "string $c[] = `circle -nr 1 0 0 -r 1.5 -n {name}_ctrl`; float $p[] = `xform -q -ws -t {name}`; move $p[0] $p[1] $p[2] $c[0]; select -r $c[0] {name}; parentConstraint -mo; select -r $c[0];"
            ));
        }
    }
    ui.separator();
    if act(ui, "Set Driven Key…") {
        app.win.sdk = true;
    }
    if act(ui, "Color: Red") {
        set_color(app, [1.0, 0.1, 0.1]);
    }
    if act(ui, "Color: Blue") {
        set_color(app, [0.1, 0.3, 1.0]);
    }
    if act(ui, "Color: Yellow") {
        set_color(app, [1.0, 0.9, 0.1]);
    }
    if act(ui, "Color: Default") {
        app.checkpoint("color");
        for id in app.scene.selected() {
            app.scene.node_mut(id).unwrap().color = None;
        }
    }
}

fn set_color(app: &mut App, c: [f32; 3]) {
    app.checkpoint("color");
    for id in app.scene.selected() {
        app.scene.node_mut(id).unwrap().color = Some(glam::Vec3::from(c));
    }
}

fn key_menu(app: &mut App, ui: &mut egui::Ui) {
    item(app, ui, "Set Key  (S)", "setKeyframe;");
    item(app, ui, "Key Translate  (Shift+W)", "setKeyframe -at translate;");
    item(app, ui, "Key Rotate  (Shift+E)", "setKeyframe -at rotate;");
    item(app, ui, "Key Scale  (Shift+R)", "setKeyframe -at scale;");
    ui.separator();
    item(app, ui, &format!("Delete Key at {}", app.scene.time.current), &format!("cutKey -t {};", app.scene.time.current));
    item(app, ui, "Delete All Keys", "cutKey -cl;");
    ui.separator();
    for t in ["auto", "spline", "linear", "flat", "step", "clamped"] {
        item(app, ui, &format!("Tangents: {t}"), &format!("keyTangent -itt {t} -ott {t};"));
    }
    ui.separator();
    item(app, ui, "Bake Simulation (constraints → keys)", "bakeResults;");
    ui.checkbox(&mut app.auto_key, "Auto Key");
}

fn playback_menu(app: &mut App, ui: &mut egui::Ui) {
    if act(ui, if app.playing { "Stop  (Alt+V)" } else { "Play  (Alt+V)" }) {
        app.playing = !app.playing;
    }
    if act(ui, "Go to Start") {
        let t = app.scene.time.min;
        app.set_time(t);
    }
    if act(ui, "Go to End") {
        let t = app.scene.time.max;
        app.set_time(t);
    }
    if act(ui, "Next Key  (.)") {
        app.next_key(true);
    }
    if act(ui, "Previous Key  (,)") {
        app.next_key(false);
    }
    ui.separator();
    ui.checkbox(&mut app.prefs.play_every_frame, "Play Every Frame");
}

fn shading_menu(app: &mut App, ui: &mut egui::Ui) {
    for (l, s) in [("Assign New Lambert", "lambert"), ("Assign New Blinn", "blinn"), ("Assign New Phong", "phong"), ("Assign New Standard Surface", "standardSurface"), ("Assign New Surface Shader", "surfaceShader")] {
        if act(ui, l) {
            let _ = app.mel(&format!("string $m = `shadingNode -asShader {s}`; hyperShade -assign $m;"));
        }
    }
    ui.separator();
    if act(ui, "Hypershade…") {
        app.win.hypershade = true;
    }
}

fn texturing_menu(app: &mut App, ui: &mut egui::Ui) {
    if act(ui, "Checker on Selected Material") {
        app.checkpoint("texture");
        for id in app.scene.selected() {
            let mname = app.scene.material_for(id).name.clone();
            if mname == "lambert1" {
                continue;
            }
            if let Some(m) = app.scene.materials.get_mut(&mname) {
                m.texture = rg_core::Texture::Checker { color2: glam::Vec3::splat(0.1), repeats: 8.0 };
            }
        }
        app.scene.touch();
    }
    if act(ui, "Open Hypershade for textures…") {
        app.win.hypershade = true;
    }
}

fn render_menu(app: &mut App, ui: &mut egui::Ui) {
    if act(ui, "Render Current Frame") {
        app.start_render();
    }
    if act(ui, "Render Settings…") {
        app.win.render_settings = true;
    }
    if act(ui, "Render View…") {
        app.win.render_view = true;
    }
}

// ------------------------------------------------------------ status line

pub fn status_line(app: &mut App, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("menuset").selected_text(app.menu_set.name()).width(100.0).show_ui(ui, |ui| {
            for m in MenuSet::ALL {
                ui.selectable_value(&mut app.menu_set, m, m.name());
            }
        });
        ui.separator();
        if ui.button("🗋").on_hover_text("New scene").clicked() {
            app.confirm_new = Some(PendingAction::New);
        }
        if ui.button("🗁").on_hover_text("Open scene").clicked() {
            app.file_dialog = Some(FileDialog::open("Open Scene", &["rgs", "ma", "obj"], FileAction::OpenScene));
        }
        if ui.button("💾").on_hover_text("Save scene").clicked() {
            app.save();
        }
        ui.separator();
        if ui.add_enabled(app.history.can_undo(), egui::Button::new("⟲")).on_hover_text("Undo (Z)").clicked() {
            app.undo();
        }
        if ui.add_enabled(app.history.can_redo(), egui::Button::new("⟳")).on_hover_text("Redo (Shift+Z)").clicked() {
            app.redo();
        }
        ui.separator();
        let cm = app.scene.components.mode;
        for (label, m, tip) in [("Obj", ComponentMode::Object, "Object mode (F8)"), ("Vtx", ComponentMode::Vertex, "Vertex (F9)"), ("Edge", ComponentMode::Edge, "Edge (F10)"), ("Face", ComponentMode::Face, "Face (F11)")] {
            if ui.selectable_label(cm == m, label).on_hover_text(tip).clicked() {
                app.checkpoint("component mode");
                app.set_component_mode(m);
            }
        }
        ui.separator();
        ui.toggle_value(&mut app.prefs.snap_grid, "Snap").on_hover_text("Snap to grid (hold X)");
        ui.toggle_value(&mut app.pivot_mode, "Pivot").on_hover_text("Edit pivot (D)");
        ui.separator();
        if ui.button("Render").on_hover_text("Render current frame").clicked() {
            app.start_render();
        }
        if ui.button("⚙").on_hover_text("Render settings").clicked() {
            app.win.render_settings = true;
        }
        if ui.button("Hypershade").clicked() {
            app.win.hypershade = true;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.selectable_label(app.show_right && app.right_tab == RightTab::ChannelBox, "Channel Box").clicked() {
                toggle_right(app, RightTab::ChannelBox);
            }
            if ui.selectable_label(app.show_right && app.right_tab == RightTab::AttributeEditor, "Attribute Editor").clicked() {
                toggle_right(app, RightTab::AttributeEditor);
            }
            if ui.selectable_label(app.show_right && app.right_tab == RightTab::ToolSettings, "Tool Settings").clicked() {
                toggle_right(app, RightTab::ToolSettings);
            }
            ui.toggle_value(&mut app.show_outliner, "Outliner");
            if let Some((msg, until)) = &app.status_message {
                if *until > app.last_frame_time {
                    let col = if msg.starts_with("Error") { Color32::from_rgb(255, 120, 110) } else { Color32::from_rgb(240, 220, 140) };
                    ui.label(RichText::new(msg).color(col));
                }
            }
        });
    });
}

fn toggle_right(app: &mut App, tab: RightTab) {
    if app.show_right && app.right_tab == tab {
        app.show_right = false;
    } else {
        app.show_right = true;
        app.right_tab = tab;
    }
}

// ------------------------------------------------------------------ shelf

const SHELVES: &[(&str, &[(&str, &str, &str)])] = &[
    (
        "Poly Modeling",
        &[
            ("Sphere", "polySphere;", "Polygon sphere"),
            ("Cube", "polyCube;", "Polygon cube"),
            ("Cyl", "polyCylinder;", "Polygon cylinder"),
            ("Cone", "polyCone;", "Polygon cone"),
            ("Torus", "polyTorus;", "Polygon torus"),
            ("Plane", "polyPlane;", "Polygon plane"),
            ("|", "", ""),
            ("Comb", "polyUnite;", "Combine"),
            ("Sep", "polySeparate;", "Separate"),
            ("Smooth", "polySmooth -dv 1;", "Smooth"),
            ("Mirror", "polyMirrorFace -d 0;", "Mirror X"),
            ("|", "", ""),
            ("Extrude", "polyExtrudeFacet -ltz 0.5;", "Extrude faces"),
            ("Inset", "polyInset -off 0.2;", "Inset faces"),
            ("Bridge", "polyBridgeEdge;", "Bridge border edges"),
            ("Loop", "polySplitRing -wt 0.5;", "Insert edge loop on selected edge"),
            ("Merge", "polyMergeVertex -d 0.01;", "Merge vertices"),
            ("Fill", "polyCloseBorder;", "Fill hole"),
            ("|", "", ""),
            ("Freeze", "makeIdentity -apply true;", "Freeze transformations"),
            ("CPiv", "centerPivot;", "Center pivot"),
        ],
    ),
    (
        "Rigging",
        &[
            ("Joint", "joint -p 0 0 0;", "Create joint"),
            ("Orient", "joint_orient;", "Orient joints"),
            ("IK RP", "ikHandle -sol ikRPsolver;", "IK handle (rotate plane) – select start & end joints"),
            ("IK SC", "ikHandle -sol ikSCsolver;", "IK handle (single chain)"),
            ("|", "", ""),
            ("Bind", "skinCluster -mi 4;", "Bind skin – select joints then mesh"),
            ("Unbind", "unbindSkin;", "Unbind skin"),
            ("Blend", "blendShape;", "Blend shape – targets then base"),
            ("|", "", ""),
            ("Parent", "parentConstraint -mo;", "Parent constraint"),
            ("Point", "pointConstraint -mo;", "Point constraint"),
            ("Orient", "orientConstraint -mo;", "Orient constraint"),
            ("Aim", "aimConstraint;", "Aim constraint"),
            ("PoleV", "poleVectorConstraint;", "Pole vector constraint – locator then IK handle"),
            ("|", "", ""),
            ("Circle", "circle -nr 0 1 0 -r 1.5 -n ctrl;", "Circle control"),
            ("Loc", "spaceLocator;", "Locator"),
            ("Bend", "nonLinear -type bend;", "Bend deformer"),
            ("Twist", "nonLinear -type twist;", "Twist deformer"),
        ],
    ),
    (
        "Animation",
        &[
            ("Key", "setKeyframe;", "Set key (S)"),
            ("KeyT", "setKeyframe -at translate;", "Key translate"),
            ("KeyR", "setKeyframe -at rotate;", "Key rotate"),
            ("KeyS", "setKeyframe -at scale;", "Key scale"),
            ("|", "", ""),
            ("Auto", "keyTangent -itt auto -ott auto;", "Auto tangents"),
            ("Lin", "keyTangent -itt linear -ott linear;", "Linear tangents"),
            ("Step", "keyTangent -itt step -ott step;", "Stepped tangents"),
            ("Cycle", "setInfinity -pri cycle -poi cycle;", "Cycle infinity"),
            ("|", "", ""),
            ("Bake", "bakeResults;", "Bake results"),
            ("DelKeys", "cutKey -cl;", "Delete all keys"),
        ],
    ),
    (
        "Rendering",
        &[
            ("Render", "render;", "Render current frame"),
            ("|", "", ""),
            ("Amb", "ambientLight -i 0.3;", "Ambient light"),
            ("Dir", "directionalLight; rotate -45 30 0;", "Directional light"),
            ("Point", "pointLight; move 0 4 0;", "Point light"),
            ("Spot", "spotLight; move 0 6 0; rotate -90 0 0;", "Spot light"),
            ("Area", "areaLight -i 2; move 0 5 0; rotate -90 0 0;", "Area light"),
            ("Cam", "camera;", "Camera"),
            ("|", "", ""),
            ("Lambert", "string $m = `shadingNode -asShader lambert`; hyperShade -assign $m;", "Assign lambert"),
            ("Blinn", "string $m = `shadingNode -asShader blinn`; hyperShade -assign $m;", "Assign blinn"),
            ("StdSurf", "string $m = `shadingNode -asShader standardSurface`; hyperShade -assign $m;", "Assign standard surface"),
        ],
    ),
];

pub fn shelf(app: &mut App, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        for (i, (name, _)) in SHELVES.iter().enumerate() {
            if ui.selectable_label(app.shelf_tab == i, RichText::new(*name).size(11.0)).clicked() {
                app.shelf_tab = i;
            }
        }
        if ui.selectable_label(app.shelf_tab == SHELVES.len(), RichText::new("Custom").size(11.0)).clicked() {
            app.shelf_tab = SHELVES.len();
        }
    });
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        if app.shelf_tab < SHELVES.len() {
            for (label, cmd, tip) in SHELVES[app.shelf_tab].1 {
                if *label == "|" {
                    ui.separator();
                    continue;
                }
                let b = egui::Button::new(RichText::new(*label).size(11.0)).min_size(egui::vec2(40.0, 30.0)).fill(Color32::from_rgb(82, 82, 82));
                if ui.add(b).on_hover_text(format!("{tip}\n{cmd}")).clicked() {
                    let _ = app.mel(cmd);
                }
            }
        } else {
            let custom = app.shelf_custom.clone();
            for (i, (label, cmd)) in custom.iter().enumerate() {
                let b = egui::Button::new(RichText::new(label).size(11.0)).min_size(egui::vec2(40.0, 30.0)).fill(Color32::from_rgb(70, 82, 96));
                let r = ui.add(b).on_hover_text(cmd);
                if r.clicked() {
                    let _ = app.mel(cmd);
                }
                r.context_menu(|ui| {
                    if ui.button("Remove from shelf").clicked() {
                        app.shelf_custom.remove(i);
                        ui.close();
                    }
                });
            }
            if custom.is_empty() {
                ui.label(RichText::new("Script Editor → \"Save to Shelf\" adds buttons here.").weak());
            }
        }
    });
}

/// Painted tool icons (no icon font needed).
fn tool_icon(p: &egui::Painter, r: egui::Rect, kind: u8, col: Color32) {
    use egui::{pos2, vec2, Stroke};
    let c = r.center();
    let s = r.width().min(r.height()) * 0.32;
    let st = Stroke::new(1.6, col);
    match kind {
        0 => {
            // select arrow
            let pts = vec![c + vec2(-s * 0.6, -s), c + vec2(-s * 0.6, s * 0.7), c + vec2(-s * 0.1, s * 0.25), c + vec2(s * 0.25, s * 1.0), c + vec2(s * 0.5, s * 0.85), c + vec2(s * 0.15, s * 0.1), c + vec2(s * 0.7, s * 0.1)];
            p.add(egui::Shape::convex_polygon(pts, col, Stroke::NONE));
        }
        1 => {
            // move: four arrows
            for d in [vec2(1.0, 0.0), vec2(-1.0, 0.0), vec2(0.0, 1.0), vec2(0.0, -1.0)] {
                let tip = c + d * s;
                p.line_segment([c, tip], st);
                let perp = vec2(-d.y, d.x);
                p.add(egui::Shape::convex_polygon(vec![tip + d * 3.0, tip + perp * 3.0, tip - perp * 3.0], col, Stroke::NONE));
            }
        }
        2 => {
            // rotate: ring with arrow
            p.circle_stroke(c, s, st);
            let tip = c + vec2(s, 0.0);
            p.add(egui::Shape::convex_polygon(vec![tip + vec2(-4.0, -1.0), tip + vec2(4.0, -1.0), tip + vec2(0.0, 5.0)], col, Stroke::NONE));
        }
        3 => {
            // scale: box with diagonal handle
            p.rect_stroke(egui::Rect::from_center_size(c - vec2(s * 0.25, -s * 0.25), vec2(s * 1.2, s * 1.2)), 0.0, st, egui::StrokeKind::Middle);
            p.line_segment([c, c + vec2(s, -s)], st);
            p.rect_filled(egui::Rect::from_center_size(c + vec2(s, -s), vec2(5.0, 5.0)), 0.0, col);
        }
        4 => {
            // single pane
            p.rect_stroke(egui::Rect::from_center_size(c, vec2(s * 2.0, s * 1.6)), 1.0, st, egui::StrokeKind::Middle);
        }
        5 => {
            let r = egui::Rect::from_center_size(c, vec2(s * 2.0, s * 1.6));
            p.rect_stroke(r, 1.0, st, egui::StrokeKind::Middle);
            p.line_segment([pos2(c.x, r.top()), pos2(c.x, r.bottom())], st);
            p.line_segment([pos2(r.left(), c.y), pos2(r.right(), c.y)], st);
        }
        _ => {
            let r = egui::Rect::from_center_size(c, vec2(s * 2.0, s * 1.6));
            p.rect_stroke(r, 1.0, st, egui::StrokeKind::Middle);
            p.line_segment([pos2(c.x, r.top()), pos2(c.x, r.bottom())], st);
        }
    }
}

fn icon_button(ui: &mut egui::Ui, selected: bool, kind: u8, tip: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(32.0, 30.0), egui::Sense::click());
    let bg = if selected {
        crate::theme::ACCENT
    } else if resp.hovered() {
        Color32::from_rgb(100, 100, 100)
    } else {
        Color32::from_rgb(78, 78, 78)
    };
    ui.painter().rect_filled(rect.shrink(1.0), 3.0, bg);
    tool_icon(ui.painter(), rect, kind, Color32::from_gray(235));
    resp.on_hover_text(tip)
}

pub fn toolbox(app: &mut App, ui: &mut egui::Ui) {
    ui.vertical_centered(|ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        for (t, k, tip) in [(Tool::Select, 0u8, "Select tool (Q)"), (Tool::Move, 1, "Move tool (W)"), (Tool::Rotate, 2, "Rotate tool (E)"), (Tool::Scale, 3, "Scale tool (R)")] {
            let r = icon_button(ui, app.tool == t, k, tip);
            if r.clicked() {
                if app.tool != Tool::Select {
                    app.last_tool = app.tool;
                }
                app.tool = t;
            }
            if r.double_clicked() {
                app.show_right = true;
                app.right_tab = RightTab::ToolSettings;
            }
        }
        ui.add_space(8.0);
        for (k, lay, tip) in [(4u8, Layout::Single, "Single perspective view"), (5, Layout::Quad, "Four view"), (6, Layout::TwoSide, "Two panes side by side")] {
            if icon_button(ui, app.layout == lay, k, tip).clicked() {
                app.layout = lay;
                if lay == Layout::Single {
                    app.active_pane = 0;
                }
            }
        }
    });
}
