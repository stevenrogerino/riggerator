//! Polygon primitives matching Maya's conventions: Y-up, centered at the
//! origin, counter-clockwise winding seen from outside, UVs included.

use crate::{ops, Face, Mesh};
use glam::{Mat3, Vec2, Vec3};
use std::f32::consts::{PI, TAU};

/// A subdivided grid in the XZ plane facing +Y (polyPlane).
pub fn plane(width: f32, height: f32, sx: u32, sz: u32) -> Mesh {
    let sx = sx.max(1);
    let sz = sz.max(1);
    let mut m = Mesh::default();
    for j in 0..=sz {
        for i in 0..=sx {
            let u = i as f32 / sx as f32;
            let v = j as f32 / sz as f32;
            m.positions.push(Vec3::new(-width * 0.5 + u * width, 0.0, height * 0.5 - v * height));
            m.uvs.push(Vec2::new(u, v));
        }
    }
    let row = sx + 1;
    for j in 0..sz {
        for i in 0..sx {
            let a = j * row + i;
            let b = a + 1;
            let c = a + row + 1;
            let d = a + row;
            // (x0,z_hi) (x1,z_hi) (x1,z_lo) (x0,z_lo) -> faces +Y
            let vs = vec![a, b, c, d];
            m.faces.push(Face::with_uvs(vs.clone(), vs));
        }
    }
    m.normal_angle = 30.0;
    m
}

/// polyCube with per-axis subdivisions.
pub fn cube(w: f32, h: f32, d: f32, sx: u32, sy: u32, sz: u32) -> Mesh {
    let (sx, sy, sz) = (sx.max(1), sy.max(1), sz.max(1));
    // (rotation applied to a +Y unit grid, grid subdivs, uv atlas cell)
    let sides: [(Mat3, u32, u32, (f32, f32)); 6] = [
        (Mat3::IDENTITY, sx, sz, (1.0, 1.0)),                        // +Y
        (Mat3::from_rotation_x(PI), sx, sz, (1.0, 3.0)),             // -Y
        (Mat3::from_rotation_z(-PI / 2.0), sy, sz, (2.0, 1.0)),      // +X
        (Mat3::from_rotation_z(PI / 2.0), sy, sz, (0.0, 1.0)),       // -X
        (Mat3::from_rotation_x(PI / 2.0), sx, sy, (1.0, 0.0)),       // +Z
        (Mat3::from_rotation_x(-PI / 2.0), sx, sy, (1.0, 2.0)),      // -Z
    ];
    let mut m = Mesh::default();
    for (rot, a, b, (cu, cv)) in sides {
        let mut g = plane(1.0, 1.0, a, b);
        for p in &mut g.positions {
            *p = rot * (*p + Vec3::new(0.0, 0.5, 0.0));
            *p *= Vec3::new(w, h, d);
        }
        for uv in &mut g.uvs {
            *uv = Vec2::new((cu + uv.x) / 3.0, (cv + uv.y) / 4.0);
        }
        m.append(&g);
    }
    let tol = 1e-5 * w.abs().max(h.abs()).max(d.abs()).max(1e-3);
    ops::merge_vertices(&mut m, None, tol);
    m.normal_angle = 30.0;
    m
}

