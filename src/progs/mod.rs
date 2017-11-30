// Copyright © 2017 Cormac O'Brien.
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

//! QuakeC bytecode interpreter
//!
//! # Loading
//!
//! QuakeC bytecode is typically loaded from `progs.dat` or `qwprogs.dat`. Bytecode files begin with
//! a brief header with an `i32` format version number (which must equal VERSION) and an `i32` CRC
//! checksum to ensure the correct bytecode is being loaded.
//!
//! ```text
//! version: i32,
//! crc: i32,
//! ```
//!
//! This is followed by a series of six lumps acting as a directory into the file data. Each lump
//! consists of an `i32` byte offset into the file data and an `i32` element count.
//!
//! ```text
//! statement_offset: i32,
//! statement_count: i32,
//!
//! globaldef_offset: i32,
//! globaldef_count: i32,
//!
//! fielddef_offset: i32,
//! fielddef_count: i32,
//!
//! function_offset: i32,
//! function_count: i32,
//!
//! string_offset: i32,
//! string_count: i32,
//!
//! global_offset: i32,
//! global_count: i32,
//! ```
//!
//! These offsets are not guaranteed to be in order, and in fact `progs.dat` usually has the string
//! section first. Offsets are in bytes from the beginning of the file.
//!
//! ## String data
//!
//! The string data block is located at the offset given by `string_offset` and consists of a series
//! of null-terminated ASCII strings laid end-to-end. The first string is always the empty string,
//! i.e. the first byte is always the null byte. The total size in bytes of the string data is given
//! by `string_count`.
//!
//! ## Statements
//!
//! The statement table is located at the offset given by `statement_offset` and consists of
//! `statement_count` 8-byte instructions of the form
//!
//! ```text
//! opcode: u16,
//! arg1: i16,
//! arg2: i16,
//! arg3: i16,
//! ```
//!
//! Not every opcode uses three arguments, but all statements have space for three arguments anyway,
//! probably for simplicity. The semantics of these arguments differ depending on the opcode.
//!
//! ## Function Definitions
//!
//! Function definitions contain both high-level information about the function (name and source
//! file) and low-level information necessary to execute it (entry point, argument count, etc).
//! Functions are stored on disk as follows:
//!
//! ```text
//! statement_id: i32,     // index of first statement; negatives are built-in functions
//! arg_start: i32,        // address to store/load first argument
//! local_count: i32,      // number of local variables on the stack
//! profile: i32,          // incremented every time function called
//! fnc_name_ofs: i32,     // offset of function name in string table
//! srcfile_name_ofs: i32, // offset of source file name in string table
//! arg_count: i32,        // number of arguments (max. 8)
//! arg_sizes: [u8; 8],    // sizes of each argument
//! ```

mod functions;
mod globals;
mod ops;

use std::cell::Cell;
use std::cell::RefCell;
use std::collections::HashMap;
use std::convert::TryInto;
use std::error::Error;
use std::fmt;
use std::io::BufReader;
use std::io::Cursor;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::rc::Rc;

use engine;
use entity::EntityList;

use byteorder::LittleEndian;
use byteorder::ReadBytesExt;
use cgmath::Vector3;
use num::FromPrimitive;

use self::ops::Opcode;
use self::functions::FunctionDef;
pub use self::functions::FunctionId;
use self::functions::FunctionKind;
use self::functions::Functions;
use self::functions::MAX_ARGS;
use self::functions::Statement;
use self::globals::Globals;
use self::globals::GlobalsStatic;
use self::globals::GLOBAL_DYNAMIC_START;
use self::globals::GLOBAL_RESERVED_COUNT;
use self::globals::GLOBAL_STATIC_COUNT;
use self::globals::GLOBAL_STATIC_START;

const VERSION: i32 = 6;
const CRC: i32 = 5927;
const MAX_CALL_STACK_DEPTH: usize = 32;
const MAX_LOCAL_STACK_DEPTH: usize = 2048;
const LUMP_COUNT: usize = 6;
const SAVE_GLOBAL: u16 = 1 << 15;

// the on-disk size of a bytecode statement
const STATEMENT_SIZE: usize = 8;

// the on-disk size of a function declaration
const FUNCTION_SIZE: usize = 36;

// the on-disk size of a global or field definition
const DEF_SIZE: usize = 8;

const MAX_ENTITIES: usize = 600;

#[derive(Debug)]
pub enum ProgsError {
    Io(::std::io::Error),
    Other(String),
}

impl ProgsError {
    pub fn with_msg<S>(msg: S) -> Self
    where
        S: AsRef<str>,
    {
        ProgsError::Other(msg.as_ref().to_owned())
    }
}

impl fmt::Display for ProgsError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            ProgsError::Io(ref err) => err.fmt(f),
            ProgsError::Other(ref msg) => write!(f, "{}", msg),
        }
    }
}

impl Error for ProgsError {
    fn description(&self) -> &str {
        match *self {
            ProgsError::Io(ref err) => err.description(),
            ProgsError::Other(ref msg) => &msg,
        }
    }
}

impl From<::std::io::Error> for ProgsError {
    fn from(error: ::std::io::Error) -> Self {
        ProgsError::Io(error)
    }
}

#[derive(Copy, Clone, Debug, Default, Eq, Hash, PartialEq)]
#[repr(C)]
pub struct StringId(usize);

impl TryInto<i32> for StringId {
    type Error = ProgsError;

    fn try_into(self) -> Result<i32, Self::Error> {
        if self.0 > ::std::i32::MAX as usize {
            Err(ProgsError::with_msg("string id out of i32 range"))
        } else {
            Ok(self.0 as i32)
        }
    }
}

impl StringId {
    pub fn new() -> StringId {
        StringId(0)
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq)]
#[repr(C)]
pub struct EntityId(pub i32);

#[derive(Copy, Clone, Debug, Default, PartialEq)]
#[repr(C)]
pub struct FieldAddr(pub i32);

#[derive(Copy, Clone, Debug, Default, PartialEq)]
#[repr(C)]
pub struct EntityFieldAddr {
    pub entity_id: usize,
    pub field_addr: usize,
}

