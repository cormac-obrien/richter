// Copyright © 2018 Cormac O'Brien
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
pub mod phys;

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
};

use self::{
    entity::Entity,
    phys::{Collide, CollideKind},
};
pub use self::{
    entity::{
        EntityError, EntityFlags, EntitySolid, EntityTypeDef, FieldAddrEntityId, FieldAddrFloat,
        FieldAddrFunctionId, FieldAddrStringId, FieldAddrVector,
    },
    phys::{MoveKind, Trace, TraceEnd, TraceEndKind, TraceStart},
};

use crate::{
    common::{
        bsp,
        bsp::{BspCollisionHull, BspLeafContents},
        mdl,
        model::{Model, ModelKind},
        parse, sprite,
        vfs::Vfs,
    },
    server::progs::{
        EntityFieldAddr, EntityId, FieldAddr, FieldDef, FunctionId, ProgsError, StringId,
        StringTable, Type,
    },
};

use arrayvec::ArrayVec;
use cgmath::{InnerSpace, Vector3, Zero};

const AREA_DEPTH: usize = 4;
const NUM_AREA_NODES: usize = 2usize.pow(AREA_DEPTH as u32 + 1) - 1;
const MAX_ENTITIES: usize = 600;

#[derive(Debug)]
enum AreaNodeKind {
    Branch(AreaBranch),
    Leaf,
}

#[derive(Debug)]
struct AreaNode {
    kind: AreaNodeKind,
    triggers: HashSet<EntityId>,
    solids: HashSet<EntityId>,
}

// The areas form a quadtree-like BSP tree which alternates splitting on the X
// and Y axes.
//
//                               00                                X
//               01                              02                Y
//       03              04              05              06        X
//   07      08      09      10      11      12      13      14    Y
// 15  16  17  18  19  20  21  22  23  24  25  26  27  28  29  30  Leaves
//
//  [21]      [19]      [17]      [15]
//   ||        ||        ||        ||
//   ||        ||        ||        ||
//   12===05===11        08===03===07
//   ||   ||   ||        ||   ||   ||
//   ||   ||   ||        ||   ||   ||
//  [22]  ||  [20]      [18]  ||  [16]
//        ||                  ||
//        02========00========01
//        ||                  ||
//  [29]  ||  [27]      [25]  ||  [23]
//   ||   ||   ||        ||   ||   ||
//   ||   ||   ||        ||   ||   ||
//   14===06===13        10===04===09
//   ||        ||        ||        ||
//   ||        ||        ||        ||
//  [30]      [28]      [26]      [24]
//
// The tree won't necessarily look like this, this just assumes a rectangular area with width
// between 1-2x its length.

impl AreaNode {
    /// Generate a breadth-first 2-D binary space partitioning tree with the given extents.
    pub fn generate(mins: Vector3<f32>, maxs: Vector3<f32>) -> ArrayVec<AreaNode, NUM_AREA_NODES> {
        let mut nodes: ArrayVec<AreaNode, NUM_AREA_NODES> = ArrayVec::new();

        // we generate the skeleton of the tree iteratively -- the nodes are linked but have no
        // geometric data.

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

        // recursively assign geometric data to the nodes
        AreaNode::setup(&mut nodes, 0, mins, maxs);

        nodes
    }

