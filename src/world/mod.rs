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

mod entity;

use std::collections::HashMap;
use std::collections::HashSet;
use std::rc::Rc;

use self::entity::Entity;
use self::entity::EntityFlags;
use self::entity::EntitySolid;
pub use self::entity::EntityTypeDef;
pub use self::entity::FieldAddrEntityId;
pub use self::entity::FieldAddrFloat;
pub use self::entity::FieldAddrFunctionId;
pub use self::entity::FieldAddrStringId;
pub use self::entity::FieldAddrVector;
use self::entity::STATIC_ADDRESS_COUNT;

use bsp;
use console::CvarRegistry;
use mdl;
use model::Model;
use pak::Pak;
use parse;
use progs::EntityFieldAddr;
use progs::EntityId;
use progs::ExecutionContext;
use progs::FieldDef;
use progs::GlobalAddrEntity;
use progs::Globals;
use progs::ProgsError;
use progs::StringId;
use progs::StringTable;
use progs::Type;
use server::Server;
use sprite;

use cgmath::Vector3;
use cgmath::Zero;

const AREA_DEPTH: usize = 4;

// 2 ^ (AREA_DEPTH + 1) - 1
// TODO: change this definition once constfn is a thing
const MAX_AREA_NODES: usize = 31;

const MAX_ENTITIES: usize = 600;

enum AreaNodeKind {
    Branch(AreaBranch),
    Leaf,
}

struct AreaNode {
    kind: AreaNodeKind,
    triggers: HashSet<EntityId>,
    solids: HashSet<EntityId>,
}

// Apologies in advance.
//                               00                                X
//               01                              02                Y
//       03              04              05              06        X
//   07      08      09      10      11      12      13      14    Y
// 15  16  17  18  19  20  21  22  23  24  25  26  27  28  29  30  Leaves
//
//   21    19    17    15
//   ||    ||    ||    ||
//   12=05=11    08=03=07
//   || || ||    || || ||
//   22 || 20    18 || 16
//      02====00====01
//   29 || 27    25 || 23
//   || || ||    || || ||
//   14=06=13    10=04=09
//   ||    ||    ||    ||
//   30    28    26    24
//
// The tree won't necessarily look like this, this just assumes a rectangular area with width
// between 1-2x its length.

impl AreaNode {
    /// Generate a breadth-first 2-D binary space partitioning tree with the given extents.
    pub fn generate(mins: Vector3<f32>, maxs: Vector3<f32>) -> Vec<AreaNode> {
        let mut nodes = Vec::with_capacity(2usize.pow(AREA_DEPTH as u32 + 1) - 1);

        // place internal nodes
        for i in 0..AREA_DEPTH {
            for _ in 0..2usize.pow(i as u32) {
                let len = nodes.len();
                nodes.push(AreaNode {
                    kind: AreaNodeKind::Branch(AreaBranch {
                        axis: AreaBranchAxis::X,
                        dist: 0.0,
                        front: 2 * len + 1,
                        back: 2 * len + 2,
                    }),
                    triggers: HashSet::new(),
                    solids: HashSet::new(),
                });
            }
        }

        // place leaves
        for _ in 0..2usize.pow(AREA_DEPTH as u32) {
            nodes.push(AreaNode {
                kind: AreaNodeKind::Leaf,
                triggers: HashSet::new(),
                solids: HashSet::new(),
            });
        }

        AreaNode::setup(&mut nodes, 0, mins, maxs);

        for (i, node) in nodes.iter().enumerate() {
            match node.kind {
                AreaNodeKind::Branch(ref b) => {
                    debug!(
                        "area node {}: axis = {:?} dist = {} front = {} back = {}",
                        i,
                        b.axis,
                        b.dist,
                        b.front,
                        b.back
                    );
                }
                AreaNodeKind::Leaf => debug!("area node {}: leaf", i),
            }
        }
        nodes
    }

