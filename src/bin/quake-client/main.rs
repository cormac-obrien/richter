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

#![feature(custom_attribute, plugin)]
#![plugin(flamer)]

extern crate cgmath;
extern crate chrono;
extern crate env_logger;
extern crate flame;
extern crate gfx;
extern crate gfx_device_gl;
extern crate gfx_window_glutin;
extern crate glutin;
extern crate richter;
extern crate rodio;

use std::cell::RefCell;
use std::env;
use std::fs::File;
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
use richter::client::render::SceneRenderer;
use richter::client::render::UiRenderer;
use richter::common;
use richter::common::console::CmdRegistry;
use richter::common::console::Console;
use richter::common::console::CvarRegistry;
use richter::common::host::Host;
use richter::common::host::Program;
use richter::common::net::SignOnStage;
use richter::common::pak::Pak;
use richter::common::wad::Wad;

use cgmath::Matrix4;
use cgmath::SquareMatrix;
use chrono::Duration;
use gfx::Encoder;
use gfx::handle::DepthStencilView;
use gfx::handle::RenderTargetView;
use gfx_device_gl::CommandBuffer;
use gfx_device_gl::Device;
use gfx_device_gl::Factory as GlFactory;
use gfx_device_gl::Resources;
use glutin::ElementState;
use glutin::Event;
use glutin::EventsLoop;
use glutin::GlContext;
use glutin::GlWindow;
use glutin::KeyboardInput;
use glutin::WindowEvent;
use rodio::Endpoint;

struct ClientProgram {
    pak: Rc<Pak>,
    cvars: Rc<RefCell<CvarRegistry>>,
    cmds: Rc<RefCell<CmdRegistry>>,
    console: Rc<RefCell<Console>>,

    events_loop: RefCell<EventsLoop>,
    window: RefCell<GlWindow>,

    device: RefCell<Device>,
    factory: RefCell<GlFactory>,
    encoder: RefCell<Encoder<Resources, CommandBuffer>>,
    color: RenderTargetView<Resources, render::ColorFormat>,
    depth: DepthStencilView<Resources, render::DepthFormat>,
    data: RefCell<render::pipe::Data<Resources>>,
    data_2d: RefCell<render::pipeline2d::Data<Resources>>,

    bindings: Rc<RefCell<Bindings>>,
    endpoint: Rc<Endpoint>,

    palette: render::Palette,

    client: Option<RefCell<Client>>,
    actions: RefCell<GameInput>,
    scene_renderer: Option<RefCell<SceneRenderer>>,
    ui_renderer: Option<RefCell<UiRenderer>>,
}

impl ClientProgram  {
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

        let bindings = Rc::new(RefCell::new(Bindings::new(cvars.clone(), cmds.clone())));
        bindings.borrow_mut().assign_defaults();

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

        use gfx::Factory;
        use gfx::traits::FactoryExt;
        let (_, dummy_texture) = factory.create_texture_immutable_u8::<render::ColorFormat>(
            gfx::texture::Kind::D2(0, 0, gfx::texture::AaMode::Single),
            gfx::texture::Mipmap::Allocated,
            &[&[]]
        ).expect("dummy texture generation failed");

        let sampler = factory.create_sampler(gfx::texture::SamplerInfo::new(
            gfx::texture::FilterMethod::Scale,
            gfx::texture::WrapMode::Tile,
        ));

        let mut data = render::pipe::Data {
            vertex_buffer: factory.create_vertex_buffer(&[]),
            transform: Matrix4::identity().into(),
            sampler: (dummy_texture.clone(), sampler.clone()),
            out_color: color.clone(),
            out_depth: depth.clone(),
        };

        let mut data_2d = render::pipeline2d::Data {
            vertex_buffer: factory.create_vertex_buffer(&[]),
            transform: Matrix4::identity().into(),
            sampler: (dummy_texture.clone(), sampler.clone()),
            out_color: color.clone(),
            out_depth: depth.clone(),
        };

        let encoder = factory.create_command_buffer().into();

        let endpoint = Rc::new(rodio::get_endpoints_list().next().unwrap());

        let palette = render::Palette::load(&pak, "gfx/palette.lmp");

        ClientProgram {
            pak: Rc::new(pak),
            cvars,
            cmds,
            console,
            events_loop: RefCell::new(events_loop),
            window: RefCell::new(window),
            device: RefCell::new(device),
            factory: RefCell::new(factory),
            encoder: RefCell::new(encoder),
            data: RefCell::new(data),
            data_2d: RefCell::new(data_2d),
            color: color,
            depth: depth,
            bindings,
            endpoint,
            palette,
            client: None,
            actions: RefCell::new(GameInput::new()),
            scene_renderer: None,
            ui_renderer: None,
        }
    }

    fn connect<A>(&mut self, server_addrs: A)
    where
        A: ToSocketAddrs,
    {
        self.client = Some(RefCell::new({
            let cl = Client::connect(
                server_addrs,
                self.pak.clone(),
                self.cvars.clone(),
                self.cmds.clone(),
                self.console.clone(),
                self.endpoint.clone(),
            ).unwrap();
            cl.register_cmds(&mut self.cmds.borrow_mut());
            cl
        }
        ));

    }
}

