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

extern crate cgmath;
extern crate env_logger;
extern crate log;
extern crate nom;
extern crate richter;

use std::io::Write;

use richter::bsp;
use richter::console::CvarRegistry;
use richter::pak::Pak;
use richter::parse;
use richter::progs;
use richter::server;
use richter::world;

use cgmath::Vector3;
use nom::IResult;

fn main() {
    env_logger::init().unwrap();
    let mut pak = Pak::new();
    match pak.add("pak0.pak") {
        Ok(_) => (),
        Err(why) => {
            println!(
                "Couldn't load pak0.pak: {} (make sure it's in the execution directory)",
                why
            );
            std::process::exit(1);
        }
    };

    let (mut execution_context, mut globals, entity_type_def, string_table) =
        progs::load(pak.open("progs.dat").unwrap()).unwrap();

    let mut server = server::Server::new(string_table.clone());

    let (brush_models, ent_string) = bsp::load(pak.open("maps/e1m1.bsp").unwrap()).unwrap();

    for i in 0..brush_models.len() {
        // TODO: shouldn't have to insert this in string table
        server.precache_model(string_table.insert(format!("*{}", i)));
    }

    let maps = match parse::entity_maps(ent_string.as_bytes()) {
        IResult::Done(_, m) => m,
        _ => panic!("parse failed"),
    };

    let mut cvars = CvarRegistry::new();
    cvars.register_updateinfo("teamplay", "0").unwrap();
    cvars.register("skill", "1").unwrap();
    cvars.register("deathmatch", "0").unwrap();
    cvars.register_updateinfo("sv_gravity", "800").unwrap();

    let mut world = world::World::create(brush_models, entity_type_def, string_table.clone())
        .unwrap();

    let mut dot_file = std::fs::File::create("hull.dot").unwrap();
    dot_file
        .write(
            world
                .hull_for_entity(
                    progs::EntityId(0),
                    Vector3::new(0.0, 0.0, 0.0),
                    Vector3::new(32.0, 1.0, 1.0),
                )
                .unwrap()
                .0
                .gen_dot_graph()
                .as_bytes(),
        )
        .unwrap();

    // spawn dummy entity for client
    world.spawn_entity().unwrap();

    for m in maps {
        world
            .spawn_entity_from_map(
                &mut execution_context,
                &mut globals,
                &mut cvars,
                &mut server,
                m,
                &pak,
            )
            .unwrap();
    }

    let start_frame = globals
        .get_function_id(progs::GlobalAddrFunction::StartFrame as i16)
        .unwrap();

    execution_context
        .execute_program(
            &mut globals,
            &mut world,
            &mut cvars,
            &mut server,
            &pak,
            start_frame,
        )
        .unwrap();
}
