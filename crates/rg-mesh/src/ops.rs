//! Modeling operations. Functions mutate the mesh in place and take face /
//! edge / vertex indices for component-level tools.

use crate::{newell_normal, triangulate_polygon, Edge, Face, Mesh};
use glam::{Vec2, Vec3};
use std::collections::{HashMap, HashSet};

// ---------------------------------------------------------------- merge ---

/// Merge vertices closer than `tol`. If `subset` is given, only those
/// vertices participate. Degenerate faces are removed and the mesh compacted.
pub fn merge_vertices(m: &mut Mesh, subset: Option<&[u32]>, tol: f32) -> usize {
    let n = m.positions.len();
    let active: Vec<bool> = match subset {
        Some(s) => {
            let mut a = vec![false; n];
            for &v in s {
                if (v as usize) < n {
                    a[v as usize] = true;
                }
            }
            a
        }
        None => vec![true; n],
    };
    let cell = tol.max(1e-9) * 2.0;
    let key = |p: Vec3| ((p.x / cell).floor() as i64, (p.y / cell).floor() as i64, (p.z / cell).floor() as i64);
    let mut grid: HashMap<(i64, i64, i64), Vec<u32>> = HashMap::new();
    let mut remap: Vec<u32> = (0..n as u32).collect();
    let mut merged = 0;
    for i in 0..n {
        if !active[i] {
            continue;
        }
        let p = m.positions[i];
        let (kx, ky, kz) = key(p);
        let mut found = None;
        'search: for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(list) = grid.get(&(kx + dx, ky + dy, kz + dz)) {
                        for &j in list {
                            if m.positions[j as usize].distance(p) <= tol {
                                found = Some(j);
                                break 'search;
                            }
                        }
                    }
                }
            }
        }
        match found {
            Some(j) => {
                remap[i] = j;
                merged += 1;
            }
            None => grid.entry((kx, ky, kz)).or_default().push(i as u32),
        }
    }
    if merged == 0 {
        return 0;
    }
    for f in &mut m.faces {
        let has_uv = f.has_uvs();
        let mut nv = Vec::with_capacity(f.verts.len());
        let mut nu = Vec::with_capacity(f.uvs.len());
        for (i, &v) in f.verts.iter().enumerate() {
            let r = remap[v as usize];
            if nv.last() == Some(&r) {
                continue;
            }
            nv.push(r);
            if has_uv {
                nu.push(f.uvs[i]);
            }
        }
        while nv.len() > 1 && nv.first() == nv.last() {
            nv.pop();
            if has_uv {
                nu.pop();
            }
        }
        f.verts = nv;
        f.uvs = nu;
    }
    m.compact();
    merged
}

/// Collapse the given vertices to their centroid (Merge To Center).
pub fn merge_to_center(m: &mut Mesh, verts: &[u32]) {
    if verts.is_empty() {
        return;
    }
    let c: Vec3 = verts.iter().map(|&v| m.positions[v as usize]).sum::<Vec3>() / verts.len() as f32;
    for &v in verts {
        m.positions[v as usize] = c;
    }
    merge_vertices(m, Some(verts), 1e-6);
}

// -------------------------------------------------------------- extrude ---

/// Extrude faces keeping them together (polyExtrudeFacet, keepFacesTogether=1).
/// `thickness` moves along the averaged region normal, `offset` insets toward
/// each new vertex's adjacent face centers. Returns the indices of the new
/// cap faces (same indices as the input faces).
pub fn extrude_faces(m: &mut Mesh, faces: &[u32], thickness: f32, offset: f32) -> Vec<u32> {
    let sel: HashSet<u32> = faces.iter().copied().collect();
    if sel.is_empty() {
        return Vec::new();
    }
    // Directed edge counts within the selection to find the region border.
    let mut dir_edges: HashSet<(u32, u32)> = HashSet::new();
    for &fi in &sel {
        for e in m.faces[fi as usize].edges() {
            dir_edges.insert(e);
        }
    }
    let mut border: Vec<(u32, u32)> = Vec::new();
    let mut ordered: Vec<u32> = sel.iter().copied().collect();
    ordered.sort();
    for &fi in &ordered {
        for (a, b) in m.faces[fi as usize].edges() {
            if !dir_edges.contains(&(b, a)) {
                border.push((a, b));
            }
        }
    }
    // Region vertex normals and face-center accumulators.
    let mut vnorm: HashMap<u32, Vec3> = HashMap::new();
    let mut vcent: HashMap<u32, (Vec3, f32)> = HashMap::new();
    for &fi in &ordered {
        let n = m.face_normal(fi as usize);
        let c = m.face_center(fi as usize);
        for &v in &m.faces[fi as usize].verts {
            *vnorm.entry(v).or_insert(Vec3::ZERO) += n;
            let e = vcent.entry(v).or_insert((Vec3::ZERO, 0.0));
            e.0 += c;
            e.1 += 1.0;
        }
    }
    let mut newv: HashMap<u32, u32> = HashMap::new();
    let mut keys: Vec<u32> = vnorm.keys().copied().collect();
    keys.sort();
    for v in keys {
        let p = m.positions[v as usize];
        let mut n = vnorm[&v].normalize_or_zero();
        // Scale so flat sides stay parallel (like Maya's corner compensation).
        let faces_n: Vec<Vec3> = ordered
            .iter()
            .filter(|&&f| m.faces[f as usize].verts.contains(&v))
            .map(|&f| m.face_normal(f as usize))
            .collect();
        let min_dot = faces_n.iter().map(|fnm| fnm.dot(n)).fold(1.0f32, f32::min).max(0.25);
        n /= min_dot;
        let (cs, cn) = vcent[&v];
        let center = cs / cn;
        let np = p + n * thickness + (center - p) * offset;
        newv.insert(v, m.positions.len() as u32);
        m.positions.push(np);
    }
    for &fi in &ordered {
        for v in &mut m.faces[fi as usize].verts {
            *v = newv[v];
        }
    }
    for (a, b) in border {
        m.faces.push(Face::new(vec![a, b, newv[&b], newv[&a]]));
    }
    ordered
}

