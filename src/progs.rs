// Copyright © 2015 Cormac O'Brien.
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
//! ### Loading from disk
//! 
//! 

use std::io::{Read, Seek, SeekFrom};
use std::fs::File;
use std::path::Path;

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use load::Load;
use math::Vec3;

const VERSION: i32 = 6;
const CRC: i32 = 5927;
const MAX_ARGS: usize = 8;
const MAX_STACK_DEPTH: usize = 32;
const LUMP_COUNT: usize = 6;

enum LumpId {
    Statements = 0,
    GlobalDefs = 1,
    FieldDefs = 2,
    Functions = 3,
    Strings = 4,
    Globals = 5,
}

enum DefType {
    QVoid = 0,
    QString = 1,
    QFloat = 2,
    QVector = 3,
    QEntity = 4,
    QField = 5,
    QFunction = 6,
    QPointer = 7,
}

#[derive(Copy, Clone)]
struct Lump {
    offset: usize,
    count: usize,
}

#[repr(C)]
struct Statement {
    op: u16,
    args: [i16; 3],
}

struct Function {
}

struct Progs {
    text: Box<[Statement]>,
    data: Box<[u8]>,
}

impl Progs {
    fn load<P>(&self, path: P) -> Progs 
            where P: AsRef<Path> {
        let mut f = File::open(path).unwrap();

        assert!(f.load_i32le() == VERSION);
        assert!(f.load_i32le() == CRC);

        let mut lumps = [Lump { offset: 0, count: 0 }; LUMP_COUNT];
        for i in 0..LUMP_COUNT {
            lumps[i] = Lump {
                offset: f.load_i32le() as usize,
                count: f.load_i32le() as usize,
            };
        }

        let field_count = f.load_i32le() as usize;

        let statement_lump = &lumps[LumpId::Statements as usize];
        f.seek(SeekFrom::Start(statement_lump.offset as u64)).unwrap();
        let mut statement_vec = Vec::with_capacity(statement_lump.count);
        for _ in 0..statement_lump.count {
            let op = f.load_u16le();
            let mut args = [0; 3];
            for i in 0..args.len() {
                args[i] = f.load_i16le();
            }
            statement_vec.push(Statement {
                op: op,
                args: args,
            });
        }

        let globaldef_lump = &lumps[LumpId::GlobalDefs as usize];
        f.seek(SeekFrom::Start(globaldef_lump.offset as u64)).unwrap();
        //let mut globaldef_vec = Vec::with_capacity(globaldef_lump.count);
        for _ in 0..globaldef_lump.count {
        }
        
        Progs {
            text: Default::default(),
            data: Default::default(),
        }
    }

    fn load_f(&self, addr: u16) -> f32 {
        (&self.data[addr as usize..]).load_f32le()
    }

    fn store_f(&mut self, val: f32, addr: u16) {
        (&mut self.data[addr as usize..]).write_f32::<LittleEndian>(val);
    }

    fn load_v(&self, addr: u16) -> Vec3 {
        let mut components = [0.0; 3];
        let mut src = &self.data[addr as usize..];
        for i in 0..components.len() {
            components[i] = src.load_f32le();
        }
        Vec3::from_components(components)
    }

    fn store_v(&mut self, val: Vec3, addr: u16) {
        let components: [f32; 3] = val.into();
        let mut dst = &mut self.data[addr as usize..];
        for i in 0..components.len() {
            dst.write_f32::<LittleEndian>(components[i]);
        }
    }

    // ADD_F: Float addition
    fn add_f(&mut self, term1: u16, term2: u16, sum: u16) {
        let a = self.load_f(term1);
        let b = self.load_f(term2);
        self.store_f(a + b, sum);
    }

    // ADD_V: Vector addition
    fn add_v(&mut self, term1: u16, term2: u16, sum: u16) {
        let a = self.load_v(term1);
        let b = self.load_v(term2);
        self.store_v(a + b, sum);
    }

    // SUB_F: Float subtraction
    fn sub_f(&mut self, term1: u16, term2: u16, diff: u16) {
        let a = self.load_f(term1);
        let b = self.load_f(term2);
        self.store_f(a - b, diff);
    }

    // SUB_V: Vector subtraction
    fn sub_v(&mut self, term1: u16, term2: u16, diff: u16) {
        let a = self.load_v(term1);
        let b = self.load_v(term2);
        self.store_v(a - b, diff);
    }

    // MUL_F: Float multiplication
    fn mul_f(&mut self, term1: u16, term2: u16, prod: u16) {
        let a = self.load_f(term1);
        let b = self.load_f(term2);
        self.store_f(a * b, prod);
    }

