//! rg-render — "Riggerator Software Renderer": a multithreaded CPU path
//! tracer with a BVH, physically based materials, Maya light types, sky
//! lighting, textures and progressive output. It fills the role of Maya's
//! Software / Arnold renderer for look-dev on a personal scale.

mod bvh;
mod sample;

use bvh::{Bvh, Tri};
use glam::{Mat4, Vec2, Vec3};
use rg_core::{LightType, Material, NodeId, NodeKind, Scene, ShaderType, Texture};
use sample::Rng;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct RenderCamera {
    pub world: Mat4,
    /// Vertical field of view in radians (perspective).
    pub vfov: f32,
    pub orthographic: bool,
    pub ortho_width: f32,
}

impl RenderCamera {
    /// Camera from a scene camera node (looks down -Z).
    pub fn from_node(scene: &Scene, ev: &rg_core::Evaluated, id: NodeId, aspect: f32) -> Option<Self> {
        let NodeKind::Camera(c) = &scene.nodes.get(&id)?.kind else { return None };
        let w = ev.world(id);
        let (_, r, t) = w.to_scale_rotation_translation();
        Some(Self { world: Mat4::from_rotation_translation(r, t), vfov: c.vfov(aspect), orthographic: c.orthographic, ortho_width: c.ortho_width })
    }
    pub fn look_at(eye: Vec3, target: Vec3, vfov_deg: f32) -> Self {
        let view = rg_core::look_at(eye, target, Vec3::Y);
        Self { world: view.inverse(), vfov: vfov_deg.to_radians(), orthographic: false, ortho_width: 10.0 }
    }
    fn ray(&self, u: f32, v: f32, aspect: f32) -> (Vec3, Vec3) {
        // u,v in [-1,1]
        if self.orthographic {
            let hw = self.ortho_width * 0.5;
            let hh = hw / aspect;
            let o = self.world.transform_point3(Vec3::new(u * hw, v * hh, 0.0));
            let d = self.world.transform_vector3(Vec3::NEG_Z).normalize();
            (o, d)
        } else {
            let th = (self.vfov * 0.5).tan();
            let d = Vec3::new(u * th * aspect, v * th, -1.0);
            (self.world.transform_point3(Vec3::ZERO), self.world.transform_vector3(d).normalize())
        }
    }
}

#[derive(Clone, Debug)]
pub struct RenderOptions {
    pub width: u32,
    pub height: u32,
    pub samples: u32,
    pub max_bounces: u32,
    pub threads: usize,
}

impl RenderOptions {
    pub fn from_scene(scene: &Scene) -> Self {
        Self {
            width: scene.render.width.max(1),
            height: scene.render.height.max(1),
            samples: scene.render.samples.max(1),
            max_bounces: scene.render.max_bounces,
            threads: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4),
        }
    }
}

#[derive(Clone)]
struct Light {
    kind: LightType,
    color: Vec3,
    pos: Vec3,
    dir: Vec3,
    /// Area light edges (full extents).
    u: Vec3,
    v: Vec3,
    cos_outer: f32,
    cos_inner: f32,
    decay: u32,
    shadows: bool,
}

struct Tex {
    w: u32,
    h: u32,
    px: Vec<Vec3>,
}

pub struct PreparedScene {
    bvh: Bvh,
    materials: Vec<Material>,
    textures: HashMap<String, Arc<Tex>>,
    lights: Vec<Light>,
    ambient: Vec3,
    sky: bool,
    sky_intensity: f32,
    background: Vec3,
}

/// Image result in linear float RGBA.
#[derive(Clone)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[f32; 4]>,
}

impl Image {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height, pixels: vec![[0.0; 4]; (width * height) as usize] }
    }
    /// Tone-mapped sRGB 8-bit RGBA.
    pub fn to_rgba8(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.pixels.len() * 4);
        for p in &self.pixels {
            for c in &p[..3] {
                out.push(to_srgb8(*c));
            }
            out.push((p[3].clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
        }
        out
    }
    pub fn save_png(&self, path: &std::path::Path) -> Result<(), String> {
        image::save_buffer(path, &self.to_rgba8(), self.width, self.height, image::ExtendedColorType::Rgba8).map_err(|e| e.to_string())
    }
}

