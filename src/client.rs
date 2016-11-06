use std;
use std::cell::Cell;
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, ToSocketAddrs, UdpSocket};
use std::str::FromStr;
use std::str;
use time::{Duration, PreciseTime};
use protocol;
use net::{Message, UdpPacket, UserInfo};

const RETRY_INTERVAL: f64 = 5.0;

#[derive(Copy, Clone, PartialEq, PartialOrd)]
pub enum CxnStatus {
    Disconnected = 0,
    DemoStart = 1,
    Connected = 2,
    OnServer = 3,
    Active = 4,
}

pub struct Client {
    socket: UdpSocket,
    challenge: Cell<i32>,
    server: SocketAddr,
    qport: u16,
    cxn_status: Cell<CxnStatus>,

    /// The last time this client sent a connection request to the server.
    cxn_time: Cell<Option<PreciseTime>>,
    userinfo: UserInfo,
}

impl Message for Client {
    fn get_socket(&self) -> &UdpSocket {
        &self.socket
    }
}


impl Client {
    /// Create a new `Client` by initiating a connection to `server`. The client
    /// will request a challenge from the server and then begin normal operation.
    pub fn connect<A>(server: A) -> Client
        where A: ToSocketAddrs {

        let client = Client {
            socket: match UdpSocket::bind("127.0.0.1:27001") {
                Ok(s) => s,
                Err(why) => panic!("Failed to open UDP socket: {}", why),
            },

            challenge: Cell::new(0),
            userinfo: UserInfo::default(),
            server: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 27500)),
            qport: 0,
            cxn_status: Cell::new(CxnStatus::Disconnected),
            cxn_time: Cell::new(None)
        };

        match client.send_outofband("getchallenge\n".as_bytes(), server) {
            Ok(n) => debug!("Sent {} bytes", n),
            Err(why) => panic!("Connection failed: {}", why),
        }

        client
    }

    /// Sends a connection message to the server.
    ///
    /// Connection messages are out-band-messages of the form
    ///     connect <protocol> <qport> <challenge> <userinfo>
    pub fn send_connect(&self) {
        self.send_outofband(format!("connect {} {} {} \"{}\"",
                                    protocol::VERSION,
                                    27001,
                                    self.challenge.get(),
                                    self.userinfo.serialize()), self.server);
        self.cxn_time.set(Some(PreciseTime::now()));
    }


    pub fn retry_connect(&self) {
        match self.cxn_time.get() {
            // Has it been 5 seconds since last connection attempt?
            Some(x) if x.to(PreciseTime::now()) > Duration::seconds(5) => {
                debug!("Five seconds elapsed, retrying...");
                self.send_connect();
            },

            _ => (),
        }
    }

    pub fn read_packets(&self) {
        loop {
            match self.get_packet(false) {
                Ok(packet) => {
                    if packet[..4] == [0xff, 0xff, 0xff, 0xff] {
                        self.proc_outofband(&packet);
                    } else {
                        debug!("Unrecognized packet type");
                    }
                },

                // no data available, return
                Err(ref why) if why.kind() == std::io::ErrorKind::WouldBlock => {
                    return;
                },

                Err(why) => {
                    panic!("get_packet failed: {}", why);
                },
            }
        }
    }

    pub fn proc_outofband(&self, msg: &UdpPacket) {
        if msg[..4] != [0xff, 0xff, 0xff, 0xff] {
            panic!("Called proc_outofband on an in-band message");
        }

        match msg[4] as char {
            'c' => {
                // challenge
                debug!("Received challenge from server");
                self.challenge.set(
                    match i32::from_str(
                        match str::from_utf8(&msg[5..]) {
                            Ok(s) => s,
                            Err(why) => {
                                warn!("Bad challenge from server.");
                                ""
                            },
                        }) {
                    Ok(n) => n,
                    Err(why) => {
                        warn!("Bad challenge from server.");
                        0
                    },
                });
                self.send_connect();
            },

            'j' => {
                // connection
                if self.cxn_status.get() >= CxnStatus::Connected {
                    return;
                }

                debug!("Server accepted connection requst");
                let mut msg: Vec<u8> = Vec::new();
                msg.push(protocol::ClCmd::StringCmd as u8);
                msg.extend_from_slice("new".as_bytes());
                self.cxn_status.set(CxnStatus::Connected);
                self.send_packet(msg, self.server);
            },

            'k' => {
                // ping
                debug!("Received ping");
                self.send_outofband(['l' as u8, 0], msg.sender());
            },

            _ => panic!("Unrecognized out-of-band message"),
        }
    }

    pub fn get_cxn_status(&self) -> CxnStatus {
        self.cxn_status.get()
    }
}
