// Copyright © 2019 Cormac O'Brien
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

use std::{cell::RefCell, rc::Rc};

use crate::{client::menu::Menu, common::console::Console};

use failure::Error;
use winit::event::{ElementState, Event, KeyboardInput, VirtualKeyCode as Key, WindowEvent};

pub struct MenuInput {
    menu: Rc<RefCell<Menu>>,
    console: Rc<RefCell<Console>>,
}

impl MenuInput {
    pub fn new(menu: Rc<RefCell<Menu>>, console: Rc<RefCell<Console>>) -> MenuInput {
        MenuInput { menu, console }
    }

    pub fn handle_event<T>(&self, event: Event<T>) -> Result<(), Error> {
        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::ReceivedCharacter(_) => (),

                WindowEvent::KeyboardInput {
                    input:
                        KeyboardInput {
                            virtual_keycode: Some(key),
                            state: ElementState::Pressed,
                            ..
                        },
                    ..
                } => match key {
                    Key::Escape => {
                        if self.menu.borrow().at_root() {
                            self.console.borrow().stuff_text("togglemenu\n");
                        } else {
                            self.menu.borrow().back()?;
                        }
                    }

                    Key::Up => self.menu.borrow().prev()?,
                    Key::Down => self.menu.borrow().next()?,
                    Key::Return => self.menu.borrow().activate()?,

                    _ => (),
                },

                _ => (),
            },

            _ => (),
        }

        Ok(())
    }
}