fn to_srgb8(c: f32) -> u8 {
    // soft shoulder to avoid harsh clipping
    let c = c.max(0.0);
    let c = if c > 0.8 { 0.8 + (1.0 - (-(c - 0.8) * 5.0).exp()) * 0.2 } else { c };
    let s = if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
    (s.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// Gather triangles, materials and lights from the evaluated scene.
pub fn prepare(scene: &Scene) -> PreparedScene {
    let ev = scene.evaluate();
    let mut tris: Vec<Tri> = Vec::new();
    let mut materials: Vec<Material> = Vec::new();
    let mut mat_idx: HashMap<String, u32> = HashMap::new();
    let mut textures: HashMap<String, Arc<Tex>> = HashMap::new();
    let mut lights = Vec::new();
    let mut ambient = Vec3::ZERO;
    for (&id, n) in &scene.nodes {
        if !scene.is_visible(id) {
            continue;
        }
        let w = ev.world(id);
        match &n.kind {
            NodeKind::Mesh(ms) => {
                let Some(mesh) = ev.mesh(id) else { continue };
                let mesh = if ms.smooth_level > 0 { Arc::new(rg_core::mesh::ops::smooth(mesh, ms.smooth_level.min(3))) } else { mesh.clone() };
                let mat = scene.material_for(id).clone();
                let mi = *mat_idx.entry(mat.name.clone()).or_insert_with(|| {
                    if let Texture::File { path } = &mat.texture {
                        if !textures.contains_key(path) {
                            if let Ok(img) = image::open(path) {
                                let rgb = img.to_rgb32f();
                                let px = rgb.pixels().map(|p| Vec3::new(p[0], p[1], p[2]).powf(2.2)).collect();
                                textures.insert(path.clone(), Arc::new(Tex { w: rgb.width(), h: rgb.height(), px }));
                            }
                        }
                    }
                    materials.push(mat.clone());
                    (materials.len() - 1) as u32
                });
                let rm = mesh.to_render();
                let nm = w.inverse().transpose();
                let flip = w.determinant() < 0.0;
                for t in rm.indices.chunks(3) {
                    let (a, b, c) = if flip { (t[0], t[2], t[1]) } else { (t[0], t[1], t[2]) };
                    let p = |i: u32| w.transform_point3(rm.positions[i as usize]);
                    let nn = |i: u32| nm.transform_vector3(rm.normals[i as usize]).normalize_or_zero();
                    let uv = |i: u32| rm.uvs[i as usize];
                    tris.push(Tri::new([p(a), p(b), p(c)], [nn(a), nn(b), nn(c)], [uv(a), uv(b), uv(c)], mi));
                }
            }
            NodeKind::Light(l) => {
                let pos = w.transform_point3(Vec3::ZERO);
                let dir = w.transform_vector3(Vec3::NEG_Z).normalize_or_zero();
                let color = l.color * l.intensity;
                if l.kind == LightType::Ambient {
                    ambient += color;
                    continue;
                }
                let outer = (l.cone_angle * 0.5).to_radians();
                let inner = (l.cone_angle * 0.5 - l.penumbra.abs()).max(0.0).to_radians();
                let (outer, inner) = if l.penumbra < 0.0 { (outer, inner) } else { ((l.cone_angle * 0.5 + l.penumbra).to_radians(), (l.cone_angle * 0.5).to_radians()) };
                lights.push(Light {
                    kind: l.kind,
                    color,
                    pos,
                    dir,
                    u: w.transform_vector3(Vec3::X * 2.0),
                    v: w.transform_vector3(Vec3::Y * 2.0),
                    cos_outer: outer.min(3.1).cos(),
                    cos_inner: inner.min(3.1).cos(),
                    decay: l.decay,
                    shadows: l.cast_shadows,
                });
            }
            _ => {}
        }
    }
    if lights.is_empty() && scene.render.default_light {
        // Maya-like default: a key light from the upper front-left plus fill
        lights.push(Light {
            kind: LightType::Directional,
            color: Vec3::splat(0.9),
            pos: Vec3::ZERO,
            dir: Vec3::new(0.45, -0.75, -0.5).normalize(),
            u: Vec3::X,
            v: Vec3::Y,
            cos_outer: 0.0,
            cos_inner: 0.0,
            decay: 0,
            shadows: true,
        });
    }
    PreparedScene {
        bvh: Bvh::build(tris),
        materials,
        textures,
        lights,
        ambient,
        sky: scene.render.sky,
        sky_intensity: scene.render.sky_intensity,
        background: scene.render.background,
    }
}

/// The camera configured in render settings, else the first scene camera.
pub fn scene_camera(scene: &Scene, aspect: f32) -> Option<RenderCamera> {
    let ev = scene.evaluate();
    let id = scene
        .render
        .camera
        .filter(|c| scene.nodes.contains_key(c))
        .or_else(|| scene.walk().into_iter().find(|id| matches!(scene.nodes[id].kind, NodeKind::Camera(_))))?;
    RenderCamera::from_node(scene, &ev, id, aspect)
}

/// Shared progressive render state for UIs.
pub struct Progress {
    pub rows_done: AtomicUsize,
    pub cancel: AtomicBool,
    pub image: Mutex<Image>,
}

impl Progress {
    pub fn new(w: u32, h: u32) -> Arc<Self> {
        Arc::new(Self { rows_done: AtomicUsize::new(0), cancel: AtomicBool::new(false), image: Mutex::new(Image::new(w, h)) })
    }
}

/// Render synchronously; `progress` (if given) receives rows as they finish.
pub fn render(scene: &Scene, cam: &RenderCamera, opts: &RenderOptions, progress: Option<&Progress>) -> Image {
    let prep = prepare(scene);
    render_prepared(&prep, cam, opts, progress)
}

pub fn render_prepared(prep: &PreparedScene, cam: &RenderCamera, opts: &RenderOptions, progress: Option<&Progress>) -> Image {
    let (w, h) = (opts.width.max(1), opts.height.max(1));
    let aspect = w as f32 / h as f32;
    let next_row = AtomicUsize::new(0);
    let rows: Mutex<Vec<(u32, Vec<[f32; 4]>)>> = Mutex::new(Vec::new());
    let threads = opts.threads.max(1);
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| loop {
                if progress.is_some_and(|p| p.cancel.load(Ordering::Relaxed)) {
                    break;
                }
                let y = next_row.fetch_add(1, Ordering::Relaxed) as u32;
                if y >= h {
                    break;
                }
                let mut row = Vec::with_capacity(w as usize);
                let mut rng = Rng::new(y as u64 * 7919 + 17);
                for x in 0..w {
                    let mut acc = Vec3::ZERO;
                    let mut alpha = 0.0;
                    for s in 0..opts.samples {
                        let (jx, jy) = if opts.samples == 1 { (0.5, 0.5) } else { sample::r2(s, rng.f()) };
                        let u = ((x as f32 + jx) / w as f32) * 2.0 - 1.0;
                        let v = 1.0 - ((y as f32 + jy) / h as f32) * 2.0;
                        let (o, d) = cam.ray(u, v, aspect);
                        let (c, a) = trace(prep, o, d, opts.max_bounces, &mut rng);
                        acc += c;
                        alpha += a;
                    }
                    let n = opts.samples as f32;
                    let c = acc / n;
                    row.push([c.x, c.y, c.z, alpha / n]);
                }
                if let Some(p) = progress {
                    let mut img = p.image.lock().unwrap();
                    let off = (y * w) as usize;
                    if img.pixels.len() >= off + w as usize {
                        img.pixels[off..off + w as usize].copy_from_slice(&row);
                    }
                    p.rows_done.fetch_add(1, Ordering::Relaxed);
                }
                rows.lock().unwrap().push((y, row));
            });
        }
    });
    let mut img = Image::new(w, h);
    for (y, row) in rows.into_inner().unwrap() {
        let off = (y * w) as usize;
        img.pixels[off..off + w as usize].copy_from_slice(&row);
    }
    img
}

