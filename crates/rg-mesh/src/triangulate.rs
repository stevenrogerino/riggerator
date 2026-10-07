use glam::{Vec2, Vec3};

/// Ear-clipping triangulation of a (possibly concave) planar polygon.
/// Returns local corner indices. Falls back to a fan if clipping stalls.
pub fn triangulate_polygon(pts: &[Vec3], normal: Vec3) -> Vec<[u32; 3]> {
    let n = pts.len();
    if n < 3 {
        return Vec::new();
    }
    if n == 3 {
        return vec![[0, 1, 2]];
    }
    // Project onto the dominant plane.
    let nrm = if normal.length_squared() > 0.0 { normal } else { Vec3::Z };
    let (u, v) = basis(nrm);
    let p2: Vec<Vec2> = pts.iter().map(|p| Vec2::new(p.dot(u), p.dot(v))).collect();

    let mut idx: Vec<u32> = (0..n as u32).collect();
    let mut out = Vec::with_capacity(n - 2);
    let mut guard = 0;
    while idx.len() > 3 && guard < n * n {
        guard += 1;
        let m = idx.len();
        let mut clipped = false;
        for i in 0..m {
            let a = idx[(i + m - 1) % m];
            let b = idx[i];
            let c = idx[(i + 1) % m];
            let (pa, pb, pc) = (p2[a as usize], p2[b as usize], p2[c as usize]);
            if cross2(pb - pa, pc - pb) <= 1e-12 {
                continue; // reflex or degenerate
            }
            let inside = idx.iter().any(|&o| {
                o != a && o != b && o != c && point_in_tri(p2[o as usize], pa, pb, pc)
            });
            if inside {
                continue;
            }
            out.push([a, b, c]);
            idx.remove(i);
            clipped = true;
            break;
        }
        if !clipped {
            break;
        }
    }
    if idx.len() == 3 {
        out.push([idx[0], idx[1], idx[2]]);
    } else if idx.len() > 3 {
        for i in 1..idx.len() - 1 {
            out.push([idx[0], idx[i], idx[i + 1]]);
        }
    }
    out
}

fn basis(n: Vec3) -> (Vec3, Vec3) {
    let n = n.normalize();
    let up = if n.z.abs() < 0.9 { Vec3::Z } else { Vec3::X };
    let u = up.cross(n).normalize();
    let v = n.cross(u);
    (u, v)
}

fn cross2(a: Vec2, b: Vec2) -> f32 {
    a.x * b.y - a.y * b.x
}

fn point_in_tri(p: Vec2, a: Vec2, b: Vec2, c: Vec2) -> bool {
    let d1 = cross2(b - a, p - a);
    let d2 = cross2(c - b, p - b);
    let d3 = cross2(a - c, p - c);
    d1 > 0.0 && d2 > 0.0 && d3 > 0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concave_l_shape() {
        let pts = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(2.0, 1.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(1.0, 2.0, 0.0),
            Vec3::new(0.0, 2.0, 0.0),
        ];
        let tris = triangulate_polygon(&pts, Vec3::Z);
        assert_eq!(tris.len(), 4);
        let area: f32 = tris
            .iter()
            .map(|t| {
                let (a, b, c) = (pts[t[0] as usize], pts[t[1] as usize], pts[t[2] as usize]);
                (b - a).cross(c - a).z * 0.5
            })
            .sum();
        assert!((area - 3.0).abs() < 1e-5);
    }
}
