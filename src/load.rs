// Copyright © 2016 Cormac O'Brien
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

use std::collections::Bound;
use std::collections::range::RangeArgument;
use std::convert::AsRef;
use std::error::Error;
use std::fs::File;
use std::io::{BufReader, Cursor, Read};
use byteorder::{LittleEndian, ReadBytesExt};

#[derive(Debug)]
pub enum LoadError {
    Read,
    Range,
}

fn in_range<T>(x: T, range: &RangeArgument<T>) -> bool
    where T: PartialOrd + Copy
{
    match range.start() {
        Bound::Included(&s) => {
            if x < s {
                return false;
            }
        }
        Bound::Excluded(&s) => {
            if x <= s {
                return false;
            }
        }
        Bound::Unbounded => (),
    }

    match range.end() {
        Bound::Included(&e) => {
            if x > e {
                return false;
            }
        }
        Bound::Excluded(&e) => {
            if x >= e {
                return false;
            }
        }
        Bound::Unbounded => (),
    }

    true
}

pub trait Load: ReadBytesExt {
    fn load_u8(&mut self, range: Option<&RangeArgument<u8>>) -> Result<u8, LoadError> {
        let x = match ReadBytesExt::read_u8(self) {
            Ok(x) => x,
            Err(_) => return Err(LoadError::Read),
        };

        if let Some(r) = range {
            if !in_range(x, r) {
                return Err(LoadError::Range);
            }
        }

        Ok(x)
    }

    fn load_u16le(&mut self, range: Option<&RangeArgument<u16>>) -> Result<u16, LoadError> {
        let x = match ReadBytesExt::read_u16::<LittleEndian>(self) {
            Ok(x) => x,
            Err(_) => return Err(LoadError::Read),
        };

        if let Some(r) = range {
            if !in_range(x, r) {
                return Err(LoadError::Range);
            }
        }

        Ok(x)
    }

    fn load_i16le(&mut self, range: Option<&RangeArgument<i16>>) -> Result<i16, LoadError> {
        let x = match ReadBytesExt::read_i16::<LittleEndian>(self) {
            Ok(x) => x,
            Err(_) => return Err(LoadError::Read),
        };

        if let Some(r) = range {
            if !in_range(x, r) {
                return Err(LoadError::Range);
            }
        }

        Ok(x)
    }

    fn load_u32le(&mut self, range: Option<&RangeArgument<u32>>) -> Result<u32, LoadError> {
        let x = match ReadBytesExt::read_u32::<LittleEndian>(self) {
            Ok(x) => x,
            Err(_) => return Err(LoadError::Read),
        };

        if let Some(r) = range {
            if !in_range(x, r) {
                return Err(LoadError::Range);
            }
        }

        Ok(x)
    }

    fn load_i32le(&mut self, range: Option<&RangeArgument<i32>>) -> Result<i32, LoadError> {
        let x = match ReadBytesExt::read_i32::<LittleEndian>(self) {
            Ok(x) => x,
            Err(_) => return Err(LoadError::Read),
        };

        if let Some(r) = range {
            if !in_range(x, r) {
                return Err(LoadError::Range);
            }
        }

        Ok(x)
    }

    fn load_f32le(&mut self, range: Option<&RangeArgument<f32>>) -> Result<f32, LoadError> {
        let x = match ReadBytesExt::read_f32::<LittleEndian>(self) {
            Ok(x) => x,
            Err(_) => return Err(LoadError::Read),
        };

        if let Some(r) = range {
            if !in_range(x, r) {
                return Err(LoadError::Range);
            }
        }

        Ok(x)
    }
}

impl<R> Load for BufReader<R> where R: Read {}
impl<T> Load for Cursor<T> where T: AsRef<[u8]> {}
impl Load for File {}
impl<'a> Load for &'a [u8] {}
