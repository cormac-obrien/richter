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

use engine;
use progs::EntityId;
use progs::FunctionId;
use progs::StringId;

use byteorder::LittleEndian;
use byteorder::ReadBytesExt;
use cgmath::Deg;
use cgmath::Vector3;
use cgmath::Zero;
use chrono::Duration;

// TODO:
// - The OFS_* constants can probably be converted to enums based on their types and typechecked on
//   access using num::FromPrimitive. They also only apply to NetQuake; a different set must be
//   defined for QuakeWorld.

const OFS_MODEL_INDEX: usize = 0;

const OFS_ABS_MIN: usize = 1;
const OFS_ABS_MIN_X: usize = 1;
const OFS_ABS_MIN_Y: usize = 2;
const OFS_ABS_MIN_Z: usize = 3;

const OFS_ABS_MAX: usize = 4;
const OFS_ABS_MAX_X: usize = 4;
const OFS_ABS_MAX_Y: usize = 5;
const OFS_ABS_MAX_Z: usize = 6;

const OFS_LOCAL_TIME: usize = 7;
const OFS_MOVE_TYPE: usize = 8;
const OFS_SOLID: usize = 9;

const OFS_ORIGIN: usize = 10;
const OFS_ORIGIN_X: usize = 10;
const OFS_ORIGIN_Y: usize = 11;
const OFS_ORIGIN_Z: usize = 12;

const OFS_OLD_ORIGIN: usize = 13;
const OFS_OLD_ORIGIN_X: usize = 13;
const OFS_OLD_ORIGIN_Y: usize = 14;
const OFS_OLD_ORIGIN_Z: usize = 15;

const OFS_VELOCITY: usize = 16;
const OFS_VELOCITY_X: usize = 16;
const OFS_VELOCITY_Y: usize = 17;
const OFS_VELOCITY_Z: usize = 18;

const OFS_ANGLES: usize = 19;
const OFS_ANGLES_X: usize = 19;
const OFS_ANGLES_Y: usize = 20;
const OFS_ANGLES_Z: usize = 21;

const OFS_ANGULAR_VELOCITY: usize = 22;
const OFS_ANGULAR_VELOCITY_X: usize = 22;
const OFS_ANGULAR_VELOCITY_Y: usize = 23;
const OFS_ANGULAR_VELOCITY_Z: usize = 24;

const OFS_PUNCH_ANGLE: usize = 25;
const OFS_PUNCH_ANGLE_X: usize = 25;
const OFS_PUNCH_ANGLE_Y: usize = 26;
const OFS_PUNCH_ANGLE_Z: usize = 27;

const OFS_CLASS_NAME: usize = 28;
const OFS_MODEL_NAME: usize = 29;
const OFS_FRAME_ID: usize = 30;
const OFS_SKIN_ID: usize = 31;
const OFS_EFFECTS: usize = 32;

const OFS_MINS: usize = 33;
const OFS_MINS_X: usize = 33;
const OFS_MINS_Y: usize = 34;
const OFS_MINS_Z: usize = 35;

const OFS_MAXS: usize = 36;
const OFS_MAXS_X: usize = 36;
const OFS_MAXS_Y: usize = 37;
const OFS_MAXS_Z: usize = 38;

const OFS_SIZE: usize = 39;
const OFS_SIZE_X: usize = 39;
const OFS_SIZE_Y: usize = 40;
const OFS_SIZE_Z: usize = 41;

const OFS_TOUCH_FNC: usize = 42;
const OFS_USE_FNC: usize = 43;
const OFS_THINK_FNC: usize = 44;
const OFS_BLOCKED_FNC: usize = 45;
const OFS_NEXT_THINK: usize = 46;
const OFS_GROUND_ENTITY: usize = 47;
const OFS_HEALTH: usize = 48;
const OFS_FRAGS: usize = 49;
const OFS_WEAPON: usize = 50;
const OFS_WEAPON_MODEL: usize = 51;
const OFS_WEAPON_FRAME: usize = 52;
const OFS_CURRENT_AMMO: usize = 53;
const OFS_AMMO_SHELLS: usize = 54;
const OFS_AMMO_NAILS: usize = 55;
const OFS_AMMO_ROCKETS: usize = 56;
const OFS_AMMO_CELLS: usize = 57;
const OFS_ITEMS: usize = 58;
const OFS_TAKE_DAMAGE: usize = 59;
const OFS_CHAIN: usize = 60;
const OFS_DEAD_FLAG: usize = 61;

