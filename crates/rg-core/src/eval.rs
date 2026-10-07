//! Scene evaluation: driven keys → hierarchy → constraints → IK → deformers.
//!
//! Animation curves are applied by [`Scene::set_time`] (as Maya writes curve
//! output into the attribute), so evaluation works from the stored values and
//! produces an [`Evaluated`] overlay without mutating the scene.

use crate::node::*;
use crate::scene::{node_get_attr, node_set_attr, ConstraintKind, Scene};
use glam::{Mat4, Quat, Vec3};
use rg_mesh::Mesh;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
pub struct Evaluated {
    /// Nodes whose values were changed by evaluation (driven keys, constraints, IK).
    pub overrides: HashMap<NodeId, Node>,
    pub world: HashMap<NodeId, Mat4>,
    /// Deformed meshes in object space (only for deformed nodes).
    pub deformed: HashMap<NodeId, Arc<Mesh>>,
    base_meshes: HashMap<NodeId, Arc<Mesh>>,
}

impl Evaluated {
    pub fn world(&self, id: NodeId) -> Mat4 {
        self.world.get(&id).copied().unwrap_or(Mat4::IDENTITY)
    }
    pub fn mesh(&self, id: NodeId) -> Option<&Arc<Mesh>> {
        self.deformed.get(&id).or_else(|| self.base_meshes.get(&id))
    }
    pub fn is_deformed(&self, id: NodeId) -> bool {
        self.deformed.contains_key(&id)
    }
    /// Evaluated attribute value (falls back to the scene value).
    pub fn attr(&self, scene: &Scene, id: NodeId, attr: &str) -> Option<f64> {
        match self.overrides.get(&id) {
            Some(n) => node_get_attr(n, attr),
            None => scene.get_attr(id, attr),
        }
    }
    pub fn node<'a>(&'a self, scene: &'a Scene, id: NodeId) -> Option<&'a Node> {
        self.overrides.get(&id).or_else(|| scene.nodes.get(&id))
    }
    pub fn world_position(&self, scene: &Scene, id: NodeId) -> Vec3 {
        let p = self.node(scene, id).map(|n| n.transform.pivot).unwrap_or(Vec3::ZERO);
        self.world(id).transform_point3(p)
    }
}

struct Ctx<'a> {
    scene: &'a Scene,
    ev: Evaluated,
}

impl<'a> Ctx<'a> {
    fn node(&self, id: NodeId) -> &Node {
        self.ev.overrides.get(&id).unwrap_or_else(|| &self.scene.nodes[&id])
    }
    fn node_mut(&mut self, id: NodeId) -> &mut Node {
        let scene = self.scene;
        self.ev.overrides.entry(id).or_insert_with(|| scene.nodes[&id].clone())
    }
    fn parent_world(&self, id: NodeId) -> Mat4 {
        self.node(id).parent.map(|p| self.ev.world(p)).unwrap_or(Mat4::IDENTITY)
    }
    fn recompute(&mut self, id: NodeId) {
        let pw = self.parent_world(id);
        let w = pw * self.node(id).local_matrix();
        self.ev.world.insert(id, w);
        let kids = self.node(id).children.clone();
        for k in kids {
            self.recompute(k);
        }
    }
    fn set_world(&mut self, id: NodeId, world: Mat4) {
        let local = self.parent_world(id).inverse() * world;
        let n = self.node_mut(id);
        let jo = n.joint_orient();
        n.transform.set_from_matrix(local, jo);
        self.recompute(id);
    }
}

