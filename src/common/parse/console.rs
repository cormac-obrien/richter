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

use common::parse::{line_ending, non_newline_space, non_newline_spaces, quoted};

use combine::char::string;
use combine::parser::repeat::{skip_many, skip_until};
use combine::parser::sequence::skip;
use combine::{
    choice, many, many1, not_followed_by, optional, satisfy, token, try, ParseError, Parser, Stream,
};

pub fn is_line_ending(c: char) -> bool {
    "\n;".contains(c)
}

/// Match a line comment, which begins with two forward slashes and ends at the next newline.
pub fn line_comment<I>() -> impl Parser<Input = I, Output = ()>
where
    I: Stream<Item = char>,
    I::Error: ParseError<I::Item, I::Range, I::Position>,
{
    (string("//"), skip_until(token('\n')))
        .map(|_| ())
        .message("in line_comment")
}

pub fn empty_line<I>() -> impl Parser<Input = I, Output = ()>
where
    I: Stream<Item = char>,
    I::Error: ParseError<I::Item, I::Range, I::Position>,
{
    (
        skip_many(choice((non_newline_space().map(|_| ()), line_comment()))),
        line_ending(),
    ).map(|_| ())
        .message("in empty_line")
}

/// Match a sequence of any non-whitespace, non-line-ending characters, ending with whitespace or a
/// line terminator.
pub fn basic_arg<I>() -> impl Parser<Input = I, Output = String>
where
    I: Stream<Item = char>,
    I::Error: ParseError<I::Item, I::Range, I::Position>,
{
    (
        not_followed_by(string("//")),
        many1(satisfy(|c: char| !c.is_whitespace() && !is_line_ending(c))),
    ).map(|(_, s)| s)
        .message("in basic_arg")
}

/// Match a basic argument or a quoted string.
pub fn arg<I>() -> impl Parser<Input = I, Output = String>
where
    I: Stream<Item = char>,
    I::Error: ParseError<I::Item, I::Range, I::Position>,
{
    choice((quoted(), basic_arg())).message("in arg")
}

pub fn command<I>() -> impl Parser<Input = I, Output = Vec<String>>
where
    I: Stream<Item = char>,
    I::Error: ParseError<I::Item, I::Range, I::Position>,
{
    (
        skip_many(non_newline_space()),
        many1(
            (
                // parse an argument
                arg(),
                // skip following spaces
                optional(non_newline_spaces()),
            ).map(|(a, _)| a),
        ).skip(optional(line_comment()))
            .skip(line_ending())
            .message("in command"),
    ).map(|(_, args)| args)
}

pub fn commands<I>() -> impl Parser<Input = I, Output = Vec<Vec<String>>>
where
    I: Stream<Item = char>,
    I::Error: ParseError<I::Item, I::Range, I::Position>,
{
    (
        // skip leading empty lines
        skip_many(try(empty_line())),
        many(
            (
                // parse command
                command(),
                // then any trailing empty lines
                skip_many(empty_line()),
            ).map(|(c, _)| c),
        ),
    ).map(|(_, cs)| cs)
        .message("in commands")
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_line_comment() {
        let result = line_comment()
            .easy_parse("// a comment\nnext line")
            .unwrap();
        assert_eq!(result, ((), "\nnext line"));
    }

    #[test]
    fn test_empty_line() {
        let result = empty_line()
            .easy_parse("  \t \t // a comment\nnext line")
            .unwrap();
        assert_eq!(result, ((), "next line"));
    }

    #[test]
    fn test_basic_arg_space_terminated() {
        let result = basic_arg().easy_parse("space_terminated ").unwrap();
        assert_eq!(result, ("space_terminated".to_owned(), " "));
    }

    #[test]
    fn test_basic_arg_newline_terminated() {
        let result = basic_arg().easy_parse("newline_terminated\n").unwrap();
        assert_eq!(result, ("newline_terminated".to_owned(), "\n"));
    }

    #[test]
    fn test_basic_arg_semicolon_terminated() {
        let result = basic_arg().easy_parse("semicolon_terminated;").unwrap();
        assert_eq!(result, ("semicolon_terminated".to_owned(), ";"));
    }

    #[test]
    fn test_arg_basic() {
        let result = arg().easy_parse("basic_arg \t;").unwrap();
        assert_eq!(result, ("basic_arg".to_owned(), " \t;"));
    }

    #[test]
    fn test_quoted_arg() {
        let result = arg().easy_parse("\"quoted \\\"argument\\\"\";\n").unwrap();
        assert_eq!(result, ("quoted \"argument\"".to_owned(), ";\n"));
    }

    #[test]
    fn test_command_basic() {
        let result = command().easy_parse("arg_0 arg_1;\n").unwrap();
        assert_eq!(result, (vec!["arg_0".to_owned(), "arg_1".to_owned()], "\n"));
    }

    #[test]
    fn test_command_quoted() {
        let result = command().easy_parse("bind \"space\" \"+jump\";\n").unwrap();
        assert_eq!(
            result,
            (
                vec!["bind".to_owned(), "space".to_owned(), "+jump".to_owned()],
                "\n"
            )
        );
    }

    #[test]
    fn test_command_comment() {
        let result = command()
            .easy_parse("bind \"space\" \"+jump\" // bind space to jump\n\n")
            .unwrap();
        assert_eq!(
            result,
            (
                vec!["bind".to_owned(), "space".to_owned(), "+jump".to_owned()],
                "\n"
            )
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
            vec!["exec".to_owned(), "default.cfg".to_owned()],
            vec!["exec".to_owned(), "config.cfg".to_owned()],
            vec!["exec".to_owned(), "autoexec.cfg".to_owned()],
            vec!["stuffcmds".to_owned()],
            vec![
                "startdemos".to_owned(),
                "demo1".to_owned(),
                "demo2".to_owned(),
                "demo3".to_owned(),
            ],
        ];

        let result = commands().easy_parse(script).unwrap();
        assert_eq!(result, (expected, ""));
    }
}