/// Extrude each face separately (keepFacesTogether=0).
pub fn extrude_faces_individual(m: &mut Mesh, faces: &[u32], thickness: f32, offset: f32) {
    for &f in faces {
        extrude_faces(m, &[f], thickness, offset);
    }
}

/// Inset each face individually by `amount` (0..1 toward its center).
pub fn inset_faces(m: &mut Mesh, faces: &[u32], amount: f32) {
    extrude_faces_individual(m, faces, 0.0, amount.clamp(0.0, 0.999));
}

/// Extrude boundary edges outward/along a direction (polyExtrudeEdge).
pub fn extrude_edges(m: &mut Mesh, edges: &[Edge], translate: Vec3) {
    let ef = m.edge_faces();
    let mut newv: HashMap<u32, u32> = HashMap::new();
    for e in edges {
        let Some(fs) = ef.get(e) else { continue };
        let f = &m.faces[fs[0] as usize];
        // find direction a->b used by the face
        let (a, b) = f.edges().find(|&(x, y)| Edge::new(x, y) == *e).unwrap();
        for v in [a, b] {
            if !newv.contains_key(&v) {
                newv.insert(v, m.positions.len() as u32);
                let p = m.positions[v as usize] + translate;
                m.positions.push(p);
            }
        }
        m.faces.push(Face::new(vec![b, a, newv[&a], newv[&b]]));
    }
}

// ------------------------------------------------------------- deletion ---

pub fn delete_faces(m: &mut Mesh, faces: &[u32]) {
    let del: HashSet<u32> = faces.iter().copied().collect();
    let mut i = 0;
    m.faces.retain(|_| {
        let keep = !del.contains(&i);
        i += 1;
        keep
    });
    m.compact();
}

/// Delete vertices and all faces that use them.
pub fn delete_vertices(m: &mut Mesh, verts: &[u32]) {
    let del: HashSet<u32> = verts.iter().copied().collect();
    m.faces.retain(|f| !f.verts.iter().any(|v| del.contains(v)));
    m.compact();
}

/// Dissolve edges, merging the two faces on either side (Delete Edge).
pub fn dissolve_edges(m: &mut Mesh, edges: &[Edge]) {
    for e in edges {
        let ef = m.edge_faces();
        let Some(fs) = ef.get(e) else { continue };
        if fs.len() != 2 || fs[0] == fs[1] {
            continue;
        }
        let (fa, fb) = (fs[0] as usize, fs[1] as usize);
        let a = &m.faces[fa];
        let b = &m.faces[fb];
        // In face A the edge runs x->y; in B it runs y->x.
        let ia = a.verts.iter().position(|&v| {
            let n = a.verts.len();
            let i = a.verts.iter().position(|&w| w == v).unwrap();
            Edge::new(v, a.verts[(i + 1) % n]) == *e
        });
        let Some(ia) = ia else { continue };
        let na = a.verts.len();
        let x = a.verts[ia];
        let y = a.verts[(ia + 1) % na];
        let nb = b.verts.len();
        let Some(ib) = b.verts.iter().position(|&v| v == y) else { continue };
        if b.verts[(ib + 1) % nb] != x {
            continue; // inconsistent winding
        }
        let mut verts = Vec::new();
        // walk A from y around to x
        for k in 0..na {
            verts.push(a.verts[(ia + 1 + k) % na]);
        }
        // verts now: y ... x ; walk B from after x around to before y
        for k in 2..nb {
            verts.push(b.verts[(ib + k) % nb]);
        }
        let merged = Face::new(verts);
        let (lo, hi) = if fa < fb { (fa, fb) } else { (fb, fa) };
        m.faces.remove(hi);
        m.faces[lo] = merged;
    }
    m.compact();
}

// ---------------------------------------------------------------- holes ---

/// Boundary loops as vertex lists, oriented along the faces' winding.
pub fn boundary_loops(m: &Mesh) -> Vec<Vec<u32>> {
    let mut directed: HashSet<(u32, u32)> = HashSet::new();
    for f in &m.faces {
        for e in f.edges() {
            directed.insert(e);
        }
    }
    let mut next: HashMap<u32, u32> = HashMap::new();
    let mut starts: Vec<u32> = Vec::new();
    for f in &m.faces {
        for (a, b) in f.edges() {
            if !directed.contains(&(b, a)) {
                next.insert(a, b);
                starts.push(a);
            }
        }
    }
    let mut used: HashSet<u32> = HashSet::new();
    let mut loops = Vec::new();
    for s in starts {
        if used.contains(&s) {
            continue;
        }
        let mut lp = vec![s];
        used.insert(s);
        let mut cur = s;
        let mut ok = false;
        while let Some(&n) = next.get(&cur) {
            if n == s {
                ok = true;
                break;
            }
            if !used.insert(n) {
                break;
            }
            lp.push(n);
            cur = n;
        }
        if ok && lp.len() >= 3 {
            loops.push(lp);
        }
    }
    loops
}

