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

extern crate byteorder;
extern crate env_logger;
#[macro_use]
extern crate num;
extern crate log;
extern crate pnet;
extern crate richter;

use byteorder::{LittleEndian, ReadBytesExt};
use num::FromPrimitive;
use std::fmt;
use std::io::Cursor;
use std::str::{self, FromStr};
use pnet::packet::Packet;
use pnet::packet::udp::UdpPacket;
use pnet::packet::ip::IpNextHeaderProtocols::Udp;
use pnet::transport::{self, TransportChannelType};
use richter::proto::{self, ClCmd, MoveDelta, MoveDeltaFlags, SvCmd};

static USAGE: &'static str = "Usage: cl-sniff <client IP> <client port> <server IP> <server port>";

const SEQUENCE_RELIABLE: i32 = (1 << 31);

enum QwPacket {
    Oob(OobPacket),
    Netchan(NetchanPacket),
}

impl QwPacket {
    pub fn new<'a>(src: &'a [u8]) -> QwPacket {
        if src.len() < 4 {
            panic!("Packet is too short");
        }

        let mut curs = Cursor::new(src);
        let seq = curs.read_i32::<LittleEndian>().unwrap();

        if seq == -1 {
            return QwPacket::Oob(OobPacket::new(&curs.into_inner()[4..]));
        }

        if src.len() < 8 {
            panic!("Packet is too short for a netchannel packet");
        }

        let ack_seq = curs.read_i32::<LittleEndian>().unwrap();
        let payload = Vec::from(&curs.into_inner()[8..]);

        QwPacket::Netchan(NetchanPacket {
            seq: seq,
            ack: ack_seq,
            payload: payload,
        })
    }
}

enum OobPacket {
    GetChallenge,
    Challenge(i32),
    Connect(ConnectPacket),
    Accept,
    Ping,
    Ack,
    Status,
    Log,
    Rcon,
}

impl OobPacket {
    pub fn new<'a>(src: &'a [u8]) -> OobPacket {
        // TODO: specify a maximum out-of-band packet length
        let mut len = 0;
        while len < src.len() && src[len] != 0 {
            len += 1;
        }

        let cmd_text = String::from(str::from_utf8(&src[..len]).unwrap());
        let mut cmd_args = cmd_text.split_whitespace();

        match cmd_args.next().unwrap() {
            "getchallenge" => OobPacket::GetChallenge,
            "ping" | "k" => OobPacket::Ping,
            "l" => OobPacket::Ack,
            "status" => OobPacket::Status,
            "log" => OobPacket::Log,
            "connect" => {
                let protocol = i32::from_str(cmd_args.next().unwrap()).unwrap();
                let qport = u16::from_str(cmd_args.next().unwrap()).unwrap();
                let challenge = i32::from_str(cmd_args.next().unwrap()).unwrap();

                OobPacket::Connect(ConnectPacket {
                    protocol: protocol,
                    qport: qport,
                    challenge: challenge,
                    userinfo: String::from(cmd_args.next().unwrap()),
                })
            }
            "rcon" => OobPacket::Rcon,
            "j" => OobPacket::Accept,
            s => {
                if s.starts_with("c") {
                    return OobPacket::Challenge(i32::from_str(&s[1..]).unwrap());
                } else {
                    panic!("Unrecognized out-of-band command");
                }
            }
        }
    }
}

struct ConnectPacket {
    protocol: i32,
    qport: u16,
    challenge: i32,
    userinfo: String,
}

impl fmt::Display for ConnectPacket {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,
               "proto={} qport={} challenge={} userinfo={}",
               self.protocol,
               self.qport,
               self.challenge,
               self.userinfo)
    }
}

struct NetchanPacket {
    seq: i32,
    ack: i32,
    payload: Vec<u8>,
}

impl NetchanPacket {
    pub fn get_sequence(&self) -> i32 {
        self.seq & !SEQUENCE_RELIABLE
    }

    pub fn get_sequence_reliable(&self) -> bool {
        self.seq & SEQUENCE_RELIABLE == SEQUENCE_RELIABLE
    }

    pub fn get_ack_sequence(&self) -> i32 {
        self.ack & !SEQUENCE_RELIABLE
    }

    pub fn get_ack_sequence_reliable(&self) -> bool {
        self.ack & SEQUENCE_RELIABLE == SEQUENCE_RELIABLE
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
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

    let (_, mut rx) = match pnet::transport::transport_channel(4096,
                                                               TransportChannelType::Layer3(Udp)) {
        Ok((tx, rx)) => (tx, rx),
        Err(why) => panic!("Error creating channel: {}", why),
    };

    let mut iter = transport::ipv4_packet_iter(&mut rx);
    loop {
        match iter.next() {
            Ok((packet, addr)) => {
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

                let qw_packet = QwPacket::new(udp_packet.payload());
                match qw_packet {
                    QwPacket::Oob(oob_packet) => {
                        match oob_packet {
                            OobPacket::GetChallenge => println!("getchallenge"),
                            OobPacket::Challenge(challenge) => println!("challenge={}", challenge),
                            OobPacket::Connect(conn_packet) => println!("{}", conn_packet),
                            _ => (),
                        }
                    }

                    QwPacket::Netchan(nc_packet) => {
                        print!("seq={} [", nc_packet.get_sequence());
                        if nc_packet.get_sequence_reliable() {
                            print!("R");
                        } else {
                            print!(" ");
                        }
                        print!("] ack={} ", nc_packet.get_ack_sequence());

                        let mut curs = Cursor::new(nc_packet.payload());

                        if dest_ip == server_ip && dest_port == server_port {
                            let qport = curs.read_u16::<LittleEndian>().unwrap();
                            print!("qport={} ", qport);

                            match ClCmd::from_u8(curs.read_u8().unwrap()).unwrap() {
                                ClCmd::Move => {
                                    let flags = MoveDeltaFlags::from_bits(curs.read_u8().unwrap())
                                                    .unwrap();
                                    print!("move: ");

                                    if flags.contains(proto::CM_ANGLE1) {
                                        print!("angle1 ");
                                    }
                                }
                                _ => (),
                            }
                        } else if dest_ip == client_ip && dest_port == client_port {
                        }
                    }
                }

                println!("");
            }

            Err(why) => panic!("Error reading packet: {}"),
        }
    }
}
