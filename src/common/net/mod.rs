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

// TODO: need to figure out an equivalence relation for read_/write_coord and read_/write_angle

pub mod connect;

use std::collections::VecDeque;
use std::error::Error;
use std::fmt;
use std::io;
use std::io::BufRead;
use std::io::BufReader;
use std::io::BufWriter;
use std::io::Cursor;
use std::io::Read;
use std::io::Write;
use std::net::SocketAddr;
use std::net::UdpSocket;

use common::engine;
use common::util;

use byteorder::LittleEndian;
use byteorder::NetworkEndian;
use byteorder::ReadBytesExt;
use byteorder::WriteBytesExt;
use cgmath::Deg;
use cgmath::Vector3;
use cgmath::Zero;
use chrono::Duration;
use num::FromPrimitive;

const MAX_MESSAGE: usize = 8192;
const MAX_DATAGRAM: usize = 1024;
const HEADER_SIZE: usize = 8;
const MAX_PACKET: usize = HEADER_SIZE + MAX_DATAGRAM;

pub const PROTOCOL_VERSION: u8 = 15;

const NAME_LEN: usize = 64;

const VELOCITY_READ_FACTOR: f32 = 16.0;
const VELOCITY_WRITE_FACTOR: f32 = 1.0 / VELOCITY_READ_FACTOR;

const PARTICLE_DIRECTION_READ_FACTOR: f32 = 1.0 / 16.0;
const PARTICLE_DIRECTION_WRITE_FACTOR: f32 = 1.0 / PARTICLE_DIRECTION_READ_FACTOR;

pub static GAME_NAME: &'static str = "QUAKE";
pub const MAX_CLIENTS: usize = 16;

#[derive(Debug)]
pub enum NetError {
    Io(::std::io::Error),
    InvalidData(String),
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
            NetError::InvalidData(ref msg) => write!(f, "Invalid data: {}", msg),
            NetError::Other(ref msg) => write!(f, "{}", msg),
        }
    }
}

impl Error for NetError {
    fn description(&self) -> &str {
        match *self {
            NetError::Io(ref err) => err.description(),
            NetError::InvalidData(_) => "Invalid data",
            NetError::Other(ref msg) => &msg,
        }
    }
}

impl From<::std::io::Error> for NetError {
    fn from(error: ::std::io::Error) -> Self {
        NetError::Io(error)
    }
}

// the original engine treats these as bitflags, but all of them are mutually exclusive except for
// NETFLAG_DATA (reliable message) and NETFLAG_EOM (end of reliable message).
#[derive(Debug, Eq, FromPrimitive, PartialEq)]
pub enum MsgKind {
    Reliable = 0x0001,
    Ack = 0x0002,
    ReliableEom = 0x0009,
    Unreliable = 0x0010,
    Ctl = 0x8000,
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
    pub struct ClientUpdateFlags: u16 {
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

bitflags! {
    pub struct ItemFlags: u32 {
        const SHOTGUN          = 0x00000001;
        const SUPER_SHOTGUN    = 0x00000002;
        const NAILGUN          = 0x00000004;
        const SUPER_NAILGUN    = 0x00000008;
        const GRENADE_LAUNCHER = 0x00000010;
        const ROCKET_LAUNCHER  = 0x00000020;
        const LIGHTNING        = 0x00000040;
        const SUPER_LIGHTNING  = 0x00000080;
        const SHELLS           = 0x00000100;
        const NAILS            = 0x00000200;
        const ROCKETS          = 0x00000400;
        const CELLS            = 0x00000800;
        const AXE              = 0x00001000;
        const ARMOR_1          = 0x00002000;
        const ARMOR_2          = 0x00004000;
        const ARMOR_3          = 0x00008000;
        const SUPER_HEALTH     = 0x00010000;
        const KEY_1            = 0x00020000;
        const KEY_2            = 0x00040000;
        const INVISIBILITY     = 0x00080000;
        const INVULNERABILITY  = 0x00100000;
        const SUIT             = 0x00200000;
        const QUAD             = 0x00400000;
        const SIGIL_1          = 0x10000000;
        const SIGIL_2          = 0x20000000;
        const SIGIL_3          = 0x40000000;
        const SIGIL_4          = 0x80000000;
    }
}

bitflags! {
    pub struct ButtonFlags: u8 {
        const ATTACK = 0x01;
        const JUMP = 0x02;
    }
}

pub struct PlayerColor {
    top: u8,
    bottom: u8,
}

impl PlayerColor {
    pub fn new(top: u8, bottom: u8) -> PlayerColor {
        if top > 15 {
            warn!("Top color index ({}) will be truncated", top);
        }

        if bottom > 15 {
            warn!("Bottom color index ({}) will be truncated", bottom);
        }

        PlayerColor { top, bottom }
    }

    pub fn bits(&self) -> u8 {
        self.top << 4 | (self.bottom & 0x0F)
    }
}

impl ::std::convert::From<u8> for PlayerColor {
    fn from(src: u8) -> PlayerColor {
        PlayerColor {
            top: src << 4,
            bottom: src & 0x0F,
        }
    }
}

pub struct ColorShift {
    pub dest_color: [u8; 3],
    pub percent: u8,
}

#[derive(FromPrimitive)]
pub enum IntermissionKind {
    None = 0,
    Intermission = 1,
    Finale = 2,
    Cutscene = 3,
}

#[derive(Copy, Clone, Debug, Eq, FromPrimitive, PartialEq)]
pub enum ClientStat {
    Health = 0,
    Frags = 1,
    Weapon = 2,
    Ammo = 3,
    Armor = 4,
    WeaponFrame = 5,
    Shells = 6,
    Nails = 7,
    Rockets = 8,
    Cells = 9,
    ActiveWeapon = 10,
    TotalSecrets = 11,
    TotalMonsters = 12,
    FoundSecrets = 13,
    KilledMonsters = 14,
}

/// Numeric codes used to identify the type of a temporary entity.
#[derive(Debug, Eq, FromPrimitive, PartialEq)]
pub enum TempEntityCode {
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

/// Information used to initialize a temporary entity that exists at a single point in space.
#[derive(Debug, PartialEq)]
pub struct TempEntityPoint {
    origin: Vector3<f32>,
}

impl TempEntityPoint {
    fn deserialize<R>(reader: &mut R) -> Result<TempEntityPoint, NetError>
    where
        R: BufRead + ReadBytesExt,
    {
        let mut origin = Vector3::zero();
        for i in 0..3 {
            origin[i] = read_coord(reader)?;
        }

        Ok(TempEntityPoint { origin })
    }

    fn serialize<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        for i in 0..3 {
            write_coord(writer, self.origin[i])?;
        }

        Ok(())
    }
}

/// Information used to initialize a temporary entity that spans a line segment.
#[derive(Debug, PartialEq)]
pub struct TempEntityBeam {
    entity_id: u16,
    start: Vector3<f32>,
    end: Vector3<f32>,
}

impl TempEntityBeam {
    fn deserialize<R>(reader: &mut R) -> Result<TempEntityBeam, NetError>
    where
        R: BufRead + ReadBytesExt,
    {
        let entity_id = reader.read_u16::<LittleEndian>()?;

        let mut start = Vector3::zero();
        for i in 0..3 {
            start[i] = read_coord(reader)?;
        }

        let mut end = Vector3::zero();
        for i in 0..3 {
            end[i] = read_coord(reader)?;
        }

        Ok(TempEntityBeam {
            entity_id,
            start,
            end,
        })
    }

    fn serialize<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        for i in 0..3 {
            write_coord(writer, self.start[i])?;
        }

        for i in 0..3 {
            write_coord(writer, self.end[i])?;
        }

        Ok(())
    }
}

/// Information used to initialize a temporary entity representing a color-mapped explosion.
#[derive(Debug, PartialEq)]
pub struct TempEntityColorExplosion {
    origin: Vector3<f32>,
    color_start: u8,
    color_len: u8,
}

impl TempEntityColorExplosion {
    fn deserialize<R>(reader: &mut R) -> Result<TempEntityColorExplosion, NetError>
    where
        R: BufRead + ReadBytesExt,
    {
        let mut origin = Vector3::zero();
        for i in 0..3 {
            origin[i] = read_coord(reader)?;
        }

        let color_start = reader.read_u8()?;
        let color_len = reader.read_u8()?;

        Ok(TempEntityColorExplosion {
            origin,
            color_start,
            color_len,
        })
    }

    fn serialize<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        for i in 0..3 {
            write_coord(writer, self.origin[i])?;
        }

        writer.write_u8(self.color_start)?;
        writer.write_u8(self.color_len)?;

        Ok(())
    }
}

#[derive(Debug, PartialEq)]
pub enum TempEntity {
    Spike(TempEntityPoint),
    SuperSpike(TempEntityPoint),
    Gunshot(TempEntityPoint),
    Explosion(TempEntityPoint),
    TarExplosion(TempEntityPoint),
    Lightning1(TempEntityBeam),
    Lightning2(TempEntityBeam),
    WizSpike(TempEntityPoint),
    KnightSpike(TempEntityPoint),
    Lightning3(TempEntityBeam),
    LavaSplash(TempEntityPoint),
    Teleport(TempEntityPoint),
    Explosion2(TempEntityColorExplosion),
    Beam(TempEntityBeam),
}