/// polySphere: `sa` subdivisions around the Y axis, `sh` along height.
pub fn sphere(radius: f32, sa: u32, sh: u32) -> Mesh {
    let sa = sa.max(3);
    let sh = sh.max(2);
    let mut m = Mesh::default();
    // vertex 0 = south pole, then rings, then north pole
    m.positions.push(Vec3::new(0.0, -radius, 0.0));
    for j in 1..sh {
        let phi = -PI / 2.0 + PI * j as f32 / sh as f32;
        for i in 0..sa {
            let th = TAU * i as f32 / sa as f32;
            m.positions.push(Vec3::new(radius * phi.cos() * th.cos(), radius * phi.sin(), -radius * phi.cos() * th.sin()));
        }
    }
    let north = m.positions.len() as u32;
    m.positions.push(Vec3::new(0.0, radius, 0.0));
    // UV grid (sa+1) x (sh+1) with seam duplicates
    for j in 0..=sh {
        for i in 0..=sa {
            m.uvs.push(Vec2::new(i as f32 / sa as f32, j as f32 / sh as f32));
        }
    }
    let uv = |i: u32, j: u32| j * (sa + 1) + i;
    let ring = |j: u32, i: u32| 1 + (j - 1) * sa + (i % sa);
    for i in 0..sa {
        // bottom cap triangle
        m.faces.push(Face::with_uvs(vec![0, ring(1, i + 1), ring(1, i)], vec![uv(i, 0), uv(i + 1, 1), uv(i, 1)]));
    }
    for j in 1..sh - 1 {
        for i in 0..sa {
            m.faces.push(Face::with_uvs(
                vec![ring(j, i), ring(j, i + 1), ring(j + 1, i + 1), ring(j + 1, i)],
                vec![uv(i, j), uv(i + 1, j), uv(i + 1, j + 1), uv(i, j + 1)],
            ));
        }
    }
    for i in 0..sa {
        m.faces.push(Face::with_uvs(
            vec![ring(sh - 1, i), ring(sh - 1, i + 1), north],
            vec![uv(i, sh - 1), uv(i + 1, sh - 1), uv(i, sh)],
        ));
    }
    m.normal_angle = 180.0;
    m
}

/// polyCylinder along Y, centered, with n-gon caps.
pub fn cylinder(radius: f32, height: f32, sa: u32, sh: u32, caps: bool) -> Mesh {
    let sa = sa.max(3);
    let sh = sh.max(1);
    let mut m = Mesh::default();
    for j in 0..=sh {
        let y = -height * 0.5 + height * j as f32 / sh as f32;
        for i in 0..sa {
            let th = TAU * i as f32 / sa as f32;
            m.positions.push(Vec3::new(radius * th.cos(), y, -radius * th.sin()));
        }
    }
    for j in 0..=sh {
        for i in 0..=sa {
            m.uvs.push(Vec2::new(i as f32 / sa as f32 * 0.5 + 0.25, 0.25 + 0.5 * j as f32 / sh as f32));
        }
    }
    let v = |j: u32, i: u32| j * sa + (i % sa);
    let uvi = |j: u32, i: u32| j * (sa + 1) + i;
    for j in 0..sh {
        for i in 0..sa {
            m.faces.push(Face::with_uvs(
                vec![v(j, i), v(j, i + 1), v(j + 1, i + 1), v(j + 1, i)],
                vec![uvi(j, i), uvi(j, i + 1), uvi(j + 1, i + 1), uvi(j + 1, i)],
            ));
        }
    }
    if caps {
        let base = m.uvs.len() as u32;
        for i in 0..sa {
            let th = TAU * i as f32 / sa as f32;
            m.uvs.push(Vec2::new(0.5 + 0.12 * th.cos(), 0.12 + 0.11 * th.sin()));
        }
        for i in 0..sa {
            let th = TAU * i as f32 / sa as f32;
            m.uvs.push(Vec2::new(0.5 + 0.12 * th.cos(), 0.88 + 0.11 * th.sin()));
        }
        let bottom: Vec<u32> = (0..sa).rev().map(|i| v(0, i)).collect();
        let bottom_uv: Vec<u32> = (0..sa).rev().map(|i| base + i).collect();
        let top: Vec<u32> = (0..sa).map(|i| v(sh, i)).collect();
        let top_uv: Vec<u32> = (0..sa).map(|i| base + sa + i).collect();
        m.faces.push(Face::with_uvs(bottom, bottom_uv));
        m.faces.push(Face::with_uvs(top, top_uv));
    }
    m.normal_angle = 30.0;
    m
}

