use std::f32::consts::PI;

use crate::{
    client::input::game::{Action, GameInput},
    common::{
        engine::{duration_from_f32, duration_to_f32},
        math::{self, Angles},
    },
};

use super::IntermissionKind;
use cgmath::{Angle as _, Deg, InnerSpace as _, Vector3, Zero as _};
use chrono::Duration;

pub struct View {
    // entity "holding" the camera
    entity_id: usize,

    // how high the entity is "holding" the camera
    view_height: f32,

    // TODO
    ideal_pitch: Deg<f32>,

    // view angles from the server
    msg_angles: [Angles; 2],

    // view angles from client input
    input_angles: Angles,

    // pitch and roll from damage
    damage_angles: Angles,

    // time at which damage punch decays to zero
    damage_time: Duration,

    // punch angles from server
    punch_angles: Angles,

    // final angles combining all sources
    final_angles: Angles,

    // final origin accounting for view bob
    final_origin: Vector3<f32>,
}

impl View {
    pub fn new() -> View {
        View {
            entity_id: 0,
            view_height: 0.0,
            ideal_pitch: Deg(0.0),
            msg_angles: [Angles::zero(); 2],
            input_angles: Angles::zero(),
            damage_angles: Angles::zero(),
            damage_time: Duration::zero(),
            punch_angles: Angles::zero(),
            final_angles: Angles::zero(),
            final_origin: Vector3::zero(),
        }
    }

    pub fn entity_id(&self) -> usize {
        self.entity_id
    }

    pub fn set_entity_id(&mut self, id: usize) {
        self.entity_id = id;
    }

    pub fn view_height(&self) -> f32 {
        self.view_height
    }

    pub fn set_view_height(&mut self, view_height: f32) {
        self.view_height = view_height;
    }

    pub fn ideal_pitch(&self) -> Deg<f32> {
        self.ideal_pitch
    }

    pub fn set_ideal_pitch(&mut self, ideal_pitch: Deg<f32>) {
        self.ideal_pitch = ideal_pitch;
    }

    pub fn punch_angles(&self) -> Angles {
        self.punch_angles
    }

    pub fn set_punch_angles(&mut self, punch_angles: Angles) {
        self.punch_angles = punch_angles;
    }

    pub fn input_angles(&self) -> Angles {
        self.input_angles
    }

    /// Update the current input angles with a new value.
    pub fn update_input_angles(&mut self, input_angles: Angles) {
        self.input_angles = input_angles;
    }

    pub fn handle_input(
        &mut self,
        frame_time: Duration,
        game_input: &GameInput,
        intermission: Option<&IntermissionKind>,
        mlook: bool,
        cl_anglespeedkey: f32,
        cl_pitchspeed: f32,
        cl_yawspeed: f32,
        mouse_vars: MouseVars,
    ) {
        let frame_time_f32 = duration_to_f32(frame_time);
        let speed = if game_input.action_state(Action::Speed) {
            frame_time_f32 * cl_anglespeedkey
        } else {
            frame_time_f32
        };

        // ignore camera controls during intermission
        if intermission.is_some() {
            return;
        }

        if !game_input.action_state(Action::Strafe) {
            let right_factor = game_input.action_state(Action::Right) as i32 as f32;
            let left_factor = game_input.action_state(Action::Left) as i32 as f32;
            self.input_angles.yaw += Deg(speed * cl_yawspeed * (left_factor - right_factor));
            self.input_angles.yaw = self.input_angles.yaw.normalize();
        }

        let lookup_factor = game_input.action_state(Action::LookUp) as i32 as f32;
        let lookdown_factor = game_input.action_state(Action::LookDown) as i32 as f32;
        self.input_angles.pitch += Deg(speed * cl_pitchspeed * (lookdown_factor - lookup_factor));

        if mlook {
            let pitch_factor = mouse_vars.m_pitch * mouse_vars.sensitivity;
            let yaw_factor = mouse_vars.m_yaw * mouse_vars.sensitivity;
            self.input_angles.pitch += Deg(game_input.mouse_delta().1 as f32 * pitch_factor);
            self.input_angles.yaw -= Deg(game_input.mouse_delta().0 as f32 * yaw_factor);
        }

        if lookup_factor != 0.0 || lookdown_factor != 0.0 {
            // TODO: V_StopPitchDrift
        }

        // clamp pitch to [-70, 80] and roll to [-50, 50]
        self.input_angles.pitch = math::clamp_deg(self.input_angles.pitch, Deg(-70.0), Deg(80.0));
        self.input_angles.roll = math::clamp_deg(self.input_angles.roll, Deg(-50.0), Deg(50.0));
    }

