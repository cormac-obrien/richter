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
use progs::ProgsError;

use cgmath::Deg;
use cgmath::Vector3;
use cgmath::Zero;
use chrono::Duration;
use num::FromPrimitive;

#[derive(Debug, FromPrimitive)]
pub enum FieldAddrFloat {
    ModelIndex = 0,
    AbsMinX = 1,
    AbsMinY = 2,
    AbsMinZ = 3,
    AbsMaxX = 4,
    AbsMaxY = 5,
    AbsMaxZ = 6,
    LocalTime = 7,
    MoveType = 8,
    Solid = 9,
    OriginX = 10,
    OriginY = 11,
    OriginZ = 12,
    OldOriginX = 13,
    OldOriginY = 14,
    OldOriginZ = 15,
    VelocityX = 16,
    VelocityY = 17,
    VelocityZ = 18,
    AnglesX = 19,
    AnglesY = 20,
    AnglesZ = 21,
    AngularVelocityX = 22,
    AngularVelocityY = 23,
    AngularVelocityZ = 24,
    PunchAngleX = 25,
    PunchAngleY = 26,
    PunchAngleZ = 27,
    FrameId = 30,
    SkinId = 31,
    Effects = 32,
    MinsX = 33,
    MinsY = 34,
    MinsZ = 35,
    MaxsX = 36,
    MaxsY = 37,
    MaxsZ = 38,
    SizeX = 39,
    SizeY = 40,
    SizeZ = 41,
    NextThink = 46,
    Health = 48,
    Frags = 49,
    Weapon = 50,
    WeaponFrame = 52,
    CurrentAmmo = 53,
    AmmoShells = 54,
    AmmoNails = 55,
    AmmoRockets = 56,
    AmmoCells = 57,
    Items = 58,
    TakeDamage = 59,
    DeadFlag = 61,
    ViewOffsetX = 62,
    ViewOffsetY = 63,
    ViewOffsetZ = 64,
    Button0 = 65,
    Button1 = 66,
    Button2 = 67,
    Impulse = 68,
    FixAngle = 69,
    ViewAngleX = 70,
    ViewAngleY = 71,
    ViewAngleZ = 72,
    IdealPitch = 73,
    Flags = 76,
    Colormap = 77,
    Team = 78,
    MaxHealth = 79,
    TeleportTime = 80,
    ArmorStrength = 81,
    ArmorValue = 82,
    WaterLevel = 83,
    Contents = 84,
    IdealYaw = 85,
    YawSpeed = 86,
    SpawnFlags = 89,
    DmgTake = 92,
    DmgSave = 93,
    MoveDirectionX = 96,
    MoveDirectionY = 97,
    MoveDirectionZ = 98,
    Sounds = 100,
}

#[derive(Debug, FromPrimitive)]
pub enum FieldAddrVector {
    AbsMin = 1,
    AbsMax = 4,
    Origin = 10,
    OldOrigin = 13,
    Velocity = 16,
    Angles = 19,
    AngularVelocity = 22,
    PunchAngle = 25,
    Mins = 33,
    Maxs = 36,
    Size = 39,
    ViewOffset = 62,
    ViewAngle = 70,
    MoveDirection = 96,
}

#[derive(Debug, FromPrimitive)]
pub enum FieldAddrStringId {
    ClassName = 28,
    ModelName = 29,
    WeaponModelName = 51,
    NetName = 74,
    Target = 90,
    TargetName = 91,
    Message = 99,
    Noise0Name = 101,
    Noise1Name = 102,
    Noise2Name = 103,
    Noise3Name = 104,
}

#[derive(Debug, FromPrimitive)]
pub enum FieldAddrEntityId {
    Ground = 47,
    Chain = 60,
    Enemy = 75,
    Aim = 87,
    Goal = 88,
    DmgInflictor = 94,
    Owner = 95,
}

#[derive(Debug, FromPrimitive)]
pub enum FieldAddrFunctionId {
    Touch = 42,
    Use = 43,
    Think = 44,
    Blocked = 45,
}

#[derive(Copy, Clone, FromPrimitive)]
pub enum MoveType {
    None = 0,
    AngleNoClip = 1,
    AngleClip = 2,
    Walk = 3,
    Step = 4,
    Fly = 5,
    Toss = 6,
    Push = 7,
    NoClip = 8,
    FlyMissile = 9,
    Bounce = 10,
}

