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

use std::collections::VecDeque;
use std::collections::HashMap;
use std::iter::FromIterator;

use glutin::VirtualKeyCode as Key;

/// Stores console commands.
pub struct CmdRegistry<'a> {
    cmds: HashMap<String, Box<Fn(Vec<&str>) + 'a>>,
}

impl<'a> CmdRegistry<'a> {
    pub fn new() -> CmdRegistry<'a> {
        CmdRegistry {
            cmds: HashMap::new(),
        }
    }

    /// Registers a new command.
    ///
    /// Returns an error if a command with the specified name already exists.
    pub fn add_cmd<S>(&mut self, name: S, cmd: Box<Fn(Vec<&str>) + 'a>) -> Result<(), ()>
    where
        S: AsRef<str>,
    {
        let name = name.as_ref().to_owned();

        match self.cmds.get(&name) {
            Some(_) => {
                error!("Command \"{}\" already registered.", name);
                return Err(());
            }
            None => {
                self.cmds.insert(name, cmd);
            }
        }

        Ok(())
    }

    /// Executes a command.
    ///
    /// Returns an error if no command with the specified name exists.
    pub fn exec_cmd<S>(&mut self, name: S, args: Vec<&str>) -> Result<(), ()>
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
}

/// A configuration variable.
///
/// Cvars are the primary method of configuring the game.
struct Cvar {
    // Value of this variable
    val: String,

    // If true, this variable should be archived in vars.rc
    archive: bool,

    // If true, updating this variable must also update serverinfo/userinfo
    info: bool,

    // The default value of this variable
    default: String,
}

pub struct CvarRegistry {
    cvars: HashMap<String, Cvar>
}

impl CvarRegistry {
    /// Construct a new empty `CvarRegistry`.
    pub fn new() -> CvarRegistry {
        CvarRegistry {
            cvars: HashMap::new(),
        }
    }

    /// Register a new `Cvar` with the given name.
    pub fn register<S>(&mut self, name: S, default: S) -> Result<(), ()> where S: AsRef<str> {
        let name = name.as_ref();
        let default = default.as_ref();

        match self.cvars.get(name) {
            Some(_) => return Err(()),
            None => {
                self.cvars.insert(name.to_owned(), Cvar {
                    val: default.to_owned(),
                    archive: false,
                    info: false,
                    default: default.to_owned(),
                });
            }
        }

        Ok(())
    }

    /// Register a new archived `Cvar` with the given name.
    ///
    /// The value of this `Cvar` should be written to `vars.rc` whenever the game is closed or
    /// `host_writeconfig` is issued.
    pub fn register_archive<S>(&mut self, name: S, default: S) -> Result<(), ()> where S: AsRef<str> {
        let name = name.as_ref();
        let default = default.as_ref();

        match self.cvars.get(name) {
            Some(_) => return Err(()),
            None => {
                self.cvars.insert(name.to_owned(), Cvar {
                    val: default.to_owned(),
                    archive: true,
                    info: false,
                    default: default.to_owned(),
                });
            }
        }

        Ok(())
    }

    /// Register a new info `Cvar` with the given name.
    ///
    /// When this `Cvar` is set, the serverinfo or userinfo string should be update to reflect its
    /// new value.
    pub fn register_updateinfo<S>(&mut self, name: S, default: S) -> Result<(), ()> where S: AsRef<str> {
        let name = name.as_ref();
        let default = default.as_ref();

        match self.cvars.get(name) {
            Some(_) => return Err(()),
            None => {
                self.cvars.insert(name.to_owned(), Cvar {
                    val: default.to_owned(),
                    archive: false,
                    info: true,
                    default: default.to_owned(),
                });
            }
        }

        Ok(())
    }

    /// Register a new info + archived `Cvar` with the given name.
    ///
    /// The value of this `Cvar` should be written to `vars.rc` whenever the game is closed or
    /// `host_writeconfig` is issued. Additionally, when this `Cvar` is set, the serverinfo or
    /// userinfo string should be updated to reflect its new value.
    pub fn register_archive_updateinfo<S>(&mut self, name: S, default: S) -> Result<(), ()>
        where S: AsRef<str> {
        let name = name.as_ref();
        let default = default.as_ref();

        match self.cvars.get(name) {
            Some(_) => return Err(()),
            None => {
                self.cvars.insert(name.to_owned(), Cvar {
                    val: default.to_owned(),
                    archive: true,
                    info: true,
                    default: default.to_owned(),
                });
            }
        }

        Ok(())
    }

