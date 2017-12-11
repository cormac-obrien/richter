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

use math::Hyperplane;
use progs::EntityId;

use cgmath::Vector3;
use cgmath::Zero;

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

#[derive(Copy, Clone, Debug, Eq, FromPrimitive, PartialEq)]
pub enum CollideKind {
    Normal = 0,
    NoMonsters = 1,
    Missile = 2,
}

pub struct Collide {
    // the ID of the entity being moved
    pub e_id: Option<EntityId>,

    // the minimum extent of the entire move
    pub move_min: Vector3<f32>,

    // the maximum extent of the entire move
    pub move_max: Vector3<f32>,

    // the minimum extent of the moving object
    pub min: Vector3<f32>,

    // the maximum extent of the moving object
    pub max: Vector3<f32>,

    // the minimum extent of the moving object when colliding with a monster
    pub monster_min: Vector3<f32>,

    // the maximum extent of the moving object when colliding with a monster
    pub monster_max: Vector3<f32>,

    // the start point of the move
    pub start: Vector3<f32>,

    // the end point of the move
    pub end: Vector3<f32>,

    // how this move collides with other entities
    pub kind: CollideKind,
}

#[derive(Debug)]
/// Represents an attempted move by an entity.
pub struct Trace {
    // entity never left a solid area
    pub all_solid: bool,

    // entity started in a solid area
    pub start_solid: bool,
    pub in_open: bool,
    pub in_water: bool,

    /// How much of the intended move was completed before collision. A value of 1.0 indicates no
    /// collision (i.e. the full move was completed).
    pub ratio: f32,

    pub end_pos: Vector3<f32>,

    /// If the entity collided with a solid surface, this is the surface normal at the impact point.
    pub plane: Hyperplane,

    /// If the entity collided with another solid entity, this is the ID of the other entity.
    pub entity_id: Option<EntityId>,
}

impl Trace {
    pub fn new() -> Trace {
        Trace {
            all_solid: false,
            start_solid: false,
            in_open: false,
            in_water: false,
            ratio: 0.0,
            end_pos: Vector3::zero(),
            plane: Hyperplane::axis_x(0.0),
            entity_id: None,
        }
    }

    pub fn allsolid() -> Trace {
        Trace {
            all_solid: true,
            start_solid: false,
            in_open: false,
            in_water: false,
            ratio: 0.0,
            end_pos: Vector3::zero(),
            plane: Hyperplane::axis_x(0.0),
            entity_id: None,
        }
    }
}

pub fn bounds_for_move(
    start: Vector3<f32>,
    min: Vector3<f32>,
    max: Vector3<f32>,
    end: Vector3<f32>,
) -> (Vector3<f32>, Vector3<f32>) {
    let mut box_min = Vector3::zero();
    let mut box_max = Vector3::zero();

    for i in 0..3 {
        if end[i] > start[i] {
            box_min[i] = start[i] + min[i] - 1.0;
            box_max[i] = end[i] + max[i] + 1.0;
        } else {
            box_min[i] = end[i] + min[i] - 1.0;
            box_max[i] = start[i] + max[i] + 1.0;
        }
    }

    (box_min, box_max)
}
