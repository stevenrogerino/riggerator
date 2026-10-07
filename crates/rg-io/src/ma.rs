//! Maya ASCII (.ma) subset.
//!
//! Export writes transforms, joints, meshes (vt/ed/fc/uv), cameras, lights,
//! locators, NURBS curves, shaders with shading engines and animation curves.
//! Import reads the same plus poly creator nodes (polyCube, polySphere…)
//! connected to mesh shapes, which is how most simple Maya scenes are saved.

use glam::{Vec2, Vec3};
use rg_core::anim::{AnimCurve, Infinity, Key, Tangent};
use rg_core::mesh::{primitives, Edge, Face, Mesh};
use rg_core::*;
use std::collections::HashMap;
use std::fmt::Write as _;

// ------------------------------------------------------------------ export --

fn tan_code(t: Tangent) -> i32 {
    match t {
        Tangent::Auto => 18,
        Tangent::Spline => 9,
        Tangent::Clamped => 10,
        Tangent::Linear => 2,
        Tangent::Flat => 3,
        Tangent::Stepped => 5,
        Tangent::Fixed => 1,
    }
}

fn tan_from_code(c: i32) -> Tangent {
    match c {
        18 => Tangent::Auto,
        9 | 4 => Tangent::Spline,
        10 | 8 => Tangent::Clamped,
        2 => Tangent::Linear,
        3 => Tangent::Flat,
        5 | 17 => Tangent::Stepped,
        1 => Tangent::Fixed,
        _ => Tangent::Auto,
    }
}

fn inf_code(i: Infinity) -> i32 {
    match i {
        Infinity::Constant => 0,
        Infinity::Linear => 1,
        Infinity::Cycle => 3,
        Infinity::CycleRelative => 4,
        Infinity::Oscillate => 5,
    }
}

fn inf_from_code(c: i32) -> Infinity {
    match c {
        1 => Infinity::Linear,
        3 => Infinity::Cycle,
        4 => Infinity::CycleRelative,
        5 => Infinity::Oscillate,
        _ => Infinity::Constant,
    }
}

fn short_attr(a: &str) -> String {
    match a {
        "translateX" => "tx".into(),
        "translateY" => "ty".into(),
        "translateZ" => "tz".into(),
        "rotateX" => "rx".into(),
        "rotateY" => "ry".into(),
        "rotateZ" => "rz".into(),
        "scaleX" => "sx".into(),
        "scaleY" => "sy".into(),
        "scaleZ" => "sz".into(),
        "visibility" => "v".into(),
        other => other.to_string(),
    }
}

fn v3(v: Vec3) -> String {
    format!("{} {} {}", fnum(v.x), fnum(v.y), fnum(v.z))
}

fn fnum(x: f32) -> String {
    let s = format!("{x}");
    if s == "-0" { "0".into() } else { s }
}

