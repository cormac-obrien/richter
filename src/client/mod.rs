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

pub mod input;
pub mod sound;

use std::error::Error;
use std::fmt;
use std::io::BufReader;
use std::net::ToSocketAddrs;

use client::sound::AudioSource;
use client::sound::Channel;
use client::sound::StaticSound;
use common::bsp;
use common::model::Model;
use common::net;
use common::net::BlockingMode;
use common::net::ClientCmd;
use common::net::ClientCmdStringCmd;
use common::net::ColorShift;
use common::net::GameType;
use common::net::IntermissionKind;
use common::net::ItemFlags;
use common::net::NetError;
use common::net::PlayerColor;
use common::net::QSocket;
use common::net::ServerCmd;
use common::net::ServerCmdPrint;
use common::net::ServerCmdServerInfo;
use common::net::SignOnStage;
use common::net::connect::CONNECT_PROTOCOL_VERSION;
use common::net::connect::ConnectSocket;
use common::net::connect::Request;
use common::net::connect::Response;
use common::pak::Pak;

use cgmath::Deg;
use cgmath::Vector3;
use cgmath::Zero;
use chrono::Duration;
use rodio;
use rodio::Endpoint;

// connections are tried 3 times, see
// https://github.com/id-Software/Quake/blob/master/WinQuake/net_dgrm.c#L1248
const MAX_CONNECT_ATTEMPTS: usize = 3;

const MAX_STATS: usize = 32;

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

struct ServerInfo {
    max_clients: u8,
    game_type: GameType,
}

struct ClientView {
    lerp_view_angles: [Vector3<Deg<f32>>; 2],
    view_angles: Vector3<Deg<f32>>,
    punch_angle: Vector3<Deg<f32>>,
    view_height: f32,
}

struct ScoreboardEntry {
    name: String,
    join_time: Duration,
    frags: i32,
    colors: PlayerColor,
    // translations: [u8; VID_GRADES],
}

struct ClientEntity {
    force_link: bool,

    // baseline: EntityState,
    last_update: Duration,

    msg_origins: [Vector3<f32>; 2],
    origin: Vector3<f32>,

    msg_angles: [Vector3<Deg<f32>>; 2],
    angles: Vector3<Deg<f32>>,

    model: Option<Model>,
    frame: usize,

    // TODO: make Duration?
    sync_base: f32,

    effects: i32,
    skin_id: usize,
    vis_frame: usize,
}

// client information regarding the current level
struct ClientState {
    // model precache
    models: Vec<Model>,

    // audio source precache
    sounds: Vec<AudioSource>,

    // ambient sounds (infinite looping, static position)
    static_sounds: Vec<StaticSound>,

    // move_msg_count: usize,
    // cmd: MoveCmd,
    // stats: [i32; MAX_STATS],
    // items: ItemFlags,
    // item_get_time: [f32; 32],
    // face_anim_time: f32,
    // color_shifts: [ColorShift; 4],
    // prev_color_shifts: [ColorShift; 4],

    // view: ClientView,

    // m_velocity: [Vector3<f32>; 2],
    // velocity: Vector3<f32>,

    // ideal_pitch: Deg<f32>,
    // pitch_velocity: f32,
    // no_drift: bool,
    // drift_move: f32,
    // last_stop: f64,

    // paused: bool,
    // on_ground: bool,
    // in_water: bool,

    // intermission: IntermissionKind,
    // completed_time: Duration,

    // m_time: [Duration; 2],
    // time: Duration,
    // old_time: Duration,

    // last_received_message: f32,

    // level_name: String,
    // view_ent: usize,

    // server_info: ServerInfo,

    // worldmodel: Model,
}

impl ClientState {
    pub fn new(pak: &Pak) -> ClientState {
        ClientState {
            models: vec![Model::none()],
            sounds: vec![AudioSource::load(pak, "misc/null.wav").unwrap()],
            static_sounds: Vec::new(),
        }
    }
}

pub struct Client {
    qsock: QSocket,
    compose: Vec<u8>,

    audio_endpoint: Endpoint,
    state: ClientState,
}

