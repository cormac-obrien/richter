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
    rc::Rc,
};

use richter::{
    client::{
        input::{Input, InputFocus},
        menu::Menu,
        render::{Camera, GraphicsState, HudState, UiOverlay, UiRenderer, UiState, WorldRenderer},
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

#[derive(Clone, Copy)]
enum InGameFocus {
    // active in game
    Game,

    // in menu
    Menu,

    // in console
    Console,
}

struct InGameState<'a> {
    cmds: Rc<RefCell<CmdRegistry>>,
    world_renderer: WorldRenderer<'a>,
    ui_renderer: Rc<UiRenderer<'a>>,
    focus: Rc<Cell<InGameFocus>>,
}

impl<'a> InGameState<'a> {
    pub fn new(
        cmds: Rc<RefCell<CmdRegistry>>,
        world_renderer: WorldRenderer<'a>,
        ui_renderer: Rc<UiRenderer<'a>>,
        focus: InGameFocus,
    ) -> InGameState<'a> {
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
            ui_renderer,
            focus: focus_rc,
        }
    }
}

impl<'a> ::std::ops::Drop for InGameState<'a> {
    fn drop(&mut self) {
        // TODO: delete toggleconsole from cmds
    }
}

enum GameState<'a> {
    // loading level resources
    Loading,

    // in game
    InGame(InGameState<'a>),
}

pub struct Game<'a> {
    vfs: Rc<Vfs>,
    cvars: Rc<RefCell<CvarRegistry>>,
    cmds: Rc<RefCell<CmdRegistry>>,
    menu: Rc<RefCell<Menu>>,
    ui_renderer: Rc<UiRenderer<'a>>,
    state: GameState<'a>,
    input: Rc<RefCell<Input>>,
    client: Client,
}

impl<'a> Game<'a> {
    pub fn new(
        vfs: Rc<Vfs>,
        cvars: Rc<RefCell<CvarRegistry>>,
        cmds: Rc<RefCell<CmdRegistry>>,
        menu: Rc<RefCell<Menu>>,
        ui_renderer: Rc<UiRenderer<'a>>,
        input: Rc<RefCell<Input>>,
        client: Client,
    ) -> Result<Game<'a>, Error> {
        input.borrow().register_cmds(&mut cmds.borrow_mut());

        Ok(Game {
            vfs,
            cvars,
            cmds,
            menu,
            ui_renderer,
            state: GameState::Loading,
            input,
            client,
        })
    }

    // advance the simulation
    pub fn frame(&mut self, gfx_state: &GraphicsState<'a>, frame_duration: Duration) {
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

                self.state = GameState::InGame(InGameState::new(
                    self.cmds.clone(),
                    world_renderer,
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
    }

    pub fn render(
        &self,
        gfx_state: &GraphicsState<'a>,
        color_attachment_view: &wgpu::TextureView,
        width: u32,
        height: u32,
        console: &Console,
        menu: &Menu,
    ) {
        println!("rendering...");
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
                    self.client.view_angles(),
                    perspective,
                );

                info!("Beginning render pass");
                let mut encoder = gfx_state
                    .device()
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

                let depth_view = gfx_state
                    .framebuffer()
                    .depth_attachment()
                    .create_default_view();
                let color_view = gfx_state
                    .framebuffer()
                    .color_attachment()
                    .create_default_view();

                {
                    // quad_commands must outlive pass
                    let mut quad_commands = Vec::new();
                    let mut glyph_commands = Vec::new();

                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                            attachment: &color_view,
                            resolve_target: Some(color_attachment_view),
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color {
                                    r: 0.0,
                                    g: 0.0,
                                    b: 0.0,
                                    a: 1.0,
                                }),
                                store: true,
                            },
                        }],
                        depth_stencil_attachment: Some(
                            wgpu::RenderPassDepthStencilAttachmentDescriptor {
                                attachment: &depth_view,
                                depth_ops: Some(wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(1.0),
                                    store: true,
                                }),
                                stencil_ops: Some(wgpu::Operations {
                                    load: wgpu::LoadOp::Load,
                                    store: true,
                                }),
                            },
                        ),
                    });

                    // render world
                    state.world_renderer.render_pass(
                        gfx_state,
                        &mut pass,
                        &camera,
                        self.client.time(),
                        self.client.iter_visible_entities(),
                        self.client.lightstyle_values().unwrap().as_slice(),
                        &self.cvars.borrow(),
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
                        &mut pass,
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
