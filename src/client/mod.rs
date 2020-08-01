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
mod demo;
pub mod entity;
pub mod input;
pub mod menu;
pub mod render;
pub mod sound;
pub mod state;
pub mod trace;
pub mod view;

pub use self::cvars::register_cvars;

use std::{cell::RefCell, collections::HashMap, io::BufReader, net::ToSocketAddrs, rc::Rc};

use crate::{
    client::{
        demo::{DemoServer, DemoServerError},
        entity::{particle::Particle, ClientEntity, Light, MAX_STATIC_ENTITIES},
        input::{game::GameInput, Input},
        sound::{AudioSource, Channel, Listener, StaticSound},
        state::{ClientState, PlayerInfo},
        trace::{TraceEntity, TraceFrame},
        view::{IdleVars, KickVars, MouseVars, RollVars},
    },
    common::{
        console::{CmdRegistry, Console, ConsoleError, CvarRegistry},
        engine,
        math::Angles,
        model::ModelError,
        net::{
            self,
            connect::{ConnectSocket, Request, Response, CONNECT_PROTOCOL_VERSION},
            BlockingMode, ClientCmd, ClientStat, ColorShift, EntityEffects, EntityState, GameType,
            ItemFlags, NetError, PlayerColor, QSocket, ServerCmd, SignOnStage,
        },
        vfs::{Vfs, VfsError},
    },
};

use cgmath::{Deg, Vector3};
use chrono::Duration;
use input::InputFocus;
use menu::Menu;
use render::{ClientRenderer, GraphicsState, WorldRenderer};
use sound::SoundError;
use thiserror::Error;

// connections are tried 3 times, see
// https://github.com/id-Software/Quake/blob/master/WinQuake/net_dgrm.c#L1248
const MAX_CONNECT_ATTEMPTS: usize = 3;
const MAX_STATS: usize = 32;

const DEFAULT_SOUND_PACKET_VOLUME: u8 = 255;
const DEFAULT_SOUND_PACKET_ATTENUATION: f32 = 1.0;

const MAX_CHANNELS: usize = 128;

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

struct ClientChannel {
    start_time: Duration,
    ent_id: usize,
    ent_channel: i8,
    channel: Channel,
}

pub struct Mixer {
    audio_device: Rc<rodio::Device>,
    // TODO: replace with an array once const type parameters are implemented
    channels: Box<[Option<ClientChannel>]>,
}

impl Mixer {
    pub fn new(audio_device: Rc<rodio::Device>) -> Mixer {
        let mut channel_vec = Vec::new();

        for _ in 0..MAX_CHANNELS {
            channel_vec.push(None);
        }

        Mixer {
            audio_device,
            channels: channel_vec.into_boxed_slice(),
        }
    }

    fn find_free_channel(&self, ent_id: usize, ent_channel: i8) -> usize {
        let mut oldest = 0;

        for (i, channel) in self.channels.iter().enumerate() {
            match *channel {
                Some(ref chan) => {
                    // if this channel is free, return it right away
                    if !chan.channel.in_use() {
                        return i;
                    }

                    // replace sounds on the same entity channel
                    if ent_channel != 0
                        && chan.ent_id == ent_id
                        && (chan.ent_channel == ent_channel || ent_channel == -1)
                    {
                        return i;
                    }

                    // TODO: don't clobber player sounds with monster sounds

                    // keep track of which sound started the earliest
                    match self.channels[oldest] {
                        Some(ref o) => {
                            if chan.start_time < o.start_time {
                                oldest = i;
                            }
                        }
                        None => oldest = i,
                    }
                }

                None => return i,
            }
        }

        // if there are no good channels, just replace the one that's been running the longest
        oldest
    }

    pub fn start_sound(
        &mut self,
        src: AudioSource,
        time: Duration,
        ent_id: usize,
        ent_channel: i8,
        volume: f32,
        attenuation: f32,
        ents: &[ClientEntity],
        listener: &Listener,
    ) {
        let chan_id = self.find_free_channel(ent_id, ent_channel);
        let new_channel = Channel::new(self.audio_device.clone());

        new_channel.play(
            src.clone(),
            ents[ent_id].origin,
            listener,
            volume,
            attenuation,
        );
        self.channels[chan_id] = Some(ClientChannel {
            start_time: time,
            ent_id,
            ent_channel,
            channel: new_channel,
        })
    }
}

enum ConnectionStatus {
    Maintain,
    Disconnect,
}