struct Surface {
    base: Vec3,
    metal: f32,
    rough: f32,
    spec: f32,
    spec_color: Vec3,
    diffuse: f32,
    transmission: f32,
    ior: f32,
    emission: Vec3,
    opacity: f32,
    unlit: bool,
}

fn surface(prep: &PreparedScene, mi: u32, uv: Vec2, p: Vec3) -> Surface {
    let m = &prep.materials[mi as usize];
    let mut base = m.color;
    match &m.texture {
        Texture::None => {}
        Texture::Checker { color2, repeats } => {
            let r = repeats.max(1.0);
            let c = ((uv.x * r).floor() as i64 + (uv.y * r).floor() as i64) & 1;
            if c == 1 {
                base = *color2;
            }
        }
        Texture::Ramp { color2 } => base = m.color.lerp(*color2, uv.y.clamp(0.0, 1.0)),
        Texture::Noise { color2, scale } => {
            let n = sample::value_noise(p * scale.max(1e-3));
            base = m.color.lerp(*color2, n);
        }
        Texture::File { path } => {
            if let Some(t) = prep.textures.get(path) {
                let x = ((uv.x.rem_euclid(1.0)) * t.w as f32) as u32 % t.w.max(1);
                let y = (((1.0 - uv.y).rem_euclid(1.0)) * t.h as f32) as u32 % t.h.max(1);
                base = t.px[(y * t.w + x) as usize];
            }
        }
    }
    let (spec, rough) = match m.shader {
        ShaderType::Lambert => (0.0, 1.0),
        _ => (m.specular, m.roughness),
    };
    Surface {
        base,
        metal: m.metalness.clamp(0.0, 1.0),
        rough: rough.clamp(0.02, 1.0),
        spec: spec.clamp(0.0, 1.0),
        spec_color: m.specular_color,
        diffuse: m.diffuse,
        transmission: m.transmission.clamp(0.0, 1.0),
        ior: m.ior.max(1.0),
        emission: m.emission_color * m.emission,
        opacity: m.opacity.clamp(0.0, 1.0),
        unlit: m.shader == ShaderType::SurfaceShader,
    }
}

