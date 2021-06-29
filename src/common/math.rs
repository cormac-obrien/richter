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

use std::{cmp::Ordering, convert::Into, ops::Neg};

use cgmath::{Angle, Deg, InnerSpace, Matrix3, Matrix4, Vector2, Vector3, Zero};

trait CoordSys {}

struct Quake;
impl CoordSys for Quake {}

struct Wgpu;
impl CoordSys for Wgpu {}

pub const VERTEX_NORMAL_COUNT: usize = 162;
lazy_static! {
    /// Precomputed vertex normals used for alias models and particle effects
    pub static ref VERTEX_NORMALS: [Vector3<f32>; VERTEX_NORMAL_COUNT] = [
        [-0.525731, 0.000000, 0.850651].into(),
        [-0.442863, 0.238856, 0.864188].into(),
        [-0.295242, 0.000000, 0.955423].into(),
        [-0.309017, 0.500000, 0.809017].into(),
        [-0.162460, 0.262866, 0.951056].into(),
        [0.000000, 0.000000, 1.000000].into(),
        [0.000000, 0.850651, 0.525731].into(),
        [-0.147621, 0.716567, 0.681718].into(),
        [0.147621, 0.716567, 0.681718].into(),
        [0.000000, 0.525731, 0.850651].into(),
        [0.309017, 0.500000, 0.809017].into(),
        [0.525731, 0.000000, 0.850651].into(),
        [0.295242, 0.000000, 0.955423].into(),
        [0.442863, 0.238856, 0.864188].into(),
        [0.162460, 0.262866, 0.951056].into(),
        [-0.681718, 0.147621, 0.716567].into(),
        [-0.809017, 0.309017, 0.500000].into(),
        [-0.587785, 0.425325, 0.688191].into(),
        [-0.850651, 0.525731, 0.000000].into(),
        [-0.864188, 0.442863, 0.238856].into(),
        [-0.716567, 0.681718, 0.147621].into(),
        [-0.688191, 0.587785, 0.425325].into(),
        [-0.500000, 0.809017, 0.309017].into(),
        [-0.238856, 0.864188, 0.442863].into(),
        [-0.425325, 0.688191, 0.587785].into(),
        [-0.716567, 0.681718, -0.147621].into(),
        [-0.500000, 0.809017, -0.309017].into(),
        [-0.525731, 0.850651, 0.000000].into(),
        [0.000000, 0.850651, -0.525731].into(),
        [-0.238856, 0.864188, -0.442863].into(),
        [0.000000, 0.955423, -0.295242].into(),
        [-0.262866, 0.951056, -0.162460].into(),
        [0.000000, 1.000000, 0.000000].into(),
        [0.000000, 0.955423, 0.295242].into(),
        [-0.262866, 0.951056, 0.162460].into(),
        [0.238856, 0.864188, 0.442863].into(),
        [0.262866, 0.951056, 0.162460].into(),
        [0.500000, 0.809017, 0.309017].into(),
        [0.238856, 0.864188, -0.442863].into(),
        [0.262866, 0.951056, -0.162460].into(),
        [0.500000, 0.809017, -0.309017].into(),
        [0.850651, 0.525731, 0.000000].into(),
        [0.716567, 0.681718, 0.147621].into(),
        [0.716567, 0.681718, -0.147621].into(),
        [0.525731, 0.850651, 0.000000].into(),
        [0.425325, 0.688191, 0.587785].into(),
        [0.864188, 0.442863, 0.238856].into(),
        [0.688191, 0.587785, 0.425325].into(),
        [0.809017, 0.309017, 0.500000].into(),
        [0.681718, 0.147621, 0.716567].into(),
        [0.587785, 0.425325, 0.688191].into(),
        [0.955423, 0.295242, 0.000000].into(),
        [1.000000, 0.000000, 0.000000].into(),
        [0.951056, 0.162460, 0.262866].into(),
        [0.850651, -0.525731, 0.000000].into(),
        [0.955423, -0.295242, 0.000000].into(),
        [0.864188, -0.442863, 0.238856].into(),
        [0.951056, -0.162460, 0.262866].into(),
        [0.809017, -0.309017, 0.500000].into(),
        [0.681718, -0.147621, 0.716567].into(),
        [0.850651, 0.000000, 0.525731].into(),
        [0.864188, 0.442863, -0.238856].into(),
        [0.809017, 0.309017, -0.500000].into(),
        [0.951056, 0.162460, -0.262866].into(),
        [0.525731, 0.000000, -0.850651].into(),
        [0.681718, 0.147621, -0.716567].into(),
        [0.681718, -0.147621, -0.716567].into(),
        [0.850651, 0.000000, -0.525731].into(),
        [0.809017, -0.309017, -0.500000].into(),
        [0.864188, -0.442863, -0.238856].into(),
        [0.951056, -0.162460, -0.262866].into(),
        [0.147621, 0.716567, -0.681718].into(),
        [0.309017, 0.500000, -0.809017].into(),
        [0.425325, 0.688191, -0.587785].into(),
        [0.442863, 0.238856, -0.864188].into(),
        [0.587785, 0.425325, -0.688191].into(),
        [0.688191, 0.587785, -0.425325].into(),
        [-0.147621, 0.716567, -0.681718].into(),
        [-0.309017, 0.500000, -0.809017].into(),
        [0.000000, 0.525731, -0.850651].into(),
        [-0.525731, 0.000000, -0.850651].into(),
        [-0.442863, 0.238856, -0.864188].into(),
        [-0.295242, 0.000000, -0.955423].into(),
        [-0.162460, 0.262866, -0.951056].into(),
        [0.000000, 0.000000, -1.000000].into(),
        [0.295242, 0.000000, -0.955423].into(),
        [0.162460, 0.262866, -0.951056].into(),
        [-0.442863, -0.238856, -0.864188].into(),
        [-0.309017, -0.500000, -0.809017].into(),
        [-0.162460, -0.262866, -0.951056].into(),
        [0.000000, -0.850651, -0.525731].into(),
        [-0.147621, -0.716567, -0.681718].into(),
        [0.147621, -0.716567, -0.681718].into(),
        [0.000000, -0.525731, -0.850651].into(),
        [0.309017, -0.500000, -0.809017].into(),
        [0.442863, -0.238856, -0.864188].into(),
        [0.162460, -0.262866, -0.951056].into(),
        [0.238856, -0.864188, -0.442863].into(),
        [0.500000, -0.809017, -0.309017].into(),
        [0.425325, -0.688191, -0.587785].into(),
        [0.716567, -0.681718, -0.147621].into(),
        [0.688191, -0.587785, -0.425325].into(),
        [0.587785, -0.425325, -0.688191].into(),
        [0.000000, -0.955423, -0.295242].into(),
        [0.000000, -1.000000, 0.000000].into(),
        [0.262866, -0.951056, -0.162460].into(),
        [0.000000, -0.850651, 0.525731].into(),
        [0.000000, -0.955423, 0.295242].into(),
        [0.238856, -0.864188, 0.442863].into(),
        [0.262866, -0.951056, 0.162460].into(),
        [0.500000, -0.809017, 0.309017].into(),
        [0.716567, -0.681718, 0.147621].into(),
        [0.525731, -0.850651, 0.000000].into(),
        [-0.238856, -0.864188, -0.442863].into(),
        [-0.500000, -0.809017, -0.309017].into(),
        [-0.262866, -0.951056, -0.162460].into(),
        [-0.850651, -0.525731, 0.000000].into(),
        [-0.716567, -0.681718, -0.147621].into(),
        [-0.716567, -0.681718, 0.147621].into(),
        [-0.525731, -0.850651, 0.000000].into(),
        [-0.500000, -0.809017, 0.309017].into(),
        [-0.238856, -0.864188, 0.442863].into(),
        [-0.262866, -0.951056, 0.162460].into(),
        [-0.864188, -0.442863, 0.238856].into(),
        [-0.809017, -0.309017, 0.500000].into(),
        [-0.688191, -0.587785, 0.425325].into(),
        [-0.681718, -0.147621, 0.716567].into(),
        [-0.442863, -0.238856, 0.864188].into(),
        [-0.587785, -0.425325, 0.688191].into(),
        [-0.309017, -0.500000, 0.809017].into(),
        [-0.147621, -0.716567, 0.681718].into(),
        [-0.425325, -0.688191, 0.587785].into(),
        [-0.162460, -0.262866, 0.951056].into(),
        [0.442863, -0.238856, 0.864188].into(),
        [0.162460, -0.262866, 0.951056].into(),
        [0.309017, -0.500000, 0.809017].into(),
        [0.147621, -0.716567, 0.681718].into(),
        [0.000000, -0.525731, 0.850651].into(),
        [0.425325, -0.688191, 0.587785].into(),
        [0.587785, -0.425325, 0.688191].into(),
        [0.688191, -0.587785, 0.425325].into(),
        [-0.955423, 0.295242, 0.000000].into(),
        [-0.951056, 0.162460, 0.262866].into(),
        [-1.000000, 0.000000, 0.000000].into(),
        [-0.850651, 0.000000, 0.525731].into(),
        [-0.955423, -0.295242, 0.000000].into(),
        [-0.951056, -0.162460, 0.262866].into(),
        [-0.864188, 0.442863, -0.238856].into(),
        [-0.951056, 0.162460, -0.262866].into(),
        [-0.809017, 0.309017, -0.500000].into(),
        [-0.864188, -0.442863, -0.238856].into(),
        [-0.951056, -0.162460, -0.262866].into(),
        [-0.809017, -0.309017, -0.500000].into(),
        [-0.681718, 0.147621, -0.716567].into(),
        [-0.681718, -0.147621, -0.716567].into(),
        [-0.850651, 0.000000, -0.525731].into(),
        [-0.688191, 0.587785, -0.425325].into(),
        [-0.587785, 0.425325, -0.688191].into(),
        [-0.425325, 0.688191, -0.587785].into(),
        [-0.425325, -0.688191, -0.587785].into(),
        [-0.587785, -0.425325, -0.688191].into(),
        [-0.688191, -0.587785, -0.425325].into(),
    ];
}

