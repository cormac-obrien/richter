// Copyright © 2017 Cormac O'Brien
//
// Permission is hereby granted, free of charge, to any person obtaining a copy of this software
// and associated documentation files (the "Software"), to deal in the Software without
// restriction, including without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all copies or
// substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING
// BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
// NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
// DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

extern crate chrono;
extern crate cgmath;
extern crate env_logger;
#[macro_use]
extern crate gfx;
extern crate gfx_device_gl;
extern crate gfx_window_glutin;
extern crate glutin;
extern crate richter;

use cgmath::Angle;
use cgmath::Deg;
use cgmath::Euler;
use cgmath::Matrix3;
use cgmath::Matrix4;
use cgmath::Rad;
use cgmath::Vector3;
use chrono::Utc;
use gfx::Device;
use gfx::Factory;
use gfx::traits::FactoryExt;
use glutin::GlContext;
use glutin::GlRequest;
use glutin::Api::OpenGl;

type ColorFormat = gfx::format::Srgba8;
type DepthFormat = gfx::format::DepthStencil;

gfx_defines!{
    vertex Vertex {
        position: [f32; 3] = "v_position",
        texcoord: [f32; 2] = "v_texcoord",
    }

    constant Locals {
        transform: [[f32; 4]; 4] = "u_transform",
    }

    pipeline pipe {
        vertex_buffer: gfx::VertexBuffer<Vertex> = (),
        transform: gfx::Global<[[f32; 4]; 4]> = "u_transform",
        sampler: gfx::TextureSampler<[f32; 4]> = "u_texture",
        out_color: gfx::RenderTarget<ColorFormat> = "Target0",
        out_depth: gfx::DepthTarget<DepthFormat> = gfx::preset::depth::LESS_EQUAL_WRITE,
    }
}

impl From<[f32; 5]> for Vertex {
    fn from(src: [f32; 5]) -> Self {
        Vertex {
            position: [src[0], src[1], src[2]],
            texcoord: [src[3], src[4]],
        }
    }
}

struct Face {
    vertex_id: usize,
    vertex_count: usize,
    tex_id: usize,
}

impl From<(usize, usize, usize)> for Face {
    fn from(src: (usize, usize, usize)) -> Self {
        Face {
            vertex_id: src.0,
            vertex_count: src.1,
            tex_id: src.2,
        }
    }
}