bitflags! {
    pub struct EntityFlags: u16 {
        const FLY            = 0b0000000000001;
        const SWIM           = 0b0000000000010;
        const CONVEYOR       = 0b0000000000100;
        const CLIENT         = 0b0000000001000;
        const IN_WATER       = 0b0000000010000;
        const MONSTER        = 0b0000000100000;
        const GOD_MODE       = 0b0000001000000;
        const NO_TARGET      = 0b0000010000000;
        const ITEM           = 0b0000100000000;
        const ON_GROUND      = 0b0001000000000;
        const PARTIAL_GROUND = 0b0010000000000;
        const WATER_JUMP     = 0b0100000000000;
        const JUMP_RELEASED  = 0b1000000000000;
    }
}

bitflags! {
    pub struct EntityEffects: u16 {
        const BRIGHT_FIELD = 0b0001;
        const MUZZLE_FLASH = 0b0010;
        const BRIGHT_LIGHT = 0b0100;
        const DIM_LIGHT    = 0b1000;
    }
}

fn float_addr(addr: usize) -> Result<FieldAddrFloat, ProgsError> {
    match FieldAddrFloat::from_usize(addr) {
        Some(f) => Ok(f),
        None => {
            Err(ProgsError::with_msg(
                format!("float_addr: invalid address ({})", addr),
            ))
        }
    }
}

fn vector_addr(addr: usize) -> Result<FieldAddrVector, ProgsError> {
    match FieldAddrVector::from_usize(addr) {
        Some(v) => Ok(v),
        None => {
            Err(ProgsError::with_msg(
                format!("vector_addr: invalid address ({})", addr),
            ))
        }
    }
}

/// Statically defined fields for an entity.
///
/// The different variants represent different classes of entity.
pub enum EntityStatics {
    Generic(GenericEntityStatics),
}

/// Static variables for an ambient noise entity.
pub struct AmbientEntityStatics {
    pub class_name: StringId,
    pub origin: Vector3<f32>,
}

impl AmbientEntityStatics {
    pub fn get_float(&self, addr: usize) -> Result<f32, ProgsError> {
        Ok(match float_addr(addr)? {
            FieldAddrFloat::OriginX => self.origin[0],
            FieldAddrFloat::OriginY => self.origin[1],
            FieldAddrFloat::OriginZ => self.origin[2],
            f => panic!("attempted access of {:?} on AmbientEntityStatics", f),
        })
    }

    pub fn put_float(&mut self, val: f32, addr: usize) -> Result<(), ProgsError> {
        match float_addr(addr)? {
            FieldAddrFloat::OriginX => self.origin[0] = val,
            FieldAddrFloat::OriginY => self.origin[1] = val,
            FieldAddrFloat::OriginZ => self.origin[2] = val,
            f => panic!("attempted access of {:?} on AmbientEntityStatics", f),
        }

        Ok(())
    }

    pub fn get_vector(&self, addr: usize) -> Result<[f32; 3], ProgsError> {
        Ok(match vector_addr(addr)? {
            FieldAddrVector::Origin => self.origin.into(),
            v => panic!("attempted access of {:?} on AmbientEntityStatics", v),
        })
    }

    pub fn put_vector(&mut self, val: [f32; 3], addr: usize) -> Result<(), ProgsError> {
        Ok(match vector_addr(addr)? {
            FieldAddrVector::Origin => self.origin = Vector3::from(val),
            v => panic!("attempted access of {:?} on AmbientEntityStatics", v),
        })
    }
}

/// Statically defined fields which may apply to any entity.
pub struct GenericEntityStatics {
    // index in the model list
    pub model_index: f32,

    // absolute minimum extent of entity
    pub abs_min: Vector3<f32>,

    // absolute maximum extent of entity
    pub abs_max: Vector3<f32>,

    // how far in time this entity has been processed
    pub local_time: Duration,

    // TODO find definitions for movement types
    pub move_type: MoveType,

    // is this entity solid (i.e. does it have collision)
    pub solid: f32,

    // this entity's current position
    pub origin: Vector3<f32>,

    // this entity's position prior to last movement
    pub old_origin: Vector3<f32>,

    // this entity's velocity vector
    pub velocity: Vector3<f32>,

    // this entity's pitch, yaw, and roll
    pub angles: Vector3<Deg<f32>>,

    // the rate at which this entity is rotating (only in pitch and yaw)
    pub angular_velocity: Vector3<Deg<f32>>,

    // the temporary angle modifier applied by damage and recoil
    pub punch_angle: Vector3<Deg<f32>>,

