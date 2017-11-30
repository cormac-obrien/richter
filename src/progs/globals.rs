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
    pub statics: GlobalsStatic,
    pub dynamics: Vec<[u8; 4]>,
}

impl Globals {
    /// Attempts to retrieve an `f32` from the given virtual address.
    pub fn get_float(&self, addr: i16) -> Result<f32, ProgsError> {
        if addr < 0 {
            return Err(ProgsError::with_msg("get_f: negative address"));
        }

        let addr = addr as usize;

        if addr < GLOBAL_STATIC_START {
            self.get_float_reserved(addr)
        } else if addr < GLOBAL_DYNAMIC_START {
            self.get_float_static(addr)
        } else {
            self.get_dynamic_float(addr)
        }
    }

    fn get_float_reserved(&self, addr: usize) -> Result<f32, ProgsError> {
        Ok(self.statics.reserved[addr]
            .as_ref()
            .read_f32::<LittleEndian>()?)
    }

    fn get_float_static(&self, addr: usize) -> Result<f32, ProgsError> {
        let f_addr = match GlobalFloatAddress::from_usize(addr) {
            Some(f) => f,
            None => {
                return Err(ProgsError::with_msg(
                    format!("get_float_static: invalid address ({})", addr),
                ))
            }
        };

        Ok(match f_addr {
            GlobalFloatAddress::Time => engine::duration_to_f32(self.statics.time),
            GlobalFloatAddress::FrameTime => engine::duration_to_f32(self.statics.frame_time),
            GlobalFloatAddress::ForceRetouch => self.statics.force_retouch,
            GlobalFloatAddress::Deathmatch => self.statics.deathmatch,
            GlobalFloatAddress::Coop => self.statics.coop,
            GlobalFloatAddress::TeamPlay => self.statics.team_play,
            GlobalFloatAddress::ServerFlags => self.statics.server_flags,
            GlobalFloatAddress::TotalSecrets => self.statics.total_secrets,
            GlobalFloatAddress::TotalMonsters => self.statics.total_monsters,
            GlobalFloatAddress::FoundSecrets => self.statics.found_secrets,
            GlobalFloatAddress::KilledMonsters => self.statics.killed_monsters,
            GlobalFloatAddress::Arg0 => self.statics.args[0],
            GlobalFloatAddress::Arg1 => self.statics.args[1],
            GlobalFloatAddress::Arg2 => self.statics.args[2],
            GlobalFloatAddress::Arg3 => self.statics.args[3],
            GlobalFloatAddress::Arg4 => self.statics.args[4],
            GlobalFloatAddress::Arg5 => self.statics.args[5],
            GlobalFloatAddress::Arg6 => self.statics.args[6],
            GlobalFloatAddress::Arg7 => self.statics.args[7],
            GlobalFloatAddress::Arg8 => self.statics.args[8],
            GlobalFloatAddress::Arg9 => self.statics.args[9],
            GlobalFloatAddress::Arg10 => self.statics.args[10],
            GlobalFloatAddress::Arg11 => self.statics.args[11],
            GlobalFloatAddress::Arg12 => self.statics.args[12],
            GlobalFloatAddress::Arg13 => self.statics.args[13],
            GlobalFloatAddress::Arg14 => self.statics.args[14],
            GlobalFloatAddress::Arg15 => self.statics.args[15],
            GlobalFloatAddress::VForwardX => self.statics.v_forward[0],
            GlobalFloatAddress::VForwardY => self.statics.v_forward[1],
            GlobalFloatAddress::VForwardZ => self.statics.v_forward[2],
            GlobalFloatAddress::VUpX => self.statics.v_up[0],
            GlobalFloatAddress::VUpY => self.statics.v_up[1],
            GlobalFloatAddress::VUpZ => self.statics.v_up[2],
            GlobalFloatAddress::VRightX => self.statics.v_right[0],
            GlobalFloatAddress::VRightY => self.statics.v_right[1],
            GlobalFloatAddress::VRightZ => self.statics.v_right[2],
            GlobalFloatAddress::TraceAllSolid => self.statics.trace_all_solid,
            GlobalFloatAddress::TraceStartSolid => self.statics.trace_start_solid,
            GlobalFloatAddress::TraceFraction => self.statics.trace_fraction,
            GlobalFloatAddress::TraceEndPosX => self.statics.trace_end_pos[0],
            GlobalFloatAddress::TraceEndPosY => self.statics.trace_end_pos[1],
            GlobalFloatAddress::TraceEndPosZ => self.statics.trace_end_pos[2],
            GlobalFloatAddress::TracePlaneNormalX => self.statics.trace_plane_normal[0],
            GlobalFloatAddress::TracePlaneNormalY => self.statics.trace_plane_normal[1],
            GlobalFloatAddress::TracePlaneNormalZ => self.statics.trace_plane_normal[2],
            GlobalFloatAddress::TracePlaneDist => self.statics.trace_plane_dist,
            GlobalFloatAddress::TraceInOpen => self.statics.trace_in_open,
            GlobalFloatAddress::TraceInWater => self.statics.trace_in_water,
        })
    }

