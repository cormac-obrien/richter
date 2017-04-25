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
use glium::Surface;
use glium::glutin::{ElementState, Event, VirtualKeyCode as Key};
use richter::bsp;
use richter::client::{Client, CxnStatus};
use richter::console::{CmdRegistry, Console, InputLine, History};
use richter::entity;
use richter::event;
use richter::input::InputState;
use richter::math;
use richter::pak;
use richter::progs;

static POP: [u8; 256] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                         0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x66, 0x00, 0x00, 0x00,
                         0x00, 0x00, 0x00, 0x00, 0x66, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x66,
                         0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x67, 0x00, 0x00,
                         0x00, 0x00, 0x66, 0x65, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                         0x00, 0x65, 0x66, 0x00, 0x00, 0x63, 0x65, 0x61, 0x00, 0x00, 0x00, 0x00,
                         0x00, 0x00, 0x00, 0x00, 0x00, 0x61, 0x65, 0x63, 0x00, 0x64, 0x65, 0x61,
                         0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x61, 0x65, 0x64,
                         0x00, 0x64, 0x65, 0x64, 0x00, 0x00, 0x64, 0x69, 0x69, 0x69, 0x64, 0x00,
                         0x00, 0x64, 0x65, 0x64, 0x00, 0x63, 0x65, 0x68, 0x62, 0x00, 0x00, 0x64,
                         0x68, 0x64, 0x00, 0x00, 0x62, 0x68, 0x65, 0x63, 0x00, 0x00, 0x65, 0x67,
                         0x69, 0x63, 0x00, 0x64, 0x67, 0x64, 0x00, 0x63, 0x69, 0x67, 0x65, 0x00,
                         0x00, 0x00, 0x62, 0x66, 0x67, 0x69, 0x6a, 0x68, 0x67, 0x68, 0x6a, 0x69,
                         0x67, 0x66, 0x62, 0x00, 0x00, 0x00, 0x00, 0x62, 0x65, 0x66, 0x66, 0x66,
                         0x66, 0x66, 0x66, 0x66, 0x65, 0x62, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                         0x00, 0x62, 0x63, 0x64, 0x66, 0x64, 0x63, 0x62, 0x00, 0x00, 0x00, 0x00,
                         0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x62, 0x66, 0x62, 0x00, 0x00,
                         0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x61,
                         0x66, 0x61, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                         0x00, 0x00, 0x00, 0x00, 0x65, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                         0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x64, 0x00, 0x00, 0x00,
                         0x00, 0x00, 0x00, 0x00];

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

    let input = InputState::new();
    let mut con = Console::new();
    let mut reg = CmdRegistry::new();
    let mut cmdline = InputLine::new();
    let mut hist = History::new();
    con.add_cvar("show_fps", "0", false, false).unwrap();
    con.add_cvar("host_speeds", "0", false, false).unwrap();
    con.add_cvar("developer", "0", false, false).unwrap();
    con.add_cvar("cl_warncmd", "0", false, false).unwrap();
    con.add_cvar("cl_upspeed", "200", false, false).unwrap();
    con.add_cvar("cl_forwardspeed", "200", true, false).unwrap();
    con.add_cvar("cl_backspeed", "200", true, false).unwrap();
    con.add_cvar("cl_sidespeed", "350", false, false).unwrap();
    con.add_cvar("cl_movespeedkey", "2.0", false, false).unwrap();
    con.add_cvar("cl_yawspeed", "140", false, false).unwrap();
    con.add_cvar("cl_pitchspeed", "150", false, false).unwrap();
    con.add_cvar("cl_anglespeedkey", "1.5", false, false).unwrap();
    con.add_cvar("cl_shownet", "0", false, false).unwrap();
    con.add_cvar("cl_sbar", "0", true, false).unwrap();
    con.add_cvar("cl_hudswap", "0", true, false).unwrap();
    con.add_cvar("cl_maxfps", "0", true, false).unwrap();
    con.add_cvar("lookspring", "0", true, false).unwrap();
    con.add_cvar("lookstrafe", "0", true, false).unwrap();
    con.add_cvar("sensitivity", "3", true, false).unwrap();
    con.add_cvar("m_pitch", "0.022", true, false).unwrap();
    con.add_cvar("m_yaw", "0.022", false, false).unwrap();
    con.add_cvar("m_forward", "1", false, false).unwrap();
    con.add_cvar("m_side", "0.8", false, false).unwrap();
    con.add_cvar("rcon_password", "", false, false).unwrap();
    con.add_cvar("rcon_address", "", false, false).unwrap();
    con.add_cvar("entlatency", "20", false, false).unwrap();
    con.add_cvar("cl_predict_players2", "1", false, false).unwrap();
    con.add_cvar("cl_predict_players", "1", false, false).unwrap();
    con.add_cvar("cl_solid_players", "1", false, false).unwrap();
    con.add_cvar("localid", "", false, false).unwrap();
    con.add_cvar("baseskin", "base", false, false).unwrap();
    con.add_cvar("noskins", "0", false, false).unwrap();

    // userinfo cvars
    con.add_cvar("name", "unnamed", true, true).unwrap();
    con.add_cvar("password", "", false, true).unwrap();
    con.add_cvar("spectator", "", false, true).unwrap();
    con.add_cvar("skin", "", true, true).unwrap();
    con.add_cvar("team", "", true, true).unwrap();
    con.add_cvar("topcolor", "0", true, true).unwrap();
    con.add_cvar("bottomcolor", "0", true, true).unwrap();
    con.add_cvar("rate", "2500", true, true).unwrap();
    con.add_cvar("msg", "1", true, true).unwrap();
    con.add_cvar("noaim", "0", true, true).unwrap();

    // TODO: write an actual quit function
    reg.add_cmd("quit", Box::new(|| std::process::exit(0))).unwrap();

    // input commands
    reg.add_cmd("+forward", Box::new(|| input.forward.set(true))).unwrap();
    reg.add_cmd("-forward", Box::new(|| input.forward.set(false))).unwrap();
    reg.add_cmd("+back", Box::new(|| input.back.set(true))).unwrap();
    reg.add_cmd("-back", Box::new(|| input.back.set(false))).unwrap();
    reg.add_cmd("+moveleft", Box::new(|| input.moveleft.set(true))).unwrap();
    reg.add_cmd("-moveleft", Box::new(|| input.moveleft.set(false))).unwrap();
    reg.add_cmd("+moveright", Box::new(|| input.moveright.set(true))).unwrap();
    reg.add_cmd("-moveright", Box::new(|| input.moveright.set(false))).unwrap();
    reg.add_cmd("+moveup", Box::new(|| input.moveup.set(true))).unwrap();
    reg.add_cmd("-moveup", Box::new(|| input.moveup.set(false))).unwrap();
    reg.add_cmd("+movedown", Box::new(|| input.movedown.set(true))).unwrap();
    reg.add_cmd("-movedown", Box::new(|| input.movedown.set(false))).unwrap();
    reg.add_cmd("+left", Box::new(|| input.left.set(true))).unwrap();
    reg.add_cmd("-left", Box::new(|| input.left.set(false))).unwrap();
    reg.add_cmd("+right", Box::new(|| input.right.set(true))).unwrap();
    reg.add_cmd("-right", Box::new(|| input.right.set(false))).unwrap();
    reg.add_cmd("+lookup", Box::new(|| input.lookup.set(true))).unwrap();
    reg.add_cmd("-lookup", Box::new(|| input.lookup.set(false))).unwrap();
    reg.add_cmd("+lookdown", Box::new(|| input.lookdown.set(true))).unwrap();
    reg.add_cmd("-lookdown", Box::new(|| input.lookdown.set(false))).unwrap();
    reg.add_cmd("+speed", Box::new(|| input.speed.set(true))).unwrap();
    reg.add_cmd("-speed", Box::new(|| input.speed.set(false))).unwrap();
    reg.add_cmd("+jump", Box::new(|| input.jump.set(true))).unwrap();
    reg.add_cmd("-jump", Box::new(|| input.jump.set(false))).unwrap();
    reg.add_cmd("+strafe", Box::new(|| input.strafe.set(true))).unwrap();
    reg.add_cmd("-strafe", Box::new(|| input.strafe.set(false))).unwrap();
    reg.add_cmd("+attack", Box::new(|| input.attack.set(true))).unwrap();
    reg.add_cmd("-attack", Box::new(|| input.attack.set(false))).unwrap();
    reg.add_cmd("+use", Box::new(|| input.use_.set(true))).unwrap();
    reg.add_cmd("-use", Box::new(|| input.use_.set(false))).unwrap();
    reg.add_cmd("+klook", Box::new(|| input.klook.set(true))).unwrap();
    reg.add_cmd("-klook", Box::new(|| input.klook.set(false))).unwrap();
    reg.add_cmd("+mlook", Box::new(|| input.mlook.set(true))).unwrap();
    reg.add_cmd("-mlook", Box::new(|| input.mlook.set(false))).unwrap();
    reg.add_cmd("+showscores", Box::new(|| input.showscores.set(true))).unwrap();
    reg.add_cmd("-showscores", Box::new(|| input.showscores.set(false))).unwrap();
    reg.add_cmd("+showteamscores",
                Box::new(|| input.showteamscores.set(true)))
       .unwrap();
    reg.add_cmd("-showteamscores",
                Box::new(|| input.showteamscores.set(false)))
       .unwrap();

    loop {
        for event in display.poll_events() {
            match event {
                Event::ReceivedCharacter(c) => {
                    info!("Got char {:?}", c);
                    match c {
                        // backspace
                        '\x08' => cmdline.backspace(),

                        // delete
                        '\x7f' => cmdline.delete(),

                        // TODO: tab completion
                        '\t' => (),

                        _ => cmdline.insert(c),
                    }
                    println!("{}", cmdline.debug_string());

                }

                Event::KeyboardInput(ElementState::Pressed, _, Some(key)) => {
                    match key {
                        Key::Right => cmdline.cursor_right(),
                        Key::Left => cmdline.cursor_left(),
                        Key::Up => cmdline.set_text(&hist.line_up()),
                        Key::Down => cmdline.set_text(&hist.line_down()),
                        Key::Return => {
                            use std::iter::FromIterator;
                            let line = cmdline.get_text();
                            let cmd = String::from_iter(line.to_owned());
                            let mut args = cmd.split_whitespace();

                            let name = match args.next() {
                                Some(n) => n,
                                None => break,
                            };

                            if let Err(_) = reg.exec_cmd(name, args.collect()) {
                                println!("Error executing \"{}\"", name);
                            }

                            hist.add_line(line);
                            cmdline.clear();
                        }

                        _ => (),
                    }
                    println!("{}", cmdline.debug_string());

                }

                Event::Closed => {
                    exit(0);
                }

                _ => (),
            }

            let mut frame = display.draw();
            frame.clear_color(0.0, 0.0, 0.0, 0.0);
            frame.clear_depth(0.0);
            frame.finish().unwrap();

            display.swap_buffers().unwrap();
        }
    }

    let cl = Client::connect(Ipv4Addr::new(127, 0, 0, 1));

    loop {
        frame(&cl);
    }

    exit(0);

    use glium::DisplayBuild;

    let mut pak = pak::Pak::new();
    match pak.add("pak0.pak") {
        Ok(_) => (),
        Err(why) => {
            let mut e = why.cause();
            while let Some(c) = e {
                println!("{:?}", c);
                e = c.cause();
            }
            panic!("End of error trace.");
        }
    }

    let bsp_data = pak.open("maps/e1m1.bsp").unwrap();
    let bsp = bsp::Bsp::load(&display, bsp_data);

    let progs = progs::Progs::load(pak.open("progs.dat").unwrap());

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