    // entity class name
    pub class_name: StringId,

    // name of alias model (MDL) associated with this entity
    pub model_name: StringId,

    // animation frame in the alias model
    pub frame_id: f32,

    // skin index in the alias model
    pub skin_id: f32,

    // model effects
    pub effects: EntityEffects,

    // minimum extent of entity relative to origin
    pub mins: Vector3<f32>,

    // maximum extent of entity relative to origin
    pub maxs: Vector3<f32>,

    // dimensions of this entity (maxs - mins)
    pub size: Vector3<f32>,

    // function to call when another entity collides with this one
    pub touch_fnc: FunctionId,

    // function to call when +use is issued on this entity
    pub use_fnc: FunctionId,

    // function to call when next_think elapses
    pub think_fnc: FunctionId,

    // function to call when this entity is blocked from movement
    pub blocked_fnc: FunctionId,

    // time remaining until next think
    pub next_think: Duration,

    // TODO: ???
    pub ground_entity: EntityId,

    // current health
    pub health: f32,

    // current kill count (multiplayer)
    pub frags: f32,

    // equipped weapon (bitflags)
    pub weapon: f32,

    // alias model for the equipped weapon
    pub weapon_model_name: StringId,

    // animation frame for the weapon model
    pub weapon_frame: f32,

    // ammo for current weapon
    pub current_ammo: f32,

    // shotgun ammo remaining
    pub ammo_shells: f32,

    // nailgun ammo remaining
    pub ammo_nails: f32,

    // rockets remaining
    pub ammo_rockets: f32,

    // energy cells remaining (for lightning gun)
    pub ammo_cells: f32,

    // bitflags representing what items player has
    pub items: f32,

    // can this entity be damaged?
    pub take_damage: f32,

    // next entity in a chained list
    pub chain: EntityId,

    // is this entity dead?
    pub dead_flag: f32,

    // position of camera relative to origin
    pub view_offset: Vector3<f32>,

    // +fire
    pub button_0: f32,

    // +use
    pub button_1: f32,

    // +jump
    pub button_2: f32,

    // TODO: document impulse
    pub impulse: f32,

    // TODO: something to do with updating player angle
    pub fix_angle: f32,

    // player view angle
    pub view_angle: Vector3<Deg<f32>>,

    // calculated default view angle
    pub ideal_pitch: Deg<f32>,

    // screen name
    pub net_name: StringId,

    // this entity's enemy (for monsters)
    pub enemy: EntityId,

    // various state flags
    pub flags: EntityFlags,

    // player colors in multiplayer
    pub colormap: f32,

    // team number in multiplayer
    pub team: f32,

    // maximum player health
    pub max_health: f32,

    // time player last teleported
    pub teleport_time: Duration,

    // percentage of incoming damage blocked (between 0 and 1)
    pub armor_strength: f32,

    // armor points remaining
    pub armor_value: f32,

    // how submerged this entity is, 0 (none) -> 3 (full)
    pub water_level: f32,

    // one of the CONTENTS_* constants (bspfile.h)
    pub contents: f32,

    // ideal pathfinding direction (for monsters)
    pub ideal_yaw: Deg<f32>,

    // turn rate
    pub yaw_speed: Deg<f32>,

    // TODO: maybe entity being aimed at?
    pub aim_entity: EntityId,

    // monster's goal entity
    pub goal_entity: EntityId,

    // meaning differs based on classname
    pub spawn_flags: f32,

    // target_name of the entity to activate
    pub target: StringId,

    // this entity's activation name
    pub target_name: StringId,

    // damage accumulator
    pub dmg_take: f32,

    // damage block accumulator?
    pub dmg_save: f32,

    // which entity inflicted damage
    pub dmg_inflictor: EntityId,

    // entity that owns this entity
    pub owner: EntityId,

    // which direction this entity should move
    pub move_direction: Vector3<f32>,

    // message to display on entity trigger
    pub message: StringId,

    // sound ID
    pub sounds: f32,

    // sounds played on noise channels
    pub noise_0: StringId,
    pub noise_1: StringId,
    pub noise_2: StringId,
    pub noise_3: StringId,
}