enum LumpId {
    Statements = 0,
    GlobalDefs = 1,
    Fielddefs = 2,
    Functions = 3,
    Strings = 4,
    Globals = 5,
}

#[derive(Copy, Clone, Debug, FromPrimitive, PartialEq)]
#[repr(u16)]
pub enum Type {
    QVoid = 0,
    QString = 1,
    QFloat = 2,
    QVector = 3,
    QEntity = 4,
    QField = 5,
    QFunction = 6,
    QPointer = 7,
}

pub enum Value {
    QString(StringId),
    QFloat(f32),
    QEntity(EntityId),
    QField(FieldAddr),
    QFunction(FunctionId),
}

#[derive(Copy, Clone, Debug)]
struct Lump {
    offset: usize,
    count: usize,
}

#[derive(Debug)]
pub struct GlobalDef {
    save: bool,
    type_: Type,
    offset: u16,
    name_id: StringId,
}

#[derive(Debug)]
pub struct FieldDef {
    pub type_: Type,
    pub offset: u16,
    pub name_id: StringId,
}

#[derive(Debug)]
pub struct StringTable {
    byte_count: Cell<usize>,
    lump: String,
    table: RefCell<HashMap<StringId, String>>,
}

impl StringTable {
    pub fn new(data: Vec<u8>) -> StringTable {
        let mut table = HashMap::new();

        StringTable {
            byte_count: Cell::new(data.len()),
            lump: String::from_utf8(data).unwrap(),
            table: RefCell::new(table),
        }
    }

    pub fn find<S>(&self, target: S) -> Option<StringId>
    where
        S: AsRef<str>,
    {
        let target = target.as_ref();

        if let Some(id) = self.lump.find(target) {
            return Some(StringId(id));
        }

        match self.table.borrow().iter().find(|&(_, &ref v)| v == target) {
            Some((k, _)) => Some(*k),
            None => None,
        }
    }

    pub fn get(&self, id: StringId) -> Option<String> {
        if id.0 < self.lump.len() {
            let mut nul_byte = id.0;

            for i in id.0..self.lump.len() {
                if self.lump.as_bytes()[i] == 0 {
                    nul_byte = i;
                    break;
                }
            }

            Some(
                ::std::str::from_utf8(&self.lump.as_bytes()[id.0..nul_byte])
                    .unwrap()
                    .to_owned(),
            )
        } else {
            match self.table.borrow().get(&id) {
                Some(s) => Some(s.to_owned()),
                None => None,
            }
        }
    }

    pub fn insert<S>(&self, value: S) -> StringId
    where
        S: AsRef<str>,
    {
        let s = value.as_ref().to_owned();
        let id = StringId(self.byte_count.get());
        let len = s.len();

        println!("StringTable: inserting {}", s);
        match self.table.borrow_mut().insert(id, s) {
            Some(_) => panic!("duplicate ID in string table"),
            None => (),
        }

        self.byte_count.set(self.byte_count.get() + len);

        id
    }

    pub fn id_from_i32(&self, value: i32) -> Result<StringId, ProgsError> {
        if value < 0 {
            return Err(ProgsError::with_msg("id < 0"));
        }

        let id = StringId(value as usize);

        if id.0 < self.lump.len() || self.table.borrow().contains_key(&id) {
            Ok(id)
        } else {
            Err(ProgsError::with_msg(format!("no string with ID {}", value)))
        }
    }
}