    fn get_dynamic_float(&self, addr: usize) -> Result<f32, ProgsError> {
        if addr > GLOBAL_RESERVED_COUNT + GLOBAL_STATIC_COUNT + self.dynamics.len() {
            return Err(ProgsError::with_msg(format!(
                "get_float_dynamic: address out of range ({})",
                addr
            )));
        }

        Ok(self.dynamics[addr - GLOBAL_DYNAMIC_START]
            .as_ref()
            .read_f32::<LittleEndian>()?)
    }

    /// Attempts to store an `f32` at the given virtual address.
    pub fn put_float(&mut self, val: f32, addr: i16) -> Result<(), ProgsError> {
        if addr < 0 {
            return Err(ProgsError::with_msg("put_float: negative address"));
        }

        let addr = addr as usize;

        if addr < GLOBAL_STATIC_START {
            self.put_float_reserved(val, addr)
        } else if addr < GLOBAL_DYNAMIC_START {
            self.put_float_static(val, addr)
        } else {
            self.put_float_dynamic(val, addr)
        }
    }

    fn put_float_reserved(&mut self, val: f32, addr: usize) -> Result<(), ProgsError> {
        Ok(self.statics.reserved[addr]
            .as_mut()
            .write_f32::<LittleEndian>(val)?)
    }

    fn put_float_static(&mut self, val: f32, addr: usize) -> Result<(), ProgsError> {
        let f_addr = match GlobalFloatAddress::from_usize(addr) {
            Some(f) => f,
            None => {
                return Err(ProgsError::with_msg(format!(
                    "put_float_static: invalid static global float address ({})",
                    addr
                )))
            }
        };

        Ok(match f_addr {
            GlobalFloatAddress::Time => self.statics.time = engine::duration_from_f32(val),
            GlobalFloatAddress::FrameTime => {
                self.statics.frame_time = engine::duration_from_f32(val)
            }
            GlobalFloatAddress::ForceRetouch => self.statics.force_retouch = val,
            GlobalFloatAddress::Deathmatch => self.statics.deathmatch = val,
            GlobalFloatAddress::Coop => self.statics.coop = val,
            GlobalFloatAddress::TeamPlay => self.statics.team_play = val,
            GlobalFloatAddress::ServerFlags => self.statics.server_flags = val,
            GlobalFloatAddress::TotalSecrets => self.statics.total_secrets = val,
            GlobalFloatAddress::TotalMonsters => self.statics.total_monsters = val,
            GlobalFloatAddress::FoundSecrets => self.statics.found_secrets = val,
            GlobalFloatAddress::KilledMonsters => self.statics.killed_monsters = val,
            GlobalFloatAddress::Arg0 => self.statics.args[0] = val,
            GlobalFloatAddress::Arg1 => self.statics.args[1] = val,
            GlobalFloatAddress::Arg2 => self.statics.args[2] = val,
            GlobalFloatAddress::Arg3 => self.statics.args[3] = val,
            GlobalFloatAddress::Arg4 => self.statics.args[4] = val,
            GlobalFloatAddress::Arg5 => self.statics.args[5] = val,
            GlobalFloatAddress::Arg6 => self.statics.args[6] = val,
            GlobalFloatAddress::Arg7 => self.statics.args[7] = val,
            GlobalFloatAddress::Arg8 => self.statics.args[8] = val,
            GlobalFloatAddress::Arg9 => self.statics.args[9] = val,
            GlobalFloatAddress::Arg10 => self.statics.args[10] = val,
            GlobalFloatAddress::Arg11 => self.statics.args[11] = val,
            GlobalFloatAddress::Arg12 => self.statics.args[12] = val,
            GlobalFloatAddress::Arg13 => self.statics.args[13] = val,
            GlobalFloatAddress::Arg14 => self.statics.args[14] = val,
            GlobalFloatAddress::Arg15 => self.statics.args[15] = val,
            GlobalFloatAddress::VForwardX => self.statics.v_forward[0] = val,
            GlobalFloatAddress::VForwardY => self.statics.v_forward[1] = val,
            GlobalFloatAddress::VForwardZ => self.statics.v_forward[2] = val,
            GlobalFloatAddress::VUpX => self.statics.v_up[0] = val,
            GlobalFloatAddress::VUpY => self.statics.v_up[1] = val,
            GlobalFloatAddress::VUpZ => self.statics.v_up[2] = val,
            GlobalFloatAddress::VRightX => self.statics.v_right[0] = val,
            GlobalFloatAddress::VRightY => self.statics.v_right[1] = val,
            GlobalFloatAddress::VRightZ => self.statics.v_right[2] = val,
            GlobalFloatAddress::TraceAllSolid => self.statics.trace_all_solid = val,
            GlobalFloatAddress::TraceStartSolid => self.statics.trace_start_solid = val,
            GlobalFloatAddress::TraceFraction => self.statics.trace_fraction = val,
            GlobalFloatAddress::TraceEndPosX => self.statics.trace_end_pos[0] = val,
            GlobalFloatAddress::TraceEndPosY => self.statics.trace_end_pos[1] = val,
            GlobalFloatAddress::TraceEndPosZ => self.statics.trace_end_pos[2] = val,
            GlobalFloatAddress::TracePlaneNormalX => self.statics.trace_plane_normal[0] = val,
            GlobalFloatAddress::TracePlaneNormalY => self.statics.trace_plane_normal[1] = val,
            GlobalFloatAddress::TracePlaneNormalZ => self.statics.trace_plane_normal[2] = val,
            GlobalFloatAddress::TracePlaneDist => self.statics.trace_plane_dist = val,
            GlobalFloatAddress::TraceInOpen => self.statics.trace_in_open = val,
            GlobalFloatAddress::TraceInWater => self.statics.trace_in_water = val,
        })
    }

