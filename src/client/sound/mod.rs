// Copyright © 2018 Cormac O'Brien
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

use std::{
    cell::{Cell, RefCell},
    io::{self, BufReader, Cursor, Read},
};

use crate::common::vfs::{Vfs, VfsError};

use cgmath::{InnerSpace, Vector3};
use rodio::{
    source::{Buffered, SamplesConverter},
    Decoder, OutputStreamHandle, Sink, Source,
};
use thiserror::Error;
use chrono::Duration;
use super::entity::ClientEntity;

pub const DISTANCE_ATTENUATION_FACTOR: f32 = 0.001;
const MAX_ENTITY_CHANNELS: usize = 128;

#[derive(Error, Debug)]
pub enum SoundError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("Virtual filesystem error: {0}")]
    Vfs(#[from] VfsError),
    #[error("WAV decoder error: {0}")]
    Decoder(#[from] rodio::decoder::DecoderError),
}

/// Data needed for sound spatialization.
///
/// This struct is updated every frame.
#[derive(Debug)]
pub struct Listener {
    origin: Cell<Vector3<f32>>,
    left_ear: Cell<Vector3<f32>>,
    right_ear: Cell<Vector3<f32>>,
}

impl Listener {
    pub fn new() -> Listener {
        Listener {
            origin: Cell::new(Vector3::new(0.0, 0.0, 0.0)),
            left_ear: Cell::new(Vector3::new(0.0, 0.0, 0.0)),
            right_ear: Cell::new(Vector3::new(0.0, 0.0, 0.0)),
        }
    }

    pub fn origin(&self) -> Vector3<f32> {
        self.origin.get()
    }

    pub fn left_ear(&self) -> Vector3<f32> {
        self.left_ear.get()
    }

    pub fn right_ear(&self) -> Vector3<f32> {
        self.right_ear.get()
    }

    pub fn set_origin(&self, new_origin: Vector3<f32>) {
        self.origin.set(new_origin);
    }

    pub fn set_left_ear(&self, new_origin: Vector3<f32>) {
        self.left_ear.set(new_origin);
    }

    pub fn set_right_ear(&self, new_origin: Vector3<f32>) {
        self.right_ear.set(new_origin);
    }

    pub fn attenuate(
        &self,
        emitter_origin: Vector3<f32>,
        base_volume: f32,
        attenuation: f32,
    ) -> f32 {
        let decay = (emitter_origin - self.origin.get()).magnitude()
            * attenuation
            * DISTANCE_ATTENUATION_FACTOR;
        let volume = ((1.0 - decay) * base_volume).max(0.0);
        volume
    }
}

#[derive(Clone)]
pub struct AudioSource(Buffered<SamplesConverter<Decoder<BufReader<Cursor<Vec<u8>>>>, f32>>);

impl AudioSource {
    pub fn load<S>(vfs: &Vfs, name: S) -> Result<AudioSource, SoundError>
    where
        S: AsRef<str>,
    {
        let name = name.as_ref();
        let full_path = "sound/".to_owned() + name;
        let mut file = vfs.open(&full_path)?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;

        let src = Decoder::new(BufReader::new(Cursor::new(data)))?
            .convert_samples()
            .buffered();

        Ok(AudioSource(src))
    }
}

pub struct StaticSound {
    origin: Vector3<f32>,
    sink: RefCell<Sink>,
    volume: f32,
    attenuation: f32,
}

impl StaticSound {
    pub fn new(
        stream: &OutputStreamHandle,
        origin: Vector3<f32>,
        src: AudioSource,
        volume: f32,
        attenuation: f32,
        listener: &Listener,
    ) -> StaticSound {
        // TODO: handle PlayError once PR accepted
        let sink = Sink::try_new(&stream).unwrap();
        let infinite = src.0.clone().repeat_infinite();
        sink.append(infinite);
        sink.set_volume(listener.attenuate(origin, volume, attenuation));

        StaticSound {
            origin,
            sink: RefCell::new(sink),
            volume,
            attenuation,
        }
    }

    pub fn update(&self, listener: &Listener) {
        let sink = self.sink.borrow_mut();

        sink.set_volume(listener.attenuate(self.origin, self.volume, self.attenuation));
    }
}

/// Represents a single audio channel, capable of playing one sound at a time.
pub struct Channel {
    stream: OutputStreamHandle,
    sink: RefCell<Option<Sink>>,
    master_vol: Cell<f32>,
    attenuation: Cell<f32>,
}

impl Channel {
    /// Create a new `Channel` backed by the given `Device`.
    pub fn new(stream: OutputStreamHandle) -> Channel {
        Channel {
            stream,
            sink: RefCell::new(None),
            master_vol: Cell::new(0.0),
            attenuation: Cell::new(0.0),
        }
    }

    /// Play a new sound on this channel, cutting off any sound that was previously playing.
    pub fn play(
        &self,
        src: AudioSource,
        ent_pos: Vector3<f32>,
        listener: &Listener,
        volume: f32,
        attenuation: f32,
    ) {
        self.master_vol.set(volume);
        self.attenuation.set(attenuation);

        // stop the old sound
        self.sink.replace(None);

        // start the new sound
        let new_sink = Sink::try_new(&self.stream).unwrap();
        new_sink.append(src.0);
        new_sink.set_volume(listener.attenuate(
            ent_pos,
            self.master_vol.get(),
            self.attenuation.get(),
        ));

        self.sink.replace(Some(new_sink));
    }

    pub fn update(&self, ent_pos: Vector3<f32>, listener: &Listener) {
        if let Some(ref sink) = *self.sink.borrow_mut() {
            // attenuate using quake coordinates since distance is the same either way
            sink.set_volume(listener.attenuate(
                ent_pos,
                self.master_vol.get(),
                self.attenuation.get(),
            ));
        };
    }

    /// Stop the sound currently playing on this channel, if there is one.
    pub fn stop(&self) {
        self.sink.replace(None);
    }

    /// Returns whether or not this `Channel` is currently in use.
    pub fn in_use(&self) -> bool {
        let replace_sink;
        match *self.sink.borrow() {
            Some(ref sink) => replace_sink = sink.empty(),
            None => return false,
        }

        // if the sink isn't in use, free it
        if replace_sink {
            self.sink.replace(None);
            false
        } else {
            true
        }
    }
}

pub struct EntityChannel {
    start_time: Duration,
    // if None, sound is associated with a temp entity
    ent_id: Option<usize>,
    ent_channel: i8,
    channel: Channel,
}

impl EntityChannel {
    pub fn channel(&self) -> &Channel {
        &self.channel
    }

    pub fn entity_id(&self) -> Option<usize> {
        self.ent_id
    }
}

pub struct EntityMixer {
    stream: OutputStreamHandle,
    // TODO: replace with an array once const type parameters are implemented
    channels: Box<[Option<EntityChannel>]>,
}

impl EntityMixer {
    pub fn new(stream: OutputStreamHandle) -> EntityMixer {
        let mut channel_vec = Vec::new();

        for _ in 0..MAX_ENTITY_CHANNELS {
            channel_vec.push(None);
        }

        EntityMixer {
            stream,
            channels: channel_vec.into_boxed_slice(),
        }
    }

    fn find_free_channel(&self, ent_id: Option<usize>, ent_channel: i8) -> usize {
        let mut oldest = 0;

        for (i, channel) in self.channels.iter().enumerate() {
            match *channel {
                Some(ref chan) => {
                    // if this channel is free, return it
                    if !chan.channel.in_use() {
                        return i;
                    }

                    // replace sounds on the same entity channel
                    if ent_channel != 0
                        && chan.ent_id == ent_id
                        && (chan.ent_channel == ent_channel || ent_channel == -1)
                    {
                        return i;
                    }

                    // TODO: don't clobber player sounds with monster sounds

                    // keep track of which sound started the earliest
                    match self.channels[oldest] {
                        Some(ref o) => {
                            if chan.start_time < o.start_time {
                                oldest = i;
                            }
                        }
                        None => oldest = i,
                    }
                }

                None => return i,
            }
        }

        // if there are no good channels, just replace the one that's been running the longest
        oldest
    }

    pub fn start_sound(
        &mut self,
        src: AudioSource,
        time: Duration,
        ent_id: Option<usize>,
        ent_channel: i8,
        volume: f32,
        attenuation: f32,
        origin: Vector3<f32>,
        listener: &Listener,
    ) {
        let chan_id = self.find_free_channel(ent_id, ent_channel);
        let new_channel = Channel::new(self.stream.clone());

        new_channel.play(
            src.clone(),
            origin,
            listener,
            volume,
            attenuation,
        );
        self.channels[chan_id] = Some(EntityChannel {
            start_time: time,
            ent_id,
            ent_channel,
            channel: new_channel,
        })
    }

    pub fn iter_entity_channels(&self) -> impl Iterator<Item = &EntityChannel> {
        self.channels.iter().filter_map(|e| e.as_ref())
    }

    pub fn stream(&self) -> OutputStreamHandle {
        self.stream.clone()
    }
}
