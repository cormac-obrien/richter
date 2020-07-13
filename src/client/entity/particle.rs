// Copyright © 2020 Cormac O'Brien
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

use std::{cell::RefCell, collections::HashSet};

use crate::common::engine;

use cgmath::Vector3;
use chrono::Duration;
use slab::Slab;

lazy_static! {
    static ref COLOR_RAMP_EXPLOSION_FAST: ColorRamp = ColorRamp {
        ramp: vec![0x6F, 0x6D, 0x6B, 0x69, 0x67, 0x65, 0x63, 0x61],
        fps: 10.0,
    };
    static ref COLOR_RAMP_EXPLOSION_SLOW: ColorRamp = ColorRamp {
        ramp: vec![0x6F, 0x6E, 0x6D, 0x6C, 0x6B, 0x6A, 0x68, 0x66],
        fps: 5.0,
    };
    static ref COLOR_RAMP_FIRE: ColorRamp = ColorRamp {
        ramp: vec![0x6D, 0x6B, 0x06, 0x05, 0x04, 0x03],
        fps: 15.0,
    };
}

// TODO: make max configurable
pub const MIN_PARTICLES: usize = 512;
pub const MAX_PARTICLES: usize = 2048;

/// An animated color ramp.
///
/// Colors are specified using 8-bit indexed values, which should be translated
/// using the palette.
#[derive(Debug)]
pub struct ColorRamp {
    // TODO: arrayvec, tinyvec, or array once const generics are stable
    ramp: Vec<u8>,

    // frames per second of the animation
    fps: f32,
}

impl ColorRamp {
    /// Returns the frame corresponding to the given time.
    ///
    /// If the animation has already completed by `elapsed`, returns `None`.
    pub fn color(&self, elapsed: Duration) -> Option<u8> {
        let frame = (engine::duration_to_f32(elapsed) * self.fps) as usize;
        self.ramp.get(frame).map(|c| *c)
    }
}

/// Dictates the behavior of a particular particle.
///
/// Particles which are animated with a color ramp are despawned automatically
/// when the animation is complete.
#[derive(Copy, Clone, Debug)]
pub enum ParticleKind {
    /// Normal particle, unaffected by gravity.
    Static,

    /// Normal particle, affected by gravity.
    Grav,

    /// Fire and smoke particles. Animated using `COLOR_RAMP_FIRE`. Inversely
    /// affected by gravity, rising instead of falling.
    Fire,

    /// Explosion particles. May have `COLOR_RAMP_EXPLOSION_FAST` or
    /// `COLOR_RAMP_EXPLOSION_SLOW`. Affected by gravity.
    Explosion {
        /// Specifies the color ramp to use.
        ramp: &'static ColorRamp,
    },

    /// Spawn (enemy) death explosion particle. Accelerates at
    /// `v(t2) = v(t1) + 4 * (t2 - t1)`. May or may not have an intrinsic
    /// z-velocity.
    Blob {
        /// If false, particle only moves in the XY plane and is unaffected by
        /// gravity.
        has_z_velocity: bool,
    },
}

/// Factor at which particles are affected by gravity.
pub const PARTICLE_GRAVITY_FACTOR: f32 = 0.05;

/// A live particle.
#[derive(Copy, Clone, Debug)]
pub struct Particle {
    kind: ParticleKind,
    origin: Vector3<f32>,
    velocity: Vector3<f32>,
    color: u8,
    spawned: Duration,
    expire: Duration,
}

