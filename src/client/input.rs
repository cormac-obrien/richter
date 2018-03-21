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

use common::parse;

use std::collections::HashMap;
use std::str::FromStr;

use nom::IResult;
use winit::VirtualKeyCode as Key;
use winit::MouseButton;
use winit::MouseScrollDelta;

lazy_static! {
    pub static ref DEFAULT_BINDINGS: Bindings = {
        let mut binds = Bindings::new();
        binds.bind(Key::W, BindTarget::from_str("+forward").unwrap());
        binds.bind(Key::A, BindTarget::from_str("+moveleft").unwrap());
        binds.bind(Key::S, BindTarget::from_str("+back").unwrap());
        binds.bind(Key::D, BindTarget::from_str("+moveright").unwrap());
        binds.bind(Key::Space, BindTarget::from_str("+jump").unwrap());
        binds.bind(Key::Up, BindTarget::from_str("+lookup").unwrap());
        binds.bind(Key::Left, BindTarget::from_str("+left").unwrap());
        binds.bind(Key::Down, BindTarget::from_str("+lookdown").unwrap());
        binds.bind(Key::LControl, BindTarget::from_str("+attack").unwrap());
        binds.bind(Key::E, BindTarget::from_str("+use").unwrap());
        binds
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Forward,
    Back,
    MoveLeft,
    MoveRight,

    MoveUp,
    MoveDown,

    LookUp,
    LookDown,
    Left,
    Right,

    Speed,
    Jump,
    Strafe,
    Attack,
    Use,

    KLook,
    MLook,

    ShowScores,
    ShowTeamScores,
}

impl FromStr for Action {
    // TODO: implement an error type
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let action = match s.to_lowercase().as_str() {
            "forward" => Action::Forward,
            "back" => Action::Back,
            "moveleft" => Action::MoveLeft,
            "moveright" => Action::MoveRight,
            "moveup" => Action::MoveUp,
            "movedown" => Action::MoveDown,
            "lookup" => Action::LookUp,
            "lookdown" => Action::LookDown,
            "left" => Action::Left,
            "right" => Action::Right,
            "speed" => Action::Speed,
            "jump" => Action::Jump,
            "strafe" => Action::Strafe,
            "attack" => Action::Attack,
            "use" => Action::Use,
            "klook" => Action::KLook,
            "mlook" => Action::MLook,
            "showscores" => Action::ShowScores,
            "showteamscores" => Action::ShowTeamScores,
            _ => return Err(()),
        };

        Ok(action)
    }
}

#[derive(Clone, Debug)]
pub enum BindTarget {
    Action {
        // + is true, - is false
        // so "+forward" maps to active_state: true, action: Action::Forward
        active_state: bool,
        action: Action,
    },

    Other(String),
}

impl FromStr for BindTarget {
    // TODO: implement an error type
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (active_state, action_str) = match parse::action(s.as_bytes()) {
            IResult::Done(_remaining, output) => output,
            IResult::Incomplete(_) => {
                error!("\"{}\" is not a valid action", s);
                return Err(())
            }
            IResult::Error(e) => {
                error!("\"{}\" is not a valid action: {}", s, e);
                return Err(())
            }
        };

        let action = match Action::from_str(action_str) {
            Ok(a) => a,
            Err(err) => {
                // TODO: update when we have a real error type
                error!("Invalid action string");
                return Err(())
            }
        };

        Ok(BindTarget::Action {
            active_state,
            action,
        })
    }
}

#[derive(Clone)]
pub struct Bindings(HashMap<BindInput, BindTarget>);

impl Bindings {
    pub fn new() -> Bindings {
        Bindings(HashMap::new())
    }

    pub fn bind<I, T>(&mut self, input: I, target: T) -> Option<BindTarget>
    where
        I: Into<BindInput>,
        T: Into<BindTarget>,
    {
        self.0.insert(input.into(), target.into())
    }

    pub fn get<I>(&self, input: I) -> Option<&BindTarget>
    where
        I: Into<BindInput>,
    {
        self.0.get(&input.into())
    }
}

pub enum InputFocus {
    Game,
    Console,
    Menu,
}

