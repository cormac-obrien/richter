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

pub mod alias;
pub mod bitmap;
pub mod brush;
pub mod console;
pub mod glyph;
pub mod hud;
pub mod menu;
pub mod wgpu;
pub mod world;

use std::{
    cell::{Ref, RefCell, RefMut},
    collections::HashMap,
    io::BufReader,
    ops::DerefMut,
    rc::Rc,
};

use crate::{
    client::ClientEntity,
    common::{
        console::Console,
        model::{Model, ModelKind},
        vfs::Vfs,
        wad::{QPic, Wad},
    },
};

use byteorder::ReadBytesExt;
use cgmath::{Deg, Euler, Matrix3, Matrix4, SquareMatrix, Vector3, Vector4, Zero};
use chrono::Duration;
use failure::Error;
use flame;
use gfx::{
    self,
    format::{Unorm, R8, R8_G8_B8_A8},
    handle::{Buffer, DepthStencilView, RenderTargetView, Sampler, ShaderResourceView, Texture},
    pso::{PipelineData, PipelineState},
    texture::{self, SamplerInfo},
    traits::FactoryExt,
    IndexBuffer, Slice,
};
use gfx_device_gl::{Factory, Resources};

pub use gfx::format::{DepthStencil as DepthFormat, Srgba8 as ColorFormat};

use self::{
    alias::AliasRenderer, bitmap::BitmapTexture, brush::BrushRenderer, console::ConsoleRenderer,
    glyph::GlyphRenderer, world::WorldRenderer,
};

const PALETTE_SIZE: usize = 768;

// TODO: per-API coordinate system conversions
pub static VERTEX_SHADER_GLSL: &[u8] = br#"
#version 430

layout (location = 0) in vec3 a_Pos;
layout (location = 1) in vec2 a_Texcoord;

out vec2 f_texcoord;

uniform mat4 u_Transform;

void main() {
    f_texcoord = a_Texcoord;
    gl_Position = u_Transform * vec4(-a_Pos.y, a_Pos.z, -a_Pos.x, 1.0);
}
"#;

pub static FRAGMENT_SHADER_GLSL: &[u8] = br#"
#version 430

in vec2 f_texcoord;

uniform sampler2D u_Texture;

out vec4 Target0;

void main() {
    vec4 color = texture(u_Texture, f_texcoord);
    if (color.a == 0) {
        discard;
    } else {
        Target0 = color;
    }
}"#;

// TODO: per-API coordinate system conversions
pub static VERTEX_SHADER_2D_GLSL: &[u8] = br#"
#version 430

layout (location = 0) in vec2 a_Pos;
layout (location = 1) in vec2 a_Texcoord;

out vec2 f_texcoord;

uniform mat4 u_Transform;

void main() {
    f_texcoord = a_Texcoord;
    gl_Position = u_Transform * vec4(a_Pos.x, a_Pos.y, 0.0, 1.0);
}
"#;

pub static FRAGMENT_SHADER_2D_GLSL: &[u8] = br#"
#version 430

in vec2 f_texcoord;

uniform sampler2D u_Texture;

out vec4 Target0;

void main() {
    vec4 color = texture(u_Texture, f_texcoord);
    if (color.a == 0) {
        discard;
    } else {
        Target0 = color;
    }
}
"#;

pub fn screen_space_vertex_transform(
    display_w: u32,
    display_h: u32,
    quad_w: u32,
    quad_h: u32,
    pos_x: i32,
    pos_y: i32,
) -> Matrix4<f32> {
    // find center
    let center_x = pos_x + quad_w as i32 / 2;
    let center_y = pos_y + quad_h as i32 / 2;

    // rescale from [0, DISPLAY_*] to [-1, 1] (NDC)
    // TODO: this may break on APIs other than OpenGL
    let ndc_x = (center_x * 2 - display_w as i32) as f32 / display_w as f32;
    let ndc_y = (center_y * 2 - display_h as i32) as f32 / display_h as f32;

    let scale_x = quad_w as f32 / display_w as f32;
    let scale_y = quad_h as f32 / display_h as f32;

    Matrix4::from_translation([ndc_x, ndc_y, 0.0].into())
        * Matrix4::from_nonuniform_scale(scale_x, scale_y, 1.0)
}

