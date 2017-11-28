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

use std::ops::Index;

use engine;
use progs::EntityId;
use progs::FunctionId;
use progs::ProgsError;
use progs::StringId;

use byteorder::LittleEndian;
use byteorder::ReadBytesExt;
use cgmath::Deg;
use cgmath::Vector3;
use cgmath::Zero;
use chrono::Duration;
use num::FromPrimitive;

const MAX_ENTITIES: usize = 600;
const MAX_ENT_LEAVES: usize = 16;

// dynamic entity fields start after this point (i.e. defined in progs.dat, not accessible here)
const ADDR_DYNAMIC_START: usize = 105;
const STATIC_ADDRESS_COUNT: usize = 105;

#[derive(FromPrimitive)]
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

#[derive(FromPrimitive)]
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

#[derive(FromPrimitive)]
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

#[derive(FromPrimitive)]
pub enum FieldAddrEntityId {
    Ground = 47,
    Chain = 60,
    Enemy = 75,
    Aim = 87,
    Goal = 88,
    DamageInflictor = 94,
    Owner = 95,
}

#[derive(FromPrimitive)]
pub enum FieldAddrFunctionId {
    Touch = 42,
    Use = 43,
    Think = 44,
    Blocked = 45,
}

#[derive(Copy, Clone)]
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

pub struct EntityStatic {
    // index in the model list
    model_index: f32,

    // absolute minimum extent of entity
    abs_min: Vector3<f32>,

    // absolute maximum extent of entity
    abs_max: Vector3<f32>,

    // how far in time this entity has been processed
    local_time: Duration,

    // TODO find definitions for movement types
    move_type: MoveType,

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

    // model effects
    effects: EntityEffects,

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

    // various state flags
    flags: EntityFlags,

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
}

impl Default for EntityStatic {
    fn default() -> Self {
        EntityStatic {
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
            class_name: StringId(0),
            model_name: StringId(0),
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
        }
    }
}

pub struct EntityState {
    origin: Vector3<f32>,
    angles: Vector3<Deg<f32>>,
    model_id: usize,
    frame_id: usize,

    // TODO: more specific types for these
    colormap: i32,
    skin: i32,
    effects: i32,
}

pub struct Entity {
    // TODO: figure out how to link entities into the world
    // link: SomeType,
    leaf_count: usize,
    leaf_ids: [u16; MAX_ENT_LEAVES],
    baseline: EntityState,
    statics: EntityStatic,
    dynamics: Vec<[u8; 4]>,
}

impl Entity {
    pub fn get_float(&self, addr: i16) -> Result<f32, ProgsError> {
        if addr < 0 {
            panic!("negative offset");
        }

        let addr = addr as usize;

        if addr >= ADDR_DYNAMIC_START + self.dynamics.len() {
            println!("out-of-bounds offset ({})", addr);
            return Ok(0.0);
        }

        if addr < ADDR_DYNAMIC_START {
            self.get_float_static(addr)
        } else {
            self.get_float_dynamic(addr)
        }
    }

