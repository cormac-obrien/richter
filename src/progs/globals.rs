// Copyright © 2017 Cormac O'Brien.
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

use std::convert::TryInto;
use std::error::Error;
use std::fmt;
use std::rc::Rc;

use progs::EntityId;
use progs::FieldAddr;
use progs::FunctionId;
use progs::GlobalDef;
use progs::StringId;
use progs::StringTable;
use progs::Type;

use byteorder::LittleEndian;
use byteorder::ReadBytesExt;
use byteorder::WriteBytesExt;
use cgmath::Deg;
use cgmath::Euler;
use cgmath::InnerSpace;
use cgmath::Matrix3;
use cgmath::Vector3;
use chrono::Duration;
use num::FromPrimitive;

pub const GLOBAL_RESERVED_START: usize = 0;
pub const GLOBAL_STATIC_START: usize = 28;
pub const GLOBAL_DYNAMIC_START: usize = 64;

pub const GLOBAL_RESERVED_COUNT: usize = GLOBAL_STATIC_START - GLOBAL_RESERVED_START;
pub const GLOBAL_STATIC_COUNT: usize = GLOBAL_DYNAMIC_START - GLOBAL_STATIC_START;

pub const GLOBAL_ADDR_NULL: usize = 0;
pub const GLOBAL_ADDR_RETURN: usize = 1;
pub const GLOBAL_ADDR_ARG_0: usize = 4;
pub const GLOBAL_ADDR_ARG_1: usize = 7;
pub const GLOBAL_ADDR_ARG_2: usize = 10;
pub const GLOBAL_ADDR_ARG_3: usize = 13;
pub const GLOBAL_ADDR_ARG_4: usize = 16;
pub const GLOBAL_ADDR_ARG_5: usize = 19;
pub const GLOBAL_ADDR_ARG_6: usize = 22;
pub const GLOBAL_ADDR_ARG_7: usize = 25;

#[derive(Debug)]
pub enum GlobalsError {
    Io(::std::io::Error),
    Address(isize),
    Other(String),
}

impl GlobalsError {
    pub fn with_msg<S>(msg: S) -> Self
    where
        S: AsRef<str>,
    {
        GlobalsError::Other(msg.as_ref().to_owned())
    }
}

impl fmt::Display for GlobalsError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            GlobalsError::Io(ref err) => {
                write!(f, "I/O error: ")?;
                err.fmt(f)
            }
            GlobalsError::Address(val) => write!(f, "Invalid address ({})", val),
            GlobalsError::Other(ref msg) => write!(f, "{}", msg),
        }
    }
}

impl Error for GlobalsError {
    fn description(&self) -> &str {
        match *self {
            GlobalsError::Io(ref err) => err.description(),
            GlobalsError::Address(_) => "Invalid address",
            GlobalsError::Other(ref msg) => &msg,
        }
    }
}

impl From<::std::io::Error> for GlobalsError {
    fn from(error: ::std::io::Error) -> Self {
        GlobalsError::Io(error)
    }
}

#[derive(FromPrimitive)]
pub enum GlobalAddrFloat {
    Time = 31,
    FrameTime = 32,
    ForceRetouch = 33,
    Deathmatch = 35,
    Coop = 36,
    TeamPlay = 37,
    ServerFlags = 38,
    TotalSecrets = 39,
    TotalMonsters = 40,
    FoundSecrets = 41,
    KilledMonsters = 42,
    Arg0 = 43,
    Arg1 = 44,
    Arg2 = 45,
    Arg3 = 46,
    Arg4 = 47,
    Arg5 = 48,
    Arg6 = 49,
    Arg7 = 50,
    Arg8 = 51,
    Arg9 = 52,
    Arg10 = 53,
    Arg11 = 54,
    Arg12 = 55,
    Arg13 = 56,
    Arg14 = 57,
    Arg15 = 58,
    VForwardX = 59,
    VForwardY = 60,
    VForwardZ = 61,
    VUpX = 62,
    VUpY = 63,
    VUpZ = 64,
    VRightX = 65,
    VRightY = 66,
    VRightZ = 67,
    TraceAllSolid = 68,
    TraceStartSolid = 69,
    TraceFraction = 70,
    TraceEndPosX = 71,
    TraceEndPosY = 72,
    TraceEndPosZ = 73,
    TracePlaneNormalX = 74,
    TracePlaneNormalY = 75,
    TracePlaneNormalZ = 76,
    TracePlaneDist = 77,
    TraceInOpen = 79,
    TraceInWater = 80,
}

