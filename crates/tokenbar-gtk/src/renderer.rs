//! Minimal glow renderer for the Phase 1 GLArea toolchain probe.
//!
//! Draws a single RGB triangle to prove the whole chain works on this machine:
//! GTK4 GLArea context → epoxy function loading → glow → a compiled shader
//! program drawing real geometry. The orbitable 3D contribution graph replaces
//! this renderer once the chain is confirmed.

use glow::HasContext;

/// Owns the GL program and vertex buffers. GL resources are tied to the GLArea's
/// context: build in `realize`, drop in `unrealize`.
pub struct Renderer {
    gl: glow::Context,
    program: glow::Program,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
}

const VERT_SRC: &str = r#"#version 330 core
layout (location = 0) in vec2 a_pos;
layout (location = 1) in vec3 a_color;
out vec3 v_color;
void main() {
    v_color = a_color;
    gl_Position = vec4(a_pos, 0.0, 1.0);
}
"#;

const FRAG_SRC: &str = r#"#version 330 core
in vec3 v_color;
out vec4 f_color;
void main() {
    f_color = vec4(v_color, 1.0);
}
"#;

// Interleaved (x, y, r, g, b) for three vertices.
#[rustfmt::skip]
const VERTICES: [f32; 15] = [
     0.0,  0.6,   1.0, 0.25, 0.30,
    -0.6, -0.5,   0.30, 0.85, 0.45,
     0.6, -0.5,   0.30, 0.45, 1.0,
];

impl Renderer {
    /// Build the program and upload the triangle. `gl` must be current (the
    /// caller made the GLArea context current before constructing).
    pub fn new(gl: glow::Context) -> Result<Self, String> {
        unsafe {
            let program = link_program(&gl, VERT_SRC, FRAG_SRC)?;

            let vao = gl
                .create_vertex_array()
                .map_err(|e| format!("create VAO: {e}"))?;
            gl.bind_vertex_array(Some(vao));

            let vbo = gl.create_buffer().map_err(|e| format!("create VBO: {e}"))?;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            let bytes = core::slice::from_raw_parts(
                VERTICES.as_ptr() as *const u8,
                core::mem::size_of_val(&VERTICES),
            );
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STATIC_DRAW);

            let stride = 5 * core::mem::size_of::<f32>() as i32;
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, stride, 0);
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(
                1,
                3,
                glow::FLOAT,
                false,
                stride,
                2 * core::mem::size_of::<f32>() as i32,
            );
            gl.enable_vertex_attrib_array(1);

            gl.bind_vertex_array(None);
            Ok(Self { gl, program, vao, vbo })
        }
    }

    /// Draw one frame into the GLArea's framebuffer (already bound by GTK).
    pub fn draw(&self) {
        let gl = &self.gl;
        unsafe {
            gl.clear_color(0.06, 0.06, 0.08, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.use_program(Some(self.program));
            gl.bind_vertex_array(Some(self.vao));
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.bind_vertex_array(None);
        }
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        let gl = &self.gl;
        unsafe {
            gl.delete_buffer(self.vbo);
            gl.delete_vertex_array(self.vao);
            gl.delete_program(self.program);
        }
    }
}

/// Compile + link a vertex/fragment program, surfacing compile/link logs as
/// errors instead of leaving a silently-broken program bound.
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
    // Shaders can be detached/deleted once linked into the program.
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