    fn setup(nodes: &mut Vec<AreaNode>, index: usize, mins: Vector3<f32>, maxs: Vector3<f32>) {
        debug!(
            "node {: >2}: size = {:?} mins = {:?} maxs = {:?}",
            index,
            maxs - mins,
            mins,
            maxs
        );
        let size = maxs - mins;

        let axis;
        if size.x > size.y {
            axis = AreaBranchAxis::X;
        } else {
            axis = AreaBranchAxis::Y;
        }

        let dist = 0.5 * (maxs[axis as usize] + mins[axis as usize]);

        let mut front_mins = mins;
        front_mins[axis as usize] = dist;

        let mut back_maxs = maxs;
        back_maxs[axis as usize] = dist;

        let front;
        let back;
        match nodes[index].kind {
            AreaNodeKind::Branch(ref mut b) => {
                b.axis = axis;
                b.dist = dist;
                front = b.front;
                back = b.back;
            }
            AreaNodeKind::Leaf => return,
        }

        AreaNode::setup(nodes, front, front_mins, maxs);
        AreaNode::setup(nodes, back, mins, back_maxs);
    }
}

#[derive(Copy, Clone, Debug)]
enum AreaBranchAxis {
    X = 0,
    Y = 1,
}

struct AreaBranch {
    axis: AreaBranchAxis,
    dist: f32,
    front: usize,
    back: usize,
}

struct WorldEntity {
    entity: Entity,
    area_id: Option<usize>,
}

enum WorldEntitySlot {
    Vacant,
    Occupied(WorldEntity),
}

struct WorldEntities {
    string_table: Rc<StringTable>,
    type_def: EntityTypeDef,
    slots: Box<[WorldEntitySlot]>,
}

impl WorldEntities {
    pub fn new(type_def: EntityTypeDef, string_table: Rc<StringTable>) -> WorldEntities {
        let mut slots = Vec::with_capacity(MAX_ENTITIES);
        for _ in 0..MAX_ENTITIES {
            slots.push(WorldEntitySlot::Vacant);
        }

        WorldEntities {
            string_table,
            type_def,
            slots: slots.into_boxed_slice(),
        }
    }
}

pub struct World {
    string_table: Rc<StringTable>,

    area_nodes: Box<[AreaNode]>,

    type_def: EntityTypeDef,
    slots: Box<[WorldEntitySlot]>,

    models: Vec<Model>,
}

impl World {
    pub fn create(
        mut brush_models: Vec<Model>,
        type_def: EntityTypeDef,
        string_table: Rc<StringTable>,
    ) -> Result<World, ()> {
        // generate area tree for world model
        let area_nodes = AreaNode::generate(brush_models[0].min(), brush_models[0].max());

        // take ownership of all brush models
        let mut models = Vec::with_capacity(brush_models.len() + 1);
        models.push(Model::none());
        models.append(&mut brush_models);

        let mut slots = Vec::with_capacity(MAX_ENTITIES);
        for _ in 0..MAX_ENTITIES {
            slots.push(WorldEntitySlot::Vacant);
        }

        Ok(World {
            string_table,
            area_nodes: area_nodes.into_boxed_slice(),
            type_def,
            slots: slots.into_boxed_slice(),
            models,
        })
    }

    pub fn add_model(&mut self, pak: &Pak, name_id: StringId) -> Result<(), ProgsError> {
        let name = self.string_table.get(name_id).unwrap();

        if name.ends_with(".bsp") {
            let data = pak.open(name).unwrap();
            let (mut brush_models, _) = bsp::load(data).unwrap();
            if brush_models.len() > 1 {
                return Err(ProgsError::with_msg(
                    "Complex brush models must be loaded before world creation",
                ));
            }
            self.models.append(&mut brush_models);
            Ok(())
        } else if name.ends_with(".mdl") {
            let data = pak.open(&name).unwrap();
            let alias_model = mdl::load(data).unwrap();
            self.models.push(
                Model::from_alias_model(&name, alias_model),
            );
            Ok(())
        } else if name.ends_with(".spr") {
            let data = pak.open(&name).unwrap();
            let sprite_model = sprite::load(data);
            self.models.push(
                Model::from_sprite_model(&name, sprite_model),
            );
            Ok(())
        } else {
            return Err(ProgsError::with_msg(
                format!("Unrecognized model type: {}", name),
            ));
        }
    }

