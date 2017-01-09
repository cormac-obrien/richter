// Copyright © 2016 Cormac O'Brien
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

use std;
use std::cell::Cell;
use std::collections::HashMap;
use std::io::{Cursor, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, ToSocketAddrs, UdpSocket};
use std::str::{self, FromStr};
use time::{Duration, PreciseTime};
use protocol::{self, SvCmd, UserInfo};
use net::{Message, NetworkBuffer, NetworkChannel};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

const RETRY_INTERVAL: i64 = 5;

#[derive(Copy, Clone, PartialEq, PartialOrd)]
pub enum CxnStatus {
    Disconnected = 0,
    DemoStart = 1,
    Connected = 2,
    OnServer = 3,
    Active = 4,
}

pub struct Client {
    netchannel: NetworkChannel,
    challenge: Cell<i32>,
    cxn_status: Cell<CxnStatus>,

    /// The last time this client sent a connection request to the server.
    cxn_time: Cell<Option<PreciseTime>>,
    userinfo: UserInfo,
}

impl Client {
    /// Create a new `Client` by initiating a connection to `server`. The client
    /// will request a challenge from the server and then begin normal operation.
    pub fn connect<A>(server: A) -> Client
        where A: ToSocketAddrs
    {

        let client = Client {
            netchannel: NetworkChannel::new(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1),
                                                              protocol::PORT_SERVER),
                                            protocol::PORT_CLIENT),
            challenge: Cell::new(0),
            userinfo: UserInfo::default(),
            cxn_status: Cell::new(CxnStatus::Disconnected),
            cxn_time: Cell::new(None),
        };

        client.netchannel.out_of_band("getchallenge\n".as_bytes());
        client
    }

    /// Sends a connection message to the server.
    ///
    /// Connection messages are out-band-messages of the form:
    ///
    /// ```
    /// connect <protocol> <qport> <challenge> <userinfo>
    /// ```
    ///
    /// If all goes well, the server will reply with an out-of-band message
    /// containing a single 'j'.
    pub fn send_connect(&self) {
        self.netchannel.out_of_band(format!("connect {} {} {} \"{}\"",
                                            protocol::VERSION,
                                            27001,
                                            self.challenge.get(),
                                            self.userinfo.serialize())
                                        .as_bytes());
        self.cxn_time.set(Some(PreciseTime::now()));
    }


    pub fn retry_connect(&self) {
        match self.cxn_time.get() {
            // Has it been 5 seconds since last connection attempt?
            Some(x) if x.to(PreciseTime::now()) > Duration::seconds(RETRY_INTERVAL) => {
                debug!("Five seconds elapsed, retrying...");
                self.send_connect();
            }

            _ => (),
        }
    }

    pub fn read_packets(&self) {
        while let Some(msg) = self.netchannel.process() {
            match msg {
                Message::InBand(mut msg) => self.parse_msg(&mut *msg),
                Message::OutOfBand(mut msg) => self.proc_outofband(&mut *msg),
            }
        }
    }

    pub fn parse_msg(&self, msg: &mut NetworkBuffer) {
        loop {
            use num::FromPrimitive;

            let first = msg.read_u8().unwrap();

            if first == std::u8::MAX {
                // msg_readcount++
                break;
            }

            let cmd: SvCmd = match FromPrimitive::from_u8(first) {
                Some(c) => c,
                None => break,
            };

            match cmd {
                SvCmd::ServerData => self.parse_serverdata(msg),
                _ => panic!("No handler for {:?}", cmd),
            };
        }
    }

    /// Parse data sent by the server in a SvCmd::ServerData message.
    ///
    /// see Quake/QW/client/cl_parse.c:525
    ///
    /// Server data is sent in the following format:
    /// ```
    /// protocol version: u32
    /// server count: u32
    /// game directory: null-terminated string
    /// player number: u8
    /// level name: null-terminated string
    /// gravity: f32
    /// max speed: f32
    /// max spec speed: f32
    /// acceleration: f32
    /// air acceleration: f32
    /// water acceleration: f32
    /// friction: f32
    /// water friction: f32
    /// entity gravity: f32
    /// ```
    pub fn parse_serverdata(&self, msg: &mut NetworkBuffer) {
        let proto = msg.read_u32::<LittleEndian>().unwrap();

        if proto != protocol::VERSION {
            // TODO: allow demo playback on versions 26-29
            // otherwise end the game
            panic!("Bad version handler unimplemented");
        }

        let servcount = msg.read_u32::<LittleEndian>().unwrap();

        let mut gamedir_bytes: Vec<u8> = Vec::new();
        loop {
            match msg.read_u8().unwrap() {
                0 => break,
                c => gamedir_bytes.push(c),
            }
        }
        let gamedir_str = String::from_utf8(gamedir_bytes).unwrap();

        // TODO: if current game dir differs, do host_writeconfig
    }

    pub fn proc_outofband(&self, msg: &mut NetworkBuffer) {
        match msg.read_u8().unwrap() as char {
            'c' => {
                // challenge
                debug!("Received challenge from server");
                let mut challenge_bytes = Vec::new();
                msg.read_to_end(&mut challenge_bytes);

                let challenge_str = match str::from_utf8(&challenge_bytes) {
                    Ok(s) => s,
                    Err(why) => {
                        warn!("Bad challenge from server.");
                        ""
                    }
                };

                debug!("{:?}", challenge_str);

                let challenge = match i32::from_str(&challenge_str) {
                    Ok(n) => n,
                    Err(why) => {
                        warn!("Bad challenge from server ({}): \"{}\"", why, challenge_str);
                        0
                    }
                };

                self.challenge.set(challenge);
                self.send_connect();
            }

            'j' => {
                // connection
                if self.cxn_status.get() >= CxnStatus::Connected {
                    return;
                }

                debug!("Server accepted connection request");
                self.cxn_status.set(CxnStatus::Connected);

                let mut msg: Vec<u8> = Vec::new();
                let mut cursor = Cursor::new(msg);
                cursor.write("new".as_bytes()).unwrap();
                cursor.write_u8(0).unwrap();
            }

            'k' => {
                // ping
                debug!("Received ping");
                self.netchannel.out_of_band(&['l' as u8, 0]);
            }

            _ => panic!("Unrecognized out-of-band message"),
        }
    }

    pub fn get_cxn_status(&self) -> CxnStatus {
        self.cxn_status.get()
    }
}