    fn put_float_dynamic(&mut self, val: f32, addr: usize) -> Result<(), ProgsError> {
        if addr > GLOBAL_RESERVED_COUNT + GLOBAL_STATIC_COUNT + self.dynamics.len() {
            return Err(ProgsError::with_msg(format!(
                "put_float_dynamic: address out of range ({})",
                addr
            )));
        }

        Ok(self.dynamics[addr - GLOBAL_DYNAMIC_START]
            .as_mut()
            .write_f32::<LittleEndian>(val)?)
    }

    /// Attempts to load an `[f32; 3]` from the given address.
    pub fn get_vector(&self, addr: i16) -> Result<[f32; 3], ProgsError> {
        if addr < 0 {
            return Err(ProgsError::with_msg("get_vector: negative address"));
        }

        let addr = addr as usize;

        if addr < GLOBAL_STATIC_START {
            self.get_vector_reserved(addr)
        } else if addr < GLOBAL_DYNAMIC_START {
            self.get_vector_static(addr)
        } else {
            self.get_vector_dynamic(addr)
        }
    }

    fn get_vector_reserved(&self, addr: usize) -> Result<[f32; 3], ProgsError> {
        let mut v = [0.0; 3];
        for c in 0..v.len() {
            v[c] = self.get_float_reserved(addr + c)?;
        }
        Ok(v)
    }

    fn get_vector_static(&self, addr: usize) -> Result<[f32; 3], ProgsError> {
        let v_addr = match GlobalVectorAddress::from_usize(addr) {
            Some(v) => v,
            None => {
                return Err(ProgsError::with_msg(
                    format!("get_vector_static: invalid address ({})", addr),
                ))
            }
        };

        Ok(match v_addr {
            GlobalVectorAddress::VForward => self.statics.v_forward.into(),
            GlobalVectorAddress::VUp => self.statics.v_up.into(),
            GlobalVectorAddress::VRight => self.statics.v_right.into(),
            GlobalVectorAddress::TraceEndPos => self.statics.trace_end_pos.into(),
            GlobalVectorAddress::TracePlaneNormal => self.statics.trace_plane_normal.into(),
        })
    }

    fn get_vector_dynamic(&self, addr: usize) -> Result<[f32; 3], ProgsError> {
        // subtract 2 from range to account for size of vector
        if addr > GLOBAL_RESERVED_COUNT + GLOBAL_STATIC_COUNT + self.dynamics.len() - 2 {
            return Err(ProgsError::with_msg(format!(
                "get_vector_dynamic: address out of range ({})",
                addr
            )));
        }

        let mut v = [0.0; 3];
        for c in 0..v.len() {
            v[c] = self.dynamics[addr - GLOBAL_DYNAMIC_START + c]
                .as_ref()
                .read_f32::<LittleEndian>()?;
        }

        Ok(v)
    }