pub fn evaluate(scene: &Scene) -> Evaluated {
    let mut cx = Ctx { scene, ev: Evaluated::default() };

    // 1. driven keys
    for dk in &scene.driven_keys {
        if !scene.nodes.contains_key(&dk.driver) || !scene.nodes.contains_key(&dk.driven) {
            continue;
        }
        let dv = node_get_attr(cx.node(dk.driver), &dk.driver_attr).unwrap_or(0.0);
        let out = dk.curve.evaluate(dv);
        let _ = node_set_attr(cx.node_mut(dk.driven), &dk.driven_attr, out, true);
    }

    // 2. hierarchy
    for id in scene.walk() {
        let pw = cx.parent_world(id);
        let w = pw * cx.node(id).local_matrix();
        cx.ev.world.insert(id, w);
    }

    // 3. constraints
    for c in &scene.constraints {
        if c.enabled <= 0.0 || c.kind == ConstraintKind::PoleVector || !scene.nodes.contains_key(&c.driven) {
            continue;
        }
        let tws: Vec<Mat4> = c.targets.iter().map(|t| cx.ev.world(*t)).collect();
        let total: f32 = c.weights.iter().sum::<f32>().max(1e-6);
        let dw = cx.ev.world(c.driven);
        let (ds, dr, dt) = dw.to_scale_rotation_translation();
        let mut s = ds;
        let mut r = dr;
        let mut t = dt;
        match c.kind {
            ConstraintKind::Point => {
                t = tws.iter().zip(&c.offsets).zip(&c.weights).map(|((tw, off), w)| (tw.w_axis.truncate() + off.w_axis.truncate()) * *w).sum::<Vec3>() / total;
            }
            ConstraintKind::Orient => {
                let qs: Vec<Quat> = tws.iter().zip(&c.offsets).map(|(tw, off)| tw.to_scale_rotation_translation().1 * off.to_scale_rotation_translation().1).collect();
                r = blend_quats(&qs, &c.weights);
            }
            ConstraintKind::Scale => {
                s = tws.iter().zip(&c.offsets).zip(&c.weights).map(|((tw, off), w)| tw.to_scale_rotation_translation().0 * off.to_scale_rotation_translation().0 * *w).sum::<Vec3>() / total;
            }
            ConstraintKind::Parent => {
                let ms: Vec<Mat4> = tws.iter().zip(&c.offsets).map(|(tw, off)| *tw * *off).collect();
                let parts: Vec<(Vec3, Quat, Vec3)> = ms.iter().map(|m| m.to_scale_rotation_translation()).collect();
                t = parts.iter().zip(&c.weights).map(|(p, w)| p.2 * *w).sum::<Vec3>() / total;
                let qs: Vec<Quat> = parts.iter().map(|p| p.1).collect();
                r = blend_quats(&qs, &c.weights);
            }
            ConstraintKind::Aim => {
                let target = tws.iter().zip(&c.weights).map(|(tw, w)| tw.w_axis.truncate() * *w).sum::<Vec3>() / total;
                let aim = aim_rotation(dt, target, c.aim_axis, c.up_axis, c.world_up);
                r = aim * c.offsets[0].to_scale_rotation_translation().1;
            }
            ConstraintKind::PoleVector => {}
        }
        let blend = c.enabled.clamp(0.0, 1.0);
        let t = dt.lerp(t, blend);
        let r = dr.slerp(r, blend);
        let s = ds.lerp(s, blend);
        cx.set_world(c.driven, Mat4::from_scale_rotation_translation(s, r, t));
    }

    // 4. IK handles
    for id in scene.walk() {
        let NodeKind::IkHandle(ik) = &cx.node(id).kind else { continue };
        let ik = ik.clone();
        solve_ik(&mut cx, id, &ik);
    }

    // 5. deformation
    for (id, n) in &scene.nodes {
        let NodeKind::Mesh(base_ms) = &cx.node(*id).kind else { continue };
        let ms = base_ms.clone();
        let _ = n;
        cx.ev.base_meshes.insert(*id, ms.mesh.clone());
        let blend_active = ms.blend_shapes.iter().any(|b| b.weight.abs() > 1e-6);
        let def_active = ms.deformers.iter().any(|d| d.envelope.abs() > 1e-6);
        let skin_active = ms.skin.as_ref().is_some_and(|s| s.envelope > 0.0);
        if !(blend_active || def_active || skin_active) {
            continue;
        }
        let mut mesh = (*ms.mesh).clone();
        if blend_active {
            for b in &ms.blend_shapes {
                if b.weight.abs() < 1e-6 {
                    continue;
                }
                for (p, d) in mesh.positions.iter_mut().zip(&b.deltas) {
                    *p += *d * b.weight;
                }
            }
        }
        for d in &ms.deformers {
            apply_deformer(&mut mesh, d);
        }
        if let Some(skin) = &ms.skin {
            if skin.envelope > 0.0 {
                let mesh_world_inv = cx.ev.world(*id).inverse();
                let mats: Vec<Mat4> = skin
                    .joints
                    .iter()
                    .zip(&skin.inverse_bind)
                    .map(|(j, ib)| mesh_world_inv * cx.ev.world(*j) * *ib * skin.geom_bind)
                    .collect();
                for (vi, p) in mesh.positions.iter_mut().enumerate() {
                    let Some(ws) = skin.weights.get(vi) else { continue };
                    if ws.is_empty() {
                        continue;
                    }
                    let mut acc = Vec3::ZERO;
                    let mut wsum = 0.0;
                    for &(j, w) in ws {
                        if let Some(m) = mats.get(j as usize) {
                            acc += m.transform_point3(*p) * w;
                            wsum += w;
                        }
                    }
                    if wsum > 0.0 {
                        let skinned = acc / wsum;
                        *p = p.lerp(skinned, skin.envelope.min(1.0));
                    }
                }
            }
        }
        cx.ev.deformed.insert(*id, Arc::new(mesh));
    }
    cx.ev
}