impl GenericEntityStatics {
    pub fn get_float(&self, addr: usize) -> Result<f32, ProgsError> {
        Ok(match float_addr(addr)? {
            FieldAddrFloat::ModelIndex => self.model_index,
            FieldAddrFloat::AbsMinX => self.abs_min[0],
            FieldAddrFloat::AbsMinY => self.abs_min[1],
            FieldAddrFloat::AbsMinZ => self.abs_min[2],
            FieldAddrFloat::AbsMaxX => self.abs_max[0],
            FieldAddrFloat::AbsMaxY => self.abs_max[1],
            FieldAddrFloat::AbsMaxZ => self.abs_max[2],
            FieldAddrFloat::LocalTime => engine::duration_to_f32(self.local_time),
            FieldAddrFloat::MoveType => self.move_type as u32 as f32,
            FieldAddrFloat::Solid => self.solid,
            FieldAddrFloat::OriginX => self.origin[0],
            FieldAddrFloat::OriginY => self.origin[1],
            FieldAddrFloat::OriginZ => self.origin[2],
            FieldAddrFloat::OldOriginX => self.old_origin[0],
            FieldAddrFloat::OldOriginY => self.old_origin[1],
            FieldAddrFloat::OldOriginZ => self.old_origin[2],
            FieldAddrFloat::VelocityX => self.velocity[0],
            FieldAddrFloat::VelocityY => self.velocity[1],
            FieldAddrFloat::VelocityZ => self.velocity[2],
            FieldAddrFloat::AnglesX => self.angles[0].0,
            FieldAddrFloat::AnglesY => self.angles[1].0,
            FieldAddrFloat::AnglesZ => self.angles[2].0,
            FieldAddrFloat::AngularVelocityX => self.angular_velocity[0].0,
            FieldAddrFloat::AngularVelocityY => self.angular_velocity[1].0,
            FieldAddrFloat::AngularVelocityZ => self.angular_velocity[2].0,
            FieldAddrFloat::PunchAngleX => self.punch_angle[0].0,
            FieldAddrFloat::PunchAngleY => self.punch_angle[1].0,
            FieldAddrFloat::PunchAngleZ => self.punch_angle[2].0,
            FieldAddrFloat::FrameId => self.frame_id,
            FieldAddrFloat::SkinId => self.skin_id,
            FieldAddrFloat::Effects => self.effects.bits() as i32 as f32,
            FieldAddrFloat::MinsX => self.mins[0],
            FieldAddrFloat::MinsY => self.mins[1],
            FieldAddrFloat::MinsZ => self.mins[2],
            FieldAddrFloat::MaxsX => self.maxs[0],
            FieldAddrFloat::MaxsY => self.maxs[1],
            FieldAddrFloat::MaxsZ => self.maxs[2],
            FieldAddrFloat::SizeX => self.size[0],
            FieldAddrFloat::SizeY => self.size[1],
            FieldAddrFloat::SizeZ => self.size[2],
            FieldAddrFloat::NextThink => engine::duration_to_f32(self.next_think),
            FieldAddrFloat::Health => self.health,
            FieldAddrFloat::Frags => self.frags,
            FieldAddrFloat::Weapon => self.weapon,
            FieldAddrFloat::WeaponFrame => self.weapon_frame,
            FieldAddrFloat::CurrentAmmo => self.current_ammo,
            FieldAddrFloat::AmmoShells => self.ammo_shells,
            FieldAddrFloat::AmmoNails => self.ammo_nails,
            FieldAddrFloat::AmmoRockets => self.ammo_rockets,
            FieldAddrFloat::AmmoCells => self.ammo_cells,
            FieldAddrFloat::Items => self.items,
            FieldAddrFloat::TakeDamage => self.take_damage,
            FieldAddrFloat::DeadFlag => self.dead_flag,
            FieldAddrFloat::ViewOffsetX => self.view_offset[0],
            FieldAddrFloat::ViewOffsetY => self.view_offset[1],
            FieldAddrFloat::ViewOffsetZ => self.view_offset[2],
            FieldAddrFloat::Button0 => self.button_0,
            FieldAddrFloat::Button1 => self.button_1,
            FieldAddrFloat::Button2 => self.button_2,
            FieldAddrFloat::Impulse => self.impulse,
            FieldAddrFloat::FixAngle => self.fix_angle,
            FieldAddrFloat::ViewAngleX => self.view_angle[0].0,
            FieldAddrFloat::ViewAngleY => self.view_angle[1].0,
            FieldAddrFloat::ViewAngleZ => self.view_angle[2].0,
            FieldAddrFloat::IdealPitch => self.ideal_pitch.0,
            FieldAddrFloat::Flags => self.flags.bits() as i32 as f32,
            FieldAddrFloat::Colormap => self.colormap,
            FieldAddrFloat::Team => self.team,
            FieldAddrFloat::MaxHealth => self.max_health,
            FieldAddrFloat::TeleportTime => engine::duration_to_f32(self.teleport_time),
            FieldAddrFloat::ArmorStrength => self.armor_strength,
            FieldAddrFloat::ArmorValue => self.armor_value,
            FieldAddrFloat::WaterLevel => self.water_level,
            FieldAddrFloat::Contents => self.contents,
            FieldAddrFloat::IdealYaw => self.ideal_yaw.0,
            FieldAddrFloat::YawSpeed => self.yaw_speed.0,
            FieldAddrFloat::SpawnFlags => self.spawn_flags,
            FieldAddrFloat::DmgTake => self.dmg_take,
            FieldAddrFloat::DmgSave => self.dmg_save,
            FieldAddrFloat::MoveDirectionX => self.move_direction[0],
            FieldAddrFloat::MoveDirectionY => self.move_direction[1],
            FieldAddrFloat::MoveDirectionZ => self.move_direction[2],
            FieldAddrFloat::Sounds => self.sounds,
        })
    }