    fn find_def<S>(&self, name: S) -> Result<&FieldDef, ProgsError>
    where
        S: AsRef<str>,
    {
        let name = name.as_ref();
        let name_id = self.string_table.find(name).unwrap();

        match self.type_def.field_defs().iter().find(|def| {
            self.string_table.get(def.name_id).unwrap() == name
        }) {
            Some(d) => Ok(d),
            None => Err(ProgsError::with_msg(format!("no field with name {}", name))),
        }
    }

    /// Convert an entity ID and field address to an internal representation used by the VM.
    ///
    /// This representation should be compatible with the one used by the original Quake.
    pub fn ent_fld_addr_to_i32(&self, ent_fld_addr: EntityFieldAddr) -> i32 {
        let total_addr = (ent_fld_addr.entity_id * self.type_def.addr_count() +
                              ent_fld_addr.field_addr) * 4;

        if total_addr > ::std::i32::MAX as usize {
            panic!("ent_fld_addr_to_i32: total_addr overflow");
        }

        total_addr as i32
    }

    /// Convert the internal representation of a field offset back to struct form.
    pub fn ent_fld_addr_from_i32(&self, val: i32) -> EntityFieldAddr {
        if val < 0 {
            panic!("ent_fld_addr_from_i32: negative value ({})", val);
        }

        if val % 4 != 0 {
            panic!("ent_fld_addr_from_i32: value % 4 != 0 ({})", val);
        }

        let total_addr = val as usize / 4;
        EntityFieldAddr {
            entity_id: total_addr / self.type_def.addr_count(),
            field_addr: total_addr % self.type_def.addr_count(),
        }
    }

    fn gen_dynamics(&self) -> Vec<[u8; 4]> {
        let mut v = Vec::with_capacity(self.type_def.addr_count() - STATIC_ADDRESS_COUNT);
        for _ in 0..self.type_def.addr_count() {
            v.push([0; 4]);
        }
        v
    }

    fn find_vacant_slot(&self) -> Result<usize, ()> {
        for (i, slot) in self.slots.iter().enumerate() {
            if let &WorldEntitySlot::Vacant = slot {
                return Ok(i);
            }
        }

        panic!("no vacant slots");
    }

    pub fn alloc_uninitialized(&mut self) -> Result<EntityId, ProgsError> {
        let slot_id = self.find_vacant_slot().unwrap();

        self.slots[slot_id] = WorldEntitySlot::Occupied(WorldEntity {
            entity: Entity::new(self.string_table.clone(), self.gen_dynamics()),
            area_id: None,
        });

        Ok(EntityId(slot_id as i32))
    }