/// Shared resources between different renderers.
pub struct GraphicsPackage {
    palette: Palette,
    gfx_wad: Wad,
    glyph_renderer: Rc<GlyphRenderer>,
    console_renderer: ConsoleRenderer,
    factory: RefCell<Factory>,
    color_target: RenderTargetView<Resources, ColorFormat>,
    depth_stencil: DepthStencilView<Resources, DepthFormat>,
    quad_vertex_buffer: Buffer<Resources, Vertex2d>,
    dummy_diffuse_texture: ShaderResourceView<Resources, [f32; 4]>,
    sampler: Sampler<Resources>,
    pipeline_2d: PipelineState2d,
}

impl GraphicsPackage {
    pub fn new(
        vfs: &Vfs,
        mut factory: Factory,
        color_target: RenderTargetView<Resources, ColorFormat>,
        depth_stencil: DepthStencilView<Resources, DepthFormat>,
        console: Rc<RefCell<Console>>,
    ) -> GraphicsPackage {
        let palette = Palette::load(&vfs, "gfx/palette.lmp");
        let gfx_wad = Wad::load(vfs.open("gfx.wad").unwrap()).unwrap();
        let quad_vertex_buffer = factory.create_vertex_buffer(&QUAD_VERTICES);

        let glyph_renderer = Rc::new(
            GlyphRenderer::new(&mut factory, &gfx_wad.open_conchars().unwrap(), &palette).unwrap(),
        );

        let console_renderer = ConsoleRenderer::new(
            &vfs,
            &mut factory,
            quad_vertex_buffer.clone(),
            &palette,
            console.clone(),
            glyph_renderer.clone(),
        )
        .unwrap();

        let (_handle, dummy_diffuse_texture) = create_dummy_texture(&mut factory).unwrap();

        use gfx::{texture::WrapMode, Factory};
        let sampler = factory.create_sampler(SamplerInfo::new(
            gfx::texture::FilterMethod::Scale,
            WrapMode::Tile,
        ));

        let shader_set_2d = factory
            .create_shader_set(VERTEX_SHADER_2D_GLSL, FRAGMENT_SHADER_2D_GLSL)
            .unwrap();

        let rasterizer_2d = gfx::state::Rasterizer {
            front_face: gfx::state::FrontFace::Clockwise,
            cull_face: gfx::state::CullFace::Back,
            method: gfx::state::RasterMethod::Fill,
            offset: None,
            samples: Some(gfx::state::MultiSample),
        };

        let pipeline_2d = factory
            .create_pipeline_state(
                &shader_set_2d,
                gfx::Primitive::TriangleList,
                rasterizer_2d,
                pipeline2d::new(),
            )
            .unwrap();

        GraphicsPackage {
            palette,
            gfx_wad,
            glyph_renderer,
            console_renderer,
            factory: RefCell::new(factory),
            color_target,
            depth_stencil,
            quad_vertex_buffer,
            dummy_diffuse_texture,
            sampler,
            pipeline_2d,
        }
    }

    pub fn texture_from_qpic<S>(&self, vfs: &Vfs, name: S) -> BitmapTexture
    where
        S: AsRef<str>,
    {
        BitmapTexture::from_qpic(
            self.factory.borrow_mut().deref_mut(),
            &QPic::load(vfs.open(name).unwrap()).unwrap(),
            &self.palette,
        )
        .unwrap()
    }

    pub fn palette(&self) -> &Palette {
        &self.palette
    }

    pub fn gfx_wad(&self) -> &Wad {
        &self.gfx_wad
    }

    pub fn glyph_renderer(&self) -> &GlyphRenderer {
        &self.glyph_renderer
    }

    pub fn console_renderer(&self) -> &ConsoleRenderer {
        &self.console_renderer
    }

    pub fn factory(&self) -> Ref<Factory> {
        self.factory.borrow()
    }

    pub fn factory_mut(&self) -> RefMut<Factory> {
        self.factory.borrow_mut()
    }

    pub fn color_target(&self) -> RenderTargetView<Resources, ColorFormat> {
        self.color_target.clone()
    }

    pub fn depth_stencil(&self) -> DepthStencilView<Resources, DepthFormat> {
        self.depth_stencil.clone()
    }

    pub fn quad_vertex_buffer(&self) -> Buffer<Resources, Vertex2d> {
        self.quad_vertex_buffer.clone()
    }

    pub fn dummy_diffuse_texture(&self) -> ShaderResourceView<Resources, [f32; 4]> {
        self.dummy_diffuse_texture.clone()
    }

    pub fn sampler(&self) -> Sampler<Resources> {
        self.sampler.clone()
    }