fn main() {
    env_logger::init().unwrap();

    let mut events_loop = glutin::EventsLoop::new();
    let window_builder = glutin::WindowBuilder::new().with_title("BSP renderer: gfx-rs backend");
    let context_builder = glutin::ContextBuilder::new()
        .with_gl(GlRequest::Specific(OpenGl, (3, 3)))
        .with_vsync(true);

    let (window, mut device, mut factory, color, depth) =
        gfx_window_glutin::init::<ColorFormat, DepthFormat>(
            window_builder,
            context_builder,
            &events_loop,
        );

    let mut encoder: gfx::Encoder<gfx_device_gl::Resources, gfx_device_gl::CommandBuffer> =
        factory.create_command_buffer().into();

    let shader_set = factory
        .create_shader_set(
            r#"
#version 430

layout (location = 0) in vec3 v_position;
layout (location = 1) in vec2 v_texcoord;

out vec2 f_texcoord;

uniform mat4 u_transform;

void main() {
    f_texcoord = v_texcoord;
    // gl_Position = u_transform * vec4(v_position.xyz, 1.0);
    gl_Position = u_transform * vec4(-v_position.y, v_position.z, -v_position.x, 1.0);
}
"#
                .as_bytes(),
            r#"
#version 430

in vec2 f_texcoord;

uniform sampler2D u_texture;

out vec4 Target0;

void main() {
    Target0 = texture(u_texture, f_texcoord);
}"#
                .as_bytes(),
        )
        .unwrap();

    let rasterizer = gfx::state::Rasterizer {
        front_face: gfx::state::FrontFace::Clockwise,
        cull_face: gfx::state::CullFace::Back,
        method: gfx::state::RasterMethod::Fill,
        offset: None,
        samples: Some(gfx::state::MultiSample),
    };

    let pso = factory
        .create_pipeline_state(
            &shader_set,
            gfx::Primitive::TriangleList,
            rasterizer,
            pipe::new(),
        )
        .unwrap();

    let mut pak = richter::pak::Pak::new();
    pak.add("pak0.pak").unwrap();
    let (worldmodel, _, _) = richter::bsp::load(pak.open("maps/e1m1.bsp").unwrap()).unwrap();

    let textures = worldmodel
        .bsp_data()
        .textures()
        .iter()
        .map(|tex| {
            let mipmap_full =
                richter::engine::indexed_to_rgba(tex.mipmap(richter::bsp::BspTextureMipmap::Full));
            let (width, height) = tex.dimensions();

            let (_, view) =
                factory
                    .create_texture_immutable_u8::<ColorFormat>(
                        gfx::texture::Kind::D2(
                            width as u16,
                            height as u16,
                            gfx::texture::AaMode::Single,
                        ),
                        &[&mipmap_full],
                    )
                    .unwrap();

            view
        })
        .collect::<Vec<_>>();

    let (face_data, vertex_data): (Vec<Face>, Vec<Vertex>) =
        worldmodel.bsp_data().gen_render_data_interleaved();
    let vertex_buffer = factory.create_vertex_buffer(&vertex_data);

    let sampler = factory.create_sampler(gfx::texture::SamplerInfo::new(
        gfx::texture::FilterMethod::Scale,
        gfx::texture::WrapMode::Tile,
    ));

    let (fb_width, fb_height) = window.window().get_inner_size_pixels().unwrap();

    let perspective = cgmath::perspective(
        cgmath::Rad::from(Deg(75.0)),
        fb_width as f32 / fb_height as f32,
        1.0,
        65536.0,
    );

    let mut data = pipe::Data {
        vertex_buffer: vertex_buffer,
        transform: perspective.into(),
        sampler: (textures[0].clone(), sampler),
        out_color: color,
        out_depth: depth,
    };

    println!("WASD to move");
    println!("Arrow keys to look");
    println!("Space to ascend, Left Control to descend");

    let mut move_forward = false;
    let mut move_back = false;
    let mut move_left = false;
    let mut move_right = false;
    let mut move_up = false;
    let mut move_down = false;

    let mut look_left = false;
    let mut look_right = false;
    let mut look_up = false;
    let mut look_down = false;

    let mut camera_pos = Vector3::new(0.0, 0.0, 0.0);
    let mut camera_angles = Euler::new(Rad(0.0), Rad(0.0), Rad(0.0));

    let start_time = Utc::now();
    let mut prev_frame_time = Utc::now().signed_duration_since(start_time);
    let mut exit = false;
    loop {
        let frame_time = Utc::now().signed_duration_since(start_time);
        let frame_duration = frame_time - prev_frame_time;

        events_loop.poll_events(|event| if let glutin::Event::WindowEvent {
            event, ..
        } = event
        {
            match event {
                glutin::WindowEvent::Closed => exit = true,
                glutin::WindowEvent::KeyboardInput { device_id, input } => {
                    let pressed = match input.state {
                        glutin::ElementState::Pressed => true,
                        glutin::ElementState::Released => false,
                    };

                    if let Some(key) = input.virtual_keycode {
                        match key {
                            glutin::VirtualKeyCode::W => move_forward = pressed,
                            glutin::VirtualKeyCode::A => move_left = pressed,
                            glutin::VirtualKeyCode::S => move_back = pressed,
                            glutin::VirtualKeyCode::D => move_right = pressed,
                            glutin::VirtualKeyCode::Space => move_up = pressed,
                            glutin::VirtualKeyCode::LControl => move_down = pressed,
                            glutin::VirtualKeyCode::Up => look_up = pressed,
                            glutin::VirtualKeyCode::Down => look_down = pressed,
                            glutin::VirtualKeyCode::Left => look_left = pressed,
                            glutin::VirtualKeyCode::Right => look_right = pressed,
                            _ => (),
                        }
                    }
                }
                _ => (),
            }
        });

        if exit {
            break;
        }

        // turn rate of Pi radians per second
        let turn_rate = Rad(::std::f32::consts::PI) * frame_duration.num_milliseconds() as f32 /
            1000.0;

        if look_up {
            camera_angles.x -= turn_rate;
            if camera_angles.x < Rad::from(Deg(-90.0)) {
                camera_angles.x = Rad::from(Deg(-90.0));
            }
        }

        if look_down {
            camera_angles.x += turn_rate;
            if camera_angles.x > Rad::from(Deg(90.0)) {
                camera_angles.x = Rad::from(Deg(90.0));
            }
        }

        if look_right {
            camera_angles.y += turn_rate;
        }

        if look_left {
            camera_angles.y -= turn_rate;
        }

        let rotation = Matrix3::from(camera_angles);

        let mut move_vector = Vector3::new(0.0, 0.0, 0.0);

        if move_forward {
            move_vector.x -= camera_angles.y.sin();
            move_vector.z += camera_angles.y.cos();
        }

        if move_back {
            move_vector.x += camera_angles.y.sin();
            move_vector.z -= camera_angles.y.cos();
        }

        if move_left {
            move_vector.x += camera_angles.y.cos();
            move_vector.z += camera_angles.y.sin();
        }

        if move_right {
            move_vector.x -= camera_angles.y.cos();
            move_vector.z -= camera_angles.y.sin();
        }

        if move_up {
            move_vector.y -= 1.0;
        }

        if move_down {
            move_vector.y += 1.0;
        }

        camera_pos += move_vector;

        encoder.clear(&data.out_color, [0.0, 0.0, 0.0, 1.0]);
        encoder.clear_depth(&data.out_depth, 1.0);
        for f in face_data.iter() {
            let slice = gfx::Slice {
                start: 0,
                end: f.vertex_count as u32,
                base_vertex: f.vertex_id as u32,
                instances: None,
                buffer: gfx::IndexBuffer::Auto,
            };

            let frame = worldmodel.bsp_data().texture_frame_for_time(
                f.tex_id,
                frame_time,
            );
            data.sampler.0 = textures[frame].clone();
            data.transform = (perspective * Matrix4::from(rotation) *
                                  Matrix4::from_translation(camera_pos)).into();
            encoder.draw(&slice, &pso, &data);
        }
        encoder.flush(&mut device);
        window.swap_buffers().unwrap();
        device.cleanup();

        prev_frame_time = frame_time;
    }
}