impl TempEntity {
    pub fn read_temp_entity<R>(reader: &mut R) -> Result<TempEntity, NetError>
    where
        R: BufRead + ReadBytesExt,
    {
        let code_byte = reader.read_u8()?;
        let code = match TempEntityCode::from_u8(code_byte) {
            Some(c) => c,
            None => {
                return Err(NetError::InvalidData(
                    format!("Temp entity code {}", code_byte),
                ))
            }
        };

        Ok(match code {
            TempEntityCode::Spike => TempEntity::Spike(TempEntityPoint::deserialize(reader)?),
            TempEntityCode::SuperSpike => {
                TempEntity::SuperSpike(TempEntityPoint::deserialize(reader)?)
            }
            TempEntityCode::Gunshot => TempEntity::Gunshot(TempEntityPoint::deserialize(reader)?),
            TempEntityCode::Explosion => {
                TempEntity::Explosion(TempEntityPoint::deserialize(reader)?)
            }
            TempEntityCode::TarExplosion => {
                TempEntity::TarExplosion(TempEntityPoint::deserialize(reader)?)
            }
            TempEntityCode::Lightning1 => {
                TempEntity::Lightning1(TempEntityBeam::deserialize(reader)?)
            }
            TempEntityCode::Lightning2 => {
                TempEntity::Lightning2(TempEntityBeam::deserialize(reader)?)
            }
            TempEntityCode::WizSpike => TempEntity::WizSpike(TempEntityPoint::deserialize(reader)?),
            TempEntityCode::KnightSpike => {
                TempEntity::KnightSpike(TempEntityPoint::deserialize(reader)?)
            }
            TempEntityCode::Lightning3 => {
                TempEntity::Lightning3(TempEntityBeam::deserialize(reader)?)
            }
            TempEntityCode::LavaSplash => {
                TempEntity::LavaSplash(TempEntityPoint::deserialize(reader)?)
            }
            TempEntityCode::Teleport => TempEntity::Teleport(TempEntityPoint::deserialize(reader)?),
            TempEntityCode::Explosion2 => {
                TempEntity::Explosion2(TempEntityColorExplosion::deserialize(reader)?)
            }
            TempEntityCode::Beam => TempEntity::Beam(TempEntityBeam::deserialize(reader)?),
        })
    }

    pub fn write_temp_entity<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        match *self {
            TempEntity::Spike(ref point) => {
                writer.write_u8(TempEntityCode::Spike as u8)?;
                point.serialize(writer)?;
            }
            TempEntity::SuperSpike(ref point) => {
                writer.write_u8(TempEntityCode::SuperSpike as u8)?;
                point.serialize(writer)?;
            }
            TempEntity::Gunshot(ref point) => {
                writer.write_u8(TempEntityCode::Gunshot as u8)?;
                point.serialize(writer)?;
            }
            TempEntity::Explosion(ref point) => {
                writer.write_u8(TempEntityCode::Explosion as u8)?;
                point.serialize(writer)?;
            }
            TempEntity::TarExplosion(ref point) => {
                writer.write_u8(TempEntityCode::TarExplosion as u8)?;
                point.serialize(writer)?;
            }
            TempEntity::Lightning1(ref beam) => {
                writer.write_u8(TempEntityCode::Lightning1 as u8)?;
                beam.serialize(writer)?;
            }
            TempEntity::Lightning2(ref beam) => {
                writer.write_u8(TempEntityCode::Lightning2 as u8)?;
                beam.serialize(writer)?;
            }
            TempEntity::WizSpike(ref point) => {
                writer.write_u8(TempEntityCode::WizSpike as u8)?;
                point.serialize(writer)?;
            }
            TempEntity::KnightSpike(ref point) => {
                writer.write_u8(TempEntityCode::KnightSpike as u8)?;
                point.serialize(writer)?;
            }
            TempEntity::Lightning3(ref beam) => {
                writer.write_u8(TempEntityCode::Lightning3 as u8)?;
                beam.serialize(writer)?;
            }
            TempEntity::LavaSplash(ref point) => {
                writer.write_u8(TempEntityCode::LavaSplash as u8)?;
                point.serialize(writer)?;
            }
            TempEntity::Teleport(ref point) => {
                writer.write_u8(TempEntityCode::Teleport as u8)?;
                point.serialize(writer)?;
            }
            TempEntity::Explosion2(ref expl) => {
                writer.write_u8(TempEntityCode::Explosion2 as u8)?;
                expl.serialize(writer)?;
            }
            TempEntity::Beam(ref beam) => {
                writer.write_u8(TempEntityCode::Beam as u8)?;
                beam.serialize(writer)?;
            }
        }

        Ok(())
    }
}

#[derive(Copy, Clone, Ord, Debug, Eq, FromPrimitive, PartialOrd, PartialEq)]
pub enum SignOnStage {
    Not = 0,
    Prespawn = 1,
    ClientInfo = 2,
    Begin = 3,
    Done = 4,
}

bitflags! {
    pub struct EntityEffects: u16 {
        const BRIGHT_FIELD = 0b0001;
        const MUZZLE_FLASH = 0b0010;
        const BRIGHT_LIGHT = 0b0100;
        const DIM_LIGHT    = 0b1000;
    }
}

#[derive(Debug)]
pub struct EntityState {
    pub origin: Vector3<f32>,
    pub angles: Vector3<Deg<f32>>,
    pub model_id: usize,
    pub frame_id: usize,

    // TODO: more specific types for these
    pub colormap: u8,
    pub skin_id: u8,
    pub effects: EntityEffects,
}

impl EntityState {
    pub fn uninitialized() -> EntityState {
        EntityState {
            origin: Vector3::new(0.0, 0.0, 0.0),
            angles: Vector3::new(Deg(0.0), Deg(0.0), Deg(0.0)),
            model_id: 0,
            frame_id: 0,
            colormap: 0,
            skin_id: 0,
            effects: EntityEffects::empty(),
        }
    }
}

/// A trait for in-game server and client network commands.
pub trait Cmd: Sized {
    /// Returns the numeric value of this command's code.
    fn code(&self) -> u8;

    /// Reads data from the given source and constructs a command object.
    fn deserialize<R>(reader: &mut R) -> Result<Self, NetError>
    where
        R: BufRead + ReadBytesExt;

    /// Writes this command's content to the given sink.
    fn serialize<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt;
}

