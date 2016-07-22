extern crate byteorder;
#[macro_use] extern crate glium;
#[macro_use] extern crate lazy_static;
#[macro_use] extern crate log;
extern crate env_logger;
extern crate regex;

pub mod bsp;
pub mod engine;
pub mod gl;
pub mod mdl;

use std::process::exit;
use glium::{Frame, Surface};
use glium::draw_parameters::DrawParameters;
use glium::glutin::Event;
use glium::program::{Program, ProgramCreationInput};
use glium::index::NoIndices;
use mdl::Mdl;

#[derive(Copy, Clone)]
struct Vertex {
    pos: [f32; 3],
    texcoord: [f32; 2],
}
implement_vertex!(Vertex, pos, texcoord);

const PI: f32 = 3.14159265;

static IDENTITY_MATRIX: [[f32; 4]; 4] = [[1.0, 0.0, 0.0, 0.0],
                                         [0.0, 1.0, 0.0, 0.0],
                                         [0.0, 0.0, 1.0, 0.0],
                                         [0.0, 0.0, 0.0, 1.0]];

static VERTEX_SHADER: &'static str = r#"
#version 330

layout(location = 0) in vec3 pos;
layout(location = 1) in vec2 texcoord;

out vec2 Texcoord;

// uniform mat4 perspective;
// uniform mat4 view;
// uniform mat4 model;

void main() {
    Texcoord = texcoord;
    // vec4 model_pos = vec4(pos.x, pos.y, pos.z, 1.0f);
    // vec4 world_pos = view * model_pos;
    // gl_Position = perspective * world_pos;
    gl_Position = vec4(pos.x, pos.y, pos.z, 1.0f);
}
"#;

static FRAGMENT_SHADER: &'static str = r#"
#version 330

in vec2 Texcoord;

out vec4 color;

uniform sampler2D tex;

void main() {
    color = texture(tex, Texcoord);
}

"#;

fn perspective_matrix(target: &Frame, fov: f32) -> [[f32; 4]; 4] {
    let (w, h) = target.get_dimensions();
    let aspect = w as f32 / h as f32;
    let znear = 0.125;
    let zfar = 1024.0;
    let f = 1.0 / (fov / 2.0).tan();

    [[f / aspect, 0.0,                                    0.0,  0.0],
     [       0.0,   f,                                    0.0,  0.0],
     [       0.0, 0.0,        (zfar + znear) / (zfar - znear), -1.0],
     [       0.0, 0.0, -(2.0 * zfar * znear) / (zfar - znear),  0.0]]
}

fn main() {
    env_logger::init().unwrap();
    info!("Richter v0.0.1");

    let mdl = match Mdl::open("armor.mdl") {
        Err(why) => {
            println!("MDL load failed: {}", why);
            exit(1);
        }

        Ok(m) => m,
    };

    use glium::DisplayBuild;

    let window = match glium::glutin::WindowBuilder::new()
                     .with_dimensions(1024, 768)
                     .with_title(format!("Richter"))
                     .build_glium() {
        Ok(w) => w,
        Err(why) => {
            use std::error::Error;
            let mut error: Option<&Error> = Some(&why as &Error);
            while let Some(e) = error {
                println!("{}", e);
                error = e.cause();
            }
            exit(0);
        }
    };

    let gl_mdl = gl::GlMdl::from_mdl(&window, &mdl);

    let program = match Program::new(&window,
        ProgramCreationInput::SourceCode {
            vertex_shader: VERTEX_SHADER,
            tessellation_control_shader: None,
            tessellation_evaluation_shader: None,
            geometry_shader: None,
            fragment_shader: FRAGMENT_SHADER,
            outputs_srgb: false,
            uses_point_size: false,
            transform_feedback_varyings: None,
        }) {
        Err(why) => {
            println!("Error while compiling shader program: {}", why);
            exit(1);
        }
        Ok(p) => p,
    };

    'outer: loop {
        let mut target = window.draw();
        let perspective = perspective_matrix(&target, 2.0 * (PI / 3.0));

        let uniforms = uniform! {
            // perspective: perspective,
            // view: IDENTITY_MATRIX,
            // model: IDENTITY_MATRIX,
            tex: match gl_mdl.skins[0] {
                gl::GlMdlSkin::Single(ref s) => &s.texture,
                _ => panic!("asdf"),
            },
        };

        target.clear_color(0.0, 0.0, 0.0, 1.0);
        target.clear_depth(1.0);

        let vertex_buffer = glium::VertexBuffer::new(&window, &[
            Vertex { pos: [-0.5,  0.5, 0.0], texcoord: [0.0,  0.0] }, // tl
            Vertex { pos: [-0.5, -0.5, 0.0], texcoord: [0.0,  1.0] }, // bl
            Vertex { pos: [ 0.5, -0.5, 0.0], texcoord: [1.0,  1.0] }, // br
            Vertex { pos: [-0.5,  0.5, 0.0], texcoord: [0.0,  0.0] }, // tl
            Vertex { pos: [ 0.5,  0.5, 0.0], texcoord: [1.0,  0.0] }, // tr
            Vertex { pos: [ 0.5, -0.5, 0.0], texcoord: [1.0,  1.0] }, // br
        ]).unwrap();

        target.draw(&vertex_buffer, &NoIndices(glium::index::PrimitiveType::TrianglesList), &program, &uniforms, &DrawParameters::default());

        target.finish();

        for event in window.poll_events() {
            match event {
                Event::Closed => {
                    break 'outer;
                }
                _ => (),
            }
        }
    }
}
