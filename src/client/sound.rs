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

use std::cell::RefCell;
use std::fmt;
use std::io::{BufReader, Cursor, Read};
use std::rc::Rc;

use common::vfs::Vfs;

use cgmath::Vector3;
use failure::Error;
use rodio::{Decoder, Endpoint, Sink, Source};
use rodio::decoder::DecoderError;
use rodio::source::{Buffered, SamplesConverter};

#[derive(Clone)]
pub struct AudioSource(Buffered<SamplesConverter<Decoder<BufReader<Cursor<Vec<u8>>>>, f32>>);

impl AudioSource {
    pub fn load<S>(vfs: &Vfs, name: S) -> Result<AudioSource, Error>
    where
        S: AsRef<str>,
    {
        let full_path = "sound/".to_owned() + name.as_ref();
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
    src: AudioSource,
    sink: Sink,
    volume: u8,
    attenuation: u8,
}

impl StaticSound {
    pub fn new(
        endpoint: &Endpoint,
        origin: Vector3<f32>,
        src: AudioSource,
        volume: u8,
        attenuation: u8,
    ) -> StaticSound {
        let mut sink = Sink::new(endpoint);
        let infinite = src.0.clone().repeat_infinite();
        sink.append(infinite);
        // TODO: set volume, attenuation and spatialize

        StaticSound {
            origin,
            src,
            sink,
            volume,
            attenuation,
        }
    }
}

/// Represents a single audio channel, capable of playing one sound at a time.
pub struct Channel {
    endpoint: Rc<Endpoint>,
    sink: RefCell<Option<Sink>>,
}

impl Channel {
    /// Create a new `Channel` backed by the given `Endpoint`.
    pub fn new(endpoint: Rc<Endpoint>) -> Channel {
        Channel {
            endpoint,
            sink: RefCell::new(None),
        }
    }

    /// Play a new sound on this channel, cutting off any sound that was previously playing.
    pub fn play(&self, src: AudioSource) {
        // stop the old sound
        self.sink.replace(None);

        // start the new sound
        let mut new_sink = Sink::new(&self.endpoint);
        new_sink.append(src.0);
        new_sink.set_volume(8.0);

        self.sink.replace(Some(new_sink));
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
