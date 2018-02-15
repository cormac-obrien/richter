// Copyright © 2018 Cormac O'Brien
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

use std::io::BufReader;
use std::io::Cursor;
use std::io::ErrorKind;
use std::mem::size_of;
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use std::net::UdpSocket;

use common::net::MAX_MESSAGE;
use common::net::NetError;
use common::net::QSocket;
use common::util;

use byteorder::LittleEndian;
use byteorder::NetworkEndian;
use byteorder::ReadBytesExt;
use byteorder::WriteBytesExt;
use chrono::Duration;
use num::FromPrimitive;

pub const CONNECT_PROTOCOL_VERSION: u8 = 3;
const CONNECT_CONTROL: i32 = 1 << 31;
const CONNECT_LENGTH_MASK: i32 = 0x0000FFFF;

pub trait ConnectPacket {
    /// Returns the numeric value of this packet's code.
    fn code(&self) -> u8;

    /// Returns the length in bytes of this packet's content.
    fn content_len(&self) -> usize;

    /// Writes this packet's content to the given sink.
    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt;

    /// Returns the total length in bytes of this packet, including the header.
    fn packet_len(&self) -> i32 {
        let mut len = 0;

        // control header
        len += size_of::<i32>();

        // request/reply code
        len += size_of::<u8>();

        len += self.content_len();

        len as i32
    }

    /// Generates the control header for this packet.
    fn control_header(&self) -> i32 {
        CONNECT_CONTROL | (self.packet_len() & CONNECT_LENGTH_MASK)
    }

    /// Generates the byte representation of this packet for transmission.
    fn to_bytes(&self) -> Result<Vec<u8>, NetError> {
        let mut writer = Cursor::new(Vec::new());
        writer.write_i32::<NetworkEndian>(self.control_header())?;
        writer.write_u8(self.code())?;
        self.write_content(&mut writer)?;
        let packet = writer.into_inner();
        Ok(packet)
    }
}

#[derive(Debug, FromPrimitive)]
pub enum RequestCode {
    Connect = 1,
    ServerInfo = 2,
    PlayerInfo = 3,
    RuleInfo = 4,
}

#[derive(Debug)]
pub struct RequestConnect {
    pub game_name: String,
    pub proto_ver: u8,
}

impl ConnectPacket for RequestConnect {
    fn code(&self) -> u8 {
        RequestCode::Connect as u8
    }