    // MUL_V: Vector dot-product
    fn mul_v(&mut self, term1: u16, term2: u16, dot: u16) {
        let a = self.load_v(term1);
        let b = self.load_v(term2);
        self.store_f(a.dot(b), dot);
    }

    // DIV: Float division
    fn div(&mut self, term1: u16, term2: u16, quot: u16) {
        let a = self.load_f(term1);
        let b = self.load_f(term2);
        self.store_f(a / b, quot);
    }
}

fn main() {
    println!("Hello, world!");
}

#[cfg(test)]
mod test {
    use super::*;
    use std::mem::{size_of, transmute};
    use math::Vec3;
    use progs::Progs;

    #[test]
    fn test_progs_load_f() {
        let to_load = 42.0;

        let data: [u8; 4];
        unsafe {
            data = transmute(to_load);
        }
        let mut progs = Progs {
            data: data.to_vec().into_boxed_slice(),
            text: Default::default(),
        };

        assert!(progs.load_f(0) == to_load);
    }

    #[test]
    fn test_progs_store_f() {
        let to_store = 365.0;
        
        let mut progs = Progs {
            data: vec![0, 0, 0, 0].into_boxed_slice(),
            text: Default::default(),
        };

        progs.store_f(to_store, 0);
        assert!(progs.load_f(0) == to_store);
    }

    #[test]
    fn test_progs_load_v() {
        let to_load = Vec3::new(10.0, -10.0, 0.0);
        let data: [u8; 12];
        unsafe {
            data = transmute(to_load);
        }
        let mut progs = Progs {
            data: data.to_vec().into_boxed_slice(),
            text: Default::default(),
        };

        assert!(progs.load_v(0) == to_load);
    }

    #[test]
    fn test_progs_store_v() {
        let to_store = Vec3::new(245.2, 50327.99, 0.0002);
        
        let mut progs = Progs {
            data: vec![0; 12].into_boxed_slice(),
            text: Default::default(),
        };

        progs.store_v(to_store, 0);
        assert!(progs.load_v(0) == to_store);
    }

    #[test]
    fn test_progs_add_f() {
        let f32_size = size_of::<f32>() as u16;
        let term1 = 5.0;
        let t1_addr = 0 * f32_size;
        let term2 = 7.0;
        let t2_addr = 1 * f32_size;
        let sum_addr = 2 * f32_size;

        let mut progs = Progs {
            data: vec![0; 12].into_boxed_slice(),
            text: Default::default(),
        };

        progs.store_f(term1, t1_addr);
        progs.store_f(term2, t2_addr);
        progs.add_f(t1_addr as u16, t2_addr as u16, sum_addr as u16);
        assert!(progs.load_f(sum_addr) == term1 + term2);
    }

    #[test]
    fn test_progs_sub_f() {
        let f32_size = size_of::<f32>() as u16;
        let term1 = 9.0;
        let t1_addr = 0 * f32_size;
        let term2 = 2.0;
        let t2_addr = 1 * f32_size;
        let diff_addr = 2 * f32_size;

        let mut progs = Progs {
            data: vec![0; 12].into_boxed_slice(),
            text: Default::default(),
        };

        progs.store_f(term1, t1_addr);
        progs.store_f(term2, t2_addr);
        progs.sub_f(t1_addr as u16, t2_addr as u16, diff_addr as u16);
        assert!(progs.load_f(diff_addr) == term1 - term2);
    }

    #[test]
    fn test_progs_mul_f() {
        let f32_size = size_of::<f32>() as u16;
        let term1 = 3.0;
        let t1_addr = 0 * f32_size;
        let term2 = 8.0;
        let t2_addr = 1 * f32_size;
        let prod_addr = 2 * f32_size;

        let mut progs = Progs {
            data: vec![0; 12].into_boxed_slice(),
            text: Default::default(),
        };

        progs.store_f(term1, t1_addr);
        progs.store_f(term2, t2_addr);
        progs.mul_f(t1_addr as u16, t2_addr as u16, prod_addr as u16);
        assert!(progs.load_f(prod_addr) == term1 * term2);
    }

    #[test]
    fn test_progs_div() {
        let f32_size = size_of::<f32>() as u16;
        let term1 = 6.0;
        let t1_addr = 0 * f32_size;
        let term2 = 4.0;
        let t2_addr = 1 * f32_size;
        let quot_addr = 2 * f32_size;

        let mut progs = Progs {
            data: vec![0; 12].into_boxed_slice(),
            text: Default::default(),
        };

        progs.store_f(term1, t1_addr);
        progs.store_f(term2, t2_addr);
        progs.div(t1_addr as u16, t2_addr as u16, quot_addr as u16);
        assert!(progs.load_f(quot_addr) == term1 / term2);
    }
}
