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

// TODO: implement proper Error types

// TODO: have console commands take an IntoIter<AsRef<str>> instead of a Vec<String>

use std::cell::{Ref, RefCell};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::iter::FromIterator;
use std::rc::Rc;

use crate::common::parse;

use combine::Parser;
use failure::Error;

/// Stores console commands.
pub struct CmdRegistry {
    cmds: HashMap<String, Box<Fn(&[&str])>>,
}

impl CmdRegistry {
    pub fn new() -> CmdRegistry {
        CmdRegistry {
            cmds: HashMap::new(),
        }
    }

    /// Registers a new command with the given name.
    ///
    /// Returns an error if a command with the specified name already exists.
    pub fn insert<S>(&mut self, name: S, cmd: Box<Fn(&[&str])>) -> Result<(), ()>
    where
        S: AsRef<str>,
    {
        match self.cmds.get(name.as_ref()) {
            Some(_) => {
                error!("Command \"{}\" already registered.", name.as_ref());
                return Err(());
            }
            None => {
                self.cmds.insert(name.as_ref().to_owned(), cmd);
            }
        }

        Ok(())
    }

    /// Registers a new command with the given name, or replaces one if the name is in use.
    pub fn insert_or_replace<S>(&mut self, name: S, cmd: Box<Fn(&[&str])>) -> Result<(), ()>
    where
        S: AsRef<str>,
    {
        self.cmds.insert(name.as_ref().to_owned(), cmd);
        Ok(())
    }

    /// Executes a command.
    ///
    /// Returns an error if no command with the specified name exists.
    pub fn exec<S>(&mut self, name: S, args: &[&str]) -> Result<(), ()>
    where
        S: AsRef<str>,
    {
        let name = name.as_ref().to_owned();

        match self.cmds.get(&name) {
            Some(cmd) => cmd(args),
            None => return Err(()),
        }

        Ok(())
    }

    pub fn contains<S>(&self, name: S) -> bool
    where
        S: AsRef<str>,
    {
        self.cmds.contains_key(name.as_ref())
    }
}

/// A configuration variable.
///
/// Cvars are the primary method of configuring the game.
struct Cvar {
    // Value of this variable
    val: String,

    // If true, this variable should be archived in vars.rc
    archive: bool,

    // If true:
    // - If a server cvar, broadcast updates to clients
    // - If a client cvar, update userinfo
    notify: bool,

    // The default value of this variable
    default: String,
}

pub struct CvarRegistry {
    cvars: RefCell<HashMap<String, Cvar>>,
}

impl CvarRegistry {
    /// Construct a new empty `CvarRegistry`.
    pub fn new() -> CvarRegistry {
        CvarRegistry {
            cvars: RefCell::new(HashMap::new()),
        }
    }

    fn register_impl<S>(&self, name: S, default: S, archive: bool, notify: bool) -> Result<(), ()>
    where
        S: AsRef<str>,
    {
        let name = name.as_ref();
        let default = default.as_ref();

        let mut cvars = self.cvars.borrow_mut();
        match cvars.get(name) {
            Some(_) => return Err(()),
            None => {
                cvars.insert(
                    name.to_owned(),
                    Cvar {
                        val: default.to_owned(),
                        archive,
                        notify,
                        default: default.to_owned(),
                    },
                );
            }
        }

        Ok(())
    }

    /// Register a new `Cvar` with the given name.
    pub fn register<S>(&self, name: S, default: S) -> Result<(), ()>
    where
        S: AsRef<str>,
    {
        self.register_impl(name, default, false, false)
    }

    /// Register a new archived `Cvar` with the given name.
    ///
    /// The value of this `Cvar` should be written to `vars.rc` whenever the game is closed or
    /// `host_writeconfig` is issued.
    pub fn register_archive<S>(&self, name: S, default: S) -> Result<(), ()>
    where
        S: AsRef<str>,
    {
        self.register_impl(name, default, true, false)
    }

