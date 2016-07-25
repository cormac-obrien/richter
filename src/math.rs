use std;

pub use std::f32::consts::PI as PI;

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
    pub fn identity() -> Self {
        Mat4([[1.0, 0.0, 0.0, 0.0],
              [0.0, 1.0, 0.0, 0.0],
              [0.0, 0.0, 1.0, 0.0],
              [0.0, 0.0, 0.0, 1.0]])
    }

    pub fn rotation_x(theta: f32) -> Self {
        let s = theta.sin();
        let c = theta.cos();
        Mat4([[1.0, 0.0, 0.0, 0.0],
              [0.0,   c,   s, 0.0],
              [0.0,  -s,   c, 0.0],
              [0.0, 0.0, 0.0, 1.0]])
    }

    pub fn rotation_y(theta: f32) -> Self {
        let s = theta.sin();
        let c = theta.cos();
        Mat4([[  c, 0.0,   s, 0.0],
              [0.0, 1.0, 0.0, 0.0],
              [ -s, 0.0,   c, 0.0],
              [0.0, 0.0, 0.0, 1.0]])
    }

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
}