impl Program for ClientProgram  {
    fn frame(&mut self, frame_duration: Duration) {
        let _guard = flame::start_guard("ClientProgram::frame");
        if let Some(ref client) = self.client {
            flame::start("Client::frame");
            client.borrow_mut().frame(frame_duration).unwrap();
            flame::end("Client::frame");

            if client.borrow().get_signon_stage() == SignOnStage::Done {
                if self.scene_renderer.is_none() {
                    self.scene_renderer = Some(RefCell::new(SceneRenderer::new(
                        client.borrow().get_models().unwrap(),
                        1,
                        &self.palette,
                        &mut self.factory.borrow_mut(),
                        self.color.clone(),
                        self.depth.clone(),
                    ).unwrap()));
                }

                if self.ui_renderer.is_none() {
                    let gfx_wad = Wad::load(self.pak.open("gfx.wad").unwrap()).unwrap();
                    self.ui_renderer = Some(RefCell::new(UiRenderer::new(
                        &gfx_wad,
                        &self.palette,
                        &mut self.factory.borrow_mut(),
                    ).unwrap()));
                }

                self.bindings
                    .borrow()
                    .handle(&mut self.actions.borrow_mut(), MouseWheel::Up, ElementState::Released);
                self.bindings.borrow().handle(
                    &mut self.actions.borrow_mut(),
                    MouseWheel::Down,
                    ElementState::Released,
                );

                flame::start("EventsLoop::poll_events");
                self.events_loop
                    .borrow_mut()
                    .poll_events(|event| match event {
                        Event::WindowEvent { event, .. } => match event {
                            WindowEvent::Closed => {
                                // TODO: handle quit properly
                                flame::dump_html(File::create("flame.html").unwrap()).unwrap();
                                std::process::exit(0);
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
                                self.bindings.borrow().handle(&mut self.actions.borrow_mut(), key, state);
                            }

                            WindowEvent::MouseInput { state, button, .. } => {
                                self.bindings.borrow().handle(&mut self.actions.borrow_mut(), button, state);
                            }

                            WindowEvent::MouseWheel { delta, .. } => {
                                self.bindings.borrow().handle(
                                    &mut self.actions.borrow_mut(),
                                    delta,
                                    ElementState::Pressed,
                                );
                            }

                            _ => (),
                        },

                        _ => (),
                    });
                flame::end("EventsLoop::poll_events");

                client
                    .borrow_mut()
                    .handle_input(&mut self.actions.borrow(), frame_duration, 0)
                    .unwrap();
            }

            // run console commands
            self.console.borrow_mut().execute();

            client.borrow_mut().send().unwrap();

            self.encoder.borrow_mut().clear(&self.data.borrow().out_color, [0.0, 0.0, 0.0, 1.0]);
            let (win_w, win_h) = self.window.borrow().get_inner_size().unwrap();
            if let Some(ref scene_renderer) = self.scene_renderer {
                let cl = client.borrow();

                let fov_x = self.cvars.borrow().get_value("fov").unwrap();
                let aspect = win_w as f32 / win_h as f32;
                let fov_y = common::math::fov_x_to_fov_y(cgmath::Deg(fov_x), aspect).unwrap();

                let perspective = cgmath::perspective(
                    fov_y,
                    aspect,
                    1.0,
                    65536.0
                );

                let camera = render::Camera::new(
                    cl.get_view_origin(),
                    cl.get_view_angles(),
                    perspective,
                );

                self.encoder.borrow_mut().clear_depth(&self.data.borrow().out_depth, 1.0);
                scene_renderer.borrow_mut().render(
                    &mut self.encoder.borrow_mut(),
                    &mut self.data.borrow_mut(),
                    cl.get_entities().unwrap(),
                    cl.get_time(),
                    &camera,
                    &cl.lightstyle_values().unwrap(),
                ).unwrap();
            }

            if let Some(ref ui_renderer) = self.ui_renderer {
                ui_renderer.borrow_mut().render(
                    &mut self.factory.borrow_mut(),
                    &mut self.encoder.borrow_mut(),
                    &mut self.data_2d.borrow_mut(),
                    &client.borrow(),
                    win_w,
                    win_h,
                ).unwrap();
            }

            use std::ops::DerefMut;

            flame::start("Encoder::flush");
            self.encoder.borrow_mut().flush(self.device.borrow_mut().deref_mut());
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
