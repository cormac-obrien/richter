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

pub mod input;
pub mod render;
pub mod sound;

use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::io::BufReader;
use std::net::ToSocketAddrs;

use client::sound::AudioSource;
use client::sound::StaticSound;
use common::bsp;
use common::engine;
use common::model::Model;
use common::model::ModelKind;
use common::model::SyncType;
use common::net;
use common::net::BlockingMode;
use common::net::ClientCmd;
use common::net::ClientStat;
use common::net::EntityEffects;
use common::net::EntityState;
use common::net::GameType;
use common::net::ItemFlags;
use common::net::NetError;
use common::net::PlayerColor;
use common::net::QSocket;
use common::net::ServerCmd;
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
    ent_id: usize,
    msg_view_angles: [Vector3<Deg<f32>>; 2],

    // TODO: this may not need to be a field (calculated once per frame)
    view_angles: Vector3<Deg<f32>>,

    ideal_pitch: Deg<f32>,
    punch_angle: Vector3<Deg<f32>>,
    view_height: f32,
}

impl ClientView {
    pub fn new() -> ClientView {
        ClientView {
            ent_id: 0,
            msg_view_angles: [
                Vector3::new(Deg(0.0), Deg(0.0), Deg(0.0)),
                Vector3::new(Deg(0.0), Deg(0.0), Deg(0.0)),
            ],
            view_angles: Vector3::new(Deg(0.0), Deg(0.0), Deg(0.0)),
            ideal_pitch: Deg(0.0),
            punch_angle: Vector3::new(Deg(0.0), Deg(0.0), Deg(0.0)),
            view_height: 0.0,
        }
    }
}

struct PlayerInfo {
    name: String,
    frags: i32,
    colors: PlayerColor,
    // translations: [u8; VID_GRADES],
}

pub struct ClientEntity {
    force_link: bool,
    baseline: EntityState,

    msg_time: Duration,
    msg_origins: [Vector3<f32>; 2],
    origin: Vector3<f32>,
    msg_angles: [Vector3<Deg<f32>>; 2],
    angles: Vector3<Deg<f32>>,
    model_id: usize,
    frame_id: usize,
    skin_id: usize,
    sync_base: Duration,
    effects: EntityEffects,
    // vis_frame: usize,
}

impl ClientEntity {
    pub fn from_baseline(baseline: EntityState) -> ClientEntity {
        ClientEntity {
            force_link: false,
            baseline: baseline.clone(),
            msg_time: Duration::zero(),
            msg_origins: [Vector3::new(0.0, 0.0, 0.0), Vector3::new(0.0, 0.0, 0.0)],
            origin: baseline.origin,
            msg_angles: [
                Vector3::new(Deg(0.0), Deg(0.0), Deg(0.0)),
                Vector3::new(Deg(0.0), Deg(0.0), Deg(0.0)),
            ],
            angles: baseline.angles,
            model_id: baseline.model_id,
            frame_id: baseline.frame_id,
            skin_id: baseline.skin_id,
            sync_base: Duration::zero(),
            effects: baseline.effects,
        }
    }

    pub fn uninitialized() -> ClientEntity {
        ClientEntity {
            force_link: false,
            baseline: EntityState::uninitialized(),
            msg_time: Duration::zero(),
            msg_origins: [Vector3::new(0.0, 0.0, 0.0), Vector3::new(0.0, 0.0, 0.0)],
            origin: Vector3::new(0.0, 0.0, 0.0),
            msg_angles: [
                Vector3::new(Deg(0.0), Deg(0.0), Deg(0.0)),
                Vector3::new(Deg(0.0), Deg(0.0), Deg(0.0)),
            ],
            angles: Vector3::new(Deg(0.0), Deg(0.0), Deg(0.0)),
            model_id: 0,
            frame_id: 0,
            skin_id: 0,
            sync_base: Duration::zero(),
            effects: EntityEffects::empty(),
        }
    }
}

// client information regarding the current level
struct ClientState {
    // model precache
    models: Vec<Model>,

    // audio source precache
    sounds: Vec<AudioSource>,

    // ambient sounds (infinite looping, static position)
    static_sounds: Vec<StaticSound>,

    entities: Vec<ClientEntity>,

    light_styles: HashMap<u8, String>,

    // various values relevant to the player and level (see common::net::ClientStat)
    stats: [i32; MAX_STATS],

    max_players: usize,
    player_info: [Option<PlayerInfo>; net::MAX_CLIENTS],

