//! Scene commands with Maya/MEL names and flags.

use crate::interp::{Host, HostRequest, Interpreter};
use crate::value::Value;
use glam::{Mat4, Quat, Vec3};
use rg_core::anim::{Infinity, Tangent};
use rg_core::mesh::{ops, primitives, Edge, Mesh};
use rg_core::*;
use std::collections::BTreeSet;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub enum RawArg {
    Flag(String),
    Val(Value),
}

/// (command, one-line help)
pub const COMMANDS: &[(&str, &str)] = &[
    ("polyCube", "-w -h -d -sx -sy -sz -n  create a polygon cube"),
    ("polySphere", "-r -sa -sh -n  create a polygon sphere"),
    ("polyCylinder", "-r -h -sa -sh -rcp -n  create a polygon cylinder"),
    ("polyCone", "-r -h -sa -n  create a polygon cone"),
    ("polyTorus", "-r -sr -sa -sh -n  create a polygon torus"),
    ("polyPlane", "-w -h -sx -sy -n  create a polygon plane"),
    ("polyPipe", "-r -h -t -sa -n  create a polygon pipe"),
    ("polyPlatonicSolid", "-st 0..2 -r -n  tetrahedron/octahedron/icosahedron"),
    ("spaceLocator", "-p x y z -n  create a locator"),
    ("camera", "-fl -n  create a camera"),
    ("directionalLight", "-i -rgb r g b -n"),
    ("pointLight", "-i -rgb r g b -d decay -n"),
    ("spotLight", "-i -rgb -ca coneAngle -p penumbra -n"),
    ("areaLight", "-i -rgb -n"),
    ("ambientLight", "-i -rgb -n"),
    ("circle", "-r -nr x y z -s sections -n  NURBS circle (rig control)"),
    ("curve", "-d degree -p x y z (repeat) -per -n  create a curve"),
    ("joint", "-p x y z -n -rad  create a joint under the selected joint"),
    ("group", "-n -em  group selected objects"),
    ("select", "-r -add -tgl -d -cl -all [objects|components]"),
    ("ls", "-sl -type t -tr -l [patterns]  list nodes"),
    ("delete", "[objects|components]  delete objects or components"),
    ("duplicate", "-n name  duplicate objects"),
    ("instance", "duplicate (instances are copied)"),
    ("rename", "old new"),
    ("parent", "-w | child... parent  reparent objects"),
    ("move", "-r -a -os -ws x y z [objects]"),
    ("rotate", "-r -a -os -ws x y z [objects]"),
    ("scale", "-r -a x y z [objects]"),
    ("xform", "-q -t -ro -s -piv -ws -cp  query/set transforms"),
    ("makeIdentity", "-apply 1 -t 1 -r 1 -s 1  freeze transformations"),
    ("resetTransform", "reset translate/rotate/scale"),
    ("centerPivot", "center pivots"),
    ("setAttr", "-l -k -type obj.attr values..."),
    ("getAttr", "obj.attr  get an attribute value"),
    ("addAttr", "-ln name -at double -dv default [obj]"),
    ("listAttr", "-k [obj]  list keyable attributes"),
    ("connectAttr", "driver.attr driven.attr  (creates a 1:1 driven key)"),
    ("setKeyframe", "-at attr -t time -v value [objects]"),
    ("cutKey", "-at attr -t time|start:end -cl [objects]"),
    ("keyTangent", "-itt type -ott type -t time -at attr"),
    ("setInfinity", "-pri type -poi type -at attr"),
    ("keyframe", "-q -kc -tc  query keys; -e -r -tc offset to move keys"),
    ("bakeResults", "-t start:end -sb step [objects]"),
    ("setDrivenKeyframe", "-cd driver.attr -dv driverValue -v value driven.attr"),
    ("currentTime", "frame  |  -q"),
    ("playbackOptions", "-min -max -ast -aet -fps"),
    ("play", "-st true|false  start/stop playback"),
    ("parentConstraint", "-mo -w targets... driven"),
    ("pointConstraint", "-mo targets... driven"),
    ("orientConstraint", "-mo targets... driven"),
    ("scaleConstraint", "-mo targets... driven"),
    ("aimConstraint", "-mo -aim x y z -u x y z -wu x y z targets... driven"),
    ("poleVectorConstraint", "target ikHandle"),
    ("ikHandle", "-sj startJoint -ee endJoint -sol ikRPsolver|ikSCsolver -n"),
    ("joint_orient", "orient selected joint chains (X down the bone)"),
    ("skinCluster", "-mi maxInfluences -dr dropoff joints... mesh  smooth bind"),
    ("unbindSkin", "[mesh]  remove skin"),
    ("blendShape", "-n name targets... base"),
    ("nonLinear", "-type bend|twist|squash|flare|sine|wave [mesh]"),
    ("polyExtrudeFacet", "-ltz thickness -off offset -kft keepTogether [faces]"),
    ("polyExtrudeEdge", "-t x y z [edges]"),
    ("polyInset", "-off amount [faces]"),
    ("polySmooth", "-dv divisions [mesh]  Catmull-Clark subdivision"),
    ("polyBevel", "-o offset [mesh]  chamfer all edges"),
    ("polySplitRing", "-wt weight [edge]  insert edge loop"),
    ("polySelectEdgeLoop", "[edge]  select edge loop"),
    ("polySelectEdgeRing", "[edge]  select edge ring"),
    ("polyMergeVertex", "-d distance [mesh|vertices]"),
    ("polyMergeToCenter", "[components]"),
    ("polyBridgeEdge", "[border edges]  bridge two border loops"),
    ("polyCloseBorder", "[mesh|components]  fill holes"),
    ("polyTriangulate", "[mesh]"),
    ("polyQuad", "-a angle [mesh]"),
    ("polyPoke", "[faces]"),
    ("polyAverageVertex", "-i iterations [mesh|vertices]"),
    ("polyNormal", "-nm 0  reverse normals"),
    ("polySoftEdge", "-a angle [mesh]  normal smoothing angle"),
    ("polyMirrorFace", "-d axis(0..2) -p position -mt threshold [mesh]"),
    ("polyUnite", "-n name [meshes]  combine"),
    ("polySeparate", "[mesh]  separate shells"),
    ("polyChipOff", "-dup 0 [faces]  extract faces"),
    ("polyDelEdge", "[edges]  dissolve edges"),
    ("polyPlanarProjection", "-md x|y|z [mesh]  planar UVs"),
    ("polyCylindricalProjection", "[mesh]  cylindrical UVs"),
    ("polyEvaluate", "-v -e -f -t [mesh]  count components"),
    ("shadingNode", "-asShader lambert|blinn|phong|standardSurface|surfaceShader -n"),
    ("hyperShade", "-assign material  assign to selection"),
    ("sets", "-e -forceElement materialSG [objects]  assign material"),
    ("createDisplayLayer", "-n name [objects]"),
    ("hide", "[objects]"),
    ("showHidden", "-all [objects]"),
    ("listRelatives", "-c -p -ad [object]"),
    ("nodeType", "object"),
    ("objExists", "name"),
    ("file", "-new -f | -o path | -s | -rn path | -i path | -es path | -ea path"),
    ("render", "-x width -y height -s samples [camera] -o out.png  (-sf start -ef end -o img_####.png for sequences)"),
    ("viewFit", "-all  frame selection in the viewport"),
    ("setToolTo", "moveSuperContext|RotateSuperContext|scaleSuperContext|selectSuperContext"),
    ("selectMode", "-object | -co with selectType -v/-e/-f  component mode"),
    ("undo", ""),
    ("redo", ""),
    ("help", "[command]"),
    ("whatIs", "name"),
    ("source", "path.mel"),
];

fn zero_arity(cmd: &str, flag: &str) -> bool {
    const COMMON: &[&str] = &["q", "query", "e", "edit"];
    if COMMON.contains(&flag) {
        return true;
    }
    match cmd {
        "select" => ["r", "replace", "add", "tgl", "toggle", "d", "deselect", "cl", "clear", "all", "ne", "hi"].contains(&flag),
        "ls" => ["sl", "selection", "tr", "transforms", "l", "long", "dag", "sn", "shapes", "lights", "cameras", "materials"].contains(&flag),
        "move" | "rotate" | "scale" => ["r", "relative", "a", "absolute", "ws", "worldSpace", "os", "objectSpace", "ls", "cs", "wd"].contains(&flag),
        "xform" => ["ws", "worldSpace", "os", "objectSpace", "r", "relative", "a", "absolute", "cp", "centerPivots"].contains(&flag),
        "delete" => ["all", "ch", "constructionHistory"].contains(&flag),
        "duplicate" => ["rr", "returnRootsOnly", "un", "upstreamNodes"].contains(&flag),
        "parent" => ["w", "world", "r", "relative", "a", "absolute", "s", "shape", "add"].contains(&flag),
        "group" => ["em", "empty", "w", "world"].contains(&flag),
        "cutKey" | "keyframe" => ["cl", "clear", "kc", "keyframeCount", "tc", "timeChange", "vc", "valueChange", "r", "relative", "a", "absolute", "q", "e"].contains(&flag)
            && !(cmd == "keyframe" && (flag == "tc" || flag == "vc")),
        "currentTime" | "playbackOptions" => false,
        "parentConstraint" | "pointConstraint" | "orientConstraint" | "scaleConstraint" | "aimConstraint" => ["mo", "maintainOffset"].contains(&flag),
        "shadingNode" => ["asShader", "asTexture", "asUtility", "asLight"].contains(&flag),
        "file" => [
            "new", "f", "force", "o", "open", "s", "save", "i", "import", "es", "exportSelected", "ea", "exportAll", "sn", "sceneName",
        ]
        .contains(&flag),
        "sets" => ["e", "edit", "empty", "em"].contains(&flag),
        "showHidden" | "hide" => ["all", "a"].contains(&flag),
        "listRelatives" => ["c", "children", "p", "parent", "ad", "allDescendents", "s", "shapes", "f", "fullPath"].contains(&flag),
        "polyEvaluate" => ["v", "vertex", "e", "edge", "f", "face", "t", "triangle", "uv", "b", "boundingBox"].contains(&flag),
        "listAttr" => ["k", "keyable", "ud", "userDefined", "cb", "channelBox"].contains(&flag),
        "joint" => ["a", "absolute", "r", "relative"].contains(&flag),
        "curve" => ["per_"].contains(&flag),
        "skinCluster" => ["tsb", "toSelectedBones", "ub", "unbind"].contains(&flag),
        "viewFit" => ["all", "a"].contains(&flag),
        "selectMode" | "selectType" => true,
        "setKeyframe" => ["breakdown", "hierarchy"].contains(&flag),
        "polyChipOff" => false,
        "play" => ["f", "forward"].contains(&flag),
        _ => false,
    }
}

fn arity(cmd: &str, flag: &str) -> usize {
    if zero_arity(cmd, flag) {
        return 0;
    }
    match (cmd, flag) {
        (_, "p") | (_, "position") if ["joint", "spaceLocator", "curve"].contains(&cmd) => 3,
        ("polyMirrorFace", "p") => 1,
        ("spotLight", "p") => 1,
        (_, "rgb") | (_, "nr") | (_, "normal") | (_, "aim") | (_, "u") | (_, "wu") | (_, "worldUpVector") | (_, "c") | (_, "center") | (_, "ax") | (_, "axis") => 3,
        ("polyExtrudeEdge", "t") => 3,
        ("xform", "t") | ("xform", "translation") | ("xform", "ro") | ("xform", "rotation") | ("xform", "s") | ("xform", "scale") | ("xform", "piv") | ("xform", "pivots") | ("xform", "rp") | ("xform", "sp") => 3,
        ("blendShape", "w") => 2,
        _ => 1,
    }
}

