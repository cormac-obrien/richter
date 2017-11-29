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
extern crate richter;

use richter::bsp;
use richter::entity::Entity;
use richter::pak::Pak;
use richter::parse;
use richter::progs;

fn main() {
    env_logger::init().unwrap();
    let mut pak = Pak::new();
    pak.add("pak0.pak").unwrap();

    let (mut progs, mut globals, mut entity_list) = progs::load(pak.open("progs.dat").unwrap())
        .unwrap();

    let (bsp, ent_string) = bsp::load(pak.open("maps/e1m1.bsp").unwrap()).unwrap();

    let ent_maps = parse::entity_maps(ent_string.as_bytes());
    println!("{:?}", ent_maps);

    println!("=========\nFUNCTIONS\n=========\n");
    progs.dump_functions();

    entity_list.fill_all_uninitialized();
    progs.validate(&mut globals, &mut entity_list);
}