#[derive(Debug, FromPrimitive)]
pub enum ServerCmdCode {
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
    SignOnStage = 25,
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

#[derive(Copy, Clone, Debug, Eq, FromPrimitive, PartialEq)]
pub enum GameType {
    CoOp = 0,
    Deathmatch = 1,
}

#[derive(Debug, PartialEq)]
pub enum ServerCmd {
    Bad,
    NoOp,
    Disconnect,
    UpdateStat { stat: ClientStat, value: i32 },
    Version { version: i32 },
    SetView { ent_id: i16 },
    Sound {
        volume: Option<u8>,
        attenuation: Option<u8>,
        entity_id: u16,
        channel: u8,
        sound_id: u8,
        position: Vector3<f32>,
    },
    Time { time: f32 },
    Print { text: String },
    StuffText { text: String },
    SetAngle { angles: Vector3<Deg<f32>> },
    ServerInfo {
        protocol_version: i32,
        max_clients: u8,
        game_type: GameType,
        message: String,
        model_precache: Vec<String>,
        sound_precache: Vec<String>,
    },
    LightStyle { id: u8, value: String },
    UpdateName { player_id: u8, new_name: String },
    UpdateFrags { player_id: u8, new_frags: i16 },
    ClientData {
        view_height: Option<f32>,
        ideal_pitch: Option<Deg<f32>>,
        punch_pitch: Option<Deg<f32>>,
        velocity_x: Option<f32>,
        punch_yaw: Option<Deg<f32>>,
        velocity_y: Option<f32>,
        punch_roll: Option<Deg<f32>>,
        velocity_z: Option<f32>,
        items: i32,
        on_ground: bool,
        in_water: bool,
        weapon_frame: Option<u8>,
        armor: Option<u8>,
        weapon: Option<u8>,
        health: i16,
        ammo: u8,
        ammo_shells: u8,
        ammo_nails: u8,
        ammo_rockets: u8,
        ammo_cells: u8,
        active_weapon: u8,
    },
    StopSound { entity_id: u16, channel: u8 },
    UpdateColors { player_id: u8, colors: u8 },
    Particle {
        origin: Vector3<f32>,
        direction: Vector3<f32>,
        count: u8,
        color: u8,
    },
    Damage {
        armor: u8,
        blood: u8,
        source: Vector3<f32>,
    },
    SpawnStatic {
        model_id: u8,
        frame_id: u8,
        colormap: u8,
        skin_id: u8,
        origin: Vector3<f32>,
        angles: Vector3<Deg<f32>>,
    },
    // SpawnBinary, // unused
    SpawnBaseline {
        ent_id: u16,
        model_id: u8,
        frame_id: u8,
        colormap: u8,
        skin_id: u8,
        origin: Vector3<f32>,
        angles: Vector3<Deg<f32>>,
    },
    TempEntity { temp_entity: TempEntity },
    SetPause { paused: bool },
    SignOnStage { stage: SignOnStage },
    CenterPrint { text: String },
    KilledMonster,
    FoundSecret,
    SpawnStaticSound {
        origin: Vector3<f32>,
        sound_id: u8,
        volume: u8,
        attenuation: u8,
    },
    Intermission,
    Finale { text: String },
    CdTrack { track: u8, loop_: u8 },
    SellScreen,
    Cutscene { text: String },
}

impl ServerCmd {
    pub fn code(&self) -> u8 {
        let code = match *self {
            ServerCmd::Bad => ServerCmdCode::Bad,
            ServerCmd::NoOp => ServerCmdCode::NoOp,
            ServerCmd::Disconnect => ServerCmdCode::Disconnect,
            ServerCmd::UpdateStat { .. } => ServerCmdCode::UpdateStat,
            ServerCmd::Version { .. } => ServerCmdCode::Version,
            ServerCmd::SetView { .. } => ServerCmdCode::SetView,
            ServerCmd::Sound { .. } => ServerCmdCode::Sound,
            ServerCmd::Time { .. } => ServerCmdCode::Time,
            ServerCmd::Print { .. } => ServerCmdCode::Print,
            ServerCmd::StuffText { .. } => ServerCmdCode::StuffText,
            ServerCmd::SetAngle { .. } => ServerCmdCode::SetAngle,
            ServerCmd::ServerInfo { .. } => ServerCmdCode::ServerInfo,
            ServerCmd::LightStyle { .. } => ServerCmdCode::LightStyle,
            ServerCmd::UpdateName { .. } => ServerCmdCode::UpdateName,
            ServerCmd::UpdateFrags { .. } => ServerCmdCode::UpdateFrags,
            ServerCmd::ClientData { .. } => ServerCmdCode::ClientData,
            ServerCmd::StopSound { .. } => ServerCmdCode::StopSound,
            ServerCmd::UpdateColors { .. } => ServerCmdCode::UpdateColors,
            ServerCmd::Particle { .. } => ServerCmdCode::Particle,
            ServerCmd::Damage { .. } => ServerCmdCode::Damage,
            ServerCmd::SpawnStatic { .. } => ServerCmdCode::SpawnStatic,
            ServerCmd::SpawnBaseline { .. } => ServerCmdCode::SpawnBaseline,
            ServerCmd::TempEntity { .. } => ServerCmdCode::TempEntity,
            ServerCmd::SetPause { .. } => ServerCmdCode::SetPause,
            ServerCmd::SignOnStage { .. } => ServerCmdCode::SignOnStage,
            ServerCmd::CenterPrint { .. } => ServerCmdCode::CenterPrint,
            ServerCmd::KilledMonster => ServerCmdCode::KilledMonster,
            ServerCmd::FoundSecret => ServerCmdCode::FoundSecret,
            ServerCmd::SpawnStaticSound { .. } => ServerCmdCode::SpawnStaticSound,
            ServerCmd::Intermission => ServerCmdCode::Intermission,
            ServerCmd::Finale { .. } => ServerCmdCode::Finale,
            ServerCmd::CdTrack { .. } => ServerCmdCode::CdTrack,
            ServerCmd::SellScreen => ServerCmdCode::SellScreen,
            ServerCmd::Cutscene { .. } => ServerCmdCode::Cutscene,
        };

        code as u8
    }