    /// Register a new notify `Cvar` with the given name.
    ///
    /// When this `Cvar` is set:
    /// - If the host is a server, broadcast that the variable has been changed to all clients.
    /// - If the host is a client, update the clientinfo string.
    pub fn register_notify<S>(&self, name: S, default: S) -> Result<(), ()>
    where
        S: AsRef<str>,
    {
        self.register_impl(name, default, false, true)
    }

    /// Register a new notify + archived `Cvar` with the given name.
    ///
    /// The value of this `Cvar` should be written to `vars.rc` whenever the game is closed or
    /// `host_writeconfig` is issued.
    ///
    /// Additionally, when this `Cvar` is set:
    /// - If the host is a server, broadcast that the variable has been changed to all clients.
    /// - If the host is a client, update the clientinfo string.
    pub fn register_archive_notify<S>(&mut self, name: S, default: S) -> Result<(), ()>
    where
        S: AsRef<str>,
    {
        self.register_impl(name, default, true, true)
    }

    pub fn get<S>(&self, name: S) -> Result<String, ()>
    where
        S: AsRef<str>,
    {
        match self.cvars.borrow().get(name.as_ref()) {
            Some(s) => Ok(s.val.to_owned()),
            None => Err(()),
        }
    }

    pub fn get_value<S>(&self, name: S) -> Result<f32, ()>
    where
        S: AsRef<str>,
    {
        match self.cvars.borrow().get(name.as_ref()) {
            Some(s) => match s.val.parse() {
                Ok(f) => Ok(f),
                Err(_) => Err(()),
            },
            None => Err(()),
        }
    }

    pub fn set<S>(&self, name: S, value: S) -> Result<(), ()>
    where
        S: AsRef<str>,
    {
        debug!("cvar assignment: {} {}", name.as_ref(), value.as_ref());
        match self.cvars.borrow_mut().get_mut(name.as_ref()) {
            Some(s) => {
                s.val = value.as_ref().to_owned();
                if s.notify {
                    // TODO: update userinfo/serverinfo
                    unimplemented!();
                }
                Ok(())
            }
            None => Err(()),
        }
    }

    pub fn contains<S>(&self, name: S) -> bool
    where
        S: AsRef<str>,
    {
        self.cvars.borrow().contains_key(name.as_ref())
    }
}

/// The line of text currently being edited in the console.
pub struct ConsoleInput {
    text: Vec<char>,
    curs: usize,
}

impl ConsoleInput {
    /// Constructs a new `ConsoleInput`.
    ///
    /// Initializes the text content to be empty and places the cursor at position 0.
    pub fn new() -> ConsoleInput {
        ConsoleInput {
            text: Vec::new(),
            curs: 0,
        }
    }

    /// Returns the current content of the `ConsoleInput`.
    pub fn get_text(&self) -> Vec<char> {
        self.text.to_owned()
    }

    /// Sets the content of the `ConsoleInput` to `Text`.
    ///
    /// This also moves the cursor to the end of the line.
    pub fn set_text(&mut self, text: &Vec<char>) {
        self.text = text.clone();
        self.curs = self.text.len();
    }

    /// Inserts the specified character at the position of the cursor.
    ///
    /// The cursor is moved one character to the right.
    pub fn insert(&mut self, c: char) {
        self.text.insert(self.curs, c);
        self.cursor_right();
    }

    /// Moves the cursor to the right.
    ///
    /// If the cursor is at the end of the current text, no change is made.
    pub fn cursor_right(&mut self) {
        if self.curs < self.text.len() {
            self.curs += 1;
        }
    }

    /// Moves the cursor to the left.
    ///
    /// If the cursor is at the beginning of the current text, no change is made.
    pub fn cursor_left(&mut self) {
        if self.curs > 0 {
            self.curs -= 1;
        }
    }

    /// Deletes the character to the right of the cursor.
    ///
    /// If the cursor is at the end of the current text, no character is deleted.
    pub fn delete(&mut self) {
        if self.curs < self.text.len() {
            self.text.remove(self.curs);
        }
    }

    /// Deletes the character to the left of the cursor.
    ///
    /// If the cursor is at the beginning of the current text, no character is deleted.
    pub fn backspace(&mut self) {
        if self.curs > 0 {
            self.text.remove(self.curs - 1);
            self.curs -= 1;
        }
    }

