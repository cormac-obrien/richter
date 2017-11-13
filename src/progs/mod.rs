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
//! QuakeC bytecode consists of eight-byte instructions (referred to as "statements" in the original
//! source, and henceforth referred to as such here) comprised of a 16-bit opcode and three 16-bit
//! operands. The operands correspond to 4-byte offsets within the globals table. Thus, an operand
//! of 0x0004 (4) indicates that the value should be loaded from 0x0010 (16) bytes into the global
//! table. The statements are kept in a contiguous block of code (the "statements table").
//!
//! The function table consists of named records containing the information necessary to execute
//! the functions they describe, including the index of the first statement in the statements table,
//! the number, sizes and locations of the arguments, and the number of local values used by the
//! function.
//!
//! The call stack consists of stack frames containing the index of the function in the function
//! table and the index offset (from the functions first statement) of the statement to reenter on.

mod globals;
mod ops;

use std::io::{Cursor, Read, Seek, SeekFrom};
use std::mem::transmute;

use byteorder::{LittleEndian, WriteBytesExt};
use load::{Load, LoadError};
use math::Vec3;
use num::FromPrimitive;

use self::ops::Opcode;
use self::globals::Globals;

const VERSION: i32 = 6;
const CRC: i32 = 5927;
const MAX_ARGS: usize = 8;
const MAX_STACK_DEPTH: usize = 32;
const LUMP_COUNT: usize = 6;
const SAVE_GLOBAL: u16 = 1 << 15;

#[repr(C)]
struct FunctionId(i32);

#[repr(C)]
struct StringId(i32);

enum LumpId {
    Statements = 0,
    GlobalDefs = 1,
    FieldDefs = 2,
    Functions = 3,
    Strings = 4,
    Globals = 5,
}

#[derive(FromPrimitive)]
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

#[repr(C)]
struct Statement {
    opcode: Opcode,
    arg1: u16,
    arg2: u16,
    result: u16,
}

struct Function {
    statement_id: usize,
    arg_start: usize,
    locals: usize,
    name_id: usize,
    srcfile_id: usize,
    argc: usize,
    argsz: [u8; MAX_ARGS],
}

struct StackFrame {
    instr_id: i32,
    func_id: u32,
}

#[derive(Copy, Clone)]
struct Lump {
    offset: usize,
    count: usize,
}

#[repr(C)]
struct Def {
    save: bool,
    type_: Type,
    offset: u16,
    name_id: i32,
}

pub struct Progs {
    functions: Box<[Function]>,
    statements: Box<[Statement]>,

    globaldefs: Box<[Def]>,
    fielddefs: Box<[Def]>,
    globals: Globals,
}

