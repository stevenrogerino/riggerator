//! rg-core — Riggerator's scene model: a Maya-style DAG of transforms with
//! shapes (meshes, cameras, lights, joints, curves, IK handles), keyable
//! attributes, animation, constraints, skinning, deformers and undo.

pub mod eval;
pub mod history;
pub mod material;
pub mod node;
pub mod scene;

pub use eval::Evaluated;
pub use history::History;
pub use material::{Material, ShaderType, Texture};
pub use node::*;
pub use scene::*;

pub use glam;
pub use rg_anim as anim;
pub use rg_mesh as mesh;