#[derive(Default, Debug)]
pub struct Args {
    pub flags: Vec<(String, Vec<Value>)>,
    pub pos: Vec<Value>,
}

impl Args {
    pub fn has(&self, names: &[&str]) -> bool {
        self.flags.iter().any(|(f, _)| names.contains(&f.as_str()))
    }
    pub fn get(&self, names: &[&str]) -> Option<&Vec<Value>> {
        self.flags.iter().rev().find(|(f, _)| names.contains(&f.as_str())).map(|(_, v)| v)
    }
    pub fn all(&self, names: &[&str]) -> Vec<&Vec<Value>> {
        self.flags.iter().filter(|(f, _)| names.contains(&f.as_str())).map(|(_, v)| v).collect()
    }
    pub fn f(&self, names: &[&str], d: f64) -> f64 {
        self.get(names).and_then(|v| v.first()).map(|v| v.as_f64()).unwrap_or(d)
    }
    pub fn u(&self, names: &[&str], d: u32) -> u32 {
        self.f(names, d as f64).max(0.0) as u32
    }
    pub fn s(&self, names: &[&str]) -> Option<String> {
        self.get(names).and_then(|v| v.first()).map(|v| v.as_string())
    }
    pub fn b(&self, names: &[&str], d: bool) -> bool {
        match self.get(names) {
            None => d,
            Some(v) if v.is_empty() => true,
            Some(v) => v[0].truthy(),
        }
    }
    pub fn v3(&self, names: &[&str]) -> Option<Vec3> {
        self.get(names).filter(|v| v.len() >= 3).map(|v| Vec3::new(v[0].as_f64() as f32, v[1].as_f64() as f32, v[2].as_f64() as f32))
    }
    pub fn pos_strings(&self) -> Vec<String> {
        self.pos.iter().map(|v| v.as_string()).collect()
    }
    pub fn pos_nums(&self) -> Vec<f64> {
        self.pos.iter().filter(|v| v.is_numeric() || v.as_string().parse::<f64>().is_ok()).map(|v| v.as_f64()).collect()
    }
    pub fn pos_names(&self) -> Vec<String> {
        self.pos.iter().filter(|v| !v.is_numeric() && v.as_string().parse::<f64>().is_err()).map(|v| v.as_string()).collect()
    }
}

fn parse_args(cmd: &str, raw: Vec<RawArg>) -> Result<Args, String> {
    let mut a = Args::default();
    let query = raw.iter().any(|r| matches!(r, RawArg::Flag(f) if f == "q" || f == "query"));
    let mut it = raw.into_iter().peekable();
    while let Some(r) = it.next() {
        match r {
            RawArg::Flag(f) => {
                let n = if query && !["at", "attribute", "type"].contains(&f.as_str()) { 0 } else { arity(cmd, &f) };
                let mut vals = Vec::new();
                for _ in 0..n {
                    match it.next() {
                        Some(RawArg::Val(v)) => {
                            // a vector/array value can satisfy several slots
                            match v {
                                Value::Vector(x) if n == 3 && vals.is_empty() => {
                                    vals.extend(x.iter().map(|f| Value::Float(*f)));
                                    break;
                                }
                                v => vals.push(v),
                            }
                        }
                        _ => return Err(format!("flag -{f} expects {n} value(s)")),
                    }
                }
                if n == 0 {
                    if let Some(RawArg::Val(Value::Str(s))) = it.peek() {
                        if ["true", "false", "on", "off", "yes", "no"].contains(&s.as_str()) {
                            let s = s.clone();
                            it.next();
                            vals.push(Value::Int((s == "true" || s == "on" || s == "yes") as i64));
                        }
                    }
                }
                a.flags.push((f, vals));
            }
            RawArg::Val(v) => a.pos.extend(v.flatten()),
        }
    }
    Ok(a)
}

// ------------------------------------------------------------ helpers ---

pub fn glob_match(pat: &str, s: &str) -> bool {
    fn rec(p: &[char], s: &[char]) -> bool {
        match (p.first(), s.first()) {
            (None, None) => true,
            (Some('*'), _) => rec(&p[1..], s) || (!s.is_empty() && rec(p, &s[1..])),
            (Some('?'), Some(_)) => rec(&p[1..], &s[1..]),
            (Some(a), Some(b)) if a == b => rec(&p[1..], &s[1..]),
            _ => false,
        }
    }
    let p: Vec<char> = pat.chars().collect();
    let s: Vec<char> = s.chars().collect();
    rec(&p, &s)
}

#[derive(Debug, Clone)]
pub struct ComponentRef {
    pub node: NodeId,
    pub mode: ComponentMode,
    pub indices: Vec<u32>,
}

/// Parse "pCube1.f[0:3]" style component strings.
pub fn parse_component(scene: &Scene, s: &str) -> Option<ComponentRef> {
    let (obj, rest) = s.split_once('.')?;
    let open = rest.find('[')?;
    let kind = &rest[..open];
    let inner = rest[open + 1..].trim_end_matches(']');
    let node = scene.find(obj)?;
    let mesh = scene.nodes[&node].mesh()?.mesh.clone();
    let (mode, count) = match kind {
        "vtx" | "v" => (ComponentMode::Vertex, mesh.positions.len()),
        "e" => (ComponentMode::Edge, mesh.edges().len()),
        "f" => (ComponentMode::Face, mesh.faces.len()),
        _ => return None,
    };
    let indices: Vec<u32> = if inner == "*" {
        (0..count as u32).collect()
    } else if let Some((a, b)) = inner.split_once(':') {
        let a: u32 = a.parse().ok()?;
        let b: u32 = b.parse().ok()?;
        (a..=b.min(count.saturating_sub(1) as u32)).collect()
    } else {
        vec![inner.parse().ok()?]
    };
    Some(ComponentRef { node, mode, indices })
}

pub fn component_strings(scene: &Scene) -> Vec<String> {
    let c = &scene.components;
    let Some(node) = c.node else { return Vec::new() };
    let name = scene.name_of(node);
    let mut out = Vec::new();
    match c.mode {
        ComponentMode::Vertex => out.extend(c.verts.iter().map(|i| format!("{name}.vtx[{i}]"))),
        ComponentMode::Face => out.extend(c.faces.iter().map(|i| format!("{name}.f[{i}]"))),
        ComponentMode::Edge => {
            if let Some(m) = scene.nodes.get(&node).and_then(|n| n.mesh()) {
                let edges = m.mesh.edges();
                for e in &c.edges {
                    if let Some(i) = edges.iter().position(|x| x == e) {
                        out.push(format!("{name}.e[{i}]"));
                    }
                }
            }
        }
        ComponentMode::Object => {}
    }
    out
}

fn resolve(scene: &Scene, names: &[String]) -> Result<Vec<NodeId>, String> {
    let mut out = Vec::new();
    for n in names {
        if n.contains('*') || n.contains('?') {
            for id in scene.walk() {
                if glob_match(n, &scene.nodes[&id].name) && !out.contains(&id) {
                    out.push(id);
                }
            }
            continue;
        }
        let obj = n.split('.').next().unwrap_or(n);
        match scene.find(obj) {
            Some(id) => {
                if !out.contains(&id) {
                    out.push(id);
                }
            }
            None => return Err(format!("no object matches name: {n}")),
        }
    }
    Ok(out)
}

/// Objects named on the command line, or the selection.
fn targets(host: &Host, a: &Args) -> Result<Vec<NodeId>, String> {
    let names = a.pos_names();
    if names.is_empty() {
        let sel = host.scene.selected();
        if sel.is_empty() {
            if let Some(n) = host.scene.components.node {
                return Ok(vec![n]);
            }
        }
        Ok(sel)
    } else {
        resolve(host.scene, &names)
    }
}

fn need_targets(host: &Host, a: &Args) -> Result<Vec<NodeId>, String> {
    let t = targets(host, a)?;
    if t.is_empty() {
        Err("nothing selected".into())
    } else {
        Ok(t)
    }
}

/// Apply component strings among positional args to the component selection.
fn select_components_from_args(host: &mut Host, a: &Args) -> bool {
    let comps: Vec<ComponentRef> = a.pos_names().iter().filter_map(|s| parse_component(host.scene, s)).collect();
    if comps.is_empty() {
        return false;
    }
    apply_components(host.scene, &comps, rg_core::SelectMode::Replace);
    true
}

fn apply_components(scene: &mut Scene, comps: &[ComponentRef], mode: SelectMode) {
    let first = &comps[0];
    if scene.components.node != Some(first.node) || scene.components.mode != first.mode || mode == SelectMode::Replace {
        scene.components.node = Some(first.node);
        scene.components.mode = first.mode;
        scene.components.clear();
    }
    let edges = scene.nodes.get(&first.node).and_then(|n| n.mesh()).map(|m| m.mesh.edges()).unwrap_or_default();
    for c in comps.iter().filter(|c| c.node == first.node && c.mode == first.mode) {
        for &i in &c.indices {
            let add = mode != SelectMode::Remove;
            let toggle = mode == SelectMode::Toggle;
            match c.mode {
                ComponentMode::Vertex => set_member(&mut scene.components.verts, i, add, toggle),
                ComponentMode::Face => set_member(&mut scene.components.faces, i, add, toggle),
                ComponentMode::Edge => {
                    if let Some(e) = edges.get(i as usize) {
                        set_member(&mut scene.components.edges, *e, add, toggle)
                    }
                }
                ComponentMode::Object => {}
            }
        }
    }
    if !scene.selection.contains(&first.node) {
        scene.selection = vec![first.node];
    }
    scene.touch();
}

fn set_member<T: Ord + Copy>(set: &mut BTreeSet<T>, v: T, add: bool, toggle: bool) {
    if toggle {
        if !set.remove(&v) {
            set.insert(v);
        }
    } else if add {
        set.insert(v);
    } else {
        set.remove(&v);
    }
}

fn split_attr(scene: &Scene, s: &str) -> Result<(NodeId, String), String> {
    let (obj, attr) = s.split_once('.').ok_or_else(|| format!("expected object.attribute, got '{s}'"))?;
    let id = scene.find(obj).ok_or_else(|| format!("no object '{obj}'"))?;
    Ok((id, long_attr_name(attr).to_string()))
}

fn ret_names(scene: &Scene, ids: &[NodeId]) -> Value {
    Value::strings(ids.iter().map(|i| scene.name_of(*i)).collect())
}

fn create_shape(host: &mut Host, a: &Args, default: &str, kind: NodeKind) -> Value {
    let name = a.s(&["n", "name"]).unwrap_or_else(|| default.to_string());
    let id = host.scene.add_node(&name, kind, None);
    if a.s(&["n", "name"]).is_some() {
        host.scene.rename(id, &name);
    }
    host.scene.select(&[id], SelectMode::Replace);
    host.scene.components = ComponentSelection::default();
    ret_names(host.scene, &[id])
}

