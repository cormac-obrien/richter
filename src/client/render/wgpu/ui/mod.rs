pub mod console;
pub mod glyph;
pub mod hud;
pub mod layout;
pub mod menu;
pub mod quad;

use std::cell::RefCell;

use crate::{
    client::{
        menu::Menu,
        render::wgpu::{
            ui::{
                console::ConsoleRenderer,
                glyph::{GlyphRenderer, GlyphRendererCommand},
                hud::{HudRenderer, HudState},
                menu::MenuRenderer,
                quad::{QuadRenderer, QuadRendererCommand, QuadUniforms},
            },
            uniform::{self, DynamicUniformBufferBlock},
            GraphicsState,
        },
    },
    common::{console::Console, util::any_slice_as_bytes},
};

use cgmath::{Matrix4, Vector2};
use chrono::Duration;

pub fn screen_space_vertex_translate(
    display_w: u32,
    display_h: u32,
    pos_x: i32,
    pos_y: i32,
) -> Vector2<f32> {
    // rescale from [0, DISPLAY_*] to [-1, 1] (NDC)
    Vector2::new(
        (pos_x * 2 - display_w as i32) as f32 / display_w as f32,
        (pos_y * 2 - display_h as i32) as f32 / display_h as f32,
    )
}

pub fn screen_space_vertex_scale(
    display_w: u32,
    display_h: u32,
    quad_w: u32,
    quad_h: u32,
) -> Vector2<f32> {
    Vector2::new(
        (quad_w * 2) as f32 / display_w as f32,
        (quad_h * 2) as f32 / display_h as f32,
    )
}

pub fn screen_space_vertex_transform(
    display_w: u32,
    display_h: u32,
    quad_w: u32,
    quad_h: u32,
    pos_x: i32,
    pos_y: i32,
) -> Matrix4<f32> {
    let Vector2 { x: ndc_x, y: ndc_y } =
        screen_space_vertex_translate(display_w, display_h, pos_x, pos_y);

    let Vector2 {
        x: scale_x,
        y: scale_y,
    } = screen_space_vertex_scale(display_w, display_h, quad_w, quad_h);

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
        hud: HudState<'a>,
        overlay: Option<UiOverlay<'a>>,
    },
}

pub struct UiRenderer<'a> {
    console_renderer: ConsoleRenderer,
    menu_renderer: MenuRenderer,
    hud_renderer: HudRenderer,
    glyph_renderer: GlyphRenderer,
    quad_renderer: QuadRenderer,
    quad_uniform_blocks: RefCell<Vec<DynamicUniformBufferBlock<'a, QuadUniforms>>>,
}

impl<'a> UiRenderer<'a> {
    pub fn new(state: &GraphicsState<'a>, menu: &Menu) -> UiRenderer<'a> {
        UiRenderer {
            console_renderer: ConsoleRenderer::new(state),
            menu_renderer: MenuRenderer::new(state, menu),
            hud_renderer: HudRenderer::new(state),
            glyph_renderer: GlyphRenderer::new(state),
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
        quad_commands: &[QuadRendererCommand],
    ) {
        trace!("Updating UI uniform buffers");

        let quad_uniforms =
            self.quad_renderer
                .generate_uniforms(quad_commands, display_width, display_height);

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
        glyph_commands: &'pass mut Vec<GlyphRendererCommand>,
    ) where
        'a: 'pass,
    {
        let (hud_state, overlay) = match ui_state {
            UiState::Title { overlay } => (None, Some(overlay)),
            UiState::InGame { hud, overlay } => (Some(hud), overlay),
        };

        if let Some(hs) = hud_state {
            self.hud_renderer
                .generate_commands(state, time, hs, quad_commands, glyph_commands);
        }

        if let Some(o) = overlay {
            match o {
                UiOverlay::Menu(menu) => self.menu_renderer.generate_commands(
                    state,
                    menu,
                    time,
                    quad_commands,
                    glyph_commands,
                ),
                UiOverlay::Console(console) => self.console_renderer.generate_commands(
                    state,
                    console,
                    quad_commands,
                    glyph_commands,
                ),
            }
        }

        self.update_uniform_buffers(state, display_width, display_height, time, quad_commands);

        let glyph_instances =
            self.glyph_renderer
                .generate_instances(glyph_commands, display_width, display_height);
        state
            .queue()
            .write_buffer(state.glyph_instance_buffer(), 0, unsafe {
                any_slice_as_bytes(&glyph_instances)
            });

        let quad_blocks = self.quad_uniform_blocks.borrow();
        self.quad_renderer
            .record_draw(state, pass, quad_commands, &quad_blocks);
        self.glyph_renderer
            .record_draw(state, pass, glyph_instances.len() as u32);
    }
}