    /// Allocate a new entity and initialize it with the data in the given map.
    ///
    /// For each entry in `map`, this will locate a field definition for the entry key, parse the
    /// entry value to the correct type, and store it at that field. It will then locate the spawn
    /// method for the entity's `classname` and execute it.
    ///
    /// ## Special cases
    ///
    /// There are two cases where the keys do not directly correspond to entity fields:
    ///
    /// - `angle`: This allows QuakeEd to write a single value instead of a set of Euler angles.
    ///   The value should be interpreted as the second component of the `angles` field.
    /// - `light`: This is simply an alias for `light_lev`.
    pub fn alloc_from_map(&mut self, map: HashMap<&str, &str>) -> Result<EntityId, ProgsError> {
        let mut ent = Entity::new(self.string_table.clone(), self.gen_dynamics());

        for (key, val) in map.iter() {
            debug!(".{} = {}", key, val);
            match *key {
                // ignore keys starting with an underscore
                k if k.starts_with("_") => (),

                "angle" => {
                    // this is referred to in the original source as "anglehack" -- essentially,
                    // only the yaw (Y) value is given. see
                    // https://github.com/id-Software/Quake/blob/master/WinQuake/pr_edict.c#L826-L834
                    let def = self.find_def("angles")?.clone();
                    ent.put_vector(
                        [0.0, val.parse().unwrap(), 0.0],
                        def.offset as i16,
                    )?;
                }

                "light" => {
                    // more fun hacks brought to you by Carmack & Friends
                    let def = self.find_def("light_lev")?.clone();
                    ent.put_float(val.parse().unwrap(), def.offset as i16)?;
                }

                k => {
                    let def = self.find_def(k)?.clone();

                    match def.type_ {
                        // void has no value, skip it
                        Type::QVoid => (),

                        // TODO: figure out if this ever happens
                        Type::QPointer => unimplemented!(),

                        Type::QString => {
                            let s_id = self.string_table.insert(val);
                            ent.put_string_id(s_id, def.offset as i16)?;
                        }

                        Type::QFloat => ent.put_float(val.parse().unwrap(), def.offset as i16)?,
                        Type::QVector => {
                            ent.put_vector(
                                parse::vector3_components(val).unwrap(),
                                def.offset as i16,
                            )?
                        }
                        Type::QEntity => {
                            let id: usize = val.parse().unwrap();

                            if id > MAX_ENTITIES {
                                panic!("out-of-bounds entity access");
                            }

                            match self.slots[id] {
                                WorldEntitySlot::Vacant => panic!("no entity with id {}", id),
                                WorldEntitySlot::Occupied(_) => (),
                            }

                            ent.put_entity_id(EntityId(id as i32), def.offset as i16)?
                        }
                        Type::QField => panic!("attempted to store field of type Field in entity"),
                        Type::QFunction => {
                            // TODO: need to validate this against function table
                        }
                    }
                }
            }
        }

        let entry_id = self.find_vacant_slot().unwrap();

        self.slots[entry_id] = WorldEntitySlot::Occupied(WorldEntity {
            entity: ent,
            area_id: None,
        });

        Ok(EntityId(entry_id as i32))
    }

    pub fn free(&mut self, entity_id: EntityId) -> Result<(), ProgsError> {
        // TODO: unlink entity from world

        if entity_id.0 as usize > self.slots.len() {
            return Err(ProgsError::with_msg(
                format!("Invalid entity ID ({:?})", entity_id),
            ));
        }

        if let WorldEntitySlot::Vacant = self.slots[entity_id.0 as usize] {
            return Ok(());
        }

        self.slots[entity_id.0 as usize] = WorldEntitySlot::Vacant;
        Ok(())
    }

    pub fn try_get_entity(&self, entity_id: usize) -> Result<&Entity, ProgsError> {
        if entity_id > self.slots.len() {
            return Err(ProgsError::with_msg(
                format!("Invalid entity ID ({})", entity_id),
            ));
        }

        match self.slots[entity_id] {
            WorldEntitySlot::Vacant => Err(ProgsError::with_msg(
                format!("No entity at list entry {}", entity_id),
            )),
            WorldEntitySlot::Occupied(ref e) => Ok(&e.entity),
        }
    }

    pub fn try_get_entity_mut(&mut self, entity_id: usize) -> Result<&mut Entity, ProgsError> {
        if entity_id > self.slots.len() {
            return Err(ProgsError::with_msg(
                format!("Invalid entity ID ({})", entity_id),
            ));
        }

        match self.slots[entity_id] {
            WorldEntitySlot::Vacant => Err(ProgsError::with_msg(
                format!("No entity at list entry {}", entity_id),
            )),
            WorldEntitySlot::Occupied(ref mut e) => Ok(&mut e.entity),
        }
    }

    fn try_get_world_entity(&self, entity_id: usize) -> Result<&WorldEntity, ProgsError> {
        if entity_id > self.slots.len() {
            return Err(ProgsError::with_msg(
                format!("Invalid entity ID ({})", entity_id),
            ));
        }

        match self.slots[entity_id] {
            WorldEntitySlot::Vacant => Err(ProgsError::with_msg(
                format!("No entity at list entry {}", entity_id),
            )),
            WorldEntitySlot::Occupied(ref e) => Ok(e),
        }
    }

