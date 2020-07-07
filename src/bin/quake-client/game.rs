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
use chrono::Duration;
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
    trace: Rc<RefCell<Option<Vec<TraceFrame>>>>,
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

                // final render pass
                {
                    // quad_commands must outlive final pass
                    let mut quad_commands = Vec::new();
                    let mut glyph_commands = Vec::new();

                    let final_pass_builder = gfx_state
                        .final_pass_target()
                        .render_pass_builder(Some(color_attachment_view));
                    let mut final_pass =
                        encoder.begin_render_pass(&final_pass_builder.descriptor());

                    log::debug!("color shift = {:?}", self.client.color_shift());
                    state.postprocess_renderer.record_draw(
                        gfx_state,
                        &mut final_pass,
                        self.client.color_shift(),
                    );

                    let overlay = match state.focus.get() {
                        InGameFocus::Game => None,
                        InGameFocus::Console => Some(UiOverlay::Console(console)),
                        InGameFocus::Menu => Some(UiOverlay::Menu(menu)),
                    };

                    let ui_state = UiState::InGame {
                        hud: HudState {
                            items: self.client.items(),
                            item_pickup_time: self.client.item_get_time(),
                            stats: self.client.stats(),
                            face_anim_time: self.client.face_anim_time(),
                        },
                        overlay,
                    };

                    self.ui_renderer.render_pass(
                        &gfx_state,
                        &mut final_pass,
                        width,
                        height,
                        self.client.time(),
                        ui_state,
                        &mut quad_commands,
                        &mut glyph_commands,
                    );
                }

                let command_buffer = encoder.finish();
                {
                    let _submit_guard = flame::start_guard("Submit and poll");
                    gfx_state.queue().submit(vec![command_buffer]);
                    gfx_state.device().poll(wgpu::Maintain::Wait);
                }
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
