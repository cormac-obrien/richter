// Copyright © 2017 Cormac O'Brien
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

extern crate gfx;
extern crate gfx_window_glutin;
extern crate glutin;
extern crate richter;

use richter::console::CmdRegistry;
use richter::console::Console;
use richter::console::ConsoleInput;
use richter::console::CvarRegistry;
use richter::console::History;
use richter::input::InputFocus;

use glutin::ElementState;
use glutin::Event;
use glutin::KeyboardInput;
use glutin::WindowEvent;
use glutin::VirtualKeyCode;

type ColorFormat = gfx::format::Srgba8;
type DepthFormat = gfx::format::DepthStencil;

fn main() {
    let mut events_loop = glutin::EventsLoop::new();
    let window_builder = glutin::WindowBuilder::new().with_title("BSP renderer: gfx-rs backend");
    let context_builder = glutin::ContextBuilder::new()
        .with_gl(glutin::GlRequest::Specific(glutin::Api::OpenGl, (3, 3)))
        .with_vsync(true);

    let (window, mut device, mut factory, color, depth) =
        gfx_window_glutin::init::<ColorFormat, DepthFormat>(
            window_builder,
            context_builder,
            &events_loop,
        );

    let mut cvars = CvarRegistry::new();
    let mut cmds = CmdRegistry::new();
    let mut con = Console::new();
    let mut cmdline = ConsoleInput::new();

    let mut input_focus = InputFocus::Console;

    let mut quit = false;
    loop {
        match input_focus {
            InputFocus::Console => {
                events_loop.poll_events(|event| match event {
                    Event::WindowEvent {
                        event: e,
                        ..
                    } => match e {
                        WindowEvent::Closed => quit = true,
                        WindowEvent::ReceivedCharacter('`') => input_focus = InputFocus::Game,
                        WindowEvent::ReceivedCharacter(c) => con.send_char(c).unwrap(),
                        WindowEvent::KeyboardInput {
                            input: KeyboardInput {
                                state: ElementState::Pressed,
                                virtual_keycode: Some(k),
                                ..
                            },
                            ..
                        } => con.send_key(k),
                        _ => (),
                    }
                    _ => (),
                });
            }

            InputFocus::Game => {
                events_loop.poll_events(|event| match event {
                    Event::WindowEvent {
                        event: e,
                        ..
                    } => match e {
                        WindowEvent::Closed => quit = true,
                        WindowEvent::ReceivedCharacter('`') => input_focus = InputFocus::Console,
                        WindowEvent::KeyboardInput {
                            input: KeyboardInput {
                                virtual_keycode: Some(k),
                                ..
                            },
                            ..
                        } => match k {
                            _ => (),
                        }
                        _ => (),
                    }
                    _ => (),
                });
            }

            _ => (),
        }

        if quit {
            break;
        }
    }
}
