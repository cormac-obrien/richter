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

// TODO:
// - dynamic string handling

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

mod globals;
mod ops;

use std::error::Error;
use std::fmt;
use std::io::Cursor;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;

use entity::Entity;

use byteorder::LittleEndian;
use byteorder::ReadBytesExt;
use byteorder::WriteBytesExt;
use num::FromPrimitive;

use self::ops::Opcode;
use self::globals::Globals;

const VERSION: i32 = 6;
const CRC: i32 = 5927;
const MAX_ARGS: usize = 8;
const MAX_STACK_DEPTH: usize = 32;
const LUMP_COUNT: usize = 6;
const SAVE_GLOBAL: u16 = 1 << 15;

// the on-disk size of a bytecode statement
const STATEMENT_SIZE: usize = 8;

// the on-disk size of a function declaration
const FUNCTION_SIZE: usize = 36;

// the on-disk size of a global or field definition
const DEF_SIZE: usize = 8;

#[derive(Debug)]
pub enum ProgsError {
    Io(::std::io::Error),
    Other(String),
}

impl ProgsError {
    fn with_msg<S>(msg: S) -> Self
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

#[derive(Debug, Default)]
#[repr(C)]
pub struct StringId(pub i32);

#[derive(Debug, Default)]
#[repr(C)]
pub struct EntityId(pub i32);

#[derive(Debug, Default)]
#[repr(C)]
pub struct FunctionId(pub i32);

enum LumpId {
    Statements = 0,
    GlobalDefs = 1,
    Fielddefs = 2,
    Functions = 3,
    Strings = 4,
    Globals = 5,
    Count = 6,
}

#[derive(Copy, Clone, Debug, FromPrimitive, PartialEq)]
#[repr(u16)]
enum Type {
    QVoid = 0,
    QString = 1,
    QFloat = 2,
    QVector = 3,
    QEntity = 4,
    QField = 5,
    QFunction = 6,
    QPointer = 7,
}

#[derive(Debug)]
#[repr(C)]
struct Statement {
    opcode: Opcode,
    arg1: i16,
    arg2: i16,
    arg3: i16,
}

impl Statement {
    fn new(op: i16, arg1: i16, arg2: i16, arg3: i16) -> Result<Statement, ProgsError> {
        let opcode = match Opcode::from_i16(op) {
            Some(o) => o,
            None => return Err(ProgsError::with_msg(format!("Bad opcode 0x{:x}", op))),
        };

        Ok(Statement {
            opcode: opcode,
            arg1: arg1,
            arg2: arg2,
            arg3: arg3,
        })
    }
}

#[derive(Debug)]
enum FunctionKind {
    BuiltIn(usize),
    QuakeC(usize),
}

#[derive(Debug)]
struct Function {
    kind: FunctionKind,
    arg_start: usize,
    locals: usize,
    name_ofs: i32,
    srcfile_ofs: i32,
    argc: usize,
    argsz: [u8; MAX_ARGS],
}

#[derive(Debug)]
struct StackFrame {
    instr_id: i32,
    func_id: u32,
}

#[derive(Copy, Clone, Debug)]
struct Lump {
    offset: usize,
    count: usize,
}

#[derive(Debug)]
#[repr(C)]
struct Def {
    save: bool,
    type_: Type,
    offset: u16,
    name_ofs: i32,
}

pub struct ProgsLoader {
    lumps: [Lump; LumpId::Count as usize],
    ent_field_count: usize,
    strings: Vec<u8>,
    globaldefs: Vec<Def>,
    globaldef_offsets: Vec<usize>,
    fielddefs: Vec<Def>,
    fielddef_offsets: Vec<usize>,
    memory: Vec<[u8; 4]>,
}

impl ProgsLoader {
    pub fn new() -> ProgsLoader {
        ProgsLoader {
            lumps: [Lump {
                offset: 0,
                count: 0,
            }; LumpId::Count as usize],
            ent_field_count: 0,
            strings: Vec::new(),
            globaldefs: Vec::new(),
            globaldef_offsets: Vec::new(),
            fielddefs: Vec::new(),
            fielddef_offsets: Vec::new(),
            memory: Vec::new(),
        }
    }