#[derive(FromPrimitive)]
pub enum GlobalAddrVector {
    VForward = 59,
    VUp = 62,
    VRight = 65,
    TraceEndPos = 71,
    TracePlaneNormal = 74,
}

#[derive(FromPrimitive)]
pub enum GlobalAddrString {
    MapName = 34,
}

#[derive(FromPrimitive)]
pub enum GlobalAddrEntity {
    Self_ = 28,
    Other = 29,
    World = 30,
    TraceEntity = 78,
    MsgEntity = 81,
}

#[derive(FromPrimitive)]
pub enum GlobalAddrField {}

#[derive(FromPrimitive)]
pub enum GlobalAddrFunction {
    Main = 82,
    StartFrame = 83,
    PlayerPreThink = 84,
    PlayerPostThink = 85,
    ClientKill = 86,
    ClientConnect = 87,
    PutClientInServer = 88,
    ClientDisconnect = 89,
    SetNewArgs = 90,
    SetChangeArgs = 91,
}

#[derive(Debug)]
pub struct GlobalsStatic {
    pub reserved: [[u8; 4]; GLOBAL_RESERVED_COUNT],
    pub self_: EntityId,
    pub other: EntityId,
    pub world: EntityId,
    pub time: Duration,
    pub frame_time: Duration,
    pub force_retouch: f32,
    pub map_name: StringId,
    pub deathmatch: f32,
    pub coop: f32,
    pub team_play: f32,
    pub server_flags: f32,
    pub total_secrets: f32,
    pub total_monsters: f32,
    pub found_secrets: f32,
    pub killed_monsters: f32,
    pub args: [f32; 16],
    pub v_forward: Vector3<f32>,
    pub v_up: Vector3<f32>,
    pub v_right: Vector3<f32>,
    pub trace_all_solid: f32,
    pub trace_start_solid: f32,
    pub trace_fraction: f32,
    pub trace_end_pos: Vector3<f32>,
    pub trace_plane_normal: Vector3<f32>,
    pub trace_plane_dist: f32,
    pub trace_ent: EntityId,
    pub trace_in_open: f32,
    pub trace_in_water: f32,
    pub msg_entity: EntityId,
    pub main: FunctionId,
    pub start_frame: FunctionId,
    pub player_pre_think: FunctionId,
    pub player_post_think: FunctionId,
    pub client_kill: FunctionId,
    pub client_connect: FunctionId,
    pub put_client_in_server: FunctionId,
    pub client_disconnect: FunctionId,
    pub set_new_args: FunctionId,
    pub set_change_args: FunctionId,
}

