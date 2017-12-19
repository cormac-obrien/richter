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
use std::io::BufRead;
use std::io::Cursor;
use std::mem::size_of;
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use std::net::UdpSocket;

use util;

use byteorder::LittleEndian;
use byteorder::ReadBytesExt;
use byteorder::WriteBytesExt;
use cgmath::Deg;
use cgmath::Vector3;
use num::FromPrimitive;

const MAX_NET_MESSAGE: usize = 8192;
const MAX_DATAGRAM: usize = 1024;
const NAME_LEN: usize = 64;
const HEADER_SIZE: usize = 8;
const DATAGRAM_SIZE: usize = HEADER_SIZE + MAX_DATAGRAM;
const PROTOCOL_VERSION: i32 = 15;

const VELOCITY_READ_FACTOR: f32 = 16.0;
const VELOCITY_WRITE_FACTOR: f32 = 1.0 / VELOCITY_READ_FACTOR;

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

/// A trait for in-game server and client network commands.
pub trait Cmd: Sized {
    /// Returns the numeric value of this command's code.
    fn code(&self) -> u8;

    /// Reads data from the given source and constructs a command object.
    fn read_content<R>(reader: &mut R) -> Result<Self, NetError>
    where
        R: BufRead + ReadBytesExt;

    /// Writes this command's content to the given sink.
    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt;

    /// Writes this command to the given sink.
    fn write_cmd<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        writer.write_u8(self.code())?;
        self.write_content(writer)?;
        Ok(())
    }
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

#[derive(Debug, Eq, PartialEq)]
pub struct ServerCmdUpdateStat {
    stat: ClientStat,
    value: i32,
}

impl Cmd for ServerCmdUpdateStat {
    fn code(&self) -> u8 {
        ServerCmdCode::UpdateStat as u8
    }