/// Fill holes. If `touching` is non-empty, only holes containing one of
/// those vertices are filled. Returns the number of holes filled.
pub fn fill_holes(m: &mut Mesh, touching: &[u32]) -> usize {
    let t: HashSet<u32> = touching.iter().copied().collect();
    let mut n = 0;
    for lp in boundary_loops(m) {
        if !t.is_empty() && !lp.iter().any(|v| t.contains(v)) {
            continue;
        }
        let mut f = lp.clone();
        f.reverse();
        m.faces.push(Face::new(f));
        n += 1;
    }
    n
}

/// Bridge two boundary loops with quads. Loops must have equal vertex count.
pub fn bridge_loops(m: &mut Mesh, a: &[u32], b: &[u32]) -> Result<(), String> {
    if a.len() != b.len() || a.len() < 3 {
        return Err(format!("bridge needs matching border loops ({} vs {})", a.len(), b.len()));
    }
    let n = a.len();
    let br: Vec<u32> = b.iter().rev().copied().collect();
    let mut best = (f32::INFINITY, 0);
    for off in 0..n {
        let d: f32 = (0..n).map(|i| m.positions[a[i] as usize].distance(m.positions[br[(i + off) % n] as usize])).sum();
        if d < best.0 {
            best = (d, off);
        }
    }
    let off = best.1;
    for i in 0..n {
        let j = (i + off) % n;
        m.faces.push(Face::new(vec![a[(i + 1) % n], a[i], br[j], br[(j + 1) % n]]));
    }
    Ok(())
}

/// Bridge: find the border loops that contain the given edges and bridge them.
pub fn bridge_edges(m: &mut Mesh, edges: &[Edge]) -> Result<(), String> {
    let loops = boundary_loops(m);
    let mut chosen: Vec<Vec<u32>> = Vec::new();
    for lp in loops {
        let set: HashSet<u32> = lp.iter().copied().collect();
        if edges.iter().any(|e| set.contains(&e.0) && set.contains(&e.1)) || edges.is_empty() {
            chosen.push(lp);
        }
    }
    if chosen.len() != 2 {
        return Err(format!("bridge needs exactly two border loops, found {}", chosen.len()));
    }
    let (a, b) = (chosen[0].clone(), chosen[1].clone());
    bridge_loops(m, &a, &b)
}

// ------------------------------------------------------------ topology ---

pub fn reverse_faces(m: &mut Mesh, faces: Option<&[u32]>) {
    let all: Vec<u32>;
    let list = match faces {
        Some(f) => f,
        None => {
            all = (0..m.faces.len() as u32).collect();
            &all
        }
    };
    for &f in list {
        let f = &mut m.faces[f as usize];
        f.verts.reverse();
        f.uvs.reverse();
    }
}

pub fn triangulate(m: &mut Mesh) {
    let mut out = Vec::with_capacity(m.faces.len() * 2);
    for f in &m.faces {
        if f.verts.len() == 3 {
            out.push(f.clone());
            continue;
        }
        let pts: Vec<Vec3> = f.verts.iter().map(|&v| m.positions[v as usize]).collect();
        let n = newell_normal(pts.iter().copied());
        for [a, b, c] in triangulate_polygon(&pts, n) {
            let (a, b, c) = (a as usize, b as usize, c as usize);
            let uvs = if f.has_uvs() { vec![f.uvs[a], f.uvs[b], f.uvs[c]] } else { Vec::new() };
            out.push(Face { verts: vec![f.verts[a], f.verts[b], f.verts[c]], uvs });
        }
    }
    m.faces = out;
}

/// Merge pairs of triangles sharing an edge into quads when nearly coplanar.
pub fn quadrangulate(m: &mut Mesh, angle_deg: f32) {
    let cos_t = angle_deg.to_radians().cos();
    let ef = m.edge_faces();
    let mut used: HashSet<u32> = HashSet::new();
    let mut edges: Vec<(&Edge, &Vec<u32>)> = ef.iter().collect();
    edges.sort_by_key(|(e, _)| **e);
    let mut dissolve = Vec::new();
    for (e, fs) in edges {
        if fs.len() != 2 {
            continue;
        }
        let (a, b) = (fs[0], fs[1]);
        if used.contains(&a) || used.contains(&b) {
            continue;
        }
        if m.faces[a as usize].len() != 3 || m.faces[b as usize].len() != 3 {
            continue;
        }
        if m.face_normal(a as usize).dot(m.face_normal(b as usize)) < cos_t {
            continue;
        }
        used.insert(a);
        used.insert(b);
        dissolve.push(*e);
    }
    dissolve_edges(m, &dissolve);
}

/// Poke: add a center vertex to each face and fan triangles to it.
pub fn poke_faces(m: &mut Mesh, faces: &[u32]) {
    let sel: HashSet<u32> = faces.iter().copied().collect();
    let mut out = Vec::new();
    for (fi, f) in m.faces.clone().iter().enumerate() {
        if !sel.contains(&(fi as u32)) {
            out.push(f.clone());
            continue;
        }
        let c = m.face_center(fi);
        let ci = m.positions.len() as u32;
        m.positions.push(c);
        let n = f.verts.len();
        for i in 0..n {
            out.push(Face::new(vec![f.verts[i], f.verts[(i + 1) % n], ci]));
        }
    }
    m.faces = out;
}

/// Catmull-Clark subdivision (polySmooth). UVs are interpolated linearly.
pub fn smooth(m: &Mesh, levels: u32) -> Mesh {
    let mut cur = m.clone();
    for _ in 0..levels {
        cur = catmull_clark(&cur);
    }
    cur
}

