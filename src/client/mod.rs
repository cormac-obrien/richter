// Copyright © 2020 Cormac O'Brien
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

mod cvars;
pub mod demo;
pub mod entity;
pub mod input;
pub mod menu;
pub mod render;
pub mod sound;
pub mod state;
pub mod trace;
pub mod view;

pub use self::cvars::register_cvars;

use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    io::BufReader,
    net::ToSocketAddrs,
    rc::Rc,
};

use crate::{
    client::{
        demo::{DemoServer, DemoServerError},
        entity::{ClientEntity, MAX_STATIC_ENTITIES},
        input::{game::GameInput, Input},
        sound::{MusicPlayer, StaticSound},
        state::{ClientState, PlayerInfo},
        trace::{TraceEntity, TraceFrame},
        view::{IdleVars, KickVars, MouseVars, RollVars},
    },
    common::{
        console::{CmdRegistry, Console, ConsoleError, CvarRegistry},
        engine,
        model::ModelError,
        net::{
            self,
            connect::{ConnectSocket, Request, Response, CONNECT_PROTOCOL_VERSION},
            BlockingMode, ClientCmd, ClientStat, ColorShift, EntityEffects, EntityState, GameType,
            NetError, PlayerColor, QSocket, ServerCmd, SignOnStage,
        },
        vfs::{Vfs, VfsError},
    },
};

use cgmath::Deg;
use chrono::Duration;
use input::InputFocus;
use menu::Menu;
use render::{ClientRenderer, GraphicsState, WorldRenderer};
use rodio::{OutputStream, OutputStreamHandle};
use sound::SoundError;
use thiserror::Error;

// connections are tried 3 times, see
// https://github.com/id-Software/Quake/blob/master/WinQuake/net_dgrm.c#L1248
const MAX_CONNECT_ATTEMPTS: usize = 3;
const MAX_STATS: usize = 32;

const DEFAULT_SOUND_PACKET_VOLUME: u8 = 255;
const DEFAULT_SOUND_PACKET_ATTENUATION: f32 = 1.0;

const CONSOLE_DIVIDER: &'static str = "\
\n\n\
\x1D\x1E\x1E\x1E\x1E\x1E\x1E\x1E\
\x1E\x1E\x1E\x1E\x1E\x1E\x1E\x1E\
\x1E\x1E\x1E\x1E\x1E\x1E\x1E\x1E\
\x1E\x1E\x1E\x1E\x1E\x1E\x1E\x1F\
\n\n";

