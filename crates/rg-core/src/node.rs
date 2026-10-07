use glam::{EulerRot, Mat3, Mat4, Quat, Vec3};
use rg_anim::AnimCurve;
use rg_mesh::Mesh;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RotateOrder {
    #[default]
    XYZ,
    YZX,
    ZXY,
    XZY,
    YXZ,
    ZYX,
}

impl RotateOrder {
    pub const ALL: [RotateOrder; 6] =
        [RotateOrder::XYZ, RotateOrder::YZX, RotateOrder::ZXY, RotateOrder::XZY, RotateOrder::YXZ, RotateOrder::ZYX];
    pub fn name(self) -> &'static str {
        match self {
            RotateOrder::XYZ => "xyz",
            RotateOrder::YZX => "yzx",
            RotateOrder::ZXY => "zxy",
            RotateOrder::XZY => "xzy",
            RotateOrder::YXZ => "yxz",
            RotateOrder::ZYX => "zyx",
        }
    }
    /// Maya order "abc" applies a first: R = Rc * Rb * Ra, which is glam's
    /// intrinsic order "CBA".
    fn glam(self) -> EulerRot {
        match self {
            RotateOrder::XYZ => EulerRot::ZYX,
            RotateOrder::YZX => EulerRot::XZY,
            RotateOrder::ZXY => EulerRot::YXZ,
            RotateOrder::XZY => EulerRot::YZX,
            RotateOrder::YXZ => EulerRot::ZXY,
            RotateOrder::ZYX => EulerRot::XYZ,
        }
    }
    fn axes(self) -> [usize; 3] {
        match self {
            RotateOrder::XYZ => [0, 1, 2],
            RotateOrder::YZX => [1, 2, 0],
            RotateOrder::ZXY => [2, 0, 1],
            RotateOrder::XZY => [0, 2, 1],
            RotateOrder::YXZ => [1, 0, 2],
            RotateOrder::ZYX => [2, 1, 0],
        }
    }
    /// Quaternion from Euler angles in degrees.
    pub fn to_quat(self, deg: Vec3) -> Quat {
        let [a, b, c] = self.axes();
        let r = deg * (std::f32::consts::PI / 180.0);
        Quat::from_euler(self.glam(), r[c], r[b], r[a])
    }
    /// Euler degrees from a quaternion. `near` picks the solution closest to
    /// an existing rotation (avoids 180° flips while dragging).
    pub fn from_quat(self, q: Quat, near: Option<Vec3>) -> Vec3 {
        let [a, b, c] = self.axes();
        let (ec, eb, ea) = q.to_euler(self.glam());
        let mut out = Vec3::ZERO;
        out[a] = ea.to_degrees();
        out[b] = eb.to_degrees();
        out[c] = ec.to_degrees();
        if let Some(n) = near {
            // alternate solution: (a+180, 180-b, c+180)
            let mut alt = out;
            alt[a] += 180.0;
            alt[b] = 180.0 - alt[b];
            alt[c] += 180.0;
            let wrap = |v: Vec3| {
                let mut v = v;
                for i in 0..3 {
                    v[i] += ((n[i] - v[i]) / 360.0).round() * 360.0;
                }
                v
            };
            let o1 = wrap(out);
            let o2 = wrap(alt);
            return if (o1 - n).length_squared() <= (o2 - n).length_squared() { o1 } else { o2 };
        }
        out
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    pub translate: Vec3,
    /// Euler rotation in degrees.
    pub rotate: Vec3,
    pub scale: Vec3,
    #[serde(default)]
    pub rotate_order: RotateOrder,
    /// Rotate/scale pivot in local space.
    #[serde(default)]
    pub pivot: Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Self { translate: Vec3::ZERO, rotate: Vec3::ZERO, scale: Vec3::ONE, rotate_order: RotateOrder::XYZ, pivot: Vec3::ZERO }
    }
}