    pub fn pipeline_2d(&self) -> &PipelineState2d {
        &self.pipeline_2d
    }

    pub fn gen_user_data_2d(&self) -> PipelineData2d {
        PipelineData2d {
            vertex_buffer: self.quad_vertex_buffer(),
            transform: Matrix4::identity().into(),
            sampler: (self.dummy_diffuse_texture(), self.sampler()),
            out_color: self.color_target(),
            out_depth: self.depth_stencil(),
        }
    }
}

// these have to be wound clockwise
static QUAD_VERTICES: [Vertex2d; 6] = [
    Vertex2d {
        pos: [-1.0, -1.0],
        texcoord: [0.0, 1.0],
    }, // bottom left
    Vertex2d {
        pos: [-1.0, 1.0],
        texcoord: [0.0, 0.0],
    }, // top left
    Vertex2d {
        pos: [1.0, 1.0],
        texcoord: [1.0, 0.0],
    }, // top right
    Vertex2d {
        pos: [-1.0, -1.0],
        texcoord: [0.0, 1.0],
    }, // bottom left
    Vertex2d {
        pos: [1.0, 1.0],
        texcoord: [1.0, 0.0],
    }, // top right
    Vertex2d {
        pos: [1.0, -1.0],
        texcoord: [1.0, 1.0],
    }, // bottom right
];

static QUAD_SLICE: Slice<Resources> = Slice {
    start: 0,
    end: 6,
    base_vertex: 0,
    instances: None,
    buffer: IndexBuffer::Auto,
};

gfx_defines! {
    vertex Vertex {
        pos: [f32; 3] = "a_Pos",
        texcoord: [f32; 2] = "a_Texcoord",
    }

    constant Locals {
        transform: [[f32; 4]; 4] = "u_Transform",
    }

    pipeline pipe {
        vertex_buffer: gfx::VertexBuffer<Vertex> = (),
        transform: gfx::Global<[[f32; 4]; 4]> = "u_Transform",
        sampler: gfx::TextureSampler<[f32; 4]> = "u_Texture",
        out_color: gfx::RenderTarget<ColorFormat> = "Target0",
        out_depth: gfx::DepthTarget<DepthFormat> = gfx::preset::depth::LESS_EQUAL_WRITE,
    }
}

gfx_defines! {
    vertex Vertex2d {
        pos: [f32; 2] = "a_Pos",
        texcoord: [f32; 2] = "a_Texcoord",
    }

    constant Locals2d {
        transform: [[f32; 4]; 4] = "u_Transform",
    }

    pipeline pipeline2d {
        vertex_buffer: gfx::VertexBuffer<Vertex2d> = (),
        transform: gfx::Global<[[f32; 4]; 4]> = "u_Transform",
        sampler: gfx::TextureSampler<[f32; 4]> = "u_Texture",
        out_color: gfx::RenderTarget<ColorFormat> = "Target0",
        out_depth: gfx::DepthTarget<DepthFormat> = gfx::preset::depth::PASS_TEST,
    }
}

pub type PipelineState2d =
    PipelineState<Resources, <pipeline2d::Data<Resources> as PipelineData<Resources>>::Meta>;
pub type PipelineData2d = pipeline2d::Data<Resources>;

pub struct Camera {
    origin: Vector3<f32>,
    angles: Vector3<Deg<f32>>,
    projection: Matrix4<f32>,

    transform: Matrix4<f32>,
}

impl Camera {
    pub fn new(
        origin: Vector3<f32>,
        angles: Vector3<Deg<f32>>,
        projection: Matrix4<f32>,
    ) -> Camera {
        // negate the camera origin and angles
        // TODO: the OpenGL coordinate conversion is hardcoded here! XXX
        let converted_origin = Vector3::new(-origin.y, origin.z, -origin.x);
        let translation = Matrix4::from_translation(-converted_origin);
        let rotation = Matrix4::from(Euler::new(angles.x, -angles.y, -angles.z));

        Camera {
            origin,
            angles,
            projection,
            transform: projection * rotation * translation,
        }
    }

    pub fn origin(&self) -> Vector3<f32> {
        self.origin
    }

    pub fn angles(&self) -> Vector3<Deg<f32>> {
        self.angles
    }

    pub fn transform(&self) -> Matrix4<f32> {
        self.transform
    }
}

pub struct SceneRenderer {
    pipeline: PipelineState<Resources, <pipe::Data<Resources> as PipelineData<Resources>>::Meta>,
    world_renderer: WorldRenderer,
    brush_renderers: HashMap<usize, BrushRenderer>,
    alias_renderers: HashMap<usize, AliasRenderer>,
    // spr_renderers: ...,
}