// we only care about the direction the mouse wheel moved, not how far it went in one event
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MouseWheel {
    Up,
    Down,
}

// TODO: this currently doesn't handle NaN and treats 0.0 as negative which is probably not optimal
impl ::std::convert::From<MouseScrollDelta> for MouseWheel {
    fn from(src: MouseScrollDelta) -> MouseWheel {
        match src {
            MouseScrollDelta::LineDelta(_, y) => {
                if y > 0.0 {
                    MouseWheel::Up
                } else {
                    MouseWheel::Down
                }
            }

            MouseScrollDelta::PixelDelta(_, y) => {
                if y > 0.0 {
                    MouseWheel::Up
                } else {
                    MouseWheel::Down
                }
            }
        }
    }
}

/// A physical input that can be bound to a command.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BindInput {
    Key(Key),
    MouseButton(MouseButton),
    MouseWheel(MouseWheel),
}

impl ::std::convert::From<Key> for BindInput {
    fn from(src: Key) -> BindInput {
        BindInput::Key(src)
    }
}

impl ::std::convert::From<MouseButton> for BindInput {
    fn from(src: MouseButton) -> BindInput {
        BindInput::MouseButton(src)
    }
}

impl ::std::convert::From<MouseScrollDelta> for BindInput {
    fn from(src: MouseScrollDelta) -> BindInput {
        BindInput::MouseWheel(MouseWheel::from(src))
    }
}

pub struct GameInput {
    pub forward: bool,
    pub back: bool,
    pub moveleft: bool,
    pub moveright: bool,

    pub moveup: bool,
    pub movedown: bool,

    pub left: bool,
    pub right: bool,
    pub lookup: bool,
    pub lookdown: bool,

    pub speed: bool,
    pub jump: bool,
    pub strafe: bool,
    pub attack: bool,
    pub use_: bool,

    pub klook: bool,
    pub mlook: bool,

    pub showscores: bool,
    pub showteamscores: bool,
}

impl GameInput {
    pub fn new() -> Self {
        GameInput {
            forward: false,
            back: false,
            moveleft: false,
            moveright: false,

            moveup: false,
            movedown: false,

            left: false,
            right: false,
            lookup: false,
            lookdown: false,

            speed: false,
            jump: false,
            strafe: false,
            attack: false,
            use_: false,

            klook: false,
            mlook: false,

            showscores: false,
            showteamscores: false,
        }
    }

    pub fn handle_action(&mut self, action: Action, state: bool) {
        match action {
            Action::Forward => self.forward = state,
            Action::Back => self.back = state,
            Action::MoveLeft => self.moveleft = state,
            Action::MoveRight => self.moveright = state,

            Action::MoveUp => self.moveup = state,
            Action::MoveDown => self.movedown = state,

            Action::LookUp => self.lookup = state,
            Action::LookDown => self.lookdown = state,
            Action::Left => self.left = state,
            Action::Right => self.right = state,

            Action::Speed => self.speed = state,
            Action::Jump => self.jump = state,
            Action::Strafe => self.strafe = state,
            Action::Attack => self.attack = state,
            Action::Use => self.use_ = state,

            Action::KLook => self.klook = state,
            Action::MLook => self.mlook = state,

            Action::ShowScores => self.showscores = state,
            Action::ShowTeamScores => self.showteamscores = state,
        }
    }
}

