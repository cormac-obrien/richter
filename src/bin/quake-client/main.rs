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
    net::ToSocketAddrs,
    path::Path,
    process::exit,
    rc::Rc,
};

use game::Game;

use chrono::Duration;
use richter::{
    client::{
        self,
        input::{Input, InputFocus},
        menu::Menu,
        render::{self, Extent2d, GraphicsState, UiRenderer, DIFFUSE_ATTACHMENT_FORMAT},
        Client,
    },
    common::{
        self,
        console::{CmdRegistry, Console, CvarRegistry},
        host::{Host, Program},
        vfs::Vfs,
    },
};
use structopt::StructOpt;
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop, EventLoopWindowTarget},
    window::{Window, WindowBuilder},
};

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

    window: Window,
    window_dimensions_changed: Cell<bool>,

    instance: wgpu::Instance,
    surface: wgpu::Surface,
    adapter: wgpu::Adapter,
    swap_chain: RefCell<wgpu::SwapChain>,
    gfx_state: RefCell<GraphicsState>,
    ui_renderer: Rc<UiRenderer>,

    audio_device: Rc<rodio::Device>,

    state: RefCell<ProgramState>,
    input: Rc<RefCell<Input>>,
}

impl ClientProgram {
    pub async fn new(window: Window, audio_device: rodio::Device) -> ClientProgram {
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
        client::register_cvars(&cvars.borrow()).unwrap();
        render::register_cvars(&cvars.borrow());

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

        let instance = wgpu::Instance::new(wgpu::BackendBit::PRIMARY);
        let surface = unsafe { instance.create_surface(&window) };
        let adapter = instance
            .request_adapter(
                &wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    compatible_surface: Some(&surface),
                },
                wgpu::UnsafeFeatures::disallow(),
            )
            .await
            .unwrap();
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    features: wgpu::Features::SAMPLED_TEXTURE_BINDING_ARRAY
                        | wgpu::Features::SAMPLED_TEXTURE_ARRAY_DYNAMIC_INDEXING
                        | wgpu::Features::SAMPLED_TEXTURE_ARRAY_NON_UNIFORM_INDEXING,
                    limits: wgpu::Limits::default(),
                    shader_validation: true,
                },
                Some(Path::new("./trace/")),
            )
            .await
            .unwrap();
        let size: Extent2d = window.inner_size().into();
        let swap_chain = RefCell::new(device.create_swap_chain(
            &surface,
            &wgpu::SwapChainDescriptor {
                usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
                format: DIFFUSE_ATTACHMENT_FORMAT,
                width: size.width,
                height: size.height,
                present_mode: wgpu::PresentMode::Immediate,
            },
        ));

        let vfs = Rc::new(vfs);

        // TODO: warn user if r_msaa_samples is invalid
        let mut sample_count = cvars.borrow().get_value("r_msaa_samples").unwrap_or(2.0) as u32;
        if !&[2, 4].contains(&sample_count) {
            sample_count = 2;
        }

        let gfx_state = GraphicsState::new(device, queue, size, sample_count, vfs.clone()).unwrap();
        let ui_renderer = Rc::new(UiRenderer::new(&gfx_state, &menu.borrow()));

        // this will also execute config.cfg and autoexec.cfg (assuming an unmodified quake.rc)
        console.borrow().stuff_text("exec quake.rc\n");

        ClientProgram {
            vfs,
            cvars,
            cmds,
            console,
            menu,
            window,
            window_dimensions_changed: Cell::new(false),
            instance,
            surface,
            adapter,
            swap_chain,
            gfx_state: RefCell::new(gfx_state),
            ui_renderer,
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
                self.cvars.clone(),
                self.cmds.clone(),
                self.ui_renderer.clone(),
                self.input.clone(),
                cl,
            )
            .unwrap(),
        ));
    }

    /// Builds a new swap chain with the specified present mode and the window's current dimensions.
    fn recreate_swap_chain(&self, present_mode: wgpu::PresentMode) {
        let winit::dpi::PhysicalSize { width, height } = self.window.inner_size();
        let swap_chain = self.gfx_state.borrow().device().create_swap_chain(
            &self.surface,
            &wgpu::SwapChainDescriptor {
                usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
                format: DIFFUSE_ATTACHMENT_FORMAT,
                width,
                height,
                present_mode,
            },
        );
        let _ = self.swap_chain.replace(swap_chain);
    }

    fn render(&mut self) {
        let swap_chain_output = self.swap_chain.borrow_mut().get_next_frame().unwrap();

        match *self.state.borrow_mut() {
            ProgramState::Title => unimplemented!(),
            ProgramState::Game(ref mut game) => {
                let winit::dpi::PhysicalSize { width, height } = self.window.inner_size();
                game.render(
                    &self.gfx_state.borrow(),
                    &swap_chain_output.output.view,
                    width,
                    height,
                    &self.console.borrow(),
                    &self.menu.borrow(),
                );
            }
        }
    }
}