    fn try_get_world_entity_mut(
        &mut self,
        entity_id: usize,
    ) -> Result<&mut WorldEntity, ProgsError> {
        if entity_id > self.slots.len() {
            return Err(ProgsError::with_msg(
                format!("Invalid entity ID ({})", entity_id),
            ));
        }

        match self.slots[entity_id] {
            WorldEntitySlot::Vacant => Err(ProgsError::with_msg(
                format!("No entity at list entry {}", entity_id),
            )),
            WorldEntitySlot::Occupied(ref mut e) => Ok(e),
        }
    }

    pub fn spawn_entity(&mut self) -> Result<EntityId, ProgsError> {
        self.alloc_uninitialized()
        // TODO: link
    }

    pub fn spawn_entity_from_map(
        &mut self,
        execution_context: &mut ExecutionContext,
        globals: &mut Globals,
        cvars: &mut CvarRegistry,
        server: &mut Server,
        map: HashMap<&str, &str>,
        pak: &Pak,
    ) -> Result<EntityId, ProgsError> {
        let classname = match map.get("classname") {
            Some(c) => c.to_owned(),
            None => return Err(ProgsError::with_msg("No classname for entity")),
        };

        let e_id = self.alloc_from_map(map)?;

        // TODO: set origin, mins and maxs here if needed

        // set `self` before calling spawn function
        globals.put_entity_id(e_id, GlobalAddrEntity::Self_ as i16)?;

        execution_context.execute_program_by_name(
            globals,
            self,
            cvars,
            server,
            pak,
            classname,
        )?;

        // TODO: should touch triggers?
        self.link_entity(e_id, false)?;

        Ok(e_id)
    }

    fn unlink_entity(&mut self, e_id: EntityId) -> Result<(), ProgsError> {
        // if this entity has been removed or freed, do nothing
        if let WorldEntitySlot::Vacant = self.slots[e_id.0 as usize] {
            return Ok(());
        }

        let area_id = match self.try_get_world_entity(e_id.0 as usize)?.area_id {
            Some(i) => i,

            // entity not linked
            None => return Ok(()),
        };

        if self.area_nodes[area_id].triggers.remove(&e_id) {
            debug!("Unlinking entity {} from area triggers", e_id.0);
        } else if self.area_nodes[area_id].solids.remove(&e_id) {
            debug!("Unlinking entity {} from area solids", e_id.0);
        }

        self.try_get_world_entity_mut(e_id.0 as usize)?.area_id = None;

        Ok(())
    }

