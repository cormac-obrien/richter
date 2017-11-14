// Copyright © 2017 Cormac O'Brien
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
extern crate winit;

use std::error::Error;
use std::process::exit;

use richter::bsp;
use richter::client::{Client, CxnStatus};
use richter::console::{CmdRegistry, Console, InputLine, History};
use richter::entity;
use richter::event;
use richter::input::{InputFocus, InputState};
use richter::math;
use richter::pak;
use richter::progs;

static POP: [u8; 256] = [
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x66,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x66,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x66,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x67,
    0x00,
    0x00,
    0x00,
    0x00,
    0x66,
    0x65,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x65,
    0x66,
    0x00,
    0x00,
    0x63,
    0x65,
    0x61,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x61,
    0x65,
    0x63,
    0x00,
    0x64,
    0x65,
    0x61,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x61,
    0x65,
    0x64,
    0x00,
    0x64,
    0x65,
    0x64,
    0x00,
    0x00,
    0x64,
    0x69,
    0x69,
    0x69,
    0x64,
    0x00,
    0x00,
    0x64,
    0x65,
    0x64,
    0x00,
    0x63,
    0x65,
    0x68,
    0x62,
    0x00,
    0x00,
    0x64,
    0x68,
    0x64,
    0x00,
    0x00,
    0x62,
    0x68,
    0x65,
    0x63,
    0x00,
    0x00,
    0x65,
    0x67,
    0x69,
    0x63,
    0x00,
    0x64,
    0x67,
    0x64,
    0x00,
    0x63,
    0x69,
    0x67,
    0x65,
    0x00,
    0x00,
    0x00,
    0x62,
    0x66,
    0x67,
    0x69,
    0x6a,
    0x68,
    0x67,
    0x68,
    0x6a,
    0x69,
    0x67,
    0x66,
    0x62,
    0x00,
    0x00,
    0x00,
    0x00,
    0x62,
    0x65,
    0x66,
    0x66,
    0x66,
    0x66,
    0x66,
    0x66,
    0x66,
    0x65,
    0x62,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x62,
    0x63,
    0x64,
    0x66,
    0x64,
    0x63,
    0x62,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x62,
    0x66,
    0x62,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x61,
    0x66,
    0x61,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x65,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x64,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
];

