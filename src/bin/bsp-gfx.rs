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

extern crate cgmath;
#[macro_use]
extern crate gfx;
extern crate gfx_device_gl;
extern crate gfx_window_glutin;
extern crate glutin;
extern crate richter;

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

    let pso = factory
        .create_pipeline_simple(
            r#"
#version 430

layout (location = 0) in vec3 v_position;
layout (location = 1) in vec2 v_texcoord;

out vec2 f_texcoord;

uniform mat4 u_transform;

void main() {
    f_texcoord = v_texcoord;
    gl_Position = u_transform * vec4(v_position, 1.0);
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
            pipe::new(),
        )
        .unwrap();

    let bsp = richter::bsp::Bsp::load("pak0.pak.d/maps/e1m1.bsp").unwrap();

    let textures = bsp.gen_textures(|tex| {
        // TODO: get all mipmaps
        let mipmap_0 = richter::engine::indexed_to_rgba(&tex.mipmaps[0]);

        let (_, view) = factory
            .create_texture_immutable_u8::<ColorFormat>(
                gfx::texture::Kind::D2(
                    tex.width as u16,
                    tex.height as u16,
                    gfx::texture::AaMode::Single,
                ),
                &[&mipmap_0],
            )
            .unwrap();

        view
    });

    let (face_data, vertex_data): (Vec<Face>, Vec<Vertex>) = bsp.gen_render_data_interleaved();
    let vertex_buffer = factory.create_vertex_buffer(&vertex_data);
    let slice = gfx::Slice::new_match_vertex_buffer(&vertex_buffer);

    let sampler = factory.create_sampler(gfx::texture::SamplerInfo::new(
        gfx::texture::FilterMethod::Scale,
        gfx::texture::WrapMode::Tile,
    ));

    let (fb_width, fb_height) = window.window().get_inner_size_pixels().unwrap();

    let mut data = pipe::Data {
        vertex_buffer: vertex_buffer,
        transform: cgmath::perspective(
            cgmath::Deg(75.0),
            fb_width as f32 / fb_height as f32,
            1.0,
            1024.0,
        ).into(),
        sampler: (textures[0].clone(), sampler),
        out_color: color,
        out_depth: depth,
    };

    let mut exit = false;
    loop {
        events_loop.poll_events(|event| if let glutin::Event::WindowEvent {
            event, ..
        } = event
        {
            match event {
                glutin::WindowEvent::Closed => exit = true,
                _ => (),
            }
        });

        if exit {
            break;
        }

        encoder.clear(&data.out_color, [0.0, 0.0, 0.0, 1.0]);
        for f in face_data.iter() {
            let slice = gfx::Slice {
                start: 0,
                end: f.vertex_count as u32,
                base_vertex: f.vertex_id as u32,
                instances: None,
                buffer: gfx::IndexBuffer::Auto,
            };

            data.sampler.0 = textures[f.tex_id].clone();
            encoder.draw(&slice, &pso, &data);
        }
        encoder.flush(&mut device);
        window.swap_buffers().unwrap();
        device.cleanup();
    }
}
