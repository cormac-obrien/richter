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

use std::cell::RefCell;
use std::convert::TryInto;
use std::rc::Rc;

use engine;
use progs::EntityId;
use progs::FieldAddr;
use progs::FunctionId;
use progs::GlobalDef;
use progs::ProgsError;
use progs::StringId;
use progs::StringTable;
use progs::Type;

use byteorder::LittleEndian;
use byteorder::ReadBytesExt;
use byteorder::WriteBytesExt;
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

#[derive(FromPrimitive)]
pub enum GlobalFloatAddress {
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
pub enum GlobalVectorAddress {
    VForward = 59,
    VUp = 62,
    VRight = 65,
    TraceEndPos = 71,
    TracePlaneNormal = 74,
}

#[derive(FromPrimitive)]
pub enum GlobalStringAddress {
    MapName = 34,
}

#[derive(FromPrimitive)]
pub enum GlobalEntityAddress {
    Self_ = 28,
    Other = 29,
    World = 30,
    TraceEntity = 78,
    MsgEntity = 81,
}

#[derive(FromPrimitive)]
pub enum GlobalFieldAddress {}

#[derive(FromPrimitive)]
pub enum GlobalFunctionAddress {
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
    pub string_table: Rc<StringTable>,
    pub defs: Box<[GlobalDef]>,
    pub addrs: Box<[[u8; 4]]>,
}

impl Globals {
    fn type_check(&self, addr: usize, type_: Type) -> Result<(), ProgsError> {
        match self.defs.iter().find(|def| def.offset as usize == addr) {
            Some(d) => {
                if type_ == d.type_ {
                    return Ok(());
                } else if type_ == Type::QFloat && d.type_ == Type::QVector {
                    return Ok(());
                } else if type_ == Type::QVector && d.type_ == Type::QFloat {
                    return Ok(());
                } else {
                    return Err(ProgsError::with_msg("type check failed"));
                }
            }
            None => return Ok(()),
        }
    }

    pub fn type_at_addr(&self, addr: usize) -> Result<Option<Type>, ProgsError> {
        if addr > self.addrs.len() {
            return Err(ProgsError::with_msg(
                format!("address is out of range ({})", addr),
            ));
        }

        Ok(match self.defs.iter().find(
            |def| def.offset as usize == addr,
        ) {
            Some(d) => Some(d.type_),
            None => None,
        })
    }

    pub fn get_addr(&self, addr: i16) -> Result<&[u8], ProgsError> {
        if addr < 0 {
            return Err(ProgsError::with_msg("get_addr: negative address"));
        }

        let addr = addr as usize;

        if addr > self.addrs.len() {
            return Err(ProgsError::with_msg(
                format!("address out of range ({})", addr),
            ));
        }

        Ok(&self.addrs[addr])
    }

    pub fn get_addr_mut(&mut self, addr: i16) -> Result<&mut [u8], ProgsError> {
        if addr < 0 {
            return Err(ProgsError::with_msg("get_addr: negative address"));
        }

        let addr = addr as usize;

        if addr > self.addrs.len() {
            return Err(ProgsError::with_msg(
                format!("address out of range ({})", addr),
            ));
        }

        Ok(&mut self.addrs[addr])
    }

    pub fn get_bytes(&self, addr: i16) -> Result<[u8; 4], ProgsError> {
        if addr < 0 {
            return Err(ProgsError::with_msg("get_bytes: negative address"));
        }

        let addr = addr as usize;

        if addr > self.addrs.len() {
            return Err(ProgsError::with_msg(
                format!("address out of range ({})", addr),
            ));
        }

        Ok(self.addrs[addr])
    }

    pub fn put_bytes(&mut self, val: [u8; 4], addr: i16) -> Result<(), ProgsError> {
        if addr < 0 {
            return Err(ProgsError::with_msg("put_bytes: negative address"));
        }

        let addr = addr as usize;

        if addr > self.addrs.len() {
            return Err(ProgsError::with_msg(
                format!("address out of range ({})", addr),
            ));
        }

        self.addrs[addr] = val;
        Ok(())
    }

    pub fn get_int(&self, addr: i16) -> Result<i32, ProgsError> {
        Ok(self.get_addr(addr)?.read_i32::<LittleEndian>()?)
    }

