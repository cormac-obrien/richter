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

use std::ops::RangeInclusive;

use crate::{
    client::ClientEntity,
    common::{
        alloc::LinkedSlab,
        engine,
        math::{self, VERTEX_NORMAL_COUNT},
    },
};

use cgmath::{InnerSpace as _, Vector3, Zero as _};
use chrono::Duration;
use rand::{
    distributions::{Distribution as _, Uniform},
    rngs::SmallRng,
    SeedableRng,
};

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
    static ref EXPLOSION_SCATTER_DISTRIBUTION: Uniform<f32> = Uniform::new(-16.0, 16.0);
    static ref EXPLOSION_VELOCITY_DISTRIBUTION: Uniform<f32> = Uniform::new(-256.0, 256.0);
}

// TODO: make max configurable
pub const MIN_PARTICLES: usize = 512;

// should be possible to get the whole particle list in cache at once
pub const MAX_PARTICLES: usize = 16384;

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
    pub fn color(&self, elapsed: Duration, frame_skip: usize) -> Option<u8> {
        let frame = (engine::duration_to_f32(elapsed) * self.fps) as usize + frame_skip;
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
    Fire {
        /// Specifies the number of frames to skip.
        frame_skip: usize,
    },

    /// Explosion particles. May have `COLOR_RAMP_EXPLOSION_FAST` or
    /// `COLOR_RAMP_EXPLOSION_SLOW`. Affected by gravity.
    Explosion {
        /// Specifies the color ramp to use.
        ramp: &'static ColorRamp,

        /// Specifies the number of frames to skip.
        frame_skip: usize,
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

            Fire { frame_skip } => match COLOR_RAMP_FIRE.color(time - self.spawned, frame_skip) {
                Some(c) => {
                    self.origin += self.velocity * velocity_factor;
                    // rises instead of falling
                    self.velocity.z += gravity;
                    self.color = c;
                    true
                }
                None => false,
            },

            Explosion { ramp, frame_skip } => match ramp.color(time - self.spawned, frame_skip) {
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

pub enum TrailKind {
    Rocket = 0,
    Smoke = 1,
    Blood = 2,
    TracerGreen = 3,
    BloodSlight = 4,
    TracerRed = 5,
    Vore = 6,
}

/// A list of particles.
///
/// Space for new particles is allocated from an internal [`Slab`](slab::Slab) of fixed
/// size.
pub struct Particles {
    // allocation pool
    slab: LinkedSlab<Particle>,

    // random number generator
    rng: SmallRng,

    angle_velocities: [Vector3<f32>; VERTEX_NORMAL_COUNT],
}

impl Particles {
    /// Create a new particle list with the given capacity.
    ///
    /// This determines the capacity of both the underlying `Slab` and the set of
    /// live particles.
    pub fn with_capacity(capacity: usize) -> Particles {
        lazy_static! {
            // avelocities initialized with (rand() & 255) * 0.01;
            static ref VELOCITY_DISTRIBUTION: Uniform<f32> = Uniform::new(0.0, 2.56);
        }

        let slab = LinkedSlab::with_capacity(capacity.min(MAX_PARTICLES));
        let rng = SmallRng::from_entropy();
        let angle_velocities = [Vector3::zero(); VERTEX_NORMAL_COUNT];

        let mut particles = Particles {
            slab,
            rng,
            angle_velocities,
        };

        for i in 0..angle_velocities.len() {
            particles.angle_velocities[i] = particles.random_vector3(&VELOCITY_DISTRIBUTION);
        }

        particles
    }

    /// Insert a particle into the live list.
    // TODO: come up with a better eviction policy
    // the original engine ignores new particles if at capacity, but it's not ideal
    pub fn insert(&mut self, particle: Particle) -> bool {
        // check capacity
        if self.slab.len() == self.slab.capacity() {
            return false;
        }

        // insert it
        self.slab.insert(particle);
        true
    }

    /// Clears all particles.
    pub fn clear(&mut self) {
        self.slab.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = &Particle> {
        self.slab.iter()
    }

    /// Update all live particles, deleting any that are expired.
    ///
    /// Particles are updated with [Particle::update]. That
    /// function's return value indicates whether the particle should be retained
    /// or not.
    pub fn update(&mut self, time: Duration, frame_time: Duration, sv_gravity: f32) {
        self.slab
            .retain(|_, particle| particle.update(time, frame_time, sv_gravity));
    }

    fn scatter(&mut self, origin: Vector3<f32>, scatter_distr: &Uniform<f32>) -> Vector3<f32> {
        origin
            + Vector3::new(
                scatter_distr.sample(&mut self.rng),
                scatter_distr.sample(&mut self.rng),
                scatter_distr.sample(&mut self.rng),
            )
    }

    fn random_vector3(&mut self, velocity_distr: &Uniform<f32>) -> Vector3<f32> {
        Vector3::new(
            velocity_distr.sample(&mut self.rng),
            velocity_distr.sample(&mut self.rng),
            velocity_distr.sample(&mut self.rng),
        )
    }

    /// Creates a spherical cloud of particles around an entity.
    pub fn create_entity_field(&mut self, time: Duration, entity: &ClientEntity) {
        let beam_length = 16.0;
        let dist = 64.0;

        for i in 0..VERTEX_NORMAL_COUNT {
            let float_time = engine::duration_to_f32(time);

            let angles = float_time * self.angle_velocities[i];

            let sin_yaw = angles[0].sin();
            let cos_yaw = angles[0].cos();
            let sin_pitch = angles[1].sin();
            let cos_pitch = angles[1].cos();

            let forward = Vector3::new(cos_pitch * cos_yaw, cos_pitch * sin_yaw, -sin_pitch);
            let ttl = Duration::milliseconds(10);

            let origin = entity.origin + dist * math::VERTEX_NORMALS[i] + beam_length * forward;

            self.insert(Particle {
                kind: ParticleKind::Explosion {
                    ramp: &COLOR_RAMP_EXPLOSION_FAST,
                    frame_skip: 0,
                },
                origin,
                velocity: Vector3::zero(),
                color: COLOR_RAMP_EXPLOSION_FAST.ramp[0],
                spawned: time,
                expire: time + ttl,
            });
        }
    }

    /// Spawns a cloud of particles at a point.
    ///
    /// Each particle's origin is offset by a vector with components sampled
    /// from `scatter_distr`, and each particle's velocity is assigned a
    /// vector with components sampled from `velocity_distr`.
    ///
    /// Each particle's color is taken from `colors`, which is an inclusive
    /// range of palette indices. The spawned particles have evenly distributed
    /// colors throughout the range.
    pub fn create_random_cloud(
        &mut self,
        count: usize,
        colors: RangeInclusive<u8>,
        kind: ParticleKind,
        time: Duration,
        ttl: Duration,
        origin: Vector3<f32>,
        scatter_distr: &Uniform<f32>,
        velocity_distr: &Uniform<f32>,
    ) {
        let color_start = *colors.start() as usize;
        let color_end = *colors.end() as usize;
        for i in 0..count {
            let origin = self.scatter(origin, scatter_distr);
            let velocity = self.random_vector3(velocity_distr);
            let color = (color_start + i % (color_end - color_start + 1)) as u8;
            if !self.insert(Particle {
                kind,
                origin,
                velocity,
                color,
                spawned: time,
                expire: time + ttl,
            }) {
                // can't fit any more particles
                return;
            };
        }
    }

    /// Creates a rocket explosion.
    pub fn create_explosion(&mut self, time: Duration, origin: Vector3<f32>) {
        lazy_static! {
            static ref FRAME_SKIP_DISTRIBUTION: Uniform<usize> = Uniform::new(0, 4);
        }

        // spawn 512 particles each for both color ramps
        for ramp in [&*COLOR_RAMP_EXPLOSION_FAST, &*COLOR_RAMP_EXPLOSION_SLOW].iter() {
            let frame_skip = FRAME_SKIP_DISTRIBUTION.sample(&mut self.rng);
            self.create_random_cloud(
                512,
                ramp.ramp[frame_skip]..=ramp.ramp[frame_skip],
                ParticleKind::Explosion { ramp, frame_skip },
                time,
                Duration::seconds(5),
                origin,
                &EXPLOSION_SCATTER_DISTRIBUTION,
                &EXPLOSION_VELOCITY_DISTRIBUTION,
            );
        }
    }

    /// Creates an explosion using the given range of colors.
    pub fn create_color_explosion(
        &mut self,
        time: Duration,
        origin: Vector3<f32>,
        colors: RangeInclusive<u8>,
    ) {
        self.create_random_cloud(
            512,
            colors,
            ParticleKind::Blob {
                has_z_velocity: true,
            },
            time,
            Duration::milliseconds(300),
            origin,
            &EXPLOSION_SCATTER_DISTRIBUTION,
            &EXPLOSION_VELOCITY_DISTRIBUTION,
        );
    }

    /// Creates a death explosion for the Spawn.
    pub fn create_spawn_explosion(&mut self, time: Duration, origin: Vector3<f32>) {
        // R_BlobExplosion picks a random ttl with 1 + (rand() & 8) * 0.05
        // which gives a value of either 1 or 1.4 seconds.
        // (it's possible it was supposed to be 1 + (rand() & 7) * 0.05, which
        // would yield between 1 and 1.35 seconds in increments of 50ms.)
        let ttls = [Duration::seconds(1), Duration::milliseconds(1400)];

        for ttl in ttls.iter().cloned() {
            self.create_random_cloud(
                256,
                66..=71,
                ParticleKind::Blob {
                    has_z_velocity: true,
                },
                time,
                ttl,
                origin,
                &EXPLOSION_SCATTER_DISTRIBUTION,
                &EXPLOSION_VELOCITY_DISTRIBUTION,
            );

            self.create_random_cloud(
                256,
                150..=155,
                ParticleKind::Blob {
                    has_z_velocity: false,
                },
                time,
                ttl,
                origin,
                &EXPLOSION_SCATTER_DISTRIBUTION,
                &EXPLOSION_VELOCITY_DISTRIBUTION,
            );
        }
    }

    /// Creates a projectile impact.
    pub fn create_projectile_impact(
        &mut self,
        time: Duration,
        origin: Vector3<f32>,
        direction: Vector3<f32>,
        color: u8,
        count: usize,
    ) {
        lazy_static! {
            static ref SCATTER_DISTRIBUTION: Uniform<f32> = Uniform::new(-8.0, 8.0);

            // any color in block of 8 (see below)
            static ref COLOR_DISTRIBUTION: Uniform<u8> = Uniform::new(0, 8);

            // ttl between 0.1 and 0.5 seconds
            static ref TTL_DISTRIBUTION: Uniform<i64> = Uniform::new(100, 500);
        }

        for _ in 0..count {
            let scatter = self.random_vector3(&SCATTER_DISTRIBUTION);
            let color = color & !7 + COLOR_DISTRIBUTION.sample(&mut self.rng);
            let ttl = Duration::milliseconds(TTL_DISTRIBUTION.sample(&mut self.rng));

            self.insert(Particle {
                kind: ParticleKind::Grav,
                origin: origin + scatter,
                velocity: 15.0 * direction,
                // picks any color in the block of 8 the original color belongs to.
                // e.g., if the color argument is 17, picks randomly in [16, 23]
                color,
                spawned: time,
                expire: time + ttl,
            });
        }
    }

    /// Creates a lava splash effect.
    pub fn create_lava_splash(&mut self, time: Duration, origin: Vector3<f32>) {
        lazy_static! {
            // ttl between 2 and 2.64 seconds
            static ref TTL_DISTRIBUTION: Uniform<i64> = Uniform::new(2000, 2640);

            // any color on row 14
            static ref COLOR_DISTRIBUTION: Uniform<u8> = Uniform::new(224, 232);

            static ref DIR_OFFSET_DISTRIBUTION: Uniform<f32> = Uniform::new(0.0, 8.0);
            static ref SCATTER_Z_DISTRIBUTION: Uniform<f32> = Uniform::new(0.0, 64.0);
            static ref VELOCITY_DISTRIBUTION: Uniform<f32> = Uniform::new(50.0, 114.0);
        }

        for i in -16..16 {
            for j in -16..16 {
                let direction = Vector3::new(
                    8.0 * i as f32 + DIR_OFFSET_DISTRIBUTION.sample(&mut self.rng),
                    8.0 * j as f32 + DIR_OFFSET_DISTRIBUTION.sample(&mut self.rng),
                    256.0,
                );

                let scatter = Vector3::new(
                    direction.x,
                    direction.y,
                    SCATTER_Z_DISTRIBUTION.sample(&mut self.rng),
                );

                let velocity = VELOCITY_DISTRIBUTION.sample(&mut self.rng);

                let color = COLOR_DISTRIBUTION.sample(&mut self.rng);
                let ttl = Duration::milliseconds(TTL_DISTRIBUTION.sample(&mut self.rng));

                self.insert(Particle {
                    kind: ParticleKind::Grav,
                    origin: origin + scatter,
                    velocity: direction.normalize() * velocity,
                    color,
                    spawned: time,
                    expire: time + ttl,
                });
            }
        }
    }

    /// Creates a teleporter warp effect.
    pub fn create_teleporter_warp(&mut self, time: Duration, origin: Vector3<f32>) {
        lazy_static! {
            // ttl between 0.2 and 0.34 seconds
            static ref TTL_DISTRIBUTION: Uniform<i64> = Uniform::new(200, 340);

            // random grey particles
            static ref COLOR_DISTRIBUTION: Uniform<u8> = Uniform::new(7, 14);

            static ref SCATTER_DISTRIBUTION: Uniform<f32> = Uniform::new(0.0, 4.0);
            static ref VELOCITY_DISTRIBUTION: Uniform<f32> = Uniform::new(50.0, 114.0);
        }

        for i in (-16..16).step_by(4) {
            for j in (-16..16).step_by(4) {
                for k in (-24..32).step_by(4) {
                    let direction = Vector3::new(j as f32, i as f32, k as f32) * 8.0;
                    let scatter = Vector3::new(i as f32, j as f32, k as f32)
                        + self.random_vector3(&SCATTER_DISTRIBUTION);
                    let velocity = VELOCITY_DISTRIBUTION.sample(&mut self.rng);
                    let color = COLOR_DISTRIBUTION.sample(&mut self.rng);
                    let ttl = Duration::milliseconds(TTL_DISTRIBUTION.sample(&mut self.rng));

                    self.insert(Particle {
                        kind: ParticleKind::Grav,
                        origin: origin + scatter,
                        velocity: direction.normalize() * velocity,
                        color,
                        spawned: time,
                        expire: time + ttl,
                    });
                }
            }
        }
    }

    /// Create a particle trail between two points.
    ///
    /// Used for rocket fire/smoke trails, blood spatter, and projectile tracers.
    /// If `sparse` is true, the interval between particles is increased by 3 units.
    pub fn create_trail(
        &mut self,
        time: Duration,
        start: Vector3<f32>,
        end: Vector3<f32>,
        kind: TrailKind,
        sparse: bool,
    ) {
        use TrailKind::*;

        lazy_static! {
            static ref SCATTER_DISTRIBUTION: Uniform<f32> = Uniform::new(-3.0, 3.0);
            static ref FRAME_SKIP_DISTRIBUTION: Uniform<usize> = Uniform::new(0, 4);
            static ref BLOOD_COLOR_DISTRIBUTION: Uniform<u8> = Uniform::new(67, 71);
            static ref VORE_COLOR_DISTRIBUTION: Uniform<u8> = Uniform::new(152, 156);
        }

        let distance = (end - start).magnitude();
        let direction = (end - start).normalize();

        // particle interval in units
        let interval = if sparse { 3.0 } else { 1.0 }
            + match kind {
                BloodSlight => 3.0,
                _ => 0.0,
            };

        let ttl = Duration::seconds(2);

        for step in 0..(distance / interval) as i32 {
            let frame_skip = FRAME_SKIP_DISTRIBUTION.sample(&mut self.rng);
            let particle_kind = match kind {
                Rocket => ParticleKind::Fire { frame_skip },
                Smoke => ParticleKind::Fire {
                    frame_skip: frame_skip + 2,
                },
                Blood | BloodSlight => ParticleKind::Grav,
                TracerGreen | TracerRed | Vore => ParticleKind::Static,
            };

            let scatter = self.random_vector3(&SCATTER_DISTRIBUTION);

            let origin = start
                + direction * interval
                + match kind {
                    // vore scatter is [-16, 15] in original
                    // this gives range of ~[-16, 16]
                    Vore => scatter * 5.33,
                    _ => scatter,
                };

            let velocity = match kind {
                TracerGreen | TracerRed => {
                    30.0 * if step & 1 == 1 {
                        Vector3::new(direction.y, -direction.x, 0.0)
                    } else {
                        Vector3::new(-direction.y, direction.x, 0.0)
                    }
                }

                _ => Vector3::zero(),
            };

            let color = match kind {
                Rocket => COLOR_RAMP_FIRE.ramp[frame_skip],
                Smoke => COLOR_RAMP_FIRE.ramp[frame_skip + 2],
                Blood | BloodSlight => BLOOD_COLOR_DISTRIBUTION.sample(&mut self.rng),
                TracerGreen => 52 + 2 * (step & 4) as u8,
                TracerRed => 230 + 2 * (step & 4) as u8,
                Vore => VORE_COLOR_DISTRIBUTION.sample(&mut self.rng),
            };

            self.insert(Particle {
                kind: particle_kind,
                origin,
                velocity,
                color,
                spawned: time,
                expire: time + ttl,
            });
        }
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
        let mut list = Particles::with_capacity(10);
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