    fn setup(
        nodes: &mut ArrayVec<AreaNode, NUM_AREA_NODES>,
        index: usize,
        mins: Vector3<f32>,
        maxs: Vector3<f32>,
    ) {
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

#[derive(Debug)]
struct AreaBranch {
    axis: AreaBranchAxis,
    dist: f32,
    front: usize,
    back: usize,
}

#[derive(Debug)]
struct AreaEntity {
    entity: Entity,
    area_id: Option<usize>,
}

#[derive(Debug)]
enum AreaEntitySlot {
    Vacant,
    Occupied(AreaEntity),
}

/// A representation of the current state of the game world.
#[derive(Debug)]
pub struct World {
    string_table: Rc<RefCell<StringTable>>,
    type_def: Rc<EntityTypeDef>,

    area_nodes: ArrayVec<AreaNode, NUM_AREA_NODES>,
    slots: Box<[AreaEntitySlot]>,
    models: Vec<Model>,
}

impl World {
    pub fn create(
        mut brush_models: Vec<Model>,
        type_def: Rc<EntityTypeDef>,
        string_table: Rc<RefCell<StringTable>>,
    ) -> Result<World, ProgsError> {
        // generate area tree for world model
        let area_nodes = AreaNode::generate(brush_models[0].min(), brush_models[0].max());

        let mut models = Vec::with_capacity(brush_models.len() + 1);

        // put null model at index 0
        models.push(Model::none());

        // take ownership of all brush models
        models.append(&mut brush_models);

        // generate world entity
        let mut world_entity = Entity::new(string_table.clone(), type_def.clone());
        world_entity.put_string_id(
            string_table.borrow_mut().find_or_insert(models[1].name()),
            FieldAddrStringId::ModelName as i16,
        )?;
        world_entity.put_float(1.0, FieldAddrFloat::ModelIndex as i16)?;
        world_entity.put_float(EntitySolid::Bsp as u32 as f32, FieldAddrFloat::Solid as i16)?;
        world_entity.put_float(
            MoveKind::Push as u32 as f32,
            FieldAddrFloat::MoveKind as i16,
        )?;

        let mut slots = Vec::with_capacity(MAX_ENTITIES);
        slots.push(AreaEntitySlot::Occupied(AreaEntity {
            entity: world_entity,
            area_id: None,
        }));
        for _ in 0..MAX_ENTITIES - 1 {
            slots.push(AreaEntitySlot::Vacant);
        }

        Ok(World {
            string_table,
            area_nodes,
            type_def,
            slots: slots.into_boxed_slice(),
            models,
        })
    }

    pub fn add_model(&mut self, vfs: &Vfs, name_id: StringId) -> Result<(), ProgsError> {
        let strs = self.string_table.borrow();
        let name = strs.get(name_id).unwrap();

        if name.ends_with(".bsp") {
            let data = vfs.open(name).unwrap();
            let (mut brush_models, _) = bsp::load(data).unwrap();
            if brush_models.len() > 1 {
                return Err(ProgsError::with_msg(
                    "Complex brush models must be loaded before world creation",
                ));
            }
            self.models.append(&mut brush_models);
        } else if name.ends_with(".mdl") {
            let data = vfs.open(&name).unwrap();
            let alias_model = mdl::load(data).unwrap();
            self.models
                .push(Model::from_alias_model(&name, alias_model));
        } else if name.ends_with(".spr") {
            let data = vfs.open(&name).unwrap();
            let sprite_model = sprite::load(data);
            self.models
                .push(Model::from_sprite_model(&name, sprite_model));
        } else {
            return Err(ProgsError::with_msg(format!(
                "Unrecognized model type: {}",
                name
            )));
        }

        Ok(())
    }

    fn find_def<S>(&self, name: S) -> Result<&FieldDef, ProgsError>
    where
        S: AsRef<str>,
    {
        let name = name.as_ref();

        match self
            .type_def
            .field_defs()
            .iter()
            .find(|def| self.string_table.borrow().get(def.name_id).unwrap() == name)
        {
            Some(d) => Ok(d),
            None => Err(ProgsError::with_msg(format!("no field with name {}", name))),
        }
    }

    /// Convert an entity ID and field address to an internal representation used by the VM.
    ///
    /// This representation should be compatible with the one used by the original Quake.
    pub fn ent_fld_addr_to_i32(&self, ent_fld_addr: EntityFieldAddr) -> i32 {
        let total_addr =
            (ent_fld_addr.entity_id.0 * self.type_def.addr_count() + ent_fld_addr.field_addr.0) * 4;

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
            entity_id: EntityId(total_addr / self.type_def.addr_count()),
            field_addr: FieldAddr(total_addr % self.type_def.addr_count()),
        }
    }

    fn find_vacant_slot(&self) -> Result<usize, ()> {
        for (i, slot) in self.slots.iter().enumerate() {
            if let &AreaEntitySlot::Vacant = slot {
                return Ok(i);
            }
        }

        panic!("no vacant slots");
    }

    pub fn alloc_uninitialized(&mut self) -> Result<EntityId, ProgsError> {
        let slot_id = self.find_vacant_slot().unwrap();

        self.slots[slot_id] = AreaEntitySlot::Occupied(AreaEntity {
            entity: Entity::new(self.string_table.clone(), self.type_def.clone()),
            area_id: None,
        });

        Ok(EntityId(slot_id))
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
        let mut ent = Entity::new(self.string_table.clone(), self.type_def.clone());

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
                    ent.put_vector([0.0, val.parse().unwrap(), 0.0], def.offset as i16)?;
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
                            let s_id = self.string_table.borrow_mut().insert(val);
                            ent.put_string_id(s_id, def.offset as i16)?;
                        }

                        Type::QFloat => ent.put_float(val.parse().unwrap(), def.offset as i16)?,
                        Type::QVector => ent.put_vector(
                            parse::vector3_components(val).unwrap(),
                            def.offset as i16,
                        )?,
                        Type::QEntity => {
                            let id: usize = val.parse().unwrap();

                            if id > MAX_ENTITIES {
                                panic!("out-of-bounds entity access");
                            }

                            match self.slots[id] {
                                AreaEntitySlot::Vacant => panic!("no entity with id {}", id),
                                AreaEntitySlot::Occupied(_) => (),
                            }

                            ent.put_entity_id(EntityId(id), def.offset as i16)?
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

        self.slots[entry_id] = AreaEntitySlot::Occupied(AreaEntity {
            entity: ent,
            area_id: None,
        });

        Ok(EntityId(entry_id))
    }

    pub fn free(&mut self, entity_id: EntityId) -> Result<(), ProgsError> {
        // TODO: unlink entity from world

        if entity_id.0 as usize > self.slots.len() {
            return Err(ProgsError::with_msg(format!(
                "Invalid entity ID ({:?})",
                entity_id
            )));
        }

        if let AreaEntitySlot::Vacant = self.slots[entity_id.0 as usize] {
            return Ok(());
        }

        self.slots[entity_id.0 as usize] = AreaEntitySlot::Vacant;
        Ok(())
    }

    /// Returns a reference to an entity.
    ///
    /// # Panics
    ///
    /// This method panics if `entity_id` does not refer to a valid slot or if
    /// the slot is vacant.
    #[inline]
    pub fn entity(&self, entity_id: EntityId) -> &Entity {
        match self.slots[entity_id.0 as usize] {
            AreaEntitySlot::Vacant => panic!("no such entity: {:?}", entity_id),
            AreaEntitySlot::Occupied(ref e) => &e.entity,
        }
    }

    pub fn try_entity(&self, entity_id: EntityId) -> Result<&Entity, ProgsError> {
        if entity_id.0 as usize > self.slots.len() {
            return Err(ProgsError::with_msg(format!(
                "Invalid entity ID ({})",
                entity_id.0 as usize
            )));
        }

        match self.slots[entity_id.0 as usize] {
            AreaEntitySlot::Vacant => Err(ProgsError::with_msg(format!(
                "No entity at list entry {}",
                entity_id.0 as usize
            ))),
            AreaEntitySlot::Occupied(ref e) => Ok(&e.entity),
        }
    }

    pub fn entity_mut(&mut self, entity_id: EntityId) -> Result<&mut Entity, ProgsError> {
        if entity_id.0 as usize > self.slots.len() {
            return Err(ProgsError::with_msg(format!(
                "Invalid entity ID ({})",
                entity_id.0 as usize
            )));
        }

        match self.slots[entity_id.0 as usize] {
            AreaEntitySlot::Vacant => Err(ProgsError::with_msg(format!(
                "No entity at list entry {}",
                entity_id.0 as usize
            ))),
            AreaEntitySlot::Occupied(ref mut e) => Ok(&mut e.entity),
        }
    }

    pub fn entity_exists(&mut self, entity_id: EntityId) -> bool {
        matches!(
            self.slots[entity_id.0 as usize],
            AreaEntitySlot::Occupied(_)
        )
    }

    pub fn list_entities(&self, list: &mut Vec<EntityId>) {
        for (id, slot) in self.slots.iter().enumerate() {
            if let &AreaEntitySlot::Occupied(_) = slot {
                list.push(EntityId(id));
            }
        }
    }

    fn area_entity(&self, entity_id: EntityId) -> Result<&AreaEntity, ProgsError> {
        if entity_id.0 as usize > self.slots.len() {
            return Err(ProgsError::with_msg(format!(
                "Invalid entity ID ({})",
                entity_id.0 as usize
            )));
        }

        match self.slots[entity_id.0 as usize] {
            AreaEntitySlot::Vacant => Err(ProgsError::with_msg(format!(
                "No entity at list entry {}",
                entity_id.0 as usize
            ))),
            AreaEntitySlot::Occupied(ref e) => Ok(e),
        }
    }

    fn area_entity_mut(&mut self, entity_id: EntityId) -> Result<&mut AreaEntity, ProgsError> {
        if entity_id.0 as usize > self.slots.len() {
            return Err(ProgsError::with_msg(format!(
                "Invalid entity ID ({})",
                entity_id.0 as usize
            )));
        }

        match self.slots[entity_id.0 as usize] {
            AreaEntitySlot::Vacant => Err(ProgsError::with_msg(format!(
                "No entity at list entry {}",
                entity_id.0 as usize
            ))),
            AreaEntitySlot::Occupied(ref mut e) => Ok(e),
        }
    }

    /// Lists the triggers touched by an entity.
    ///
    /// The triggers' IDs are stored in `touched`.
    pub fn list_touched_triggers(
        &mut self,
        touched: &mut Vec<EntityId>,
        ent_id: EntityId,
        area_id: usize,
    ) -> Result<(), ProgsError> {
        'next_trigger: for trigger_id in self.area_nodes[area_id].triggers.iter().copied() {
            if trigger_id == ent_id {
                // Don't trigger self.
                continue;
            }

            let ent = self.entity(ent_id);
            let trigger = self.entity(trigger_id);

            let trigger_touch = trigger.load(FieldAddrFunctionId::Touch)?;
            if trigger_touch == FunctionId(0) || trigger.solid()? == EntitySolid::Not {
                continue;
            }

            for i in 0..3 {
                if ent.abs_min()?[i] > trigger.abs_max()?[i]
                    || ent.abs_max()?[i] < trigger.abs_min()?[i]
                {
                    // Entities are not touching.
                    continue 'next_trigger;
                }
            }

            touched.push(trigger_id);
        }

        // Touch all triggers in sub-areas.
        if let AreaNodeKind::Branch(AreaBranch { front, back, .. }) = self.area_nodes[area_id].kind
        {
            self.list_touched_triggers(touched, ent_id, front)?;
            self.list_touched_triggers(touched, ent_id, back)?;
        }

        Ok(())
    }

