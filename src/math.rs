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

use std::ops::Neg;

use cgmath::InnerSpace;
use cgmath::Vector3;

#[derive(Debug, PartialEq)]
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
    // TODO: check this against the original game logic. Note that the current implementation will
    // treat both 0.0 and -0.0 as Positive.
    pub fn from_dist(dist: f32) -> HyperplaneSide {
        if dist >= 0.0 {
            HyperplaneSide::Positive
        } else {
            HyperplaneSide::Negative
        }
    }
}

#[derive(Debug)]
/// The intersection of a line or line segment with a plane.
pub enum LinePlaneIntersect {
    /// The line or line segment never intersects with the plane.
    NoIntersection(HyperplaneSide),

    /// The line or line segment intersects with the plane at precisely one point.
    PointIntersection(Vector3<f32>),

    /// The line or line segment is entirely contained within the plane.
    FullIntersection,
}

#[derive(Copy, Clone, Debug, FromPrimitive)]
pub enum Axis {
    X = 0,
    Y = 1,
    Z = 2,
}

#[derive(Debug)]
enum Alignment {
    Axis(Axis),
    Normal(Vector3<f32>),
}

#[derive(Debug)]
pub struct Hyperplane {
    alignment: Alignment,
    dist: f32,
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
            _ => Self::normal(normal, dist),
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
    pub fn normal(normal: Vector3<f32>, dist: f32) -> Hyperplane {
        Hyperplane {
            alignment: Alignment::Normal(normal),
            dist,
        }
    }

    /// Determines whether this hyperplane contains the given point.
    pub fn contains(&self, point: Vector3<f32>) -> bool {
        match self.alignment {
            Alignment::Axis(a) => point[a as usize] == self.dist,
            Alignment::Normal(n) => point.dot(n) == self.dist,
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
    pub fn point_side(&self, point: Vector3<f32>) -> HyperplaneSide {
        let point_dist_greater = match self.alignment {
            Alignment::Axis(a) => point[a as usize] > self.dist,
            Alignment::Normal(n) => point.dot(n) - self.dist > 0.0,
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

        // if both distances are zero, the segment is entirely contained by this hyperplane
        if start_dist == 0.0 && end_dist == 0.0 {
            return LinePlaneIntersect::FullIntersection;
        }

        let start_side = HyperplaneSide::from_dist(start_dist);
        let end_side = HyperplaneSide::from_dist(end_dist);

        // if both points fall on the same side of the hyperplane, there is no intersection
        if start_side == end_side {
            return LinePlaneIntersect::NoIntersection(start_side);
        }

        // calculate how far along the segment the intersection occurred
        let ratio = start_dist / (start_dist - end_dist);

        let mid = start + ratio * (end - start);

        LinePlaneIntersect::PointIntersection(mid)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_plane_point_dist_x() {
        let plane = Hyperplane::axis_x(1.0);
        assert_eq!(plane.point_dist(Vector3::unit_x() * 2.0), 1.0);
    }

    #[test]
    fn test_plane_point_dist_y() {
        let plane = Hyperplane::axis_y(1.0);
        assert_eq!(plane.point_dist(Vector3::unit_y() * 2.0), 1.0);
    }

    #[test]
    fn test_plane_point_dist_z() {
        let plane = Hyperplane::axis_z(1.0);
        assert_eq!(plane.point_dist(Vector3::unit_z() * 2.0), 1.0)
    }

    #[test]
    fn test_plane_point_dist_x_no_axis() {
        let plane = Hyperplane::normal(Vector3::unit_x(), 1.0);
        assert_eq!(plane.point_dist(Vector3::unit_x() * 2.0), 1.0);
    }

    #[test]
    fn test_plane_point_dist_y_no_axis() {
        let plane = Hyperplane::normal(Vector3::unit_y(), 1.0);
        assert_eq!(plane.point_dist(Vector3::unit_y() * 2.0), 1.0);
    }

    #[test]
    fn test_plane_point_dist_z_no_axis() {
        let plane = Hyperplane::normal(Vector3::unit_z(), 1.0);
        assert_eq!(plane.point_dist(Vector3::unit_z() * 2.0), 1.0)
    }
}
