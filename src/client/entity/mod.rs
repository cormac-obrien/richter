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

use crate::common::net::{EntityState, EntityEffects};

use cgmath::{Deg, Vector3};
use chrono::Duration;

pub const MAX_BEAMS: usize = 24;

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

pub struct Light {
    pub origin: Vector3<f32>,
    pub radius: f32,
    pub decay_rate: f32,
    pub min_light: f32,
    pub expire: Duration,
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

pub struct Beam {
    pub entity_id: usize,
    pub model_id: usize,
    pub expire: Duration,
    pub start: Vector3<f32>,
    pub end: Vector3<f32>,
}
