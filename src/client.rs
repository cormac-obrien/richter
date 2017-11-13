// Copyright © 2017 Cormac O'Brien
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

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use math::Vec3;
use qw::{self, ClCmd, Message, PacketEntities, QwSocket, SockType, SvCmd, UserCmd, UserInfo};
use std;
use std::default::Default;
use std::io::{Cursor, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::str::{self, FromStr};
use time::{Duration, PreciseTime};

const RETRY_INTERVAL: i64 = 5;
const MAX_CLIENTS: usize = 32;
const MAX_QUEUED_UPDATES: usize = 64;

#[derive(Copy, Clone, PartialEq, PartialOrd)]
pub enum CxnStatus {
    Disconnected = 0,
    DemoStart = 1,
    Connected = 2,
    OnServer = 3,
    Active = 4,
}

#[derive(Copy, Clone)]
pub struct PlayerState {
    message: usize,
    time: PreciseTime,
    cmd: UserCmd,
    origin: Vec3,
    angles: Vec3,
    velocity: Vec3, // TODO: finish this up
}

pub struct Update {
    // main content of the update
    cmd: UserCmd,

    // when this update was initially sent
    time_sent: Duration,

    // sequence number to delta from, or None if full update
    delta: Option<usize>,

    // when this update was received, or None if it hasn't been
    time_received: Option<Duration>,

    // last updated position of each player
    player_state: [Option<PlayerState>; MAX_CLIENTS],

    packet_entities: PacketEntities,

    // whether the packet_entities delta was valid
    valid: bool,
}

impl Default for Update {
    fn default() -> Update {
        Update {
            cmd: UserCmd::default(),
            time_sent: Duration::zero(),
            delta: None,
            time_received: None,
            player_state: [None; MAX_CLIENTS],
            packet_entities: Box::default(),
            valid: false,
        }
    }
}

pub struct Client {
    socket: QwSocket,
    challenge: i32,
    cxn_status: CxnStatus,

    /// The last time this client sent a connection request to the server.
    cxn_time: Option<PreciseTime>,
    user_info: UserInfo,

    /// The last MAX_QUEUED_UPDATES updates the client received.
    updates: Vec<Update>,
}

impl Client {
    /// Create a new `Client` by initiating a connection to `server`. The client
    /// will request a challenge from the server and then begin normal operation.
    pub fn connect(server: Ipv4Addr) -> Client {
        let remote = SocketAddr::V4(SocketAddrV4::new(server, qw::PORT_SERVER));

        let mut client = Client {
            socket: QwSocket::bind(remote, SockType::Client).unwrap(),
            challenge: 0,
            cxn_status: CxnStatus::Disconnected,
            cxn_time: None,
            user_info: UserInfo::default(),
            updates: Vec::new(),
        };

        client
            .socket
            .out_of_band("getchallenge\n".as_bytes())
            .unwrap();
        client
    }

    /// Sends a connection message to the server.
    ///
    /// Connection messages are out-band-messages of the form:
    ///
    /// ```
    /// connect <qw> <qport> <challenge> <userinfo>
    /// ```
    ///
    /// If all goes well, the server will reply with an out-of-band message
    /// containing a single 'j'.
    pub fn send_connect(&mut self) {
        let connect = format!(
            "connect {} {} {} \"{}\"",
            qw::VERSION,
            27001,
            self.challenge,
            self.user_info.serialize()
        );
        debug!("Sending connect message: {}", connect);
        self.socket.out_of_band(connect.as_bytes()).unwrap();
        self.cxn_time = Some(PreciseTime::now());
    }


    pub fn retry_connect(&mut self) {
        if let Some(x) = self.cxn_time {
            if x.to(PreciseTime::now()) > Duration::seconds(RETRY_INTERVAL) {
                debug!("Five seconds elapsed, retrying...");
                self.send_connect();
            }
        }
    }

    pub fn read_packets(&mut self) -> Result<(), qw::NetworkError> {
        loop {
            match self.socket.process()? {
                Message::InBand => self.parse_msg(),
                Message::OutOfBand(mut msg) => self.proc_outofband(&mut *msg),
                Message::None => break,
            }
        }

        Ok(())
    }

    pub fn parse_msg(&mut self) {
        loop {
            use num::FromPrimitive;

            let first = self.socket.read_u8().unwrap();

            if first == std::u8::MAX {
                // self.parse_readcount++
                break;
            }

            let cmd: SvCmd = match FromPrimitive::from_u8(first) {
                Some(c) => c,
                None => break,
            };

            match cmd {
                SvCmd::ServerData => self.parse_serverdata(),
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
    /// qw version: u32
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
    pub fn parse_serverdata(&mut self) {
        let qw = self.socket.read_u32::<LittleEndian>().unwrap();

        if qw != qw::VERSION {
            // TODO: allow demo playback on versions 26-29
            // otherwise end the game
            panic!("Bad version handler unimplemented");
        }

        let servcount = self.socket.read_u32::<LittleEndian>().unwrap();

        let mut gamedir_bytes: Vec<u8> = Vec::new();
        loop {
            match self.socket.read_u8().unwrap() {
                0 => break,
                c => gamedir_bytes.push(c),
            }
        }
        let gamedir_str = String::from_utf8(gamedir_bytes).unwrap();

        // TODO: if current game dir differs, do host_writeconfig
    }

    pub fn proc_outofband(&mut self, msg: &[u8]) {
        let mut curs = Cursor::new(msg);

        match curs.read_u8().unwrap() as char {
            'c' => {
                // challenge
                debug!("Received challenge from server");
                let mut challenge_bytes = Vec::new();
                curs.read_to_end(&mut challenge_bytes).unwrap();

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

                self.challenge = challenge;
                self.send_connect();
            }

            'j' => {
                // connection
                if self.cxn_status >= CxnStatus::Connected {
                    return;
                }

                debug!("Server accepted connection request");
                self.cxn_status = CxnStatus::Connected;

                self.socket.write_u8(ClCmd::StringCmd as u8).unwrap();
                self.socket.write("new".as_bytes()).unwrap();
                self.socket.write_u8(0).unwrap();
                self.socket.transmit().unwrap();
            }

            'k' => {
                // ping
                debug!("Received ping");
                self.socket.out_of_band(&['l' as u8, 0]);
            }

            _ => panic!("Unrecognized out-of-band message"),
        }
    }

    pub fn get_cxn_status(&self) -> CxnStatus {
        self.cxn_status
    }
}