fn sky(prep: &PreparedScene, d: Vec3) -> Vec3 {
    if !prep.sky {
        return prep.background;
    }
    let t = d.y * 0.5 + 0.5;
    let horizon = Vec3::new(0.85, 0.88, 0.92);
    let zenith = Vec3::new(0.35, 0.55, 0.9);
    let ground = Vec3::new(0.25, 0.23, 0.21);
    let c = if d.y >= 0.0 { horizon.lerp(zenith, d.y.powf(0.6)) } else { horizon.lerp(ground, (-d.y).powf(0.4)) };
    let _ = t;
    c * prep.sky_intensity
}

fn fresnel_schlick(f0: Vec3, cos: f32) -> Vec3 {
    f0 + (Vec3::ONE - f0) * (1.0 - cos.clamp(0.0, 1.0)).powi(5)
}

fn ggx_d(nh: f32, a: f32) -> f32 {
    let a2 = a * a;
    let d = nh * nh * (a2 - 1.0) + 1.0;
    a2 / (std::f32::consts::PI * d * d).max(1e-12)
}

fn smith_g1(nv: f32, a: f32) -> f32 {
    let a2 = a * a;
    2.0 * nv / (nv + (a2 + (1.0 - a2) * nv * nv).sqrt()).max(1e-12)
}

/// BRDF * cos for next-event estimation.
fn eval_brdf(s: &Surface, n: Vec3, wo: Vec3, wi: Vec3) -> Vec3 {
    let nl = n.dot(wi);
    let nv = n.dot(wo);
    if nl <= 0.0 || nv <= 0.0 {
        return Vec3::ZERO;
    }
    let diffuse = s.base * s.diffuse * (1.0 - s.metal) * (1.0 - s.transmission) / std::f32::consts::PI;
    let mut spec = Vec3::ZERO;
    if s.spec > 0.0 || s.metal > 0.0 {
        let h = (wo + wi).normalize();
        let a = s.rough * s.rough;
        let f0 = Vec3::splat(0.04 * s.spec * 2.0).lerp(s.base, s.metal) * s.spec_color.lerp(Vec3::ONE, s.metal);
        let f = fresnel_schlick(f0, h.dot(wo));
        let d = ggx_d(n.dot(h).max(0.0), a.max(1e-3));
        let g = smith_g1(nl, a) * smith_g1(nv, a);
        spec = f * d * g / (4.0 * nl * nv).max(1e-6);
    }
    (diffuse + spec) * nl
}