const OFS_VIEW_OFFSET: usize = 62;
const OFS_VIEW_OFFSET_X: usize = 62;
const OFS_VIEW_OFFSET_Y: usize = 63;
const OFS_VIEW_OFFSET_Z: usize = 64;

const OFS_BUTTON_0: usize = 65;
const OFS_BUTTON_1: usize = 66;
const OFS_BUTTON_2: usize = 67;
const OFS_IMPULSE: usize = 68;
const OFS_FIX_ANGLE: usize = 69;

const OFS_VIEW_ANGLE: usize = 70;
const OFS_VIEW_ANGLE_X: usize = 70;
const OFS_VIEW_ANGLE_Y: usize = 71;
const OFS_VIEW_ANGLE_Z: usize = 72;

const OFS_IDEAL_PITCH: usize = 73;
const OFS_NET_NAME: usize = 74;
const OFS_ENEMY: usize = 75;
const OFS_FLAGS: usize = 76;
const OFS_COLORMAP: usize = 77;
const OFS_TEAM: usize = 78;
const OFS_MAX_HEALTH: usize = 79;
const OFS_TELEPORT_TIME: usize = 80;
const OFS_ARMOR_STRENGTH: usize = 81;
const OFS_ARMOR_VALUE: usize = 82;
const OFS_WATER_LEVEL: usize = 83;
const OFS_CONTENTS: usize = 84;
const OFS_IDEAL_YAW: usize = 85;
const OFS_YAW_SPEED: usize = 86;
const OFS_AIM_ENTITY: usize = 87;
const OFS_GOAL_ENTITY: usize = 88;
const OFS_SPAWN_FLAGS: usize = 89;
const OFS_TARGET: usize = 90;
const OFS_TARGET_NAME: usize = 91;
const OFS_DMG_TAKE: usize = 92;
const OFS_DMG_SAVE: usize = 93;
const OFS_DMG_INFLICTOR: usize = 94;
const OFS_OWNER: usize = 95;

const OFS_MOVE_DIRECTION: usize = 96;
const OFS_MOVE_DIRECTION_X: usize = 96;
const OFS_MOVE_DIRECTION_Y: usize = 97;
const OFS_MOVE_DIRECTION_Z: usize = 98;

const OFS_MESSAGE: usize = 99;
const OFS_SOUNDS: usize = 100;
const OFS_NOISE_0: usize = 101;
const OFS_NOISE_1: usize = 102;
const OFS_NOISE_2: usize = 103;
const OFS_NOISE_3: usize = 104;

// dynamic entity fields start after this point (i.e. defined in progs.dat, not accessible here)
const OFS_DYNAMIC_START: usize = 105;

const STATIC_ADDRESS_COUNT: usize = 105;

pub struct Entity {
    // index in the model list
    model_index: f32,

    // absolute minimum extent of entity
    abs_min: Vector3<f32>,

    // absolute maximum extent of entity
    abs_max: Vector3<f32>,

    // how far in time this entity has been processed
    local_time: Duration,

    // TODO find definitions for movement types
    move_type: f32,

    // is this entity solid (i.e. does it have collision)
    solid: f32,

    // this entity's current position
    origin: Vector3<f32>,

    // this entity's position prior to last movement
    old_origin: Vector3<f32>,

    // this entity's velocity vector
    velocity: Vector3<f32>,

    // this entity's pitch, yaw, and roll
    angles: Vector3<Deg<f32>>,