pub fn export(scene: &Scene, only: Option<&[NodeId]>) -> String {
    let mut o = String::new();
    o.push_str("//Maya ASCII 2024 scene\n//Name: riggerator.ma\n//Codeset: UTF-8\n");
    o.push_str("requires maya \"2024\";\ncurrentUnit -l centimeter -a degree -t film;\n");
    o.push_str("fileInfo \"application\" \"riggerator\";\n");
    let include = |id: NodeId| -> bool {
        match only {
            None => true,
            Some(list) => list.iter().any(|&r| r == id || scene.is_ancestor(r, id)),
        }
    };
    let mut connections: Vec<(String, String)> = Vec::new();
    let mut sg_members: HashMap<String, Vec<String>> = HashMap::new();

    for id in scene.walk() {
        if !include(id) {
            continue;
        }
        let n = &scene.nodes[&id];
        if matches!(n.kind, NodeKind::IkHandle(_)) {
            continue; // IK handles need ikSolver plumbing; not exported yet
        }
        let parent = n.parent.filter(|p| include(*p)).map(|p| scene.name_of(p));
        let ty = if n.is_joint() { "joint" } else { "transform" };
        let _ = write!(o, "createNode {ty} -n \"{}\"", n.name);
        if let Some(p) = &parent {
            let _ = write!(o, " -p \"{p}\"");
        }
        o.push_str(";\n");
        let t = &n.transform;
        if !n.visible {
            o.push_str("\tsetAttr \".v\" no;\n");
        }
        let _ = writeln!(o, "\tsetAttr \".t\" -type \"double3\" {} ;", v3(t.translate));
        let _ = writeln!(o, "\tsetAttr \".r\" -type \"double3\" {} ;", v3(t.rotate));
        let _ = writeln!(o, "\tsetAttr \".s\" -type \"double3\" {} ;", v3(t.scale));
        let ro = RotateOrder::ALL.iter().position(|r| *r == t.rotate_order).unwrap_or(0);
        if ro != 0 {
            let _ = writeln!(o, "\tsetAttr \".ro\" {ro};");
        }
        if t.pivot != Vec3::ZERO {
            let _ = writeln!(o, "\tsetAttr \".rp\" -type \"double3\" {} ;", v3(t.pivot));
            let _ = writeln!(o, "\tsetAttr \".sp\" -type \"double3\" {} ;", v3(t.pivot));
        }
        for (k, v) in &n.user_attrs {
            let _ = writeln!(o, "\taddAttr -ci true -k true -sn \"{k}\" -ln \"{k}\" -at \"double\";");
            let _ = writeln!(o, "\tsetAttr -k on \".{k}\" {v};");
        }
        let shape = format!("{}Shape", n.name);
        match &n.kind {
            NodeKind::Joint(j) => {
                let _ = writeln!(o, "\tsetAttr \".jo\" -type \"double3\" {} ;", v3(j.orient));
                let _ = writeln!(o, "\tsetAttr \".radi\" {};", j.radius);
            }
            NodeKind::Mesh(ms) => {
                let _ = writeln!(o, "createNode mesh -n \"{shape}\" -p \"{}\";", n.name);
                write_mesh(&mut o, &ms.mesh);
                let mat = ms.material.clone().unwrap_or_else(|| "lambert1".into());
                sg_members.entry(mat).or_default().push(shape.clone());
            }
            NodeKind::Camera(c) => {
                let _ = writeln!(o, "createNode camera -n \"{shape}\" -p \"{}\";", n.name);
                let _ = writeln!(o, "\tsetAttr \".fl\" {};", c.focal_length);
                let _ = writeln!(o, "\tsetAttr \".ncp\" {};", c.near);
                let _ = writeln!(o, "\tsetAttr \".fcp\" {};", c.far);
                if c.orthographic {
                    o.push_str("\tsetAttr \".o\" yes;\n");
                }
                let _ = writeln!(o, "\tsetAttr \".ow\" {};", c.ortho_width);
            }
            NodeKind::Light(l) => {
                let lt = match l.kind {
                    LightType::Directional => "directionalLight",
                    LightType::Point => "pointLight",
                    LightType::Spot => "spotLight",
                    LightType::Area => "areaLight",
                    LightType::Ambient => "ambientLight",
                };
                let _ = writeln!(o, "createNode {lt} -n \"{shape}\" -p \"{}\";", n.name);
                let _ = writeln!(o, "\tsetAttr \".cl\" -type \"float3\" {} ;", v3(l.color));
                let _ = writeln!(o, "\tsetAttr \".in\" {};", l.intensity);
                if l.kind == LightType::Spot {
                    let _ = writeln!(o, "\tsetAttr \".ca\" {};", l.cone_angle);
                    let _ = writeln!(o, "\tsetAttr \".pe\" {};", l.penumbra);
                }
                if l.decay > 0 {
                    let _ = writeln!(o, "\tsetAttr \".de\" {};", l.decay);
                }
            }
            NodeKind::Locator => {
                let _ = writeln!(o, "createNode locator -n \"{shape}\" -p \"{}\";", n.name);
            }
            NodeKind::Curve(c) => {
                let _ = writeln!(o, "createNode nurbsCurve -n \"{shape}\" -p \"{}\";", n.name);
                write_curve(&mut o, c);
            }
            _ => {}
        }
        for (attr, curve) in &n.anim {
            let suffix = if attr.starts_with("translate") {
                "TL"
            } else if attr.starts_with("rotate") {
                "TA"
            } else {
                "TU"
            };
            let cname = format!("{}_{}", n.name, attr.replace('.', "_"));
            let _ = writeln!(o, "createNode animCurve{suffix} -n \"{cname}\";");
            o.push_str("\tsetAttr \".tan\" 18;\n\tsetAttr \".wgt\" no;\n");
            let k = curve.keys.len();
            let _ = write!(o, "\tsetAttr -s {k} \".ktv[0:{}]\" ", k.saturating_sub(1));
            for key in &curve.keys {
                let _ = write!(o, " {} {}", key.time, key.value);
            }
            o.push_str(";\n");
            let _ = write!(o, "\tsetAttr -s {k} \".kit[0:{}]\" ", k.saturating_sub(1));
            for key in &curve.keys {
                let _ = write!(o, " {}", tan_code(key.in_tangent));
            }
            o.push_str(";\n");
            let _ = write!(o, "\tsetAttr -s {k} \".kot[0:{}]\" ", k.saturating_sub(1));
            for key in &curve.keys {
                let _ = write!(o, " {}", tan_code(key.out_tangent));
            }
            o.push_str(";\n");
            if curve.pre_infinity != Infinity::Constant {
                let _ = writeln!(o, "\tsetAttr \".pre\" {};", inf_code(curve.pre_infinity));
            }
            if curve.post_infinity != Infinity::Constant {
                let _ = writeln!(o, "\tsetAttr \".pst\" {};", inf_code(curve.post_infinity));
            }
            connections.push((format!("{cname}.o"), format!("{}.{}", n.name, short_attr(attr))));
        }
    }

    for (mname, members) in &sg_members {
        let m = scene.materials.get(mname).cloned().unwrap_or_else(Material::default_lambert);
        let sg = if mname == "lambert1" { "initialShadingGroup".to_string() } else { format!("{mname}SG") };
        if mname != "lambert1" {
            let ty = match m.shader {
                ShaderType::Lambert => "lambert",
                ShaderType::Blinn => "blinn",
                ShaderType::Phong => "phong",
                ShaderType::StandardSurface => "standardSurface",
                ShaderType::SurfaceShader => "surfaceShader",
            };
            let _ = writeln!(o, "createNode {ty} -n \"{mname}\";");
            match m.shader {
                ShaderType::StandardSurface => {
                    let _ = writeln!(o, "\tsetAttr \".bc\" -type \"float3\" {} ;", v3(m.color));
                    let _ = writeln!(o, "\tsetAttr \".b\" {};", m.diffuse);
                    let _ = writeln!(o, "\tsetAttr \".s\" {};", m.specular);
                    let _ = writeln!(o, "\tsetAttr \".sr\" {};", m.roughness);
                    let _ = writeln!(o, "\tsetAttr \".m\" {};", m.metalness);
                    let _ = writeln!(o, "\tsetAttr \".e\" {};", m.emission);
                    let _ = writeln!(o, "\tsetAttr \".ec\" -type \"float3\" {} ;", v3(m.emission_color));
                    let _ = writeln!(o, "\tsetAttr \".t\" {};", m.transmission);
                }
                ShaderType::SurfaceShader => {
                    let _ = writeln!(o, "\tsetAttr \".oc\" -type \"float3\" {} ;", v3(m.color));
                }
                _ => {
                    let _ = writeln!(o, "\tsetAttr \".c\" -type \"float3\" {} ;", v3(m.color));
                    let _ = writeln!(o, "\tsetAttr \".dc\" {};", m.diffuse);
                    let tr = 1.0 - m.opacity;
                    if tr > 0.0 {
                        let _ = writeln!(o, "\tsetAttr \".it\" -type \"float3\" {tr} {tr} {tr} ;");
                    }
                    if m.emission > 0.0 {
                        let _ = writeln!(o, "\tsetAttr \".ic\" -type \"float3\" {} ;", v3(m.emission_color * m.emission));
                    }
                    if m.shader != ShaderType::Lambert {
                        let _ = writeln!(o, "\tsetAttr \".sc\" -type \"float3\" {} ;", v3(m.specular_color * m.specular));
                    }
                    if m.shader == ShaderType::Blinn {
                        let _ = writeln!(o, "\tsetAttr \".ec\" {};", m.roughness.max(0.02));
                    }
                    if m.shader == ShaderType::Phong {
                        let _ = writeln!(o, "\tsetAttr \".cp\" {};", (2.0 / (m.roughness * m.roughness).max(1e-3) - 2.0).clamp(2.0, 100.0));
                    }
                }
            }
            let _ = writeln!(o, "createNode shadingEngine -n \"{sg}\";\n\tsetAttr \".ihi\" 0;\n\tsetAttr \".ro\" yes;");
            connections.push((format!("{mname}.oc"), format!("{sg}.ss")));
            connections.push((format!("{sg}.pa"), ":renderPartition.st".into()));
        }
        for m in members {
            connections.push((format!("{m}.iog"), format!("{sg}.dsm")));
        }
    }
    for (a, b) in connections {
        let na = if b.ends_with(".dsm") || b.ends_with(".st") { " -na" } else { "" };
        let _ = writeln!(o, "connectAttr \"{a}\" \"{b}\"{na};");
    }
    o.push_str("// End of riggerator.ma\n");
    o
}

