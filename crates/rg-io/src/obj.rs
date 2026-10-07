use glam::{Vec2, Vec3};
use rg_core::mesh::{Face, Mesh};
use rg_core::{Material, NodeId, NodeKind, Scene, ShaderType, Texture};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;

/// Parse OBJ text into named meshes (one per `o`/`g`) plus material names.
pub fn parse(text: &str) -> (Vec<(String, Mesh, Option<String>)>, Vec<String>) {
    let mut pos: Vec<Vec3> = Vec::new();
    let mut uv: Vec<Vec2> = Vec::new();
    let mut out: Vec<(String, Mesh, Option<String>)> = Vec::new();
    let mut mtllibs = Vec::new();
    // per-object remaps from global index to local
    let mut vmap: HashMap<usize, u32> = HashMap::new();
    let mut umap: HashMap<usize, u32> = HashMap::new();
    let mut cur_name = "polySurface".to_string();
    let mut cur_mat: Option<String> = None;
    let mut cur = Mesh { normal_angle: 30.0, ..Default::default() };
    let flush = |name: &str, mesh: &mut Mesh, mat: &Option<String>, out: &mut Vec<(String, Mesh, Option<String>)>, vmap: &mut HashMap<usize, u32>, umap: &mut HashMap<usize, u32>| {
        if !mesh.faces.is_empty() {
            out.push((name.to_string(), std::mem::take(mesh), mat.clone()));
            mesh.normal_angle = 30.0;
        }
        vmap.clear();
        umap.clear();
    };
    for line in text.lines() {
        let line = line.trim();
        let mut it = line.split_whitespace();
        let Some(tag) = it.next() else { continue };
        match tag {
            "v" => {
                let v: Vec<f32> = it.take(3).filter_map(|x| x.parse().ok()).collect();
                if v.len() == 3 {
                    pos.push(Vec3::new(v[0], v[1], v[2]));
                }
            }
            "vt" => {
                let v: Vec<f32> = it.take(2).filter_map(|x| x.parse().ok()).collect();
                uv.push(Vec2::new(*v.first().unwrap_or(&0.0), *v.get(1).unwrap_or(&0.0)));
            }
            "o" | "g" => {
                let name = it.collect::<Vec<_>>().join("_");
                flush(&cur_name, &mut cur, &cur_mat, &mut out, &mut vmap, &mut umap);
                if !name.is_empty() {
                    cur_name = name;
                }
            }
            "usemtl" => {
                let m = it.next().map(|s| s.to_string());
                if !cur.faces.is_empty() && m != cur_mat {
                    // new material mid-object: split into a new mesh
                    flush(&cur_name, &mut cur, &cur_mat, &mut out, &mut vmap, &mut umap);
                }
                cur_mat = m;
            }
            "mtllib" => mtllibs.extend(it.map(|s| s.to_string())),
            "s" => {
                if let Some(v) = it.next() {
                    cur.normal_angle = if v == "off" || v == "0" { 30.0 } else { 180.0 };
                }
            }
            "f" => {
                let mut fv = Vec::new();
                let mut fu = Vec::new();
                let mut has_uv = true;
                for tok in it {
                    let mut parts = tok.split('/');
                    let vi: i64 = parts.next().and_then(|x| x.parse().ok()).unwrap_or(0);
                    let ti: Option<i64> = parts.next().and_then(|x| x.parse().ok());
                    let gv = if vi < 0 { pos.len() as i64 + vi } else { vi - 1 };
                    if gv < 0 || gv as usize >= pos.len() {
                        continue;
                    }
                    let gv = gv as usize;
                    let lv = *vmap.entry(gv).or_insert_with(|| {
                        cur.positions.push(pos[gv]);
                        (cur.positions.len() - 1) as u32
                    });
                    fv.push(lv);
                    match ti {
                        Some(t) => {
                            let gt = if t < 0 { uv.len() as i64 + t } else { t - 1 };
                            if gt >= 0 && (gt as usize) < uv.len() {
                                let gt = gt as usize;
                                let lt = *umap.entry(gt).or_insert_with(|| {
                                    cur.uvs.push(uv[gt]);
                                    (cur.uvs.len() - 1) as u32
                                });
                                fu.push(lt);
                            } else {
                                has_uv = false;
                            }
                        }
                        None => has_uv = false,
                    }
                }
                if fv.len() >= 3 {
                    cur.faces.push(Face { verts: fv, uvs: if has_uv { fu } else { Vec::new() } });
                }
            }
            _ => {}
        }
    }
    flush(&cur_name, &mut cur, &cur_mat, &mut out, &mut vmap, &mut umap);
    (out, mtllibs)
}