    // TODO: need get_vector_unchecked for copying function arguments

    /// Attempts to store an `[f32; 3]` at the given address.
    pub fn put_vector(&mut self, val: [f32; 3], addr: i16) -> Result<(), ProgsError> {
        if addr < 0 {
            return Err(ProgsError::with_msg("put_vector: negative address"));
        }

        let addr = addr as usize;

        if addr < GLOBAL_STATIC_START {
            self.put_vector_reserved(val, addr)
        } else if addr < GLOBAL_DYNAMIC_START {
            self.put_vector_static(val, addr)
        } else {
            self.put_vector_dynamic(val, addr)
        }
    }

    fn put_vector_reserved(&mut self, val: [f32; 3], addr: usize) -> Result<(), ProgsError> {
        for c in 0..val.len() {
            self.put_float_reserved(val[c], addr + c)?;
        }

        Ok(())
    }

    fn put_vector_static(&mut self, val: [f32; 3], addr: usize) -> Result<(), ProgsError> {
        let v_addr = match GlobalVectorAddress::from_usize(addr) {
            Some(v) => v,
            None => {
                return Err(ProgsError::with_msg(format!(
                    "put_vector_static: invalid static global vector address ({})",
                    addr
                )))
            }
        };

        match v_addr {
            GlobalVectorAddress::VForward => self.statics.v_forward = Vector3::from(val),
            GlobalVectorAddress::VUp => self.statics.v_up = Vector3::from(val),
            GlobalVectorAddress::VRight => self.statics.v_right = Vector3::from(val),
            GlobalVectorAddress::TraceEndPos => self.statics.trace_end_pos = Vector3::from(val),
            GlobalVectorAddress::TracePlaneNormal => {
                self.statics.trace_plane_normal = Vector3::from(val)
            }
        }

        Ok(())
    }

    fn put_vector_dynamic(&mut self, val: [f32; 3], addr: usize) -> Result<(), ProgsError> {
        // subtract 2 from range to account for size of vector
        if addr > GLOBAL_RESERVED_COUNT + GLOBAL_STATIC_COUNT + self.dynamics.len() - 2 {
            return Err(ProgsError::with_msg(format!(
                "put_vector_dynamic: dynamic global vector address out of range ({})",
                addr
            )));
        }

        for c in 0..val.len() {
            self.dynamics[addr - GLOBAL_DYNAMIC_START + c]
                .as_mut()
                .write_f32::<LittleEndian>(val[c])?;
        }

        Ok(())
    }

    pub fn get_string_id(&self, addr: i16) -> Result<StringId, ProgsError> {
        if addr < 0 {
            return Err(ProgsError::with_msg("get_string_id: negative address"));
        }

        let addr = addr as usize;

        if addr < GLOBAL_STATIC_START {
            self.get_string_id_reserved(addr)
        } else if addr < GLOBAL_DYNAMIC_START {
            self.get_string_id_static(addr)
        } else {
            self.get_string_id_dynamic(addr)
        }
    }

    fn get_string_id_reserved(&self, addr: usize) -> Result<StringId, ProgsError> {
        Ok(self.string_table.id_from_i32(self.statics.reserved[addr]
            .as_ref()
            .read_i32::<LittleEndian>()?)?)
    }

    fn get_string_id_static(&self, addr: usize) -> Result<StringId, ProgsError> {
        let s_addr = match GlobalStringAddress::from_usize(addr) {
            Some(s) => s,
            None => {
                return Err(ProgsError::with_msg(
                    format!("get_string_id_static: invalid address ({})", addr),
                ))
            }
        };

        Ok(match s_addr {
            GlobalStringAddress::MapName => self.statics.map_name,
        })
    }

    fn get_string_id_dynamic(&self, addr: usize) -> Result<StringId, ProgsError> {
        if addr > GLOBAL_RESERVED_COUNT + GLOBAL_STATIC_COUNT + self.dynamics.len() {
            return Err(ProgsError::with_msg(format!(
                "get_string_id_dynamic: address out of range ({})",
                addr
            )));
        }

        Ok(self.string_table.id_from_i32(self.dynamics[addr]
            .as_ref()
            .read_i32::<LittleEndian>()?)?)
    }

