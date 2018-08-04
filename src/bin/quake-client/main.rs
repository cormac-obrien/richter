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

extern crate cgmath;
extern crate chrono;
extern crate env_logger;
extern crate failure;
extern crate flame;
extern crate gfx;
extern crate gfx_device_gl;
extern crate gfx_window_glutin;
extern crate glutin;
extern crate richter;
extern crate rodio;

mod game;

use std::cell::RefCell;
use std::env;
use std::fs::File;
use std::net::ToSocketAddrs;
use std::path::Path;
use std::process::exit;
use std::rc::Rc;

use richter::client::input::game::MouseWheel;
use richter::client::input::{Input, InputFocus};
use richter::client::render::{self, GraphicsPackage};
use richter::client::{self, Client};
use richter::common;
use richter::common::console::{CmdRegistry, Console, CvarRegistry};
use richter::common::host::{Host, Program};
use richter::common::pak::Pak;

use game::Game;

use cgmath::{Matrix4, SquareMatrix};
use chrono::Duration;
use gfx::Encoder;
use gfx_device_gl::{CommandBuffer, Device, Resources};
use glutin::{CursorState, Event, EventsLoop, GlContext, GlWindow, MouseCursor, WindowEvent};
use rodio::Endpoint;

enum TitleState {
    Menu,
    Console,
}

enum ProgramState {
    Title,
    Game(Game),
}

struct ClientProgram {
    pak: Rc<Pak>,
    cvars: Rc<RefCell<CvarRegistry>>,
    cmds: Rc<RefCell<CmdRegistry>>,
    console: Rc<RefCell<Console>>,

    events_loop: RefCell<EventsLoop>,
    window: RefCell<GlWindow>,

    gfx_pkg: Rc<RefCell<GraphicsPackage>>,
    device: RefCell<Device>,
    encoder: RefCell<Encoder<Resources, CommandBuffer>>,
    data: RefCell<render::pipe::Data<Resources>>,

    endpoint: Rc<Endpoint>,

    state: RefCell<ProgramState>,
    input: Rc<RefCell<Input>>,
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

        let cvars = Rc::new(RefCell::new(CvarRegistry::new()));
        client::register_cvars(&cvars.borrow_mut());

        let cmds = Rc::new(RefCell::new(CmdRegistry::new()));
        // TODO: register commands as other subsystems come online

        let console = Rc::new(RefCell::new(Console::new(cmds.clone(), cvars.clone())));

        let input = Rc::new(RefCell::new(Input::new(InputFocus::Game, console.clone())));
        input.borrow_mut().bind_defaults();

        let events_loop = glutin::EventsLoop::new();
        let window_builder = glutin::WindowBuilder::new()
            .with_title("Richter client")
            .with_dimensions(1600, 900);
        let context_builder = glutin::ContextBuilder::new()
            .with_gl(glutin::GlRequest::Specific(glutin::Api::OpenGl, (3, 3)))
            .with_vsync(false);

        let (window, device, mut factory, color, depth) =
            gfx_window_glutin::init::<render::ColorFormat, render::DepthFormat>(
                window_builder,
                context_builder,
                &events_loop,
            );

        use gfx::traits::FactoryExt;
        use gfx::Factory;
        let (_, dummy_texture) = factory
            .create_texture_immutable_u8::<render::ColorFormat>(
                gfx::texture::Kind::D2(0, 0, gfx::texture::AaMode::Single),
                gfx::texture::Mipmap::Allocated,
                &[&[]],
            )
            .expect("dummy texture generation failed");

        let sampler = factory.create_sampler(gfx::texture::SamplerInfo::new(
            gfx::texture::FilterMethod::Scale,
            gfx::texture::WrapMode::Tile,
        ));

        let data = render::pipe::Data {
            vertex_buffer: factory.create_vertex_buffer(&[]),
            transform: Matrix4::identity().into(),
            sampler: (dummy_texture.clone(), sampler.clone()),
            out_color: color.clone(),
            out_depth: depth.clone(),
        };

        let encoder = factory.create_command_buffer().into();

        let endpoint = Rc::new(rodio::get_endpoints_list().next().unwrap());