/// Loads all data from a `progs.dat` file.
///
/// This returns objects representing the necessary context to execute QuakeC bytecode.
pub fn load(data: &[u8]) -> Result<(Functions, Globals, EntityList), ProgsError> {
    let mut src = BufReader::new(Cursor::new(data));
    assert!(src.read_i32::<LittleEndian>()? == VERSION);
    assert!(src.read_i32::<LittleEndian>()? == CRC);

    let mut lumps = [Lump {
        offset: 0,
        count: 0,
    }; LUMP_COUNT];

    for l in 0..lumps.len() as usize {
        lumps[l] = Lump {
            offset: src.read_i32::<LittleEndian>()? as usize,
            count: src.read_i32::<LittleEndian>()? as usize,
        };

        debug!("{:?}: {:?}", l, lumps[l]);
    }

    let ent_addr_count = src.read_i32::<LittleEndian>()? as usize;
    debug!("Field count: {}", ent_addr_count);

    // Read string data and construct StringTable

    let string_lump = &lumps[LumpId::Strings as usize];
    src.seek(SeekFrom::Start(string_lump.offset as u64))?;
    let mut strings = Vec::new();
    (&mut src).take(string_lump.count as u64).read_to_end(
        &mut strings,
    )?;
    let string_table = Rc::new(StringTable::new(strings));

    assert_eq!(
        src.seek(SeekFrom::Current(0))?,
        src.seek(SeekFrom::Start(
            (string_lump.offset + string_lump.count) as u64,
        ))?
    );

    // Read function definitions and statements and construct Functions

    let function_lump = &lumps[LumpId::Functions as usize];
    src.seek(SeekFrom::Start(function_lump.offset as u64))?;
    let mut function_defs = Vec::with_capacity(function_lump.count);
    for i in 0..function_lump.count {
        assert_eq!(
            src.seek(SeekFrom::Current(0))?,
            src.seek(SeekFrom::Start(
                (function_lump.offset + i * FUNCTION_SIZE) as u64,
            ))?
        );

        let kind = match src.read_i32::<LittleEndian>()? {
            x if x < 0 => FunctionKind::BuiltIn(-x as usize),
            x => FunctionKind::QuakeC(x as usize),
        };

        let arg_start = src.read_i32::<LittleEndian>()?;
        let locals = src.read_i32::<LittleEndian>()?;

        // throw away profile variable
        let _ = src.read_i32::<LittleEndian>()?;

        let name_id = string_table.id_from_i32(src.read_i32::<LittleEndian>()?)?;
        let srcfile_id = string_table.id_from_i32(src.read_i32::<LittleEndian>()?)?;

        let argc = src.read_i32::<LittleEndian>()?;
        let mut argsz = [0; MAX_ARGS];
        src.read(&mut argsz)?;

        function_defs.push(FunctionDef {
            kind: kind,
            arg_start: arg_start as usize,
            locals: locals as usize,
            name_id,
            srcfile_id,
            argc: argc as usize,
            argsz,
        });
    }

    assert_eq!(
        src.seek(SeekFrom::Current(0))?,
        src.seek(SeekFrom::Start(
            (function_lump.offset + function_lump.count * FUNCTION_SIZE) as
                u64,
        ))?
    );

    let statement_lump = &lumps[LumpId::Statements as usize];
    src.seek(SeekFrom::Start(statement_lump.offset as u64))?;
    let mut statements = Vec::with_capacity(statement_lump.count);
    for _ in 0..statement_lump.count {
        statements.push(Statement::new(
            src.read_i16::<LittleEndian>()?,
            src.read_i16::<LittleEndian>()?,
            src.read_i16::<LittleEndian>()?,
            src.read_i16::<LittleEndian>()?,
        )?);
    }

    assert_eq!(
        src.seek(SeekFrom::Current(0))?,
        src.seek(SeekFrom::Start(
            (statement_lump.offset + statement_lump.count * STATEMENT_SIZE) as
                u64,
        ))?
    );

    let functions = Functions {
        string_table: string_table.clone(),
        defs: function_defs.into_boxed_slice(),
        statements: statements.into_boxed_slice(),
    };

    let globaldef_lump = &lumps[LumpId::GlobalDefs as usize];
    src.seek(SeekFrom::Start(globaldef_lump.offset as u64))?;
    let mut globaldefs = Vec::new();
    for _ in 0..globaldef_lump.count {
        let type_ = src.read_u16::<LittleEndian>()?;
        let offset = src.read_u16::<LittleEndian>()?;
        let name_id = string_table.id_from_i32(src.read_i32::<LittleEndian>()?)?;
        globaldefs.push(GlobalDef {
            save: type_ & SAVE_GLOBAL != 0,
            type_: Type::from_u16(type_ & !SAVE_GLOBAL).unwrap(),
            offset,
            name_id,
        });
    }

    assert_eq!(
        src.seek(SeekFrom::Current(0))?,
        src.seek(SeekFrom::Start(
            (globaldef_lump.offset +
                 globaldef_lump.count * DEF_SIZE) as u64,
        ))?
    );

    let fielddef_lump = &lumps[LumpId::Fielddefs as usize];
    src.seek(SeekFrom::Start(fielddef_lump.offset as u64))?;
    let mut field_defs = Vec::new();
    for _ in 0..fielddef_lump.count {
        let type_ = src.read_u16::<LittleEndian>()?;
        let offset = src.read_u16::<LittleEndian>()?;
        let name_id = string_table.id_from_i32(src.read_i32::<LittleEndian>()?)?;

        if type_ & SAVE_GLOBAL != 0 {
            return Err(ProgsError::with_msg(
                "Save flag not allowed in field definitions",
            ));
        }
        field_defs.push(FieldDef {
            type_: Type::from_u16(type_).unwrap(),
            offset,
            name_id,
        });
    }

    assert_eq!(
        src.seek(SeekFrom::Current(0))?,
        src.seek(SeekFrom::Start(
            (fielddef_lump.offset +
                 fielddef_lump.count * DEF_SIZE) as u64,
        ))?
    );

    let globals_lump = &lumps[LumpId::Globals as usize];
    src.seek(SeekFrom::Start(globals_lump.offset as u64))?;

    if globals_lump.count < GLOBAL_STATIC_COUNT {
        return Err(ProgsError::with_msg(
            "Global count lower than static global count",
        ));
    }

    let mut addrs = Vec::with_capacity(globals_lump.count);
    for _ in 0..globals_lump.count {
        let mut block = [0; 4];
        src.read(&mut block)?;

        // TODO: handle endian conversions (BigEndian systems should use BigEndian internally)
        addrs.push(block);
    }

    assert_eq!(
        src.seek(SeekFrom::Current(0))?,
        src.seek(SeekFrom::Start(
            (globals_lump.offset + globals_lump.count * 4) as u64,
        ))?
    );

    let globals = Globals {
        string_table: string_table.clone(),
        defs: globaldefs.into_boxed_slice(),
        addrs: addrs.into_boxed_slice(),
    };

    let entity_list = EntityList::new(
        ent_addr_count,
        string_table.clone(),
        field_defs.into_boxed_slice(),
    );

    Ok((functions, globals, entity_list))
}

#[derive(Debug)]
struct StackFrame {
    instr_id: usize,
    func_id: FunctionId,
}

pub struct ExecutionContext {
    pc: usize,
    current_function: FunctionId,
    call_stack: Vec<StackFrame>,
    local_stack: Vec<Value>,
}

impl ExecutionContext {
    fn enter_function(&mut self, functions: &Functions, globals: &mut Globals, f: FunctionId) {
        let def = functions.get_def(f).unwrap();

        // save stack frame
        self.call_stack.push(StackFrame {
            instr_id: self.pc,
            func_id: self.current_function,
        });

        // check call stack overflow
        if self.call_stack.len() >= MAX_CALL_STACK_DEPTH {
            panic!("call stack overflow");
        }

        // preemptively check local stack overflow
        if self.local_stack.len() + def.locals > MAX_LOCAL_STACK_DEPTH {
            panic!("local stack overflow");
        }

        // save locals to stack
        for i in 0..def.locals {
            let addr = def.arg_start + i;
            match globals.type_at_addr(addr).unwrap() {
                Some(Type::QFloat) |
                Some(Type::QVector) |
                None => {
                    self.local_stack.push(Value::QFloat(
                        globals.get_float(addr as i16).unwrap(),
                    ));
                }
                Some(Type::QString) => {
                    self.local_stack.push(Value::QString(
                        globals.get_string_id(addr as i16).unwrap(),
                    ));
                }
                Some(Type::QEntity) => {
                    self.local_stack.push(Value::QEntity(
                        globals.get_entity_id(addr as i16).unwrap(),
                    ))
                }
                Some(Type::QFunction) => {
                    self.local_stack.push(Value::QFunction(
                        globals.get_function_id(addr as i16).unwrap(),
                    ))
                }
                _ => unimplemented!(),
            }
        }
    }

    fn leave_function(&self) {}

