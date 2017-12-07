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

use bsp::BspModel;
use sprite::SpriteModel;

use cgmath::Vector3;

pub struct Model {
    name: String,
    kind: ModelKind,
}

pub enum ModelKind {
    // TODO: find a more elegant way to express the null model
    None,
    Brush(BspModel),
    Alias,
    Sprite(SpriteModel),
}

impl Model {
    pub fn none() -> Model {
        Model {
            name: String::new(),
            kind: ModelKind::None,
        }
    }

    /// Construct a new generic model from a brush model.
    pub fn from_brush_model(name: String, brush_model: BspModel) -> Model {
        Model {
            name,
            kind: ModelKind::Brush(brush_model),
        }
    }

    pub fn from_sprite_model(name: String, sprite_model: SpriteModel) -> Model {
        Model {
            name,
            kind: ModelKind::Sprite(sprite_model),
        }
    }

    /// Return the name of this model.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Return the minimum extent of this model.
    pub fn min(&self) -> Vector3<f32> {
        match self.kind {
            ModelKind::None => panic!("attempted to take min() of NULL model"),
            ModelKind::Brush(ref bmodel) => bmodel.min(),
            ModelKind::Sprite(ref smodel) => smodel.min(),
            _ => unimplemented!(),
        }
    }

    /// Return the maximum extent of this model.
    pub fn max(&self) -> Vector3<f32> {
        match self.kind {
            ModelKind::None => panic!("attempted to take max() of NULL model"),
            ModelKind::Brush(ref bmodel) => bmodel.max(),
            ModelKind::Sprite(ref smodel) => smodel.max(),
            _ => unimplemented!(),
        }
    }
}