    // the rate at which this entity is rotating (only in pitch and yaw)
    angular_velocity: Vector3<Deg<f32>>,

    // the temporary angle modifier applied by damage and recoil
    punch_angle: Vector3<Deg<f32>>,

    // entity class name
    class_name: StringId,

    // name of alias model (MDL) associated with this entity
    model_name: StringId,

    // animation frame in the alias model
    frame_id: f32,

    // skin index in the alias model
    skin_id: f32,

    // TODO: better explanation
    // model effects
    effects: f32,

    // minimum extent of entity relative to origin
    mins: Vector3<f32>,

    // maximum extent of entity relative to origin
    maxs: Vector3<f32>,

    // dimensions of this entity (maxs - mins)
    size: Vector3<f32>,

    // function to call when another entity collides with this one
    touch_fnc: FunctionId,

    // function to call when +use is issued on this entity
    use_fnc: FunctionId,

    // function to call when next_think elapses
    think_fnc: FunctionId,

    // function to call when this entity is blocked from movement
    blocked_fnc: FunctionId,

    // time remaining until next think
    next_think: Duration,

    // TODO: ???
    ground_entity: EntityId,

    // current health
    health: f32,

    // current kill count (multiplayer)
    frags: f32,

    // equipped weapon (bitflags)
    weapon: f32,

    // alias model for the equipped weapon
    weapon_model: StringId,

    // animation frame for the weapon model
    weapon_frame: f32,

    // ammo for current weapon
    current_ammo: f32,

    // shotgun ammo remaining
    ammo_shells: f32,

    // nailgun ammo remaining
    ammo_nails: f32,

    // rockets remaining
    ammo_rockets: f32,

    // energy cells remaining (for lightning gun)
    ammo_cells: f32,

    // bitflags representing what items player has
    items: f32,

    // can this entity be damaged?
    take_damage: f32,

    // next entity in a chained list
    chain: EntityId,

    // is this entity dead?
    dead_flag: f32,

    // position of camera relative to origin
    view_offset: Vector3<f32>,

    // +fire
    button_0: f32,

    // +use
    button_1: f32,

    // +jump
    button_2: f32,

    // TODO: document impulse
    impulse: f32,

    // TODO: something to do with updating player angle
    fix_angle: f32,

    // player view angle
    view_angle: Vector3<Deg<f32>>,

    // calculated default view angle
    ideal_pitch: Deg<f32>,

    // screen name
    net_name: StringId,

    // this entity's enemy (for monsters)
    enemy: EntityId,

    // TODO: ?
    flags: f32,

    // player colors in multiplayer
    colormap: f32,

    // team number in multiplayer
    team: f32,

    // maximum player health
    max_health: f32,

    // time player last teleported
    teleport_time: Duration,

    // percentage of incoming damage blocked (between 0 and 1)
    armor_strength: f32,

    // armor points remaining
    armor_value: f32,

    // how submerged this entity is, 0 (none) -> 3 (full)
    water_level: f32,

    // one of the CONTENTS_* constants (bspfile.h)
    contents: f32,

    // ideal pathfinding direction (for monsters)
    ideal_yaw: Deg<f32>,

    // turn rate
    yaw_speed: Deg<f32>,

    // TODO: maybe entity being aimed at?
    aim_entity: EntityId,

    // monster's goal entity
    goal_entity: EntityId,

    // meaning differs based on classname
    spawn_flags: f32,

    // monster's target
    target: StringId,

    // name of target
    target_name: StringId,

    // damage accumulator
    dmg_take: f32,

    // damage block accumulator?
    dmg_save: f32,

    // which entity inflicted damage
    dmg_inflictor: EntityId,

    // entity that owns this entity
    owner: EntityId,

    // which direction this entity should move
    move_direction: Vector3<f32>,

    // message to display on entity trigger
    message: StringId,

    // sound ID
    sounds: f32,

    // sounds played on noise channels
    noise_0: StringId,
    noise_1: StringId,
    noise_2: StringId,
    noise_3: StringId,

