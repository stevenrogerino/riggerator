//! OpenGL viewport renderer (runs inside an egui paint callback).

use egui_glow::glow::{self, HasContext};
use glam::{Mat3, Mat4, Vec3};
use rg_core::mesh::{Edge, Mesh};
use std::collections::HashMap;
use std::sync::Arc;

/// CPU-side render data for one mesh (interleaved pos/normal/uv).
pub struct CpuMesh {
    pub verts: Vec<f32>,
    pub tris: Vec<u32>,
    pub edges: Vec<u32>,
}

impl CpuMesh {
    pub fn build(mesh: &Mesh) -> Self {
        let rm = mesh.to_render();
        let mut verts = Vec::with_capacity(rm.positions.len() * 8);
        for i in 0..rm.positions.len() {
            let p = rm.positions[i];
            let n = rm.normals[i];
            let uv = rm.uvs[i];
            verts.extend_from_slice(&[p.x, p.y, p.z, n.x, n.y, n.z, uv.x, uv.y]);
        }
        // polygon edges (not triangle diagonals) referencing face-corner verts
        let mut edges = Vec::new();
        let mut seen: std::collections::HashSet<Edge> = std::collections::HashSet::new();
        let mut base = 0u32;
        for f in &mesh.faces {
            let n = f.verts.len() as u32;
            if n < 3 {
                continue;
            }
            for i in 0..n {
                let a = f.verts[i as usize];
                let b = f.verts[((i + 1) % n) as usize];
                if seen.insert(Edge::new(a, b)) {
                    edges.push(base + i);
                    edges.push(base + (i + 1) % n);
                }
            }
            base += n;
        }
        Self { verts, tris: rm.indices, edges }
    }
}