fn catmull_clark(m: &Mesh) -> Mesh {
    let nv = m.positions.len();
    let edges = m.edges();
    let eidx: HashMap<Edge, u32> = edges.iter().enumerate().map(|(i, e)| (*e, i as u32)).collect();
    let ef = m.edge_faces();
    let fpts: Vec<Vec3> = (0..m.faces.len()).map(|i| m.face_center(i)).collect();

    let mut epts = Vec::with_capacity(edges.len());
    for e in &edges {
        let (a, b) = (m.positions[e.0 as usize], m.positions[e.1 as usize]);
        let fs = &ef[e];
        if fs.len() == 2 {
            epts.push((a + b + fpts[fs[0] as usize] + fpts[fs[1] as usize]) * 0.25);
        } else {
            epts.push((a + b) * 0.5);
        }
    }

    // vertex adjacency
    let mut v_edges: Vec<Vec<Edge>> = vec![Vec::new(); nv];
    for e in &edges {
        v_edges[e.0 as usize].push(*e);
        v_edges[e.1 as usize].push(*e);
    }
    let vf = m.vertex_faces();
    let mut vpts = Vec::with_capacity(nv);
    for v in 0..nv {
        let p = m.positions[v];
        let es = &v_edges[v];
        let boundary: Vec<&Edge> = es.iter().filter(|e| ef[e].len() == 1).collect();
        if es.is_empty() {
            vpts.push(p);
        } else if !boundary.is_empty() {
            if boundary.len() == 2 {
                let a = m.positions[boundary[0].other(v as u32) as usize];
                let b = m.positions[boundary[1].other(v as u32) as usize];
                vpts.push((p * 6.0 + a + b) / 8.0);
            } else {
                vpts.push(p);
            }
        } else {
            let n = vf[v].len() as f32;
            let q: Vec3 = vf[v].iter().map(|&f| fpts[f as usize]).sum::<Vec3>() / n;
            let r: Vec3 = es
                .iter()
                .map(|e| (m.positions[e.0 as usize] + m.positions[e.1 as usize]) * 0.5)
                .sum::<Vec3>()
                / es.len() as f32;
            let k = es.len() as f32;
            vpts.push((q + r * 2.0 + p * (k - 3.0)) / k);
        }
    }

    let mut out = Mesh { normal_angle: m.normal_angle.max(60.0), ..Default::default() };
    out.positions.extend(vpts);
    let e_off = out.positions.len() as u32;
    out.positions.extend(epts);
    let f_off = out.positions.len() as u32;
    out.positions.extend(fpts.iter().copied());

    for (fi, f) in m.faces.iter().enumerate() {
        let n = f.verts.len();
        let fp = f_off + fi as u32;
        let uv_base = out.uvs.len() as u32;
        let has_uv = f.has_uvs();
        if has_uv {
            let fuv: Vec<Vec2> = f.uvs.iter().map(|&u| m.uvs[u as usize]).collect();
            let center = fuv.iter().copied().sum::<Vec2>() / n as f32;
            // layout: [corner uvs (n)] [edge mid uvs (n)] [center]
            out.uvs.extend(fuv.iter().copied());
            for i in 0..n {
                out.uvs.push((fuv[i] + fuv[(i + 1) % n]) * 0.5);
            }
            out.uvs.push(center);
        }
        for i in 0..n {
            let prev = (i + n - 1) % n;
            let v = f.verts[i];
            let e_next = e_off + eidx[&Edge::new(v, f.verts[(i + 1) % n])];
            let e_prev = e_off + eidx[&Edge::new(f.verts[prev], v)];
            let verts = vec![v, e_next, fp, e_prev];
            let uvs = if has_uv {
                vec![uv_base + i as u32, uv_base + (n + i) as u32, uv_base + 2 * n as u32, uv_base + (n + prev) as u32]
            } else {
                Vec::new()
            };
            out.faces.push(Face { verts, uvs });
        }
    }
    out
}

/// Laplacian smoothing of vertex positions (Average Vertices). Border
/// vertices are pinned.
pub fn average_vertices(m: &mut Mesh, verts: Option<&[u32]>, iterations: u32, strength: f32) {
    let edges = m.edges();
    let ef = m.edge_faces();
    let mut nbrs: Vec<Vec<u32>> = vec![Vec::new(); m.positions.len()];
    let mut border = vec![false; m.positions.len()];
    for e in &edges {
        nbrs[e.0 as usize].push(e.1);
        nbrs[e.1 as usize].push(e.0);
        if ef[e].len() == 1 {
            border[e.0 as usize] = true;
            border[e.1 as usize] = true;
        }
    }
    let targets: Vec<u32> = match verts {
        Some(v) => v.to_vec(),
        None => (0..m.positions.len() as u32).collect(),
    };
    for _ in 0..iterations {
        let old = m.positions.clone();
        for &v in &targets {
            let vi = v as usize;
            if border[vi] || nbrs[vi].is_empty() {
                continue;
            }
            let avg: Vec3 = nbrs[vi].iter().map(|&n| old[n as usize]).sum::<Vec3>() / nbrs[vi].len() as f32;
            m.positions[vi] = old[vi].lerp(avg, strength);
        }
    }
}

// --------------------------------------------------------------- bevel ---

