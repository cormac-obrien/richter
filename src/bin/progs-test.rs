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

extern crate env_logger;
extern crate log;
extern crate nom;
extern crate richter;

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::collections::HashSet;

use richter::bsp;
use richter::entity::Entity;
use richter::pak::Pak;
use richter::parse;
use richter::progs;

use nom::IResult;

fn main() {
    env_logger::init().unwrap();
    let mut pak = Pak::new();
    pak.add("pak0.pak").unwrap();

    let (functions, mut globals, mut entity_list) = progs::load(pak.open("progs.dat").unwrap())
        .unwrap();

    let (world_model, sub_models, ent_string) = bsp::load(pak.open("maps/e1m1.bsp").unwrap())
        .unwrap();

    let maps = match parse::entity_maps(ent_string.as_bytes()) {
        IResult::Done(_, m) => m,
        _ => panic!("parse failed"),
    };

    for m in maps {
        entity_list.alloc_from_map(m).unwrap();
    }

    progs::validate(&functions, &mut globals, &mut entity_list);
}
