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

mod statics;

use self::statics::EntityStatics;
use self::statics::GenericEntityStatics;
pub use self::statics::FieldAddrFloat;

use std::collections::HashMap;
use std::convert::TryInto;
use std::ops::Index;
use std::rc::Rc;

use console::CvarRegistry;
use parse;
use progs::EntityFieldAddr;
use progs::EntityId;
use progs::ExecutionContext;
use progs::FieldDef;
use progs::FunctionId;
use progs::Functions;
use progs::Globals;
use progs::ProgsError;
use progs::StringId;
use progs::StringTable;
use progs::Type;

use byteorder::LittleEndian;
use byteorder::ReadBytesExt;
use byteorder::WriteBytesExt;
use cgmath::Deg;
use cgmath::Vector3;
use chrono::Duration;

const MAX_ENTITIES: usize = 600;
const MAX_ENT_LEAVES: usize = 16;

// dynamic entity fields start after this point (i.e. defined in progs.dat, not accessible here)
const ADDR_DYNAMIC_START: usize = 105;
const STATIC_ADDRESS_COUNT: usize = 105;

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

impl EntityState {
    pub fn uninitialized() -> EntityState {
        EntityState {
            origin: Vector3::new(0.0, 0.0, 0.0),
            angles: Vector3::new(Deg(0.0), Deg(0.0), Deg(0.0)),
            model_id: 0,
            frame_id: 0,
            colormap: 0,
            skin: 0,
            effects: 0,
        }
    }
}

pub struct Entity {
    // TODO: figure out how to link entities into the world
    // link: SomeType,
    string_table: Rc<StringTable>,
    leaf_count: usize,
    leaf_ids: [u16; MAX_ENT_LEAVES],
    baseline: EntityState,
    statics: EntityStatics,
    dynamics: Vec<[u8; 4]>,
}

impl Entity {
    pub fn get_float(&self, addr: i16) -> Result<f32, ProgsError> {
        if addr < 0 {
            panic!("negative offset");
        }

        let addr = addr as usize;

        if addr >= ADDR_DYNAMIC_START + self.dynamics.len() {
            return Err(ProgsError::with_msg(
                format!("out-of-bounds offset ({})", addr),
            ));
        }

        if addr < ADDR_DYNAMIC_START {
            self.get_float_static(addr)
        } else {
            self.get_float_dynamic(addr)
        }
    }

    fn get_float_static(&self, addr: usize) -> Result<f32, ProgsError> {
        match self.statics {
            EntityStatics::Generic(ref g) => g.get_float(addr),
        }
    }

    fn get_float_dynamic(&self, addr: usize) -> Result<f32, ProgsError> {
        Ok(self.dynamics[addr - ADDR_DYNAMIC_START]
            .as_ref()
            .read_f32::<LittleEndian>()?)
    }

    pub fn put_float(&mut self, val: f32, addr: i16) -> Result<(), ProgsError> {
        if addr < 0 {
            panic!("negative offset");
        }

        let addr = addr as usize;

        if addr >= ADDR_DYNAMIC_START + self.dynamics.len() {
            return Err(ProgsError::with_msg(
                format!("out-of-bounds offset ({})", addr),
            ));
        }

        if addr < ADDR_DYNAMIC_START {
            self.put_float_static(val, addr)
        } else {
            self.put_float_dynamic(val, addr)
        }
    }

    fn put_float_static(&mut self, val: f32, addr: usize) -> Result<(), ProgsError> {
        match self.statics {
            EntityStatics::Generic(ref mut g) => g.put_float(val, addr),
        }
    }

    fn put_float_dynamic(&mut self, val: f32, addr: usize) -> Result<(), ProgsError> {
        self.dynamics[addr - ADDR_DYNAMIC_START]
            .as_mut()
            .write_f32::<LittleEndian>(val)?;

        Ok(())
    }

    pub fn get_vector(&self, addr: i16) -> Result<[f32; 3], ProgsError> {
        if addr < 0 {
            panic!("negative offset");
        }

        let addr = addr as usize;

        // subtract 2 to account for size of vector
        if addr >= ADDR_DYNAMIC_START + self.dynamics.len() - 2 {
            return Err(ProgsError::with_msg(
                format!("out-of-bounds offset ({})", addr),
            ));
        }

        if addr < ADDR_DYNAMIC_START {
            self.get_vector_static(addr)
        } else {
            self.get_vector_dynamic(addr)
        }
    }