impl Client {
    pub fn connect<A>(server_addrs: A, pak: &Pak) -> Result<Client, ClientError>
    where
        A: ToSocketAddrs,
    {
        let mut con_sock = ConnectSocket::bind("0.0.0.0:0")?;
        let server_addr = server_addrs.to_socket_addrs().unwrap().next().unwrap();

        let mut response = None;

        for attempt in 0..MAX_CONNECT_ATTEMPTS {
            println!(
                "Connecting...(attempt {} of {})",
                attempt + 1,
                MAX_CONNECT_ATTEMPTS
            );
            con_sock.send_request(
                Request::connect(
                    net::GAME_NAME,
                    CONNECT_PROTOCOL_VERSION,
                ),
                server_addr,
            )?;

            // TODO: get rid of magic constant (2.5 seconds wait time for response)
            match con_sock.recv_response(Some(Duration::milliseconds(2500))) {
                Err(err) => {
                    match err {
                        // if the message is invalid, log it but don't quit
                        NetError::InvalidData(msg) => error!("{}", msg),

                        // other errors are fatal
                        _ => return Err(ClientError::from(err)),
                    }
                }

                Ok(opt) => {
                    if let Some((resp, remote)) = opt {
                        // if this response came from the right server, we're done
                        if remote == server_addr {
                            response = Some(resp);
                            break;
                        }
                    }
                }
            }
        }

        // make sure we actually got a response
        if response.is_none() {
            // TODO: specific error for this. Shouldn't be fatal.
            return Err(ClientError::with_msg("No response"));
        }

        // we can unwrap this because we just checked it
        let port = match response.unwrap() {
            // if the server accepted our connect request, make sure the port number makes sense
            Response::Accept(accept) => {
                if accept.port < 0 || accept.port > ::std::u16::MAX as i32 {
                    return Err(ClientError::with_msg(format!("Invalid port number")));
                }

                println!("Connection accepted on port {}", accept.port);
                accept.port as u16
            }

            // our request was rejected. TODO: this error shouldn't be fatal.
            Response::Reject(reject) => {
                return Err(ClientError::with_msg(
                    format!("Connection rejected: {}", reject.message),
                ))
            }

            // the server sent back a response that doesn't make sense here (i.e. something other
            // than an Accept or Reject).
            // TODO: more specific error. this shouldn't be fatal.
            _ => return Err(ClientError::with_msg("Invalid connect response")),
        };

        let mut new_addr = server_addr;
        new_addr.set_port(port);

        // we're done with the connection socket, so turn it into a QSocket with the new address
        let mut qsock = con_sock.into_qsocket(new_addr);

        Ok(Client {
            qsock,
            compose: Vec::new(),
            // TODO: inherit endpoint from host
            audio_endpoint: rodio::default_endpoint().unwrap(),
            state: ClientState::new(pak),
        })
    }

    pub fn add_cmd(&mut self, cmd: ClientCmd) -> Result<(), ClientError> {
        cmd.serialize(&mut self.compose)?;

        Ok(())
    }

    pub fn send(&mut self) -> Result<(), ClientError> {
        // TODO: check can_send on qsock
        self.qsock.begin_send_msg(&self.compose)?;
        self.compose.clear();

        Ok(())
    }

    pub fn parse_server_msg(&mut self, block: BlockingMode, pak: &Pak) -> Result<(), ClientError> {
        let msg = self.qsock.recv_msg(block)?;

        // no data available at this time
        if msg.is_empty() {
            return Ok(());
        }

        let mut reader = BufReader::new(msg.as_slice());

        while let Some(cmd) = ServerCmd::deserialize(&mut reader)? {
            match cmd {
                ServerCmd::Bad => panic!("Invalid command from server"),
                ServerCmd::NoOp => (),

                ServerCmd::CdTrack(cdtrack_cmd) => {
                    // TODO: play CD track
                    debug!("CD tracks not yet implemented");
                }
                ServerCmd::Print(print_cmd) => {
                    // TODO: print to in-game console
                    println!("{}", print_cmd.text);
                }
                ServerCmd::ServerInfo(server_info) => self.update_server_info(server_info, pak)?,
                ServerCmd::SetView(setview) => {
                    // TODO: sanity check on this value
                    // self.state.view_ent = setview.view_ent as usize;
                }

                ServerCmd::SignOnStage(signon) => self.handle_signon(signon.stage)?,

                ServerCmd::SpawnBaseline(baseline) => {
                    // TODO
                }

                ServerCmd::SpawnStaticSound(static_sound) => {
                    self.state.static_sounds.push(StaticSound::new(
                        &self.audio_endpoint,
                        static_sound.origin,
                        self.state.sounds[static_sound.sound_id as usize]
                            .clone(),
                        static_sound.volume,
                        static_sound.attenuation,
                    ));
                }

                x => {
                    debug!("{:?}", x);
                    unimplemented!();
                }
            }
        }

        Ok(())
    }