    pub fn deserialize<R>(reader: &mut R) -> Result<Option<ServerCmd>, NetError>
    where
        R: BufRead + ReadBytesExt,
    {
        let code_num = match reader.read_u8() {
            Ok(c) => c,
            Err(ref e) if e.kind() == ::std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(NetError::from(e)),
        };

        if code_num & 0x80 != 0 {
            panic!("fast update handling not implemented");
        }

        let code = match ServerCmdCode::from_u8(code_num) {
            Some(c) => c,
            None => {
                return Err(NetError::InvalidData(
                    format!("Invalid server command code: {}", code_num),
                ))
            }
        };

        let cmd = match code {
            ServerCmdCode::Bad => ServerCmd::Bad,
            ServerCmdCode::NoOp => ServerCmd::NoOp,
            ServerCmdCode::Disconnect => ServerCmd::Disconnect,

            ServerCmdCode::UpdateStat => {
                let stat_id = reader.read_u8()?;
                let stat = match ClientStat::from_u8(stat_id) {
                    Some(c) => c,
                    None => {
                        return Err(NetError::InvalidData(format!(
                            "value for ClientStat: {}",
                            stat_id,
                        )))
                    }
                };
                let value = reader.read_i32::<LittleEndian>()?;

                ServerCmd::UpdateStat { stat, value }
            }

            ServerCmdCode::Version => {
                let version = reader.read_i32::<LittleEndian>()?;
                ServerCmd::Version { version }
            }

            ServerCmdCode::SetView => {
                let ent_id = reader.read_i16::<LittleEndian>()?;
                ServerCmd::SetView { ent_id }
            }

            ServerCmdCode::Sound => {
                let flags_bits = reader.read_u8()?;
                let flags = match SoundFlags::from_bits(flags_bits) {
                    Some(f) => f,
                    None => {
                        return Err(NetError::InvalidData(
                            format!("SoundFlags: {:b}", flags_bits),
                        ))
                    }
                };

                let volume = match flags.contains(SoundFlags::VOLUME) {
                    true => Some(reader.read_u8()?),
                    false => None,
                };

                let attenuation = match flags.contains(SoundFlags::ATTENUATION) {
                    true => Some(reader.read_u8()?),
                    false => None,
                };

                let entity_channel = reader.read_i16::<LittleEndian>()?;
                let entity_id = (entity_channel >> 3) as u16;
                let channel = (entity_channel & 0b111) as u8;
                let sound_id = reader.read_u8()?;
                let position = Vector3::new(
                    read_coord(reader)?,
                    read_coord(reader)?,
                    read_coord(reader)?,
                );

                ServerCmd::Sound {
                    volume,
                    attenuation,
                    entity_id,
                    channel,
                    sound_id,
                    position,
                }
            }

            ServerCmdCode::Time => {
                let time = reader.read_f32::<LittleEndian>()?;
                ServerCmd::Time { time }
            }

            ServerCmdCode::Print => {
                let text = match util::read_cstring(reader) {
                    Ok(t) => t,
                    Err(e) => return Err(NetError::with_msg(format!("{}", e))),
                };

                ServerCmd::Print { text }
            }

            ServerCmdCode::StuffText => {
                let text = match util::read_cstring(reader) {
                    Ok(t) => t,
                    Err(e) => return Err(NetError::with_msg(format!("{}", e))),
                };

                ServerCmd::StuffText { text }
            }

            ServerCmdCode::SetAngle => {
                let angles = Vector3::new(
                    read_angle(reader)?,
                    read_angle(reader)?,
                    read_angle(reader)?,
                );

                ServerCmd::SetAngle { angles }
            }

            ServerCmdCode::ServerInfo => {
                let protocol_version = reader.read_i32::<LittleEndian>()?;
                let max_clients = reader.read_u8()?;
                let game_type_code = reader.read_u8()?;
                let game_type = match GameType::from_u8(game_type_code) {
                    Some(g) => g,
                    None => {
                        return Err(NetError::InvalidData(
                            format!("Invalid game type ({})", game_type_code),
                        ))
                    }
                };

                let message = util::read_cstring(reader).unwrap();

                let mut model_precache = Vec::new();
                loop {
                    let model_name = util::read_cstring(reader).unwrap();
                    if model_name.is_empty() {
                        break;
                    }
                    model_precache.push(model_name);
                }

                let mut sound_precache = Vec::new();
                loop {
                    let sound_name = util::read_cstring(reader).unwrap();
                    if sound_name.is_empty() {
                        break;
                    }
                    sound_precache.push(sound_name);
                }

                ServerCmd::ServerInfo {
                    protocol_version,
                    max_clients,
                    game_type,
                    message,
                    model_precache,
                    sound_precache,
                }
            }

            ServerCmdCode::LightStyle => {
                let id = reader.read_u8()?;
                let value = util::read_cstring(reader).unwrap();
                ServerCmd::LightStyle { id, value }
            }

            ServerCmdCode::UpdateName => {
                let player_id = reader.read_u8()?;
                let new_name = util::read_cstring(reader).unwrap();
                ServerCmd::UpdateName {
                    player_id,
                    new_name,
                }
            }

            ServerCmdCode::UpdateFrags => {
                let player_id = reader.read_u8()?;
                let new_frags = reader.read_i16::<LittleEndian>()?;

                ServerCmd::UpdateFrags {
                    player_id,
                    new_frags,
                }
            }

            ServerCmdCode::ClientData => {
                let flags_bits = reader.read_u16::<LittleEndian>()?;
                let flags = match ClientUpdateFlags::from_bits(flags_bits) {
                    Some(f) => f,
                    None => {
                        return Err(NetError::InvalidData(
                            format!("client update flags: {:b}", flags_bits),
                        ))
                    }
                };

                let view_height = match flags.contains(ClientUpdateFlags::VIEW_HEIGHT) {
                    true => Some(reader.read_i8()? as f32),
                    false => None,
                };

                let ideal_pitch = match flags.contains(ClientUpdateFlags::IDEAL_PITCH) {
                    true => Some(Deg(reader.read_i8()? as f32)),
                    false => None,
                };

                let punch_pitch = match flags.contains(ClientUpdateFlags::PUNCH_PITCH) {
                    true => Some(Deg(reader.read_i8()? as f32)),
                    false => None,
                };

                let velocity_x = match flags.contains(ClientUpdateFlags::VELOCITY_X) {
                    true => Some(reader.read_i8()? as f32 * VELOCITY_READ_FACTOR),
                    false => None,
                };

                let punch_yaw = match flags.contains(ClientUpdateFlags::PUNCH_YAW) {
                    true => Some(Deg(reader.read_i8()? as f32)),
                    false => None,
                };

                let velocity_y = match flags.contains(ClientUpdateFlags::VELOCITY_Y) {
                    true => Some(reader.read_i8()? as f32 * VELOCITY_READ_FACTOR),
                    false => None,
                };

                let punch_roll = match flags.contains(ClientUpdateFlags::PUNCH_ROLL) {
                    true => Some(Deg(reader.read_i8()? as f32)),
                    false => None,
                };

                let velocity_z = match flags.contains(ClientUpdateFlags::VELOCITY_Z) {
                    true => Some(reader.read_i8()? as f32 * VELOCITY_READ_FACTOR),
                    false => None,
                };

                let items = reader.read_i32::<LittleEndian>()?;
                let on_ground = flags.contains(ClientUpdateFlags::ON_GROUND);
                let in_water = flags.contains(ClientUpdateFlags::IN_WATER);

                let weapon_frame = match flags.contains(ClientUpdateFlags::WEAPON_FRAME) {
                    true => Some(reader.read_u8()?),
                    false => None,
                };

                let armor = match flags.contains(ClientUpdateFlags::ARMOR) {
                    true => Some(reader.read_u8()?),
                    false => None,
                };

                let weapon = match flags.contains(ClientUpdateFlags::WEAPON) {
                    true => Some(reader.read_u8()?),
                    false => None,
                };

                let health = reader.read_i16::<LittleEndian>()?;
                let ammo = reader.read_u8()?;
                let ammo_shells = reader.read_u8()?;
                let ammo_nails = reader.read_u8()?;
                let ammo_rockets = reader.read_u8()?;
                let ammo_cells = reader.read_u8()?;
                let active_weapon = reader.read_u8()?;

                ServerCmd::ClientData {
                    view_height,
                    ideal_pitch,
                    punch_pitch,
                    velocity_x,
                    punch_yaw,
                    velocity_y,
                    punch_roll,
                    velocity_z,
                    items,
                    on_ground,
                    in_water,
                    weapon_frame,
                    armor,
                    weapon,
                    health,
                    ammo,
                    ammo_shells,
                    ammo_nails,
                    ammo_rockets,
                    ammo_cells,
                    active_weapon,
                }
            }

            ServerCmdCode::StopSound => {
                let entity_channel = reader.read_u16::<LittleEndian>()?;
                let entity_id = entity_channel >> 3;
                let channel = (entity_channel & 0b111) as u8;

                ServerCmd::StopSound { entity_id, channel }
            }

            ServerCmdCode::UpdateColors => {
                let player_id = reader.read_u8()?;
                let colors = reader.read_u8()?;

                ServerCmd::UpdateColors { player_id, colors }
            }

            ServerCmdCode::Particle => {
                let mut origin = Vector3::zero();
                for i in 0..3 {
                    origin[i] = read_coord(reader)?;
                }

                let mut direction = Vector3::zero();
                for i in 0..3 {
                    direction[i] = reader.read_i8()? as f32 * PARTICLE_DIRECTION_READ_FACTOR;
                }

                let count = reader.read_u8()?;
                let color = reader.read_u8()?;

                ServerCmd::Particle {
                    origin,
                    direction,
                    count,
                    color,
                }
            }

            ServerCmdCode::Damage => {
                let armor = reader.read_u8()?;
                let blood = reader.read_u8()?;
                let mut source = Vector3::zero();

                for i in 0..3 {
                    source[i] = read_coord(reader)?;
                }

                ServerCmd::Damage {
                    armor,
                    blood,
                    source,
                }
            }

            ServerCmdCode::SpawnStatic => {
                let model_id = reader.read_u8()?;
                let frame_id = reader.read_u8()?;
                let colormap = reader.read_u8()?;
                let skin_id = reader.read_u8()?;

                let mut origin = Vector3::zero();
                let mut angles = Vector3::new(Deg(0.0), Deg(0.0), Deg(0.0));
                for i in 0..3 {
                    origin[i] = read_coord(reader)?;
                    angles[i] = read_angle(reader)?;
                }

                ServerCmd::SpawnStatic {
                    model_id,
                    frame_id,
                    colormap,
                    skin_id,
                    origin,
                    angles,
                }
            }

            ServerCmdCode::SpawnBaseline => {
                let ent_id = reader.read_u16::<LittleEndian>()?;
                let model_id = reader.read_u8()?;
                let frame_id = reader.read_u8()?;
                let colormap = reader.read_u8()?;
                let skin_id = reader.read_u8()?;

                let mut origin = Vector3::zero();
                let mut angles = Vector3::new(Deg(0.0), Deg(0.0), Deg(0.0));
                for i in 0..3 {
                    origin[i] = read_coord(reader)?;
                    angles[i] = read_angle(reader)?;
                }

                ServerCmd::SpawnBaseline {
                    ent_id,
                    model_id,
                    frame_id,
                    colormap,
                    skin_id,
                    origin,
                    angles,
                }
            }

            ServerCmdCode::TempEntity => {
                let temp_entity = TempEntity::read_temp_entity(reader)?;

                ServerCmd::TempEntity { temp_entity }
            }

            ServerCmdCode::SetPause => {
                let paused = match reader.read_u8()? {
                    0 => false,
                    1 => true,
                    x => return Err(NetError::InvalidData(format!("setpause: {}", x))),
                };

                ServerCmd::SetPause { paused }
            }

            ServerCmdCode::SignOnStage => {
                let stage_num = reader.read_u8()?;
                let stage = match SignOnStage::from_u8(stage_num) {
                    Some(s) => s,
                    None => {
                        return Err(NetError::InvalidData(
                            format!("Invalid value for sign-on stage: {}", stage_num),
                        ))
                    }
                };

                ServerCmd::SignOnStage { stage }
            }

            ServerCmdCode::CenterPrint => {
                let text = match util::read_cstring(reader) {
                    Ok(t) => t,
                    Err(e) => return Err(NetError::with_msg(format!("{}", e))),
                };

                ServerCmd::CenterPrint { text }
            }

            ServerCmdCode::KilledMonster => ServerCmd::KilledMonster,
            ServerCmdCode::FoundSecret => ServerCmd::FoundSecret,

            ServerCmdCode::SpawnStaticSound => {
                let mut origin = Vector3::zero();
                for i in 0..3 {
                    origin[i] = read_coord(reader)?;
                }

                let sound_id = reader.read_u8()?;
                let volume = reader.read_u8()?;
                let attenuation = reader.read_u8()?;

                ServerCmd::SpawnStaticSound {
                    origin,
                    sound_id,
                    volume,
                    attenuation,
                }
            }

            ServerCmdCode::Intermission => ServerCmd::Intermission,

            ServerCmdCode::Finale => {
                let text = match util::read_cstring(reader) {
                    Ok(t) => t,
                    Err(e) => return Err(NetError::with_msg(format!("{}", e))),
                };

                ServerCmd::Finale { text }
            }

            ServerCmdCode::CdTrack => {
                let track = reader.read_u8()?;
                let loop_ = reader.read_u8()?;
                ServerCmd::CdTrack { track, loop_ }
            }

            ServerCmdCode::SellScreen => ServerCmd::SellScreen,

            ServerCmdCode::Cutscene => {
                let text = match util::read_cstring(reader) {
                    Ok(t) => t,
                    Err(e) => return Err(NetError::with_msg(format!("{}", e))),
                };

                ServerCmd::Cutscene { text }
            }
        };

        Ok(Some(cmd))
    }

    pub fn serialize<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        writer.write_u8(self.code())?;

