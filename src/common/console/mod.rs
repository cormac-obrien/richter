// Copyright © 2018 Cormac O'Brien
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

use std::{
    cell::{Ref, RefCell},
    collections::{HashMap, VecDeque},
    iter::FromIterator,
    rc::Rc,
};

use crate::common::parse;

use chrono::{Duration, Utc};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConsoleError {
    #[error("{0}")]
    CmdError(String),
    #[error("Could not parse cvar as a number: {name} = \"{value}\"")]
    CvarParseFailed { name: String, value: String },
    #[error(
        "Command already registered: {0} (to replace existing definition, use `insert_or_replace`)"
    )]
    DuplicateCommand(String),
    #[error("Cvar already registered: {0}")]
    DuplicateCvar(String),
    #[error("No such command: {0}")]
    NoSuchCommand(String),
    #[error("No such cvar: {0}")]
    NoSuchCvar(String),
}

type Cmd = Box<dyn Fn(&[&str]) -> String>;

/// Stores console commands.
pub struct CmdRegistry {
    cmds: HashMap<String, Cmd>,
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
    pub fn insert<S>(&mut self, name: S, cmd: Cmd) -> Result<(), ConsoleError>
    where
        S: AsRef<str>,
    {
        let name = name.as_ref();
        match self.cmds.get(name) {
            Some(_) => Err(ConsoleError::DuplicateCommand(name.to_owned()))?,
            None => {
                self.cmds.insert(name.to_owned(), cmd);
            }
        }

        Ok(())
    }

    /// Registers a new command with the given name, or replaces one if the name is in use.
    pub fn insert_or_replace<S>(&mut self, name: S, cmd: Cmd)
    where
        S: AsRef<str>,
    {
        self.cmds.insert(name.as_ref().to_owned(), cmd);
    }

    /// Removes the command with the given name.
    ///
    /// Returns an error if there was no command with that name.
    pub fn remove<S>(&mut self, name: S) -> Result<(), ConsoleError>
    where
        S: AsRef<str>,
    {
        match self.cmds.remove(name.as_ref()) {
            Some(_) => Ok(()),
            None => Err(ConsoleError::NoSuchCommand(name.as_ref().to_string()))?,
        }
    }

