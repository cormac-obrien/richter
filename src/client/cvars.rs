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

use common::console::CvarRegistry;

pub fn register_cvars(cvars: &CvarRegistry) {
    cvars.register("cl_anglespeedkey", "1.5").unwrap();
    cvars.register_archive("cl_backspeed", "200").unwrap();
    cvars.register("cl_bob", "0.02").unwrap();
    cvars.register("cl_bobcycle", "0.6").unwrap();
    cvars.register("cl_bobup", "0.5").unwrap();
    cvars.register_archive("_cl_color", "0").unwrap();
    cvars.register("cl_crossx", "0").unwrap();
    cvars.register("cl_crossy", "0").unwrap();
    cvars.register_archive("cl_forwardspeed", "400").unwrap();
    cvars.register("cl_movespeedkey", "2.0").unwrap();
    cvars.register_archive("_cl_name", "player").unwrap();
    cvars.register("cl_nolerp", "0").unwrap();
    cvars.register("cl_pitchspeed", "150").unwrap();
    cvars.register("cl_rollangle", "2.0").unwrap();
    cvars.register("cl_rollspeed", "200").unwrap();
    cvars.register("cl_shownet", "0").unwrap();
    cvars.register("cl_sidespeed", "350").unwrap();
    cvars.register("cl_upspeed", "200").unwrap();
    cvars.register("cl_yawspeed", "140").unwrap();
    cvars.register("fov", "90").unwrap();
}
