use std;
use std::collections::HashMap;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use time::PreciseTime;
use protocol;

pub struct UdpPacket {
    src: SocketAddr,
    msg: Box<[u8]>,
}

impl UdpPacket {
    pub fn new(src: SocketAddr, msg: Box<[u8]>) -> UdpPacket {
        UdpPacket {
            src: src,
            msg: msg,
        }
    }

    pub fn sender(&self) -> SocketAddr {
        self.src
    }
}

impl std::ops::Index<usize> for UdpPacket {
    type Output = u8;
    fn index(&self, i: usize) -> &u8 {
        self.msg.index(i)
    }
}

impl std::ops::Index<std::ops::Range<usize>> for UdpPacket {
    type Output = [u8];
    fn index (&self, r: std::ops::Range<usize>) -> &[u8] {
        self.msg.index(r)
    }
}

impl std::ops::Index<std::ops::RangeTo<usize>> for UdpPacket {
    type Output = [u8];
    fn index (&self, r: std::ops::RangeTo<usize>) -> &[u8] {
        self.msg.index(r)
    }
}

impl std::ops::Index<std::ops::RangeFrom<usize>> for UdpPacket {
    type Output = [u8];
    fn index (&self, r: std::ops::RangeFrom<usize>) -> &[u8] {
        self.msg.index(r)
    }
}

// TODO: find a better home for UserInfo
pub struct UserInfo(HashMap<String, String>);

impl std::default::Default for UserInfo {
    fn default() -> UserInfo {
        let mut userinfo: HashMap<String, String> = HashMap::new();
        userinfo.insert(String::from("name"), String::from("unnamed"));
        userinfo.insert(String::from("topcolor"), String::from("0"));
        userinfo.insert(String::from("bottomcolor"), String::from("0"));
        userinfo.insert(String::from("rate"), String::from("2500"));
        userinfo.insert(String::from("msg"), String::from("0"));
        userinfo.insert(String::from("*ver"), String::from("RICHTER"));
        UserInfo(userinfo)
    }
}

impl UserInfo {
    pub fn serialize(&self) -> String {
        let mut result = String::new();
        for (key, val) in self.0.iter() {
            result.push_str(key);
            result.push_str("\\");
            result.push_str(val);
            result.push_str("\\");
        }
        result
    }
}

pub trait Message {
    /// Returns the UDP socket to be used for all transmissions.
    fn get_socket(&self) -> &UdpSocket;

    /// Sends a UDP packet containing `data` to `to`.
    fn send_packet<D, A>(&self, data: D, to: A) -> std::io::Result<usize>
        where D: AsRef<[u8]>,
              A: ToSocketAddrs {
        self.get_socket().send_to(data.as_ref(), to)
    }

    /// Attempts to retrieve a UDP packet.
    ///
    /// If `block` is true, the function will block until data becomes available
    /// or the socket timeout is reached; it blocks indefinitely if no timeout is
    /// set. If `block` is false, and no data is available, it returns
    /// immediately with an Error value.
    fn get_packet(&self, block: bool) -> std::io::Result<UdpPacket> {
        let mut response: Vec<u8> = Vec::new();
        response.resize(protocol::PACKET_MAX, 0);

        self.get_socket().set_nonblocking(!block);
        let (len, addr) = match self.get_socket().recv_from(&mut response) {
            Ok(x) => x,
            Err(why) => return Err(why),
        };
        self.get_socket().set_nonblocking(false);

        response.truncate(len);
        Ok(UdpPacket::new(addr, response.into_boxed_slice()))
    }

    /// Sends an out-of-band message containing `data` to `to`.
    ///
    /// Out-of-band messages are identified with a leading `0xFFFFFFFF`. The
    /// function blocks until all data is sent.
    fn send_outofband<D, A>(&self, data: D, to: A) -> std::io::Result<usize>
        where D: AsRef<[u8]>,
              A: ToSocketAddrs {
        let mut msg: Vec<u8> = Vec::with_capacity(4 + data.as_ref().len());
        msg.extend_from_slice(b"\xff\xff\xff\xff");
        msg.extend_from_slice(data.as_ref());
        self.send_packet(msg, to)
    }
}