impl Particle {
    /// Particle update function.
    ///
    /// The return value indicates whether the particle should be retained after this
    /// frame.
    ///
    /// For details on how individual particles behave, see the documentation for
    /// [`ParticleKind`](ParticleKind).
    pub fn update(&mut self, time: Duration, frame_time: Duration, sv_gravity: f32) -> bool {
        use ParticleKind::*;

        let velocity_factor = engine::duration_to_f32(frame_time);
        let gravity = velocity_factor * sv_gravity * PARTICLE_GRAVITY_FACTOR;

        // don't bother updating expired particles
        if time >= self.expire {
            return false;
        }

        match self.kind {
            Static => true,

            Grav => {
                self.origin += self.velocity * velocity_factor;
                self.velocity.z -= gravity;
                true
            }

            Fire => match COLOR_RAMP_FIRE.color(time - self.spawned) {
                Some(c) => {
                    self.origin += self.velocity * velocity_factor;
                    // rises instead of falling
                    self.velocity.z += gravity;
                    self.color = c;
                    true
                }
                None => false,
            },

            Explosion { ramp } => match ramp.color(time - self.spawned) {
                Some(c) => {
                    self.origin += self.velocity * velocity_factor;
                    self.velocity.z -= gravity;
                    self.color = c;
                    true
                }
                None => false,
            },

            Blob { has_z_velocity } => {
                if !has_z_velocity {
                    let xy_velocity = Vector3::new(self.velocity.x, self.velocity.y, 0.0);
                    self.origin += xy_velocity * velocity_factor;
                } else {
                    self.origin += self.velocity * velocity_factor;
                    self.velocity.z -= gravity;
                }

                true
            }
        }
    }
}

/// A list of particles.
///
/// Space for new particles is allocated from an internal [`Slab`](slab::Slab) of fixed
/// size.
pub struct ParticleList {
    // allocation pool
    slab: RefCell<Slab<Particle>>,

    // set of live particles
    live: RefCell<HashSet<usize>>,
}

impl ParticleList {
    /// Create a new particle list with the given capacity.
    ///
    /// This determines the capacity of both the underlying `Slab` and the set of
    /// live particles.
    pub fn with_capacity(capacity: usize) -> ParticleList {
        let slab = RefCell::new(Slab::with_capacity(capacity.min(MAX_PARTICLES)));

        // TODO: tune capacity
        let live = RefCell::new(HashSet::with_capacity(capacity.min(MAX_PARTICLES)));

        ParticleList { slab, live }
    }

    /// Insert a particle into the live list.
    // TODO: come up with a better eviction policy
    // the original engine ignores new particles if at capacity, but it's not ideal
    pub fn insert(&mut self, particle: Particle) -> bool {
        if self.slab.borrow().len() == self.slab.borrow().capacity() {
            return false;
        }

        let slab_id = self.slab.borrow_mut().insert(particle);
        self.live.borrow_mut().insert(slab_id);
        true
    }

    /// Update all live particles.
    ///
    /// Particles are updated with [Particle::update]. That
    /// function's return value indicates whether the particle should be retained
    /// or not.
    pub fn update(&mut self, time: Duration, frame_time: Duration, sv_gravity: f32) {
        self.live.borrow_mut().retain(|part_id| {
            let retain = match self.slab.borrow_mut().get_mut(*part_id) {
                Some(part) => part.update(time, frame_time, sv_gravity),
                None => unreachable!(
                    "ParticleList::update: no Particle with id {} in slab",
                    part_id
                ),
            };

            if !retain {
                self.slab.borrow_mut().remove(*part_id);
            }

            retain
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cgmath::Zero;

    fn particles_eq(p1: &Particle, p2: &Particle) -> bool {
        p1.color == p2.color && p1.velocity == p2.velocity && p1.origin == p2.origin
    }

    #[test]
    fn test_particle_list_update() {
        let mut list = ParticleList::with_capacity(10);
        let exp_times = vec![10, 5, 2, 7, 3];
        for exp in exp_times.iter() {
            list.insert(Particle {
                kind: ParticleKind::Static,
                origin: Vector3::zero(),
                velocity: Vector3::zero(),
                color: 0,
                spawned: Duration::zero(),
                expire: Duration::seconds(*exp),
            });
        }

        let expected: Vec<_> = exp_times
            .iter()
            .filter(|t| **t > 5)
            .map(|t| Particle {
                kind: ParticleKind::Static,
                origin: Vector3::zero(),
                velocity: Vector3::zero(),
                color: 0,
                spawned: Duration::zero(),
                expire: Duration::seconds(*t),
            })
            .collect();
        let mut after_update: Vec<Particle> = Vec::new();
        list.update(Duration::seconds(5), Duration::milliseconds(17), 10.0);
        after_update
            .iter()
            .zip(expected.iter())
            .for_each(|(p1, p2)| assert!(particles_eq(p1, p2)));
    }
}
