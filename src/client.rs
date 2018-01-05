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

use std::error::Error;
use std::fmt;
use std::net::ToSocketAddrs;

use net::GAME_NAME;
use net::NetError;
use net::connect::CONNECT_PROTOCOL_VERSION;
use net::connect::ConnectSocket;
use net::connect::Request;
use net::connect::Response;

use chrono::Duration;

// connections are tried 3 times, see
// https://github.com/id-Software/Quake/blob/master/WinQuake/net_dgrm.c#L1248
const MAX_CONNECT_ATTEMPTS: usize = 3;

#[derive(Debug)]
pub enum ClientError {
    Io(::std::io::Error),
    Net(NetError),
    Other(String),
}

impl ClientError {
    pub fn with_msg<S>(msg: S) -> Self
    where
        S: AsRef<str>,
    {
        ClientError::Other(msg.as_ref().to_owned())
    }
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            ClientError::Io(ref err) => {
                write!(f, "I/O error: ")?;
                err.fmt(f)
            }
            ClientError::Net(ref err) => {
                write!(f, "Network error: ")?;
                err.fmt(f)
            }
            ClientError::Other(ref msg) => write!(f, "{}", msg),
        }
    }
}

impl Error for ClientError {
    fn description(&self) -> &str {
        match *self {
            ClientError::Io(ref err) => err.description(),
            ClientError::Net(ref err) => err.description(),
            ClientError::Other(ref msg) => &msg,
        }
    }
}

impl From<::std::io::Error> for ClientError {
    fn from(error: ::std::io::Error) -> Self {
        ClientError::Io(error)
    }
}

impl From<NetError> for ClientError {
    fn from(error: NetError) -> Self {
        ClientError::Net(error)
    }
}

pub struct Client {}

impl Client {
    pub fn connect<A>(server_addrs: A) -> Result<(), ClientError>
    where
        A: ToSocketAddrs,
    {
        let mut con_sock = ConnectSocket::bind("0.0.0.0:0")?;
        let server_addr = server_addrs.to_socket_addrs().unwrap().next().unwrap();

        let mut response = None;

        for attempt in 0..MAX_CONNECT_ATTEMPTS {
            println!("Connecting...(attempt {} of {})", attempt + 1, MAX_CONNECT_ATTEMPTS);
            con_sock.send_request(
                Request::connect(GAME_NAME, CONNECT_PROTOCOL_VERSION),
                server_addr,
            )?;

            // TODO: get rid of magic constant (2.5 seconds wait time for response)
            match con_sock.recv_response(Some(Duration::milliseconds(2500))) {
                Err(err) => match err {
                    // if the message is invalid, log it but don't quit
                    NetError::InvalidData(msg) => error!("{}", msg),

                    // other errors are fatal
                    _ => return Err(ClientError::from(err)),
                }

                Ok(opt) => if let Some((resp, remote)) = opt {
                    // if this response came from the right server, we're done
                    if remote == server_addr {
                        response = Some(resp);
                        break;
                    }
                }
            }
        }

        if response.is_none() {
            return Err(ClientError::with_msg("No response"));
        }

        match response.unwrap() {
            Response::Accept(accept) => {
                println!("Connection accepted on port {}", accept.port);
            },
            Response::Reject(reject) => {
                println!("Connection rejected: {}", reject.message);
            },

            // TODO: more specific error
            _ => return Err(ClientError::with_msg("Invalid connect response")),
        }

        Ok(())
    }
}
