// Copyright © 2017 Cormac O'Brien
//
// Permission is hereby granted, free of charge, to any person obtaining a copy of this software
// and associated documentation files (the "Software"), to deal in the Software without
// restriction, including without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all copies or
// substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING
// BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
// NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
// DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use math::Vec3;
use num::FromPrimitive;
use std::collections::HashMap;
use std::convert::From;
use std::default::Default;
use std::error::Error;
use std::fmt;
use std::io::{self, BufRead, Cursor, Read, Write};
use std::net::{SocketAddr, UdpSocket};
use std::str::{self, FromStr};
use util;

pub const MAX_CLIENTS: usize = 32;

/// The maximum number of entities per packet, excluding nails.
pub const MAX_PACKET_ENTITIES: usize = 64;

pub const MAX_SOUNDS: usize = 256;

pub const MIN_CLIENT_PACKET: usize = 10;
pub const MIN_SERVER_PACKET: usize = 8;

/// The maximum allowed size of a UDP packet.
pub const PACKET_MAX: usize = 8192;
pub const VERSION: u32 = 28;

pub const PORT_MASTER: u16 = 27000;
pub const PORT_CLIENT: u16 = 27001;
pub const PORT_SERVER: u16 = 27500;

const RELIABLE_FLAG: i32 = (1 << 31);

#[derive(Debug)]
pub enum NetworkError {
    Io(io::Error),
    PacketSize(usize),
    Other,
}

impl fmt::Display for NetworkError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            NetworkError::Io(ref err) => write!(f, "Network error: {}", err),
            NetworkError::PacketSize(size) => write!(f, "Invalid packet size ({} bytes)", size),
            NetworkError::Other => write!(f, "Unknown network error"),
        }
    }
}

impl Error for NetworkError {
    fn description(&self) -> &str {
        match *self {
            NetworkError::Io(_) => "I/O error",
            NetworkError::PacketSize(_) => "Invalid packet size",
            NetworkError::Other => "Unknown network error",
        }
    }

    fn cause(&self) -> Option<&Error> {
        match *self {
            NetworkError::Io(ref err) => Some(err),
            NetworkError::PacketSize(_) => None,
            NetworkError::Other => None,
        }
    }
}

impl From<io::Error> for NetworkError {
    fn from(err: io::Error) -> NetworkError {
        NetworkError::Io(err)
    }
}

pub enum Message {
    InBand,
    OutOfBand(Box<[u8]>),
    None
}

#[derive(PartialEq)]
pub enum SockType {
    Client,
    Server
}

pub struct QwSocket {
    // underlying UDP socket
    socket: UdpSocket,

    // what kind of socket this is (client or server)
    sock_type: SockType,

    // remote address this socket is connected to
    remote: SocketAddr,

    // sequence number of the most recently received packet
    in_seq: i32,

    // true if last packet received was reliable
    in_seq_reliable: bool,

    // sequence number of packet most recently acked by remote
    ack: i32,

    // true if remote acked most recently sent reliable packet
    remote_acked_reliable: bool,

    // ?
    out_seq: i32,

    // ?
    out_seq_reliable: bool,

    // last reliable sequence sent
    last_reliable: i32,

    // number of packets dropped this frame
    dropped: i32,

    // number of frames with dropped packets
    drop_count: i32,

    // buffer for messages being composed
    compose: Cursor<Vec<u8>>,

    // buffer for messages being parsed
    parse: Cursor<Vec<u8>>,
}

impl QwSocket {
    pub fn from_raw_parts(socket: UdpSocket, remote: SocketAddr, sock_type: SockType) -> QwSocket {
        QwSocket {
            socket: socket,
            sock_type: sock_type,
            remote: remote,

            in_seq: 0,
            in_seq_reliable: false,
            ack: 0,
            remote_acked_reliable: false,
            out_seq: 0,
            out_seq_reliable: false,
            last_reliable: 0,
            dropped: 0,
            drop_count: 0,

            compose: Cursor::new(Vec::new()),
            parse: Cursor::new(Vec::new()),
        }
    }