fn mesh_target(host: &Host, a: &Args) -> Result<NodeId, String> {
    let names = a.pos_names();
    for n in &names {
        if let Some(c) = parse_component(host.scene, n) {
            return Ok(c.node);
        }
    }
    let t = targets(host, a)?;
    t.into_iter()
        .find(|id| host.scene.nodes[id].mesh().is_some())
        .ok_or_else(|| "select a polygon mesh".to_string())
}

fn sel_faces(host: &mut Host, a: &Args, node: NodeId) -> Vec<u32> {
    select_components_from_args(host, a);
    let c = &host.scene.components;
    if c.node == Some(node) && c.mode == ComponentMode::Face && !c.faces.is_empty() {
        c.faces.iter().copied().collect()
    } else {
        let n = host.scene.nodes[&node].mesh().map(|m| m.mesh.faces.len()).unwrap_or(0);
        (0..n as u32).collect()
    }
}

fn sel_edges(host: &mut Host, a: &Args, node: NodeId) -> Vec<Edge> {
    select_components_from_args(host, a);
    let c = &host.scene.components;
    if c.node == Some(node) && c.mode == ComponentMode::Edge {
        c.edges.iter().copied().collect()
    } else {
        Vec::new()
    }
}

fn sel_verts(host: &mut Host, a: &Args, node: NodeId) -> Option<Vec<u32>> {
    select_components_from_args(host, a);
    let c = &host.scene.components;
    if c.node == Some(node) && c.mode != ComponentMode::Object && !c.is_empty() {
        let mesh = host.scene.nodes[&node].mesh()?.mesh.clone();
        Some(c.affected_verts(&mesh).into_iter().collect())
    } else {
        None
    }
}

fn tangent_parse(s: &str) -> Result<Tangent, String> {
    Tangent::parse(s).or(match s {
        "smooth" => Some(Tangent::Spline),
        "fixed" => Some(Tangent::Fixed),
        "plateau" => Some(Tangent::Auto),
        "stepnext" => Some(Tangent::Stepped),
        _ => None,
    })
    .ok_or_else(|| format!("unknown tangent type '{s}'"))
}

fn time_range(v: &Value) -> (f64, f64) {
    let s = v.as_string();
    if let Some((a, b)) = s.split_once(':') {
        let a = a.trim().parse().unwrap_or(f64::NEG_INFINITY);
        let b = b.trim().parse().unwrap_or(f64::INFINITY);
        (a, b)
    } else {
        let t = v.as_f64();
        (t, t)
    }
}

/// Translate the world-space pivot of a node by `delta` (world).
fn move_world(scene: &mut Scene, id: NodeId, delta: Vec3) {
    let pw = scene.parent_world(id);
    let local = pw.inverse().transform_vector3(delta);
    if let Some(n) = scene.node_mut(id) {
        n.transform.translate += local;
    }
}

// ----------------------------------------------------------- dispatch ---

