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

use crate::common::console::{ConsoleError, CvarRegistry};

pub fn register_cvars(cvars: &CvarRegistry) -> Result<(), ConsoleError> {
    cvars.register("cl_anglespeedkey", "1.5")?;
    cvars.register_archive("cl_backspeed", "200")?;
    cvars.register("cl_bob", "0.02")?;
    cvars.register("cl_bobcycle", "0.6")?;
    cvars.register("cl_bobup", "0.5")?;
    cvars.register_archive("_cl_color", "0")?;
    cvars.register("cl_crossx", "0")?;
    cvars.register("cl_crossy", "0")?;
    cvars.register_archive("cl_forwardspeed", "400")?;
    cvars.register("cl_movespeedkey", "2.0")?;
    cvars.register_archive("_cl_name", "player")?;
    cvars.register("cl_nolerp", "0")?;
    cvars.register("cl_pitchspeed", "150")?;
    cvars.register("cl_rollangle", "2.0")?;
    cvars.register("cl_rollspeed", "200")?;
    cvars.register("cl_shownet", "0")?;
    cvars.register("cl_sidespeed", "350")?;
    cvars.register("cl_upspeed", "200")?;
    cvars.register("cl_yawspeed", "140")?;
    cvars.register("fov", "90")?;
    cvars.register_archive("m_pitch", "0.022")?;
    cvars.register_archive("m_yaw", "0.022")?;
    cvars.register_archive("sensitivity", "3")?;
    cvars.register("v_idlescale", "0")?;
    cvars.register("v_ipitch_cycle", "1")?;
    cvars.register("v_ipitch_level", "0.3")?;
    cvars.register("v_iroll_cycle", "0.5")?;
    cvars.register("v_iroll_level", "0.1")?;
    cvars.register("v_iyaw_cycle", "2")?;
    cvars.register("v_iyaw_level", "0.3")?;
    cvars.register("v_kickpitch", "0.6")?;
    cvars.register("v_kickroll", "0.6")?;
    cvars.register("v_kicktime", "0.5")?;

    // some server cvars are needed by the client, but if the server is running
    // in the same process they will have been set already, so we can ignore
    // the duplicate cvar error
    let _ = cvars.register("sv_gravity", "800");

    Ok(())
}