    pub fn get<S>(&mut self, name: S) -> Result<&str, ()> where S: AsRef<str> {
        debug!("cvar lookup: {}", name.as_ref());
        match self.cvars.get(name.as_ref()) {
            Some(s) => Ok(&s.val),
            None => Err(()),
        }
    }

    pub fn get_value<S>(&mut self, name: S) -> Result<f32, ()> where S: AsRef<str> {
        debug!("cvar value lookup: {}", name.as_ref());
        match self.cvars.get(name.as_ref()) {
            Some(s) => {
                match s.val.parse() {
                    Ok(f) => Ok(f),
                    Err(_) => Err(()),
                }
            }
            None => Err(())
        }
    }

    pub fn set<S>(&mut self, name: S, value: S) -> Result<(), ()> where S: AsRef<str> {
        debug!("cvar assignment: {} {}", name.as_ref(), value.as_ref());
        match self.cvars.get_mut(name.as_ref()) {
            Some(s) => {
                s.val = value.as_ref().to_owned();
                Ok(())
            }
            None => Err(())
        }
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
    lines: Vec<Vec<char>>,
}

impl ConsoleOutput {
    pub fn println<S>(&mut self, msg: S)
    where
        S: AsRef<str>,
    {
        println!("{}", msg.as_ref());
    }
}

pub struct Console {
    input: ConsoleInput,
    hist: History,
}

impl Console {
    pub fn new() -> Console {
        Console {
            input: ConsoleInput::new(),
            hist: History::new(),
        }
    }

    pub fn send_char(&mut self, c: char) -> Result<(), ()> {
        match c {
            '\r' => {
                let entered = self.get_string();
                let mut parts = entered.split_whitespace();

                let cmd_name = match parts.next() {
                    Some(c) => c,
                    None => return Ok(()),
                };

                let args: Vec<&str> = parts.collect();

                self.hist.add_line(self.input.get_text());
                self.input.clear();
            }

            // backspace
            '\x08' => self.input.backspace(),

            // delete
            '\x7f' => self.input.delete(),

            '\t' => (), // TODO: tab completion

            c => self.input.insert(c),
        }

        println!("{}", self.debug_string());

        Ok(())
    }

    pub fn send_key(&mut self, key: Key) {
        match key {
            Key::Up => if let Some(line) = self.hist.line_up() {
                self.input.set_text(&line);
            }

            Key::Down => if let Some(line) = self.hist.line_down() {
                self.input.set_text(&line);
            }

            Key::Right => self.input.cursor_right(),
            Key::Left => self.input.cursor_left(),
            _ => return,
        }

        println!("{}", self.debug_string());
    }

    fn get_string(&self) -> String {
        String::from_iter(self.input.text.clone().into_iter())
    }

    fn debug_string(&self) -> String {
        format!(
            "{}_{}",
            String::from_iter(self.input.text[..self.input.curs].to_owned().into_iter()),
            String::from_iter(self.input.text[self.input.curs..].to_owned().into_iter())
        )
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
            match self.get_remaining_input()
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
    pub fn next(&mut self) -> Option<&'a str> {
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
                        Some(&self.input[offset..offset + len])
                    }

                    // This case should not be possible
                    Some(_) => None,

                    // This means an unmatched quote.
                    // TODO: this should not panic, make it fail gracefully and update the docs
                    None => panic!("Unmatched quote in Tokenizer::next()"),
                }
            }

            // Any other token ends on the next whitespace character
            Some((start_i, _)) => {
                let offset = self.byte_offset + start_i;

                match char_indices.take_while(|&(_, c)| !c.is_whitespace()).last() {
                    Some((end_i, _)) => {
                        let len = end_i + 1 - start_i;
                        self.byte_offset += len;
                        Some(&self.input[offset..offset + len])
                    }
                    None => None,
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
