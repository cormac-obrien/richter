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

use std::sync::{Once, ONCE_INIT};

use glium::{DrawParameters, IndexBuffer, Frame, Program, Surface, Texture2d, VertexBuffer};
use glium::backend::Facade;
use glium::program::ProgramCreationInput;
use glium::uniforms::Sampler;

static FULLSCREEN_TEXTURE_VERTICES: [f32; 16] = [
    -1.0,  1.0, 0.0, 1.0,  // Top left
     1.0,  1.0, 0.0, 1.0,  // Top right
    -1.0, -1.0, 0.0, 1.0,  // Bottom left
     1.0, -1.0, 0.0, 1.0]; // Bottom right

static FULLSCREEN_TEXTURE_INDICES: [u32; 6] = [0, 2, 3, 0, 1, 3];

static FULLSCREEN_TEXTURE_COORDINATES: [f32; 8] = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0];

static FULLSCREEN_TEXTURE_VERTEX_SHADER: &'static str = r#"
#version 330

layout(location = 0) in vec4 pos;
layout(location = 1) in vec2 texcoord;

out vec2 Texcoord;

void main() {
    Texcoord = texcoord;
    gl_Position = pos;
}

"#;

static FULLSCREEN_TEXTURE_FRAGMENT_SHADER: &'static str = r#"
#version 330

in vec2 Texcoord;

out vec4 color;

uniform sampler2D tex;

void main() {
    color = texture(tex, Texcoord);
}
"#;

static INIT: Once = ONCE_INIT;

fn fullscreen_texture_program<F>(facade: &F) -> Program where F: Facade {
    let program = match Program::new(facade, ProgramCreationInput::SourceCode {
        vertex_shader: FULLSCREEN_TEXTURE_VERTEX_SHADER,
        tessellation_control_shader: None,
        tessellation_evaluation_shader: None,
        geometry_shader: None,
        fragment_shader: FULLSCREEN_TEXTURE_FRAGMENT_SHADER,
        outputs_srgb: false,
        uses_point_size: false,
        transform_feedback_varyings: None,
    }) {
        Err(why) => panic!("{}", why),
        Ok(p) => p,
    };
}

fn fullscreen_texture(target: &Frame, program: &Program, sampler: &Sampler<Texture2d>) {
    INIT.call_once(|| {
        DEFAULT_DRAW_PARAMS: = DrawParameters::default();

    });
    static FULLSCREEN_TEXTURE_VERTEX_BUFFER: VertexBuffer<f32> = VertexBuffer::new(FULLSCREEN_TEXTURE_INDICES);
    static FULLSCREEN_TEXTURE_INDEX_BUFFER: IndexBuffer<u32> = IndexBuffer::new(FULLSCREEN_TEXTURE_INDICES);

    let uniforms = uniform! {
        tex: *sampler,
    };

    target.draw(&FULLSCREEN_TEXTURE_VERTEX_BUFFER,
                &FULLSCREEN_TEXTURE_INDEX_BUFFER,
                &program,
                &uniforms,
                &DEFAULT_DRAW_PARAMS);
}