    pub fn put_string_id(&mut self, val: StringId, addr: i16) -> Result<(), ProgsError> {
        if addr < 0 {
            return Err(ProgsError::with_msg("put_string_id: negative address"));
        }

        let addr = addr as usize;

        if addr < GLOBAL_STATIC_START {
            self.put_string_id_reserved(val, addr)
        } else if addr < GLOBAL_DYNAMIC_START {
            self.put_string_id_static(val, addr)
        } else {
            self.put_string_id_dynamic(val, addr)
        }
    }

    fn put_string_id_reserved(&mut self, val: StringId, addr: usize) -> Result<(), ProgsError> {
        self.statics.reserved[addr]
            .as_mut()
            .write_i32::<LittleEndian>(val.try_into()?)?;

        Ok(())
    }

    fn put_string_id_static(&mut self, val: StringId, addr: usize) -> Result<(), ProgsError> {
        let s_addr = match GlobalStringAddress::from_usize(addr) {
            Some(s) => s,
            None => {
                return Err(ProgsError::with_msg(
                    format!("put_string_id_static: invalid address ({})", addr),
                ))
            }
        };

        match s_addr {
            GlobalStringAddress::MapName => self.statics.map_name = val,
        }

        Ok(())
    }

    fn put_string_id_dynamic(&mut self, val: StringId, addr: usize) -> Result<(), ProgsError> {
        if addr > GLOBAL_RESERVED_COUNT + GLOBAL_STATIC_COUNT + self.dynamics.len() {
            return Err(ProgsError::with_msg(format!(
                "put_string_id_dynamic: address out of range ({})",
                addr
            )));
        }

        self.dynamics[addr - GLOBAL_DYNAMIC_START]
            .as_mut()
            .write_i32::<LittleEndian>(val.try_into()?)?;

        Ok(())
    }

    pub fn get_entity_id(&self, addr: i16) -> Result<EntityId, ProgsError> {
        if addr < 0 {
            return Err(ProgsError::with_msg("get_entity_id: negative address"));
        }

        let addr = addr as usize;

        if addr < GLOBAL_STATIC_START {
            self.get_entity_id_reserved(addr)
        } else if addr < GLOBAL_DYNAMIC_START {
            self.get_entity_id_static(addr)
        } else {
            self.get_entity_id_dynamic(addr)
        }
    }

    fn get_entity_id_reserved(&self, addr: usize) -> Result<EntityId, ProgsError> {
        Ok(EntityId(self.statics.reserved[addr]
            .as_ref()
            .read_i32::<LittleEndian>()?))
    }

    fn get_entity_id_static(&self, addr: usize) -> Result<EntityId, ProgsError> {
        let s_addr = match GlobalEntityAddress::from_usize(addr) {
            Some(s) => s,
            None => {
                return Err(ProgsError::with_msg(
                    format!("get_entity_id_static: invalid address ({})", addr),
                ))
            }
        };

        Ok(match s_addr {
            GlobalEntityAddress::Self_ => self.statics.self_,
            GlobalEntityAddress::Other => self.statics.other,
            GlobalEntityAddress::World => self.statics.world,
            GlobalEntityAddress::TraceEntity => self.statics.trace_ent,
            GlobalEntityAddress::MsgEntity => self.statics.msg_entity,
        })
    }

    fn get_entity_id_dynamic(&self, addr: usize) -> Result<EntityId, ProgsError> {
        if addr > GLOBAL_RESERVED_COUNT + GLOBAL_STATIC_COUNT + self.dynamics.len() {
            return Err(ProgsError::with_msg(format!(
                "get_entity_id_dynamic: address out of range ({})",
                addr
            )));
        }

        Ok(EntityId(self.dynamics[addr - GLOBAL_DYNAMIC_START]
            .as_ref()
            .read_i32::<LittleEndian>()?))
    }

    pub fn put_entity_id(&mut self, val: EntityId, addr: i16) -> Result<(), ProgsError> {
        if addr < 0 {
            return Err(ProgsError::with_msg("put_entity_id: negative address"));
        }

        let addr = addr as usize;

        if addr < GLOBAL_STATIC_START {
            self.put_entity_id_reserved(val, addr)
        } else if addr < GLOBAL_DYNAMIC_START {
            self.put_entity_id_static(val, addr)
        } else {
            self.put_entity_id_dynamic(val, addr)
        }
    }

    fn put_entity_id_reserved(&mut self, val: EntityId, addr: usize) -> Result<(), ProgsError> {
        self.statics.reserved[addr]
            .as_mut()
            .write_i32::<LittleEndian>(val.0)?;

        Ok(())
    }

