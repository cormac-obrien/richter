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

pub mod console;
pub mod map;

use cgmath::Vector3;
use nom::{
    branch::alt,
    bytes::complete::{tag, take_while1},
    character::complete::{alphanumeric1, one_of, space1},
    combinator::map,
    sequence::{delimited, tuple},
};
use winit::event::ElementState;

pub use self::{console::commands, map::entities};

pub fn non_newline_spaces(input: &str) -> nom::IResult<&str, &str> {
    space1(input)
}

fn string_contents(input: &str) -> nom::IResult<&str, &str> {
    take_while1(|c: char| !"\"".contains(c) && c.is_ascii() && !c.is_ascii_control())(input)
}

pub fn quoted(input: &str) -> nom::IResult<&str, &str> {
    delimited(tag("\""), string_contents, tag("\""))(input)
}

pub fn action(input: &str) -> nom::IResult<&str, (ElementState, &str)> {
    tuple((
        map(one_of("+-"), |c| match c {
            '+' => ElementState::Pressed,
            '-' => ElementState::Released,
            _ => unreachable!(),
        }),
        alphanumeric1,
    ))(input)
}

pub fn newline(input: &str) -> nom::IResult<&str, &str> {
    nom::character::complete::line_ending(input)
}

// TODO: rename to line_terminator and move to console module
pub fn line_ending(input: &str) -> nom::IResult<&str, &str> {
    alt((tag(";"), nom::character::complete::line_ending))(input)
}

pub fn vector3_components<S>(src: S) -> Option<[f32; 3]>
where
    S: AsRef<str>,
{
    let src = src.as_ref();

    let components: Vec<_> = src.split(' ').collect();
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

    let components: Vec<_> = src.split(' ').collect();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quoted() {
        let s = "\"hello\"";
        assert_eq!(quoted(s), Ok(("", "hello")))
    }

    #[test]
    fn test_action() {
        let s = "+up";
        assert_eq!(action(s), Ok(("", (ElementState::Pressed, "up"))))
    }
}
