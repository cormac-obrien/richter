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
pub mod game;
pub mod menu;

use std::{cell::RefCell, rc::Rc};

use crate::{
    client::menu::Menu,
    common::console::{CmdRegistry, Console},
};

use failure::Error;
use winit::event::{Event, WindowEvent};

use self::{
    console::ConsoleInput,
    game::{BindInput, BindTarget, GameInput},
    menu::MenuInput,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputFocus {
    Game,
    Console,
    Menu,
}

pub struct Input {
    window_focused: bool,
    focus: InputFocus,

    game_input: GameInput,
    console_input: ConsoleInput,
    menu_input: MenuInput,
}

impl Input {
    pub fn new(
        init_focus: InputFocus,
        console: Rc<RefCell<Console>>,
        menu: Rc<RefCell<Menu>>,
    ) -> Input {
        Input {
            window_focused: true,
            focus: init_focus,

            game_input: GameInput::new(console.clone()),
            console_input: ConsoleInput::new(console.clone()),
            menu_input: MenuInput::new(menu, console),
        }
    }

    pub fn handle_event<T>(&mut self, event: Event<T>) -> Result<(), Error> {
        match event {
            // we're polling for hardware events, so we have to check window focus ourselves
            Event::WindowEvent {
                event: WindowEvent::Focused(focused),
                ..
            } => self.window_focused = focused,

            _ => {
                if self.window_focused {
                    match self.focus {
                        InputFocus::Game => self.game_input.handle_event(event),
                        InputFocus::Console => self.console_input.handle_event(event)?,
                        InputFocus::Menu => self.menu_input.handle_event(event)?,
                    }
                }
            }
        }

        Ok(())
    }

    pub fn focus(&self) -> InputFocus {
        self.focus
    }

    pub fn set_focus(&mut self, new_focus: InputFocus) {
        self.focus = new_focus;
    }

    /// Bind a `BindInput` to a `BindTarget`.
    pub fn bind<I, T>(&mut self, input: I, target: T) -> Option<BindTarget>
    where
        I: Into<BindInput>,
        T: Into<BindTarget>,
    {
        self.game_input.bind(input, target)
    }

    pub fn bind_defaults(&mut self) {
        self.game_input.bind_defaults();
    }

    pub fn game_input(&self) -> Option<&GameInput> {
        if let InputFocus::Game = self.focus {
            Some(&self.game_input)
        } else {
            None
        }
    }

    pub fn game_input_mut(&mut self) -> Option<&mut GameInput> {
        if let InputFocus::Game = self.focus {
            Some(&mut self.game_input)
        } else {
            None
        }
    }

    pub fn register_cmds(&self, cmds: &mut CmdRegistry) {
        self.game_input.register_cmds(cmds);
    }
}