fn write_mesh(o: &mut String, m: &Mesh) {
    let edges = m.edges();
    let eidx: HashMap<Edge, usize> = edges.iter().enumerate().map(|(i, e)| (*e, i)).collect();
    if !m.uvs.is_empty() {
        let _ = write!(o, "\tsetAttr -s {} \".uvst[0].uvsp[0:{}]\" -type \"float2\"", m.uvs.len(), m.uvs.len() - 1);
        for uv in &m.uvs {
            let _ = write!(o, " {} {}", fnum(uv.x), fnum(uv.y));
        }
        o.push_str(";\n");
    }
    let _ = write!(o, "\tsetAttr -s {} \".vt[0:{}]\"", m.positions.len(), m.positions.len().saturating_sub(1));
    for p in &m.positions {
        let _ = write!(o, "  {}", v3(*p));
    }
    o.push_str(";\n");
    let hard = m.normal_angle < 90.0;
    let _ = write!(o, "\tsetAttr -s {} \".ed[0:{}]\"", edges.len(), edges.len().saturating_sub(1));
    for e in &edges {
        let _ = write!(o, "  {} {} {}", e.0, e.1, if hard { 0 } else { 1 });
    }
    o.push_str(";\n");
    let _ = write!(o, "\tsetAttr -s {} \".fc[0:{}]\" -type \"polyFaces\"", m.faces.len(), m.faces.len().saturating_sub(1));
    for f in &m.faces {
        let _ = write!(o, " f {}", f.verts.len());
        for (a, b) in f.edges() {
            let e = Edge::new(a, b);
            let i = eidx[&e] as i64;
            let signed = if e.0 == a { i } else { -i - 1 };
            let _ = write!(o, " {signed}");
        }
        if f.has_uvs() {
            let _ = write!(o, " mu 0 {}", f.uvs.len());
            for u in &f.uvs {
                let _ = write!(o, " {u}");
            }
        }
    }
    o.push_str(";\n");
}

fn write_curve(o: &mut String, c: &CurveShape) {
    let n = c.cvs.len();
    let d = c.degree.max(1) as usize;
    let (spans, form, knots, cvs): (usize, i32, Vec<f32>, Vec<Vec3>) = if c.periodic && d == 3 && n >= 3 {
        let knots: Vec<f32> = (-2..(n as i32 + 3)).map(|k| k as f32).collect();
        let mut cvs = c.cvs.clone();
        cvs.extend_from_slice(&c.cvs[..3]);
        (n, 2, knots, cvs)
    } else {
        let spans = n.saturating_sub(d).max(1);
        let mut knots = vec![0.0; d];
        for i in 1..spans {
            knots.push(i as f32);
        }
        knots.extend(std::iter::repeat_n(spans as f32, d));
        (spans, 0, knots, c.cvs.clone())
    };
    let _ = write!(o, "\tsetAttr \".cc\" -type \"nurbsCurve\" {d} {spans} {form} no 3 {}", knots.len());
    for k in &knots {
        let _ = write!(o, " {k}");
    }
    let _ = write!(o, " {}", cvs.len());
    for p in &cvs {
        let _ = write!(o, " {}", v3(*p));
    }
    o.push_str(";\n");
}

// ------------------------------------------------------------------ import --

