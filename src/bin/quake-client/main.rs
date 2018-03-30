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
extern crate gfx;
extern crate gfx_device_gl;
extern crate gfx_window_glutin;
extern crate glutin;
extern crate richter;
extern crate rodio;

use std::cell::RefCell;
use std::env;
use std::net::ToSocketAddrs;
use std::path::Path;
use std::process::exit;
use std::rc::Rc;

use richter::client;
use richter::client::Client;
use richter::client::input::Bindings;
use richter::client::input::GameInput;
use richter::client::input::MouseWheel;
use richter::client::render;
use richter::common;
use richter::common::console::CmdRegistry;
use richter::common::console::CvarRegistry;
use richter::common::host::Host;
use richter::common::host::Program;
use richter::common::net::SignOnStage;
use richter::common::pak::Pak;

use chrono::Duration;
use glutin::ElementState;
use glutin::Event;
use glutin::EventsLoop;
use glutin::GlWindow;
use glutin::KeyboardInput;
use glutin::WindowEvent;
use rodio::Endpoint;

struct ClientProgram {
    pak: Rc<Pak>,
    cvars: Rc<RefCell<CvarRegistry>>,
    cmds: Rc<RefCell<CmdRegistry>>,

    events_loop: RefCell<EventsLoop>,
    window: RefCell<GlWindow>,
    bindings: Rc<RefCell<Bindings>>,
    endpoint: Rc<Endpoint>,

    client: Option<RefCell<Client>>,
}

impl ClientProgram {
    pub fn new() -> ClientProgram {
        let mut pak = Pak::new();
        for pak_id in 0..common::MAX_PAKFILES {
            // TODO: check `-basedir` command line argument
            let basedir = common::DEFAULT_BASEDIR;
            let path_string = format!("{}/pak{}.pak", basedir, pak_id);
            let path = Path::new(&path_string);

            // keep adding PAKs until we don't find one or we hit MAX_PAKFILES
            if !path.exists() {
                break;
            }

            pak.add(path).unwrap();
        }

        let mut cvars = Rc::new(RefCell::new(CvarRegistry::new()));
        client::register_cvars(&cvars.borrow_mut());

        let mut cmds = Rc::new(RefCell::new(CmdRegistry::new()));
        // TODO: register commands as other subsystems come online

        let mut bindings = Rc::new(RefCell::new(Bindings::new(cvars.clone(), cmds.clone())));
        bindings.borrow_mut().assign_defaults();

        let mut events_loop = glutin::EventsLoop::new();
        let window_builder = glutin::WindowBuilder::new()
            .with_title("Richter client")
            .with_dimensions(1366, 768);
        let context_builder = glutin::ContextBuilder::new()
            .with_gl(glutin::GlRequest::Specific(glutin::Api::OpenGl, (3, 3)))
            .with_vsync(true);

        let (window, mut device, mut factory, color, depth) =
            gfx_window_glutin::init::<render::ColorFormat, render::DepthFormat>(
                window_builder,
                context_builder,
                &events_loop,
            );

        let endpoint = Rc::new(rodio::get_endpoints_list().next().unwrap());

        ClientProgram {
            pak: Rc::new(pak),
            cvars,
            cmds,
            events_loop: RefCell::new(events_loop),
            window: RefCell::new(window),
            bindings,
            endpoint,
            client: None,
        }
    }

    fn connect<A>(&mut self, server_addrs: A) where A: ToSocketAddrs {
        self.client = Some(RefCell::new(
            Client::connect(server_addrs, self.pak.clone(), self.cvars.clone(), self.endpoint.clone()).unwrap()));
    }
}

impl Program for ClientProgram {
    fn frame(&mut self, frame_duration: Duration) {
        // may have to take ownership of the client here
        if let Some(ref client) = self.client {
            client.borrow_mut().parse_server_msg().unwrap();

            if client.borrow().get_signon_stage() == SignOnStage::Done {
                let mut actions = GameInput::new();
                self.bindings.borrow().handle(
                    &mut actions,
                    MouseWheel::Up,
                    ElementState::Released,
                );
                self.bindings.borrow().handle(
                    &mut actions,
                    MouseWheel::Down,
                    ElementState::Released,
                );

                self.events_loop.borrow_mut().poll_events(|event| match event {
                    Event::WindowEvent { event, .. } => match event {
                        WindowEvent::Closed => {
                            // TODO: handle quit properly
                            unimplemented!();
                        }

                        WindowEvent::KeyboardInput {
                            input:
                                KeyboardInput {
                                    state,
                                    virtual_keycode: Some(key),
                                    ..
                                },
                            ..
                        } => {
                            self.bindings.borrow().handle(&mut actions, key, state);
                        }

                        WindowEvent::MouseInput { state, button, .. } => {
                            self.bindings.borrow().handle(&mut actions, button, state);
                        }

                        WindowEvent::MouseWheel { delta, .. } => {
                            self.bindings.borrow().handle(&mut actions, delta, ElementState::Pressed);
                        }

                        _ => (),
                    },

                    _ => (),
                });
                client
                    .borrow_mut()
                    .handle_input(&actions, frame_duration, 0)
                    .unwrap();
            }

            client.borrow_mut().send().unwrap();
        }
    }
}

fn main() {
    env_logger::init();

    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        println!("Usage: {} <server_address>", args[0]);
        exit(1);
    }

    let mut client_program = ClientProgram::new();
    client_program.connect(&args[1]);
    let mut host = Host::new(client_program);

    let mut quit = false;
    loop {
        host.frame();
    }
}
