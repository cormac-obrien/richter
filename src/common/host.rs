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

use crate::common::engine;

use chrono::{DateTime, Duration, Utc};
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopWindowTarget},
};

// TODO: replace with cvar
const FPS_MAX: f32 = 144.0;

pub trait Program: Sized {
    fn handle_event<T>(
        &mut self,
        event: Event<T>,
        _target: &EventLoopWindowTarget<T>,
        control_flow: &mut ControlFlow,
    );

    fn frame(&mut self, frame_duration: Duration);
    fn shutdown(&mut self);
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

        Host {
            program,
            init_time,
            prev_frame_time: init_time,
            prev_frame_duration: Duration::zero(),
        }
    }

    pub fn handle_event<T>(&mut self, event: Event<T>, _target: &EventLoopWindowTarget<T>, control_flow: &mut ControlFlow) {
        match event {
            Event::WindowEvent{event: WindowEvent::CloseRequested, ..} => {
                self.program.shutdown();
                *control_flow = ControlFlow::Exit;
            }

            Event::MainEventsCleared => self.frame(),
            Event::Suspended | Event::Resumed => unimplemented!(),
            Event::LoopDestroyed => {
                // TODO:
                // - host_writeconfig
                // - others...
            }

            e => self.program.handle_event(e, _target, control_flow),

            _ => (),
        }
    }

    pub fn frame(&mut self) {
        let new_frame_time = Utc::now();
        self.prev_frame_duration = new_frame_time.signed_duration_since(self.prev_frame_time);

        // if the time elapsed since the last frame is too low, don't run this one yet
        let prev_frame_duration = self.prev_frame_duration;
        if !self.check_frame_duration(prev_frame_duration) {
            // TODO: not sure about this performance wise. we'll see.
            // avoid busy waiting if we're running at a really high framerate.
            std::thread::yield_now();
            return;
        }

        // we're running this frame, so update the frame time
        self.prev_frame_time = new_frame_time;

        self.program.frame(self.prev_frame_duration);
    }

    // Returns whether enough time has elapsed to run the next frame.
    fn check_frame_duration(&mut self, frame_duration: Duration) -> bool {
        // let fps_max = self.cvars.get_value("fps_max").unwrap();
        let min_frame_duration = engine::duration_from_f32(1.0 / FPS_MAX);
        frame_duration >= min_frame_duration
    }

    pub fn uptime(&self) -> Duration {
        self.prev_frame_time.signed_duration_since(self.init_time)
    }
}
