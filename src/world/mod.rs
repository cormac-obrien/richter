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
use std::rc::Rc;

use self::entity::Entity;
pub use self::entity::EntityTypeDef;
pub use self::entity::FieldAddrEntityId;
pub use self::entity::FieldAddrFloat;
pub use self::entity::FieldAddrFunctionId;
pub use self::entity::FieldAddrStringId;
pub use self::entity::FieldAddrVector;
use self::entity::STATIC_ADDRESS_COUNT;

use console::CvarRegistry;
use model::Model;
use parse;
use progs::EntityFieldAddr;
use progs::EntityId;
use progs::ExecutionContext;
use progs::FieldDef;
use progs::GlobalAddrEntity;
use progs::Globals;
use progs::ProgsError;
use progs::StringTable;
use progs::Type;
use server::Server;

use cgmath::Vector3;

const AREA_DEPTH: usize = 4;

const MAX_ENTITIES: usize = 600;

enum WorldLinkKind {
    None,
    Entity,
    Area,
}

struct WorldLink {
    prev: WorldLinkKind,
    next: WorldLinkKind,
}

impl WorldLink {
    pub fn none() -> WorldLink {
        WorldLink {
            prev: WorldLinkKind::None,
            next: WorldLinkKind::None,
        }
    }
}

struct AreaTree(AreaNode);

impl AreaTree {
    pub fn generate(mins: Vector3<f32>, maxs: Vector3<f32>) -> AreaTree {
        if mins.x >= maxs.x || mins.y >= maxs.y || mins.z >= maxs.z {
            panic!("Invalid bounding box (min: {:?} max: {:?})", mins, maxs);
        }

        AreaTree(AreaNode::generate(AREA_DEPTH, mins, maxs))
    }
}

enum AreaNode {
    Branch(AreaBranch),
    Leaf(AreaLeaf),
}

impl AreaNode {
    pub fn generate(depth: usize, mins: Vector3<f32>, maxs: Vector3<f32>) -> AreaNode {
        if depth == 0 {
            return AreaNode::Leaf(AreaLeaf::new());
        }

        let axis;
        let size = maxs - mins;

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

        let front = AreaNode::generate(depth - 1, front_mins, maxs);
        let back = AreaNode::generate(depth - 1, mins, back_maxs);

        AreaNode::Branch(AreaBranch {
            axis,
            dist,
            front: Box::new(front),
            back: Box::new(back),
            triggers: WorldLink::none(),
            solids: WorldLink::none(),
        })
    }
}

#[derive(Copy, Clone)]
enum AreaBranchAxis {
    X = 0,
    Y = 1,
}

struct AreaBranch {
    axis: AreaBranchAxis,
    dist: f32,
    front: Box<AreaNode>,
    back: Box<AreaNode>,
    triggers: WorldLink,
    solids: WorldLink,
}

struct AreaLeaf {
    triggers: WorldLink,
    solids: WorldLink,
}

impl AreaLeaf {
    pub fn new() -> AreaLeaf {
        AreaLeaf {
            triggers: WorldLink::none(),
            solids: WorldLink::none(),
        }
    }
}

enum WorldEntitySlot {
    Vacant,
    Occupied(Entity),
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

        self.slots[slot_id] =
            WorldEntitySlot::Occupied(Entity::new(self.string_table.clone(), self.gen_dynamics()));

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
        self.slots[entry_id] = WorldEntitySlot::Occupied(ent);

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
            WorldEntitySlot::Occupied(ref mut e) => Ok(e),
        }
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
            WorldEntitySlot::Occupied(ref e) => Ok(e),
        }
    }
}

pub struct World {
    area_tree: AreaTree,
    entities: WorldEntities,
    models: Vec<Model>,
}

impl World {
    pub fn create(
        mut brush_models: Vec<Model>,
        type_def: EntityTypeDef,
        string_table: Rc<StringTable>,
    ) -> Result<World, ()> {
        // generate area tree for world model
        let area_tree = AreaTree::generate(brush_models[0].min(), brush_models[0].max());

        let entities = WorldEntities::new(type_def, string_table);

        // take ownership of all brush models
        let mut models = Vec::with_capacity(brush_models.len() + 1);
        models.push(Model::none());
        models.append(&mut brush_models);

        Ok(World {
            area_tree,
            entities,
            models,
        })
    }

    pub fn try_get_entity(&self, e_id: usize) -> Result<&Entity, ProgsError> {
        self.entities.try_get_entity(e_id)
    }

    pub fn try_get_entity_mut(&mut self, e_id: usize) -> Result<&mut Entity, ProgsError> {
        self.entities.try_get_entity_mut(e_id)
    }

    pub fn ent_fld_addr_to_i32(&self, ent_fld_addr: EntityFieldAddr) -> i32 {
        self.entities.ent_fld_addr_to_i32(ent_fld_addr)
    }

    pub fn ent_fld_addr_from_i32(&self, val: i32) -> EntityFieldAddr {
        self.entities.ent_fld_addr_from_i32(val)
    }

    pub fn spawn_entity(&mut self) -> Result<EntityId, ProgsError> {
        self.entities.alloc_uninitialized()
    }

    pub fn spawn_entity_from_map(
        &mut self,
        execution_context: &mut ExecutionContext,
        globals: &mut Globals,
        cvars: &mut CvarRegistry,
        server: &mut Server,
        map: HashMap<&str, &str>,
    ) -> Result<EntityId, ProgsError> {
        let classname = match map.get("classname") {
            Some(c) => c.to_owned(),
            None => return Err(ProgsError::with_msg("No classname for entity")),
        };

        let e_id = self.entities.alloc_from_map(map)?;

        // set `self` before calling spawn function
        globals.put_entity_id(e_id, GlobalAddrEntity::Self_ as i16)?;

        execution_context.execute_program_by_name(
            globals,
            self,
            cvars,
            server,
            classname,
        )?;

        // TODO: link entity into world

        Ok(e_id)
    }

    pub fn remove_entity(&mut self, e_id: EntityId) -> Result<(), ProgsError> {
        self.entities.free(e_id)?;
        // TODO: UNLINK ENTITY FROM WORLD
        Ok(())
    }
}