impl GlobalsStatic {
    pub fn new() -> GlobalsStatic {
        GlobalsStatic {
            reserved: [[0; 4]; GLOBAL_RESERVED_COUNT],
            self_: EntityId(0),
            other: EntityId(0),
            world: EntityId(0),
            time: Duration::zero(),
            frame_time: Duration::zero(),
            force_retouch: 0.0,
            map_name: StringId(0),
            deathmatch: 0.0,
            coop: 0.0,
            team_play: 0.0,
            server_flags: 0.0,
            total_secrets: 0.0,
            total_monsters: 0.0,
            found_secrets: 0.0,
            killed_monsters: 0.0,
            args: [0.0; 16],
            v_forward: Vector3::new(0.0, 0.0, 0.0),
            v_up: Vector3::new(0.0, 0.0, 0.0),
            v_right: Vector3::new(0.0, 0.0, 0.0),
            trace_all_solid: 0.0,
            trace_start_solid: 0.0,
            trace_fraction: 0.0,
            trace_end_pos: Vector3::new(0.0, 0.0, 0.0),
            trace_plane_normal: Vector3::new(0.0, 0.0, 0.0),
            trace_plane_dist: 0.0,
            trace_ent: EntityId(0),
            trace_in_open: 0.0,
            trace_in_water: 0.0,
            msg_entity: EntityId(0),
            main: FunctionId(0),
            start_frame: FunctionId(0),
            player_pre_think: FunctionId(0),
            player_post_think: FunctionId(0),
            client_kill: FunctionId(0),
            client_connect: FunctionId(0),
            put_client_in_server: FunctionId(0),
            client_disconnect: FunctionId(0),
            set_new_args: FunctionId(0),
            set_change_args: FunctionId(0),
        }
    }
}

#[derive(Debug)]
pub struct Globals {
    string_table: Rc<StringTable>,
    defs: Box<[GlobalDef]>,
    addrs: Box<[[u8; 4]]>,
}

impl Globals {
    /// Constructs a new `Globals` object.
    pub fn new(
        string_table: Rc<StringTable>,
        defs: Box<[GlobalDef]>,
        addrs: Box<[[u8; 4]]>,
    ) -> Globals {
        Globals {
            string_table,
            defs,
            addrs,
        }
    }

    /// Performs a type check at `addr` with type `type_`.
    ///
    /// The type check allows checking `QFloat` against `QVector` and vice-versa, since vectors have
    /// overlapping definitions with their x-components (e.g. a vector `origin` and its x-component
    /// `origin_X` will have the same address).
    pub fn type_check(&self, addr: usize, type_: Type) -> Result<(), GlobalsError> {
        match self.defs.iter().find(|def| def.offset as usize == addr) {
            Some(d) => {
                if type_ == d.type_ {
                    return Ok(());
                } else if type_ == Type::QFloat && d.type_ == Type::QVector {
                    return Ok(());
                } else if type_ == Type::QVector && d.type_ == Type::QFloat {
                    return Ok(());
                } else {
                    return Err(GlobalsError::with_msg("type check failed"));
                }
            }
            None => return Ok(()),
        }
    }

    /// Returns a reference to the memory at the given address.
    pub fn get_addr(&self, addr: i16) -> Result<&[u8], GlobalsError> {
        if addr < 0 {
            return Err(GlobalsError::Address(addr as isize));
        }

        let addr = addr as usize;

        if addr > self.addrs.len() {
            return Err(GlobalsError::Address(addr as isize));
        }

        Ok(&self.addrs[addr])
    }

    /// Returns a mutable reference to the memory at the given address.
    pub fn get_addr_mut(&mut self, addr: i16) -> Result<&mut [u8], GlobalsError> {
        if addr < 0 {
            return Err(GlobalsError::Address(addr as isize));
        }

        let addr = addr as usize;

        if addr > self.addrs.len() {
            return Err(GlobalsError::Address(addr as isize));
        }

        Ok(&mut self.addrs[addr])
    }

    /// Returns a copy of the memory at the given address.
    pub fn get_bytes(&self, addr: i16) -> Result<[u8; 4], GlobalsError> {
        if addr < 0 {
            return Err(GlobalsError::Address(addr as isize));
        }

        let addr = addr as usize;

        if addr > self.addrs.len() {
            return Err(GlobalsError::Address(addr as isize));
        }

        Ok(self.addrs[addr])
    }

    /// Writes the provided data to the memory at the given address.
    ///
    /// This can be used to circumvent the type checker in cases where an operation is not dependent
    /// of the type of the data.
    pub fn put_bytes(&mut self, val: [u8; 4], addr: i16) -> Result<(), GlobalsError> {
        if addr < 0 {
            return Err(GlobalsError::Address(addr as isize));
        }

        let addr = addr as usize;

        if addr > self.addrs.len() {
            return Err(GlobalsError::Address(addr as isize));
        }

        self.addrs[addr] = val;
        Ok(())
    }