    pub fn load(mut self, data: &[u8]) -> Result<Progs, ProgsError> {
        let mut src = Cursor::new(data);
        assert!(src.read_i32::<LittleEndian>()? == VERSION);
        assert!(src.read_i32::<LittleEndian>()? == CRC);

        for l in 0..LumpId::Count as usize {
            self.lumps[l] = Lump {
                offset: src.read_i32::<LittleEndian>()? as usize,
                count: src.read_i32::<LittleEndian>()? as usize,
            };

            debug!("{:?}: {:?}", l, self.lumps[l]);
        }

        self.ent_field_count = src.read_i32::<LittleEndian>()? as usize;
        debug!("Field count: {}", self.ent_field_count);

        let string_lump = &self.lumps[LumpId::Strings as usize];
        src.seek(SeekFrom::Start(string_lump.offset as u64))?;
        (&mut src).take(string_lump.count as u64).read_to_end(
            &mut self.strings,
        )?;

        let function_lump = &self.lumps[LumpId::Functions as usize];
        src.seek(SeekFrom::Start(function_lump.offset as u64))?;
        let mut functions = Vec::with_capacity(function_lump.count);
        for _ in 0..function_lump.count {
            let kind = match src.read_i32::<LittleEndian>()? {
                x if x < 0 => FunctionKind::BuiltIn(-x as usize),
                x => FunctionKind::QuakeC(x as usize),
            };
            let arg_start = src.read_i32::<LittleEndian>()?;
            let locals = src.read_i32::<LittleEndian>()?;

            // throw away profile variable
            let _ = src.read_i32::<LittleEndian>()?;

            let name_ofs = src.read_i32::<LittleEndian>()?;
            let srcfile_ofs = src.read_i32::<LittleEndian>()?;

            let argc = src.read_i32::<LittleEndian>()?;
            let mut argsz = [0; MAX_ARGS];
            src.read(&mut argsz)?;

            functions.push(Function {
                kind: kind,
                arg_start: arg_start as usize,
                locals: locals as usize,
                name_ofs: name_ofs,
                srcfile_ofs: srcfile_ofs,
                argc: argc as usize,
                argsz: argsz,
            });
        }

        assert_eq!(
            src.seek(SeekFrom::Current(0))?,
            src.seek(SeekFrom::Start(
                (function_lump.offset + function_lump.count * FUNCTION_SIZE) as
                    u64,
            ))?
        );

        let globaldef_lump = &self.lumps[LumpId::GlobalDefs as usize];
        src.seek(SeekFrom::Start(globaldef_lump.offset as u64))?;
        for _ in 0..globaldef_lump.count {
            let type_ = src.read_u16::<LittleEndian>()?;
            let offset = src.read_u16::<LittleEndian>()?;
            self.globaldef_offsets.push(offset as usize);
            let name_ofs = src.read_i32::<LittleEndian>()?;
            self.globaldefs.push(Def {
                save: type_ & SAVE_GLOBAL != 0,
                type_: Type::from_u16(type_ & !SAVE_GLOBAL).unwrap(),
                offset: offset,
                name_ofs: name_ofs,
            });
        }

        for (i, g) in self.globaldefs.iter().enumerate() {
            debug!("{}: {:?}", i, g);
        }

        assert_eq!(
            src.seek(SeekFrom::Current(0))?,
            src.seek(SeekFrom::Start(
                (globaldef_lump.offset + globaldef_lump.count * DEF_SIZE) as
                    u64,
            ))?
        );

        let fielddef_lump = &self.lumps[LumpId::Fielddefs as usize];
        src.seek(SeekFrom::Start(fielddef_lump.offset as u64))?;
        for _ in 0..fielddef_lump.count {
            let type_ = src.read_u16::<LittleEndian>()?;
            let offset = src.read_u16::<LittleEndian>()?;
            self.fielddef_offsets.push(offset as usize);
            let name_ofs = src.read_i32::<LittleEndian>()?;
            self.fielddefs.push(Def {
                save: type_ & SAVE_GLOBAL != 0,
                type_: Type::from_u16(type_ & !SAVE_GLOBAL).unwrap(),
                offset: offset,
                name_ofs: name_ofs,
            });
        }

        for (i, f) in self.fielddefs.iter().enumerate() {
            debug!(
                "Field {:>3}: name={:<16} ofs={:>8}",
                i,
                self.get_string_as_str(f.name_ofs)?,
                f.offset
            );
        }

        assert_eq!(
            src.seek(SeekFrom::Current(0))?,
            src.seek(SeekFrom::Start(
                (fielddef_lump.offset + fielddef_lump.count * DEF_SIZE) as
                    u64,
            ))?
        );

        // statements must be loaded last in order to validate operands
        let statement_lump = &self.lumps[LumpId::Statements as usize];
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

        let memory_lump = &self.lumps[LumpId::Globals as usize];
        src.seek(SeekFrom::Start(memory_lump.offset as u64))?;
        for _ in 0..memory_lump.count {
            let mut block = [0; 4];
            src.read(&mut block)?;
            self.memory.push(block);
        }

        assert_eq!(
            src.seek(SeekFrom::Current(0))?,
            src.seek(SeekFrom::Start(
                (memory_lump.offset + memory_lump.count * 4) as u64,
            ))?
        );

        Ok(Progs {
            ent_field_count: self.ent_field_count,
            functions: functions.into_boxed_slice(),
            statements: statements.into_boxed_slice(),
            strings: self.strings.into_boxed_slice(),
            globaldefs: self.globaldefs.into_boxed_slice(),
            globaldef_offsets: self.globaldef_offsets.into_boxed_slice(),
            fielddefs: self.fielddefs.into_boxed_slice(),
            fielddef_offsets: self.fielddef_offsets.into_boxed_slice(),
            memory: self.memory.into_boxed_slice(),
        })
    }

    fn get_string_as_str(&self, ofs: i32) -> Result<&str, ProgsError> {
        if ofs < 0 {
            return Err(ProgsError::with_msg(
                "get_string_as_str: negative string offset",
            ));
        }

        let ofs = ofs as usize;

        if ofs > self.strings.len() {
            return Err(ProgsError::with_msg(
                "get_string_as_str: out-of-bounds string offset",
            ));
        }

        let mut end_index = ofs;
        while self.strings[end_index] != 0 {
            end_index += 1;
        }

        Ok(
            ::std::str::from_utf8(&self.strings[ofs..end_index]).unwrap(),
        )
    }
}

#[derive(Debug)]
pub struct Progs {
    ent_field_count: usize,

    functions: Box<[Function]>,
    statements: Box<[Statement]>,

    strings: Box<[u8]>,

    globaldefs: Box<[Def]>,
    globaldef_offsets: Box<[usize]>,

