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

use std::rc::Rc;

use engine;
use progs::EntityId;
use progs::FieldAddr;
use progs::FunctionId;
use progs::GlobalDef;
use progs::ProgsError;
use progs::StringId;

use byteorder::LittleEndian;
use byteorder::ReadBytesExt;
use byteorder::WriteBytesExt;
use cgmath::Vector3;
use chrono::Duration;
use num::FromPrimitive;

pub const GLOBAL_STATIC_COUNT: usize = 64;
pub const GLOBAL_DYNAMIC_START: usize = 64;

#[derive(FromPrimitive)]
pub enum GlobalFloatAddress {
    Time = 3,
    FrameTime = 4,
    ForceRetouch = 5,
    Deathmatch = 7,
    Coop = 8,
    TeamPlay = 9,
    ServerFlags = 10,
    TotalSecrets = 11,
    TotalMonsters = 12,
    FoundSecrets = 13,
    KilledMonsters = 14,
    Arg0 = 15,
    Arg1 = 16,
    Arg2 = 17,
    Arg3 = 18,
    Arg4 = 19,
    Arg5 = 20,
    Arg6 = 21,
    Arg7 = 22,
    Arg8 = 23,
    Arg9 = 24,
    Arg10 = 25,
    Arg11 = 26,
    Arg12 = 27,
    Arg13 = 28,
    Arg14 = 29,
    Arg15 = 30,
    VForwardX = 31,
    VForwardY = 32,
    VForwardZ = 33,
    VUpX = 34,
    VUpY = 35,
    VUpZ = 36,
    VRightX = 37,
    VRightY = 38,
    VRightZ = 39,
    TraceAllSolid = 40,
    TraceStartSolid = 41,
    TraceFraction = 42,
    TraceEndPosX = 43,
    TraceEndPosY = 44,
    TraceEndPosZ = 45,
    TracePlaneNormalX = 46,
    TracePlaneNormalY = 47,
    TracePlaneNormalZ = 48,
    TracePlaneDist = 49,
    TraceInOpen = 51,
    TraceInWater = 52,
}

#[derive(FromPrimitive)]
pub enum GlobalVectorAddress {
    VForward = 31,
    VUp = 34,
    VRight = 37,
    TraceEndPos = 43,
    TracePlaneNormal = 46,
}

#[derive(FromPrimitive)]
pub enum GlobalStringAddress {
    MapName = 6,
}

#[derive(FromPrimitive)]
pub enum GlobalEntityAddress {
    Self_ = 0,
    Other = 1,
    World = 2,
    TraceEntity = 50,
    MsgEntity = 53,
}

#[derive(FromPrimitive)]
pub enum GlobalFieldAddress {}

#[derive(FromPrimitive)]
pub enum GlobalFunctionAddress {
    Main = 54,
    StartFrame = 55,
    PlayerPreThink = 56,
    PlayerPostThink = 57,
    ClientKill = 58,
    ClientConnect = 59,
    PutClientInServer = 60,
    ClientDisconnect = 61,
    SetNewArgs = 62,
    SetChangeArgs = 63,
}

#[derive(Debug)]
pub struct GlobalsStatic {
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
    pub strings: Rc<Box<[u8]>>,
    pub defs: Box<[GlobalDef]>,
    pub statics: GlobalsStatic,
    pub dynamics: Vec<[u8; 4]>,
}

impl Globals {
    fn get_string_as_str(&self, ofs: i32) -> Result<&str, ProgsError> {
        if ofs < 0 {
            return Err(ProgsError::with_msg(
                "get_string_as_str: negative string offset",
            ));
        }

        let ofs = ofs as usize;

        if ofs > self.strings.len() {
            return Err(ProgsError::with_msg(
                "get_string_as_str: out-of-bounds string offset",
            ));
        }

        let mut end_index = ofs;
        while self.strings[end_index] != 0 {
            end_index += 1;
        }

        Ok(
            ::std::str::from_utf8(&self.strings[ofs..end_index]).unwrap(),
        )
    }