    /// Loads an `i32` from the given virtual address.
    pub fn get_int(&self, addr: i16) -> Result<i32, GlobalsError> {
        Ok(self.get_addr(addr)?.read_i32::<LittleEndian>()?)
    }

    /// Loads an `i32` from the given virtual address.
    pub fn put_int(&mut self, val: i32, addr: i16) -> Result<(), GlobalsError> {
        self.get_addr_mut(addr)?.write_i32::<LittleEndian>(val)?;
        Ok(())
    }

    /// Loads an `f32` from the given virtual address.
    pub fn get_float(&self, addr: i16) -> Result<f32, GlobalsError> {
        self.type_check(addr as usize, Type::QFloat)?;
        Ok(self.get_addr(addr)?.read_f32::<LittleEndian>()?)
    }

    /// Stores an `f32` at the given virtual address.
    pub fn put_float(&mut self, val: f32, addr: i16) -> Result<(), GlobalsError> {
        self.type_check(addr as usize, Type::QFloat)?;
        self.get_addr_mut(addr)?.write_f32::<LittleEndian>(val)?;
        Ok(())
    }

    /// Loads an `[f32; 3]` from the given virtual address.
    pub fn get_vector(&self, addr: i16) -> Result<[f32; 3], GlobalsError> {
        self.type_check(addr as usize, Type::QVector)?;

        let mut v = [0.0; 3];

        for i in 0..3 {
            v[i] = self.get_float(addr + i as i16)?;
        }

        Ok(v)
    }

    /// Stores an `[f32; 3]` at the given virtual address.
    pub fn put_vector(&mut self, val: [f32; 3], addr: i16) -> Result<(), GlobalsError> {
        self.type_check(addr as usize, Type::QVector)?;

        for i in 0..3 {
            self.put_float(val[i], addr + i as i16)?;
        }

        Ok(())
    }

    /// Loads a `StringId` from the given virtual address.
    pub fn get_string_id(&self, addr: i16) -> Result<StringId, GlobalsError> {
        self.type_check(addr as usize, Type::QString)?;

        Ok(StringId(
            self.get_addr(addr)?.read_i32::<LittleEndian>()? as usize,
        ))
    }

    /// Stores a `StringId` at the given virtual address.
    pub fn put_string_id(&mut self, val: StringId, addr: i16) -> Result<(), GlobalsError> {
        self.type_check(addr as usize, Type::QString)?;

        self.get_addr_mut(addr)?.write_i32::<LittleEndian>(
            val.try_into().unwrap(),
        )?;
        Ok(())
    }

    /// Loads an `EntityId` from the given virtual address.
    pub fn get_entity_id(&self, addr: i16) -> Result<EntityId, GlobalsError> {
        self.type_check(addr as usize, Type::QEntity)?;

        match self.get_addr(addr)?.read_i32::<LittleEndian>()? {
            e if e < 0 => Err(GlobalsError::with_msg(
                format!("Negative entity ID ({})", e),
            )),
            e => Ok(EntityId(e as usize)),
        }
    }

    /// Stores an `EntityId` at the given virtual address.
    pub fn put_entity_id(&mut self, val: EntityId, addr: i16) -> Result<(), GlobalsError> {
        self.type_check(addr as usize, Type::QEntity)?;

        self.get_addr_mut(addr)?.write_i32::<LittleEndian>(
            val.0 as i32,
        )?;
        Ok(())
    }

    /// Loads a `FieldAddr` from the given virtual address.
    pub fn get_field_addr(&self, addr: i16) -> Result<FieldAddr, GlobalsError> {
        self.type_check(addr as usize, Type::QField)?;

        match self.get_addr(addr)?.read_i32::<LittleEndian>()? {
            f if f < 0 => Err(GlobalsError::with_msg(
                format!("Negative entity ID ({})", f),
            )),
            f => Ok(FieldAddr(f as usize)),
        }
    }