    pub fn execute_program(
        &self,
        functions: &Functions,
        globals: &mut Globals,
        entities: &mut EntityList,
        f: FunctionId,
    ) {
        let def = functions.get_def(f).unwrap();
    }
}

// run through all statements and see if we crash. elegant!
pub fn validate(functions: &Functions, globals: &mut Globals, entities: &mut EntityList) {
    'functions: for f in 0..functions.defs.len() {
        let name = functions
            .string_table
            .get(functions.defs[f].name_id)
            .unwrap()
            .to_owned();

        let first = match functions.defs[f].kind {
            FunctionKind::BuiltIn(_) => continue,
            FunctionKind::QuakeC(s) => s,
        };

        println!("FUNCTION {}: {}", f, name);

        for i in first..functions.statements.len() {
            let op = functions.statements[i].opcode;
            let arg1 = functions.statements[i].arg1;
            let arg2 = functions.statements[i].arg2;
            let arg3 = functions.statements[i].arg3;
            println!(
                "    {:<9} {:>5} {:>5} {:>5}",
                format!("{:?}", op),
                arg1,
                arg2,
                arg3
            );
            match op {
                Opcode::MulF => mul_f(globals, arg1, arg2, arg3).unwrap(),
                Opcode::MulV => mul_v(globals, arg1, arg2, arg3).unwrap(),
                Opcode::MulFV => mul_fv(globals, arg1, arg2, arg3).unwrap(),
                Opcode::MulVF => mul_vf(globals, arg1, arg2, arg3).unwrap(),
                Opcode::Div => div(globals, arg1, arg2, arg3).unwrap(),
                Opcode::AddF => add_f(globals, arg1, arg2, arg3).unwrap(),
                Opcode::AddV => add_v(globals, arg1, arg2, arg3).unwrap(),
                Opcode::SubF => sub_f(globals, arg1, arg2, arg3).unwrap(),
                Opcode::SubV => sub_v(globals, arg1, arg2, arg3).unwrap(),
                Opcode::EqF => eq_f(globals, arg1, arg2, arg3).unwrap(),
                Opcode::EqV => eq_v(globals, arg1, arg2, arg3).unwrap(),
                Opcode::EqS => eq_s(globals, arg1, arg2, arg3).unwrap(),
                Opcode::EqEnt => eq_ent(globals, arg1, arg2, arg3).unwrap(),
                Opcode::EqFnc => eq_fnc(globals, arg1, arg2, arg3).unwrap(),
                Opcode::NeF => ne_f(globals, arg1, arg2, arg3).unwrap(),
                Opcode::NeV => ne_v(globals, arg1, arg2, arg3).unwrap(),
                Opcode::NeS => ne_s(globals, arg1, arg2, arg3).unwrap(),
                Opcode::NeEnt => ne_ent(globals, arg1, arg2, arg3).unwrap(),
                Opcode::NeFnc => ne_fnc(globals, arg1, arg2, arg3).unwrap(),
                Opcode::Le => le(globals, arg1, arg2, arg3).unwrap(),
                Opcode::Ge => ge(globals, arg1, arg2, arg3).unwrap(),
                Opcode::Lt => lt(globals, arg1, arg2, arg3).unwrap(),
                Opcode::Gt => gt(globals, arg1, arg2, arg3).unwrap(),
                Opcode::LoadF => load_f(globals, entities, arg1, arg2, arg3).unwrap(),
                Opcode::LoadV => load_v(globals, entities, arg1, arg2, arg3).unwrap(),
                Opcode::LoadS => load_s(globals, entities, arg1, arg2, arg3).unwrap(),
                Opcode::LoadEnt => load_ent(globals, entities, arg1, arg2, arg3).unwrap(),
                Opcode::LoadFld => panic!("load_fld not implemented"),
                Opcode::LoadFnc => load_fnc(globals, entities, arg1, arg2, arg3).unwrap(),
                Opcode::Address => address(globals, entities, arg1, arg2, arg3).unwrap(),
                Opcode::StoreF => store_f(globals, arg1, arg2, arg3).unwrap(),
                Opcode::StoreV => store_v(globals, arg1, arg2, arg3).unwrap(),
                Opcode::StoreS => store_s(globals, arg1, arg2, arg3).unwrap(),
                Opcode::StoreEnt => store_ent(globals, arg1, arg2, arg3).unwrap(),
                Opcode::StoreFld => store_fld(globals, arg1, arg2, arg3).unwrap(),
                Opcode::StoreFnc => store_fnc(globals, arg1, arg2, arg3).unwrap(),
                Opcode::StorePF => storep_f(globals, entities, arg1, arg2, arg3).unwrap(),
                Opcode::StorePV => storep_v(globals, entities, arg1, arg2, arg3).unwrap(),
                Opcode::StorePS => storep_s(globals, entities, arg1, arg2, arg3).unwrap(),
                Opcode::StorePEnt => storep_ent(globals, entities, arg1, arg2, arg3).unwrap(),
                Opcode::StorePFld => panic!("storep_fld not implemented"),
                Opcode::StorePFnc => storep_fnc(globals, entities, arg1, arg2, arg3).unwrap(),
                // Opcode::Return
                Opcode::NotF => not_f(globals, arg1, arg2, arg3).unwrap(),
                Opcode::NotV => not_v(globals, arg1, arg2, arg3).unwrap(),
                Opcode::NotS => not_s(globals, arg1, arg2, arg3).unwrap(),
                Opcode::NotEnt => not_ent(globals, arg1, arg2, arg3).unwrap(),
                Opcode::NotFnc => not_fnc(globals, arg1, arg2, arg3).unwrap(),
                // Opcode::If
                // Opcode::IfNot
                // Opcode::Call0
                // Opcode::Call1
                // Opcode::Call2
                // Opcode::Call3
                // Opcode::Call4
                // Opcode::Call5
                // Opcode::Call6
                // Opcode::Call7
                // Opcode::Call8
                // Opcode::State
                // Opcode::Goto
                Opcode::And => and(globals, arg1, arg2, arg3).unwrap(),
                Opcode::Or => or(globals, arg1, arg2, arg3).unwrap(),
                Opcode::BitAnd => bit_and(globals, arg1, arg2, arg3).unwrap(),
                Opcode::BitOr => bit_or(globals, arg1, arg2, arg3).unwrap(),

                Opcode::Done | Opcode::Return => continue 'functions,
                _ => (),
            }
        }
    }
}


