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

use crate::common::parse::quoted;

use nom::{
    branch::alt,
    bytes::complete::tag,
    character::complete::{line_ending, multispace1, not_line_ending, one_of, space0},
    combinator::{map, opt, recognize, verify},
    multi::{many0, many1},
    sequence::{delimited, preceded, terminated, tuple},
};

/// Match a line comment.
///
/// A line comment is considered to be composed of:
/// - Two forward slashes (`"//"`)
/// - Zero or more characters, excluding line endings (`"\n"` or `"\r\n"`)
pub fn line_comment(input: &str) -> nom::IResult<&str, &str> {
    recognize(preceded(tag("//"), not_line_ending))(input)
}

/// Match an empty line.
///
/// An empty line is considered to be composed of:
/// - Zero or more spaces or tabs
/// - An optional line comment
/// - A line ending (`"\n"` or `"\r\n"`)
pub fn empty_line(input: &str) -> nom::IResult<&str, &str> {
    recognize(tuple((space0, opt(line_comment), line_ending)))(input)
}

/// Match a basic argument terminator.
///
/// Basic (unquoted) arguments can be terminated by any of:
/// - A non-newline whitespace character (`" "` or `"\t"`)
/// - The beginning of a line comment (`"//"`)
/// - A line ending (`"\r\n"` or `"\n"`)
/// - A semicolon (`";"`)
pub fn basic_arg_terminator(input: &str) -> nom::IResult<&str, &str> {
    alt((recognize(one_of(" \t;")), line_ending, tag("//")))(input)
}

/// Match a sequence of any non-whitespace, non-line-ending ASCII characters,
/// ending with whitespace, a line comment or a line terminator.
pub fn basic_arg(input: &str) -> nom::IResult<&str, &str> {
    // break on comment, semicolon, quote, or whitespace
    let patterns = ["//", ";", "\"", " ", "\t", "\r\n", "\n"];

    // length in bytes of matched sequence
    let mut match_len = 0;

    // consume characters not matching any of the patterns
    loop {
        let remaining = input.split_at(match_len).1;
        let terminator = patterns.iter().fold(false, |found_match: bool, p| {
            found_match || remaining.starts_with(*p)
        });

        let chr = match remaining.chars().nth(0) {
            Some(c) => c,
            None => break,
        };

        if terminator || !chr.is_ascii() || chr.is_ascii_control() {
            break;
        }

        match_len += chr.len_utf8();
    }

    match match_len {
        // TODO: more descriptive error?
        0 => Err(nom::Err::Error((input, nom::error::ErrorKind::Many1))),
        len => {
            let (matched, rest) = input.split_at(len);
            Ok((rest, matched))
        }
    }
}

/// Match a basic argument or a quoted string.
pub fn arg(input: &str) -> nom::IResult<&str, &str> {
    alt((quoted, basic_arg))(input)
}

/// Match a command terminator.
///
/// Commands can be terminated by either:
/// - A semicolon (`";"`), or
/// - An empty line (see `empty_line`)
pub fn command_terminator(input: &str) -> nom::IResult<&str, &str> {
    alt((empty_line, tag(";")))(input)
}

/// Match a single command.
///
/// A command is considered to be composed of:
/// - Zero or more leading non-newline whitespace characters
/// - One or more arguments, separated by non-newline whitespace characters
/// - A command terminator (see `command_terminator`)
pub fn command(input: &str) -> nom::IResult<&str, Vec<&str>> {
    terminated(many1(preceded(space0, arg)), command_terminator)(input)
}

pub fn commands(input: &str) -> nom::IResult<&str, Vec<Vec<&str>>> {
    delimited(
        many0(empty_line),
        many0(terminated(command, many0(empty_line))),
        many0(empty_line),
    )(input)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_line_comment() {
        let result = line_comment("// a comment\nnext line");
        assert_eq!(result, Ok(("\nnext line", "// a comment")));
    }

    #[test]
    fn test_empty_line() {
        let result = empty_line("  \t \t // a comment\nnext line");
        assert_eq!(result, Ok(("next line", "  \t \t // a comment\n")));
    }

    #[test]
    fn test_basic_arg_space_terminated() {
        let result = basic_arg("space_terminated ");
        assert_eq!(result, Ok((" ", "space_terminated")));
    }

    #[test]
    fn test_basic_arg_newline_terminated() {
        let result = basic_arg("newline_terminated\n");
        assert_eq!(result, Ok(("\n", "newline_terminated")));
    }

    #[test]
    fn test_basic_arg_semicolon_terminated() {
        let result = basic_arg("semicolon_terminated;");
        assert_eq!(result, Ok((";", "semicolon_terminated")));
    }

    #[test]
    fn test_arg_basic() {
        let result = arg("basic_arg \t;");
        assert_eq!(result, Ok((" \t;", "basic_arg")));
    }

    #[test]
    fn test_quoted_arg() {
        let result = arg("\"quoted \\\"argument\\\"\";\n");
        assert_eq!(result, Ok((";\n", "quoted \"argument\"")));
    }

    #[test]
    fn test_command_basic() {
        let result = command("arg_0 arg_1;\n");
        assert_eq!(
            result,
            Ok(("\n", vec!["arg_0", "arg_1"]))
        );
    }

    #[test]
    fn test_command_quoted() {
        let result = command("bind \"space\" \"+jump\";\n");
        assert_eq!(
            result,
            Ok((
                "\n",
                vec!["bind", "space", "+jump"]
            ))
        );
    }

    #[test]
    fn test_command_comment() {
        let result = command("bind \"space\" \"+jump\" // bind space to jump\n\n");
        assert_eq!(
            result,
            Ok((
                "\n",
                vec!["bind", "space", "+jump"]
            ))
        );
    }

    #[test]
    fn test_commands_quake_rc() {
        let script = "
// load the base configuration
exec default.cfg

// load the last saved configuration
exec config.cfg

// run a user script file if present
exec autoexec.cfg

//
// stuff command line statements
//
stuffcmds

// start demos if not already running a server
startdemos demo1 demo2 demo3
";
        let expected = vec![
            vec!["exec", "default.cfg"],
            vec!["exec", "config.cfg"],
            vec!["exec", "autoexec.cfg"],
            vec!["stuffcmds"],
            vec![
                "startdemos",
                "demo1",
                "demo2",
                "demo3",
            ],
        ];

        let result = commands(script);
        assert_eq!(result, Ok(("", expected)));
    }
}