impl Program for ClientProgram {
    fn handle_event<T>(
        &mut self,
        event: Event<T>,
        _target: &EventLoopWindowTarget<T>,
        _control_flow: &mut ControlFlow,
    ) {
        match event {
            Event::WindowEvent {
                event: WindowEvent::Resized(_),
                ..
            } => {
                self.window_dimensions_changed.set(true);
            }

            e => self.input.borrow_mut().handle_event(e).unwrap(),
        }
    }

    fn frame(&mut self, frame_duration: Duration) {
        let _guard = flame::start_guard("ClientProgram::frame");

        // recreate swapchain if needed
        if self.window_dimensions_changed.get() {
            self.window_dimensions_changed.set(false);
            self.recreate_swap_chain(wgpu::PresentMode::Immediate);
        }

        let size: Extent2d = self.window.inner_size().into();

        // TODO: warn user if r_msaa_samples is invalid
        let mut sample_count = self
            .cvars
            .borrow()
            .get_value("r_msaa_samples")
            .unwrap_or(2.0) as u32;
        if !&[2, 4].contains(&sample_count) {
            sample_count = 2;
        }

        // recreate attachments and rebuild pipelines if necessary
        self.gfx_state.borrow_mut().update(size, sample_count);

        match *self.state.borrow_mut() {
            ProgramState::Title => unimplemented!(),

            ProgramState::Game(ref mut game) => {
                game.frame(&self.gfx_state.borrow(), frame_duration);
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

#[derive(StructOpt, Debug)]
struct Opt {
    #[structopt(name = "SERVER")]
    server: String,
}

fn main() {
    env_logger::init();
    let opt = Opt::from_args();

    let audio_device = rodio::default_output_device().unwrap();

    let event_loop = EventLoop::new();
    let window = {
        #[cfg(target_os = "windows")]
        {
            use winit::platform::windows::WindowBuilderExtWindows as _;
            winit::window::WindowBuilder::new()
                // disable file drag-and-drop so cpal and winit play nice
                .with_drag_and_drop(false)
                .with_title("Richter client")
                .with_inner_size(winit::dpi::PhysicalSize::<u32>::from((1366u32, 768)))
                .build(&event_loop)
                .unwrap()
        }

        #[cfg(not(target_os = "windows"))]
        {
            winit::window::WindowBuilder::new()
                .with_title("Richter client")
                .with_inner_size(winit::dpi::PhysicalSize::<u32>::from((1366u32, 768)))
                .build(&event_loop)
                .unwrap()
        }
    };

    let mut client_program = futures::executor::block_on(ClientProgram::new(window, audio_device));
    client_program.connect(opt.server);
    let mut host = Host::new(client_program);

    event_loop.run(move |event, _target, control_flow| {
        host.handle_event(event, _target, control_flow);
    });
}
