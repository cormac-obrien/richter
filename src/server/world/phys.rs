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

use common::bsp::BspLeafContents;
use common::math::Hyperplane;
use server::progs::EntityId;

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

#[derive(Debug)]
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
pub struct TraceStart {
    point: Vector3<f32>,
    ratio: f32,
}

impl TraceStart {
    pub fn new(point: Vector3<f32>, ratio: f32) -> TraceStart {
        TraceStart { point, ratio }
    }
}

#[derive(Debug)]
pub struct TraceEndBoundary {
    ratio: f32,
    plane: Hyperplane,
}

#[derive(Debug)]
pub enum TraceEndKind {
    /// This endpoint falls within a leaf.
    Terminal,

    /// This endpoint falls on a leaf boundary (a plane).
    Boundary(TraceEndBoundary),
}

#[derive(Debug)]
pub struct TraceEnd {
    point: Vector3<f32>,
    kind: TraceEndKind,
}

impl TraceEnd {
    pub fn terminal(point: Vector3<f32>) -> TraceEnd {
        TraceEnd {
            point,
            kind: TraceEndKind::Terminal,
        }
    }

    pub fn boundary(point: Vector3<f32>, ratio: f32, plane: Hyperplane) -> TraceEnd {
        TraceEnd {
            point,
            kind: TraceEndKind::Boundary(TraceEndBoundary { ratio, plane }),
        }
    }
}

#[derive(Debug)]
pub struct Trace {
    start: TraceStart,
    end: TraceEnd,
    contents: BspLeafContents,
    start_solid: bool,
}

impl Trace {
    pub fn new(start: TraceStart, end: TraceEnd, contents: BspLeafContents) -> Trace {
        let start_solid = contents == BspLeafContents::Solid;
        Trace {
            start,
            end,
            contents,
            start_solid,
        }
    }

    /// Join this trace end-to-end with another.
    ///
    /// - If `self.end_point()` does not equal `other.start_point()`, returns `self`.
    /// - If `self.contents` equals `other.contents`, the traces are combined (e.g. the new trace
    ///   starts with `self.start` and ends with `other.end`).
    /// - If `self.contents` is `Solid` but `other.contents` is not, the trace is allowed to move
    ///   out of the solid area. The `startsolid` flag should be set accordingly.
    /// - Otherwise, `self` is returned, representing a collision or transition between leaf types.
    ///
    /// ## Panics
    /// - If `self.end.kind` is `Terminal`.
    /// - If `self.end.point` does not equal `other.start.point`.
    pub fn join(self, other: Trace) -> Trace {
        debug!(
            "start1={:?} end1={:?} start2={:?} end2={:?}",
            self.start.point,
            self.end.point,
            other.start.point,
            other.end.point
        );
        // don't allow chaining after terminal
        // TODO: impose this constraint with the type system
        if let TraceEndKind::Terminal = self.end.kind {
            panic!("Attempted to join after terminal trace");
        }

        // don't allow joining disjoint traces
        if self.end.point != other.start.point {
            panic!("Attempted to join disjoint traces");
        }

        // combine traces with the same contents
        if self.contents == other.contents {
            return Trace {
                start: self.start,
                end: other.end,
                contents: self.contents,
                start_solid: self.start_solid,
            };
        }

        if self.contents == BspLeafContents::Solid && other.contents != BspLeafContents::Solid {
            return Trace {
                start: self.start,
                end: other.end,
                contents: other.contents,
                start_solid: true,
            };
        }

        self
    }

    pub fn adjust(self, offset: Vector3<f32>) -> Trace {
        Trace {
            start: TraceStart {
                point: self.start.point + offset,
                ratio: self.start.ratio,
            },
            end: TraceEnd {
                point: self.end.point + offset,
                kind: self.end.kind,
            },
            contents: self.contents,
            start_solid: self.start_solid,
        }
    }

    pub fn start_point(&self) -> Vector3<f32> {
        self.start.point
    }

    pub fn end_point(&self) -> Vector3<f32> {
        self.end.point
    }

    pub fn all_solid(&self) -> bool {
        self.contents == BspLeafContents::Solid
    }

    pub fn start_solid(&self) -> bool {
        self.start_solid
    }

    pub fn in_open(&self) -> bool {
        self.contents == BspLeafContents::Empty
    }

    pub fn in_water(&self) -> bool {
        self.contents != BspLeafContents::Empty && self.contents != BspLeafContents::Solid
    }

    pub fn is_terminal(&self) -> bool {
        if let TraceEndKind::Terminal = self.end.kind {
            true
        } else {
            false
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
