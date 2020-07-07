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

use std::{
    cell::{Cell, RefCell},
    fs::File,
    io::BufWriter,
    path::PathBuf,
    rc::Rc,
};

use richter::{
    client::{
        input::{Input, InputFocus},
        menu::Menu,
        render::{
            Camera, GraphicsState, HudState, PostProcessRenderer, RenderTarget as _, UiOverlay,
            UiRenderer, UiState, WorldRenderer,
        },
        trace::TraceFrame,
        Client,
    },
    common::{
        console::{CmdRegistry, Console, CvarRegistry},
        math,
        net::SignOnStage,
        vfs::Vfs,
    },
};

use cgmath;
use chrono::{Duration, Utc};
use failure::Error;
use log::info;

const DEFAULT_TRACE_PATH: &'static str = "richter-trace.json";

#[derive(Clone, Copy)]
enum InGameFocus {
    // active in game
    Game,

    // in menu
    Menu,

    // in console
    Console,
}

struct InGameState {
    cmds: Rc<RefCell<CmdRegistry>>,
    world_renderer: WorldRenderer,
    postprocess_renderer: PostProcessRenderer,
    ui_renderer: Rc<UiRenderer>,
    focus: Rc<Cell<InGameFocus>>,
}

impl InGameState {
    pub fn new(
        cmds: Rc<RefCell<CmdRegistry>>,
        world_renderer: WorldRenderer,
        postprocess_renderer: PostProcessRenderer,
        ui_renderer: Rc<UiRenderer>,
        focus: InGameFocus,
    ) -> InGameState {
        let focus_rc = Rc::new(Cell::new(focus));
        let toggleconsole_focus = focus_rc.clone();

        cmds.borrow_mut()
            .insert(
                "toggleconsole",
                Box::new(move |_| match toggleconsole_focus.get() {
                    InGameFocus::Game => {
                        println!("toggleconsole: ON");
                        toggleconsole_focus.set(InGameFocus::Console);
                    }

                    InGameFocus::Console => {
                        println!("toggleconsole: OFF");
                        toggleconsole_focus.set(InGameFocus::Game);
                    }

                    InGameFocus::Menu => (),
                }),
            )
            .unwrap();

        let togglemenu_focus = focus_rc.clone();

        cmds.borrow_mut()
            .insert(
                "togglemenu",
                Box::new(move |_| match togglemenu_focus.get() {
                    InGameFocus::Game => {
                        println!("togglemenu: ON");
                        togglemenu_focus.set(InGameFocus::Menu);
                    }

                    InGameFocus::Menu | InGameFocus::Console => {
                        println!("togglemenu: OFF");
                        togglemenu_focus.set(InGameFocus::Game);
                    }
                }),
            )
            .unwrap();

        InGameState {
            cmds,
            world_renderer,
            postprocess_renderer,
            ui_renderer,
            focus: focus_rc,
        }
    }
}

impl ::std::ops::Drop for InGameState {
    fn drop(&mut self) {
        // TODO: delete toggleconsole from cmds
    }
}

enum GameState {
    // loading level resources
    Loading,

    // in game
    InGame(InGameState),
}

pub struct Game {
    cvars: Rc<RefCell<CvarRegistry>>,
    cmds: Rc<RefCell<CmdRegistry>>,
    ui_renderer: Rc<UiRenderer>,
    state: GameState,
    input: Rc<RefCell<Input>>,
    client: Client,

    // if Some(v), trace is in progress
    trace: Rc<RefCell<Option<Vec<TraceFrame>>>>,

    // if Some(path), take a screenshot and save it to path
    screenshot_path: Rc<RefCell<Option<PathBuf>>>,
}

