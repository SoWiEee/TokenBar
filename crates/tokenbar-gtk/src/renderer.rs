//! glow renderer for the 3D contribution graph.
//!
//! Step B: one instanced draw of a unit cube, once per `Bar` — each instance
//! carries its grid offset, height, and color. A perspective camera framed to
//! the grid width looks down the depth axis. Orbit controls and real
//! `tb_reports` data come next.

use glam::Mat4;
use glow::HasContext;

use crate::graph::Bar;

pub struct Renderer {
    gl: glow::Context,
    program: glow::Program,
    vao: glow::VertexArray,
    cube_vbo: glow::Buffer,
    inst_vbo: glow::Buffer,
    u_vp: glow::UniformLocation,
    vertex_count: i32,
    instance_count: i32,
}

const VERT_SRC: &str = r#"#version 330 core
layout (location = 0) in vec3 a_pos;
layout (location = 1) in vec3 a_normal;
layout (location = 2) in vec2 i_offset;
layout (location = 3) in float i_height;
layout (location = 4) in vec3 i_color;
uniform mat4 u_vp;
out vec3 v_normal;
out vec3 v_color;
void main() {
    vec3 p = a_pos;
    p.xz *= 0.82;          // shrink so a gap shows between bars
    p.y *= i_height;       // grow upward from y = 0
    p.x += i_offset.x;
    p.z += i_offset.y;
    v_normal = a_normal;
    v_color = i_color;
    gl_Position = u_vp * vec4(p, 1.0);
}
"#;

const FRAG_SRC: &str = r#"#version 330 core
in vec3 v_normal;
in vec3 v_color;
out vec4 f_color;
void main() {
    vec3 light_dir = normalize(vec3(0.45, 0.85, 0.55));
    float diff = max(dot(normalize(v_normal), light_dir), 0.0);
    vec3 col = v_color * (0.34 + 0.66 * diff);
    f_color = vec4(col, 1.0);
}
"#;

/// Interleaved (position.xyz, normal.xyz) for a unit cube: base on y = 0, top on
/// y = 1, so scaling y grows a bar upward.
fn cube_mesh() -> Vec<f32> {
    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        ([0., 1., 0.], [[-0.5, 1., -0.5], [-0.5, 1., 0.5], [0.5, 1., 0.5], [0.5, 1., -0.5]]),
        ([0., -1., 0.], [[-0.5, 0., 0.5], [-0.5, 0., -0.5], [0.5, 0., -0.5], [0.5, 0., 0.5]]),
        ([0., 0., 1.], [[-0.5, 0., 0.5], [0.5, 0., 0.5], [0.5, 1., 0.5], [-0.5, 1., 0.5]]),
        ([0., 0., -1.], [[0.5, 0., -0.5], [-0.5, 0., -0.5], [-0.5, 1., -0.5], [0.5, 1., -0.5]]),
        ([1., 0., 0.], [[0.5, 0., 0.5], [0.5, 0., -0.5], [0.5, 1., -0.5], [0.5, 1., 0.5]]),
        ([-1., 0., 0.], [[-0.5, 0., -0.5], [-0.5, 0., 0.5], [-0.5, 1., 0.5], [-0.5, 1., -0.5]]),
    ];
    let mut v = Vec::with_capacity(36 * 6);
    for (normal, corners) in faces {
        for &i in &[0usize, 1, 2, 0, 2, 3] {
            v.extend_from_slice(&corners[i]);
            v.extend_from_slice(&normal);
        }
    }
    v
}

/// Pack bars into the per-instance buffer layout: (x, z, height, r, g, b).
fn instance_data(bars: &[Bar]) -> Vec<f32> {
    let mut v = Vec::with_capacity(bars.len() * 6);
    for b in bars {
        v.extend_from_slice(&[b.x, b.z, b.height, b.color[0], b.color[1], b.color[2]]);
    }
    v
}

