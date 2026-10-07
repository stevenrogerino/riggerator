//! rg-io — file formats.
//!
//! * `.rgs`  Riggerator scene (JSON, lossless)
//! * `.obj`/`.mtl`  Wavefront geometry import/export
//! * `.ma`  Maya ASCII subset import/export (transforms, meshes, poly*
//!   creator nodes, joints, cameras, lights, lambert/blinn/phong shaders,
//!   animation curves)
//! * `.gltf`  glTF 2.0 export with embedded buffers

pub mod gltf;
pub mod ma;
pub mod obj;

use rg_core::Scene;
use std::path::Path;

pub const SCENE_FORMAT: &str = "riggerator-scene";
pub const SCENE_VERSION: u32 = 1;

#[derive(serde::Serialize, serde::Deserialize)]
struct Envelope {
    format: String,
    version: u32,
    scene: Scene,
}

pub fn save_scene(scene: &Scene, path: &Path) -> Result<(), String> {
    let env = Envelope { format: SCENE_FORMAT.into(), version: SCENE_VERSION, scene: scene.clone() };
    let s = serde_json::to_string_pretty(&env).map_err(|e| e.to_string())?;
    std::fs::write(path, s).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn load_scene(path: &Path) -> Result<Scene, String> {
    let s = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let v: serde_json::Value = serde_json::from_str(&s).map_err(|e| e.to_string())?;
    let scene_json = if v.get("format").and_then(|f| f.as_str()) == Some(SCENE_FORMAT) {
        v.get("scene").cloned().ok_or("missing scene")?
    } else {
        v
    };
    Scene::from_json(&scene_json.to_string())
}

/// Open any supported file into a fresh scene, or import into an existing one.
pub fn import_file(scene: &mut Scene, path: &Path) -> Result<Vec<rg_core::NodeId>, String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "obj" => obj::import(scene, path),
        "ma" => {
            let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
            ma::import(scene, &text)
        }
        "rgs" | "json" => {
            let other = load_scene(path)?;
            Ok(merge_scene(scene, other))
        }
        _ => Err(format!("unsupported file type: .{ext}")),
    }
}

pub fn export_file(scene: &Scene, path: &Path, only: Option<&[rg_core::NodeId]>) -> Result<(), String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "obj" => obj::export(scene, path, only),
        "ma" => std::fs::write(path, ma::export(scene, only)).map_err(|e| e.to_string()),
        "gltf" => std::fs::write(path, gltf::export(scene, only)?).map_err(|e| e.to_string()),
        "rgs" => save_scene(scene, path),
        _ => Err(format!("unsupported export type: .{ext}")),
    }
}

/// Merge another scene's nodes and materials into `scene` (File > Import).
pub fn merge_scene(scene: &mut Scene, other: Scene) -> Vec<rg_core::NodeId> {
    use std::collections::HashMap;
    let mut map: HashMap<rg_core::NodeId, rg_core::NodeId> = HashMap::new();
    let mut mat_map: HashMap<String, String> = HashMap::new();
    for (name, m) in &other.materials {
        if name == "lambert1" {
            mat_map.insert(name.clone(), name.clone());
            continue;
        }
        let new = scene.unique_name(name);
        let mut m = m.clone();
        m.name = new.clone();
        scene.materials.insert(new.clone(), m);
        mat_map.insert(name.clone(), new);
    }
    let mut roots = Vec::new();
    for id in other.walk() {
        let n = &other.nodes[&id];
        let parent = n.parent.and_then(|p| map.get(&p).copied());
        let mut kind = n.kind.clone();
        if let rg_core::NodeKind::Mesh(ms) = &mut kind {
            ms.material = ms.material.as_ref().and_then(|m| mat_map.get(m).cloned());
            ms.skin = None;
        }
        if let rg_core::NodeKind::IkHandle(_) = kind {
            continue;
        }
        let nid = scene.add_node(&n.name, kind, parent);
        let dst = scene.nodes.get_mut(&nid).unwrap();
        dst.transform = n.transform.clone();
        dst.visible = n.visible;
        dst.user_attrs = n.user_attrs.clone();
        dst.anim = n.anim.clone();
        dst.color = n.color;
        map.insert(id, nid);
        if parent.is_none() {
            roots.push(nid);
        }
    }
    scene.touch();
    roots
}

#[cfg(test)]
mod tests {
    use super::*;
    use rg_core::mesh::primitives;

    #[test]
    fn rgs_roundtrip() {
        let dir = std::env::temp_dir().join(format!("rgio_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = Scene::new();
        let c = s.add_mesh("pCube", primitives::cube(1.0, 1.0, 1.0, 1, 1, 1));
        s.set_key(c, "ty", 5.0, Some(3.0)).unwrap();
        let p = dir.join("a.rgs");
        save_scene(&s, &p).unwrap();
        let back = load_scene(&p).unwrap();
        assert_eq!(back.nodes.len(), 1);
        assert_eq!(back.nodes[&c].anim["translateY"].len(), 1);

        let mut merged = Scene::new();
        merged.add_mesh("pCube", primitives::cube(1.0, 1.0, 1.0, 1, 1, 1));
        let roots = import_file(&mut merged, &p).unwrap();
        assert_eq!(roots.len(), 1);
        assert_eq!(merged.name_of(roots[0]), "pCube2");
    }
}