    fn get_float_static(&self, addr: usize) -> Result<f32, ProgsError> {
        if addr >= ADDR_DYNAMIC_START {
            panic!("Invalid offset for static entity field");
        }

        let f_addr = match FieldAddrFloat::from_usize(addr) {
            Some(f) => f,
            None => {
                return Err(ProgsError::with_msg(
                    format!("get_float_static: invalid address ({})", addr),
                ))
            }
        };

        Ok(match f_addr {
            FieldAddrFloat::ModelIndex => self.statics.model_index,
            FieldAddrFloat::AbsMinX => self.statics.abs_min[0],
            FieldAddrFloat::AbsMinY => self.statics.abs_min[1],
            FieldAddrFloat::AbsMinZ => self.statics.abs_min[2],
            FieldAddrFloat::AbsMaxX => self.statics.abs_max[0],
            FieldAddrFloat::AbsMaxY => self.statics.abs_max[1],
            FieldAddrFloat::AbsMaxZ => self.statics.abs_max[2],
            FieldAddrFloat::LocalTime => engine::duration_to_f32(self.statics.local_time),
            FieldAddrFloat::MoveType => self.statics.move_type as u32 as f32,
            FieldAddrFloat::Solid => self.statics.solid,
            FieldAddrFloat::OriginX => self.statics.origin[0],
            FieldAddrFloat::OriginY => self.statics.origin[1],
            FieldAddrFloat::OriginZ => self.statics.origin[2],
            FieldAddrFloat::OldOriginX => self.statics.old_origin[0],
            FieldAddrFloat::OldOriginY => self.statics.old_origin[1],
            FieldAddrFloat::OldOriginZ => self.statics.old_origin[2],
            FieldAddrFloat::VelocityX => self.statics.velocity[0],
            FieldAddrFloat::VelocityY => self.statics.velocity[1],
            FieldAddrFloat::VelocityZ => self.statics.velocity[2],
            FieldAddrFloat::AnglesX => self.statics.angles[0].0,
            FieldAddrFloat::AnglesY => self.statics.angles[1].0,
            FieldAddrFloat::AnglesZ => self.statics.angles[2].0,
            FieldAddrFloat::AngularVelocityX => self.statics.angular_velocity[0].0,
            FieldAddrFloat::AngularVelocityY => self.statics.angular_velocity[1].0,
            FieldAddrFloat::AngularVelocityZ => self.statics.angular_velocity[2].0,
            FieldAddrFloat::PunchAngleX => self.statics.punch_angle[0].0,
            FieldAddrFloat::PunchAngleY => self.statics.punch_angle[1].0,
            FieldAddrFloat::PunchAngleZ => self.statics.punch_angle[2].0,
            FieldAddrFloat::FrameId => self.statics.frame_id,
            FieldAddrFloat::SkinId => self.statics.skin_id,
            FieldAddrFloat::Effects => self.statics.effects.bits() as i32 as f32,
            FieldAddrFloat::MinsX => self.statics.mins[0],
            FieldAddrFloat::MinsY => self.statics.mins[1],
            FieldAddrFloat::MinsZ => self.statics.mins[2],
            FieldAddrFloat::MaxsX => self.statics.maxs[0],
            FieldAddrFloat::MaxsY => self.statics.maxs[1],
            FieldAddrFloat::MaxsZ => self.statics.maxs[2],
            FieldAddrFloat::SizeX => self.statics.size[0],
            FieldAddrFloat::SizeY => self.statics.size[1],
            FieldAddrFloat::SizeZ => self.statics.size[2],
            FieldAddrFloat::NextThink => engine::duration_to_f32(self.statics.next_think),
            FieldAddrFloat::Health => self.statics.health,
            FieldAddrFloat::Frags => self.statics.frags,
            FieldAddrFloat::Weapon => self.statics.weapon,
            FieldAddrFloat::WeaponFrame => self.statics.weapon_frame,
            FieldAddrFloat::CurrentAmmo => self.statics.current_ammo,
            FieldAddrFloat::AmmoShells => self.statics.ammo_shells,
            FieldAddrFloat::AmmoNails => self.statics.ammo_nails,
            FieldAddrFloat::AmmoRockets => self.statics.ammo_rockets,
            FieldAddrFloat::AmmoCells => self.statics.ammo_cells,
            FieldAddrFloat::Items => self.statics.items,
            FieldAddrFloat::TakeDamage => self.statics.take_damage,
            FieldAddrFloat::DeadFlag => self.statics.dead_flag,
            FieldAddrFloat::ViewOffsetX => self.statics.view_offset[0],
            FieldAddrFloat::ViewOffsetY => self.statics.view_offset[1],
            FieldAddrFloat::ViewOffsetZ => self.statics.view_offset[2],
            FieldAddrFloat::Button0 => self.statics.button_0,
            FieldAddrFloat::Button1 => self.statics.button_1,
            FieldAddrFloat::Button2 => self.statics.button_2,
            FieldAddrFloat::Impulse => self.statics.impulse,
            FieldAddrFloat::FixAngle => self.statics.fix_angle,
            FieldAddrFloat::ViewAngleX => self.statics.view_angle[0].0,
            FieldAddrFloat::ViewAngleY => self.statics.view_angle[1].0,
            FieldAddrFloat::ViewAngleZ => self.statics.view_angle[2].0,
            FieldAddrFloat::IdealPitch => self.statics.ideal_pitch.0,
            FieldAddrFloat::Flags => self.statics.flags.bits() as i32 as f32,
            FieldAddrFloat::Colormap => self.statics.colormap,
            FieldAddrFloat::Team => self.statics.team,
            FieldAddrFloat::MaxHealth => self.statics.max_health,
            FieldAddrFloat::TeleportTime => engine::duration_to_f32(self.statics.teleport_time),
            FieldAddrFloat::ArmorStrength => self.statics.armor_strength,
            FieldAddrFloat::ArmorValue => self.statics.armor_value,
            FieldAddrFloat::WaterLevel => self.statics.water_level,
            FieldAddrFloat::Contents => self.statics.contents,
            FieldAddrFloat::IdealYaw => self.statics.ideal_yaw.0,
            FieldAddrFloat::YawSpeed => self.statics.yaw_speed.0,
            FieldAddrFloat::SpawnFlags => self.statics.spawn_flags,
            FieldAddrFloat::DmgTake => self.statics.dmg_take,
            FieldAddrFloat::DmgSave => self.statics.dmg_save,
            FieldAddrFloat::MoveDirectionX => self.statics.move_direction[0],
            FieldAddrFloat::MoveDirectionY => self.statics.move_direction[1],
            FieldAddrFloat::MoveDirectionZ => self.statics.move_direction[2],
            FieldAddrFloat::Sounds => self.statics.sounds,
        })
    }

    fn get_float_dynamic(&self, addr: usize) -> Result<f32, ProgsError> {
        Ok(
            self.dynamics[addr - ADDR_DYNAMIC_START]
                .as_ref()
                .read_f32::<LittleEndian>()
                .unwrap(),
        )
    }

