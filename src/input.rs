// Copyright © 2017 Cormac O'Brien
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

use std::cell::Cell;

pub struct InputState {
    pub forward: Cell<bool>,
    pub back: Cell<bool>,
    pub moveleft: Cell<bool>,
    pub moveright: Cell<bool>,

    pub moveup: Cell<bool>,
    pub movedown: Cell<bool>,

    pub left: Cell<bool>,
    pub right: Cell<bool>,
    pub lookup: Cell<bool>,
    pub lookdown: Cell<bool>,

    pub speed: Cell<bool>,
    pub jump: Cell<bool>,
    pub strafe: Cell<bool>,
    pub attack: Cell<bool>,
    pub use_: Cell<bool>,

    pub klook: Cell<bool>,
    pub mlook: Cell<bool>,

    pub showscores: Cell<bool>,
    pub showteamscores: Cell<bool>,
}

impl InputState {
    pub fn new() -> Self {
        InputState {
            forward: Cell::new(false),
            back: Cell::new(false),
            moveleft: Cell::new(false),
            moveright: Cell::new(false),

            moveup: Cell::new(false),
            movedown: Cell::new(false),

            left: Cell::new(false),
            right: Cell::new(false),
            lookup: Cell::new(false),
            lookdown: Cell::new(false),

            speed: Cell::new(false),
            jump: Cell::new(false),
            strafe: Cell::new(false),
            attack: Cell::new(false),
            use_: Cell::new(false),

            klook: Cell::new(false),
            mlook: Cell::new(false),

            showscores: Cell::new(false),
            showteamscores: Cell::new(false),
        }
    }
}
