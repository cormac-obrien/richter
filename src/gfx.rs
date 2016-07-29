// Copyright © 2016 Cormac O'Brien
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

use std::fmt;
use std::ops;

use glium;
use glium::uniforms::{MinifySamplerFilter, MagnifySamplerFilter, SamplerWrapFunction};
use glium::program::ProgramCreationInput;

const MAGNIFY_FILTER: MagnifySamplerFilter = MagnifySamplerFilter::Nearest;
const MINIFY_FILTER: MinifySamplerFilter = MinifySamplerFilter::Nearest;
const WRAP_FUNCTION: SamplerWrapFunction = SamplerWrapFunction::Repeat;

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


#[derive(Copy, Clone)]
pub struct Vertex {
    pub pos: [f32; 3],
}
implement_vertex!(Vertex, pos);

impl ops::Index<usize> for Vertex {
    type Output = f32;

    fn index(&self, i: usize) -> &f32 {
        &self.pos[i]
    }
}

impl fmt::Display for Vertex {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{{x: {}, y: {}, z: {}}}", self[0], self[1], self[2])
    }
}

#[derive(Copy, Clone)]
pub struct TexCoord {
    pub texcoord: [f32; 2],
}
implement_vertex!(TexCoord, texcoord);

impl ops::Index<usize> for TexCoord {
    type Output = f32;

    fn index(&self, i: usize) -> &f32 {
        &self.texcoord[i]
    }
}

impl fmt::Display for TexCoord {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{{s: {}, t: {}}}", self[0], self[1])
    }
}

pub trait Draw {
    fn get_vertices(&self) -> &glium::VertexBuffer<Vertex>;
    fn get_indices(&self) -> &glium::IndexBuffer<u32>;
    fn get_texcoords(&self) -> &glium::VertexBuffer<TexCoord>;
    fn get_texture(&self) -> &glium::Texture2d;
}

pub fn sample(texture: &glium::Texture2d) -> glium::uniforms::Sampler<glium::Texture2d> {
    texture.sampled()
           .magnify_filter(MAGNIFY_FILTER)
           .minify_filter(MINIFY_FILTER)
           .wrap_function(WRAP_FUNCTION)
}

// TODO: Can this be made a compile-time constant?
pub fn get_shader_source<'a>() -> ProgramCreationInput<'a> {
    ProgramCreationInput::SourceCode {
        vertex_shader: &VERTEX_SHADER,
        tessellation_control_shader: None,
        tessellation_evaluation_shader: None,
        geometry_shader: None,
        fragment_shader: &FRAGMENT_SHADER,
        outputs_srgb: false,
        uses_point_size: false,
        transform_feedback_varyings: None,
    }
}