/// polyCone along Y with base at -h/2 and apex at +h/2.
pub fn cone(radius: f32, height: f32, sa: u32) -> Mesh {
    let sa = sa.max(3);
    let mut m = Mesh::default();
    for i in 0..sa {
        let th = TAU * i as f32 / sa as f32;
        m.positions.push(Vec3::new(radius * th.cos(), -height * 0.5, -radius * th.sin()));
    }
    let apex = sa;
    m.positions.push(Vec3::new(0.0, height * 0.5, 0.0));
    for i in 0..sa {
        m.faces.push(Face::new(vec![i, (i + 1) % sa, apex]));
    }
    m.faces.push(Face::new((0..sa).rev().collect()));
    m.normal_angle = 30.0;
    m
}

/// polyTorus lying in the XZ plane.
pub fn torus(radius: f32, section_radius: f32, sa: u32, sh: u32) -> Mesh {
    let sa = sa.max(3);
    let sh = sh.max(3);
    let mut m = Mesh::default();
    for i in 0..sa {
        let th = TAU * i as f32 / sa as f32;
        let dir = Vec3::new(th.cos(), 0.0, -th.sin());
        for j in 0..sh {
            let ph = TAU * j as f32 / sh as f32;
            m.positions.push(dir * (radius + section_radius * ph.cos()) + Vec3::Y * section_radius * ph.sin());
        }
    }
    for i in 0..=sa {
        for j in 0..=sh {
            m.uvs.push(Vec2::new(i as f32 / sa as f32, j as f32 / sh as f32));
        }
    }
    let v = |i: u32, j: u32| (i % sa) * sh + (j % sh);
    let uvi = |i: u32, j: u32| i * (sh + 1) + j;
    for i in 0..sa {
        for j in 0..sh {
            m.faces.push(Face::with_uvs(
                vec![v(i, j), v(i + 1, j), v(i + 1, j + 1), v(i, j + 1)],
                vec![uvi(i, j), uvi(i + 1, j), uvi(i + 1, j + 1), uvi(i, j + 1)],
            ));
        }
    }
    m.normal_angle = 180.0;
    m
}

/// polyPipe: a hollow cylinder.
pub fn pipe(radius: f32, height: f32, thickness: f32, sa: u32) -> Mesh {
    let sa = sa.max(3);
    let inner = (radius - thickness).max(1e-4);
    let mut m = Mesh::default();
    // rings: 0 outer-bottom, 1 outer-top, 2 inner-top, 3 inner-bottom
    for (r, y) in [(radius, -0.5), (radius, 0.5), (inner, 0.5), (inner, -0.5)] {
        for i in 0..sa {
            let th = TAU * i as f32 / sa as f32;
            m.positions.push(Vec3::new(r * th.cos(), y * height, -r * th.sin()));
        }
    }
    let v = |k: u32, i: u32| k * sa + (i % sa);
    for k in 0..4 {
        for i in 0..sa {
            let k2 = (k + 1) % 4;
            m.faces.push(Face::new(vec![v(k, i), v(k, i + 1), v(k2, i + 1), v(k2, i)]));
        }
    }
    m.normal_angle = 30.0;
    m
}