    pub fn bind(remote: SocketAddr, sock_type: SockType) -> Result<QwSocket, NetworkError> {
        Ok(QwSocket::from_raw_parts(UdpSocket::bind("127.0.0.1:27001")?, remote, sock_type))
    }

    pub fn out_of_band(&self, data: &[u8]) -> Result<(), NetworkError> {
        let mut buf = Vec::new();
        buf.write_i32::<LittleEndian>(-1)?;
        buf.write(data)?;
        self.socket.send_to(&buf, self.remote)?;
        Ok(())
    }

    pub fn process(&mut self) -> Result<Message, NetworkError> {
        debug!("Processing remote message");

        let mut bytes = [0u8; 8192];
        let (size, _) = self.socket.recv_from(&mut bytes)?;
        let mut data = bytes.to_vec();
        data.truncate(size);

        let min_size = match self.sock_type {
            SockType::Client => MIN_CLIENT_PACKET,
            SockType::Server => MIN_SERVER_PACKET,
        };

        if data.len() < min_size {
            return Err(NetworkError::PacketSize(data.len()));
        }

        let mut buf = Cursor::new(data);

        // sequence number of incoming packet
        debug!("Reading sequence number");
        let mut seq = match buf.read_i32::<LittleEndian>()? {
            -1 => return Ok(Message::OutOfBand(Vec::from(&buf.into_inner()[4..]).into_boxed_slice())),
            x => x
        };

        // most recently acked packet from remote
        debug!("Reading ack number");
        let mut ack = buf.read_i32::<LittleEndian>()?;

        // save reliable flags
        let seq_reliable = seq & RELIABLE_FLAG == RELIABLE_FLAG;
        let ack_reliable = ack & RELIABLE_FLAG == RELIABLE_FLAG;

        // mask off flags
        seq &= !RELIABLE_FLAG;
        ack &= !RELIABLE_FLAG;

        // check for outdated or duplicate packets
        if seq <= self.in_seq {
            // TODO: handle this gracefully
            panic!("Outdated or duplicate packet");
        }

        // how many packets have we missed?
        self.dropped = seq - (self.in_seq + 1);

        // bump drop counter if we missed any packets
        if self.dropped > 0 {
            debug!("Dropped {} packets this frame", self.dropped);
            self.drop_count += 1;
        }

        if ack_reliable == self.in_seq_reliable {
            // TODO: clear reliable message (see net_chan.c line 246)
        }

        self.in_seq = seq;
        self.ack = ack;
        self.remote_acked_reliable = ack_reliable;

        if seq_reliable {
            self.in_seq_reliable = !self.in_seq_reliable;
        }

        let mut payload = Vec::new();
        buf.read_to_end(&mut payload)?;
        self.parse = Cursor::new(payload);

        Ok(Message::InBand)
    }

    pub fn transmit(&mut self) -> Result<(), NetworkError> {
        // TODO: check if message exceeds max length

        // send reliable if remote acked packets past our last reliable
        // or resend reliable if remote hasn't acked it yet
        let send_reliable = self.ack > self.last_reliable && self.remote_acked_reliable != self.out_seq_reliable;

        // TODO: if no current reliable message, copy it out

        let mut buf = Cursor::new(Vec::new());

        // sequence number
        let mut seq = self.out_seq;
        if send_reliable {
            seq |= RELIABLE_FLAG;
        }

        // ack remote's sequence
        let mut ack = self.in_seq;
        if self.in_seq_reliable {
            ack |= RELIABLE_FLAG;
        }

        // write the packet header
        buf.write_i32::<LittleEndian>(seq)?;
        buf.write_i32::<LittleEndian>(ack)?;

        self.out_seq += 1;

        // if this is a client socket, include the qport
        if self.sock_type == SockType::Client {
            // TODO: send qport
            buf.write_i16::<LittleEndian>(0)?;
        }

        if send_reliable {
            // TODO: copy reliable message first
        }

        // TODO: check for max length before copying unreliable message
        buf.write(self.compose.get_ref())?;
        self.socket.send_to(buf.get_ref(), self.remote)?;

        // clear message buffer
        self.compose = Cursor::new(Vec::new());

        Err(NetworkError::Other)
    }
}

