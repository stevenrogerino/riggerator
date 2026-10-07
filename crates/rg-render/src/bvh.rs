use glam::{Vec2, Vec3};

#[derive(Clone)]
pub struct Tri {
    pub p: [Vec3; 3],
    pub n: [Vec3; 3],
    pub uv: [Vec2; 3],
    pub mat: u32,
}

impl Tri {
    pub fn new(p: [Vec3; 3], n: [Vec3; 3], uv: [Vec2; 3], mat: u32) -> Self {
        Self { p, n, uv, mat }
    }
    pub fn geo_normal(&self) -> Vec3 {
        (self.p[1] - self.p[0]).cross(self.p[2] - self.p[0]).normalize_or_zero()
    }
    fn centroid(&self) -> Vec3 {
        (self.p[0] + self.p[1] + self.p[2]) / 3.0
    }
    pub fn interpolate(&self, o: Vec3, d: Vec3, h: Hit) -> (Vec3, Vec3, Vec2) {
        let w = 1.0 - h.u - h.v;
        let p = o + d * h.t;
        let mut n = (self.n[0] * w + self.n[1] * h.u + self.n[2] * h.v).normalize_or_zero();
        if n == Vec3::ZERO {
            n = self.geo_normal();
        }
        let uv = self.uv[0] * w + self.uv[1] * h.u + self.uv[2] * h.v;
        (p, n, uv)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub t: f32,
    pub u: f32,
    pub v: f32,
    pub tri: u32,
}

#[derive(Clone, Copy)]
struct Node {
    min: Vec3,
    max: Vec3,
    /// leaf: first tri index; inner: right child index
    start: u32,
    count: u32,
}

pub struct Bvh {
    pub tris: Vec<Tri>,
    nodes: Vec<Node>,
}

impl Bvh {
    pub fn build(mut tris: Vec<Tri>) -> Self {
        let mut nodes = Vec::with_capacity(tris.len() * 2 / 3 + 1);
        if tris.is_empty() {
            return Self { tris, nodes };
        }
        let mut cents: Vec<Vec3> = tris.iter().map(|t| t.centroid()).collect();
        let n = cents.len();
        build_rec(&mut tris, &mut cents, 0, n, &mut nodes);
        Self { tris, nodes }
    }

    pub fn intersect(&self, o: Vec3, d: Vec3, tmin: f32, tmax: f32) -> Option<Hit> {
        if self.nodes.is_empty() {
            return None;
        }
        let inv = Vec3::ONE / d;
        let mut best: Option<Hit> = None;
        let mut tmax = tmax;
        let mut stack = [0u32; 64];
        let mut sp = 1;
        while sp > 0 {
            sp -= 1;
            let ni = stack[sp] as usize;
            let n = self.nodes[ni];
            if !slab(n.min, n.max, o, inv, tmin, tmax) {
                continue;
            }
            if n.count > 0 {
                for i in n.start..n.start + n.count {
                    if let Some((t, u, v)) = tri_hit(&self.tris[i as usize], o, d) {
                        if t > tmin && t < tmax {
                            tmax = t;
                            best = Some(Hit { t, u, v, tri: i });
                        }
                    }
                }
            } else if sp + 2 <= stack.len() {
                stack[sp] = (ni + 1) as u32;
                stack[sp + 1] = n.start;
                sp += 2;
            }
        }
        best
    }