impl Game {
    pub fn new(
        cvars: Rc<RefCell<CvarRegistry>>,
        cmds: Rc<RefCell<CmdRegistry>>,
        ui_renderer: Rc<UiRenderer>,
        input: Rc<RefCell<Input>>,
        client: Client,
    ) -> Result<Game, Error> {
        input.borrow().register_cmds(&mut cmds.borrow_mut());

        // set up screenshots
        let screenshot_path = Rc::new(RefCell::new(None));
        let screenshot_screenshot_path = screenshot_path.clone();
        cmds.borrow_mut()
            .insert(
                "screenshot",
                Box::new(move |args| {
                    let path = match args.len() {
                        // TODO: make default path configurable
                        0 => PathBuf::from(format!(
                            "richter-{}.png",
                            Utc::now().format("%FT%H-%M-%S")
                        )),
                        1 => PathBuf::from(args[0]),
                        _ => {
                            log::error!("Usage: screenshot [PATH]");
                            return;
                        }
                    };

                    screenshot_screenshot_path.replace(Some(PathBuf::from(path)));
                }),
            )
            .unwrap();

        // set up frame tracing
        let trace = Rc::new(RefCell::new(None));

        let trace_begin_trace = trace.clone();
        cmds.borrow_mut()
            .insert(
                "trace_begin",
                Box::new(move |_| {
                    if trace_begin_trace.borrow().is_some() {
                        log::error!("trace already in progress");
                    } else {
                        // start a new trace
                        trace_begin_trace.replace(Some(Vec::new()));
                    }
                }),
            )
            .unwrap();

        let trace_end_cvars = cvars.clone();
        let trace_end_trace = trace.clone();
        cmds.borrow_mut()
            .insert(
                "trace_end",
                Box::new(move |_| {
                    if let Some(trace_frames) = trace_end_trace.replace(None) {
                        let trace_path = trace_end_cvars
                            .borrow()
                            .get("trace_path")
                            .unwrap_or(DEFAULT_TRACE_PATH.to_string());
                        let trace_file = match File::create(&trace_path) {
                            Ok(f) => f,
                            Err(e) => {
                                log::error!("Couldn't open trace file for write: {}", e);
                                return;
                            }
                        };

                        let mut writer = BufWriter::new(trace_file);

                        match serde_json::to_writer(&mut writer, &trace_frames) {
                            Ok(()) => (),
                            Err(e) => log::error!("Couldn't serialize trace: {}", e),
                        };

                        log::debug!("wrote {} frames to {}", trace_frames.len(), &trace_path);
                    } else {
                        log::error!("no trace in progress");
                    }
                }),
            )
            .unwrap();

        Ok(Game {
            cvars,
            cmds,
            ui_renderer,
            state: GameState::Loading,
            input,
            client,
            trace,
            screenshot_path,
        })
    }

    // advance the simulation
    pub fn frame(&mut self, gfx_state: &GraphicsState, frame_duration: Duration) {
        self.client.frame(frame_duration).unwrap();

        if let GameState::Loading = self.state {
            println!("loading...");
            // check if we've finished getting server info yet
            if self.client.signon_stage() == SignOnStage::Done {
                println!("finished loading");
                // if we have, build renderers
                let world_renderer = WorldRenderer::new(
                    gfx_state,
                    self.client.models().unwrap(),
                    1,
                    &mut self.cvars.borrow_mut(),
                );

                let postprocess_renderer = PostProcessRenderer::new(
                    gfx_state,
                    gfx_state.initial_pass_target().diffuse_view(),
                );

                self.state = GameState::InGame(InGameState::new(
                    self.cmds.clone(),
                    world_renderer,
                    postprocess_renderer,
                    self.ui_renderer.clone(),
                    InGameFocus::Game,
                ));
            }
        }

        // update input focus
        match self.state {
            // ignore inputs during loading
            GameState::Loading => return,

            GameState::InGame(ref state) => {
                // set the proper focus
                match state.focus.get() {
                    InGameFocus::Game => {
                        self.input.borrow_mut().set_focus(InputFocus::Game).unwrap()
                    }
                    InGameFocus::Menu => {
                        self.input.borrow_mut().set_focus(InputFocus::Menu).unwrap()
                    }
                    InGameFocus::Console => self
                        .input
                        .borrow_mut()
                        .set_focus(InputFocus::Console)
                        .unwrap(),
                }
            }
        }

        if let Some(ref mut game_input) = self.input.borrow_mut().game_input_mut() {
            self.client
                .handle_input(game_input, frame_duration)
                .unwrap();
        }

        if let Some(ref mut trace_frames) = *self.trace.borrow_mut() {
            trace_frames.push(self.client.trace(&[self.client.view_ent()]));
        }
    }