struct GpuMesh {
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    ebo: glow::Buffer,
    eebo: glow::Buffer,
    n_tris: i32,
    n_edges: i32,
    _keep: Arc<Mesh>,
    last_used: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MeshKey {
    pub ptr: usize,
    pub smooth: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TexMode {
    None,
    Checker { color2: [f32; 3], repeats: f32 },
    Ramp { color2: [f32; 3] },
    File,
}

#[derive(Clone, Debug)]
pub struct DrawItem {
    pub key: MeshKey,
    pub model: Mat4,
    pub color: [f32; 4],
    pub shaded: bool,
    pub wire: Option<[f32; 4]>,
    pub spec: f32,
    pub rough: f32,
    pub emission: [f32; 3],
    pub tex: TexMode,
    pub tex_path: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub struct GlLight {
    /// w = 0 directional (xyz = direction light travels), w = 1 positional
    pub pos: [f32; 4],
    pub dir: [f32; 3],
    pub color: [f32; 3],
    /// cos(outer), cos(inner) for spots; (-2,-2) for non-spots
    pub cone: [f32; 2],
}

#[derive(Clone, Default)]
pub struct DrawList {
    pub view: Mat4,
    pub proj: Mat4,
    pub eye: Vec3,
    pub items: Vec<DrawItem>,
    /// pos(3) color(4)
    pub lines: Vec<f32>,
    /// lines drawn on top of geometry (no depth test)
    pub lines_top: Vec<f32>,
    /// pos(3) color(4), translucent overlay triangles
    pub tris_overlay: Vec<f32>,
    /// overlay triangles drawn without depth test (smooth-preview cages)
    pub tris_overlay_top: Vec<f32>,
    pub lights: Vec<GlLight>,
    pub use_lights: bool,
    pub xray: bool,
    pub backface_cull: bool,
    pub flat_lighting: bool,
}

pub struct GlRenderer {
    prog_mesh: glow::Program,
    prog_line: glow::Program,
    meshes: HashMap<MeshKey, GpuMesh>,
    pending: Vec<(MeshKey, CpuMesh, Arc<Mesh>)>,
    textures: HashMap<String, Option<glow::Texture>>,
    dyn_vao: glow::VertexArray,
    dyn_vbo: glow::Buffer,
    frame: u64,
    pub error: Option<String>,
}

const MESH_VS: &str = r#"
layout(location = 0) in vec3 a_pos;
layout(location = 1) in vec3 a_nrm;
layout(location = 2) in vec2 a_uv;
uniform mat4 u_mvp;
uniform mat4 u_model;
uniform mat3 u_nmat;
out vec3 v_n;
out vec3 v_wp;
out vec2 v_uv;
void main() {
    v_wp = (u_model * vec4(a_pos, 1.0)).xyz;
    v_n = u_nmat * a_nrm;
    v_uv = a_uv;
    gl_Position = u_mvp * vec4(a_pos, 1.0);
}
"#;

const MESH_FS: &str = r#"
in vec3 v_n;
in vec3 v_wp;
in vec2 v_uv;
uniform vec4 u_color;
uniform vec3 u_eye;
uniform int u_mode;        // 0 shaded, 1 flat color (wire)
uniform int u_tex;         // 0 none, 1 checker, 2 ramp, 3 file
uniform vec3 u_color2;
uniform float u_rep;
uniform sampler2D u_sampler;
uniform int u_nlights;
uniform int u_use_lights;
uniform int u_flat;
uniform vec4 u_lpos[8];
uniform vec3 u_ldir[8];
uniform vec3 u_lcol[8];
uniform vec2 u_lcone[8];
uniform float u_spec;
uniform float u_rough;
uniform vec3 u_emis;
out vec4 frag;

vec3 to_srgb(vec3 c) { return pow(clamp(c, 0.0, 1.0), vec3(1.0 / 2.2)); }

void main() {
    if (u_mode == 1) { frag = u_color; return; }
    vec3 base = u_color.rgb;
    if (u_tex == 1) {
        float c = mod(floor(v_uv.x * u_rep) + floor(v_uv.y * u_rep), 2.0);
        base = mix(base, u_color2, c);
    } else if (u_tex == 2) {
        base = mix(base, u_color2, clamp(v_uv.y, 0.0, 1.0));
    } else if (u_tex == 3) {
        base = pow(texture(u_sampler, vec2(v_uv.x, 1.0 - v_uv.y)).rgb, vec3(2.2));
    }
    vec3 n = normalize(v_n);
    if (!gl_FrontFacing) n = -n;
    vec3 V = normalize(u_eye - v_wp);
    vec3 lit = vec3(0.0);
    if (u_flat == 1) {
        lit = base;
    } else if (u_use_lights == 0 || u_nlights == 0) {
        // Maya default lighting: a light from the camera plus soft ambient
        float d = max(dot(n, V), 0.0);
        vec3 H = V;
        float sh = mix(8.0, 256.0, 1.0 - u_rough);
        float s = pow(max(dot(n, H), 0.0), sh) * u_spec;
        lit = base * (0.15 + 0.85 * d) + vec3(s) * 0.5;
    } else {
        lit = base * 0.05;
        for (int i = 0; i < 8; i++) {
            if (i >= u_nlights) break;
            vec3 L;
            float atten = 1.0;
            if (u_lpos[i].w == 0.0) {
                L = -normalize(u_ldir[i]);
            } else {
                vec3 tl = u_lpos[i].xyz - v_wp;
                L = normalize(tl);
                if (u_lcone[i].x > -1.5) {
                    float c = dot(-L, normalize(u_ldir[i]));
                    atten *= smoothstep(u_lcone[i].x, u_lcone[i].y, c);
                }
            }
            float d = max(dot(n, L), 0.0);
            vec3 H = normalize(L + V);
            float sh = mix(8.0, 256.0, 1.0 - u_rough);
            float s = pow(max(dot(n, H), 0.0), sh) * u_spec;
            lit += (base * d + vec3(s) * d) * u_lcol[i] * atten;
        }
    }
    lit += u_emis;
    frag = vec4(to_srgb(lit), u_color.a);
}
"#;

const LINE_VS: &str = r#"
layout(location = 0) in vec3 a_pos;
layout(location = 1) in vec4 a_col;
uniform mat4 u_mvp;
out vec4 v_col;
void main() { v_col = a_col; gl_Position = u_mvp * vec4(a_pos, 1.0); }
"#;

const LINE_FS: &str = r#"
in vec4 v_col;
out vec4 frag;
void main() { frag = v_col; }
"#;

unsafe fn compile(gl: &glow::Context, vs: &str, fs: &str) -> Result<glow::Program, String> {
    unsafe {
        let header = "#version 330 core\n";
        let prog = gl.create_program()?;
        let mut shaders = Vec::new();
        for (ty, src) in [(glow::VERTEX_SHADER, vs), (glow::FRAGMENT_SHADER, fs)] {
            let sh = gl.create_shader(ty)?;
            gl.shader_source(sh, &format!("{header}{src}"));
            gl.compile_shader(sh);
            if !gl.get_shader_compile_status(sh) {
                return Err(gl.get_shader_info_log(sh));
            }
            gl.attach_shader(prog, sh);
            shaders.push(sh);
        }
        gl.link_program(prog);
        if !gl.get_program_link_status(prog) {
            return Err(gl.get_program_info_log(prog));
        }
        for sh in shaders {
            gl.detach_shader(prog, sh);
            gl.delete_shader(sh);
        }
        Ok(prog)
    }
}

impl GlRenderer {
    pub fn new(gl: &glow::Context) -> Result<Self, String> {
        unsafe {
            let prog_mesh = compile(gl, MESH_VS, MESH_FS)?;
            let prog_line = compile(gl, LINE_VS, LINE_FS)?;
            let dyn_vao = gl.create_vertex_array()?;
            let dyn_vbo = gl.create_buffer()?;
            Ok(Self {
                prog_mesh,
                prog_line,
                meshes: HashMap::new(),
                pending: Vec::new(),
                textures: HashMap::new(),
                dyn_vao,
                dyn_vbo,
                frame: 0,
                error: None,
            })
        }
    }

    pub fn has(&self, key: &MeshKey) -> bool {
        self.meshes.contains_key(key) || self.pending.iter().any(|p| p.0 == *key)
    }

    pub fn queue_upload(&mut self, key: MeshKey, data: CpuMesh, keep: Arc<Mesh>) {
        self.pending.push((key, data, keep));
    }

    /// Advance the frame counter and free meshes unused for a while.
    pub fn begin_frame(&mut self, gl: &glow::Context) {
        self.frame += 1;
        let f = self.frame;
        let stale: Vec<MeshKey> = self.meshes.iter().filter(|(_, m)| f.saturating_sub(m.last_used) > 120).map(|(k, _)| *k).collect();
        for k in stale {
            if let Some(m) = self.meshes.remove(&k) {
                unsafe {
                    gl.delete_vertex_array(m.vao);
                    gl.delete_buffer(m.vbo);
                    gl.delete_buffer(m.ebo);
                    gl.delete_buffer(m.eebo);
                }
            }
        }
    }

    fn upload_pending(&mut self, gl: &glow::Context) {
        for (key, data, keep) in self.pending.drain(..) {
            unsafe {
                let Ok(vao) = gl.create_vertex_array() else { continue };
                let vbo = gl.create_buffer().unwrap();
                let ebo = gl.create_buffer().unwrap();
                let eebo = gl.create_buffer().unwrap();
                gl.bind_vertex_array(Some(vao));
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
                gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes_f32(&data.verts), glow::STATIC_DRAW);
                gl.enable_vertex_attrib_array(0);
                gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, 32, 0);
                gl.enable_vertex_attrib_array(1);
                gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, 32, 12);
                gl.enable_vertex_attrib_array(2);
                gl.vertex_attrib_pointer_f32(2, 2, glow::FLOAT, false, 32, 24);
                gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ebo));
                gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, bytes_u32(&data.tris), glow::STATIC_DRAW);
                gl.bind_vertex_array(None);
                gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(eebo));
                gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, bytes_u32(&data.edges), glow::STATIC_DRAW);
                gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, None);
                if let Some(old) = self.meshes.insert(
                    key,
                    GpuMesh { vao, vbo, ebo, eebo, n_tris: data.tris.len() as i32, n_edges: data.edges.len() as i32, _keep: keep, last_used: self.frame },
                ) {
                    gl.delete_vertex_array(old.vao);
                    gl.delete_buffer(old.vbo);
                    gl.delete_buffer(old.ebo);
                    gl.delete_buffer(old.eebo);
                }
            }
        }
    }

    fn texture(&mut self, gl: &glow::Context, path: &str) -> Option<glow::Texture> {
        if let Some(t) = self.textures.get(path) {
            return *t;
        }
        let tex = (|| unsafe {
            let img = image::open(path).ok()?.to_rgba8();
            let t = gl.create_texture().ok()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(t));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                img.width() as i32,
                img.height() as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(img.as_raw())),
            );
            gl.generate_mipmap(glow::TEXTURE_2D);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR_MIPMAP_LINEAR as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::REPEAT as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::REPEAT as i32);
            Some(t)
        })();
        self.textures.insert(path.to_string(), tex);
        tex
    }

    pub fn paint(&mut self, gl: &glow::Context, dl: &DrawList) {
        self.upload_pending(gl);
        unsafe {
            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::LEQUAL);
            gl.depth_mask(true);
            gl.clear_depth_f32(1.0);
            gl.clear(glow::DEPTH_BUFFER_BIT);
            gl.disable(glow::CULL_FACE);
            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);

            let vp = dl.proj * dl.view;
            // ---- grid & scene lines (depth tested)
            self.draw_lines(gl, &dl.lines, vp, true);

            // ---- meshes
            gl.use_program(Some(self.prog_mesh));
            let prog = self.prog_mesh;
            let loc = |n: &str| gl.get_uniform_location(prog, n);
            gl.uniform_3_f32(loc("u_eye").as_ref(), dl.eye.x, dl.eye.y, dl.eye.z);
            gl.uniform_1_i32(loc("u_use_lights").as_ref(), dl.use_lights as i32);
            gl.uniform_1_i32(loc("u_flat").as_ref(), dl.flat_lighting as i32);
            let nl = dl.lights.len().min(8);
            gl.uniform_1_i32(loc("u_nlights").as_ref(), nl as i32);
            for (i, l) in dl.lights.iter().take(8).enumerate() {
                gl.uniform_4_f32(loc(&format!("u_lpos[{i}]")).as_ref(), l.pos[0], l.pos[1], l.pos[2], l.pos[3]);
                gl.uniform_3_f32(loc(&format!("u_ldir[{i}]")).as_ref(), l.dir[0], l.dir[1], l.dir[2]);
                gl.uniform_3_f32(loc(&format!("u_lcol[{i}]")).as_ref(), l.color[0], l.color[1], l.color[2]);
                gl.uniform_2_f32(loc(&format!("u_lcone[{i}]")).as_ref(), l.cone[0], l.cone[1]);
            }
            if dl.backface_cull {
                gl.enable(glow::CULL_FACE);
                gl.cull_face(glow::BACK);
            }
            let u_mvp = loc("u_mvp");
            let u_model = loc("u_model");
            let u_nmat = loc("u_nmat");
            let u_color = loc("u_color");
            let u_mode = loc("u_mode");
            let u_tex = loc("u_tex");
            let u_color2 = loc("u_color2");
            let u_rep = loc("u_rep");
            let u_spec = loc("u_spec");
            let u_rough = loc("u_rough");
            let u_emis = loc("u_emis");
            let frame = self.frame;
            // opaque first, then translucent (x-ray / transparent)
            let mut order: Vec<&DrawItem> = dl.items.iter().filter(|i| i.shaded).collect();
            order.sort_by(|a, b| (a.color[3] < 0.999 || dl.xray).cmp(&(b.color[3] < 0.999 || dl.xray)));
            gl.enable(glow::POLYGON_OFFSET_FILL);
            gl.polygon_offset(1.0, 1.0);
            for it in order {
                let tex_id = match (&it.tex, &it.tex_path) {
                    (TexMode::File, Some(p)) => self.texture(gl, p),
                    _ => None,
                };
                let Some(m) = self.meshes.get_mut(&it.key) else { continue };
                m.last_used = frame;
                let mvp = vp * it.model;
                let nmat = Mat3::from_mat4(it.model).inverse().transpose();
                gl.uniform_matrix_4_f32_slice(u_mvp.as_ref(), false, &mvp.to_cols_array());
                gl.uniform_matrix_4_f32_slice(u_model.as_ref(), false, &it.model.to_cols_array());
                gl.uniform_matrix_3_f32_slice(u_nmat.as_ref(), false, &nmat.to_cols_array());
                let a = if dl.xray { it.color[3].min(0.45) } else { it.color[3] };
                gl.uniform_4_f32(u_color.as_ref(), it.color[0], it.color[1], it.color[2], a);
                gl.uniform_1_i32(u_mode.as_ref(), 0);
                gl.uniform_1_f32(u_spec.as_ref(), it.spec);
                gl.uniform_1_f32(u_rough.as_ref(), it.rough);
                gl.uniform_3_f32(u_emis.as_ref(), it.emission[0], it.emission[1], it.emission[2]);
                match it.tex {
                    TexMode::None => gl.uniform_1_i32(u_tex.as_ref(), 0),
                    TexMode::Checker { color2, repeats } => {
                        gl.uniform_1_i32(u_tex.as_ref(), 1);
                        gl.uniform_3_f32(u_color2.as_ref(), color2[0], color2[1], color2[2]);
                        gl.uniform_1_f32(u_rep.as_ref(), repeats);
                    }
                    TexMode::Ramp { color2 } => {
                        gl.uniform_1_i32(u_tex.as_ref(), 2);
                        gl.uniform_3_f32(u_color2.as_ref(), color2[0], color2[1], color2[2]);
                    }
                    TexMode::File => {
                        if let Some(t) = tex_id {
                            gl.active_texture(glow::TEXTURE0);
                            gl.bind_texture(glow::TEXTURE_2D, Some(t));
                            gl.uniform_1_i32(loc("u_sampler").as_ref(), 0);
                            gl.uniform_1_i32(u_tex.as_ref(), 3);
                        } else {
                            gl.uniform_1_i32(u_tex.as_ref(), 0);
                        }
                    }
                }
                if a < 0.999 {
                    gl.depth_mask(false);
                }
                gl.bind_vertex_array(Some(m.vao));
                gl.draw_elements(glow::TRIANGLES, m.n_tris, glow::UNSIGNED_INT, 0);
                gl.depth_mask(true);
            }
            gl.disable(glow::POLYGON_OFFSET_FILL);
            gl.disable(glow::CULL_FACE);
            // wireframes
            gl.uniform_1_i32(u_mode.as_ref(), 1);
            for it in &dl.items {
                let Some(w) = it.wire else { continue };
                let Some(m) = self.meshes.get_mut(&it.key) else { continue };
                m.last_used = frame;
                let mvp = vp * it.model;
                gl.uniform_matrix_4_f32_slice(u_mvp.as_ref(), false, &mvp.to_cols_array());
                gl.uniform_4_f32(u_color.as_ref(), w[0], w[1], w[2], w[3]);
                gl.bind_vertex_array(Some(m.vao));
                gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(m.eebo));
                gl.draw_elements(glow::LINES, m.n_edges, glow::UNSIGNED_INT, 0);
                // restore the VAO's element buffer binding
                gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(m.ebo));
            }
            gl.bind_vertex_array(None);

            // translucent face highlight overlay
            if !dl.tris_overlay.is_empty() {
                gl.enable(glow::POLYGON_OFFSET_FILL);
                gl.polygon_offset(-1.0, -1.0);
                gl.depth_mask(false);
                self.draw_prims(gl, &dl.tris_overlay, vp, glow::TRIANGLES);
                gl.depth_mask(true);
                gl.disable(glow::POLYGON_OFFSET_FILL);
            }
            // overlay lines
            gl.disable(glow::DEPTH_TEST);
            if !dl.tris_overlay_top.is_empty() {
                self.draw_prims(gl, &dl.tris_overlay_top, vp, glow::TRIANGLES);
            }
            self.draw_lines(gl, &dl.lines_top, vp, false);
            gl.disable(glow::BLEND);
            gl.use_program(None);
            if let Some(e) = check(gl) {
                self.error = Some(e);
            }
        }
    }

    unsafe fn draw_lines(&self, gl: &glow::Context, data: &[f32], vp: Mat4, depth: bool) {
        if data.is_empty() {
            return;
        }
        unsafe {
            if depth {
                gl.enable(glow::DEPTH_TEST);
            }
            self.draw_prims(gl, data, vp, glow::LINES);
        }
    }

    unsafe fn draw_prims(&self, gl: &glow::Context, data: &[f32], vp: Mat4, mode: u32) {
        unsafe {
            gl.use_program(Some(self.prog_line));
            let l = gl.get_uniform_location(self.prog_line, "u_mvp");
            gl.uniform_matrix_4_f32_slice(l.as_ref(), false, &vp.to_cols_array());
            gl.bind_vertex_array(Some(self.dyn_vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.dyn_vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes_f32(data), glow::STREAM_DRAW);
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, 28, 0);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 4, glow::FLOAT, false, 28, 12);
            gl.draw_arrays(mode, 0, (data.len() / 7) as i32);
            gl.bind_vertex_array(None);
            gl.use_program(Some(self.prog_mesh));
        }
    }
}

unsafe fn check(gl: &glow::Context) -> Option<String> {
    let e = unsafe { gl.get_error() };
    (e != glow::NO_ERROR).then(|| format!("GL error 0x{e:x}"))
}

fn bytes_f32(v: &[f32]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}
fn bytes_u32(v: &[u32]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}

/// Append a line segment (pos, rgba) to a line buffer.
pub fn push_line(buf: &mut Vec<f32>, a: Vec3, b: Vec3, c: [f32; 4]) {
    buf.extend_from_slice(&[a.x, a.y, a.z, c[0], c[1], c[2], c[3], b.x, b.y, b.z, c[0], c[1], c[2], c[3]]);
}

pub fn push_tri(buf: &mut Vec<f32>, a: Vec3, b: Vec3, c: Vec3, col: [f32; 4]) {
    for p in [a, b, c] {
        buf.extend_from_slice(&[p.x, p.y, p.z, col[0], col[1], col[2], col[3]]);
    }
}