pub fn dispatch(interp: &mut Interpreter, host: &mut Host, name: &str, raw: Vec<RawArg>) -> Result<Value, String> {
    let a = parse_args(name, raw)?;
    let empty = Value::Str(String::new());
    let q = a.has(&["q", "query"]);
    match name {
        // ------------------------------------------------ creation
        "polyCube" => {
            let m = primitives::cube(a.f(&["w", "width"], 1.0) as f32, a.f(&["h", "height"], 1.0) as f32, a.f(&["d", "depth"], 1.0) as f32,
                a.u(&["sx", "subdivisionsX", "sw"], 1), a.u(&["sy", "subdivisionsY", "sh"], 1), a.u(&["sz", "subdivisionsZ", "sd"], 1));
            Ok(create_shape(host, &a, "pCube", NodeKind::Mesh(MeshShape::new(m))))
        }
        "polySphere" => {
            let m = primitives::sphere(a.f(&["r", "radius"], 1.0) as f32, a.u(&["sa", "subdivisionsAxis", "sx"], 20), a.u(&["sh", "subdivisionsHeight", "sy"], 20));
            Ok(create_shape(host, &a, "pSphere", NodeKind::Mesh(MeshShape::new(m))))
        }
        "polyCylinder" => {
            let m = primitives::cylinder(a.f(&["r", "radius"], 1.0) as f32, a.f(&["h", "height"], 2.0) as f32, a.u(&["sa", "sx", "subdivisionsAxis"], 20), a.u(&["sh", "sy", "subdivisionsHeight"], 1), true);
            Ok(create_shape(host, &a, "pCylinder", NodeKind::Mesh(MeshShape::new(m))))
        }
        "polyCone" => {
            let m = primitives::cone(a.f(&["r", "radius"], 1.0) as f32, a.f(&["h", "height"], 2.0) as f32, a.u(&["sa", "sx", "subdivisionsAxis"], 20));
            Ok(create_shape(host, &a, "pCone", NodeKind::Mesh(MeshShape::new(m))))
        }
        "polyTorus" => {
            let m = primitives::torus(a.f(&["r", "radius"], 1.0) as f32, a.f(&["sr", "sectionRadius"], 0.5) as f32, a.u(&["sa", "sx", "subdivisionsAxis"], 20), a.u(&["sh", "sy", "subdivisionsHeight"], 20));
            Ok(create_shape(host, &a, "pTorus", NodeKind::Mesh(MeshShape::new(m))))
        }
        "polyPlane" => {
            let m = primitives::plane(a.f(&["w", "width"], 1.0) as f32, a.f(&["h", "height"], 1.0) as f32, a.u(&["sx", "subdivisionsX", "sw"], 10), a.u(&["sy", "subdivisionsY", "sh"], 10));
            Ok(create_shape(host, &a, "pPlane", NodeKind::Mesh(MeshShape::new(m))))
        }
        "polyPipe" => {
            let m = primitives::pipe(a.f(&["r", "radius"], 1.0) as f32, a.f(&["h", "height"], 2.0) as f32, a.f(&["t", "thickness"], 0.5) as f32, a.u(&["sa", "subdivisionsAxis"], 20));
            Ok(create_shape(host, &a, "pPipe", NodeKind::Mesh(MeshShape::new(m))))
        }
        "polyPlatonicSolid" => {
            let kind = match a.u(&["st", "solidType"], 2) {
                0 => "tetrahedron",
                1 => "octahedron",
                _ => "icosahedron",
            };
            let m = primitives::platonic(kind, a.f(&["r", "radius"], 1.0) as f32);
            Ok(create_shape(host, &a, "pSolid", NodeKind::Mesh(MeshShape::new(m))))
        }
        "spaceLocator" => {
            let v = create_shape(host, &a, "locator", NodeKind::Locator);
            if let Some(p) = a.v3(&["p", "position"]) {
                let id = host.scene.selected()[0];
                host.scene.node_mut(id).unwrap().transform.translate = p;
            }
            Ok(v)
        }
        "camera" => {
            let c = CameraShape { focal_length: a.f(&["fl", "focalLength"], 35.0) as f32, ..Default::default() };
            Ok(create_shape(host, &a, "camera", NodeKind::Camera(c)))
        }
        "directionalLight" | "pointLight" | "spotLight" | "areaLight" | "ambientLight" => {
            let kind = match name {
                "directionalLight" => LightType::Directional,
                "pointLight" => LightType::Point,
                "spotLight" => LightType::Spot,
                "areaLight" => LightType::Area,
                _ => LightType::Ambient,
            };
            let mut l = LightShape::new(kind);
            l.intensity = a.f(&["i", "intensity"], 1.0) as f32;
            if let Some(c) = a.v3(&["rgb"]) {
                l.color = c;
            }
            l.cone_angle = a.f(&["ca", "coneAngle"], 40.0) as f32;
            l.penumbra = a.f(&["p", "penumbra"], 0.0) as f32;
            l.decay = a.u(&["d", "decayRate"], 0);
            Ok(create_shape(host, &a, name, NodeKind::Light(l)))
        }
        "circle" => {
            let nr = a.v3(&["nr", "normal"]).unwrap_or(Vec3::Y);
            let axis = if nr.x.abs() > 0.5 { 0 } else if nr.z.abs() > 0.5 { 2 } else { 1 };
            let c = CurveShape::circle(a.f(&["r", "radius"], 1.0) as f32, a.u(&["s", "sections"], 8) as usize, axis);
            let v = create_shape(host, &a, "nurbsCircle", NodeKind::Curve(c));
            if let Some(cen) = a.v3(&["c", "center"]) {
                let id = host.scene.selected()[0];
                host.scene.node_mut(id).unwrap().transform.translate = cen;
            }
            Ok(v)
        }
        "curve" => {
            let pts: Vec<Vec3> = a.all(&["p", "point"]).iter().map(|v| Vec3::new(v[0].as_f64() as f32, v[1].as_f64() as f32, v[2].as_f64() as f32)).collect();
            if pts.len() < 2 {
                return Err("curve needs at least two -p points".into());
            }
            let c = CurveShape { cvs: pts, degree: a.u(&["d", "degree"], 3), periodic: a.b(&["per", "periodic"], false) };
            Ok(create_shape(host, &a, "curve", NodeKind::Curve(c)))
        }
        "joint" => {
            let parent = host.scene.selected().into_iter().rev().find(|id| host.scene.nodes[id].is_joint());
            let p = a.v3(&["p", "position"]).unwrap_or(Vec3::ZERO);
            let nm = a.s(&["n", "name"]).unwrap_or_else(|| "joint".into());
            let ids = host.scene.create_joint_chain(&[p], parent);
            let id = ids[0];
            if a.s(&["n", "name"]).is_some() {
                host.scene.rename(id, &nm);
            }
            if let Some(r) = a.get(&["rad", "radius"]) {
                let _ = host.scene.set_attr(id, "radius", r[0].as_f64());
            }
            if let Some(par) = parent {
                // re-orient the parent toward its new child
                let _ = par;
            }
            host.scene.select(&[id], SelectMode::Replace);
            Ok(ret_names(host.scene, &[id]))
        }
        "joint_orient" => {
            for id in need_targets(host, &a)? {
                if host.scene.nodes[&id].is_joint() {
                    host.scene.orient_joints(id, Vec3::Y);
                }
            }
            Ok(empty)
        }
        "group" => {
            let ids = if a.has(&["em", "empty"]) { Vec::new() } else { targets(host, &a)? };
            let g = host.scene.group(&ids);
            if let Some(n) = a.s(&["n", "name"]) {
                host.scene.rename(g, &n);
            }
            host.scene.select(&[g], SelectMode::Replace);
            Ok(ret_names(host.scene, &[g]))
        }

        // ------------------------------------------------ selection / listing
        "select" => {
            let mode = if a.has(&["add"]) {
                SelectMode::Add
            } else if a.has(&["tgl", "toggle"]) {
                SelectMode::Toggle
            } else if a.has(&["d", "deselect"]) {
                SelectMode::Remove
            } else {
                SelectMode::Replace
            };
            if a.has(&["cl", "clear"]) {
                host.scene.select(&[], SelectMode::Replace);
                host.scene.components = ComponentSelection { mode: host.scene.components.mode, ..Default::default() };
                return Ok(empty);
            }
            if a.has(&["all"]) {
                let all: Vec<NodeId> = host.scene.roots.iter().copied().filter(|id| host.scene.is_selectable(*id)).collect();
                host.scene.select(&all, SelectMode::Replace);
                return Ok(empty);
            }
            let names = a.pos_names();
            let comps: Vec<ComponentRef> = names.iter().filter_map(|s| parse_component(host.scene, s)).collect();
            if !comps.is_empty() {
                apply_components(host.scene, &comps, mode);
                return Ok(empty);
            }
            let ids = resolve(host.scene, &names)?;
            let ids = if a.has(&["hi"]) {
                let mut all = ids.clone();
                for id in &ids {
                    all.extend(host.scene.descendants(*id));
                }
                all
            } else {
                ids
            };
            host.scene.select(&ids, mode);
            if mode == SelectMode::Replace {
                host.scene.components = ComponentSelection::default();
            }
            Ok(empty)
        }
        "ls" => {
            let sc = &*host.scene;
            let mut ids: Vec<NodeId> = if a.has(&["sl", "selection"]) {
                let comps = component_strings(sc);
                if !comps.is_empty() {
                    return Ok(Value::strings(comps));
                }
                sc.selected()
            } else {
                sc.walk()
            };
            let pats = a.pos_names();
            if !pats.is_empty() {
                ids.retain(|id| pats.iter().any(|p| glob_match(p, &sc.nodes[id].name)));
            }
            if let Some(t) = a.s(&["type", "typ"]) {
                ids.retain(|id| sc.nodes[id].kind.type_name() == t || (t == "light" && matches!(sc.nodes[id].kind, NodeKind::Light(_))));
            }
            if a.has(&["lights"]) {
                ids.retain(|id| matches!(sc.nodes[id].kind, NodeKind::Light(_)));
            }
            if a.has(&["cameras"]) {
                ids.retain(|id| matches!(sc.nodes[id].kind, NodeKind::Camera(_)));
            }
            if a.has(&["materials", "mat"]) {
                return Ok(Value::strings(sc.materials.keys().cloned().collect()));
            }
            let long = a.has(&["l", "long"]);
            Ok(Value::strings(ids.iter().map(|id| if long { sc.path(*id) } else { sc.name_of(*id) }).collect()))
        }
        "listRelatives" => {
            let id = *need_targets(host, &a)?.first().unwrap();
            let sc = &*host.scene;
            let ids: Vec<NodeId> = if a.has(&["p", "parent"]) {
                sc.nodes[&id].parent.into_iter().collect()
            } else if a.has(&["ad", "allDescendents"]) {
                sc.descendants(id)
            } else {
                sc.nodes[&id].children.clone()
            };
            let ids: Vec<NodeId> = match a.s(&["type"]) {
                Some(t) => ids.into_iter().filter(|i| sc.nodes[i].kind.type_name() == t).collect(),
                None => ids,
            };
            Ok(ret_names(sc, &ids))
        }
        "nodeType" | "objectType" => {
            let n = a.pos_names().first().cloned().ok_or("nodeType needs an object")?;
            if host.scene.materials.contains_key(&n) {
                return Ok(Value::Str(host.scene.materials[&n].shader.name().into()));
            }
            let id = host.scene.find(&n).ok_or_else(|| format!("no object '{n}'"))?;
            Ok(Value::Str(host.scene.nodes[&id].kind.type_name().into()))
        }
        "objExists" => {
            let n = a.pos.first().map(|v| v.as_string()).unwrap_or_default();
            let obj = n.split('.').next().unwrap_or(&n);
            Ok(Value::Int((host.scene.find(obj).is_some() || host.scene.materials.contains_key(obj)) as i64))
        }

        // ------------------------------------------------ edit
        "delete" => {
            let names = a.pos_names();
            let has_comp = names.iter().any(|n| parse_component(host.scene, n).is_some());
            if has_comp || (names.is_empty() && !host.scene.components.is_empty() && host.scene.components.mode != ComponentMode::Object) {
                if has_comp {
                    select_components_from_args(host, &a);
                }
                let c = host.scene.components.clone();
                let node = c.node.ok_or("no components selected")?;
                match c.mode {
                    ComponentMode::Face => {
                        let f: Vec<u32> = c.faces.iter().copied().collect();
                        host.scene.edit_mesh(node, |m| ops::delete_faces(m, &f));
                    }
                    ComponentMode::Vertex => {
                        let v: Vec<u32> = c.verts.iter().copied().collect();
                        host.scene.edit_mesh(node, |m| ops::delete_vertices(m, &v));
                    }
                    ComponentMode::Edge => {
                        let e: Vec<Edge> = c.edges.iter().copied().collect();
                        host.scene.edit_mesh(node, |m| ops::dissolve_edges(m, &e));
                    }
                    ComponentMode::Object => {}
                }
                host.scene.components.clear();
                return Ok(empty);
            }
            if a.has(&["ch", "constructionHistory"]) {
                return Ok(empty); // no construction history to delete
            }
            let ids = if a.has(&["all"]) { host.scene.roots.clone() } else { need_targets(host, &a)? };
            for id in ids {
                host.scene.delete(id);
            }
            Ok(empty)
        }
        "duplicate" | "instance" => {
            let ids = need_targets(host, &a)?;
            let mut out = Vec::new();
            for id in ids {
                if let Some(d) = host.scene.duplicate(id) {
                    out.push(d);
                }
            }
            if let (Some(n), Some(&first)) = (a.s(&["n", "name"]), out.first()) {
                host.scene.rename(first, &n);
            }
            host.scene.select(&out, SelectMode::Replace);
            Ok(ret_names(host.scene, &out))
        }
        "rename" => {
            let names = a.pos_strings();
            let (old, new) = match names.len() {
                1 => (host.scene.selected().first().copied().ok_or("nothing selected")?, names[0].clone()),
                2 => (host.scene.find(&names[0]).ok_or_else(|| format!("no object '{}'", names[0]))?, names[1].clone()),
                _ => return Err("rename [old] new".into()),
            };
            Ok(Value::Str(host.scene.rename(old, &new)))
        }
        "parent" => {
            let ids = need_targets(host, &a)?;
            if a.has(&["w", "world"]) {
                for id in ids {
                    host.scene.set_parent(id, None, !a.has(&["r", "relative"]))?;
                }
                return Ok(empty);
            }
            if ids.len() < 2 {
                return Err("select children then the parent".into());
            }
            let parent = *ids.last().unwrap();
            for &c in &ids[..ids.len() - 1] {
                host.scene.set_parent(c, Some(parent), !a.has(&["r", "relative"]))?;
            }
            Ok(ret_names(host.scene, &ids[..ids.len() - 1]))
        }
        "move" | "rotate" | "scale" => transform_cmd(host, name, &a),
        "xform" => xform(host, &a, q),
        "makeIdentity" => {
            for id in need_targets(host, &a)? {
                if a.b(&["apply", "a"], false) {
                    host.scene.freeze_transforms(id);
                } else {
                    host.scene.reset_transform(id);
                }
            }
            Ok(empty)
        }
        "resetTransform" => {
            for id in need_targets(host, &a)? {
                host.scene.reset_transform(id);
            }
            Ok(empty)
        }
        "centerPivot" | "CenterPivot" => {
            for id in need_targets(host, &a)? {
                host.scene.center_pivot(id);
            }
            Ok(empty)
        }
        "hide" => {
            let ids = if a.has(&["all"]) { host.scene.roots.clone() } else { need_targets(host, &a)? };
            for id in ids {
                let _ = host.scene.set_attr(id, "visibility", 0.0);
            }
            Ok(empty)
        }
        "showHidden" => {
            let ids = if a.has(&["all", "a"]) || a.pos_names().is_empty() && host.scene.selection.is_empty() {
                host.scene.nodes.keys().copied().collect()
            } else {
                need_targets(host, &a)?
            };
            for id in ids {
                let _ = host.scene.set_attr(id, "visibility", 1.0);
            }
            Ok(empty)
        }

        // ------------------------------------------------ attributes
        "setAttr" => {
            let names = a.pos_names();
            let target = names.first().ok_or("setAttr object.attr value")?.clone();
            let vals = a.pos_nums();
            let bool_val = a.pos.iter().skip(1).find_map(|v| match v.as_string().as_str() {
                "true" | "on" | "yes" => Some(1.0),
                "false" | "off" | "no" => Some(0.0),
                _ => None,
            });
            let (obj, attr) = target.split_once('.').ok_or("expected object.attribute")?;
            // render globals pseudo-nodes
            if obj == "defaultResolution" || obj == "defaultRenderGlobals" {
                let v = vals.first().copied().unwrap_or(0.0);
                match attr {
                    "width" | "w" => host.scene.render.width = v as u32,
                    "height" | "h" => host.scene.render.height = v as u32,
                    "samples" => host.scene.render.samples = v as u32,
                    "maxBounces" | "bounces" => host.scene.render.max_bounces = v as u32,
                    "sky" => host.scene.render.sky = v != 0.0,
                    _ => return Err(format!("unknown render setting {attr}")),
                }
                host.scene.touch();
                return Ok(empty);
            }
            if host.scene.materials.contains_key(obj) {
                return set_material_attr(host.scene, obj, attr, &vals, &a);
            }
            let id = host.scene.find(obj).ok_or_else(|| format!("no object '{obj}'"))?;
            if let Some(l) = a.get(&["l", "lock"]) {
                let lock = l.first().map(|v| v.truthy()).unwrap_or(true);
                let n = host.scene.node_mut(id).unwrap();
                for at in expand_attr(attr) {
                    if lock {
                        n.locked.insert(at);
                    } else {
                        n.locked.remove(&at);
                    }
                }
                if vals.is_empty() {
                    return Ok(empty);
                }
            }
            let attrs = expand_attr(attr);
            let vals: Vec<f64> = if vals.is_empty() { bool_val.into_iter().collect() } else { vals };
            if vals.is_empty() {
                if a.has(&["k", "keyable", "l", "lock", "cb"]) {
                    return Ok(empty);
                }
                return Err(format!("setAttr {target}: no value given"));
            }
            for (i, at) in attrs.iter().enumerate() {
                let v = vals.get(i).or(vals.last()).copied().unwrap();
                host.scene.set_attr(id, at, v)?;
            }
            Ok(empty)
        }
        "getAttr" => {
            let target = a.pos_names().first().cloned().ok_or("getAttr object.attr")?;
            let (obj, attr) = target.split_once('.').ok_or("expected object.attribute")?;
            if let Some(m) = host.scene.materials.get(obj) {
                return m.get(attr).map(|v| Value::Float(v as f64)).ok_or_else(|| format!("no attribute {attr}"));
            }
            let id = host.scene.find(obj).ok_or_else(|| format!("no object '{obj}'"))?;
            if let Some(t) = a.get(&["t", "time"]) {
                let t = t[0].as_f64();
                let n = &host.scene.nodes[&id];
                let at = long_attr_name(attr);
                if let Some(c) = n.anim.get(at) {
                    return Ok(Value::Float(c.evaluate(t)));
                }
            }
            let attrs = expand_attr(attr);
            let ev = host.scene.evaluate();
            let vals: Vec<f64> = attrs.iter().filter_map(|at| ev.attr(host.scene, id, at)).collect();
            if attr == "worldMatrix" || attr == "wm" {
                return Ok(Value::Array(ev.world(id).to_cols_array().iter().map(|v| Value::Float(*v as f64)).collect()));
            }
            match vals.len() {
                0 => Err(format!("no attribute '{attr}' on {obj}")),
                1 => Ok(Value::Float(vals[0])),
                _ => Ok(Value::Array(vals.into_iter().map(Value::Float).collect())),
            }
        }
        "addAttr" => {
            let ln = a.s(&["ln", "longName", "sn", "shortName"]).ok_or("addAttr -ln name")?;
            for id in need_targets(host, &a)? {
                host.scene.add_user_attr(id, &ln, a.f(&["dv", "defaultValue"], 0.0))?;
            }
            Ok(empty)
        }
        "listAttr" => {
            let id = *need_targets(host, &a)?.first().unwrap();
            let mut v = host.scene.keyable_attrs(id);
            if a.has(&["ud", "userDefined"]) {
                v = host.scene.nodes[&id].user_attrs.keys().cloned().collect();
            } else if !a.has(&["k", "keyable"]) {
                v.extend(host.scene.shape_attrs(id));
            }
            Ok(Value::strings(v))
        }
        "connectAttr" => {
            let names = a.pos_names();
            if names.len() < 2 {
                return Err("connectAttr src.attr dst.attr".into());
            }
            let (d, da) = split_attr(host.scene, &names[0])?;
            let (t, ta) = split_attr(host.scene, &names[1])?;
            host.scene.driven_keys.retain(|k| !(k.driven == t && k.driven_attr == ta));
            let mut curve = rg_core::anim::AnimCurve::from_keys([(0.0, 0.0), (1.0, 1.0)]);
            curve.pre_infinity = Infinity::Linear;
            curve.post_infinity = Infinity::Linear;
            for k in &mut curve.keys {
                k.in_tangent = Tangent::Linear;
                k.out_tangent = Tangent::Linear;
            }
            host.scene.driven_keys.push(DrivenKey { driver: d, driver_attr: da, driven: t, driven_attr: ta, curve });
            host.scene.touch();
            Ok(empty)
        }

        // ------------------------------------------------ animation
        "setKeyframe" => {
            let ids = need_targets(host, &a)?;
            let t = a.get(&["t", "time"]).map(|v| v[0].as_f64()).unwrap_or(host.scene.time.current);
            let v = a.get(&["v", "value"]).map(|v| v[0].as_f64());
            let attrs: Vec<String> = a.all(&["at", "attribute"]).iter().flat_map(|v| v.iter().flat_map(|x| expand_attr(&x.as_string()))).collect();
            let mut count = 0;
            for id in ids {
                let list = if attrs.is_empty() { host.scene.keyable_attrs(id) } else { attrs.clone() };
                for at in list {
                    host.scene.set_key(id, &at, t, v)?;
                    count += 1;
                }
            }
            Ok(Value::Int(count))
        }
        "cutKey" => {
            let ids = need_targets(host, &a)?;
            let attrs: Vec<String> = a.all(&["at", "attribute"]).iter().flat_map(|v| v.iter().flat_map(|x| expand_attr(&x.as_string()))).collect();
            let range = a.get(&["t", "time"]).map(|v| time_range(&v[0])).unwrap_or((f64::NEG_INFINITY, f64::INFINITY));
            let mut n = 0;
            for id in ids {
                let node = host.scene.node_mut(id).unwrap();
                for (at, c) in node.anim.iter_mut() {
                    if attrs.is_empty() || attrs.contains(at) {
                        n += c.remove_range(range.0, range.1);
                    }
                }
                node.anim.retain(|_, c| !c.is_empty());
            }
            Ok(Value::Int(n as i64))
        }
        "keyTangent" => {
            let ids = need_targets(host, &a)?;
            let attrs: Vec<String> = a.all(&["at", "attribute"]).iter().flat_map(|v| v.iter().flat_map(|x| expand_attr(&x.as_string()))).collect();
            let range = a.get(&["t", "time"]).map(|v| time_range(&v[0])).unwrap_or((f64::NEG_INFINITY, f64::INFINITY));
            let itt = a.s(&["itt", "inTangentType"]).map(|s| tangent_parse(&s)).transpose()?;
            let ott = a.s(&["ott", "outTangentType"]).map(|s| tangent_parse(&s)).transpose()?;
            for id in ids {
                let node = host.scene.node_mut(id).unwrap();
                for (at, c) in node.anim.iter_mut() {
                    if !attrs.is_empty() && !attrs.contains(at) {
                        continue;
                    }
                    for k in &mut c.keys {
                        if k.time >= range.0 - 1e-6 && k.time <= range.1 + 1e-6 {
                            if let Some(t) = itt {
                                k.in_tangent = t;
                            }
                            if let Some(t) = ott {
                                k.out_tangent = t;
                            }
                        }
                    }
                }
            }
            Ok(empty)
        }
        "setInfinity" => {
            let ids = need_targets(host, &a)?;
            let attrs: Vec<String> = a.all(&["at", "attribute"]).iter().flat_map(|v| v.iter().flat_map(|x| expand_attr(&x.as_string()))).collect();
            let pri = a.s(&["pri", "preInfinite"]).map(|s| Infinity::parse(&s).ok_or(format!("bad infinity {s}"))).transpose()?;
            let poi = a.s(&["poi", "postInfinite"]).map(|s| Infinity::parse(&s).ok_or(format!("bad infinity {s}"))).transpose()?;
            for id in ids {
                for (at, c) in host.scene.node_mut(id).unwrap().anim.iter_mut() {
                    if !attrs.is_empty() && !attrs.contains(at) {
                        continue;
                    }
                    if let Some(p) = pri {
                        c.pre_infinity = p;
                    }
                    if let Some(p) = poi {
                        c.post_infinity = p;
                    }
                }
            }
            Ok(empty)
        }
        "keyframe" => {
            let ids = need_targets(host, &a)?;
            let attrs: Vec<String> = a.all(&["at", "attribute"]).iter().flat_map(|v| v.iter().flat_map(|x| expand_attr(&x.as_string()))).collect();
            if q {
                let times = host.scene.key_times(&ids);
                if a.has(&["kc", "keyframeCount"]) {
                    let mut n = 0;
                    for id in &ids {
                        for (at, c) in &host.scene.nodes[id].anim {
                            if attrs.is_empty() || attrs.contains(at) {
                                n += c.len();
                            }
                        }
                    }
                    return Ok(Value::Int(n as i64));
                }
                return Ok(Value::Array(times.into_iter().map(Value::Float).collect()));
            }
            let dt = a.f(&["tc", "timeChange"], 0.0);
            let dv = a.f(&["vc", "valueChange"], 0.0);
            let range = a.get(&["t", "time"]).map(|v| time_range(&v[0])).unwrap_or((f64::NEG_INFINITY, f64::INFINITY));
            for id in ids {
                for (at, c) in host.scene.node_mut(id).unwrap().anim.iter_mut() {
                    if attrs.is_empty() || attrs.contains(at) {
                        c.offset_keys(range.0, range.1, dt, dv);
                    }
                }
            }
            let t = host.scene.time.current;
            host.scene.set_time(t);
            Ok(empty)
        }
        "bakeResults" => {
            let ids = need_targets(host, &a)?;
            let (t0, t1) = a.get(&["t", "time"]).map(|v| time_range(&v[0])).unwrap_or((host.scene.time.min, host.scene.time.max));
            let step = a.f(&["sb", "sampleBy"], 1.0);
            // sample evaluated (constraint/IK) values
            let cur = host.scene.time.current;
            let mut samples: Vec<(NodeId, String, Vec<(f64, f64)>)> = Vec::new();
            for &id in &ids {
                for at in host.scene.keyable_attrs(id) {
                    samples.push((id, at, Vec::new()));
                }
            }
            let mut t = t0;
            while t <= t1 + 1e-9 {
                host.scene.set_time(t);
                let ev = host.scene.evaluate();
                for (id, at, v) in samples.iter_mut() {
                    if let Some(x) = ev.attr(host.scene, *id, at) {
                        v.push((t, x));
                    }
                }
                t += step.max(0.01);
            }
            // remove constraints driving these nodes
            host.scene.constraints.retain(|c| !ids.contains(&c.driven));
            for (id, at, v) in samples {
                let node = host.scene.node_mut(id).unwrap();
                let c = node.anim.entry(at).or_default();
                c.remove_range(t0, t1);
                for (t, x) in v {
                    c.set_key(t, x);
                }
            }
            host.scene.set_time(cur);
            Ok(empty)
        }
        "setDrivenKeyframe" => {
            let driver = a.s(&["cd", "currentDriver"]).ok_or("setDrivenKeyframe -cd driver.attr driven.attr")?;
            let (d, da) = split_attr(host.scene, &driver)?;
            let dv = a.get(&["dv", "driverValue"]).map(|v| v[0].as_f64()).unwrap_or_else(|| host.scene.get_attr(d, &da).unwrap_or(0.0));
            let mut driven: Vec<(NodeId, String)> = Vec::new();
            for n in a.pos_names() {
                if n.contains('.') {
                    driven.push(split_attr(host.scene, &n)?);
                } else {
                    let id = host.scene.find(&n).ok_or_else(|| format!("no object '{n}'"))?;
                    if let Some(at) = a.s(&["at", "attribute"]) {
                        for x in expand_attr(&at) {
                            driven.push((id, x));
                        }
                    }
                }
            }
            if driven.is_empty() {
                return Err("specify driven attributes (object.attr or -at)".into());
            }
            for (t, ta) in driven {
                let v = a.get(&["v", "value"]).map(|v| v[0].as_f64()).unwrap_or_else(|| host.scene.get_attr(t, &ta).unwrap_or(0.0));
                let idx = host.scene.driven_keys.iter().position(|k| k.driver == d && k.driver_attr == da && k.driven == t && k.driven_attr == ta);
                match idx {
                    Some(i) => {
                        host.scene.driven_keys[i].curve.set_key(dv, v);
                    }
                    None => host.scene.driven_keys.push(DrivenKey {
                        driver: d,
                        driver_attr: da.clone(),
                        driven: t,
                        driven_attr: ta,
                        curve: rg_core::anim::AnimCurve::from_keys([(dv, v)]),
                    }),
                }
            }
            host.scene.touch();
            Ok(empty)
        }
        "currentTime" => {
            if q {
                return Ok(Value::Float(host.scene.time.current));
            }
            let t = a.pos_nums().first().copied().ok_or("currentTime frame")?;
            host.scene.set_time(t);
            Ok(Value::Float(t))
        }
        "playbackOptions" => {
            let ts = &mut host.scene.time;
            if q {
                if a.has(&["min", "minTime"]) {
                    return Ok(Value::Float(ts.min));
                }
                if a.has(&["max", "maxTime"]) {
                    return Ok(Value::Float(ts.max));
                }
                if a.has(&["ast", "animationStartTime"]) {
                    return Ok(Value::Float(ts.anim_start));
                }
                if a.has(&["aet", "animationEndTime"]) {
                    return Ok(Value::Float(ts.anim_end));
                }
                if a.has(&["fps"]) {
                    return Ok(Value::Float(ts.fps));
                }
                return Ok(empty);
            }
            if let Some(v) = a.get(&["min", "minTime"]) {
                ts.min = v[0].as_f64();
            }
            if let Some(v) = a.get(&["max", "maxTime"]) {
                ts.max = v[0].as_f64();
            }
            if let Some(v) = a.get(&["ast", "animationStartTime"]) {
                ts.anim_start = v[0].as_f64();
            }
            if let Some(v) = a.get(&["aet", "animationEndTime"]) {
                ts.anim_end = v[0].as_f64();
            }
            if let Some(v) = a.get(&["fps"]) {
                ts.fps = v[0].as_f64().max(1.0);
            }
            ts.anim_start = ts.anim_start.min(ts.min);
            ts.anim_end = ts.anim_end.max(ts.max);
            host.scene.touch();
            Ok(empty)
        }
        "play" => {
            let st = a.b(&["st", "state"], true);
            host.requests.push(HostRequest::Playback(st));
            Ok(empty)
        }

        // ------------------------------------------------ rigging
        "parentConstraint" | "pointConstraint" | "orientConstraint" | "scaleConstraint" | "aimConstraint" | "poleVectorConstraint" => {
            let ids = need_targets(host, &a)?;
            if ids.len() < 2 {
                return Err("select target(s) then the constrained object".into());
            }
            let kind = match name {
                "parentConstraint" => ConstraintKind::Parent,
                "pointConstraint" => ConstraintKind::Point,
                "orientConstraint" => ConstraintKind::Orient,
                "scaleConstraint" => ConstraintKind::Scale,
                "aimConstraint" => ConstraintKind::Aim,
                _ => ConstraintKind::PoleVector,
            };
            let driven = *ids.last().unwrap();
            let targets = &ids[..ids.len() - 1];
            let cname = host.scene.add_constraint(kind, targets, driven, a.has(&["mo", "maintainOffset"]))?;
            if let Some(c) = host.scene.constraints.iter_mut().find(|c| c.name == cname) {
                if let Some(w) = a.get(&["w", "weight"]) {
                    let w = w[0].as_f64() as f32;
                    c.weights.iter_mut().for_each(|x| *x = w);
                }
                if let Some(v) = a.v3(&["aim", "aimVector"]) {
                    c.aim_axis = v;
                }
                if let Some(v) = a.v3(&["u", "upVector"]) {
                    c.up_axis = v;
                }
                if let Some(v) = a.v3(&["wu", "worldUpVector"]) {
                    c.world_up = v;
                }
            }
            Ok(Value::Str(cname))
        }
        "ikHandle" => {
            let sj = a.s(&["sj", "startJoint"]);
            let ee = a.s(&["ee", "endEffector"]);
            let (s, e) = match (sj, ee) {
                (Some(s), Some(e)) => (host.scene.find(&s).ok_or(format!("no joint {s}"))?, host.scene.find(&e).ok_or(format!("no joint {e}"))?),
                _ => {
                    let sel = host.scene.selected();
                    if sel.len() < 2 {
                        return Err("select the start joint and end joint".into());
                    }
                    (sel[0], sel[sel.len() - 1])
                }
            };
            let solver = match a.s(&["sol", "solver"]).as_deref() {
                Some("ikSCsolver") | Some("sc") => IkSolver::SingleChain,
                _ => IkSolver::RotatePlane,
            };
            let h = host.scene.create_ik_handle(s, e, solver)?;
            if let Some(n) = a.s(&["n", "name"]) {
                host.scene.rename(h, &n);
            }
            host.scene.select(&[h], SelectMode::Replace);
            Ok(ret_names(host.scene, &[h]))
        }
        "skinCluster" => {
            if a.has(&["ub", "unbind"]) || a.has(&["e"]) && a.has(&["ub"]) {
                for id in need_targets(host, &a)? {
                    host.scene.unbind_skin(id);
                }
                return Ok(empty);
            }
            let ids = need_targets(host, &a)?;
            let joints: Vec<NodeId> = ids.iter().copied().filter(|i| host.scene.nodes[i].is_joint()).collect();
            let meshes: Vec<NodeId> = ids.iter().copied().filter(|i| host.scene.nodes[i].mesh().is_some()).collect();
            if meshes.is_empty() {
                return Err("select joints and a mesh".into());
            }
            let joints = if a.has(&["tsb", "toSelectedBones"]) || joints.len() > 1 {
                joints
            } else {
                // bind to the whole hierarchy below the selected root
                let mut all = joints.clone();
                for j in &joints {
                    all.extend(host.scene.descendants(*j).into_iter().filter(|d| host.scene.nodes[d].is_joint()));
                }
                all
            };
            for m in &meshes {
                host.scene.bind_skin(*m, &joints, a.u(&["mi", "maximumInfluences"], 4) as usize, a.f(&["dr", "dropoffRate"], 4.0) as f32)?;
            }
            Ok(Value::Str("skinCluster1".into()))
        }
        "unbindSkin" => {
            for id in need_targets(host, &a)? {
                host.scene.unbind_skin(id);
            }
            Ok(empty)
        }
        "blendShape" => {
            let ids = need_targets(host, &a)?;
            if ids.len() < 2 {
                return Err("select target shapes then the base mesh".into());
            }
            let base = *ids.last().unwrap();
            let mut names = Vec::new();
            for &t in &ids[..ids.len() - 1] {
                names.push(host.scene.add_blend_target(base, t)?);
            }
            Ok(Value::strings(names))
        }
        "nonLinear" => {
            let ty = a.s(&["type"]).ok_or("nonLinear -type bend|twist|squash|flare|sine|wave")?;
            let kind = match ty.as_str() {
                "bend" => DeformerKind::Bend,
                "twist" => DeformerKind::Twist,
                "squash" => DeformerKind::Squash,
                "flare" => DeformerKind::Flare,
                "sine" => DeformerKind::Sine,
                "wave" => DeformerKind::Wave,
                _ => return Err(format!("unknown deformer type {ty}")),
            };
            let mut out = Vec::new();
            for id in need_targets(host, &a)? {
                if host.scene.nodes[&id].mesh().is_some() {
                    let n = host.scene.add_deformer(id, kind)?;
                    for (f, vals) in &a.flags {
                        if kind.params().contains(&f.as_str()) {
                            let _ = host.scene.set_attr(id, &format!("{n}.{f}"), vals[0].as_f64());
                        }
                    }
                    out.push(n);
                }
            }
            Ok(Value::strings(out))
        }

        // ------------------------------------------------ modeling
        "polyExtrudeFacet" | "polyInset" => {
            let node = mesh_target(host, &a)?;
            let faces = sel_faces(host, &a, node);
            let thick = a.f(&["ltz", "localTranslateZ", "thickness"], 0.0) as f32;
            let off = a.f(&["off", "offset"], if name == "polyInset" { 0.2 } else { 0.0 }) as f32;
            let together = a.b(&["kft", "keepFacesTogether"], true);
            host.scene.edit_mesh(node, |m| {
                if name == "polyInset" {
                    ops::inset_faces(m, &faces, off)
                } else if together {
                    ops::extrude_faces(m, &faces, thick, off);
                } else {
                    ops::extrude_faces_individual(m, &faces, thick, off)
                }
            });
            Ok(empty)
        }
        "polyExtrudeEdge" => {
            let node = mesh_target(host, &a)?;
            let edges = sel_edges(host, &a, node);
            let t = a.v3(&["t", "translate"]).unwrap_or(Vec3::Y * 0.5);
            host.scene.edit_mesh(node, |m| ops::extrude_edges(m, &edges, t));
            Ok(empty)
        }
        "polySmooth" => {
            let node = mesh_target(host, &a)?;
            let dv = a.u(&["dv", "divisions"], 1).clamp(1, 4);
            host.scene.edit_mesh(node, |m| *m = ops::smooth(m, dv));
            host.scene.components.clear();
            Ok(empty)
        }
        "polyBevel" | "polyBevel3" => {
            let node = mesh_target(host, &a)?;
            let off = a.f(&["o", "offset", "fraction", "f"], 0.1) as f32;
            host.scene.edit_mesh(node, |m| ops::bevel_all(m, off)).unwrap_or(Ok(()))?;
            host.scene.components.clear();
            Ok(empty)
        }
        "polySplitRing" | "insertEdgeLoop" => {
            let node = mesh_target(host, &a)?;
            let edges = sel_edges(host, &a, node);
            let e = *edges.first().ok_or("select an edge")?;
            let t = a.f(&["wt", "weight"], 0.5) as f32;
            let n = host.scene.edit_mesh(node, |m| ops::insert_edge_loop(m, e, t)).unwrap_or(0);
            host.scene.components.clear();
            Ok(Value::Int(n as i64))
        }
        "polySelectEdgeLoop" | "polySelectEdgeRing" => {
            let node = mesh_target(host, &a)?;
            let edges = sel_edges(host, &a, node);
            let e = *edges.first().ok_or("select an edge")?;
            let mesh = host.scene.nodes[&node].mesh().unwrap().mesh.clone();
            let found = if name == "polySelectEdgeLoop" { ops::edge_loop(&mesh, e) } else { ops::edge_ring(&mesh, e) };
            host.scene.components.edges.extend(found);
            host.scene.touch();
            Ok(empty)
        }
        "polyMergeVertex" => {
            let node = mesh_target(host, &a)?;
            let verts = sel_verts(host, &a, node);
            let d = a.f(&["d", "distance"], 0.001) as f32;
            let n = host.scene.edit_mesh(node, |m| ops::merge_vertices(m, verts.as_deref(), d)).unwrap_or(0);
            host.scene.components.clear();
            Ok(Value::Int(n as i64))
        }
        "polyMergeToCenter" => {
            let node = mesh_target(host, &a)?;
            let verts = sel_verts(host, &a, node).ok_or("select components")?;
            host.scene.edit_mesh(node, |m| ops::merge_to_center(m, &verts));
            host.scene.components.clear();
            Ok(empty)
        }
        "polyBridgeEdge" => {
            let node = mesh_target(host, &a)?;
            let edges = sel_edges(host, &a, node);
            host.scene.edit_mesh(node, |m| ops::bridge_edges(m, &edges)).unwrap_or(Ok(()))?;
            host.scene.components.clear();
            Ok(empty)
        }
        "polyCloseBorder" => {
            let node = mesh_target(host, &a)?;
            let verts = sel_verts(host, &a, node).unwrap_or_default();
            let n = host.scene.edit_mesh(node, |m| ops::fill_holes(m, &verts)).unwrap_or(0);
            Ok(Value::Int(n as i64))
        }
        "polyTriangulate" | "polyQuad" | "polySeparate" | "polyNormal" | "polySoftEdge" | "polyMirrorFace" | "polyPlanarProjection"
        | "polyCylindricalProjection" | "polyAverageVertex" | "polyPoke" | "polyDelEdge" | "polyChipOff" | "polyEvaluate" => {
            mesh_ops(host, name, &a)
        }
        "polyUnite" => {
            let ids: Vec<NodeId> = need_targets(host, &a)?.into_iter().filter(|i| host.scene.nodes[i].mesh().is_some()).collect();
            if ids.len() < 2 {
                return Err("select two or more meshes".into());
            }
            let id = host.scene.combine(&ids).ok_or("nothing to combine")?;
            if let Some(n) = a.s(&["n", "name"]) {
                host.scene.rename(id, &n);
            }
            host.scene.select(&[id], SelectMode::Replace);
            Ok(ret_names(host.scene, &[id]))
        }

        // ------------------------------------------------ shading
        "shadingNode" => {
            let ty = a.pos_names().first().cloned().ok_or("shadingNode -asShader type")?;
            let shader = ShaderType::parse(&ty).or(match ty.as_str() {
                "aiStandardSurface" => Some(ShaderType::StandardSurface),
                "phongE" => Some(ShaderType::Phong),
                _ => None,
            });
            let shader = shader.ok_or_else(|| format!("unknown shader type {ty}"))?;
            let n = host.scene.create_material(&a.s(&["n", "name"]).unwrap_or_else(|| format!("{}1", shader.name())), shader);
            Ok(Value::Str(n))
        }
        "hyperShade" => {
            let mat = a.s(&["assign", "a"]).ok_or("hyperShade -assign material")?;
            let ids = need_targets(host, &a)?;
            for id in ids {
                if host.scene.nodes[&id].mesh().is_some() {
                    host.scene.assign_material(id, &mat)?;
                }
                for d in host.scene.descendants(id) {
                    if host.scene.nodes[&d].mesh().is_some() {
                        host.scene.assign_material(d, &mat)?;
                    }
                }
            }
            Ok(empty)
        }
        "sets" => {
            if let Some(sg) = a.s(&["forceElement", "fe"]) {
                let mat = if sg == "initialShadingGroup" { "lambert1".to_string() } else { sg.trim_end_matches("SG").to_string() };
                for id in need_targets(host, &a)? {
                    if host.scene.nodes[&id].mesh().is_some() {
                        host.scene.assign_material(id, &mat)?;
                    }
                }
                return Ok(empty);
            }
            // creating a shading group for a material is implicit
            Ok(Value::Str(a.s(&["n", "name"]).unwrap_or_else(|| "set1".into())))
        }
        "createDisplayLayer" => {
            let ids = if a.has(&["empty", "e"]) { Vec::new() } else { targets(host, &a)? };
            let n = host.scene.create_layer(&a.s(&["n", "name"]).unwrap_or_else(|| "layer1".into()), &ids);
            Ok(Value::Str(n))
        }

        // ------------------------------------------------ files & app
        "file" => file_cmd(host, &a),
        "render" => {
            let w = a.u(&["x", "xresolution"], host.scene.render.width);
            let h = a.u(&["y", "yresolution"], host.scene.render.height);
            let samples = a.u(&["s", "samples"], host.scene.render.samples);
            let out = a.s(&["o", "output"]);
            let cam_name = a.pos_names().first().cloned();
            if out.is_none() && a.get(&["sf", "startFrame"]).is_none() {
                host.requests.push(HostRequest::RenderView(cam_name));
                return Ok(empty);
            }
            let aspect = w as f32 / h.max(1) as f32;
            let cam = match cam_name {
                Some(c) => {
                    let id = host.scene.find(&c).ok_or_else(|| format!("no camera {c}"))?;
                    let ev = host.scene.evaluate();
                    rg_render::RenderCamera::from_node(host.scene, &ev, id, aspect).ok_or("not a camera")?
                }
                None => match rg_render::scene_camera(host.scene, aspect) {
                    Some(c) => c,
                    None => {
                        if out.is_none() {
                            host.requests.push(HostRequest::RenderView(None));
                            return Ok(empty);
                        }
                        let ev = host.scene.evaluate();
                        let b = host.scene.world_bounds(&host.scene.roots.clone(), &ev);
                        let c = if b.is_empty() { Vec3::ZERO } else { b.center() };
                        let r = if b.is_empty() { 5.0 } else { b.size().length().max(1.0) };
                        rg_render::RenderCamera::look_at(c + Vec3::new(1.0, 0.7, 1.2).normalize() * r * 1.4, c, 40.0)
                    }
                },
            };
            if let (Some(sf), Some(path)) = (a.get(&["sf", "startFrame"]).map(|v| v[0].as_f64()), out.clone()) {
                let ef = a.f(&["ef", "endFrame"], sf);
                let cur = host.scene.time.current;
                let mut opts = rg_render::RenderOptions::from_scene(host.scene);
                opts.width = w;
                opts.height = h;
                opts.samples = samples.max(1);
                let mut written = Vec::new();
                let mut t = sf;
                while t <= ef + 1e-9 {
                    host.scene.set_time(t);
                    let cam = match a.pos_names().first() {
                        Some(c) => {
                            let id = host.scene.find(c).ok_or_else(|| format!("no camera {c}"))?;
                            let ev = host.scene.evaluate();
                            rg_render::RenderCamera::from_node(host.scene, &ev, id, aspect).ok_or("not a camera")?
                        }
                        None => rg_render::scene_camera(host.scene, aspect).unwrap_or_else(|| cam.clone()),
                    };
                    let img = rg_render::render(host.scene, &cam, &opts, None);
                    let frame = format!("{:04}", t.round() as i64);
                    let p = if path.contains("####") { path.replace("####", &frame) } else { format!("{path}.{frame}.png") };
                    img.save_png(std::path::Path::new(&p))?;
                    interp.print(&format!("// rendered frame {} -> {p}\n", crate::value::fmt_float(t)));
                    written.push(p);
                    t += 1.0;
                }
                host.scene.set_time(cur);
                return Ok(Value::strings(written));
            }
            match out {
                Some(path) => {
                    let mut opts = rg_render::RenderOptions::from_scene(host.scene);
                    opts.width = w;
                    opts.height = h;
                    opts.samples = samples.max(1);
                    let img = rg_render::render(host.scene, &cam, &opts, None);
                    img.save_png(std::path::Path::new(&path))?;
                    Ok(Value::Str(path))
                }
                None => {
                    host.requests.push(HostRequest::RenderView(None));
                    Ok(empty)
                }
            }
        }
        "viewFit" | "fitPanel" => {
            host.requests.push(if a.has(&["all", "a"]) { HostRequest::FrameAll } else { HostRequest::FrameSelected });
            Ok(empty)
        }
        "undo" => {
            host.touched_history = true;
            match host.history.undo(host.scene) {
                Some(l) => {
                    interp.print(&format!("// Undo: {l}\n"));
                    Ok(Value::Str(l))
                }
                None => Err("nothing to undo".into()),
            }
        }
        "redo" => {
            host.touched_history = true;
            match host.history.redo(host.scene) {
                Some(l) => {
                    interp.print(&format!("// Redo: {l}\n"));
                    Ok(Value::Str(l))
                }
                None => Err("nothing to redo".into()),
            }
        }
        "refresh" | "currentUnit" | "requires" | "fileInfo" | "undoInfo" | "flushUndo" | "dgdirty" => Ok(empty),
        "setToolTo" => {
            let t = a.pos_strings().first().cloned().unwrap_or_default();
            host.requests.push(HostRequest::SetTool(t));
            Ok(empty)
        }
        "selectMode" | "selectType" => {
            let m = if a.has(&["co", "component"]) || a.has(&["v", "vertex"]) {
                "vertex"
            } else if a.has(&["e", "edge"]) {
                "edge"
            } else if a.has(&["f", "facet"]) {
                "face"
            } else {
                "object"
            };
            host.requests.push(HostRequest::ComponentMode(m.into()));
            Ok(empty)
        }
        "showWindow" | "window" => {
            host.requests.push(HostRequest::OpenWindow(a.pos_strings().first().cloned().unwrap_or_default()));
            Ok(empty)
        }
        "confirmDialog" | "warning" | "error" | "inViewMessage" => {
            let msg = a.s(&["m", "message", "amg"]).or_else(|| a.pos_strings().first().cloned()).unwrap_or_default();
            if name == "error" {
                return Err(msg);
            }
            if name == "warning" {
                interp.print(&format!("// Warning: {msg}\n"));
            }
            host.requests.push(HostRequest::Message(msg));
            Ok(empty)
        }
        _ => Err(format!("Cannot find procedure \"{name}\".")),
    }
}