    // the last two timestamps sent by the server (for lerping)
    msg_times: [Duration; 2],
    time: Duration,
    // old_time: Duration,

    // move_msg_count: usize,
    // cmd: MoveCmd,
    items: ItemFlags,
    item_get_time: [Duration; net::MAX_ITEMS],
    // face_anim_time: f32,
    // color_shifts: [ColorShift; 4],
    // prev_color_shifts: [ColorShift; 4],
    view: ClientView,

    msg_velocity: [Vector3<f32>; 2],
    velocity: Vector3<f32>,

    // ideal_pitch: Deg<f32>,
    // pitch_velocity: f32,
    // no_drift: bool,
    // drift_move: f32,
    // last_stop: f64,

    // paused: bool,
    on_ground: bool,
    in_water: bool,
    // intermission: IntermissionKind,
    // completed_time: Duration,

    // last_received_message: f32,

    // level_name: String,
    // view_ent: usize,

    // server_info: ServerInfo,

    // worldmodel: Model,
}

impl ClientState {
    // TODO: add parameter for number of player slots and reserve them in entity list
    pub fn new(pak: &Pak) -> ClientState {
        ClientState {
            models: vec![Model::none()],
            sounds: vec![AudioSource::load(pak, "misc/null.wav").unwrap()],
            static_sounds: Vec::new(),
            entities: Vec::new(),
            light_styles: HashMap::new(),
            stats: [0; MAX_STATS],
            max_players: 0,
            // TODO: for the love of god can the lang team hurry up (https://github.com/rust-lang/rfcs/pull/2203)
            // this might make more sense as a different data structure anyway who knows
            player_info: [
                None, None, None, None, None, None, None, None, None, None, None, None, None, None,
                None, None,
            ],
            msg_times: [Duration::zero(), Duration::zero()],
            time: Duration::zero(),
            items: ItemFlags::empty(),
            // TODO: make this less horrific once const fn array initializers are available
            item_get_time: [
                Duration::zero(),
                Duration::zero(),
                Duration::zero(),
                Duration::zero(),
                Duration::zero(),
                Duration::zero(),
                Duration::zero(),
                Duration::zero(),
                Duration::zero(),
                Duration::zero(),
                Duration::zero(),
                Duration::zero(),
                Duration::zero(),
                Duration::zero(),
                Duration::zero(),
                Duration::zero(),
                Duration::zero(),
                Duration::zero(),
                Duration::zero(),
                Duration::zero(),
                Duration::zero(),
                Duration::zero(),
                Duration::zero(),
                Duration::zero(),
                Duration::zero(),
                Duration::zero(),
                Duration::zero(),
                Duration::zero(),
                Duration::zero(),
                Duration::zero(),
                Duration::zero(),
                Duration::zero(),
            ],
            view: ClientView::new(),
            msg_velocity: [Vector3::zero(), Vector3::zero()],
            velocity: Vector3::zero(),
            on_ground: false,
            in_water: false,
        }
    }
}

