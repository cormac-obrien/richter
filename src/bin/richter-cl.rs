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

extern crate env_logger;
extern crate glium;
#[macro_use]
extern crate log;
extern crate richter;

use std::error::Error;
use std::net::Ipv4Addr;
use std::process::exit;
use glium::glutin::Event;
use richter::bsp;
use richter::bspload;
use richter::client::{Client, CxnStatus};
use richter::entity;
use richter::event;
use richter::math;
use richter::pak;
use richter::progs;

fn frame(cl: &Client) {
    // TODO: handle key input
    // TODO: handle mouse/controller input
    // TODO: run console/script commands

    cl.read_packets();

    if cl.get_cxn_status() == CxnStatus::Disconnected {
        // TODO: resend connection request
        cl.retry_connect();
    } else {
        // TODO: send commands to server
    }

    // TODO: client-side prediction
    // TODO: refresh entity list
}

fn main() {
    env_logger::init().unwrap();
    info!("Richter v0.0.1");

    let cl = Client::connect(Ipv4Addr::new(127, 0, 0, 1));
    loop {
        frame(&cl);
    }

    exit(0);

    use glium::DisplayBuild;

    let display = match glium::glutin::WindowBuilder::new()
                            .with_dimensions(1024, 768)
                            .with_title(format!("Richter"))
                            .build_glium() {
        Ok(w) => w,
        Err(why) => {
            use std::error::Error;
            let mut error: Option<&Error> = Some(&why as &Error);
            while let Some(e) = error {
                println!("{}", e);
                error = e.cause();
            }
            exit(0);
        }
    };

    let pak0 = match pak::Pak::load("pak0.pak") {
        Ok(p) => p,
        Err(why) => {
            let mut e = why.cause();
            while let Some(c) = e {
                println!("{:?}", c);
                e = c.cause();
            }
            panic!("End of error trace.");
        }
    };

    let mut bsp_data = pak0.open("maps/e1m1.bsp").unwrap();
    let bsp = bsp::Bsp::from_disk(&display, bspload::DiskBsp::load(&mut bsp_data).unwrap());

    let progs = progs::Progs::load(pak0.open("progs.dat").unwrap());

    let mut key_state = event::KeyState::new();
    let player = entity::Entity::new();
    player.set_position(352.0, 88.0, -480.0);

    'outer: loop {
        let pos = player.get_position();
        let angle = player.get_angle();

        let view_matrix = math::Mat4::rotation_x(angle[0]) * math::Mat4::rotation_y(angle[1]) *
                          math::Mat4::translation(-pos[0], -pos[1], -pos[2]);

        bsp.draw_naive(&display, &view_matrix);

        for event in display.poll_events() {
            match event {
                Event::Closed => {
                    debug!("Caught Event::Closed, exiting.");
                    break 'outer;
                }

                Event::KeyboardInput(state, _, key) => {
                    debug!("{:?}", event);
                    if let Some(k) = key {
                        key_state.update(k, state);
                    }
                }
                _ => (),
            }
        }

        if key_state.is_pressed(glium::glutin::VirtualKeyCode::W) {
            let delta = math::Vec3::new(0.0, 0.0, -2.0).rotate_y(player.get_angle()[1]);
            player.adjust_position(delta[0], delta[1], delta[2]);
        }

        if key_state.is_pressed(glium::glutin::VirtualKeyCode::S) {
            let delta = math::Vec3::new(0.0, 0.0, 2.0).rotate_y(player.get_angle()[1]);
            player.adjust_position(delta[0], delta[1], delta[2]);
        }

        if key_state.is_pressed(glium::glutin::VirtualKeyCode::A) {
            let delta = math::Vec3::new(-2.0, 0.0, 0.0).rotate_y(player.get_angle()[1]);
            player.adjust_position(delta[0], delta[1], delta[2]);
        }

        if key_state.is_pressed(glium::glutin::VirtualKeyCode::D) {
            let delta = math::Vec3::new(2.0, 0.0, 0.0).rotate_y(player.get_angle()[1]);
            player.adjust_position(delta[0], delta[1], delta[2]);
        }

        if key_state.is_pressed(glium::glutin::VirtualKeyCode::Space) {
            player.adjust_position(0.0, 2.0, 0.0);
        }

        if key_state.is_pressed(glium::glutin::VirtualKeyCode::LControl) {
            player.adjust_position(0.0, -2.0, 0.0);
        }

        if key_state.is_pressed(glium::glutin::VirtualKeyCode::Left) {
            player.adjust_angle(0.0, 0.05, 0.0);
        }

        if key_state.is_pressed(glium::glutin::VirtualKeyCode::Right) {
            player.adjust_angle(0.0, -0.05, 0.0);
        }

        if key_state.is_pressed(glium::glutin::VirtualKeyCode::Up) {
            player.adjust_angle(-0.05, 0.0, 0.0);
        }

        if key_state.is_pressed(glium::glutin::VirtualKeyCode::Down) {
            player.adjust_angle(0.05, 0.0, 0.0);
        }
    }
}