    pub fn handle_damage(
        &mut self,
        time: Duration,
        armor_dmg: f32,
        health_dmg: f32,
        view_ent_origin: Vector3<f32>,
        view_ent_angles: Angles,
        src_origin: Vector3<f32>,
        vars: KickVars,
    ) {
        self.damage_time = time + duration_from_f32(vars.v_kicktime);

        // dmg_factor is at most 10.0
        let dmg_factor = (armor_dmg + health_dmg).min(20.0) / 2.0;
        let dmg_vector = (view_ent_origin - src_origin).normalize();
        let rot = view_ent_angles.mat3_quake();

        let roll_factor = dmg_vector.dot(-rot.x);
        self.damage_angles.roll = Deg(dmg_factor * roll_factor * vars.v_kickroll);

        let pitch_factor = dmg_vector.dot(rot.y);
        self.damage_angles.pitch = Deg(dmg_factor * pitch_factor * vars.v_kickpitch);
    }

    pub fn calc_final_angles(
        &mut self,
        time: Duration,
        intermission: Option<&IntermissionKind>,
        velocity: Vector3<f32>,
        mut idle_vars: IdleVars,
        kick_vars: KickVars,
        roll_vars: RollVars,
    ) {
        let move_angles = Angles {
            pitch: Deg(0.0),
            roll: roll(self.input_angles, velocity, roll_vars),
            yaw: Deg(0.0),
        };

        let kick_factor = duration_to_f32(self.damage_time - time).max(0.0) / kick_vars.v_kicktime;
        let damage_angles = self.damage_angles * kick_factor;

        // always idle during intermission
        if intermission.is_some() {
            idle_vars.v_idlescale = 1.0;
        }
        let idle_angles = idle(time, idle_vars);

        self.final_angles =
            self.input_angles + move_angles + damage_angles + self.punch_angles + idle_angles;
    }

    pub fn final_angles(&self) -> Angles {
        self.final_angles
    }

    pub fn calc_final_origin(
        &mut self,
        time: Duration,
        origin: Vector3<f32>,
        velocity: Vector3<f32>,
        bob_vars: BobVars,
    ) {
        // offset the view by 1/32 unit to keep it from intersecting liquid planes
        let plane_offset = Vector3::new(1.0 / 32.0, 1.0 / 32.0, 1.0 / 32.0);
        let height_offset = Vector3::new(0.0, 0.0, self.view_height);
        let bob_offset = Vector3::new(0.0, 0.0, bob(time, velocity, bob_vars));
        self.final_origin = origin + plane_offset + height_offset + bob_offset;
    }

    pub fn final_origin(&self) -> Vector3<f32> {
        self.final_origin
    }
}

#[derive(Copy, Clone, Debug)]
pub struct MouseVars {
    pub m_pitch: f32,
    pub m_yaw: f32,
    pub sensitivity: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct KickVars {
    pub v_kickpitch: f32,
    pub v_kickroll: f32,
    pub v_kicktime: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct BobVars {
    pub cl_bob: f32,
    pub cl_bobcycle: f32,
    pub cl_bobup: f32,
}

pub fn bob(time: Duration, velocity: Vector3<f32>, vars: BobVars) -> f32 {
    let time = duration_to_f32(time);
    let ratio = (time % vars.cl_bobcycle) / vars.cl_bobcycle;
    let cycle = if ratio < vars.cl_bobup {
        PI * ratio / vars.cl_bobup
    } else {
        PI + PI * (ratio - vars.cl_bobup) / (1.0 - vars.cl_bobup)
    };

    // drop z coordinate
    let vel_mag = velocity.truncate().magnitude();
    let bob = vars.cl_bob * (vel_mag * 0.3 + vel_mag * 0.7 * cycle.sin());

    bob.max(-7.0).min(4.0)
}

#[derive(Clone, Copy, Debug)]
pub struct RollVars {
    pub cl_rollangle: f32,
    pub cl_rollspeed: f32,
}

pub fn roll(angles: Angles, velocity: Vector3<f32>, vars: RollVars) -> Deg<f32> {
    let rot = angles.mat3_quake();
    let side = velocity.dot(rot.y);
    let sign = side.signum();
    let side_abs = side.abs();

    let roll_abs = if side < vars.cl_rollspeed {
        side_abs * vars.cl_rollangle / vars.cl_rollspeed
    } else {
        vars.cl_rollangle
    };

    Deg(roll_abs * sign)
}

#[derive(Clone, Copy, Debug)]
pub struct IdleVars {
    pub v_idlescale: f32,
    pub v_ipitch_cycle: f32,
    pub v_ipitch_level: f32,
    pub v_iroll_cycle: f32,
    pub v_iroll_level: f32,
    pub v_iyaw_cycle: f32,
    pub v_iyaw_level: f32,
}

pub fn idle(time: Duration, vars: IdleVars) -> Angles {
    let time = duration_to_f32(time);
    let pitch = Deg(vars.v_idlescale * (time * vars.v_ipitch_cycle).sin() * vars.v_ipitch_level);
    let roll = Deg(vars.v_idlescale * (time * vars.v_iroll_cycle).sin() * vars.v_iroll_level);
    let yaw = Deg(vars.v_idlescale * (time * vars.v_iyaw_cycle).sin() * vars.v_iyaw_level);

    Angles { pitch, roll, yaw }
}