/// Chamfer every edge of a closed mesh (polyBevel on an object, 1 segment).
/// Each face shrinks by `offset`, edges become quads, vertices become n-gons.
pub fn bevel_all(m: &mut Mesh, offset: f32) -> Result<(), String> {
    if !m.boundary_edges().is_empty() {
        return Err("bevel needs a closed mesh (fill holes first)".into());
    }
    let mut out = Mesh { normal_angle: m.normal_angle, ..Default::default() };
    // corner vertex for (face, corner index)
    let mut corner: Vec<Vec<u32>> = Vec::with_capacity(m.faces.len());
    for f in &m.faces {
        let n = f.verts.len();
        let mut ids = Vec::with_capacity(n);
        for i in 0..n {
            let v = m.positions[f.verts[i] as usize];
            let prev = m.positions[f.verts[(i + n - 1) % n] as usize];
            let next = m.positions[f.verts[(i + 1) % n] as usize];
            let e1 = (prev - v).normalize_or_zero();
            let e2 = (next - v).normalize_or_zero();
            // limit so corners never cross the edge midpoint
            let lim = (prev - v).length().min((next - v).length()) * 0.49;
            let d = offset.min(lim);
            out.positions.push(v + (e1 + e2) * d);
            ids.push((out.positions.len() - 1) as u32);
        }
        out.faces.push(Face::new(ids.clone()));
        corner.push(ids);
    }
    // locate (face, corner) by directed edge a->b
    let mut dir: HashMap<(u32, u32), (usize, usize)> = HashMap::new();
    for (fi, f) in m.faces.iter().enumerate() {
        let n = f.verts.len();
        for i in 0..n {
            dir.insert((f.verts[i], f.verts[(i + 1) % n]), (fi, i));
        }
    }
    let cid = |fi: usize, ci: usize| corner[fi][ci];
    let mut links: HashMap<u32, Vec<(u32, u32)>> = HashMap::new();
    let mut done: HashSet<Edge> = HashSet::new();
    for (fi, f) in m.faces.iter().enumerate() {
        let n = f.verts.len();
        for i in 0..n {
            let (a, b) = (f.verts[i], f.verts[(i + 1) % n]);
            if !done.insert(Edge::new(a, b)) {
                continue;
            }
            let Some(&(f2, j)) = dir.get(&(b, a)) else { return Err("non-manifold edge".into()) };
            let n2 = m.faces[f2].verts.len();
            // corners: F1(a)=i, F1(b)=i+1 ; F2(b)=j, F2(a)=j+1
            let f1a = cid(fi, i);
            let f1b = cid(fi, (i + 1) % n);
            let f2b = cid(f2, j);
            let f2a = cid(f2, (j + 1) % n2);
            out.faces.push(Face::new(vec![f1b, f1a, f2a, f2b]));
            // vertex-polygon links (opposite direction to the strip edges)
            links.entry(a).or_default().push((f2a, f1a));
            links.entry(b).or_default().push((f1b, f2b));
        }
    }
    for (_, ls) in links {
        if ls.len() < 3 {
            continue;
        }
        let next: HashMap<u32, u32> = ls.iter().copied().collect();
        let start = ls[0].0;
        let mut poly = vec![start];
        let mut cur = start;
        while let Some(&nx) = next.get(&cur) {
            if nx == start || poly.len() > ls.len() {
                break;
            }
            poly.push(nx);
            cur = nx;
        }
        if poly.len() >= 3 {
            out.faces.push(Face::new(poly));
        }
    }
    *m = out;
    Ok(())
}

// ------------------------------------------------------------ edge loop ---

/// Walk the quad ring starting at `start` and return the ring faces with
/// their (entry edge origin, entry edge other) pairs.
pub fn edge_ring(m: &Mesh, start: Edge) -> Vec<Edge> {
    let mut ring = vec![start];
    let ef = m.edge_faces();
    let mut seen_faces: HashSet<u32> = HashSet::new();
    for dir in 0..2 {
        let mut e = start;
        let Some(fs) = ef.get(&start) else { return ring };
        let Some(&first) = fs.get(dir) else { continue };
        let mut face = first;
        loop {
            if !seen_faces.insert(face) {
                break;
            }
            let f = &m.faces[face as usize];
            if f.len() != 4 {
                break;
            }
            let i = (0..4).find(|&i| Edge::new(f.verts[i], f.verts[(i + 1) % 4]) == e).unwrap();
            let opp = Edge::new(f.verts[(i + 2) % 4], f.verts[(i + 3) % 4]);
            if ring.contains(&opp) {
                break;
            }
            if dir == 0 {
                ring.push(opp);
            } else {
                ring.insert(0, opp);
            }
            e = opp;
            match ef[&opp].iter().find(|&&f2| f2 != face) {
                Some(&nf) => face = nf,
                None => break,
            }
        }
    }
    ring
}

/// Edge loop through the given edge (follows valence-4 vertices).
pub fn edge_loop(m: &Mesh, start: Edge) -> Vec<Edge> {
    let ef = m.edge_faces();
    let edges = m.edges();
    let mut v_edges: HashMap<u32, Vec<Edge>> = HashMap::new();
    for e in &edges {
        v_edges.entry(e.0).or_default().push(*e);
        v_edges.entry(e.1).or_default().push(*e);
    }
    let mut out = vec![start];
    for (from, mut v) in [(start, start.1), (start, start.0)] {
        let mut prev = from;
        loop {
            let es = &v_edges[&v];
            if es.len() != 4 {
                break;
            }
            // the continuing edge shares no face with prev
            let pf: HashSet<u32> = ef[&prev].iter().copied().collect();
            let next = es.iter().find(|e| **e != prev && !ef[e].iter().any(|f| pf.contains(f)));
            let Some(&next) = next else { break };
            if out.contains(&next) {
                break;
            }
            out.push(next);
            v = next.other(v);
            prev = next;
        }
    }
    out
}