pub fn get_input_by_name<S>(name: S) -> Option<BindInput>
where
    S: AsRef<str>,
{
    match name.as_ref().to_uppercase().as_ref() {
        "0" => Some(BindInput::Key(Key::Key0)),
        "1" => Some(BindInput::Key(Key::Key1)),
        "2" => Some(BindInput::Key(Key::Key2)),
        "3" => Some(BindInput::Key(Key::Key3)),
        "4" => Some(BindInput::Key(Key::Key4)),
        "5" => Some(BindInput::Key(Key::Key5)),
        "6" => Some(BindInput::Key(Key::Key6)),
        "7" => Some(BindInput::Key(Key::Key7)),
        "8" => Some(BindInput::Key(Key::Key8)),
        "9" => Some(BindInput::Key(Key::Key9)),
        "A" => Some(BindInput::Key(Key::A)),
        "ALT" => Some(BindInput::Key(Key::LAlt)),
        "B" => Some(BindInput::Key(Key::B)),
        "BACKSPACE" => Some(BindInput::Key(Key::Back)),
        "C" => Some(BindInput::Key(Key::C)),
        "CTRL" => Some(BindInput::Key(Key::LControl)),
        "D" => Some(BindInput::Key(Key::D)),
        "DEL" => Some(BindInput::Key(Key::Delete)),
        "DOWN" => Some(BindInput::Key(Key::Down)),
        "E" => Some(BindInput::Key(Key::E)),
        "END" => Some(BindInput::Key(Key::End)),
        "ENTER" => Some(BindInput::Key(Key::Return)),
        "ESCAPE" => Some(BindInput::Key(Key::Escape)),
        "F" => Some(BindInput::Key(Key::F)),
        "F1" => Some(BindInput::Key(Key::F1)),
        "F10" => Some(BindInput::Key(Key::F10)),
        "F11" => Some(BindInput::Key(Key::F11)),
        "F12" => Some(BindInput::Key(Key::F12)),
        "F2" => Some(BindInput::Key(Key::F2)),
        "F3" => Some(BindInput::Key(Key::F3)),
        "F4" => Some(BindInput::Key(Key::F4)),
        "F5" => Some(BindInput::Key(Key::F5)),
        "F6" => Some(BindInput::Key(Key::F6)),
        "F7" => Some(BindInput::Key(Key::F7)),
        "F8" => Some(BindInput::Key(Key::F8)),
        "F9" => Some(BindInput::Key(Key::F9)),
        "G" => Some(BindInput::Key(Key::G)),
        "H" => Some(BindInput::Key(Key::H)),
        "HOME" => Some(BindInput::Key(Key::Home)),
        "I" => Some(BindInput::Key(Key::I)),
        "INS" => Some(BindInput::Key(Key::Insert)),
        "J" => Some(BindInput::Key(Key::J)),
        "K" => Some(BindInput::Key(Key::K)),
        "L" => Some(BindInput::Key(Key::L)),
        "LEFTARROW" => Some(BindInput::Key(Key::Left)),
        "M" => Some(BindInput::Key(Key::M)),
        "MOUSE1" => Some(BindInput::MouseButton(MouseButton::Left)),
        "MOUSE2" => Some(BindInput::MouseButton(MouseButton::Right)),
        "MOUSE3" => Some(BindInput::MouseButton(MouseButton::Middle)),
        "MWHEELDOWN" => Some(BindInput::MouseWheel(MouseWheel::Down)),
        "MWHEELUP" => Some(BindInput::MouseWheel(MouseWheel::Up)),
        "N" => Some(BindInput::Key(Key::N)),
        "O" => Some(BindInput::Key(Key::O)),
        "P" => Some(BindInput::Key(Key::P)),
        "PGDN" => Some(BindInput::Key(Key::PageDown)),
        "PGUP" => Some(BindInput::Key(Key::PageUp)),
        "Q" => Some(BindInput::Key(Key::Q)),
        "R" => Some(BindInput::Key(Key::R)),
        "RIGHTARROW" => Some(BindInput::Key(Key::Right)),
        "S" => Some(BindInput::Key(Key::S)),
        "SEMICOLON" => Some(BindInput::Key(Key::Semicolon)),
        "SHIFT" => Some(BindInput::Key(Key::LShift)),
        "SPACE" => Some(BindInput::Key(Key::Space)),
        "T" => Some(BindInput::Key(Key::T)),
        "TAB" => Some(BindInput::Key(Key::Tab)),
        "U" => Some(BindInput::Key(Key::U)),
        "UPARROW" => Some(BindInput::Key(Key::Up)),
        "V" => Some(BindInput::Key(Key::V)),
        "W" => Some(BindInput::Key(Key::W)),
        "X" => Some(BindInput::Key(Key::X)),
        "Y" => Some(BindInput::Key(Key::Y)),
        "Z" => Some(BindInput::Key(Key::Z)),
        _ => None,
    }
}