    fn put_entity_id_static(&mut self, val: EntityId, addr: usize) -> Result<(), ProgsError> {
        let s_addr = match GlobalEntityAddress::from_usize(addr) {
            Some(s) => s,
            None => {
                return Err(ProgsError::with_msg(
                    format!("put_entity_id_static: invalid address ({})", addr),
                ))
            }
        };

        match s_addr {
            GlobalEntityAddress::Self_ => self.statics.self_ = val,
            GlobalEntityAddress::Other => self.statics.other = val,
            GlobalEntityAddress::World => self.statics.world = val,
            GlobalEntityAddress::TraceEntity => self.statics.trace_ent = val,
            GlobalEntityAddress::MsgEntity => self.statics.msg_entity = val,
        }

        Ok(())
    }

    fn put_entity_id_dynamic(&mut self, val: EntityId, addr: usize) -> Result<(), ProgsError> {
        if addr > GLOBAL_RESERVED_COUNT + GLOBAL_STATIC_COUNT + self.dynamics.len() {
            return Err(ProgsError::with_msg(format!(
                "put_entity_id_dynamic: address out of range ({})",
                addr
            )));
        }

        self.dynamics[addr - GLOBAL_DYNAMIC_START]
            .as_mut()
            .write_i32::<LittleEndian>(val.0)?;

        Ok(())
    }

    pub fn get_field_addr(&self, addr: i16) -> Result<FieldAddr, ProgsError> {
        if addr < 0 {
            return Err(ProgsError::with_msg("get_field_addr: negative address"));
        }

        let addr = addr as usize;

        if addr < GLOBAL_STATIC_COUNT {
            self.get_field_addr_static(addr)
        } else {
            self.get_field_addr_dynamic(addr)
        }
    }

    fn get_field_addr_reserved(&self, addr: usize) -> Result<FieldAddr, ProgsError> {
        Ok(FieldAddr(self.statics.reserved[addr]
            .as_ref()
            .read_i32::<LittleEndian>()?))
    }

    fn get_field_addr_static(&self, addr: usize) -> Result<FieldAddr, ProgsError> {
        let f_addr = match GlobalFieldAddress::from_usize(addr) {
            Some(s) => s,
            None => {
                return Err(ProgsError::with_msg(
                    format!("get_field_addr_static: invalid address ({})", addr),
                ))
            }
        };

        return Err(ProgsError::with_msg(
            format!("get_field_addr_static: invalid address ({})", addr),
        ));
    }

    fn get_field_addr_dynamic(&self, addr: usize) -> Result<FieldAddr, ProgsError> {
        if addr > GLOBAL_RESERVED_COUNT + GLOBAL_STATIC_COUNT + self.dynamics.len() {
            return Err(ProgsError::with_msg(format!(
                "get_field_addr_dynamic: address out of range ({})",
                addr
            )));
        }

        Ok(FieldAddr(self.dynamics[addr - GLOBAL_DYNAMIC_START]
            .as_ref()
            .read_i32::<LittleEndian>()?))
    }

    pub fn put_field_addr(&mut self, val: FieldAddr, addr: i16) -> Result<(), ProgsError> {
        if addr < 0 {
            return Err(ProgsError::with_msg("put_field_addr: negative address"));
        }

        let addr = addr as usize;

        if addr < GLOBAL_STATIC_COUNT {
            self.put_field_addr_static(val, addr)
        } else {
            self.put_field_addr_dynamic(val, addr)
        }
    }

    fn put_field_addr_static(&mut self, val: FieldAddr, addr: usize) -> Result<(), ProgsError> {
        let f_addr = match GlobalFieldAddress::from_usize(addr) {
            Some(s) => s,
            None => {
                return Err(ProgsError::with_msg(
                    format!("put_field_addr_static: invalid address ({})", addr),
                ))
            }
        };

        return Err(ProgsError::with_msg(
            format!("put_field_addr_static: invalid address ({})", addr),
        ));
    }

    fn put_field_addr_dynamic(&mut self, val: FieldAddr, addr: usize) -> Result<(), ProgsError> {
        if addr > GLOBAL_RESERVED_COUNT + GLOBAL_STATIC_COUNT + self.dynamics.len() {
            return Err(ProgsError::with_msg(format!(
                "put_field_addr_dynamic: address out of range ({})",
                addr
            )));
        }

        self.dynamics[addr - GLOBAL_DYNAMIC_START]
            .as_mut()
            .write_i32::<LittleEndian>(val.0)?;

        Ok(())
    }