enum ConnectionState {
    SignOn(SignOnStage),
    Connected(WorldRenderer),
}

enum ConnectionKind {
    Server { qsock: QSocket, compose: Vec<u8> },
    Demo(DemoServer),
}

pub struct Connection {
    state: ClientState,
    conn_state: Rc<RefCell<ConnectionState>>,
    kind: ConnectionKind,
}

impl Connection {
    fn handle_signon(
        &mut self,
        new_stage: SignOnStage,
        gfx_state: &GraphicsState,
    ) -> Result<(), ClientError> {
        use SignOnStage::*;

        let new_conn_state = match *self.conn_state.borrow_mut() {
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

        self.conn_state.replace(new_conn_state);

        Ok(())
    }

    fn parse_server_msg(
        &mut self,
        vfs: &Vfs,
        gfx_state: &GraphicsState,
        cmds: &mut CmdRegistry,
        console: &mut Console,
        audio_device: &rodio::Device,
        kick_vars: KickVars,
    ) -> Result<ConnectionStatus, ClientError> {
        use ConnectionStatus::*;

        let (msg, demo_view_angles) = match self.kind {
            ConnectionKind::Server { ref mut qsock, .. } => {
                let msg = qsock.recv_msg(match *self.conn_state.borrow() {
                    // if we're in the game, don't block waiting for messages
                    ConnectionState::Connected(_) => BlockingMode::NonBlocking,

                    // otherwise, give the server some time to respond
                    // TODO: might make sense to make this a future or something
                    ConnectionState::SignOn(_) => BlockingMode::Timeout(Duration::seconds(5)),
                })?;

                (msg, None)
            }

            ConnectionKind::Demo(ref mut demo_srv) => {
                // only get the next update once we've made it all the way to
                // the previous one
                if self.state.time >= self.state.msg_times[0] {
                    let msg_view = match demo_srv.next() {
                        Some(v) => v,
                        None => {
                            return Ok(Disconnect);
                        }
                    };

                    let mut view_angles = msg_view.view_angles();
                    // invert entity angles to get the camera direction right.
                    // yaw is already inverted.
                    view_angles.x = -view_angles.x;
                    view_angles.z = -view_angles.z;

                    // TODO: we shouldn't have to copy the message here
                    (msg_view.message().to_owned(), Some(view_angles))
                } else {
                    (Vec::new(), None)
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

                ServerCmd::CdTrack { .. } => {
                    // TODO: play CD track
                    warn!("CD tracks not yet implemented");
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

                ServerCmd::Disconnect => return Ok(Disconnect),

                ServerCmd::FastUpdate(ent_update) => {
                    // first update signals the last sign-on stage
                    self.handle_signon(SignOnStage::Done, gfx_state)?;

                    let ent_id = ent_update.ent_id as usize;
                    self.state.update_entity(ent_id, ent_update)?;

                    // patch view angles in demos
                    if let Some(angles) = demo_view_angles {
                        if ent_id == self.state.view_entity_id() {
                            self.state.entities[ent_id].msg_angles[0] = angles;
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
                    // TODO: print to in-game console
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

                    // TODO: print sign-on message to in-game console
                    println!("{}", message);

                    let _server_info = ServerInfo {
                        _max_clients: max_clients,
                        _game_type: game_type,
                    };

                    let audio_device = self.state.mixer.audio_device.clone();
                    self.state = ClientState::from_server_info(
                        vfs,
                        audio_device,
                        max_clients,
                        model_precache,
                        sound_precache,
                    )?;

                    // TODO: replace console commands holding `Rc`s to the old ClientState
                    let bonus_cshift =
                        self.state.color_shifts[ColorShiftCode::Bonus as usize].clone();
                    cmds.insert_or_replace(
                        "bf",
                        Box::new(move |_| {
                            bonus_cshift.replace(ColorShift {
                                dest_color: [215, 186, 69],
                                percent: 50,
                            });
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
                    position: _,
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
                        entity_id as usize,
                        channel,
                        volume as f32 / 255.0,
                        attenuation,
                        &self.state.entities,
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
                        audio_device,
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
        audio_device: &rodio::Device,
        idle_vars: IdleVars,
        kick_vars: KickVars,
        roll_vars: RollVars,
        cl_nolerp: f32,
        sv_gravity: f32,
    ) -> Result<(), ClientError> {
        debug!("frame time: {}ms", frame_time.num_milliseconds());

        // do this _before_ parsing server messages so that we know when to
        // request the next message from the demo server.
        self.state.advance_time(frame_time);
        self.parse_server_msg(vfs, gfx_state, cmds, console, audio_device, kick_vars)?;
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
        if let ConnectionState::Connected(_) = *self.conn_state.borrow() {
            // update view
            self.state.calc_final_view(idle_vars, kick_vars, roll_vars);

            // update ear positions
            self.state.update_listener();

            // spatialize sounds for new ear positions
            self.state.update_sound_spatialization();

            // update camera color shifts for new position/effects
            self.state.update_color_shifts(frame_time)?;
        }

        Ok(())
    }
}

pub struct Client {
    vfs: Rc<Vfs>,
    cvars: Rc<RefCell<CvarRegistry>>,
    cmds: Rc<RefCell<CmdRegistry>>,
    console: Rc<RefCell<Console>>,
    input: Rc<RefCell<Input>>,
    audio_device: Rc<rodio::Device>,
    conn: Option<Connection>,
    renderer: ClientRenderer,
}

impl Client {
    /// Implements the `reconnect` command.
    fn cmd_reconnect(conn_state: Rc<RefCell<ConnectionState>>) -> Box<dyn Fn(&[&str])> {
        Box::new(move |_| {
            conn_state.replace(ConnectionState::SignOn(SignOnStage::Prespawn));
        })
    }

    pub fn new(
        vfs: Rc<Vfs>,
        cvars: Rc<RefCell<CvarRegistry>>,
        cmds: Rc<RefCell<CmdRegistry>>,
        console: Rc<RefCell<Console>>,
        input: Rc<RefCell<Input>>,
        audio_device: Rc<rodio::Device>,
        gfx_state: &GraphicsState,
        menu: &Menu,
    ) -> Client {
        // make toggle commands only toggle between menu and console
        cmds.borrow_mut().insert_or_replace(
            "toggleconsole",
            cmd_toggleconsolemenu_disconnected(input.clone()),
        );
        cmds.borrow_mut().insert_or_replace(
            "togglemenu",
            cmd_toggleconsolemenu_disconnected(input.clone()),
        );

        Client {
            vfs,
            cvars,
            cmds,
            console,
            input,
            audio_device,
            conn: None,
            renderer: ClientRenderer::new(gfx_state, menu),
        }
    }

    pub fn play_demo<S>(&mut self, demo_path: S) -> Result<(), ClientError>
    where
        S: AsRef<str>,
    {
        let mut demo_file = self.vfs.open(demo_path)?;
        let demo_server = DemoServer::new(&mut demo_file)?;

        let conn_state = Rc::new(RefCell::new(ConnectionState::SignOn(SignOnStage::Prespawn)));
        self.cmds
            .borrow_mut()
            .insert_or_replace("reconnect", Client::cmd_reconnect(conn_state.clone()));

        self.conn = Some(Connection {
            state: ClientState::new(self.audio_device.clone())?,
            kind: ConnectionKind::Demo(demo_server),
            conn_state,
        });

        self.cmds.borrow_mut().insert_or_replace(
            "toggleconsole",
            cmd_toggleconsole_connected(self.input.clone()),
        );
        self.cmds
            .borrow_mut()
            .insert_or_replace("togglemenu", cmd_togglemenu_connected(self.input.clone()));

        self.input.borrow_mut().set_focus(InputFocus::Game);

        Ok(())
    }

    pub fn connect<A>(&mut self, server_addrs: A) -> Result<(), ClientError>
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

        // set up reconnect
        let conn_state = Rc::new(RefCell::new(ConnectionState::SignOn(SignOnStage::Prespawn)));
        self.cmds
            .borrow_mut()
            .insert_or_replace("reconnect", Client::cmd_reconnect(conn_state.clone()));

        self.conn = Some(Connection {
            state: ClientState::new(self.audio_device.clone())?,
            kind: ConnectionKind::Server {
                qsock,
                compose: Vec::new(),
            },
            conn_state,
        });

        self.cmds.borrow_mut().insert_or_replace(
            "toggleconsole",
            cmd_toggleconsole_connected(self.input.clone()),
        );
        self.cmds
            .borrow_mut()
            .insert_or_replace("togglemenu", cmd_togglemenu_connected(self.input.clone()));

        self.input.borrow_mut().set_focus(InputFocus::Game);

        Ok(())
    }

    pub fn disconnect(&mut self) {
        self.conn = None;

        // make toggle commands only toggle between menu and console
        self.cmds.borrow_mut().insert_or_replace(
            "toggleconsole",
            cmd_toggleconsolemenu_disconnected(self.input.clone()),
        );
        self.cmds.borrow_mut().insert_or_replace(
            "togglemenu",
            cmd_toggleconsolemenu_disconnected(self.input.clone()),
        );

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

        if let Some(ref mut conn) = self.conn {
            conn.frame(
                frame_time,
                &self.vfs,
                gfx_state,
                &mut self.cmds.borrow_mut(),
                &mut self.console.borrow_mut(),
                &self.audio_device,
                idle_vars,
                kick_vars,
                roll_vars,
                cl_nolerp,
                sv_gravity,
            )?;
        } else {
            // don't allow game focus when disconnected
            let focus = self.input.borrow().focus();
            if let InputFocus::Game = focus {
                self.input.borrow_mut().set_focus(InputFocus::Console);
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
            self.conn.as_ref(),
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

        match self.conn {
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

    pub fn state(&self) -> Option<&ClientState> {
        self.conn.as_ref().map(|c| &c.state)
    }

    pub fn get_entity(&self, id: usize) -> Result<&ClientEntity, ClientError> {
        match self.conn {
            Some(Connection { ref state, .. }) => {
                state.check_entity_id(id)?;
                Ok(&state.entities[id])
            }

            None => Err(ClientError::NotConnected),
        }
    }

    pub fn get_entity_mut(&mut self, id: usize) -> Result<&mut ClientEntity, ClientError> {
        match self.conn {
            Some(Connection { ref mut state, .. }) => {
                state.check_entity_id(id)?;
                Ok(&mut state.entities[id])
            }

            None => Err(ClientError::NotConnected),
        }
    }

    pub fn view_origin(&self) -> Result<Vector3<f32>, ClientError> {
        match self.conn {
            Some(Connection { ref state, .. }) => Ok(state.entities[state.view.entity_id()].origin
                + Vector3::new(0.0, 0.0, state.view.view_height())),

            None => Err(ClientError::NotConnected),
        }
    }

    pub fn view_angles(&self, time: Duration) -> Result<Angles, ClientError> {
        let angles = match self.conn {
            Some(Connection {
                ref state,
                ref kind,
                ..
            }) => match kind {
                ConnectionKind::Server { .. } => state.view.final_angles(),

                ConnectionKind::Demo(_) => {
                    let v = state.entities[state.view_entity_id()].angles;
                    Angles {
                        pitch: -v.x,
                        yaw: v.y,
                        roll: -v.z,
                    }
                }
            },

            None => Err(ClientError::NotConnected)?,
        };

        Ok(angles)
    }

    pub fn view_ent(&self) -> Result<usize, ClientError> {
        match self.conn {
            Some(Connection { ref state, .. }) => Ok(state.view.entity_id()),
            None => Err(ClientError::NotConnected),
        }
    }

    pub fn time(&self) -> Result<Duration, ClientError> {
        match self.conn {
            Some(Connection { ref state, .. }) => Ok(state.time),
            None => Err(ClientError::NotConnected),
        }
    }

    pub fn get_lerp_factor(&mut self) -> Result<f32, ClientError> {
        match self.conn {
            Some(Connection { ref state, .. }) => Ok(state.lerp_factor),
            None => Err(ClientError::NotConnected),
        }
    }

    pub fn iter_visible_entities(&self) -> Option<impl Iterator<Item = &ClientEntity> + Clone> {
        self.conn
            .as_ref()
            .map(|conn| conn.state.iter_visible_entities())
    }

    pub fn iter_lights(&self) -> Result<impl Iterator<Item = &Light>, ClientError> {
        match self.conn {
            Some(Connection { ref state, .. }) => Ok(state.lights.iter()),
            None => Err(ClientError::NotConnected),
        }
    }

    pub fn iter_particles(&self) -> Result<impl Iterator<Item = &Particle>, ClientError> {
        match self.conn {
            Some(Connection { ref state, .. }) => Ok(state.particles.iter()),
            None => Err(ClientError::NotConnected),
        }
    }

    pub fn intermission(&self) -> Result<Option<&IntermissionKind>, ClientError> {
        match self.conn {
            Some(Connection { ref state, .. }) => Ok(state.intermission.as_ref()),
            None => Err(ClientError::NotConnected),
        }
    }

    pub fn start_time(&self) -> Result<Duration, ClientError> {
        match self.conn {
            Some(Connection { ref state, .. }) => Ok(state.start_time),
            None => Err(ClientError::NotConnected),
        }
    }

    pub fn completion_time(&self) -> Result<Option<Duration>, ClientError> {
        match self.conn {
            Some(Connection { ref state, .. }) => Ok(state.completion_time),
            None => Err(ClientError::NotConnected),
        }
    }

    pub fn items(&self) -> Result<ItemFlags, ClientError> {
        match self.conn {
            Some(Connection { ref state, .. }) => Ok(state.items),
            None => Err(ClientError::NotConnected),
        }
    }

    pub fn item_get_time(&self) -> Result<&[Duration; net::MAX_ITEMS], ClientError> {
        match self.conn {
            Some(Connection { ref state, .. }) => Ok(&state.item_get_time),
            None => Err(ClientError::NotConnected),
        }
    }

    pub fn weapon(&self) -> Result<i32, ClientError> {
        match self.conn {
            Some(Connection { ref state, .. }) => Ok(state.stats[ClientStat::Weapon as usize]),
            None => Err(ClientError::NotConnected),
        }
    }

    pub fn active_weapon(&self) -> Result<i32, ClientError> {
        match self.conn {
            Some(Connection { ref state, .. }) => {
                Ok(state.stats[ClientStat::ActiveWeapon as usize])
            }
            None => Err(ClientError::NotConnected),
        }
    }

    pub fn stats(&self) -> Result<&[i32; MAX_STATS], ClientError> {
        match self.conn {
            Some(Connection { ref state, .. }) => Ok(&state.stats),
            None => Err(ClientError::NotConnected),
        }
    }

    pub fn face_anim_time(&self) -> Result<Duration, ClientError> {
        match self.conn {
            Some(Connection { ref state, .. }) => Ok(state.face_anim_time),
            None => Err(ClientError::NotConnected),
        }
    }

    pub fn color_shift(&self) -> [f32; 4] {
        match self.conn {
            Some(Connection { ref state, .. }) => {
                state.color_shifts.iter().fold([0.0; 4], |accum, elem| {
                    let elem_a = elem.borrow().percent as f32 / 255.0 / 2.0;
                    if elem_a == 0.0 {
                        return accum;
                    }
                    let in_a = accum[3];
                    let out_a = in_a + elem_a * (1.0 - in_a);
                    let color_factor = elem_a / out_a;

                    let mut out = [0.0; 4];
                    for i in 0..3 {
                        out[i] = accum[i] * (1.0 - color_factor)
                            + elem.borrow().dest_color[i] as f32 / 255.0 * color_factor;
                    }
                    out[3] = out_a.min(1.0).max(0.0);
                    out
                })
            }

            None => [0.0; 4],
        }
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

    pub fn trace<'a, I>(&self, entity_ids: I) -> Result<TraceFrame, ClientError>
    where
        I: IntoIterator<Item = &'a usize>,
    {
        match self.conn {
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

// implements the "toggleconsole" and "togglemenu" commands when the client is disconnected
fn cmd_toggleconsolemenu_disconnected(input: Rc<RefCell<Input>>) -> Box<dyn Fn(&[&str])> {
    Box::new(move |_| {
        let focus = input.borrow().focus();
        match focus {
            InputFocus::Console => input.borrow_mut().set_focus(InputFocus::Menu),
            InputFocus::Game => unreachable!(),
            InputFocus::Menu => input.borrow_mut().set_focus(InputFocus::Console),
        }
    })
}

// implements the "toggleconsole" command when the client is connected
fn cmd_toggleconsole_connected(input: Rc<RefCell<Input>>) -> Box<dyn Fn(&[&str])> {
    Box::new(move |_| {
        let focus = input.borrow().focus();
        match focus {
            InputFocus::Game => input.borrow_mut().set_focus(InputFocus::Console),
            InputFocus::Console => input.borrow_mut().set_focus(InputFocus::Game),
            InputFocus::Menu => input.borrow_mut().set_focus(InputFocus::Console),
        }
    })
}

// implements the "togglemenu" command when the client is connected
fn cmd_togglemenu_connected(input: Rc<RefCell<Input>>) -> Box<dyn Fn(&[&str])> {
    Box::new(move |_| {
        let focus = input.borrow().focus();
        match focus {
            InputFocus::Game => input.borrow_mut().set_focus(InputFocus::Menu),
            InputFocus::Console => input.borrow_mut().set_focus(InputFocus::Menu),
            InputFocus::Menu => input.borrow_mut().set_focus(InputFocus::Game),
        }
    })
}
