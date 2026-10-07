use crate::eval::Evaluated;
use crate::material::Material;
use crate::node::*;
use glam::{Mat4, Quat, Vec3};
use rg_anim::{AnimCurve, Tangent};
use rg_mesh::{ops, Aabb, Edge, Mesh};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConstraintKind {
    Parent,
    Point,
    Orient,
    Scale,
    Aim,
    /// Pole-vector constraint: the driven node is an IK handle.
    PoleVector,
}

impl ConstraintKind {
    pub fn name(self) -> &'static str {
        match self {
            ConstraintKind::Parent => "parentConstraint",
            ConstraintKind::Point => "pointConstraint",
            ConstraintKind::Orient => "orientConstraint",
            ConstraintKind::Scale => "scaleConstraint",
            ConstraintKind::Aim => "aimConstraint",
            ConstraintKind::PoleVector => "poleVectorConstraint",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Constraint {
    pub name: String,
    pub kind: ConstraintKind,
    pub driven: NodeId,
    pub targets: Vec<NodeId>,
    pub weights: Vec<f32>,
    /// Per-target offset (target space -> driven world) when maintaining offset.
    pub offsets: Vec<Mat4>,
    pub maintain_offset: bool,
    pub aim_axis: Vec3,
    pub up_axis: Vec3,
    pub world_up: Vec3,
    #[serde(default = "one")]
    pub enabled: f32,
}

fn one() -> f32 {
    1.0
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DrivenKey {
    pub driver: NodeId,
    pub driver_attr: String,
    pub driven: NodeId,
    pub driven_attr: String,
    /// Curve mapping driver value (as "time") to driven value.
    pub curve: AnimCurve,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DisplayLayer {
    pub name: String,
    pub visible: bool,
    /// Reference layers are visible but not selectable.
    pub reference: bool,
    pub color: Option<Vec3>,
    pub members: Vec<NodeId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TimeSettings {
    pub current: f64,
    /// Playback range.
    pub min: f64,
    pub max: f64,
    /// Animation range.
    pub anim_start: f64,
    pub anim_end: f64,
    pub fps: f64,
}

impl Default for TimeSettings {
    fn default() -> Self {
        Self { current: 1.0, min: 1.0, max: 120.0, anim_start: 1.0, anim_end: 200.0, fps: 24.0 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RenderSettings {
    pub width: u32,
    pub height: u32,
    pub samples: u32,
    pub max_bounces: u32,
    pub camera: Option<NodeId>,
    pub background: Vec3,
    /// Use a simple sky dome as environment light.
    pub sky: bool,
    pub sky_intensity: f32,
    /// Default key light when the scene has no lights (like Maya's default light).
    pub default_light: bool,
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            width: 960,
            height: 540,
            samples: 16,
            max_bounces: 3,
            camera: None,
            background: Vec3::new(0.0, 0.0, 0.0),
            sky: true,
            sky_intensity: 0.6,
            default_light: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ComponentMode {
    #[default]
    Object,
    Vertex,
    Edge,
    Face,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ComponentSelection {
    pub mode: ComponentMode,
    pub node: Option<NodeId>,
    pub verts: BTreeSet<u32>,
    pub edges: BTreeSet<Edge>,
    pub faces: BTreeSet<u32>,
}

impl ComponentSelection {
    pub fn clear(&mut self) {
        self.verts.clear();
        self.edges.clear();
        self.faces.clear();
    }
    pub fn is_empty(&self) -> bool {
        self.verts.is_empty() && self.edges.is_empty() && self.faces.is_empty()
    }
    /// All vertices touched by the current component selection.
    pub fn affected_verts(&self, mesh: &Mesh) -> BTreeSet<u32> {
        let mut out = BTreeSet::new();
        match self.mode {
            ComponentMode::Vertex => out.extend(self.verts.iter().copied()),
            ComponentMode::Edge => {
                for e in &self.edges {
                    out.insert(e.0);
                    out.insert(e.1);
                }
            }
            ComponentMode::Face => {
                for &f in &self.faces {
                    if let Some(f) = mesh.faces.get(f as usize) {
                        out.extend(f.verts.iter().copied());
                    }
                }
            }
            ComponentMode::Object => {}
        }
        out
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    pub nodes: BTreeMap<NodeId, Node>,
    pub roots: Vec<NodeId>,
    next_id: u64,
    pub materials: BTreeMap<String, Material>,
    pub constraints: Vec<Constraint>,
    pub driven_keys: Vec<DrivenKey>,
    pub layers: Vec<DisplayLayer>,
    pub time: TimeSettings,
    pub render: RenderSettings,
    pub selection: Vec<NodeId>,
    #[serde(default)]
    pub components: ComponentSelection,
    /// Bumped on every mutation; used by caches.
    #[serde(skip)]
    pub revision: u64,
    /// Scene-level notes / file info.
    #[serde(default)]
    pub info: BTreeMap<String, String>,
}

impl Default for Scene {
    fn default() -> Self {
        Self::new()
    }
}

pub const TRANSFORM_ATTRS: [&str; 10] =
    ["translateX", "translateY", "translateZ", "rotateX", "rotateY", "rotateZ", "scaleX", "scaleY", "scaleZ", "visibility"];

/// Map Maya short attribute names to long ones.
pub fn long_attr_name(a: &str) -> &str {
    match a {
        "tx" => "translateX",
        "ty" => "translateY",
        "tz" => "translateZ",
        "rx" => "rotateX",
        "ry" => "rotateY",
        "rz" => "rotateZ",
        "sx" => "scaleX",
        "sy" => "scaleY",
        "sz" => "scaleZ",
        "v" => "visibility",
        "jox" => "jointOrientX",
        "joy" => "jointOrientY",
        "joz" => "jointOrientZ",
        "fl" => "focalLength",
        "in" => "intensity",
        _ => a,
    }
}

/// Compound attributes expand to their children (translate -> tx,ty,tz).
pub fn expand_attr(a: &str) -> Vec<String> {
    match a {
        "t" | "translate" => vec!["translateX".into(), "translateY".into(), "translateZ".into()],
        "r" | "rotate" => vec!["rotateX".into(), "rotateY".into(), "rotateZ".into()],
        "s" | "scale" => vec!["scaleX".into(), "scaleY".into(), "scaleZ".into()],
        _ => vec![long_attr_name(a).to_string()],
    }
}

impl Scene {
    pub fn new() -> Self {
        let mut materials = BTreeMap::new();
        materials.insert("lambert1".to_string(), Material::default_lambert());
        Self {
            nodes: BTreeMap::new(),
            roots: Vec::new(),
            next_id: 1,
            materials,
            constraints: Vec::new(),
            driven_keys: Vec::new(),
            layers: Vec::new(),
            time: TimeSettings::default(),
            render: RenderSettings::default(),
            selection: Vec::new(),
            components: ComponentSelection::default(),
            revision: 0,
            info: BTreeMap::new(),
        }
    }

    pub fn touch(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    // ------------------------------------------------------------ naming --

    pub fn find(&self, name: &str) -> Option<NodeId> {
        // allow "|group1|pCube1" style paths: match the last component
        let n = name.rsplit('|').next().unwrap_or(name);
        self.nodes.values().find(|x| x.name == n).map(|x| x.id)
    }

    pub fn name_exists(&self, name: &str) -> bool {
        self.nodes.values().any(|n| n.name == name)
            || self.materials.contains_key(name)
            || self.constraints.iter().any(|c| c.name == name)
    }

    /// "pCube" -> "pCube1"; "pCube3" -> next free "pCubeN".
    pub fn unique_name(&self, wanted: &str) -> String {
        let wanted = if wanted.is_empty() { "node" } else { wanted };
        let base = wanted.trim_end_matches(|c: char| c.is_ascii_digit());
        let base = if base.is_empty() { "node" } else { base };
        if wanted != base && !self.name_exists(wanted) {
            return wanted.to_string();
        }
        let mut i = 1;
        loop {
            let cand = format!("{base}{i}");
            if !self.name_exists(&cand) {
                return cand;
            }
            i += 1;
        }
    }

    /// The wanted name if free, otherwise the next numbered variant.
    pub fn unique_exact(&self, wanted: &str) -> String {
        if !wanted.is_empty() && !self.name_exists(wanted) {
            wanted.to_string()
        } else {
            self.unique_name(wanted)
        }
    }

    pub fn rename(&mut self, id: NodeId, name: &str) -> String {
        let clean: String = name.chars().map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' }).collect();
        let current = self.nodes.get(&id).map(|n| n.name.clone());
        if current.as_deref() == Some(clean.as_str()) {
            return clean;
        }
        let unique = if self.name_exists(&clean) { self.unique_name(&clean) } else { clean };
        if let Some(n) = self.nodes.get_mut(&id) {
            n.name = unique.clone();
        }
        self.touch();
        unique
    }

    pub fn path(&self, id: NodeId) -> String {
        let mut parts = Vec::new();
        let mut cur = Some(id);
        while let Some(c) = cur {
            let Some(n) = self.nodes.get(&c) else { break };
            parts.push(n.name.clone());
            cur = n.parent;
        }
        parts.reverse();
        format!("|{}", parts.join("|"))
    }

    // ------------------------------------------------------------- nodes --

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }
    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.revision = self.revision.wrapping_add(1);
        self.nodes.get_mut(&id)
    }
    pub fn name_of(&self, id: NodeId) -> String {
        self.nodes.get(&id).map(|n| n.name.clone()).unwrap_or_else(|| format!("<{}>", id.0))
    }

    pub fn add_node(&mut self, name: &str, kind: NodeKind, parent: Option<NodeId>) -> NodeId {
        let id = NodeId(self.next_id);
        self.next_id += 1;
        let name = self.unique_name(name);
        let node = Node {
            id,
            name,
            parent: None,
            children: Vec::new(),
            transform: Transform::default(),
            visible: true,
            kind,
            user_attrs: BTreeMap::new(),
            locked: BTreeSet::new(),
            anim: BTreeMap::new(),
            color: None,
            template: false,
            notes: String::new(),
        };
        self.nodes.insert(id, node);
        match parent.filter(|p| self.nodes.contains_key(p)) {
            Some(p) => {
                self.nodes.get_mut(&p).unwrap().children.push(id);
                self.nodes.get_mut(&id).unwrap().parent = Some(p);
            }
            None => self.roots.push(id),
        }
        self.touch();
        id
    }

    pub fn add_mesh(&mut self, name: &str, mesh: Mesh) -> NodeId {
        self.add_node(name, NodeKind::Mesh(MeshShape::new(mesh)), None)
    }

    pub fn children(&self, id: Option<NodeId>) -> &[NodeId] {
        match id {
            Some(id) => self.nodes.get(&id).map(|n| n.children.as_slice()).unwrap_or(&[]),
            None => &self.roots,
        }
    }

    /// Depth-first ids (parents before children).
    pub fn walk(&self) -> Vec<NodeId> {
        let mut out = Vec::with_capacity(self.nodes.len());
        let mut stack: Vec<NodeId> = self.roots.iter().rev().copied().collect();
        while let Some(id) = stack.pop() {
            out.push(id);
            if let Some(n) = self.nodes.get(&id) {
                stack.extend(n.children.iter().rev().copied());
            }
        }
        out
    }

    pub fn descendants(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack = vec![id];
        while let Some(c) = stack.pop() {
            if let Some(n) = self.nodes.get(&c) {
                for &ch in n.children.iter().rev() {
                    out.push(ch);
                    stack.push(ch);
                }
            }
        }
        out
    }

    pub fn is_ancestor(&self, ancestor: NodeId, of: NodeId) -> bool {
        let mut cur = self.nodes.get(&of).and_then(|n| n.parent);
        while let Some(c) = cur {
            if c == ancestor {
                return true;
            }
            cur = self.nodes.get(&c).and_then(|n| n.parent);
        }
        false
    }

    fn detach(&mut self, id: NodeId) {
        let parent = self.nodes.get(&id).and_then(|n| n.parent);
        match parent {
            Some(p) => {
                if let Some(pn) = self.nodes.get_mut(&p) {
                    pn.children.retain(|c| *c != id);
                }
            }
            None => self.roots.retain(|c| *c != id),
        }
        if let Some(n) = self.nodes.get_mut(&id) {
            n.parent = None;
        }
    }

    /// Reparent; with `keep_world` the node keeps its world transform.
    pub fn set_parent(&mut self, id: NodeId, parent: Option<NodeId>, keep_world: bool) -> Result<(), String> {
        if !self.nodes.contains_key(&id) {
            return Err("no such node".into());
        }
        if let Some(p) = parent {
            if p == id || self.is_ancestor(id, p) {
                return Err(format!("cannot parent {} under its own descendant", self.name_of(id)));
            }
            if !self.nodes.contains_key(&p) {
                return Err("no such parent".into());
            }
        }
        let world = self.world_matrix(id);
        self.detach(id);
        match parent {
            Some(p) => {
                self.nodes.get_mut(&p).unwrap().children.push(id);
                self.nodes.get_mut(&id).unwrap().parent = Some(p);
            }
            None => self.roots.push(id),
        }
        if keep_world {
            let pw = parent.map(|p| self.world_matrix(p)).unwrap_or(Mat4::IDENTITY);
            let local = pw.inverse() * world;
            let n = self.nodes.get_mut(&id).unwrap();
            let jo = n.joint_orient();
            n.transform.set_from_matrix(local, jo);
        }
        self.touch();
        Ok(())
    }

    /// Delete nodes and their descendants, cleaning up references.
    pub fn delete(&mut self, id: NodeId) {
        if !self.nodes.contains_key(&id) {
            return;
        }
        let mut doomed = self.descendants(id);
        doomed.push(id);
        // IK handles whose chain is being deleted go too.
        let set: HashSet<NodeId> = doomed.iter().copied().collect();
        let extra: Vec<NodeId> = self
            .nodes
            .values()
            .filter_map(|n| match &n.kind {
                NodeKind::IkHandle(ik) if set.contains(&ik.start_joint) || set.contains(&ik.end_joint) => Some(n.id),
                _ => None,
            })
            .filter(|h| !set.contains(h))
            .collect();
        self.detach(id);
        for d in &doomed {
            self.nodes.remove(d);
        }
        let set: HashSet<NodeId> = doomed.iter().copied().collect();
        self.selection.retain(|s| !set.contains(s));
        if self.components.node.is_some_and(|n| set.contains(&n)) {
            self.components = ComponentSelection::default();
        }
        self.constraints.retain(|c| !set.contains(&c.driven) && !c.targets.iter().any(|t| set.contains(t)));
        self.driven_keys.retain(|d| !set.contains(&d.driver) && !set.contains(&d.driven));
        for l in &mut self.layers {
            l.members.retain(|m| !set.contains(m));
        }
        // Unbind skins that reference deleted joints.
        for n in self.nodes.values_mut() {
            if let NodeKind::Mesh(ms) = &mut n.kind {
                if ms.skin.as_ref().is_some_and(|s| s.joints.iter().any(|j| set.contains(j))) {
                    ms.skin = None;
                }
            }
            if let NodeKind::IkHandle(ik) = &mut n.kind {
                if ik.pole_target.is_some_and(|p| set.contains(&p)) {
                    ik.pole_target = None;
                }
            }
        }
        if self.render.camera.is_some_and(|c| set.contains(&c)) {
            self.render.camera = None;
        }
        for e in extra {
            self.delete(e);
        }
        self.touch();
    }

    /// Duplicate a node and its subtree. Returns the new root.
    pub fn duplicate(&mut self, id: NodeId) -> Option<NodeId> {
        let parent = self.nodes.get(&id)?.parent;
        let new = self.dup_rec(id, parent);
        Some(new)
    }

    fn dup_rec(&mut self, id: NodeId, parent: Option<NodeId>) -> NodeId {
        let src = self.nodes[&id].clone();
        let new_id = self.add_node(&src.name, src.kind.clone(), parent);
        {
            let n = self.nodes.get_mut(&new_id).unwrap();
            n.transform = src.transform.clone();
            n.visible = src.visible;
            n.user_attrs = src.user_attrs.clone();
            n.locked = src.locked.clone();
            n.color = src.color;
            n.notes = src.notes.clone();
            // Maya's plain duplicate does not copy animation or skinning.
            if let NodeKind::Mesh(ms) = &mut n.kind {
                if let Some(skin) = ms.skin.take() {
                    let _ = skin;
                }
            }
        }
        // a duplicated skinned mesh keeps its deformed pose? Maya duplicates
        // the original shape; we keep the base mesh.
        for c in src.children {
            self.dup_rec(c, Some(new_id));
        }
        new_id
    }

    // ---------------------------------------------------------- matrices --

    /// Base (non-evaluated) world matrix from stored attribute values.
    pub fn world_matrix(&self, id: NodeId) -> Mat4 {
        let mut m = Mat4::IDENTITY;
        let mut cur = Some(id);
        while let Some(c) = cur {
            let Some(n) = self.nodes.get(&c) else { break };
            m = n.local_matrix() * m;
            cur = n.parent;
        }
        m
    }

    pub fn parent_world(&self, id: NodeId) -> Mat4 {
        self.nodes.get(&id).and_then(|n| n.parent).map(|p| self.world_matrix(p)).unwrap_or(Mat4::IDENTITY)
    }

    /// Set a node's world matrix by solving for its local transform.
    pub fn set_world_matrix(&mut self, id: NodeId, world: Mat4) {
        let local = self.parent_world(id).inverse() * world;
        if let Some(n) = self.nodes.get_mut(&id) {
            let jo = n.joint_orient();
            n.transform.set_from_matrix(local, jo);
        }
        self.touch();
    }

    pub fn world_position(&self, id: NodeId) -> Vec3 {
        let n = &self.nodes[&id];
        self.world_matrix(id).transform_point3(n.transform.pivot)
    }

    // -------------------------------------------------------- attributes --

    /// Keyable/channel-box attributes of a node.
    pub fn keyable_attrs(&self, id: NodeId) -> Vec<String> {
        let Some(n) = self.nodes.get(&id) else { return Vec::new() };
        let mut v: Vec<String> = TRANSFORM_ATTRS.iter().map(|s| s.to_string()).collect();
        v.extend(n.user_attrs.keys().cloned());
        v
    }

    /// Shape attributes (shown under SHAPES in the channel box).
    pub fn shape_attrs(&self, id: NodeId) -> Vec<String> {
        let Some(n) = self.nodes.get(&id) else { return Vec::new() };
        let mut v: Vec<String> = Vec::new();
        match &n.kind {
            NodeKind::Mesh(ms) => {
                v.push("smoothLevel".into());
                for b in &ms.blend_shapes {
                    v.push(format!("blendShape.{}", b.name));
                }
                for d in &ms.deformers {
                    v.push(format!("{}.envelope", d.name));
                    for p in d.kind.params() {
                        v.push(format!("{}.{}", d.name, p));
                    }
                }
                if ms.skin.is_some() {
                    v.push("skinCluster.envelope".into());
                }
            }
            NodeKind::Camera(_) => v.extend(["focalLength", "nearClipPlane", "farClipPlane", "orthographicWidth"].map(String::from)),
            NodeKind::Light(_) => v.extend(
                ["intensity", "colorR", "colorG", "colorB", "coneAngle", "penumbraAngle", "decayRate"].map(String::from),
            ),
            NodeKind::Joint(_) => v.extend(["jointOrientX", "jointOrientY", "jointOrientZ", "radius"].map(String::from)),
            NodeKind::IkHandle(_) => {
                v.extend(["twist", "ikBlend", "poleVectorX", "poleVectorY", "poleVectorZ"].map(String::from))
            }
            _ => {}
        }
        v
    }

    pub fn get_attr(&self, id: NodeId, attr: &str) -> Option<f64> {
        node_get_attr(self.nodes.get(&id)?, attr)
    }

    pub fn set_attr(&mut self, id: NodeId, attr: &str, value: f64) -> Result<(), String> {
        let n = self.nodes.get_mut(&id).ok_or("no such node")?;
        node_set_attr(n, attr, value, false)?;
        self.touch();
        Ok(())
    }

    pub fn add_user_attr(&mut self, id: NodeId, name: &str, default: f64) -> Result<(), String> {
        let n = self.nodes.get_mut(&id).ok_or("no such node")?;
        if TRANSFORM_ATTRS.contains(&name) || n.user_attrs.contains_key(name) {
            return Err(format!("attribute {name} already exists"));
        }
        n.user_attrs.insert(name.to_string(), default);
        self.touch();
        Ok(())
    }

    // ---------------------------------------------------------- keyframes --

    pub fn set_key(&mut self, id: NodeId, attr: &str, time: f64, value: Option<f64>) -> Result<(), String> {
        let a = long_attr_name(attr).to_string();
        let v = match value {
            Some(v) => v,
            None => self.get_attr(id, &a).ok_or_else(|| format!("no attribute {a}"))?,
        };
        let n = self.nodes.get_mut(&id).ok_or("no such node")?;
        let is_rot = a.starts_with("rotate");
        let c = n.anim.entry(a.clone()).or_default();
        let idx = c.set_key(time, v);
        if a == "visibility" {
            c.set_tangents(idx, Tangent::Stepped);
        }
        let _ = is_rot;
        if (time - self.time.current).abs() < 1e-6 {
            let _ = self.set_attr(id, &a, v);
        }
        self.touch();
        Ok(())
    }

    /// Key all keyable transform channels (the S hotkey).
    pub fn key_all(&mut self, id: NodeId, time: f64) {
        for a in self.keyable_attrs(id) {
            let _ = self.set_key(id, &a, time, None);
        }
    }

    pub fn remove_key(&mut self, id: NodeId, attr: Option<&str>, time: f64) -> usize {
        let Some(n) = self.nodes.get_mut(&id) else { return 0 };
        let mut count = 0;
        for (a, c) in n.anim.iter_mut() {
            if attr.is_none_or(|x| long_attr_name(x) == a) && c.remove_key_at(time) {
                count += 1;
            }
        }
        n.anim.retain(|_, c| !c.is_empty());
        self.touch();
        count
    }

    pub fn key_times(&self, ids: &[NodeId]) -> Vec<f64> {
        let mut t: Vec<f64> = Vec::new();
        for id in ids {
            if let Some(n) = self.nodes.get(id) {
                for c in n.anim.values() {
                    t.extend(c.keys.iter().map(|k| k.time));
                }
            }
        }
        t.sort_by(|a, b| a.total_cmp(b));
        t.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
        t
    }

    /// Go to a time: animated attributes take their curve values (as in Maya).
    pub fn set_time(&mut self, t: f64) {
        self.time.current = t;
        let ids: Vec<NodeId> = self.nodes.keys().copied().collect();
        for id in ids {
            let vals: Vec<(String, f64)> = self.nodes[&id].anim.iter().map(|(a, c)| (a.clone(), c.evaluate(t))).collect();
            let n = self.nodes.get_mut(&id).unwrap();
            for (a, v) in vals {
                let _ = node_set_attr(n, &a, v, true);
            }
        }
        self.touch();
    }

    // ---------------------------------------------------------- selection --

    pub fn select(&mut self, ids: &[NodeId], mode: SelectMode) {
        match mode {
            SelectMode::Replace => self.selection = ids.to_vec(),
            SelectMode::Add => {
                for id in ids {
                    if !self.selection.contains(id) {
                        self.selection.push(*id);
                    }
                }
            }
            SelectMode::Toggle => {
                for id in ids {
                    if let Some(p) = self.selection.iter().position(|s| s == id) {
                        self.selection.remove(p);
                    } else {
                        self.selection.push(*id);
                    }
                }
            }
            SelectMode::Remove => self.selection.retain(|s| !ids.contains(s)),
        }
        self.selection.retain(|s| self.nodes.contains_key(s));
        self.touch();
    }

    pub fn selected(&self) -> Vec<NodeId> {
        self.selection.clone()
    }

    pub fn is_selectable(&self, id: NodeId) -> bool {
        let Some(n) = self.nodes.get(&id) else { return false };
        if n.template {
            return false;
        }
        !self.layers.iter().any(|l| l.reference && l.members.contains(&id))
    }

    /// Visible considering hierarchy and display layers.
    pub fn is_visible(&self, id: NodeId) -> bool {
        let mut cur = Some(id);
        while let Some(c) = cur {
            let Some(n) = self.nodes.get(&c) else { return false };
            if !n.visible {
                return false;
            }
            if self.layers.iter().any(|l| !l.visible && l.members.contains(&c)) {
                return false;
            }
            cur = n.parent;
        }
        true
    }

    // ---------------------------------------------------------- materials --

    pub fn create_material(&mut self, name: &str, shader: crate::material::ShaderType) -> String {
        let name = self.unique_exact(name);
        self.materials.insert(name.clone(), Material::new(&name, shader));
        self.touch();
        name
    }

    pub fn assign_material(&mut self, id: NodeId, mat: &str) -> Result<(), String> {
        if !self.materials.contains_key(mat) {
            return Err(format!("no material {mat}"));
        }
        let n = self.nodes.get_mut(&id).ok_or("no such node")?;
        let ms = n.mesh_mut().ok_or_else(|| "not a mesh".to_string())?;
        ms.material = Some(mat.to_string());
        self.touch();
        Ok(())
    }

    pub fn material_for(&self, id: NodeId) -> &Material {
        self.nodes
            .get(&id)
            .and_then(|n| n.mesh())
            .and_then(|m| m.material.as_ref())
            .and_then(|m| self.materials.get(m))
            .or_else(|| self.materials.get("lambert1"))
            .expect("lambert1 always exists")
    }

    // ----------------------------------------------------------- modeling --

    /// Mesh in world space (base pose).
    pub fn world_mesh(&self, id: NodeId) -> Option<Mesh> {
        let n = self.nodes.get(&id)?;
        let ms = n.mesh()?;
        Some(ms.mesh.transformed(self.world_matrix(id)))
    }

    /// Edit a node's mesh in place (copy-on-write).
    pub fn edit_mesh<R>(&mut self, id: NodeId, f: impl FnOnce(&mut Mesh) -> R) -> Option<R> {
        let n = self.nodes.get_mut(&id)?;
        let ms = n.mesh_mut()?;
        let r = f(Arc::make_mut(&mut ms.mesh));
        // topology changes invalidate skin weights / blend targets
        let nv = ms.mesh.positions.len();
        if ms.skin.as_ref().is_some_and(|s| s.weights.len() != nv) {
            ms.skin = None;
        }
        ms.blend_shapes.retain(|b| b.deltas.len() == nv);
        self.touch();
        Some(r)
    }

    /// Freeze transformations: bake TRS into mesh points and children.
    pub fn freeze_transforms(&mut self, id: NodeId) {
        let Some(n) = self.nodes.get(&id) else { return };
        let local = n.local_matrix();
        let children = n.children.clone();
        if let Some(ms) = self.nodes.get_mut(&id).and_then(|n| n.mesh_mut()) {
            let m = ms.mesh.transformed(local);
            ms.mesh = Arc::new(m);
        }
        if let Some(NodeKind::Curve(c)) = self.nodes.get_mut(&id).map(|n| &mut n.kind) {
            for p in &mut c.cvs {
                *p = local.transform_point3(*p);
            }
        }
        for c in children {
            let cl = self.nodes[&c].local_matrix();
            let nm = local * cl;
            let cn = self.nodes.get_mut(&c).unwrap();
            let jo = cn.joint_orient();
            cn.transform.set_from_matrix(nm, jo);
        }
        let n = self.nodes.get_mut(&id).unwrap();
        let ro = n.transform.rotate_order;
        n.transform = Transform { rotate_order: ro, ..Default::default() };
        self.touch();
    }

    /// Move the pivot to the center of the object's bounding box.
    pub fn center_pivot(&mut self, id: NodeId) {
        let Some(n) = self.nodes.get(&id) else { return };
        let b = match &n.kind {
            NodeKind::Mesh(ms) => ms.mesh.bounds(),
            NodeKind::Curve(c) => {
                let mut b = Aabb::EMPTY;
                for p in &c.cvs {
                    b.extend(*p);
                }
                b
            }
            _ => return,
        };
        if b.is_empty() {
            return;
        }
        let new_pivot = b.center();
        // keep the object in place: adjust translate for the pivot change
        let before = n.local_matrix();
        let n = self.nodes.get_mut(&id).unwrap();
        n.transform.pivot = new_pivot;
        let after = n.local_matrix();
        let delta = before.transform_point3(Vec3::ZERO) - after.transform_point3(Vec3::ZERO);
        n.transform.translate += delta;
        self.touch();
    }

    pub fn reset_transform(&mut self, id: NodeId) {
        if let Some(n) = self.nodes.get_mut(&id) {
            let p = n.transform.pivot;
            n.transform = Transform { pivot: p, ..Default::default() };
        }
        self.touch();
    }

    /// Combine meshes into a new node (world space). Originals are deleted.
    pub fn combine(&mut self, ids: &[NodeId]) -> Option<NodeId> {
        let mut out = Mesh::default();
        let mut mat = None;
        let mut angle: f32 = 0.0;
        for &id in ids {
            if let Some(m) = self.world_mesh(id) {
                out.append(&m);
                angle = angle.max(m.normal_angle);
                if mat.is_none() {
                    mat = self.nodes[&id].mesh().and_then(|m| m.material.clone());
                }
            }
        }
        if out.faces.is_empty() {
            return None;
        }
        out.normal_angle = angle;
        for &id in ids {
            self.delete(id);
        }
        let nid = self.add_mesh("polySurface", out);
        if let Some(m) = mat {
            let _ = self.assign_material(nid, &m);
        }
        Some(nid)
    }

    /// Separate a mesh into one node per connected shell.
    pub fn separate(&mut self, id: NodeId) -> Vec<NodeId> {
        let Some(n) = self.nodes.get(&id) else { return Vec::new() };
        let Some(ms) = n.mesh() else { return Vec::new() };
        let parts = ops::separate(&ms.mesh);
        if parts.len() < 2 {
            return vec![id];
        }
        let xf = n.transform.clone();
        let parent = n.parent;
        let mat = ms.material.clone();
        let grp = self.add_node(&n.name.clone(), NodeKind::Transform, parent);
        self.nodes.get_mut(&grp).unwrap().transform = xf;
        let mut out = Vec::new();
        for p in parts {
            let c = self.add_node("polySurface", NodeKind::Mesh(MeshShape::new(p)), Some(grp));
            if let Some(m) = &mat {
                let _ = self.assign_material(c, m);
            }
            out.push(c);
        }
        let name = self.name_of(id);
        self.delete(id);
        self.rename(grp, &name);
        out
    }

    /// Group nodes under a new transform (Ctrl+G).
    pub fn group(&mut self, ids: &[NodeId]) -> NodeId {
        let parent = ids.first().and_then(|i| self.nodes.get(i)).and_then(|n| n.parent);
        let g = self.add_node("group", NodeKind::Transform, parent);
        for &id in ids {
            let _ = self.set_parent(id, Some(g), true);
        }
        g
    }

    // ------------------------------------------------------------ rigging --

    /// Joint chain creation helper: positions in world space.
    pub fn create_joint_chain(&mut self, positions: &[Vec3], parent: Option<NodeId>) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut prev = parent;
        for &p in positions {
            let j = self.add_node("joint", NodeKind::Joint(JointShape::default()), prev);
            let pw = self.parent_world(j);
            let local = pw.inverse().transform_point3(p);
            self.nodes.get_mut(&j).unwrap().transform.translate = local;
            out.push(j);
            prev = Some(j);
        }
        if out.len() > 1 {
            self.orient_joints(out[0], Vec3::Y);
        }
        out
    }

    /// Orient joints so X aims at the (first) child, using `up` as secondary.
    /// Rotations are zeroed and baked into joint orient.
    pub fn orient_joints(&mut self, root: NodeId, up: Vec3) {
        let mut chain = vec![root];
        chain.extend(self.descendants(root));
        // record world positions first
        let positions: BTreeMap<NodeId, Vec3> = chain.iter().map(|&j| (j, self.world_position(j))).collect();
        for &j in &chain {
            let Some(n) = self.nodes.get(&j) else { continue };
            if !n.is_joint() {
                continue;
            }
            let child = n.children.iter().copied().find(|c| self.nodes.get(c).is_some_and(|x| x.is_joint()));
            let pw = self.parent_world(j);
            let world_rot = match child {
                Some(c) => {
                    let x = (positions[&c] - positions[&j]).normalize_or_zero();
                    if x == Vec3::ZERO {
                        None
                    } else {
                        let mut z = x.cross(up);
                        if z.length_squared() < 1e-8 {
                            z = x.cross(Vec3::Z);
                        }
                        let z = z.normalize();
                        let y = z.cross(x);
                        Some(Quat::from_mat3(&glam::Mat3::from_cols(x, y, z)))
                    }
                }
                None => None,
            };
            let parent_rot = pw.to_scale_rotation_translation().1;
            let local_rot = match world_rot {
                Some(r) => parent_rot.inverse() * r,
                None => Quat::IDENTITY, // leaf joints inherit the parent orientation
            };
            let p_local = pw.inverse().transform_point3(positions[&j]);
            let n = self.nodes.get_mut(&j).unwrap();
            n.transform.rotate = Vec3::ZERO;
            n.transform.translate = p_local;
            if let NodeKind::Joint(js) = &mut n.kind {
                js.orient = RotateOrder::XYZ.from_quat(local_rot, None);
            }
        }
        self.touch();
    }

    /// Chain of joints from start to end (inclusive), if end descends from start.
    pub fn joint_chain(&self, start: NodeId, end: NodeId) -> Option<Vec<NodeId>> {
        let mut chain = vec![end];
        let mut cur = end;
        while cur != start {
            cur = self.nodes.get(&cur)?.parent?;
            chain.push(cur);
        }
        chain.reverse();
        Some(chain)
    }

    pub fn create_ik_handle(&mut self, start: NodeId, end: NodeId, solver: IkSolver) -> Result<NodeId, String> {
        let chain = self.joint_chain(start, end).ok_or("end joint is not below start joint")?;
        if chain.len() < 2 {
            return Err("IK chain needs at least two joints".into());
        }
        let ps: Vec<Vec3> = chain.iter().map(|&j| self.world_position(j)).collect();
        let a = ps[0];
        let c = *ps.last().unwrap();
        // pole vector from the chain's bend
        let mid = ps[ps.len() / 2];
        let axis = (c - a).normalize_or_zero();
        let mut pole = (mid - a) - axis * (mid - a).dot(axis);
        if pole.length_squared() < 1e-8 {
            pole = if axis.cross(Vec3::Z).length_squared() > 1e-6 { axis.cross(Vec3::Z) } else { Vec3::Y };
        }
        let pole = pole.normalize();
        let h = self.add_node(
            "ikHandle",
            NodeKind::IkHandle(IkHandleShape {
                start_joint: start,
                end_joint: end,
                solver,
                pole_vector: pole,
                pole_target: None,
                twist: 0.0,
                ik_blend: 1.0,
            }),
            None,
        );
        self.nodes.get_mut(&h).unwrap().transform.translate = c;
        Ok(h)
    }

    /// Create a constraint, recording offsets from the current pose.
    pub fn add_constraint(&mut self, kind: ConstraintKind, targets: &[NodeId], driven: NodeId, maintain_offset: bool) -> Result<String, String> {
        if targets.is_empty() {
            return Err("constraint needs at least one target".into());
        }
        if targets.contains(&driven) {
            return Err("an object cannot constrain itself".into());
        }
        if kind == ConstraintKind::PoleVector {
            let target = targets[0];
            let tp = self.world_position(target);
            let n = self.nodes.get_mut(&driven).ok_or("no such node")?;
            let NodeKind::IkHandle(ik) = &mut n.kind else { return Err("poleVectorConstraint drives an ikHandle".into()) };
            ik.pole_target = Some(target);
            let _ = tp;
            let name = self.unique_name(&format!("{}_poleVectorConstraint", self.name_of(driven)));
            self.constraints.push(Constraint {
                name: name.clone(),
                kind,
                driven,
                targets: vec![target],
                weights: vec![1.0],
                offsets: vec![Mat4::IDENTITY],
                maintain_offset: false,
                aim_axis: Vec3::X,
                up_axis: Vec3::Y,
                world_up: Vec3::Y,
                enabled: 1.0,
            });
            self.touch();
            return Ok(name);
        }
        let ev = self.evaluate();
        let dw = ev.world(driven);
        let offsets = targets
            .iter()
            .map(|&t| {
                if !maintain_offset && kind != ConstraintKind::Aim {
                    return Mat4::IDENTITY;
                }
                let tw = ev.world(t);
                let (ts, tr, tt) = tw.to_scale_rotation_translation();
                let (ds, dr, dt) = dw.to_scale_rotation_translation();
                match kind {
                    ConstraintKind::Point => Mat4::from_translation(dt - tt),
                    ConstraintKind::Orient => Mat4::from_quat(tr.inverse() * dr),
                    ConstraintKind::Scale => Mat4::from_scale(ds / ts.max(Vec3::splat(1e-6))),
                    ConstraintKind::Aim => {
                        if maintain_offset {
                            let aim = crate::eval::aim_rotation(dt, tt, Vec3::X, Vec3::Y, Vec3::Y);
                            Mat4::from_quat(aim.inverse() * dr)
                        } else {
                            Mat4::IDENTITY
                        }
                    }
                    _ => tw.inverse() * dw,
                }
            })
            .collect();
        let name = self.unique_name(&format!("{}_{}", self.name_of(driven), kind.name()));
        self.constraints.push(Constraint {
            name: name.clone(),
            kind,
            driven,
            targets: targets.to_vec(),
            weights: vec![1.0; targets.len()],
            offsets,
            maintain_offset,
            aim_axis: Vec3::X,
            up_axis: Vec3::Y,
            world_up: Vec3::Y,
            enabled: 1.0,
        });
        self.touch();
        Ok(name)
    }

    /// Smooth bind: closest-bone weighting with falloff and max influences.
    pub fn bind_skin(&mut self, mesh_id: NodeId, joints: &[NodeId], max_influences: usize, dropoff: f32) -> Result<(), String> {
        if joints.is_empty() {
            return Err("select joints and a mesh".into());
        }
        let world = self.world_matrix(mesh_id);
        let mesh = self.nodes.get(&mesh_id).and_then(|n| n.mesh()).ok_or("not a mesh")?.mesh.clone();
        // bone segments: joint -> each joint child in the influence list (or point)
        let jpos: Vec<Vec3> = joints.iter().map(|&j| self.world_position(j)).collect();
        let mut segs: Vec<(usize, Vec3, Vec3)> = Vec::new();
        for (i, &j) in joints.iter().enumerate() {
            let kids: Vec<NodeId> = self.nodes[&j].children.iter().copied().filter(|c| self.nodes[c].is_joint()).collect();
            if kids.is_empty() {
                segs.push((i, jpos[i], jpos[i]));
            }
            for k in kids {
                segs.push((i, jpos[i], self.world_position(k)));
            }
        }
        let max_inf = max_influences.max(1);
        let mut weights = Vec::with_capacity(mesh.positions.len());
        for p in &mesh.positions {
            let wp = world.transform_point3(*p);
            let mut best: Vec<(u32, f32)> = vec![(0, f32::INFINITY); joints.len()];
            for b in best.iter_mut().enumerate() {
                b.1 .0 = b.0 as u32;
            }
            for &(i, a, b) in &segs {
                let d = point_segment_distance(wp, a, b);
                if d < best[i].1 {
                    best[i].1 = d;
                }
            }
            best.sort_by(|a, b| a.1.total_cmp(&b.1));
            best.truncate(max_inf);
            let dmin = best[0].1.max(1e-4);
            let mut w: Vec<(u32, f32)> =
                best.iter().map(|&(j, d)| (j, (dmin / d.max(1e-4)).powf(dropoff.max(0.1) * 2.0))).collect();
            w.retain(|x| x.1 > 1e-3);
            let s: f32 = w.iter().map(|x| x.1).sum();
            for x in &mut w {
                x.1 /= s;
            }
            weights.push(w);
        }
        let ev = self.evaluate();
        let inverse_bind = joints.iter().map(|&j| ev.world(j).inverse()).collect();
        let skin = SkinCluster { joints: joints.to_vec(), inverse_bind, geom_bind: world, weights, envelope: 1.0 };
        let ms = self.nodes.get_mut(&mesh_id).unwrap().mesh_mut().unwrap();
        ms.skin = Some(Arc::new(skin));
        self.touch();
        Ok(())
    }

    pub fn unbind_skin(&mut self, mesh_id: NodeId) {
        if let Some(ms) = self.nodes.get_mut(&mesh_id).and_then(|n| n.mesh_mut()) {
            ms.skin = None;
        }
        self.touch();
    }

    /// Add a blend shape target from another mesh with identical topology.
    pub fn add_blend_target(&mut self, base: NodeId, target: NodeId) -> Result<String, String> {
        let tmesh = self.nodes.get(&target).and_then(|n| n.mesh()).ok_or("target is not a mesh")?.mesh.clone();
        let tname = self.name_of(target);
        let bmesh = self.nodes.get(&base).and_then(|n| n.mesh()).ok_or("base is not a mesh")?.mesh.clone();
        if tmesh.positions.len() != bmesh.positions.len() {
            return Err("blendShape targets need matching vertex counts".into());
        }
        let deltas = tmesh.positions.iter().zip(&bmesh.positions).map(|(t, b)| *t - *b).collect();
        let ms = self.nodes.get_mut(&base).unwrap().mesh_mut().unwrap();
        ms.blend_shapes.push(BlendTarget { name: tname.clone(), deltas, weight: 0.0 });
        self.touch();
        Ok(tname)
    }

    pub fn add_deformer(&mut self, mesh_id: NodeId, kind: DeformerKind) -> Result<String, String> {
        let existing: usize = self
            .nodes
            .values()
            .filter_map(|n| n.mesh())
            .map(|m| m.deformers.iter().filter(|d| d.kind == kind).count())
            .sum();
        let name = format!("{}{}", kind.name(), existing + 1);
        let n = self.nodes.get_mut(&mesh_id).ok_or("no such node")?;
        let ms = n.mesh_mut().ok_or("not a mesh")?;
        let b = ms.mesh.bounds();
        let mut params = BTreeMap::new();
        for p in kind.params() {
            let v = match *p {
                "wavelength" => 2.0,
                "factor" | "startFlare" | "endFlare" => if *p == "factor" { 0.0 } else { 1.0 },
                _ => 0.0,
            };
            params.insert(p.to_string(), v);
        }
        ms.deformers.push(Deformer { name: name.clone(), kind, params, envelope: 1.0, low_bound: b.min.y, high_bound: b.max.y });
        self.touch();
        Ok(name)
    }

    // ------------------------------------------------------------- layers --

    pub fn create_layer(&mut self, name: &str, members: &[NodeId]) -> String {
        let mut base = name.to_string();
        let mut i = 1;
        while self.layers.iter().any(|l| l.name == base) {
            base = format!("{name}{i}");
            i += 1;
        }
        self.layers.push(DisplayLayer { name: base.clone(), visible: true, reference: false, color: None, members: members.to_vec() });
        self.touch();
        base
    }

    // ------------------------------------------------------------- bounds --

    pub fn world_bounds(&self, ids: &[NodeId], ev: &Evaluated) -> Aabb {
        let mut b = Aabb::EMPTY;
        for &id in ids {
            let Some(n) = self.nodes.get(&id) else { continue };
            let w = ev.world(id);
            match &n.kind {
                NodeKind::Mesh(_) => {
                    if let Some(m) = ev.mesh(id) {
                        for p in &m.positions {
                            b.extend(w.transform_point3(*p));
                        }
                    }
                }
                NodeKind::Curve(c) => {
                    for p in &c.cvs {
                        b.extend(w.transform_point3(*p));
                    }
                }
                _ => {
                    let p = w.transform_point3(Vec3::ZERO);
                    b.extend(p - Vec3::splat(0.5));
                    b.extend(p + Vec3::splat(0.5));
                }
            }
            // include children
            let kids = n.children.clone();
            let cb = self.world_bounds(&kids, ev);
            if !cb.is_empty() {
                b = b.union(&cb);
            }
        }
        b
    }

    pub fn evaluate(&self) -> Evaluated {
        crate::eval::evaluate(self)
    }

    /// Stable list of all node names in hierarchy order.
    pub fn ls(&self) -> Vec<String> {
        self.walk().into_iter().map(|id| self.name_of(id)).collect()
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }

    pub fn from_json(s: &str) -> Result<Scene, String> {
        let mut sc: Scene = serde_json::from_str(s).map_err(|e| e.to_string())?;
        sc.next_id = sc.nodes.keys().map(|k| k.0).max().unwrap_or(0) + 1;
        if !sc.materials.contains_key("lambert1") {
            sc.materials.insert("lambert1".into(), Material::default_lambert());
        }
        sc.touch();
        Ok(sc)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectMode {
    Replace,
    Add,
    Toggle,
    Remove,
}

pub fn point_segment_distance(p: Vec3, a: Vec3, b: Vec3) -> f32 {
    let ab = b - a;
    let l2 = ab.length_squared();
    if l2 < 1e-12 {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / l2).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

fn shape_get(kind: &NodeKind, a: &str) -> Option<f32> {
    match kind {
        NodeKind::Mesh(ms) => {
            if a == "smoothLevel" {
                return Some(ms.smooth_level as f32);
            }
            if a == "skinCluster.envelope" {
                return ms.skin.as_ref().map(|s| s.envelope);
            }
            if let Some(t) = a.strip_prefix("blendShape.") {
                return ms.blend_shapes.iter().find(|b| b.name == t).map(|b| b.weight);
            }
            if let Some((d, p)) = a.split_once('.') {
                let def = ms.deformers.iter().find(|x| x.name == d)?;
                if p == "envelope" {
                    return Some(def.envelope);
                }
                return def.params.get(p).copied();
            }
            None
        }
        NodeKind::Camera(c) => match a {
            "focalLength" => Some(c.focal_length),
            "nearClipPlane" => Some(c.near),
            "farClipPlane" => Some(c.far),
            "orthographicWidth" => Some(c.ortho_width),
            "orthographic" => Some(c.orthographic as u8 as f32),
            _ => None,
        },
        NodeKind::Light(l) => match a {
            "intensity" => Some(l.intensity),
            "colorR" => Some(l.color.x),
            "colorG" => Some(l.color.y),
            "colorB" => Some(l.color.z),
            "coneAngle" => Some(l.cone_angle),
            "penumbraAngle" => Some(l.penumbra),
            "decayRate" => Some(l.decay as f32),
            _ => None,
        },
        NodeKind::Joint(j) => match a {
            "jointOrientX" => Some(j.orient.x),
            "jointOrientY" => Some(j.orient.y),
            "jointOrientZ" => Some(j.orient.z),
            "radius" => Some(j.radius),
            _ => None,
        },
        NodeKind::IkHandle(ik) => match a {
            "twist" => Some(ik.twist),
            "ikBlend" => Some(ik.ik_blend),
            "poleVectorX" => Some(ik.pole_vector.x),
            "poleVectorY" => Some(ik.pole_vector.y),
            "poleVectorZ" => Some(ik.pole_vector.z),
            _ => None,
        },
        _ => None,
    }
}

pub(crate) fn shape_set(kind: &mut NodeKind, a: &str, v: f32) -> bool {
    match kind {
        NodeKind::Mesh(ms) => {
            if a == "smoothLevel" {
                ms.smooth_level = v.clamp(0.0, 4.0) as u32;
                return true;
            }
            if a == "skinCluster.envelope" {
                if let Some(s) = &mut ms.skin {
                    Arc::make_mut(s).envelope = v;
                    return true;
                }
                return false;
            }
            if let Some(t) = a.strip_prefix("blendShape.") {
                if let Some(b) = ms.blend_shapes.iter_mut().find(|b| b.name == t) {
                    b.weight = v;
                    return true;
                }
                return false;
            }
            if let Some((d, p)) = a.split_once('.') {
                if let Some(def) = ms.deformers.iter_mut().find(|x| x.name == d) {
                    if p == "envelope" {
                        def.envelope = v;
                        return true;
                    }
                    if let Some(x) = def.params.get_mut(p) {
                        *x = v;
                        return true;
                    }
                }
            }
            false
        }
        NodeKind::Camera(c) => {
            match a {
                "focalLength" => c.focal_length = v.max(1.0),
                "nearClipPlane" => c.near = v.max(1e-4),
                "farClipPlane" => c.far = v,
                "orthographicWidth" => c.ortho_width = v.max(1e-3),
                "orthographic" => c.orthographic = v >= 0.5,
                _ => return false,
            }
            true
        }
        NodeKind::Light(l) => {
            match a {
                "intensity" => l.intensity = v,
                "colorR" => l.color.x = v,
                "colorG" => l.color.y = v,
                "colorB" => l.color.z = v,
                "coneAngle" => l.cone_angle = v.clamp(0.1, 179.0),
                "penumbraAngle" => l.penumbra = v,
                "decayRate" => l.decay = v.clamp(0.0, 2.0) as u32,
                _ => return false,
            }
            true
        }
        NodeKind::Joint(j) => {
            match a {
                "jointOrientX" => j.orient.x = v,
                "jointOrientY" => j.orient.y = v,
                "jointOrientZ" => j.orient.z = v,
                "radius" => j.radius = v.max(0.01),
                _ => return false,
            }
            true
        }
        NodeKind::IkHandle(ik) => {
            match a {
                "twist" => ik.twist = v,
                "ikBlend" => ik.ik_blend = v.clamp(0.0, 1.0),
                "poleVectorX" => ik.pole_vector.x = v,
                "poleVectorY" => ik.pole_vector.y = v,
                "poleVectorZ" => ik.pole_vector.z = v,
                _ => return false,
            }
            true
        }
        _ => false,
    }
}

pub fn node_get_attr(n: &Node, attr: &str) -> Option<f64> {
        let a = long_attr_name(attr);
        let t = &n.transform;
        let v = match a {
            "translateX" => t.translate.x,
            "translateY" => t.translate.y,
            "translateZ" => t.translate.z,
            "rotateX" => t.rotate.x,
            "rotateY" => t.rotate.y,
            "rotateZ" => t.rotate.z,
            "scaleX" => t.scale.x,
            "scaleY" => t.scale.y,
            "scaleZ" => t.scale.z,
            "visibility" => {
                if n.visible {
                    1.0
                } else {
                    0.0
                }
            }
            "rotatePivotX" => t.pivot.x,
            "rotatePivotY" => t.pivot.y,
            "rotatePivotZ" => t.pivot.z,
            "rotateOrder" => RotateOrder::ALL.iter().position(|o| *o == t.rotate_order).unwrap_or(0) as f32,
            _ => {
                if let Some(v) = n.user_attrs.get(a) {
                    return Some(*v);
                }
                return shape_get(&n.kind, a).map(|v| v as f64);
            }
        };
        Some(v as f64)
    }

pub fn node_set_attr(n: &mut Node, attr: &str, value: f64, force: bool) -> Result<(), String> {
        let a = long_attr_name(attr).to_string();
        if !force && n.locked.contains(&a) {
            return Err(format!("{}.{} is locked", n.name, a));
        }
        let v = value as f32;
        let t = &mut n.transform;
        match a.as_str() {
            "translateX" => t.translate.x = v,
            "translateY" => t.translate.y = v,
            "translateZ" => t.translate.z = v,
            "rotateX" => t.rotate.x = v,
            "rotateY" => t.rotate.y = v,
            "rotateZ" => t.rotate.z = v,
            "scaleX" => t.scale.x = v,
            "scaleY" => t.scale.y = v,
            "scaleZ" => t.scale.z = v,
            "visibility" => n.visible = value >= 0.5,
            "rotatePivotX" => t.pivot.x = v,
            "rotatePivotY" => t.pivot.y = v,
            "rotatePivotZ" => t.pivot.z = v,
            "rotateOrder" => t.rotate_order = RotateOrder::ALL[(value as usize).min(5)],
            _ => {
                if let Some(u) = n.user_attrs.get_mut(&a) {
                    *u = value;
                } else if !shape_set(&mut n.kind, &a, v) {
                    return Err(format!("{} has no attribute '{}'", n.name, a));
                }
            }
        }
        Ok(())
}