    /// Clears the contents of the `ConsoleInput`.
    ///
    /// Also moves the cursor to position 0.
    pub fn clear(&mut self) {
        self.text.clear();
        self.curs = 0;
    }

    pub fn debug_string(&self) -> String {
        format!(
            "{}_{}",
            String::from_iter(self.text[..self.curs].to_owned().into_iter()),
            String::from_iter(self.text[self.curs..].to_owned().into_iter())
        )
    }
}

pub struct History {
    lines: VecDeque<Vec<char>>,
    curs: usize,
}

impl History {
    pub fn new() -> History {
        History {
            lines: VecDeque::new(),
            curs: 0,
        }
    }

    pub fn add_line(&mut self, line: Vec<char>) {
        self.lines.push_front(line);
        self.curs = 0;
    }

    // TODO: handle case where history is empty
    pub fn line_up(&mut self) -> Option<Vec<char>> {
        if self.lines.len() == 0 || self.curs >= self.lines.len() {
            None
        } else {
            self.curs += 1;
            Some(self.lines[self.curs - 1].clone())
        }
    }

    pub fn line_down(&mut self) -> Option<Vec<char>> {
        if self.curs > 0 {
            self.curs -= 1;
        }

        if self.curs > 0 {
            Some(self.lines[self.curs - 1].clone())
        } else {
            Some(Vec::new().clone())
        }
    }
}

pub struct ConsoleOutput {
    lines: VecDeque<Vec<char>>,
}

impl ConsoleOutput {
    pub fn new() -> ConsoleOutput {
        ConsoleOutput {
            lines: VecDeque::new(),
        }
    }

    pub fn push(&mut self, chars: Vec<char>) {
        self.lines.push_front(chars);
        // TODO: set maximum capacity and pop_back when we reach it
    }

    pub fn lines(&self) -> impl Iterator<Item = &[char]> {
        self.lines.iter().map(|v| v.as_slice())
    }
}

pub struct Console {
    cmds: Rc<RefCell<CmdRegistry>>,
    cvars: Rc<RefCell<CvarRegistry>>,
    aliases: Rc<RefCell<HashMap<String, String>>>,

    input: ConsoleInput,
    hist: History,
    buffer: RefCell<String>,
    output: Rc<RefCell<ConsoleOutput>>,
}

impl Console {
    pub fn new(cmds: Rc<RefCell<CmdRegistry>>, cvars: Rc<RefCell<CvarRegistry>>) -> Console {
        let output = Rc::new(RefCell::new(ConsoleOutput::new()));
        let echo_output = output.clone();
        cmds.borrow_mut()
            .insert(
                "echo",
                Box::new(move |args| {
                    let msg = match args.len() {
                        0 => "",
                        _ => args[0],
                    };

                    echo_output.borrow_mut().push(msg.chars().collect());
                }),
            )
            .unwrap();

        let aliases: Rc<RefCell<HashMap<String, String>>> = Rc::new(RefCell::new(HashMap::new()));
        let cmd_aliases = aliases.clone();
        cmds.borrow_mut()
            .insert(
                "alias",
                Box::new(move |args| match args.len() {
                    0 => {
                        for (name, script) in cmd_aliases.borrow().iter() {
                            println!("    {}: {}", name, script);
                        }
                        println!("{} alias command(s)", cmd_aliases.borrow().len());
                    }

                    2 => {
                        let name = args[0].to_string();
                        let script = args[1].to_string();
                        let _ = cmd_aliases.borrow_mut().insert(name, script);
                    }

                    _ => (),
                }),
            )
            .unwrap();

        Console {
            cmds,
            cvars,
            aliases: aliases.clone(),
            input: ConsoleInput::new(),
            hist: History::new(),
            buffer: RefCell::new(String::new()),
            output: output.clone(),
        }
    }