pub struct Client {
    qsock: QSocket,
    compose: Vec<u8>,
    signon: SignOnStage,

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
                Request::connect(net::GAME_NAME, CONNECT_PROTOCOL_VERSION),
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
                return Err(ClientError::with_msg(format!(
                    "Connection rejected: {}",
                    reject.message
                )))
            }

            // the server sent back a response that doesn't make sense here (i.e. something other
            // than an Accept or Reject).
            // TODO: more specific error. this shouldn't be fatal.
            _ => return Err(ClientError::with_msg("Invalid connect response")),
        };

        let mut new_addr = server_addr;
        new_addr.set_port(port);

        // we're done with the connection socket, so turn it into a QSocket with the new address
        let qsock = con_sock.into_qsocket(new_addr);

        Ok(Client {
            qsock,
            compose: Vec::new(),
            signon: SignOnStage::Not,
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

    // return an error if the given entity ID does not refer to a valid entity
    fn check_entity_id(&self, id: usize) -> Result<(), ClientError> {
        if id == 0 {
            return Err(ClientError::Other(String::from("entity 0 is NULL")));
        }

        if id >= self.state.entities.len() {
            return Err(ClientError::Other(format!("invalid entity id ({})", id)));
        }

        Ok(())
    }

    fn check_player_id(&self, id: usize) -> Result<(), ClientError> {
        if id > net::MAX_CLIENTS {
            return Err(ClientError::Other(format!(
                "player ID {} exceeds net::MAX_CLIENTS ({})",
                id,
                net::MAX_CLIENTS
            )));
        }

        if id > self.state.max_players {
            return Err(ClientError::Other(format!(
                "player ID ({}) exceeds max_players ({})",
                id, self.state.max_players,
            )));
        }

        Ok(())
    }

    /// Spawn an entity with the given ID, also spawning any uninitialized entities between the former
    /// last entity and the new one.
    // TODO: skipping entities indicates that the entities have been freed by the server. it may
    // make more sense to use a HashMap to store entities by ID since the lookup table is relatively
    // sparse.
    pub fn spawn_entities(
        &mut self,
        ent_id: u16,
        model_id: u8,
        frame_id: u8,
        colormap: u8,
        skin_id: u8,
        origin: Vector3<f32>,
        angles: Vector3<Deg<f32>>,
    ) -> Result<(), ClientError> {
        let id = ent_id as usize;

        // don't clobber existing entities
        if id < self.state.entities.len() {
            return Err(ClientError::Other(format!("entity {} already exists", id)));
        }

        // spawn intermediate entities (uninitialized)
        for i in self.state.entities.len()..id {
            debug!("Spawning uninitialized entity with ID {}", i);
            self.state.entities.push(ClientEntity::uninitialized());
        }

        let baseline = EntityState {
            origin: origin,
            angles: angles,
            model_id: model_id as usize,
            frame_id: frame_id as usize,
            colormap: colormap,
            skin_id: skin_id as usize,
            effects: EntityEffects::empty(),
        };

        debug!(
            "Spawning entity with id {} from baseline {:?}",
            id, baseline
        );

        self.state
            .entities
            .push(ClientEntity::from_baseline(baseline));

        Ok(())
    }

    pub fn get_entity(&self, id: usize) -> Result<&ClientEntity, ClientError> {
        self.check_entity_id(id)?;
        Ok(&self.state.entities[id])
    }

    pub fn get_entity_mut(&mut self, id: usize) -> Result<&mut ClientEntity, ClientError> {
        self.check_entity_id(id)?;
        Ok(&mut self.state.entities[id])
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

                ServerCmd::CdTrack { .. } => {
                    // TODO: play CD track
                    debug!("CD tracks not yet implemented");
                }

                ServerCmd::ClientData {
                    view_height,
                    ideal_pitch,
                    punch_pitch,
                    velocity_x,
                    punch_yaw,
                    velocity_y,
                    punch_roll,
                    velocity_z,
                    items,
                    on_ground,
                    in_water,
                    weapon_frame,
                    armor,
                    weapon,
                    health,
                    ammo,
                    ammo_shells,
                    ammo_nails,
                    ammo_rockets,
                    ammo_cells,
                    active_weapon,
                } => {
                    debug!("Updating local client data: {:?}", &cmd);
                    self.state.view.view_height = view_height.unwrap_or(net::DEFAULT_VIEWHEIGHT);
                    self.state.view.ideal_pitch = ideal_pitch.unwrap_or(Deg(0.0));

                    self.state.view.punch_angle[0] = punch_pitch.unwrap_or(Deg(0.0));
                    self.state.view.punch_angle[1] = punch_yaw.unwrap_or(Deg(0.0));
                    self.state.view.punch_angle[2] = punch_roll.unwrap_or(Deg(0.0));

                    // store old velocity
                    self.state.msg_velocity[1] = self.state.msg_velocity[0];
                    self.state.msg_velocity[0].x = velocity_x.unwrap_or(0.0);
                    self.state.msg_velocity[0].y = velocity_y.unwrap_or(0.0);
                    self.state.msg_velocity[0].z = velocity_z.unwrap_or(0.0);

                    if items != self.state.items {
                        // item flags have changed, something got picked up
                        // TODO: original engine calls Sbar_Changed() here to update status bar
                        for i in 0..net::MAX_ITEMS {
                            if (items.bits() & 1 << i) != 0
                                && (self.state.items.bits() & 1 << i) == 0
                            {
                                // item with flag value `i` was picked up
                                self.state.item_get_time[i] = self.state.time;
                            }
                        }

                        self.state.items = items;
                    }

                    self.state.on_ground = on_ground;
                    self.state.in_water = in_water;

                    self.state.stats[ClientStat::WeaponFrame as usize] =
                        weapon_frame.unwrap_or(0) as i32;

                    // TODO: these ClientStat conditionals should be convertible to a method

                    let armor = armor.unwrap_or(0);
                    if self.state.stats[ClientStat::Armor as usize] != armor as i32 {
                        self.state.stats[ClientStat::Armor as usize] = armor as i32;
                        // TODO: update status bar
                    }

                    let weapon = weapon.unwrap_or(0);
                    if self.state.stats[ClientStat::Weapon as usize] != weapon as i32 {
                        self.state.stats[ClientStat::Weapon as usize] = weapon as i32;
                        // TODO: update status bar
                    }

                    if self.state.stats[ClientStat::Health as usize] != health as i32 {
                        self.state.stats[ClientStat::Health as usize] = health as i32;
                        // TODO: update status bar
                    }

                    if self.state.stats[ClientStat::Ammo as usize] != ammo as i32 {
                        self.state.stats[ClientStat::Ammo as usize] = ammo as i32;
                        // TODO: update status bar
                    }

                    if self.state.stats[ClientStat::Shells as usize] != ammo_shells as i32 {
                        self.state.stats[ClientStat::Shells as usize] = ammo_shells as i32;
                        // TODO: update status bar
                    }

                    if self.state.stats[ClientStat::Nails as usize] != ammo_nails as i32 {
                        self.state.stats[ClientStat::Nails as usize] = ammo_nails as i32;
                        // TODO: update status bar
                    }

                    if self.state.stats[ClientStat::Rockets as usize] != ammo_rockets as i32 {
                        self.state.stats[ClientStat::Rockets as usize] = ammo_rockets as i32;
                        // TODO: update status bar
                    }

                    if self.state.stats[ClientStat::Cells as usize] != ammo_cells as i32 {
                        self.state.stats[ClientStat::Cells as usize] = ammo_cells as i32;
                        // TODO: update status bar
                    }

                    // TODO: this behavior assumes the `standard_quake` behavior and will likely
                    // break with the mission packs
                    if self.state.stats[ClientStat::ActiveWeapon as usize] != active_weapon as i32 {
                        self.state.stats[ClientStat::ActiveWeapon as usize] = active_weapon as i32;
                        // TODO: update status bar
                    }
                }

                ServerCmd::FastUpdate {
                    ent_id,
                    model_id,
                    frame_id,
                    colormap,
                    skin_id,
                    effects,
                    origin_x,
                    pitch,
                    origin_y,
                    yaw,
                    origin_z,
                    roll,
                    no_lerp,
                } => {
                    // first update signals the last sign-on stage
                    if self.signon == SignOnStage::Begin {
                        self.signon = SignOnStage::Done;
                        let signon = self.signon;
                        self.handle_signon(signon)?;
                    }

                    let mut force_link = false;

                    let ent_id = ent_id as usize;
                    self.check_entity_id(ent_id)?;

                    // did we get an update for this entity last frame?
                    if self.state.entities[ent_id].msg_time != self.state.msg_times[1] {
                        // if not, we can't lerp
                        force_link = true;
                    }

                    // update entity update time
                    self.state.entities[ent_id].msg_time = self.state.msg_times[0];

                    let new_model_id = match model_id {
                        Some(m_id) => {
                            if m_id as usize >= self.state.models.len() {
                                return Err(ClientError::with_msg(format!(
                                    "Update for entity {}: model ID {} is out of range",
                                    ent_id, m_id
                                )));
                            }

                            m_id as usize
                        }

                        None => self.state.entities[ent_id].baseline.model_id,
                    };

                    if self.state.entities[ent_id].model_id != new_model_id {
                        // model has changed
                        self.state.entities[ent_id].model_id = new_model_id;
                        match self.state.models[new_model_id].kind() {
                            &ModelKind::None => force_link = true,
                            m => {
                                self.state.entities[ent_id].sync_base =
                                    match self.state.models[new_model_id].sync_type() {
                                        SyncType::Sync => Duration::zero(),
                                        SyncType::Rand => unimplemented!(), // TODO
                                    }
                            }
                        }
                    }

                    self.state.entities[ent_id].frame_id = frame_id
                        .map(|x| x as usize)
                        .unwrap_or(self.state.entities[ent_id].baseline.frame_id);

                    let new_colormap =
                        colormap.unwrap_or(self.state.entities[ent_id].baseline.colormap) as usize;
                    if new_colormap == 0 {
                        // TODO: use default colormap
                    } else {
                        // only players may have custom colormaps
                        if new_colormap > self.state.max_players {
                            return Err(ClientError::with_msg(format!(
                                "Attempted to assign custom colormap to entity with ID {}",
                                ent_id
                            )));
                        }

                        // TODO: set player custom colormaps
                        warn!("Player colormaps not yet implemented");
                    }

                    self.state.entities[ent_id].skin_id = skin_id
                        .map(|x| x as usize)
                        .unwrap_or(self.state.entities[ent_id].baseline.skin_id);
                    self.state.entities[ent_id].effects =
                        effects.unwrap_or(self.state.entities[ent_id].baseline.effects);

                    // save previous origin and angles
                    self.state.entities[ent_id].msg_origins[1] =
                        self.state.entities[ent_id].msg_origins[0];
                    self.state.entities[ent_id].msg_angles[1] =
                        self.state.entities[ent_id].msg_angles[0];

                    // update origin
                    self.state.entities[ent_id].msg_origins[0].x =
                        origin_x.unwrap_or(self.state.entities[ent_id].baseline.origin.x);
                    self.state.entities[ent_id].msg_origins[0].y =
                        origin_y.unwrap_or(self.state.entities[ent_id].baseline.origin.y);
                    self.state.entities[ent_id].msg_origins[0].z =
                        origin_z.unwrap_or(self.state.entities[ent_id].baseline.origin.z);

                    // update angles
                    self.state.entities[ent_id].msg_angles[0][0] =
                        pitch.unwrap_or(self.state.entities[ent_id].baseline.angles[0]);
                    self.state.entities[ent_id].msg_angles[0][1] =
                        yaw.unwrap_or(self.state.entities[ent_id].baseline.angles[1]);
                    self.state.entities[ent_id].msg_angles[0][2] =
                        roll.unwrap_or(self.state.entities[ent_id].baseline.angles[2]);

                    if no_lerp {
                        force_link = true;
                    }

                    if force_link {
                        self.state.entities[ent_id].msg_origins[1] =
                            self.state.entities[ent_id].msg_origins[0];
                        self.state.entities[ent_id].origin =
                            self.state.entities[ent_id].msg_origins[0];
                        self.state.entities[ent_id].msg_angles[1] =
                            self.state.entities[ent_id].msg_angles[0];
                        self.state.entities[ent_id].angles =
                            self.state.entities[ent_id].msg_angles[0];
                        self.state.entities[ent_id].force_link = true;
                    }
                }

                ServerCmd::LightStyle { id, value } => {
                    debug!("Inserting light style {} with value {}", id, &value);
                    let _ = self.state.light_styles.insert(id, value);
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
                    self.update_server_info(
                        protocol_version,
                        max_clients,
                        game_type,
                        message,
                        model_precache,
                        sound_precache,
                        pak,
                    )?;
                }

                ServerCmd::SetAngle { angles } => {
                    debug!("Set view angles to {:?}", angles);
                    self.state.view.msg_view_angles[1] = self.state.view.msg_view_angles[0];
                    self.state.view.msg_view_angles[0] = angles;
                }

                ServerCmd::SetView { ent_id } => {
                    let new_view_ent_id = ent_id as usize;
                    if new_view_ent_id == 0 {
                        return Err(ClientError::with_msg("Server set view entity to NULL"));
                    }

                    // we have to allow the server to SetView on the player entity ID, which will
                    // be uninitialized at first.
                    if new_view_ent_id >= self.state.max_players
                        && new_view_ent_id >= self.state.entities.len()
                    {
                        return Err(ClientError::with_msg(format!(
                            "View entity ID is out of range: {}",
                            new_view_ent_id
                        )));
                    }

                    debug!("Set view entity to {}", ent_id);
                    self.state.view.ent_id = new_view_ent_id;
                }

                ServerCmd::SignOnStage { stage } => self.handle_signon(stage)?,

                ServerCmd::SpawnBaseline {
                    ent_id,
                    model_id,
                    frame_id,
                    colormap,
                    skin_id,
                    origin,
                    angles,
                } => {
                    self.spawn_entities(
                        ent_id,
                        model_id,
                        frame_id,
                        colormap,
                        skin_id,
                        origin,
                        angles,
                    )?;
                }

                ServerCmd::SpawnStaticSound {
                    origin,
                    sound_id,
                    volume,
                    attenuation,
                } => {
                    self.state.static_sounds.push(StaticSound::new(
                        &self.audio_endpoint,
                        origin,
                        self.state.sounds[sound_id as usize].clone(),
                        volume,
                        attenuation,
                    ));
                }

                ServerCmd::Time { time } => {
                    self.state.msg_times[1] = self.state.msg_times[0];
                    self.state.msg_times[0] = engine::duration_from_f32(time);
                }

                ServerCmd::UpdateColors {
                    player_id,
                    new_colors,
                } => {
                    let player_id = player_id as usize;
                    self.check_player_id(player_id)?;

                    match self.state.player_info[player_id] {
                        Some(ref mut info) => {
                            debug!(
                                "Player {} (ID {}) colors: {:?} -> {:?}",
                                info.name, player_id, info.colors, new_colors,
                            );
                            info.colors = new_colors;
                        }

                        None => {
                            return Err(ClientError::with_msg(format!(
                                "No player with ID {}",
                                player_id
                            )));
                        }
                    }
                }

                ServerCmd::UpdateFrags {
                    player_id,
                    new_frags,
                } => {
                    let player_id = player_id as usize;
                    self.check_player_id(player_id)?;

                    match self.state.player_info[player_id] {
                        Some(ref mut info) => {
                            debug!(
                                "Player {} (ID {}) frags: {} -> {}",
                                &info.name, player_id, info.frags, new_frags
                            );
                            info.frags = new_frags as i32;
                        }
                        None => {
                            return Err(ClientError::with_msg(format!(
                                "No player with ID {}",
                                player_id
                            )));
                        }
                    }
                }

                ServerCmd::UpdateName {
                    player_id,
                    new_name,
                } => {
                    let player_id = player_id as usize;
                    self.check_player_id(player_id)?;

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
                    debug!(
                        "{:?}: {} -> {}",
                        stat, self.state.stats[stat as usize], value
                    );
                    self.state.stats[stat as usize] = value;
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
                self.add_cmd(ClientCmd::StringCmd {
                    cmd: String::from("prespawn"),
                })?;
            }
            SignOnStage::ClientInfo => {
                // TODO: fill in client info here
                self.add_cmd(ClientCmd::StringCmd {
                    cmd: format!("name \"{}\"\n", "UNNAMED"),
                })?;
                self.add_cmd(ClientCmd::StringCmd {
                    cmd: format!("color {} {}", 0, 0),
                })?;
                // TODO: need default spawn parameters?
                self.add_cmd(ClientCmd::StringCmd {
                    cmd: format!("spawn {}", ""),
                })?;
            }
            SignOnStage::Begin => {
                self.add_cmd(ClientCmd::StringCmd {
                    cmd: String::from("begin"),
                })?;
            }
            SignOnStage::Done => {
                debug!("Signon complete");
                // TODO: end load screen and start render loop
            }
        }

        self.signon = stage;

        Ok(())
    }

    fn update_server_info(
        &mut self,
        protocol_version: i32,
        max_clients: u8,
        game_type: GameType,
        message: String,
        model_precache: Vec<String>,
        sound_precache: Vec<String>,
        pak: &Pak,
    ) -> Result<(), ClientError> {
        let mut new_client_state = ClientState::new(pak);

        // check protocol version
        if protocol_version != net::PROTOCOL_VERSION as i32 {
            return Err(ClientError::with_msg(format!(
                "Incompatible protocol version (got {}, should be {})",
                protocol_version,
                net::PROTOCOL_VERSION
            )));
        }

        // TODO: print sign-on message to in-game console
        println!("{}", message);

        // parse model precache
        // TODO: validate submodel names
        for mod_name in model_precache {
            if mod_name.ends_with(".bsp") {
                let bsp_data = match pak.open(&mod_name) {
                    Some(b) => b,
                    None => {
                        return Err(ClientError::with_msg(format!(
                            "Model not found in pak archive: {}",
                            mod_name
                        )))
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

        // parse sound precache
        for ref snd_name in sound_precache {
            debug!("Loading sound {}", snd_name);

            // TODO: waiting on tomaka/rodio#157
            new_client_state
                .sounds
                .push(match AudioSource::load(pak, snd_name) {
                    Ok(a) => a,
                    Err(e) => {
                        warn!("Loading {} failed: {}", snd_name, e);
                        AudioSource::load(pak, "misc/null.wav").unwrap()
                    }
                });

            // TODO: send keepalive message?
        }

        let server_info = ServerInfo {
            max_clients: max_clients,
            game_type: game_type,
        };

        new_client_state.max_players = server_info.max_clients as usize;

        // TODO: set up rest of client state (R_NewMap)

        self.state = new_client_state;
        Ok(())
    }
}
