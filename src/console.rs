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

type CmdFn<'a> = Box<FnMut() + 'a>;

/// A console command.
///
/// Console commands are not restricted only to the console. They can also be bound to keys and
/// composed into scripts.
struct Cmd<'a> {
    name: String,
    cmd: CmdFn<'a>,
}

impl<'a> PartialEq for Cmd<'a> {
    fn eq(&self, other: &Self) -> bool {
        self.name.eq(&other.name)
    }
}

impl<'a> Eq for Cmd<'a> {}

impl<'a> PartialOrd for Cmd<'a> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.name.partial_cmp(&other.name)
    }
}

impl<'a> Ord for Cmd<'a> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.name.cmp(&other.name)
    }
}

/// The editable command line of the console.
pub struct CmdLine {
    text: Vec<char>,
    curs: usize,
}

impl CmdLine {
    fn new() -> CmdLine {
        CmdLine {
            text: Vec::new(),
            curs: 0,
        }
    }

    fn insert(&mut self, c: char) {
        self.text.insert(self.curs, c);
        self.cursor_right();
    }

    fn cursor_right(&mut self) {
        if self.curs < self.text.len() {
            self.curs += 1;
        }
    }

    fn cursor_left(&mut self) {
        if self.curs > 0 {
            self.curs -= 1;
        }
    }

    fn delete(&mut self) {
        if self.curs < self.text.len() {
            self.text.remove(self.curs);
        }
    }

    fn backspace(&mut self) {
        if self.curs > 0 {
            self.text.remove(self.curs - 1);
            self.curs -= 1;
        }
    }

    fn get_string(&self) -> String {
        String::from_iter(self.text.clone().into_iter())
    }

    fn debug_string(&self) -> String {
        format!("{}_{}",
                String::from_iter(self.text[..self.curs].to_owned().into_iter()),
                String::from_iter(self.text[self.curs..].to_owned().into_iter()))
    }
}

pub struct Console<'a> {
    cvars: Vec<Cvar>,
    cmds: Vec<Cmd<'a>>,

    hist: VecDeque<String>,
    cmdline: CmdLine,
}

impl<'a> Console<'a> {
    pub fn new() -> Console<'a> {
        Console {
            cvars: Vec::new(),
            cmds: Vec::new(),
            hist: VecDeque::new(),
            cmdline: CmdLine::new(),
        }
    }

    pub fn add_cvar<S>(&mut self,
                       name: S,
                       default_val: S,
                       archive: bool,
                       info: bool)
                       -> Result<(), ()>
        where S: AsRef<str>
    {
        let new_cvar = Cvar {
            name: name.as_ref().to_owned(),
            val: default_val.as_ref().to_owned(),
            default_val: default_val.as_ref().to_owned(),
            archive: archive,
            info: info,
        };

        match self.cvars.binary_search(&new_cvar) {
            Ok(_) => return Err(()),
            Err(n) => self.cvars.insert(n, new_cvar),
        }

        Ok(())
    }

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

    pub fn add_cmd<S>(&mut self, name: S, cmd: CmdFn<'a>) -> Result<(), ()>
        where S: AsRef<str>
    {
        let new_cmd = Cmd {
            name: name.as_ref().to_owned(),
            cmd: cmd,
        };

        match self.cmds.binary_search(&new_cmd) {
            Ok(_) => return Err(()),
            Err(n) => self.cmds.insert(n, new_cmd),
        }

        Ok(())
    }

    pub fn exec_cmd<S>(&mut self, name: S, args: Vec<&str>) -> Result<(), ()>
        where S: AsRef<str>
    {
        let search_cmd = Cmd {
            name: name.as_ref().to_owned(),
            cmd: Box::new(|| ()),
        };

        match self.cmds.binary_search(&search_cmd) {
            Ok(c) => {
                let mut cmdfn = &mut self.cmds[c].cmd;
                (cmdfn)()
            }
            Err(_) => return Err(()),
        }

        Ok(())
    }

    pub fn put_char(&mut self, c: char) -> Result<(), ()> {
        match c {
            '\r' => {
                let entered = self.cmdline.get_string();
                let mut parts = entered.split_whitespace();

                let cmd_name = match parts.next() {
                    Some(c) => c,
                    None => return Ok(()),
                };

                let args: Vec<&str> = parts.collect();
            }

            // backspace
            '\x08' => self.cmdline.backspace(),

            // delete
            '\x7f' => self.cmdline.delete(),

            '\t' => (), // TODO: tab completion

            c => self.cmdline.insert(c),
        }

        println!("{}", self.cmdline.debug_string());

        Ok(())
    }

    pub fn send_key(&mut self, key: Key) {
        match key {
            Key::Right => self.cmdline.cursor_right(),
            Key::Left => self.cmdline.cursor_left(),
            _ => (),
        }

        println!("{}", self.cmdline.debug_string());
    }
}