    /// Executes a command.
    ///
    /// Returns an error if no command with the specified name exists.
    pub fn exec<S>(&mut self, name: S, args: &[&str]) -> Result<String, ConsoleError>
    where
        S: AsRef<str>,
    {
        let cmd = self
            .cmds
            .get(name.as_ref())
            .ok_or(ConsoleError::NoSuchCommand(name.as_ref().to_string()))?;

        Ok(cmd(args))
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

    fn register_impl<S>(
        &self,
        name: S,
        default: S,
        archive: bool,
        notify: bool,
    ) -> Result<(), ConsoleError>
    where
        S: AsRef<str>,
    {
        let name = name.as_ref();
        let default = default.as_ref();

        let mut cvars = self.cvars.borrow_mut();
        match cvars.get(name) {
            Some(_) => Err(ConsoleError::DuplicateCvar(name.to_owned()))?,
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
    pub fn register<S>(&self, name: S, default: S) -> Result<(), ConsoleError>
    where
        S: AsRef<str>,
    {
        self.register_impl(name, default, false, false)
    }

    /// Register a new archived `Cvar` with the given name.
    ///
    /// The value of this `Cvar` should be written to `vars.rc` whenever the game is closed or
    /// `host_writeconfig` is issued.
    pub fn register_archive<S>(&self, name: S, default: S) -> Result<(), ConsoleError>
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
    pub fn register_notify<S>(&self, name: S, default: S) -> Result<(), ConsoleError>
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
    pub fn register_archive_notify<S>(&mut self, name: S, default: S) -> Result<(), ConsoleError>
    where
        S: AsRef<str>,
    {
        self.register_impl(name, default, true, true)
    }

    pub fn get<S>(&self, name: S) -> Result<String, ConsoleError>
    where
        S: AsRef<str>,
    {
        Ok(self
            .cvars
            .borrow()
            .get(name.as_ref())
            .ok_or(ConsoleError::NoSuchCvar(name.as_ref().to_owned()))?
            .val
            .clone())
    }

    pub fn get_value<S>(&self, name: S) -> Result<f32, ConsoleError>
    where
        S: AsRef<str>,
    {
        let name = name.as_ref();
        let mut cvars = self.cvars.borrow_mut();
        let cvar = cvars
            .get_mut(name)
            .ok_or(ConsoleError::NoSuchCvar(name.to_owned()))?;

        // try parsing as f32
        let val_string = cvar.val.clone();
        let val = match val_string.parse::<f32>() {
            Ok(v) => Ok(v),
            // if parse fails, reset to default value and try again
            Err(_) => {
                cvar.val = cvar.default.clone();
                cvar.val.parse::<f32>()
            }
        }
        .or(Err(ConsoleError::CvarParseFailed {
            name: name.to_owned(),
            value: val_string.clone(),
        }))?;

        Ok(val)
    }

    pub fn set<S>(&self, name: S, value: S) -> Result<(), ConsoleError>
    where
        S: AsRef<str>,
    {
        trace!("cvar assignment: {} {}", name.as_ref(), value.as_ref());
        let mut cvars = self.cvars.borrow_mut();
        let mut cvar = cvars
            .get_mut(name.as_ref())
            .ok_or(ConsoleError::NoSuchCvar(name.as_ref().to_owned()))?;
        cvar.val = value.as_ref().to_owned();
        if cvar.notify {
            // TODO: update userinfo/serverinfo
            unimplemented!();
        }

        Ok(())
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
            Some(Vec::new())
        }
    }
}

pub struct ConsoleOutput {
    // A ring buffer of lines of text. Each line has an optional timestamp used
    // to determine whether it should be displayed on screen. If the timestamp
    // is `None`, the message will not be displayed.
    //
    // The timestamp is specified in seconds since the Unix epoch (so it is
    // decoupled from client/server time).
    lines: VecDeque<(Vec<char>, Option<i64>)>,
}

impl ConsoleOutput {
    pub fn new() -> ConsoleOutput {
        ConsoleOutput {
            lines: VecDeque::new(),
        }
    }

    fn push<C>(&mut self, chars: C, timestamp: Option<i64>)
    where
        C: IntoIterator<Item = char>,
    {
        self.lines
            .push_front((chars.into_iter().collect(), timestamp))
        // TODO: set maximum capacity and pop_back when we reach it
    }

    pub fn lines(&self) -> impl Iterator<Item = &[char]> {
        self.lines.iter().map(|(v, _)| v.as_slice())
    }

    /// Return an iterator over lines that have been printed in the last
    /// `interval` of time.
    ///
    /// The iterator yields the oldest results first.
    ///
    /// `max_candidates` specifies the maximum number of lines to consider,
    /// while `max_results` specifies the maximum number of lines that should
    /// be returned.
    pub fn recent_lines(
        &self,
        interval: Duration,
        max_candidates: usize,
        max_results: usize,
    ) -> impl Iterator<Item = &[char]> {
        let timestamp = (Utc::now() - interval).timestamp();
        self.lines
            .iter()
            // search only the most recent `max_candidates` lines
            .take(max_candidates)
            // yield oldest to newest
            .rev()
            // eliminate non-timestamped lines and lines older than `timestamp`
            .filter_map(move |(l, t)| if (*t)? > timestamp { Some(l) } else { None })
            // return at most `max_results` lines
            .take(max_results)
            .map(Vec::as_slice)
    }
}

pub struct Console {
    cmds: Rc<RefCell<CmdRegistry>>,
    cvars: Rc<RefCell<CvarRegistry>>,
    aliases: Rc<RefCell<HashMap<String, String>>>,

    input: ConsoleInput,
    hist: History,
    buffer: RefCell<String>,

    out_buffer: RefCell<Vec<char>>,
    output: RefCell<ConsoleOutput>,
}

impl Console {
    pub fn new(cmds: Rc<RefCell<CmdRegistry>>, cvars: Rc<RefCell<CvarRegistry>>) -> Console {
        let output = RefCell::new(ConsoleOutput::new());
        cmds.borrow_mut()
            .insert(
                "echo",
                Box::new(move |args| {
                    let msg = match args.len() {
                        0 => "",
                        _ => args[0],
                    };

                    msg.to_owned()
                }),
            )
            .unwrap();

        let aliases: Rc<RefCell<HashMap<String, String>>> = Rc::new(RefCell::new(HashMap::new()));
        let cmd_aliases = aliases.clone();
        cmds.borrow_mut()
            .insert(
                "alias",
                Box::new(move |args| {
                    match args.len() {
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
                    }
                    String::new()
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
            out_buffer: RefCell::new(Vec::new()),
            output,
        }
    }

    // The timestamp is applied to any line flushed during this call.
    fn print_impl<S>(&self, s: S, timestamp: Option<i64>)
    where
        S: AsRef<str>,
    {
        let mut buf = self.out_buffer.borrow_mut();
        let mut it = s.as_ref().chars();

        while let Some(c) = it.next() {
            if c == '\n' {
                // Flush and clear the line buffer.
                self.output
                    .borrow_mut()
                    .push(buf.iter().copied(), timestamp);
                buf.clear();
            } else {
                buf.push(c);
            }
        }
    }

    pub fn print<S>(&self, s: S)
    where
        S: AsRef<str>,
    {
        self.print_impl(s, None);
    }

    pub fn print_alert<S>(&self, s: S)
    where
        S: AsRef<str>,
    {
        self.print_impl(s, Some(Utc::now().timestamp()));
    }

    pub fn println<S>(&self, s: S)
    where
        S: AsRef<str>,
    {
        self.print_impl(s, None);
        self.print_impl("\n", None);
    }

    pub fn println_alert<S>(&self, s: S)
    where
        S: AsRef<str>,
    {
        let ts = Some(Utc::now().timestamp());
        self.print_impl(s, ts);
        self.print_impl("\n", ts);
    }

    pub fn send_char(&mut self, c: char) {
        match c {
            // ignore grave and escape keys
            '`' | '\x1b' => (),

            '\r' => {
                // cap with a newline and push to the execution buffer
                let mut entered = self.get_string();
                entered.push('\n');
                self.buffer.borrow_mut().push_str(&entered);

                // add the current input to the history
                self.hist.add_line(self.input.get_text());

                // echo the input to console output
                let mut input_echo: Vec<char> = vec![']'];
                input_echo.append(&mut self.input.get_text());
                self.output.borrow_mut().push(input_echo, None);

                // clear the input line
                self.input.clear();
            }

            '\x08' => self.input.backspace(),
            '\x7f' => self.input.delete(),

            '\t' => warn!("Tab completion not implemented"), // TODO: tab completion

            // TODO: we should probably restrict what characters are allowed
            c => self.input.insert(c),
        }
    }

    pub fn cursor(&self) -> usize {
        self.input.curs
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
        let text = self.buffer.replace(String::new());

        let (_remaining, commands) = parse::commands(text.as_str()).unwrap();

        for command in commands.iter() {
            debug!("{:?}", command);
        }

        for args in commands {
            if let Some(arg_0) = args.get(0) {
                let maybe_alias = self.aliases.borrow().get(*arg_0).map(|a| a.to_owned());
                match maybe_alias {
                    Some(a) => {
                        self.stuff_text(a);
                        self.execute();
                    }

                    None => {
                        let tail_args: Vec<&str> =
                            args.iter().map(|s| s.as_ref()).skip(1).collect();

                        if self.cmds.borrow().contains(arg_0) {
                            match self.cmds.borrow_mut().exec(arg_0, &tail_args) {
                                Ok(o) => {
                                    if !o.is_empty() {
                                        self.println(o)
                                    }
                                }
                                Err(e) => self.println(format!("{}", e)),
                            }
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
                                    self.println(msg);
                                }
                            }
                        } else {
                            // TODO: try sending to server first
                            self.println(format!("Unrecognized command \"{}\"", arg_0));
                        }
                    }
                }
            }
        }
    }

    pub fn get_string(&self) -> String {
        String::from_iter(self.input.text.clone().into_iter())
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
