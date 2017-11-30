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

use progs::ProgsError;
use progs::StringId;
use progs::StringTable;
use progs::ops::Opcode;

use std::convert::TryInto;
use std::rc::Rc;

use num::FromPrimitive;

pub const MAX_ARGS: usize = 8;

#[derive(Debug)]
#[repr(C)]
pub struct Statement {
    pub opcode: Opcode,
    pub arg1: i16,
    pub arg2: i16,
    pub arg3: i16,
}

impl Statement {
    pub fn new(op: i16, arg1: i16, arg2: i16, arg3: i16) -> Result<Statement, ProgsError> {
        let opcode = match Opcode::from_i16(op) {
            Some(o) => o,
            None => return Err(ProgsError::with_msg(format!("Bad opcode 0x{:x}", op))),
        };

        Ok(Statement {
            opcode,
            arg1,
            arg2,
            arg3,
        })
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq)]
#[repr(C)]
pub struct FunctionId(pub usize);

impl TryInto<i32> for FunctionId {
    type Error = ProgsError;

    fn try_into(self) -> Result<i32, Self::Error> {
        if self.0 > ::std::i32::MAX as usize {
            Err(ProgsError::with_msg("function ID out of range"))
        } else {
            Ok(self.0 as i32)
        }
    }
}

#[derive(Debug)]
pub enum FunctionKind {
    BuiltIn(usize),
    QuakeC(usize),
}

#[derive(Debug)]
pub struct FunctionDef {
    pub kind: FunctionKind,
    pub arg_start: usize,
    pub locals: usize,
    pub name_id: StringId,
    pub srcfile_id: StringId,
    pub argc: usize,
    pub argsz: [u8; MAX_ARGS],
}

pub struct Functions {
    pub string_table: Rc<StringTable>,
    pub defs: Box<[FunctionDef]>,
    pub statements: Box<[Statement]>,
}

impl Functions {
    pub fn id_from_i32(&self, value: i32) -> Result<FunctionId, ProgsError> {
        if value < 0 {
            return Err(ProgsError::with_msg("id < 0"));
        }

        if (value as usize) < self.defs.len() {
            Ok(FunctionId(value as usize))
        } else {
            Err(ProgsError::with_msg(
                format!("no function with ID {}", value),
            ))
        }
    }

    pub fn get_def(&self, id: FunctionId) -> Result<&FunctionDef, ProgsError> {
        if id.0 >= self.defs.len() {
            Err(ProgsError::with_msg(
                format!("No function with ID {}", id.0),
            ))
        } else {
            Ok(&self.defs[id.0])
        }
    }
}
