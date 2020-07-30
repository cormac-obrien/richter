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

use std::{
    collections::VecDeque,
    error::Error,
    fmt,
    io::{BufRead, BufReader, Cursor, Read, Write},
    net::{SocketAddr, UdpSocket},
};

use crate::common::{engine, util};

use byteorder::{LittleEndian, NetworkEndian, ReadBytesExt, WriteBytesExt};
use cgmath::{Deg, Vector3, Zero};
use chrono::Duration;
use num::FromPrimitive;

pub const MAX_MESSAGE: usize = 8192;
const MAX_DATAGRAM: usize = 1024;
const HEADER_SIZE: usize = 8;
const MAX_PACKET: usize = HEADER_SIZE + MAX_DATAGRAM;

pub const PROTOCOL_VERSION: u8 = 15;

const NAME_LEN: usize = 64;

const FAST_UPDATE_FLAG: u8 = 0x80;

const VELOCITY_READ_FACTOR: f32 = 16.0;
const VELOCITY_WRITE_FACTOR: f32 = 1.0 / VELOCITY_READ_FACTOR;

const PARTICLE_DIRECTION_READ_FACTOR: f32 = 1.0 / 16.0;
const PARTICLE_DIRECTION_WRITE_FACTOR: f32 = 1.0 / PARTICLE_DIRECTION_READ_FACTOR;

const SOUND_ATTENUATION_WRITE_FACTOR: u8 = 64;
const SOUND_ATTENUATION_READ_FACTOR: f32 = 1.0 / SOUND_ATTENUATION_WRITE_FACTOR as f32;

pub static GAME_NAME: &'static str = "QUAKE";
pub const MAX_CLIENTS: usize = 16;
pub const MAX_ITEMS: usize = 32;