    pub fn put_float(&mut self, val: f32, addr: usize) -> Result<(), ProgsError> {
        match float_addr(addr)? {
            FieldAddrFloat::ModelIndex => self.model_index = val,
            FieldAddrFloat::AbsMinX => self.abs_min[0] = val,
            FieldAddrFloat::AbsMinY => self.abs_min[1] = val,
            FieldAddrFloat::AbsMinZ => self.abs_min[2] = val,
            FieldAddrFloat::AbsMaxX => self.abs_max[0] = val,
            FieldAddrFloat::AbsMaxY => self.abs_max[1] = val,
            FieldAddrFloat::AbsMaxZ => self.abs_max[2] = val,
            FieldAddrFloat::LocalTime => self.local_time = engine::duration_from_f32(val),
            FieldAddrFloat::MoveType => self.move_type = MoveType::from_u32(val as u32).unwrap(),
            FieldAddrFloat::Solid => self.solid = val,
            FieldAddrFloat::OriginX => self.origin[0] = val,
            FieldAddrFloat::OriginY => self.origin[1] = val,
            FieldAddrFloat::OriginZ => self.origin[2] = val,
            FieldAddrFloat::OldOriginX => self.old_origin[0] = val,
            FieldAddrFloat::OldOriginY => self.old_origin[1] = val,
            FieldAddrFloat::OldOriginZ => self.old_origin[2] = val,
            FieldAddrFloat::VelocityX => self.velocity[0] = val,
            FieldAddrFloat::VelocityY => self.velocity[1] = val,
            FieldAddrFloat::VelocityZ => self.velocity[2] = val,
            FieldAddrFloat::AnglesX => self.angles[0] = Deg(val),
            FieldAddrFloat::AnglesY => self.angles[1] = Deg(val),
            FieldAddrFloat::AnglesZ => self.angles[2] = Deg(val),
            FieldAddrFloat::AngularVelocityX => self.angular_velocity[0] = Deg(val),
            FieldAddrFloat::AngularVelocityY => self.angular_velocity[1] = Deg(val),
            FieldAddrFloat::AngularVelocityZ => self.angular_velocity[2] = Deg(val),
            FieldAddrFloat::PunchAngleX => self.punch_angle[0] = Deg(val),
            FieldAddrFloat::PunchAngleY => self.punch_angle[1] = Deg(val),
            FieldAddrFloat::PunchAngleZ => self.punch_angle[2] = Deg(val),
            FieldAddrFloat::FrameId => self.frame_id = val,
            FieldAddrFloat::SkinId => self.skin_id = val,
            FieldAddrFloat::Effects => self.effects = EntityEffects::from_bits(val as u16).unwrap(),
            FieldAddrFloat::MinsX => self.mins[0] = val,
            FieldAddrFloat::MinsY => self.mins[1] = val,
            FieldAddrFloat::MinsZ => self.mins[2] = val,
            FieldAddrFloat::MaxsX => self.maxs[0] = val,
            FieldAddrFloat::MaxsY => self.maxs[1] = val,
            FieldAddrFloat::MaxsZ => self.maxs[2] = val,
            FieldAddrFloat::SizeX => self.size[0] = val,
            FieldAddrFloat::SizeY => self.size[1] = val,
            FieldAddrFloat::SizeZ => self.size[2] = val,
            FieldAddrFloat::NextThink => self.next_think = engine::duration_from_f32(val),
            FieldAddrFloat::Health => self.health = val,
            FieldAddrFloat::Frags => self.frags = val,
            FieldAddrFloat::Weapon => self.weapon = val,
            FieldAddrFloat::WeaponFrame => self.weapon_frame = val,
            FieldAddrFloat::CurrentAmmo => self.current_ammo = val,
            FieldAddrFloat::AmmoShells => self.ammo_shells = val,
            FieldAddrFloat::AmmoNails => self.ammo_nails = val,
            FieldAddrFloat::AmmoRockets => self.ammo_rockets = val,
            FieldAddrFloat::AmmoCells => self.ammo_cells = val,
            FieldAddrFloat::Items => self.items = val,
            FieldAddrFloat::TakeDamage => self.take_damage = val,
            FieldAddrFloat::DeadFlag => self.dead_flag = val,
            FieldAddrFloat::ViewOffsetX => self.view_offset[0] = val,
            FieldAddrFloat::ViewOffsetY => self.view_offset[1] = val,
            FieldAddrFloat::ViewOffsetZ => self.view_offset[2] = val,
            FieldAddrFloat::Button0 => self.button_0 = val,
            FieldAddrFloat::Button1 => self.button_1 = val,
            FieldAddrFloat::Button2 => self.button_2 = val,
            FieldAddrFloat::Impulse => self.impulse = val,
            FieldAddrFloat::FixAngle => self.fix_angle = val,
            FieldAddrFloat::ViewAngleX => self.view_angle[0] = Deg(val),
            FieldAddrFloat::ViewAngleY => self.view_angle[1] = Deg(val),
            FieldAddrFloat::ViewAngleZ => self.view_angle[2] = Deg(val),
            FieldAddrFloat::IdealPitch => self.ideal_pitch = Deg(val),
            FieldAddrFloat::Flags => {
                self.flags = match EntityFlags::from_bits(val as u16) {
                    Some(f) => f,
                    None => {
                        warn!(
                            "invalid entity flags ({:b}), converting to none",
                            val as u16
                        );
                        EntityFlags::empty()
                    }
                }
            }
            FieldAddrFloat::Colormap => self.colormap = val,
            FieldAddrFloat::Team => self.team = val,
            FieldAddrFloat::MaxHealth => self.max_health = val,
            FieldAddrFloat::TeleportTime => self.teleport_time = engine::duration_from_f32(val),
            FieldAddrFloat::ArmorStrength => self.armor_strength = val,
            FieldAddrFloat::ArmorValue => self.armor_value = val,
            FieldAddrFloat::WaterLevel => self.water_level = val,
            FieldAddrFloat::Contents => self.contents = val,
            FieldAddrFloat::IdealYaw => self.ideal_yaw = Deg(val),
            FieldAddrFloat::YawSpeed => self.yaw_speed = Deg(val),
            FieldAddrFloat::SpawnFlags => self.spawn_flags = val,
            FieldAddrFloat::DmgTake => self.dmg_take = val,
            FieldAddrFloat::DmgSave => self.dmg_save = val,
            FieldAddrFloat::MoveDirectionX => self.move_direction[0] = val,
            FieldAddrFloat::MoveDirectionY => self.move_direction[1] = val,
            FieldAddrFloat::MoveDirectionZ => self.move_direction[2] = val,
            FieldAddrFloat::Sounds => self.sounds = val,
        }

        Ok(())
    }

