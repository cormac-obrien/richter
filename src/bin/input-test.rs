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

extern crate chrono;
extern crate env_logger;
#[macro_use]
extern crate log;
extern crate richter;
extern crate winit;

use richter::client::input::BindInput;
use richter::client::input::BindTarget;
use richter::client::input::DEFAULT_BINDINGS;
use richter::client::input::GameInput;
use richter::client::input::MouseWheel;
use richter::common::console::CmdRegistry;
use richter::common::console::CvarRegistry;
use winit::ElementState;
use winit::Event;
use winit::EventsLoop;
use winit::KeyboardInput;
use winit::WindowBuilder;
use winit::WindowEvent;
use winit::VirtualKeyCode;

fn main() {
    env_logger::init();

    let bindings = DEFAULT_BINDINGS.clone();
    let mut game_input = GameInput::new();
    let mut cmd_registry = CmdRegistry::new();
    let mut cvar_registry = CvarRegistry::new();

    let mut events_loop = EventsLoop::new();

    let window = WindowBuilder::new()
        .with_title("richter input test")
        .with_dimensions(1366, 768)
        .build(&events_loop)
        .unwrap();

    let mut quit = false;
    loop {
        // there's no release event for the mousewheel, so send a release for both scroll directions
        // at the beginning of every frame
        bindings.handle(&mut game_input, &mut cmd_registry, &mut cvar_registry, MouseWheel::Up, ElementState::Released);
        bindings.handle(&mut game_input, &mut cmd_registry, &mut cvar_registry, MouseWheel::Down, ElementState::Released);

        events_loop.poll_events(|event| match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::Closed => quit = true,

                WindowEvent::KeyboardInput {
                    input:
                        KeyboardInput {
                            state,
                            virtual_keycode: Some(key),
                            ..
                        },
                    ..
                } => {
                    bindings.handle(&mut game_input, &mut cmd_registry, &mut cvar_registry, key, state);
                }

                WindowEvent::MouseInput { state, button, .. } => {
                    bindings.handle(&mut game_input, &mut cmd_registry, &mut cvar_registry, button, state);
                }

                WindowEvent::MouseWheel { delta, .. } => {
                    bindings.handle(&mut game_input, &mut cmd_registry, &mut cvar_registry, delta, ElementState::Pressed);
                }

                _ => (),
            },

            _ => (),
        });

        if quit {
            break;
        }
    }
}