impl SceneRenderer {
    pub fn new(
        models: &[Model],
        worldmodel_id: usize,
        gfx_pkg: &mut GraphicsPackage,
    ) -> Result<SceneRenderer, Error> {
        use gfx::traits::FactoryExt;
        let shader_set = gfx_pkg
            .factory_mut()
            .create_shader_set(VERTEX_SHADER_GLSL, FRAGMENT_SHADER_GLSL)
            .unwrap();

        let rasterizer = gfx::state::Rasterizer {
            front_face: gfx::state::FrontFace::Clockwise,
            cull_face: gfx::state::CullFace::Back,
            method: gfx::state::RasterMethod::Fill,
            offset: None,
            samples: Some(gfx::state::MultiSample),
        };

        let pipeline = gfx_pkg
            .factory_mut()
            .create_pipeline_state(
                &shader_set,
                gfx::Primitive::TriangleList,
                rasterizer,
                pipe::new(),
            )
            .unwrap();

        let mut maybe_world_renderer = None;
        let mut brush_renderers = HashMap::new();
        let mut alias_renderers = HashMap::new();
        for (i, model) in models.iter().enumerate() {
            if i == worldmodel_id {
                match *model.kind() {
                    ModelKind::Brush(ref bmodel) => {
                        debug!("model {}: world model", i);
                        maybe_world_renderer = Some(WorldRenderer::new(
                            &bmodel,
                            gfx_pkg.palette(),
                            gfx_pkg.factory_mut().deref_mut(),
                            gfx_pkg.color_target(),
                            gfx_pkg.depth_stencil(),
                        )?);
                    }

                    _ => bail!("Invalid kind for worldmodel"),
                }
            } else {
                match *model.kind() {
                    ModelKind::Brush(ref bmodel) => {
                        debug!("model {}: brush model", i);
                        brush_renderers.insert(
                            i,
                            BrushRenderer::new(
                                &bmodel,
                                gfx_pkg.palette(),
                                gfx_pkg.factory_mut().deref_mut(),
                                gfx_pkg.color_target(),
                                gfx_pkg.depth_stencil(),
                            )?,
                        );
                    }

                    ModelKind::Alias(ref amodel) => {
                        debug!("model {}: alias model", i);
                        alias_renderers.insert(
                            i,
                            AliasRenderer::new(
                                &amodel,
                                gfx_pkg.palette(),
                                gfx_pkg.factory_mut().deref_mut(),
                            )?,
                        );
                    }

                    // TODO handle sprite and null models
                    ModelKind::Sprite(_) => debug!("model {}: sprite model", i),
                    _ => (),
                }
            }
        }

        let world_renderer = match maybe_world_renderer {
            Some(w) => w,
            None => bail!("No worldmodel provided"),
        };

        Ok(SceneRenderer {
            pipeline,
            world_renderer,
            brush_renderers,
            alias_renderers,
        })
    }

    pub fn render<C>(
        &self,
        encoder: &mut gfx::Encoder<Resources, C>,
        user_data: &mut pipe::Data<Resources>,
        entities: &[ClientEntity],
        view_ent_id: usize,
        view_model_id: usize,
        time: Duration,
        camera: &Camera,
        lightstyle_values: &[f32],
    ) -> Result<(), Error>
    where
        C: gfx::CommandBuffer<Resources>,
    {
        flame::start("render_world");
        self.world_renderer.render(
            encoder,
            time,
            camera,
            Vector3::zero(),
            Vector3::new(Deg(0.0), Deg(0.0), Deg(0.0)),
            lightstyle_values,
        )?;
        flame::end("render_world");

        flame::start("render_entities");
        for (ent_id, ent) in entities.iter().enumerate() {
            // draw viewmodel in first person perspective
            if ent_id == view_ent_id {
                let mut angles = camera.angles();
                angles.x = -angles.x;
                if let Some(ref alias_renderer) = self.alias_renderers.get(&view_model_id) {
                    // TODO: need keyframe, texture ID
                    // also need to disable depth testing to stop viewmodel clipping into walls
                    alias_renderer.render(
                        encoder,
                        &self.pipeline,
                        user_data,
                        time,
                        camera,
                        camera.origin(),
                        camera.angles(),
                        0,
                        0,
                    )?;
                }
                continue;
            }

            let model_id = ent.get_model_id();
            if let Some(ref brush_renderer) = self.brush_renderers.get(&model_id) {
                brush_renderer.render(
                    encoder,
                    time,
                    camera,
                    ent.get_origin(),
                    ent.get_angles(),
                    lightstyle_values,
                )?;
            } else if let Some(ref alias_renderer) = self.alias_renderers.get(&model_id) {
                // TODO: pull keyframe and texture ID
                alias_renderer.render(
                    encoder,
                    &self.pipeline,
                    user_data,
                    time,
                    camera,
                    ent.get_origin(),
                    ent.get_angles(),
                    0,
                    0,
                )?;
            }
        }
        flame::end("render_entities");

        Ok(())
    }
}

