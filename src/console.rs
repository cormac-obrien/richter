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

use glium::glutin::VirtualKeyCode as Key;
use std::cmp::Ordering;
use std::collections::VecDeque;
use std::iter::FromIterator;

/// A configuration variable.
///
/// Cvars are the primary method of configuring the game.
struct Cvar {
    // Name of this variable
    name: String,

    // Value of this variable
    val: String,

    // If true, this variable should be archived in vars.rc
    archive: bool,

    // If true, updating this variable must also update serverinfo/userinfo
    info: bool,

    default_val: String,
}

impl PartialEq for Cvar {
    fn eq(&self, other: &Self) -> bool {
        self.name.eq(&other.name)
    }
}

impl Eq for Cvar {}

impl PartialOrd for Cvar {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.name.partial_cmp(&other.name)
    }
}

impl Ord for Cvar {
    fn cmp(&self, other: &Self) -> Ordering {
        self.name.cmp(&other.name)
    }
}

pub struct CmdRegistry<'a> {
    cmd_names: Vec<String>,
    cmds: Vec<Box<Fn() + 'a>>,
}

impl<'a> CmdRegistry<'a> {
    pub fn new() -> CmdRegistry<'a> {
        CmdRegistry {
            cmd_names: Vec::new(),
            cmds: Vec::new(),
        }
    }

    /// Registers a new command.
    pub fn add_cmd<S>(&mut self, name: S, cmd: Box<Fn() + 'a>) -> Result<(), ()>
        where S: AsRef<str>
    {
        let name = name.as_ref().to_owned();

        match self.cmd_names.binary_search(&name) {
            Ok(_) => return Err(()),
            Err(n) => {
                self.cmd_names.insert(n, name);
                self.cmds.insert(n, cmd);
            }
        }

        Ok(())
    }

    /// Executes a command.
    pub fn exec_cmd<S>(&mut self, name: S, args: Vec<&str>) -> Result<(), ()>
        where S: AsRef<str>
    {
        let name = name.as_ref().to_owned();

        match self.cmd_names.binary_search(&name) {
            Ok(c) => {
                debug!("Executing {}", name);
                (&self.cmds[c])()
            }
            Err(_) => return Err(()),
        }

        Ok(())
    }
}

pub struct InputLine {
    text: Vec<char>,
    curs: usize,
}

impl InputLine {
    pub fn new() -> InputLine {
        InputLine {
            text: Vec::new(),
            curs: 0,
        }
    }

    pub fn get_text(&self) -> Vec<char> {
        self.text.to_owned()
    }

    pub fn set_text(&mut self, text: &Vec<char>) {
        self.text = text.clone()
    }

    pub fn insert(&mut self, c: char) {
        self.text.insert(self.curs, c);
        self.cursor_right();
    }

    pub fn cursor_right(&mut self) {
        if self.curs < self.text.len() {
            self.curs += 1;
        }
    }

    pub fn cursor_left(&mut self) {
        if self.curs > 0 {
            self.curs -= 1;
        }
    }

    pub fn delete(&mut self) {
        if self.curs < self.text.len() {
            self.text.remove(self.curs);
        }
    }

    pub fn backspace(&mut self) {
        if self.curs > 0 {
            self.text.remove(self.curs - 1);
            self.curs -= 1;
        }
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.curs = 0;
    }

    pub fn debug_string(&self) -> String {
        format!("{}_{}",
                String::from_iter(self.text[..self.curs].to_owned().into_iter()),
                String::from_iter(self.text[self.curs..].to_owned().into_iter()))
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
        where S: AsRef<str>
    {
        println!("{}", msg.as_ref());
    }
}

pub struct Console<'a> {
    cvars: Vec<Cvar>,
    line: InputLine,
    hist: History,
    cmds: CmdRegistry<'a>,
}

impl<'a> Console<'a> {
    pub fn new() -> Console<'a> {
        Console {
            cvars: Vec::new(),
            line: InputLine::new(),
            hist: History::new(),
            cmds: CmdRegistry::new(),
        }
    }

    /// Registers a new configuration variable.
    pub fn add_cvar<S>(&mut self, name: S, default: S, archive: bool, info: bool) -> Result<(), ()>
        where S: AsRef<str>
    {
        let new_cvar = Cvar {
            name: name.as_ref().to_owned(),
            val: default.as_ref().to_owned(),
            default_val: default.as_ref().to_owned(),
            archive: archive,
            info: info,
        };

        match self.cvars.binary_search(&new_cvar) {
            Ok(_) => return Err(()),
            Err(n) => self.cvars.insert(n, new_cvar),
        }

        Ok(())
    }

    /// Sets the value of a configuration variable.
    pub fn set_cvar<S>(&mut self, name: S, val: S) -> Result<(), ()>
        where S: AsRef<str>
    {
        let search_cvar = Cvar {
            name: name.as_ref().to_owned(),
            val: "".to_owned(),
            default_val: "".to_owned(),
            archive: false,
            info: false,
        };

        match self.cvars.binary_search(&search_cvar) {
            Ok(n) => {
                let _ = ::std::mem::replace::<String>(&mut self.cvars[n].val,
                                                      val.as_ref().to_owned());
            }
            Err(_) => return Err(()),
        }

        Ok(())
    }

    /// Retrieves the value of a configuration variable.
    pub fn get_cvar<S>(&self, name: S) -> Option<&str>
        where S: AsRef<str>
    {
        let search_cvar = Cvar {
            name: name.as_ref().to_owned(),
            val: "".to_owned(),
            default_val: "".to_owned(),
            archive: false,
            info: false,
        };

        match self.cvars.binary_search(&search_cvar) {
            Ok(n) => Some(&self.cvars[n].val),
            Err(_) => None,
        }
    }

    pub fn send_char(&mut self, reg: &mut CmdRegistry, c: char) -> Result<(), ()> {
        match c {
            '\r' => {
                let entered = self.get_string();
                let mut parts = entered.split_whitespace();

                let cmd_name = match parts.next() {
                    Some(c) => c,
                    None => return Ok(()),
                };

                let args: Vec<&str> = parts.collect();

                // match reg.exec_cmd(self, cmd_name, args) {
                // Ok(_) => (),
                // Err(_) => println!("Unknown command \"{}\"", cmd_name),
                // }
                //

                self.hist.add_line(self.line.get_text());
                self.line.clear();
            }

            // backspace
            '\x08' => self.line.backspace(),

            // delete
            '\x7f' => self.line.delete(),

            '\t' => (), // TODO: tab completion

            c => self.line.insert(c),
        }

        println!("{}", self.debug_string());

        Ok(())
    }

    pub fn send_key(&mut self, key: Key) {
        match key {
            Key::Up => self.line.set_text(&self.hist.line_up().unwrap()),
            Key::Down => self.line.set_text(&self.hist.line_down().unwrap()),
            Key::Right => self.line.cursor_right(),
            Key::Left => self.line.cursor_left(),
            _ => (),
        }

        println!("{}", self.debug_string());
    }


    fn get_string(&self) -> String {
        String::from_iter(self.line.text.clone().into_iter())
    }

    fn debug_string(&self) -> String {
        format!("{}_{}",
                String::from_iter(self.line.text[..self.line.curs].to_owned().into_iter()),
                String::from_iter(self.line.text[self.line.curs..].to_owned().into_iter()))
    }
}
