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

use std::cell::{Ref, RefCell};
use math;
use math::Vec3;

pub struct Entity {
    position: RefCell<Vec3>,
    velocity: RefCell<Vec3>,
    angle: RefCell<Vec3>,
}

impl Entity {
    pub fn new() -> Self {
        Entity {
            position: RefCell::new(Vec3::new(0.0, 0.0, 0.0)),
            velocity: RefCell::new(Vec3::new(0.0, 0.0, 0.0)),
            angle: RefCell::new(Vec3::new(0.0, 0.0, 0.0)),
        }
    }

    pub fn get_position(&self) -> Vec3 {
        (*self.position.borrow()).clone()
    }

    pub fn set_position(&self, x: f32, y: f32, z: f32) {
        let mut pos = self.position.borrow_mut();
        pos[0] = x;
        pos[1] = y;
        pos[2] = z;
    }

    pub fn adjust_position(&self, x: f32, y: f32, z: f32) {
        let mut pos = self.position.borrow_mut();
        pos[0] += x;
        pos[1] += y;
        pos[2] += z;
    }

    pub fn get_angle(&self) -> Vec3 {
        (*self.angle.borrow()).clone()
    }

    fn clamp_angle(&self) {
        let mut angle = self.angle.borrow_mut();
        if angle[0] < -math::PI / 2.0 {
            angle[0] = -math::PI / 2.0;
        } else if angle[0] > math::PI / 2.0 {
            angle[0] = math::PI / 2.0;
        }
    }

    pub fn set_angle(&self, x: f32, y: f32, z: f32) {
        {
            let mut angle = self.angle.borrow_mut();
            angle[0] = x;
            angle[1] = y;
            angle[2] = z;
        }
        self.clamp_angle();
    }

    pub fn adjust_angle(&self, x: f32, y: f32, z: f32) {
        {
            let mut angle = self.angle.borrow_mut();
            angle[0] += x;
            angle[1] += y;
            angle[2] += z;
        }
        self.clamp_angle();
    }
}