    /// Attempts to retrieve an `f32` from the given virtual address.
    pub fn get_float(&self, addr: i16) -> Result<f32, ProgsError> {
        if addr < 0 {
            return Err(ProgsError::with_msg("get_f: negative address"));
        }

        let addr = addr as usize;

        if addr < GLOBAL_STATIC_COUNT {
            self.get_float_static(addr)
        } else {
            self.get_dynamic_float(addr)
        }
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
        if addr > GLOBAL_STATIC_COUNT + self.dynamics.len() {
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

        if addr < GLOBAL_STATIC_COUNT {
            self.put_float_static(val, addr)
        } else {
            self.put_float_dynamic(val, addr)
        }
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
        if addr > GLOBAL_STATIC_COUNT + self.dynamics.len() {
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

        if addr < GLOBAL_STATIC_COUNT {
            self.get_static_vector(addr)
        } else {
            self.get_dynamic_vector(addr)
        }
    }

    fn get_static_vector(&self, addr: usize) -> Result<[f32; 3], ProgsError> {
        let v_addr = match GlobalVectorAddress::from_usize(addr) {
            Some(v) => v,
            None => {
                return Err(ProgsError::with_msg(format!(
                    "get_static_v: invalid static global vector address ({})",
                    addr
                )))
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

    fn get_dynamic_vector(&self, addr: usize) -> Result<[f32; 3], ProgsError> {
        // subtract 2 from range to account for size of vector
        if addr > GLOBAL_STATIC_COUNT + self.dynamics.len() - 2 {
            return Err(ProgsError::with_msg(format!(
                "get_dynamic_v: dynamic global vector address out of range ({})",
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

        if addr < GLOBAL_STATIC_COUNT {
            self.put_vector_static(val, addr)
        } else {
            self.put_vector_dynamic(val, addr)
        }
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
        if addr > GLOBAL_STATIC_COUNT + self.dynamics.len() - 2 {
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

        if addr < GLOBAL_STATIC_COUNT {
            self.get_string_id_static(addr)
        } else {
            self.get_string_id_dynamic(addr)
        }
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
        if addr > GLOBAL_STATIC_COUNT + self.dynamics.len() {
            return Err(ProgsError::with_msg(format!(
                "get_string_id_dynamic: address out of range ({})",
                addr
            )));
        }

        Ok(StringId(self.dynamics[addr - GLOBAL_DYNAMIC_START]
            .as_ref()
            .read_i32::<LittleEndian>()?))
    }

    pub fn put_string_id(&mut self, val: StringId, addr: i16) -> Result<(), ProgsError> {
        if addr < 0 {
            return Err(ProgsError::with_msg("put_string_id: negative address"));
        }

        let addr = addr as usize;

        if addr < GLOBAL_STATIC_COUNT {
            self.put_string_id_static(val, addr)
        } else {
            self.put_string_id_dynamic(val, addr)
        }
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
        if addr > GLOBAL_STATIC_COUNT + self.dynamics.len() {
            return Err(ProgsError::with_msg(format!(
                "put_string_id_dynamic: address out of range ({})",
                addr
            )));
        }

        self.dynamics[addr - GLOBAL_DYNAMIC_START]
            .as_mut()
            .write_i32::<LittleEndian>(val.0)?;

        Ok(())
    }

    pub fn get_entity_id(&self, addr: i16) -> Result<EntityId, ProgsError> {
        if addr < 0 {
            return Err(ProgsError::with_msg("get_entity_id: negative address"));
        }

        let addr = addr as usize;

        if addr < GLOBAL_STATIC_COUNT {
            self.get_entity_id_static(addr)
        } else {
            self.get_entity_id_dynamic(addr)
        }
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
        if addr > GLOBAL_STATIC_COUNT + self.dynamics.len() {
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

        if addr < GLOBAL_STATIC_COUNT {
            self.put_entity_id_static(val, addr)
        } else {
            self.put_entity_id_dynamic(val, addr)
        }
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
        if addr > GLOBAL_STATIC_COUNT + self.dynamics.len() {
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
        if addr > GLOBAL_STATIC_COUNT + self.dynamics.len() {
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
        if addr > GLOBAL_STATIC_COUNT + self.dynamics.len() {
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
        if addr > GLOBAL_STATIC_COUNT + self.dynamics.len() {
            return Err(ProgsError::with_msg(format!(
                "get_function_id_dynamic: address out of range ({})",
                addr
            )));
        }

        Ok(FunctionId(self.dynamics[addr - GLOBAL_DYNAMIC_START]
            .as_ref()
            .read_i32::<LittleEndian>()?))
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
        if addr > GLOBAL_STATIC_COUNT + self.dynamics.len() {
            return Err(ProgsError::with_msg(format!(
                "put_function_id_dynamic: address out of range ({})",
                addr
            )));
        }

        self.dynamics[addr - GLOBAL_DYNAMIC_START]
            .as_mut()
            .write_i32::<LittleEndian>(val.0)?;

        Ok(())
    }

    pub fn generic_copy(&mut self, src_addr: i16, dst_addr: i16) -> Result<(), ProgsError> {
        if src_addr < 0 {
            return Err(ProgsError::with_msg(format!(
                "generic_copy: negative source address ({})",
                src_addr
            )));
        }

        if dst_addr < 0 {
            return Err(ProgsError::with_msg(format!(
                "generic_copy: negative destination address ({})",
                dst_addr
            )));
        }

        let src_addr = src_addr as usize;
        let dst_addr = dst_addr as usize;

        if src_addr > self.dynamics.len() + GLOBAL_DYNAMIC_START {
            return Err(ProgsError::with_msg(format!(
                "generic_copy: source address out of range({})",
                src_addr
            )));
        }

        if dst_addr > self.dynamics.len() + GLOBAL_DYNAMIC_START {
            return Err(ProgsError::with_msg(
                format!("generic_copy: address out of range({})", dst_addr),
            ));
        }

        if src_addr < GLOBAL_STATIC_COUNT {
            // don't check for vectors -- those addresses will be overlapped by floats
            if GlobalFloatAddress::from_usize(src_addr).is_some() {
                let f = self.get_float_static(src_addr)?;
                self.put_float_dynamic(f, dst_addr)?;
            } else if GlobalStringAddress::from_usize(src_addr).is_some() {
                let s = self.get_string_id_static(src_addr)?;
                self.put_string_id_dynamic(s, dst_addr)?;
            } else if GlobalEntityAddress::from_usize(src_addr).is_some() {
                let e = self.get_entity_id_static(src_addr)?;
                self.put_entity_id_dynamic(e, dst_addr)?;
            } else {
                panic!("Failed to typecheck address {}", src_addr);
            }
        } else {
            // just copy the memory
            self.dynamics[dst_addr] = self.dynamics[src_addr];
        }

        Ok(())
    }
}