    pub fn unlink_entity(&mut self, e_id: EntityId) -> Result<(), ProgsError> {
        // if this entity has been removed or freed, do nothing
        if let AreaEntitySlot::Vacant = self.slots[e_id.0 as usize] {
            return Ok(());
        }

        let area_id = match self.area_entity(e_id)?.area_id {
            Some(i) => i,

            // entity not linked
            None => return Ok(()),
        };

        if self.area_nodes[area_id].triggers.remove(&e_id) {
            debug!("Unlinking entity {} from area triggers", e_id.0);
        } else if self.area_nodes[area_id].solids.remove(&e_id) {
            debug!("Unlinking entity {} from area solids", e_id.0);
        }

        self.area_entity_mut(e_id)?.area_id = None;

        Ok(())
    }

    pub fn link_entity(&mut self, e_id: EntityId) -> Result<(), ProgsError> {
        // don't link the world entity
        if e_id.0 == 0 {
            return Ok(());
        }

        // if this entity has been removed or freed, do nothing
        if let AreaEntitySlot::Vacant = self.slots[e_id.0 as usize] {
            return Ok(());
        }

        self.unlink_entity(e_id)?;

        let mut abs_min;
        let mut abs_max;
        let solid;
        {
            let ent = self.entity_mut(e_id)?;

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

            ent.put_vector(abs_min.into(), FieldAddrVector::AbsMin as i16)?;
            ent.put_vector(abs_max.into(), FieldAddrVector::AbsMax as i16)?;

            // Mark leaves containing entity for PVS.
            ent.leaf_count = 0;
            let model_index = ent.get_float(FieldAddrFloat::ModelIndex as i16)?;
            if model_index != 0.0 {
                // TODO: SV_FindTouchedLeafs
                todo!("SV_FindTouchedLeafs");
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
                        abs_min, abs_max, b.dist
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
            self.area_entity_mut(e_id)?.area_id = Some(node_id);
        } else {
            debug!("Linking entity {} into area {} solids", e_id.0, node_id);
            self.area_nodes[node_id].solids.insert(e_id);
            self.area_entity_mut(e_id)?.area_id = Some(node_id);
        }

        Ok(())
    }

    pub fn set_entity_model(&mut self, e_id: EntityId, model_id: usize) -> Result<(), ProgsError> {
        if model_id == 0 {
            self.set_entity_size(e_id, Vector3::zero(), Vector3::zero())?;
        } else {
            let min = self.models[model_id].min();
            let max = self.models[model_id].max();
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
        let ent = self.entity_mut(e_id)?;
        ent.set_min_max_size(min, max)?;
        Ok(())
    }

    /// Unlink an entity from the world and remove it.
    pub fn remove_entity(&mut self, e_id: EntityId) -> Result<(), ProgsError> {
        self.unlink_entity(e_id)?;
        self.free(e_id)?;
        Ok(())
    }

    // TODO: handle the offset return value internally
    pub fn hull_for_entity(
        &self,
        e_id: EntityId,
        min: Vector3<f32>,
        max: Vector3<f32>,
    ) -> Result<(BspCollisionHull, Vector3<f32>), ProgsError> {
        let solid = self.entity(e_id).solid()?;
        debug!("Entity solid type: {:?}", solid);

        match solid {
            EntitySolid::Bsp => {
                if self.entity(e_id).move_kind()? != MoveKind::Push {
                    return Err(ProgsError::with_msg(format!(
                        "Brush entities must have MoveKind::Push (has {:?})",
                        self.entity(e_id).move_kind()
                    )));
                }

                let size = max - min;
                match self.models[self.entity(e_id).model_index()?].kind() {
                    &ModelKind::Brush(ref bmodel) => {
                        let hull_index;

                        // TODO: replace these magic constants
                        if size[0] < 3.0 {
                            debug!("Using hull 0");
                            hull_index = 0;
                        } else if size[0] <= 32.0 {
                            debug!("Using hull 1");
                            hull_index = 1;
                        } else {
                            debug!("Using hull 2");
                            hull_index = 2;
                        }

                        let hull = bmodel.hull(hull_index).unwrap();

                        let offset = hull.min() - min + self.entity(e_id).origin()?;

                        Ok((hull, offset))
                    }
                    _ => Err(ProgsError::with_msg(format!(
                        "Non-brush entities may not have MoveKind::Push"
                    ))),
                }
            }

            _ => {
                let hull = BspCollisionHull::for_bounds(
                    self.entity(e_id).min()?,
                    self.entity(e_id).max()?,
                )
                .unwrap();
                let offset = self.entity(e_id).origin()?;

                Ok((hull, offset))
            }
        }
    }

    // TODO: come up with a better name. This doesn't actually move the entity!
    pub fn move_entity(
        &mut self,
        e_id: EntityId,
        start: Vector3<f32>,
        min: Vector3<f32>,
        max: Vector3<f32>,
        end: Vector3<f32>,
        kind: CollideKind,
    ) -> Result<(Trace, Option<EntityId>), ProgsError> {
        debug!(
            "start={:?} min={:?} max={:?} end={:?}",
            start, min, max, end
        );

        debug!("Collision test: Entity {} with world entity", e_id.0);
        let world_trace = self.collide_move_with_entity(EntityId(0), start, min, max, end)?;

        debug!(
            "End position after collision test with world hull: {:?}",
            world_trace.end_point()
        );

        // if this is a rocket or a grenade, expand the monster collision box
        let (monster_min, monster_max) = match kind {
            CollideKind::Missile => (
                min - Vector3::new(15.0, 15.0, 15.0),
                max + Vector3::new(15.0, 15.0, 15.0),
            ),
            _ => (min, max),
        };

        let (move_min, move_max) =
            self::phys::bounds_for_move(start, monster_min, monster_max, end);

        let collide = Collide {
            e_id: Some(e_id),
            move_min,
            move_max,
            min,
            max,
            monster_min,
            monster_max,
            start,
            end,
            kind,
        };

        let (collide_trace, collide_ent) = self.collide(&collide)?;

        if collide_trace.all_solid()
            || collide_trace.start_solid()
            || collide_trace.ratio() < world_trace.ratio()
        {
            Ok((collide_trace, collide_ent))
        } else {
            Ok((world_trace, Some(EntityId(0))))
        }
    }

    pub fn collide(&self, collide: &Collide) -> Result<(Trace, Option<EntityId>), ProgsError> {
        self.collide_area(0, collide)
    }

    fn collide_area(
        &self,
        area_id: usize,
        collide: &Collide,
    ) -> Result<(Trace, Option<EntityId>), ProgsError> {
        let mut trace = Trace::new(
            TraceStart::new(Vector3::zero(), 0.0),
            TraceEnd::terminal(Vector3::zero()),
            BspLeafContents::Empty,
        );

        let mut collide_entity = None;

        let area = &self.area_nodes[area_id];

        for touch in area.solids.iter() {
            // don't collide an entity with itself
            if let Some(e) = collide.e_id {
                if e == *touch {
                    continue;
                }
            }

            match self.entity(*touch).solid()? {
                // if the other entity has no collision, skip it
                EntitySolid::Not => continue,

                // triggers should not appear in the solids list
                EntitySolid::Trigger => {
                    return Err(ProgsError::with_msg(format!(
                        "Trigger in solids list with ID ({})",
                        touch.0
                    )))
                }

                // don't collide with monsters if the collide specifies not to do so
                s => {
                    if s != EntitySolid::Bsp && collide.kind == CollideKind::NoMonsters {
                        continue;
                    }
                }
            }

            // if bounding boxes never intersect, skip this entity
            for i in 0..3 {
                if collide.move_min[i] > self.entity(*touch).abs_max()?[i]
                    || collide.move_max[i] < self.entity(*touch).abs_min()?[i]
                {
                    continue;
                }
            }

            if let Some(e) = collide.e_id {
                if self.entity(e).size()?[0] != 0.0 && self.entity(*touch).size()?[0] == 0.0 {
                    continue;
                }
            }

            if trace.all_solid() {
                return Ok((trace, collide_entity));
            }

            if let Some(e) = collide.e_id {
                // don't collide against owner or owned entities
                if self.entity(*touch).owner()? == e || self.entity(e).owner()? == *touch {
                    continue;
                }
            }

            // select bounding boxes based on whether or not candidate is a monster
            let tmp_trace;
            if self.entity(*touch).flags()?.contains(EntityFlags::MONSTER) {
                tmp_trace = self.collide_move_with_entity(
                    *touch,
                    collide.start,
                    collide.monster_min,
                    collide.monster_max,
                    collide.end,
                )?;
            } else {
                tmp_trace = self.collide_move_with_entity(
                    *touch,
                    collide.start,
                    collide.min,
                    collide.max,
                    collide.end,
                )?;
            }

            let old_dist = (trace.end_point() - collide.start).magnitude();
            let new_dist = (tmp_trace.end_point() - collide.start).magnitude();

            // check to see if this candidate is the closest yet and update trace if so
            if tmp_trace.all_solid() || tmp_trace.start_solid() || new_dist < old_dist {
                collide_entity = Some(*touch);
                trace = tmp_trace;
            }
        }

        match area.kind {
            AreaNodeKind::Leaf => (),

            AreaNodeKind::Branch(ref b) => {
                if collide.move_max[b.axis as usize] > b.dist {
                    self.collide_area(b.front, collide)?;
                }

                if collide.move_min[b.axis as usize] < b.dist {
                    self.collide_area(b.back, collide)?;
                }
            }
        }

        Ok((trace, collide_entity))
    }

    pub fn collide_move_with_entity(
        &self,
        e_id: EntityId,
        start: Vector3<f32>,
        min: Vector3<f32>,
        max: Vector3<f32>,
        end: Vector3<f32>,
    ) -> Result<Trace, ProgsError> {
        let (hull, offset) = self.hull_for_entity(e_id, min, max)?;
        debug!("hull offset: {:?}", offset);
        debug!(
            "hull contents at start: {:?}",
            hull.contents_at_point(start).unwrap()
        );

        Ok(hull
            .trace(start - offset, end - offset)
            .unwrap()
            .adjust(offset))
    }
}