// MUL_F: Float multiplication
fn mul_f(globals: &mut Globals, f1_id: i16, f2_id: i16, prod_id: i16) -> Result<(), ProgsError> {
    let f1 = globals.get_float(f1_id)?;
    let f2 = globals.get_float(f2_id)?;
    globals.put_float(f1 * f2, prod_id)
}

// MUL_V: Vector dot-product
fn mul_v(globals: &mut Globals, v1_id: i16, v2_id: i16, dot_id: i16) -> Result<(), ProgsError> {
    let v1 = globals.get_vector(v1_id)?;
    let v2 = globals.get_vector(v2_id)?;

    let mut dot = 0.0;

    for c in 0..3 {
        dot += v1[c] * v2[c];
    }
    globals.put_float(dot, dot_id)
}

// MUL_FV: Component-wise multiplication of vector by scalar
fn mul_fv(globals: &mut Globals, f_id: i16, v_id: i16, prod_id: i16) -> Result<(), ProgsError> {
    let f = globals.get_float(f_id)?;
    let v = globals.get_vector(v_id)?;

    let mut prod = [0.0; 3];
    for c in 0..prod.len() {
        prod[c] = v[c] * f;
    }

    globals.put_vector(prod, prod_id)
}

// MUL_VF: Component-wise multiplication of vector by scalar
fn mul_vf(globals: &mut Globals, v_id: i16, f_id: i16, prod_id: i16) -> Result<(), ProgsError> {
    let v = globals.get_vector(v_id)?;
    let f = globals.get_float(f_id)?;

    let mut prod = [0.0; 3];
    for c in 0..prod.len() {
        prod[c] = v[c] * f;
    }

    globals.put_vector(prod, prod_id)
}

// DIV: Float division
fn div(globals: &mut Globals, f1_id: i16, f2_id: i16, quot_id: i16) -> Result<(), ProgsError> {
    let f1 = globals.get_float(f1_id)?;
    let f2 = globals.get_float(f2_id)?;
    globals.put_float(f1 / f2, quot_id)
}

// ADD_F: Float addition
fn add_f(globals: &mut Globals, f1_ofs: i16, f2_ofs: i16, sum_ofs: i16) -> Result<(), ProgsError> {
    let f1 = globals.get_float(f1_ofs)?;
    let f2 = globals.get_float(f2_ofs)?;
    globals.put_float(f1 + f2, sum_ofs)
}

// ADD_V: Vector addition
fn add_v(globals: &mut Globals, v1_id: i16, v2_id: i16, sum_id: i16) -> Result<(), ProgsError> {
    let v1 = globals.get_vector(v1_id)?;
    let v2 = globals.get_vector(v2_id)?;

    let mut sum = [0.0; 3];
    for c in 0..sum.len() {
        sum[c] = v1[c] + v2[c];
    }

    globals.put_vector(sum, sum_id)
}

// SUB_F: Float subtraction
fn sub_f(globals: &mut Globals, f1_id: i16, f2_id: i16, diff_id: i16) -> Result<(), ProgsError> {
    let f1 = globals.get_float(f1_id)?;
    let f2 = globals.get_float(f2_id)?;
    globals.put_float(f1 - f2, diff_id)
}

// SUB_V: Vector subtraction
fn sub_v(globals: &mut Globals, v1_id: i16, v2_id: i16, diff_id: i16) -> Result<(), ProgsError> {
    let v1 = globals.get_vector(v1_id)?;
    let v2 = globals.get_vector(v2_id)?;

    let mut diff = [0.0; 3];
    for c in 0..diff.len() {
        diff[c] = v1[c] - v2[c];
    }

    globals.put_vector(diff, diff_id)
}

// EQ_F: Test equality of two floats
fn eq_f(globals: &mut Globals, f1_id: i16, f2_id: i16, eq_id: i16) -> Result<(), ProgsError> {
    let f1 = globals.get_float(f1_id)?;
    let f2 = globals.get_float(f2_id)?;
    globals.put_float(
        match f1 == f2 {
            true => 1.0,
            false => 0.0,
        },
        eq_id,
    )
}

// EQ_V: Test equality of two vectors
fn eq_v(globals: &mut Globals, v1_id: i16, v2_id: i16, eq_id: i16) -> Result<(), ProgsError> {
    let v1 = globals.get_vector(v1_id)?;
    let v2 = globals.get_vector(v2_id)?;
    globals.put_float(
        match v1 == v2 {
            true => 1.0,
            false => 0.0,
        },
        eq_id,
    )
}

// EQ_S: Test equality of two strings
fn eq_s(globals: &mut Globals, s1_ofs: i16, s2_ofs: i16, eq_ofs: i16) -> Result<(), ProgsError> {
    if s1_ofs < 0 || s2_ofs < 0 {
        return Err(ProgsError::with_msg("eq_s: negative string offset"));
    }

    if s1_ofs == s2_ofs || globals.get_string_id(s1_ofs)? == globals.get_string_id(s2_ofs)? {
        globals.put_float(1.0, eq_ofs)
    } else {
        globals.put_float(0.0, eq_ofs)
    }
}

// EQ_ENT: Test equality of two entities (by identity)
fn eq_ent(globals: &mut Globals, e1_ofs: i16, e2_ofs: i16, eq_ofs: i16) -> Result<(), ProgsError> {
    let e1 = globals.get_entity_id(e1_ofs)?;
    let e2 = globals.get_entity_id(e2_ofs)?;

    globals.put_float(
        match e1 == e2 {
            true => 1.0,
            false => 0.0,
        },
        eq_ofs,
    )
}

// EQ_FNC: Test equality of two functions (by identity)
fn eq_fnc(globals: &mut Globals, f1_ofs: i16, f2_ofs: i16, eq_ofs: i16) -> Result<(), ProgsError> {
    let f1 = globals.get_function_id(f1_ofs)?;
    let f2 = globals.get_function_id(f2_ofs)?;

    globals.put_float(
        match f1 == f2 {
            true => 1.0,
            false => 0.0,
        },
        eq_ofs,
    )
}

