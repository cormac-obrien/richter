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

use std::error::Error;
use std::fmt;
use std::io::BufReader;
use std::io::Cursor;

use common::pak::Pak;

use cgmath::Vector3;
use rodio::Decoder;
use rodio::Endpoint;
use rodio::Sink;
use rodio::Source;
use rodio::decoder::DecoderError;
use rodio::source::Buffered;
use rodio::source::SamplesConverter;

#[derive(Debug)]
pub enum SoundError {
    Decoder(DecoderError),
    Io(::std::io::Error),
    Other(String),
}

impl SoundError {
    pub fn with_msg<S>(msg: S) -> Self
    where
        S: AsRef<str>,
    {
        SoundError::Other(msg.as_ref().to_owned())
    }
}

impl fmt::Display for SoundError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            SoundError::Decoder(ref err) => {
                write!(f, "Decoder error: ")?;
                err.fmt(f)
            }
            SoundError::Io(ref err) => {
                write!(f, "I/O error: ")?;
                err.fmt(f)
            }
            SoundError::Other(ref msg) => write!(f, "{}", msg),
        }
    }
}

impl Error for SoundError {
    fn description(&self) -> &str {
        match *self {
            SoundError::Decoder(ref err) => err.description(),
            SoundError::Io(ref err) => err.description(),
            SoundError::Other(ref msg) => &msg,
        }
    }
}

impl From<DecoderError> for SoundError {
    fn from(error: DecoderError) -> Self {
        SoundError::Decoder(error)
    }
}

impl From<::std::io::Error> for SoundError {
    fn from(error: ::std::io::Error) -> Self {
        SoundError::Io(error)
    }
}

#[derive(Clone)]
pub struct AudioSource(Buffered<SamplesConverter<Decoder<BufReader<Cursor<Vec<u8>>>>, f32>>);

impl AudioSource {
    pub fn load<S>(pak: &Pak, name: S) -> Result<AudioSource, SoundError>
    where
        S: AsRef<str>,
    {
        let full_path = "sound/".to_owned() + name.as_ref();
        let data = match pak.open(&full_path) {
            Some(d) => d.to_owned(),
            None => {
                return Err(SoundError::with_msg(
                    format!("File not found in pak archive: {}", full_path),
                ))
            }
        };

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
        StaticSound {
            origin,
            src,
            sink: Sink::new(endpoint),
            volume,
            attenuation,
        }
    }

    pub fn play(&self) {
        self.sink.append(self.src.0.clone().repeat_infinite());
        self.sink.play();
    }
}

pub struct Channel {
    sink: Sink,
}

impl Channel {
    pub fn new(endpoint: &Endpoint) -> Channel {
        Channel { sink: Sink::new(endpoint) }
    }

    pub fn play(&self, src: AudioSource) {
        // stop and remove the previous sound
        self.sink.stop();

        // add the new sound
        self.sink.append(src.0);

        // play the new sound
        self.sink.play();
    }
}
