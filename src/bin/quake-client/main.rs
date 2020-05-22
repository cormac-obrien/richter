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
mod menu;

use std::{cell::RefCell, env, fs::File, net::ToSocketAddrs, path::Path, process::exit, rc::Rc};

use richter::{
    client::{
        self,
        input::{Input, InputFocus},
        menu::Menu,
        render::{self, GraphicsPackage},
        Client,
    },
    common,
    common::{
        console::{CmdRegistry, Console, CvarRegistry},
        host::{Host, Program},
        vfs::Vfs,
    },
};

use crate::game::Game;

use cgmath::{Matrix4, SquareMatrix};
use chrono::Duration;
use gfx::Encoder;
use gfx_device_gl::{CommandBuffer, Device, Resources};
use glutin::{Event, EventsLoop, WindowEvent, WindowedContext};

enum TitleState {
    Menu,
    Console,
}

enum ProgramState {
    Title,
    Game(Game),
}

struct ClientProgram {
    vfs: Rc<Vfs>,
    cvars: Rc<RefCell<CvarRegistry>>,
    cmds: Rc<RefCell<CmdRegistry>>,
    console: Rc<RefCell<Console>>,
    menu: Rc<RefCell<Menu>>,

    events_loop: RefCell<EventsLoop>,
    windowed_context: RefCell<WindowedContext>,

    gfx_pkg: Rc<RefCell<GraphicsPackage>>,
    device: RefCell<Device>,
    encoder: RefCell<Encoder<Resources, CommandBuffer>>,
    data: RefCell<render::pipe::Data<Resources>>,

    audio_device: Rc<rodio::Device>,

    state: RefCell<ProgramState>,
    input: Rc<RefCell<Input>>,
}

impl ClientProgram {
    pub fn new() -> ClientProgram {
        let mut vfs = Vfs::new();

        // add basedir first
        vfs.add_directory(common::DEFAULT_BASEDIR).unwrap();

        // then add PAK archives
        for vfs_id in 0..common::MAX_PAKFILES {
            // TODO: check `-basedir` command line argument
            let basedir = common::DEFAULT_BASEDIR;
            let path_string = format!("{}/pak{}.pak", basedir, vfs_id);
            let path = Path::new(&path_string);

            // keep adding PAKs until we don't find one or we hit MAX_PAKFILES
            if !path.exists() {
                break;
            }

            vfs.add_pakfile(path).unwrap();
        }

        let cvars = Rc::new(RefCell::new(CvarRegistry::new()));
        client::register_cvars(&cvars.borrow_mut());

        let cmds = Rc::new(RefCell::new(CmdRegistry::new()));
        // TODO: register commands as other subsystems come online

        let console = Rc::new(RefCell::new(Console::new(cmds.clone(), cvars.clone())));
        let menu = Rc::new(RefCell::new(menu::build_main_menu().unwrap()));

        let input = Rc::new(RefCell::new(Input::new(
            InputFocus::Game,
            console.clone(),
            menu.clone(),
        )));
        input.borrow_mut().bind_defaults();

        let events_loop = glutin::EventsLoop::new();
        let window_builder = glutin::WindowBuilder::new()
            .with_title("Richter client")
            .with_dimensions((1600, 900).into());
        let context_builder = glutin::ContextBuilder::new()
            .with_gl(glutin::GlRequest::Specific(glutin::Api::OpenGl, (4, 3)))
            .with_vsync(false);

        let (windowed_context, device, mut factory, color, depth) =
            gfx_window_glutin::init::<render::ColorFormat, render::DepthFormat>(
                window_builder,
                context_builder,
                &events_loop,
            )
            .unwrap();

        use gfx::{traits::FactoryExt, Factory};
        let (_, dummy_texture) = factory
            .create_texture_immutable_u8::<render::ColorFormat>(
                gfx::texture::Kind::D2(1, 1, gfx::texture::AaMode::Single),
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

        let audio_device = Rc::new(rodio::default_output_device().unwrap());

        let gfx_pkg = Rc::new(RefCell::new(GraphicsPackage::new(
            &vfs,
            factory,
            color,
            depth,
            console.clone(),
        )));

        // this will also execute config.cfg and autoexec.cfg (assuming an unmodified quake.rc)
        console.borrow().stuff_text("exec quake.rc\n");

        ClientProgram {
            vfs: Rc::new(vfs),
            cvars,
            cmds,
            console,
            menu,
            events_loop: RefCell::new(events_loop),
            windowed_context: RefCell::new(windowed_context),
            gfx_pkg,
            device: RefCell::new(device),
            encoder: RefCell::new(encoder),
            data: RefCell::new(data),
            audio_device,
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
            self.vfs.clone(),
            self.cvars.clone(),
            self.cmds.clone(),
            self.console.clone(),
            self.audio_device.clone(),
        )
        .unwrap();

        cl.register_cmds(&mut self.cmds.borrow_mut());

        self.state.replace(ProgramState::Game(
            Game::new(
                self.vfs.clone(),
                self.cvars.clone(),
                self.cmds.clone(),
                self.menu.clone(),
                self.gfx_pkg.clone(),
                self.input.clone(),
                cl,
            )
            .unwrap(),
        ));
    }

    fn render(&mut self) {
        self.encoder
            .borrow_mut()
            .clear(&self.gfx_pkg.borrow().color_target(), [0.0, 0.0, 0.0, 1.0]);
        self.encoder
            .borrow_mut()
            .clear_depth(&self.gfx_pkg.borrow().depth_stencil(), 1.0);
        let (win_w, win_h) = self
            .windowed_context
            .borrow()
            .get_inner_size()
            .unwrap()
            .into();

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
        self.windowed_context.borrow_mut().swap_buffers().unwrap();
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

        flame::start("EventsLoop::poll_events");
        self.events_loop
            .borrow_mut()
            .poll_events(|event| match event {
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
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
                self.windowed_context
                    .borrow_mut()
                    .grab_cursor(true)
                    .unwrap();
                self.windowed_context.borrow_mut().hide_cursor(true);
            }

            _ => {
                self.windowed_context
                    .borrow_mut()
                    .grab_cursor(false)
                    .unwrap();
                self.windowed_context.borrow_mut().hide_cursor(false);
            }
        }

        // run console commands
        self.console.borrow().execute();

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