// NE_F: Test inequality of two floats
fn ne_f(globals: &mut Globals, f1_ofs: i16, f2_ofs: i16, ne_ofs: i16) -> Result<(), ProgsError> {
    let f1 = globals.get_float(f1_ofs)?;
    let f2 = globals.get_float(f2_ofs)?;
    globals.put_float(
        match f1 != f2 {
            true => 1.0,
            false => 0.0,
        },
        ne_ofs,
    )
}

// NE_V: Test inequality of two vectors
fn ne_v(globals: &mut Globals, v1_ofs: i16, v2_ofs: i16, ne_ofs: i16) -> Result<(), ProgsError> {
    let v1 = globals.get_vector(v1_ofs)?;
    let v2 = globals.get_vector(v2_ofs)?;
    globals.put_float(
        match v1 != v2 {
            true => 1.0,
            false => 0.0,
        },
        ne_ofs,
    )
}

// NE_S: Test inequality of two strings
fn ne_s(globals: &mut Globals, s1_ofs: i16, s2_ofs: i16, ne_ofs: i16) -> Result<(), ProgsError> {
    if s1_ofs < 0 || s2_ofs < 0 {
        return Err(ProgsError::with_msg("eq_s: negative string offset"));
    }

    if s1_ofs != s2_ofs && globals.get_string_id(s1_ofs)? != globals.get_string_id(s2_ofs)? {
        globals.put_float(1.0, ne_ofs)
    } else {
        globals.put_float(0.0, ne_ofs)
    }
}

fn ne_ent(globals: &mut Globals, e1_ofs: i16, e2_ofs: i16, ne_ofs: i16) -> Result<(), ProgsError> {
    let e1 = globals.get_entity_id(e1_ofs)?;
    let e2 = globals.get_entity_id(e2_ofs)?;

    globals.put_float(
        match e1 != e2 {
            true => 1.0,
            false => 0.0,
        },
        ne_ofs,
    )
}

fn ne_fnc(globals: &mut Globals, f1_ofs: i16, f2_ofs: i16, ne_ofs: i16) -> Result<(), ProgsError> {
    let f1 = globals.get_function_id(f1_ofs)?;
    let f2 = globals.get_function_id(f2_ofs)?;

    globals.put_float(
        match f1 != f2 {
            true => 1.0,
            false => 0.0,
        },
        ne_ofs,
    )
}

// LE: Less than or equal to comparison
fn le(globals: &mut Globals, f1_ofs: i16, f2_ofs: i16, le_ofs: i16) -> Result<(), ProgsError> {
    let f1 = globals.get_float(f1_ofs)?;
    let f2 = globals.get_float(f2_ofs)?;
    globals.put_float(
        match f1 <= f2 {
            true => 1.0,
            false => 0.0,
        },
        le_ofs,
    )
}

// GE: Greater than or equal to comparison
fn ge(globals: &mut Globals, f1_ofs: i16, f2_ofs: i16, ge_ofs: i16) -> Result<(), ProgsError> {
    let f1 = globals.get_float(f1_ofs)?;
    let f2 = globals.get_float(f2_ofs)?;
    globals.put_float(
        match f1 >= f2 {
            true => 1.0,
            false => 0.0,
        },
        ge_ofs,
    )
}

// LT: Less than comparison
fn lt(globals: &mut Globals, f1_ofs: i16, f2_ofs: i16, lt_ofs: i16) -> Result<(), ProgsError> {
    let f1 = globals.get_float(f1_ofs)?;
    let f2 = globals.get_float(f2_ofs)?;
    globals.put_float(
        match f1 < f2 {
            true => 1.0,
            false => 0.0,
        },
        lt_ofs,
    )
}

// GT: Greater than comparison
fn gt(globals: &mut Globals, f1_ofs: i16, f2_ofs: i16, gt_ofs: i16) -> Result<(), ProgsError> {
    let f1 = globals.get_float(f1_ofs)?;
    let f2 = globals.get_float(f2_ofs)?;
    globals.put_float(
        match f1 > f2 {
            true => 1.0,
            false => 0.0,
        },
        gt_ofs,
    )
}

// LOAD_F: load float field from entity
fn load_f(
    globals: &mut Globals,
    entity_list: &EntityList,
    e_ofs: i16,
    e_f: i16,
    dest_ofs: i16,
) -> Result<(), ProgsError> {
    let ent_id = globals.get_entity_id(e_ofs)?;

    let fld_ofs = globals.get_field_addr(e_f)?;

    let f = entity_list.try_get_entity(ent_id.0 as usize)?.get_float(
        fld_ofs.0 as
            i16,
    )?;
    globals.put_float(f, dest_ofs)
}

// LOAD_V: load vector field from entity
fn load_v(
    globals: &mut Globals,
    entity_list: &EntityList,
    ent_id_addr: i16,
    ent_vector_addr: i16,
    dest_addr: i16,
) -> Result<(), ProgsError> {
    let ent_id = globals.get_entity_id(ent_id_addr)?;
    let ent_vector = globals.get_field_addr(ent_vector_addr)?;
    let v = entity_list.try_get_entity(ent_id.0 as usize)?.get_vector(
        ent_vector.0 as
            i16,
    )?;
    globals.put_vector(v, dest_addr)
}

fn load_s(
    globals: &mut Globals,
    entity_list: &EntityList,
    ent_id_addr: i16,
    ent_string_id_addr: i16,
    dest_addr: i16,
) -> Result<(), ProgsError> {
    let ent_id = globals.get_entity_id(ent_id_addr)?;
    let ent_string_id = globals.get_field_addr(ent_string_id_addr)?;
    let s = entity_list
        .try_get_entity(ent_id.0 as usize)?
        .get_string_id(ent_string_id.0 as i16)?;
    globals.put_string_id(s, dest_addr)
}

fn load_ent(
    globals: &mut Globals,
    entity_list: &EntityList,
    ent_id_addr: i16,
    ent_entity_id_addr: i16,
    dest_addr: i16,
) -> Result<(), ProgsError> {
    let ent_id = globals.get_entity_id(ent_id_addr)?;
    let ent_entity_id = globals.get_field_addr(ent_entity_id_addr)?;
    let e = entity_list
        .try_get_entity(ent_id.0 as usize)?
        .get_entity_id(ent_entity_id.0 as i16)?;
    globals.put_entity_id(e, dest_addr)
}