    pub fn send_char(&mut self, c: char) -> Result<(), Error> {
        match c {
            // ignore grave key
            '`' => (),

            '\r' => {
                // cap with a newline
                self.input.insert('\n');

                // push this line to the execution buffer
                let entered = self.get_string();
                self.buffer.borrow_mut().push_str(&entered);

                // add the current input to the history
                self.hist.add_line(self.input.get_text());

                // echo the input to console output
                let mut input_echo: Vec<char> = vec![']'];
                input_echo.append(&mut self.input.get_text());
                self.output.borrow_mut().push(input_echo);

                // clear the input line
                self.input.clear();
            }

            '\x08' => self.input.backspace(),
            '\x7f' => self.input.delete(),

            '\t' => warn!("Tab completion not implemented"), // TODO: tab completion

            // TODO: we should probably restrict what characters are allowed
            c => self.input.insert(c),
        }

        Ok(())
    }

    pub fn cursor_right(&mut self) {
        self.input.cursor_right()
    }

    pub fn cursor_left(&mut self) {
        self.input.cursor_left()
    }

    pub fn history_up(&mut self) {
        if let Some(line) = self.hist.line_up() {
            self.input.set_text(&line);
        }
    }

    pub fn history_down(&mut self) {
        if let Some(line) = self.hist.line_down() {
            self.input.set_text(&line);
        }
    }

    /// Interprets the contents of the execution buffer.
    pub fn execute(&self) {
        let text = self.buffer.borrow().to_owned();
        self.buffer.borrow_mut().clear();

        let (commands, _remaining) = parse::commands().easy_parse(text.as_str()).unwrap();

        for command in commands.iter() {
            debug!("{:?}", command);
        }

        for args in commands {
            if let Some(arg_0) = args.get(0) {
                let maybe_alias = self.aliases.borrow().get(arg_0).map(|s| s.to_owned());
                match maybe_alias {
                    Some(a) => {
                        self.stuff_text(a);
                        self.execute();
                    }

                    None => {
                        let tail_args: Vec<&str> =
                            (&args[1..]).iter().map(|s| s.as_str()).collect();

                        if self.cmds.borrow().contains(arg_0) {
                            self.cmds.borrow_mut().exec(arg_0, &tail_args).unwrap();
                        } else if self.cvars.borrow().contains(arg_0) {
                            // TODO error handling on cvar set
                            match args.get(1) {
                                Some(arg_1) => self.cvars.borrow_mut().set(arg_0, arg_1).unwrap(),
                                None => {
                                    let msg = format!(
                                        "\"{}\" is \"{}\"",
                                        arg_0,
                                        self.cvars.borrow().get(arg_0).unwrap()
                                    );
                                    self.output
                                        .borrow_mut()
                                        .push(msg.as_str().chars().collect());
                                }
                            }
                        } else {
                            // TODO: try sending to server first
                            self.output.borrow_mut().push(
                                format!("Unrecognized command \"{}\"", arg_0)
                                    .as_str()
                                    .chars()
                                    .collect(),
                            );
                        }
                    }
                }
            }
        }
    }

    pub fn get_string(&self) -> String {
        String::from_iter(self.input.text.clone().into_iter())
    }

    pub fn debug_string(&self) -> String {
        format!(
            "{}_{}",
            String::from_iter(self.input.text[..self.input.curs].to_owned().into_iter()),
            String::from_iter(self.input.text[self.input.curs..].to_owned().into_iter())
        )
    }

    pub fn stuff_text<S>(&self, text: S)
    where
        S: AsRef<str>,
    {
        debug!("stuff_text:\n{:?}", text.as_ref());
        self.buffer.borrow_mut().push_str(text.as_ref());

        // in case the last line doesn't end with a newline
        self.buffer.borrow_mut().push_str("\n");
    }

    pub fn output(&self) -> Ref<ConsoleOutput> {
        self.output.borrow()
    }
}

pub struct Tokenizer<'a> {
    input: &'a str,
    byte_offset: usize,
}

impl<'a> Tokenizer<'a> {
    /// Constructs a command tokenizer with the specified input stream.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate richter;
    /// use richter::common::console::Tokenizer;
    ///
    /// # fn main() {
    /// let tokenizer = Tokenizer::new("map e1m1");
    /// # }
    /// ```
    pub fn new(input: &'a str) -> Tokenizer<'a> {
        Tokenizer {
            input: input,
            byte_offset: 0,
        }
    }

    fn get_remaining_input(&self) -> &'a str {
        &self.input[self.byte_offset..]
    }

