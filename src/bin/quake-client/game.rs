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
            Camera, ClientRenderer, DeferredRenderer, DeferredUniforms, Extent2d, GraphicsState,
            HudState, PointLight, PostProcessRenderer, RenderTarget as _, RenderTargetResolve as _,
            SwapChainTarget, UiOverlay, UiRenderer, UiState, WorldRenderer,
        },
        trace::TraceFrame,
        Client, ClientError,
    },
    common::{
        console::{CmdRegistry, Console, CvarRegistry},
        math,
        net::SignOnStage,
    },
};

use cgmath::{self, Deg};
use chrono::Duration;
use failure::Error;
use log::info;

pub struct Game {
    cvars: Rc<RefCell<CvarRegistry>>,
    cmds: Rc<RefCell<CmdRegistry>>,
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
        input: Rc<RefCell<Input>>,
        client: Client,
    ) -> Result<Game, Error> {
        // set up input commands
        input.borrow().register_cmds(&mut cmds.borrow_mut());

        // set up focus toggles
        cmds.borrow_mut()
            .insert(
                "toggleconsole",
                cmd_toggleconsolemenu_disconnected(input.clone()),
            )
            .unwrap();
        cmds.borrow_mut()
            .insert(
                "togglemenu",
                cmd_toggleconsolemenu_disconnected(input.clone()),
            )
            .unwrap();

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
            input,
            client,
            trace,
            screenshot_path,
        })
    }

    // advance the simulation
    pub fn frame(&mut self, gfx_state: &GraphicsState, frame_duration: Duration) {
        use ClientError::*;

        match self.client.frame(frame_duration) {
            Ok(()) => (),
            Err(e) => match e {
                Cvar(_)
                | UnrecognizedProtocol(_)
                | NoSuchClient(_)
                | NoSuchPlayer(_)
                | NoSuchEntity(_)
                | NullEntity
                | EntityExists(_)
                | InvalidViewEntity(_)
                | TooManyStaticEntities
                | NoSuchLightmapAnimation(_)
                | Model(_)
                | Network(_)
                | Sound(_)
                | Vfs(_) => {
                    log::error!("{}", e);
                    self.client.disconnect();
                }

                _ => panic!("{}", e),
            },
        };

        match self.client.signon() {
            None => (),
            Some(signon) => match signon {
                SignOnStage::Done => {
                    println!("finished loading");
                    self.cmds.borrow_mut().insert_or_replace(
                        "toggleconsole",
                        cmd_toggleconsole_connected(self.input.clone()),
                    );
                    self.cmds.borrow_mut().insert_or_replace(
                        "togglemenu",
                        cmd_togglemenu_connected(self.input.clone()),
                    );
                }

                _ => println!("loading..."),
            },
        }

        if let Some(ref mut game_input) = self.input.borrow_mut().game_input_mut() {
            self.client
                .handle_input(game_input, frame_duration)
                .unwrap();
        }

        // if there's an active trace, record this frame
        if let Some(ref mut trace_frames) = *self.trace.borrow_mut() {
            trace_frames.push(
                self.client
                    .trace(&[self.client.view_ent().unwrap()])
                    .unwrap(),
            );
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
        info!("Beginning render pass");
        let mut encoder = gfx_state
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        // render world, hud, console, menus
        self.client.render(
            gfx_state,
            &mut encoder,
            width,
            height,
            menu,
            self.input.borrow().focus(),
        ).unwrap();

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
            let swap_chain_target = SwapChainTarget::with_swap_chain_view(color_attachment_view);
            let blit_pass_builder = swap_chain_target.render_pass_builder();
            let mut blit_pass = encoder.begin_render_pass(&blit_pass_builder.descriptor());
            gfx_state.blit_pipeline().blit(gfx_state, &mut blit_pass);
        }

        let command_buffer = encoder.finish();
        {
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

impl std::ops::Drop for Game {
    fn drop(&mut self) {
        let _ = self.cmds.borrow_mut().remove("trace_begin");
        let _ = self.cmds.borrow_mut().remove("trace_end");
    }
}

// implements the "toggleconsole" and "togglemenu" commands when the client is disconnected
fn cmd_toggleconsolemenu_disconnected(input: Rc<RefCell<Input>>) -> Box<dyn Fn(&[&str])> {
    Box::new(move |_| {
        let focus = input.borrow().focus();
        match focus {
            InputFocus::Console => input.borrow_mut().set_focus(InputFocus::Menu),
            InputFocus::Game => unreachable!(),
            InputFocus::Menu => input.borrow_mut().set_focus(InputFocus::Console),
        }
    })
}

// implements the "toggleconsole" command when the client is connected
fn cmd_toggleconsole_connected(input: Rc<RefCell<Input>>) -> Box<dyn Fn(&[&str])> {
    Box::new(move |_| {
        let focus = input.borrow().focus();
        match focus {
            InputFocus::Game => input.borrow_mut().set_focus(InputFocus::Console),
            InputFocus::Console => input.borrow_mut().set_focus(InputFocus::Game),
            InputFocus::Menu => input.borrow_mut().set_focus(InputFocus::Console),
        }
    })
}

// implements the "togglemenu" command when the client is connected
fn cmd_togglemenu_connected(input: Rc<RefCell<Input>>) -> Box<dyn Fn(&[&str])> {
    Box::new(move |_| {
        let focus = input.borrow().focus();
        match focus {
            InputFocus::Game => input.borrow_mut().set_focus(InputFocus::Menu),
            InputFocus::Console => input.borrow_mut().set_focus(InputFocus::Menu),
            InputFocus::Menu => input.borrow_mut().set_focus(InputFocus::Game),
        }
    })
}