    fn content_len(&self) -> usize {
        let mut len = 0;

        // game name and terminating zero byte
        len += self.game_name.len() + size_of::<u8>();

        // protocol version
        len += size_of::<u8>();

        len
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        writer.write(self.game_name.as_bytes())?;
        writer.write_u8(0)?;
        writer.write_u8(self.proto_ver)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct RequestServerInfo {
    pub game_name: String,
}

impl ConnectPacket for RequestServerInfo {
    fn code(&self) -> u8 {
        RequestCode::ServerInfo as u8
    }

    fn content_len(&self) -> usize {
        // game name and terminating zero byte
        self.game_name.len() + size_of::<u8>()
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        writer.write(self.game_name.as_bytes())?;
        writer.write_u8(0)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct RequestPlayerInfo {
    pub player_id: u8,
}

impl ConnectPacket for RequestPlayerInfo {
    fn code(&self) -> u8 {
        RequestCode::PlayerInfo as u8
    }

    fn content_len(&self) -> usize {
        // player id
        size_of::<u8>()
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        writer.write_u8(self.player_id)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct RequestRuleInfo {
    pub prev_cvar: String,
}

impl ConnectPacket for RequestRuleInfo {
    fn code(&self) -> u8 {
        RequestCode::RuleInfo as u8
    }

    fn content_len(&self) -> usize {
        // previous cvar in rule chain and terminating zero byte
        self.prev_cvar.len() + size_of::<u8>()
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        writer.write(self.prev_cvar.as_bytes())?;
        writer.write_u8(0)?;
        Ok(())
    }
}

/// A request from a client to retrieve information from or connect to the server.
#[derive(Debug)]
pub enum Request {
    Connect(RequestConnect),
    ServerInfo(RequestServerInfo),
    PlayerInfo(RequestPlayerInfo),
    RuleInfo(RequestRuleInfo),
}

impl Request {
    pub fn connect<S>(game_name: S, proto_ver: u8) -> Request
    where
        S: AsRef<str>,
    {
        Request::Connect(RequestConnect {
            game_name: game_name.as_ref().to_owned(),
            proto_ver,
        })
    }

    pub fn server_info<S>(game_name: S) -> Request
    where
        S: AsRef<str>,
    {
        Request::ServerInfo(RequestServerInfo {
            game_name: game_name.as_ref().to_owned(),
        })
    }

    pub fn player_info(player_id: u8) -> Request {
        Request::PlayerInfo(RequestPlayerInfo { player_id })
    }

    pub fn rule_info<S>(prev_cvar: S) -> Request
    where
        S: AsRef<str>,
    {
        Request::RuleInfo(RequestRuleInfo {
            prev_cvar: prev_cvar.as_ref().to_string(),
        })
    }
}

impl ConnectPacket for Request {
    fn code(&self) -> u8 {
        use self::Request::*;
        match *self {
            Connect(ref c) => c.code(),
            ServerInfo(ref s) => s.code(),
            PlayerInfo(ref p) => p.code(),
            RuleInfo(ref r) => r.code(),
        }
    }

    fn content_len(&self) -> usize {
        use self::Request::*;
        match *self {
            Connect(ref c) => c.content_len(),
            ServerInfo(ref s) => s.content_len(),
            PlayerInfo(ref p) => p.content_len(),
            RuleInfo(ref r) => r.content_len(),
        }
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        use self::Request::*;
        match *self {
            Connect(ref c) => c.write_content(writer),
            ServerInfo(ref s) => s.write_content(writer),
            PlayerInfo(ref p) => p.write_content(writer),
            RuleInfo(ref r) => r.write_content(writer),
        }
    }
}

#[derive(Debug, FromPrimitive)]
pub enum ResponseCode {
    Accept = 0x81,
    Reject = 0x82,
    ServerInfo = 0x83,
    PlayerInfo = 0x84,
    RuleInfo = 0x85,
}

#[derive(Debug)]
pub struct ResponseAccept {
    pub port: i32,
}

impl ConnectPacket for ResponseAccept {
    fn code(&self) -> u8 {
        ResponseCode::Accept as u8
    }

    fn content_len(&self) -> usize {
        // port number
        size_of::<i32>()
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        writer.write_i32::<LittleEndian>(self.port)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct ResponseReject {
    pub message: String,
}

impl ConnectPacket for ResponseReject {
    fn code(&self) -> u8 {
        ResponseCode::Reject as u8
    }

    fn content_len(&self) -> usize {
        // message plus terminating zero byte
        self.message.len() + size_of::<u8>()
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        writer.write(self.message.as_bytes())?;
        writer.write_u8(0)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct ResponseServerInfo {
    pub address: String,
    pub hostname: String,
    pub levelname: String,
    pub client_count: u8,
    pub client_max: u8,
    pub protocol_version: u8,
}

impl ConnectPacket for ResponseServerInfo {
    fn code(&self) -> u8 {
        ResponseCode::ServerInfo as u8
    }

    fn content_len(&self) -> usize {
        let mut len = 0;

        // address string and terminating zero byte
        len += self.address.len() + size_of::<u8>();

        // hostname string and terminating zero byte
        len += self.hostname.len() + size_of::<u8>();

        // levelname string and terminating zero byte
        len += self.levelname.len() + size_of::<u8>();

        // current client count
        len += size_of::<u8>();

        // maximum client count
        len += size_of::<u8>();

        // protocol version
        len += size_of::<u8>();

        len
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        writer.write(self.address.as_bytes())?;
        writer.write_u8(0)?;
        writer.write(self.hostname.as_bytes())?;
        writer.write_u8(0)?;
        writer.write(self.levelname.as_bytes())?;
        writer.write_u8(0)?;
        writer.write_u8(self.client_count)?;
        writer.write_u8(self.client_max)?;
        writer.write_u8(self.protocol_version)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct ResponsePlayerInfo {
    pub player_id: u8,
    pub player_name: String,
    pub colors: i32,
    pub frags: i32,
    pub connect_duration: i32,
    pub address: String,
}

impl ConnectPacket for ResponsePlayerInfo {
    fn code(&self) -> u8 {
        ResponseCode::PlayerInfo as u8
    }

    fn content_len(&self) -> usize {
        let mut len = 0;

        // player id
        len += size_of::<u8>();

        // player name and terminating zero byte
        len += self.player_name.len() + size_of::<u8>();

        // colors
        len += size_of::<i32>();

        // frags
        len += size_of::<i32>();

        // connection duration
        len += size_of::<i32>();

        // address and terminating zero byte
        len += self.address.len() + size_of::<u8>();

        len
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        writer.write_u8(self.player_id)?;
        writer.write(self.player_name.as_bytes())?;
        writer.write_u8(0)?;
        writer.write_i32::<LittleEndian>(self.colors)?;
        writer.write_i32::<LittleEndian>(self.frags)?;
        writer.write_i32::<LittleEndian>(self.connect_duration)?;
        writer.write(self.address.as_bytes())?;
        writer.write_u8(0)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct ResponseRuleInfo {
    pub cvar_name: String,
    pub cvar_val: String,
}

impl ConnectPacket for ResponseRuleInfo {
    fn code(&self) -> u8 {
        ResponseCode::RuleInfo as u8
    }

    fn content_len(&self) -> usize {
        let mut len = 0;

        // cvar name and terminating zero byte
        len += self.cvar_name.len() + size_of::<u8>();

        // cvar val and terminating zero byte
        len += self.cvar_val.len() + size_of::<u8>();

        len
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        writer.write(self.cvar_name.as_bytes())?;
        writer.write_u8(0)?;
        writer.write(self.cvar_val.as_bytes())?;
        writer.write_u8(0)?;
        Ok(())
    }
}

#[derive(Debug)]
pub enum Response {
    Accept(ResponseAccept),
    Reject(ResponseReject),
    ServerInfo(ResponseServerInfo),
    PlayerInfo(ResponsePlayerInfo),
    RuleInfo(ResponseRuleInfo),
}

impl ConnectPacket for Response {
    fn code(&self) -> u8 {
        use self::Response::*;
        match *self {
            Accept(ref a) => a.code(),
            Reject(ref r) => r.code(),
            ServerInfo(ref s) => s.code(),
            PlayerInfo(ref p) => p.code(),
            RuleInfo(ref r) => r.code(),
        }
    }

    fn content_len(&self) -> usize {
        use self::Response::*;
        match *self {
            Accept(ref a) => a.content_len(),
            Reject(ref r) => r.content_len(),
            ServerInfo(ref s) => s.content_len(),
            PlayerInfo(ref p) => p.content_len(),
            RuleInfo(ref r) => r.content_len(),
        }
    }

    fn write_content<W>(&self, writer: &mut W) -> Result<(), NetError>
    where
        W: WriteBytesExt,
    {
        use self::Response::*;
        match *self {
            Accept(ref a) => a.write_content(writer),
            Reject(ref r) => r.write_content(writer),
            ServerInfo(ref s) => s.write_content(writer),
            PlayerInfo(ref p) => p.write_content(writer),
            RuleInfo(ref r) => r.write_content(writer),
        }
    }
}

/// A socket that listens for new connections or queries.
pub struct ConnectListener {
    socket: UdpSocket,
}

impl ConnectListener {
    /// Creates a `ConnectListener` from the given address.
    pub fn bind<A>(addr: A) -> Result<ConnectListener, NetError>
    where
        A: ToSocketAddrs,
    {
        let socket = UdpSocket::bind(addr)?;

        Ok(ConnectListener { socket })
    }

    /// Receives a request and returns it along with its remote address.
    pub fn recv_request(&self) -> Result<(Request, SocketAddr), NetError> {
        // Original engine receives connection requests in `net_message`,
        // allocated at https://github.com/id-Software/Quake/blob/master/WinQuake/net_main.c#L851
        let mut recv_buf = [0u8; MAX_MESSAGE];
        let (len, remote) = self.socket.recv_from(&mut recv_buf)?;
        let mut reader = BufReader::new(&recv_buf[..len]);

        let control = reader.read_i32::<NetworkEndian>()?;

        // TODO: figure out what a control value of -1 means
        if control == -1 {
            return Err(NetError::with_msg("Control value is -1"));
        }

        // high 4 bits must be 0x8000 (CONNECT_CONTROL)
        if control & !CONNECT_LENGTH_MASK != CONNECT_CONTROL {
            return Err(NetError::InvalidData(format!(
                "control value {:X}",
                control & !CONNECT_LENGTH_MASK
            )));
        }

        // low 4 bits must be total length of packet
        let control_len = (control & CONNECT_LENGTH_MASK) as usize;
        if control_len != len {
            return Err(NetError::InvalidData(format!(
                "Actual packet length ({}) differs from header value ({})", len,
                control_len,
            )));
        }

        // validate request code
        let request_byte = reader.read_u8()?;
        let request_code = match RequestCode::from_u8(request_byte) {
            Some(r) => r,
            None => {
                return Err(NetError::InvalidData(
                    format!("request code {}", request_byte),
                ))
            }
        };

        let request = match request_code {
            RequestCode::Connect => {
                let game_name = util::read_cstring(&mut reader).unwrap();
                let proto_ver = reader.read_u8()?;
                Request::Connect(RequestConnect {
                    game_name,
                    proto_ver,
                })
            }

            RequestCode::ServerInfo => {
                let game_name = util::read_cstring(&mut reader).unwrap();
                Request::ServerInfo(RequestServerInfo { game_name })
            }

            RequestCode::PlayerInfo => {
                let player_id = reader.read_u8()?;
                Request::PlayerInfo(RequestPlayerInfo { player_id })
            }

            RequestCode::RuleInfo => {
                let prev_cvar = util::read_cstring(&mut reader).unwrap();
                Request::RuleInfo(RequestRuleInfo { prev_cvar })
            }
        };

        Ok((request, remote))
    }

    pub fn send_response(&self, response: Response, remote: SocketAddr) -> Result<(), NetError> {
        self.socket.send_to(&response.to_bytes()?, remote)?;
        Ok(())
    }
}

pub struct ConnectSocket {
    socket: UdpSocket,
}

impl ConnectSocket {
    pub fn bind<A>(local: A) -> Result<ConnectSocket, NetError>
    where
        A: ToSocketAddrs,
    {
        let socket = UdpSocket::bind(local)?;

        Ok(ConnectSocket { socket })
    }

    pub fn into_qsocket(self, remote: SocketAddr) -> QSocket {
        QSocket::new(self.socket, remote)
    }

    /// Send a `Request` to the server at the specified address.
    pub fn send_request(&mut self, request: Request, remote: SocketAddr) -> Result<(), NetError> {
        self.socket.send_to(&request.to_bytes()?, remote)?;
        Ok(())
    }

    /// Receive a `Response` from the server.
    ///
    /// If `timeout` is not `None`, the operation times out after the specified duration and the
    /// function returns `None`.
    pub fn recv_response(
        &mut self,
        timeout: Option<Duration>,
    ) -> Result<Option<(Response, SocketAddr)>, NetError> {
        let mut recv_buf = [0u8; MAX_MESSAGE];

        // if a timeout was specified, apply it for this recv
        self.socket.set_read_timeout(
            timeout.map(|d| d.to_std().unwrap()),
        )?;
        let (len, remote) = match self.socket.recv_from(&mut recv_buf) {
            Err(e) => {
                match e.kind() {
                    ErrorKind::WouldBlock | ErrorKind::TimedOut => return Ok(None),
                    _ => return Err(NetError::from(e)),
                }
            }
            Ok(ret) => ret,
        };
        self.socket.set_read_timeout(None)?;

        let mut reader = BufReader::new(&recv_buf[..len]);

        let control = reader.read_i32::<NetworkEndian>()?;

        // TODO: figure out what a control value of -1 means
        if control == -1 {
            return Err(NetError::with_msg("Control value is -1"));
        }

        // high 4 bits must be 0x8000 (CONNECT_CONTROL)
        if control & !CONNECT_LENGTH_MASK != CONNECT_CONTROL {
            return Err(NetError::InvalidData(format!(
                "control value {:X}",
                control & !CONNECT_LENGTH_MASK
            )));
        }

        // low 4 bits must be total length of packet
        let control_len = (control & CONNECT_LENGTH_MASK) as usize;
        if control_len != len {
            return Err(NetError::with_msg(format!(
                "Actual packet length ({}) differs from header value ({})", len,
                control_len,
            )));
        }

        let response_byte = reader.read_u8()?;
        let response_code = match ResponseCode::from_u8(response_byte) {
            Some(r) => r,
            None => {
                return Err(NetError::InvalidData(
                    format!("response code {}", response_byte),
                ))
            }
        };

        let response = match response_code {
            ResponseCode::Accept => {
                let port = reader.read_i32::<LittleEndian>()?;
                Response::Accept(ResponseAccept { port })
            }

            ResponseCode::Reject => {
                let message = util::read_cstring(&mut reader).unwrap();
                Response::Reject(ResponseReject { message })
            }

            ResponseCode::ServerInfo => {
                let address = util::read_cstring(&mut reader).unwrap();
                let hostname = util::read_cstring(&mut reader).unwrap();
                let levelname = util::read_cstring(&mut reader).unwrap();
                let client_count = reader.read_u8()?;
                let client_max = reader.read_u8()?;
                let protocol_version = reader.read_u8()?;

                Response::ServerInfo(ResponseServerInfo {
                    address,
                    hostname,
                    levelname,
                    client_count,
                    client_max,
                    protocol_version,
                })
            }

            ResponseCode::PlayerInfo => unimplemented!(),
            ResponseCode::RuleInfo => unimplemented!(),
        };

        Ok(Some((response, remote)))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    // test_request_*_packet_len
    //
    // These tests ensure that ConnectPacket::packet_len() returns an accurate value by comparing it
    // with the number of bytes returned by ConnectPacket::to_bytes().
    #[test]
    fn test_request_connect_packet_len() {
        let request_connect = RequestConnect {
            game_name: String::from("QUAKE"),
            proto_ver: CONNECT_PROTOCOL_VERSION,
        };

        let packet_len = request_connect.packet_len() as usize;
        let packet = request_connect.to_bytes().unwrap();
        assert_eq!(packet_len, packet.len());
    }

    #[test]
    fn test_request_server_info_packet_len() {
        let request_server_info = RequestServerInfo { game_name: String::from("QUAKE") };
        let packet_len = request_server_info.packet_len() as usize;
        let packet = request_server_info.to_bytes().unwrap();
        assert_eq!(packet_len, packet.len());
    }

    #[test]
    fn test_request_player_info_packet_len() {
        let request_player_info = RequestPlayerInfo { player_id: 0 };
        let packet_len = request_player_info.packet_len() as usize;
        let packet = request_player_info.to_bytes().unwrap();
        assert_eq!(packet_len, packet.len());
    }

    #[test]
    fn test_request_rule_info_packet_len() {
        let request_rule_info = RequestRuleInfo { prev_cvar: String::from("sv_gravity") };
        let packet_len = request_rule_info.packet_len() as usize;
        let packet = request_rule_info.to_bytes().unwrap();
        assert_eq!(packet_len, packet.len());
    }

    #[test]
    fn test_response_accept_packet_len() {
        let response_accept = ResponseAccept { port: 26000 };
        let packet_len = response_accept.packet_len() as usize;
        let packet = response_accept.to_bytes().unwrap();
        assert_eq!(packet_len, packet.len());
    }

    #[test]
    fn test_response_reject_packet_len() {
        let response_reject = ResponseReject { message: String::from("error") };
        let packet_len = response_reject.packet_len() as usize;
        let packet = response_reject.to_bytes().unwrap();
        assert_eq!(packet_len, packet.len());
    }

    #[test]
    fn test_response_server_info_packet_len() {
        let response_server_info = ResponseServerInfo {
            address: String::from("127.0.0.1"),
            hostname: String::from("localhost"),
            levelname: String::from("e1m1"),
            client_count: 1,
            client_max: 16,
            protocol_version: 15,
        };
        let packet_len = response_server_info.packet_len() as usize;
        let packet = response_server_info.to_bytes().unwrap();
        assert_eq!(packet_len, packet.len());
    }

    #[test]
    fn test_response_player_info_packet_len() {
        let response_player_info = ResponsePlayerInfo {
            player_id: 0,
            player_name: String::from("player"),
            colors: 0,
            frags: 0,
            connect_duration: 120,
            address: String::from("127.0.0.1"),
        };
        let packet_len = response_player_info.packet_len() as usize;
        let packet = response_player_info.to_bytes().unwrap();
        assert_eq!(packet_len, packet.len());
    }

    #[test]
    fn test_connect_listener_bind() {
        let _listener = ConnectListener::bind("127.0.0.1:26000").unwrap();
    }
}
