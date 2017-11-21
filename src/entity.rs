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

use progs::EntityId;
use progs::FunctionId;
use progs::StringId;

use cgmath::Deg;
use cgmath::Vector3;

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

const OFS_ANGLE_VELOCITY: usize = 22;
const OFS_ANGLE_VELOCITY_X: usize = 22;
const OFS_ANGLE_VELOCITY_Y: usize = 23;
const OFS_ANGLE_VELOCITY_Z: usize = 24;

const OFS_PUNCH_ANGLE: usize = 25;
const OFS_PUNCH_ANGLE_X: usize = 25;
const OFS_PUNCH_ANGLE_Y: usize = 26;
const OFS_PUNCH_ANGLE_Z: usize = 27;

const OFS_CLASS_NAME: usize = 28;
const OFS_MODEL_NAME: usize = 29;
const OFS_FRAME: usize = 30;
const OFS_SKIN: usize = 31;
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
const OFS_WATER_TYPE: usize = 84;
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

pub struct Entity {
    // index in the model list
    model_index: f32,

    // absolute minimum extent of entity
    abs_min: Vector3<f32>,

    // absolute maximum extent of entity
    abs_max: Vector3<f32>,

    // how far in time this entity has been processed
    local_time: f32,

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
    angle_velocity: Vector3<Deg<f32>>,

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
    next_think: f32,

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
    ideal_pitch: f32,

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
    teleport_time: f32,

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
}