fn blend_quats(qs: &[Quat], ws: &[f32]) -> Quat {
    let mut acc = glam::Vec4::ZERO;
    let first = qs.first().copied().unwrap_or(Quat::IDENTITY);
    for (q, w) in qs.iter().zip(ws) {
        let q = if q.dot(first) < 0.0 { -*q } else { *q };
        acc += glam::Vec4::from(q) * *w;
    }
    let q = Quat::from_vec4(acc);
    if q.length_squared() < 1e-12 {
        first
    } else {
        q.normalize()
    }
}

/// Rotation that points `aim_axis` from `from` toward `target`, keeping
/// `up_axis` as close as possible to `world_up`.
pub fn aim_rotation(from: Vec3, target: Vec3, aim_axis: Vec3, up_axis: Vec3, world_up: Vec3) -> Quat {
    let dir = (target - from).normalize_or_zero();
    if dir == Vec3::ZERO {
        return Quat::IDENTITY;
    }
    let q1 = Quat::from_rotation_arc(aim_axis.normalize(), dir);
    // twist around dir so up_axis aligns with world_up projected
    let cur_up = q1 * up_axis.normalize();
    let want = (world_up - dir * world_up.dot(dir)).normalize_or_zero();
    if want == Vec3::ZERO {
        return q1;
    }
    let cu = (cur_up - dir * cur_up.dot(dir)).normalize_or_zero();
    let angle = cu.cross(want).dot(dir).atan2(cu.dot(want));
    Quat::from_axis_angle(dir, angle) * q1
}