/// Insert an edge loop across the quad ring through `start`, splitting each
/// ring edge at fraction `t` measured from the ring's origin side.
pub fn insert_edge_loop(m: &mut Mesh, start: Edge, t: f32) -> usize {
    let ef = m.edge_faces();
    // (face, entry directed edge with origin first)
    let mut visits: Vec<(u32, u32, u32)> = Vec::new(); // face, origin, other on entry edge
    let mut seen: HashSet<u32> = HashSet::new();
    let Some(start_faces) = ef.get(&start).cloned() else { return 0 };
    for (k, &sf) in start_faces.iter().enumerate().take(2) {
        // the origin side must be consistent between both directions:
        // direction 0 uses start.0 as origin, direction 1 also start.0.
        let _ = k;
        let mut face = sf;
        let mut origin = start.0;
        let mut other = start.1;
        loop {
            if !seen.insert(face) {
                break;
            }
            let f = &m.faces[face as usize];
            if f.len() != 4 {
                seen.remove(&face);
                break;
            }
            visits.push((face, origin, other));
            let io = f.verts.iter().position(|&v| v == origin).unwrap();
            let ix = f.verts.iter().position(|&v| v == other).unwrap();
            // opposite edge: the vertex adjacent to origin (not `other`) and
            // the vertex adjacent to other (not origin)
            let o2 = if (io + 1) % 4 == ix { f.verts[(io + 3) % 4] } else { f.verts[(io + 1) % 4] };
            let x2 = if (ix + 1) % 4 == io { f.verts[(ix + 3) % 4] } else { f.verts[(ix + 1) % 4] };
            let opp = Edge::new(o2, x2);
            origin = o2;
            other = x2;
            match ef.get(&opp).and_then(|fs| fs.iter().find(|&&f2| f2 != face)) {
                Some(&nf) => face = nf,
                None => break,
            }
        }
    }
    if visits.is_empty() {
        return 0;
    }
    // split points keyed by undirected edge
    let mut split: HashMap<Edge, u32> = HashMap::new();
    let mut get_split = |m: &mut Mesh, o: u32, x: u32| -> u32 {
        *split.entry(Edge::new(o, x)).or_insert_with(|| {
            let p = m.positions[o as usize].lerp(m.positions[x as usize], t);
            m.positions.push(p);
            (m.positions.len() - 1) as u32
        })
    };
    let mut new_faces: Vec<(u32, Face, Face)> = Vec::new();
    for &(face, origin, other) in &visits {
        let f = m.faces[face as usize].clone();
        let io = f.verts.iter().position(|&v| v == origin).unwrap();
        let ix = f.verts.iter().position(|&v| v == other).unwrap();
        let o2 = if (io + 1) % 4 == ix { f.verts[(io + 3) % 4] } else { f.verts[(io + 1) % 4] };
        let x2 = if (ix + 1) % 4 == io { f.verts[(ix + 3) % 4] } else { f.verts[(ix + 1) % 4] };
        let m1 = get_split(m, origin, other);
        let m2 = get_split(m, o2, x2);
        // rotate so that the entry edge is verts[0]->verts[1] in winding order
        let (q0, q1, q2, q3, mm1, mm2) = if (io + 1) % 4 == ix {
            (origin, other, x2, o2, m1, m2)
        } else {
            (other, origin, o2, x2, m1, m2)
        };
        let uv_of = |v: u32| f.verts.iter().position(|&w| w == v).map(|i| f.uvs.get(i).copied());
        let _ = uv_of;
        new_faces.push((face, Face::new(vec![q0, mm1, mm2, q3]), Face::new(vec![mm1, q1, q2, mm2])));
    }
    // UV handling: interpolate per-face where available.
    let mut appended = Vec::new();
    for (face, mut a, mut b) in new_faces {
        let old = m.faces[face as usize].clone();
        if old.has_uvs() {
            let uv = |v: u32| -> Option<Vec2> {
                old.verts.iter().position(|&w| w == v).map(|i| m.uvs[old.uvs[i] as usize])
            };
            let assign = |f: &mut Face, uvs_store: &mut Vec<Vec2>| {
                let mut ids = Vec::new();
                for &v in &f.verts {
                    let p = match uv(v) {
                        Some(p) => p,
                        None => {
                            // midpoint vertex: find which edge it splits
                            let (e, _) = split.iter().find(|(_, &id)| id == v).unwrap();
                            let (o, x) = (e.0, e.1);
                            let (uo, ux) = (uv(o).unwrap_or(Vec2::ZERO), uv(x).unwrap_or(Vec2::ZERO));
                            let po = m.positions[o as usize];
                            let px = m.positions[x as usize];
                            let pm = m.positions[v as usize];
                            let s = if po.distance(px) > 0.0 { po.distance(pm) / po.distance(px) } else { 0.5 };
                            uo.lerp(ux, s)
                        }
                    };
                    ids.push((m.uvs.len() + uvs_store.len()) as u32);
                    uvs_store.push(p);
                }
                f.uvs = ids;
            };
            let mut store = Vec::new();
            assign(&mut a, &mut store);
            assign(&mut b, &mut store);
            m.uvs.extend(store);
        }
        m.faces[face as usize] = a;
        appended.push(b);
    }
    let ring_faces: HashSet<u32> = visits.iter().map(|v| v.0).collect();
    // Fix T-junctions on non-ring faces that share a split edge.
    for (fi, f) in m.faces.iter_mut().enumerate() {
        if ring_faces.contains(&(fi as u32)) {
            continue;
        }
        let n = f.verts.len();
        let mut nv = Vec::with_capacity(n + 2);
        let mut nu = Vec::new();
        let had_uv = f.has_uvs();
        for i in 0..n {
            nv.push(f.verts[i]);
            if had_uv {
                nu.push(f.uvs[i]);
            }
            if let Some(&s) = split.get(&Edge::new(f.verts[i], f.verts[(i + 1) % n])) {
                nv.push(s);
                if had_uv {
                    nu.push(f.uvs[i]);
                }
            }
        }
        f.verts = nv;
        f.uvs = nu;
    }
    m.faces.extend(appended);
    visits.len()
}