    pub fn get_vector(&self, addr: usize) -> Result<[f32; 3], ProgsError> {
        let v_addr = match FieldAddrVector::from_usize(addr) {
            Some(v) => v,
            None => {
                return Err(ProgsError::with_msg(
                    format!("get_vector_static: invalid address ({})", addr),
                ));
            }
        };

        Ok(match v_addr {
            FieldAddrVector::AbsMin => self.abs_min.into(),
            FieldAddrVector::AbsMax => self.abs_max.into(),
            FieldAddrVector::Origin => self.origin.into(),
            FieldAddrVector::OldOrigin => self.old_origin.into(),
            FieldAddrVector::Velocity => self.velocity.into(),
            FieldAddrVector::Angles => engine::deg_vector_to_f32_vector(self.angles).into(),
            FieldAddrVector::AngularVelocity => {
                engine::deg_vector_to_f32_vector(self.angular_velocity).into()
            }
            FieldAddrVector::PunchAngle => {
                engine::deg_vector_to_f32_vector(self.punch_angle).into()
            }
            FieldAddrVector::Mins => self.mins.into(),
            FieldAddrVector::Maxs => self.maxs.into(),
            FieldAddrVector::Size => self.size.into(),
            FieldAddrVector::ViewOffset => self.view_offset.into(),
            FieldAddrVector::ViewAngle => engine::deg_vector_to_f32_vector(self.view_angle).into(),
            FieldAddrVector::MoveDirection => self.move_direction.into(),
        })
    }

