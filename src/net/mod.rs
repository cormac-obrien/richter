// Copyright © 2017 Cormac O'Brien
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

pub mod connect;

use std::error::Error;
use std::fmt;
use std::io::BufReader;
use std::io::Cursor;
use std::mem::size_of;
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use std::net::UdpSocket;

use util;

use byteorder::NetworkEndian;
use byteorder::ReadBytesExt;
use byteorder::WriteBytesExt;
use num::FromPrimitive;

const MAX_NET_MESSAGE: usize = 8192;
const MAX_DATAGRAM: usize = 1024;
const NAME_LEN: usize = 64;
const HEADER_SIZE: usize = 8;
const DATAGRAM_SIZE: usize = HEADER_SIZE + MAX_DATAGRAM;
const PROTOCOL_VERSION: i32 = 15;

static GAME_NAME: &'static str = "QUAKE";

#[derive(Debug)]
pub enum NetError {
    Io(::std::io::Error),
    InvalidRequest(u8),
    InvalidResponse(u8),
    Other(String),
}

impl NetError {
    pub fn with_msg<S>(msg: S) -> Self
    where
        S: AsRef<str>,
    {
        NetError::Other(msg.as_ref().to_owned())
    }
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            NetError::Io(ref err) => {
                write!(f, "I/O error: ")?;
                err.fmt(f)
            }
            NetError::InvalidRequest(code) => write!(f, "Invalid request code: {:X}", code),
            NetError::InvalidResponse(code) => write!(f, "Invalid response code: {:X}", code),
            NetError::Other(ref msg) => write!(f, "{}", msg),
        }
    }
}

impl Error for NetError {
    fn description(&self) -> &str {
        match *self {
            NetError::Io(ref err) => err.description(),
            NetError::InvalidRequest(_) => "Invalid request code",
            NetError::InvalidResponse(_) => "Invalid response code",
            NetError::Other(ref msg) => &msg,
        }
    }
}

impl From<::std::io::Error> for NetError {
    fn from(error: ::std::io::Error) -> Self {
        NetError::Io(error)
    }
}


bitflags! {
    pub struct UpdateFlags: u16 {
        const MORE_BITS = 1 << 0;
        const ORIGIN_X = 1 << 1;
        const ORIGIN_Y = 1 << 2;
        const ORIGIN_Z = 1 << 3;
        const YAW = 1 << 4;
        const NO_LERP = 1 << 5;
        const FRAME = 1 << 6;
        const SIGNAL = 1 << 7;
        const PITCH = 1 << 8;
        const ROLL = 1 << 9;
        const MODEL = 1 << 10;
        const COLORMAP = 1 << 11;
        const SKIN = 1 << 12;
        const EFFECTS = 1 << 13;
        const LONG_ENTITY = 1 << 14;
    }
}

bitflags! {
    pub struct ExtendedUpdateFlags: u16 {
        const VIEW_HEIGHT = 1 << 0;
        const IDEAL_PITCH = 1 << 1;
        const PUNCH_PITCH = 1 << 2;
        const PUNCH_YAW = 1 << 3;
        const PUNCH_ROLL = 1 << 4;
        const VELOCITY_X = 1 << 5;
        const VELOCITY_Y = 1 << 6;
        const VELOCITY_Z = 1 << 7;
        // const AIM_ENT = 1 << 8; // unused
        const ITEMS = 1 << 9;
        const ON_GROUND = 1 << 10;
        const IN_WATER = 1 << 11;
        const WEAPON_FRAME = 1 << 12;
        const ARMOR = 1 << 13;
        const WEAPON = 1 << 14;
    }
}

bitflags! {
    pub struct SoundFlags: u8 {
        const VOLUME = 1 << 0;
        const ATTENUATION = 1 << 1;
        const LOOPING = 1 << 2;
    }
}

pub enum ServerCmd {
    Bad = 0,
    NoOp = 1,
    Disconnect = 2,
    UpdateStat = 3,
    Version = 4,
    SetView = 5,
    Sound = 6,
    Time = 7,
    Print = 8,
    StuffText = 9,
    SetAngle = 10,
    ServerInfo = 11,
    LightStyle = 12,
    UpdateName = 13,
    UpdateFrags = 14,
    ClientData = 15,
    StopSound = 16,
    UpdateColors = 17,
    Particle = 18,
    Damage = 19,
    SpawnStatic = 20,
    // SpawnBinary = 21, // unused
    SpawnBaseline = 22,
    TempEntity = 23,
    SetPause = 24,
    SignOnNum = 25,
    CenterPrint = 26,
    KilledMonster = 27,
    FoundSecret = 28,
    SpawnStaticSound = 29,
    Intermission = 30,
    Finale = 31,
    CdTrack = 32,
    SellScreen = 33,
    Cutscene = 34,
}

pub enum ClientCmd {
    Bad = 0,
    NoOp = 1,
    Disconnect = 2,
    Move = 3,
    StringCmd = 4,
}

pub enum TempEntity {
    Spike = 0,
    SuperSpike = 1,
    Gunshot = 2,
    Explosion = 3,
    TarExplosion = 4,
    Lightning1 = 5,
    Lightning2 = 6,
    WizSpike = 7,
    KnightSpike = 8,
    Lightning3 = 9,
    LavaSplash = 10,
    Teleport = 11,
    Explosion2 = 12,
    Beam = 13,
}

pub struct QSocket {
    socket: UdpSocket,
    remote: SocketAddr,

    ack_sequence: u32,
    send_sequence: u32,
    unreliable_send_sequence: u32,
    send_buf: [u8; MAX_NET_MESSAGE],

    recv_sequence: u32,
    unreliable_recv_sequence: u32,
    recv_buf: [u8; MAX_NET_MESSAGE],
}