    fn get_vector_static(&self, addr: usize) -> Result<[f32; 3], ProgsError> {
        match self.statics {
            EntityStatics::Generic(ref g) => g.get_vector(addr),
        }
    }

    fn get_vector_dynamic(&self, addr: usize) -> Result<[f32; 3], ProgsError> {
        let mut v = [0.0; 3];

        for c in 0..v.len() {
            v[c] = self.get_float_dynamic(addr + c)?;
        }

        Ok(v)
    }

    pub fn put_vector(&mut self, val: [f32; 3], addr: i16) -> Result<(), ProgsError> {
        if addr < 0 {
            panic!("negative offset");
        }

        let addr = addr as usize;

        // subtract 2 to account for size of vector
        if addr >= ADDR_DYNAMIC_START + self.dynamics.len() - 2 {
            return Err(ProgsError::with_msg(
                format!("out-of-bounds offset ({})", addr),
            ));
        }

        if addr < ADDR_DYNAMIC_START {
            self.put_vector_static(val, addr)
        } else {
            self.put_vector_dynamic(val, addr)
        }
    }

    fn put_vector_static(&mut self, val: [f32; 3], addr: usize) -> Result<(), ProgsError> {
        match self.statics {
            EntityStatics::Generic(ref mut g) => g.put_vector(val, addr),
        }
    }

    fn put_vector_dynamic(&mut self, val: [f32; 3], addr: usize) -> Result<(), ProgsError> {
        for c in 0..val.len() {
            self.put_float_dynamic(val[c], addr + c)?;
        }

        Ok(())
    }

    pub fn get_string_id(&self, addr: i16) -> Result<StringId, ProgsError> {
        if addr < 0 {
            panic!("negative offset");
        }

        let addr = addr as usize;

        if addr >= ADDR_DYNAMIC_START + self.dynamics.len() {
            return Err(ProgsError::with_msg(
                format!("out-of-bounds offset ({})", addr),
            ));
        }

        if addr < ADDR_DYNAMIC_START {
            self.get_string_id_static(addr)
        } else {
            self.get_string_id_dynamic(addr)
        }
    }

    fn get_string_id_static(&self, addr: usize) -> Result<StringId, ProgsError> {
        match self.statics {
            EntityStatics::Generic(ref g) => g.get_string_id(addr),
        }
    }

    fn get_string_id_dynamic(&self, addr: usize) -> Result<StringId, ProgsError> {
        Ok(self.string_table.id_from_i32(
            self.dynamics[addr - ADDR_DYNAMIC_START]
                .as_ref()
                .read_i32::<LittleEndian>()?,
        )?)
    }

    pub fn put_string_id(&mut self, val: StringId, addr: i16) -> Result<(), ProgsError> {
        if addr < 0 {
            panic!("negative offset");
        }

        let addr = addr as usize;

        if addr >= ADDR_DYNAMIC_START + self.dynamics.len() {
            return Err(ProgsError::with_msg(
                format!("out-of-bounds offset ({})", addr),
            ));
        }

        if addr < ADDR_DYNAMIC_START {
            self.put_string_id_static(val, addr)
        } else {
            self.put_string_id_dynamic(val, addr)
        }
    }

    fn put_string_id_static(&mut self, val: StringId, addr: usize) -> Result<(), ProgsError> {
        match self.statics {
            EntityStatics::Generic(ref mut g) => g.put_string_id(val, addr),
        }
    }

    fn put_string_id_dynamic(&mut self, val: StringId, addr: usize) -> Result<(), ProgsError> {
        self.dynamics[addr - ADDR_DYNAMIC_START]
            .as_mut()
            .write_i32::<LittleEndian>(val.try_into()?)?;

        Ok(())
    }