    /// Stores a `FieldAddr` at the given virtual address.
    pub fn put_field_addr(&mut self, val: FieldAddr, addr: i16) -> Result<(), GlobalsError> {
        self.type_check(addr as usize, Type::QField)?;
        self.get_addr_mut(addr)?.write_i32::<LittleEndian>(
            val.0 as i32,
        )?;
        Ok(())
    }

    /// Loads a `FunctionId` from the given virtual address.
    pub fn get_function_id(&self, addr: i16) -> Result<FunctionId, GlobalsError> {
        self.type_check(addr as usize, Type::QFunction)?;
        Ok(FunctionId(
            self.get_addr(addr)?.read_i32::<LittleEndian>()? as usize,
        ))
    }

    /// Stores a `FunctionId` at the given virtual address.
    pub fn put_function_id(&mut self, val: FunctionId, addr: i16) -> Result<(), GlobalsError> {
        self.type_check(addr as usize, Type::QFunction)?;
        self.get_addr_mut(addr)?.write_i32::<LittleEndian>(
            val.try_into().unwrap(),
        )?;
        Ok(())
    }

    // TODO: typecheck these with QPointer?

    pub fn get_entity_field(&self, addr: i16) -> Result<i32, GlobalsError> {
        Ok(self.get_addr(addr)?.read_i32::<LittleEndian>()?)
    }

    pub fn put_entity_field(&mut self, val: i32, addr: i16) -> Result<(), GlobalsError> {
        self.get_addr_mut(addr)?.write_i32::<LittleEndian>(val)?;
        Ok(())
    }

    /// Copies the data at `src_addr` to `dst_addr` without type checking.
    pub fn untyped_copy(&mut self, src_addr: i16, dst_addr: i16) -> Result<(), GlobalsError> {
        let src = self.get_addr(src_addr)?.to_owned();
        let dst = self.get_addr_mut(dst_addr)?;

        for i in 0..4 {
            dst[i] = src[i]
        }

        Ok(())
    }

    /// Calculate `v_forward`, `v_right` and `v_up` from `angles`.
    ///
    /// This requires some careful coordinate system transformations. Angle vectors are stored
    /// as `[pitch, yaw, roll]` -- that is, rotations about the lateral (right), vertical (up), and
    /// longitudinal (forward) axes respectively. However, Quake's coordinate system maps `x` to the
    /// longitudinal (forward) axis, `y` to the *negative* lateral (leftward) axis, and `z` to the
    /// vertical (up) axis. As a result, the rotation matrix has to be calculated from `[roll,
    /// -pitch, yaw]` instead.
    pub fn make_vectors(&mut self) -> Result<(), GlobalsError> {
        let angles = self.get_vector(GLOBAL_ADDR_ARG_0 as i16)?;

        let rotation_matrix = make_vectors(angles);

        self.put_vector(
            rotation_matrix.x.into(),
            GlobalAddrVector::VForward as i16,
        )?;
        self.put_vector(
            rotation_matrix.y.into(),
            GlobalAddrVector::VRight as i16,
        )?;
        self.put_vector(
            rotation_matrix.z.into(),
            GlobalAddrVector::VUp as i16,
        )?;

        Ok(())
    }

    /// Calculate the magnitude of a vector.
    ///
    /// Loads the vector from `GLOBAL_ADDR_ARG_0` and stores its magnitude at
    /// `GLOBAL_ADDR_RETURN`.
    pub fn v_len(&mut self) -> Result<(), GlobalsError> {
        let v = Vector3::from(self.get_vector(GLOBAL_ADDR_ARG_0 as i16)?);
        self.put_float(v.magnitude(), GLOBAL_ADDR_RETURN as i16)?;
        Ok(())
    }