#[derive(Debug, Clone)]
struct Rec {
    ty: String,
    name: String,
    parent: Option<String>,
    shared: bool,
    attrs: Vec<(String, Vec<String>)>,
}

fn tokenize_statements(text: &str) -> Vec<Vec<String>> {
    let mut stmts = Vec::new();
    let mut cur: Vec<String> = Vec::new();
    let mut tok = String::new();
    let mut in_str = false;
    let mut chars = text.chars().peekable();
    let mut has_tok = false;
    while let Some(c) = chars.next() {
        if in_str {
            match c {
                '\\' => {
                    if let Some(n) = chars.next() {
                        tok.push(n);
                    }
                }
                '"' => in_str = false,
                _ => tok.push(c),
            }
            continue;
        }
        match c {
            '"' => {
                in_str = true;
                has_tok = true;
            }
            '/' if chars.peek() == Some(&'/') && !has_tok => {
                for n in chars.by_ref() {
                    if n == '\n' {
                        break;
                    }
                }
            }
            ';' => {
                if has_tok {
                    cur.push(std::mem::take(&mut tok));
                    has_tok = false;
                }
                if !cur.is_empty() {
                    stmts.push(std::mem::take(&mut cur));
                }
            }
            c if c.is_whitespace() => {
                if has_tok {
                    cur.push(std::mem::take(&mut tok));
                    has_tok = false;
                }
            }
            _ => {
                tok.push(c);
                has_tok = true;
            }
        }
    }
    if has_tok {
        cur.push(tok);
    }
    if !cur.is_empty() {
        stmts.push(cur);
    }
    stmts
}

/// Parse "[a:b]" or "[a]" index suffix; returns (base attr, start index).
fn split_index(attr: &str) -> (String, usize) {
    if let Some(open) = attr.rfind('[') {
        let inner = &attr[open + 1..attr.len().saturating_sub(1)];
        let start = inner.split(':').next().and_then(|s| s.parse().ok()).unwrap_or(0);
        (attr[..open].to_string(), start)
    } else {
        (attr.to_string(), 0)
    }
}

fn nums(v: &[String]) -> Vec<f32> {
    v.iter().filter_map(|s| s.parse::<f32>().ok()).collect()
}

fn get<'a>(r: &'a Rec, a: &str) -> Option<&'a Vec<String>> {
    r.attrs.iter().rev().find(|(k, _)| k == a).map(|(_, v)| v)
}

fn get_f(r: &Rec, a: &str) -> Option<f32> {
    get(r, a).and_then(|v| nums(v).first().copied())
}

fn get_v3(r: &Rec, a: &str) -> Option<Vec3> {
    get(r, a).and_then(|v| {
        let n = nums(v);
        (n.len() >= 3).then(|| Vec3::new(n[0], n[1], n[2]))
    })
}

fn yes(r: &Rec, a: &str) -> bool {
    get(r, a).and_then(|v| v.first()).is_some_and(|s| s == "yes" || s == "1" || s == "true")
}