impl Renderer {
    pub fn new(gl: glow::Context, bars: &[Bar]) -> Result<Self, String> {
        unsafe {
            let program = link_program(&gl, VERT_SRC, FRAG_SRC)?;
            let u_vp = gl.get_uniform_location(program, "u_vp").ok_or("uniform u_vp not found")?;

            let vao = gl.create_vertex_array().map_err(|e| format!("create VAO: {e}"))?;
            gl.bind_vertex_array(Some(vao));

            // Cube mesh: position (loc 0) + normal (loc 1), per-vertex.
            let mesh = cube_mesh();
            let vertex_count = (mesh.len() / 6) as i32;
            let cube_vbo = gl.create_buffer().map_err(|e| format!("create cube VBO: {e}"))?;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(cube_vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(&mesh), glow::STATIC_DRAW);
            let vstride = 6 * F32 as i32;
            gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, vstride, 0);
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, vstride, 3 * F32 as i32);
            gl.enable_vertex_attrib_array(1);

            // Per-instance: offset.xz (loc 2), height (loc 3), color.rgb (loc 4).
            let inst = instance_data(bars);
            let inst_vbo = gl.create_buffer().map_err(|e| format!("create inst VBO: {e}"))?;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(inst_vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(&inst), glow::STATIC_DRAW);
            let istride = 6 * F32 as i32;
            gl.vertex_attrib_pointer_f32(2, 2, glow::FLOAT, false, istride, 0);
            gl.enable_vertex_attrib_array(2);
            gl.vertex_attrib_divisor(2, 1);
            gl.vertex_attrib_pointer_f32(3, 1, glow::FLOAT, false, istride, 2 * F32 as i32);
            gl.enable_vertex_attrib_array(3);
            gl.vertex_attrib_divisor(3, 1);
            gl.vertex_attrib_pointer_f32(4, 3, glow::FLOAT, false, istride, 3 * F32 as i32);
            gl.enable_vertex_attrib_array(4);
            gl.vertex_attrib_divisor(4, 1);

            gl.bind_vertex_array(None);

            Ok(Self {
                gl,
                program,
                vao,
                cube_vbo,
                inst_vbo,
                u_vp,
                vertex_count,
                instance_count: bars.len() as i32,
            })
        }
    }

    /// Draw all bars with the given view-projection matrix (from the camera).
    pub fn draw(&self, vp: &Mat4) {
        let gl = &self.gl;
        unsafe {
            gl.enable(glow::DEPTH_TEST);
            gl.clear_color(0.05, 0.05, 0.07, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
            gl.use_program(Some(self.program));
            gl.uniform_matrix_4_f32_slice(Some(&self.u_vp), false, &vp.to_cols_array());
            gl.bind_vertex_array(Some(self.vao));
            gl.draw_arrays_instanced(glow::TRIANGLES, 0, self.vertex_count, self.instance_count);
            gl.bind_vertex_array(None);
        }
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        let gl = &self.gl;
        unsafe {
            gl.delete_buffer(self.cube_vbo);
            gl.delete_buffer(self.inst_vbo);
            gl.delete_vertex_array(self.vao);
            gl.delete_program(self.program);
        }
    }
}

const F32: usize = core::mem::size_of::<f32>();

/// View a `&[f32]` as raw bytes for `buffer_data_u8_slice`.
fn as_bytes(v: &[f32]) -> &[u8] {
    unsafe { core::slice::from_raw_parts(v.as_ptr() as *const u8, v.len() * F32) }
}

unsafe fn link_program(
    gl: &glow::Context,
    vert: &str,
    frag: &str,
) -> Result<glow::Program, String> {
    let compile = |ty: u32, src: &str| -> Result<glow::Shader, String> {
        let shader = gl.create_shader(ty).map_err(|e| format!("create shader: {e}"))?;
        gl.shader_source(shader, src);
        gl.compile_shader(shader);
        if !gl.get_shader_compile_status(shader) {
            let log = gl.get_shader_info_log(shader);
            gl.delete_shader(shader);
            return Err(format!("shader compile failed: {log}"));
        }
        Ok(shader)
    };

    let vs = compile(glow::VERTEX_SHADER, vert)?;
    let fs = compile(glow::FRAGMENT_SHADER, frag)?;

    let program = gl.create_program().map_err(|e| format!("create program: {e}"))?;
    gl.attach_shader(program, vs);
    gl.attach_shader(program, fs);
    gl.link_program(program);
    gl.detach_shader(program, vs);
    gl.detach_shader(program, fs);
    gl.delete_shader(vs);
    gl.delete_shader(fs);
    if !gl.get_program_link_status(program) {
        let log = gl.get_program_info_log(program);
        gl.delete_program(program);
        return Err(format!("program link failed: {log}"));
    }
    Ok(program)
}