    pub fn get_entity_id(&self, addr: i16) -> Result<EntityId, ProgsError> {
        if addr < 0 {
            panic!("negative offset");
        }

        let addr = addr as usize;

        if addr >= ADDR_DYNAMIC_START + self.dynamics.len() {
            return Err(ProgsError::with_msg(
                format!("out-of-bounds offset ({})", addr),
            ));
        }

        if addr < ADDR_DYNAMIC_START {
            self.get_entity_id_static(addr)
        } else {
            self.get_entity_id_dynamic(addr)
        }
    }

    fn get_entity_id_static(&self, addr: usize) -> Result<EntityId, ProgsError> {
        match self.statics {
            EntityStatics::Generic(ref g) => g.get_entity_id(addr),
        }
    }

    fn get_entity_id_dynamic(&self, addr: usize) -> Result<EntityId, ProgsError> {
        Ok(EntityId(self.dynamics[addr - ADDR_DYNAMIC_START]
            .as_ref()
            .read_i32::<LittleEndian>()?))
    }

    pub fn put_entity_id(&mut self, val: EntityId, addr: i16) -> Result<(), ProgsError> {
        if addr < 0 {
            panic!("negative offset");
        }

        let addr = addr as usize;

        if addr >= ADDR_DYNAMIC_START + self.dynamics.len() {
            return Err(ProgsError::with_msg(
                format!("out-of-bounds offset ({})", addr),
            ));
        }

        if addr < ADDR_DYNAMIC_START {
            self.put_entity_id_static(val, addr)
        } else {
            self.put_entity_id_dynamic(val, addr)
        }
    }

    fn put_entity_id_static(&mut self, val: EntityId, addr: usize) -> Result<(), ProgsError> {
        match self.statics {
            EntityStatics::Generic(ref mut g) => g.put_entity_id(val, addr),
        }
    }

    fn put_entity_id_dynamic(&mut self, val: EntityId, addr: usize) -> Result<(), ProgsError> {
        self.dynamics[addr - ADDR_DYNAMIC_START]
            .as_mut()
            .write_i32::<LittleEndian>(val.0)?;

        Ok(())
    }

    pub fn get_function_id(&self, addr: i16) -> Result<FunctionId, ProgsError> {
        if addr < 0 {
            panic!("negative offset");
        }

        let addr = addr as usize;

        if addr >= ADDR_DYNAMIC_START + self.dynamics.len() {
            return Err(ProgsError::with_msg(
                format!("out-of-bounds offset ({})", addr),
            ));
        }

        if addr < ADDR_DYNAMIC_START {
            self.get_function_id_static(addr)
        } else {
            self.get_function_id_dynamic(addr)
        }
    }

    fn get_function_id_static(&self, addr: usize) -> Result<FunctionId, ProgsError> {
        match self.statics {
            EntityStatics::Generic(ref g) => g.get_function_id(addr),
        }
    }

    fn get_function_id_dynamic(&self, addr: usize) -> Result<FunctionId, ProgsError> {
        Ok(FunctionId(self.dynamics[addr - ADDR_DYNAMIC_START]
            .as_ref()
            .read_i32::<LittleEndian>()? as usize))
    }

    pub fn put_function_id(&mut self, val: FunctionId, addr: i16) -> Result<(), ProgsError> {
        if addr < 0 {
            panic!("negative offset");
        }

        let addr = addr as usize;

        if addr >= ADDR_DYNAMIC_START + self.dynamics.len() {
            return Err(ProgsError::with_msg(
                format!("out-of-bounds offset ({})", addr),
            ));
        }

        if addr < ADDR_DYNAMIC_START {
            self.put_function_id_static(val, addr)
        } else {
            self.put_function_id_dynamic(val, addr)
        }
    }

    fn put_function_id_static(&mut self, val: FunctionId, addr: usize) -> Result<(), ProgsError> {
        match self.statics {
            EntityStatics::Generic(ref mut g) => g.put_function_id(val, addr),
        }
    }

    fn put_function_id_dynamic(&mut self, val: FunctionId, addr: usize) -> Result<(), ProgsError> {
        self.dynamics[addr - ADDR_DYNAMIC_START]
            .as_mut()
            .write_i32::<LittleEndian>(val.try_into()?)?;

        Ok(())
    }
}

pub enum EntityListEntry {
    Free(Duration),
    NotFree(Entity),
}