        match *self {
            ServerCmd::Bad | ServerCmd::NoOp | ServerCmd::Disconnect => (),

            ServerCmd::UpdateStat { stat, value } => {
                writer.write_u8(stat as u8)?;
                writer.write_i32::<LittleEndian>(value)?;
            }

            ServerCmd::Version { version } => {
                writer.write_i32::<LittleEndian>(version)?;
            }

            ServerCmd::SetView { ent_id } => {
                writer.write_i16::<LittleEndian>(ent_id)?;
            }

            ServerCmd::Sound {
                volume,
                attenuation,
                entity_id,
                channel,
                sound_id,
                position,
            } => {
                let mut sound_flags = SoundFlags::empty();

                if volume.is_some() {
                    sound_flags |= SoundFlags::VOLUME;
                }

                if attenuation.is_some() {
                    sound_flags |= SoundFlags::ATTENUATION;
                }

                writer.write_u8(sound_flags.bits())?;

                if let Some(v) = volume {
                    writer.write_u8(v)?;
                }

                if let Some(a) = attenuation {
                    writer.write_u8(a)?;
                }

                // TODO: document this better. The entity and channel fields are combined in Sound commands.
                let ent_channel = (entity_id as i16) << 3 | channel as i16 & 0b111;
                writer.write_i16::<LittleEndian>(ent_channel)?;

                writer.write_u8(sound_id)?;

                for component in 0..3 {
                    write_coord(writer, position[component])?;
                }
            }

            ServerCmd::Time { time } => {
                writer.write_f32::<LittleEndian>(time)?;
            }

            ServerCmd::Print { ref text } => {
                writer.write(text.as_bytes())?;
                writer.write_u8(0)?;
            }

            ServerCmd::StuffText { ref text } => {
                writer.write(text.as_bytes())?;
                writer.write_u8(0)?;
            }

            ServerCmd::SetAngle { angles } => {
                for i in 0..3 {
                    write_angle(writer, angles[i])?;
                }
            }

            ServerCmd::ServerInfo {
                protocol_version,
                max_clients,
                game_type,
                ref message,
                ref model_precache,
                ref sound_precache,
            } => {
                writer.write_i32::<LittleEndian>(protocol_version)?;
                writer.write_u8(max_clients)?;
                writer.write_u8(game_type as u8)?;

                writer.write(message.as_bytes())?;
                writer.write_u8(0)?;

                for model_name in model_precache.iter() {
                    writer.write(model_name.as_bytes())?;
                    writer.write_u8(0)?;
                }
                writer.write_u8(0)?;

                for sound_name in sound_precache.iter() {
                    writer.write(sound_name.as_bytes())?;
                    writer.write_u8(0)?;
                }
                writer.write_u8(0)?;
            }

            ServerCmd::LightStyle { id, ref value } => {
                writer.write_u8(id)?;
                writer.write(value.as_bytes())?;
                writer.write_u8(0)?;
            }

            ServerCmd::UpdateName {
                player_id,
                ref new_name,
            } => {
                writer.write_u8(player_id)?;
                writer.write(new_name.as_bytes())?;
                writer.write_u8(0)?;
            }

            ServerCmd::UpdateFrags {
                player_id,
                new_frags,
            } => {
                writer.write_u8(player_id)?;
                writer.write_i16::<LittleEndian>(new_frags)?;
            }

            ServerCmd::ClientData {
                view_height,
                ideal_pitch,
                punch_pitch,
                velocity_x,
                punch_yaw,
                velocity_y,
                punch_roll,
                velocity_z,
                items,
                on_ground,
                in_water,
                weapon_frame,
                armor,
                weapon,
                health,
                ammo,
                ammo_shells,
                ammo_nails,
                ammo_rockets,
                ammo_cells,
                active_weapon,
            } => {
                let mut flags = ClientUpdateFlags::empty();
                if view_height.is_some() {
                    flags |= ClientUpdateFlags::VIEW_HEIGHT;
                }
                if ideal_pitch.is_some() {
                    flags |= ClientUpdateFlags::IDEAL_PITCH;
                }
                if punch_pitch.is_some() {
                    flags |= ClientUpdateFlags::PUNCH_PITCH;
                }
                if velocity_x.is_some() {
                    flags |= ClientUpdateFlags::VELOCITY_X;
                }
                if punch_yaw.is_some() {
                    flags |= ClientUpdateFlags::PUNCH_YAW;
                }
                if velocity_y.is_some() {
                    flags |= ClientUpdateFlags::VELOCITY_Y;
                }
                if punch_roll.is_some() {
                    flags |= ClientUpdateFlags::PUNCH_ROLL;
                }
                if velocity_z.is_some() {
                    flags |= ClientUpdateFlags::VELOCITY_Z;
                }

                // items are always sent
                flags |= ClientUpdateFlags::ITEMS;

                if on_ground {
                    flags |= ClientUpdateFlags::ON_GROUND;
                }
                if in_water {
                    flags |= ClientUpdateFlags::IN_WATER;
                }
                if weapon_frame.is_some() {
                    flags |= ClientUpdateFlags::WEAPON_FRAME;
                }
                if armor.is_some() {
                    flags |= ClientUpdateFlags::ARMOR;
                }
                if weapon.is_some() {
                    flags |= ClientUpdateFlags::WEAPON;
                }

                // write flags
                writer.write_u16::<LittleEndian>(flags.bits())?;

                if let Some(vh) = view_height {
                    writer.write_u8(vh as i32 as u8)?;
                }
                if let Some(ip) = ideal_pitch {
                    writer.write_u8(ip.0 as i32 as u8)?;
                }
                if let Some(pp) = punch_pitch {
                    writer.write_u8(pp.0 as i32 as u8)?;
                }
                if let Some(vx) = velocity_x {
                    writer.write_u8((vx * VELOCITY_WRITE_FACTOR) as i32 as u8)?;
                }
                if let Some(py) = punch_yaw {
                    writer.write_u8(py.0 as i32 as u8)?;
                }
                if let Some(vy) = velocity_y {
                    writer.write_u8((vy * VELOCITY_WRITE_FACTOR) as i32 as u8)?;
                }
                if let Some(pr) = punch_roll {
                    writer.write_u8(pr.0 as i32 as u8)?;
                }
                if let Some(vz) = velocity_z {
                    writer.write_u8((vz * VELOCITY_WRITE_FACTOR) as i32 as u8)?;
                }
                writer.write_i32::<LittleEndian>(items)?;
                if let Some(wf) = weapon_frame {
                    writer.write_u8(wf)?;
                }
                if let Some(a) = armor {
                    writer.write_u8(a)?;
                }
                if let Some(w) = weapon {
                    writer.write_u8(w)?;
                }
                writer.write_i16::<LittleEndian>(health)?;
                writer.write_u8(ammo)?;
                writer.write_u8(ammo_shells)?;
                writer.write_u8(ammo_nails)?;
                writer.write_u8(ammo_rockets)?;
                writer.write_u8(ammo_cells)?;
                writer.write_u8(active_weapon)?;
            }

            ServerCmd::StopSound { entity_id, channel } => {
                let entity_channel = entity_id << 3 | channel as u16 & 0b111;
                writer.write_u16::<LittleEndian>(entity_channel)?;
            }

            ServerCmd::UpdateColors { player_id, colors } => {
                writer.write_u8(player_id)?;
                writer.write_u8(colors)?;
            }

            ServerCmd::Particle {
                origin,
                direction,
                count,
                color,
            } => {
                for i in 0..3 {
                    write_coord(writer, origin[i])?;
                }

                for i in 0..3 {
                    writer.write_i8(
                        match direction[i] * PARTICLE_DIRECTION_WRITE_FACTOR {
                            d if d > ::std::i8::MAX as f32 => ::std::i8::MAX,
                            d if d < ::std::i8::MIN as f32 => ::std::i8::MIN,
                            d => d as i8,
                        },
                    )?;
                }

                writer.write_u8(count)?;
                writer.write_u8(color)?;
            }

            ServerCmd::Damage {
                armor,
                blood,
                source,
            } => {
                writer.write_u8(armor)?;
                writer.write_u8(blood)?;
                for i in 0..3 {
                    write_coord(writer, source[i])?;
                }
            }

            ServerCmd::SpawnStatic {
                model_id,
                frame_id,
                colormap,
                skin_id,
                origin,
                angles,
            } => {
                writer.write_u8(model_id)?;
                writer.write_u8(frame_id)?;
                writer.write_u8(colormap)?;
                writer.write_u8(skin_id)?;

                for i in 0..3 {
                    write_coord(writer, origin[i])?;
                    write_angle(writer, angles[i])?;
                }
            }

            ServerCmd::SpawnBaseline {
                ent_id,
                model_id,
                frame_id,
                colormap,
                skin_id,
                origin,
                angles,
            } => {
                writer.write_u16::<LittleEndian>(ent_id)?;
                writer.write_u8(model_id)?;
                writer.write_u8(frame_id)?;
                writer.write_u8(colormap)?;
                writer.write_u8(skin_id)?;

                for i in 0..3 {
                    write_coord(writer, origin[i])?;
                    write_angle(writer, angles[i])?;
                }
            }

            ServerCmd::TempEntity { ref temp_entity } => {
                temp_entity.write_temp_entity(writer)?;
            }

            ServerCmd::SetPause { paused } => {
                writer.write_u8(match paused {
                    false => 0,
                    true => 1,
                })?;
            }

            ServerCmd::SignOnStage { stage } => {
                writer.write_u8(stage as u8)?;
            }

            ServerCmd::CenterPrint { ref text } => {
                writer.write(text.as_bytes())?;
                writer.write_u8(0)?;
            }

            ServerCmd::KilledMonster | ServerCmd::FoundSecret => (),

            ServerCmd::SpawnStaticSound {
                origin,
                sound_id,
                volume,
                attenuation,
            } => {
                for i in 0..3 {
                    write_coord(writer, origin[i]);
                }

                writer.write_u8(sound_id)?;
                writer.write_u8(volume)?;
                writer.write_u8(attenuation)?;
            }

            ServerCmd::Intermission => (),

            ServerCmd::Finale { ref text } => {
                writer.write(text.as_bytes())?;
                writer.write_u8(0)?;
            }

            ServerCmd::CdTrack { track, loop_ } => {
                writer.write_u8(track)?;
                writer.write_u8(loop_)?;
            }

            ServerCmd::SellScreen => (),

            ServerCmd::Cutscene { ref text } => {
                writer.write(text.as_bytes())?;
                writer.write_u8(0)?;
            }
        }