    pub fn get_function_id(&self, addr: i16) -> Result<FunctionId, ProgsError> {
        if addr < 0 {
            return Err(ProgsError::with_msg("get_function_id: negative address"));
        }

        let addr = addr as usize;

        if addr < GLOBAL_STATIC_COUNT {
            self.get_function_id_static(addr)
        } else {
            self.get_function_id_dynamic(addr)
        }
    }

    fn get_function_id_static(&self, addr: usize) -> Result<FunctionId, ProgsError> {
        let f_addr = match GlobalFunctionAddress::from_usize(addr) {
            Some(s) => s,
            None => {
                return Err(ProgsError::with_msg(format!(
                    "get_function_id_static: invalid address ({})",
                    addr
                )))
            }
        };

        Ok(match f_addr {
            GlobalFunctionAddress::Main => self.statics.main,
            GlobalFunctionAddress::StartFrame => self.statics.start_frame,
            GlobalFunctionAddress::PlayerPreThink => self.statics.player_pre_think,
            GlobalFunctionAddress::PlayerPostThink => self.statics.player_post_think,
            GlobalFunctionAddress::ClientKill => self.statics.client_kill,
            GlobalFunctionAddress::ClientConnect => self.statics.client_connect,
            GlobalFunctionAddress::PutClientInServer => self.statics.put_client_in_server,
            GlobalFunctionAddress::ClientDisconnect => self.statics.client_disconnect,
            GlobalFunctionAddress::SetNewArgs => self.statics.set_new_args,
            GlobalFunctionAddress::SetChangeArgs => self.statics.set_change_args,
        })
    }

    fn get_function_id_dynamic(&self, addr: usize) -> Result<FunctionId, ProgsError> {
        if addr > GLOBAL_RESERVED_COUNT + GLOBAL_STATIC_COUNT + self.dynamics.len() {
            return Err(ProgsError::with_msg(format!(
                "get_function_id_dynamic: address out of range ({})",
                addr
            )));
        }

        Ok(FunctionId(self.dynamics[addr - GLOBAL_DYNAMIC_START]
            .as_ref()
            .read_i32::<LittleEndian>()? as usize))
    }

    pub fn put_function_id(&mut self, val: FunctionId, addr: i16) -> Result<(), ProgsError> {
        if addr < 0 {
            return Err(ProgsError::with_msg("put_function_id: negative address"));
        }

        let addr = addr as usize;

        if addr < GLOBAL_STATIC_COUNT {
            self.put_function_id_static(val, addr)
        } else {
            self.put_function_id_dynamic(val, addr)
        }
    }

    fn put_function_id_static(&mut self, val: FunctionId, addr: usize) -> Result<(), ProgsError> {
        let f_addr = match GlobalFunctionAddress::from_usize(addr) {
            Some(s) => s,
            None => {
                return Err(ProgsError::with_msg(format!(
                    "put_function_id_static: invalid address ({})",
                    addr
                )))
            }
        };

        match f_addr {
            GlobalFunctionAddress::Main => self.statics.main = val,
            GlobalFunctionAddress::StartFrame => self.statics.start_frame = val,
            GlobalFunctionAddress::PlayerPreThink => self.statics.player_pre_think = val,
            GlobalFunctionAddress::PlayerPostThink => self.statics.player_post_think = val,
            GlobalFunctionAddress::ClientKill => self.statics.client_kill = val,
            GlobalFunctionAddress::ClientConnect => self.statics.client_connect = val,
            GlobalFunctionAddress::PutClientInServer => self.statics.put_client_in_server = val,
            GlobalFunctionAddress::ClientDisconnect => self.statics.client_disconnect = val,
            GlobalFunctionAddress::SetNewArgs => self.statics.set_new_args = val,
            GlobalFunctionAddress::SetChangeArgs => self.statics.set_change_args = val,
        }

        Ok(())
    }

    fn put_function_id_dynamic(&mut self, val: FunctionId, addr: usize) -> Result<(), ProgsError> {
        if addr > GLOBAL_RESERVED_COUNT + GLOBAL_STATIC_COUNT + self.dynamics.len() {
            return Err(ProgsError::with_msg(format!(
                "put_function_id_dynamic: address out of range ({})",
                addr
            )));
        }

        self.dynamics[addr - GLOBAL_DYNAMIC_START]
            .as_mut()
            .write_i32::<LittleEndian>(val.try_into()?)?;

        Ok(())
    }

    pub fn get_entity_field(&self, addr: i16) -> Result<i32, ProgsError> {
        if addr < 0 {
            return Err(ProgsError::with_msg(
                format!("get_entity_field: negative address ({})", addr),
            ));
        }

        let addr = addr as usize;

        if addr < GLOBAL_DYNAMIC_START {
            panic!("get_entity_field: address must be dynamic");
        }

        Ok(self.dynamics[addr - GLOBAL_DYNAMIC_START]
            .as_ref()
            .read_i32::<LittleEndian>()?)
    }

