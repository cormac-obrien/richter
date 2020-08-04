use std::{
    io,
    ops::Range,
};

use crate::common::{
    net::{self, NetError},
    util::read_f32_3,
    vfs::VirtualFile,
};

use arrayvec::ArrayVec;
use byteorder::{LittleEndian, ReadBytesExt};
use cgmath::{Deg, Vector3};
use io::BufReader;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DemoServerError {
    #[error("Invalid CD track number")]
    InvalidCdTrack,
    #[error("No such CD track: {0}")]
    NoSuchCdTrack(i32),
    #[error("Message size ({0}) exceeds maximum allowed size {}", net::MAX_MESSAGE)]
    MessageTooLong(u32),
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("Network error: {0}")]
    Net(#[from] NetError),
}

struct DemoMessage {
    view_angles: Vector3<Deg<f32>>,
    msg_range: Range<usize>,
}

pub struct DemoMessageView<'a> {
    view_angles: Vector3<Deg<f32>>,
    message: &'a [u8],
}

impl<'a> DemoMessageView<'a> {
    pub fn view_angles(&self) -> Vector3<Deg<f32>> {
        self.view_angles
    }

    pub fn message(&self) -> &[u8] {
        self.message
    }
}

pub struct DemoServer {
    track_override: Option<u32>,

    // id of next message to "send"
    message_id: usize,

    messages: Vec<DemoMessage>,

    // all message data
    message_data: Vec<u8>,
}

impl DemoServer {
    pub fn new(file: &mut VirtualFile) -> Result<DemoServer, DemoServerError> {
        let mut dem_reader = BufReader::new(file);

        let mut buf = ArrayVec::<[u8; 3]>::new();
        // copy CD track number (terminated by newline) into buffer
        for i in 0..buf.capacity() {
            match dem_reader.read_u8()? {
                b'\n' => break,
                // cannot panic because we won't exceed capacity with a loop this small
                b => buf.push(b),
            }

            if i >= buf.capacity() - 1 {
                // CD track would be more than 2 digits long, which is impossible
                Err(DemoServerError::InvalidCdTrack)?;
            }
        }

        let track_override = {
            let track_str = match std::str::from_utf8(&buf) {
                Ok(s) => s,
                Err(_) => Err(DemoServerError::InvalidCdTrack)?,
            };

            match track_str {
                // if track is empty, default to track 0
                "" => Some(0),
                s => match s.parse::<i32>() {
                    Ok(track) => match track {
                        // if track is -1, allow demo to specify tracks in messages
                        -1 => None,
                        t if t < -1 => Err(DemoServerError::InvalidCdTrack)?,
                        _ => Some(track as u32),
                    },
                    Err(_) => Err(DemoServerError::InvalidCdTrack)?,
                },
            }
        };
        // TODO: verify that track exists

        let mut message_data = Vec::new();
        let mut messages = Vec::new();

        // read all messages
        while let Ok(msg_len) = dem_reader.read_u32::<LittleEndian>() {
            // get view angles
            let view_angles_f32 = read_f32_3(&mut dem_reader)?;
            let view_angles = Vector3::new(
                Deg(view_angles_f32[0]),
                Deg(view_angles_f32[1]),
                Deg(view_angles_f32[2]),
            );

            // read next message
            let msg_start = message_data.len();
            for _ in 0..msg_len {
                message_data.push(dem_reader.read_u8()?);
            }
            let msg_end = message_data.len();

            messages.push(DemoMessage {
                view_angles,
                msg_range: msg_start..msg_end,
            });
        }

        Ok(DemoServer {
            track_override,
            message_id: 0,
            messages,
            message_data,
        })
    }

    pub fn next(&mut self) -> Option<DemoMessageView> {
        if self.message_id >= self.messages.len() {
            return None;
        }

        let msg = &self.messages[self.message_id];
        self.message_id += 1;

        Some(DemoMessageView {
            view_angles: msg.view_angles,
            message: &self.message_data[msg.msg_range.clone()],
        })
    }
}