/// Platonic solid helper: a regular tetrahedron / octahedron / icosahedron.
pub fn platonic(kind: &str, radius: f32) -> Mesh {
    let (pts, tris): (Vec<Vec3>, Vec<[u32; 3]>) = match kind {
        "tetrahedron" => (
            vec![Vec3::new(1., 1., 1.), Vec3::new(-1., -1., 1.), Vec3::new(-1., 1., -1.), Vec3::new(1., -1., -1.)],
            vec![[0, 1, 3], [0, 2, 1], [0, 3, 2], [1, 2, 3]],
        ),
        "octahedron" => (
            vec![Vec3::X, Vec3::NEG_X, Vec3::Y, Vec3::NEG_Y, Vec3::Z, Vec3::NEG_Z],
            vec![[0, 2, 4], [4, 2, 1], [1, 2, 5], [5, 2, 0], [4, 3, 0], [1, 3, 4], [5, 3, 1], [0, 3, 5]],
        ),
        _ => {
            let t = (1.0 + 5f32.sqrt()) / 2.0;
            (
                vec![
                    Vec3::new(-1., t, 0.), Vec3::new(1., t, 0.), Vec3::new(-1., -t, 0.), Vec3::new(1., -t, 0.),
                    Vec3::new(0., -1., t), Vec3::new(0., 1., t), Vec3::new(0., -1., -t), Vec3::new(0., 1., -t),
                    Vec3::new(t, 0., -1.), Vec3::new(t, 0., 1.), Vec3::new(-t, 0., -1.), Vec3::new(-t, 0., 1.),
                ],
                vec![
                    [0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11], [1, 5, 9], [5, 11, 4], [11, 10, 2],
                    [10, 7, 6], [7, 1, 8], [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9], [4, 9, 5],
                    [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
                ],
            )
        }
    };
    let mut m = Mesh::new(
        pts.into_iter().map(|p| p.normalize() * radius).collect(),
        tris.into_iter().map(|t| Face::new(t.to_vec())).collect(),
    );
    // make sure every face points outward
    for fi in 0..m.faces.len() {
        if m.face_normal(fi).dot(m.face_center(fi)) < 0.0 {
            m.faces[fi].verts.reverse();
        }
    }
    m.normal_angle = 30.0;
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn closed(m: &Mesh) {
        assert!(m.validate().is_empty(), "{:?}", m.validate());
        assert!(m.boundary_edges().is_empty(), "boundary: {:?}", m.boundary_edges().len());
        // Euler characteristic of a sphere-like closed mesh
    }

    fn outward(m: &Mesh) {
        let c = m.bounds().center();
        let mut good = 0;
        for fi in 0..m.faces.len() {
            if m.face_normal(fi).dot(m.face_center(fi) - c) > 0.0 {
                good += 1;
            }
        }
        assert_eq!(good, m.faces.len());
    }

    #[test]
    fn plane_faces_up() {
        let p = plane(2.0, 2.0, 3, 2);
        assert_eq!(p.faces.len(), 6);
        for fi in 0..p.faces.len() {
            assert!(p.face_normal(fi).y > 0.99);
        }
    }

    #[test]
    fn subdivided_cube() {
        let c = cube(1.0, 2.0, 3.0, 2, 3, 4);
        closed(&c);
        outward(&c);
        let b = c.bounds();
        assert!((b.size() - Vec3::new(1.0, 2.0, 3.0)).length() < 1e-5);
        // V - E + F = 2
        let v = c.positions.len() as i64;
        let e = c.edges().len() as i64;
        let f = c.faces.len() as i64;
        assert_eq!(v - e + f, 2);
    }

    #[test]
    fn sphere_closed() {
        let s = sphere(1.0, 20, 20);
        closed(&s);
        outward(&s);
        assert_eq!(s.faces.len(), 20 * 20);
    }

    #[test]
    fn cylinder_cone_torus_closed() {
        let c = cylinder(1.0, 2.0, 20, 2, true);
        closed(&c);
        outward(&c);
        let k = cone(1.0, 2.0, 12);
        closed(&k);
        let t = torus(1.0, 0.25, 20, 10);
        closed(&t);
        let p = pipe(1.0, 1.0, 0.2, 16);
        closed(&p);
        for kind in ["tetrahedron", "octahedron", "icosahedron"] {
            let m = platonic(kind, 1.0);
            closed(&m);
            outward(&m);
        }
    }
}

#[cfg(test)]
mod torus_tests {
    use super::*;
    #[test]
    fn torus_outward() {
        let t = torus(2.0, 0.5, 16, 8);
        for fi in 0..t.faces.len() {
            let c = t.face_center(fi);
            let ring = Vec3::new(c.x, 0.0, c.z).normalize() * 2.0;
            assert!(t.face_normal(fi).dot(c - ring) > 0.0, "face {fi} inward");
        }
    }
}
