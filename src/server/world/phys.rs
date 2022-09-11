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

//! Physics and collision detection.

use crate::{
    common::{bsp::BspLeafContents, math::Hyperplane},
    server::progs::EntityId,
};

use bitflags::bitflags;
use cgmath::{InnerSpace, Vector3, Zero};

/// Velocity in units/second under which a *component* (not the entire
/// velocity!) is instantly reduced to zero.
///
/// This prevents objects from sliding indefinitely at low velocity.
const STOP_THRESHOLD: f32 = 0.1;

#[derive(Copy, Clone, Debug, Eq, FromPrimitive, PartialEq)]
pub enum MoveKind {
    /// Does not move.
    None = 0,
    AngleNoClip = 1,
    AngleClip = 2,
    /// Player-controlled.
    Walk = 3,
    /// Moves in discrete steps (monsters).
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
    /// The ID of the entity being moved.
    pub e_id: Option<EntityId>,

    /// The minimum extent of the entire move.
    pub move_min: Vector3<f32>,

    /// The maximum extent of the entire move.
    pub move_max: Vector3<f32>,

    /// The minimum extent of the moving object.
    pub min: Vector3<f32>,

    /// The maximum extent of the moving object.
    pub max: Vector3<f32>,

    /// The minimum extent of the moving object when colliding with a monster.
    pub monster_min: Vector3<f32>,

    /// The maximum extent of the moving object when colliding with a monster.
    pub monster_max: Vector3<f32>,

    /// The start point of the move.
    pub start: Vector3<f32>,

    /// The end point of the move.
    pub end: Vector3<f32>,

    /// How this move collides with other entities.
    pub kind: CollideKind,
}

/// Calculates a new velocity after collision with a surface.
///
/// `overbounce` approximates the elasticity of the collision. A value of `1`
/// reduces the component of `initial` antiparallel to `surface_normal` to zero,
/// while a value of `2` reflects that component to be parallel to
/// `surface_normal`.
pub fn velocity_after_collision(
    initial: Vector3<f32>,
    surface_normal: Vector3<f32>,
    overbounce: f32,
) -> (Vector3<f32>, CollisionFlags) {
    let mut flags = CollisionFlags::empty();

    if surface_normal.z > 0.0 {
        flags |= CollisionFlags::HORIZONTAL;
    } else if surface_normal.z == 0.0 {
        flags |= CollisionFlags::VERTICAL;
    }

    let change = (overbounce * initial.dot(surface_normal)) * surface_normal;
    let mut out = initial - change;

    for i in 0..3 {
        if out[i].abs() < STOP_THRESHOLD {
            out[i] = 0.0;
        }
    }

    (out, flags)
}

/// Calculates a new velocity after collision with multiple surfaces.
pub fn velocity_after_multi_collision(
    initial: Vector3<f32>,
    planes: &[Hyperplane],
    overbounce: f32,
) -> Option<Vector3<f32>> {
    // Try to find a plane which produces a post-collision velocity that will
    // not cause a subsequent collision with any of the other planes.

    if let Some((a, plane_a)) = planes.iter().enumerate().next() {
        let (velocity_a, _flags) = velocity_after_collision(initial, plane_a.normal(), overbounce);

        for (b, plane_b) in planes.iter().enumerate() {
            if a == b {
                // Don't test a plane against itself.
                continue;
            }

            if velocity_a.dot(plane_b.normal()) < 0.0 {
                // New velocity would be directed into another plane.
                break;
            }
        }

        // This velocity is not expected to cause immediate collisions with
        // other planes, so return it.
        return Some(velocity_a);
    }

    if planes.len() > 2 {
        // Quake simply gives up in this case. This is distinct from returning
        // the zero vector, as it indicates that the trajectory has really
        // wedged something in a corner.
        None
    } else {
        // Redirect velocity along the intersection of the planes.
        let dir = planes[0].normal().cross(planes[1].normal());
        let scale = initial.dot(dir);
        Some(scale * dir)
    }
}

/// Represents the start of a collision trace.
#[derive(Clone, Debug)]
pub struct TraceStart {
    point: Vector3<f32>,
    /// The ratio along the original trace length at which this (sub)trace
    /// begins.
    ratio: f32,
}

impl TraceStart {
    pub fn new(point: Vector3<f32>, ratio: f32) -> TraceStart {
        TraceStart { point, ratio }
    }
}

/// Represents the end of a trace which crossed between leaves.
#[derive(Clone, Debug)]
pub struct TraceEndBoundary {
    pub ratio: f32,
    pub plane: Hyperplane,
}

/// Indicates the the nature of the end of a trace.
#[derive(Clone, Debug)]
pub enum TraceEndKind {
    /// This endpoint falls within a leaf.
    Terminal,

    /// This endpoint falls on a leaf boundary (a plane).
    Boundary(TraceEndBoundary),
}

/// Represents the end of a trace.
#[derive(Clone, Debug)]
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

    pub fn kind(&self) -> &TraceEndKind {
        &self.kind
    }
}

#[derive(Clone, Debug)]
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
            self.start.point, self.end.point, other.start.point, other.end.point
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

    /// Adjusts the start and end points of the trace by an offset.
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

    /// Returns the point at which the trace began.
    pub fn start_point(&self) -> Vector3<f32> {
        self.start.point
    }

    /// Returns the end of this trace.
    pub fn end(&self) -> &TraceEnd {
        &self.end
    }

    /// Returns the point at which the trace ended.
    pub fn end_point(&self) -> Vector3<f32> {
        self.end.point
    }

    /// Returns true if the entire trace is within solid leaves.
    pub fn all_solid(&self) -> bool {
        self.contents == BspLeafContents::Solid
    }

    /// Returns true if the trace began in a solid leaf but ended outside it.
    pub fn start_solid(&self) -> bool {
        self.start_solid
    }

    pub fn in_open(&self) -> bool {
        self.contents == BspLeafContents::Empty
    }

    pub fn in_water(&self) -> bool {
        self.contents != BspLeafContents::Empty && self.contents != BspLeafContents::Solid
    }

    /// Returns whether the trace ended without a collision.
    pub fn is_terminal(&self) -> bool {
        if let TraceEndKind::Terminal = self.end.kind {
            true
        } else {
            false
        }
    }

    /// Returns the ratio of travelled distance to intended distance.
    ///
    /// This indicates how far along the original trajectory the trace proceeded
    /// before colliding with a different medium.
    pub fn ratio(&self) -> f32 {
        match &self.end.kind {
            TraceEndKind::Terminal => 1.0,
            TraceEndKind::Boundary(boundary) => boundary.ratio,
        }
    }
}

bitflags! {
    pub struct CollisionFlags: u32 {
        const HORIZONTAL = 1;
        const VERTICAL = 2;
        const STOPPED = 4;
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