    dynamic_fields: Vec<[u8; 4]>,
}

impl Default for Entity {
    fn default() -> Self {
        Entity {
            model_index: 0.0,
            abs_min: Vector3::zero(),
            abs_max: Vector3::zero(),
            local_time: Duration::seconds(0),
            move_type: 0.0,
            solid: 0.0,
            origin: Vector3::zero(),
            old_origin: Vector3::zero(),
            velocity: Vector3::zero(),
            angles: Vector3::new(Deg(0.0), Deg(0.0), Deg(0.0)),
            angular_velocity: Vector3::new(Deg(0.0), Deg(0.0), Deg(0.0)),
            punch_angle: Vector3::new(Deg(0.0), Deg(0.0), Deg(0.0)),
            class_name: StringId(0),
            model_name: StringId(0),
            frame_id: 0.0,
            skin_id: 0.0,
            effects: 0.0,
            mins: Vector3::zero(),
            maxs: Vector3::zero(),
            size: Vector3::zero(),
            touch_fnc: FunctionId(0),
            use_fnc: FunctionId(0),
            think_fnc: FunctionId(0),
            blocked_fnc: FunctionId(0),
            next_think: Duration::seconds(0),
            ground_entity: EntityId(0),
            health: 0.0,
            frags: 0.0,
            weapon: 0.0,
            weapon_model: StringId(0),
            weapon_frame: 0.0,
            current_ammo: 0.0,
            ammo_shells: 0.0,
            ammo_nails: 0.0,
            ammo_rockets: 0.0,
            ammo_cells: 0.0,
            items: 0.0,
            take_damage: 0.0,
            chain: EntityId(0),
            dead_flag: 0.0,
            view_offset: Vector3::zero(),
            button_0: 0.0,
            button_1: 0.0,
            button_2: 0.0,
            impulse: 0.0,
            fix_angle: 0.0,
            view_angle: Vector3::new(Deg(0.0), Deg(0.0), Deg(0.0)),
            ideal_pitch: Deg(0.0),
            net_name: StringId(0),
            enemy: EntityId(0),
            flags: 0.0,
            colormap: 0.0,
            team: 0.0,
            max_health: 0.0,
            teleport_time: Duration::seconds(0),
            armor_strength: 0.0,
            armor_value: 0.0,
            water_level: 0.0,
            contents: 0.0,
            ideal_yaw: Deg(0.0),
            yaw_speed: Deg(0.0),
            aim_entity: EntityId(0),
            goal_entity: EntityId(0),
            spawn_flags: 0.0,
            target: StringId(0),
            target_name: StringId(0),
            dmg_take: 0.0,
            dmg_save: 0.0,
            dmg_inflictor: EntityId(0),
            owner: EntityId(0),
            move_direction: Vector3::zero(),
            message: StringId(0),
            sounds: 0.0,
            noise_0: StringId(0),
            noise_1: StringId(0),
            noise_2: StringId(0),
            noise_3: StringId(0),
            dynamic_fields: Vec::new(),
        }
    }
}

impl Entity {
    // TODO: temp function, remove this
    pub fn with_field_count(field_count: usize) -> Entity {
        if field_count < STATIC_ADDRESS_COUNT {
            panic!("Invalid field count");
        }

        let mut dynamic_fields = Vec::with_capacity(field_count - STATIC_ADDRESS_COUNT);
        for _ in 0..field_count - STATIC_ADDRESS_COUNT {
            dynamic_fields.push([0; 4]);
        }

        Entity {
            dynamic_fields,
            ..Default::default()
        }
    }

    pub fn get_f(&self, ofs: i16) -> Result<f32, ()> {
        if ofs < 0 {
            panic!("negative offset");
        }

        let ofs = ofs as usize;

        if ofs >= OFS_DYNAMIC_START + self.dynamic_fields.len() {
            println!("out-of-bounds offset ({})", ofs);
            return Ok(0.0);
        }

        if ofs < OFS_DYNAMIC_START {
            self.get_static_f(ofs)
        } else {
            self.get_dynamic_f(ofs)
        }
    }

