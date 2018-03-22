// Copyright © 2018 Cormac O'Brien
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

extern crate chrono;
extern crate env_logger;
extern crate richter;

use std::env;
use std::process::exit;

use richter::client::Client;
use richter::common::net::BlockingMode;
use richter::common::net::ClientCmd;
use richter::common::net::SignOnStage;
use richter::common::pak::Pak;

use chrono::Duration;

fn main() {
    env_logger::init();

    let mut pak = Pak::new();
    pak.add("pak0.pak").unwrap();

    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        println!("Usage: {} <server_address>", args[0]);
        exit(1);
    }

    let mut client = match Client::connect(&args[1], &pak) {
        Ok(cl) => cl,
        Err(err) => {
            println!("{}", err);
            exit(1);
        }
    };

    let mut quit = false;
    loop {
        client.parse_server_msg().unwrap();
        client.send().unwrap();

        //
        match client.get_signon_stage() {
            SignOnStage::Done => (),
            _ => continue,
        }

        // there's no release event for the mousewheel, so send a release for both scroll directions
        // at the beginning of every frame
        bindings.handle(&mut game_input, &mut cmd_registry, &mut cvar_registry, MouseWheel::Up, ElementState::Released);
        bindings.handle(&mut game_input, &mut cmd_registry, &mut cvar_registry, MouseWheel::Down, ElementState::Released);

        events_loop.poll_events(|event| match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::Closed => quit = true,

                WindowEvent::KeyboardInput {
                    input:
                        KeyboardInput {
                            state,
                            virtual_keycode: Some(key),
                            ..
                        },
                    ..
                } => {
                    bindings.handle(&mut game_input, &mut cmd_registry, &mut cvar_registry, key, state);
                }

                WindowEvent::MouseInput { state, button, .. } => {
                    bindings.handle(&mut game_input, &mut cmd_registry, &mut cvar_registry, button, state);
                }

                WindowEvent::MouseWheel { delta, .. } => {
                    bindings.handle(&mut game_input, &mut cmd_registry, &mut cvar_registry, delta, ElementState::Pressed);
                }

                _ => (),
            },

            _ => (),
        });

        if quit {
            break;
        }
    }
}
