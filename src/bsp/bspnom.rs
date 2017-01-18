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

use gfx::Vertex;
use math::Vec3;
use nom::{le_i32, le_f32};
use num::FromPrimitive;
use std::collections::HashMap;

pub fn load_bsp(bsp_data: &[u8]) {
    let bsp_data = bsp_data;
    let (_, bsp_header) = parse_bsp_header(bsp_data).unwrap();
    let (_, entities) = parse_entities(&bsp_data[bsp_header.entities.offset as usize..]).unwrap();
}

#[derive(Debug)]
struct BspHeader {
    version: i32,
    entities: BspHeaderEntry,
    planes: BspHeaderEntry,
    textures: BspHeaderEntry,
    vertices: BspHeaderEntry,
    visdata: BspHeaderEntry,
    nodes: BspHeaderEntry,
    texinfo: BspHeaderEntry,
    faces: BspHeaderEntry,
    lightmaps: BspHeaderEntry,
    clipnodes: BspHeaderEntry,
    leaves: BspHeaderEntry,
    facelist: BspHeaderEntry,
    edges: BspHeaderEntry,
    edgelist: BspHeaderEntry,
    models: BspHeaderEntry,
}

/// One of the hyperplanes partitioning the map.
struct Plane {
    normal: Vec3,
    distance: f32,
    kind: PlaneKind,
}

#[derive(FromPrimitive)]
enum PlaneKind {
    X = 0,
    Y = 1,
    Z = 2,
    AnyX = 3,
    AnyY = 4,
    AnyZ = 5,
}

named!(parse_bsp_header(&[u8]) -> BspHeader,
    do_parse!(
        version:   le_i32                 >>
        entities:  parse_bsp_header_entry >>
        planes:    parse_bsp_header_entry >>
        textures:  parse_bsp_header_entry >>
        vertices:  parse_bsp_header_entry >>
        visdata:   parse_bsp_header_entry >>
        nodes:     parse_bsp_header_entry >>
        texinfo:   parse_bsp_header_entry >>
        faces:     parse_bsp_header_entry >>
        lightmaps: parse_bsp_header_entry >>
        clipnodes: parse_bsp_header_entry >>
        leaves:    parse_bsp_header_entry >>
        facelist:  parse_bsp_header_entry >>
        edges:     parse_bsp_header_entry >>
        edgelist:  parse_bsp_header_entry >>
        models:    parse_bsp_header_entry >>
        (BspHeader {
            version: version,
            entities: entities,
            planes: planes,
            textures: textures,
            vertices: vertices,
            visdata: visdata,
            nodes: nodes,
            texinfo: texinfo,
            faces: faces,
            lightmaps: lightmaps,
            clipnodes: clipnodes,
            leaves: leaves,
            facelist: facelist,
            edges: edges,
            edgelist: edgelist,
            models: models,
        })
    )
);

#[derive(Debug)]
struct BspHeaderEntry {
    offset: i32,
    size: i32,
}

named!(parse_bsp_header_entry(&[u8]) -> BspHeaderEntry,
    do_parse!(
        offset: le_i32 >>
        size: le_i32 >>
        (BspHeaderEntry {
            offset: offset,
            size: size }
        )
    )
);

named!(parse_entities(&[u8]) -> Vec<HashMap<String, String>>,
       fold_many0!(
           parse_entity,
           Vec::new(),
           |mut vec: Vec<_>, item: HashMap<_, _>| {
               vec.push(item);
               vec
           }
       )
);

named!(parse_entity(&[u8]) -> HashMap<String, String>,
       delimited!(
           tag!("{\n"),
           fold_many0!(
               parse_key_value_pair,
               HashMap::new(),
               |mut map: HashMap<_, _>, item: (String, String)| {
                   map.insert(item.0, item.1);
                   map
               }),
           tag!("}\n")
       )
);

named!(parse_key_value_pair(&[u8]) -> (String, String),
       do_parse!(
           key: parse_quoted >>
               tag!(" ") >>
               val: parse_quoted >>
               tag!("\n") >>
               (
                   (String::from_utf8(Vec::from(key)).unwrap(), String::from_utf8(Vec::from(val)).unwrap())
               )
       )
);

named!(parse_quoted(&[u8]) -> &[u8],
       delimited!(tag!("\""), take_until!("\""), tag!("\""))
);

named!(parse_plane(&[u8]) -> Plane,
    do_parse!(
        normal:   parse_vec3 >>
        distance: le_f32     >>
        kind:     le_i32     >>
        (Plane {
            normal: normal,
            distance: distance,
            kind: PlaneKind::from_i32(kind).unwrap(),
        })
    )
);

named!(parse_vertex(&[u8]) -> Vertex,
    do_parse!(
        pos: count_fixed!(f32, le_f32, 3) >>
        (Vertex::new(pos))
    )
);

named!(parse_vec3(&[u8]) -> Vec3,
    do_parse!(
        x: le_f32 >>
        y: le_f32 >>
        z: le_f32 >>
        (Vec3::new(x, y, z))
    )
);

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_quoted() {
        let (_, inner) = parse_quoted("\"test\"".as_bytes()).unwrap();
        assert_eq!(inner, "test".as_bytes());
    }

    #[test]
    fn test_parse_key_value_pair() {
        let (_, (key, val)) = parse_key_value_pair("\"key\" \"val\"\n".as_bytes()).unwrap();
        assert_eq!(key, "key");
        assert_eq!(val, "val");
    }

    #[test]
    fn test_parse_entity() {
        let (_, entity) = parse_entity("{\n\"key\" \"val\"\n\"key2\" \"val2\"\n}\n".as_bytes())
                              .unwrap();
        assert_eq!(entity.get("key").unwrap(), "val");
        assert_eq!(entity.get("key2").unwrap(), "val2");
    }

    #[test]
    fn test_load_bsp() {
        let bsp_data = include_bytes!("../pak0/maps/e1m1.bsp");
        let _ = load_bsp(bsp_data);
    }
}