        let gfx_pkg = Rc::new(RefCell::new(GraphicsPackage::new(
            &pak,
            factory,
            color,
            depth,
            console.clone(),
        )));

        ClientProgram {
            pak: Rc::new(pak),
            cvars,
            cmds,
            console,
            events_loop: RefCell::new(events_loop),
            window: RefCell::new(window),
            gfx_pkg,
            device: RefCell::new(device),
            encoder: RefCell::new(encoder),
            data: RefCell::new(data),
            endpoint,
            state: RefCell::new(ProgramState::Title),
            input,
        }
    }

    fn connect<A>(&mut self, server_addrs: A)
    where
        A: ToSocketAddrs,
    {
        let cl = Client::connect(
            server_addrs,
            self.pak.clone(),
            self.cvars.clone(),
            self.cmds.clone(),
            self.console.clone(),
            self.endpoint.clone(),
        ).unwrap();

        cl.register_cmds(&mut self.cmds.borrow_mut());

        self.state.replace(ProgramState::Game(
            Game::new(
                self.pak.clone(),
                self.cvars.clone(),
                self.cmds.clone(),
                self.gfx_pkg.clone(),
                self.input.clone(),
                cl,
            ).unwrap(),
        ));
    }

    fn render(&mut self) {
        self.encoder
            .borrow_mut()
            .clear(&self.gfx_pkg.borrow().color_target(), [0.0, 0.0, 0.0, 1.0]);
        self.encoder
            .borrow_mut()
            .clear_depth(&self.gfx_pkg.borrow().depth_stencil(), 1.0);
        let (win_w, win_h) = self.window.borrow().get_inner_size().unwrap();

        match *self.state.borrow_mut() {
            ProgramState::Title => unimplemented!(),
            ProgramState::Game(ref mut game) => {
                game.render(
                    &mut self.encoder.borrow_mut(),
                    &mut self.data.borrow_mut(),
                    win_w,
                    win_h,
                );
            }
        }

        use std::ops::DerefMut;
        flame::start("Encoder::flush");
        self.encoder
            .borrow_mut()
            .flush(self.device.borrow_mut().deref_mut());
        flame::end("Encoder::flush");

        flame::start("Window::swap_buffers");
        self.window.borrow_mut().swap_buffers().unwrap();
        flame::end("Window::swap_buffers");

        use gfx::Device;
        flame::start("Device::cleanup");
        self.device.borrow_mut().cleanup();
        flame::end("Device::cleanup");
    }
}

impl Program for ClientProgram {
    fn frame(&mut self, frame_duration: Duration) {
        let _guard = flame::start_guard("ClientProgram::frame");
        match *self.state.borrow_mut() {
            ProgramState::Title => unimplemented!(),

            ProgramState::Game(ref mut game) => {
                game.frame(frame_duration);
            }
        }

        if let Some(ref mut game_input) = self.input.borrow_mut().game_input_mut() {
            game_input.clear_mouse().unwrap();
        }

        flame::start("EventsLoop::poll_events");
        self.events_loop
            .borrow_mut()
            .poll_events(|event| match event {
                Event::WindowEvent {
                    event: WindowEvent::Closed,
                    ..
                } => {
                    // TODO: handle quit properly
                    flame::dump_html(File::create("flame.html").unwrap()).unwrap();
                    std::process::exit(0);
                }

                e => match *self.state.borrow_mut() {
                    ProgramState::Title => unimplemented!(),
                    ProgramState::Game(ref mut game) => game.handle_input(e),
                },
            });
        flame::end("EventsLoop::poll_events");

        match self.input.borrow().current_focus() {
            InputFocus::Game => {
                self.window
                    .borrow_mut()
                    .set_cursor_state(CursorState::Grab)
                    .unwrap();
                self.window.borrow_mut().set_cursor(MouseCursor::NoneCursor);
            }

            _ => {
                self.window
                    .borrow_mut()
                    .set_cursor_state(CursorState::Normal)
                    .unwrap();
                self.window.borrow_mut().set_cursor(MouseCursor::Default);
            }
        }

        // run console commands
        self.console.borrow_mut().execute();

        self.render();
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

    loop {
        host.frame();
    }
}