    fn get_static_f(&self, ofs: usize) -> Result<f32, ()> {
        if ofs >= OFS_DYNAMIC_START {
            panic!("Invalid offset for static entity field");
        }

        Ok(match ofs {
            OFS_MODEL_INDEX => self.model_index,

            OFS_ABS_MIN_X => self.abs_min[0],
            OFS_ABS_MIN_Y => self.abs_min[1],
            OFS_ABS_MIN_Z => self.abs_min[2],

            OFS_ABS_MAX_X => self.abs_max[0],
            OFS_ABS_MAX_Y => self.abs_max[1],
            OFS_ABS_MAX_Z => self.abs_max[2],

            OFS_LOCAL_TIME => engine::duration_to_f32(self.local_time),
            OFS_MOVE_TYPE => self.move_type,
            OFS_SOLID => self.solid,

            OFS_ORIGIN_X => self.origin[0],
            OFS_ORIGIN_Y => self.origin[1],
            OFS_ORIGIN_Z => self.origin[2],

            OFS_OLD_ORIGIN_X => self.old_origin[0],
            OFS_OLD_ORIGIN_Y => self.old_origin[1],
            OFS_OLD_ORIGIN_Z => self.old_origin[2],

            OFS_VELOCITY_X => self.velocity[0],
            OFS_VELOCITY_Y => self.velocity[1],
            OFS_VELOCITY_Z => self.velocity[2],

            OFS_ANGLES_X => self.angles[0].0,
            OFS_ANGLES_Y => self.angles[1].0,
            OFS_ANGLES_Z => self.angles[2].0,

            OFS_ANGULAR_VELOCITY_X => self.angular_velocity[0].0,
            OFS_ANGULAR_VELOCITY_Y => self.angular_velocity[1].0,
            OFS_ANGULAR_VELOCITY_Z => self.angular_velocity[2].0,

            OFS_PUNCH_ANGLE_X => self.punch_angle[0].0,
            OFS_PUNCH_ANGLE_Y => self.punch_angle[1].0,
            OFS_PUNCH_ANGLE_Z => self.punch_angle[2].0,

            OFS_FRAME_ID => self.frame_id,
            OFS_SKIN_ID => self.skin_id,
            OFS_EFFECTS => self.effects,

            OFS_MINS_X => self.mins[0],
            OFS_MINS_Y => self.mins[1],
            OFS_MINS_Z => self.mins[2],

            OFS_MAXS_X => self.maxs[0],
            OFS_MAXS_Y => self.maxs[1],
            OFS_MAXS_Z => self.maxs[2],

            OFS_SIZE_X => self.size[0],
            OFS_SIZE_Y => self.size[1],
            OFS_SIZE_Z => self.size[2],

            OFS_NEXT_THINK => engine::duration_to_f32(self.next_think),
            OFS_HEALTH => self.health,
            OFS_FRAGS => self.frags,
            OFS_WEAPON => self.weapon,
            OFS_WEAPON_FRAME => self.weapon_frame,
            OFS_CURRENT_AMMO => self.current_ammo,
            OFS_AMMO_SHELLS => self.ammo_shells,
            OFS_AMMO_NAILS => self.ammo_nails,
            OFS_AMMO_ROCKETS => self.ammo_rockets,
            OFS_AMMO_CELLS => self.ammo_cells,
            OFS_ITEMS => self.items,
            OFS_TAKE_DAMAGE => self.take_damage,
            OFS_DEAD_FLAG => self.dead_flag,

            OFS_VIEW_OFFSET_X => self.view_offset[0],
            OFS_VIEW_OFFSET_Y => self.view_offset[1],
            OFS_VIEW_OFFSET_Z => self.view_offset[2],

            OFS_BUTTON_0 => self.button_0,
            OFS_BUTTON_1 => self.button_1,
            OFS_BUTTON_2 => self.button_2,
            OFS_IMPULSE => self.impulse,
            OFS_FIX_ANGLE => self.fix_angle,

            OFS_VIEW_ANGLE_X => self.view_angle[0].0,
            OFS_VIEW_ANGLE_Y => self.view_angle[1].0,
            OFS_VIEW_ANGLE_Z => self.view_angle[2].0,

            OFS_IDEAL_PITCH => self.ideal_pitch.0,
            OFS_FLAGS => self.flags,
            OFS_COLORMAP => self.colormap,
            OFS_TEAM => self.team,
            OFS_MAX_HEALTH => self.max_health,
            OFS_TELEPORT_TIME => engine::duration_to_f32(self.teleport_time),
            OFS_ARMOR_STRENGTH => self.armor_strength,
            OFS_ARMOR_VALUE => self.armor_value,
            OFS_WATER_LEVEL => self.water_level,
            OFS_CONTENTS => self.contents,
            OFS_IDEAL_YAW => self.ideal_yaw.0,
            OFS_YAW_SPEED => self.yaw_speed.0,
            OFS_SPAWN_FLAGS => self.spawn_flags,
            OFS_DMG_TAKE => self.dmg_take,
            OFS_DMG_SAVE => self.dmg_save,

            OFS_MOVE_DIRECTION_X => self.move_direction[0],
            OFS_MOVE_DIRECTION_Y => self.move_direction[1],
            OFS_MOVE_DIRECTION_Z => self.move_direction[2],

            OFS_SOUNDS => self.sounds,

            _ => return Err(()),
        })
    }

