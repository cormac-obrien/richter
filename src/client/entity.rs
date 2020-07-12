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

use std::{collections::HashSet, cell::RefCell};

use cgmath::Vector3;
use chrono::Duration;
use slab::Slab;

// TODO: make max configurable
pub const MIN_PARTICLES: usize = 512;
pub const MAX_PARTICLES: usize = 2048;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ParticleKind {
    Static = 0,
    Grav = 1,
    SlowGrav = 2,
    Fire = 3,
    Explosion = 4,
    ColorExplosion = 5,
    Blob = 6,
    Blob2 = 7,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Particle {
    kind: ParticleKind,
    origin: Vector3<f32>,
    velocity: Vector3<f32>,
    color: f32,
    expire: Duration,
}

pub struct ParticleList {
    // allocation pool
    slab: RefCell<Slab<Particle>>,

    // set of live particles
    live: RefCell<HashSet<usize>>,
}

impl ParticleList {
    pub fn with_capacity(capacity: usize) -> ParticleList {
        let slab = RefCell::new(Slab::with_capacity(capacity));

        // TODO: tune capacity
        let live = RefCell::new(HashSet::with_capacity(capacity / 8));

        ParticleList {
            slab,
            live,
        }
    }

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

    pub fn update<F>(&mut self, time: Duration, mut f: F)
    where
        F: FnMut(&mut Particle),
    {
        self.live.borrow_mut().retain(|part_id| {
            let retain = match self.slab.borrow_mut().get_mut(*part_id) {
                Some(part) => {
                    if part.expire <= time {
                        false
                    } else {
                        // apply update function
                        f(part);
                        true
                    }
                }
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

pub struct Light {
    origin: Vector3<f32>,
    radius: f32,
    decay_rate: f32,
    min_light: f32,
    expire: Duration,
}

impl Light {
    pub fn origin(&self) -> Vector3<f32> {
        self.origin
    }

    pub fn radius(&self) -> f32 {
        self.radius
    }

    pub fn decay_rate(&self) -> f32 {
        self.decay_rate
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cgmath::Zero;

    #[test]
    fn test_particle_list_update() {
        let mut list = ParticleList::with_capacity(10);
        let exp_times = vec![10, 5, 2, 7, 3];
        for exp in exp_times.iter() {
            list.insert(Particle {
                kind: ParticleKind::Static,
                origin: Vector3::zero(),
                velocity: Vector3::zero(),
                color: 0.0,
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
                color: 0.0,
                expire: Duration::seconds(*t),
            })
            .collect();
        let mut after_update = Vec::new();
        list.update(Duration::seconds(5), |p| after_update.push(*p));
        assert_eq!(after_update, expected);
    }
}
