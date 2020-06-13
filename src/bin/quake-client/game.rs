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
        render::wgpu::{Camera, GraphicsState, Renderer},
        Client,
    },
    common::{
        console::{CmdRegistry, CvarRegistry},
        math,
        net::SignOnStage,
        vfs::Vfs,
    },
};

use cgmath;
use chrono::Duration;
use failure::Error;
use winit::event::Event;

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
    renderer: Renderer<'a>,
    // hud_renderer: HudRenderer,
    focus: Rc<Cell<InGameFocus>>,
}

impl<'a> InGameState<'a> {
    pub fn new(
        cmds: Rc<RefCell<CmdRegistry>>,
        renderer: Renderer<'a>,
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
            renderer,
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
    gfx_state: Rc<GraphicsState<'a>>,
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
        gfx_state: Rc<GraphicsState<'a>>,
        input: Rc<RefCell<Input>>,
        client: Client,
    ) -> Result<Game, Error> {
        input.borrow().register_cmds(&mut cmds.borrow_mut());

        Ok(Game {
            vfs,
            cvars,
            cmds,
            menu,
            gfx_state,
            state: GameState::Loading,
            input,
            client,
        })
    }

    // advance the simulation
    pub fn frame(&mut self, frame_duration: Duration) {
        self.client.frame(frame_duration).unwrap();

        if let Some(ref mut game_input) = self.input.borrow_mut().game_input_mut() {
            self.client
                .handle_input(game_input, frame_duration)
                .unwrap();
        }

        if let GameState::Loading = self.state {
            println!("loading...");
            // check if we've finished getting server info yet
            if self.client.signon_stage() == SignOnStage::Done {
                println!("finished loading");
                // if we have, build renderers
                let renderer =
                    Renderer::new(self.client.models().unwrap(), 1, self.gfx_state.clone());

                self.state = GameState::InGame(InGameState::new(
                    self.cmds.clone(),
                    renderer,
                    InGameFocus::Game,
                ));
            }
        }
    }

    pub fn handle_input<T>(&mut self, event: Event<T>) {
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

        self.input.borrow_mut().handle_event(event).unwrap();
    }

    pub fn render(&self, color_attachment_view: &wgpu::TextureView, aspect_ratio: f32) {
        println!("rendering...");
        match self.state {
            // TODO: loading screen
            GameState::Loading => (),

            GameState::InGame(ref state) => {
                let fov_x = self.cvars.borrow().get_value("fov").unwrap();
                let fov_y = math::fov_x_to_fov_y(cgmath::Deg(fov_x), aspect_ratio).unwrap();
                let perspective = cgmath::perspective(fov_y, aspect_ratio, 4.0, 4096.0);

                let camera = Camera::new(
                    self.client.view_origin(),
                    self.client.view_angles(),
                    perspective,
                );

                // render world
                state.renderer.render_pass(
                    color_attachment_view,
                    &camera,
                    self.client.time(),
                    self.client.iter_visible_entities(),
                    self.client.lightstyle_values().unwrap().as_slice(),
                );

                // state
                //     .hud_renderer
                //     .render(encoder, &self.client, display_width, display_height)
                //     .unwrap();

                match state.focus.get() {
                    // don't need to render anything else
                    InGameFocus::Game => (),

                    // render the console
                    InGameFocus::Console => {
                        // self.state
                        //     .borrow()
                        //     .console_renderer()
                        //     .render(
                        //         encoder,
                        //         self.state.borrow().pipeline_2d(),
                        //         &mut data,
                        //         display_width,
                        //         display_height,
                        //         0.5,
                        //         1.0,
                        //     )
                        //     .unwrap();
                    }

                    // render the menu
                    InGameFocus::Menu => {
                        // self.menu_renderer
                        //     .render(
                        //         encoder,
                        //         self.state.borrow().pipeline_2d(),
                        //         &mut data,
                        //         display_width,
                        //         display_height,
                        //         0.5,
                        //     )
                        //     .unwrap();
                    }
                }
            }
        }
    }
}