    fn handle_signon(&mut self, stage: SignOnStage) -> Result<(), ClientError> {
        match stage {
            SignOnStage::Not => (), // TODO this is an error (invalid value)
            SignOnStage::Prespawn => {
                self.add_cmd(ClientCmd::StringCmd(
                    ClientCmdStringCmd { cmd: String::from("prespawn") },
                ));
            }
            SignOnStage::ClientInfo => {
                // TODO: fill in client info here
                self.add_cmd(ClientCmd::StringCmd(ClientCmdStringCmd {
                    cmd: format!("name \"{}\"\n", "UNNAMED"),
                }));
                self.add_cmd(ClientCmd::StringCmd(
                    ClientCmdStringCmd { cmd: format!("color {} {}", 0, 0) },
                ));
                // TODO: need default spawn parameters?
                self.add_cmd(ClientCmd::StringCmd(
                    ClientCmdStringCmd { cmd: format!("spawn {}", "") },
                ));
            }
            SignOnStage::Begin => {
                self.add_cmd(ClientCmd::StringCmd(
                    ClientCmdStringCmd { cmd: String::from("begin") },
                ));
            }
            SignOnStage::Done => {
                // TODO: end load screen and start render loop
            }
        }

        Ok(())
    }

    fn update_server_info(
        &mut self,
        server_info_cmd: ServerCmdServerInfo,
        pak: &Pak,
    ) -> Result<(), ClientError> {
        let mut new_client_state = ClientState::new(pak);

        if server_info_cmd.protocol_version != net::PROTOCOL_VERSION as i32 {
            return Err(ClientError::with_msg(format!(
                "Incompatible protocol version (got {}, should be {})",
                server_info_cmd.protocol_version,
                net::PROTOCOL_VERSION
            )));
        }

        // TODO: print sign-on message to in-game console
        println!("{}", server_info_cmd.message);

        // TODO: validate submodel names
        for mod_name in server_info_cmd.model_precache {
            if mod_name.ends_with(".bsp") {
                let bsp_data = match pak.open(&mod_name) {
                    Some(b) => b,
                    None => {
                        return Err(ClientError::with_msg(
                            format!("Model not found in pak archive: {}", mod_name),
                        ))
                    }
                };

                let (mut brush_models, _) = bsp::load(bsp_data).unwrap();
                new_client_state.models.append(&mut brush_models);
            } else if !mod_name.starts_with("*") {
                debug!("Loading model {}", mod_name);
                new_client_state.models.push(Model::load(pak, mod_name));
            }

            // TODO: send keepalive message?
        }

        for ref snd_name in server_info_cmd.sound_precache {
            debug!("Loading sound {}", snd_name);

            // TODO: waiting on ruuda/hound#20 (some WAV files don't load under rodio)
            new_client_state.sounds.push(match AudioSource::load(
                pak,
                snd_name,
            ) {
                Ok(a) => a,
                Err(e) => {
                    warn!("Loading {} failed: {}", snd_name, e);
                    AudioSource::load(pak, "misc/null.wav").unwrap()
                }
            });

            // TODO: send keepalive message?
        }

        let server_info = ServerInfo {
            max_clients: server_info_cmd.max_clients,
            game_type: server_info_cmd.game_type,
        };

        // TODO: set up rest of client state (R_NewMap)

        self.state = new_client_state;
        Ok(())
    }
}