fn frame(cl: &mut Client) {
    // TODO: handle key input
    // TODO: handle mouse/controller input
    // TODO: run console/script commands

    match cl.read_packets() {
        Ok(_) => (),
        Err(ref why) => {
            println!("{}", why);
            let mut e: &Error = why;
            while let Some(c) = e.cause() {
                println!("{}", c);
                e = c;
            }
            exit(1);
        }
    }

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

    let mut cl: Option<Client> = None;

    info!("Successfully constructed display");

    let focus = InputFocus::Game;

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
    con.add_cvar("cl_movespeedkey", "2.0", false, false)
        .unwrap();
    con.add_cvar("cl_yawspeed", "140", false, false).unwrap();
    con.add_cvar("cl_pitchspeed", "150", false, false).unwrap();
    con.add_cvar("cl_anglespeedkey", "1.5", false, false)
        .unwrap();
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
    con.add_cvar("cl_predict_players2", "1", false, false)
        .unwrap();
    con.add_cvar("cl_predict_players", "1", false, false)
        .unwrap();
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
    reg.add_cmd("quit", Box::new(|_| std::process::exit(0)))
        .unwrap();

    // input commands
    reg.add_cmd("+forward", Box::new(|_| input.forward.set(true)))
        .unwrap();
    reg.add_cmd("-forward", Box::new(|_| input.forward.set(false)))
        .unwrap();
    reg.add_cmd("+back", Box::new(|_| input.back.set(true)))
        .unwrap();
    reg.add_cmd("-back", Box::new(|_| input.back.set(false)))
        .unwrap();
    reg.add_cmd("+moveleft", Box::new(|_| input.moveleft.set(true)))
        .unwrap();
    reg.add_cmd("-moveleft", Box::new(|_| input.moveleft.set(false)))
        .unwrap();
    reg.add_cmd("+moveright", Box::new(|_| input.moveright.set(true)))
        .unwrap();
    reg.add_cmd("-moveright", Box::new(|_| input.moveright.set(false)))
        .unwrap();
    reg.add_cmd("+moveup", Box::new(|_| input.moveup.set(true)))
        .unwrap();
    reg.add_cmd("-moveup", Box::new(|_| input.moveup.set(false)))
        .unwrap();
    reg.add_cmd("+movedown", Box::new(|_| input.movedown.set(true)))
        .unwrap();
    reg.add_cmd("-movedown", Box::new(|_| input.movedown.set(false)))
        .unwrap();
    reg.add_cmd("+left", Box::new(|_| input.left.set(true)))
        .unwrap();
    reg.add_cmd("-left", Box::new(|_| input.left.set(false)))
        .unwrap();
    reg.add_cmd("+right", Box::new(|_| input.right.set(true)))
        .unwrap();
    reg.add_cmd("-right", Box::new(|_| input.right.set(false)))
        .unwrap();
    reg.add_cmd("+lookup", Box::new(|_| input.lookup.set(true)))
        .unwrap();
    reg.add_cmd("-lookup", Box::new(|_| input.lookup.set(false)))
        .unwrap();
    reg.add_cmd("+lookdown", Box::new(|_| input.lookdown.set(true)))
        .unwrap();
    reg.add_cmd("-lookdown", Box::new(|_| input.lookdown.set(false)))
        .unwrap();
    reg.add_cmd("+speed", Box::new(|_| input.speed.set(true)))
        .unwrap();
    reg.add_cmd("-speed", Box::new(|_| input.speed.set(false)))
        .unwrap();
    reg.add_cmd("+jump", Box::new(|_| input.jump.set(true)))
        .unwrap();
    reg.add_cmd("-jump", Box::new(|_| input.jump.set(false)))
        .unwrap();
    reg.add_cmd("+strafe", Box::new(|_| input.strafe.set(true)))
        .unwrap();
    reg.add_cmd("-strafe", Box::new(|_| input.strafe.set(false)))
        .unwrap();
    reg.add_cmd("+attack", Box::new(|_| input.attack.set(true)))
        .unwrap();
    reg.add_cmd("-attack", Box::new(|_| input.attack.set(false)))
        .unwrap();
    reg.add_cmd("+use", Box::new(|_| input.use_.set(true)))
        .unwrap();
    reg.add_cmd("-use", Box::new(|_| input.use_.set(false)))
        .unwrap();
    reg.add_cmd("+klook", Box::new(|_| input.klook.set(true)))
        .unwrap();
    reg.add_cmd("-klook", Box::new(|_| input.klook.set(false)))
        .unwrap();
    reg.add_cmd("+mlook", Box::new(|_| input.mlook.set(true)))
        .unwrap();
    reg.add_cmd("-mlook", Box::new(|_| input.mlook.set(false)))
        .unwrap();
    reg.add_cmd("+showscores", Box::new(|_| input.showscores.set(true)))
        .unwrap();
    reg.add_cmd("-showscores", Box::new(|_| input.showscores.set(false)))
        .unwrap();
    reg.add_cmd(
        "+showteamscores",
        Box::new(|_| input.showteamscores.set(true)),
    ).unwrap();
    reg.add_cmd(
        "-showteamscores",
        Box::new(|_| input.showteamscores.set(false)),
    ).unwrap();

    reg.add_cmd(
        "connect",
        Box::new(|args| {
            println!("{:?}", args);
        }),
    ).unwrap();

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

    let e1m1 = bsp::Bsp::load("pak0.pak.d/maps/e1m1.bsp");
    let progs = progs::Progs::load(pak.open("progs.dat").unwrap());

    let mut key_state = event::KeyState::new();
}
