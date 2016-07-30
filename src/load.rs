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
use std::io::{BufReader, Read};
use byteorder::{LittleEndian, ReadBytesExt};

pub trait Load: ReadBytesExt {
    fn read_u8(&mut self) -> u8 {
        ReadBytesExt::read_u8(self).unwrap()
    }

    fn read_u16le(&mut self) -> u16 {
        self.read_u16::<LittleEndian>().unwrap()
    }

    fn read_i16le(&mut self) -> i16 {
        self.read_i16::<LittleEndian>().unwrap()
    }

    fn read_u32le(&mut self) -> u32 {
        self.read_u32::<LittleEndian>().unwrap()
    }

    fn read_i32le(&mut self) -> i32 {
        self.read_i32::<LittleEndian>().unwrap()
    }

    fn read_f32le(&mut self) -> f32 {
        self.read_f32::<LittleEndian>().unwrap()
    }
}

impl<R> Load for BufReader<R> where R: Read {
}