pub struct Palette {
    rgb: [[u8; 3]; 256],
}

impl Palette {
    pub fn load<S>(vfs: &Vfs, path: S) -> Palette
    where
        S: AsRef<str>,
    {
        let mut data = BufReader::new(vfs.open(path).unwrap());

        let mut rgb = [[0u8; 3]; 256];

        for color in 0..256 {
            for component in 0..3 {
                rgb[color][component] = data.read_u8().unwrap();
            }
        }

        Palette { rgb }
    }

    // TODO: this will not render console characters correctly, as they use index 0 (black) to
    // indicate transparency.
    /// Translates a set of indices into a list of RGBA values and a list of fullbright values.
    pub fn translate(&self, indices: &[u8]) -> (Vec<u8>, Vec<u8>) {
        let mut rgba = Vec::with_capacity(indices.len() * 4);
        let mut fullbright = Vec::with_capacity(indices.len());

        for index in indices {
            match *index {
                0xFF => {
                    for _ in 0..4 {
                        rgba.push(0);
                        fullbright.push(0);
                    }
                }

                i => {
                    for component in 0..3 {
                        rgba.push(self.rgb[*index as usize][component]);
                    }
                    rgba.push(0xFF);

                    fullbright.push(if i > 223 { 0xFF } else { 0 });
                }
            }
        }

        (rgba, fullbright)
    }
}

pub fn create_texture<F>(
    factory: &mut F,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> Result<
    (
        Texture<Resources, R8_G8_B8_A8>,
        ShaderResourceView<Resources, [f32; 4]>,
    ),
    Error,
>
where
    F: gfx::Factory<Resources>,
{
    ensure!(
        (width * height * 4) as usize == rgba.len(),
        "Invalid dimensions for texture"
    );
    let ret = factory.create_texture_immutable_u8::<ColorFormat>(
        gfx::texture::Kind::D2(width as u16, height as u16, gfx::texture::AaMode::Single),
        gfx::texture::Mipmap::Allocated,
        &[&rgba],
    )?;

    Ok(ret)
}

pub fn create_dummy_texture<F>(
    factory: &mut F,
) -> Result<
    (
        Texture<Resources, R8_G8_B8_A8>,
        ShaderResourceView<Resources, [f32; 4]>,
    ),
    Error,
>
where
    F: gfx::Factory<Resources>,
{
    // the infamous Source engine "missing texture" texture
    let rgba = [
        0xFF, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0xFF, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0xFF,
        0xFF,
    ];

    let ret = factory.create_texture_immutable_u8::<ColorFormat>(
        gfx::texture::Kind::D2(2, 2, gfx::texture::AaMode::Single),
        gfx::texture::Mipmap::Allocated,
        &[&rgba],
    )?;

    Ok(ret)
}

pub fn create_dummy_fullbright<F>(
    factory: &mut F,
) -> Result<(Texture<Resources, R8>, ShaderResourceView<Resources, f32>), Error>
where
    F: gfx::Factory<Resources>,
{
    let ret = factory.create_texture_immutable_u8::<(R8, Unorm)>(
        texture::Kind::D2(1, 1, texture::AaMode::Single),
        texture::Mipmap::Allocated,
        &[&[0]],
    )?;

    Ok(ret)
}

pub fn create_dummy_lightmap<F>(
    factory: &mut F,
) -> Result<(Texture<Resources, R8>, ShaderResourceView<Resources, f32>), Error>
where
    F: gfx::Factory<Resources>,
{
    let ret = factory.create_texture_immutable_u8::<(R8, Unorm)>(
        texture::Kind::D2(1, 1, texture::AaMode::Single),
        texture::Mipmap::Allocated,
        &[&[0xFF]],
    )?;

    Ok(ret)
}
