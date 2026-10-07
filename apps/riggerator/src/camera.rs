//! Viewport cameras (persp/top/front/side or a scene camera) and projection helpers.

use glam::{Mat4, Vec2, Vec3};
use rg_core::{Evaluated, NodeId, NodeKind, Scene};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CamKind {
    Persp,
    Top,
    Front,
    Side,
    Scene(NodeId),
}

impl CamKind {
    pub fn label(&self, scene: &Scene) -> String {
        match self {
            CamKind::Persp => "persp".into(),
            CamKind::Top => "top".into(),
            CamKind::Front => "front".into(),
            CamKind::Side => "side".into(),
            CamKind::Scene(id) => scene.name_of(*id),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ViewCamera {
    pub kind: CamKind,
    pub target: Vec3,
    pub distance: f32,
    /// Radians around Y.
    pub yaw: f32,
    /// Radians above the horizon.
    pub pitch: f32,
    /// Horizontal field of view (radians) for persp.
    pub hfov: f32,
    /// Visible width for orthographic views.
    pub ortho_width: f32,
}

impl ViewCamera {
    pub fn new(kind: CamKind) -> Self {
        let mut c = Self {
            kind,
            target: Vec3::ZERO,
            distance: 45.0,
            yaw: 45f32.to_radians(),
            pitch: 27f32.to_radians(),
            hfov: 54.43f32.to_radians(),
            ortho_width: 30.0,
        };
        match kind {
            CamKind::Top => {
                c.yaw = 0.0;
                c.pitch = 90f32.to_radians();
                c.distance = 1000.0;
            }
            CamKind::Front => {
                c.yaw = 0.0;
                c.pitch = 0.0;
                c.distance = 1000.0;
            }
            CamKind::Side => {
                c.yaw = 90f32.to_radians();
                c.pitch = 0.0;
                c.distance = 1000.0;
            }
            _ => {}
        }
        c
    }

    pub fn is_ortho(&self, scene: &Scene) -> bool {
        match self.kind {
            CamKind::Top | CamKind::Front | CamKind::Side => true,
            CamKind::Scene(id) => matches!(scene.nodes.get(&id).map(|n| &n.kind), Some(NodeKind::Camera(c)) if c.orthographic),
            CamKind::Persp => false,
        }
    }

    pub fn eye(&self) -> Vec3 {
        let dir = Vec3::new(self.pitch.cos() * self.yaw.sin(), self.pitch.sin(), self.pitch.cos() * self.yaw.cos());
        self.target + dir * self.distance
    }

    fn up(&self) -> Vec3 {
        if self.pitch.abs() > 89.9f32.to_radians() {
            // looking straight down: Maya's top view has -Z up the screen
            Vec3::new(-self.yaw.sin(), 0.0, -self.yaw.cos())
        } else {
            Vec3::Y
        }
    }

    /// World matrix of the camera (scene camera if looking through one).
    pub fn world(&self, scene: &Scene, ev: &Evaluated) -> Mat4 {
        if let CamKind::Scene(id) = self.kind {
            if scene.nodes.contains_key(&id) {
                let (_, r, t) = ev.world(id).to_scale_rotation_translation();
                return Mat4::from_rotation_translation(r, t);
            }
        }
        rg_core::look_at(self.eye(), self.target, self.up()).inverse()
    }

    pub fn view(&self, scene: &Scene, ev: &Evaluated) -> Mat4 {
        self.world(scene, ev).inverse()
    }

    pub fn proj(&self, scene: &Scene, aspect: f32) -> Mat4 {
        if let CamKind::Scene(id) = self.kind {
            if let Some(NodeKind::Camera(c)) = scene.nodes.get(&id).map(|n| &n.kind) {
                return c.projection(aspect);
            }
        }
        if self.is_ortho(scene) {
            let w = self.ortho_width * 0.5;
            let h = w / aspect.max(1e-6);
            rg_core::ortho_gl(-w, w, -h, h, 0.1, 5000.0)
        } else {
            let vfov = 2.0 * ((self.hfov * 0.5).tan() / aspect.max(1e-6)).atan();
            let near = (self.distance * 0.002).clamp(0.01, 1.0);
            rg_core::perspective_gl(vfov, aspect, near, 20000.0)
        }
    }

    pub fn tumble(&mut self, d: Vec2) {
        self.yaw -= d.x * 0.008;
        self.pitch = (self.pitch + d.y * 0.008).clamp(-89.5f32.to_radians(), 89.5f32.to_radians());
    }

    /// Pan by a screen delta (pixels) given the viewport size.
    pub fn track(&mut self, d: Vec2, viewport: Vec2, scene: &Scene) {
        let w = viewport.x.max(1.0);
        let world_per_px = if self.is_ortho(scene) {
            self.ortho_width / w
        } else {
            2.0 * self.distance * (self.hfov * 0.5).tan() / w
        };
        let fwd = (self.target - self.eye()).normalize();
        let right = fwd.cross(self.up()).normalize();
        let up = right.cross(fwd);
        self.target += (-right * d.x + up * d.y) * world_per_px;
    }

    pub fn dolly(&mut self, amount: f32, scene: &Scene) {
        let f = (1.0 - amount).clamp(0.2, 5.0);
        if self.is_ortho(scene) {
            self.ortho_width = (self.ortho_width * f).clamp(0.01, 100000.0);
        } else {
            self.distance = (self.distance * f).clamp(0.01, 100000.0);
        }
    }

    pub fn frame(&mut self, center: Vec3, radius: f32, scene: &Scene) {
        self.target = center;
        let r = radius.max(0.05);
        if self.is_ortho(scene) {
            self.ortho_width = r * 2.6;
        } else {
            self.distance = r / (self.hfov * 0.5).tan() * 1.3;
        }
    }
}

/// Screen-space projector for one viewport.
#[derive(Clone, Copy)]
pub struct Projector {
    pub view_proj: Mat4,
    pub inv_view_proj: Mat4,
    pub rect: egui::Rect,
    pub eye: Vec3,
    pub ortho: bool,
    pub cam_forward: Vec3,
}

impl Projector {
    pub fn new(view: Mat4, proj: Mat4, rect: egui::Rect, ortho: bool) -> Self {
        let vp = proj * view;
        let inv_view = view.inverse();
        Self {
            view_proj: vp,
            inv_view_proj: vp.inverse(),
            rect,
            eye: inv_view.transform_point3(Vec3::ZERO),
            ortho,
            cam_forward: inv_view.transform_vector3(Vec3::NEG_Z).normalize(),
        }
    }

    /// World -> screen point (None if behind the camera).
    pub fn project(&self, p: Vec3) -> Option<egui::Pos2> {
        let c = self.view_proj * p.extend(1.0);
        if c.w <= 1e-5 {
            return None;
        }
        let ndc = c.truncate() / c.w;
        Some(egui::pos2(
            self.rect.left() + (ndc.x * 0.5 + 0.5) * self.rect.width(),
            self.rect.top() + (0.5 - ndc.y * 0.5) * self.rect.height(),
        ))
    }

    pub fn ray(&self, pos: egui::Pos2) -> (Vec3, Vec3) {
        let x = (pos.x - self.rect.left()) / self.rect.width() * 2.0 - 1.0;
        let y = 1.0 - (pos.y - self.rect.top()) / self.rect.height() * 2.0;
        let a = self.inv_view_proj.project_point3(Vec3::new(x, y, -1.0));
        let b = self.inv_view_proj.project_point3(Vec3::new(x, y, 1.0));
        (a, (b - a).normalize())
    }

    /// World units per pixel at a point (for constant-size gizmos).
    pub fn pixel_size(&self, p: Vec3) -> f32 {
        let a = self.project(p);
        let right = self.inv_view_proj.transform_vector3(Vec3::X).normalize();
        let b = self.project(p + right);
        match (a, b) {
            (Some(a), Some(b)) => 1.0 / (a.distance(b)).max(1e-6),
            _ => 0.01,
        }
    }
}

/// Closest parameter `t` along line (o + d t) to a ray (ro + rd s).
pub fn closest_on_line(o: Vec3, d: Vec3, ro: Vec3, rd: Vec3) -> Option<f32> {
    let w = o - ro;
    let a = d.dot(d);
    let b = d.dot(rd);
    let c = rd.dot(rd);
    let dd = d.dot(w);
    let e = rd.dot(w);
    let den = a * c - b * b;
    if den.abs() < 1e-9 {
        return None;
    }
    Some((b * e - c * dd) / den)
}

pub fn ray_plane(ro: Vec3, rd: Vec3, p: Vec3, n: Vec3) -> Option<Vec3> {
    let den = rd.dot(n);
    if den.abs() < 1e-7 {
        return None;
    }
    let t = (p - ro).dot(n) / den;
    (t > 0.0).then(|| ro + rd * t)
}

pub fn seg_dist(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let ab = b - a;
    let l2 = ab.length_sq();
    if l2 < 1e-6 {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / l2).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}