fn solve_ik(cx: &mut Ctx, handle: NodeId, ik: &IkHandleShape) {
    let scene = cx.scene;
    let Some(chain) = scene.joint_chain(ik.start_joint, ik.end_joint) else { return };
    if chain.len() < 2 || ik.ik_blend <= 0.0 {
        return;
    }
    let pos: Vec<Vec3> = chain.iter().map(|&j| cx.ev.world(j).w_axis.truncate()).collect();
    let lens: Vec<f32> = pos.windows(2).map(|w| w[0].distance(w[1])).collect();
    let total: f32 = lens.iter().sum();
    let root = pos[0];
    let goal = cx.ev.world(handle).w_axis.truncate();
    let pole_point = match ik.pole_target {
        Some(p) if scene.nodes.contains_key(&p) => cx.ev.world(p).w_axis.truncate(),
        _ => root + ik.pole_vector,
    };

    let mut new_pos = pos.clone();
    let to_goal = goal - root;
    let dist = to_goal.length().max(1e-6);
    let dir = to_goal / dist;

    if chain.len() == 3 {
        // analytic two-bone solve in the pole plane
        let (a, b) = (lens[0], lens[1]);
        let d = dist.min(a + b - 1e-4).max((a - b).abs() + 1e-4);
        let x = (a * a - b * b + d * d) / (2.0 * d);
        let h = (a * a - x * x).max(0.0).sqrt();
        let mut bend = pole_point - root;
        bend -= dir * bend.dot(dir);
        if ik.solver == IkSolver::SingleChain || bend.length_squared() < 1e-10 {
            // SC: keep the current bend direction
            bend = pos[1] - root;
            bend -= dir * bend.dot(dir);
        }
        let bend = bend.normalize_or(dir.any_orthonormal_vector());
        new_pos[1] = root + dir * x + bend * h;
        new_pos[2] = root + dir * d;
    } else {
        // FABRIK
        if dist >= total {
            for i in 1..pos.len() {
                new_pos[i] = new_pos[i - 1] + dir * lens[i - 1];
            }
        } else {
            for _ in 0..24 {
                let n = new_pos.len();
                new_pos[n - 1] = goal;
                for i in (0..n - 1).rev() {
                    let d = (new_pos[i] - new_pos[i + 1]).normalize_or_zero();
                    new_pos[i] = new_pos[i + 1] + d * lens[i];
                }
                new_pos[0] = root;
                for i in 1..n {
                    let d = (new_pos[i] - new_pos[i - 1]).normalize_or_zero();
                    new_pos[i] = new_pos[i - 1] + d * lens[i - 1];
                }
                if new_pos[n - 1].distance(goal) < 1e-4 {
                    break;
                }
            }
            if ik.solver == IkSolver::RotatePlane {
                // rotate solved chain about the root->goal axis toward the pole
                let mid = new_pos[new_pos.len() / 2] - root;
                let cur = (mid - dir * mid.dot(dir)).normalize_or_zero();
                let mut want = pole_point - root;
                want = (want - dir * want.dot(dir)).normalize_or_zero();
                if cur != Vec3::ZERO && want != Vec3::ZERO {
                    let ang = cur.cross(want).dot(dir).atan2(cur.dot(want));
                    let q = Quat::from_axis_angle(dir, ang);
                    for p in new_pos.iter_mut().skip(1) {
                        *p = root + q * (*p - root);
                    }
                }
            }
        }
    }
    if ik.twist.abs() > 1e-6 {
        let q = Quat::from_axis_angle(dir, ik.twist.to_radians());
        for p in new_pos.iter_mut().skip(1) {
            *p = root + q * (*p - root);
        }
    }
    let blend = ik.ik_blend.clamp(0.0, 1.0);
    for (np, p) in new_pos.iter_mut().zip(&pos) {
        *np = p.lerp(*np, blend);
    }

    // convert positions to joint rotations
    for i in 0..chain.len() - 1 {
        let j = chain[i];
        let w = cx.ev.world(j);
        let p = w.w_axis.truncate();
        let child_now = cx.ev.world(chain[i + 1]).w_axis.truncate();
        let cur = (child_now - p).normalize_or_zero();
        let want = (new_pos[i + 1] - p).normalize_or_zero();
        if cur == Vec3::ZERO || want == Vec3::ZERO {
            continue;
        }
        let q = Quat::from_rotation_arc(cur, want);
        let nw = Mat4::from_translation(p) * Mat4::from_quat(q) * Mat4::from_translation(-p) * w;
        cx.set_world(j, nw);
    }
}

