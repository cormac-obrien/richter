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

use std::collections::HashMap;

use cgmath::Vector3;

// Parse quoted strings
named!(
    quoted<&str>,
    map_res!(
        delimited!(tag!("\""), take_until_s!("\""), tag!("\"")),
        ::std::str::from_utf8
    )
);

// Parse a pair of quoted strings separated by a space and followed by a newline
named!(
    key_val<(&str, &str)>,
    terminated!(separated_pair!(quoted, tag!(" "), quoted), tag!("\n"))
);

named!(
    entity_map<HashMap<&str, &str>>,
    map!(
        delimited!(tag!("{\n"), many0!(key_val), tag!("}\n")),
        |tuples| {
            let mut map = HashMap::new();
            for (k, v) in tuples {
                map.insert(k, v);
            }
            map
        }
    )
);

named!(
    pub entity_maps <Vec<HashMap<&str, &str>>>,
    many0!(entity_map)
);

pub fn vector3_components<S>(src: S) -> Option<[f32; 3]>
where
    S: AsRef<str>,
{
    let src = src.as_ref();

    let components: Vec<_> = src.split(" ").collect();
    if components.len() != 3 {
        return None;
    }

    let x: f32 = match components[0].parse().ok() {
        Some(p) => p,
        None => return None,
    };

    let y: f32 = match components[1].parse().ok() {
        Some(p) => p,
        None => return None,
    };

    let z: f32 = match components[2].parse().ok() {
        Some(p) => p,
        None => return None,
    };

    Some([x, y, z])
}

pub fn vector3<S>(src: S) -> Option<Vector3<f32>>
where
    S: AsRef<str>,
{
    let src = src.as_ref();

    let components: Vec<_> = src.split(" ").collect();
    if components.len() != 3 {
        return None;
    }

    let x: f32 = match components[0].parse().ok() {
        Some(p) => p,
        None => return None,
    };

    let y: f32 = match components[1].parse().ok() {
        Some(p) => p,
        None => return None,
    };

    let z: f32 = match components[2].parse().ok() {
        Some(p) => p,
        None => return None,
    };

    Some(Vector3::new(x, y, z))
}