    fielddefs: Box<[Def]>,
    fielddef_offsets: Box<[usize]>,

    memory: Box<[[u8; 4]]>,
}

impl Progs {
    pub fn dump_functions(&self) {
        for f in self.functions.iter() {
            let name = self.get_string_as_str(f.name_ofs).unwrap();
            print!("{}: ", name);

            match f.kind {
                FunctionKind::BuiltIn(_) => println!("built-in function"),
                FunctionKind::QuakeC(o) => {
                    println!("begins at statement {}", o);
                    for s in self.statements.iter().skip(o) {
                        println!(
                            "    {:<9} {:>5} {:>5} {:>5}",
                            format!("{:?}", s.opcode),
                            s.arg1,
                            s.arg2,
                            s.arg3
                        );
                        if s.opcode == Opcode::Return || s.opcode == Opcode::Done {
                            break;
                        }
                    }
                }
            }
        }
    }
    // run through all statements and see if we crash. elegant!
    pub fn validate(&mut self) {
        'functions: for f in 0..self.functions.len() {
            let name = self.get_string_as_str(self.functions[f].name_ofs)
                .unwrap()
                .to_owned();
            let first = match self.functions[f].kind {
                FunctionKind::BuiltIn(_) => continue,
                FunctionKind::QuakeC(s) => s,
            };

            println!("FUNCTION {}: {}", f, name);

            for i in first..self.statements.len() {
                let op = self.statements[i].opcode;
                let arg1 = self.statements[i].arg1;
                let arg2 = self.statements[i].arg2;
                let arg3 = self.statements[i].arg3;
                match op {
                    Opcode::MulF => self.mul_f(arg1, arg2, arg3).unwrap(),
                    Opcode::MulV => self.mul_v(arg1, arg2, arg3).unwrap(),
                    Opcode::MulFV => self.mul_fv(arg1, arg2, arg3).unwrap(),
                    Opcode::MulVF => self.mul_vf(arg1, arg2, arg3).unwrap(),
                    Opcode::Div => self.div(arg1, arg2, arg3).unwrap(),
                    Opcode::AddF => self.add_f(arg1, arg2, arg3).unwrap(),
                    Opcode::AddV => self.add_v(arg1, arg2, arg3).unwrap(),
                    Opcode::SubF => self.sub_f(arg1, arg2, arg3).unwrap(),
                    Opcode::SubV => self.sub_v(arg1, arg2, arg3).unwrap(),
                    Opcode::EqF => self.eq_f(arg1, arg2, arg3).unwrap(),
                    Opcode::EqV => self.eq_v(arg1, arg2, arg3).unwrap(),
                    Opcode::EqS => self.eq_s(arg1, arg2, arg3).unwrap(),
                    Opcode::EqEnt => self.eq_ent(arg1, arg2, arg3).unwrap(),
                    Opcode::EqFnc => self.eq_fnc(arg1, arg2, arg3).unwrap(),
                    Opcode::NeF => self.ne_f(arg1, arg2, arg3).unwrap(),
                    Opcode::NeV => self.ne_v(arg1, arg2, arg3).unwrap(),
                    Opcode::NeS => self.ne_s(arg1, arg2, arg3).unwrap(),
                    Opcode::NeEnt => self.ne_ent(arg1, arg2, arg3).unwrap(),
                    Opcode::NeFnc => self.ne_fnc(arg1, arg2, arg3).unwrap(),
                    Opcode::Le => self.le(arg1, arg2, arg3).unwrap(),
                    Opcode::Ge => self.ge(arg1, arg2, arg3).unwrap(),
                    Opcode::Lt => self.lt(arg1, arg2, arg3).unwrap(),
                    Opcode::Gt => self.gt(arg1, arg2, arg3).unwrap(),
                    Opcode::LoadF => self.load_f(arg1, arg2, arg3).unwrap(),
                    // Opcode::LoadV
                    // Opcode::LoadS
                    // Opcode::LoadEnt
                    // Opcode::LoadFld
                    // Opcode::LoadFnc
                    // Opcode::Address
                    Opcode::StoreF => self.store_f(arg1, arg2, arg3).unwrap(),
                    Opcode::StoreV => self.store_v(arg1, arg2, arg3).unwrap(),
                    Opcode::StoreS => self.store_s(arg1, arg2, arg3).unwrap(),
                    Opcode::StoreEnt => self.store_ent(arg1, arg2, arg3).unwrap(),
                    Opcode::StoreFld => self.store_fld(arg1, arg2, arg3).unwrap(),
                    Opcode::StoreFnc => self.store_fnc(arg1, arg2, arg3).unwrap(),
                    // Opcode::StorePF
                    // Opcode::StorePV
                    // Opcode::StorePS
                    // Opcode::StorePEnt
                    // Opcode::StorePFld
                    // Opcode::StorePFnc
                    // Opcode::Return
                    Opcode::NotF => self.not_f(arg1, arg2, arg3).unwrap(),
                    Opcode::NotV => self.not_v(arg1, arg2, arg3).unwrap(),
                    Opcode::NotS => self.not_s(arg1, arg2, arg3).unwrap(),
                    Opcode::NotEnt => self.not_ent(arg1, arg2, arg3).unwrap(),
                    Opcode::NotFnc => self.not_fnc(arg1, arg2, arg3).unwrap(),
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
                    Opcode::And => self.and(arg1, arg2, arg3).unwrap(),
                    Opcode::Or => self.or(arg1, arg2, arg3).unwrap(),
                    Opcode::BitAnd => self.bit_and(arg1, arg2, arg3).unwrap(),
                    Opcode::BitOr => self.bit_or(arg1, arg2, arg3).unwrap(),

                    Opcode::Done | Opcode::Return => continue 'functions,
                    _ => (),
                }
            }
        }
    }

