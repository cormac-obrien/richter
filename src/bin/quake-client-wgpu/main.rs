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

mod game;
mod menu;

use std::{
    cell::{Cell, RefCell},
    env,
    fs::File,
    net::ToSocketAddrs,
    path::Path,
    process::exit,
    rc::Rc,
};

use game::Game;

use cgmath::{Matrix4, SquareMatrix};
use chrono::Duration;
use richter::{
    client::{
        self,
        input::{Input, InputFocus},
        menu::Menu,
        render::wgpu::{GraphicsPackage, COLOR_ATTACHMENT_FORMAT},
        Client,
    },
    common::{
        self,
        console::{CmdRegistry, Console, CvarRegistry},
        host::{Host, Program},
        vfs::Vfs,
    },
};
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop, EventLoopWindowTarget},
    window::{Window, WindowBuilder},
};

enum TitleState {
    Menu,
    Console,
}

enum ProgramState<'a> {
    Title,
    Game(Game<'a>),
}

struct ClientProgram<'a> {
    vfs: Rc<Vfs>,
    cvars: Rc<RefCell<CvarRegistry>>,
    cmds: Rc<RefCell<CmdRegistry>>,
    console: Rc<RefCell<Console>>,
    menu: Rc<RefCell<Menu>>,

    window: Window,
    window_dimensions_changed: Cell<bool>,

    surface: wgpu::Surface,
    adapter: wgpu::Adapter,
    swap_chain: RefCell<wgpu::SwapChain>,
    gfx_pkg: Rc<GraphicsPackage<'a>>,

    audio_device: Rc<rodio::Device>,

    state: RefCell<ProgramState<'a>>,
    input: Rc<RefCell<Input>>,
}

impl<'a> ClientProgram<'a> {
    pub async fn new(window: Window, audio_device: rodio::Device) -> ClientProgram<'a> {
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

        let surface = wgpu::Surface::create(&window);
        let adapter = wgpu::Adapter::request(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
            },
            wgpu::BackendBit::PRIMARY,
        )
        .await
        .unwrap();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                extensions: wgpu::Extensions {
                    anisotropic_filtering: false,
                },
                limits: wgpu::Limits::default(),
            })
            .await;
        let winit::dpi::PhysicalSize { width, height } = window.inner_size();
        let swap_chain = RefCell::new(device.create_swap_chain(
            &surface,
            &wgpu::SwapChainDescriptor {
                usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
                format: COLOR_ATTACHMENT_FORMAT,
                width,
                height,
                present_mode: wgpu::PresentMode::Immediate,
            },
        ));

        let gfx_pkg = Rc::new(GraphicsPackage::new(device, queue, width, height, &vfs).unwrap());

        // this will also execute config.cfg and autoexec.cfg (assuming an unmodified quake.rc)
        console.borrow().stuff_text("exec quake.rc\n");

        ClientProgram {
            vfs: Rc::new(vfs),
            cvars,
            cmds,
            console,
            menu,
            window,
            window_dimensions_changed: Cell::new(false),
            surface,
            adapter,
            swap_chain,
            gfx_pkg,
            audio_device: Rc::new(audio_device),
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

    /// Builds a new swap chain with the specified present mode and the window's current dimensions.
    fn recreate_swap_chain(&self, present_mode: wgpu::PresentMode) {
        let winit::dpi::PhysicalSize { width, height } = self.window.inner_size();
        let swap_chain = self.gfx_pkg.device().create_swap_chain(
            &self.surface,
            &wgpu::SwapChainDescriptor {
                usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
                format: COLOR_ATTACHMENT_FORMAT,
                width,
                height,
                present_mode,
            },
        );
        let _ = self.swap_chain.replace(swap_chain);
    }

    fn render(&mut self) {
        let swap_chain_output = self.swap_chain.borrow_mut().get_next_texture().unwrap();

        match *self.state.borrow_mut() {
            ProgramState::Title => unimplemented!(),
            ProgramState::Game(ref mut game) => {
                let winit::dpi::PhysicalSize { width, height } = self.window.inner_size();
                game.render(&swap_chain_output.view, width as f32 / height as f32);
            }
        }
    }
}

impl<'a> Program for ClientProgram<'a> {
    fn handle_event<T>(
        &mut self,
        event: Event<T>,
        _target: &EventLoopWindowTarget<T>,
        control_flow: &mut ControlFlow,
    ) {
        match event {
            Event::WindowEvent {
                event: WindowEvent::Resized(new_size),
                ..
            } => {
                self.window_dimensions_changed.set(true);
            }

            e => self.input.borrow_mut().handle_event(e).unwrap(),
        }
    }

    fn frame(&mut self, frame_duration: Duration) {
        let _guard = flame::start_guard("ClientProgram::frame");

        if self.window_dimensions_changed.get() {
            self.recreate_swap_chain(wgpu::PresentMode::Immediate);

            let winit::dpi::PhysicalSize { width, height } = self.window.inner_size();
            self.gfx_pkg.recreate_depth_attachment(width, height);
        }

        match *self.state.borrow_mut() {
            ProgramState::Title => unimplemented!(),

            ProgramState::Game(ref mut game) => {
                game.frame(frame_duration);
            }
        }

        match self.input.borrow().current_focus() {
            InputFocus::Game => {
                self.window.set_cursor_grab(true).unwrap();
                self.window.set_cursor_visible(false);
            }

            _ => {
                self.window.set_cursor_grab(false).unwrap();
                self.window.set_cursor_visible(true);
            }
        }

        // run console commands
        self.console.borrow().execute();

        self.render();
    }

    fn shutdown(&mut self) {
        // TODO: do cleanup things here
    }
}

fn main() {
    env_logger::init();

    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        println!("Usage: {} <server_address>", args[0]);
        exit(1);
    }

    let audio_device = rodio::default_output_device().unwrap();

    let event_loop = EventLoop::new();
    let window = if cfg!(target_os = "windows") {
        use winit::platform::windows::WindowBuilderExtWindows as _;
        winit::window::WindowBuilder::new()
            // disable file drag-and-drop so cpal and winit play nice
            .with_drag_and_drop(false)
            .with_title("Richter client")
            .with_inner_size(winit::dpi::PhysicalSize::<u32>::from((1366u32, 768)))
            .build(&event_loop)
            .unwrap()
    } else {
        winit::window::WindowBuilder::new()
            .with_title("Richter client")
            .with_inner_size(winit::dpi::PhysicalSize::<u32>::from((1366u32, 768)))
            .build(&event_loop)
            .unwrap()
    };

    let mut client_program = futures::executor::block_on(ClientProgram::new(window, audio_device));
    client_program.connect(&args[1]);
    let mut host = Host::new(client_program);

    event_loop.run(move |event, _target, control_flow| {
        host.handle_event(event, _target, control_flow);
    });
}