pub fn import(scene: &mut Scene, text: &str) -> Result<Vec<NodeId>, String> {
    let stmts = tokenize_statements(text);
    let mut recs: Vec<Rec> = Vec::new();
    let mut by_name: HashMap<String, usize> = HashMap::new();
    let mut conns: Vec<(String, String)> = Vec::new();
    let mut current: Option<usize> = None;
    for st in stmts {
        match st[0].as_str() {
            "createNode" => {
                let mut r = Rec { ty: st.get(1).cloned().unwrap_or_default(), name: String::new(), parent: None, shared: false, attrs: Vec::new() };
                let mut i = 2;
                while i < st.len() {
                    match st[i].as_str() {
                        "-n" => {
                            r.name = st.get(i + 1).cloned().unwrap_or_default();
                            i += 1;
                        }
                        "-p" => {
                            r.parent = st.get(i + 1).cloned();
                            i += 1;
                        }
                        "-s" | "-ss" => r.shared = true,
                        _ => {}
                    }
                    i += 1;
                }
                if r.name.is_empty() {
                    r.name = format!("{}{}", r.ty, recs.len());
                }
                by_name.insert(r.name.clone(), recs.len());
                current = Some(recs.len());
                recs.push(r);
            }
            "select" => {
                // `select -ne :time1;` changes the current node
                current = st.last().and_then(|n| by_name.get(n.trim_start_matches(':')).copied());
            }
            "setAttr" => {
                let mut i = 1;
                let mut attr: Option<String> = None;
                let mut vals = Vec::new();
                while i < st.len() {
                    let t = &st[i];
                    if attr.is_none() {
                        match t.as_str() {
                            "-s" | "-k" | "-l" | "-cb" | "-ca" | "-av" | "-c" | "-type" => {
                                i += 2;
                                continue;
                            }
                            _ => {
                                if t.starts_with('-') && t.parse::<f64>().is_err() {
                                    i += 1;
                                    continue;
                                }
                                attr = Some(t.clone());
                            }
                        }
                    } else if t == "-type" {
                        i += 2;
                        continue;
                    } else {
                        vals.push(t.clone());
                    }
                    i += 1;
                }
                let Some(attr) = attr else { continue };
                if let Some(rel) = attr.strip_prefix('.') {
                    if let Some(c) = current {
                        recs[c].attrs.push((rel.to_string(), vals));
                    }
                } else if let Some((node, a)) = attr.split_once('.') {
                    if let Some(&c) = by_name.get(node) {
                        recs[c].attrs.push((a.to_string(), vals));
                    }
                }
            }
            "connectAttr" => {
                let plain: Vec<&String> = st[1..].iter().filter(|t| !t.starts_with('-')).collect();
                if plain.len() >= 2 {
                    conns.push((plain[0].clone(), plain[1].clone()));
                }
            }
            "parent" => {
                // parent -s -nc -r -add "shape" "transform"; — instanced shapes, ignore
            }
            _ => {}
        }
    }

    // map shape -> creator node via connections ("polyCube1.out" -> "pCubeShape1.i")
    let mut creator_of: HashMap<String, String> = HashMap::new();
    let mut anim_of: Vec<(String, String, String)> = Vec::new(); // curve, node, attr
    let mut sg_of_shader: HashMap<String, String> = HashMap::new();
    let mut shapes_in_sg: Vec<(String, String)> = Vec::new();
    for (a, b) in &conns {
        let (an, aa) = a.split_once('.').unwrap_or((a, ""));
        let (bn, ba) = b.split_once('.').unwrap_or((b, ""));
        let an = an.trim_start_matches(':');
        let bn = bn.trim_start_matches(':');
        let aty = by_name.get(an).map(|&i| recs[i].ty.as_str()).unwrap_or("");
        if (aa == "out" || aa == "output") && (ba == "i" || ba == "inMesh") {
            creator_of.insert(bn.to_string(), an.to_string());
        } else if aty.starts_with("animCurveT") && (aa == "o" || aa == "output") {
            anim_of.push((an.to_string(), bn.to_string(), ba.to_string()));
        } else if (aa == "oc" || aa == "outColor") && (ba == "ss" || ba == "surfaceShader") {
            sg_of_shader.insert(bn.to_string(), an.to_string());
        } else if aa.starts_with("iog") && (ba.starts_with("dsm") || ba.starts_with("dagSetMembers")) {
            shapes_in_sg.push((an.to_string(), bn.to_string()));
        }
    }

    // materials
    let mut mat_name_map: HashMap<String, String> = HashMap::new();
    for r in &recs {
        let shader = match r.ty.as_str() {
            "lambert" => ShaderType::Lambert,
            "blinn" => ShaderType::Blinn,
            "phong" | "phongE" => ShaderType::Phong,
            "standardSurface" | "aiStandardSurface" => ShaderType::StandardSurface,
            "surfaceShader" => ShaderType::SurfaceShader,
            _ => continue,
        };
        if r.name == "lambert1" || r.shared {
            continue;
        }
        let name = scene.create_material(&r.name, shader);
        let m = scene.materials.get_mut(&name).unwrap();
        match shader {
            ShaderType::StandardSurface => {
                if let Some(c) = get_v3(r, "bc").or_else(|| get_v3(r, "baseColor")) {
                    m.color = c;
                }
                if let Some(v) = get_f(r, "b") {
                    m.diffuse = v;
                }
                if let Some(v) = get_f(r, "s") {
                    m.specular = v;
                }
                if let Some(v) = get_f(r, "sr") {
                    m.roughness = v;
                }
                if let Some(v) = get_f(r, "m") {
                    m.metalness = v;
                }
                if let Some(v) = get_f(r, "e") {
                    m.emission = v;
                }
                if let Some(c) = get_v3(r, "ec") {
                    m.emission_color = c;
                }
                if let Some(v) = get_f(r, "t") {
                    m.transmission = v;
                }
            }
            ShaderType::SurfaceShader => {
                if let Some(c) = get_v3(r, "oc") {
                    m.color = c;
                }
            }
            _ => {
                if let Some(c) = get_v3(r, "c") {
                    m.color = c;
                }
                if let Some(v) = get_f(r, "dc") {
                    m.diffuse = v;
                }
                if let Some(t) = get_v3(r, "it") {
                    m.opacity = 1.0 - t.x;
                }
                if let Some(c) = get_v3(r, "ic") {
                    if c.max_element() > 0.0 {
                        m.emission = c.max_element();
                        m.emission_color = c / c.max_element();
                    }
                }
                if let Some(c) = get_v3(r, "sc") {
                    m.specular_color = c;
                }
                if let Some(e) = get_f(r, "ec") {
                    m.roughness = e;
                }
                if let Some(cp) = get_f(r, "cp") {
                    m.roughness = (2.0 / (cp + 2.0)).sqrt();
                }
            }
        }
        mat_name_map.insert(r.name.clone(), name);
    }
    let mut shape_material: HashMap<String, String> = HashMap::new();
    for (shape, sg) in &shapes_in_sg {
        if let Some(shader) = sg_of_shader.get(sg) {
            if let Some(m) = mat_name_map.get(shader) {
                shape_material.insert(shape.clone(), m.clone());
            }
        }
    }

    // DAG nodes
    let mut ids: HashMap<String, NodeId> = HashMap::new();
    let mut created = Vec::new();
    for r in &recs {
        if r.shared || ["persp", "top", "front", "side"].contains(&r.name.as_str()) {
            continue;
        }
        let is_xform = r.ty == "transform" || r.ty == "joint";
        if !is_xform {
            continue;
        }
        let parent = r.parent.as_ref().and_then(|p| ids.get(p.rsplit('|').next().unwrap_or(p)).copied());
        let kind = if r.ty == "joint" {
            NodeKind::Joint(JointShape { radius: get_f(r, "radi").unwrap_or(0.5), orient: get_v3(r, "jo").unwrap_or(Vec3::ZERO) })
        } else {
            NodeKind::Transform
        };
        let id = scene.add_node(&r.name, kind, parent);
        scene.rename(id, &r.name);
        let n = scene.nodes.get_mut(&id).unwrap();
        if let Some(t) = get_v3(r, "t") {
            n.transform.translate = t;
        }
        if let Some(t) = get_v3(r, "r") {
            n.transform.rotate = t;
        }
        if let Some(t) = get_v3(r, "s") {
            n.transform.scale = t;
        }
        if let Some(t) = get_v3(r, "rp") {
            n.transform.pivot = t;
        }
        if let Some(ro) = get_f(r, "ro") {
            n.transform.rotate_order = RotateOrder::ALL[(ro as usize).min(5)];
        }
        if get(r, "v").and_then(|v| v.first()).is_some_and(|v| v == "no" || v == "0") {
            n.visible = false;
        }
        ids.insert(r.name.clone(), id);
        if parent.is_none() {
            created.push(id);
        }
    }
    // user attrs that were setAttr'd on transforms (addAttr'd keyable doubles)
    for r in &recs {
        let Some(&id) = ids.get(&r.name) else { continue };
        for (k, v) in &r.attrs {
            let known = ["t", "r", "s", "rp", "sp", "ro", "v", "jo", "radi", "ra", "uocol", "oclr", "spt", "rpt", "hio"];
            if known.contains(&k.as_str()) || k.contains('[') || k.contains('.') {
                continue;
            }
            if let Some(x) = nums(v).first() {
                if scene.get_attr(id, k).is_none() && v.len() == 1 {
                    let _ = scene.add_user_attr(id, k, *x as f64);
                }
            }
        }
    }
    // shapes
    for r in &recs {
        if r.shared {
            continue;
        }
        let Some(pname) = &r.parent else { continue };
        let Some(&pid) = ids.get(pname.rsplit('|').next().unwrap_or(pname)) else { continue };
        if yes(r, "io") {
            continue; // intermediate object
        }
        let kind = match r.ty.as_str() {
            "mesh" => {
                let mesh = match creator_of.get(&r.name).and_then(|c| by_name.get(c)).map(|&i| &recs[i]) {
                    Some(cr) => creator_mesh(cr).or_else(|| read_mesh(r)),
                    None => read_mesh(r),
                };
                let Some(mesh) = mesh else { continue };
                let mut ms = MeshShape::new(mesh);
                ms.material = shape_material.get(&r.name).cloned();
                NodeKind::Mesh(ms)
            }
            "camera" => NodeKind::Camera(CameraShape {
                focal_length: get_f(r, "fl").unwrap_or(35.0),
                near: get_f(r, "ncp").unwrap_or(0.1),
                far: get_f(r, "fcp").unwrap_or(10000.0),
                orthographic: yes(r, "o"),
                ortho_width: get_f(r, "ow").unwrap_or(30.0),
                horizontal_aperture_mm: 36.0,
            }),
            "pointLight" | "directionalLight" | "spotLight" | "areaLight" | "ambientLight" => {
                let kind = match r.ty.as_str() {
                    "pointLight" => LightType::Point,
                    "directionalLight" => LightType::Directional,
                    "spotLight" => LightType::Spot,
                    "areaLight" => LightType::Area,
                    _ => LightType::Ambient,
                };
                let mut l = LightShape::new(kind);
                if let Some(c) = get_v3(r, "cl") {
                    l.color = c;
                }
                if let Some(i) = get_f(r, "in") {
                    l.intensity = i;
                }
                if let Some(c) = get_f(r, "ca") {
                    l.cone_angle = c;
                }
                if let Some(p) = get_f(r, "pe") {
                    l.penumbra = p;
                }
                if let Some(d) = get_f(r, "de") {
                    l.decay = d as u32;
                }
                NodeKind::Light(l)
            }
            "locator" => NodeKind::Locator,
            "nurbsCurve" => match read_curve(r) {
                Some(c) => NodeKind::Curve(c),
                None => continue,
            },
            _ => continue,
        };
        let n = scene.nodes.get_mut(&pid).unwrap();
        if matches!(n.kind, NodeKind::Transform) {
            n.kind = kind;
        }
    }
    // animation
    for (curve, node, attr) in anim_of {
        let Some(&id) = ids.get(&node) else { continue };
        let Some(&ri) = by_name.get(&curve) else { continue };
        let r = &recs[ri];
        let mut c = AnimCurve::new();
        // ktv entries may come in several setAttr chunks
        for (k, v) in &r.attrs {
            let (base, _) = split_index(k);
            if base == "ktv" || base == "keyTimeValue" {
                for pair in nums(v).chunks(2) {
                    if pair.len() == 2 {
                        c.insert(Key::new(pair[0] as f64, pair[1] as f64));
                    }
                }
            }
        }
        for (k, v) in &r.attrs {
            let (base, start) = split_index(k);
            let codes = nums(v);
            if base == "kit" || base == "kot" {
                for (i, code) in codes.iter().enumerate() {
                    if let Some(key) = c.keys.get_mut(start + i) {
                        let t = tan_from_code(*code as i32);
                        if base == "kit" {
                            key.in_tangent = t;
                        } else {
                            key.out_tangent = t;
                        }
                    }
                }
            }
        }
        if let Some(p) = get_f(r, "pre") {
            c.pre_infinity = inf_from_code(p as i32);
        }
        if let Some(p) = get_f(r, "pst") {
            c.post_infinity = inf_from_code(p as i32);
        }
        if c.is_empty() {
            continue;
        }
        let long = long_attr_name(&attr).to_string();
        let first = c.evaluate(scene.time.current);
        if let Some(n) = scene.nodes.get_mut(&id) {
            n.anim.insert(long.clone(), c);
        }
        let _ = scene.set_attr(id, &long, first);
    }
    scene.touch();
    Ok(created)
}