    fn execute(&mut self, function_id: FunctionId) {
        let mut callstack: Vec<StackFrame> = Vec::new();

        let pc = self.enter_function(function_id);

        loop {
            let st = &self.statements[pc];
            match st.opcode {
                Opcode::Done | Opcode::Return => (),
                _ => (),
            }
        }
    }

    fn enter_function(&mut self, function_id: FunctionId) -> usize {
        let function = &self.functions[function_id.0 as usize];
        0
    }

    fn get_type_at_offset(&self, ofs: i16) -> Result<Option<Type>, ProgsError> {
        if ofs < 0 {
            return Err(ProgsError::with_msg(
                "Attempted type lookup with negative offset",
            ));
        }

        match self.globaldef_offsets.binary_search(&(ofs as usize)) {
            Ok(o) => Ok(Some(self.globaldefs[o].type_)),
            Err(_) => Ok(None),
        }
    }

    fn get_global_name_at_offset(&self, ofs: i16) -> Result<Option<String>, ProgsError> {
        match self.globaldef_offsets.binary_search(&(ofs as usize)) {
            Ok(o) => Ok(Some(
                self.get_string_as_str(self.globaldefs[o].name_ofs as i32)
                    .unwrap()
                    .to_owned(),
            )),
            Err(_) => Ok(None),
        }
    }

    fn get_field_name_at_offset(&self, ofs: i16) -> Result<Option<String>, ProgsError> {
        match self.fielddef_offsets.binary_search(&(ofs as usize)) {
            Ok(o) => Ok(Some(
                self.get_string_as_str(self.fielddefs[o].name_ofs as i32)
                    .unwrap()
                    .to_owned(),
            )),
            Err(_) => Ok(None),
        }
    }

    fn mem_as_ref(&self, ofs: i16) -> Result<&[u8], ProgsError> {
        match ofs {
            o if o < 0 => Err(ProgsError::with_msg("Negative memory access")),
            o if o as usize > self.memory.len() => Err(ProgsError::with_msg(
                "Out-of-bounds memory access",
            )),
            _ => Ok(&self.memory.as_ref()[ofs as usize]),
        }
    }

    fn mem_as_mut(&mut self, ofs: i16) -> Result<&mut [u8], ProgsError> {
        match ofs {
            o if o < 0 => Err(ProgsError::with_msg("Negative memory access")),
            o if o as usize > self.memory.len() => Err(ProgsError::with_msg(
                "Out-of-bounds memory access",
            )),
            _ => Ok(&mut self.memory.as_mut()[ofs as usize]),
        }
    }

    fn get_string_as_str(&self, ofs: i32) -> Result<&str, ProgsError> {
        if ofs < 0 {
            return Err(ProgsError::with_msg(
                "get_string_as_str: negative string offset",
            ));
        }

        let ofs = ofs as usize;

        if ofs > self.strings.len() {
            return Err(ProgsError::with_msg(
                "get_string_as_str: out-of-bounds string offset",
            ));
        }

        let mut end_index = ofs;
        while self.strings[end_index] != 0 {
            end_index += 1;
        }

        Ok(
            ::std::str::from_utf8(&self.strings[ofs..end_index]).unwrap(),
        )
    }

    fn get_f(&self, ofs: i16) -> Result<f32, ProgsError> {
        match self.get_type_at_offset(ofs)? {
            Some(Type::QFloat) |
            Some(Type::QVector) | // allow loading from QVector for component accesses
            None => (),
            _ => return Err(ProgsError::with_msg("get_f: type check failed")),
        }

        Ok(self.mem_as_ref(ofs)?.read_f32::<LittleEndian>()?)
    }

    fn put_f(&mut self, val: f32, ofs: i16) -> Result<(), ProgsError> {
        match self.get_type_at_offset(ofs)? {
            Some(Type::QFloat) |
            Some(Type::QVector) | // allow storing to QVector for component accesses
            None => (),
            _ => return Err(ProgsError::with_msg("put_f: type check failed")),
        }

        Ok(self.mem_as_mut(ofs)?.write_f32::<LittleEndian>(val)?)
    }

    fn get_v(&self, ofs: i16) -> Result<[f32; 3], ProgsError> {
        match self.get_type_at_offset(ofs)? {
            // we have to allow loading from QFloat because the bytecode occasionally refers to
            // a vector `vec` by its x-component `vec_x`
            Some(Type::QFloat) |
            Some(Type::QVector) |
            None => (),
            Some(t) => {
                return Err(ProgsError::with_msg(
                    format!("get_v: type check failed ({:?})", t),
                ));
            }
        }

        let mut v = [0.0; 3];
        for c in 0..v.len() {
            v[c] = self.mem_as_ref(ofs + c as i16)?.read_f32::<LittleEndian>()?;
        }
        Ok(v)
    }

    fn get_v_unchecked(&self, ofs: i16) -> Result<[f32; 3], ProgsError> {
        let mut v = [0.0; 3];
        for c in 0..v.len() {
            v[c] = self.mem_as_ref(ofs + c as i16)?.read_f32::<LittleEndian>()?;
        }
        Ok(v)
    }

