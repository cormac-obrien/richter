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

pub mod particle;

use crate::common::{
    alloc::LinkedSlab,
    engine,
    net::{EntityEffects, EntityState},
};

use cgmath::{Deg, Vector3};
use chrono::Duration;

// if this is changed, it must also be changed in deferred.frag
pub const MAX_LIGHTS: usize = 32;
pub const MAX_BEAMS: usize = 24;
pub const MAX_TEMP_ENTITIES: usize = 64;
pub const MAX_STATIC_ENTITIES: usize = 128;

#[derive(Debug)]
pub struct ClientEntity {
    pub force_link: bool,
    pub baseline: EntityState,
    pub msg_time: Duration,
    pub msg_origins: [Vector3<f32>; 2],
    pub origin: Vector3<f32>,
    pub msg_angles: [Vector3<Deg<f32>>; 2],
    pub angles: Vector3<Deg<f32>>,
    pub model_id: usize,
    pub frame_id: usize,
    pub skin_id: usize,
    pub sync_base: Duration,
    pub effects: EntityEffects,
    pub light_id: Option<usize>,
    // vis_frame: usize,
}

impl ClientEntity {
    pub fn from_baseline(baseline: EntityState) -> ClientEntity {
        ClientEntity {
            force_link: false,
            baseline: baseline.clone(),
            msg_time: Duration::zero(),
            msg_origins: [Vector3::new(0.0, 0.0, 0.0), Vector3::new(0.0, 0.0, 0.0)],
            origin: baseline.origin,
            msg_angles: [
                Vector3::new(Deg(0.0), Deg(0.0), Deg(0.0)),
                Vector3::new(Deg(0.0), Deg(0.0), Deg(0.0)),
            ],
            angles: baseline.angles,
            model_id: baseline.model_id,
            frame_id: baseline.frame_id,
            skin_id: baseline.skin_id,
            sync_base: Duration::zero(),
            effects: baseline.effects,
            light_id: None,
        }
    }

    pub fn uninitialized() -> ClientEntity {
        ClientEntity {
            force_link: false,
            baseline: EntityState::uninitialized(),
            msg_time: Duration::zero(),
            msg_origins: [Vector3::new(0.0, 0.0, 0.0), Vector3::new(0.0, 0.0, 0.0)],
            origin: Vector3::new(0.0, 0.0, 0.0),
            msg_angles: [
                Vector3::new(Deg(0.0), Deg(0.0), Deg(0.0)),
                Vector3::new(Deg(0.0), Deg(0.0), Deg(0.0)),
            ],
            angles: Vector3::new(Deg(0.0), Deg(0.0), Deg(0.0)),
            model_id: 0,
            frame_id: 0,
            skin_id: 0,
            sync_base: Duration::zero(),
            effects: EntityEffects::empty(),
            light_id: None,
        }
    }

    pub fn get_origin(&self) -> Vector3<f32> {
        self.origin
    }

    pub fn get_angles(&self) -> Vector3<Deg<f32>> {
        self.angles
    }

    pub fn get_model_id(&self) -> usize {
        self.model_id
    }

    pub fn get_frame_id(&self) -> usize {
        self.frame_id
    }

    pub fn get_skin_id(&self) -> usize {
        self.skin_id
    }
}

/// A descriptor used to spawn dynamic lights.
#[derive(Clone, Debug)]
pub struct LightDesc {
    /// The origin of the light.
    pub origin: Vector3<f32>,

    /// The initial radius of the light.
    pub init_radius: f32,

    /// The rate of radius decay in units/second.
    pub decay_rate: f32,

    /// If the radius decays to this value, the light is ignored.
    pub min_radius: Option<f32>,

    /// Time-to-live of the light.
    pub ttl: Duration,
}

/// A dynamic point light.
#[derive(Clone, Debug)]
pub struct Light {
    origin: Vector3<f32>,
    init_radius: f32,
    decay_rate: f32,
    min_radius: Option<f32>,
    spawned: Duration,
    ttl: Duration,
}

impl Light {
    /// Create a light from a `LightDesc` at the specified time.
    pub fn from_desc(time: Duration, desc: LightDesc) -> Light {
        Light {
            origin: desc.origin,
            init_radius: desc.init_radius,
            decay_rate: desc.decay_rate,
            min_radius: desc.min_radius,
            spawned: time,
            ttl: desc.ttl,
        }
    }

    /// Return the origin of the light.
    pub fn origin(&self) -> Vector3<f32> {
        self.origin
    }

    /// Return the radius of the light for the given time.
    ///
    /// If the radius would decay to a negative value, returns 0.
    pub fn radius(&self, time: Duration) -> f32 {
        let lived = time - self.spawned;
        let decay = self.decay_rate * engine::duration_to_f32(lived);
        let radius = (self.init_radius - decay).max(0.0);

        if let Some(min) = self.min_radius {
            if radius < min {
                return 0.0;
            }
        }

        radius
    }

    /// Returns `true` if the light should be retained at the specified time.
    pub fn retain(&mut self, time: Duration) -> bool {
        self.spawned + self.ttl > time
    }
}

/// A set of active dynamic lights.
pub struct Lights {
    slab: LinkedSlab<Light>,
}

impl Lights {
    /// Create an empty set of lights with the given capacity.
    pub fn with_capacity(capacity: usize) -> Lights {
        Lights {
            slab: LinkedSlab::with_capacity(capacity),
        }
    }

    /// Return a reference to the light with the given key, or `None` if no
    /// such light exists.
    pub fn get(&self, key: usize) -> Option<&Light> {
        self.slab.get(key)
    }

    /// Return a mutable reference to the light with the given key, or `None`
    /// if no such light exists.
    pub fn get_mut(&mut self, key: usize) -> Option<&mut Light> {
        self.slab.get_mut(key)
    }

    /// Insert a new light into the set of lights.
    ///
    /// Returns a key corresponding to the newly inserted light.
    ///
    /// If `key` is `Some` and there is an existing light with that key, then
    /// the light will be overwritten with the new value.
    pub fn insert(&mut self, time: Duration, desc: LightDesc, key: Option<usize>) -> usize {
        if let Some(k) = key {
            if let Some(key_light) = self.slab.get_mut(k) {
                *key_light = Light::from_desc(time, desc);
                return k;
            }
        }

        self.slab.insert(Light::from_desc(time, desc))
    }

    /// Return an iterator over the active lights.
    pub fn iter(&self) -> impl Iterator<Item = &Light> {
        self.slab.iter()
    }

    /// Updates the set of dynamic lights for the specified time.
    ///
    /// This will deallocate any lights which have outlived their time-to-live.
    pub fn update(&mut self, time: Duration) {
        self.slab.retain(|_, light| light.retain(time));
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Beam {
    pub entity_id: usize,
    pub model_id: usize,
    pub expire: Duration,
    pub start: Vector3<f32>,
    pub end: Vector3<f32>,
}
