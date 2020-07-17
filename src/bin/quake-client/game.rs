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
    path::PathBuf,
    rc::Rc,
};

use crate::{
    capture::{cmd_screenshot, Capture},
    trace::{cmd_trace_begin, cmd_trace_end},
};

use richter::{
    client::{
        entity::MAX_LIGHTS,
        input::{Input, InputFocus},
        menu::Menu,
        render::{
            Camera, DeferredRenderer, DeferredUniforms, Extent2d, GraphicsState, HudState,
            PointLight, PostProcessRenderer, RenderTarget as _, RenderTargetResolve as _,
            SwapChainTarget, UiOverlay, UiRenderer, UiState, WorldRenderer,
        },
        trace::TraceFrame,
        Client,
    },
    common::{
        console::{CmdRegistry, Console, CvarRegistry},
        math,
        net::SignOnStage,
    },
};

use bumpalo::Bump;
use cgmath::{self, SquareMatrix as _, Vector3, Zero as _};
use chrono::Duration;
use failure::Error;
use log::info;

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
    world_renderer: WorldRenderer,
    deferred_renderer: DeferredRenderer,
    postprocess_renderer: PostProcessRenderer,
    focus: Rc<Cell<InGameFocus>>,
}

impl InGameState {
    pub fn new(
        cmds: Rc<RefCell<CmdRegistry>>,
        world_renderer: WorldRenderer,
        deferred_renderer: DeferredRenderer,
        postprocess_renderer: PostProcessRenderer,
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
            world_renderer,
            deferred_renderer,
            postprocess_renderer,
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
    render_pass_bump: Bump,
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
        cmds.borrow_mut()
            .insert("screenshot", cmd_screenshot(screenshot_path.clone()))
            .unwrap();

        // set up frame tracing
        let trace = Rc::new(RefCell::new(None));
        cmds.borrow_mut()
            .insert("trace_begin", cmd_trace_begin(trace.clone()))
            .unwrap();
        cmds.borrow_mut()
            .insert("trace_end", cmd_trace_end(cvars.clone(), trace.clone()))
            .unwrap();

        Ok(Game {
            cvars,
            cmds,
            ui_renderer,
            // TODO: specify a capacity
            render_pass_bump: Bump::new(),
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

                let deferred_renderer = DeferredRenderer::new(
                    gfx_state,
                    gfx_state.initial_pass_target().diffuse_view(),
                    gfx_state.initial_pass_target().normal_view(),
                    gfx_state.initial_pass_target().light_view(),
                    gfx_state.initial_pass_target().depth_view(),
                );

                let postprocess_renderer = PostProcessRenderer::new(
                    gfx_state,
                    gfx_state.deferred_pass_target().color_view(),
                );

                self.state = GameState::InGame(InGameState::new(
                    self.cmds.clone(),
                    world_renderer,
                    deferred_renderer,
                    postprocess_renderer,
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
                self.input
                    .borrow_mut()
                    .set_focus(match state.focus.get() {
                        InGameFocus::Game => InputFocus::Game,
                        InGameFocus::Menu => InputFocus::Menu,
                        InGameFocus::Console => InputFocus::Console,
                    })
                    .unwrap();
            }
        }

        if let Some(ref mut game_input) = self.input.borrow_mut().game_input_mut() {
            self.client
                .handle_input(game_input, frame_duration)
                .unwrap();
        }

        // if there's an active trace, record this frame
        if let Some(ref mut trace_frames) = *self.trace.borrow_mut() {
            trace_frames.push(self.client.trace(&[self.client.view_ent()]));
        }
    }

    pub fn render(
        &mut self,
        gfx_state: &GraphicsState,
        color_attachment_view: &wgpu::TextureView,
        width: u32,
        height: u32,
        console: &Console,
        menu: &Menu,
    ) {
        // we don't need to keep this data between frames
        self.render_pass_bump.reset();

        match self.state {
            // TODO: loading screen
            GameState::Loading => (),

            GameState::InGame(ref state) => {
                let aspect_ratio = width as f32 / height as f32;
                let fov_x = self.cvars.borrow().get_value("fov").unwrap();
                let fov_y = math::fov_x_to_fov_y(cgmath::Deg(fov_x), aspect_ratio).unwrap();

                let projection = cgmath::perspective(fov_y, aspect_ratio, 4.0, 4096.0);
                let camera = Camera::new(
                    self.client.view_origin(),
                    self.client.view_angles(self.client.time()).unwrap(),
                    projection,
                );

                info!("Beginning render pass");
                let mut encoder = gfx_state
                    .device()
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

                // initial render pass
                {
                    let init_pass_builder = gfx_state.initial_pass_target().render_pass_builder();

                    let mut init_pass = encoder.begin_render_pass(&init_pass_builder.descriptor());

                    state.world_renderer.render_pass(
                        gfx_state,
                        &mut init_pass,
                        &self.render_pass_bump,
                        &camera,
                        self.client.time(),
                        self.client.iter_visible_entities(),
                        self.client.iter_particles(),
                        self.client.lightstyle_values().unwrap().as_slice(),
                        &self.cvars.borrow(),
                    );
                }

                // deferred lighting pass
                {
                    let deferred_pass_builder =
                        gfx_state.deferred_pass_target().render_pass_builder();
                    let mut deferred_pass =
                        encoder.begin_render_pass(&deferred_pass_builder.descriptor());

                    let mut lights = [PointLight {
                        origin: Vector3::zero(),
                        radius: 0.0,
                    }; MAX_LIGHTS];

                    let mut light_count = 0;
                    for (light_id, light) in self.client.iter_lights().enumerate() {
                        light_count += 1;
                        let light_origin = light.origin();
                        let converted_origin = Vector3::new(
                            -light_origin.y,
                            light_origin.z,
                            -light_origin.x,
                        );
                        lights[light_id].origin =
                            (camera.view() * converted_origin.extend(1.0)).truncate();
                        lights[light_id].radius = light.radius(self.client.time());
                    }

                    let uniforms = DeferredUniforms {
                        inv_projection: projection.invert().unwrap().into(),
                        light_count,
                        _pad: [0; 3],
                        lights,
                    };

                    state
                        .deferred_renderer
                        .record_draw(gfx_state, &mut deferred_pass, uniforms);
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
                {
                    // quad_commands must outlive final pass
                    let mut quad_commands = Vec::new();
                    let mut glyph_commands = Vec::new();

                    let final_pass_builder = gfx_state.final_pass_target().render_pass_builder();
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
                        Extent2d { width, height },
                        self.client.time(),
                        &ui_state,
                        &mut quad_commands,
                        &mut glyph_commands,
                    );
                }

                // screenshot setup
                let capture = self.screenshot_path.borrow().as_ref().map(|_| {
                    let cap = Capture::new(gfx_state.device(), Extent2d { width, height });
                    cap.copy_from_texture(
                        &mut encoder,
                        wgpu::TextureCopyView {
                            texture: gfx_state.final_pass_target().resolve_attachment(),
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                        },
                    );
                    cap
                });

                // blit to swap chain
                {
                    let swap_chain_target =
                        SwapChainTarget::with_swap_chain_view(color_attachment_view);
                    let blit_pass_builder = swap_chain_target.render_pass_builder();
                    let mut blit_pass = encoder.begin_render_pass(&blit_pass_builder.descriptor());
                    gfx_state.blit_pipeline().blit(gfx_state, &mut blit_pass);
                }

                let command_buffer = encoder.finish();
                {
                    let _submit_guard = flame::start_guard("Submit and poll");
                    gfx_state.queue().submit(vec![command_buffer]);
                    gfx_state.device().poll(wgpu::Maintain::Wait);
                }

                // write screenshot if requested and clear screenshot path
                self.screenshot_path.replace(None).map(|path| {
                    capture
                        .as_ref()
                        .unwrap()
                        .write_to_file(gfx_state.device(), path)
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
