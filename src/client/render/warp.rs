use std::cmp::Ordering;

use crate::common::math;

use cgmath::Vector3;

// TODO: make this a cvar
const SUBDIVIDE_SIZE: f32 = 32.0;

/// Subdivide the given polygon on a grid.
///
/// The algorithm is described as follows:
/// Given a polygon *P*,
/// 1. Calculate the extents *P*min, *P*max and the midpoint *P*mid of *P*.
/// 1. Calculate the distance vector *D*<sub>i</sub> for each *P*<sub>i</sub>.
/// 1. For each axis *A* = [X, Y, Z]:
///    1. If the distance between either *P*min<sub>A</sub> or
///       *P*max<sub>A</sub> and *P*mid<sub>A</sub> is less than 8, continue to
///       the next axis.
///    1. For each vertex *v*...
/// TODO...
pub fn subdivide(verts: Vec<Vector3<f32>>) -> Vec<Vector3<f32>> {
    let mut out = Vec::new();
    subdivide_impl(verts, &mut out);
    out
}

fn subdivide_impl(mut verts: Vec<Vector3<f32>>, output: &mut Vec<Vector3<f32>>) {
    let (min, max) = math::bounds(&verts);

    let mut front = Vec::new();
    let mut back = Vec::new();

    // subdivide polygon along each axis in order
    for ax in 0..3 {
        // find the midpoint of the polygon bounds
        let mid = {
            let m = (min[ax] + max[ax]) / 2.0;
            SUBDIVIDE_SIZE * (m / SUBDIVIDE_SIZE).round()
        };

        if max[ax] - mid < 8.0 || mid - min[ax] < 8.0 {
            // this component doesn't need to be subdivided further.
            // if no components need to be subdivided further, this breaks the loop.
            continue;
        }

        // collect the distances of each vertex from the midpoint
        let mut dist: Vec<f32> = verts.iter().map(|v| (*v)[ax] - mid).collect();
        dist.push(dist[0]);

        // duplicate first vertex
        verts.push(verts[0]);
        for (vi, v) in (&verts[..verts.len() - 1]).iter().enumerate() {
            // sort vertices to front and back of axis
            let cmp = dist[vi].partial_cmp(&0.0).unwrap();
            match cmp {
                Ordering::Less => {
                    back.push(*v);
                }
                Ordering::Equal => {
                    // if this vertex is on the axis, split it in two
                    front.push(*v);
                    back.push(*v);
                    continue;
                }
                Ordering::Greater => {
                    front.push(*v);
                }
            }

            if dist[vi + 1] != 0.0 && cmp != dist[vi + 1].partial_cmp(&0.0).unwrap() {
                // segment crosses the axis, add a vertex at the intercept
                let ratio = dist[vi] / (dist[vi] - dist[vi + 1]);
                let intercept = v + ratio * (verts[vi + 1] - v);
                front.push(intercept);
                back.push(intercept);
            }
        }

        subdivide_impl(front, output);
        subdivide_impl(back, output);
        return;
    }

    // polygon is smaller than SUBDIVIDE_SIZE along all three axes
    assert!(verts.len() >= 3);
    let v1 = verts[0];
    let mut v2 = verts[1];
    for v3 in &verts[2..] {
        output.push(v1);
        output.push(v2);
        output.push(*v3);
        v2 = *v3;
    }
}