fn creator_mesh(r: &Rec) -> Option<Mesh> {
    let f = |a: &str, d: f32| get_f(r, a).unwrap_or(d);
    let u = |a: &str, d: u32| get_f(r, a).map(|x| x as u32).unwrap_or(d);
    let axis = get_v3(r, "ax");
    let mut m = match r.ty.as_str() {
        "polyCube" => primitives::cube(f("w", 1.0), f("h", 1.0), f("d", 1.0), u("sw", 1), u("sh", 1), u("sd", 1)),
        "polySphere" => primitives::sphere(f("r", 1.0), u("sa", 20), u("sh", 20)),
        "polyCylinder" => primitives::cylinder(f("r", 1.0), f("h", 2.0), u("sa", 20), u("sh", 1), true),
        "polyPlane" => primitives::plane(f("w", 1.0), f("h", 1.0), u("sw", 10), u("sh", 10)),
        "polyTorus" => primitives::torus(f("r", 1.0), f("sr", 0.5), u("sa", 20), u("sh", 20)),
        "polyCone" => primitives::cone(f("r", 1.0), f("h", 2.0), u("sa", 20)),
        "polyPipe" => primitives::pipe(f("r", 1.0), f("h", 2.0), f("t", 0.5), u("sa", 20)),
        _ => return None,
    };
    if let Some(ax) = axis {
        if (ax - Vec3::Y).length() > 1e-4 && ax.length() > 1e-4 {
            let q = glam::Quat::from_rotation_arc(Vec3::Y, ax.normalize());
            m = m.transformed(glam::Mat4::from_quat(q));
        }
    }
    Some(m)
}