    fn put_v(&mut self, val: [f32; 3], ofs: i16) -> Result<(), ProgsError> {
        match self.get_type_at_offset(ofs)? {
            // we have to allow storing to QFloat because the bytecode occasionally refers to
            // a vector `vec` by its x-component `vec_x`
            Some(Type::QFloat) |
            Some(Type::QVector) |
            None => (),
            Some(t) => {
                return Err(ProgsError::with_msg(
                    format!("put_v: type check failed ({:?})", t),
                ));
            }
        }

        for c in 0..val.len() {
            self.mem_as_mut(ofs + c as i16)?.write_f32::<LittleEndian>(
                val[c],
            )?;
        }
        Ok(())
    }

    fn get_s(&self, ofs: i16) -> Result<i32, ProgsError> {
        match self.get_type_at_offset(ofs)? {
            Some(Type::QString) |
            None => (),
            Some(t) => {
                return Err(ProgsError::with_msg(
                    format!("get_s: type check failed({:?})", t),
                ));
            }
        }

        Ok(self.mem_as_ref(ofs)?.read_i32::<LittleEndian>()?)
    }

    fn put_s(&mut self, val: i32, ofs: i16) -> Result<(), ProgsError> {
        match self.get_type_at_offset(ofs)? {
            Some(Type::QString) |
            None => (),
            Some(t) => {
                return Err(ProgsError::with_msg(
                    format!("put_s: type check failed({:?})", t),
                ));
            }
        }

        Ok(self.mem_as_mut(ofs)?.write_i32::<LittleEndian>(val)?)
    }

    fn get_ent_id(&self, ofs: i16) -> Result<i32, ProgsError> {
        match self.get_type_at_offset(ofs)? {
            Some(Type::QEntity) |
            None => (),
            _ => return Err(ProgsError::with_msg("get_ent_id: type check failed")),
        }

        Ok(self.mem_as_ref(ofs)?.read_i32::<LittleEndian>()?)
    }

    fn put_ent(&mut self, val: i32, ofs: i16) -> Result<(), ProgsError> {
        match self.get_type_at_offset(ofs)? {
            Some(Type::QEntity) |
            None => (),
            _ => return Err(ProgsError::with_msg("put_ent: type check failed")),
        }

        Ok(self.mem_as_mut(ofs)?.write_i32::<LittleEndian>(val)?)
    }

    fn get_fld(&self, ofs: i16) -> Result<i16, ProgsError> {
        match self.get_type_at_offset(ofs)? {
            Some(Type::QField) |
            None => (),
            _ => return Err(ProgsError::with_msg("get_fld: type check failed")),
        }

        Ok(self.mem_as_ref(ofs)?.read_i32::<LittleEndian>()? as i16)
    }

    fn put_fld(&mut self, val: i16, ofs: i16) -> Result<(), ProgsError> {
        match self.get_type_at_offset(ofs)? {
            Some(Type::QField) |
            None => (),
            _ => return Err(ProgsError::with_msg("put_fld: type check failed")),
        }

        Ok(self.mem_as_mut(ofs)?.write_i32::<LittleEndian>(val as i32)?)
    }

    fn get_fnc_id(&self, ofs: i16) -> Result<i32, ProgsError> {
        match self.get_type_at_offset(ofs)? {
            Some(Type::QFunction) |
            None => (),
            _ => return Err(ProgsError::with_msg("get_fnc_id: type check failed")),
        }

        Ok(self.mem_as_ref(ofs)?.read_i32::<LittleEndian>()?)
    }

    fn put_fnc(&mut self, val: i32, ofs: i16) -> Result<(), ProgsError> {
        match self.get_type_at_offset(ofs)? {
            Some(Type::QFunction) |
            None => (),
            _ => return Err(ProgsError::with_msg("put_fnc: type check failed")),
        }

        Ok(self.mem_as_mut(ofs)?.write_i32::<LittleEndian>(val)?)
    }

    // MUL_F: Float multiplication
    fn mul_f(&mut self, f1_id: i16, f2_id: i16, prod_id: i16) -> Result<(), ProgsError> {
        let f1 = self.get_f(f1_id)?;
        let f2 = self.get_f(f2_id)?;
        self.put_f(f1 * f2, prod_id)
    }

    // MUL_V: Vector dot-product
    fn mul_v(&mut self, v1_id: i16, v2_id: i16, dot_id: i16) -> Result<(), ProgsError> {
        let v1 = self.get_v(v1_id)?;
        let v2 = self.get_v(v2_id)?;

        let mut dot = 0.0;

        for c in 0..3 {
            dot += v1[c] * v2[c];
        }
        self.put_f(dot, dot_id)
    }

    // MUL_FV: Component-wise multiplication of vector by scalar
    fn mul_fv(&mut self, f_id: i16, v_id: i16, prod_id: i16) -> Result<(), ProgsError> {
        let f = self.get_f(f_id)?;
        let v = self.get_v(v_id)?;

        let mut prod = [0.0; 3];
        for c in 0..prod.len() {
            prod[c] = v[c] * f;
        }

        self.put_v(prod, prod_id)
    }

    // MUL_VF: Component-wise multiplication of vector by scalar
    fn mul_vf(&mut self, v_id: i16, f_id: i16, prod_id: i16) -> Result<(), ProgsError> {
        let v = self.get_v(v_id)?;
        let f = self.get_f(f_id)?;

        let mut prod = [0.0; 3];
        for c in 0..prod.len() {
            prod[c] = v[c] * f;
        }

        self.put_v(prod, prod_id)
    }

