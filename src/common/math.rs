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

use std::ops::Neg;

use cgmath::InnerSpace;
use cgmath::Vector3;
use cgmath::Zero;

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
            _ => Self::normal(normal.normalize(), dist),
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
            alignment: Alignment::Normal(normal.normalize()),
            dist,
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
        let plane = Hyperplane::normal(Vector3::unit_x(), 1.0);
        assert_eq!(plane.point_dist(Vector3::unit_x() * 2.0), 1.0);
        assert_eq!(plane.point_dist(Vector3::zero()), -1.0);
    }

    #[test]
    fn test_hyperplane_point_dist_y_no_axis() {
        let plane = Hyperplane::normal(Vector3::unit_y(), 1.0);
        assert_eq!(plane.point_dist(Vector3::unit_y() * 2.0), 1.0);
        assert_eq!(plane.point_dist(Vector3::zero()), -1.0);
    }

    #[test]
    fn test_hyperplane_point_dist_z_no_axis() {
        let plane = Hyperplane::normal(Vector3::unit_z(), 1.0);
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
}