    pub fn occluded(&self, o: Vec3, d: Vec3, dist: f32) -> bool {
        if self.nodes.is_empty() {
            return false;
        }
        let inv = Vec3::ONE / d;
        let mut stack = [0u32; 64];
        let mut sp = 1;
        while sp > 0 {
            sp -= 1;
            let ni = stack[sp] as usize;
            let n = self.nodes[ni];
            if !slab(n.min, n.max, o, inv, 1e-4, dist) {
                continue;
            }
            if n.count > 0 {
                for i in n.start..n.start + n.count {
                    if let Some((t, _, _)) = tri_hit(&self.tris[i as usize], o, d) {
                        if t > 1e-4 && t < dist {
                            return true;
                        }
                    }
                }
            } else if sp + 2 <= stack.len() {
                stack[sp] = (ni + 1) as u32;
                stack[sp + 1] = n.start;
                sp += 2;
            }
        }
        false
    }
}

fn build_rec(tris: &mut [Tri], cents: &mut [Vec3], offset: usize, len: usize, nodes: &mut Vec<Node>) -> usize {
    let me = nodes.len();
    let (mut min, mut max) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    let (mut cmin, mut cmax) = (min, max);
    for (i, t) in tris.iter().enumerate() {
        for p in t.p {
            min = min.min(p);
            max = max.max(p);
        }
        cmin = cmin.min(cents[i]);
        cmax = cmax.max(cents[i]);
    }
    nodes.push(Node { min, max, start: offset as u32, count: len as u32 });
    if len <= 4 {
        return me;
    }
    let ext = cmax - cmin;
    let axis = if ext.x >= ext.y && ext.x >= ext.z { 0 } else if ext.y >= ext.z { 1 } else { 2 };
    if ext[axis] <= 1e-9 {
        return me;
    }
    // binned SAH
    const BINS: usize = 12;
    let mut bmin = [Vec3::splat(f32::INFINITY); BINS];
    let mut bmax = [Vec3::splat(f32::NEG_INFINITY); BINS];
    let mut bcount = [0usize; BINS];
    let bin_of = |c: Vec3| (((c[axis] - cmin[axis]) / ext[axis] * BINS as f32) as usize).min(BINS - 1);
    for (i, t) in tris.iter().enumerate() {
        let b = bin_of(cents[i]);
        bcount[b] += 1;
        for p in t.p {
            bmin[b] = bmin[b].min(p);
            bmax[b] = bmax[b].max(p);
        }
    }
    let area = |a: Vec3, b: Vec3| {
        let e = (b - a).max(Vec3::ZERO);
        e.x * e.y + e.y * e.z + e.z * e.x
    };
    let mut best = (f32::INFINITY, BINS / 2);
    for split in 1..BINS {
        let (mut lmin, mut lmax, mut lc) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY), 0);
        for b in 0..split {
            lmin = lmin.min(bmin[b]);
            lmax = lmax.max(bmax[b]);
            lc += bcount[b];
        }
        let (mut rmin, mut rmax, mut rc) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY), 0);
        for b in split..BINS {
            rmin = rmin.min(bmin[b]);
            rmax = rmax.max(bmax[b]);
            rc += bcount[b];
        }
        if lc == 0 || rc == 0 {
            continue;
        }
        let cost = area(lmin, lmax) * lc as f32 + area(rmin, rmax) * rc as f32;
        if cost < best.0 {
            best = (cost, split);
        }
    }
    // partition
    let mut i = 0;
    let mut j = len;
    while i < j {
        if bin_of(cents[i]) < best.1 {
            i += 1;
        } else {
            j -= 1;
            tris.swap(i, j);
            cents.swap(i, j);
        }
    }
    let mid = if i == 0 || i == len { len / 2 } else { i };
    let (lt, rt) = tris.split_at_mut(mid);
    let (lc, rc) = cents.split_at_mut(mid);
    nodes[me].count = 0;
    build_rec(lt, lc, offset, mid, nodes);
    let right = build_rec(rt, rc, offset + mid, len - mid, nodes);
    nodes[me].start = right as u32;
    me
}

#[inline]
fn slab(min: Vec3, max: Vec3, o: Vec3, inv: Vec3, tmin: f32, tmax: f32) -> bool {
    let t1 = (min - o) * inv;
    let t2 = (max - o) * inv;
    let lo = t1.min(t2).max_element().max(tmin);
    let hi = t1.max(t2).min_element().min(tmax);
    lo <= hi
}

#[inline]
fn tri_hit(t: &Tri, o: Vec3, d: Vec3) -> Option<(f32, f32, f32)> {
    let e1 = t.p[1] - t.p[0];
    let e2 = t.p[2] - t.p[0];
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - t.p[0];
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    Some((e2.dot(q) * inv, u, v))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bvh_matches_brute_force() {
        let mut tris = Vec::new();
        for i in 0..200 {
            let f = i as f32;
            let c = Vec3::new((f * 1.3).sin() * 5.0, (f * 0.7).cos() * 5.0, (f * 0.37).sin() * 5.0);
            tris.push(Tri::new([c, c + Vec3::X, c + Vec3::Y], [Vec3::Z; 3], [Vec2::ZERO; 3], 0));
        }
        let brute = tris.clone();
        let bvh = Bvh::build(tris);
        for k in 0..100 {
            let kf = k as f32;
            let o = Vec3::new(kf.sin() * 3.0, kf.cos() * 3.0, 20.0);
            let d = Vec3::new(0.01 * kf.cos(), 0.01 * kf.sin(), -1.0).normalize();
            let a = bvh.intersect(o, d, 1e-4, f32::INFINITY).map(|h| h.t);
            let b = brute.iter().filter_map(|t| tri_hit(t, o, d).map(|x| x.0)).filter(|t| *t > 1e-4).fold(None, |m: Option<f32>, t| Some(m.map_or(t, |m| m.min(t))));
            assert_eq!(a.is_some(), b.is_some());
            if let (Some(a), Some(b)) = (a, b) {
                assert!((a - b).abs() < 1e-4);
            }
        }
    }
}