        Ok(())
    }
}

#[derive(FromPrimitive)]
pub enum ClientCmdCode {
    Bad = 0,
    NoOp = 1,
    Disconnect = 2,
    Move = 3,
    StringCmd = 4,
}

pub struct ClientCmdMove {
    send_time: Duration,
    angles: Vector3<Deg<f32>>,
    fwd_move: u16,
    side_move: u16,
    up_move: u16,
    button_flags: ButtonFlags,
    impulse: u8,
}

impl Cmd for ClientCmdMove {
    fn code(&self) -> u8 {
        ClientCmdCode::Move as u8
    }

    fn deserialize<R>(reader: &mut R) -> Result<ClientCmdMove, NetError>
    where
        R: ReadBytesExt + BufRead,
    {
        let send_time = engine::duration_from_f32(reader.read_f32::<LittleEndian>()?);
        let angles = Vector3::new(
            read_angle(reader)?,
            read_angle(reader)?,
            read_angle(reader)?,
        );
        let fwd_move = reader.read_u16::<LittleEndian>()?;
        let side_move = reader.read_u16::<LittleEndian>()?;
        let up_move = reader.read_u16::<LittleEndian>()?;
        let button_flags_val = reader.read_u8()?;
        let button_flags = match ButtonFlags::from_bits(button_flags_val) {
            Some(bf) => bf,
            None => {
                return Err(NetError::InvalidData(format!(
                    "Invalid value for button flags: {}",
                    button_flags_val
                )))
            }
        };
        let impulse = reader.read_u8()?;

        Ok(ClientCmdMove {
            send_time,
            angles,
            fwd_move,
            side_move,
            up_move,
            button_flags,
            impulse,
        })
    }

    fn serialize<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        writer.write_f32::<LittleEndian>(
            engine::duration_to_f32(self.send_time),
        )?;
        for i in 0..3 {
            write_angle(writer, self.angles[i])?;
        }
        writer.write_u16::<LittleEndian>(self.fwd_move)?;
        writer.write_u16::<LittleEndian>(self.side_move)?;
        writer.write_u16::<LittleEndian>(self.up_move)?;
        writer.write_u8(self.button_flags.bits())?;
        writer.write_u8(self.impulse)?;

        Ok(())
    }
}

pub struct ClientCmdStringCmd {
    pub cmd: String,
}

impl Cmd for ClientCmdStringCmd {
    fn code(&self) -> u8 {
        ClientCmdCode::StringCmd as u8
    }

    fn deserialize<R>(reader: &mut R) -> Result<ClientCmdStringCmd, NetError>
    where
        R: ReadBytesExt + BufRead,
    {
        let cmd = util::read_cstring(reader).unwrap();

        Ok(ClientCmdStringCmd { cmd })
    }

    fn serialize<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        writer.write(self.cmd.as_bytes())?;
        writer.write_u8(0)?;

        Ok(())
    }
}

pub enum ClientCmd {
    Bad,
    NoOp,
    Disconnect,
    Move(ClientCmdMove),
    StringCmd(ClientCmdStringCmd),
}

impl ClientCmd {
    pub fn code(&self) -> u8 {
        match *self {
            ClientCmd::Bad => ClientCmdCode::Bad as u8,
            ClientCmd::NoOp => ClientCmdCode::NoOp as u8,
            ClientCmd::Disconnect => ClientCmdCode::Disconnect as u8,
            ClientCmd::Move(_) => ClientCmdCode::Move as u8,
            ClientCmd::StringCmd(_) => ClientCmdCode::StringCmd as u8,
        }
    }

    pub fn deserialize<R>(reader: &mut R) -> Result<ClientCmd, NetError>
    where
        R: ReadBytesExt + BufRead,
    {
        let code_val = reader.read_u8()?;
        let code = match ClientCmdCode::from_u8(code_val) {
            Some(c) => c,
            None => {
                return Err(NetError::InvalidData(
                    format!("Invalid client command code: {}", code_val),
                ))
            }
        };

        let cmd = match code {
            ClientCmdCode::Bad => ClientCmd::Bad,
            ClientCmdCode::NoOp => ClientCmd::NoOp,
            ClientCmdCode::Disconnect => ClientCmd::Disconnect,
            ClientCmdCode::Move => ClientCmd::Move(ClientCmdMove::deserialize(reader)?),
            ClientCmdCode::StringCmd => ClientCmd::StringCmd(
                ClientCmdStringCmd::deserialize(reader)?,
            ),
        };

        Ok(cmd)
    }

    pub fn serialize<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        writer.write_u8(self.code())?;

        match *self {
            ClientCmd::Bad => (),
            ClientCmd::NoOp => (),
            ClientCmd::Disconnect => (),
            ClientCmd::Move(ref move_cmd) => move_cmd.serialize(writer)?,
            ClientCmd::StringCmd(ref string_cmd) => string_cmd.serialize(writer)?,
        }

        Ok(())
    }
}

#[derive(PartialEq)]
pub enum BlockingMode {
    Blocking,
    NonBlocking,
    Timeout(Duration),
}

pub struct QSocket {
    socket: UdpSocket,
    remote: SocketAddr,

    unreliable_send_sequence: u32,
    unreliable_recv_sequence: u32,

    ack_sequence: u32,

    send_sequence: u32,
    send_queue: VecDeque<Box<[u8]>>,
    send_cache: Box<[u8]>,
    send_next: bool,
    send_count: usize,
    resend_count: usize,

    recv_sequence: u32,
    recv_buf: [u8; MAX_MESSAGE],
}

impl QSocket {
    pub fn new(socket: UdpSocket, remote: SocketAddr) -> QSocket {
        QSocket {
            socket,
            remote,

            unreliable_send_sequence: 0,
            unreliable_recv_sequence: 0,

            ack_sequence: 0,

            send_sequence: 0,
            send_queue: VecDeque::new(),
            send_cache: Box::new([]),
            send_count: 0,
            send_next: false,
            resend_count: 0,

            recv_sequence: 0,
            recv_buf: [0; MAX_MESSAGE],
        }
    }

    pub fn can_send(&self) -> bool {
        self.send_queue.is_empty() && self.send_cache.is_empty()
    }

    /// Begin sending a reliable message over this socket.
    pub fn begin_send_msg(&mut self, msg: &[u8]) -> Result<(), NetError> {
        // make sure all reliable messages have been ACKed in their entirety
        if !self.send_queue.is_empty() {
            return Err(NetError::with_msg(
                "Called begin_send_msg() with previous message unacknowledged",
            ));
        }

        // empty messages are an error
        if msg.len() == 0 {
            return Err(NetError::with_msg("Input data has zero length"));
        }

        // check upper message length bound
        if msg.len() > MAX_MESSAGE {
            return Err(NetError::with_msg("Input data exceeds MAX_MESSAGE"));
        }

        // split the message into chunks and enqueue them
        for chunk in msg.chunks(MAX_DATAGRAM) {
            self.send_queue.push_back(
                chunk.to_owned().into_boxed_slice(),
            );
        }

        // send the first chunk
        self.send_msg_next()?;

        Ok(())
    }

    /// Resend the last reliable message packet.
    pub fn resend_msg(&mut self) -> Result<(), NetError> {
        if self.send_cache.is_empty() {
            Err(NetError::with_msg("Attempted resend with empty send cache"))
        } else {
            self.socket.send_to(&self.send_cache, self.remote)?;
            self.resend_count += 1;

            Ok(())
        }
    }