pub const DEFAULT_VIEWHEIGHT: f32 = 22.0;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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

    pub fn from_bits(bits: u8) -> PlayerColor {
        let top = bits >> 4;
        let bottom = bits & 0x0F;

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

#[derive(Clone, Copy, Debug)]
pub struct ColorShift {
    pub dest_color: [u8; 3],
    pub percent: i32,
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
    ColorExplosion = 12,
    Grapple = 13,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PointEntityKind {
    Spike,
    SuperSpike,
    Gunshot,
    Explosion,
    ColorExplosion { color_start: u8, color_len: u8 },
    TarExplosion,
    WizSpike,
    KnightSpike,
    LavaSplash,
    Teleport,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BeamEntityKind {
    /// Lightning bolt
    Lightning {
        /// id of the lightning model to use. must be 1, 2, or 3.
        model_id: u8,
    },
    /// Grappling hook cable
    Grapple,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TempEntity {
    Point {
        kind: PointEntityKind,
        origin: Vector3<f32>,
    },
    Beam {
        kind: BeamEntityKind,
        entity_id: i16,
        start: Vector3<f32>,
        end: Vector3<f32>,
    },
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
                return Err(NetError::InvalidData(format!(
                    "Temp entity code {}",
                    code_byte
                )))
            }
        };

        use TempEntity::*;
        use TempEntityCode as Code;

        Ok(match code {
            Code::Spike
            | Code::SuperSpike
            | Code::Gunshot
            | Code::Explosion
            | Code::TarExplosion
            | Code::WizSpike
            | Code::KnightSpike
            | Code::LavaSplash
            | Code::Teleport => Point {
                kind: match code {
                    Code::Spike => PointEntityKind::Spike,
                    Code::SuperSpike => PointEntityKind::SuperSpike,
                    Code::Gunshot => PointEntityKind::Gunshot,
                    Code::Explosion => PointEntityKind::Explosion,
                    Code::TarExplosion => PointEntityKind::TarExplosion,
                    Code::WizSpike => PointEntityKind::WizSpike,
                    Code::KnightSpike => PointEntityKind::KnightSpike,
                    Code::LavaSplash => PointEntityKind::LavaSplash,
                    Code::Teleport => PointEntityKind::Teleport,
                    _ => unreachable!(),
                },
                origin: read_coord_vector3(reader)?,
            },
            Code::ColorExplosion => {
                let origin = read_coord_vector3(reader)?;
                let color_start = reader.read_u8()?;
                let color_len = reader.read_u8()?;

                Point {
                    origin,
                    kind: PointEntityKind::ColorExplosion {
                        color_start,
                        color_len,
                    },
                }
            }
            Code::Lightning1 | Code::Lightning2 | Code::Lightning3 => Beam {
                kind: BeamEntityKind::Lightning {
                    model_id: match code {
                        Code::Lightning1 => 1,
                        Code::Lightning2 => 2,
                        Code::Lightning3 => 3,
                        _ => unreachable!(),
                    },
                },
                entity_id: reader.read_i16::<LittleEndian>()?,
                start: read_coord_vector3(reader)?,
                end: read_coord_vector3(reader)?,
            },
            Code::Grapple => Beam {
                kind: BeamEntityKind::Grapple,
                entity_id: reader.read_i16::<LittleEndian>()?,
                start: read_coord_vector3(reader)?,
                end: read_coord_vector3(reader)?,
            },
        })
    }

    pub fn write_temp_entity<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        use TempEntityCode as Code;

        match *self {
            TempEntity::Point { kind, origin } => {
                use PointEntityKind as Pk;
                match kind {
                    Pk::Spike
                    | Pk::SuperSpike
                    | Pk::Gunshot
                    | Pk::Explosion
                    | Pk::TarExplosion
                    | Pk::WizSpike
                    | Pk::KnightSpike
                    | Pk::LavaSplash
                    | Pk::Teleport => {
                        let code = match kind {
                            Pk::Spike => Code::Spike,
                            Pk::SuperSpike => Code::SuperSpike,
                            Pk::Gunshot => Code::Gunshot,
                            Pk::Explosion => Code::Explosion,
                            Pk::TarExplosion => Code::TarExplosion,
                            Pk::WizSpike => Code::WizSpike,
                            Pk::KnightSpike => Code::KnightSpike,
                            Pk::LavaSplash => Code::LavaSplash,
                            Pk::Teleport => Code::Teleport,
                            _ => unreachable!(),
                        };

                        // write code
                        writer.write_u8(code as u8)?;
                    }
                    PointEntityKind::ColorExplosion {
                        color_start,
                        color_len,
                    } => {
                        // write code and colors
                        writer.write_u8(Code::ColorExplosion as u8)?;
                        writer.write_u8(color_start)?;
                        writer.write_u8(color_len)?;
                    }
                };

                write_coord_vector3(writer, origin)?;
            }

            TempEntity::Beam {
                kind,
                entity_id,
                start,
                end,
            } => {
                let code = match kind {
                    BeamEntityKind::Lightning { model_id } => match model_id {
                        1 => Code::Lightning1,
                        2 => Code::Lightning2,
                        3 => Code::Lightning3,
                        // TODO: error
                        _ => panic!("invalid lightning model id: {}", model_id),
                    },
                    BeamEntityKind::Grapple => Code::Grapple,
                };
                writer.write_i16::<LittleEndian>(entity_id)?;
                writer.write_u8(code as u8)?;
                write_coord_vector3(writer, start)?;
                write_coord_vector3(writer, end)?;
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
    pub struct EntityEffects: u8 {
        const BRIGHT_FIELD = 0b0001;
        const MUZZLE_FLASH = 0b0010;
        const BRIGHT_LIGHT = 0b0100;
        const DIM_LIGHT    = 0b1000;
    }
}

#[derive(Clone, Debug)]
pub struct EntityState {
    pub origin: Vector3<f32>,
    pub angles: Vector3<Deg<f32>>,
    pub model_id: usize,
    pub frame_id: usize,

    // TODO: more specific types for these
    pub colormap: u8,
    pub skin_id: usize,
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

#[derive(Clone, Debug, PartialEq)]
pub struct EntityUpdate {
    pub ent_id: u16,
    pub model_id: Option<u8>,
    pub frame_id: Option<u8>,
    pub colormap: Option<u8>,
    pub skin_id: Option<u8>,
    pub effects: Option<EntityEffects>,
    pub origin_x: Option<f32>,
    pub pitch: Option<Deg<f32>>,
    pub origin_y: Option<f32>,
    pub yaw: Option<Deg<f32>>,
    pub origin_z: Option<f32>,
    pub roll: Option<Deg<f32>>,
    pub no_lerp: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayerData {
    pub view_height: Option<f32>,
    pub ideal_pitch: Option<Deg<f32>>,
    pub punch_pitch: Option<Deg<f32>>,
    pub velocity_x: Option<f32>,
    pub punch_yaw: Option<Deg<f32>>,
    pub velocity_y: Option<f32>,
    pub punch_roll: Option<Deg<f32>>,
    pub velocity_z: Option<f32>,
    pub items: ItemFlags,
    pub on_ground: bool,
    pub in_water: bool,
    pub weapon_frame: Option<u8>,
    pub armor: Option<u8>,
    pub weapon: Option<u8>,
    pub health: i16,
    pub ammo: u8,
    pub ammo_shells: u8,
    pub ammo_nails: u8,
    pub ammo_rockets: u8,
    pub ammo_cells: u8,
    pub active_weapon: u8,
}

impl EntityUpdate {
    /// Create an `EntityState` from this update, filling in any `None` values
    /// from the specified baseline state.
    pub fn to_entity_state(&self, baseline: &EntityState) -> EntityState {
        EntityState {
            origin: Vector3::new(
                self.origin_x.unwrap_or(baseline.origin.x),
                self.origin_y.unwrap_or(baseline.origin.y),
                self.origin_z.unwrap_or(baseline.origin.z),
            ),
            angles: Vector3::new(
                self.pitch.unwrap_or(baseline.angles[0]),
                self.yaw.unwrap_or(baseline.angles[1]),
                self.roll.unwrap_or(baseline.angles[2]),
            ),
            model_id: self.model_id.map_or(baseline.model_id, |m| m as usize),
            frame_id: self.frame_id.map_or(baseline.frame_id, |f| f as usize),
            skin_id: self.skin_id.map_or(baseline.skin_id, |s| s as usize),
            effects: self.effects.unwrap_or(baseline.effects),
            colormap: self.colormap.unwrap_or(baseline.colormap),
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

// TODO: use feature(arbitrary_enum_discriminant)
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
    PlayerData = 15,
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
    UpdateStat {
        stat: ClientStat,
        value: i32,
    },
    Version {
        version: i32,
    },
    SetView {
        ent_id: i16,
    },
    Sound {
        volume: Option<u8>,
        attenuation: Option<f32>,
        entity_id: u16,
        channel: i8,
        sound_id: u8,
        position: Vector3<f32>,
    },
    Time {
        time: f32,
    },
    Print {
        text: String,
    },
    StuffText {
        text: String,
    },
    SetAngle {
        angles: Vector3<Deg<f32>>,
    },
    ServerInfo {
        protocol_version: i32,
        max_clients: u8,
        game_type: GameType,
        message: String,
        model_precache: Vec<String>,
        sound_precache: Vec<String>,
    },
    LightStyle {
        id: u8,
        value: String,
    },
    UpdateName {
        player_id: u8,
        new_name: String,
    },
    UpdateFrags {
        player_id: u8,
        new_frags: i16,
    },
    PlayerData(PlayerData),
    StopSound {
        entity_id: u16,
        channel: u8,
    },
    UpdateColors {
        player_id: u8,
        new_colors: PlayerColor,
    },
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
    TempEntity {
        temp_entity: TempEntity,
    },
    SetPause {
        paused: bool,
    },
    SignOnStage {
        stage: SignOnStage,
    },
    CenterPrint {
        text: String,
    },
    KilledMonster,
    FoundSecret,
    SpawnStaticSound {
        origin: Vector3<f32>,
        sound_id: u8,
        volume: u8,
        attenuation: u8,
    },
    Intermission,
    Finale {
        text: String,
    },
    CdTrack {
        track: u8,
        loop_: u8,
    },
    SellScreen,
    Cutscene {
        text: String,
    },
    FastUpdate(EntityUpdate),
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
            ServerCmd::PlayerData(_) => ServerCmdCode::PlayerData,
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
            // TODO: figure out a more elegant way of doing this
            ServerCmd::FastUpdate(_) => panic!("FastUpdate has no code"),
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

        if code_num & FAST_UPDATE_FLAG != 0 {
            let all_bits;
            let low_bits = code_num & !FAST_UPDATE_FLAG;
            if low_bits & UpdateFlags::MORE_BITS.bits() as u8 != 0 {
                let high_bits = reader.read_u8()?;
                all_bits = (high_bits as u16) << 8 | low_bits as u16;
            } else {
                all_bits = low_bits as u16;
            }

            let update_flags = match UpdateFlags::from_bits(all_bits) {
                Some(u) => u,
                None => {
                    return Err(NetError::InvalidData(format!(
                        "UpdateFlags: {:b}",
                        all_bits
                    )))
                }
            };

            let ent_id;
            if update_flags.contains(UpdateFlags::LONG_ENTITY) {
                ent_id = reader.read_u16::<LittleEndian>()?;
            } else {
                ent_id = reader.read_u8()? as u16;
            }

            let model_id;
            if update_flags.contains(UpdateFlags::MODEL) {
                model_id = Some(reader.read_u8()?);
            } else {
                model_id = None;
            }

            let frame_id;
            if update_flags.contains(UpdateFlags::FRAME) {
                frame_id = Some(reader.read_u8()?);
            } else {
                frame_id = None;
            }

            let colormap;
            if update_flags.contains(UpdateFlags::COLORMAP) {
                colormap = Some(reader.read_u8()?);
            } else {
                colormap = None;
            }

            let skin_id;
            if update_flags.contains(UpdateFlags::SKIN) {
                skin_id = Some(reader.read_u8()?);
            } else {
                skin_id = None;
            }

            let effects;
            if update_flags.contains(UpdateFlags::EFFECTS) {
                let effects_bits = reader.read_u8()?;
                effects = match EntityEffects::from_bits(effects_bits) {
                    Some(e) => Some(e),
                    None => {
                        return Err(NetError::InvalidData(format!(
                            "EntityEffects: {:b}",
                            effects_bits
                        )))
                    }
                };
            } else {
                effects = None;
            }

            let origin_x;
            if update_flags.contains(UpdateFlags::ORIGIN_X) {
                origin_x = Some(read_coord(reader)?);
            } else {
                origin_x = None;
            }

            let pitch;
            if update_flags.contains(UpdateFlags::PITCH) {
                pitch = Some(read_angle(reader)?);
            } else {
                pitch = None;
            }

            let origin_y;
            if update_flags.contains(UpdateFlags::ORIGIN_Y) {
                origin_y = Some(read_coord(reader)?);
            } else {
                origin_y = None;
            }

            let yaw;
            if update_flags.contains(UpdateFlags::YAW) {
                yaw = Some(read_angle(reader)?);
            } else {
                yaw = None;
            }

            let origin_z;
            if update_flags.contains(UpdateFlags::ORIGIN_Z) {
                origin_z = Some(read_coord(reader)?);
            } else {
                origin_z = None;
            }

            let roll;
            if update_flags.contains(UpdateFlags::ROLL) {
                roll = Some(read_angle(reader)?);
            } else {
                roll = None;
            }

            let no_lerp = update_flags.contains(UpdateFlags::NO_LERP);

            return Ok(Some(ServerCmd::FastUpdate(EntityUpdate {
                ent_id,
                model_id,
                frame_id,
                colormap,
                skin_id,
                effects,
                origin_x,
                pitch,
                origin_y,
                yaw,
                origin_z,
                roll,
                no_lerp,
            })));
        }

        let code = match ServerCmdCode::from_u8(code_num) {
            Some(c) => c,
            None => {
                return Err(NetError::InvalidData(format!(
                    "Invalid server command code: {}",
                    code_num
                )))
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
                        return Err(NetError::InvalidData(format!(
                            "SoundFlags: {:b}",
                            flags_bits
                        )))
                    }
                };

                let volume = match flags.contains(SoundFlags::VOLUME) {
                    true => Some(reader.read_u8()?),
                    false => None,
                };

                let attenuation = match flags.contains(SoundFlags::ATTENUATION) {
                    true => Some(reader.read_u8()? as f32 * SOUND_ATTENUATION_READ_FACTOR),
                    false => None,
                };

                let entity_channel = reader.read_i16::<LittleEndian>()?;
                let entity_id = (entity_channel >> 3) as u16;
                let channel = (entity_channel & 0b111) as i8;
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
                        return Err(NetError::InvalidData(format!(
                            "Invalid game type ({})",
                            game_type_code
                        )))
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

            ServerCmdCode::PlayerData => {
                let flags_bits = reader.read_u16::<LittleEndian>()?;
                let flags = match ClientUpdateFlags::from_bits(flags_bits) {
                    Some(f) => f,
                    None => {
                        return Err(NetError::InvalidData(format!(
                            "client update flags: {:b}",
                            flags_bits
                        )))
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

                let items_bits = reader.read_u32::<LittleEndian>()?;
                let items = match ItemFlags::from_bits(items_bits) {
                    Some(i) => i,
                    None => {
                        return Err(NetError::InvalidData(format!(
                            "ItemFlags: {:b}",
                            items_bits
                        )))
                    }
                };

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

                ServerCmd::PlayerData(PlayerData {
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
                })
            }

            ServerCmdCode::StopSound => {
                let entity_channel = reader.read_u16::<LittleEndian>()?;
                let entity_id = entity_channel >> 3;
                let channel = (entity_channel & 0b111) as u8;

                ServerCmd::StopSound { entity_id, channel }
            }

            ServerCmdCode::UpdateColors => {
                let player_id = reader.read_u8()?;
                let new_colors_bits = reader.read_u8()?;
                let new_colors = PlayerColor::from_bits(new_colors_bits);

                ServerCmd::UpdateColors {
                    player_id,
                    new_colors,
                }
            }

            ServerCmdCode::Particle => {
                let origin = read_coord_vector3(reader)?;

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
                let source = read_coord_vector3(reader)?;

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
                        return Err(NetError::InvalidData(format!(
                            "Invalid value for sign-on stage: {}",
                            stage_num
                        )))
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
                let origin = read_coord_vector3(reader)?;
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
                    writer.write_u8(a as u8 * SOUND_ATTENUATION_WRITE_FACTOR)?;
                }

                // TODO: document this better. The entity and channel fields are combined in Sound commands.
                let ent_channel = (entity_id as i16) << 3 | channel as i16 & 0b111;
                writer.write_i16::<LittleEndian>(ent_channel)?;

                writer.write_u8(sound_id)?;

                for component in 0..3 {
                    write_coord(writer, position[component])?;
                }
            }

            ServerCmd::Time { time } => writer.write_f32::<LittleEndian>(time)?,

            ServerCmd::Print { ref text } => {
                writer.write(text.as_bytes())?;
                writer.write_u8(0)?;
            }

            ServerCmd::StuffText { ref text } => {
                writer.write(text.as_bytes())?;
                writer.write_u8(0)?;
            }

            ServerCmd::SetAngle { angles } => write_angle_vector3(writer, angles)?,

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

            ServerCmd::PlayerData(PlayerData {
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
            }) => {
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
                writer.write_u32::<LittleEndian>(items.bits())?;
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

            ServerCmd::UpdateColors {
                player_id,
                new_colors,
            } => {
                writer.write_u8(player_id)?;
                writer.write_u8(new_colors.bits())?;
            }

            ServerCmd::Particle {
                origin,
                direction,
                count,
                color,
            } => {
                write_coord_vector3(writer, origin)?;

                for i in 0..3 {
                    writer.write_i8(match direction[i] * PARTICLE_DIRECTION_WRITE_FACTOR {
                        d if d > ::std::i8::MAX as f32 => ::std::i8::MAX,
                        d if d < ::std::i8::MIN as f32 => ::std::i8::MIN,
                        d => d as i8,
                    })?;
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
                write_coord_vector3(writer, source)?;
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
                write_coord_vector3(writer, origin)?;
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

            // TODO
            ServerCmd::FastUpdate(_) => unimplemented!(),
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

#[derive(Debug, PartialEq)]
pub enum ClientCmd {
    Bad,
    NoOp,
    Disconnect,
    Move {
        send_time: Duration,
        angles: Vector3<Deg<f32>>,
        fwd_move: i16,
        side_move: i16,
        up_move: i16,
        button_flags: ButtonFlags,
        impulse: u8,
    },
    StringCmd {
        cmd: String,
    },
}

impl ClientCmd {
    pub fn code(&self) -> u8 {
        match *self {
            ClientCmd::Bad => ClientCmdCode::Bad as u8,
            ClientCmd::NoOp => ClientCmdCode::NoOp as u8,
            ClientCmd::Disconnect => ClientCmdCode::Disconnect as u8,
            ClientCmd::Move { .. } => ClientCmdCode::Move as u8,
            ClientCmd::StringCmd { .. } => ClientCmdCode::StringCmd as u8,
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
                return Err(NetError::InvalidData(format!(
                    "Invalid client command code: {}",
                    code_val
                )))
            }
        };

        let cmd = match code {
            ClientCmdCode::Bad => ClientCmd::Bad,
            ClientCmdCode::NoOp => ClientCmd::NoOp,
            ClientCmdCode::Disconnect => ClientCmd::Disconnect,
            ClientCmdCode::Move => {
                let send_time = engine::duration_from_f32(reader.read_f32::<LittleEndian>()?);
                let angles = Vector3::new(
                    read_angle(reader)?,
                    read_angle(reader)?,
                    read_angle(reader)?,
                );
                let fwd_move = reader.read_i16::<LittleEndian>()?;
                let side_move = reader.read_i16::<LittleEndian>()?;
                let up_move = reader.read_i16::<LittleEndian>()?;
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
                ClientCmd::Move {
                    send_time,
                    angles,
                    fwd_move,
                    side_move,
                    up_move,
                    button_flags,
                    impulse,
                }
            }
            ClientCmdCode::StringCmd => {
                let cmd = util::read_cstring(reader).unwrap();
                ClientCmd::StringCmd { cmd }
            }
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
            ClientCmd::Move {
                send_time,
                angles,
                fwd_move,
                side_move,
                up_move,
                button_flags,
                impulse,
            } => {
                writer.write_f32::<LittleEndian>(engine::duration_to_f32(send_time))?;
                write_angle_vector3(writer, angles)?;
                writer.write_i16::<LittleEndian>(fwd_move)?;
                writer.write_i16::<LittleEndian>(side_move)?;
                writer.write_i16::<LittleEndian>(up_move)?;
                writer.write_u8(button_flags.bits())?;
                writer.write_u8(impulse)?;
            }
            ClientCmd::StringCmd { ref cmd } => {
                writer.write(cmd.as_bytes())?;
                writer.write_u8(0)?;
            }
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
                "begin_send_msg: previous message unacknowledged",
            ));
        }

        // empty messages are an error
        if msg.len() == 0 {
            return Err(NetError::with_msg(
                "begin_send_msg: Input data has zero length",
            ));
        }

        // check upper message length bound
        if msg.len() > MAX_MESSAGE {
            return Err(NetError::with_msg(
                "begin_send_msg: Input data exceeds MAX_MESSAGE",
            ));
        }

        // split the message into chunks and enqueue them
        for chunk in msg.chunks(MAX_DATAGRAM) {
            self.send_queue
                .push_back(chunk.to_owned().into_boxed_slice());
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
        let content = self
            .send_queue
            .pop_front()
            .expect("Send queue is empty (this is a bug)");

        // if this was the last chunk, set the EOM flag
        let msg_kind = match self.send_queue.is_empty() {
            true => MsgKind::ReliableEom,
            false => MsgKind::Reliable,
        };

        // compose the packet
        let mut compose = Vec::with_capacity(MAX_PACKET);
        compose.write_u16::<NetworkEndian>(msg_kind as u16)?;
        compose.write_u16::<NetworkEndian>((HEADER_SIZE + content.len()) as u16)?;
        compose.write_u32::<NetworkEndian>(self.send_sequence)?;
        compose.write_all(&content)?;

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
        packet.write_u16::<NetworkEndian>(MsgKind::Unreliable as u16)?;
        packet.write_u16::<NetworkEndian>(packet_len as u16)?;
        packet.write_u32::<NetworkEndian>(self.unreliable_send_sequence)?;
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
                self.socket.set_nonblocking(false)?;
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
                    src_addr, self.remote
                );
                continue;
            }

            let mut reader = BufReader::new(Cursor::new(&self.recv_buf[..packet_len]));

            let msg_kind_code = reader.read_u16::<NetworkEndian>()?;
            let msg_kind = match MsgKind::from_u16(msg_kind_code) {
                Some(f) => f,
                None => {
                    return Err(NetError::InvalidData(format!(
                        "Invalid message kind: {}",
                        msg_kind_code
                    )))
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
                    field_len, packet_len
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
                            drop_count, sequence, self.unreliable_recv_sequence
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

fn read_coord_vector3<R>(reader: &mut R) -> Result<Vector3<f32>, NetError>
where
    R: BufRead + ReadBytesExt,
{
    Ok(Vector3::new(
        read_coord(reader)?,
        read_coord(reader)?,
        read_coord(reader)?,
    ))
}

fn write_coord<W>(writer: &mut W, coord: f32) -> Result<(), NetError>
where
    W: WriteBytesExt,
{
    writer.write_i16::<LittleEndian>((coord * 8.0) as i16)?;
    Ok(())
}

fn write_coord_vector3<W>(writer: &mut W, coords: Vector3<f32>) -> Result<(), NetError>
where
    W: WriteBytesExt,
{
    for coord in &coords[..] {
        write_coord(writer, *coord)?;
    }

    Ok(())
}

fn read_angle<R>(reader: &mut R) -> Result<Deg<f32>, NetError>
where
    R: BufRead + ReadBytesExt,
{
    Ok(Deg(reader.read_i8()? as f32 * (360.0 / 256.0)))
}

fn read_angle_vector3<R>(reader: &mut R) -> Result<Vector3<Deg<f32>>, NetError>
where
    R: BufRead + ReadBytesExt,
{
    Ok(Vector3::new(
        read_angle(reader)?,
        read_angle(reader)?,
        read_angle(reader)?,
    ))
}

fn write_angle<W>(writer: &mut W, angle: Deg<f32>) -> Result<(), NetError>
where
    W: WriteBytesExt,
{
    writer.write_u8(((angle.0 as i32 * 256 / 360) & 0xFF) as u8)?;
    Ok(())
}

fn write_angle_vector3<W>(writer: &mut W, angles: Vector3<Deg<f32>>) -> Result<(), NetError>
where
    W: WriteBytesExt,
{
    for angle in &angles[..] {
        write_angle(writer, *angle)?;
    }

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
        let src = ServerCmd::Print {
            text: String::from("print test"),
        };

        let mut packet = Vec::new();
        src.serialize(&mut packet).unwrap();
        let mut reader = BufReader::new(packet.as_slice());
        let dst = ServerCmd::deserialize(&mut reader).unwrap().unwrap();

        assert_eq!(src, dst);
    }

    #[test]
    fn test_server_cmd_stuff_text_read_write_eq() {
        let src = ServerCmd::StuffText {
            text: String::from("stufftext test"),
        };

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
            new_colors: PlayerColor::new(4, 13),
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
        let src = ServerCmd::SignOnStage {
            stage: SignOnStage::Begin,
        };
        let mut packet = Vec::new();
        src.serialize(&mut packet).unwrap();
        let mut reader = BufReader::new(packet.as_slice());
        let dst = ServerCmd::deserialize(&mut reader).unwrap().unwrap();

        assert_eq!(src, dst);
    }

    #[test]
    fn test_server_cmd_center_print_read_write_eq() {
        let src = ServerCmd::CenterPrint {
            text: String::from("Center print test"),
        };
        let mut packet = Vec::new();
        src.serialize(&mut packet).unwrap();
        let mut reader = BufReader::new(packet.as_slice());
        let dst = ServerCmd::deserialize(&mut reader).unwrap().unwrap();

        assert_eq!(src, dst);
    }

    #[test]
    fn test_server_cmd_finale_read_write_eq() {
        let src = ServerCmd::Finale {
            text: String::from("Finale test"),
        };
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
        let src = ServerCmd::Cutscene {
            text: String::from("Cutscene test"),
        };
        let mut packet = Vec::new();
        src.serialize(&mut packet).unwrap();
        let mut reader = BufReader::new(packet.as_slice());
        let dst = ServerCmd::deserialize(&mut reader).unwrap().unwrap();

        assert_eq!(src, dst);
    }

    #[test]
    fn test_client_cmd_string_cmd_read_write_eq() {
        let src = ClientCmd::StringCmd {
            cmd: String::from("StringCmd test"),
        };
        let mut packet = Vec::new();
        src.serialize(&mut packet).unwrap();
        let mut reader = BufReader::new(packet.as_slice());
        let dst = ClientCmd::deserialize(&mut reader).unwrap();

        assert_eq!(src, dst);
    }

    #[test]
    fn test_client_cmd_move_read_write_eq() {
        let src = ClientCmd::Move {
            send_time: Duration::milliseconds(1234),
            // have to use angles that won't lose precision from write_angle
            angles: Vector3::new(Deg(90.0), Deg(-90.0), Deg(0.0)),
            fwd_move: 27,
            side_move: 85,
            up_move: 76,
            button_flags: ButtonFlags::empty(),
            impulse: 121,
        };

        let mut packet = Vec::new();
        src.serialize(&mut packet).unwrap();
        let mut reader = BufReader::new(packet.as_slice());
        let dst = ClientCmd::deserialize(&mut reader).unwrap();

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
        let received = dst.recv_msg(BlockingMode::Blocking).unwrap();
        assert_eq!(message, received);

        // TODO: assert can_send == true, send_next == false, etc
    }

    #[test]
    fn test_qsocket_send_msg_unreliable_recv_msg_eq() {
        let (mut src, mut dst) = gen_qsocket_pair();

        let message = String::from("test message").into_bytes();
        src.send_msg_unreliable(&message).unwrap();
        let received = dst.recv_msg(BlockingMode::Blocking).unwrap();
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
