use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use math::Vec3;
use num::FromPrimitive;
use std::collections::HashMap;
use std::default::Default;
use std::io::{Cursor, Read, Write};
use std::str::FromStr;

/// The maximum number of entities per packet, excluding nails.
pub const MAX_PACKET_ENTITIES: usize = 64;

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
