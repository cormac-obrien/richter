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

use std::cmp::Ordering;
use std::collections::VecDeque;

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

type CmdFn = fn(Vec<&str>);

struct Cmd {
    name: String,
    cmd: CmdFn,
}

impl PartialEq for Cmd {
    fn eq(&self, other: &Self) -> bool {
        self.name.eq(&other.name)
    }
}

impl Eq for Cmd {}

impl PartialOrd for Cmd {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.name.partial_cmp(&other.name)
    }
}

impl Ord for Cmd {
    fn cmp(&self, other: &Self) -> Ordering {
        self.name.cmp(&other.name)
    }
}

pub struct Console {
    cvars: Vec<Cvar>,
    cmds: Vec<Cmd>,

    hist: Vec<String>,
}

impl Console {
    pub fn new() -> Console {
        Console {
            cvars: Vec::new(),
            cmds: Vec::new(),
            hist: Vec::new(),
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

    pub fn add_cmd<S>(&mut self, name: S, cmd: CmdFn) -> Result<(), ()>
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
}