impl Read for QwSocket {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.parse.read(buf)
    }
}

impl Write for QwSocket {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.compose.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.compose.flush()
    }
}

#[derive(Debug, FromPrimitive)]
pub enum SvCmd {
    Bad = 0,
    Nop = 1,
    Disconnect = 2,
    UpdateStat = 3,
    // Version = 4,
    SetView = 5,
    Sound = 6,
    // Time = 7,
    Print = 8,
    StuffText = 9,
    SetAngle = 10,
    ServerData = 11,
    LightStyle = 12,
    // UpdateName = 13,
    UpdateFrags = 14,
    // ClientData = 15,
    StopSound = 16,
    // UpdateColors = 17,
    // Particle = 18,
    Damage = 19,
    SpawnStatic = 20,
    // SpawnBinary = 21,
    SpawnBaseline = 22,
    TempEntity = 23,
    SetPause = 24,
    // SigNonNum = 25,
    CenterPrint = 26,
    KilledMonster = 27,
    FoundSecret = 28,
    SpawnStaticSound = 29,
    Intermission = 30,
    Finale = 31,
    CdTrack = 32,
    SellScreen = 33,

    /// set client punchangle to 2
    SmallKick = 34,

    /// set client punchangle to 4
    BigKick = 35,

    UpdatePing = 36,
    UpdateEnterTime = 37,
    UpdateStatLong = 38,
    MuzzleFlash = 39,
    UpdateUserInfo = 40,
    Download = 41,
    PlayerInfo = 42,
    Nails = 43,
    ChokeCount = 44,
    ModelList = 45,
    SoundList = 46,
    PacketEntities = 47,
    DeltaPacketEntities = 48,
    MaxSpeed = 49,
    EntGravity = 50,
    SetInfo = 51,
    ServerInfo = 52,
    UpdatePl = 53,
}

pub enum Packet {
    OutOfBand(OutOfBandPacket),
    NetChan(NetChanPacket),
}

impl Packet {
    pub fn new<'a>(src: &'a [u8]) -> Result<Packet, NetworkError> {
        if src.len() < 4 {
            return Err(NetworkError::Other);
        }

        let mut curs = Cursor::new(src);
        let seq = curs.read_i32::<LittleEndian>()?;

        if seq == -1 {
            return Ok(Packet::OutOfBand(OutOfBandPacket::new(&curs.into_inner()[4..]).unwrap()));
        }

        if src.len() < 8 {
            panic!("Packet is too short for a netchannel packet");
        }

        let ack_seq = curs.read_i32::<LittleEndian>()?;
        let payload = Vec::from(&curs.into_inner()[8..]);

        Ok(Packet::NetChan(NetChanPacket {
            seq: seq,
            ack: ack_seq,
            payload: payload,
        }))
    }
}

pub enum OutOfBandPacket {
    GetChallenge,
    Challenge(i32),
    Connect(ConnectPacket),
    Accept,
    Ping,
    Ack,
    Status,
    Log,
    Rcon,
}

impl OutOfBandPacket {
    pub fn new<'a>(src: &'a [u8]) -> Result<OutOfBandPacket, NetworkError> {
        // TODO: specify a maximum out-of-band packet length
        let mut len = 0;
        while len < src.len() && src[len] != 0 {
            len += 1;
        }

        let cmd_text = String::from(str::from_utf8(&src[..len]).unwrap());
        let mut cmd_args = cmd_text.split_whitespace();

        match cmd_args.next().unwrap() {
            "getchallenge" => Ok(OutOfBandPacket::GetChallenge),
            "ping" | "k" => Ok(OutOfBandPacket::Ping),
            "l" => Ok(OutOfBandPacket::Ack),
            "status" => Ok(OutOfBandPacket::Status),
            "log" => Ok(OutOfBandPacket::Log),
            "connect" => {
                let qwcol = i32::from_str(cmd_args.next().unwrap()).unwrap();
                let qport = u16::from_str(cmd_args.next().unwrap()).unwrap();
                let challenge = i32::from_str(cmd_args.next().unwrap()).unwrap();

                Ok(OutOfBandPacket::Connect(ConnectPacket {
                    qwcol: qwcol,
                    qport: qport,
                    challenge: challenge,
                    userinfo: String::from(cmd_args.next().unwrap()),
                }))
            }
            "rcon" => Ok(OutOfBandPacket::Rcon),
            "j" => Ok(OutOfBandPacket::Accept),
            s => {
                if s.starts_with("c") {
                    Ok(OutOfBandPacket::Challenge(i32::from_str(&s[1..]).unwrap()))
                } else {
                    Err(NetworkError::Other)
                }
            }
        }
    }
}

