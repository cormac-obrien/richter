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
use winit::Event;
use winit::EventsLoop;
use winit::KeyboardInput;
use winit::WindowBuilder;
use winit::WindowEvent;

fn main() {
    env_logger::init();

    let mut events_loop = EventsLoop::new();

    let window = WindowBuilder::new()
        .with_title("richter input test")
        .with_dimensions(1366, 768)
        .build(&events_loop)
        .unwrap();

    let mut quit = false;
    loop {
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
                    let input = BindInput::from(key);
                    println!("{:?}: {:?}", input, state);
                }

                WindowEvent::MouseInput { state, button, .. } => {
                    let input = BindInput::from(button);
                    println!("{:?}: {:?}", input, state);
                }

                WindowEvent::MouseWheel { delta, .. } => {
                    let input = BindInput::from(delta);
                    println!("{:?}", input);
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
