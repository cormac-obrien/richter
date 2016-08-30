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

use std;

pub use std::f32::consts::PI as PI;

/// A 4x4 matrix.
pub struct Mat4(pub [[f32; 4]; 4]);

impl std::ops::Deref for Mat4 {
    type Target = [[f32; 4]; 4];

    fn deref(&self) -> &[[f32; 4]; 4] {
        &self.0
    }
}

impl std::ops::Mul for Mat4 {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self {
        let mut result = [[0.0; 4]; 4];
        for i in 0..4 {
            for j in 0..4 {
                for k in 0..4 {
                    result[i][j] += self[k][j] * rhs[i][k];
                }
            }
        }
        Mat4(result)
    }
}

impl Mat4 {
    /// Returns a 4x4 identity matrix.
    pub fn identity() -> Self {
        Mat4([[1.0, 0.0, 0.0, 0.0],
              [0.0, 1.0, 0.0, 0.0],
              [0.0, 0.0, 1.0, 0.0],
              [0.0, 0.0, 0.0, 1.0]])
    }

    /// Performs a rotation about the x-axis.
    pub fn rotation_x(theta: f32) -> Self {
        let s = theta.sin();
        let c = theta.cos();
        Mat4([[1.0, 0.0, 0.0, 0.0],
              [0.0,   c,   s, 0.0],
              [0.0,  -s,   c, 0.0],
              [0.0, 0.0, 0.0, 1.0]])
    }

    /// Performs a rotation about the y-axis.
    pub fn rotation_y(theta: f32) -> Self {
        let s = theta.sin();
        let c = theta.cos();
        Mat4([[  c, 0.0,   s, 0.0],
              [0.0, 1.0, 0.0, 0.0],
              [ -s, 0.0,   c, 0.0],
              [0.0, 0.0, 0.0, 1.0]])
    }

    /// Performs a rotation about the z-axis.
    pub fn rotation_z(theta: f32) -> Self {
        let s = theta.sin();
        let c = theta.cos();
        Mat4([[  c,   s, 0.0, 0.0],
              [ -s,   c, 0.0, 0.0],
              [0.0, 0.0, 1.0, 0.0],
              [0.0, 0.0, 0.0, 1.0]])
    }

    pub fn translation(x: f32, y: f32, z: f32) -> Self {
        Mat4([[1.0, 0.0, 0.0, 0.0],
              [0.0, 1.0, 0.0, 0.0],
              [0.0, 0.0, 1.0, 0.0],
              [  x,   y,   z, 1.0]])
    }

    pub fn perspective(w: f32, h: f32, fov: f32) -> Self {
        let aspect = w / h;
        let znear = 0.125;
        let zfar = 4096.0;
        let f = 1.0 / (fov / 2.0).tan();

        Mat4([[f / aspect, 0.0,                                   0.0,  0.0],
              [       0.0,   f,                                   0.0,  0.0],
              [       0.0, 0.0,       (zfar + znear) / (zfar - znear), -1.0],
              [       0.0, 0.0, (2.0 * zfar * znear) / (zfar - znear),  0.0]])
    }
}

/// A 3-component vector.
#[derive(Copy, Clone)]
pub struct Vec3([f32; 3]);

impl Vec3 {
    /// Constructs a new Vec3 from its components.
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Vec3([x, y, z])
    }

    /// Constructs a new Vec3 from an array of components.
    pub fn from_components(components: [f32; 3]) -> Self {
        Vec3(components)
    }

    // Constructs a new Vec3 by rotating `self` about the x-axis by `theta` radians
    pub fn rotate_x(&self, theta: f32) -> Self {
        Vec3([self[0],
              self[1] * theta.cos() - self[2] * theta.sin(),
              self[1] * theta.sin() + self[2] * theta.cos()])
    }

    pub fn rotate_y(&self, theta: f32) -> Self {
        Vec3([self[0] * theta.cos() + self[2] * theta.sin(),
              self[1],
             -self[0] * theta.sin() + self[2] * theta.cos()])
    }

    pub fn rotate_z(&self, theta: f32) -> Self {
        Vec3([self[0] * theta.cos() - self[1] * theta.sin(),
              self[0] * theta.sin() + self[1] * theta.cos(),
              self[2]])
    }

    /// Calculates the dot product of this Vec3 and another.
    pub fn dot(&self, other: Vec3) -> f32 {
        self[0] * other[0] + self[1] * other[1] + self[2] * other[2]
    }
}