// ---------------------------------------------------------- mirror etc ---

/// Mirror geometry across `axis` (0=X,1=Y,2=Z) at `position`, merging seam
/// vertices within `tol`.
pub fn mirror(m: &mut Mesh, axis: usize, position: f32, tol: f32) {
    let mut copy = m.clone();
    for p in &mut copy.positions {
        p[axis] = 2.0 * position - p[axis];
    }
    reverse_faces(&mut copy, None);
    let off = m.append(&copy);
    let seam: Vec<u32> = (0..m.positions.len() as u32)
        .filter(|&v| (m.positions[v as usize][axis] - position).abs() <= tol)
        .collect();
    let _ = off;
    merge_vertices(m, Some(&seam), tol);
}

/// Split into connected components (Separate).
pub fn separate(m: &Mesh) -> Vec<Mesh> {
    let n = m.positions.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut [usize], x: usize) -> usize {
        let mut r = x;
        while p[r] != r {
            p[r] = p[p[r]];
            r = p[r];
        }
        r
    }
    for f in &m.faces {
        for w in f.verts.windows(2) {
            let (a, b) = (find(&mut parent, w[0] as usize), find(&mut parent, w[1] as usize));
            if a != b {
                parent[a] = b;
            }
        }
    }
    let mut groups: Vec<(usize, Vec<Face>)> = Vec::new();
    for f in &m.faces {
        let r = find(&mut parent, f.verts[0] as usize);
        match groups.iter_mut().find(|(k, _)| *k == r) {
            Some((_, v)) => v.push(f.clone()),
            None => groups.push((r, vec![f.clone()])),
        }
    }
    groups
        .into_iter()
        .map(|(_, faces)| {
            let mut out = Mesh { positions: m.positions.clone(), uvs: m.uvs.clone(), faces, normal_angle: m.normal_angle };
            out.compact();
            out
        })
        .collect()
}

/// Remove the given faces into a new mesh (Extract).
pub fn extract_faces(m: &mut Mesh, faces: &[u32]) -> Mesh {
    let set: HashSet<u32> = faces.iter().copied().collect();
    let mut ex = Mesh { positions: m.positions.clone(), uvs: m.uvs.clone(), faces: Vec::new(), normal_angle: m.normal_angle };
    for &f in faces {
        ex.faces.push(m.faces[f as usize].clone());
    }
    ex.compact();
    let keep: Vec<u32> = (0..m.faces.len() as u32).filter(|f| !set.contains(f)).collect();
    let _ = keep;
    delete_faces(m, faces);
    ex
}

// ------------------------------------------------------------------ UVs ---

/// Planar UV projection along an axis (0=X,1=Y,2=Z), normalized to 0..1.
pub fn planar_uvs(m: &mut Mesh, axis: usize) {
    let (ua, va) = match axis {
        0 => (2, 1),
        1 => (0, 2),
        _ => (0, 1),
    };
    let b = m.bounds();
    let s = b.size();
    let su = if s[ua] > 0.0 { s[ua] } else { 1.0 };
    let sv = if s[va] > 0.0 { s[va] } else { 1.0 };
    m.uvs = m.positions.iter().map(|p| Vec2::new((p[ua] - b.min[ua]) / su, (p[va] - b.min[va]) / sv)).collect();
    for f in &mut m.faces {
        f.uvs = f.verts.clone();
    }
}

