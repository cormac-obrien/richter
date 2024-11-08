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

use std::cell::{Ref, RefMut};

use crate::common::{console::CvarRegistry, engine};

use chrono::{DateTime, Duration, Utc};
use winit::event::{Event, WindowEvent};

pub trait Program: Sized {
    fn handle_event(&mut self, event: Event<()>);

    fn frame(&mut self, frame_duration: Duration);
    fn shutdown(&mut self);
    fn cvars(&self) -> Ref<CvarRegistry>;
    fn cvars_mut(&self) -> RefMut<CvarRegistry>;
}

pub struct Host<P>
where
    P: Program,
{
    program: P,

    init_time: DateTime<Utc>,
    prev_frame_time: DateTime<Utc>,
    prev_frame_duration: Duration,
}

impl<P> Host<P>
where
    P: Program,
{
    pub fn new(program: P) -> Host<P> {
        let init_time = Utc::now();
        program
            .cvars_mut()
            .register_archive("host_maxfps", "72")
            .unwrap();

        Host {
            program,
            init_time,
            prev_frame_time: init_time,
            prev_frame_duration: Duration::zero(),
        }
    }

    pub fn handle_event(&mut self, event: Event<()>) {
        match event {
            Event::Suspended | Event::Resumed => unimplemented!(),

            e => self.program.handle_event(e),
        }
    }

    pub fn frame(&mut self) {
        // TODO: make sure this doesn't cause weirdness with e.g. leap seconds
        let new_frame_time = Utc::now();
        self.prev_frame_duration = new_frame_time.signed_duration_since(self.prev_frame_time);

        // if the time elapsed since the last frame is too low, don't run this one yet
        let prev_frame_duration = self.prev_frame_duration;
        if !self.check_frame_duration(prev_frame_duration) {
            // avoid busy waiting if we're running at a really high framerate
            std::thread::sleep(std::time::Duration::from_millis(1));
            return;
        }

        // we're running this frame, so update the frame time
        self.prev_frame_time = new_frame_time;

        self.program.frame(self.prev_frame_duration);
    }

    // Returns whether enough time has elapsed to run the next frame.
    fn check_frame_duration(&mut self, frame_duration: Duration) -> bool {
        let host_maxfps = self
            .program
            .cvars()
            .get_value("host_maxfps")
            .unwrap_or(72.0);
        let min_frame_duration = engine::duration_from_f32(1.0 / host_maxfps);
        frame_duration >= min_frame_duration
    }

    pub fn uptime(&self) -> Duration {
        self.prev_frame_time.signed_duration_since(self.init_time)
    }
}

impl<P> winit::application::ApplicationHandler for Host<P>
where
    P: Program,
{
    fn resumed(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {}

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        log::debug!("Host -> window_event: {event:?}");
        match event {
            WindowEvent::CloseRequested => {
                self.program.shutdown();
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => self.frame(),
            _ => self
                .program
                .handle_event(Event::WindowEvent { window_id, event }),
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &winit::event_loop::ActiveEventLoop,
        device_id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        self.program
            .handle_event(Event::DeviceEvent { device_id, event });
    }
}
