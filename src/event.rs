// Copyright © 2016 Cormac O'Brien
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

use std::collections::HashSet;
use glium::glutin::{ElementState, VirtualKeyCode};

pub struct KeyState {
    key_states: HashSet<VirtualKeyCode>,
}

impl KeyState {
    pub fn new() -> Self {
        KeyState {
            key_states: HashSet::new(),
        }
    }

    pub fn is_pressed(&self, key: VirtualKeyCode) -> bool {
        self.key_states.contains(&key)
    }

    pub fn update(&mut self, key_code: VirtualKeyCode, key_state: ElementState) {
        match key_state {
            ElementState::Pressed => self.key_states.insert(key_code),
            ElementState::Released => self.key_states.remove(&key_code),
        };
    }
}