fn light_sample(prep: &PreparedScene, l: &Light, p: Vec3, rng: &mut Rng) -> Option<(Vec3, f32, Vec3)> {
    // returns (direction to light, distance, radiance*geometry)
    let pi = std::f32::consts::PI;
    match l.kind {
        LightType::Directional => Some((-l.dir, f32::INFINITY, l.color * pi)),
        LightType::Point | LightType::Spot => {
            let to = l.pos - p;
            let d = to.length().max(1e-4);
            let wi = to / d;
            let mut e = l.color * pi / d.powi(l.decay as i32).max(1e-6);
            if l.kind == LightType::Spot {
                let c = (-wi).dot(l.dir);
                if c < l.cos_outer {
                    return None;
                }
                if c < l.cos_inner {
                    let t = ((c - l.cos_outer) / (l.cos_inner - l.cos_outer).max(1e-5)).clamp(0.0, 1.0);
                    e *= t * t * (3.0 - 2.0 * t);
                }
            }
            Some((wi, d, e))
        }
        LightType::Area => {
            let q = l.pos + l.u * (rng.f() - 0.5) + l.v * (rng.f() - 0.5);
            let to = q - p;
            let d = to.length().max(1e-4);
            let wi = to / d;
            let cos_l = (-wi).dot(l.dir);
            if cos_l <= 0.0 {
                return None;
            }
            let area = l.u.cross(l.v).length();
            // physically based: radiance * cos_l * area / d²
            let e = l.color * cos_l * area / (d * d).max(1e-4) * pi;
            let _ = prep;
            Some((wi, d, e))
        }
        LightType::Ambient => None,
    }
}