#[derive(Error, Debug)]
pub enum ClientError {
    #[error("Connection rejected: {0}")]
    ConnectionRejected(String),
    #[error("Couldn't read cvar value: {0}")]
    Cvar(ConsoleError),
    #[error("Server sent an invalid port number ({0})")]
    InvalidConnectPort(i32),
    #[error("Server sent an invalid connect response")]
    InvalidConnectResponse,
    #[error("Invalid server address")]
    InvalidServerAddress,
    #[error("No response from server")]
    NoResponse,
    #[error("Unrecognized protocol: {0}")]
    UnrecognizedProtocol(i32),
    #[error("Client is not connected")]
    NotConnected,
    #[error("Client has already signed on")]
    AlreadySignedOn,
    #[error("No client with ID {0}")]
    NoSuchClient(usize),
    #[error("No player with ID {0}")]
    NoSuchPlayer(usize),
    #[error("No entity with ID {0}")]
    NoSuchEntity(usize),
    #[error("Null entity access")]
    NullEntity,
    #[error("Entity already exists: {0}")]
    EntityExists(usize),
    #[error("Invalid view entity: {0}")]
    InvalidViewEntity(usize),
    #[error("Too many static entities")]
    TooManyStaticEntities,
    #[error("No such lightmap animation: {0}")]
    NoSuchLightmapAnimation(usize),
    // TODO: wrap PlayError
    #[error("Failed to open audio output stream")]
    OutputStream,
    #[error("Demo server error: {0}")]
    DemoServer(#[from] DemoServerError),
    #[error("Model error: {0}")]
    Model(#[from] ModelError),
    #[error("Network error: {0}")]
    Network(#[from] NetError),
    #[error("Failed to load sound: {0}")]
    Sound(#[from] SoundError),
    #[error("Virtual filesystem error: {0}")]
    Vfs(#[from] VfsError),
}

pub struct MoveVars {
    cl_anglespeedkey: f32,
    cl_pitchspeed: f32,
    cl_yawspeed: f32,
    cl_sidespeed: f32,
    cl_upspeed: f32,
    cl_forwardspeed: f32,
    cl_backspeed: f32,
    cl_movespeedkey: f32,
}

#[derive(Debug, FromPrimitive)]
enum ColorShiftCode {
    Contents = 0,
    Damage = 1,
    Bonus = 2,
    Powerup = 3,
}

struct ServerInfo {
    _max_clients: u8,
    _game_type: GameType,
}

#[derive(Clone, Debug)]
pub enum IntermissionKind {
    Intermission,
    Finale { text: String },
    Cutscene { text: String },
}

/// Indicates to the client what should be done with the current connection.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum ConnectionStatus {
    /// Maintain the connection.
    Maintain,

    /// Disconnect from the server or demo server.
    Disconnect,

    /// Play the next demo in the demo queue.
    NextDemo,
}

/// Indicates the state of an active connection.
enum ConnectionState {
    /// The client is in the sign-on process.
    SignOn(SignOnStage),

    /// The client is fully connected.
    Connected(WorldRenderer),
}

/// Possible targets that a client can be connected to.
enum ConnectionKind {
    /// A regular Quake server.
    Server {
        /// The [`QSocket`](crate::net::QSocket) used to communicate with the server.
        qsock: QSocket,

        /// The client's packet composition buffer.
        compose: Vec<u8>,
    },

    /// A demo server.
    Demo(DemoServer),
}

/// A connection to a game server of some kind.
///
/// The exact nature of the connected server is specified by [`ConnectionKind`].
pub struct Connection {
    state: ClientState,
    conn_state: ConnectionState,
    kind: ConnectionKind,
}

impl Connection {
    fn handle_signon(
        &mut self,
        new_stage: SignOnStage,
        gfx_state: &GraphicsState,
    ) -> Result<(), ClientError> {
        use SignOnStage::*;

        let new_conn_state = match self.conn_state {
            // TODO: validate stage transition
            ConnectionState::SignOn(ref mut _stage) => {
                if let ConnectionKind::Server {
                    ref mut compose, ..
                } = self.kind
                {
                    match new_stage {
                        Not => (), // TODO this is an error (invalid value)
                        Prespawn => {
                            ClientCmd::StringCmd {
                                cmd: String::from("prespawn"),
                            }
                            .serialize(compose)?;
                        }
                        ClientInfo => {
                            // TODO: fill in client info here
                            ClientCmd::StringCmd {
                                cmd: format!("name \"{}\"\n", "UNNAMED"),
                            }
                            .serialize(compose)?;
                            ClientCmd::StringCmd {
                                cmd: format!("color {} {}", 0, 0),
                            }
                            .serialize(compose)?;
                            // TODO: need default spawn parameters?
                            ClientCmd::StringCmd {
                                cmd: format!("spawn {}", ""),
                            }
                            .serialize(compose)?;
                        }
                        SignOnStage::Begin => {
                            ClientCmd::StringCmd {
                                cmd: String::from("begin"),
                            }
                            .serialize(compose)?;
                        }
                        SignOnStage::Done => {
                            debug!("SignOn complete");
                            // TODO: end load screen
                            self.state.start_time = self.state.time;
                        }
                    }
                }

                match new_stage {
                    // TODO proper error
                    Not => panic!("SignOnStage::Not in handle_signon"),
                    // still signing on, advance to the new stage
                    Prespawn | ClientInfo | Begin => ConnectionState::SignOn(new_stage),

                    // finished signing on, build world renderer
                    Done => ConnectionState::Connected(WorldRenderer::new(
                        gfx_state,
                        self.state.models(),
                        1,
                    )),
                }
            }

            // ignore spurious sign-on messages
            ConnectionState::Connected { .. } => return Ok(()),
        };

        self.conn_state = new_conn_state;

        Ok(())
    }

    fn parse_server_msg(
        &mut self,
        vfs: &Vfs,
        gfx_state: &GraphicsState,
        cmds: &mut CmdRegistry,
        console: &mut Console,
        music_player: &mut MusicPlayer,
        kick_vars: KickVars,
    ) -> Result<ConnectionStatus, ClientError> {
        use ConnectionStatus::*;

        let (msg, demo_view_angles, track_override) = match self.kind {
            ConnectionKind::Server { ref mut qsock, .. } => {
                let msg = qsock.recv_msg(match self.conn_state {
                    // if we're in the game, don't block waiting for messages
                    ConnectionState::Connected(_) => BlockingMode::NonBlocking,

                    // otherwise, give the server some time to respond
                    // TODO: might make sense to make this a future or something
                    ConnectionState::SignOn(_) => BlockingMode::Timeout(Duration::seconds(5)),
                })?;

                (msg, None, None)
            }

            ConnectionKind::Demo(ref mut demo_srv) => {
                // only get the next update once we've made it all the way to
                // the previous one
                if self.state.time >= self.state.msg_times[0] {
                    let msg_view = match demo_srv.next() {
                        Some(v) => v,
                        None => {
                            // if there are no commands left in the demo, play
                            // the next demo if there is one
                            return Ok(NextDemo);
                        }
                    };

                    let mut view_angles = msg_view.view_angles();
                    // invert entity angles to get the camera direction right.
                    // yaw is already inverted.
                    view_angles.z = -view_angles.z;

                    // TODO: we shouldn't have to copy the message here
                    (
                        msg_view.message().to_owned(),
                        Some(view_angles),
                        demo_srv.track_override(),
                    )
                } else {
                    (Vec::new(), None, demo_srv.track_override())
                }
            }
        };

        // no data available at this time
        if msg.is_empty() {
            return Ok(Maintain);
        }

        let mut reader = BufReader::new(msg.as_slice());

        while let Some(cmd) = ServerCmd::deserialize(&mut reader)? {
            match cmd {
                // TODO: have an error for this instead of panicking
                // once all other commands have placeholder handlers, just error
                // in the wildcard branch
                ServerCmd::Bad => panic!("Invalid command from server"),

                ServerCmd::NoOp => (),

                ServerCmd::CdTrack { track, .. } => {
                    music_player.play_track(match track_override {
                        Some(t) => t as usize,
                        None => track as usize,
                    })?;
                }

                ServerCmd::CenterPrint { text } => {
                    // TODO: print to center of screen
                    warn!("Center print not yet implemented!");
                    println!("{}", text);
                }

                ServerCmd::PlayerData(player_data) => self.state.update_player(player_data),

                ServerCmd::Cutscene { text } => {
                    self.state.intermission = Some(IntermissionKind::Cutscene { text });
                    self.state.completion_time = Some(self.state.time);
                }

                ServerCmd::Damage {
                    armor,
                    blood,
                    source,
                } => self.state.handle_damage(armor, blood, source, kick_vars),

                ServerCmd::Disconnect => {
                    return Ok(match self.kind {
                        ConnectionKind::Demo(_) => NextDemo,
                        ConnectionKind::Server { .. } => Disconnect,
                    })
                }

                ServerCmd::FastUpdate(ent_update) => {
                    // first update signals the last sign-on stage
                    self.handle_signon(SignOnStage::Done, gfx_state)?;

                    let ent_id = ent_update.ent_id as usize;
                    self.state.update_entity(ent_id, ent_update)?;

                    // patch view angles in demos
                    if let Some(angles) = demo_view_angles {
                        if ent_id == self.state.view_entity_id() {
                            self.state.update_view_angles(angles);
                        }
                    }
                }

                ServerCmd::Finale { text } => {
                    self.state.intermission = Some(IntermissionKind::Finale { text });
                    self.state.completion_time = Some(self.state.time);
                }

                ServerCmd::FoundSecret => self.state.stats[ClientStat::FoundSecrets as usize] += 1,
                ServerCmd::Intermission => {
                    self.state.intermission = Some(IntermissionKind::Intermission);
                    self.state.completion_time = Some(self.state.time);
                }
                ServerCmd::KilledMonster => {
                    self.state.stats[ClientStat::KilledMonsters as usize] += 1
                }

                ServerCmd::LightStyle { id, value } => {
                    trace!("Inserting light style {} with value {}", id, &value);
                    let _ = self.state.light_styles.insert(id, value);
                }

                ServerCmd::Particle {
                    origin,
                    direction,
                    count,
                    color,
                } => {
                    match count {
                        // if count is 255, this is an explosion
                        255 => self
                            .state
                            .particles
                            .create_explosion(self.state.time, origin),

                        // otherwise it's an impact
                        _ => self.state.particles.create_projectile_impact(
                            self.state.time,
                            origin,
                            direction,
                            color,
                            count as usize,
                        ),
                    }
                }

                ServerCmd::Print { text } => {
                    console.println_timestamp(&text);
                    println!("{}", text);
                }

                ServerCmd::ServerInfo {
                    protocol_version,
                    max_clients,
                    game_type,
                    message,
                    model_precache,
                    sound_precache,
                } => {
                    // check protocol version
                    if protocol_version != net::PROTOCOL_VERSION as i32 {
                        Err(ClientError::UnrecognizedProtocol(protocol_version))?;
                    }

                    console.println(CONSOLE_DIVIDER);
                    console.println(message);
                    console.println(CONSOLE_DIVIDER);

                    let _server_info = ServerInfo {
                        _max_clients: max_clients,
                        _game_type: game_type,
                    };

                    self.state = ClientState::from_server_info(
                        vfs,
                        self.state.mixer.stream(),
                        max_clients,
                        model_precache,
                        sound_precache,
                    )?;

                    let bonus_cshift =
                        self.state.color_shifts[ColorShiftCode::Bonus as usize].clone();
                    cmds.insert_or_replace(
                        "bf",
                        Box::new(move |_| {
                            bonus_cshift.replace(ColorShift {
                                dest_color: [215, 186, 69],
                                percent: 50,
                            });
                            String::new()
                        }),
                    );
                }

                ServerCmd::SetAngle { angles } => self.state.set_view_angles(angles),

                ServerCmd::SetView { ent_id } => {
                    if ent_id <= 0 {
                        Err(ClientError::InvalidViewEntity(ent_id as usize))?;
                    }

                    self.state.set_view_entity(ent_id as usize)?;
                }

                ServerCmd::SignOnStage { stage } => self.handle_signon(stage, gfx_state)?,

                ServerCmd::Sound {
                    volume,
                    attenuation,
                    entity_id,
                    channel,
                    sound_id,
                    position,
                } => {
                    trace!(
                        "starting sound with id {} on entity {} channel {}",
                        sound_id,
                        entity_id,
                        channel
                    );

                    if entity_id as usize >= self.state.entities.len() {
                        warn!(
                            "server tried to start sound on nonexistent entity {}",
                            entity_id
                        );
                        break;
                    }

                    let volume = volume.unwrap_or(DEFAULT_SOUND_PACKET_VOLUME);
                    let attenuation = attenuation.unwrap_or(DEFAULT_SOUND_PACKET_ATTENUATION);
                    // TODO: apply volume, attenuation, spatialization
                    self.state.mixer.start_sound(
                        self.state.sounds[sound_id as usize].clone(),
                        self.state.msg_times[0],
                        Some(entity_id as usize),
                        channel,
                        volume as f32 / 255.0,
                        attenuation,
                        position,
                        &self.state.listener,
                    );
                }

                ServerCmd::SpawnBaseline {
                    ent_id,
                    model_id,
                    frame_id,
                    colormap,
                    skin_id,
                    origin,
                    angles,
                } => {
                    self.state.spawn_entities(
                        ent_id as usize,
                        EntityState {
                            model_id: model_id as usize,
                            frame_id: frame_id as usize,
                            colormap,
                            skin_id: skin_id as usize,
                            origin,
                            angles,
                            effects: EntityEffects::empty(),
                        },
                    )?;
                }

                ServerCmd::SpawnStatic {
                    model_id,
                    frame_id,
                    colormap,
                    skin_id,
                    origin,
                    angles,
                } => {
                    if self.state.static_entities.len() >= MAX_STATIC_ENTITIES {
                        Err(ClientError::TooManyStaticEntities)?;
                    }
                    self.state
                        .static_entities
                        .push(ClientEntity::from_baseline(EntityState {
                            origin,
                            angles,
                            model_id: model_id as usize,
                            frame_id: frame_id as usize,
                            colormap,
                            skin_id: skin_id as usize,
                            effects: EntityEffects::empty(),
                        }));
                }

                ServerCmd::SpawnStaticSound {
                    origin,
                    sound_id,
                    volume,
                    attenuation,
                } => {
                    self.state.static_sounds.push(StaticSound::new(
                        &self.state.mixer.stream(),
                        origin,
                        self.state.sounds[sound_id as usize].clone(),
                        volume as f32 / 255.0,
                        attenuation as f32 / 64.0,
                        &self.state.listener,
                    ));
                }

                ServerCmd::TempEntity { temp_entity } => self.state.spawn_temp_entity(&temp_entity),

                ServerCmd::StuffText { text } => console.stuff_text(text),

                ServerCmd::Time { time } => {
                    self.state.msg_times[1] = self.state.msg_times[0];
                    self.state.msg_times[0] = engine::duration_from_f32(time);
                }

                ServerCmd::UpdateColors {
                    player_id,
                    new_colors,
                } => {
                    let player_id = player_id as usize;
                    self.state.check_player_id(player_id)?;

                    match self.state.player_info[player_id] {
                        Some(ref mut info) => {
                            trace!(
                                "Player {} (ID {}) colors: {:?} -> {:?}",
                                info.name,
                                player_id,
                                info.colors,
                                new_colors,
                            );
                            info.colors = new_colors;
                        }

                        None => {
                            error!(
                                "Attempted to set colors on nonexistent player with ID {}",
                                player_id
                            );
                        }
                    }
                }

                ServerCmd::UpdateFrags {
                    player_id,
                    new_frags,
                } => {
                    let player_id = player_id as usize;
                    self.state.check_player_id(player_id)?;

                    match self.state.player_info[player_id] {
                        Some(ref mut info) => {
                            trace!(
                                "Player {} (ID {}) frags: {} -> {}",
                                &info.name,
                                player_id,
                                info.frags,
                                new_frags
                            );
                            info.frags = new_frags as i32;
                        }
                        None => {
                            error!(
                                "Attempted to set frags on nonexistent player with ID {}",
                                player_id
                            );
                        }
                    }
                }

                ServerCmd::UpdateName {
                    player_id,
                    new_name,
                } => {
                    let player_id = player_id as usize;
                    self.state.check_player_id(player_id)?;

                    if let Some(ref mut info) = self.state.player_info[player_id] {
                        // if this player is already connected, it's a name change
                        debug!("Player {} has changed name to {}", &info.name, &new_name);
                        info.name = new_name.to_owned();
                    } else {
                        // if this player is not connected, it's a join
                        debug!("Player {} with ID {} has joined", &new_name, player_id);
                        self.state.player_info[player_id] = Some(PlayerInfo {
                            name: new_name.to_owned(),
                            colors: PlayerColor::new(0, 0),
                            frags: 0,
                        });
                    }
                }

                ServerCmd::UpdateStat { stat, value } => {
                    trace!(
                        "{:?}: {} -> {}",
                        stat,
                        self.state.stats[stat as usize],
                        value
                    );
                    self.state.stats[stat as usize] = value;
                }

                ServerCmd::Version { version } => {
                    if version != net::PROTOCOL_VERSION as i32 {
                        // TODO: handle with an error
                        error!(
                            "Incompatible server version: server's is {}, client's is {}",
                            version,
                            net::PROTOCOL_VERSION,
                        );
                        panic!("bad version number");
                    }
                }

                x => {
                    debug!("{:?}", x);
                    unimplemented!();
                }
            }
        }

        Ok(Maintain)
    }

    fn frame(
        &mut self,
        frame_time: Duration,
        vfs: &Vfs,
        gfx_state: &GraphicsState,
        cmds: &mut CmdRegistry,
        console: &mut Console,
        music_player: &mut MusicPlayer,
        idle_vars: IdleVars,
        kick_vars: KickVars,
        roll_vars: RollVars,
        cl_nolerp: f32,
        sv_gravity: f32,
    ) -> Result<ConnectionStatus, ClientError> {
        debug!("frame time: {}ms", frame_time.num_milliseconds());

        // do this _before_ parsing server messages so that we know when to
        // request the next message from the demo server.
        self.state.advance_time(frame_time);
        match self.parse_server_msg(vfs, gfx_state, cmds, console, music_player, kick_vars)? {
            ConnectionStatus::Maintain => (),
            // if Disconnect or NextDemo, delegate up the chain
            s => return Ok(s),
        };

        self.state.update_interp_ratio(cl_nolerp);

        // interpolate entity data and spawn particle effects, lights
        self.state.update_entities()?;

        // update temp entities (lightning, etc.)
        self.state.update_temp_entities()?;

        // remove expired lights
        self.state.lights.update(self.state.time);

        // apply particle physics and remove expired particles
        self.state
            .particles
            .update(self.state.time, frame_time, sv_gravity);

        if let ConnectionKind::Server {
            ref mut qsock,
            ref mut compose,
        } = self.kind
        {
            // respond to the server
            if qsock.can_send() && !compose.is_empty() {
                qsock.begin_send_msg(&compose)?;
                compose.clear();
            }
        }

        // these all require the player entity to have spawned
        if let ConnectionState::Connected(_) = self.conn_state {
            // update view
            self.state.calc_final_view(idle_vars, kick_vars, roll_vars);

            // update ear positions
            self.state.update_listener();

            // spatialize sounds for new ear positions
            self.state.update_sound_spatialization();

            // update camera color shifts for new position/effects
            self.state.update_color_shifts(frame_time)?;
        }

        Ok(ConnectionStatus::Maintain)
    }
}

pub struct Client {
    vfs: Rc<Vfs>,
    cvars: Rc<RefCell<CvarRegistry>>,
    cmds: Rc<RefCell<CmdRegistry>>,
    console: Rc<RefCell<Console>>,
    input: Rc<RefCell<Input>>,
    _output_stream: OutputStream,
    output_stream_handle: OutputStreamHandle,
    music_player: Rc<RefCell<MusicPlayer>>,
    conn: Rc<RefCell<Option<Connection>>>,
    renderer: ClientRenderer,
    demo_queue: Rc<RefCell<VecDeque<String>>>,
}

impl Client {
    pub fn new(
        vfs: Rc<Vfs>,
        cvars: Rc<RefCell<CvarRegistry>>,
        cmds: Rc<RefCell<CmdRegistry>>,
        console: Rc<RefCell<Console>>,
        input: Rc<RefCell<Input>>,
        gfx_state: &GraphicsState,
        menu: &Menu,
    ) -> Client {
        let conn = Rc::new(RefCell::new(None));

        let (stream, handle) = match OutputStream::try_default() {
            Ok(o) => o,
            // TODO: proceed without sound and allow configuration in menu
            Err(_) => Err(ClientError::OutputStream).unwrap(),
        };

        // set up overlay/ui toggles
        cmds.borrow_mut().insert_or_replace(
            "toggleconsole",
            cmd_toggleconsole(conn.clone(), input.clone()),
        );
        cmds.borrow_mut()
            .insert_or_replace("togglemenu", cmd_togglemenu(conn.clone(), input.clone()));

        // set up connection console commands
        cmds.borrow_mut().insert_or_replace(
            "connect",
            cmd_connect(conn.clone(), input.clone(), handle.clone()),
        );
        cmds.borrow_mut()
            .insert_or_replace("reconnect", cmd_reconnect(conn.clone(), input.clone()));
        cmds.borrow_mut()
            .insert_or_replace("disconnect", cmd_disconnect(conn.clone(), input.clone()));

        // set up demo playback
        cmds.borrow_mut().insert_or_replace(
            "playdemo",
            cmd_playdemo(conn.clone(), vfs.clone(), input.clone(), handle.clone()),
        );

        let demo_queue = Rc::new(RefCell::new(VecDeque::new()));
        cmds.borrow_mut().insert_or_replace(
            "startdemos",
            cmd_startdemos(
                conn.clone(),
                vfs.clone(),
                input.clone(),
                handle.clone(),
                demo_queue.clone(),
            ),
        );

        let music_player = Rc::new(RefCell::new(MusicPlayer::new(vfs.clone(), handle.clone())));
        cmds.borrow_mut()
            .insert_or_replace("music", cmd_music(music_player.clone()));
        cmds.borrow_mut()
            .insert_or_replace("music_stop", cmd_music_stop(music_player.clone()));
        cmds.borrow_mut()
            .insert_or_replace("music_pause", cmd_music_pause(music_player.clone()));
        cmds.borrow_mut()
            .insert_or_replace("music_resume", cmd_music_resume(music_player.clone()));

        Client {
            vfs,
            cvars,
            cmds,
            console,
            input,
            _output_stream: stream,
            output_stream_handle: handle,
            music_player,
            conn,
            renderer: ClientRenderer::new(gfx_state, menu),
            demo_queue,
        }
    }

    pub fn disconnect(&mut self) {
        self.conn.replace(None);
        self.input.borrow_mut().set_focus(InputFocus::Console);
    }

    pub fn frame(
        &mut self,
        frame_time: Duration,
        gfx_state: &GraphicsState,
    ) -> Result<(), ClientError> {
        let cl_nolerp = self.cvar_value("cl_nolerp")?;
        let sv_gravity = self.cvar_value("sv_gravity")?;
        let idle_vars = self.idle_vars()?;
        let kick_vars = self.kick_vars()?;
        let roll_vars = self.roll_vars()?;

        let status = match *self.conn.borrow_mut() {
            Some(ref mut conn) => conn.frame(
                frame_time,
                &self.vfs,
                gfx_state,
                &mut self.cmds.borrow_mut(),
                &mut self.console.borrow_mut(),
                &mut self.music_player.borrow_mut(),
                idle_vars,
                kick_vars,
                roll_vars,
                cl_nolerp,
                sv_gravity,
            )?,
            None => ConnectionStatus::Disconnect,
        };

        use ConnectionStatus::*;
        match status {
            Maintain => (),
            _ => {
                let conn = match status {
                    // if client is already disconnected, this is a no-op
                    Disconnect => None,

                    // get the next demo from the queue
                    NextDemo => match self.demo_queue.borrow_mut().pop_front() {
                        Some(demo) => {
                            let mut demo_file = match self.vfs.open(format!("{}.dem", demo)) {
                                Ok(f) => Some(f),
                                Err(e) => {
                                    // log the error, dump the demo queue and disconnect
                                    self.console.borrow_mut().println(format!("{}", e));
                                    self.demo_queue.borrow_mut().clear();
                                    None
                                }
                            };

                            demo_file.as_mut().and_then(|df| match DemoServer::new(df) {
                                Ok(d) => Some(Connection {
                                    kind: ConnectionKind::Demo(d),
                                    state: ClientState::new(self.output_stream_handle.clone()),
                                    conn_state: ConnectionState::SignOn(SignOnStage::Prespawn),
                                }),
                                Err(e) => {
                                    self.console.borrow_mut().println(format!("{}", e));
                                    self.demo_queue.borrow_mut().clear();
                                    None
                                }
                            })
                        }

                        // if there are no more demos in the queue, disconnect
                        None => None,
                    },

                    // covered in first match
                    Maintain => unreachable!(),
                };

                match conn {
                    Some(_) => self.input.borrow_mut().set_focus(InputFocus::Game),

                    // don't allow game focus when disconnected
                    None => self.input.borrow_mut().set_focus(InputFocus::Console),
                }

                self.conn.replace(conn);
            }
        }

        Ok(())
    }

    pub fn render(
        &mut self,
        gfx_state: &GraphicsState,
        encoder: &mut wgpu::CommandEncoder,
        width: u32,
        height: u32,
        menu: &Menu,
        focus: InputFocus,
    ) -> Result<(), ClientError> {
        let fov = Deg(self.cvar_value("fov")?);
        let cvars = self.cvars.borrow();
        let console = self.console.borrow();

        self.renderer.render(
            gfx_state,
            encoder,
            self.conn.borrow().as_ref(),
            width,
            height,
            fov,
            &cvars,
            &console,
            menu,
            focus,
        );

        Ok(())
    }

    pub fn cvar_value<S>(&self, name: S) -> Result<f32, ClientError>
    where
        S: AsRef<str>,
    {
        self.cvars
            .borrow()
            .get_value(name.as_ref())
            .map_err(ClientError::Cvar)
    }

    pub fn handle_input(
        &mut self,
        game_input: &mut GameInput,
        frame_time: Duration,
    ) -> Result<(), ClientError> {
        let move_vars = self.move_vars()?;
        let mouse_vars = self.mouse_vars()?;

        match *self.conn.borrow_mut() {
            Some(Connection {
                ref mut state,
                kind: ConnectionKind::Server { ref mut qsock, .. },
                ..
            }) => {
                let move_cmd = state.handle_input(game_input, frame_time, move_vars, mouse_vars);
                // TODO: arrayvec here
                let mut msg = Vec::new();
                move_cmd.serialize(&mut msg)?;
                qsock.send_msg_unreliable(&msg)?;

                // clear mouse and impulse
                game_input.refresh();
            }

            _ => (),
        }

        Ok(())
    }

    fn move_vars(&self) -> Result<MoveVars, ClientError> {
        Ok(MoveVars {
            cl_anglespeedkey: self.cvar_value("cl_anglespeedkey")?,
            cl_pitchspeed: self.cvar_value("cl_pitchspeed")?,
            cl_yawspeed: self.cvar_value("cl_yawspeed")?,
            cl_sidespeed: self.cvar_value("cl_sidespeed")?,
            cl_upspeed: self.cvar_value("cl_upspeed")?,
            cl_forwardspeed: self.cvar_value("cl_forwardspeed")?,
            cl_backspeed: self.cvar_value("cl_backspeed")?,
            cl_movespeedkey: self.cvar_value("cl_movespeedkey")?,
        })
    }

    fn idle_vars(&self) -> Result<IdleVars, ClientError> {
        Ok(IdleVars {
            v_idlescale: self.cvar_value("v_idlescale")?,
            v_ipitch_cycle: self.cvar_value("v_ipitch_cycle")?,
            v_ipitch_level: self.cvar_value("v_ipitch_level")?,
            v_iroll_cycle: self.cvar_value("v_iroll_cycle")?,
            v_iroll_level: self.cvar_value("v_iroll_level")?,
            v_iyaw_cycle: self.cvar_value("v_iyaw_cycle")?,
            v_iyaw_level: self.cvar_value("v_iyaw_level")?,
        })
    }

    fn kick_vars(&self) -> Result<KickVars, ClientError> {
        Ok(KickVars {
            v_kickpitch: self.cvar_value("v_kickpitch")?,
            v_kickroll: self.cvar_value("v_kickroll")?,
            v_kicktime: self.cvar_value("v_kicktime")?,
        })
    }

    fn mouse_vars(&self) -> Result<MouseVars, ClientError> {
        Ok(MouseVars {
            m_pitch: self.cvar_value("m_pitch")?,
            m_yaw: self.cvar_value("m_yaw")?,
            sensitivity: self.cvar_value("sensitivity")?,
        })
    }

    fn roll_vars(&self) -> Result<RollVars, ClientError> {
        Ok(RollVars {
            cl_rollangle: self.cvar_value("cl_rollangle")?,
            cl_rollspeed: self.cvar_value("cl_rollspeed")?,
        })
    }

    pub fn view_entity_id(&self) -> Option<usize> {
        match *self.conn.borrow() {
            Some(Connection { ref state, .. }) => Some(state.view_entity_id()),
            None => None,
        }
    }

    pub fn trace<'a, I>(&self, entity_ids: I) -> Result<TraceFrame, ClientError>
    where
        I: IntoIterator<Item = &'a usize>,
    {
        match *self.conn.borrow() {
            Some(Connection { ref state, .. }) => {
                let mut trace = TraceFrame {
                    msg_times_ms: [
                        state.msg_times[0].num_milliseconds(),
                        state.msg_times[1].num_milliseconds(),
                    ],
                    time_ms: state.time.num_milliseconds(),
                    lerp_factor: state.lerp_factor,
                    entities: HashMap::new(),
                };

                for id in entity_ids.into_iter() {
                    let ent = &state.entities[*id];

                    let msg_origins = [ent.msg_origins[0].into(), ent.msg_origins[1].into()];
                    let msg_angles_deg = [
                        [
                            ent.msg_angles[0][0].0,
                            ent.msg_angles[0][1].0,
                            ent.msg_angles[0][2].0,
                        ],
                        [
                            ent.msg_angles[1][0].0,
                            ent.msg_angles[1][1].0,
                            ent.msg_angles[1][2].0,
                        ],
                    ];

                    trace.entities.insert(
                        *id as u32,
                        TraceEntity {
                            msg_origins,
                            msg_angles_deg,
                            origin: ent.origin.into(),
                        },
                    );
                }

                Ok(trace)
            }

            None => Err(ClientError::NotConnected),
        }
    }
}

impl std::ops::Drop for Client {
    fn drop(&mut self) {
        // if this errors, it was already removed so we don't care
        let _ = self.cmds.borrow_mut().remove("reconnect");
    }
}

// implements the "toggleconsole" command
fn cmd_toggleconsole(
    conn: Rc<RefCell<Option<Connection>>>,
    input: Rc<RefCell<Input>>,
) -> Box<dyn Fn(&[&str]) -> String> {
    Box::new(move |_| {
        let focus = input.borrow().focus();
        match *conn.borrow() {
            Some(_) => match focus {
                InputFocus::Game => input.borrow_mut().set_focus(InputFocus::Console),
                InputFocus::Console => input.borrow_mut().set_focus(InputFocus::Game),
                InputFocus::Menu => input.borrow_mut().set_focus(InputFocus::Console),
            },
            None => match focus {
                InputFocus::Console => input.borrow_mut().set_focus(InputFocus::Menu),
                InputFocus::Game => unreachable!(),
                InputFocus::Menu => input.borrow_mut().set_focus(InputFocus::Console),
            },
        }
        String::new()
    })
}

// implements the "togglemenu" command
fn cmd_togglemenu(
    conn: Rc<RefCell<Option<Connection>>>,
    input: Rc<RefCell<Input>>,
) -> Box<dyn Fn(&[&str]) -> String> {
    Box::new(move |_| {
        let focus = input.borrow().focus();
        match *conn.borrow() {
            Some(_) => match focus {
                InputFocus::Game => input.borrow_mut().set_focus(InputFocus::Menu),
                InputFocus::Console => input.borrow_mut().set_focus(InputFocus::Menu),
                InputFocus::Menu => input.borrow_mut().set_focus(InputFocus::Game),
            },
            None => match focus {
                InputFocus::Console => input.borrow_mut().set_focus(InputFocus::Menu),
                InputFocus::Game => unreachable!(),
                InputFocus::Menu => input.borrow_mut().set_focus(InputFocus::Console),
            },
        }
        String::new()
    })
}

fn connect<A>(server_addrs: A, stream: OutputStreamHandle) -> Result<Connection, ClientError>
where
    A: ToSocketAddrs,
{
    let mut con_sock = ConnectSocket::bind("0.0.0.0:0")?;
    let server_addr = match server_addrs.to_socket_addrs() {
        Ok(ref mut a) => a.next().ok_or(ClientError::InvalidServerAddress),
        Err(_) => Err(ClientError::InvalidServerAddress),
    }?;

    let mut response = None;

    for attempt in 0..MAX_CONNECT_ATTEMPTS {
        println!(
            "Connecting...(attempt {} of {})",
            attempt + 1,
            MAX_CONNECT_ATTEMPTS
        );
        con_sock.send_request(
            Request::connect(net::GAME_NAME, CONNECT_PROTOCOL_VERSION),
            server_addr,
        )?;

        // TODO: get rid of magic constant (2.5 seconds wait time for response)
        match con_sock.recv_response(Some(Duration::milliseconds(2500))) {
            Err(err) => {
                match err {
                    // if the message is invalid, log it but don't quit
                    // TODO: this should probably disconnect
                    NetError::InvalidData(msg) => error!("{}", msg),

                    // other errors are fatal
                    e => return Err(e.into()),
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

    let port = match response.ok_or(ClientError::NoResponse)? {
        Response::Accept(accept) => {
            // validate port number
            if accept.port < 0 || accept.port >= std::u16::MAX as i32 {
                Err(ClientError::InvalidConnectPort(accept.port))?;
            }

            debug!("Connection accepted on port {}", accept.port);
            accept.port as u16
        }

        // our request was rejected.
        Response::Reject(reject) => Err(ClientError::ConnectionRejected(reject.message))?,

        // the server sent back a response that doesn't make sense here (i.e. something other
        // than an Accept or Reject).
        _ => Err(ClientError::InvalidConnectResponse)?,
    };

    let mut new_addr = server_addr;
    new_addr.set_port(port);

    // we're done with the connection socket, so turn it into a QSocket with the new address
    let qsock = con_sock.into_qsocket(new_addr);

    Ok(Connection {
        state: ClientState::new(stream),
        kind: ConnectionKind::Server {
            qsock,
            compose: Vec::new(),
        },
        conn_state: ConnectionState::SignOn(SignOnStage::Prespawn),
    })
}

// TODO: when an audio device goes down, every command with an
// OutputStreamHandle needs to be reconstructed so it doesn't pass out
// references to a dead output stream

// TODO: this will hang while connecting. ideally, input should be handled in a
// separate thread so the OS doesn't think the client has gone unresponsive.
fn cmd_connect(
    conn: Rc<RefCell<Option<Connection>>>,
    input: Rc<RefCell<Input>>,
    stream: OutputStreamHandle,
) -> Box<dyn Fn(&[&str]) -> String> {
    Box::new(move |args| {
        if args.len() < 1 {
            // TODO: print to console
            return "usage: connect <server_ip>:<server_port>".to_owned();
        }

        match connect(args[0], stream.clone()) {
            Ok(new_conn) => {
                conn.replace(Some(new_conn));
                input.borrow_mut().set_focus(InputFocus::Game);
                String::new()
            }
            Err(e) => format!("{}", e),
        }
    })
}

fn cmd_reconnect(
    conn: Rc<RefCell<Option<Connection>>>,
    input: Rc<RefCell<Input>>,
) -> Box<dyn Fn(&[&str]) -> String> {
    Box::new(move |_| {
        match *conn.borrow_mut() {
            Some(ref mut conn) => {
                // TODO: clear client state
                conn.conn_state = ConnectionState::SignOn(SignOnStage::Prespawn);
                input.borrow_mut().set_focus(InputFocus::Game);
                String::new()
            }
            // TODO: log message, e.g. "can't reconnect while disconnected"
            None => "not connected".to_string(),
        }
    })
}

fn cmd_disconnect(
    conn: Rc<RefCell<Option<Connection>>>,
    input: Rc<RefCell<Input>>,
) -> Box<dyn Fn(&[&str]) -> String> {
    Box::new(move |_| {
        let connected = conn.borrow().is_some();
        if connected {
            conn.replace(None);
            input.borrow_mut().set_focus(InputFocus::Console);
            String::new()
        } else {
            "not connected".to_string()
        }
    })
}

fn cmd_playdemo(
    conn: Rc<RefCell<Option<Connection>>>,
    vfs: Rc<Vfs>,
    input: Rc<RefCell<Input>>,
    stream: OutputStreamHandle,
) -> Box<dyn Fn(&[&str]) -> String> {
    Box::new(move |args| {
        if args.len() != 1 {
            return "usage: playdemo [DEMOFILE]".to_owned();
        }

        let mut demo_file = match vfs.open(format!("{}.dem", args[0])) {
            Ok(f) => f,
            Err(e) => return format!("{}", e),
        };

        let demo_server = match DemoServer::new(&mut demo_file) {
            Ok(d) => d,
            Err(e) => return format!("{}", e),
        };

        conn.replace(Some(Connection {
            state: ClientState::new(stream.clone()),
            kind: ConnectionKind::Demo(demo_server),
            conn_state: ConnectionState::SignOn(SignOnStage::Prespawn),
        }));

        input.borrow_mut().set_focus(InputFocus::Game);
        String::new()
    })
}

fn cmd_startdemos(
    conn: Rc<RefCell<Option<Connection>>>,
    vfs: Rc<Vfs>,
    input: Rc<RefCell<Input>>,
    stream: OutputStreamHandle,
    demo_queue: Rc<RefCell<VecDeque<String>>>,
) -> Box<dyn Fn(&[&str]) -> String> {
    Box::new(move |args| {
        if args.len() == 0 {
            return "usage: startdemos [DEMOS]".to_owned();
        }

        for arg in args {
            demo_queue.borrow_mut().push_back(arg.to_string());
        }

        let mut demo_file = match vfs.open(format!(
            "{}.dem",
            demo_queue.borrow_mut().pop_front().unwrap()
        )) {
            Ok(f) => f,
            Err(e) => return format!("{}", e),
        };

        let demo_server = match DemoServer::new(&mut demo_file) {
            Ok(d) => d,
            Err(e) => return format!("{}", e),
        };

        conn.replace(Some(Connection {
            state: ClientState::new(stream.clone()),
            kind: ConnectionKind::Demo(demo_server),
            conn_state: ConnectionState::SignOn(SignOnStage::Prespawn),
        }));

        input.borrow_mut().set_focus(InputFocus::Game);

        String::new()
    })
}

fn cmd_music(music_player: Rc<RefCell<MusicPlayer>>) -> Box<dyn Fn(&[&str]) -> String> {
    Box::new(move |args| {
        if args.len() != 1 {
            return "usage: music [TRACKNAME]".to_owned();
        }

        let res = music_player.borrow_mut().play_named(args[0]);
        match res {
            Ok(()) => String::new(),
            Err(e) => {
                music_player.borrow_mut().stop();
                format!("{}", e)
            }
        }
    })
}

fn cmd_music_stop(music_player: Rc<RefCell<MusicPlayer>>) -> Box<dyn Fn(&[&str]) -> String> {
    Box::new(move |_| {
        music_player.borrow_mut().stop();
        String::new()
    })
}

fn cmd_music_pause(music_player: Rc<RefCell<MusicPlayer>>) -> Box<dyn Fn(&[&str]) -> String> {
    Box::new(move |_| {
        music_player.borrow_mut().pause();
        String::new()
    })
}

fn cmd_music_resume(music_player: Rc<RefCell<MusicPlayer>>) -> Box<dyn Fn(&[&str]) -> String> {
    Box::new(move |_| {
        music_player.borrow_mut().resume();
        String::new()
    })
}
