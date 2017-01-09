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

use std::cell::{Cell, RefMut, RefCell};
use std::io::{self, Cursor, Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use byteorder::{ByteOrder, LittleEndian, ReadBytesExt, WriteBytesExt};
use protocol;

const MAX_MESSAGE: usize = 1450;
const HEADER_SIZE: usize = 8;

pub enum Message<'a> {
    InBand(RefMut<'a, NetworkBuffer>),
    OutOfBand(RefMut<'a, NetworkBuffer>),
}

pub struct NetworkBuffer {
    buf: Cursor<Vec<u8>>,
}

/// A general-purpose buffer for networking purposes.
impl NetworkBuffer {
    fn new() -> NetworkBuffer {
        NetworkBuffer { buf: Cursor::new(Vec::with_capacity(HEADER_SIZE + MAX_MESSAGE)) }
    }

    fn is_empty(&self) -> bool {
        self.buf.get_ref().len() == 0
    }

    fn rewind(&mut self) {
        self.buf.set_position(0);
    }

    fn clear(&mut self) {
        self.buf.get_mut().clear();
        self.buf.set_position(0);
    }

    fn get_ref(&self) -> &[u8] {
        self.buf.get_ref()
    }

    fn get_mut(&mut self) -> &mut [u8] {
        self.buf.get_mut()
    }
}

impl Read for NetworkBuffer {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.buf.read(buf)
    }
}

impl Write for NetworkBuffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buf.write(buf)
    }

    fn flush(&mut self) -> Result<(), io::Error> {
        self.buf.flush()
    }
}

pub struct NetworkChannel {
    sock: UdpSocket,
    serv: SocketAddrV4,
    qport: u16,

    // TODO: try to find the clearest possible naming scheme for the sequence number variables
    //
    // sequence number of the most recently received message
    incoming_sequence: Cell<u32>,
    incoming_sequence_is_reliable: Cell<bool>,

    // sequence number most recently acknowledged by other side
    incoming_ack: Cell<u32>,
    incoming_ack_is_reliable: Cell<bool>,

    outgoing_sequence: Cell<u32>,
    outgoing_sequence_is_reliable: Cell<bool>,

    // reliable sequence most recently sent from this side
    previous_reliable_sequence: Cell<u32>,

    // stores the most recently received message data. This is also returned to the programmer
    // for message parsing.
    recv_buf: RefCell<NetworkBuffer>,

    // internal buffer for socket send calls
    send_buf: RefCell<NetworkBuffer>,

    // stores messages while they are being constructed
    message_buf: RefCell<NetworkBuffer>,

    // stores the most recently sent reliable message that hasn't been acknowledged yet
    reliable_buf: RefCell<NetworkBuffer>,

    // number of successfully received packets
    good_count: Cell<u64>,

    // number of drops. multiple consecutive dropped packets are counted as a single drop!
    drop_count: Cell<u64>,
}