    // DIV: Float division
    fn div(&mut self, f1_id: i16, f2_id: i16, quot_id: i16) -> Result<(), ProgsError> {
        let f1 = self.get_f(f1_id)?;
        let f2 = self.get_f(f2_id)?;
        self.put_f(f1 / f2, quot_id)
    }

    // ADD_F: Float addition
    fn add_f(&mut self, f1_ofs: i16, f2_ofs: i16, sum_ofs: i16) -> Result<(), ProgsError> {
        let f1 = self.get_f(f1_ofs)?;
        let f2 = self.get_f(f2_ofs)?;
        self.put_f(f1 + f2, sum_ofs)
    }

    // ADD_V: Vector addition
    fn add_v(&mut self, v1_id: i16, v2_id: i16, sum_id: i16) -> Result<(), ProgsError> {
        let v1 = self.get_v(v1_id)?;
        let v2 = self.get_v(v2_id)?;

        let mut sum = [0.0; 3];
        for c in 0..sum.len() {
            sum[c] = v1[c] + v2[c];
        }

        self.put_v(sum, sum_id)
    }

    // SUB_F: Float subtraction
    fn sub_f(&mut self, f1_id: i16, f2_id: i16, diff_id: i16) -> Result<(), ProgsError> {
        let f1 = self.get_f(f1_id)?;
        let f2 = self.get_f(f2_id)?;
        self.put_f(f1 - f2, diff_id)
    }

    // SUB_V: Vector subtraction
    fn sub_v(&mut self, v1_id: i16, v2_id: i16, diff_id: i16) -> Result<(), ProgsError> {
        let v1 = self.get_v(v1_id)?;
        let v2 = self.get_v(v2_id)?;

        let mut diff = [0.0; 3];
        for c in 0..diff.len() {
            diff[c] = v1[c] - v2[c];
        }

        self.put_v(diff, diff_id)
    }

    // EQ_F: Test equality of two floats
    fn eq_f(&mut self, f1_id: i16, f2_id: i16, eq_id: i16) -> Result<(), ProgsError> {
        let f1 = self.get_f(f1_id)?;
        let f2 = self.get_f(f2_id)?;
        self.put_f(
            match f1 == f2 {
                true => 1.0,
                false => 0.0,
            },
            eq_id,
        )
    }

    // EQ_V: Test equality of two vectors
    fn eq_v(&mut self, v1_id: i16, v2_id: i16, eq_id: i16) -> Result<(), ProgsError> {
        let v1 = self.get_v(v1_id)?;
        let v2 = self.get_v(v2_id)?;
        self.put_f(
            match v1 == v2 {
                true => 1.0,
                false => 0.0,
            },
            eq_id,
        )
    }

    // EQ_S: Test equality of two strings
    fn eq_s(&mut self, s1_ofs: i16, s2_ofs: i16, eq_ofs: i16) -> Result<(), ProgsError> {
        if s1_ofs < 0 || s2_ofs < 0 {
            return Err(ProgsError::with_msg("eq_s: negative string offset"));
        }

        if s1_ofs as usize > self.strings.len() || s2_ofs as usize > self.strings.len() {
            return Err(ProgsError::with_msg("not_s: out-of-bounds string offset"));
        }

        if s1_ofs == s2_ofs || self.get_s(s1_ofs)? == self.get_s(s2_ofs)? {
            self.put_f(1.0, eq_ofs)
        } else {
            self.put_f(0.0, eq_ofs)
        }
    }

    // EQ_ENT: Test equality of two entities (by identity)
    fn eq_ent(&mut self, e1_ofs: i16, e2_ofs: i16, eq_ofs: i16) -> Result<(), ProgsError> {
        let e1 = self.get_ent_id(e1_ofs)?;
        let e2 = self.get_ent_id(e2_ofs)?;

        self.put_f(
            match e1 == e2 {
                true => 1.0,
                false => 0.0,
            },
            eq_ofs,
        )
    }

    // EQ_FNC: Test equality of two functions (by identity)
    fn eq_fnc(&mut self, f1_ofs: i16, f2_ofs: i16, eq_ofs: i16) -> Result<(), ProgsError> {
        let f1 = self.get_fnc_id(f1_ofs)?;
        let f2 = self.get_fnc_id(f2_ofs)?;

        self.put_f(
            match f1 == f2 {
                true => 1.0,
                false => 0.0,
            },
            eq_ofs,
        )
    }

    // NE_F: Test inequality of two floats
    fn ne_f(&mut self, f1_ofs: i16, f2_ofs: i16, ne_ofs: i16) -> Result<(), ProgsError> {
        let f1 = self.get_f(f1_ofs)?;
        let f2 = self.get_f(f2_ofs)?;
        self.put_f(
            match f1 != f2 {
                true => 1.0,
                false => 0.0,
            },
            ne_ofs,
        )
    }

    // NE_V: Test inequality of two vectors
    fn ne_v(&mut self, v1_ofs: i16, v2_ofs: i16, ne_ofs: i16) -> Result<(), ProgsError> {
        let v1 = self.get_v(v1_ofs)?;
        let v2 = self.get_v(v2_ofs)?;
        self.put_f(
            match v1 != v2 {
                true => 1.0,
                false => 0.0,
            },
            ne_ofs,
        )
    }