impl std::fmt::Display for Vec3 {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{{{}, {}, {}}}", self[0], self[1], self[2])
    }
}

// Vec3 Dereferencing

impl std::ops::Deref for Vec3 {
    type Target = [f32; 3];

    fn deref(&self) -> &[f32; 3] {
        &self.0
    }
}

// Vec3 Index Operations

impl std::ops::Index<usize> for Vec3 {
    type Output = f32;

    fn index(&self, i: usize) -> &f32 {
        &self.0[i]
    }
}

impl std::ops::IndexMut<usize> for Vec3 {
    fn index_mut<'a>(&'a mut self, i: usize) -> &'a mut f32 {
        &mut self.0[i]
    }
}

// Vec3 Conversion Traits

impl std::convert::From<[f32; 3]> for Vec3 {
    fn from(__arg_0: [f32; 3]) -> Self {
        Vec3(__arg_0)
    }
}

// Vec3 Arithmetic Operations

impl std::ops::Mul<f32> for Vec3 {
    type Output = Self;

    fn mul(self, scalar: f32) -> Vec3 {
        Vec3([self[0] * scalar, self[1] * scalar, self[2] * scalar])
    }
}

impl<'a> std::ops::Mul<f32> for &'a Vec3 {
    type Output = Vec3;

    fn mul(self, scalar: f32) -> Vec3 {
        Vec3([self[0] * scalar, self[1] * scalar, self[2] * scalar])
    }
}

impl<'a> std::ops::Mul<&'a Vec3> for f32 {
    type Output = Vec3;

    fn mul(self, vec: &'a Vec3) -> Vec3 {
        vec * self
    }
}

impl<'a> std::ops::Add<&'a Vec3> for Vec3 {
    type Output = Vec3;

    fn add(self, other: &'a Vec3) -> Vec3 {
        Vec3([self[0] + other[0], self[1] + other[1], self[2] + other[2]])
    }
}

impl<'a, 'b> std::ops::Add<&'a Vec3> for &'b Vec3 {
    type Output = Vec3;

    fn add(self, other: &'a Vec3) -> Vec3 {
        Vec3([self[0] + other[0], self[1] + other[1], self[2] + other[2]])
    }
}

impl<'a> std::ops::Sub<&'a Vec3> for Vec3 {
    type Output = Vec3;

    fn sub(self, other: &'a Vec3) -> Vec3 {
        Vec3([self[0] - other[0], self[1] - other[1], self[2] - other[2]])
    }
}

impl<'a, 'b> std::ops::Sub<&'a Vec3> for &'b Vec3 {
    type Output = Vec3;

    fn sub(self, other: &'a Vec3) -> Vec3 {
        Vec3([self[0] - other[0], self[1] - other[1], self[2] - other[2]])
    }
}

impl Vec3 {
    /// Calculates the dot product of this Vec3 and another.
    pub fn dot<V>(&self, other: V) -> f32 where V: AsRef<[f32; 3]> {
        let o = other.as_ref();
        self[0] * o[0] + self[1] * o[1] + self[2] * o[2]
    }
}

pub struct Radians(pub f32);

impl std::convert::From<Degrees> for Radians {
    fn from(__arg_0: Degrees) -> Self {
        Radians(__arg_0.0.to_radians())
    }
}

pub struct Degrees(pub f32);

impl std::convert::From<Radians> for Degrees {
    fn from(__arg_0: Radians) -> Self {
        Degrees(__arg_0.0.to_degrees())
    }
}

#[cfg(test)]
mod test {
    use math::{Degrees, PI, Radians};

    #[test]
    fn test_deg2rad() {
        let d = Degrees(180.0);
        match Radians::from(d).0 {
            r if r > PI - 0.1 && r < PI + 0.1 => (),
            r => panic!("Got {}, expected {}", r, PI),
        }
    }

    #[test]
    fn test_rad2deg() {
        let r = Radians(PI);
        match Degrees::from(r).0 {
            d if d > 179.9 && d < 180.1 => (),
            d => panic!("Got {}, expected {}", d, 180.0),
        }
    }
}