fn set_material_attr(scene: &mut Scene, mat: &str, attr: &str, vals: &[f64], a: &Args) -> Result<Value, String> {
    let m = scene.materials.get_mut(mat).unwrap();
    let v3 = || Vec3::new(vals.first().copied().unwrap_or(0.0) as f32, vals.get(1).copied().unwrap_or(0.0) as f32, vals.get(2).copied().unwrap_or(0.0) as f32);
    match attr {
        "color" | "c" | "baseColor" | "bc" | "outColor" | "oc" => m.color = v3(),
        "specularColor" | "sc" => m.specular_color = v3(),
        "incandescence" | "ic" => {
            let c = v3();
            m.emission = c.max_element();
            if m.emission > 0.0 {
                m.emission_color = c / m.emission;
            }
        }
        "emissionColor" => m.emission_color = v3(),
        "transparency" | "it" => m.opacity = 1.0 - v3().x,
        "eccentricity" | "ec" | "specularRoughness" | "sr" => m.roughness = vals.first().copied().unwrap_or(0.3) as f32,
        "metalness" | "m" => m.metalness = vals.first().copied().unwrap_or(0.0) as f32,
        "base" | "diffuse" | "dc" => m.diffuse = vals.first().copied().unwrap_or(0.8) as f32,
        "specular" | "s" => m.specular = vals.first().copied().unwrap_or(0.5) as f32,
        "transmission" | "t" => m.transmission = vals.first().copied().unwrap_or(0.0) as f32,
        "specularIOR" | "ior" => m.ior = vals.first().copied().unwrap_or(1.5) as f32,
        "emission" | "e" => m.emission = vals.first().copied().unwrap_or(0.0) as f32,
        "fileTextureName" | "texture" => {
            let p = a.pos_strings().get(1).cloned().unwrap_or_default();
            m.texture = if p.is_empty() { Texture::None } else { Texture::File { path: p } };
        }
        other => {
            let v = vals.first().copied().ok_or("no value")? as f32;
            if !m.set(other, v) {
                return Err(format!("{mat} has no attribute '{other}'"));
            }
        }
    }
    scene.touch();
    Ok(Value::default())
}

