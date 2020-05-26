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

mod error;
pub use self::error::{SoundError, SoundErrorKind};

use std::{
    cell::{Cell, RefCell},
    io::{BufReader, BufWriter, Cursor, Read},
    rc::Rc,
};

use crate::common::vfs::Vfs;

use failure::ResultExt;

use cgmath::{InnerSpace, Vector3};
use failure::Error;
use hound::{WavReader, WavWriter};
use rodio::{
    source::{Buffered, SamplesConverter},
    Decoder, Device, Sink, Source,
};

pub const DISTANCE_ATTENUATION_FACTOR: f32 = 0.001;

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
        let mut file = vfs.open(&full_path).context(SoundErrorKind::Io { name: name.to_owned() })?;
        let mut data = Vec::new();
        file.read_to_end(&mut data).context(SoundErrorKind::Io { name: name.to_owned() })?;

        let spec = {
            let wav_reader =
                WavReader::new(Cursor::new(&mut data)).context(SoundErrorKind::WavReadFailed {
                    name: name.to_owned(),
                })?;
            wav_reader.spec()
        };

        // have to convert from 8- to 16-bit here because rodio chokes on 8-bit PCM
        // TODO: file an issue with rodio
        if spec.bits_per_sample == 8 {
            let mut wav_reader =
                WavReader::new(Cursor::new(&mut data)).context(SoundErrorKind::WavReadFailed {
                    name: name.to_owned(),
                })?;
            let len = wav_reader.len();
            let mut data_16bit: Vec<i16> = Vec::with_capacity(len as usize);
            for sample in wav_reader.samples::<i8>() {
                data_16bit.push(sample.unwrap() as i16 * 256);
            }

            data.clear();
            let w = BufWriter::new(Cursor::new(&mut data));
            let mut spec16 = spec;
            spec16.bits_per_sample = 16;
            let mut wav_writer =
                WavWriter::new(w, spec16).context(SoundErrorKind::WavWriteFailed {
                    name: name.to_owned(),
                })?;
            let mut i16_writer = wav_writer.get_i16_writer(len);
            for s in data_16bit {
                i16_writer.write_sample(s);
            }
            i16_writer.flush().context(SoundErrorKind::WavWriteFailed {
                name: name.to_owned(),
            })?;
        }

        let src = Decoder::new(BufReader::new(Cursor::new(data)))
            .context(SoundErrorKind::DecodeFailed {
                name: name.to_owned(),
            })?
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
        device: &Device,
        origin: Vector3<f32>,
        src: AudioSource,
        volume: f32,
        attenuation: f32,
        listener: &Listener,
    ) -> StaticSound {
        let sink = Sink::new(device);
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
    device: Rc<Device>,
    sink: RefCell<Option<Sink>>,
    master_vol: Cell<f32>,
    attenuation: Cell<f32>,
}

impl Channel {
    /// Create a new `Channel` backed by the given `Device`.
    pub fn new(device: Rc<Device>) -> Channel {
        Channel {
            device,
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
        let new_sink = Sink::new(&self.device);
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
