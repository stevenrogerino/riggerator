//! rg-mesh — the polygon kernel behind Riggerator.
//!
//! A [`Mesh`] is a list of positions plus n-gon faces that index them. Faces
//! may carry face-varying UV indices. Every modeling operation (extrude,
//! smooth, edge loops, merge, bridge…) lives in [`ops`]; primitive builders
//! (polyCube, polySphere…) live in [`primitives`].

use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod ops;
pub mod primitives;
mod triangulate;

pub use triangulate::triangulate_polygon;

/// A single polygon. `uvs` is either empty or the same length as `verts`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Face {
    pub verts: Vec<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uvs: Vec<u32>,
}

impl Face {
    pub fn new(verts: Vec<u32>) -> Self {
        Self { verts, uvs: Vec::new() }
    }
    pub fn with_uvs(verts: Vec<u32>, uvs: Vec<u32>) -> Self {
        debug_assert_eq!(verts.len(), uvs.len());
        Self { verts, uvs }
    }
    pub fn len(&self) -> usize {
        self.verts.len()
    }
    pub fn is_empty(&self) -> bool {
        self.verts.is_empty()
    }
    pub fn has_uvs(&self) -> bool {
        !self.uvs.is_empty() && self.uvs.len() == self.verts.len()
    }
    /// Directed edges of this face, in winding order.
    pub fn edges(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        let n = self.verts.len();
        (0..n).map(move |i| (self.verts[i], self.verts[(i + 1) % n]))
    }
}

/// Undirected edge key with `a < b`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Edge(pub u32, pub u32);