    // NE_S: Test inequality of two strings
    fn ne_s(&mut self, s1_ofs: i16, s2_ofs: i16, ne_ofs: i16) -> Result<(), ProgsError> {
        if s1_ofs < 0 || s2_ofs < 0 {
            return Err(ProgsError::with_msg("eq_s: negative string offset"));
        }

        if s1_ofs as usize > self.strings.len() || s2_ofs as usize > self.strings.len() {
            return Err(ProgsError::with_msg("not_s: out-of-bounds string offset"));
        }

        if s1_ofs != s2_ofs && self.get_s(s1_ofs)? != self.get_s(s2_ofs)? {
            self.put_f(1.0, ne_ofs)
        } else {
            self.put_f(0.0, ne_ofs)
        }
    }

    fn ne_ent(&mut self, e1_ofs: i16, e2_ofs: i16, ne_ofs: i16) -> Result<(), ProgsError> {
        let e1 = self.get_ent_id(e1_ofs)?;
        let e2 = self.get_ent_id(e2_ofs)?;

        self.put_f(
            match e1 != e2 {
                true => 1.0,
                false => 0.0,
            },
            ne_ofs,
        )
    }

    fn ne_fnc(&mut self, f1_ofs: i16, f2_ofs: i16, ne_ofs: i16) -> Result<(), ProgsError> {
        let f1 = self.get_fnc_id(f1_ofs)?;
        let f2 = self.get_fnc_id(f2_ofs)?;

        self.put_f(
            match f1 != f2 {
                true => 1.0,
                false => 0.0,
            },
            ne_ofs,
        )
    }

    // LE: Less than or equal to comparison
    fn le(&mut self, f1_ofs: i16, f2_ofs: i16, le_ofs: i16) -> Result<(), ProgsError> {
        let f1 = self.get_f(f1_ofs)?;
        let f2 = self.get_f(f2_ofs)?;
        self.put_f(
            match f1 <= f2 {
                true => 1.0,
                false => 0.0,
            },
            le_ofs,
        )
    }

    // GE: Greater than or equal to comparison
    fn ge(&mut self, f1_ofs: i16, f2_ofs: i16, ge_ofs: i16) -> Result<(), ProgsError> {
        let f1 = self.get_f(f1_ofs)?;
        let f2 = self.get_f(f2_ofs)?;
        self.put_f(
            match f1 >= f2 {
                true => 1.0,
                false => 0.0,
            },
            ge_ofs,
        )
    }

    // LT: Less than comparison
    fn lt(&mut self, f1_ofs: i16, f2_ofs: i16, lt_ofs: i16) -> Result<(), ProgsError> {
        let f1 = self.get_f(f1_ofs)?;
        let f2 = self.get_f(f2_ofs)?;
        self.put_f(
            match f1 < f2 {
                true => 1.0,
                false => 0.0,
            },
            lt_ofs,
        )
    }

    // GT: Greater than comparison
    fn gt(&mut self, f1_ofs: i16, f2_ofs: i16, gt_ofs: i16) -> Result<(), ProgsError> {
        let f1 = self.get_f(f1_ofs)?;
        let f2 = self.get_f(f2_ofs)?;
        self.put_f(
            match f1 > f2 {
                true => 1.0,
                false => 0.0,
            },
            gt_ofs,
        )
    }

    // LOAD_F: load float field from entity
    fn load_f(&mut self, e_ofs: i16, e_f: i16, dest_ofs: i16) -> Result<(), ProgsError> {
        // TODO: this function is a placeholder.
        let ent_id = self.get_ent_id(e_ofs)?;

        let ent_name = match self.get_global_name_at_offset(e_ofs)? {
            Some(s) => s,
            None => format!("entities[ent_id@{}]", e_ofs),
        };

        let fld_ofs = self.get_fld(e_f)?;

        let fld_name = match self.get_field_name_at_offset(fld_ofs)? {
            Some(s) => s,
            None => String::from("anonymous"),
        };

        println!(
            "Ent: {:>4} Fld: {:>4} ({}.{})",
            e_ofs,
            e_f,
            ent_name,
            fld_name
        );
        let ent = Entity::with_field_count(self.ent_field_count);
        let f = ent.get_f(fld_ofs).unwrap();
        self.put_f(f, dest_ofs)
    }

    // STORE_F
    fn store_f(&mut self, src_ofs: i16, dest_ofs: i16, unused: i16) -> Result<(), ProgsError> {
        if unused != 0 {
            return Err(ProgsError::with_msg("Nonzero arg3 to STORE_F"));
        }

        let f = self.get_f(src_ofs).unwrap();
        self.put_f(f, dest_ofs)
    }

    // STORE_V
    fn store_v(&mut self, src_ofs: i16, dest_ofs: i16, unused: i16) -> Result<(), ProgsError> {
        if unused != 0 {
            return Err(ProgsError::with_msg("Nonzero arg3 to STORE_V"));
        }

        // we have to use the unchecked version because STORE_V is used to copy function arguments
        // (see https://github.com/id-Software/Quake-Tools/blob/master/qcc/pr_comp.c#L362) into the
        // global argument slots.
        let v = self.get_v_unchecked(src_ofs).unwrap();
        self.put_v(v, dest_ofs)
    }

    fn store_s(&mut self, src_ofs: i16, dest_ofs: i16, unused: i16) -> Result<(), ProgsError> {
        if unused != 0 {
            return Err(ProgsError::with_msg("Nonzero arg3 to STORE_S"));
        }

        let s = self.get_s(src_ofs)?;
        self.put_s(s, dest_ofs)
    }