    // TODO: brush entities need to take their origin, mins and maxs fields from their models
    fn link_entity(&mut self, e_id: EntityId, touch_triggers: bool) -> Result<(), ProgsError> {
        // if this entity has been removed or freed, do nothing
        if let WorldEntitySlot::Vacant = self.slots[e_id.0 as usize] {
            return Ok(());
        }

        // TODO: make sure we don't link the world entity

        self.unlink_entity(e_id)?;

        let mut abs_min;
        let mut abs_max;
        let solid;
        {
            let ent = self.try_get_entity_mut(e_id.0 as usize)?;

            let origin = Vector3::from(ent.get_vector(FieldAddrVector::Origin as i16)?);
            let mins = Vector3::from(ent.get_vector(FieldAddrVector::Mins as i16)?);
            let maxs = Vector3::from(ent.get_vector(FieldAddrVector::Maxs as i16)?);
            debug!("origin = {:?} mins = {:?} maxs = {:?}", origin, mins, maxs);
            abs_min = origin + mins;
            abs_max = origin + maxs;

            let flags_f = ent.get_float(FieldAddrFloat::Flags as i16)?;
            let flags = EntityFlags::from_bits(flags_f as u16).unwrap();
            if flags.contains(EntityFlags::ITEM) {
                abs_min.x -= 15.0;
                abs_min.y -= 15.0;
                abs_max.x += 15.0;
                abs_max.y += 15.0;
            } else {
                abs_min.x -= 1.0;
                abs_min.y -= 1.0;
                abs_min.z -= 1.0;
                abs_max.x += 1.0;
                abs_max.y += 1.0;
                abs_max.z += 1.0;
            }

            ent.put_vector(
                abs_min.into(),
                FieldAddrVector::AbsMin as i16,
            )?;
            ent.put_vector(
                abs_max.into(),
                FieldAddrVector::AbsMax as i16,
            )?;

            ent.leaf_count = 0;
            let model_index = ent.get_float(FieldAddrFloat::ModelIndex as i16)?;
            if model_index != 0.0 {
                // TODO: SV_FindTouchedLeafs
            }

            solid = ent.solid()?;

            if solid == EntitySolid::Not {
                // this entity has no touch interaction, we're done
                return Ok(());
            }
        }

        let mut node_id = 0;
        loop {
            match self.area_nodes[node_id].kind {
                AreaNodeKind::Branch(ref b) => {
                    debug!(
                        "abs_min = {:?} | abs_max = {:?} | dist = {}",
                        abs_min,
                        abs_max,
                        b.dist
                    );
                    if abs_min[b.axis as usize] > b.dist {
                        node_id = b.front;
                    } else if abs_max[b.axis as usize] < b.dist {
                        node_id = b.back;
                    } else {
                        // entity spans both sides of the plane
                        break;
                    }
                }

                AreaNodeKind::Leaf => break,
            }
        }

        if solid == EntitySolid::Trigger {
            debug!("Linking entity {} into area {} triggers", e_id.0, node_id);
            self.area_nodes[node_id].triggers.insert(e_id);
            self.try_get_world_entity_mut(e_id.0 as usize)?.area_id = Some(node_id);
        } else {
            debug!("Linking entity {} into area {} solids", e_id.0, node_id);
            self.area_nodes[node_id].solids.insert(e_id);
            self.try_get_world_entity_mut(e_id.0 as usize)?.area_id = Some(node_id);
        }

        if touch_triggers {
            unimplemented!();
        }

        Ok(())
    }

    /// Update this entity's position and relink it into the world.
    pub fn set_entity_origin(
        &mut self,
        e_id: EntityId,
        origin: Vector3<f32>,
    ) -> Result<(), ProgsError> {
        {
            let ent = self.try_get_entity_mut(e_id.0 as usize)?;
            ent.put_vector(
                origin.into(),
                FieldAddrVector::Origin as i16,
            )?;
        }

        self.link_entity(e_id, false)?;
        Ok(())
    }

    pub fn set_entity_model(
        &mut self,
        e_id: EntityId,
        model_name_id: StringId,
        server: &Server,
    ) -> Result<(), ProgsError> {
        let model_index;
        {
            let ent = self.try_get_entity_mut(e_id.0 as usize)?;

            ent.put_string_id(
                model_name_id,
                FieldAddrStringId::ModelName as i16,
            )?;

            // TODO: change this to `?` syntax once `server` has a proper error type
            model_index = match server.model_precache_lookup(model_name_id) {
                Ok(i) => i,
                Err(_) => return Err(ProgsError::with_msg("model not precached")),
            };

            ent.put_float(
                model_index as f32,
                FieldAddrFloat::ModelIndex as i16,
            )?;
        }

        if model_index == 0 {
            self.set_entity_size(e_id, Vector3::zero(), Vector3::zero())?;
        } else {
            let min = self.models[model_index].min();
            let max = self.models[model_index].max();
            self.set_entity_size(e_id, min, max)?;
        }

        Ok(())
    }

    pub fn set_entity_size(
        &mut self,
        e_id: EntityId,
        min: Vector3<f32>,
        max: Vector3<f32>,
    ) -> Result<(), ProgsError> {
        let ent = self.try_get_entity_mut(e_id.0 as usize)?;
        ent.set_min_max_size(min, max)?;
        Ok(())
    }

    /// Unlink an entity from the world and remove it.
    pub fn remove_entity(&mut self, e_id: EntityId) -> Result<(), ProgsError> {
        self.unlink_entity(e_id)?;
        self.free(e_id)?;
        Ok(())
    }
}