fn transform_cmd(host: &mut Host, name: &str, a: &Args) -> Result<Value, String> {
    let nums = a.pos_nums();
    if nums.is_empty() {
        return Err(format!("{name} x y z"));
    }
    let v = Vec3::new(nums[0] as f32, nums.get(1).copied().unwrap_or(if name == "scale" { nums[0] } else { 0.0 }) as f32, nums.get(2).copied().unwrap_or(if name == "scale" { nums[0] } else { 0.0 }) as f32);
    let relative = a.has(&["r", "relative", "wd"]);
    let object_space = a.has(&["os", "objectSpace", "ls"]);
    // components?
    let names = a.pos_names();
    let comp_given = names.iter().any(|n| parse_component(host.scene, n).is_some());
    if comp_given {
        select_components_from_args(host, a);
    }
    let comp = host.scene.components.clone();
    if (comp_given || names.is_empty()) && comp.mode != ComponentMode::Object && !comp.is_empty() {
        let node = comp.node.unwrap();
        let mesh = host.scene.nodes[&node].mesh().ok_or("not a mesh")?.mesh.clone();
        let verts: Vec<u32> = comp.affected_verts(&mesh).into_iter().collect();
        let w = host.scene.world_matrix(node);
        let inv = w.inverse();
        let centroid: Vec3 = verts.iter().map(|&i| w.transform_point3(mesh.positions[i as usize])).sum::<Vec3>() / verts.len().max(1) as f32;
        let xf: Mat4 = match name {
            "move" => {
                let d = if relative { v } else { v - centroid };
                Mat4::from_translation(d)
            }
            "rotate" => {
                let q = RotateOrder::XYZ.to_quat(v);
                Mat4::from_translation(centroid) * Mat4::from_quat(q) * Mat4::from_translation(-centroid)
            }
            _ => Mat4::from_translation(centroid) * Mat4::from_scale(v) * Mat4::from_translation(-centroid),
        };
        host.scene.edit_mesh(node, |m| {
            for &i in &verts {
                let p = w.transform_point3(m.positions[i as usize]);
                m.positions[i as usize] = inv.transform_point3(xf.transform_point3(p));
            }
        });
        return Ok(Value::default());
    }
    let ids = need_targets(host, a)?;
    for id in ids {
        match name {
            "move" => {
                if relative {
                    let delta = if object_space { host.scene.world_matrix(id).transform_vector3(v.normalize_or_zero()) * v.length() } else { v };
                    move_world(host.scene, id, delta);
                } else {
                    let cur = host.scene.world_position(id);
                    move_world(host.scene, id, v - cur);
                }
            }
            "rotate" => {
                let n = host.scene.node_mut(id).unwrap();
                if relative {
                    if object_space {
                        let q = n.transform.rotation() * RotateOrder::XYZ.to_quat(v);
                        let ro = n.transform.rotate_order;
                        n.transform.rotate = ro.from_quat(q, Some(n.transform.rotate));
                    } else {
                        let world = host.scene.world_matrix(id);
                        let (s, r, t) = world.to_scale_rotation_translation();
                        let nr: Quat = RotateOrder::XYZ.to_quat(v) * r;
                        host.scene.set_world_matrix(id, Mat4::from_scale_rotation_translation(s, nr, t));
                    }
                } else {
                    n.transform.rotate = v;
                }
            }
            _ => {
                let n = host.scene.node_mut(id).unwrap();
                if relative {
                    n.transform.scale *= v;
                } else {
                    n.transform.scale = v;
                }
            }
        }
    }
    host.scene.touch();
    Ok(Value::default())
}

