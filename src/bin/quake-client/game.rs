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

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use richter::client::Client;
use richter::client::input::{Input, InputFocus};
use richter::client::render::{self, GraphicsPackage, PipelineState2d, pipe, SceneRenderer};
use richter::client::render::hud::HudRenderer;
use richter::common::math;
use richter::common::console::{CmdRegistry, CvarRegistry};
use richter::common::net::SignOnStage;
use richter::common::pak::Pak;

use cgmath;
use chrono::Duration;
use failure::Error;
use gfx::{CommandBuffer, Encoder};
use gfx_device_gl::Resources;
use glutin::WindowEvent;

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
    renderer: SceneRenderer,
    hud_renderer: HudRenderer,
    focus: Rc<Cell<InGameFocus>>,
}

impl InGameState {
    pub fn new(
        cmds: Rc<RefCell<CmdRegistry>>,
        scene_renderer: SceneRenderer,
        hud_renderer: HudRenderer,
        focus: InGameFocus
    ) -> InGameState {
        let focus_rc = Rc::new(Cell::new(focus));
        let toggleconsole_focus = focus_rc.clone();

        cmds.borrow_mut().insert("toggleconsole", Box::new(move |_| {
            match toggleconsole_focus.get() {
                InGameFocus::Game => {
                    println!("toggleconsole: ON");
                    toggleconsole_focus.set(InGameFocus::Console);
                }

                InGameFocus::Console => {
                    println!("toggleconsole: OFF");
                    toggleconsole_focus.set(InGameFocus::Game);
                }

                InGameFocus::Menu => (),
            }
        })).unwrap();

        InGameState {
            cmds,
            renderer: scene_renderer,
            hud_renderer,
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
    pak: Rc<Pak>,
    cvars: Rc<RefCell<CvarRegistry>>,
    cmds: Rc<RefCell<CmdRegistry>>,
    gfx_pkg: Rc<RefCell<GraphicsPackage>>,
    state: GameState,
    input: Rc<RefCell<Input>>,
    client: Client,
}

impl Game {
    pub fn new(
        pak: Rc<Pak>,
        cvars: Rc<RefCell<CvarRegistry>>,
        cmds: Rc<RefCell<CmdRegistry>>,
        gfx_pkg: Rc<RefCell<GraphicsPackage>>,
        input: Rc<RefCell<Input>>,
        client: Client
    ) -> Result<Game, Error> {
        Ok(Game {
            pak,
            cvars,
            cmds,
            gfx_pkg,
            state: GameState::Loading,
            input,
            client,
        })
    }

    // advance the simulation
    pub fn frame(&mut self, frame_duration: Duration) {
        self.client.frame(frame_duration).unwrap();

        if let Some(ref game_input) = self.input.borrow().game_input() {
            self.client.handle_input(game_input, frame_duration, 0).unwrap();
        }

        if let GameState::Loading = self.state {
            println!("loading...");
            // check if we've finished getting server info yet
            if self.client.signon_stage() == SignOnStage::Done {
                println!("finished loading");
                // if we have, build renderers
                let renderer = SceneRenderer::new(
                    self.client.models().unwrap(),
                    1,
                    &mut self.gfx_pkg.borrow_mut(),
                ).unwrap();

                let hud_renderer = HudRenderer::new(self.gfx_pkg.clone()).unwrap();

                self.state = GameState::InGame(InGameState::new(
                    self.cmds.clone(),
                    renderer,
                    hud_renderer,
                    InGameFocus::Game,
                ));
            }
        }
    }

    pub fn handle_input(&mut self, event: WindowEvent) {
        match self.state {
            // ignore inputs during loading
            GameState::Loading => return,

            GameState::InGame(ref state) => {
                // set the proper focus
                match state.focus.get() {
                    InGameFocus::Game => self.input.borrow_mut().set_focus(InputFocus::Game).unwrap(),
                    InGameFocus::Menu => self.input.borrow_mut().set_focus(InputFocus::Menu).unwrap(),
                    InGameFocus::Console => self.input.borrow_mut().set_focus(InputFocus::Console).unwrap(),
                }
            }
        }

        self.input.borrow_mut().handle_event(event).unwrap();
    }

    pub fn render<C>(
        &mut self,
        encoder: &mut Encoder<Resources, C>,
        user_data: &mut pipe::Data<Resources>,
        display_width: u32,
        display_height: u32,
    )
    where
        C: CommandBuffer<Resources>
    {
        match self.state {
            // TODO: loading screen
            GameState::Loading => (),

            GameState::InGame(ref mut state) => {
                let aspect = display_width as f32 / display_height as f32;
                let fov_x = self.cvars.borrow().get_value("fov").unwrap();
                let fov_y = math::fov_x_to_fov_y(cgmath::Deg(fov_x), aspect).unwrap();

                let perspective = cgmath::perspective(
                    fov_y,
                    aspect,
                    4.0,
                    4096.0,
                );

                let camera = render::Camera::new(
                    self.client.view_origin(),
                    self.client.view_angles(),
                    perspective,
                );

                // render world
                state.renderer.render(
                    encoder,
                    user_data,
                    self.client.entities().unwrap(),
                    self.client.time(),
                    &camera,
                    self.client.lightstyle_values().unwrap().as_slice(),
                ).unwrap();

                state.hud_renderer.render(
                    encoder,
                    &self.client,
                    display_width,
                    display_height,
                ).unwrap();

                match state.focus.get() {
                    // don't need to render anything else
                    InGameFocus::Game => (),

                    // render the console
                    InGameFocus::Console => {
                        let mut data = self.gfx_pkg.borrow().gen_user_data_2d();

                        self.gfx_pkg.borrow().console_renderer().render(
                            encoder,
                            self.gfx_pkg.borrow().pipeline_2d(),
                            &mut data,
                            display_width,
                            display_height,
                            1.0,
                            1.0,
                        ).unwrap();
                    },

                    // render the menu
                    InGameFocus::Menu => unimplemented!(),
                }
            }
        }
    }
}
