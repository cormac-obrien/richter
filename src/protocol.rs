/// The maximum allowed size of a UDP packet.
pub const PACKET_MAX: usize = 8192;
pub const VERSION: u32 = 28;

const PORT_MASTER: u16 = 27000;
const PORT_CLIENT: u16 = 27001;
const PORT_SERVER: u16 = 27500;

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