impl NetworkChannel {
    pub fn new(serv: SocketAddrV4, local_port: u16) -> NetworkChannel {
        let sock = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), local_port))
                       .unwrap();

        debug!("Bound UDP socket at {:?}", sock.local_addr().unwrap());

        NetworkChannel {
            sock: sock,
            serv: serv,
            qport: 0,

            incoming_sequence: Cell::new(0),
            incoming_sequence_is_reliable: Cell::new(false),

            incoming_ack: Cell::new(0),
            incoming_ack_is_reliable: Cell::new(false),

            outgoing_sequence: Cell::new(0),
            outgoing_sequence_is_reliable: Cell::new(false),

            previous_reliable_sequence: Cell::new(0),

            recv_buf: RefCell::new(NetworkBuffer::new()),
            send_buf: RefCell::new(NetworkBuffer::new()),
            message_buf: RefCell::new(NetworkBuffer::new()),
            reliable_buf: RefCell::new(NetworkBuffer::new()),

            good_count: Cell::new(0),
            drop_count: Cell::new(0),
        }
    }

    pub fn out_of_band(&self, data: &[u8]) {
        let mut send_buf = self.send_buf.borrow_mut();
        send_buf.clear();
        send_buf.write_i32::<LittleEndian>(-1);
        send_buf.write(data);
        self.sock.send_to(send_buf.get_ref(), &self.serv);
    }

    pub fn transmit(&self, data: &[u8]) {
        // we can send a reliable message if...
        let mut should_send_reliable = self.incoming_ack > self.previous_reliable_sequence &&
                                       self.incoming_ack_is_reliable !=
                                       self.outgoing_sequence_is_reliable;

        // if we're not waiting on a reliable message to be acknowledged and there's a message
        // waiting, copy it to the reliable buffer
        if self.reliable_buf.borrow().is_empty() && !self.message_buf.borrow().is_empty() {
            self.reliable_buf.borrow_mut().write(self.message_buf.borrow().get_ref());
            self.message_buf.borrow_mut().clear();
            self.outgoing_sequence_is_reliable.set(!self.outgoing_sequence_is_reliable.get());
        }

        {
            // borrow send_buf while we're composing the packet
            let mut send_buf = self.send_buf.borrow_mut();
            send_buf.clear();

            let mut w1 = self.outgoing_sequence.get();
            if should_send_reliable {
                w1 |= 1 << 31;
            }

            let mut w2 = self.incoming_sequence.get();
            if self.incoming_sequence_is_reliable.get() {
                w2 |= 1 << 31;
            }

            send_buf.write_u32::<LittleEndian>(w1);
            send_buf.write_u32::<LittleEndian>(w2);

            // TODO:
            // if client {
            send_buf.write_u16::<LittleEndian>(self.qport);
            // }

            if should_send_reliable {
                send_buf.write(self.reliable_buf.borrow().get_ref());
                self.previous_reliable_sequence.set(self.outgoing_sequence.get());
            }

            // TODO: write in the unreliable part if there's space left.
            // we'll need NetworkBuffer to have a constant maximum size

        } // finished writing to send_buf

        // TODO: do bandwidth calculations

        self.sock.send(self.send_buf.borrow().get_ref()).unwrap();

        // TODO: time updates and stuff
    }

    pub fn process(&self) -> Option<Message> {
        let mut recv_array: [u8; HEADER_SIZE + MAX_MESSAGE] = [0; HEADER_SIZE + MAX_MESSAGE];
        let len = match self.sock.recv(&mut recv_array) {
            Ok(n) => {
                debug!("received packet of {} bytes", n);
                n
            }
            Err(why) => panic!("{}", why),
        };

        let mut recv_buf = self.recv_buf.borrow_mut();
        recv_buf.clear();
        recv_buf.write(&recv_array[..len]);
        recv_buf.rewind();

        let mut msg_sequence = recv_buf.read_u32::<LittleEndian>().unwrap();
        if msg_sequence == ::std::u32::MAX {
            return Some(Message::OutOfBand(recv_buf));
        }
        let mut ack_sequence = recv_buf.read_u32::<LittleEndian>().unwrap();

        // TODO:
        // if server {
        //     let qport = recv_buf.read_u16::<LittleEndian>().unwrap();
        // }

        let msg_is_reliable = msg_sequence >> 31 == 1;
        let ack_is_reliable = ack_sequence >> 31 == 1;
        msg_sequence &= !(1 << 31);
        ack_sequence &= !(1 << 31);

        if msg_sequence <= self.incoming_sequence.get() {
            // TODO: handle out-of-order packets properly
            panic!("out of order packets are not yet handled properly");
        }

        let dropped_since_last = msg_sequence - (self.incoming_sequence.get() + 1);
        if dropped_since_last > 0 {
            // TODO: log dropped packets
        }

        // if our last reliable message went through, make way for a new one
        if ack_is_reliable {
            self.reliable_buf.borrow_mut().clear();
        }

        self.incoming_sequence.set(msg_sequence);
        self.incoming_ack.set(ack_sequence);
        self.incoming_ack_is_reliable.set(true);

        if msg_is_reliable {
            self.incoming_sequence_is_reliable.set(!self.incoming_sequence_is_reliable.get());
        }

        Some(Message::InBand(self.recv_buf.borrow_mut()))
    }
}
