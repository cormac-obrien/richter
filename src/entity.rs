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

use std::cell::{Ref, RefCell};
use math;
use math::Vec3;

pub struct Entity {
    position: RefCell<Vec3>,
    velocity: RefCell<Vec3>,
    angle: RefCell<Vec3>,
}

impl Entity {
    pub fn new() -> Self {
        Entity {
            position: RefCell::new(Vec3::new(0.0, 0.0, 0.0)),
            velocity: RefCell::new(Vec3::new(0.0, 0.0, 0.0)),
            angle: RefCell::new(Vec3::new(0.0, 0.0, 0.0)),
        }
    }

    pub fn get_position(&self) -> Vec3 {
        (*self.position.borrow()).clone()
    }

    pub fn set_position(&self, x: f32, y: f32, z: f32) {
        let mut pos = self.position.borrow_mut();
        pos[0] = x;
        pos[1] = y;
        pos[2] = z;
    }

    pub fn adjust_position(&self, x: f32, y: f32, z: f32) {
        let mut pos = self.position.borrow_mut();
        pos[0] += x;
        pos[1] += y;
        pos[2] += z;
    }

    pub fn get_angle(&self) -> Vec3 {
        (*self.angle.borrow()).clone()
    }

    fn clamp_angle(&self) {
        let mut angle = self.angle.borrow_mut();
        if angle[0] < -math::PI / 2.0 {
            angle[0] = -math::PI / 2.0;
        } else if angle[0] > math::PI / 2.0 {
            angle[0] = math::PI / 2.0;
        }
    }

    pub fn set_angle(&self, x: f32, y: f32, z: f32) {
        {
            let mut angle = self.angle.borrow_mut();
            angle[0] = x;
            angle[1] = y;
            angle[2] = z;
        }
        self.clamp_angle();
    }

    pub fn adjust_angle(&self, x: f32, y: f32, z: f32) {
        {
            let mut angle = self.angle.borrow_mut();
            angle[0] += x;
            angle[1] += y;
            angle[2] += z;
        }
        self.clamp_angle();
    }
}

type StringAddr = i32;
type FuncAddr = i32;

#[repr(C)]
pub struct Edict {
    model_id: f32,
    abs_min: Vec3,
    abs_max: Vec3,
    ltime: f32,
    last_run: f32,
    move_type: f32,
    is_solid: f32,
    origin: Vec3,
    last_origin: Vec3,
    velocity: Vec3,
    angles: Vec3,
    avelocity: Vec3,
    classname: StringAddr,
    modelname: StringAddr,
    frame: f32,
    skin: f32,
    effects: f32,
    mins: Vec3,
    maxs: Vec3,
    size: Vec3,
    touch: FuncAddr,
    use_: FuncAddr,
    think: FuncAddr,
    blocked: FuncAddr,
    nextthink: f32,
    groundentity: i32,
    health: f32,
    frags: f32,
    weapon: f32,
    weaponmodel: StringAddr,
    weaponframe: f32,
    currentammo: f32,
    ammo_shells: f32,
    ammo_nails: f32,
    ammo_rockets: f32,
    ammo_cells: f32,
    items: f32,
    takedamage: f32,
    chain: i32,
    deadflag: f32,
    view_ofs: Vec3,
    button0: f32,
    button1: f32,
    button2: f32,
    impulse: f32,
    fixangle: f32,
    v_angle: Vec3,
    netname: StringAddr,
    enemy: i32,
    flags: f32,
    colormap: f32,
    team: f32,
    max_health: f32,
    teleport_time: f32,
    armortype: f32,
    armorvalue: f32,
    waterlevel: f32,
    watertype: f32,
    ideal_yaw: f32,
    yaw_speed: f32,
    aiment: i32,
    goalentity: i32,
    spawnflags: f32,
    target: StringAddr,
    targetname: StringAddr,
    dmg_take: f32,
    dmg_save: f32,
    dmg_inflictor: i32,
    owner: i32,
    movedir: Vec3,
    message: StringAddr,
    sounds: f32,
    noise: StringAddr,
    noise1: StringAddr,
    noise2: StringAddr,
    noise3: StringAddr,
}
