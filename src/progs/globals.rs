// Copyright © 2015 Cormac O'Brien.
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

use std::io::{Cursor, Read};
use std::mem::{size_of, transmute};

use load::{Load, LoadError};
use math::Vec3;
use progs::{FunctionId, StringId};

pub const GLOBALS_COUNT: usize = 90;
pub const GLOBALS_SIZE: usize = GLOBALS_COUNT * 4;

#[repr(C)]
pub struct Globals {
    pad: [i32; 28],
    self_: i32,
    other: i32,
    world: i32,
    time: f32,
    frametime: f32,
    newmis: i32,
    force_retouch: f32,
    mapname: StringId,
    serverflags: f32,
    total_secrets: f32,
    total_monsters: f32,
    found_secrets: f32,
    killed_monsters: f32,
    args: [f32; 16],
    v_forward: Vec3,
    v_up: Vec3,
    v_right: Vec3,
    trace_allsolid: f32,
    trace_startsolid: f32,
    trace_fraction: f32,
    trace_endpos: Vec3,
    trace_plane_normal: Vec3,
    trace_plane_dist: f32,
    trace_ent: i32,
    trace_inopen: f32,
    trace_inwater: f32,
    msg_entity: i32,
    main: FunctionId,
    start_frame: FunctionId,
    player_pre_think: FunctionId,
    player_post_think: FunctionId,
    client_kill: FunctionId,
    client_connect: FunctionId,
    put_client_in_server: FunctionId,
    client_disconnect: FunctionId,
    set_new_parms: FunctionId,
    set_change_parms: FunctionId,
}

impl Globals {
    pub fn new() -> Globals {
        Globals {
            pad: [0; 28],
            self_: 0,
            other: 0,
            world: 0,
            time: 0.0,
            frametime: 0.0,
            newmis: 0,
            force_retouch: 0.0,
            mapname: StringId(0),
            serverflags: 0.0,
            total_secrets: 0.0,
            total_monsters: 0.0,
            found_secrets: 0.0,
            killed_monsters: 0.0,
            args: [0.0; 16],
            v_forward: Vec3::new(0.0, 0.0, 0.0),
            v_up: Vec3::new(0.0, 0.0, 0.0),
            v_right: Vec3::new(0.0, 0.0, 0.0),
            trace_allsolid: 0.0,
            trace_startsolid: 0.0,
            trace_fraction: 0.0,
            trace_endpos: Vec3::new(0.0, 0.0, 0.0),
            trace_plane_normal: Vec3::new(0.0, 0.0, 0.0),
            trace_plane_dist: 0.0,
            trace_ent: 0,
            trace_inopen: 0.0,
            trace_inwater: 0.0,
            msg_entity: 0,
            main: FunctionId(0),
            start_frame: FunctionId(0),
            player_pre_think: FunctionId(0),
            player_post_think: FunctionId(0),
            client_kill: FunctionId(0),
            client_connect: FunctionId(0),
            put_client_in_server: FunctionId(0),
            client_disconnect: FunctionId(0),
            set_new_parms: FunctionId(0),
            set_change_parms: FunctionId(0),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn globals_size_check() {
        assert_eq!(size_of::<Globals>() % size_of::<f32>(), 0);
        assert_eq!(size_of::<Globals>(), GLOBALS_SIZE);
    }
}
