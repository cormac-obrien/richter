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

extern crate byteorder;
extern crate num;
extern crate pnet;
extern crate richter;

use byteorder::{LittleEndian, ReadBytesExt};
use num::FromPrimitive;
use std::io::Cursor;
use std::str::{self, FromStr};
use pnet::packet::Packet;
use pnet::packet::udp::UdpPacket;
use pnet::packet::ip::IpNextHeaderProtocols::Udp;
use pnet::transport::{self, TransportChannelType};
use richter::qw::{self, ClCmd, MoveDeltaFlags, SvCmd};
use richter::util;

static USAGE: &'static str = "Usage: cl-sniff <client IP> <client port> <server IP> <server port>";

fn transcribe_clcmd<'a>(src: &'a [u8]) -> String {
    let mut result = String::new();
    let mut curs = Cursor::new(src);

    let qport = curs.read_u16::<LittleEndian>().unwrap();
    result += &format!("qport={} ", qport);

    let opcode = match curs.read_u8() {
        Ok(o) => o,
        Err(why) => {
            result += &format!("ERROR: {}", why);
            return result;
        }
    };

    let clcmd = match ClCmd::from_u8(opcode) {
        Some(c) => c,
        None => {
            result += &format!("ERROR: Unrecognized opcode {}", opcode);
            return result;
        }
    };

    match clcmd {

        // TODO: use MoveDelta::from_bytes
        ClCmd::Move => {
            result += &format!("Move ");
            result += &format!("crc={} ", curs.read_u8().unwrap());
            result += &format!("loss={} ", curs.read_u8().unwrap());
            result += "\n";

            for i in 0..3 {
                result += &format!("| [D{}] ", i);

                let flags = MoveDeltaFlags::from_bits(curs.read_u8().unwrap()).unwrap();

                result += &format!("flags={:08b} ", flags.bits());

                if flags.contains(MoveDeltaFlags::MOVE_ANGLE1) {
                    result += &format!("angle1={} ", curs.read_u16::<LittleEndian>().unwrap());
                }

                if flags.contains(MoveDeltaFlags::MOVE_ANGLE2) {
                    result += &format!("angle2={} ", curs.read_u16::<LittleEndian>().unwrap());
                }

                if flags.contains(MoveDeltaFlags::MOVE_ANGLE3) {
                    result += &format!("angle3={} ", curs.read_u16::<LittleEndian>().unwrap());
                }

                if flags.contains(MoveDeltaFlags::MOVE_FORWARD) {
                    result += &format!("forward={} ", curs.read_u16::<LittleEndian>().unwrap());
                }

                if flags.contains(MoveDeltaFlags::MOVE_SIDE) {
                    result += &format!("side={} ", curs.read_u16::<LittleEndian>().unwrap());
                }

                if flags.contains(MoveDeltaFlags::MOVE_UP) {
                    result += &format!("up={} ", curs.read_u16::<LittleEndian>().unwrap());
                }

                if flags.contains(MoveDeltaFlags::MOVE_BUTTONS) {
                    result += &format!("buttons={} ", curs.read_u8().unwrap());
                }

                if flags.contains(MoveDeltaFlags::MOVE_IMPULSE) {
                    result += &format!("impulse={} ", curs.read_u8().unwrap());
                }

                result += &format!("msec={} ", curs.read_u8().unwrap());
                result += "\n";
            }
        }

        ClCmd::StringCmd => {
            let cmd = util::read_cstring(&mut curs).unwrap();
            result += &format!("cmd=\"{}\"", cmd);
        }

        c => {
            result += &format!("{:?} ", c);
        }
    }

    return result;
}

