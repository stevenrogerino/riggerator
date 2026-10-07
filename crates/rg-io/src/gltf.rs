//! glTF 2.0 export (single .gltf with an embedded base64 buffer).
//! Meshes are exported in their evaluated (deformed) pose with the node
//! hierarchy, TRS transforms and PBR materials.

use rg_core::{NodeId, NodeKind, Scene, ShaderType};
use serde_json::{json, Value};
use std::collections::HashMap;

fn b64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    s
}

pub fn export(scene: &Scene, only: Option<&[NodeId]>) -> Result<String, String> {
    let ev = scene.evaluate();
    let mut buf: Vec<u8> = Vec::new();
    let mut views: Vec<Value> = Vec::new();
    let mut accessors: Vec<Value> = Vec::new();
    let mut meshes: Vec<Value> = Vec::new();
    let mut nodes: Vec<Value> = Vec::new();
    let mut materials: Vec<Value> = Vec::new();
    let mut mat_index: HashMap<String, usize> = HashMap::new();
    let mut node_index: HashMap<NodeId, usize> = HashMap::new();

    let include = |id: NodeId| match only {
        None => true,
        Some(l) => l.iter().any(|&r| r == id || scene.is_ancestor(r, id)),
    };

    let mut push_view = |buf: &mut Vec<u8>, bytes: &[u8], target: u32| -> usize {
        while buf.len() % 4 != 0 {
            buf.push(0);
        }
        let off = buf.len();
        buf.extend_from_slice(bytes);
        views.push(json!({"buffer": 0, "byteOffset": off, "byteLength": bytes.len(), "target": target}));
        views.len() - 1
    };

    let order: Vec<NodeId> = scene.walk().into_iter().filter(|&id| include(id)).collect();
    for &id in &order {
        let n = &scene.nodes[&id];
        let t = &n.transform;
        // bake pivot & joint orient into a matrix-free TRS where possible
        let local = ev.node(scene, id).map(|n| n.local_matrix()).unwrap_or_default();
        let (s, r, tr) = local.to_scale_rotation_translation();
        let _ = t;
        let mut node = json!({
            "name": n.name,
            "translation": [tr.x, tr.y, tr.z],
            "rotation": [r.x, r.y, r.z, r.w],
            "scale": [s.x, s.y, s.z],
        });
        if let NodeKind::Mesh(ms) = &n.kind {
            if let Some(m) = ev.mesh(id) {
                let mut m = (**m).clone();
                if ms.smooth_level > 0 {
                    m = rg_core::mesh::ops::smooth(&m, ms.smooth_level.min(3));
                }
                let rm = m.to_render();
                if !rm.indices.is_empty() {
                    let pos: Vec<u8> = rm.positions.iter().flat_map(|p| [p.x, p.y, p.z]).flat_map(f32::to_le_bytes).collect();
                    let nrm: Vec<u8> = rm.normals.iter().flat_map(|p| [p.x, p.y, p.z]).flat_map(f32::to_le_bytes).collect();
                    let uv: Vec<u8> = rm.uvs.iter().flat_map(|p| [p.x, 1.0 - p.y]).flat_map(f32::to_le_bytes).collect();
                    let idx: Vec<u8> = rm.indices.iter().flat_map(|i| i.to_le_bytes()).collect();
                    let b = m.bounds();
                    let pv = push_view(&mut buf, &pos, 34962);
                    accessors.push(json!({"bufferView": pv, "componentType": 5126, "count": rm.positions.len(), "type": "VEC3",
                        "min": [b.min.x, b.min.y, b.min.z], "max": [b.max.x, b.max.y, b.max.z]}));
                    let pa = accessors.len() - 1;
                    let nv = push_view(&mut buf, &nrm, 34962);
                    accessors.push(json!({"bufferView": nv, "componentType": 5126, "count": rm.normals.len(), "type": "VEC3"}));
                    let na = accessors.len() - 1;
                    let uvv = push_view(&mut buf, &uv, 34962);
                    accessors.push(json!({"bufferView": uvv, "componentType": 5126, "count": rm.uvs.len(), "type": "VEC2"}));
                    let ua = accessors.len() - 1;
                    let iv = push_view(&mut buf, &idx, 34963);
                    accessors.push(json!({"bufferView": iv, "componentType": 5125, "count": rm.indices.len(), "type": "SCALAR"}));
                    let ia = accessors.len() - 1;
                    let mat = scene.material_for(id);
                    let mi = *mat_index.entry(mat.name.clone()).or_insert_with(|| {
                        let rough = if mat.shader == ShaderType::Lambert { 1.0 } else { mat.roughness };
                        let mut mj = json!({
                            "name": mat.name,
                            "pbrMetallicRoughness": {
                                "baseColorFactor": [mat.color.x, mat.color.y, mat.color.z, mat.opacity],
                                "metallicFactor": mat.metalness,
                                "roughnessFactor": rough,
                            },
                            "emissiveFactor": [
                                (mat.emission_color.x * mat.emission).min(1.0),
                                (mat.emission_color.y * mat.emission).min(1.0),
                                (mat.emission_color.z * mat.emission).min(1.0)
                            ],
                        });
                        if mat.opacity < 1.0 {
                            mj["alphaMode"] = json!("BLEND");
                        }
                        materials.push(mj);
                        materials.len() - 1
                    });
                    meshes.push(json!({
                        "name": format!("{}Shape", n.name),
                        "primitives": [{
                            "attributes": {"POSITION": pa, "NORMAL": na, "TEXCOORD_0": ua},
                            "indices": ia,
                            "material": mi,
                        }]
                    }));
                    node["mesh"] = json!(meshes.len() - 1);
                }
            }
        }
        nodes.push(node);
        node_index.insert(id, nodes.len() - 1);
    }
    let mut roots = Vec::new();
    for &id in &order {
        let n = &scene.nodes[&id];
        let kids: Vec<usize> = n.children.iter().filter_map(|c| node_index.get(c).copied()).collect();
        if !kids.is_empty() {
            nodes[node_index[&id]]["children"] = json!(kids);
        }
        if n.parent.is_none_or(|p| !node_index.contains_key(&p)) {
            roots.push(node_index[&id]);
        }
    }
    let doc = json!({
        "asset": {"version": "2.0", "generator": "Riggerator"},
        "scene": 0,
        "scenes": [{"nodes": roots}],
        "nodes": nodes,
        "meshes": meshes,
        "materials": materials,
        "accessors": accessors,
        "bufferViews": views,
        "buffers": [{"byteLength": buf.len(), "uri": format!("data:application/octet-stream;base64,{}", b64(&buf))}],
    });
    serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rg_core::mesh::primitives;

    #[test]
    fn base64() {
        assert_eq!(b64(b"Man"), "TWFu");
        assert_eq!(b64(b"Ma"), "TWE=");
        assert_eq!(b64(b"M"), "TQ==");
    }

    #[test]
    fn exports_valid_json() {
        let mut s = Scene::new();
        let g = s.add_node("group", NodeKind::Transform, None);
        let c = s.add_mesh("pCube", primitives::cube(1.0, 1.0, 1.0, 1, 1, 1));
        s.set_parent(c, Some(g), false).unwrap();
        let out = export(&s, None).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["nodes"].as_array().unwrap().len(), 2);
        assert_eq!(v["scenes"][0]["nodes"].as_array().unwrap().len(), 1);
        assert_eq!(v["accessors"][3]["count"], 36);
    }
}
