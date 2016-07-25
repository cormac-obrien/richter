extern crate byteorder;
#[macro_use] extern crate glium;
#[macro_use] extern crate lazy_static;
#[macro_use] extern crate log;
extern crate env_logger;
extern crate regex;

pub mod bsp;
pub mod engine;
pub mod gfx;
pub mod math;
pub mod mdl;
pub mod pak;

use std::process::exit;
use gfx::{TexCoord, Vertex};
use glium::{Frame, Surface};
use glium::draw_parameters::DrawParameters;
use glium::glutin::Event;
use glium::program::{Program, ProgramCreationInput};

static VERTEX_SHADER: &'static str = r#"
#version 330

layout(location = 0) in vec3 pos;
layout(location = 1) in vec2 texcoord;

out vec2 Texcoord;

uniform mat4 perspective;
uniform mat4 view;
uniform mat4 world;

void main() {
    Texcoord = texcoord;
    // gl_Position = perspective * view * world * vec4(pos, 1.0f);
    gl_Position = perspective * view * world * vec4(pos, 1.0f);
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

    [[f / aspect, 0.0,                                   0.0,  0.0],
     [       0.0,   f,                                   0.0,  0.0],
     [       0.0, 0.0,       (zfar + znear) / (zfar - znear), -1.0],
     [       0.0, 0.0, (2.0 * zfar * znear) / (zfar - znear),  0.0]]
}

fn main() {
    let draw_parameters: glium::draw_parameters::DrawParameters<'static> = DrawParameters {
        depth: glium::Depth {
            test: glium::DepthTest::IfMoreOrEqual,
            write: true,
            .. Default::default()
        },
        backface_culling: glium::BackfaceCullingMode::CullCounterClockwise,
        .. Default::default()
    };

    env_logger::init().unwrap();
    info!("Richter v0.0.1");

    use glium::DisplayBuild;

    let display = match glium::glutin::WindowBuilder::new()
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

    let mdl = mdl::Mdl::load(&display, "armor.mdl").unwrap();
    let mut bspfile = std::fs::File::open("pak0/maps/e1m1.bsp").unwrap();
    let bsp = bsp::Bsp::load(&display, &mut bspfile);

    let program = match Program::new(&display,
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
        let mut target = display.draw();
        let perspective = perspective_matrix(&target, 2.0 * (math::PI / 3.0));

        let uniforms = uniform! {
            perspective: perspective,
            view: *math::Mat4::translation(0.0, 0.0, -50.0),
            world: *(math::Mat4::rotation_y(90.0f32.to_radians()) * math::Mat4::rotation_x(-90.0f32.to_radians())),
            tex: match mdl.skins[0] {
                mdl::Skin::Single(ref s) => s.texture.sampled()
                                                          .magnify_filter(glium::uniforms::MagnifySamplerFilter::Nearest)
                                                          .minify_filter(glium::uniforms::MinifySamplerFilter::LinearMipmapLinear)
                                                          .wrap_function(glium::uniforms::SamplerWrapFunction::Clamp),
                _ => panic!("asdf"),
            },
        };

        target.clear_color(0.0, 0.0, 0.0, 1.0);
        target.clear_depth(0.0);

        let vertices = match mdl.frames[0] {
            mdl::Frame::Single(ref s) => &s.vertices,
            _ => panic!("asdf")
        };

        let draw_status = target.draw(
            (vertices, &mdl.texcoords),
            &mdl.indices,
            &program,
            &uniforms,
            &draw_parameters);

        if draw_status.is_err() {
            error!("Draw failed: {}", draw_status.err().unwrap());
            exit(1);
        }

        let finish_status = target.finish();
        if finish_status.is_err() {
            error!("Frame finish failed: {}", finish_status.err().unwrap());
            exit(1);
        }

        for event in display.poll_events() {
            match event {
                Event::Closed => {
                    debug!("Caught Event::Closed, exiting.");
                    break 'outer;
                }
                _ => (),
            }
        }
    }
}