fn load_fnc(
    globals: &mut Globals,
    entity_list: &EntityList,
    ent_id_addr: i16,
    ent_function_id_addr: i16,
    dest_addr: i16,
) -> Result<(), ProgsError> {
    let ent_id = globals.get_entity_id(ent_id_addr)?;
    let fnc_function_id = globals.get_field_addr(ent_function_id_addr)?;
    let f = entity_list
        .try_get_entity(ent_id.0 as usize)?
        .get_function_id(fnc_function_id.0 as i16)?;
    globals.put_function_id(f, dest_addr)
}

fn address(
    globals: &mut Globals,
    entity_list: &EntityList,
    ent_id_addr: i16,
    fld_addr_addr: i16,
    dest_addr: i16,
) -> Result<(), ProgsError> {
    let ent_id = globals.get_entity_id(ent_id_addr)?;
    let fld_addr = globals.get_field_addr(fld_addr_addr)?;
    globals.put_entity_field(
        entity_list.ent_fld_addr_to_i32(EntityFieldAddr {
            entity_id: ent_id.0 as usize,
            field_addr: fld_addr.0 as usize,
        }),
        dest_addr,
    )?;

    Ok(())
}

// STORE_F
fn store_f(
    globals: &mut Globals,
    src_ofs: i16,
    dest_ofs: i16,
    unused: i16,
) -> Result<(), ProgsError> {
    if unused != 0 {
        return Err(ProgsError::with_msg("Nonzero arg3 to STORE_F"));
    }

    let f = globals.get_float(src_ofs)?;
    globals.put_float(f, dest_ofs)
}

// STORE_V
fn store_v(
    globals: &mut Globals,
    src_ofs: i16,
    dest_ofs: i16,
    unused: i16,
) -> Result<(), ProgsError> {
    if unused != 0 {
        return Err(ProgsError::with_msg("Nonzero arg3 to STORE_V"));
    }

    if dest_ofs > 0 && dest_ofs < GLOBAL_STATIC_START as i16 {
        // we have to use the reserved copy because STORE_V is used to copy function arguments (see
        // https://github.com/id-Software/Quake-Tools/blob/master/qcc/pr_comp.c#L362) into the global
        // argument slots.
        for c in 0..3 {
            globals.untyped_copy(
                src_ofs + c as i16,
                dest_ofs + c as i16,
            )?;
        }
    } else {
        for c in 0..3 {
            let f = globals.get_float(src_ofs + c)?;
            globals.put_float(f, dest_ofs + c)?;
        }
    }

    Ok(())
}

fn store_s(
    globals: &mut Globals,
    src_ofs: i16,
    dest_ofs: i16,
    unused: i16,
) -> Result<(), ProgsError> {
    if unused != 0 {
        return Err(ProgsError::with_msg("Nonzero arg3 to STORE_S"));
    }

    let s = globals.get_string_id(src_ofs)?;
    globals.put_string_id(s, dest_ofs)
}

fn store_ent(
    globals: &mut Globals,
    src_ofs: i16,
    dest_ofs: i16,
    unused: i16,
) -> Result<(), ProgsError> {
    if unused != 0 {
        return Err(ProgsError::with_msg("Nonzero arg3 to STORE_ENT"));
    }

    let ent = globals.get_entity_id(src_ofs)?;
    globals.put_entity_id(ent, dest_ofs)
}

fn store_fld(
    globals: &mut Globals,
    src_ofs: i16,
    dest_ofs: i16,
    unused: i16,
) -> Result<(), ProgsError> {
    if unused != 0 {
        return Err(ProgsError::with_msg("Nonzero arg3 to STORE_FLD"));
    }

    let fld = globals.get_field_addr(src_ofs)?;
    globals.put_field_addr(fld, dest_ofs)
}

fn store_fnc(
    globals: &mut Globals,
    src_ofs: i16,
    dest_ofs: i16,
    unused: i16,
) -> Result<(), ProgsError> {
    if unused != 0 {
        return Err(ProgsError::with_msg("Nonzero arg3 to STORE_FNC"));
    }

    let fnc = globals.get_function_id(src_ofs)?;
    globals.put_function_id(fnc, dest_ofs)
}

fn storep_f(
    globals: &Globals,
    entities: &mut EntityList,
    src_float_addr: i16,
    dst_ent_fld_addr: i16,
    unused: i16,
) -> Result<(), ProgsError> {
    if unused != 0 {
        return Err(ProgsError::with_msg("storep_f: nonzero arg3"));
    }

    let f = globals.get_float(src_float_addr)?;
    let ent_fld_addr = entities.ent_fld_addr_from_i32(globals.get_entity_field(dst_ent_fld_addr)?);
    entities
        .try_get_entity_mut(ent_fld_addr.entity_id)?
        .put_float(f, ent_fld_addr.field_addr as i16)
}

fn storep_v(
    globals: &mut Globals,
    entities: &mut EntityList,
    src_vector_addr: i16,
    dst_ent_fld_addr: i16,
    unused: i16,
) -> Result<(), ProgsError> {
    if unused != 0 {
        return Err(ProgsError::with_msg("storep_v: nonzero arg3"));
    }

    let v = globals.get_vector(src_vector_addr)?;
    let ent_fld_addr = entities.ent_fld_addr_from_i32(globals.get_entity_field(dst_ent_fld_addr)?);
    entities
        .try_get_entity_mut(ent_fld_addr.entity_id)?
        .put_vector(v, ent_fld_addr.field_addr as i16)
}

fn storep_s(
    globals: &Globals,
    entities: &mut EntityList,
    src_string_id_addr: i16,
    dst_ent_fld_addr: i16,
    unused: i16,
) -> Result<(), ProgsError> {
    if unused != 0 {
        return Err(ProgsError::with_msg("storep_s: nonzero arg3"));
    }

    let s = globals.get_string_id(src_string_id_addr)?;
    let ent_fld_addr = entities.ent_fld_addr_from_i32(globals.get_entity_field(dst_ent_fld_addr)?);
    entities
        .try_get_entity_mut(ent_fld_addr.entity_id)?
        .put_string_id(s, ent_fld_addr.field_addr as i16)
}