    pub fn put_vector(&mut self, val: [f32; 3], addr: usize) -> Result<(), ProgsError> {
        let v_addr = match FieldAddrVector::from_usize(addr) {
            Some(v) => v,
            None => {
                return Err(ProgsError::with_msg(
                    format!("put_vector_static: invalid address ({})", addr),
                ));
            }
        };

        Ok(match v_addr {
            FieldAddrVector::AbsMin => self.abs_min = Vector3::from(val),
            FieldAddrVector::AbsMax => self.abs_max = Vector3::from(val),
            FieldAddrVector::Origin => self.origin = Vector3::from(val),
            FieldAddrVector::OldOrigin => self.old_origin = Vector3::from(val),
            FieldAddrVector::Velocity => self.velocity = Vector3::from(val),
            FieldAddrVector::Angles => {
                self.angles = engine::deg_vector_from_f32_vector(Vector3::from(val))
            }
            FieldAddrVector::AngularVelocity => {
                self.angular_velocity = engine::deg_vector_from_f32_vector(Vector3::from(val))
            }
            FieldAddrVector::PunchAngle => {
                self.punch_angle = engine::deg_vector_from_f32_vector(Vector3::from(val))
            }
            FieldAddrVector::Mins => self.mins = Vector3::from(val),
            FieldAddrVector::Maxs => self.maxs = Vector3::from(val),
            FieldAddrVector::Size => self.size = Vector3::from(val),
            FieldAddrVector::ViewOffset => self.view_offset = Vector3::from(val),
            FieldAddrVector::ViewAngle => {
                self.view_angle = engine::deg_vector_from_f32_vector(Vector3::from(val))
            }
            FieldAddrVector::MoveDirection => self.move_direction = Vector3::from(val),
        })
    }

    pub fn get_string_id(&self, addr: usize) -> Result<StringId, ProgsError> {
        let s_addr = match FieldAddrStringId::from_usize(addr) {
            Some(s) => s,
            None => {
                return Err(ProgsError::with_msg(
                    format!("get_string_id_static: invalid address ({})", addr),
                ));
            }
        };

        Ok(match s_addr {
            FieldAddrStringId::ClassName => self.class_name,
            FieldAddrStringId::ModelName => self.model_name,
            FieldAddrStringId::WeaponModelName => self.weapon_model_name,
            FieldAddrStringId::NetName => self.net_name,
            FieldAddrStringId::Target => self.target,
            FieldAddrStringId::TargetName => self.target_name,
            FieldAddrStringId::Message => self.message,
            FieldAddrStringId::Noise0Name => self.noise_0,
            FieldAddrStringId::Noise1Name => self.noise_1,
            FieldAddrStringId::Noise2Name => self.noise_2,
            FieldAddrStringId::Noise3Name => self.noise_3,
        })
    }

    pub fn put_string_id(&mut self, val: StringId, addr: usize) -> Result<(), ProgsError> {
        let s_addr = match FieldAddrStringId::from_usize(addr) {
            Some(s) => s,
            None => {
                return Err(ProgsError::with_msg(
                    format!("put_string_id_static: invalid address ({})", addr),
                ));
            }
        };

        Ok(match s_addr {
            FieldAddrStringId::ClassName => self.class_name = val,
            FieldAddrStringId::ModelName => self.model_name = val,
            FieldAddrStringId::WeaponModelName => self.weapon_model_name = val,
            FieldAddrStringId::NetName => self.net_name = val,
            FieldAddrStringId::Target => self.target = val,
            FieldAddrStringId::TargetName => self.target_name = val,
            FieldAddrStringId::Message => self.message = val,
            FieldAddrStringId::Noise0Name => self.noise_0 = val,
            FieldAddrStringId::Noise1Name => self.noise_1 = val,
            FieldAddrStringId::Noise2Name => self.noise_2 = val,
            FieldAddrStringId::Noise3Name => self.noise_3 = val,
        })
    }

