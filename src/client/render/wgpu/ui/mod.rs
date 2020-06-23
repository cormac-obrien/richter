pub mod console;
pub mod glyph;
pub mod layout;
pub mod quad;

use std::cell::RefCell;

use crate::{
    client::{
        menu::Menu,
        render::wgpu::{
            ui::{
                console::ConsoleRenderer,
                glyph::{GlyphRenderer, GlyphRendererCommand, GlyphUniforms},
                quad::{QuadRenderer, QuadRendererCommand, QuadUniforms},
            },
            uniform::{self, DynamicUniformBufferBlock},
            GraphicsState,
        },
    },
    common::console::Console,
};

use cgmath::Matrix4;
use chrono::Duration;

pub fn screen_space_vertex_transform(
    display_w: u32,
    display_h: u32,
    quad_w: u32,
    quad_h: u32,
    pos_x: i32,
    pos_y: i32,
) -> Matrix4<f32> {
    // rescale from [0, DISPLAY_*] to [-1, 1] (NDC)
    let ndc_x = (pos_x * 2 - display_w as i32) as f32 / display_w as f32;
    let ndc_y = (pos_y * 2 - display_h as i32) as f32 / display_h as f32;

    let scale_x = (quad_w * 2) as f32 / display_w as f32;
    let scale_y = (quad_h * 2) as f32 / display_h as f32;

    Matrix4::from_translation([ndc_x, ndc_y, 0.0].into())
        * Matrix4::from_nonuniform_scale(scale_x, scale_y, 1.0)
}

pub enum UiOverlay<'a> {
    Menu(&'a Menu),
    Console(&'a Console),
}

pub enum UiState<'a> {
    Title {
        overlay: UiOverlay<'a>,
    },
    InGame {
        // TODO: stats for hud
        overlay: Option<UiOverlay<'a>>,
    },
}

pub struct UiRenderer<'a> {
    console_renderer: ConsoleRenderer,

    glyph_renderer: GlyphRenderer,
    glyph_uniform_blocks: RefCell<Vec<DynamicUniformBufferBlock<'a, GlyphUniforms>>>,

    quad_renderer: QuadRenderer,
    quad_uniform_blocks: RefCell<Vec<DynamicUniformBufferBlock<'a, QuadUniforms>>>,
}

impl<'a> UiRenderer<'a> {
    pub fn new(state: &GraphicsState<'a>) -> UiRenderer<'a> {
        UiRenderer {
            console_renderer: ConsoleRenderer::new(state),
            glyph_renderer: GlyphRenderer::new(state),
            glyph_uniform_blocks: RefCell::new(Vec::new()),
            quad_renderer: QuadRenderer::new(state),
            quad_uniform_blocks: RefCell::new(Vec::new()),
        }
    }

    pub fn update_uniform_buffers<'b>(
        &'b self,
        state: &'b GraphicsState<'a>,
        display_width: u32,
        display_height: u32,
        _time: Duration,
        quad_commands: &mut Vec<QuadRendererCommand>,
        glyph_commands: &mut Vec<GlyphRendererCommand>,
    ) {
        trace!("Updating UI uniform buffers");

        let glyph_uniforms =
            self.glyph_renderer
                .generate_uniforms(&glyph_commands, display_width, display_height);
        let quad_uniforms =
            self.quad_renderer
                .generate_uniforms(&quad_commands, display_width, display_height);

        uniform::clear_and_rewrite(
            state.queue(),
            &mut state.glyph_uniform_buffer_mut(),
            &mut self.glyph_uniform_blocks.borrow_mut(),
            &glyph_uniforms,
        );
        uniform::clear_and_rewrite(
            state.queue(),
            &mut state.quad_uniform_buffer_mut(),
            &mut self.quad_uniform_blocks.borrow_mut(),
            &quad_uniforms,
        );
    }

    pub fn render_pass<'pass>(
        &'pass self,
        state: &'pass GraphicsState<'a>,
        pass: &mut wgpu::RenderPass<'pass>,
        display_width: u32,
        display_height: u32,
        time: Duration,
        ui_state: UiState<'pass>,
        quad_commands: &'pass mut Vec<QuadRendererCommand<'pass>>,
        glyph_commands: &mut Vec<GlyphRendererCommand>,
    ) where
        'a: 'pass,
    {
        let (hud, overlay): (Option<()>, _) = match ui_state {
            UiState::Title { overlay } => (None, Some(overlay)),
            UiState::InGame { overlay } => (None, overlay),
        };

        if let Some(_) = hud {
            // TODO
        }

        if let Some(o) = overlay {
            match o {
                UiOverlay::Menu(_) => (), // TODO
                UiOverlay::Console(console) => self.console_renderer.generate_commands(
                    state,
                    console,
                    quad_commands,
                    glyph_commands,
                ),
            }
        }

        self.update_uniform_buffers(
            state,
            display_width,
            display_height,
            time,
            quad_commands,
            glyph_commands,
        );

        let quad_blocks = self.quad_uniform_blocks.borrow();
        self.quad_renderer
            .record_draw(state, pass, quad_commands, &quad_blocks);
        self.glyph_renderer
            .record_draw(state, pass, &self.glyph_uniform_blocks.borrow());
    }
}