pub struct ConnectPacket {
    qwcol: i32,
    qport: u16,
    challenge: i32,
    userinfo: String,
}

impl fmt::Display for ConnectPacket {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,
               "qw={} qport={} challenge={} userinfo={}",
               self.qwcol,
               self.qport,
               self.challenge,
               self.userinfo)
    }
}

pub struct NetChanPacket {
    seq: i32,
    ack: i32,
    payload: Vec<u8>,
}

impl NetChanPacket {
    pub fn get_sequence(&self) -> i32 {
        self.seq & !RELIABLE_FLAG
    }

    pub fn get_sequence_reliable(&self) -> bool {
        self.seq & RELIABLE_FLAG == RELIABLE_FLAG
    }

    pub fn get_ack_sequence(&self) -> i32 {
        self.ack & !RELIABLE_FLAG
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

#[derive(Debug, FromPrimitive)]
pub enum PrintType {
    Low = 0,
    Medium = 1,
    High = 2,
    Chat = 3,
}

pub struct PrintPacket {
    ptype: PrintType,
    msg: String,
}

impl PrintPacket {
    pub fn from_bytes<R>(mut src: R) -> Result<PrintPacket, NetworkError>
        where R: BufRead + ReadBytesExt
    {
        let ptype = PrintType::from_u8(src.read_u8()?).unwrap();
        let msg = util::read_cstring(&mut src).unwrap();

        Ok(PrintPacket {
            ptype: ptype,
            msg: msg,
        })
    }
}

bitflags! {
    pub flags PlayerInfoFlags: u16 {
        const PF_MSEC        = 0x0001,
        const PF_COMMAND     = 0x0002,
        const PF_VELOCITY1   = 0x0004,
        const PF_VELOCITY2   = 0x0008,
        const PF_VELOCITY3   = 0x0010,
        const PF_MODEL       = 0x0020,
        const PF_SKINNUM     = 0x0040,
        const PF_EFFECTS     = 0x0080,
        const PF_WEAPONFRAME = 0x0100,
        const PF_DEAD        = 0x0200,
        const PF_GIB         = 0x0400,
        const PF_NOGRAV      = 0x0800,
    }
}

#[derive(Debug)]
pub struct PlayerInfoPacket {
    id: u8,
    flags: PlayerInfoFlags,
    origin: [i16; 3], // TODO: define types for compressed coords, etc.
    frame: u8,
    msec: u8,
    delta: MoveDelta,
    vel: [i16; 3],
    model_id: u8,
    skin_id: u8,
    effects: u8,
    weapon_frame: u8,
}

impl PlayerInfoPacket {
    pub fn from_bytes<R>(mut src: R) -> Result<PlayerInfoPacket, NetworkError>
        where R: BufRead + ReadBytesExt
    {

        let id = src.read_u8().unwrap();
        let flags = PlayerInfoFlags::from_bits(src.read_u16::<LittleEndian>()?).unwrap();

        let mut origin = [0i16; 3];
        for i in 0..origin.len() {
            origin[i] = src.read_i16::<LittleEndian>()?;
        }

        let frame = src.read_u8().unwrap();

        let mut msec = 0;
        if flags.contains(PF_MSEC) {
            msec = src.read_u8().unwrap();
        }

        let mut delta: MoveDelta = Default::default();
        if flags.contains(PF_COMMAND) {
            delta = MoveDelta::from_bytes(&mut src).unwrap();
        }

        let mut vel = [0i16; 3];

        if flags.contains(PF_VELOCITY1) {
            vel[0] = src.read_i16::<LittleEndian>()?;
        }

        if flags.contains(PF_VELOCITY2) {
            vel[1] = src.read_i16::<LittleEndian>()?;
        }

        if flags.contains(PF_VELOCITY3) {
            vel[2] = src.read_i16::<LittleEndian>()?;
        }

        let mut model_id = 0;
        if flags.contains(PF_MODEL) {
            model_id = src.read_u8().unwrap();
        }

        let mut skin_id = 0;
        if flags.contains(PF_SKINNUM) {
            skin_id = src.read_u8().unwrap();
        }

        let mut effects = 0;
        if flags.contains(PF_EFFECTS) {
            effects = src.read_u8().unwrap();
        }

        let mut weapon_frame = 0;
        if flags.contains(PF_WEAPONFRAME) {
            weapon_frame = src.read_u8().unwrap();
        }

        Ok(PlayerInfoPacket {
            id: id,
            flags: flags,
            origin: origin,
            frame: frame,
            msec: msec,
            delta: delta,
            vel: vel,
            model_id: model_id,
            skin_id: skin_id,
            effects: effects,
            weapon_frame: weapon_frame,
        })
    }
}

const PLAYER_SPECTATOR: u8 = 0x80;

pub struct ServerDataPacket {
    proto: i32,
    server_count: i32,
    game_dir: String,
    player_num: u8,
    level_name: String,
    gravity: f32,
    stop_speed: f32,
    max_speed: f32,
    spec_max_speed: f32,
    accelerate: f32,
    air_accelerate: f32,
    water_accelerate: f32,
    friction: f32,
    water_friction: f32,
    ent_gravity: f32,
}

impl ServerDataPacket {
    pub fn from_bytes<R>(mut src: R) -> Result<ServerDataPacket, NetworkError>
        where R: BufRead + ReadBytesExt
    {
        Ok(ServerDataPacket {
            proto: src.read_i32::<LittleEndian>()?,
            server_count: src.read_i32::<LittleEndian>()?,
            game_dir: util::read_cstring(&mut src).unwrap(),
            player_num: src.read_u8().unwrap(),
            level_name: util::read_cstring(&mut src).unwrap(),
            gravity: src.read_f32::<LittleEndian>()?,
            stop_speed: src.read_f32::<LittleEndian>()?,
            max_speed: src.read_f32::<LittleEndian>()?,
            spec_max_speed: src.read_f32::<LittleEndian>()?,
            accelerate: src.read_f32::<LittleEndian>()?,
            air_accelerate: src.read_f32::<LittleEndian>()?,
            water_accelerate: src.read_f32::<LittleEndian>()?,
            friction: src.read_f32::<LittleEndian>()?,
            water_friction: src.read_f32::<LittleEndian>()?,
            ent_gravity: src.read_f32::<LittleEndian>()?,
        })
    }

    pub fn get_protocol_version(&self) -> i32 {
        self.proto
    }

    pub fn get_server_count(&self) -> i32 {
        self.server_count
    }

    pub fn get_game_directory(&self) -> &str {
        &self.game_dir
    }

    pub fn get_player_number(&self) -> u8 {
        self.player_num & !PLAYER_SPECTATOR
    }

    pub fn is_spectator(&self) -> bool {
        self.player_num & PLAYER_SPECTATOR == PLAYER_SPECTATOR
    }

    pub fn get_level_name(&self) -> &str {
        &self.level_name
    }

    pub fn get_gravity(&self) -> f32 {
        self.gravity
    }

    pub fn get_stop_speed(&self) -> f32 {
        self.stop_speed
    }

    pub fn get_max_speed(&self) -> f32 {
        self.max_speed
    }

    pub fn get_spec_max_speed(&self) -> f32 {
        self.spec_max_speed
    }

    pub fn get_accelerate(&self) -> f32 {
        self.accelerate
    }

    pub fn get_air_accelerate(&self) -> f32 {
        self.air_accelerate
    }

    pub fn get_water_accelerate(&self) -> f32 {
        self.water_accelerate
    }

    pub fn get_friction(&self) -> f32 {
        self.friction
    }

    pub fn get_water_friction(&self) -> f32 {
        self.water_friction
    }

    pub fn get_ent_gravity(&self) -> f32 {
        self.ent_gravity
    }
}

pub struct ModelListPacket {
    count: u8,
    list: Vec<String>,

    // either same as count or 0.
    // if same as count, send this to server with next model list request.
    // if 0, we have all the models we need.
    progress: u8,
}

impl ModelListPacket {
    pub fn from_bytes<R>(mut src: R) -> Result<ModelListPacket, NetworkError>
        where R: BufRead + ReadBytesExt
    {
        let mut count = src.read_u8()?;
        let mut list: Vec<String> = Vec::new();

        loop {
            let model_name = util::read_cstring(&mut src).unwrap();
            if model_name.len() == 0 {
                break;
            }
            count += 1;
            list.push(model_name);
        }

        let progress = src.read_u8()?;

        Ok(ModelListPacket {
            count: count,
            list: list,
            progress: progress,
        })
    }

    pub fn get_count(&self) -> u8 {
        self.count
    }

    pub fn get_list(&self) -> &Vec<String> {
        &self.list
    }

    pub fn get_progress(&self) -> u8 {
        self.progress
    }
}

pub struct SoundListPacket {
    count: u8,
    list: Vec<String>,

    // either same as count or 0.
    // if same as count, send this to server with next sound list request.
    // if 0, we have all the sounds we need.
    progress: u8,
}

impl SoundListPacket {
    pub fn from_bytes<R>(mut src: R) -> Result<SoundListPacket, NetworkError>
        where R: BufRead + ReadBytesExt
    {
        let mut count = src.read_u8()?;
        let mut list: Vec<String> = Vec::new();

        loop {
            let sound_name = util::read_cstring(&mut src).unwrap();
            if sound_name.len() == 0 {
                break;
            }
            count += 1;
            list.push(sound_name);
        }

        let progress = src.read_u8()?;

        Ok(SoundListPacket {
            count: count,
            list: list,
            progress: progress,
        })
    }

    pub fn get_count(&self) -> u8 {
        self.count
    }

    pub fn get_list(&self) -> &Vec<String> {
        &self.list
    }

    pub fn get_progress(&self) -> u8 {
        self.progress
    }
}

#[derive(Debug, FromPrimitive)]
pub enum ClCmd {
    Bad = 0,
    Nop = 1,
    // DoubleMove = 2,
    Move = 3,
    StringCmd = 4,
    Delta = 5,
    TMove = 6,
    Upload = 7,
}

#[derive(Debug)]
pub struct MoveCmd {
    crc: u8,
    loss: u8,
    delta: [MoveDelta; 3],
}

// Move command delta flags, https://github.com/id-Software/Quake/blob/master/QW/client/protocol.h#L171
bitflags! {
    pub flags MoveDeltaFlags: u8 {
        const CM_ANGLE1  = 0x01,
        const CM_ANGLE3  = 0x02,
        const CM_FORWARD = 0x04,
        const CM_SIDE    = 0x08,
        const CM_UP      = 0x10,
        const CM_BUTTONS = 0x20,
        const CM_IMPULSE = 0x40,
        const CM_ANGLE2  = 0x80,
    }
}

#[derive(Debug)]
pub struct MoveDelta {
    flags: MoveDeltaFlags,
    angles: [u16; 3],
    moves: [u16; 3],
    buttons: u8,
    impulse: u8,
    msec: u8,
}

impl MoveDelta {
    pub fn from_bytes<R>(mut src: R) -> Result<MoveDelta, NetworkError>
        where R: BufRead + ReadBytesExt
    {
        let flags = MoveDeltaFlags::from_bits(src.read_u8().unwrap()).unwrap();

        let mut angles = [0u16; 3];

        if flags.contains(CM_ANGLE1) {
            angles[0] = src.read_u16::<LittleEndian>()?;
        }

        if flags.contains(CM_ANGLE2) {
            angles[1] = src.read_u16::<LittleEndian>()?;
        }

        if flags.contains(CM_ANGLE3) {
            angles[2] = src.read_u16::<LittleEndian>()?;
        }

        let mut moves = [0u16; 3];

        if flags.contains(CM_FORWARD) {
            moves[0] = src.read_u16::<LittleEndian>()?;
        }

        if flags.contains(CM_SIDE) {
            moves[1] = src.read_u16::<LittleEndian>()?;
        }

        if flags.contains(CM_UP) {
            moves[2] = src.read_u16::<LittleEndian>()?;
        }

        let mut buttons = 0;
        if flags.contains(CM_BUTTONS) {
            buttons = src.read_u8().unwrap();
        }

        let mut impulse = 0;
        if flags.contains(CM_IMPULSE) {
            impulse = src.read_u8().unwrap();
        }

        let msec = src.read_u8().unwrap();

        Ok(MoveDelta {
            flags: flags,
            angles: angles,
            moves: moves,
            buttons: buttons,
            impulse: impulse,
            msec: msec,
        })
    }
}

impl Default for MoveDelta {
    fn default() -> MoveDelta {
        MoveDelta {
            flags: MoveDeltaFlags::empty(),
            angles: [0; 3],
            moves: [0; 3],
            buttons: 0,
            impulse: 0,
            msec: 0,
        }
    }
}

const PROTOCOL_FTE: u32 = ('F' as u32) << 0 | ('T' as u32) << 8 | ('E' as u32) << 16 |
                          ('X' as u32) << 24;

// FTE extensions, https://github.com/mdeguzis/ftequake/blob/master/engine/common/protocol.h#L21
bitflags! {
    pub flags FteExtensions: u32 {
        const FTE_SETVIEW           = 0x00000001,
        const FTE_SCALE             = 0x00000002,
        const FTE_LIGHTSTYLECOL     = 0x00000004,
        const FTE_TRANS             = 0x00000008,
        const FTE_VIEW2             = 0x00000010,
        // const FTE_BULLETENS      = 0x00000020,
        const FTE_ACCURATETIMINGS   = 0x00000040,
        const FTE_SOUNDDBL          = 0x00000080,
        const FTE_FATNESS           = 0x00000100,
        const FTE_HLBSP             = 0x00000200,
        const FTE_TE_BULLET         = 0x00000400,
        const FTE_HULLSIZE          = 0x00000800,
        const FTE_MODELDBL          = 0x00001000,
        const FTE_ENTITYDBL         = 0x00002000,
        const FTE_ENTITYDBL2        = 0x00004000,
        const FTE_FLOATCOORDS       = 0x00008000,
        // const FTE_VWEAP          = 0x00010000,
        const FTE_Q2BSP             = 0x00020000,
        const FTE_Q3BSP             = 0x00040000,
        const FTE_COLOURMOD         = 0x00080000,
        const FTE_SPLITSCREEN       = 0x00100000,
        const FTE_HEXEN2            = 0x00200000,
        const FTE_SPAWNSTATIC2      = 0x00400000,
        const FTE_CUSTOMTEMPEFFECTS = 0x00800000,
        const FTE_256PACKETENTITIES = 0x01000000,
        // const FTE_NEVERUSED      = 0x02000000,
        const FTE_SHOWPIC           = 0x04000000,
        const FTE_SETATTACHMENT     = 0x08000000,
        // const FTE_NEVERUSED      = 0x10000000,
        const FTE_CHUNKEDDOWNLOADS  = 0x20000000,
        const FTE_CSQC              = 0x40000000,
        const FTE_DPFLAGS           = 0x80000000,
        const FTE_BIGUSERINFOS      = 0xffffffff,
    }
}

const PROTOCOL_FTE2: u32 = ('F' as u32) << 0 | ('T' as u32) << 8 | ('E' as u32) << 16 |
                           ('2' as u32) << 24;

// FTE2 extensions, https://github.com/mdeguzis/ftequake/blob/master/engine/common/protocol.h#L73
bitflags! {
    pub flags Fte2Extensions: u32 {
        const FTE2_PRYDONCURSOR      = 0x00000001,
        const FTE2_VOICECHAT         = 0x00000002,
        const FTE2_SETANGLEDELTA     = 0x00000004,
        const FTE2_REPLACEMENTDELTAS = 0x00000008,
        const FTE2_MAXPLAYERS        = 0x00000010,
        const FTE2_PREDINFO          = 0x00000020,
        const FTE2_NEWSIZEENCODING   = 0x00000040,
    }
}

pub struct Challenge {
    pub challenge: i32,
    pub fte_extensions: Option<FteExtensions>,
    pub fte2_extensions: Option<Fte2Extensions>,
}

impl Challenge {
    pub fn serialize(&self) -> Result<Vec<u8>, NetworkError> {
        let mut result = Cursor::new(Vec::new());
        result.write(&self.challenge.to_string().into_bytes()).unwrap();

        if let Some(fte) = self.fte_extensions {
            result.write_u32::<LittleEndian>(fte.bits())?;
        }

        if let Some(fte2) = self.fte2_extensions {
            result.write_u32::<LittleEndian>(fte2.bits())?;
        }

        Ok(result.into_inner())
    }

    pub fn deserialize<A>(data: A) -> Challenge
        where A: AsRef<[u8]>
    {
        let mut result = Challenge {
            challenge: 0,
            fte_extensions: None,
            fte2_extensions: None,
        };

        let data = data.as_ref();
        let mut i: usize = 0;

        if data[i] == '-' as u8 {
            i += 1;
        }

        while i < data.len() && data[i] >= '0' as u8 && data[i] <= '9' as u8 {
            i += 1;
        }

        result.challenge = i32::from_str(::std::str::from_utf8(&data[..i]).unwrap()).unwrap();

        assert!(data[i..].len() <= 16);
        assert!(data[i..].len() % 8 == 0);
        let mut curs = Cursor::new(&data[i..]);

        while let Ok(n) = curs.read_u32::<LittleEndian>() {
            match n {
                PROTOCOL_FTE => {
                    result.fte_extensions =
                        Some(FteExtensions::from_bits(curs.read_u32::<LittleEndian>().unwrap())
                                 .unwrap())
                }
                PROTOCOL_FTE2 => {
                    result.fte2_extensions =
                        Some(Fte2Extensions::from_bits(curs.read_u32::<LittleEndian>().unwrap())
                                 .unwrap())
                }
                _ => panic!("Unrecognized sequence in challenge packet"),
            }
        }

        result
    }
}

pub struct UserInfo(HashMap<String, String>);

impl UserInfo {
    pub fn new() -> UserInfo {
        UserInfo(HashMap::new())
    }

    pub fn serialize(&self) -> String {
        let mut result = String::new();

        for (k, v) in self.0.iter() {
            result.push('\\');
            result.push_str(&k);
            result.push('\\');
            result.push_str(&v);
        }

        result
    }
}

impl Default for UserInfo {
    fn default() -> UserInfo {
        let mut map = HashMap::new();
        map.insert("name".to_owned(), "unnamed".to_owned());
        map.insert("topcolor".to_owned(), "0".to_owned());
        map.insert("bottomcolor".to_owned(), "0".to_owned());
        map.insert("rate".to_owned(), "2500".to_owned());
        map.insert("msg".to_owned(), "1".to_owned());
        UserInfo(map)
    }
}

// TODO: this is a straight translation of the original struct,
// make it nicer. Also see if we can avoid making it Copy/Clone
#[derive(Copy, Clone)]
#[repr(C)]
pub struct UserCmd {
    msec: u8,
    angles: Vec3,
    fwd: i16,
    side: i16,
    back: i16,
    buttons: u8,
    impulse: u8,
}

impl Default for UserCmd {
    fn default() -> UserCmd {
        UserCmd {
            msec: 0,
            angles: Vec3::new(0.0, 0.0, 0.0),
            fwd: 0,
            side: 0,
            back: 0,
            buttons: 0,
            impulse: 0,
        }
    }
}

pub struct EntityState {
    edict_id: usize,
    flags: u32,
    origin: Vec3,
    angles: Vec3,
    model_id: usize,
    frame: usize,
    colormap: u32,
    skin_count: usize,
    effects: u32,
}

pub type PacketEntities = Box<[EntityState]>;