    fn get_dynamic_f(&self, ofs: usize) -> Result<f32, ()> {
        Ok(
            self.dynamic_fields[ofs - OFS_DYNAMIC_START]
                .as_ref()
                .read_f32::<LittleEndian>()
                .unwrap(),
        )
    }

    fn get_v(&self, ofs: i16) -> Result<[f32; 3], ()> {
        if ofs < 0 {
            panic!("negative offset");
        }

        let ofs = ofs as usize;

        // subtract 2 to account for size of vector
        if ofs >= OFS_DYNAMIC_START + self.dynamic_fields.len() - 2 {
            println!("out-of-bounds offset ({})", ofs);
            // TODO: proper error
            return Ok([0.0; 3]);
        }

        if ofs < OFS_DYNAMIC_START {
            self.get_static_v(ofs)
        } else {
            self.get_dynamic_v(ofs)
        }
    }

    fn get_static_v(&self, ofs: usize) -> Result<[f32; 3], ()> {
        Ok(match ofs {
            OFS_ABS_MIN => self.abs_min.into(),
            OFS_ABS_MAX => self.abs_max.into(),
            OFS_ORIGIN => self.origin.into(),
            OFS_OLD_ORIGIN => self.old_origin.into(),
            OFS_VELOCITY => self.velocity.into(),
            OFS_ANGLES => engine::deg_vector_to_f32_vector(self.angles).into(),
            OFS_ANGULAR_VELOCITY => engine::deg_vector_to_f32_vector(self.angular_velocity).into(),
            OFS_PUNCH_ANGLE => engine::deg_vector_to_f32_vector(self.punch_angle).into(),
            OFS_MINS => self.mins.into(),
            OFS_MAXS => self.maxs.into(),
            OFS_SIZE => self.size.into(),
            OFS_VIEW_OFFSET => self.view_offset.into(),
            OFS_VIEW_ANGLE => engine::deg_vector_to_f32_vector(self.view_angle).into(),
            OFS_MOVE_DIRECTION => self.move_direction.into(),
            _ => {
                println!("invalid static vector field {}", ofs);
                [0.0; 3]
            }
        })
    }

    fn get_dynamic_v(&self, ofs: usize) -> Result<[f32; 3], ()> {
        let mut v = [0.0; 3];

        for c in 0..v.len() {
            v[c] = self.get_dynamic_f(ofs + c)?;
        }

        Ok(v)
    }
}
