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

use bsp::BspPlane;
use progs::EntityId;

use cgmath::Vector3;

#[derive(Copy, Clone, Debug, Eq, FromPrimitive, PartialEq)]
pub enum MoveKind {
    None = 0,
    AngleNoClip = 1,
    AngleClip = 2,
    Walk = 3,
    Step = 4,
    Fly = 5,
    Toss = 6,
    Push = 7,
    NoClip = 8,
    FlyMissile = 9,
    Bounce = 10,
}

#[derive(Copy, Clone, Debug, FromPrimitive)]
pub enum CollideKind {
    Normal = 0,
    NoMonsters = 1,
    Missile = 2,
}

pub struct Collide {
    box_min: Vector3<f32>,
    box_max: Vector3<f32>,
    min: Vector3<f32>,
    max: Vector3<f32>,
    monster_min: Vector3<f32>,
    monster_max: Vector3<f32>,
    start: f32,
    end: f32,
}

/// Represents a single frame of movement by an entity.
pub struct Trace {
    // entity never left a solid area
    all_solid: bool,

    // entity started in a solid area
    start_solid: bool,
    in_open: bool,
    in_water: bool,

    // how much of the intended move was completed before collision
    // a value of 1.0 indicates no collision
    ratio: f32,

    // surface normal at the impact point
    plane: BspPlane,

    // entity the impact surface belongs to
    entity_id: EntityId,
}