    fn skip_spaces(&mut self) {
        let iter = self.get_remaining_input().char_indices();
        match iter.skip_while(|&(_, c)| c.is_whitespace()).next() {
            Some((i, _)) => self.byte_offset += i,
            None => self.byte_offset = self.input.len(),
        }
    }

    fn try_skip_line_comment(&mut self) -> bool {
        if self.get_remaining_input().starts_with("//") {
            match self
                .get_remaining_input()
                .char_indices()
                .skip_while(|&(_, c)| c != '\n')
                .next()
            {
                Some((i, _)) => self.byte_offset += i,
                None => self.byte_offset = self.input.len(),
            }

            return true;
        }

        false
    }
}

impl<'a> ::std::iter::Iterator for Tokenizer<'a> {
    type Item = &'a str;

    /// Returns the next token in the input stream.
    ///
    /// This will skip any leading any leading whitespace characters as recognized by the
    /// `.is_whitespace()` function of `std::char`. Note that the original Quake engine only
    /// expects ASCII input and recognizes as whitespace any character with a code point less than
    /// or equal to `U+0020`, including control characters. It will also skip single-line comments
    /// beginning with `//` and ending with a newline (`U+000A LINE FEED`).
    ///
    /// This function *does not* process semicolons in order to split commands.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate richter;
    /// use richter::common::console::Tokenizer;
    ///
    /// # fn main() {
    /// let mut tokenizer = Tokenizer::new("map e1m1");
    /// assert_eq!(tokenizer.next(), Some("map"));
    /// assert_eq!(tokenizer.next(), Some("e1m1"));
    /// assert_eq!(tokenizer.next(), None);
    /// # }
    /// ```
    ///
    /// # Panics
    ///
    /// The function panics if the end of input is reached and there is an unmatched double-quote.
    /// This is not permanent behavior.
    fn next(&mut self) -> Option<&'a str> {
        loop {
            // Skip leading whitespace
            self.skip_spaces();

            // If this line is a comment, move on to the next line
            if !self.try_skip_line_comment() {
                break;
            }
        }

        let mut char_indices = self.get_remaining_input().char_indices();
        match char_indices.next() {
            // On encountering an opening double-quote, find the closing double-quote
            Some((start_i, '"')) => {
                let offset = self.byte_offset + start_i;
                match char_indices.skip_while(|&(_, c)| c != '"').next() {
                    Some((end_i, '"')) => {
                        let len = end_i + 1 - start_i;
                        self.byte_offset += len;
                        Some(&self.input[offset + 1..offset + len - 1])
                    }

                    // This case should not be possible
                    Some(_) => None,

                    // This means an unmatched quote.
                    // TODO: this should not panic, make it fail gracefully and update the docs
                    None => panic!("Unmatched quote in Tokenizer::next()"),
                }
            }

            // Any other token ends on the next whitespace character
            Some((start_i, start_char)) => {
                let offset = self.byte_offset + start_i;

                match char_indices.take_while(|&(_, c)| !c.is_whitespace()).last() {
                    Some((end_i, _)) => {
                        let len = end_i + 1 - start_i;
                        self.byte_offset += len;
                        Some(&self.input[offset..offset + len])
                    }

                    // single-character token
                    None => {
                        let len = start_char.len_utf8();
                        self.byte_offset += len;
                        Some(&self.input[offset..offset + len])
                    }
                }
            }

            // If there are no characters left, tokenizer is at the end of the input stream
            None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenizer_empty() {
        let mut tokenizer = Tokenizer::new("");
        assert_eq!(tokenizer.next(), None);
    }

    #[test]
    fn test_tokenizer_whitespace_only() {
        let mut tokenizer = Tokenizer::new(" \t\n\r");
        assert_eq!(tokenizer.next(), None);
    }

    #[test]
    fn test_tokenizer_comment_only() {
        let mut tokenizer = Tokenizer::new("// this is a comment");
        assert_eq!(tokenizer.next(), None);
    }
}