fn read_mesh(r: &Rec) -> Option<Mesh> {
    let mut positions: Vec<Vec3> = Vec::new();
    let mut edges: Vec<(u32, u32)> = Vec::new();
    let mut uvs: Vec<Vec2> = Vec::new();
    let mut faces = Vec::new();
    let mut tweaks: Vec<(usize, Vec3)> = Vec::new();
    for (k, v) in &r.attrs {
        let (base, start) = split_index(k);
        match base.as_str() {
            "vt" | "vrts" => {
                for (i, c) in nums(v).chunks(3).enumerate() {
                    if c.len() == 3 {
                        let idx = start + i;
                        if positions.len() <= idx {
                            positions.resize(idx + 1, Vec3::ZERO);
                        }
                        positions[idx] = Vec3::new(c[0], c[1], c[2]);
                    }
                }
            }
            "pt" | "pnts" => {
                for (i, c) in nums(v).chunks(3).enumerate() {
                    if c.len() == 3 {
                        tweaks.push((start + i, Vec3::new(c[0], c[1], c[2])));
                    }
                }
            }
            "ed" | "edge" => {
                for (i, c) in nums(v).chunks(3).enumerate() {
                    if c.len() >= 2 {
                        let idx = start + i;
                        if edges.len() <= idx {
                            edges.resize(idx + 1, (0, 0));
                        }
                        edges[idx] = (c[0] as u32, c[1] as u32);
                    }
                }
            }
            "uvst[0].uvsp" | "uvst[0].uvSetPoints" => {
                for (i, c) in nums(v).chunks(2).enumerate() {
                    if c.len() == 2 {
                        let idx = start + i;
                        if uvs.len() <= idx {
                            uvs.resize(idx + 1, Vec2::ZERO);
                        }
                        uvs[idx] = Vec2::new(c[0], c[1]);
                    }
                }
            }
            "fc" | "face" => {
                let mut i = 0;
                let mut cur: Option<Face> = None;
                while i < v.len() {
                    match v[i].as_str() {
                        "f" => {
                            if let Some(f) = cur.take() {
                                faces.push(f);
                            }
                            let n: usize = v.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(0);
                            let mut verts = Vec::with_capacity(n);
                            for j in 0..n {
                                let e: i64 = v.get(i + 2 + j).and_then(|s| s.parse().ok()).unwrap_or(0);
                                let (idx, rev) = if e < 0 { ((-e - 1) as usize, true) } else { (e as usize, false) };
                                let Some(&(a, b)) = edges.get(idx) else { continue };
                                verts.push(if rev { b } else { a });
                            }
                            cur = Some(Face::new(verts));
                            i += 2 + n;
                        }
                        "mu" | "mf" => {
                            let n: usize = v.get(i + 2).and_then(|s| s.parse().ok()).unwrap_or(0);
                            let set: usize = v.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(0);
                            if set == 0 {
                                if let Some(f) = &mut cur {
                                    f.uvs = (0..n).filter_map(|j| v.get(i + 3 + j).and_then(|s| s.parse().ok())).collect();
                                }
                            }
                            i += 3 + n;
                        }
                        "h" => {
                            let n: usize = v.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(0);
                            i += 2 + n;
                        }
                        "fc" => {
                            let n: usize = v.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(0);
                            i += 2 + n;
                        }
                        _ => i += 1,
                    }
                }
                if let Some(f) = cur.take() {
                    faces.push(f);
                }
            }
            _ => {}
        }
    }
    if positions.is_empty() || faces.is_empty() {
        return None;
    }
    for (i, t) in tweaks {
        if let Some(p) = positions.get_mut(i) {
            *p += t;
        }
    }
    let nuv = uvs.len() as u32;
    for f in &mut faces {
        if !f.uvs.is_empty() && (f.uvs.len() != f.verts.len() || f.uvs.iter().any(|&u| u >= nuv)) {
            f.uvs.clear();
        }
    }
    let mut m = Mesh { positions, uvs, faces, normal_angle: 30.0 };
    m.compact();
    Some(m)
}