pub fn parse_mtl(text: &str) -> Vec<Material> {
    let mut mats = Vec::new();
    let mut cur: Option<Material> = None;
    for line in text.lines() {
        let line = line.trim();
        let mut it = line.split_whitespace();
        let Some(tag) = it.next() else { continue };
        let nums: Vec<f32> = line.split_whitespace().skip(1).filter_map(|x| x.parse().ok()).collect();
        let v3 = || Vec3::new(*nums.first().unwrap_or(&0.0), *nums.get(1).unwrap_or(&0.0), *nums.get(2).unwrap_or(&0.0));
        match tag {
            "newmtl" => {
                if let Some(m) = cur.take() {
                    mats.push(m);
                }
                cur = Some(Material::new(it.next().unwrap_or("material"), ShaderType::Blinn));
            }
            "Kd" => {
                if let Some(m) = &mut cur {
                    m.color = v3();
                    m.diffuse = 1.0;
                }
            }
            "Ks" => {
                if let Some(m) = &mut cur {
                    m.specular_color = v3();
                    m.specular = m.specular_color.max_element().min(1.0);
                }
            }
            "Ke" => {
                if let Some(m) = &mut cur {
                    let e = v3();
                    if e.max_element() > 0.0 {
                        m.emission_color = e / e.max_element();
                        m.emission = e.max_element();
                    }
                }
            }
            "Ns" => {
                if let Some(m) = &mut cur {
                    let ns = nums.first().copied().unwrap_or(0.0);
                    m.roughness = (2.0 / (ns + 2.0)).sqrt().clamp(0.02, 1.0);
                }
            }
            "d" => {
                if let Some(m) = &mut cur {
                    m.opacity = nums.first().copied().unwrap_or(1.0);
                }
            }
            "Tr" => {
                if let Some(m) = &mut cur {
                    m.opacity = 1.0 - nums.first().copied().unwrap_or(0.0);
                }
            }
            "Ni" => {
                if let Some(m) = &mut cur {
                    m.ior = nums.first().copied().unwrap_or(1.5);
                }
            }
            "map_Kd" => {
                if let Some(m) = &mut cur {
                    if let Some(p) = line.split_whitespace().last() {
                        m.texture = Texture::File { path: p.to_string() };
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(m) = cur {
        mats.push(m);
    }
    mats
}

pub fn import(scene: &mut Scene, path: &Path) -> Result<Vec<NodeId>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let (meshes, libs) = parse(&text);
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut mat_names: HashMap<String, String> = HashMap::new();
    for lib in libs {
        if let Ok(t) = std::fs::read_to_string(dir.join(&lib)) {
            for mut m in parse_mtl(&t) {
                if let Texture::File { path: p } = &m.texture {
                    let full = dir.join(p);
                    m.texture = Texture::File { path: full.to_string_lossy().into_owned() };
                }
                let new = scene.unique_name(&m.name);
                mat_names.insert(m.name.clone(), new.clone());
                m.name = new.clone();
                scene.materials.insert(new, m);
            }
        }
    }
    let mut ids = Vec::new();
    for (name, mesh, mat) in meshes {
        let id = scene.add_node(&name, NodeKind::Mesh(rg_core::MeshShape::new(mesh)), None);
        scene.rename(id, &name);
        if let Some(m) = mat.and_then(|m| mat_names.get(&m).cloned()) {
            let _ = scene.assign_material(id, &m);
        }
        ids.push(id);
    }
    scene.touch();
    Ok(ids)
}

/// Serialize meshes (world space, evaluated pose) to OBJ + MTL text.
pub fn to_strings(scene: &Scene, only: Option<&[NodeId]>, mtl_name: &str) -> (String, String) {
    let ev = scene.evaluate();
    let mut obj = String::new();
    let mut mtl = String::new();
    let _ = writeln!(obj, "# Riggerator OBJ export\nmtllib {mtl_name}");
    let mut voff = 1usize;
    let mut uoff = 1usize;
    let mut used_mats: Vec<String> = Vec::new();
    for id in scene.walk() {
        if let Some(o) = only {
            if !o.contains(&id) {
                continue;
            }
        }
        let n = &scene.nodes[&id];
        let NodeKind::Mesh(ms) = &n.kind else { continue };
        let Some(m) = ev.mesh(id) else { continue };
        let m = m.transformed(ev.world(id));
        let _ = writeln!(obj, "o {}", n.name);
        for p in &m.positions {
            let _ = writeln!(obj, "v {} {} {}", p.x, p.y, p.z);
        }
        for t in &m.uvs {
            let _ = writeln!(obj, "vt {} {}", t.x, t.y);
        }
        let mat = ms.material.clone().unwrap_or_else(|| "lambert1".into());
        if !used_mats.contains(&mat) {
            used_mats.push(mat.clone());
        }
        let _ = writeln!(obj, "usemtl {mat}");
        let _ = writeln!(obj, "s {}", if m.normal_angle >= 90.0 { "1" } else { "off" });
        for f in &m.faces {
            obj.push('f');
            for (i, &v) in f.verts.iter().enumerate() {
                if f.has_uvs() {
                    let _ = write!(obj, " {}/{}", v as usize + voff, f.uvs[i] as usize + uoff);
                } else {
                    let _ = write!(obj, " {}", v as usize + voff);
                }
            }
            obj.push('\n');
        }
        voff += m.positions.len();
        uoff += m.uvs.len();
    }
    for name in used_mats {
        let m = scene.materials.get(&name).cloned().unwrap_or_else(Material::default_lambert);
        let _ = writeln!(mtl, "newmtl {}", name);
        let kd = m.color * m.diffuse;
        let _ = writeln!(mtl, "Kd {} {} {}", kd.x, kd.y, kd.z);
        let ks = m.specular_color * m.specular;
        let _ = writeln!(mtl, "Ks {} {} {}", ks.x, ks.y, ks.z);
        let ns = (2.0 / (m.roughness * m.roughness).max(1e-4) - 2.0).clamp(1.0, 1000.0);
        let _ = writeln!(mtl, "Ns {ns}");
        let _ = writeln!(mtl, "d {}", m.opacity);
        if m.emission > 0.0 {
            let e = m.emission_color * m.emission;
            let _ = writeln!(mtl, "Ke {} {} {}", e.x, e.y, e.z);
        }
        if let Texture::File { path } = &m.texture {
            let _ = writeln!(mtl, "map_Kd {path}");
        }
        mtl.push('\n');
    }
    (obj, mtl)
}

pub fn export(scene: &Scene, path: &Path, only: Option<&[NodeId]>) -> Result<(), String> {
    let mtl_path = path.with_extension("mtl");
    let mtl_name = mtl_path.file_name().unwrap().to_string_lossy().into_owned();
    let (obj, mtl) = to_strings(scene, only, &mtl_name);
    std::fs::write(path, obj).map_err(|e| e.to_string())?;
    std::fs::write(mtl_path, mtl).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rg_core::mesh::primitives;

    #[test]
    fn roundtrip_text() {
        let mut s = Scene::new();
        let c = s.add_mesh("box", primitives::cube(2.0, 2.0, 2.0, 1, 1, 1));
        s.set_attr(c, "tx", 5.0).unwrap();
        let m = s.create_material("red", ShaderType::Blinn);
        s.materials.get_mut(&m).unwrap().color = Vec3::new(1.0, 0.0, 0.0);
        s.assign_material(c, &m).unwrap();
        s.add_mesh("ball", primitives::sphere(1.0, 8, 6));
        let (obj, mtl) = to_strings(&s, None, "x.mtl");
        let (meshes, libs) = parse(&obj);
        assert_eq!(libs, vec!["x.mtl"]);
        assert_eq!(meshes.len(), 2);
        assert_eq!(meshes[0].0, "box1");
        assert_eq!(meshes[0].1.faces.len(), 6);
        assert!((meshes[0].1.bounds().center().x - 5.0).abs() < 1e-5);
        assert_eq!(meshes[0].2.as_deref(), Some("red"));
        assert!(meshes[0].1.faces[0].has_uvs());
        let mats = parse_mtl(&mtl);
        let red = mats.iter().find(|m| m.name == "red").unwrap();
        assert!(red.color.x > 0.7 && red.color.y < 0.1);
    }
}