fn xform(host: &mut Host, a: &Args, q: bool) -> Result<Value, String> {
    let ids = need_targets(host, a)?;
    let ws = a.has(&["ws", "worldSpace"]);
    if q {
        let id = ids[0];
        let n = &host.scene.nodes[&id];
        let v = if a.has(&["t", "translation"]) {
            if ws { host.scene.world_matrix(id).transform_point3(Vec3::ZERO) } else { n.transform.translate }
        } else if a.has(&["ro", "rotation"]) {
            if ws {
                let r = host.scene.world_matrix(id).to_scale_rotation_translation().1;
                n.transform.rotate_order.from_quat(r, None)
            } else {
                n.transform.rotate
            }
        } else if a.has(&["s", "scale"]) {
            if ws { host.scene.world_matrix(id).to_scale_rotation_translation().0 } else { n.transform.scale }
        } else if a.has(&["piv", "pivots", "rp", "sp"]) {
            if ws { host.scene.world_position(id) } else { n.transform.pivot }
        } else if a.has(&["bb", "boundingBox"]) {
            let ev = host.scene.evaluate();
            let b = host.scene.world_bounds(&[id], &ev);
            return Ok(Value::Array([b.min.x, b.min.y, b.min.z, b.max.x, b.max.y, b.max.z].iter().map(|v| Value::Float(*v as f64)).collect()));
        } else {
            return Err("xform -q needs -t, -ro, -s, -piv or -bb".into());
        };
        return Ok(Value::Array(vec![Value::Float(v.x as f64), Value::Float(v.y as f64), Value::Float(v.z as f64)]));
    }
    for id in ids {
        if a.has(&["cp", "centerPivots"]) {
            host.scene.center_pivot(id);
        }
        if let Some(t) = a.v3(&["t", "translation"]) {
            if ws {
                let cur = host.scene.world_matrix(id).transform_point3(Vec3::ZERO);
                move_world(host.scene, id, t - cur);
            } else if a.has(&["r", "relative"]) {
                host.scene.node_mut(id).unwrap().transform.translate += t;
            } else {
                host.scene.node_mut(id).unwrap().transform.translate = t;
            }
        }
        if let Some(r) = a.v3(&["ro", "rotation"]) {
            host.scene.node_mut(id).unwrap().transform.rotate = r;
        }
        if let Some(s) = a.v3(&["s", "scale"]) {
            host.scene.node_mut(id).unwrap().transform.scale = s;
        }
        if let Some(p) = a.v3(&["piv", "pivots", "rp", "sp"]) {
            let local = if ws { host.scene.world_matrix(id).inverse().transform_point3(p) } else { p };
            let before = host.scene.nodes[&id].local_matrix();
            let n = host.scene.node_mut(id).unwrap();
            n.transform.pivot = local;
            let after = n.local_matrix();
            let delta = before.transform_point3(Vec3::ZERO) - after.transform_point3(Vec3::ZERO);
            n.transform.translate += delta;
        }
    }
    host.scene.touch();
    Ok(Value::default())
}