fn read_curve(r: &Rec) -> Option<CurveShape> {
    let v = get(r, "cc")?;
    let n: Vec<f32> = nums(v);
    // degree spans form rational(no) dim knotcount knots... cvcount cvs...
    let degree = *n.first()? as u32;
    let form = *n.get(2)? as i32;
    let mut i = 4; // skip "no" (not numeric) => tokens: d s f 3 ...
    let _dim = *n.get(3)?;
    let kc = *n.get(i)? as usize;
    i += 1 + kc;
    let cc = *n.get(i)? as usize;
    i += 1;
    let mut cvs = Vec::new();
    for k in 0..cc {
        let x = *n.get(i + k * 3)?;
        let y = *n.get(i + k * 3 + 1)?;
        let z = *n.get(i + k * 3 + 2)?;
        cvs.push(Vec3::new(x, y, z));
    }
    let periodic = form == 2;
    if periodic && cvs.len() > degree as usize {
        cvs.truncate(cvs.len() - degree as usize);
    }
    Some(CurveShape { cvs, degree, periodic })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_scene() {
        let mut s = Scene::new();
        let c = s.add_mesh("pCube", primitives::cube(2.0, 1.0, 1.0, 2, 1, 1));
        s.set_attr(c, "tx", 3.0).unwrap();
        s.set_attr(c, "ry", 45.0).unwrap();
        let m = s.create_material("blinn", ShaderType::Blinn);
        s.materials.get_mut(&m).unwrap().color = Vec3::new(0.2, 0.4, 0.8);
        s.assign_material(c, &m).unwrap();
        s.set_key(c, "ty", 1.0, Some(0.0)).unwrap();
        s.set_key(c, "ty", 10.0, Some(5.0)).unwrap();
        let js = s.create_joint_chain(&[Vec3::ZERO, Vec3::Y * 2.0], None);
        let cam = s.add_node("camera", NodeKind::Camera(CameraShape { focal_length: 50.0, ..Default::default() }), None);
        let circ = s.add_node("ctrl", NodeKind::Curve(CurveShape::circle(1.0, 8, 1)), None);

        let text = export(&s, None);
        let mut t = Scene::new();
        import(&mut t, &text).unwrap();
        let c2 = t.find("pCube1").unwrap();
        assert!((t.get_attr(c2, "tx").unwrap() - 3.0).abs() < 1e-5);
        assert!((t.get_attr(c2, "ry").unwrap() - 45.0).abs() < 1e-4);
        let mesh = &t.nodes[&c2].mesh().unwrap().mesh;
        let orig = &s.nodes[&c].mesh().unwrap().mesh;
        assert_eq!(mesh.faces.len(), orig.faces.len());
        assert_eq!(mesh.positions.len(), orig.positions.len());
        assert!(mesh.validate().is_empty(), "{:?}", mesh.validate());
        assert!(mesh.faces[0].has_uvs());
        // same winding
        for fi in 0..mesh.faces.len() {
            assert!((mesh.face_normal(fi) - orig.face_normal(fi)).length() < 1e-4);
        }
        let mat = t.material_for(c2);
        assert!((mat.color - Vec3::new(0.2, 0.4, 0.8)).length() < 1e-4);
        assert_eq!(t.nodes[&c2].anim["translateY"].len(), 2);
        let j = t.find(&s.name_of(js[1])).unwrap();
        assert!(t.nodes[&j].is_joint());
        assert!((t.world_position(j) - Vec3::Y * 2.0).length() < 1e-4);
        let cam2 = t.find(&s.name_of(cam)).unwrap();
        assert!(matches!(&t.nodes[&cam2].kind, NodeKind::Camera(c) if (c.focal_length - 50.0).abs() < 1e-4));
        let circ2 = t.find(&s.name_of(circ)).unwrap();
        match &t.nodes[&circ2].kind {
            NodeKind::Curve(cv) => {
                assert_eq!(cv.cvs.len(), 8);
                assert!(cv.periodic);
            }
            k => panic!("{k:?}"),
        }
    }

    #[test]
    fn imports_maya_style_creator_history() {
        let text = r#"//Maya ASCII 2024 scene
requires maya "2024";
createNode transform -s -n "persp";
createNode camera -s -n "perspShape" -p "persp";
createNode transform -n "pSphere1";
	rename -uid "ABC";
	setAttr ".t" -type "double3" 0 2 0 ;
createNode mesh -n "pSphereShape1" -p "pSphere1";
	setAttr -k off ".v";
	setAttr ".vir" yes;
createNode polySphere -n "polySphere1";
	setAttr ".r" 2;
	setAttr ".sa" 12;
	setAttr ".sh" 8;
createNode blinn -n "blinn1";
	setAttr ".c" -type "float3" 1 0 0 ;
createNode shadingEngine -n "blinn1SG";
connectAttr "polySphere1.out" "pSphereShape1.i";
connectAttr "blinn1.oc" "blinn1SG.ss";
connectAttr "pSphereShape1.iog" "blinn1SG.dsm" -na;
"#;
        let mut s = Scene::new();
        let roots = import(&mut s, text).unwrap();
        assert_eq!(roots.len(), 1);
        let id = s.find("pSphere1").unwrap();
        let m = &s.nodes[&id].mesh().unwrap().mesh;
        assert_eq!(m.faces.len(), 12 * 8);
        assert!((m.bounds().max.y - 2.0).abs() < 1e-4);
        assert!(s.material_for(id).color.x > 0.9);
        assert!(s.find("persp").is_none());
    }
}
