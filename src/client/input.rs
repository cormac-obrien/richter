// Copyright © 2018 Cormac O'Brien
//
// Permission is hereby granted, free of charge, to any person obtaining a copy of this software
// and associated documentation files (the "Software"), to deal in the Software without
// restriction, including without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all copies or
// substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING
// BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
// NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
// DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

pub enum InputFocus {
    Game,
    Console,
    Menu,
}

pub struct InputState {
    pub forward: bool,
    pub back: bool,
    pub moveleft: bool,
    pub moveright: bool,

    pub moveup: bool,
    pub movedown: bool,

    pub left: bool,
    pub right: bool,
    pub lookup: bool,
    pub lookdown: bool,

    pub speed: bool,
    pub jump: bool,
    pub strafe: bool,
    pub attack: bool,
    pub use_: bool,

    pub klook: bool,
    pub mlook: bool,

    pub showscores: bool,
    pub showteamscores: bool,
}

impl InputState {
    pub fn new() -> Self {
        InputState {
            forward: false,
            back: false,
            moveleft: false,
            moveright: false,

            moveup: false,
            movedown: false,

            left: false,
            right: false,
            lookup: false,
            lookdown: false,

            speed: false,
            jump: false,
            strafe: false,
            attack: false,
            use_: false,

            klook: false,
            mlook: false,

            showscores: false,
            showteamscores: false,
        }
    }
}
