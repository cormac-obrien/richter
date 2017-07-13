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

pub mod gl;
pub mod vk;

use math::Vec3;
use std::convert::From;
use std::fmt;
use std::ops;

#[derive(Copy, Clone)]
pub struct Vertex {
    pub pos: [f32; 3],
}
implement_vertex!(Vertex, pos);

impl Vertex {
    pub fn new(pos: [f32; 3]) -> Vertex {
        Vertex { pos: pos }
    }
}

impl<'a> From<&'a Vec3> for Vertex {
    fn from(__arg_0: &'a Vec3) -> Vertex {
        Vertex { pos: [__arg_0[0], __arg_0[1], __arg_0[2]] }
    }
}

impl From<Vec3> for Vertex {
    fn from(__arg_0: Vec3) -> Vertex {
        Vertex::from(&__arg_0)
    }
}

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