fn trace(prep: &PreparedScene, mut o: Vec3, mut d: Vec3, max_bounces: u32, rng: &mut Rng) -> (Vec3, f32) {
    let mut radiance = Vec3::ZERO;
    let mut throughput = Vec3::ONE;
    let mut alpha = 0.0;
    let mut bounce = 0;
    let mut passes = 0;
    loop {
        let Some(hit) = prep.bvh.intersect(o, d, 1e-4, f32::INFINITY) else {
            let bg = sky(prep, d);
            if bounce == 0 {
                alpha = if prep.sky { 1.0 } else { alpha };
            }
            radiance += throughput * bg;
            break;
        };
        if bounce == 0 {
            alpha = 1.0;
        }
        let tri = &prep.bvh.tris[hit.tri as usize];
        let (p, mut n, uv) = tri.interpolate(o, d, hit);
        let geo_n = tri.geo_normal();
        let s = surface(prep, tri.mat, uv, p);
        if s.unlit {
            radiance += throughput * s.base;
            break;
        }
        // cutout / transparency
        if s.opacity < 1.0 && rng.f() > s.opacity {
            o = p + d * 1e-3;
            passes += 1;
            if passes > 16 {
                break;
            }
            continue;
        }
        radiance += throughput * s.emission;
        let entering = geo_n.dot(d) < 0.0;
        if n.dot(d) > 0.0 {
            n = -n;
        }
        let wo = -d;
        let offset_p = p + geo_n * if entering { 1e-4 } else { -1e-4 };
        let pn = if entering { p + geo_n * 1e-4 } else { p - geo_n * 1e-4 };
        // ambient
        radiance += throughput * prep.ambient * s.base * s.diffuse;
        // direct light
        if s.transmission < 0.999 {
            for l in &prep.lights {
                let Some((wi, dist, e)) = light_sample(prep, l, pn, rng) else { continue };
                let f = eval_brdf(&s, n, wo, wi);
                if f == Vec3::ZERO {
                    continue;
                }
                if l.shadows && prep.bvh.occluded(pn, wi, dist - 1e-3) {
                    continue;
                }
                let c = throughput * f * e;
                radiance += if bounce > 0 { c.min(Vec3::splat(4.0)) } else { c };
            }
        }
        let r = rng.f();
        let transmit = r < s.transmission;
        if transmit {
            passes += 1;
            if passes > 12 + max_bounces {
                break;
            }
        } else {
            if bounce >= max_bounces {
                break;
            }
            bounce += 1;
        }
        if transmit {
            // dielectric (smooth shading normal, oriented to the incoming side)
            let eta = if entering { 1.0 / s.ior } else { s.ior };
            let ns = tri.interpolate(o, d, hit).1;
            let ns = if ns.dot(geo_n) < 0.0 { -ns } else { ns };
            let nn = if entering { ns } else { -ns };
            let cos_i = (-d).dot(nn).clamp(0.0, 1.0);
            let f0 = ((1.0 - s.ior) / (1.0 + s.ior)).powi(2);
            let fr = f0 + (1.0 - f0) * (1.0 - cos_i).powi(5);
            let sin2 = eta * eta * (1.0 - cos_i * cos_i);
            if sin2 > 1.0 || rng.f() < fr {
                d = (d - nn * 2.0 * d.dot(nn)).normalize();
                o = p + if entering { geo_n } else { -geo_n } * 1e-4;
            } else {
                let cos_t = (1.0 - sin2).sqrt();
                d = (d * eta + nn * (eta * cos_i - cos_t)).normalize();
                o = p - if entering { geo_n } else { -geo_n } * 1e-4;
                throughput *= s.base.lerp(Vec3::ONE, 0.3);
            }
            continue;
        }
        let f0 = Vec3::splat(0.04 * s.spec * 2.0).lerp(s.base, s.metal);
        let fr = fresnel_schlick(f0, n.dot(wo)).max_element();
        let p_spec = if s.spec <= 0.0 && s.metal <= 0.0 { 0.0 } else { (fr + s.metal).clamp(0.1, 0.9) };
        if rng.f() < p_spec {
            let a = (s.rough * s.rough).max(1e-3);
            let h = sample::ggx_half(n, a, rng.f(), rng.f());
            let wi = (h * 2.0 * wo.dot(h) - wo).normalize();
            let nl = n.dot(wi);
            if nl <= 0.0 {
                break;
            }
            let nv = n.dot(wo).max(1e-4);
            let vh = wo.dot(h).max(1e-4);
            let nh = n.dot(h).max(1e-4);
            let f0c = Vec3::splat(0.04 * s.spec * 2.0).lerp(s.base, s.metal) * s.spec_color.lerp(Vec3::ONE, s.metal);
            let f = fresnel_schlick(f0c, vh);
            let g = smith_g1(nl, a) * smith_g1(nv, a);
            let wgt = f * g * vh / (nv * nh);
            throughput *= wgt / p_spec * if s.spec > 0.0 { 1.0 } else { 1.0 };
            d = wi;
        } else {
            let wi = sample::cosine_hemisphere(n, rng.f(), rng.f());
            throughput *= s.base * s.diffuse * (1.0 - s.metal) * (1.0 - s.transmission) / (1.0 - p_spec);
            d = wi;
        }
        o = offset_p;
        if bounce > 2 {
            let q = throughput.max_element().clamp(0.05, 0.95);
            if rng.f() > q {
                break;
            }
            throughput /= q;
        }
        if throughput.max_element() < 1e-4 {
            break;
        }
    }
    let r = radiance;
    (Vec3::new(r.x.min(50.0), r.y.min(50.0), r.z.min(50.0)), alpha)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rg_core::mesh::primitives;

    #[test]
    fn renders_lit_sphere() {
        let mut s = Scene::new();
        s.add_mesh("pSphere", primitives::sphere(1.0, 16, 12));
        s.render.sky = false;
        let cam = RenderCamera::look_at(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, 40.0);
        let opts = RenderOptions { width: 32, height: 32, samples: 1, max_bounces: 1, threads: 2 };
        let img = render(&s, &cam, &opts, None);
        let center = img.pixels[(16 * 32 + 16) as usize];
        let corner = img.pixels[0];
        assert!(center[3] > 0.5, "sphere should cover the center");
        assert!(center[0] > 0.02, "center lit: {center:?}");
        assert_eq!(corner[3], 0.0);
    }

    #[test]
    fn shadows_and_lights() {
        let mut s = Scene::new();
        let ground = s.add_mesh("ground", primitives::plane(10.0, 10.0, 1, 1));
        let ball = s.add_mesh("ball", primitives::sphere(1.0, 16, 12));
        s.set_attr(ball, "ty", 1.5).unwrap();
        let l = s.add_node("pointLight", NodeKind::Light(rg_core::LightShape::new(LightType::Point)), None);
        s.set_attr(l, "ty", 5.0).unwrap();
        s.render.sky = false;
        let prep = prepare(&s);
        assert_eq!(prep.lights.len(), 1);
        // point under the ball is shadowed, point away is lit
        let mut rng = Rng::new(1);
        let lit = light_sample(&prep, &prep.lights[0], Vec3::new(4.0, 0.0, 0.0), &mut rng).unwrap();
        assert!(!prep.bvh.occluded(Vec3::new(4.0, 0.001, 0.0), lit.0, lit.1));
        let sh = light_sample(&prep, &prep.lights[0], Vec3::new(0.0, 0.001, 0.0), &mut rng).unwrap();
        assert!(prep.bvh.occluded(Vec3::new(0.0, 0.001, 0.0), sh.0, sh.1));
        let _ = ground;
    }
}
