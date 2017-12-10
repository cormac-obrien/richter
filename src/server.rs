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

use std::io::Cursor;
use std::io::Seek;
use std::io::SeekFrom;
use std::rc::Rc;

use progs::StringId;
use progs::StringTable;

use byteorder::WriteBytesExt;

const MAX_DATAGRAM: usize = 1024;
const MAX_LIGHTSTYLES: usize = 64;

pub struct Server {
    string_table: Rc<StringTable>,
    sound_precache: Vec<String>,
    model_precache: Vec<String>,
    lightstyles: [StringId; MAX_LIGHTSTYLES],
    datagram: Cursor<Box<[u8]>>,
}

impl Server {
    pub fn new(string_table: Rc<StringTable>) -> Server {
        let mut sound_precache = Vec::new();
        sound_precache.push(String::new()); // sound 0 is none

        let mut model_precache = Vec::new();
        model_precache.push(String::new()); // model 0 is none

        Server {
            string_table,
            sound_precache,
            model_precache,
            lightstyles: [StringId(0); MAX_LIGHTSTYLES],
            datagram: Cursor::new(Box::new([0; MAX_DATAGRAM])),
        }
    }

    pub fn precache_sound(&mut self, name_id: StringId) {
        let name = self.string_table.get(name_id).unwrap();

        if self.sound_precache.iter().find(|s| **s == name).is_some() {
            debug!("Precaching sound {}: already precached", name);
        } else {
            debug!("Precaching sound {}", name);
            self.sound_precache.push(name);
        }
    }

    pub fn sound_precache_lookup(&self, name_id: StringId) -> Result<usize, ()> {
        let target_name = self.string_table.get(name_id).unwrap();

        match self.sound_precache.iter().enumerate().find(|&(_,
           &ref item_name)| {
            *item_name == target_name
        }) {
            Some((i, _)) => Ok(i),
            None => Err(()),
        }
    }

    pub fn precache_model(&mut self, name_id: StringId) {
        let name = self.string_table.get(name_id).unwrap();

        if self.model_precache.iter().find(|s| **s == name).is_some() {
            debug!("Precaching model {}: already precached", name);
        } else {
            debug!("Precaching model {}", name);
            self.model_precache.push(name);
        }
    }

    pub fn model_precache_lookup(&self, name_id: StringId) -> Result<usize, ()> {
        let target_name = self.string_table.get(name_id).unwrap();
        debug!("Model precache lookup: {}", target_name);

        match self.model_precache.iter().enumerate().find(|&(_,
           &ref item_name)| {
            *item_name == target_name
        }) {
            Some((i, _)) => {
                debug!("Found {} at precache index {}", target_name, i);
                Ok(i)
            }
            None => Err(()),
        }
    }

    pub fn clear_datagram(&mut self) {
        self.datagram.seek(SeekFrom::Start(0)).unwrap();
        for _ in 0..self.datagram.get_ref().len() {
            self.datagram.write_u8(0).unwrap();
        }
        self.datagram.seek(SeekFrom::Start(0)).unwrap();
    }

    pub fn set_lightstyle(&mut self, lightstyle_index: usize, lightstyle_val_id: StringId) {
        self.lightstyles[lightstyle_index] = lightstyle_val_id;
    }
}