    fn read_content<R>(reader: &mut R) -> Result<ServerCmdUpdateStat, NetError>
    where
        R: BufRead + ReadBytesExt,
    {
        let stat_id = reader.read_u8()?;
        let stat = match ClientStat::from_u8(stat_id) {
            Some(c) => c,
            None => {
                return Err(NetError::with_msg(format!(
                    "Invalid value for ClientStat: {}",
                    stat_id,
                )))
            }
        };
        let value = reader.read_i32::<LittleEndian>()?;

        Ok(ServerCmdUpdateStat { stat, value })
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        writer.write_u8(self.stat as u8)?;
        writer.write_i32::<LittleEndian>(self.value)?;
        Ok(())
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct ServerCmdVersion {
    version: i32,
}

impl Cmd for ServerCmdVersion {
    fn code(&self) -> u8 {
        ServerCmdCode::Version as u8
    }

    fn read_content<R>(reader: &mut R) -> Result<ServerCmdVersion, NetError>
    where
        R: BufRead + ReadBytesExt,
    {
        let version = reader.read_i32::<LittleEndian>()?;
        Ok(ServerCmdVersion { version })
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        writer.write_i32::<LittleEndian>(self.version)?;
        Ok(())
    }
}

pub struct ServerCmdSetView {
    view_ent: i16,
}

impl Cmd for ServerCmdSetView {
    fn code(&self) -> u8 {
        ServerCmdCode::SetView as u8
    }

    fn read_content<R>(reader: &mut R) -> Result<ServerCmdSetView, NetError>
    where
        R: BufRead + ReadBytesExt,
    {
        let view_ent = reader.read_i16::<LittleEndian>()?;
        Ok(ServerCmdSetView { view_ent })
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        writer.write_i16::<LittleEndian>(self.view_ent)?;
        Ok(())
    }
}

pub struct ServerCmdSound {
    volume: Option<u8>,
    attenuation: Option<u8>,
    entity_id: u16,
    channel: u8,
    sound_id: u8,
    position: Vector3<f32>,
}

impl Cmd for ServerCmdSound {
    fn code(&self) -> u8 {
        ServerCmdCode::Sound as u8
    }

    fn read_content<R>(reader: &mut R) -> Result<ServerCmdSound, NetError>
    where
        R: BufRead + ReadBytesExt,
    {
        let flags_bits = reader.read_u8()?;
        let flags = match SoundFlags::from_bits(flags_bits) {
            Some(f) => f,
            None => {
                return Err(NetError::with_msg(
                    format!("Invalid value for SoundFlags: {:b}", flags_bits),
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

        Ok(ServerCmdSound {
            volume,
            attenuation,
            entity_id,
            channel,
            sound_id,
            position,
        })
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        let mut sound_flags = SoundFlags::empty();

        if self.volume.is_some() {
            sound_flags |= SoundFlags::VOLUME;
        }

        if self.attenuation.is_some() {
            sound_flags |= SoundFlags::ATTENUATION;
        }

        writer.write_u8(sound_flags.bits())?;

        if let Some(v) = self.volume {
            writer.write_u8(v)?;
        }

        if let Some(a) = self.attenuation {
            writer.write_u8(a)?;
        }

        // TODO: document this better. The entity and channel fields are combined in Sound commands.
        let ent_channel = (self.entity_id as i16) << 3 | self.channel as i16 & 0b111;
        writer.write_i16::<LittleEndian>(ent_channel)?;

        writer.write_u8(self.sound_id)?;

        for component in 0..3 {
            write_coord(writer, self.position[component])?;
        }

        Ok(())
    }
}

pub struct ServerCmdTime {
    time: f32,
}

impl Cmd for ServerCmdTime {
    fn code(&self) -> u8 {
        ServerCmdCode::Time as u8
    }

    fn read_content<R>(reader: &mut R) -> Result<ServerCmdTime, NetError>
    where
        R: BufRead + ReadBytesExt,
    {
        let time = reader.read_f32::<LittleEndian>()?;
        Ok(ServerCmdTime { time })
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        writer.write_f32::<LittleEndian>(self.time)?;
        Ok(())
    }
}

pub struct ServerCmdPrint {
    text: String,
}

impl Cmd for ServerCmdPrint {
    fn code(&self) -> u8 {
        ServerCmdCode::Print as u8
    }

    fn read_content<R>(reader: &mut R) -> Result<ServerCmdPrint, NetError>
    where
        R: BufRead + ReadBytesExt,
    {
        let text = match util::read_cstring(reader) {
            Ok(t) => t,
            Err(e) => return Err(NetError::with_msg(format!("{}", e))),
        };

        Ok(ServerCmdPrint { text })
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        writer.write(self.text.as_bytes())?;
        writer.write_u8(0)?;
        Ok(())
    }
}

pub struct ServerCmdStuffText {
    text: String,
}

impl Cmd for ServerCmdStuffText {
    fn code(&self) -> u8 {
        ServerCmdCode::StuffText as u8
    }

    fn read_content<R>(reader: &mut R) -> Result<ServerCmdStuffText, NetError>
    where
        R: BufRead + ReadBytesExt,
    {
        let text = match util::read_cstring(reader) {
            Ok(t) => t,
            Err(e) => return Err(NetError::with_msg(format!("{}", e))),
        };

        Ok(ServerCmdStuffText { text })
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        writer.write(self.text.as_bytes())?;
        writer.write_u8(0)?;
        Ok(())
    }
}

pub struct ServerCmdSetAngle {
    angles: Vector3<Deg<f32>>,
}

impl Cmd for ServerCmdSetAngle {
    fn code(&self) -> u8 {
        ServerCmdCode::SetAngle as u8
    }

    fn read_content<R>(reader: &mut R) -> Result<ServerCmdSetAngle, NetError>
    where
        R: BufRead + ReadBytesExt,
    {
        let angles = Vector3::new(
            read_angle(reader)?,
            read_angle(reader)?,
            read_angle(reader)?,
        );
        Ok(ServerCmdSetAngle { angles })
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        for i in 0..3 {
            write_angle(writer, self.angles[i])?;
        }
        Ok(())
    }
}

pub struct ServerCmdServerInfo {
    protocol_version: i32,
    max_clients: u8,
    game_type: u8,
    model_precache: Vec<String>,
    sound_precache: Vec<String>,
}

impl Cmd for ServerCmdServerInfo {
    fn code(&self) -> u8 {
        ServerCmdCode::ServerInfo as u8
    }

    fn read_content<R>(reader: &mut R) -> Result<ServerCmdServerInfo, NetError>
    where
        R: BufRead + ReadBytesExt,
    {
        let protocol_version = reader.read_i32::<LittleEndian>()?;
        let max_clients = reader.read_u8()?;
        let game_type = reader.read_u8()?;

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

        Ok(ServerCmdServerInfo {
            protocol_version,
            max_clients,
            game_type,
            model_precache,
            sound_precache,
        })
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        writer.write_i32::<LittleEndian>(self.protocol_version)?;
        writer.write_u8(self.max_clients)?;
        writer.write_u8(self.game_type)?;

        for model_name in self.model_precache.iter() {
            writer.write(model_name.as_bytes())?;
            writer.write_u8(0)?;
        }
        writer.write_u8(0)?;

        for sound_name in self.sound_precache.iter() {
            writer.write(sound_name.as_bytes())?;
            writer.write_u8(0)?;
        }
        writer.write_u8(0)?;

        Ok(())
    }
}

pub struct ServerCmdLightStyle {
    id: u8,
    value: String,
}

impl Cmd for ServerCmdLightStyle {
    fn code(&self) -> u8 {
        ServerCmdCode::LightStyle as u8
    }

    fn read_content<R>(reader: &mut R) -> Result<ServerCmdLightStyle, NetError>
    where
        R: BufRead + ReadBytesExt,
    {
        unimplemented!();
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        unimplemented!();
    }
}

pub struct ServerCmdUpdateName {
    player_id: u8,
    new_name: String,
}

impl Cmd for ServerCmdUpdateName {
    fn code(&self) -> u8 {
        ServerCmdCode::UpdateName as u8
    }

    fn read_content<R>(reader: &mut R) -> Result<ServerCmdUpdateName, NetError>
    where
        R: BufRead + ReadBytesExt,
    {
        unimplemented!();
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        unimplemented!();
    }
}

pub struct ServerCmdUpdateFrags {
    player_id: u8,
    new_frags: i16,
}

impl Cmd for ServerCmdUpdateFrags {
    fn code(&self) -> u8 {
        ServerCmdCode::UpdateFrags as u8
    }

    fn read_content<R>(reader: &mut R) -> Result<ServerCmdUpdateFrags, NetError>
    where
        R: BufRead + ReadBytesExt,
    {
        unimplemented!();
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        unimplemented!();
    }
}

pub struct ServerCmdClientData {
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
}

impl Cmd for ServerCmdClientData {
    fn code(&self) -> u8 {
        ServerCmdCode::ClientData as u8
    }

    fn read_content<R>(reader: &mut R) -> Result<ServerCmdClientData, NetError>
    where
        R: BufRead + ReadBytesExt,
    {
        let flags_bits = reader.read_u16::<LittleEndian>()?;
        let flags = match ClientUpdateFlags::from_bits(flags_bits) {
            Some(f) => f,
            None => {
                return Err(NetError::with_msg(format!(
                    "Invalid value for client update flags: {:b}",
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

        Ok(ServerCmdClientData {
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

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        let mut flags = ClientUpdateFlags::empty();
        if self.view_height.is_some() {
            flags |= ClientUpdateFlags::VIEW_HEIGHT;
        }
        if self.ideal_pitch.is_some() {
            flags |= ClientUpdateFlags::IDEAL_PITCH;
        }
        if self.punch_pitch.is_some() {
            flags |= ClientUpdateFlags::PUNCH_PITCH;
        }
        if self.velocity_x.is_some() {
            flags |= ClientUpdateFlags::VELOCITY_X;
        }
        if self.punch_yaw.is_some() {
            flags |= ClientUpdateFlags::PUNCH_YAW;
        }
        if self.velocity_y.is_some() {
            flags |= ClientUpdateFlags::VELOCITY_Y;
        }
        if self.punch_roll.is_some() {
            flags |= ClientUpdateFlags::PUNCH_ROLL;
        }
        if self.velocity_z.is_some() {
            flags |= ClientUpdateFlags::VELOCITY_Z;
        }

        // items are always sent
        flags |= ClientUpdateFlags::ITEMS;

        if self.on_ground {
            flags |= ClientUpdateFlags::ON_GROUND;
        }
        if self.in_water {
            flags |= ClientUpdateFlags::IN_WATER;
        }
        if self.weapon_frame.is_some() {
            flags |= ClientUpdateFlags::WEAPON_FRAME;
        }
        if self.armor.is_some() {
            flags |= ClientUpdateFlags::ARMOR;
        }
        if self.weapon.is_some() {
            flags |= ClientUpdateFlags::WEAPON;
        }

        // write flags
        writer.write_u16::<LittleEndian>(flags.bits())?;

        if let Some(vh) = self.view_height {
            writer.write_u8(vh as i32 as u8)?;
        }
        if let Some(ip) = self.ideal_pitch {
            writer.write_u8(ip.0 as i32 as u8)?;
        }
        if let Some(pp) = self.punch_pitch {
            writer.write_u8(pp.0 as i32 as u8)?;
        }
        if let Some(vx) = self.velocity_x {
            writer.write_u8((vx * VELOCITY_WRITE_FACTOR) as i32 as u8)?;
        }
        if let Some(py) = self.punch_yaw {
            writer.write_u8(py.0 as i32 as u8)?;
        }
        if let Some(vy) = self.velocity_y {
            writer.write_u8((vy * VELOCITY_WRITE_FACTOR) as i32 as u8)?;
        }
        if let Some(pr) = self.punch_roll {
            writer.write_u8(pr.0 as i32 as u8)?;
        }
        if let Some(vz) = self.velocity_z {
            writer.write_u8((vz * VELOCITY_WRITE_FACTOR) as i32 as u8)?;
        }
        writer.write_i32::<LittleEndian>(self.items)?;
        if let Some(wf) = self.weapon_frame {
            writer.write_u8(wf)?;
        }
        if let Some(a) = self.armor {
            writer.write_u8(a)?;
        }
        if let Some(w) = self.weapon {
            writer.write_u8(w)?;
        }
        writer.write_i16::<LittleEndian>(self.health)?;
        writer.write_u8(self.ammo)?;
        writer.write_u8(self.ammo_shells)?;
        writer.write_u8(self.ammo_nails)?;
        writer.write_u8(self.ammo_rockets)?;
        writer.write_u8(self.ammo_cells)?;
        writer.write_u8(self.active_weapon)?;

        Ok(())
    }
}

pub struct ServerCmdStopSound {
    entity_id: u16,
    channel: u8,
}

impl Cmd for ServerCmdStopSound {
    fn code(&self) -> u8 {
        ServerCmdCode::StopSound as u8
    }

    fn read_content<R>(reader: &mut R) -> Result<ServerCmdStopSound, NetError>
    where
        R: BufRead + ReadBytesExt,
    {
        unimplemented!();
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        unimplemented!();
    }
}

pub struct ServerCmdUpdateColors {
    client_id: u8,
    colors: u8,
}

impl Cmd for ServerCmdUpdateColors {
    fn code(&self) -> u8 {
        ServerCmdCode::UpdateColors as u8
    }

    fn read_content<R>(reader: &mut R) -> Result<ServerCmdUpdateColors, NetError>
    where
        R: BufRead + ReadBytesExt,
    {
        unimplemented!();
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        unimplemented!();
    }
}

pub struct ServerCmdParticle {
    origin: Vector3<f32>,
    direction: Vector3<f32>,
    count: u16,
    color: u8,
}

impl Cmd for ServerCmdParticle {
    fn code(&self) -> u8 {
        ServerCmdCode::Particle as u8
    }

    fn read_content<R>(reader: &mut R) -> Result<ServerCmdParticle, NetError>
    where
        R: BufRead + ReadBytesExt,
    {
        unimplemented!();
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        unimplemented!();
    }
}

pub struct ServerCmdDamage {
    armor: u8,
    blood: u8,
    source: Vector3<f32>,
}

impl Cmd for ServerCmdDamage {
    fn code(&self) -> u8 {
        ServerCmdCode::Damage as u8
    }

    fn read_content<R>(reader: &mut R) -> Result<ServerCmdDamage, NetError>
    where
        R: BufRead + ReadBytesExt,
    {
        unimplemented!();
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        unimplemented!();
    }
}

pub struct ServerCmdSpawnStatic {}

impl Cmd for ServerCmdSpawnStatic {
    fn code(&self) -> u8 {
        ServerCmdCode::SpawnStatic as u8
    }

    fn read_content<R>(reader: &mut R) -> Result<ServerCmdSpawnStatic, NetError>
    where
        R: BufRead + ReadBytesExt,
    {
        unimplemented!();
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        unimplemented!();
    }
}

pub struct ServerCmdSpawnBaseline {}

impl Cmd for ServerCmdSpawnBaseline {
    fn code(&self) -> u8 {
        ServerCmdCode::SpawnBaseline as u8
    }

    fn read_content<R>(reader: &mut R) -> Result<ServerCmdSpawnBaseline, NetError>
    where
        R: BufRead + ReadBytesExt,
    {
        unimplemented!();
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        unimplemented!();
    }
}

pub struct ServerCmdTempEntity {}

impl Cmd for ServerCmdTempEntity {
    fn code(&self) -> u8 {
        ServerCmdCode::TempEntity as u8
    }

    fn read_content<R>(reader: &mut R) -> Result<ServerCmdTempEntity, NetError>
    where
        R: BufRead + ReadBytesExt,
    {
        unimplemented!();
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        unimplemented!();
    }
}

pub struct ServerCmdSetPause {}

impl Cmd for ServerCmdSetPause {
    fn code(&self) -> u8 {
        ServerCmdCode::SetPause as u8
    }

    fn read_content<R>(reader: &mut R) -> Result<ServerCmdSetPause, NetError>
    where
        R: BufRead + ReadBytesExt,
    {
        unimplemented!();
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        unimplemented!();
    }
}

pub struct ServerCmdSignOnNum {}

impl Cmd for ServerCmdSignOnNum {
    fn code(&self) -> u8 {
        ServerCmdCode::SignOnNum as u8
    }

    fn read_content<R>(reader: &mut R) -> Result<ServerCmdSignOnNum, NetError>
    where
        R: BufRead + ReadBytesExt,
    {
        unimplemented!();
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        unimplemented!();
    }
}

pub struct ServerCmdCenterPrint {}

impl Cmd for ServerCmdCenterPrint {
    fn code(&self) -> u8 {
        ServerCmdCode::CenterPrint as u8
    }

    fn read_content<R>(reader: &mut R) -> Result<ServerCmdCenterPrint, NetError>
    where
        R: BufRead + ReadBytesExt,
    {
        unimplemented!();
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        unimplemented!();
    }
}

pub struct ServerCmdSpawnStaticSound {}

impl Cmd for ServerCmdSpawnStaticSound {
    fn code(&self) -> u8 {
        ServerCmdCode::SpawnStaticSound as u8
    }

    fn read_content<R>(reader: &mut R) -> Result<ServerCmdSpawnStaticSound, NetError>
    where
        R: BufRead + ReadBytesExt,
    {
        unimplemented!();
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        unimplemented!();
    }
}

pub struct ServerCmdIntermission {}

impl Cmd for ServerCmdIntermission {
    fn code(&self) -> u8 {
        ServerCmdCode::Intermission as u8
    }

    fn read_content<R>(reader: &mut R) -> Result<ServerCmdIntermission, NetError>
    where
        R: BufRead + ReadBytesExt,
    {
        unimplemented!();
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        unimplemented!();
    }
}

pub struct ServerCmdFinale {}

impl Cmd for ServerCmdFinale {
    fn code(&self) -> u8 {
        ServerCmdCode::Finale as u8
    }

    fn read_content<R>(reader: &mut R) -> Result<ServerCmdFinale, NetError>
    where
        R: BufRead + ReadBytesExt,
    {
        unimplemented!();
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        unimplemented!();
    }
}

pub struct ServerCmdCdTrack {}

impl Cmd for ServerCmdCdTrack {
    fn code(&self) -> u8 {
        ServerCmdCode::CdTrack as u8
    }

    fn read_content<R>(reader: &mut R) -> Result<ServerCmdCdTrack, NetError>
    where
        R: BufRead + ReadBytesExt,
    {
        unimplemented!();
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        unimplemented!();
    }
}

pub struct ServerCmdSellScreen {}

impl Cmd for ServerCmdSellScreen {
    fn code(&self) -> u8 {
        ServerCmdCode::SellScreen as u8
    }

    fn read_content<R>(reader: &mut R) -> Result<ServerCmdSellScreen, NetError>
    where
        R: BufRead + ReadBytesExt,
    {
        unimplemented!();
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        unimplemented!();
    }
}

pub struct ServerCmdCutscene {}

impl Cmd for ServerCmdCutscene {
    fn code(&self) -> u8 {
        ServerCmdCode::Cutscene as u8
    }

    fn read_content<R>(reader: &mut R) -> Result<ServerCmdCutscene, NetError>
    where
        R: BufRead + ReadBytesExt,
    {
        unimplemented!();
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        unimplemented!();
    }
}

#[derive(FromPrimitive)]
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
        let src = ServerCmdUpdateStat {
            stat: ClientStat::Nails,
            value: 64,
        };

        let mut packet = Vec::new();
        src.write_content(&mut packet).unwrap();
        let mut reader = BufReader::new(packet.as_slice());
        let dst = ServerCmdUpdateStat::read_content(&mut reader).unwrap();

        assert_eq!(src, dst);
    }

    #[test]
    fn test_server_cmd_version_read_write_eq() {
        let src = ServerCmdVersion { version: 42 };

        let mut packet = Vec::new();
        src.write_content(&mut packet).unwrap();
        let mut reader = BufReader::new(packet.as_slice());
        let dst = ServerCmdVersion::read_content(&mut reader).unwrap();

        assert_eq!(src, dst);
    }
}