    /// Calculate a yaw angle from a direction vector.
    ///
    /// Loads the direction vector from `GLOBAL_ADDR_ARG_0` and stores the yaw value at
    /// `GLOBAL_ADDR_RETURN`.
    pub fn vec_to_yaw(&mut self) -> Result<(), GlobalsError> {
        let v = self.get_vector(GLOBAL_ADDR_ARG_0 as i16)?;

        let mut yaw;
        if v[0] == 0.0 || v[1] == 0.0 {
            yaw = 0.0;
        } else {
            yaw = v[1].atan2(v[0]).to_degrees();
            if yaw < 0.0 {
                yaw += 360.0;
            }
        }

        self.put_float(yaw, GLOBAL_ADDR_RETURN as i16)?;
        Ok(())
    }

    /// Round a float to the nearest integer.
    ///
    /// Loads the float from `GLOBAL_ADDR_ARG_0` and stores the rounded value at
    /// `GLOBAL_ADDR_RETURN`.
    pub fn r_int(&mut self) -> Result<(), GlobalsError> {
        let f = self.get_float(GLOBAL_ADDR_ARG_0 as i16)?;
        self.put_float(f.round(), GLOBAL_ADDR_RETURN as i16)?;
        Ok(())
    }

    /// Round a float to the nearest integer less than or equal to it.
    ///
    /// Loads the float from `GLOBAL_ADDR_ARG_0` and stores the rounded value at
    /// `GLOBAL_ADDR_RETURN`.
    pub fn floor(&mut self) -> Result<(), GlobalsError> {
        let f = self.get_float(GLOBAL_ADDR_ARG_0 as i16)?;
        self.put_float(f.floor(), GLOBAL_ADDR_RETURN as i16)?;
        Ok(())
    }

    /// Round a float to the nearest integer greater than or equal to it.
    ///
    /// Loads the float from `GLOBAL_ADDR_ARG_0` and stores the rounded value at
    /// `GLOBAL_ADDR_RETURN`.
    pub fn ceil(&mut self) -> Result<(), GlobalsError> {
        let f = self.get_float(GLOBAL_ADDR_ARG_0 as i16)?;
        self.put_float(f.ceil(), GLOBAL_ADDR_RETURN as i16)?;
        Ok(())
    }

    /// Calculate the absolute value of a float.
    ///
    /// Loads the float from `GLOBAL_ADDR_ARG_0` and stores its absolute value at
    /// `GLOBAL_ADDR_RETURN`.
    pub fn f_abs(&mut self) -> Result<(), GlobalsError> {
        let f = self.get_float(GLOBAL_ADDR_ARG_0 as i16)?;
        self.put_float(f.abs(), GLOBAL_ADDR_RETURN as i16)?;
        Ok(())
    }
}

pub fn make_vectors(angles: [f32; 3]) -> Matrix3<f32> {
    let pitch = Deg(-angles[0]);
    let yaw = Deg(angles[1]);
    let roll = Deg(angles[2]);

    Matrix3::from(Euler::new(roll, pitch, yaw))
}

#[cfg(test)]
mod test {
    use super::*;

    use cgmath::SquareMatrix;

    #[test]
    fn test_make_vectors_no_rotation() {
        let angles_zero = [0.0; 3];
        let result = make_vectors(angles_zero);
        assert_eq!(Matrix3::identity(), result);
    }

    #[test]
    fn test_make_vectors_pitch() {
        let pitch_90 = [90.0, 0.0, 0.0];
        let result = make_vectors(pitch_90);
        assert_eq!(Matrix3::from_angle_y(Deg(-90.0)), result);
    }

    #[test]
    fn test_make_vectors_yaw() {
        let yaw_90 = [0.0, 90.0, 0.0];
        let result = make_vectors(yaw_90);
        assert_eq!(Matrix3::from_angle_z(Deg(90.0)), result);
    }

    #[test]
    fn test_make_vectors_roll() {
        let roll_90 = [0.0, 0.0, 90.0];
        let result = make_vectors(roll_90);
        assert_eq!(Matrix3::from_angle_x(Deg(90.0)), result);
    }
}