pub struct EntityList {
    addr_count: usize,
    string_table: Rc<StringTable>,
    functions: Rc<Functions>,
    field_defs: Box<[FieldDef]>,
    entries: Box<[EntityListEntry]>,
}

impl EntityList {
    /// Initializes a new entity list with the given parameters.
    pub fn new(
        addr_count: usize,
        string_table: Rc<StringTable>,
        functions: Rc<Functions>,
        field_defs: Box<[FieldDef]>,
    ) -> EntityList {
        if addr_count < STATIC_ADDRESS_COUNT {
            panic!(
                "EntityList::new: addr_count must be at least {} (was {})",
                STATIC_ADDRESS_COUNT,
                addr_count
            );
        }
        let mut entries = Vec::new();
        for _ in 0..MAX_ENTITIES {
            entries.push(EntityListEntry::Free(Duration::zero()));
        }
        let entries = entries.into_boxed_slice();

        EntityList {
            addr_count,
            string_table,
            functions,
            field_defs,
            entries,
        }
    }

    fn find_def<S>(&self, name: S) -> Result<&FieldDef, ProgsError>
    where
        S: AsRef<str>,
    {
        let name = name.as_ref();
        let name_id = self.string_table.find(name).unwrap();

        match self.field_defs.iter().find(|def| {
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
        let total_addr = (ent_fld_addr.entity_id * self.addr_count + ent_fld_addr.field_addr) * 4;

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
            entity_id: total_addr / self.addr_count,
            field_addr: total_addr % self.addr_count,
        }
    }

    fn gen_dynamics(&self) -> Vec<[u8; 4]> {
        let mut v = Vec::with_capacity(self.addr_count);
        for _ in 0..self.addr_count {
            v.push([0; 4]);
        }
        v
    }

    fn find_free_entry(&self) -> Result<usize, ProgsError> {
        for (i, entry) in self.entries.iter().enumerate() {
            if let &EntityListEntry::Free(_) = entry {
                return Ok(i);
            }
        }

        Err(ProgsError::with_msg("no free entries"))
    }

    pub fn alloc_uninitialized(&mut self) -> Result<EntityId, ProgsError> {
        let entry_id = self.find_free_entry()?;

        self.entries[entry_id] = EntityListEntry::NotFree(Entity {
            string_table: self.string_table.clone(),
            leaf_count: 0,
            leaf_ids: [0; MAX_ENT_LEAVES],
            baseline: EntityState::uninitialized(),
            statics: EntityStatics::Generic(GenericEntityStatics::default()),
            dynamics: self.gen_dynamics(),
        });

        Ok(EntityId(entry_id as i32))
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
    pub fn alloc_from_map(
        &mut self,
        execution_context: &mut ExecutionContext,
        globals: &mut Globals,
        cvars: &mut CvarRegistry,
        map: HashMap<&str, &str>,
    ) -> Result<EntityId, ProgsError> {
        let mut ent = Entity {
            leaf_count: 0,
            leaf_ids: [0; MAX_ENT_LEAVES],
            baseline: EntityState::uninitialized(),
            string_table: self.string_table.clone(),
            statics: EntityStatics::Generic(GenericEntityStatics::default()),
            dynamics: self.gen_dynamics(),
        };

        for (key, val) in map.iter() {
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

                        Type::QString => {
                            ent.put_string_id(
                                self.string_table.insert(val),
                                def.offset as i16,
                            )?;
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

                            match self.entries[id] {
                                EntityListEntry::Free(_) => panic!("no entity with id {}", id),
                                EntityListEntry::NotFree(_) => (),
                            }

                            ent.put_entity_id(EntityId(id as i32), def.offset as i16)?
                        }
                        Type::QField => panic!("attempted to store field of type Field in entity"),
                        Type::QFunction => {
                            // TODO: need to validate this against function table
                        }
                        _ => (),
                    }
                }
            }
        }

        match map.get("classname") {
            Some(c) => {
                execution_context.execute_program_by_name(
                    globals,
                    self,
                    cvars,
                    c,
                )?
            }
            None => return Err(ProgsError::with_msg("No entity for classname, can't spawn")),
        }

        let entry_id = self.find_free_entry()?;
        self.entries[entry_id] = EntityListEntry::NotFree(ent);

        Ok(EntityId(entry_id as i32))
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