    fn store_ent(&mut self, src_ofs: i16, dest_ofs: i16, unused: i16) -> Result<(), ProgsError> {
        if unused != 0 {
            return Err(ProgsError::with_msg("Nonzero arg3 to STORE_ENT"));
        }

        let ent = self.get_ent_id(src_ofs)?;
        self.put_ent(ent, dest_ofs)
    }

    fn store_fld(&mut self, src_ofs: i16, dest_ofs: i16, unused: i16) -> Result<(), ProgsError> {
        if unused != 0 {
            return Err(ProgsError::with_msg("Nonzero arg3 to STORE_FLD"));
        }

        let fld = self.get_fld(src_ofs)?;
        self.put_fld(fld, dest_ofs)
    }

    fn store_fnc(&mut self, src_ofs: i16, dest_ofs: i16, unused: i16) -> Result<(), ProgsError> {
        if unused != 0 {
            return Err(ProgsError::with_msg("Nonzero arg3 to STORE_FNC"));
        }

        let fnc = self.get_fnc_id(src_ofs)?;
        self.put_fnc(fnc, dest_ofs)
    }

    // NOT_F: Compare float to 0.0
    fn not_f(&mut self, f_id: i16, unused: i16, not_id: i16) -> Result<(), ProgsError> {
        if unused != 0 {
            return Err(ProgsError::with_msg("Nonzero arg2 to NOT_F"));
        }

        let f = self.get_f(f_id)?;
        self.put_f(
            match f == 0.0 {
                true => 1.0,
                false => 0.0,
            },
            not_id,
        )
    }

    // NOT_V: Compare vec to { 0.0, 0.0, 0.0 }
    fn not_v(&mut self, v_id: i16, unused: i16, not_id: i16) -> Result<(), ProgsError> {
        if unused != 0 {
            return Err(ProgsError::with_msg("Nonzero arg2 to NOT_V"));
        }

        let v = self.get_v(v_id)?;
        let zero_vec = [0.0; 3];
        self.put_v(
            match v == zero_vec {
                true => [1.0; 3],
                false => zero_vec,
            },
            not_id,
        )
    }

    // NOT_S: Compare string to null string
    fn not_s(&mut self, s_ofs: i16, unused: i16, not_ofs: i16) -> Result<(), ProgsError> {
        if unused != 0 {
            return Err(ProgsError::with_msg("Nonzero arg2 to NOT_S"));
        }

        if s_ofs < 0 {
            return Err(ProgsError::with_msg("not_s: negative string offset"));
        }

        if s_ofs as usize > self.strings.len() {
            return Err(ProgsError::with_msg("not_s: out-of-bounds string offset"));
        }

        if s_ofs == 0 || self.strings[s_ofs as usize] == 0 {
            self.put_f(1.0, not_ofs)?;
        } else {
            self.put_f(0.0, not_ofs)?;
        }

        Ok(())
    }

    // NOT_FNC: Compare function to null function (0)
    fn not_fnc(&mut self, fnc_id_ofs: i16, unused: i16, not_ofs: i16) -> Result<(), ProgsError> {
        if unused != 0 {
            return Err(ProgsError::with_msg("Nonzero arg2 to NOT_FNC"));
        }

        let fnc_id = self.get_fnc_id(fnc_id_ofs)?;
        self.put_f(
            match fnc_id {
                0 => 1.0,
                _ => 0.0,
            },
            not_ofs,
        )
    }

    // NOT_ENT: Compare entity to null entity (0)
    fn not_ent(&mut self, ent_ofs: i16, unused: i16, not_ofs: i16) -> Result<(), ProgsError> {
        if unused != 0 {
            return Err(ProgsError::with_msg("Nonzero arg2 to NOT_ENT"));
        }

        let ent = self.get_ent_id(ent_ofs)?;
        self.put_f(
            match ent {
                0 => 1.0,
                _ => 0.0,
            },
            not_ofs,
        )
    }

    // AND: Logical AND
    fn and(&mut self, f1_id: i16, f2_id: i16, and_id: i16) -> Result<(), ProgsError> {
        let f1 = self.get_f(f1_id)?;
        let f2 = self.get_f(f2_id)?;
        self.put_f(
            match f1 != 0.0 && f2 != 0.0 {
                true => 1.0,
                false => 0.0,
            },
            and_id,
        )
    }

    // OR: Logical OR
    fn or(&mut self, f1_id: i16, f2_id: i16, or_id: i16) -> Result<(), ProgsError> {
        let f1 = self.get_f(f1_id)?;
        let f2 = self.get_f(f2_id)?;
        self.put_f(
            match f1 != 0.0 || f2 != 0.0 {
                true => 1.0,
                false => 0.0,
            },
            or_id,
        )
    }

    // BIT_AND: Bitwise AND
    fn bit_and(&mut self, f1_ofs: i16, f2_ofs: i16, bit_and_ofs: i16) -> Result<(), ProgsError> {
        let f1 = self.get_f(f1_ofs)?;
        let f2 = self.get_f(f2_ofs)?;

        self.put_f((f1 as i32 & f2 as i32) as f32, bit_and_ofs)
    }

    // BIT_OR: Bitwise OR
    fn bit_or(&mut self, f1_ofs: i16, f2_ofs: i16, bit_or_ofs: i16) -> Result<(), ProgsError> {
        let f1 = self.get_f(f1_ofs)?;
        let f2 = self.get_f(f2_ofs)?;

        self.put_f((f1 as i32 | f2 as i32) as f32, bit_or_ofs)
    }
}