    pub fn put_entity_field(&mut self, val: i32, addr: i16) -> Result<(), ProgsError> {
        if addr < 0 {
            return Err(ProgsError::with_msg(
                format!("put_entity_field: negative address ({})", addr),
            ));
        }

        let addr = addr as usize;

        if addr < GLOBAL_DYNAMIC_START {
            panic!("put_entity_field: address must be dynamic");
        }

        self.dynamics[addr - GLOBAL_DYNAMIC_START]
            .as_mut()
            .write_i32::<LittleEndian>(val)?;

        Ok(())
    }

    pub fn reserved_copy(&mut self, src_addr: i16, dst_addr: i16) -> Result<(), ProgsError> {
        if src_addr < 0 {
            return Err(ProgsError::with_msg(format!(
                "reserved_copy: negative source address ({})",
                src_addr
            )));
        }

        // copy byte representations
        let mut src_val = [0; 4];

        let src_addr = src_addr as usize;

        if src_addr < GLOBAL_STATIC_START {
            src_val = self.statics.reserved[src_addr];
        } else if src_addr < GLOBAL_DYNAMIC_START {
            if GlobalFloatAddress::from_usize(src_addr).is_some() {
                let f = self.get_float_static(src_addr)?;
                src_val.as_mut().write_f32::<LittleEndian>(f)?;
            } else if GlobalStringAddress::from_usize(src_addr).is_some() {
                let s = self.get_string_id_static(src_addr)?;
                src_val.as_mut().write_i32::<LittleEndian>(s.try_into()?)?;
            } else if GlobalEntityAddress::from_usize(src_addr).is_some() {
                let e = self.get_entity_id_static(src_addr)?;
                src_val.as_mut().write_i32::<LittleEndian>(e.0)?;
            } else if GlobalFunctionAddress::from_usize(src_addr).is_some() {
                let f = self.get_function_id_static(src_addr)?;
                src_val.as_mut().write_i32::<LittleEndian>(f.try_into()?)?;
            } else {
                return Err(ProgsError::with_msg(format!(
                    "reserved_copy: invalid static source address ({})",
                    src_addr
                )));
            }
        } else if src_addr < GLOBAL_DYNAMIC_START + self.dynamics.len() {
            src_val = self.dynamics[src_addr - GLOBAL_DYNAMIC_START];
        } else {
            return Err(ProgsError::with_msg(format!(
                "reserved_copy: source address out of range ({})",
                src_addr
            )));
        }

        if dst_addr < 0 {
            return Err(ProgsError::with_msg(format!(
                "reserved_copy: negative destination address ({})",
                dst_addr
            )));
        }

        let dst_addr = dst_addr as usize;

        if dst_addr < GLOBAL_STATIC_START {
            self.statics.reserved[dst_addr] = src_val;
        } else if dst_addr < GLOBAL_DYNAMIC_START {
            if GlobalFloatAddress::from_usize(dst_addr).is_some() {
                let f = src_val.as_ref().read_f32::<LittleEndian>()?;
                self.put_float_static(f, dst_addr)?;
            } else if GlobalStringAddress::from_usize(dst_addr).is_some() {
                let s = self.string_table.id_from_i32(
                    src_val.as_ref().read_i32::<LittleEndian>()?,
                )?;
                self.put_string_id_static(s, dst_addr)?;
            } else if GlobalEntityAddress::from_usize(dst_addr).is_some() {
                let e = EntityId(src_val.as_ref().read_i32::<LittleEndian>()?);
                self.put_entity_id_static(e, dst_addr)?;
            } else if GlobalFunctionAddress::from_usize(dst_addr).is_some() {
                let f = FunctionId(src_val.as_ref().read_i32::<LittleEndian>()? as usize);
                self.put_function_id_static(f, dst_addr)?;
            } else {
                return Err(ProgsError::with_msg(format!(
                    "reserved_copy: invalid static destination address ({})",
                    src_addr
                )));
            }
        } else if dst_addr < GLOBAL_DYNAMIC_START + self.dynamics.len() {
            self.dynamics[dst_addr - GLOBAL_DYNAMIC_START] = src_val;
        } else {
            return Err(ProgsError::with_msg(format!(
                "reserved_copy: destination address out of range ({})",
                src_addr
            )));
        }

        Ok(())
    }
}