/// Cylindrical UV projection around Y.
pub fn cylindrical_uvs(m: &mut Mesh) {
    let b = m.bounds();
    let c = b.center();
    let h = b.size().y.max(1e-6);
    m.uvs.clear();
    for f in &mut m.faces {
        let mut ids = Vec::new();
        let mut us: Vec<f32> = Vec::new();
        for &v in &f.verts {
            let p = m.positions[v as usize] - c;
            us.push(0.5 + p.z.atan2(p.x) / std::f32::consts::TAU);
        }
        // fix seam wrap within the face
        let max = us.iter().cloned().fold(f32::MIN, f32::max);
        for u in &mut us {
            if max - *u > 0.5 {
                *u += 1.0;
            }
        }
        for (i, &v) in f.verts.iter().enumerate() {
            let p = m.positions[v as usize];
            ids.push(m.uvs.len() as u32);
            m.uvs.push(Vec2::new(us[i], (p.y - b.min.y) / h));
        }
        f.uvs = ids;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::*;

    fn euler(m: &Mesh) -> i64 {
        m.positions.len() as i64 - m.edges().len() as i64 + m.faces.len() as i64
    }

    #[test]
    fn extrude_top_face() {
        let mut c = cube(1.0, 1.0, 1.0, 1, 1, 1);
        let top = (0..6).find(|&f| c.face_normal(f).y > 0.9).unwrap() as u32;
        extrude_faces(&mut c, &[top], 1.0, 0.0);
        assert_eq!(c.faces.len(), 10);
        assert_eq!(c.positions.len(), 12);
        assert!(c.validate().is_empty(), "{:?}", c.validate());
        assert!(c.boundary_edges().is_empty());
        assert!((c.bounds().max.y - 1.5).abs() < 1e-5);
        assert_eq!(euler(&c), 2);
    }

    #[test]
    fn extrude_region_and_inset() {
        let mut p = plane(2.0, 2.0, 2, 2);
        extrude_faces(&mut p, &[0, 1, 2, 3], 0.5, 0.0);
        assert!(p.validate().is_empty(), "{:?}", p.validate());
        assert_eq!(p.faces.len(), 4 + 8);
        let mut c = cube(1.0, 1.0, 1.0, 1, 1, 1);
        inset_faces(&mut c, &[0], 0.3);
        assert_eq!(c.faces.len(), 10);
        assert!(c.validate().is_empty());
    }

    #[test]
    fn smooth_cube() {
        let c = cube(2.0, 2.0, 2.0, 1, 1, 1);
        let s = smooth(&c, 1);
        assert_eq!(s.faces.len(), 24);
        assert_eq!(s.positions.len(), 26);
        assert!(s.validate().is_empty());
        assert_eq!(euler(&s), 2);
        let s2 = smooth(&c, 2);
        assert_eq!(s2.faces.len(), 96);
        // smoothed cube shrinks toward a sphere
        assert!(s2.bounds().max.x < 1.0);
    }

    #[test]
    fn edge_loop_on_cube() {
        let mut c = cube(2.0, 2.0, 2.0, 1, 1, 1);
        // pick a vertical edge
        let e = c
            .edges()
            .into_iter()
            .find(|e| {
                let d = c.positions[e.0 as usize] - c.positions[e.1 as usize];
                d.y.abs() > 1.0
            })
            .unwrap();
        let n = insert_edge_loop(&mut c, e, 0.5);
        assert_eq!(n, 4);
        assert_eq!(c.faces.len(), 10);
        assert!(c.validate().is_empty(), "{:?}", c.validate());
        assert!(c.boundary_edges().is_empty());
        assert_eq!(euler(&c), 2);
    }

    #[test]
    fn edge_loop_selection() {
        let s = sphere(1.0, 8, 8);
        // pick an edge on the equator ring (horizontal)
        let e = s
            .edges()
            .into_iter()
            .find(|e| {
                let a = s.positions[e.0 as usize];
                let b = s.positions[e.1 as usize];
                a.y.abs() < 1e-4 && b.y.abs() < 1e-4
            })
            .unwrap();
        assert_eq!(edge_loop(&s, e).len(), 8);
        assert_eq!(edge_ring(&s, e).len(), 8 - 2 + 1);
    }

    #[test]
    fn delete_fill_bridge() {
        let mut c = cylinder(1.0, 2.0, 8, 1, true);
        let caps: Vec<u32> = (0..c.faces.len() as u32).filter(|&f| c.faces[f as usize].len() == 8).collect();
        delete_faces(&mut c, &caps);
        assert_eq!(boundary_loops(&c).len(), 2);
        assert_eq!(fill_holes(&mut c, &[]), 2);
        assert!(c.boundary_edges().is_empty());
        assert!(c.validate().is_empty());

        // two separate rings -> bridge
        let mut t = cylinder(1.0, 1.0, 8, 1, false);
        let mut t2 = cylinder(1.0, 1.0, 8, 1, false);
        for p in &mut t2.positions {
            p.y += 3.0;
        }
        t.append(&t2);
        assert_eq!(boundary_loops(&t).len(), 4);
        let loops = boundary_loops(&t);
        // the top of the first and bottom of the second
        let top = loops.iter().find(|l| (t.positions[l[0] as usize].y - 0.5).abs() < 1e-4).unwrap().clone();
        let bot = loops.iter().find(|l| (t.positions[l[0] as usize].y - 2.5).abs() < 1e-4).unwrap().clone();
        bridge_loops(&mut t, &top, &bot).unwrap();
        assert!(t.validate().is_empty(), "{:?}", t.validate());
        assert_eq!(boundary_loops(&t).len(), 2);
    }

    #[test]
    fn mirror_and_separate() {
        let mut p = plane(1.0, 1.0, 1, 1);
        for v in &mut p.positions {
            v.x += 0.5;
        }
        mirror(&mut p, 0, 0.0, 1e-4);
        assert_eq!(p.positions.len(), 6);
        assert_eq!(p.faces.len(), 2);
        assert!(p.validate().is_empty(), "{:?}", p.validate());

        let mut two = cube(1.0, 1.0, 1.0, 1, 1, 1);
        let mut other = cube(1.0, 1.0, 1.0, 1, 1, 1);
        for v in &mut other.positions {
            v.x += 3.0;
        }
        two.append(&other);
        assert_eq!(separate(&two).len(), 2);
    }

    #[test]
    fn dissolve_and_triangulate() {
        let mut c = cube(1.0, 1.0, 1.0, 1, 1, 1);
        triangulate(&mut c);
        assert_eq!(c.faces.len(), 12);
        assert!(c.validate().is_empty());
        quadrangulate(&mut c, 1.0);
        assert_eq!(c.faces.len(), 6);
        assert!(c.validate().is_empty(), "{:?}", c.validate());
    }

    #[test]
    fn bevel_cube() {
        let mut c = cube(2.0, 2.0, 2.0, 1, 1, 1);
        bevel_all(&mut c, 0.2).unwrap();
        // 6 faces + 12 edge quads + 8 corner triangles
        assert_eq!(c.faces.len(), 26);
        assert_eq!(c.positions.len(), 24);
        assert!(c.validate().is_empty(), "{:?}", c.validate());
        assert!(c.boundary_edges().is_empty());
        assert_eq!(euler(&c), 2);
        for fi in 0..c.faces.len() {
            assert!(c.face_normal(fi).dot(c.face_center(fi)) > 0.0, "face {fi} inward");
        }
    }

    #[test]
    fn poke_and_merge_center() {
        let mut c = cube(1.0, 1.0, 1.0, 1, 1, 1);
        poke_faces(&mut c, &[0]);
        assert_eq!(c.faces.len(), 9);
        merge_to_center(&mut c, &[0, 1]);
        assert!(c.validate().is_empty(), "{:?}", c.validate());
    }
}