#[derive(Clone, Copy, Debug)]
pub struct Angles {
    pub pitch: Deg<f32>,
    pub roll: Deg<f32>,
    pub yaw: Deg<f32>,
}

impl Angles {
    pub fn zero() -> Angles {
        Angles {
            pitch: Deg(0.0),
            roll: Deg(0.0),
            yaw: Deg(0.0),
        }
    }

    pub fn mat3_quake(&self) -> Matrix3<f32> {
        Matrix3::from_angle_x(-self.roll)
            * Matrix3::from_angle_y(-self.pitch)
            * Matrix3::from_angle_z(self.yaw)
    }

    pub fn mat4_quake(&self) -> Matrix4<f32> {
        Matrix4::from_angle_x(-self.roll)
            * Matrix4::from_angle_y(-self.pitch)
            * Matrix4::from_angle_z(self.yaw)
    }

    pub fn mat3_wgpu(&self) -> Matrix3<f32> {
        Matrix3::from_angle_z(-self.roll)
            * Matrix3::from_angle_x(self.pitch)
            * Matrix3::from_angle_y(-self.yaw)
    }

    pub fn mat4_wgpu(&self) -> Matrix4<f32> {
        Matrix4::from_angle_z(-self.roll)
            * Matrix4::from_angle_x(self.pitch)
            * Matrix4::from_angle_y(-self.yaw)
    }
}

