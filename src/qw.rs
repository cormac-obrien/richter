use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use math::Vec3;
use num::FromPrimitive;
use std::collections::HashMap;
use std::default::Default;
use std::io::{Cursor, Read, Write};
use std::str::FromStr;
use util;

/// The maximum number of entities per packet, excluding nails.
pub const MAX_PACKET_ENTITIES: usize = 64;

pub const MAX_SOUNDS: usize = 256;

/// The maximum allowed size of a UDP packet.
pub const PACKET_MAX: usize = 8192;
pub const VERSION: u32 = 28;

pub const PORT_MASTER: u16 = 27000;
pub const PORT_CLIENT: u16 = 27001;
pub const PORT_SERVER: u16 = 27500;

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
    pub fn from_bytes<'a>(src: &'a [u8]) -> PrintPacket {
        let mut curs = Cursor::new(src);
        let ptype = PrintType::from_u8(curs.read_u8().unwrap()).unwrap();
        let msg = util::read_cstring(&mut curs).unwrap();

        PrintPacket {
            ptype: ptype,
            msg: msg,
        }
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
    pub fn from_bytes<'a>(src: &'a [u8]) -> ServerDataPacket {
        let mut curs = Cursor::new(src);
        ServerDataPacket {
            proto: curs.read_i32::<LittleEndian>().unwrap(),
            server_count: curs.read_i32::<LittleEndian>().unwrap(),
            game_dir: util::read_cstring(&mut curs).unwrap(),
            player_num: curs.read_u8().unwrap(),
            level_name: util::read_cstring(&mut curs).unwrap(),
            gravity: curs.read_f32::<LittleEndian>().unwrap(),
            stop_speed: curs.read_f32::<LittleEndian>().unwrap(),
            max_speed: curs.read_f32::<LittleEndian>().unwrap(),
            spec_max_speed: curs.read_f32::<LittleEndian>().unwrap(),
            accelerate: curs.read_f32::<LittleEndian>().unwrap(),
            air_accelerate: curs.read_f32::<LittleEndian>().unwrap(),
            water_accelerate: curs.read_f32::<LittleEndian>().unwrap(),
            friction: curs.read_f32::<LittleEndian>().unwrap(),
            water_friction: curs.read_f32::<LittleEndian>().unwrap(),
            ent_gravity: curs.read_f32::<LittleEndian>().unwrap(),
        }
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
    pub fn from_bytes<'a>(src: &'a [u8]) -> ModelListPacket {
        let mut curs = Cursor::new(src);
        let mut count = curs.read_u8().unwrap();
        let mut list: Vec<String> = Vec::new();

        loop {
            let model_name = util::read_cstring(&mut curs).unwrap();
            if model_name.len() == 0 {
                break;
            }
            count += 1;
            list.push(model_name);
        }

        let progress = curs.read_u8().unwrap();

        ModelListPacket {
            count: count,
            list: list,
            progress: progress,
        }
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
    pub fn from_bytes<'a>(src: &'a [u8]) -> SoundListPacket {
        let mut curs = Cursor::new(src);
        let mut count = curs.read_u8().unwrap();
        let mut list: Vec<String> = Vec::new();

        loop {
            let sound_name = util::read_cstring(&mut curs).unwrap();
            if sound_name.len() == 0 {
                break;
            }
            count += 1;
            list.push(sound_name);
        }

        let progress = curs.read_u8().unwrap();

        SoundListPacket {
            count: count,
            list: list,
            progress: progress,
        }
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

pub struct MoveDelta {
    flags: MoveDeltaFlags,
    angles: [u16; 3],
    moves: [u16; 3],
    buttons: u8,
    impulse: u8,
    msec: u8,
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
    pub fn serialize(&self) -> Vec<u8> {
        let mut result = Cursor::new(Vec::new());
        result.write(&self.challenge.to_string().into_bytes()).unwrap();

        if let Some(fte) = self.fte_extensions {
            result.write_u32::<LittleEndian>(fte.bits()).unwrap();
        }

        if let Some(fte2) = self.fte2_extensions {
            result.write_u32::<LittleEndian>(fte2.bits()).unwrap();
        }

        result.into_inner()
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