fn storep_ent(
    globals: &Globals,
    entities: &mut EntityList,
    src_entity_id_addr: i16,
    dst_ent_fld_addr: i16,
    unused: i16,
) -> Result<(), ProgsError> {
    if unused != 0 {
        return Err(ProgsError::with_msg("storep_ent: nonzero arg3"));
    }

    let e = globals.get_entity_id(src_entity_id_addr)?;
    let ent_fld_addr = entities.ent_fld_addr_from_i32(globals.get_entity_field(dst_ent_fld_addr)?);
    entities
        .try_get_entity_mut(ent_fld_addr.entity_id)?
        .put_entity_id(e, ent_fld_addr.field_addr as i16)
}

fn storep_fnc(
    globals: &Globals,
    entities: &mut EntityList,
    src_function_id_addr: i16,
    dst_ent_fld_addr: i16,
    unused: i16,
) -> Result<(), ProgsError> {
    if unused != 0 {
        return Err(ProgsError::with_msg("storep_fnc: nonzero arg3"));
    }

    let f = globals.get_function_id(src_function_id_addr)?;
    let ent_fld_addr = entities.ent_fld_addr_from_i32(globals.get_entity_field(dst_ent_fld_addr)?);
    entities
        .try_get_entity_mut(ent_fld_addr.entity_id)?
        .put_function_id(f, ent_fld_addr.field_addr as i16)
}

// NOT_F: Compare float to 0.0
fn not_f(globals: &mut Globals, f_id: i16, unused: i16, not_id: i16) -> Result<(), ProgsError> {
    if unused != 0 {
        return Err(ProgsError::with_msg("Nonzero arg2 to NOT_F"));
    }

    let f = globals.get_float(f_id)?;
    globals.put_float(
        match f == 0.0 {
            true => 1.0,
            false => 0.0,
        },
        not_id,
    )
}

// NOT_V: Compare vec to { 0.0, 0.0, 0.0 }
fn not_v(globals: &mut Globals, v_id: i16, unused: i16, not_id: i16) -> Result<(), ProgsError> {
    if unused != 0 {
        return Err(ProgsError::with_msg("Nonzero arg2 to NOT_V"));
    }

    let v = globals.get_vector(v_id)?;
    let zero_vec = [0.0; 3];
    globals.put_vector(
        match v == zero_vec {
            true => [1.0; 3],
            false => zero_vec,
        },
        not_id,
    )
}

// NOT_S: Compare string to null string
fn not_s(globals: &mut Globals, s_ofs: i16, unused: i16, not_ofs: i16) -> Result<(), ProgsError> {
    if unused != 0 {
        return Err(ProgsError::with_msg("Nonzero arg2 to NOT_S"));
    }

    if s_ofs < 0 {
        return Err(ProgsError::with_msg("not_s: negative string offset"));
    }

    let s = globals
        .string_table
        .get(globals.string_table.id_from_i32(s_ofs as i32)?)
        .unwrap()
        .to_owned();

    if s_ofs == 0 || s == "" {
        globals.put_float(1.0, not_ofs)?;
    } else {
        globals.put_float(0.0, not_ofs)?;
    }

    Ok(())
}

// NOT_FNC: Compare function to null function (0)
fn not_fnc(
    globals: &mut Globals,
    fnc_id_ofs: i16,
    unused: i16,
    not_ofs: i16,
) -> Result<(), ProgsError> {
    if unused != 0 {
        return Err(ProgsError::with_msg("Nonzero arg2 to NOT_FNC"));
    }

    let fnc_id = globals.get_function_id(fnc_id_ofs)?;
    globals.put_float(
        match fnc_id {
            FunctionId(0) => 1.0,
            _ => 0.0,
        },
        not_ofs,
    )
}

// NOT_ENT: Compare entity to null entity (0)
fn not_ent(
    globals: &mut Globals,
    ent_ofs: i16,
    unused: i16,
    not_ofs: i16,
) -> Result<(), ProgsError> {
    if unused != 0 {
        return Err(ProgsError::with_msg("Nonzero arg2 to NOT_ENT"));
    }

    let ent = globals.get_entity_id(ent_ofs)?;
    globals.put_float(
        match ent {
            EntityId(0) => 1.0,
            _ => 0.0,
        },
        not_ofs,
    )
}

// AND: Logical AND
fn and(globals: &mut Globals, f1_id: i16, f2_id: i16, and_id: i16) -> Result<(), ProgsError> {
    let f1 = globals.get_float(f1_id)?;
    let f2 = globals.get_float(f2_id)?;
    globals.put_float(
        match f1 != 0.0 && f2 != 0.0 {
            true => 1.0,
            false => 0.0,
        },
        and_id,
    )
}

// OR: Logical OR
fn or(globals: &mut Globals, f1_id: i16, f2_id: i16, or_id: i16) -> Result<(), ProgsError> {
    let f1 = globals.get_float(f1_id)?;
    let f2 = globals.get_float(f2_id)?;
    globals.put_float(
        match f1 != 0.0 || f2 != 0.0 {
            true => 1.0,
            false => 0.0,
        },
        or_id,
    )
}

// BIT_AND: Bitwise AND
fn bit_and(
    globals: &mut Globals,
    f1_ofs: i16,
    f2_ofs: i16,
    bit_and_ofs: i16,
) -> Result<(), ProgsError> {
    let f1 = globals.get_float(f1_ofs)?;
    let f2 = globals.get_float(f2_ofs)?;

    globals.put_float((f1 as i32 & f2 as i32) as f32, bit_and_ofs)
}

// BIT_OR: Bitwise OR
fn bit_or(
    globals: &mut Globals,
    f1_ofs: i16,
    f2_ofs: i16,
    bit_or_ofs: i16,
) -> Result<(), ProgsError> {
    let f1 = globals.get_float(f1_ofs)?;
    let f2 = globals.get_float(f2_ofs)?;

    globals.put_float((f1 as i32 | f2 as i32) as f32, bit_or_ofs)
}
