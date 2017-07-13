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

use glium;
use glium::uniforms::{MinifySamplerFilter, MagnifySamplerFilter, SamplerWrapFunction};
use glium::program::ProgramCreationInput;
use glium::draw_parameters::DrawParameters;
use math::Vec3;

const MAGNIFY_FILTER: MagnifySamplerFilter = MagnifySamplerFilter::Nearest;
const MINIFY_FILTER: MinifySamplerFilter = MinifySamplerFilter::NearestMipmapNearest;
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

pub static BSP_VERTEX_SHADER: &'static str = r#"
#version 330

in vec3 position;
in vec2 texcoord;
out vec2 Texcoord;

uniform mat4 perspective;
uniform mat4 view;
uniform mat4 world;

void main() {
    Texcoord = texcoord;
    gl_Position = perspective * view * world * vec4(position, 1.0f);
}
"#;

pub static BSP_FRAGMENT_SHADER: &'static str = r#"
#version 330

in vec2 Texcoord;

out vec4 color;

uniform sampler2D tex;

void main() {
    color = texture(tex, Texcoord);
}

"#;

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

pub fn get_bsp_shader_source<'a>() -> ProgramCreationInput<'a> {
    ProgramCreationInput::SourceCode {
        vertex_shader: &BSP_VERTEX_SHADER,
        tessellation_control_shader: None,
        tessellation_evaluation_shader: None,
        geometry_shader: None,
        fragment_shader: &BSP_FRAGMENT_SHADER,
        outputs_srgb: false,
        uses_point_size: false,
        transform_feedback_varyings: None,
    }
}

pub fn get_draw_parameters<'a>() -> DrawParameters<'static> {
    DrawParameters {
        depth: glium::Depth {
            test: glium::DepthTest::IfMoreOrEqual,
            write: true,
            ..Default::default()
        },
        backface_culling: glium::BackfaceCullingMode::CullCounterClockwise,
        // backface_culling: glium::BackfaceCullingMode::CullClockwise,
        ..Default::default()
    }
}
