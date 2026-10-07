//! Application state, command execution, hotkeys and frame orchestration.

use crate::camera::{CamKind, ViewCamera};
use crate::gl::GlRenderer;
use crate::viewport::{ManipDrag, ViewPane};
use crate::windows::{FileDialog, GraphState, RenderJob};
use glam::Vec3;
use rg_core::{ComponentMode, Evaluated, History, NodeId, Scene};
use rg_script::{Host, HostRequest, Interpreter};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Select,
    Move,
    Rotate,
    Scale,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuSet {
    Modeling,
    Rigging,
    Animation,
    Rendering,
}

impl MenuSet {
    pub const ALL: [MenuSet; 4] = [MenuSet::Modeling, MenuSet::Rigging, MenuSet::Animation, MenuSet::Rendering];
    pub fn name(self) -> &'static str {
        match self {
            MenuSet::Modeling => "Modeling",
            MenuSet::Rigging => "Rigging",
            MenuSet::Animation => "Animation",
            MenuSet::Rendering => "Rendering",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RightTab {
    ChannelBox,
    AttributeEditor,
    ToolSettings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    Single,
    Quad,
    TwoSide,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogKind {
    Command,
    Output,
    Result,
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavMode {
    /// Alt + LMB/MMB/RMB (Maya)
    Maya,
    /// RMB orbit, MMB pan, wheel zoom (no Alt needed)
    Simple,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayMode {
    Loop,
    Once,
    Oscillate,
}

pub struct Prefs {
    pub nav: NavMode,
    pub grid_size: f32,
    pub grid_spacing: f32,
    pub manip_size: f32,
    pub move_world: bool,
    pub rotate_world: bool,
    pub snap_grid: bool,
    pub snap_step: f32,
    pub rotate_step: f32,
    pub play_every_frame: bool,
    pub play_mode: PlayMode,
    pub show_hud: bool,
    pub max_undo: usize,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            nav: NavMode::Maya,
            grid_size: 12.0,
            grid_spacing: 1.0,
            manip_size: 1.0,
            move_world: true,
            rotate_world: false,
            snap_grid: false,
            snap_step: 1.0,
            rotate_step: 15.0,
            play_every_frame: false,
            play_mode: PlayMode::Loop,
            show_hud: true,
            max_undo: 200,
        }
    }
}

#[derive(Default)]
pub struct WindowFlags {
    pub script_editor: bool,
    pub graph_editor: bool,
    pub dope_sheet: bool,
    pub hypershade: bool,
    pub render_view: bool,
    pub render_settings: bool,
    pub sdk: bool,
    pub prefs: bool,
    pub hotkeys: bool,
    pub about: bool,
    pub component_editor: bool,
    pub node_editor: bool,
}

pub struct App {
    pub scene: Scene,
    pub history: History,
    pub interp: Interpreter,
    pub file: Option<PathBuf>,
    pub saved_rev: u64,
    pub ev: Arc<Evaluated>,
    pub ev_rev: u64,
    pub tool: Tool,
    pub last_tool: Tool,
    pub menu_set: MenuSet,
    pub shelf_tab: usize,
    pub panes: Vec<ViewPane>,
    pub layout: Layout,
    pub active_pane: usize,
    pub show_outliner: bool,
    pub show_right: bool,
    pub right_tab: RightTab,
    pub win: WindowFlags,
    pub log: Vec<(LogKind, String)>,
    pub cmd_line: String,
    pub cmd_result: String,
    pub help_line: String,
    pub script_text: String,
    pub playing: bool,
    pub play_backwards: bool,
    pub play_accum: f64,
    pub oscillate_dir: f64,
    pub auto_key: bool,
    pub manip: Option<ManipDrag>,
    pub gl: Option<Arc<Mutex<GlRenderer>>>,
    pub prefs: Prefs,
    pub file_dialog: Option<FileDialog>,
    pub render_job: Option<RenderJob>,
    pub render_tex: Option<egui::TextureHandle>,
    pub render_camera: Option<NodeId>,
    pub render_status: String,
    pub graph: GraphState,
    pub mat_previews: std::collections::HashMap<String, (String, egui::TextureHandle)>,
    pub hypershade_sel: Option<String>,
    pub last_command: Option<String>,
    pub channel_sel: Vec<String>,
    pub outliner_filter: String,
    pub outliner_rename: Option<(NodeId, String)>,
    pub outliner_expanded: std::collections::HashSet<NodeId>,
    pub pivot_mode: bool,
    pub screenshot: Option<(PathBuf, u32)>,
    pub quit_after_screenshot: bool,
    pub status_message: Option<(String, f64)>,
    pub sdk: crate::windows::SdkState,
    pub confirm_new: Option<PendingAction>,
    pub shelf_custom: Vec<(String, String)>,
    pub frame_counter: u64,
    pub last_frame_time: f64,
    pub fps_display: f32,
    pub smooth_cache: std::collections::HashMap<(usize, u32), (Arc<rg_core::mesh::Mesh>, Arc<rg_core::mesh::Mesh>)>,
    pub smooth_used: std::collections::HashSet<(usize, u32)>,
    pub held_x: bool,
    pub held_j: bool,
}

#[derive(Clone, Debug)]
pub enum PendingAction {
    New,
    Open,
    Quit,
}

impl App {
    pub fn new() -> Self {
        let scene = Scene::new();
        let panes = vec![
            ViewPane::new(CamKind::Persp),
            ViewPane::new(CamKind::Top),
            ViewPane::new(CamKind::Front),
            ViewPane::new(CamKind::Side),
        ];
        let mut app = Self {
            ev: Arc::new(scene.evaluate()),
            ev_rev: u64::MAX,
            scene,
            history: History::new(),
            interp: Interpreter::new(),
            file: None,
            saved_rev: 0,
            tool: Tool::Select,
            last_tool: Tool::Move,
            menu_set: MenuSet::Modeling,
            shelf_tab: 0,
            panes,
            layout: Layout::Single,
            active_pane: 0,
            show_outliner: true,
            show_right: true,
            right_tab: RightTab::ChannelBox,
            win: WindowFlags::default(),
            log: vec![(LogKind::Output, "// Riggerator ready. Type MEL commands below or press F1 for hotkeys.".into())],
            cmd_line: String::new(),
            cmd_result: String::new(),
            help_line: String::new(),
            script_text: "// Script Editor — MEL (RigScript)\n// Ctrl+Enter runs the selection or the whole buffer.\npolySphere -r 1 -n ball;\nmove 0 1 0;\nsetKeyframe -at ty -t 1 -v 1;\nsetKeyframe -at ty -t 12 -v 4;\nsetKeyframe -at ty -t 24 -v 1;\n".into(),
            playing: false,
            play_backwards: false,
            play_accum: 0.0,
            oscillate_dir: 1.0,
            auto_key: false,
            manip: None,
            gl: None,
            prefs: Prefs::default(),
            file_dialog: None,
            render_job: None,
            render_tex: None,
            render_camera: None,
            render_status: String::new(),
            graph: GraphState::default(),
            mat_previews: Default::default(),
            hypershade_sel: None,
            last_command: None,
            channel_sel: Vec::new(),
            outliner_filter: String::new(),
            outliner_rename: None,
            outliner_expanded: Default::default(),
            pivot_mode: false,
            screenshot: None,
            quit_after_screenshot: false,
            status_message: None,
            sdk: Default::default(),
            confirm_new: None,
            shelf_custom: Vec::new(),
            frame_counter: 0,
            last_frame_time: 0.0,
            fps_display: 0.0,
            smooth_cache: Default::default(),
            smooth_used: Default::default(),
            held_x: false,
            held_j: false,
        };
        app.saved_rev = app.scene.revision;
        app
    }

    // --------------------------------------------------------------- eval --

    pub fn evaluated(&mut self) -> Arc<Evaluated> {
        if self.ev_rev != self.scene.revision {
            self.ev = Arc::new(self.scene.evaluate());
            self.ev_rev = self.scene.revision;
        }
        self.ev.clone()
    }

    pub fn is_dirty(&self) -> bool {
        self.scene.revision != self.saved_rev
    }

    pub fn title(&self) -> String {
        let name = self.file.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "untitled".into());
        format!("Riggerator — {name}{}", if self.is_dirty() { "*" } else { "" })
    }

    // ----------------------------------------------------------- commands --

    pub fn log(&mut self, kind: LogKind, s: impl Into<String>) {
        let s = s.into();
        for line in s.lines() {
            self.log.push((kind, line.to_string()));
        }
        if self.log.len() > 5000 {
            self.log.drain(0..1000);
        }
    }

    /// Run MEL as one undo chunk, echoing it to the Script Editor (like Maya).
    pub fn mel(&mut self, src: &str) -> Result<rg_script::Value, String> {
        self.mel_impl(src, true)
    }

    /// Run MEL without echo (for internal UI plumbing).
    pub fn mel_quiet(&mut self, src: &str) -> Result<rg_script::Value, String> {
        self.mel_impl(src, false)
    }

    fn mel_impl(&mut self, src: &str, echo: bool) -> Result<rg_script::Value, String> {
        if echo {
            self.log(LogKind::Command, src.trim().to_string());
        }
        let label = src.split_whitespace().next().unwrap_or("command").trim_end_matches(';').to_string();
        self.history.limit = self.prefs.max_undo;
        let mut host = Host::new(&mut self.scene, &mut self.history, &mut self.file);
        let res = self.interp.run(&mut host, src, &label);
        let requests = std::mem::take(&mut host.requests);
        let touched = host.touched_history;
        let out = self.interp.take_output();
        if !out.is_empty() {
            self.log(LogKind::Output, out.trim_end().to_string());
        }
        if touched {
            // file -new/-open or undo/redo replaced the scene
            self.manip = None;
        }
        for r in requests {
            self.handle_request(r);
        }
        match &res {
            Ok(v) => {
                let s = v.as_string();
                if !s.is_empty() && echo {
                    self.log(LogKind::Result, format!("// Result: {s}"));
                }
                self.cmd_result = s;
                if echo {
                    let first = src.split_whitespace().next().unwrap_or("");
                    if !["undo", "redo", "select", "currentTime"].contains(&first) {
                        self.last_command = Some(src.to_string());
                    }
                }
            }
            Err(e) => {
                self.log(LogKind::Error, format!("// Error: {e}"));
                self.cmd_result = format!("Error: {e}");
                self.flash(format!("Error: {e}"));
            }
        }
        res
    }

    fn handle_request(&mut self, r: HostRequest) {
        match r {
            HostRequest::FrameSelected => self.frame_selected(false),
            HostRequest::FrameAll => self.frame_selected(true),
            HostRequest::OpenWindow(w) => match w.as_str() {
                "graphEditor" | "graph" => self.win.graph_editor = true,
                "hypershade" | "hyperShadePanel" => self.win.hypershade = true,
                "renderView" => self.win.render_view = true,
                "scriptEditor" => self.win.script_editor = true,
                _ => {}
            },
            HostRequest::RenderView => {
                self.win.render_view = true;
                self.start_render();
            }
            HostRequest::Playback(on) => self.playing = on,
            HostRequest::SetTool(t) => {
                self.tool = match t.to_lowercase().as_str() {
                    "movesupercontext" | "move" => Tool::Move,
                    "rotatesupercontext" | "rotate" => Tool::Rotate,
                    "scalesupercontext" | "scale" => Tool::Scale,
                    _ => Tool::Select,
                }
            }
            HostRequest::ComponentMode(m) => {
                let mode = match m.as_str() {
                    "vertex" => ComponentMode::Vertex,
                    "edge" => ComponentMode::Edge,
                    "face" => ComponentMode::Face,
                    _ => ComponentMode::Object,
                };
                self.set_component_mode(mode);
            }
            HostRequest::Message(m) => self.flash(m),
        }
    }

    pub fn flash(&mut self, s: impl Into<String>) {
        self.status_message = Some((s.into(), self.last_frame_time + 4.0));
    }

    /// Record an undo point before a direct (non-MEL) edit.
    pub fn checkpoint(&mut self, label: &str) {
        self.history.limit = self.prefs.max_undo;
        self.history.push(label, &self.scene);
    }

    pub fn undo(&mut self) {
        match self.history.undo(&mut self.scene) {
            Some(l) => {
                self.log(LogKind::Command, "undo;");
                self.log(LogKind::Output, format!("// Undo: {l}"));
            }
            None => self.flash("Nothing to undo"),
        }
        self.manip = None;
    }

    pub fn redo(&mut self) {
        match self.history.redo(&mut self.scene) {
            Some(l) => {
                self.log(LogKind::Command, "redo;");
                self.log(LogKind::Output, format!("// Redo: {l}"));
            }
            None => self.flash("Nothing to redo"),
        }
        self.manip = None;
    }

    // ---------------------------------------------------------- selection --

    pub fn names(&self, ids: &[NodeId]) -> String {
        ids.iter().map(|i| self.scene.name_of(*i)).collect::<Vec<_>>().join(" ")
    }

    pub fn selection_names(&self) -> String {
        self.names(&self.scene.selected())
    }

    pub fn set_component_mode(&mut self, mode: ComponentMode) {
        let node = if mode == ComponentMode::Object {
            None
        } else {
            self.scene
                .selected()
                .into_iter()
                .rev()
                .find(|id| self.scene.nodes.get(id).is_some_and(|n| n.mesh().is_some()))
                .or(self.scene.components.node)
        };
        if mode != ComponentMode::Object && node.is_none() {
            self.flash("Select a polygon mesh to edit its components");
            return;
        }
        let c = &mut self.scene.components;
        if c.mode != mode || c.node != node {
            c.mode = mode;
            c.node = node;
            c.clear();
            self.scene.touch();
        }
    }

    pub fn frame_selected(&mut self, all: bool) {
        let ev = self.evaluated();
        let ids = if all || self.scene.selection.is_empty() { self.scene.roots.clone() } else { self.scene.selected() };
        let mut b = self.scene.world_bounds(&ids, &ev);
        // component framing
        if !all && self.scene.components.mode != ComponentMode::Object && !self.scene.components.is_empty() {
            if let Some(node) = self.scene.components.node {
                if let Some(m) = ev.mesh(node) {
                    let w = ev.world(node);
                    let mut cb = rg_core::mesh::Aabb::EMPTY;
                    for v in self.scene.components.affected_verts(m) {
                        if let Some(p) = m.positions.get(v as usize) {
                            cb.extend(w.transform_point3(*p));
                        }
                    }
                    if !cb.is_empty() {
                        b = cb;
                    }
                }
            }
        }
        let (c, r) = if b.is_empty() { (Vec3::ZERO, 5.0) } else { (b.center(), b.size().length() * 0.5) };
        let scene = &self.scene;
        let idx = self.active_pane;
        let pane = &mut self.panes[idx];
        pane.cam.frame(c, r.max(0.25), scene);
    }

    pub fn frame_all_panes(&mut self) {
        let save = self.active_pane;
        for i in 0..self.panes.len() {
            self.active_pane = i;
            self.frame_selected(true);
        }
        self.active_pane = save;
    }

    // ----------------------------------------------------------- playback --

    pub fn set_time(&mut self, t: f64) {
        let t = (t * 1000.0).round() / 1000.0;
        if (t - self.scene.time.current).abs() > 1e-9 {
            self.scene.set_time(t);
        }
    }

    pub fn step_time(&mut self, dt: f64) {
        let ts = &self.scene.time;
        let mut t = self.scene.time.current + dt;
        if t > ts.max {
            t = ts.min;
        }
        if t < ts.min {
            t = ts.max;
        }
        self.set_time(t);
    }

    pub fn tick_playback(&mut self, dt: f64) {
        if !self.playing {
            return;
        }
        let fps = self.scene.time.fps;
        let frames = if self.prefs.play_every_frame { 1.0 } else { dt * fps + self.play_accum };
        let whole = frames.floor();
        self.play_accum = frames - whole;
        if whole < 1.0 {
            return;
        }
        let dir = if self.play_backwards { -1.0 } else { 1.0 } * self.oscillate_dir;
        let (min, max) = (self.scene.time.min, self.scene.time.max);
        let mut t = self.scene.time.current + whole * dir;
        match self.prefs.play_mode {
            PlayMode::Loop => {
                let span = (max - min + 1.0).max(1.0);
                while t > max {
                    t -= span;
                }
                while t < min {
                    t += span;
                }
            }
            PlayMode::Once => {
                if t > max || t < min {
                    t = t.clamp(min, max);
                    self.playing = false;
                }
            }
            PlayMode::Oscillate => {
                if t > max {
                    t = max;
                    self.oscillate_dir = -self.oscillate_dir;
                } else if t < min {
                    t = min;
                    self.oscillate_dir = -self.oscillate_dir;
                }
            }
        }
        self.set_time(t);
    }

    pub fn next_key(&mut self, forward: bool) {
        let sel = self.scene.selected();
        let ids = if sel.is_empty() { self.scene.nodes.keys().copied().collect() } else { sel };
        let times = self.scene.key_times(&ids);
        let cur = self.scene.time.current;
        let t = if forward {
            times.iter().copied().find(|t| *t > cur + 1e-6).or(times.first().copied())
        } else {
            times.iter().rev().copied().find(|t| *t < cur - 1e-6).or(times.last().copied())
        };
        if let Some(t) = t {
            self.set_time(t);
        }
    }

    /// After a manual attribute edit: auto-key animated channels.
    pub fn auto_key_attrs(&mut self, id: NodeId, attrs: &[&str]) {
        if !self.auto_key {
            return;
        }
        let t = self.scene.time.current;
        for a in attrs {
            let animated = self.scene.nodes.get(&id).is_some_and(|n| n.anim.contains_key(*a));
            if animated {
                let _ = self.scene.set_key(id, a, t, None);
            }
        }
    }

    // -------------------------------------------------------------- files --

    pub fn new_scene(&mut self) {
        self.scene = Scene::new();
        self.history.clear();
        self.file = None;
        self.saved_rev = self.scene.revision;
        self.manip = None;
        self.render_tex = None;
        self.log(LogKind::Command, "file -new -f;");
    }

    pub fn open_path(&mut self, p: PathBuf) {
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        let res = if ext == "rgs" || ext == "json" {
            rg_io::load_scene(&p)
        } else {
            let mut s = Scene::new();
            rg_io::import_file(&mut s, &p).map(|_| s)
        };
        match res {
            Ok(s) => {
                self.scene = s;
                self.history.clear();
                self.log(LogKind::Command, format!("file -o \"{}\";", p.display()));
                self.file = if ext == "rgs" { Some(p) } else { None };
                self.saved_rev = self.scene.revision;
                self.manip = None;
                let t = self.scene.time.current;
                self.scene.set_time(t);
                self.saved_rev = self.scene.revision;
                self.frame_all_panes();
            }
            Err(e) => {
                self.log(LogKind::Error, format!("// Error: {e}"));
                self.flash(format!("Could not open: {e}"));
            }
        }
    }

    pub fn save_to(&mut self, p: PathBuf) {
        let p = if p.extension().is_none() { p.with_extension("rgs") } else { p };
        match rg_io::export_file(&self.scene, &p, None) {
            Ok(()) => {
                self.log(LogKind::Command, format!("file -s \"{}\";", p.display()));
                if p.extension().is_some_and(|e| e == "rgs") {
                    self.file = Some(p.clone());
                    self.saved_rev = self.scene.revision;
                }
                self.flash(format!("Saved {}", p.display()));
            }
            Err(e) => {
                self.log(LogKind::Error, format!("// Error: {e}"));
                self.flash(format!("Save failed: {e}"));
            }
        }
    }

    pub fn save(&mut self) {
        match self.file.clone() {
            Some(p) => self.save_to(p),
            None => self.file_dialog = Some(FileDialog::save("Save Scene As", &["rgs"], crate::windows::FileAction::SaveScene)),
        }
    }

    // ------------------------------------------------------------- render --

    pub fn start_render(&mut self) {
        let opts = rg_render::RenderOptions::from_scene(&self.scene);
        let aspect = opts.width as f32 / opts.height.max(1) as f32;
        let ev = self.evaluated();
        let cam = match self.render_camera.filter(|c| self.scene.nodes.contains_key(c)) {
            Some(c) => rg_render::RenderCamera::from_node(&self.scene, &ev, c, aspect),
            None => None,
        };
        let cam = cam.unwrap_or_else(|| {
            // render through the active perspective viewport
            let pane = &self.panes[self.active_pane];
            let mut world = pane.cam.world(&self.scene, &ev);
            if pane.cam.is_ortho(&self.scene) {
                world = self.panes[0].cam.world(&self.scene, &ev);
            }
            let hfov = pane.cam.hfov;
            let vfov = 2.0 * ((hfov * 0.5).tan() / aspect).atan();
            rg_render::RenderCamera { world, vfov, orthographic: false, ortho_width: 10.0 }
        });
        self.render_job = Some(RenderJob::start(self.scene.clone(), cam, opts));
        self.render_status = "Rendering…".into();
        self.win.render_view = true;
    }

    // ------------------------------------------------------------- frame --

    pub fn new_pane_cameras(&mut self) {
        for (i, k) in [CamKind::Persp, CamKind::Top, CamKind::Front, CamKind::Side].into_iter().enumerate() {
            self.panes[i].cam = ViewCamera::new(k);
        }
    }

    pub fn visible_panes(&self) -> Vec<usize> {
        match self.layout {
            Layout::Single => vec![self.active_pane],
            Layout::Quad => vec![1, 0, 2, 3],
            Layout::TwoSide => vec![0, if self.active_pane == 0 { 1 } else { self.active_pane }],
        }
    }
}
