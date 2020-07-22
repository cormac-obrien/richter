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

use crate::common::parse::quoted;

use nom::{
    bytes::complete::tag,
    character::complete::newline,
    combinator::map,
    multi::many0,
    sequence::{delimited, separated_pair, terminated},
};

// "name" "value"\n
pub fn entity_attribute(input: &str) -> nom::IResult<&str, (&str, &str)> {
    terminated(separated_pair(quoted, tag(" "), quoted), newline)(input)
}

// {
// "name1" "value1"
// "name2" "value2"
// "name3" "value3"
// }
pub fn entity(input: &str) -> nom::IResult<&str, HashMap<&str, &str>> {
    delimited(
        terminated(tag("{"), newline),
        map(many0(entity_attribute), |attrs| attrs.into_iter().collect()),
        terminated(tag("}"), newline),
    )(input)
}

pub fn entities(input: &str) -> nom::IResult<&str, Vec<HashMap<&str, &str>>> {
    many0(entity)(input)
}