    /// Send the next segment of a reliable message.
    pub fn send_msg_next(&mut self) -> Result<(), NetError> {
        // grab the first chunk in the queue
        let content = self.send_queue.pop_front().expect(
            "Send queue is empty (this is a bug)",
        );

        // if this was the last chunk, set the EOM flag
        let msg_kind = match self.send_queue.is_empty() {
            true => MsgKind::ReliableEom,
            false => MsgKind::Reliable,
        };

        // compose the packet
        let mut compose = Vec::with_capacity(MAX_PACKET);
        compose.write_u16::<NetworkEndian>(msg_kind as u16)?;
        compose.write_u16::<NetworkEndian>(
            (HEADER_SIZE + content.len()) as u16,
        )?;
        compose.write_u32::<NetworkEndian>(self.send_sequence)?;
        compose.write_all(&content);

        // store packet to send cache
        self.send_cache = compose.into_boxed_slice();

        // increment send sequence
        self.send_sequence += 1;

        // send the composed packet
        self.socket.send_to(&self.send_cache, self.remote)?;

        // TODO: update send time
        // bump send count
        self.send_count += 1;

        // don't send the next chunk until this one gets ACKed
        self.send_next = false;

        Ok(())
    }

    pub fn send_msg_unreliable(&mut self, content: &[u8]) -> Result<(), NetError> {
        if content.len() == 0 {
            return Err(NetError::with_msg("Unreliable message has zero length"));
        }

        if content.len() > MAX_DATAGRAM {
            return Err(NetError::with_msg(
                "Unreliable message length exceeds MAX_DATAGRAM",
            ));
        }

        let packet_len = HEADER_SIZE + content.len();

        // compose the packet
        let mut packet = Vec::with_capacity(MAX_PACKET);
        packet.write_u16::<NetworkEndian>(
            MsgKind::Unreliable as u16,
        )?;
        packet.write_u16::<NetworkEndian>(packet_len as u16)?;
        packet.write_u32::<NetworkEndian>(
            self.unreliable_send_sequence,
        )?;
        packet.write_all(content)?;

        // increment unreliable send sequence
        self.unreliable_send_sequence += 1;

        // send the message
        self.socket.send_to(&packet, self.remote)?;

        // bump send count
        self.send_count += 1;

        Ok(())
    }

    /// Receive a message on this socket.
    // TODO: the flow control in this function is completely baffling, make it a little less awful
    pub fn recv_msg(&mut self, block: BlockingMode) -> Result<Vec<u8>, NetError> {
        let mut msg = Vec::new();

        match block {
            BlockingMode::Blocking => {
                self.socket.set_nonblocking(false)?;
                self.socket.set_read_timeout(None)?;
            }

            BlockingMode::NonBlocking => {
                self.socket.set_nonblocking(true)?;
                self.socket.set_read_timeout(None)?;
            }

            BlockingMode::Timeout(d) => {
                self.socket.set_nonblocking(false);
                self.socket.set_read_timeout(Some(d.to_std().unwrap()))?;
            }
        }

        loop {
            let (packet_len, src_addr) = match self.socket.recv_from(&mut self.recv_buf) {
                Ok(x) => x,
                Err(e) => {
                    use std::io::ErrorKind;
                    match e.kind() {
                        // these errors are expected in nonblocking mode
                        ErrorKind::WouldBlock | ErrorKind::TimedOut => return Ok(Vec::new()),
                        _ => return Err(NetError::from(e)),
                    }
                }
            };

            if src_addr != self.remote {
                // this packet didn't come from remote, drop it
                debug!(
                    "forged packet (src_addr was {}, should be {})",
                    src_addr,
                    self.remote
                );
                continue;
            }

            let mut reader = BufReader::new(Cursor::new(&self.recv_buf[..packet_len]));

            let msg_kind_code = reader.read_u16::<NetworkEndian>()?;
            let msg_kind = match MsgKind::from_u16(msg_kind_code) {
                Some(f) => f,
                None => {
                    return Err(NetError::InvalidData(
                        format!("Invalid message kind: {}", msg_kind_code),
                    ))
                }
            };

            if packet_len < HEADER_SIZE {
                // TODO: increment short packet count
                debug!("short packet");
                continue;
            }

            let field_len = reader.read_u16::<NetworkEndian>()?;
            if field_len as usize != packet_len {
                return Err(NetError::InvalidData(format!(
                    "Length field and actual length differ ({} != {})",
                    field_len,
                    packet_len
                )));
            }

            let sequence;
            if msg_kind != MsgKind::Ctl {
                sequence = reader.read_u32::<NetworkEndian>()?;
            } else {
                sequence = 0;
            }

            match msg_kind {
                // ignore control messages
                MsgKind::Ctl => (),

                MsgKind::Unreliable => {
                    // we've received a newer datagram, ignore
                    if sequence < self.unreliable_recv_sequence {
                        println!("Stale datagram with sequence # {}", sequence);
                        break;
                    }

                    // we've skipped some datagrams, count them as dropped
                    if sequence > self.unreliable_recv_sequence {
                        let drop_count = sequence - self.unreliable_recv_sequence;
                        println!(
                            "Dropped {} packet(s) ({} -> {})",
                            drop_count,
                            sequence,
                            self.unreliable_recv_sequence
                        );
                    }

                    self.unreliable_recv_sequence = sequence + 1;

                    // copy the rest of the packet into the message buffer and return
                    reader.read_to_end(&mut msg)?;
                    return Ok(msg);
                }

                MsgKind::Ack => {
                    if sequence != self.send_sequence - 1 {
                        println!("Stale ACK received");
                    } else if sequence != self.ack_sequence {
                        println!("Duplicate ACK received");
                    } else {
                        self.ack_sequence += 1;
                        if self.ack_sequence != self.send_sequence {
                            return Err(NetError::with_msg("ACK sequencing error"));
                        }

                        // our last reliable message has been acked
                        if self.send_queue.is_empty() {
                            // the whole message is through, clear the send cache
                            self.send_cache = Box::new([]);
                        } else {
                            // send the next chunk before returning
                            self.send_next = true;
                        }
                    }
                }

                // TODO: once we start reading a reliable message, don't allow other packets until
                // we have the whole thing
                MsgKind::Reliable | MsgKind::ReliableEom => {
                    // send ack message and increment self.recv_sequence
                    let mut ack_buf: [u8; HEADER_SIZE] = [0; HEADER_SIZE];
                    let mut ack_curs = Cursor::new(&mut ack_buf[..]);
                    ack_curs.write_u16::<NetworkEndian>(MsgKind::Ack as u16)?;
                    ack_curs.write_u16::<NetworkEndian>(HEADER_SIZE as u16)?;
                    ack_curs.write_u32::<NetworkEndian>(sequence)?;
                    self.socket.send_to(ack_curs.into_inner(), self.remote)?;

                    // if this was a duplicate, drop it
                    if sequence != self.recv_sequence {
                        println!("Duplicate message received");
                        continue;
                    }

                    self.recv_sequence += 1;
                    reader.read_to_end(&mut msg)?;

                    // if this is the last chunk of a reliable message, break out and return
                    if msg_kind == MsgKind::ReliableEom {
                        break;
                    }
                }
            }
        }

        if self.send_next {
            self.send_msg_next()?;
        }

        Ok(msg)
    }
}

fn read_coord<R>(reader: &mut R) -> Result<f32, NetError>
where
    R: BufRead + ReadBytesExt,
{
    Ok(reader.read_i16::<LittleEndian>()? as f32 / 8.0)
}

fn write_coord<W>(writer: &mut W, coord: f32) -> Result<(), NetError>
where
    W: WriteBytesExt,
{
    writer.write_i16::<LittleEndian>((coord * 8.0) as i16)?;
    Ok(())
}

fn read_angle<R>(reader: &mut R) -> Result<Deg<f32>, NetError>
where
    R: BufRead + ReadBytesExt,
{
    Ok(Deg(reader.read_i8()? as f32 * (360.0 / 256.0)))
}