    fn get_vector(&self, addr: i16) -> Result<[f32; 3], ProgsError> {
        if addr < 0 {
            panic!("negative offset");
        }

        let addr = addr as usize;

        // subtract 2 to account for size of vector
        if addr >= ADDR_DYNAMIC_START + self.dynamics.len() - 2 {
            println!("out-of-bounds offset ({})", addr);
            // TODO: proper error
            return Ok([0.0; 3]);
        }

        if addr < ADDR_DYNAMIC_START {
            self.get_vector_static(addr)
        } else {
            self.get_vector_dynamic(addr)
        }
    }

    fn get_vector_static(&self, addr: usize) -> Result<[f32; 3], ProgsError> {
        let v_addr = match FieldAddrVector::from_usize(addr) {
            Some(v) => v,
            None => {
                return Err(ProgsError::with_msg(
                    format!("get_vector_static: invalid address ({})", addr),
                ));
            }
        };

        Ok(match v_addr {
            FieldAddrVector::AbsMin => self.statics.abs_min.into(),
            FieldAddrVector::AbsMax => self.statics.abs_max.into(),
            FieldAddrVector::Origin => self.statics.origin.into(),
            FieldAddrVector::OldOrigin => self.statics.old_origin.into(),
            FieldAddrVector::Velocity => self.statics.velocity.into(),
            FieldAddrVector::Angles => engine::deg_vector_to_f32_vector(self.statics.angles).into(),
            FieldAddrVector::AngularVelocity => {
                engine::deg_vector_to_f32_vector(self.statics.angular_velocity).into()
            }
            FieldAddrVector::PunchAngle => {
                engine::deg_vector_to_f32_vector(self.statics.punch_angle).into()
            }
            FieldAddrVector::Mins => self.statics.mins.into(),
            FieldAddrVector::Maxs => self.statics.maxs.into(),
            FieldAddrVector::Size => self.statics.size.into(),
            FieldAddrVector::ViewOffset => self.statics.view_offset.into(),
            FieldAddrVector::ViewAngle => {
                engine::deg_vector_to_f32_vector(self.statics.view_angle).into()
            }
            FieldAddrVector::MoveDirection => self.statics.move_direction.into(),
        })
    }

    fn get_vector_dynamic(&self, addr: usize) -> Result<[f32; 3], ProgsError> {
        let mut v = [0.0; 3];

        for c in 0..v.len() {
            v[c] = self.get_float_dynamic(addr + c)?;
        }

        Ok(v)
    }
}

pub enum EntityListEntry {
    Free(Duration),
    NotFree(Entity),
}

pub struct EntityList {
    field_count: usize,
    entries: Box<[EntityListEntry]>,
}

impl EntityList {
    pub fn with_field_count(field_count: usize) -> EntityList {
        let mut entries = Vec::new();
        for _ in 0..MAX_ENTITIES {
            entries.push(EntityListEntry::Free(Duration::zero()));
        }
        let entries = entries.into_boxed_slice();

        EntityList {
            field_count,
            entries,
        }
    }

    pub fn alloc(&mut self) -> Result<EntityId, ProgsError> {
        for (i, entry) in self.entries.iter().enumerate() {
            if let &EntityListEntry::Free(_) = entry {
                return Ok(EntityId(i as i32));
            }
        }

        Err(ProgsError::with_msg("No entity slots available"))
    }

    pub fn free(&mut self, entity_id: usize) -> Result<(), ProgsError> {
        if entity_id > self.entries.len() {
            return Err(ProgsError::with_msg(
                format!("Invalid entity ID ({})", entity_id),
            ));
        }

        if let EntityListEntry::Free(_) = self.entries[entity_id] {
            return Ok(());
        }

        self.entries[entity_id] = EntityListEntry::Free(Duration::zero());
        Ok(())
    }

    pub fn try_get_entity_mut(&mut self, entity_id: usize) -> Result<&mut Entity, ProgsError> {
        if entity_id > self.entries.len() {
            return Err(ProgsError::with_msg(
                format!("Invalid entity ID ({})", entity_id),
            ));
        }

        match self.entries[entity_id] {
            EntityListEntry::Free(_) => Err(ProgsError::with_msg(
                format!("No entity at list entry {}", entity_id),
            )),
            EntityListEntry::NotFree(ref mut e) => Ok(e),
        }
    }

    pub fn try_get_entity(&self, entity_id: usize) -> Result<&Entity, ProgsError> {
        if entity_id > self.entries.len() {
            return Err(ProgsError::with_msg(
                format!("Invalid entity ID ({})", entity_id),
            ));
        }

        match self.entries[entity_id] {
            EntityListEntry::Free(_) => Err(ProgsError::with_msg(
                format!("No entity at list entry {}", entity_id),
            )),
            EntityListEntry::NotFree(ref e) => Ok(e),
        }
    }
}

impl Index<usize> for EntityList {
    type Output = EntityListEntry;

    fn index(&self, i: usize) -> &Self::Output {
        &self.entries[i]
    }
}