    pub fn get_entity_id(&self, addr: usize) -> Result<EntityId, ProgsError> {
        let e_addr = match FieldAddrEntityId::from_usize(addr) {
            Some(e) => e,
            None => {
                return Err(ProgsError::with_msg(
                    format!("get_entity_id_static: invalid address ({})", addr),
                ));
            }
        };

        Ok(match e_addr {
            FieldAddrEntityId::Ground => self.ground_entity,
            FieldAddrEntityId::Chain => self.chain,
            FieldAddrEntityId::Enemy => self.enemy,
            FieldAddrEntityId::Aim => self.aim_entity,
            FieldAddrEntityId::Goal => self.goal_entity,
            FieldAddrEntityId::DmgInflictor => self.dmg_inflictor,
            FieldAddrEntityId::Owner => self.owner,
        })
    }

    pub fn put_entity_id(&mut self, val: EntityId, addr: usize) -> Result<(), ProgsError> {
        let e_addr = match FieldAddrEntityId::from_usize(addr) {
            Some(e) => e,
            None => {
                return Err(ProgsError::with_msg(
                    format!("put_entity_id_static: invalid address ({})", addr),
                ));
            }
        };

        Ok(match e_addr {
            FieldAddrEntityId::Ground => self.ground_entity = val,
            FieldAddrEntityId::Chain => self.chain = val,
            FieldAddrEntityId::Enemy => self.enemy = val,
            FieldAddrEntityId::Aim => self.aim_entity = val,
            FieldAddrEntityId::Goal => self.goal_entity = val,
            FieldAddrEntityId::DmgInflictor => self.dmg_inflictor = val,
            FieldAddrEntityId::Owner => self.owner = val,
        })
    }

    pub fn get_function_id(&self, addr: usize) -> Result<FunctionId, ProgsError> {
        let s_addr = match FieldAddrFunctionId::from_usize(addr) {
            Some(s) => s,
            None => {
                return Err(ProgsError::with_msg(format!(
                    "get_function_id_static: invalid address ({})",
                    addr
                )));
            }
        };

        Ok(match s_addr {
            FieldAddrFunctionId::Touch => self.touch_fnc,
            FieldAddrFunctionId::Use => self.use_fnc,
            FieldAddrFunctionId::Think => self.think_fnc,
            FieldAddrFunctionId::Blocked => self.blocked_fnc,
        })
    }

    pub fn put_function_id(&mut self, val: FunctionId, addr: usize) -> Result<(), ProgsError> {
        let s_addr = match FieldAddrFunctionId::from_usize(addr) {
            Some(s) => s,
            None => {
                return Err(ProgsError::with_msg(format!(
                    "put_function_id_static: invalid address ({})",
                    addr
                )));
            }
        };

        Ok(match s_addr {
            FieldAddrFunctionId::Touch => self.touch_fnc = val,
            FieldAddrFunctionId::Use => self.use_fnc = val,
            FieldAddrFunctionId::Think => self.think_fnc = val,
            FieldAddrFunctionId::Blocked => self.blocked_fnc = val,
        })
    }
}

impl Default for GenericEntityStatics {
    fn default() -> Self {
        GenericEntityStatics {
            model_index: 0.0,
            abs_min: Vector3::zero(),
            abs_max: Vector3::zero(),
            local_time: Duration::seconds(0),
            move_type: MoveType::None,
            solid: 0.0,
            origin: Vector3::zero(),
            old_origin: Vector3::zero(),
            velocity: Vector3::zero(),
            angles: Vector3::new(Deg(0.0), Deg(0.0), Deg(0.0)),
            angular_velocity: Vector3::new(Deg(0.0), Deg(0.0), Deg(0.0)),
            punch_angle: Vector3::new(Deg(0.0), Deg(0.0), Deg(0.0)),
            class_name: StringId::new(),
            model_name: StringId::new(),
            frame_id: 0.0,
            skin_id: 0.0,
            effects: EntityEffects::empty(),
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
            weapon_model_name: StringId::new(),
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
            net_name: StringId::new(),
            enemy: EntityId(0),
            flags: EntityFlags::empty(),
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
            target: StringId::new(),
            target_name: StringId::new(),
            dmg_take: 0.0,
            dmg_save: 0.0,
            dmg_inflictor: EntityId(0),
            owner: EntityId(0),
            move_direction: Vector3::zero(),
            message: StringId::new(),
            sounds: 0.0,
            noise_0: StringId::new(),
            noise_1: StringId::new(),
            noise_2: StringId::new(),
            noise_3: StringId::new(),
        }
    }
}
