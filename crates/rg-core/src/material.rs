use glam::Vec3;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShaderType {
    Lambert,
    Blinn,
    Phong,
    /// Physically based "standardSurface"-style shader.
    StandardSurface,
    /// Unlit flat color.
    SurfaceShader,
}

impl ShaderType {
    pub const ALL: [ShaderType; 5] =
        [ShaderType::Lambert, ShaderType::Blinn, ShaderType::Phong, ShaderType::StandardSurface, ShaderType::SurfaceShader];
    pub fn name(self) -> &'static str {
        match self {
            ShaderType::Lambert => "lambert",
            ShaderType::Blinn => "blinn",
            ShaderType::Phong => "phong",
            ShaderType::StandardSurface => "standardSurface",
            ShaderType::SurfaceShader => "surfaceShader",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.name().eq_ignore_ascii_case(s))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Texture {
    None,
    Checker { color2: Vec3, repeats: f32 },
    /// Image file mapped by UVs.
    File { path: String },
    /// Vertical ramp between base color and color2.
    Ramp { color2: Vec3 },
    /// 3D procedural noise.
    Noise { color2: Vec3, scale: f32 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Material {
    pub name: String,
    pub shader: ShaderType,
    pub color: Vec3,
    pub diffuse: f32,
    pub specular_color: Vec3,
    pub specular: f32,
    /// 0 = mirror, 1 = fully rough.
    pub roughness: f32,
    pub metalness: f32,
    pub ior: f32,
    pub emission_color: Vec3,
    pub emission: f32,
    /// 1 = opaque.
    pub opacity: f32,
    /// Refractive transmission weight (glass).
    #[serde(default)]
    pub transmission: f32,
    pub texture: Texture,
}

impl Material {
    pub fn new(name: &str, shader: ShaderType) -> Self {
        let (spec, rough) = match shader {
            ShaderType::Lambert | ShaderType::SurfaceShader => (0.0, 1.0),
            ShaderType::Blinn => (0.5, 0.3),
            ShaderType::Phong => (0.6, 0.2),
            ShaderType::StandardSurface => (1.0, 0.2),
        };
        Self {
            name: name.to_string(),
            shader,
            color: Vec3::splat(0.5),
            diffuse: if shader == ShaderType::StandardSurface { 1.0 } else { 0.8 },
            specular_color: Vec3::ONE,
            specular: spec,
            roughness: rough,
            metalness: 0.0,
            ior: 1.5,
            emission_color: Vec3::ONE,
            emission: 0.0,
            opacity: 1.0,
            transmission: 0.0,
            texture: Texture::None,
        }
    }

    pub fn default_lambert() -> Self {
        Self::new("lambert1", ShaderType::Lambert)
    }

    pub const ATTRS: [&'static str; 14] = [
        "colorR", "colorG", "colorB", "diffuse", "specular", "roughness", "metalness", "ior", "emission", "opacity",
        "transmission", "emissionColorR", "emissionColorG", "emissionColorB",
    ];

    pub fn get(&self, attr: &str) -> Option<f32> {
        Some(match attr {
            "colorR" => self.color.x,
            "colorG" => self.color.y,
            "colorB" => self.color.z,
            "diffuse" => self.diffuse,
            "specular" => self.specular,
            "roughness" => self.roughness,
            "metalness" => self.metalness,
            "ior" => self.ior,
            "emission" => self.emission,
            "opacity" => self.opacity,
            "transmission" => self.transmission,
            "emissionColorR" => self.emission_color.x,
            "emissionColorG" => self.emission_color.y,
            "emissionColorB" => self.emission_color.z,
            _ => return None,
        })
    }

    pub fn set(&mut self, attr: &str, v: f32) -> bool {
        match attr {
            "colorR" => self.color.x = v,
            "colorG" => self.color.y = v,
            "colorB" => self.color.z = v,
            "diffuse" => self.diffuse = v,
            "specular" => self.specular = v,
            "roughness" => self.roughness = v,
            "metalness" => self.metalness = v,
            "ior" => self.ior = v,
            "emission" => self.emission = v,
            "opacity" => self.opacity = v,
            "transmission" => self.transmission = v,
            "emissionColorR" => self.emission_color.x = v,
            "emissionColorG" => self.emission_color.y = v,
            "emissionColorB" => self.emission_color.z = v,
            _ => return false,
        }
        true
    }
}