    pub fn render(
        &self,
        gfx_state: &GraphicsState,
        color_attachment_view: &wgpu::TextureView,
        width: u32,
        height: u32,
        console: &Console,
        menu: &Menu,
    ) {
        match self.state {
            // TODO: loading screen
            GameState::Loading => (),

            GameState::InGame(ref state) => {
                let aspect_ratio = width as f32 / height as f32;
                let fov_x = self.cvars.borrow().get_value("fov").unwrap();
                let fov_y = math::fov_x_to_fov_y(cgmath::Deg(fov_x), aspect_ratio).unwrap();
                let perspective = cgmath::perspective(fov_y, aspect_ratio, 4.0, 4096.0);

                let camera = Camera::new(
                    self.client.view_origin(),
                    self.client.view_angles(self.client.time()).unwrap(),
                    perspective,
                );

                info!("Beginning render pass");
                let mut encoder = gfx_state
                    .device()
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

                // initial render pass
                {
                    let init_pass_builder =
                        gfx_state.initial_pass_target().render_pass_builder(None);

                    let mut init_pass = encoder.begin_render_pass(&init_pass_builder.descriptor());

                    state.world_renderer.render_pass(
                        gfx_state,
                        &mut init_pass,
                        &camera,
                        self.client.time(),
                        self.client.iter_visible_entities(),
                        self.client.lightstyle_values().unwrap().as_slice(),
                        &self.cvars.borrow(),
                    );
                }

                let ui_state = UiState::InGame {
                    hud: HudState {
                        items: self.client.items(),
                        item_pickup_time: self.client.item_get_time(),
                        stats: self.client.stats(),
                        face_anim_time: self.client.face_anim_time(),
                    },
                    overlay: match state.focus.get() {
                        InGameFocus::Game => None,
                        InGameFocus::Console => Some(UiOverlay::Console(console)),
                        InGameFocus::Menu => Some(UiOverlay::Menu(menu)),
                    },
                };

                // final render pass
                // TODO: use a separate resolve target that we then blit to the swap chain
                //       so we don't have to render twice for screenshots
                {
                    // quad_commands must outlive final pass
                    let mut quad_commands = Vec::new();
                    let mut glyph_commands = Vec::new();

                    let final_pass_builder = gfx_state
                        .final_pass_target()
                        .render_pass_builder(Some(color_attachment_view));
                    let mut final_pass =
                        encoder.begin_render_pass(&final_pass_builder.descriptor());

                    state.postprocess_renderer.record_draw(
                        gfx_state,
                        &mut final_pass,
                        self.client.color_shift(),
                    );

                    self.ui_renderer.render_pass(
                        &gfx_state,
                        &mut final_pass,
                        width,
                        height,
                        self.client.time(),
                        &ui_state,
                        &mut quad_commands,
                        &mut glyph_commands,
                    );
                }

                let screenshot_target = self.screenshot_path.borrow().as_ref().map(|_| {
                    gfx_state.device().create_texture(&wgpu::TextureDescriptor {
                        label: Some("screenshot texture"),
                        size: gfx_state.final_pass_target().size().into(),
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8UnormSrgb,
                        usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT | wgpu::TextureUsage::COPY_SRC,
                    })
                });

                // render to screenshot target
                if let Some(ref target) = screenshot_target {
                    // re-record UI commands (XXX this is a huge waste :( )
                    let mut quad_commands = Vec::new();
                    let mut glyph_commands = Vec::new();

                    let view = target.create_default_view();
                    let screenshot_pass_builder = gfx_state
                        .final_pass_target()
                        .render_pass_builder(Some(&view));
                    let mut screenshot_pass =
                        encoder.begin_render_pass(&screenshot_pass_builder.descriptor());

                    state.postprocess_renderer.record_draw(
                        gfx_state,
                        &mut screenshot_pass,
                        self.client.color_shift(),
                    );

                    self.ui_renderer.render_pass(
                        &gfx_state,
                        &mut screenshot_pass,
                        width,
                        height,
                        self.client.time(),
                        &ui_state,
                        &mut quad_commands,
                        &mut glyph_commands,
                    );
                }

                // bytes_per_row must be a multiple of 256
                // 4 bytes per pixel, so width must be multiple of 64
                let ss_buf_width = (width + 63) / 64 * 64;

                // create buffer to read screenshot target
                let screenshot_buffer = screenshot_target.as_ref().map(|_| {
                    gfx_state.device().create_buffer(&wgpu::BufferDescriptor {
                        label: Some("screenshot buffer"),
                        size: {
                            let target_size = gfx_state.final_pass_target().size();
                            (ss_buf_width * target_size.height * 4) as u64
                        },
                        usage: wgpu::BufferUsage::COPY_DST | wgpu::BufferUsage::MAP_READ,
                        mapped_at_creation: false,
                    })
                });

                let screenshot_path = match self.screenshot_path.replace(None) {
                    Some(path) => {
                        encoder.copy_texture_to_buffer(
                            wgpu::TextureCopyView {
                                texture: screenshot_target.as_ref().unwrap(),
                                mip_level: 0,
                                origin: wgpu::Origin3d::ZERO,
                            },
                            wgpu::BufferCopyView {
                                buffer: screenshot_buffer.as_ref().unwrap(),
                                layout: wgpu::TextureDataLayout {
                                    offset: 0,
                                    bytes_per_row: ss_buf_width * 4,
                                    rows_per_image: 0,
                                },
                            },
                            wgpu::Extent3d {
                                width,
                                height,
                                depth: 0,
                            },
                        );
                        Some(path)
                    }

                    None => None,
                };

                let command_buffer = encoder.finish();
                {
                    let _submit_guard = flame::start_guard("Submit and poll");
                    gfx_state.queue().submit(vec![command_buffer]);
                    gfx_state.device().poll(wgpu::Maintain::Wait);
                }

                let screenshot_data = screenshot_buffer.map(|b| {
                    let mut data = Vec::new();
                    {
                        let slice = b.slice(..);
                        let map_future = slice.map_async(wgpu::MapMode::Read);
                        gfx_state.device().poll(wgpu::Maintain::Wait);
                        futures::executor::block_on(map_future).unwrap();
                        let mapped = b.slice(..).get_mapped_range();
                        for row in mapped.chunks(ss_buf_width as usize * 4) {
                            data.extend_from_slice(&row[..width as usize * 4]);
                        }
                    }
                    b.unmap();
                    data
                });

                screenshot_data.map(|data| {
                    let f = File::create(screenshot_path.as_ref().unwrap()).unwrap();
                    let w = BufWriter::new(f);

                    let mut encoder = png::Encoder::new(w, width, height);
                    encoder.set_color(png::ColorType::RGBA);
                    encoder.set_depth(png::BitDepth::Eight);
                    let mut writer = encoder.write_header().unwrap();
                    writer.write_image_data(&data).unwrap();
                });
            }
        }
    }
}

impl std::ops::Drop for Game {
    fn drop(&mut self) {
        let _ = self.cmds.borrow_mut().remove("trace_begin");
        let _ = self.cmds.borrow_mut().remove("trace_end");
    }
}