impl Progs {
    pub fn load(data: &[u8]) -> Result<Progs, LoadError> {
        let mut src = Cursor::new(data);
        assert!(src.load_i32le(None)? == VERSION);
        assert!(src.load_i32le(None)? == CRC);

        let mut lumps = Vec::new();
        for i in 0..LUMP_COUNT {
            lumps.push(Lump {
                offset: src.load_i32le(Some(&(0..)))? as usize,
                count: src.load_i32le(Some(&(0..)))? as usize,
            });
        }

        let field_count = src.load_i32le(Some(&(0..)))? as usize;

        let statement_lump = &lumps[LumpId::Statements as usize];
        src.seek(SeekFrom::Start(statement_lump.offset as u64))?;
        let mut statements = Vec::with_capacity(statement_lump.count);
        for _ in 0..statement_lump.count {
            statements.push(Statement {
                opcode: Opcode::from_u16(src.load_u16le(None)?).unwrap(),
                arg1: src.load_u16le(None)?,
                arg2: src.load_u16le(None)?,
                result: src.load_u16le(None)?,
            });
        }

        let function_lump = &lumps[LumpId::Functions as usize];
        src.seek(SeekFrom::Start(function_lump.offset as u64))?;
        let mut functions = Vec::with_capacity(function_lump.count);
        for _ in 0..function_lump.count {
            functions.push(Function {
                statement_id: src.load_i32le(Some(&(0..)))? as usize,
                arg_start: src.load_i32le(Some(&(0..)))? as usize,
                locals: src.load_i32le(Some(&(0..)))? as usize,
                name_id: src.load_i32le(Some(&(0..)))? as usize,
                srcfile_id: src.load_i32le(Some(&(0..)))? as usize,
                argc: src.load_i32le(Some(&(0..)))? as usize,
                argsz: [
                    src.load_u8(None)?,
                    src.load_u8(None)?,
                    src.load_u8(None)?,
                    src.load_u8(None)?,
                    src.load_u8(None)?,
                    src.load_u8(None)?,
                    src.load_u8(None)?,
                    src.load_u8(None)?,
                ],
            });
        }

        let globaldef_lump = &lumps[LumpId::GlobalDefs as usize];
        src.seek(SeekFrom::Start(globaldef_lump.offset as u64))?;
        let mut globaldefs = Vec::new();
        for _ in 0..globaldef_lump.count {
            let type_ = src.load_u16le(None)?;
            globaldefs.push(Def {
                save: type_ & SAVE_GLOBAL != 0,
                type_: Type::from_u16(type_ & !SAVE_GLOBAL).unwrap(),
                offset: src.load_u16le(None)?,
                name_id: src.load_i32le(None)?,
            });
        }

        let fielddef_lump = &lumps[LumpId::FieldDefs as usize];
        src.seek(SeekFrom::Start(fielddef_lump.offset as u64))?;
        let mut fielddefs = Vec::new();
        for _ in 0..fielddef_lump.count {
            let type_ = src.load_u16le(None)?;
            fielddefs.push(Def {
                save: type_ & SAVE_GLOBAL != 0,
                type_: Type::from_u16(type_ & !SAVE_GLOBAL).unwrap(),
                offset: src.load_u16le(None)?,
                name_id: src.load_i32le(None)?,
            });
        }

        Ok(Progs {
            functions: functions.into_boxed_slice(),
            statements: statements.into_boxed_slice(),
            globaldefs: globaldefs.into_boxed_slice(),
            fielddefs: fielddefs.into_boxed_slice(),
            globals: Globals::new(),
        })
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

    fn globals_as_f32(&mut self) -> *mut [f32] {
        let globals_f: &mut [f32; globals::GLOBALS_COUNT] = unsafe { transmute(&mut self.globals) };
        globals_f as *mut [f32; globals::GLOBALS_COUNT]
    }

    fn get_f(&mut self, id: u16) -> f32 {
        unsafe { (*self.globals_as_f32())[id as usize] }
    }

    fn put_f(&mut self, val: f32, id: u16) {
        unsafe { (*self.globals_as_f32())[id as usize] = val };
    }

    fn get_v(&mut self, id: u16) -> Vec3 {
        let slice = unsafe { &(*self.globals_as_f32())[id as usize..id as usize + 3] };
        Vec3::new(slice[0], slice[1], slice[2])
    }

    fn put_v(&mut self, val: Vec3, id: u16) {
        let array: [f32; 3] = val.into();
        for i in 0..3 {
            unsafe {
                (*self.globals_as_f32())[i] = array[i];
            }
        }
    }

    // ADD_F: Float addition
    fn add_f(&mut self, f1_id: u16, f2_id: u16, sum_id: u16) {
        let f1 = self.get_f(f1_id);
        let f2 = self.get_f(f2_id);
        self.put_f(f1 + f2, sum_id);
    }

    // ADD_V: Vector addition
    fn add_v(&mut self, v1_id: u16, v2_id: u16, sum_id: u16) {
        let v1 = self.get_v(v1_id);
        let v2 = self.get_v(v2_id);
        self.put_v(v1 + v2, sum_id);
    }

    // SUB_F: Float subtraction
    fn sub_f(&mut self, f1_id: u16, f2_id: u16, diff_id: u16) {
        let f1 = self.get_f(f1_id);
        let f2 = self.get_f(f2_id);
        self.put_f(f1 - f2, diff_id);
    }

    // SUB_V: Vector subtraction
    fn sub_v(&mut self, v1_id: u16, v2_id: u16, diff_id: u16) {
        let v1 = self.get_v(v1_id);
        let v2 = self.get_v(v2_id);
        self.put_v(v1 - v2, diff_id);
    }

    // MUL_F: Float multiplication
    fn mul_f(&mut self, f1_id: u16, f2_id: u16, prod_id: u16) {
        let f1 = self.get_f(f1_id);
        let f2 = self.get_f(f2_id);
        self.put_f(f1 * f2, prod_id);
    }

    // MUL_V: Vector dot-product
    fn mul_v(&mut self, v1_id: u16, v2_id: u16, dot_id: u16) {
        let v1 = self.get_v(v1_id);
        let v2 = self.get_v(v2_id);
        self.put_f(v1.dot(v2), dot_id);
    }

    // MUL_FV: Component-wise multiplication of vector by scalar
    fn mul_fv(&mut self, f_id: u16, v_id: u16, prod_id: u16) {
        let f = self.get_f(f_id);
        let v = self.get_v(v_id);
        self.put_v(v * f, prod_id);
    }

    // MUL_VF: Component-wise multiplication of vector by scalar
    fn mul_vf(&mut self, v_id: u16, f_id: u16, prod_id: u16) {
        let v = self.get_v(v_id);
        let f = self.get_f(f_id);
        self.put_v(v * f, prod_id);
    }

    // DIV: Float division
    fn div_f(&mut self, f1_id: u16, f2_id: u16, quot_id: u16) {
        let f1 = self.get_f(f1_id);
        let f2 = self.get_f(f2_id);
        self.put_f(f1 / f2, quot_id);
    }

    // BITAND: Bitwise AND
    fn bitand(&mut self, f1_id: u16, f2_id: u16, and_id: u16) {
        let i1 = self.get_f(f1_id) as i32;
        let i2 = self.get_f(f2_id) as i32;
        self.put_f((i1 & i2) as f32, and_id);
    }

    // BITOR: Bitwise OR
    fn bitor(&mut self, f1_id: u16, f2_id: u16, or_id: u16) {
        let i1 = self.get_f(f1_id) as i32;
        let i2 = self.get_f(f2_id) as i32;
        self.put_f((i1 | i2) as f32, or_id);
    }

    // GE: Greater than or equal to comparison
    fn ge(&mut self, f1_id: u16, f2_id: u16, ge_id: u16) {
        let f1 = self.get_f(f1_id);
        let f2 = self.get_f(f2_id);
        self.put_f(
            match f1 >= f2 {
                true => 1.0,
                false => 0.0,
            },
            ge_id,
        );
    }

    // LE: Less than or equal to comparison
    fn le(&mut self, f1_id: u16, f2_id: u16, le_id: u16) {
        let f1 = self.get_f(f1_id);
        let f2 = self.get_f(f2_id);
        self.put_f(
            match f1 <= f2 {
                true => 1.0,
                false => 0.0,
            },
            le_id,
        );
    }

    // GE: Greater than comparison
    fn gt(&mut self, f1_id: u16, f2_id: u16, gt_id: u16) {
        let f1 = self.get_f(f1_id);
        let f2 = self.get_f(f2_id);
        self.put_f(
            match f1 > f2 {
                true => 1.0,
                false => 0.0,
            },
            gt_id,
        );
    }

    // LT: Less than comparison
    fn lt(&mut self, f1_id: u16, f2_id: u16, lt_id: u16) {
        let f1 = self.get_f(f1_id);
        let f2 = self.get_f(f2_id);
        self.put_f(
            match f1 < f2 {
                true => 1.0,
                false => 0.0,
            },
            lt_id,
        );
    }

    // AND: Logical AND
    fn and(&mut self, f1_id: u16, f2_id: u16, and_id: u16) {
        let f1 = self.get_f(f1_id);
        let f2 = self.get_f(f2_id);
        self.put_f(
            match f1 != 0.0 && f2 != 0.0 {
                true => 1.0,
                false => 0.0,
            },
            and_id,
        );
    }

    // OR: Logical OR
    fn or(&mut self, f1_id: u16, f2_id: u16, or_id: u16) {
        let f1 = self.get_f(f1_id);
        let f2 = self.get_f(f2_id);
        self.put_f(
            match f1 != 0.0 || f2 != 0.0 {
                true => 1.0,
                false => 0.0,
            },
            or_id,
        );
    }

    // NOT_F: Compare float to 0.0
    fn not_f(&mut self, f_id: u16, not_id: u16) {
        let f = self.get_f(f_id);
        self.put_f(
            match f == 0.0 {
                true => 1.0,
                false => 0.0,
            },
            not_id,
        );
    }

    // NOT_V: Compare vec to { 0.0, 0.0, 0.0 }
    fn not_v(&mut self, v_id: u16, not_id: u16) {
        let v = self.get_v(v_id);
        let zero_vec = Vec3::new(0.0, 0.0, 0.0);
        self.put_v(
            match v == zero_vec {
                true => Vec3::new(1.0, 1.0, 1.0),
                false => zero_vec,
            },
            not_id,
        );
    }

    // TODO
    // NOT_S: Compare string to ???

    // TODO
    // NOT_FNC: Compare function to ???

    // TODO
    // NOT_ENT: Compare entity to ???

    // EQ_F: Test equality of two floats
    fn eq_f(&mut self, f1_id: u16, f2_id: u16, eq_id: u16) {
        let f1 = self.get_f(f1_id);
        let f2 = self.get_f(f2_id);
        self.put_f(
            match f1 == f2 {
                true => 1.0,
                false => 0.0,
            },
            eq_id,
        );
    }

    // EQ_V: Test equality of two vectors
    fn eq_v(&mut self, v1_id: u16, v2_id: u16, eq_id: u16) {
        let v1 = self.get_v(v1_id);
        let v2 = self.get_v(v2_id);
        self.put_f(
            match v1 == v2 {
                true => 1.0,
                false => 0.0,
            },
            eq_id,
        );
    }

    // NE_F: Test inequality of two floats
    fn ne_f(&mut self, f1_id: u16, f2_id: u16, ne_id: u16) {
        let f1 = self.get_f(f1_id);
        let f2 = self.get_f(f2_id);
        self.put_f(
            match f1 != f2 {
                true => 1.0,
                false => 0.0,
            },
            ne_id,
        );
    }

    // NE_V: Test inequality of two vectors
    fn ne_v(&mut self, v1_id: u16, v2_id: u16, ne_id: u16) {
        let v1 = self.get_v(v1_id);
        let v2 = self.get_v(v2_id);
        self.put_f(
            match v1 != v2 {
                true => 1.0,
                false => 0.0,
            },
            ne_id,
        );
    }

    fn ne_s(&mut self, s1_id: u16, s2_id: u16, ne_id: u16) {}
}

// #[cfg(test)]
// mod test {
// use super::*;
// use std::mem::{size_of, transmute};
// use math::Vec3;
// use progs::Progs;
//
// #[test]
// fn test_progs_get_f() {
// let to_load = 42.0;
//
// let data: [u8; 4];
// unsafe {
// data = transmute(to_load);
// }
// let mut progs = Progs {
// functions: Default::default(),
// data: data.to_vec().into_boxed_slice(),
// statements: Default::default(),
// };
//
// assert!(progs.get_f(0) == to_load);
// }
//
// #[test]
// fn test_progs_put_f() {
// let to_store = 365.0;
//
// let mut progs = Progs {
// functions: Default::default(),
// data: vec![0, 0, 0, 0].into_boxed_slice(),
// statements: Default::default(),
// };
//
// progs.put_f(to_store, 0);
// assert!(progs.get_f(0) == to_store);
// }
//
// #[test]
// fn test_progs_get_v() {
// let to_load = Vec3::new(10.0, -10.0, 0.0);
// let data: [u8; 12];
// unsafe {
// data = transmute(to_load);
// }
// let mut progs = Progs {
// functions: Default::default(),
// data: data.to_vec().into_boxed_slice(),
// statements: Default::default(),
// };
//
// assert!(progs.get_v(0) == to_load);
// }
//
// #[test]
// fn test_progs_put_v() {
// let to_store = Vec3::new(245.2, 50327.99, 0.0002);
//
// let mut progs = Progs {
// functions: Default::default(),
// data: vec![0; 12].into_boxed_slice(),
// statements: Default::default(),
// };
//
//
// progs.put_v(to_store, 0);
// assert!(progs.get_v(0) == to_store);
// }
//
// #[test]
// fn test_progs_add_f() {
// let f32_size = size_of::<f32>() as u16;
// let term1 = 5.0;
// let t1_addr = 0 * f32_size;
// let term2 = 7.0;
// let t2_addr = 1 * f32_size;
// let sum_addr = 2 * f32_size;
//
// let mut progs = Progs {
// functions: Default::default(),
// data: vec![0; 12].into_boxed_slice(),
// statements: Default::default(),
// };
//
// progs.put_f(term1, t1_addr);
// progs.put_f(term2, t2_addr);
// progs.add_f(t1_addr as u16, t2_addr as u16, sum_addr as u16);
// assert!(progs.get_f(sum_addr) == term1 + term2);
// }
//
// #[test]
// fn test_progs_sub_f() {
// let f32_size = size_of::<f32>() as u16;
// let term1 = 9.0;
// let t1_addr = 0 * f32_size;
// let term2 = 2.0;
// let t2_addr = 1 * f32_size;
// let diff_addr = 2 * f32_size;
//
// let mut progs = Progs {
// functions: Default::default(),
// data: vec![0; 12].into_boxed_slice(),
// statements: Default::default(),
// };
//
// progs.put_f(term1, t1_addr);
// progs.put_f(term2, t2_addr);
// progs.sub_f(t1_addr as u16, t2_addr as u16, diff_addr as u16);
// assert!(progs.get_f(diff_addr) == term1 - term2);
// }
//
// #[test]
// fn test_progs_mul_f() {
// let f32_size = size_of::<f32>() as u16;
// let term1 = 3.0;
// let t1_addr = 0 * f32_size;
// let term2 = 8.0;
// let t2_addr = 1 * f32_size;
// let prod_addr = 2 * f32_size;
//
// let mut progs = Progs {
// functions: Default::default(),
// data: vec![0; 12].into_boxed_slice(),
// statements: Default::default(),
// };
//
// progs.put_f(term1, t1_addr);
// progs.put_f(term2, t2_addr);
// progs.mul_f(t1_addr as u16, t2_addr as u16, prod_addr as u16);
// assert!(progs.get_f(prod_addr) == term1 * term2);
// }
//
// #[test]
// fn test_progs_div_f() {
// let f32_size = size_of::<f32>() as u16;
// let term1 = 6.0;
// let t1_addr = 0 * f32_size;
// let term2 = 4.0;
// let t2_addr = 1 * f32_size;
// let quot_addr = 2 * f32_size;
//
// let mut progs = Progs {
// functions: Default::default(),
// data: vec![0; 12].into_boxed_slice(),
// statements: Default::default(),
// };
//
// progs.put_f(term1, t1_addr);
// progs.put_f(term2, t2_addr);
// progs.div_f(t1_addr as u16, t2_addr as u16, quot_addr as u16);
// assert!(progs.get_f(quot_addr) == term1 / term2);
// }
//
// #[test]
// fn test_progs_bitand() {
// let f32_size = size_of::<f32>() as u16;
// let term1: f32 = unsafe { transmute(0xFFFFFFFFu32) };
// let t1_addr = 0 * f32_size;
// let term2: f32 = unsafe { transmute(0xF0F0F0F0u32) };
// let t2_addr = 1 * f32_size;
// let result_addr = 2 * f32_size;
//
// let mut progs = Progs {
// functions: Default::default(),
// data: vec![0; 12].into_boxed_slice(),
// statements: Default::default(),
// };
//
// progs.put_f(term1, t1_addr);
// progs.put_f(term2, t2_addr);
// progs.bitand(t1_addr as u16, t2_addr as u16, result_addr as u16);
// assert_eq!(progs.get_f(result_addr) as i32, term1 as i32 & term2 as i32);
// }
// }