fn write_angle<W>(writer: &mut W, angle: Deg<f32>) -> Result<(), NetError>
where
    W: WriteBytesExt,
{
    writer.write_u8(((angle.0 as i32 * 256 / 360) & 0xFF) as u8)?;
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    use std::io::BufReader;

    #[test]
    fn test_server_cmd_update_stat_read_write_eq() {
        let src = ServerCmd::UpdateStat {
            stat: ClientStat::Nails,
            value: 64,
        };

        let mut packet = Vec::new();
        src.serialize(&mut packet).unwrap();
        let mut reader = BufReader::new(packet.as_slice());
        let dst = ServerCmd::deserialize(&mut reader).unwrap().unwrap();

        assert_eq!(src, dst);
    }

    #[test]
    fn test_server_cmd_version_read_write_eq() {
        let src = ServerCmd::Version { version: 42 };

        let mut packet = Vec::new();
        src.serialize(&mut packet).unwrap();
        let mut reader = BufReader::new(packet.as_slice());
        let dst = ServerCmd::deserialize(&mut reader).unwrap().unwrap();

        assert_eq!(src, dst);
    }

    #[test]
    fn test_server_cmd_set_view_read_write_eq() {
        let src = ServerCmd::SetView { ent_id: 17 };

        let mut packet = Vec::new();
        src.serialize(&mut packet).unwrap();
        let mut reader = BufReader::new(packet.as_slice());
        let dst = ServerCmd::deserialize(&mut reader).unwrap().unwrap();

        assert_eq!(src, dst);
    }

    #[test]
    fn test_server_cmd_time_read_write_eq() {
        let src = ServerCmd::Time { time: 23.07 };

        let mut packet = Vec::new();
        src.serialize(&mut packet).unwrap();
        let mut reader = BufReader::new(packet.as_slice());
        let dst = ServerCmd::deserialize(&mut reader).unwrap().unwrap();

        assert_eq!(src, dst);
    }

    #[test]
    fn test_server_cmd_print_read_write_eq() {
        let src = ServerCmd::Print { text: String::from("print test") };

        let mut packet = Vec::new();
        src.serialize(&mut packet).unwrap();
        let mut reader = BufReader::new(packet.as_slice());
        let dst = ServerCmd::deserialize(&mut reader).unwrap().unwrap();

        assert_eq!(src, dst);
    }

    #[test]
    fn test_server_cmd_stuff_text_read_write_eq() {
        let src = ServerCmd::StuffText { text: String::from("stufftext test") };

        let mut packet = Vec::new();
        src.serialize(&mut packet).unwrap();
        let mut reader = BufReader::new(packet.as_slice());
        let dst = ServerCmd::deserialize(&mut reader).unwrap().unwrap();

        assert_eq!(src, dst);
    }

    #[test]
    fn test_server_cmd_server_info_read_write_eq() {
        let src = ServerCmd::ServerInfo {
            protocol_version: 42,
            max_clients: 16,
            game_type: GameType::Deathmatch,
            message: String::from("Test message"),
            model_precache: vec![String::from("test1.bsp"), String::from("test2.bsp")],
            sound_precache: vec![String::from("test1.wav"), String::from("test2.wav")],
        };

        let mut packet = Vec::new();
        src.serialize(&mut packet).unwrap();
        let mut reader = BufReader::new(packet.as_slice());
        let dst = ServerCmd::deserialize(&mut reader).unwrap().unwrap();

        assert_eq!(src, dst);
    }

    #[test]
    fn test_server_cmd_light_style_read_write_eq() {
        let src = ServerCmd::LightStyle {
            id: 11,
            value: String::from("aaaaabcddeefgghjjjkaaaazzzzyxwaaaba"),
        };

        let mut packet = Vec::new();
        src.serialize(&mut packet).unwrap();
        let mut reader = BufReader::new(packet.as_slice());
        let dst = ServerCmd::deserialize(&mut reader).unwrap().unwrap();

        assert_eq!(src, dst);
    }

    #[test]
    fn test_server_cmd_update_name_read_write_eq() {
        let src = ServerCmd::UpdateName {
            player_id: 7,
            new_name: String::from("newname"),
        };

        let mut packet = Vec::new();
        src.serialize(&mut packet).unwrap();
        let mut reader = BufReader::new(packet.as_slice());
        let dst = ServerCmd::deserialize(&mut reader).unwrap().unwrap();

        assert_eq!(src, dst);
    }

    #[test]
    fn test_server_cmd_update_frags_read_write_eq() {
        let src = ServerCmd::UpdateFrags {
            player_id: 7,
            new_frags: 11,
        };

        let mut packet = Vec::new();
        src.serialize(&mut packet).unwrap();
        let mut reader = BufReader::new(packet.as_slice());
        let dst = ServerCmd::deserialize(&mut reader).unwrap().unwrap();

        assert_eq!(src, dst);
    }

    #[test]
    fn test_server_cmd_stop_sound_read_write_eq() {
        let src = ServerCmd::StopSound {
            entity_id: 17,
            channel: 3,
        };

        let mut packet = Vec::new();
        src.serialize(&mut packet).unwrap();
        let mut reader = BufReader::new(packet.as_slice());
        let dst = ServerCmd::deserialize(&mut reader).unwrap().unwrap();

        assert_eq!(src, dst);
    }

    #[test]
    fn test_server_cmd_update_colors_read_write_eq() {
        let src = ServerCmd::UpdateColors {
            player_id: 11,
            colors: 0x73,
        };

        let mut packet = Vec::new();
        src.serialize(&mut packet).unwrap();
        let mut reader = BufReader::new(packet.as_slice());
        let dst = ServerCmd::deserialize(&mut reader).unwrap().unwrap();

        assert_eq!(src, dst);
    }

    #[test]
    fn test_server_cmd_set_pause_read_write_eq() {
        let src = ServerCmd::SetPause { paused: true };
        let mut packet = Vec::new();
        src.serialize(&mut packet).unwrap();
        let mut reader = BufReader::new(packet.as_slice());
        let dst = ServerCmd::deserialize(&mut reader).unwrap().unwrap();

        assert_eq!(src, dst);
    }

    #[test]
    fn test_server_cmd_sign_on_stage_read_write_eq() {
        let src = ServerCmd::SignOnStage { stage: SignOnStage::Begin };
        let mut packet = Vec::new();
        src.serialize(&mut packet).unwrap();
        let mut reader = BufReader::new(packet.as_slice());
        let dst = ServerCmd::deserialize(&mut reader).unwrap().unwrap();

        assert_eq!(src, dst);
    }

    #[test]
    fn test_server_cmd_center_print_read_write_eq() {
        let src = ServerCmd::CenterPrint { text: String::from("Center print test") };
        let mut packet = Vec::new();
        src.serialize(&mut packet).unwrap();
        let mut reader = BufReader::new(packet.as_slice());
        let dst = ServerCmd::deserialize(&mut reader).unwrap().unwrap();

        assert_eq!(src, dst);
    }

    #[test]
    fn test_server_cmd_finale_read_write_eq() {
        let src = ServerCmd::Finale { text: String::from("Finale test") };
        let mut packet = Vec::new();
        src.serialize(&mut packet).unwrap();
        let mut reader = BufReader::new(packet.as_slice());
        let dst = ServerCmd::deserialize(&mut reader).unwrap().unwrap();

        assert_eq!(src, dst);
    }

    #[test]
    fn test_server_cmd_cd_track_read_write_eq() {
        let src = ServerCmd::CdTrack { track: 5, loop_: 1 };
        let mut packet = Vec::new();
        src.serialize(&mut packet).unwrap();
        let mut reader = BufReader::new(packet.as_slice());
        let dst = ServerCmd::deserialize(&mut reader).unwrap().unwrap();

        assert_eq!(src, dst);
    }

    #[test]
    fn test_server_cmd_cutscene_read_write_eq() {
        let src = ServerCmd::Cutscene { text: String::from("Cutscene test") };
        let mut packet = Vec::new();
        src.serialize(&mut packet).unwrap();
        let mut reader = BufReader::new(packet.as_slice());
        let dst = ServerCmd::deserialize(&mut reader).unwrap().unwrap();

        assert_eq!(src, dst);
    }

    fn gen_qsocket_pair() -> (QSocket, QSocket) {
        let src_udp = UdpSocket::bind("localhost:0").unwrap();
        let src_addr = src_udp.local_addr().unwrap();

        let dst_udp = UdpSocket::bind("localhost:0").unwrap();
        let dst_addr = dst_udp.local_addr().unwrap();

        (
            QSocket::new(src_udp, dst_addr),
            QSocket::new(dst_udp, src_addr),
        )
    }

    #[test]
    fn test_qsocket_send_msg_short() {
        let (mut src, mut dst) = gen_qsocket_pair();

        let message = String::from("test message").into_bytes();
        src.begin_send_msg(&message).unwrap();
        let received = dst.recv_msg(BlockingMode::Timeout(Duration::seconds(1)))
            .unwrap();
        assert_eq!(message, received);

        // TODO: assert can_send == true, send_next == false, etc
    }

    #[test]
    fn test_qsocket_send_msg_unreliable_recv_msg_eq() {
        let (mut src, mut dst) = gen_qsocket_pair();

        let message = String::from("test message").into_bytes();
        src.send_msg_unreliable(&message).unwrap();
        let received = dst.recv_msg(BlockingMode::Timeout(Duration::seconds(1)))
            .unwrap();
        assert_eq!(message, received);
    }

    #[test]
    #[should_panic]
    fn test_qsocket_send_msg_unreliable_zero_length_fails() {
        let (mut src, _) = gen_qsocket_pair();

        let message = [];
        src.send_msg_unreliable(&message).unwrap();
    }

    #[test]
    #[should_panic]
    fn test_qsocket_send_msg_unreliable_exceeds_max_length_fails() {
        let (mut src, _) = gen_qsocket_pair();

        let message = [0; MAX_DATAGRAM + 1];
        src.send_msg_unreliable(&message).unwrap();
    }
}
