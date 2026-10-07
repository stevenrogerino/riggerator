use glam::Vec3;

/// PCG32 random generator.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        let mut r = Rng(seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407));
        r.u32();
        r
    }
    pub fn u32(&mut self) -> u32 {
        let old = self.0;
        self.0 = old.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }
    /// Uniform float in [0,1).
    pub fn f(&mut self) -> f32 {
        (self.u32() >> 8) as f32 / (1u32 << 24) as f32
    }
}

/// R2 low-discrepancy sequence with a random rotation.
pub fn r2(i: u32, shift: f32) -> (f32, f32) {
    const G: f32 = 1.324_718;
    let a1 = 1.0 / G;
    let a2 = 1.0 / (G * G);
    (((0.5 + a1 * i as f32) + shift).fract(), ((0.5 + a2 * i as f32) + shift * 0.618).fract())
}

pub fn onb(n: Vec3) -> (Vec3, Vec3) {
    let s = if n.z >= 0.0 { 1.0 } else { -1.0 };
    let a = -1.0 / (s + n.z);
    let b = n.x * n.y * a;
    (Vec3::new(1.0 + s * n.x * n.x * a, s * b, -s * n.x), Vec3::new(b, s + n.y * n.y * a, -n.y))
}

pub fn cosine_hemisphere(n: Vec3, u1: f32, u2: f32) -> Vec3 {
    let r = u1.sqrt();
    let phi = std::f32::consts::TAU * u2;
    let (t, b) = onb(n);
    (t * (r * phi.cos()) + b * (r * phi.sin()) + n * (1.0 - u1).max(0.0).sqrt()).normalize()
}

/// Sample a GGX half vector around n.
pub fn ggx_half(n: Vec3, a: f32, u1: f32, u2: f32) -> Vec3 {
    let phi = std::f32::consts::TAU * u1;
    let cos_t = ((1.0 - u2) / (1.0 + (a * a - 1.0) * u2)).max(0.0).sqrt();
    let sin_t = (1.0 - cos_t * cos_t).max(0.0).sqrt();
    let (t, b) = onb(n);
    (t * (sin_t * phi.cos()) + b * (sin_t * phi.sin()) + n * cos_t).normalize()
}

fn hash3(x: i32, y: i32, z: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(374761393) ^ (y as u32).wrapping_mul(668265263) ^ (z as u32).wrapping_mul(2147483647);
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    ((h ^ (h >> 16)) & 0xffff) as f32 / 65535.0
}

/// Smooth 3D value noise in [0,1] (3 octaves).
pub fn value_noise(p: Vec3) -> f32 {
    let mut total = 0.0;
    let mut amp = 0.5;
    let mut q = p;
    for _ in 0..3 {
        let i = q.floor();
        let f = q - i;
        let u = f * f * (Vec3::splat(3.0) - f * 2.0);
        let (x, y, z) = (i.x as i32, i.y as i32, i.z as i32);
        let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
        let c = |dx, dy, dz| hash3(x + dx, y + dy, z + dz);
        let v = lerp(
            lerp(lerp(c(0, 0, 0), c(1, 0, 0), u.x), lerp(c(0, 1, 0), c(1, 1, 0), u.x), u.y),
            lerp(lerp(c(0, 0, 1), c(1, 0, 1), u.x), lerp(c(0, 1, 1), c(1, 1, 1), u.x), u.y),
            u.z,
        );
        total += v * amp;
        amp *= 0.5;
        q *= 2.03;
    }
    total / 0.875
}