    pub fn put_int(&mut self, val: i32, addr: i16) -> Result<(), ProgsError> {
        self.get_addr_mut(addr)?.write_i32::<LittleEndian>(val)?;
        Ok(())
    }

    /// Attempts to retrieve an `f32` from the given virtual address.
    pub fn get_float(&self, addr: i16) -> Result<f32, ProgsError> {
        self.type_check(addr as usize, Type::QFloat)?;
        Ok(self.get_addr(addr)?.read_f32::<LittleEndian>()?)
    }

    /// Attempts to store an `f32` at the given virtual address.
    pub fn put_float(&mut self, val: f32, addr: i16) -> Result<(), ProgsError> {
        self.type_check(addr as usize, Type::QFloat)?;
        self.get_addr_mut(addr)?.write_f32::<LittleEndian>(val)?;
        Ok(())
    }

    /// Attempts to load an `[f32; 3]` from the given address.
    pub fn get_vector(&self, addr: i16) -> Result<[f32; 3], ProgsError> {
        self.type_check(addr as usize, Type::QVector)?;

        let mut v = [0.0; 3];

        for i in 0..3 {
            v[i] = self.get_float(addr + i as i16)?;
        }

        Ok(v)
    }

    /// Attempts to store an `[f32; 3]` at the given address.
    pub fn put_vector(&mut self, val: [f32; 3], addr: i16) -> Result<(), ProgsError> {
        self.type_check(addr as usize, Type::QVector)?;

        for i in 0..3 {
            self.put_float(val[i], addr + i as i16)?;
        }

        Ok(())
    }

    pub fn get_string_id(&self, addr: i16) -> Result<StringId, ProgsError> {
        Ok(StringId(
            self.get_addr(addr)?.read_i32::<LittleEndian>()? as usize,
        ))
    }

    pub fn put_string_id(&mut self, val: StringId, addr: i16) -> Result<(), ProgsError> {
        self.get_addr_mut(addr)?.write_i32::<LittleEndian>(
            val.try_into()?,
        )?;
        Ok(())
    }

    pub fn get_entity_id(&self, addr: i16) -> Result<EntityId, ProgsError> {
        Ok(EntityId(self.get_addr(addr)?.read_i32::<LittleEndian>()?))
    }

    pub fn put_entity_id(&mut self, val: EntityId, addr: i16) -> Result<(), ProgsError> {
        self.get_addr_mut(addr)?.write_i32::<LittleEndian>(val.0)?;
        Ok(())
    }

    pub fn get_field_addr(&self, addr: i16) -> Result<FieldAddr, ProgsError> {
        Ok(FieldAddr(self.get_addr(addr)?.read_i32::<LittleEndian>()?))
    }

    pub fn put_field_addr(&mut self, val: FieldAddr, addr: i16) -> Result<(), ProgsError> {
        self.get_addr_mut(addr)?.write_i32::<LittleEndian>(val.0)?;
        Ok(())
    }

    pub fn get_function_id(&self, addr: i16) -> Result<FunctionId, ProgsError> {
        Ok(FunctionId(
            self.get_addr(addr)?.read_i32::<LittleEndian>()? as usize,
        ))
    }


    pub fn put_function_id(&mut self, val: FunctionId, addr: i16) -> Result<(), ProgsError> {
        self.get_addr_mut(addr)?.write_i32::<LittleEndian>(
            val.try_into()?,
        )?;
        Ok(())
    }


    pub fn get_entity_field(&self, addr: i16) -> Result<i32, ProgsError> {
        Ok(self.get_addr(addr)?.read_i32::<LittleEndian>()?)
    }

    pub fn put_entity_field(&mut self, val: i32, addr: i16) -> Result<(), ProgsError> {
        self.get_addr_mut(addr)?.write_i32::<LittleEndian>(val)?;
        Ok(())
    }

    pub fn untyped_copy(&mut self, src_addr: i16, dst_addr: i16) -> Result<(), ProgsError> {
        let src = self.get_addr(src_addr)?.to_owned();
        let dst = self.get_addr_mut(dst_addr)?;

        for i in 0..4 {
            dst[i] = src[i]
        }

        Ok(())
    }
}