impl std::ops::Add for Angles {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            pitch: self.pitch + other.pitch,
            roll: self.roll + other.roll,
            yaw: self.yaw + other.yaw,
        }
    }
}

impl std::ops::Mul<f32> for Angles {
    type Output = Self;

    fn mul(self, other: f32) -> Self {
        Self {
            pitch: self.pitch * other,
            roll: self.roll * other,
            yaw: self.yaw * other,
        }
    }
}

pub fn clamp_deg(val: Deg<f32>, min: Deg<f32>, max: Deg<f32>) -> Deg<f32> {
    assert!(min <= max);

    return if val < min {
        min
    } else if val > max {
        max
    } else {
        val
    };
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum HyperplaneSide {
    Positive = 0,
    Negative = 1,
}

impl Neg for HyperplaneSide {
    type Output = HyperplaneSide;

    fn neg(self) -> Self::Output {
        match self {
            HyperplaneSide::Positive => HyperplaneSide::Negative,
            HyperplaneSide::Negative => HyperplaneSide::Positive,
        }
    }
}

impl HyperplaneSide {
    // TODO: check this against the original game logic.
    pub fn from_dist(dist: f32) -> HyperplaneSide {
        if dist >= 0.0 {
            HyperplaneSide::Positive
        } else {
            HyperplaneSide::Negative
        }
    }
}

#[derive(Debug)]
/// The intersection of a line or segment and a plane at a point.
pub struct PointIntersection {
    // percentage of distance between start and end where crossover occurred
    ratio: f32,

    // crossover point
    point: Vector3<f32>,

    // plane crossed over
    plane: Hyperplane,
}

impl PointIntersection {
    pub fn ratio(&self) -> f32 {
        self.ratio
    }

    pub fn point(&self) -> Vector3<f32> {
        self.point
    }

    pub fn plane(&self) -> &Hyperplane {
        &self.plane
    }
}

#[derive(Debug)]
/// The intersection of a line or line segment with a plane.
///
/// A true mathematical representation would account for lines or segments contained entirely within
/// the plane, but here a distance of 0.0 is considered Positive. Thus, lines or segments contained
/// by the plane are considered to be `NoIntersection(Positive)`.
pub enum LinePlaneIntersect {
    /// The line or line segment never intersects with the plane.
    NoIntersection(HyperplaneSide),

    /// The line or line segment intersects with the plane at precisely one point.
    PointIntersection(PointIntersection),
}

#[derive(Copy, Clone, Debug, FromPrimitive)]
pub enum Axis {
    X = 0,
    Y = 1,
    Z = 2,
}

#[derive(Clone, Debug)]
enum Alignment {
    Axis(Axis),
    Normal(Vector3<f32>),
}

#[derive(Clone, Debug)]
pub struct Hyperplane {
    alignment: Alignment,
    dist: f32,
}

impl Neg for Hyperplane {
    type Output = Self;

    fn neg(self) -> Self::Output {
        let normal = match self.alignment {
            Alignment::Axis(a) => {
                let mut n = Vector3::zero();
                n[a as usize] = -1.0;
                n
            }
            Alignment::Normal(n) => -n,
        };

        Hyperplane::new(normal, -self.dist)
    }
}

impl Hyperplane {
    /// Creates a new hyperplane aligned along the given normal, `dist` units away from the origin.
    ///
    /// If the given normal is equivalent to one of the axis normals, the hyperplane will be optimized
    /// to only consider that axis when performing point comparisons.
    pub fn new(normal: Vector3<f32>, dist: f32) -> Hyperplane {
        match normal {
            n if n == Vector3::unit_x() => Self::axis_x(dist),
            n if n == Vector3::unit_y() => Self::axis_y(dist),
            n if n == Vector3::unit_z() => Self::axis_z(dist),
            _ => Self::from_normal(normal.normalize(), dist),
        }
    }

    /// Creates a new hyperplane aligned along the x-axis, `dist` units away from the origin.
    ///
    /// This hyperplane will only consider the x-axis when performing point comparisons.
    pub fn axis_x(dist: f32) -> Hyperplane {
        Hyperplane {
            alignment: Alignment::Axis(Axis::X),
            dist,
        }
    }

    /// Creates a new hyperplane aligned along the y-axis, `dist` units away from the origin.
    ///
    /// This hyperplane will only consider the y-axis when performing point comparisons.
    pub fn axis_y(dist: f32) -> Hyperplane {
        Hyperplane {
            alignment: Alignment::Axis(Axis::Y),
            dist,
        }
    }

    /// Creates a new hyperplane aligned along the z-axis, `dist` units away from the origin.
    ///
    /// This hyperplane will only consider the z-axis when performing point comparisons.
    pub fn axis_z(dist: f32) -> Hyperplane {
        Hyperplane {
            alignment: Alignment::Axis(Axis::Z),
            dist,
        }
    }

    /// Creates a new hyperplane aligned along the given normal, `dist` units away from the origin.
    ///
    /// This function will force the hyperplane alignment to be represented as a normal even if it
    /// is aligned along an axis.
    pub fn from_normal(normal: Vector3<f32>, dist: f32) -> Hyperplane {
        Hyperplane {
            alignment: Alignment::Normal(normal.normalize()),
            dist,
        }
    }

    /// Returns the surface normal of this plane.
    pub fn normal(&self) -> Vector3<f32> {
        match self.alignment {
            Alignment::Axis(ax) => match ax {
                Axis::X => Vector3::unit_x(),
                Axis::Y => Vector3::unit_y(),
                Axis::Z => Vector3::unit_z(),
            },
            Alignment::Normal(normal) => normal,
        }
    }

    /// Calculates the shortest distance between this hyperplane and the given point.
    pub fn point_dist(&self, point: Vector3<f32>) -> f32 {
        match self.alignment {
            Alignment::Axis(a) => point[a as usize] - self.dist,
            Alignment::Normal(n) => point.dot(n) - self.dist,
        }
    }

    /// Calculates which side of this hyperplane the given point belongs to.
    ///
    /// Points with a distance of 0.0 are considered to be on the positive side.
    pub fn point_side(&self, point: Vector3<f32>) -> HyperplaneSide {
        let point_dist_greater = match self.alignment {
            Alignment::Axis(a) => point[a as usize] >= self.dist,
            Alignment::Normal(n) => point.dot(n) - self.dist >= 0.0,
        };

        match point_dist_greater {
            true => HyperplaneSide::Positive,
            false => HyperplaneSide::Negative,
        }
    }

    /// Calculates the intersection of a line segment with this hyperplane.
    pub fn line_segment_intersection(
        &self,
        start: Vector3<f32>,
        end: Vector3<f32>,
    ) -> LinePlaneIntersect {
        let start_dist = self.point_dist(start);
        let end_dist = self.point_dist(end);

        debug!(
            "line_segment_intersection: alignment={:?} plane_dist={} start_dist={} end_dist={}",
            self.alignment, self.dist, start_dist, end_dist
        );

        let start_side = HyperplaneSide::from_dist(start_dist);
        let end_side = HyperplaneSide::from_dist(end_dist);

        // if both points fall on the same side of the hyperplane, there is no intersection
        if start_side == end_side {
            return LinePlaneIntersect::NoIntersection(start_side);
        }

        // calculate how far along the segment the intersection occurred
        let ratio = start_dist / (start_dist - end_dist);

        let point = start + ratio * (end - start);

        let plane = match start_side {
            HyperplaneSide::Positive => self.to_owned(),
            HyperplaneSide::Negative => -self.to_owned(),
        };

        LinePlaneIntersect::PointIntersection(PointIntersection {
            ratio,
            point,
            plane,
        })
    }
}

pub fn fov_x_to_fov_y(fov_x: Deg<f32>, aspect: f32) -> Option<Deg<f32>> {
    // aspect = tan(fov_x / 2) / tan(fov_y / 2)
    // tan(fov_y / 2) = tan(fov_x / 2) / aspect
    // fov_y / 2 = atan(tan(fov_x / 2) / aspect)
    // fov_y = 2 * atan(tan(fov_x / 2) / aspect)
    match fov_x {
        // TODO: genericize over cgmath::Angle
        f if f < Deg(0.0) => None,
        f if f > Deg(360.0) => None,
        f => Some(Deg::atan((f / 2.0).tan() / aspect) * 2.0),
    }
}

// see https://github.com/id-Software/Quake/blob/master/WinQuake/gl_rsurf.c#L1544
const COLLINEAR_EPSILON: f32 = 0.001;

/// Determines if the given points are collinear.
///
/// A set of points V is considered collinear if
/// norm(V<sub>1</sub> &minus; V<sub>0</sub>) &equals;
/// norm(V<sub>2</sub> &minus; V<sub>1</sub>) &equals;
/// .&nbsp;.&nbsp;. &equals;
/// norm(V<sub>k &minus; 1</sub> &minus; V<sub>k</sub>).
///
/// Special cases:
/// - If `vs.len() < 2`, always returns `false`.
/// - If `vs.len() == 2`, always returns `true`.
pub fn collinear(vs: &[Vector3<f32>]) -> bool {
    match vs.len() {
        l if l < 2 => false,
        2 => true,
        _ => {
            let init = (vs[1] - vs[0]).normalize();
            for i in 2..vs.len() {
                let norm = (vs[i] - vs[i - 1]).normalize();
                if (norm[0] - init[0]).abs() > COLLINEAR_EPSILON
                    || (norm[1] - init[1]).abs() > COLLINEAR_EPSILON
                    || (norm[2] - init[2]).abs() > COLLINEAR_EPSILON
                {
                    return false;
                }
            }

            true
        }
    }
}

pub fn remove_collinear(vs: Vec<Vector3<f32>>) -> Vec<Vector3<f32>> {
    assert!(vs.len() >= 3);

    let mut out = Vec::new();

    let mut vs_iter = vs.into_iter().cycle();
    let v_init = vs_iter.next().unwrap();
    let mut v1 = v_init;
    let mut v2 = vs_iter.next().unwrap();
    out.push(v1);
    for v3 in vs_iter {
        let tri = &[v1, v2, v3];

        if !collinear(tri) {
            out.push(v2);
        }

        if v3 == v_init {
            break;
        }

        v1 = v2;
        v2 = v3;
    }

    out
}

pub fn bounds<'a, I>(points: I) -> (Vector3<f32>, Vector3<f32>)
where
    I: IntoIterator<Item = &'a Vector3<f32>>,
{
    let mut min = Vector3::new(32767.0, 32767.0, 32767.0);
    let mut max = Vector3::new(-32768.0, -32768.0, -32768.0);
    for p in points.into_iter() {
        for c in 0..3 {
            min[c] = p[c].min(min[c]);
            max[c] = p[c].max(max[c]);
        }
    }
    (min, max)
}

pub fn vec2_extend_n(v: Vector2<f32>, n: usize, val: f32) -> Vector3<f32> {
    let mut ar = [0.0; 3];
    for i in 0..3 {
        match i.cmp(&n) {
            Ordering::Less => ar[i] = v[i],
            Ordering::Equal => ar[i] = val,
            Ordering::Greater => ar[i] = v[i - 1],
        }
    }

    ar.into()
}

pub fn vec3_truncate_n(v: Vector3<f32>, n: usize) -> Vector2<f32> {
    let mut ar = [0.0; 2];
    for i in 0..3 {
        match i.cmp(&n) {
            Ordering::Less => ar[i] = v[i],
            Ordering::Equal => (),
            Ordering::Greater => ar[i - 1] = v[i],
        }
    }
    ar.into()
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_hyperplane_side_x() {
        let plane = Hyperplane::axis_x(1.0);
        assert_eq!(
            plane.point_side(Vector3::unit_x() * 2.0),
            HyperplaneSide::Positive
        );
        assert_eq!(
            plane.point_side(Vector3::unit_x() * -2.0),
            HyperplaneSide::Negative
        );
    }

    #[test]
    fn test_hyperplane_side_y() {
        let plane = Hyperplane::axis_y(1.0);
        assert_eq!(
            plane.point_side(Vector3::unit_y() * 2.0),
            HyperplaneSide::Positive
        );
        assert_eq!(
            plane.point_side(Vector3::unit_y() * -2.0),
            HyperplaneSide::Negative
        );
    }

    #[test]
    fn test_hyperplane_side_z() {
        let plane = Hyperplane::axis_z(1.0);
        assert_eq!(
            plane.point_side(Vector3::unit_z() * 2.0),
            HyperplaneSide::Positive
        );
        assert_eq!(
            plane.point_side(Vector3::unit_z() * -2.0),
            HyperplaneSide::Negative
        );
    }

    #[test]
    fn test_hyperplane_side_arbitrary() {
        // test 16 hyperplanes around the origin
        for x_comp in [1.0, -1.0].into_iter() {
            for y_comp in [1.0, -1.0].into_iter() {
                for z_comp in [1.0, -1.0].into_iter() {
                    for dist in [1, -1].into_iter() {
                        let base_vector = Vector3::new(*x_comp, *y_comp, *z_comp);
                        let plane = Hyperplane::new(base_vector, *dist as f32);
                        assert_eq!(
                            plane.point_side(Vector3::zero()),
                            match *dist {
                                1 => HyperplaneSide::Negative,
                                -1 => HyperplaneSide::Positive,
                                _ => unreachable!(),
                            }
                        );
                        assert_eq!(
                            plane.point_side(base_vector * 2.0 * *dist as f32),
                            match *dist {
                                1 => HyperplaneSide::Positive,
                                -1 => HyperplaneSide::Negative,
                                _ => unreachable!(),
                            }
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn test_hyperplane_point_dist_x() {
        let plane = Hyperplane::axis_x(1.0);
        assert_eq!(plane.point_dist(Vector3::unit_x() * 2.0), 1.0);
        assert_eq!(plane.point_dist(Vector3::zero()), -1.0);
    }

    #[test]
    fn test_hyperplane_point_dist_y() {
        let plane = Hyperplane::axis_y(1.0);
        assert_eq!(plane.point_dist(Vector3::unit_y() * 2.0), 1.0);
        assert_eq!(plane.point_dist(Vector3::zero()), -1.0);
    }

    #[test]
    fn test_hyperplane_point_dist_z() {
        let plane = Hyperplane::axis_z(1.0);
        assert_eq!(plane.point_dist(Vector3::unit_z() * 2.0), 1.0);
        assert_eq!(plane.point_dist(Vector3::zero()), -1.0);
    }

    #[test]
    fn test_hyperplane_point_dist_x_no_axis() {
        let plane = Hyperplane::from_normal(Vector3::unit_x(), 1.0);
        assert_eq!(plane.point_dist(Vector3::unit_x() * 2.0), 1.0);
        assert_eq!(plane.point_dist(Vector3::zero()), -1.0);
    }

    #[test]
    fn test_hyperplane_point_dist_y_no_axis() {
        let plane = Hyperplane::from_normal(Vector3::unit_y(), 1.0);
        assert_eq!(plane.point_dist(Vector3::unit_y() * 2.0), 1.0);
        assert_eq!(plane.point_dist(Vector3::zero()), -1.0);
    }

    #[test]
    fn test_hyperplane_point_dist_z_no_axis() {
        let plane = Hyperplane::from_normal(Vector3::unit_z(), 1.0);
        assert_eq!(plane.point_dist(Vector3::unit_z() * 2.0), 1.0);
        assert_eq!(plane.point_dist(Vector3::zero()), -1.0);
    }

    #[test]
    fn test_hyperplane_line_segment_intersection_x() {
        let plane = Hyperplane::axis_x(1.0);
        let start = Vector3::new(0.0, 0.5, 0.5);
        let end = Vector3::new(2.0, 0.5, 0.5);

        match plane.line_segment_intersection(start, end) {
            LinePlaneIntersect::PointIntersection(p_i) => {
                assert_eq!(p_i.ratio(), 0.5);
                assert_eq!(p_i.point(), Vector3::new(1.0, 0.5, 0.5));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn test_hyperplane_line_segment_intersection_y() {
        let plane = Hyperplane::axis_y(1.0);
        let start = Vector3::new(0.5, 0.0, 0.5);
        let end = Vector3::new(0.5, 2.0, 0.5);

        match plane.line_segment_intersection(start, end) {
            LinePlaneIntersect::PointIntersection(p_i) => {
                assert_eq!(p_i.ratio(), 0.5);
                assert_eq!(p_i.point(), Vector3::new(0.5, 1.0, 0.5));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn test_hyperplane_line_segment_intersection_z() {
        let plane = Hyperplane::axis_z(1.0);
        let start = Vector3::new(0.5, 0.5, 0.0);
        let end = Vector3::new(0.5, 0.5, 2.0);

        match plane.line_segment_intersection(start, end) {
            LinePlaneIntersect::PointIntersection(p_i) => {
                assert_eq!(p_i.ratio(), 0.5);
                assert_eq!(p_i.point(), Vector3::new(0.5, 0.5, 1.0));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn test_collinear() {
        let cases = vec![
            (
                vec![Vector3::unit_x(), Vector3::unit_y(), Vector3::unit_z()],
                false,
            ),
            (
                vec![
                    Vector3::unit_x(),
                    Vector3::unit_x() * 2.0,
                    Vector3::unit_x() * 3.0,
                ],
                true,
            ),
            (
                vec![
                    [1400.0, 848.0, -456.0].into(),
                    [1352.0, 848.0, -456.0].into(),
                    [1272.0, 848.0, -456.0].into(),
                    [1256.0, 848.0, -456.0].into(),
                    [1208.0, 848.0, -456.0].into(),
                    [1192.0, 848.0, -456.0].into(),
                    [1176.0, 848.0, -456.0].into(),
                ],
                true,
            ),
        ];

        for (input, result) in cases.into_iter() {
            assert_eq!(collinear(&input), result);
        }
    }

    #[test]
    fn test_remove_collinear() {
        let cases = vec![
            (
                vec![
                    [1176.0, 992.0, -456.0].into(),
                    [1176.0, 928.0, -456.0].into(),
                    [1176.0, 880.0, -456.0].into(),
                    [1176.0, 864.0, -456.0].into(),
                    [1176.0, 848.0, -456.0].into(),
                    [1120.0, 848.0, -456.0].into(),
                    [1120.0, 992.0, -456.0].into(),
                ],
                vec![
                    [1176.0, 992.0, -456.0].into(),
                    [1176.0, 848.0, -456.0].into(),
                    [1120.0, 848.0, -456.0].into(),
                    [1120.0, 992.0, -456.0].into(),
                ],
            ),
            (
                vec![
                    [1400.0, 768.0, -456.0].into(),
                    [1400.0, 848.0, -456.0].into(),
                    [1352.0, 848.0, -456.0].into(),
                    [1272.0, 848.0, -456.0].into(),
                    [1256.0, 848.0, -456.0].into(),
                    [1208.0, 848.0, -456.0].into(),
                    [1192.0, 848.0, -456.0].into(),
                    [1176.0, 848.0, -456.0].into(),
                    [1120.0, 848.0, -456.0].into(),
                    [1200.0, 768.0, -456.0].into(),
                ],
                vec![
                    [1400.0, 768.0, -456.0].into(),
                    [1400.0, 848.0, -456.0].into(),
                    [1120.0, 848.0, -456.0].into(),
                    [1200.0, 768.0, -456.0].into(),
                ],
            ),
        ];

        for (input, output) in cases.into_iter() {
            assert_eq!(remove_collinear(input), output);
        }
    }
}