fn apply_deformer(mesh: &mut Mesh, d: &Deformer) {
    if d.envelope.abs() < 1e-6 {
        return;
    }
    let p = |k: &str| d.params.get(k).copied().unwrap_or(0.0);
    let lo = d.low_bound;
    let hi = d.high_bound.max(lo + 1e-4);
    let c = (lo + hi) * 0.5;
    let half = (hi - lo) * 0.5;
    for v in mesh.positions.iter_mut() {
        let orig = *v;
        let u = ((v.y - lo) / (hi - lo)).clamp(0.0, 1.0);
        let mut out = *v;
        match d.kind {
            DeformerKind::Bend => {
                let k = p("curvature");
                if k.abs() > 1e-5 {
                    let r = half / k;
                    let yc = (v.y - c).clamp(-half, half);
                    let extra = v.y - c - yc;
                    let th = yc / r;
                    let (s, co) = th.sin_cos();
                    let x = r - (r - v.x) * co;
                    let y = c + (r - v.x) * s;
                    out.x = x + extra * -s;
                    out.y = y + extra * co;
                }
            }
            DeformerKind::Twist => {
                let a = (p("startAngle") + (p("endAngle") - p("startAngle")) * u).to_radians();
                let (s, co) = a.sin_cos();
                out.x = v.x * co + v.z * s;
                out.z = -v.x * s + v.z * co;
            }
            DeformerKind::Squash => {
                let f = 1.0 + p("factor");
                let f = f.max(0.05);
                out.y = c + (v.y - c) * f;
                let bulge = 1.0 / f.sqrt();
                let w = 1.0 - ((v.y - c) / half.max(1e-4)).powi(2).min(1.0);
                let s = 1.0 + (bulge - 1.0) * (0.5 + 0.5 * w);
                out.x *= s;
                out.z *= s;
            }
            DeformerKind::Flare => {
                let s = p("startFlare") + (p("endFlare") - p("startFlare")) * u + p("curve") * (std::f32::consts::PI * u).sin();
                out.x *= s;
                out.z *= s;
            }
            DeformerKind::Sine => {
                let wl = p("wavelength").max(1e-3);
                out.x += p("amplitude") * (std::f32::consts::TAU * (v.y - lo) / wl + p("offset")).sin();
            }
            DeformerKind::Wave => {
                let wl = p("wavelength").max(1e-3);
                let r = (v.x * v.x + v.z * v.z).sqrt();
                out.y += p("amplitude") * (std::f32::consts::TAU * r / wl + p("offset")).sin();
            }
        }
        *v = orig.lerp(out, d.envelope);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::*;
    use rg_mesh::primitives;

    fn approx(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-3
    }

    #[test]
    fn hierarchy_world() {
        let mut s = Scene::new();
        let g = s.add_node("group", NodeKind::Transform, None);
        let c = s.add_mesh("pCube", primitives::cube(1.0, 1.0, 1.0, 1, 1, 1));
        s.set_parent(c, Some(g), false).unwrap();
        s.set_attr(g, "tx", 5.0).unwrap();
        s.set_attr(c, "ty", 2.0).unwrap();
        let ev = s.evaluate();
        assert!(approx(ev.world(c).w_axis.truncate(), Vec3::new(5.0, 2.0, 0.0)));
    }

    #[test]
    fn parent_keep_world() {
        let mut s = Scene::new();
        let g = s.add_node("group", NodeKind::Transform, None);
        s.set_attr(g, "tx", 3.0).unwrap();
        s.set_attr(g, "ry", 90.0).unwrap();
        let c = s.add_node("locator", NodeKind::Locator, None);
        s.set_attr(c, "tz", 1.0).unwrap();
        let before = s.world_matrix(c);
        s.set_parent(c, Some(g), true).unwrap();
        assert!(s.world_matrix(c).abs_diff_eq(before, 1e-4));
    }

    #[test]
    fn animation_set_time() {
        let mut s = Scene::new();
        let c = s.add_mesh("pCube", primitives::cube(1.0, 1.0, 1.0, 1, 1, 1));
        s.set_key(c, "tx", 1.0, Some(0.0)).unwrap();
        s.set_key(c, "tx", 11.0, Some(10.0)).unwrap();
        s.set_time(6.0);
        let v = s.get_attr(c, "tx").unwrap();
        assert!(v > 3.0 && v < 7.0, "{v}");
        s.set_time(11.0);
        assert!((s.get_attr(c, "tx").unwrap() - 10.0).abs() < 1e-6);
    }

    #[test]
    fn point_and_parent_constraint() {
        let mut s = Scene::new();
        let a = s.add_node("locator", NodeKind::Locator, None);
        let b = s.add_node("locator", NodeKind::Locator, None);
        s.set_attr(a, "tx", 4.0).unwrap();
        s.add_constraint(ConstraintKind::Point, &[a], b, false).unwrap();
        let ev = s.evaluate();
        assert!(approx(ev.world(b).w_axis.truncate(), Vec3::new(4.0, 0.0, 0.0)));

        let c = s.add_node("locator", NodeKind::Locator, None);
        s.set_attr(c, "ty", 1.0).unwrap();
        s.add_constraint(ConstraintKind::Parent, &[a], c, true).unwrap();
        s.set_attr(a, "ry", 90.0).unwrap();
        let ev = s.evaluate();
        // offset (−4,1,0) rotated 90° about Y around a
        let p = ev.world(c).w_axis.truncate();
        assert!(approx(p, Vec3::new(4.0, 1.0, 4.0)), "{p:?}");
    }

    #[test]
    fn aim_constraint_points_x() {
        let mut s = Scene::new();
        let t = s.add_node("locator", NodeKind::Locator, None);
        let d = s.add_node("locator", NodeKind::Locator, None);
        s.set_attr(t, "tz", 5.0).unwrap();
        s.add_constraint(ConstraintKind::Aim, &[t], d, false).unwrap();
        let ev = s.evaluate();
        let x = ev.world(d).transform_vector3(Vec3::X).normalize();
        assert!(approx(x, Vec3::Z), "{x:?}");
    }

    #[test]
    fn two_bone_ik_reaches() {
        let mut s = Scene::new();
        let js = s.create_joint_chain(&[Vec3::ZERO, Vec3::new(0.0, 2.0, 0.1), Vec3::new(0.0, 4.0, 0.0)], None);
        let h = s.create_ik_handle(js[0], js[2], IkSolver::RotatePlane).unwrap();
        s.set_attr(h, "tx", 1.0).unwrap();
        s.set_attr(h, "ty", 3.0).unwrap();
        let ev = s.evaluate();
        let end = ev.world(js[2]).w_axis.truncate();
        assert!(approx(end, Vec3::new(1.0, 3.0, 0.0)), "{end:?}");
        // bone lengths preserved
        let p0 = ev.world(js[0]).w_axis.truncate();
        let p1 = ev.world(js[1]).w_axis.truncate();
        assert!((p0.distance(p1) - Vec3::new(0.0, 2.0, 0.1).length()).abs() < 1e-3);
    }

    #[test]
    fn long_chain_ik_reaches() {
        let mut s = Scene::new();
        let pts: Vec<Vec3> = (0..5).map(|i| Vec3::new(i as f32, 0.0, 0.0)).collect();
        let js = s.create_joint_chain(&pts, None);
        let h = s.create_ik_handle(js[0], js[4], IkSolver::SingleChain).unwrap();
        s.set_attr(h, "tx", 2.0).unwrap();
        s.set_attr(h, "ty", 2.0).unwrap();
        let ev = s.evaluate();
        let end = ev.world(js[4]).w_axis.truncate();
        assert!(approx(end, Vec3::new(2.0, 2.0, 0.0)), "{end:?}");
    }

    #[test]
    fn skinning_follows_joint() {
        let mut s = Scene::new();
        let m = s.add_mesh("pCylinder", primitives::cylinder(0.3, 4.0, 8, 4, true));
        s.set_attr(m, "ty", 2.0).unwrap();
        let js = s.create_joint_chain(&[Vec3::ZERO, Vec3::new(0.0, 2.0, 0.0), Vec3::new(0.0, 4.0, 0.0)], None);
        s.bind_skin(m, &js, 3, 4.0).unwrap();
        // bind pose: no change
        let ev = s.evaluate();
        let base = s.nodes[&m].mesh().unwrap().mesh.clone();
        let def = ev.mesh(m).unwrap();
        for (a, b) in base.positions.iter().zip(def.positions.iter()) {
            assert!(approx(*a, *b), "bind pose moved {a:?} -> {b:?}");
        }
        // move root joint: the whole mesh moves
        s.set_attr(js[0], "tx", 3.0).unwrap();
        let ev = s.evaluate();
        let def = ev.mesh(m).unwrap();
        for (a, b) in base.positions.iter().zip(def.positions.iter()) {
            assert!(approx(*a + Vec3::new(3.0, 0.0, 0.0), *b));
        }
    }

    #[test]
    fn blend_shape_and_driven_key() {
        let mut s = Scene::new();
        let base = s.add_mesh("base", primitives::cube(1.0, 1.0, 1.0, 1, 1, 1));
        let mut t = primitives::cube(1.0, 1.0, 1.0, 1, 1, 1);
        for p in &mut t.positions {
            p.y += 1.0;
        }
        let tgt = s.add_mesh("smile", t);
        let name = s.add_blend_target(base, tgt).unwrap();
        let drv = s.add_node("ctrl", NodeKind::Locator, None);
        s.driven_keys.push(DrivenKey {
            driver: drv,
            driver_attr: "translateY".into(),
            driven: base,
            driven_attr: format!("blendShape.{name}"),
            curve: rg_anim::AnimCurve::from_keys([(0.0, 0.0), (1.0, 1.0)]),
        });
        s.set_attr(drv, "ty", 1.0).unwrap();
        let ev = s.evaluate();
        let m = ev.mesh(base).unwrap();
        assert!((m.bounds().min.y - 0.5).abs() < 1e-4);
    }

    #[test]
    fn deformers_run() {
        let mut s = Scene::new();
        let m = s.add_mesh("pCylinder", primitives::cylinder(0.5, 4.0, 8, 8, true));
        let d = s.add_deformer(m, DeformerKind::Bend).unwrap();
        s.set_attr(m, &format!("{d}.curvature"), 1.0).unwrap();
        let ev = s.evaluate();
        assert!(ev.is_deformed(m));
        let b = ev.mesh(m).unwrap().bounds();
        assert!(b.size().x > 1.2);
        let t = s.add_deformer(m, DeformerKind::Twist).unwrap();
        assert_eq!(t, "twist1");
    }

    #[test]
    fn json_roundtrip() {
        let mut s = Scene::new();
        let c = s.add_mesh("pCube", primitives::cube(1.0, 1.0, 1.0, 1, 1, 1));
        s.set_key(c, "tx", 1.0, Some(2.0)).unwrap();
        let js = s.to_json().unwrap();
        let back = Scene::from_json(&js).unwrap();
        assert_eq!(back.nodes.len(), 1);
        assert_eq!(back.find("pCube1"), Some(c));
        let mut b2 = back.clone();
        let n = b2.add_node("x", NodeKind::Locator, None);
        assert!(n.0 > c.0);
    }

    #[test]
    fn naming() {
        let mut s = Scene::new();
        let a = s.add_mesh("pCube", primitives::cube(1.0, 1.0, 1.0, 1, 1, 1));
        let b = s.add_mesh("pCube", primitives::cube(1.0, 1.0, 1.0, 1, 1, 1));
        assert_eq!(s.name_of(a), "pCube1");
        assert_eq!(s.name_of(b), "pCube2");
        let d = s.duplicate(a).unwrap();
        assert_eq!(s.name_of(d), "pCube3");
        assert_eq!(s.rename(d, "hero"), "hero");
        assert_eq!(s.rename(b, "hero"), "hero1");
    }

    #[test]
    fn freeze_and_center_pivot() {
        let mut s = Scene::new();
        let c = s.add_mesh("pCube", primitives::cube(1.0, 1.0, 1.0, 1, 1, 1));
        s.set_attr(c, "tx", 2.0).unwrap();
        s.set_attr(c, "sy", 3.0).unwrap();
        s.freeze_transforms(c);
        assert_eq!(s.get_attr(c, "tx").unwrap(), 0.0);
        let b = s.world_mesh(c).unwrap().bounds();
        assert!((b.center() - Vec3::new(2.0, 0.0, 0.0)).length() < 1e-4);
        assert!((b.size().y - 3.0).abs() < 1e-4);
        s.center_pivot(c);
        let b2 = s.world_mesh(c).unwrap().bounds();
        assert!((b2.center() - b.center()).length() < 1e-4);
        assert!((s.nodes[&c].transform.pivot - Vec3::new(2.0, 0.0, 0.0)).length() < 1e-4);
    }
}
