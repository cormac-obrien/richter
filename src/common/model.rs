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

use common::bsp::BspModel;
use common::mdl;
use common::mdl::AliasModel;
use common::pak::Pak;
use common::sprite;
use common::sprite::SpriteModel;

use cgmath::Vector3;

#[derive(FromPrimitive)]
pub enum SyncType {
    Sync = 0,
    Rand = 1,
}

pub struct Model {
    name: String,
    kind: ModelKind,
}

pub enum ModelKind {
    // TODO: find a more elegant way to express the null model
    None,
    Brush(BspModel),
    Alias(AliasModel),
    Sprite(SpriteModel),
}

impl Model {
    pub fn none() -> Model {
        Model {
            name: String::new(),
            kind: ModelKind::None,
        }
    }

    pub fn kind(&self) -> &ModelKind {
        &self.kind
    }

    pub fn load<S>(pak: &Pak, name: S) -> Model
    where
        S: AsRef<str>,
    {
        let name = name.as_ref();
        // TODO: original engine uses the magic numbers of each format instead of the extension.
        if name.ends_with(".bsp") {
            panic!("BSP files may contain multiple models, use bsp::load for this");
        } else if name.ends_with(".mdl") {
            match pak.open(name) {
                Some(m) => Model::from_alias_model(name.to_owned(), mdl::load(m).unwrap()),
                None => panic!("No such file: {}", name),
            }
        } else if name.ends_with(".spr") {
            match pak.open(name) {
                Some(m) => Model::from_sprite_model(name.to_owned(), sprite::load(m)),
                None => panic!("No such file: {}", name),
            }
        } else {
            panic!("Unrecognized model type: {}", name);
        }
    }

    /// Construct a new generic model from a brush model.
    pub fn from_brush_model<S>(name: S, brush_model: BspModel) -> Model
    where
        S: AsRef<str>,
    {
        Model {
            name: name.as_ref().to_owned(),
            kind: ModelKind::Brush(brush_model),
        }
    }

    /// Construct a new generic model from an alias model.
    pub fn from_alias_model<S>(name: S, alias_model: AliasModel) -> Model
    where
        S: AsRef<str>,
    {
        Model {
            name: name.as_ref().to_owned(),
            kind: ModelKind::Alias(alias_model),
        }
    }

    /// Construct a new generic model from a sprite model.
    pub fn from_sprite_model<S>(name: S, sprite_model: SpriteModel) -> Model
    where
        S: AsRef<str>,
    {
        Model {
            name: name.as_ref().to_owned(),
            kind: ModelKind::Sprite(sprite_model),
        }
    }

    /// Return the name of this model.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Return the minimum extent of this model.
    pub fn min(&self) -> Vector3<f32> {
        debug!("Retrieving min of model {}", self.name);
        match self.kind {
            ModelKind::None => panic!("attempted to take min() of NULL model"),
            ModelKind::Brush(ref bmodel) => bmodel.min(),
            ModelKind::Sprite(ref smodel) => smodel.min(),

            // TODO: maybe change this?
            // https://github.com/id-Software/Quake/blob/master/WinQuake/gl_model.c#L1625
            ModelKind::Alias(_) => Vector3::new(-16.0, -16.0, -16.0),
        }
    }

    /// Return the maximum extent of this model.
    pub fn max(&self) -> Vector3<f32> {
        debug!("Retrieving max of model {}", self.name);
        match self.kind {
            ModelKind::None => panic!("attempted to take max() of NULL model"),
            ModelKind::Brush(ref bmodel) => bmodel.max(),
            ModelKind::Sprite(ref smodel) => smodel.max(),

            // TODO: maybe change this?
            // https://github.com/id-Software/Quake/blob/master/WinQuake/gl_model.c#L1625
            ModelKind::Alias(_) => Vector3::new(16.0, 16.0, 16.0),
        }
    }

    pub fn sync_type(&self) -> SyncType {
        match self.kind {
            ModelKind::None => panic!("Attempted to take sync_type() of NULL model"),
            ModelKind::Brush(_) => SyncType::Sync,
            // TODO: expose sync_type in Sprite and reflect it here
            ModelKind::Sprite(ref smodel) => SyncType::Sync,
            // TODO: expose sync_type in Mdl and reflect it here
            ModelKind::Alias(ref amodel) => SyncType::Sync,
        }
    }
}