impl Edge {
    pub fn new(a: u32, b: u32) -> Self {
        if a < b { Edge(a, b) } else { Edge(b, a) }
    }
    pub fn other(&self, v: u32) -> u32 {
        if self.0 == v { self.1 } else { self.0 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mesh {
    pub positions: Vec<Vec3>,
    #[serde(default)]
    pub uvs: Vec<Vec2>,
    pub faces: Vec<Face>,
    /// Edges whose dihedral angle (degrees) exceeds this render hard.
    /// 180 = everything soft, 0 = everything hard.
    #[serde(default = "default_angle")]
    pub normal_angle: f32,
}

fn default_angle() -> f32 {
    180.0
}

impl Default for Mesh {
    fn default() -> Self {
        Self { positions: Vec::new(), uvs: Vec::new(), faces: Vec::new(), normal_angle: 180.0 }
    }
}

/// Triangulated, render-ready data with split normals.
#[derive(Clone, Debug, Default)]
pub struct RenderMesh {
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub uvs: Vec<Vec2>,
    pub indices: Vec<u32>,
    /// Source face of each triangle.
    pub tri_face: Vec<u32>,
}

#[derive(Clone, Copy, Debug)]
pub struct RayHit {
    pub t: f32,
    pub face: u32,
    pub point: Vec3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub const EMPTY: Aabb = Aabb { min: Vec3::splat(f32::INFINITY), max: Vec3::splat(f32::NEG_INFINITY) };
    pub fn is_empty(&self) -> bool {
        self.min.x > self.max.x
    }
    pub fn extend(&mut self, p: Vec3) {
        self.min = self.min.min(p);
        self.max = self.max.max(p);
    }
    pub fn union(&self, o: &Aabb) -> Aabb {
        Aabb { min: self.min.min(o.min), max: self.max.max(o.max) }
    }
    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }
    pub fn size(&self) -> Vec3 {
        self.max - self.min
    }
}

impl Mesh {
    pub fn new(positions: Vec<Vec3>, faces: Vec<Face>) -> Self {
        Self { positions, faces, ..Default::default() }
    }

    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }
    pub fn face_count(&self) -> usize {
        self.faces.len()
    }

    /// Unique undirected edges in first-seen order.
    pub fn edges(&self) -> Vec<Edge> {
        let mut seen = HashMap::new();
        let mut out = Vec::new();
        for f in &self.faces {
            for (a, b) in f.edges() {
                let e = Edge::new(a, b);
                if seen.insert(e, ()).is_none() {
                    out.push(e);
                }
            }
        }
        out
    }

    /// Map from undirected edge to the faces that use it.
    pub fn edge_faces(&self) -> HashMap<Edge, Vec<u32>> {
        let mut m: HashMap<Edge, Vec<u32>> = HashMap::new();
        for (fi, f) in self.faces.iter().enumerate() {
            for (a, b) in f.edges() {
                m.entry(Edge::new(a, b)).or_default().push(fi as u32);
            }
        }
        m
    }

    /// For every vertex, the faces that touch it.
    pub fn vertex_faces(&self) -> Vec<Vec<u32>> {
        let mut vf = vec![Vec::new(); self.positions.len()];
        for (fi, f) in self.faces.iter().enumerate() {
            for &v in &f.verts {
                vf[v as usize].push(fi as u32);
            }
        }
        vf
    }

    /// Edges used by exactly one face.
    pub fn boundary_edges(&self) -> Vec<Edge> {
        let ef = self.edge_faces();
        let mut b: Vec<Edge> = ef.into_iter().filter(|(_, f)| f.len() == 1).map(|(e, _)| e).collect();
        b.sort();
        b
    }

    pub fn face_normal(&self, fi: usize) -> Vec3 {
        newell_normal(self.faces[fi].verts.iter().map(|&v| self.positions[v as usize]))
    }

    pub fn face_center(&self, fi: usize) -> Vec3 {
        let f = &self.faces[fi];
        let s: Vec3 = f.verts.iter().map(|&v| self.positions[v as usize]).sum();
        s / f.verts.len().max(1) as f32
    }

    pub fn face_area(&self, fi: usize) -> f32 {
        let f = &self.faces[fi];
        let mut n = Vec3::ZERO;
        let k = f.verts.len();
        for i in 0..k {
            let a = self.positions[f.verts[i] as usize];
            let b = self.positions[f.verts[(i + 1) % k] as usize];
            n += a.cross(b);
        }
        n.length() * 0.5
    }

    pub fn bounds(&self) -> Aabb {
        let mut b = Aabb::EMPTY;
        for &p in &self.positions {
            b.extend(p);
        }
        b
    }

    /// Smooth, area-weighted vertex normals (ignores `normal_angle`).
    pub fn vertex_normals(&self) -> Vec<Vec3> {
        let mut n = vec![Vec3::ZERO; self.positions.len()];
        for (fi, f) in self.faces.iter().enumerate() {
            let fnrm = self.face_normal(fi) * self.face_area(fi).max(1e-12);
            for &v in &f.verts {
                n[v as usize] += fnrm;
            }
        }
        n.into_iter().map(|v| v.normalize_or_zero()).collect()
    }

    /// Append another mesh, returning the vertex offset used.
    pub fn append(&mut self, other: &Mesh) -> u32 {
        let vo = self.positions.len() as u32;
        let uo = self.uvs.len() as u32;
        self.positions.extend_from_slice(&other.positions);
        self.uvs.extend_from_slice(&other.uvs);
        for f in &other.faces {
            self.faces.push(Face {
                verts: f.verts.iter().map(|v| v + vo).collect(),
                uvs: f.uvs.iter().map(|u| u + uo).collect(),
            });
        }
        vo
    }

    pub fn transformed(&self, m: glam::Mat4) -> Mesh {
        let mut out = self.clone();
        for p in &mut out.positions {
            *p = m.transform_point3(*p);
        }
        if m.determinant() < 0.0 {
            for f in &mut out.faces {
                f.verts.reverse();
                f.uvs.reverse();
            }
        }
        out
    }

    /// Triangulate with split normals honoring `normal_angle`.
    pub fn to_render(&self) -> RenderMesh {
        let fnormals: Vec<Vec3> = (0..self.faces.len()).map(|i| self.face_normal(i)).collect();
        let vf = self.vertex_faces();
        let cos_thresh = self.normal_angle.clamp(0.0, 180.0).to_radians().cos();
        let all_soft = self.normal_angle >= 179.9;
        let smooth = if all_soft { self.vertex_normals() } else { Vec::new() };

        let mut rm = RenderMesh::default();
        for (fi, f) in self.faces.iter().enumerate() {
            if f.verts.len() < 3 {
                continue;
            }
            let fnrm = fnormals[fi];
            let base = rm.positions.len() as u32;
            for (ci, &v) in f.verts.iter().enumerate() {
                rm.positions.push(self.positions[v as usize]);
                let n = if all_soft {
                    smooth[v as usize]
                } else {
                    let mut acc = Vec3::ZERO;
                    for &of in &vf[v as usize] {
                        let on = fnormals[of as usize];
                        if on.dot(fnrm) >= cos_thresh - 1e-5 {
                            acc += on * self.face_area(of as usize).max(1e-12);
                        }
                    }
                    let acc = acc.normalize_or_zero();
                    if acc == Vec3::ZERO { fnrm } else { acc }
                };
                rm.normals.push(n);
                rm.uvs.push(if f.has_uvs() {
                    self.uvs.get(f.uvs[ci] as usize).copied().unwrap_or(Vec2::ZERO)
                } else {
                    Vec2::ZERO
                });
            }
            let pts: Vec<Vec3> = f.verts.iter().map(|&v| self.positions[v as usize]).collect();
            for [a, b, c] in triangulate_polygon(&pts, fnrm) {
                rm.indices.extend_from_slice(&[base + a, base + b, base + c]);
                rm.tri_face.push(fi as u32);
            }
        }
        rm
    }

    /// Closest ray hit against this mesh (object space).
    pub fn raycast(&self, origin: Vec3, dir: Vec3) -> Option<RayHit> {
        let mut best: Option<RayHit> = None;
        for (fi, f) in self.faces.iter().enumerate() {
            if f.verts.len() < 3 {
                continue;
            }
            let p0 = self.positions[f.verts[0] as usize];
            for i in 1..f.verts.len() - 1 {
                let p1 = self.positions[f.verts[i] as usize];
                let p2 = self.positions[f.verts[i + 1] as usize];
                if let Some(t) = ray_triangle(origin, dir, p0, p1, p2) {
                    if best.map_or(true, |b| t < b.t) {
                        best = Some(RayHit { t, face: fi as u32, point: origin + dir * t });
                    }
                }
            }
        }
        best
    }

    /// Remove unused vertices/uvs and faces with fewer than three verts.
    pub fn compact(&mut self) {
        self.faces.retain(|f| f.verts.len() >= 3);
        let mut vmap = vec![u32::MAX; self.positions.len()];
        let mut umap = vec![u32::MAX; self.uvs.len()];
        let mut np = Vec::new();
        let mut nu = Vec::new();
        for f in &mut self.faces {
            for v in &mut f.verts {
                let vi = *v as usize;
                if vmap[vi] == u32::MAX {
                    vmap[vi] = np.len() as u32;
                    np.push(self.positions[vi]);
                }
                *v = vmap[vi];
            }
            for u in &mut f.uvs {
                let ui = *u as usize;
                if ui >= umap.len() {
                    continue;
                }
                if umap[ui] == u32::MAX {
                    umap[ui] = nu.len() as u32;
                    nu.push(self.uvs[ui]);
                }
                *u = umap[ui];
            }
        }
        self.positions = np;
        self.uvs = nu;
    }

    /// Basic sanity checks; returns a list of human-readable problems.
    pub fn validate(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (fi, f) in self.faces.iter().enumerate() {
            if f.verts.len() < 3 {
                out.push(format!("face {fi} has {} verts", f.verts.len()));
            }
            for &v in &f.verts {
                if v as usize >= self.positions.len() {
                    out.push(format!("face {fi} references missing vertex {v}"));
                }
            }
            if !f.uvs.is_empty() && f.uvs.len() != f.verts.len() {
                out.push(format!("face {fi} uv count mismatch"));
            }
        }
        let mut directed = HashMap::new();
        for f in &self.faces {
            for e in f.edges() {
                *directed.entry(e).or_insert(0) += 1;
            }
        }
        for (e, c) in directed {
            if c > 1 {
                out.push(format!("directed edge {e:?} used {c} times (non-manifold or flipped)"));
            }
        }
        out
    }
}

pub fn newell_normal(pts: impl Iterator<Item = Vec3> + Clone) -> Vec3 {
    let v: Vec<Vec3> = pts.collect();
    let mut n = Vec3::ZERO;
    for i in 0..v.len() {
        let a = v[i];
        let b = v[(i + 1) % v.len()];
        n.x += (a.y - b.y) * (a.z + b.z);
        n.y += (a.z - b.z) * (a.x + b.x);
        n.z += (a.x - b.x) * (a.y + b.y);
    }
    n.normalize_or_zero()
}

pub fn ray_triangle(o: Vec3, d: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let e1 = b - a;
    let e2 = c - a;
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t > 1e-6).then_some(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cube_is_closed_and_valid() {
        let m = primitives::cube(1.0, 1.0, 1.0, 1, 1, 1);
        assert_eq!(m.positions.len(), 8);
        assert_eq!(m.faces.len(), 6);
        assert!(m.validate().is_empty(), "{:?}", m.validate());
        assert!(m.boundary_edges().is_empty());
        assert_eq!(m.edges().len(), 12);
    }

    #[test]
    fn cube_normals_point_out() {
        let m = primitives::cube(2.0, 2.0, 2.0, 1, 1, 1);
        for fi in 0..m.faces.len() {
            let n = m.face_normal(fi);
            let c = m.face_center(fi);
            assert!(n.dot(c) > 0.0, "face {fi} points inward");
        }
    }

    #[test]
    fn raycast_hits_front() {
        let m = primitives::cube(2.0, 2.0, 2.0, 1, 1, 1);
        let hit = m.raycast(Vec3::new(0.0, 0.0, 10.0), Vec3::NEG_Z).unwrap();
        assert!((hit.t - 9.0).abs() < 1e-4);
        assert!(m.face_normal(hit.face as usize).z > 0.9);
    }

    #[test]
    fn render_mesh_tri_count() {
        let m = primitives::cube(1.0, 1.0, 1.0, 1, 1, 1);
        let r = m.to_render();
        assert_eq!(r.indices.len(), 36);
        assert_eq!(r.tri_face.len(), 12);
    }
}
