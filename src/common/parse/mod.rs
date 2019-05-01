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
use combine::char::{alpha_num, string};
use combine::{
    between, choice, many, one_of, satisfy, skip_many, token, unexpected, value, ParseError,
    Parser, Stream,
};
use winit::ElementState;

pub use self::console::commands;
pub use self::map::entities;

fn is_escape(c: char) -> bool {
    match c {
        '\\' | '"' | 'n' => true,
        _ => false,
    }
}

// parse an escape character
fn escape<I>() -> impl Parser<Input = I, Output = char>
where
    I: Stream<Item = char>,
    I::Error: ParseError<I::Item, I::Range, I::Position>,
{
    satisfy(is_escape).then(|c| match c {
        '\\' => value('\\').left(),
        '"' => value('"').left(),
        'n' => value('\n').left(),
        _ => unexpected("escape sequence").with(value('?')).right(),
    })
}

pub fn is_non_newline_space(c: char) -> bool {
    c.is_whitespace() && !"\n\r".contains(c)
}

pub fn non_newline_space<I>() -> impl Parser<Input = I, Output = char>
where
    I: Stream<Item = char>,
    I::Error: ParseError<I::Item, I::Range, I::Position>,
{
    satisfy(is_non_newline_space).expected("non-newline whitespace")
}

pub fn non_newline_spaces<I>() -> impl Parser<Input = I, Output = ()>
where
    I: Stream<Item = char>,
    I::Error: ParseError<I::Item, I::Range, I::Position>,
{
    skip_many(non_newline_space()).expected("non-newline whitespaces")
}

// parse a normal character or an escape sequence
fn string_char<I>() -> impl Parser<Input = I, Output = char>
where
    I: Stream<Item = char>,
    I::Error: ParseError<I::Item, I::Range, I::Position>,
{
    satisfy(|c| c != '"').then(|c| {
        // if we encounter a backslash
        if c == '\\' {
            // return the escape sequence
            escape().left()
        } else {
            value(c).right()
        }
    })
}

// quoted: quote char* quote
pub fn quoted<I>() -> impl Parser<Input = I, Output = String>
where
    I: Stream<Item = char>,
    I::Error: ParseError<I::Item, I::Range, I::Position>,
{
    between(token('"'), token('"'), many(string_char()))
}

pub fn action<I>() -> impl Parser<Input = I, Output = (ElementState, String)>
where
    I: Stream<Item = char>,
    I::Error: ParseError<I::Item, I::Range, I::Position>,
{
    (
        one_of("+-".chars()).map(|action| match action {
            '+' => ElementState::Pressed,
            '-' => ElementState::Released,
            _ => unreachable!(),
        }),
        many(alpha_num()),
    )
}

pub fn newline<I>() -> impl Parser<Input = I, Output = ()>
where
    I: Stream<Item = char>,
    I::Error: ParseError<I::Item, I::Range, I::Position>,
{
    choice((string("\r\n"), string("\n"))).map(|_| ())
}

pub fn line_ending<I>() -> impl Parser<Input = I, Output = ()>
where
    I: Stream<Item = char>,
    I::Error: ParseError<I::Item, I::Range, I::Position>,
{
    choice((newline(), string(";").map(|_| ()))).map(|_| ())
}

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
