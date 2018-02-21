// Copyright © 2018 Cormac O'Brien
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

extern crate cgmath;
extern crate chrono;
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
use richter::client::render::bsp::BSP_FRAGMENT_SHADER_GLSL;
use richter::client::render::bsp::BSP_VERTEX_SHADER_GLSL;
use richter::client::render::Palette;
use richter::client::render::Vertex;
use richter::client::render::bsp::BspRenderer;
use richter::client::render::bsp::BspRenderFace;
use richter::common::model::Model;
use richter::common::model::ModelKind;

type ColorFormat = gfx::format::Srgba8;
type DepthFormat = gfx::format::DepthStencil;

fn main() {
    env_logger::init();

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
        .create_shader_set(BSP_VERTEX_SHADER_GLSL, BSP_FRAGMENT_SHADER_GLSL)
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
            richter::client::render::bsp::pipe::new(),
        )
        .unwrap();

    let mut pak = richter::common::pak::Pak::new();
    pak.add("pak0.pak").unwrap();
    let (mut brush_models, _) =
        richter::common::bsp::load(pak.open("maps/e1m1.bsp").unwrap()).unwrap();

    let worldmodel = match brush_models.remove(0) {
        Model {
            kind: ModelKind::Brush(bmodel),
            ..
        } => bmodel,
        _ => unreachable!(),
    };

    let palette = Palette::load(&pak, "gfx/palette.lmp");

    let bsp_renderer = BspRenderer::new(worldmodel.bsp_data(), &palette, &mut factory);

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

    let mut data = richter::client::render::bsp::pipe::Data {
        vertex_buffer: bsp_renderer.vertex_buffer(),
        transform: perspective.into(),
        sampler: (bsp_renderer.get_texture_view(0), sampler),
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

        events_loop.poll_events(|event| {
            if let glutin::Event::WindowEvent { event, .. } = event {
                match event {
                    glutin::WindowEvent::Closed => exit = true,
                    glutin::WindowEvent::KeyboardInput { input, .. } => {
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
            }
        });

        if exit {
            break;
        }

        // turn rate of Pi radians per second
        let turn_rate =
            Rad(::std::f32::consts::PI) * frame_duration.num_milliseconds() as f32 / 1000.0;

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
        bsp_renderer.render(
            &mut encoder,
            &pso,
            &mut data,
            frame_time,
            perspective,
            camera_pos,
            camera_angles,
        );
        encoder.flush(&mut device);
        window.swap_buffers().unwrap();
        device.cleanup();

        prev_frame_time = frame_time;
    }
}
