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
#[macro_use]
extern crate log;
extern crate pnet;
extern crate richter;

use std::str::FromStr;
use pnet::packet::Packet;
use pnet::packet::udp::UdpPacket;
use pnet::packet::ip::IpNextHeaderProtocols::Udp;
use pnet::transport::{self, TransportChannelType};

static USAGE: &'static str = "Usage: cl-sniff <client IP> <client port>";

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        println!("{}", USAGE);
        std::process::exit(1);
    }

    let target_ip = std::net::Ipv4Addr::from_str(&args[1]).unwrap();
    let target_port = u16::from_str(&args[2]).unwrap();

    let (mut tx, mut rx) =
        match pnet::transport::transport_channel(4096, TransportChannelType::Layer3(Udp)) {
            Ok((tx, rx)) => (tx, rx),
            Err(why) => panic!("Error creating channel: {}", why),
        };

    let mut iter = transport::ipv4_packet_iter(&mut rx);
    loop {
        match iter.next() {
            Ok((packet, addr)) => {
                if packet.get_destination() == target_ip {
                    let payload = packet.payload();
                    let udp_packet = UdpPacket::new(payload).unwrap();
                    println!("port: {}", udp_packet.get_destination());
                }
            }

            Err(why) => panic!("Error reading packet: {}"),
        }
    }
}