fn transcribe_svcmd<'a>(src: &'a [u8]) -> String {
    let mut result = String::new();
    let mut curs = Cursor::new(src);

    let cmdbyte = curs.read_u8().unwrap();

    match SvCmd::from_u8(cmdbyte).unwrap() {
        SvCmd::Disconnect => {
            result += "disconnect";
        }

        SvCmd::ModelList => {
            result += "modellist ";

            let modellist = qw::ModelListPacket::from_bytes(&mut curs).unwrap();

            result += &format!("count={} ", modellist.get_count());

            let list = modellist.get_list();
            result += "\nMODEL LIST\n==========\n";
            for model in list.into_iter() {
                result += &format!("\"{}\"\n", model);
            }
            result += "==========\n";
            result += &format!("progress={}\n", modellist.get_progress());
        }

        SvCmd::PlayerInfo => {
            result += "playerinfo ";

            let playerinfo = qw::PlayerInfoPacket::from_bytes(&mut curs).unwrap();
            result += &format!("{:#?} ", playerinfo);
        }

        SvCmd::Print => {
            result += "print ";

            let print_type = qw::PrintType::from_u8(curs.read_u8().unwrap()).unwrap();
            result += &format!("type={:?} ", print_type);

            let msg = util::read_cstring(&mut curs).unwrap();
            result += &format!("msg=\"{}\"", msg);
        }

        SvCmd::ServerData => {
            result += "serverdata ";
            let data = qw::ServerDataPacket::from_bytes(&mut curs).unwrap();

            result += &format!("proto={} ", data.get_protocol_version());
            result += &format!("servercount={} ", data.get_server_count());
            result += &format!("gamedir=\"{}\" ", data.get_game_directory());
            result += &format!("playerno={} ", data.get_player_number());
            result += &format!("levelname=\"{}\" ", data.get_level_name());
            result += &format!("gravity={} ", data.get_gravity());
            result += &format!("stopspeed={} ", data.get_stop_speed());
            result += &format!("maxspeed={} ", data.get_max_speed());
            result += &format!("spectatormaxspeed={} ", data.get_spec_max_speed());
            result += &format!("accelerate={} ", data.get_accelerate());
            result += &format!("airaccelerate={} ", data.get_air_accelerate());
            result += &format!("wateraccelerate={} ", data.get_water_accelerate());
            result += &format!("friction={} ", data.get_friction());
            result += &format!("waterfriction={} ", data.get_water_friction());
            result += &format!("entgravity={} ", data.get_ent_gravity());
        }

        SvCmd::SoundList => {
            result += "soundlist ";

            let soundlist = qw::SoundListPacket::from_bytes(&mut curs).unwrap();

            result += &format!("count={} ", soundlist.get_count());

            let list = soundlist.get_list();
            result += "\nSOUND LIST\n==========\n";
            for sound in list.into_iter() {
                result += &format!("\"{}\"\n", sound);
            }
            result += "==========\n";
            result += &format!("progress={}\n", soundlist.get_progress());
        }

        s => {
            result += &format!("{:?} ", s);
        }
    }

    return result;
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 5 {
        println!("{}", USAGE);
        std::process::exit(1);
    }

    let client_ip = std::net::Ipv4Addr::from_str(&args[1]).unwrap();
    let client_port = u16::from_str(&args[2]).unwrap();
    let server_ip = std::net::Ipv4Addr::from_str(&args[3]).unwrap();
    let server_port = u16::from_str(&args[4]).unwrap();

    let (_, mut rx) =
        match pnet::transport::transport_channel(4096, TransportChannelType::Layer3(Udp)) {
            Ok((tx, rx)) => (tx, rx),
            Err(why) => panic!("Error creating channel: {}", why),
        };

    let mut iter = transport::ipv4_packet_iter(&mut rx);
    loop {
        match iter.next() {
            Ok((packet, _)) => {
                let udp_packet = UdpPacket::new(packet.payload()).unwrap();
                let dest_ip = packet.get_destination();
                let dest_port = udp_packet.get_destination();

                if dest_ip == server_ip && dest_port == server_port {
                    print!("CL: ");
                } else if dest_ip == client_ip && dest_port == client_port {
                    print!("SV: ");
                } else {
                    continue;
                }

                let qw_packet = qw::Packet::new(udp_packet.payload()).unwrap();
                match qw_packet {
                    qw::Packet::OutOfBand(oob_packet) => {
                        match oob_packet {
                            qw::OutOfBandPacket::GetChallenge => print!("getchallenge"),
                            qw::OutOfBandPacket::Challenge(challenge) => {
                                print!("challenge={}", challenge)
                            }
                            qw::OutOfBandPacket::Connect(conn_packet) => print!("{}", conn_packet),
                            qw::OutOfBandPacket::Accept => print!("accept"),
                            _ => (),
                        }
                    }

                    qw::Packet::NetChan(nc_packet) => {
                        print!("seq={} [", nc_packet.get_sequence());
                        if nc_packet.get_sequence_reliable() {
                            print!("R");
                        } else {
                            print!(" ");
                        }
                        print!("] ack={} ", nc_packet.get_ack_sequence());

                        if dest_ip == server_ip && dest_port == server_port {
                            print!("{} ", transcribe_clcmd(nc_packet.payload()));
                        } else if dest_ip == client_ip && dest_port == client_port {
                            if nc_packet.payload().len() != 0 {
                                print!("{} ", transcribe_svcmd(nc_packet.payload()));
                            } else {
                                print!("empty");
                            }
                        }
                    }
                }

                println!("");
            }

            Err(why) => panic!("Error reading packet: {}", why),
        }
    }
}