impl Transform {
    pub fn rotation(&self) -> Quat {
        self.rotate_order.to_quat(self.rotate)
    }
    /// Local matrix: T * Tpivot * JO * R * S * Tpivot⁻¹.
    pub fn matrix(&self, joint_orient: Option<Vec3>) -> Mat4 {
        let jo = joint_orient.map(|o| RotateOrder::XYZ.to_quat(o)).unwrap_or(Quat::IDENTITY);
        Mat4::from_translation(self.translate + self.pivot)
            * Mat4::from_quat(jo * self.rotation())
            * Mat4::from_scale(self.scale)
            * Mat4::from_translation(-self.pivot)
    }
    /// Set from a matrix (pivot preserved, joint orient divided out).
    pub fn set_from_matrix(&mut self, m: Mat4, joint_orient: Option<Vec3>) {
        let (s, r, t) = m.to_scale_rotation_translation();
        let jo = joint_orient.map(|o| RotateOrder::XYZ.to_quat(o)).unwrap_or(Quat::IDENTITY);
        let r_local = jo.inverse() * r;
        self.scale = s;
        self.rotate = self.rotate_order.from_quat(r_local, Some(self.rotate));
        // t = translate + pivot - (JO R S) pivot
        let rs = Mat3::from_quat(r) * Mat3::from_diagonal(s);
        self.translate = t - self.pivot + rs * self.pivot;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LightType {
    Directional,
    Point,
    Spot,
    Area,
    Ambient,
}

impl LightType {
    pub fn name(self) -> &'static str {
        match self {
            LightType::Directional => "directionalLight",
            LightType::Point => "pointLight",
            LightType::Spot => "spotLight",
            LightType::Area => "areaLight",
            LightType::Ambient => "ambientLight",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LightShape {
    pub kind: LightType,
    pub color: Vec3,
    pub intensity: f32,
    /// Spot cone angle in degrees.
    #[serde(default = "default_cone")]
    pub cone_angle: f32,
    #[serde(default)]
    pub penumbra: f32,
    /// 0 none, 1 linear, 2 quadratic.
    #[serde(default)]
    pub decay: u32,
    #[serde(default = "default_true")]
    pub cast_shadows: bool,
}

fn default_cone() -> f32 {
    40.0
}
fn default_true() -> bool {
    true
}

impl LightShape {
    pub fn new(kind: LightType) -> Self {
        Self { kind, color: Vec3::ONE, intensity: 1.0, cone_angle: 40.0, penumbra: 0.0, decay: 0, cast_shadows: true }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CameraShape {
    /// Focal length in mm against a 36mm-wide film back (Maya default 35mm).
    pub focal_length: f32,
    pub near: f32,
    pub far: f32,
    pub orthographic: bool,
    pub ortho_width: f32,
    #[serde(default = "default_aperture")]
    pub horizontal_aperture_mm: f32,
}

fn default_aperture() -> f32 {
    36.0
}

impl Default for CameraShape {
    fn default() -> Self {
        Self { focal_length: 35.0, near: 0.1, far: 10000.0, orthographic: false, ortho_width: 30.0, horizontal_aperture_mm: 36.0 }
    }
}

impl CameraShape {
    /// Horizontal field of view in radians.
    pub fn hfov(&self) -> f32 {
        2.0 * (self.horizontal_aperture_mm * 0.5 / self.focal_length.max(1e-3)).atan()
    }
    pub fn vfov(&self, aspect: f32) -> f32 {
        2.0 * ((self.hfov() * 0.5).tan() / aspect.max(1e-6)).atan()
    }
    pub fn projection(&self, aspect: f32) -> Mat4 {
        if self.orthographic {
            let w = self.ortho_width * 0.5;
            let h = w / aspect.max(1e-6);
            ortho_gl(-w, w, -h, h, -self.far, self.far)
        } else {
            perspective_gl(self.vfov(aspect), aspect, self.near, self.far)
        }
    }
}

/// Right-handed OpenGL-style perspective projection (clip z in -1..1).
pub fn perspective_gl(fovy: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fovy * 0.5).tan();
    let nf = 1.0 / (near - far);
    Mat4::from_cols(
        glam::Vec4::new(f / aspect, 0.0, 0.0, 0.0),
        glam::Vec4::new(0.0, f, 0.0, 0.0),
        glam::Vec4::new(0.0, 0.0, (far + near) * nf, -1.0),
        glam::Vec4::new(0.0, 0.0, 2.0 * far * near * nf, 0.0),
    )
}

/// Right-handed OpenGL-style orthographic projection.
pub fn ortho_gl(l: f32, r: f32, b: f32, t: f32, n: f32, f: f32) -> Mat4 {
    Mat4::from_cols(
        glam::Vec4::new(2.0 / (r - l), 0.0, 0.0, 0.0),
        glam::Vec4::new(0.0, 2.0 / (t - b), 0.0, 0.0),
        glam::Vec4::new(0.0, 0.0, -2.0 / (f - n), 0.0),
        glam::Vec4::new(-(r + l) / (r - l), -(t + b) / (t - b), -(f + n) / (f - n), 1.0),
    )
}

/// Right-handed view matrix.
pub fn look_at(eye: Vec3, center: Vec3, up: Vec3) -> Mat4 {
    let f = (center - eye).normalize();
    let s = f.cross(up).normalize();
    let u = s.cross(f);
    Mat4::from_cols(
        glam::Vec4::new(s.x, u.x, -f.x, 0.0),
        glam::Vec4::new(s.y, u.y, -f.y, 0.0),
        glam::Vec4::new(s.z, u.z, -f.z, 0.0),
        glam::Vec4::new(-s.dot(eye), -u.dot(eye), f.dot(eye), 1.0),
    )
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JointShape {
    pub radius: f32,
    /// Joint orient in degrees (xyz order).
    pub orient: Vec3,
}

impl Default for JointShape {
    fn default() -> Self {
        Self { radius: 0.5, orient: Vec3::ZERO }
    }
}

/// A NURBS/linear curve used for rig controls and paths.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CurveShape {
    pub cvs: Vec<Vec3>,
    pub degree: u32,
    pub periodic: bool,
}

impl CurveShape {
    pub fn circle(radius: f32, sections: usize, normal_axis: usize) -> Self {
        let n = sections.max(3);
        // CVs are pushed out so that the uniform cubic B-spline passes
        // through `radius` (factor for 8 sections ≈ 1.0824).
        let k = 3.0 / (2.0 + (std::f32::consts::TAU / n as f32).cos());
        let cvs = (0..n)
            .map(|i| {
                let a = std::f32::consts::TAU * i as f32 / n as f32;
                let (s, c) = a.sin_cos();
                let p = match normal_axis {
                    0 => Vec3::new(0.0, c, s),
                    1 => Vec3::new(c, 0.0, -s),
                    _ => Vec3::new(c, s, 0.0),
                };
                p * radius * k
            })
            .collect();
        Self { cvs, degree: 3, periodic: true }
    }

    pub fn square(size: f32) -> Self {
        let h = size * 0.5;
        Self {
            cvs: vec![
                Vec3::new(-h, 0.0, -h),
                Vec3::new(h, 0.0, -h),
                Vec3::new(h, 0.0, h),
                Vec3::new(-h, 0.0, h),
                Vec3::new(-h, 0.0, -h),
            ],
            degree: 1,
            periodic: false,
        }
    }

    /// Tessellate to a polyline.
    pub fn points(&self, per_span: usize) -> Vec<Vec3> {
        let n = self.cvs.len();
        if n == 0 {
            return Vec::new();
        }
        if self.degree <= 1 || n < 4 {
            let mut v = self.cvs.clone();
            if self.periodic {
                v.push(self.cvs[0]);
            }
            return v;
        }
        let cv = |i: isize| -> Vec3 {
            if self.periodic {
                self.cvs[i.rem_euclid(n as isize) as usize]
            } else {
                self.cvs[i.clamp(0, n as isize - 1) as usize]
            }
        };
        let spans = if self.periodic { n as isize } else { n as isize + 1 };
        let start = if self.periodic { 0 } else { -2 };
        let mut out = Vec::new();
        for s in start..start + spans {
            for k in 0..per_span {
                let t = k as f32 / per_span as f32;
                let (p0, p1, p2, p3) = (cv(s - 1), cv(s), cv(s + 1), cv(s + 2));
                let t2 = t * t;
                let t3 = t2 * t;
                let b0 = (1.0 - t).powi(3) / 6.0;
                let b1 = (3.0 * t3 - 6.0 * t2 + 4.0) / 6.0;
                let b2 = (-3.0 * t3 + 3.0 * t2 + 3.0 * t + 1.0) / 6.0;
                let b3 = t3 / 6.0;
                out.push(p0 * b0 + p1 * b1 + p2 * b2 + p3 * b3);
            }
        }
        if self.periodic {
            out.push(out[0]);
        } else {
            out.push(*self.cvs.last().unwrap());
        }
        out
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IkSolver {
    /// Rotate-plane solver (pole vector controls the chain plane).
    RotatePlane,
    /// Single-chain solver.
    SingleChain,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IkHandleShape {
    pub start_joint: NodeId,
    pub end_joint: NodeId,
    pub solver: IkSolver,
    /// Pole vector relative to the start joint (world space).
    pub pole_vector: Vec3,
    /// Optional object whose position drives the pole vector.
    #[serde(default)]
    pub pole_target: Option<NodeId>,
    pub twist: f32,
    #[serde(default = "default_one")]
    pub ik_blend: f32,
}

fn default_one() -> f32 {
    1.0
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkinCluster {
    pub joints: Vec<NodeId>,
    /// Inverse of each joint's world matrix at bind time.
    pub inverse_bind: Vec<Mat4>,
    /// Mesh world matrix at bind time.
    pub geom_bind: Mat4,
    /// Per-vertex sparse weights (joint slot, weight).
    pub weights: Vec<Vec<(u32, f32)>>,
    #[serde(default = "default_one")]
    pub envelope: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BlendTarget {
    pub name: String,
    /// Per-vertex offsets from the base mesh.
    pub deltas: Vec<Vec3>,
    pub weight: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeformerKind {
    Bend,
    Twist,
    Squash,
    Flare,
    Sine,
    Wave,
}

impl DeformerKind {
    pub fn name(self) -> &'static str {
        match self {
            DeformerKind::Bend => "bend",
            DeformerKind::Twist => "twist",
            DeformerKind::Squash => "squash",
            DeformerKind::Flare => "flare",
            DeformerKind::Sine => "sine",
            DeformerKind::Wave => "wave",
        }
    }
    pub fn params(self) -> &'static [&'static str] {
        match self {
            DeformerKind::Bend => &["curvature"],
            DeformerKind::Twist => &["startAngle", "endAngle"],
            DeformerKind::Squash => &["factor"],
            DeformerKind::Flare => &["startFlare", "endFlare", "curve"],
            DeformerKind::Sine => &["amplitude", "wavelength", "offset"],
            DeformerKind::Wave => &["amplitude", "wavelength", "offset"],
        }
    }
}

/// Nonlinear deformer evaluated in the mesh's object space along Y.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Deformer {
    pub name: String,
    pub kind: DeformerKind,
    pub params: BTreeMap<String, f32>,
    pub envelope: f32,
    /// Deformation region in object-space Y; computed from bounds when created.
    pub low_bound: f32,
    pub high_bound: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeshShape {
    pub mesh: Arc<Mesh>,
    #[serde(default)]
    pub material: Option<String>,
    /// Smooth mesh preview level (Maya's 1/2/3 keys); 0 = off.
    #[serde(default)]
    pub smooth_level: u32,
    #[serde(default)]
    pub skin: Option<Arc<SkinCluster>>,
    #[serde(default)]
    pub blend_shapes: Vec<BlendTarget>,
    #[serde(default)]
    pub deformers: Vec<Deformer>,
}

impl MeshShape {
    pub fn new(mesh: Mesh) -> Self {
        Self { mesh: Arc::new(mesh), material: None, smooth_level: 0, skin: None, blend_shapes: Vec::new(), deformers: Vec::new() }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum NodeKind {
    Transform,
    Mesh(MeshShape),
    Camera(CameraShape),
    Light(LightShape),
    Joint(JointShape),
    Locator,
    Curve(CurveShape),
    IkHandle(IkHandleShape),
}

impl NodeKind {
    pub fn type_name(&self) -> &'static str {
        match self {
            NodeKind::Transform => "transform",
            NodeKind::Mesh(_) => "mesh",
            NodeKind::Camera(_) => "camera",
            NodeKind::Light(l) => l.kind.name(),
            NodeKind::Joint(_) => "joint",
            NodeKind::Locator => "locator",
            NodeKind::Curve(_) => "nurbsCurve",
            NodeKind::IkHandle(_) => "ikHandle",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub name: String,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub transform: Transform,
    pub visible: bool,
    pub kind: NodeKind,
    /// Extra keyable attributes (addAttr).
    #[serde(default)]
    pub user_attrs: BTreeMap<String, f64>,
    /// Locked attributes (channel box lock).
    #[serde(default)]
    pub locked: BTreeSet<String>,
    /// Animation curves keyed by long attribute name.
    #[serde(default)]
    pub anim: BTreeMap<String, AnimCurve>,
    /// Wireframe color override (rgb 0..1).
    #[serde(default)]
    pub color: Option<Vec3>,
    /// Template (unselectable, gray) display.
    #[serde(default)]
    pub template: bool,
    /// Free-form notes (Attribute Editor "Notes").
    #[serde(default)]
    pub notes: String,
}

impl Node {
    pub fn joint_orient(&self) -> Option<Vec3> {
        match &self.kind {
            NodeKind::Joint(j) => Some(j.orient),
            _ => None,
        }
    }
    pub fn local_matrix(&self) -> Mat4 {
        self.transform.matrix(self.joint_orient())
    }
    pub fn mesh(&self) -> Option<&MeshShape> {
        match &self.kind {
            NodeKind::Mesh(m) => Some(m),
            _ => None,
        }
    }
    pub fn mesh_mut(&mut self) -> Option<&mut MeshShape> {
        match &mut self.kind {
            NodeKind::Mesh(m) => Some(m),
            _ => None,
        }
    }
    pub fn is_joint(&self) -> bool {
        matches!(self.kind, NodeKind::Joint(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn euler_roundtrip_all_orders() {
        let angles = Vec3::new(30.0, -45.0, 70.0);
        for o in RotateOrder::ALL {
            let q = o.to_quat(angles);
            let back = o.from_quat(q, None);
            assert!((back - angles).length() < 1e-3, "{o:?}: {back:?}");
        }
    }

    #[test]
    fn maya_xyz_applies_x_first() {
        // Rotating +Y by rx=90 then rz=90 (xyz): Y -> Z (by X), Z stays under Rz.
        let q = RotateOrder::XYZ.to_quat(Vec3::new(90.0, 0.0, 90.0));
        let v = q * Vec3::Y;
        assert!((v - Vec3::Z).length() < 1e-5, "{v:?}");
    }

    #[test]
    fn transform_matrix_roundtrip() {
        let mut t = Transform { translate: Vec3::new(1.0, 2.0, 3.0), rotate: Vec3::new(10.0, 20.0, 30.0), scale: Vec3::new(1.0, 2.0, 0.5), ..Default::default() };
        t.pivot = Vec3::new(0.5, 0.0, 0.0);
        let m = t.matrix(None);
        let mut u = Transform { pivot: t.pivot, ..Default::default() };
        u.set_from_matrix(m, None);
        assert!((u.matrix(None) - m).abs_diff_eq(Mat4::ZERO, 1e-4));
    }

    #[test]
    fn circle_passes_through_radius() {
        let c = CurveShape::circle(2.0, 8, 1);
        for p in c.points(8) {
            assert!((p.length() - 2.0).abs() < 0.03, "{}", p.length());
        }
    }
}