fn mesh_ops(host: &mut Host, name: &str, a: &Args) -> Result<Value, String> {
    let node = mesh_target(host, a)?;
    match name {
        "polyEvaluate" => {
            let m = &host.scene.nodes[&node].mesh().unwrap().mesh;
            if a.has(&["v", "vertex"]) {
                return Ok(Value::Int(m.positions.len() as i64));
            }
            if a.has(&["e", "edge"]) {
                return Ok(Value::Int(m.edges().len() as i64));
            }
            if a.has(&["f", "face"]) {
                return Ok(Value::Int(m.faces.len() as i64));
            }
            if a.has(&["t", "triangle"]) {
                return Ok(Value::Int(m.faces.iter().map(|f| f.len().saturating_sub(2)).sum::<usize>() as i64));
            }
            if a.has(&["uv"]) {
                return Ok(Value::Int(m.uvs.len() as i64));
            }
            Ok(Value::Str(format!("vertex={} edge={} face={}", m.positions.len(), m.edges().len(), m.faces.len())))
        }
        "polyTriangulate" => {
            host.scene.edit_mesh(node, ops::triangulate);
            Ok(Value::default())
        }
        "polyQuad" => {
            let ang = a.f(&["a", "angle"], 30.0) as f32;
            host.scene.edit_mesh(node, |m| ops::quadrangulate(m, ang));
            Ok(Value::default())
        }
        "polySeparate" => {
            let ids = host.scene.separate(node);
            host.scene.select(&ids, SelectMode::Replace);
            Ok(ret_names(host.scene, &ids))
        }
        "polyNormal" => {
            let faces = sel_faces(host, a, node);
            host.scene.edit_mesh(node, |m| ops::reverse_faces(m, Some(&faces)));
            Ok(Value::default())
        }
        "polySoftEdge" => {
            let ang = a.f(&["a", "angle"], 30.0) as f32;
            host.scene.edit_mesh(node, |m| m.normal_angle = ang.clamp(0.0, 180.0));
            Ok(Value::default())
        }
        "polyMirrorFace" => {
            let axis = a.u(&["d", "direction", "axis"], 0).min(2) as usize;
            let p = a.f(&["p", "pivot", "mirrorPosition"], 0.0) as f32;
            let tol = a.f(&["mt", "mergeThreshold"], 0.001) as f32;
            host.scene.edit_mesh(node, |m| ops::mirror(m, axis, p, tol));
            Ok(Value::default())
        }
        "polyPlanarProjection" => {
            let axis = match a.s(&["md", "mapDirection"]).as_deref() {
                Some("x") | Some("X") => 0,
                Some("z") | Some("Z") => 2,
                _ => 1,
            };
            host.scene.edit_mesh(node, |m| ops::planar_uvs(m, axis));
            Ok(Value::default())
        }
        "polyCylindricalProjection" => {
            host.scene.edit_mesh(node, ops::cylindrical_uvs);
            Ok(Value::default())
        }
        "polyAverageVertex" => {
            let verts = sel_verts(host, a, node);
            let it = a.u(&["i", "iterations"], 1);
            host.scene.edit_mesh(node, |m| ops::average_vertices(m, verts.as_deref(), it, 0.5));
            Ok(Value::default())
        }
        "polyPoke" => {
            let faces = sel_faces(host, a, node);
            host.scene.edit_mesh(node, |m| ops::poke_faces(m, &faces));
            host.scene.components.clear();
            Ok(Value::default())
        }
        "polyDelEdge" => {
            let edges = sel_edges(host, a, node);
            host.scene.edit_mesh(node, |m| ops::dissolve_edges(m, &edges));
            host.scene.components.clear();
            Ok(Value::default())
        }
        "polyChipOff" => {
            let faces = sel_faces(host, a, node);
            let dup = a.b(&["dup", "duplicate"], true);
            let mut extracted = None;
            let mat = host.scene.nodes[&node].mesh().unwrap().material.clone();
            host.scene.edit_mesh(node, |m| {
                if dup {
                    let mut copy = m.clone();
                    extracted = Some(ops::extract_faces(&mut copy, &faces));
                } else {
                    extracted = Some(ops::extract_faces(m, &faces));
                }
            });
            let ex: Mesh = extracted.ok_or("nothing extracted")?;
            let xf = host.scene.nodes[&node].transform.clone();
            let parent = host.scene.nodes[&node].parent;
            let id = host.scene.add_node("polySurface", NodeKind::Mesh(MeshShape::new(ex)), parent);
            host.scene.node_mut(id).unwrap().transform = xf;
            if let Some(m) = mat {
                let _ = host.scene.assign_material(id, &m);
            }
            host.scene.components.clear();
            host.scene.select(&[id], SelectMode::Replace);
            Ok(ret_names(host.scene, &[id]))
        }
        _ => Err(format!("unknown mesh op {name}")),
    }
}

fn file_cmd(host: &mut Host, a: &Args) -> Result<Value, String> {
    let path = a.pos_strings().into_iter().next();
    if a.has(&["new"]) {
        *host.scene = Scene::new();
        *host.file = None;
        host.history.clear();
        host.touched_history = true;
        return Ok(Value::default());
    }
    if a.has(&["q", "query"]) && a.has(&["sn", "sceneName"]) {
        return Ok(Value::Str(host.file.as_ref().map(|p| p.display().to_string()).unwrap_or_default()));
    }
    if let Some(r) = a.s(&["rn", "rename"]) {
        *host.file = Some(PathBuf::from(r));
        return Ok(Value::default());
    }
    if a.has(&["o", "open"]) {
        let p = PathBuf::from(path.ok_or("file -o path")?);
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        let scene = if ext == "rgs" || ext == "json" {
            rg_io::load_scene(&p)?
        } else {
            let mut s = Scene::new();
            rg_io::import_file(&mut s, &p)?;
            s
        };
        *host.scene = scene;
        *host.file = Some(p);
        host.history.clear();
        host.touched_history = true;
        return Ok(Value::default());
    }
    if a.has(&["s", "save"]) {
        let p = match path {
            Some(p) => PathBuf::from(p),
            None => host.file.clone().ok_or("scene has no name; use file -rn path first")?,
        };
        rg_io::export_file(host.scene, &p, None)?;
        *host.file = Some(p.clone());
        return Ok(Value::Str(p.display().to_string()));
    }
    if a.has(&["i", "import"]) {
        let p = PathBuf::from(path.ok_or("file -i path")?);
        let ids = rg_io::import_file(host.scene, &p)?;
        return Ok(ret_names(host.scene, &ids));
    }
    if a.has(&["es", "exportSelected"]) || a.has(&["ea", "exportAll"]) {
        let p = PathBuf::from(path.ok_or("file -es|-ea path")?);
        let sel = host.scene.selected();
        let only = if a.has(&["es", "exportSelected"]) { Some(sel.as_slice()) } else { None };
        rg_io::export_file(host.scene, &p, only)?;
        return Ok(Value::Str(p.display().to_string()));
    }
    Err("file: use -new, -o, -s, -rn, -i, -es or -ea".into())
}
