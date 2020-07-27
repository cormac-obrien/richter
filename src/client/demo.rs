use std::{
    fs::File,
    io::{self, BufRead, Read as _},
    ops::Range,
};

use crate::common::{
    net::{self, NetError, ServerCmd},
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
    cmd_range: Range<usize>,
}

pub struct DemoMessageView<'a> {
    view_angles: Vector3<Deg<f32>>,
    commands: &'a [ServerCmd],
}

impl<'a> DemoMessageView<'a> {
    pub fn view_angles(&self) -> Vector3<Deg<f32>> {
        self.view_angles
    }

    pub fn iter_commands(&self) -> impl Iterator<Item = &ServerCmd> {
        self.commands.iter()
    }
}

pub struct DemoServer {
    track_override: Option<u32>,
    messages: Vec<DemoMessage>,
    commands: Vec<ServerCmd>,
}

impl DemoServer {
    pub fn new(file: &mut VirtualFile) -> Result<DemoServer, DemoServerError> {
        let mut dem_reader = BufReader::new(file);
        let mut buf = ArrayVec::<[u8; net::MAX_MESSAGE]>::new();

        // copy CD track number (terminated by newline) into buffer
        for i in 0..3 {
            match dem_reader.read_u8()? {
                b'\n' => break,
                // cannot panic because we won't exceed capacity with a loop this small
                b => buf.push(b),
            }

            if i > 1 {
                // CD track would be more than 2 digits long, which is impossible
                Err(DemoServerError::InvalidCdTrack)?;
            }
        }

        let track_override = {
            let track_str = match std::str::from_utf8(&buf[..buf.len() - 1]) {
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

        let mut commands = Vec::new();
        let mut messages = Vec::new();

        // read all messages
        while let Ok(msg_len) = dem_reader.read_u32::<LittleEndian>() {
            let view_angles_f32 = read_f32_3(&mut dem_reader)?;
            let view_angles = Vector3::new(
                Deg(view_angles_f32[0]),
                Deg(view_angles_f32[1]),
                Deg(view_angles_f32[2]),
            );

            // clear and read next message
            buf.clear();
            if msg_len as usize > buf.capacity() {
                Err(DemoServerError::MessageTooLong(msg_len))?;
            }
            for _ in 0..msg_len {
                // won't panic since we checked the message length against capacity
                buf.push(dem_reader.read_u8()?);
            }
            let mut msg_reader = BufReader::new(buf.as_slice());

            // read commands
            let cmd_start = commands.len();
            while let Some(cmd) = ServerCmd::deserialize(&mut msg_reader)? {
                commands.push(cmd);
            }
            let cmd_end = commands.len();

            messages.push(DemoMessage {
                view_angles,
                cmd_range: cmd_start..cmd_end,
            });
        }

        Ok(DemoServer {
            track_override,
            messages,
            commands,
        })
    }

    pub fn iter_messages(&self) -> DemoIterator {
        DemoIterator {
            message_id: 0,
            messages: &self.messages,
            commands: &self.commands,
        }
    }
}

pub struct DemoIterator<'a> {
    message_id: usize,
    messages: &'a [DemoMessage],
    commands: &'a [ServerCmd],
}

impl<'a> Iterator for DemoIterator<'a> {
    type Item = DemoMessageView<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.message_id >= self.messages.len() {
            return None;
        }

        let msg = &self.messages[self.message_id];
        Some(DemoMessageView {
            view_angles: msg.view_angles,
            commands: &self.commands[msg.cmd_range.clone()],
        })
    }
}
