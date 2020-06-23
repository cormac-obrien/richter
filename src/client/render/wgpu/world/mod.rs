pub mod alias;
pub mod brush;
pub mod sprite;

use std::{cell::RefCell, mem::size_of, rc::Rc};

use crate::{
    client::{
        render::wgpu::{
            uniform::{DynamicUniformBufferBlock, UniformArrayFloat},
            world::{
                alias::AliasRenderer,
                brush::{BrushRenderer, BrushRendererBuilder},
                sprite::SpriteRenderer,
            },
            GraphicsState,
        },
        ClientEntity,
    },
    common::{
        engine,
        model::{Model, ModelKind},
        sprite::SpriteKind,
        util::any_as_bytes,
    },
};

use cgmath::{Deg, Euler, Matrix4, SquareMatrix as _, Vector3, Vector4};
use chrono::Duration;

lazy_static! {
    static ref BIND_GROUP_LAYOUT_DESCRIPTOR_BINDINGS: [Vec<wgpu::BindGroupLayoutEntry>; 2] = [
        vec![
            wgpu::BindGroupLayoutEntry::new(
                0,
                wgpu::ShaderStage::all(),
                wgpu::BindingType::UniformBuffer {
                    dynamic: false,
                    min_binding_size: Some(
                        std::num::NonZeroU64::new(size_of::<FrameUniforms>() as u64).unwrap(),
                    ),
                },
            ),
        ],
        vec![
            // transform matrix
            // TODO: move this to push constants once they're exposed in wgpu
            wgpu::BindGroupLayoutEntry::new(
                0,
                wgpu::ShaderStage::VERTEX,
                wgpu::BindingType::UniformBuffer {
                    dynamic: true,
                    min_binding_size: Some(
                        std::num::NonZeroU64::new(size_of::<EntityUniforms>() as u64)
                            .unwrap(),
                    ),
                },
            ),
            // diffuse and fullbright sampler
            wgpu::BindGroupLayoutEntry::new(
                1,
                wgpu::ShaderStage::FRAGMENT,
                wgpu::BindingType::Sampler { comparison: false },
            ),
            // lightmap sampler
            wgpu::BindGroupLayoutEntry::new(
                2,
                wgpu::ShaderStage::FRAGMENT,
                wgpu::BindingType::Sampler { comparison: false },
            ),
        ],
    ];

    pub static ref BIND_GROUP_LAYOUT_DESCRIPTORS: [wgpu::BindGroupLayoutDescriptor<'static>; 2] = [
        // group 0: updated per-frame
        wgpu::BindGroupLayoutDescriptor {
            label: Some("per-frame bind group"),
            bindings: &BIND_GROUP_LAYOUT_DESCRIPTOR_BINDINGS[0],
        },
        // group 1: updated per-entity
        wgpu::BindGroupLayoutDescriptor {
            label: Some("brush per-entity bind group"),
            bindings: &BIND_GROUP_LAYOUT_DESCRIPTOR_BINDINGS[1],
        },
    ];
}

#[derive(Clone, Copy, Debug)]
pub enum BindGroupLayoutId {
    PerFrame = 0,
    PerEntity = 1,
    PerTexture = 2,
    PerFace = 3,
}

pub struct Camera {
    origin: Vector3<f32>,
    angles: Vector3<Deg<f32>>,
    transform: Matrix4<f32>,
}

impl Camera {
    pub fn new(
        origin: Vector3<f32>,
        angles: Vector3<Deg<f32>>,
        projection: Matrix4<f32>,
    ) -> Camera {
        // convert coordinates
        let converted_origin = Vector3::new(-origin.y, origin.z, -origin.x);
        // translate the world by inverse of camera position
        let translation = Matrix4::from_translation(-converted_origin);
        let rotation = Matrix4::from(Euler::new(angles.x, -angles.y, -angles.z));

        Camera {
            origin,
            angles,
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

#[repr(C, align(256))]
#[derive(Copy, Clone)]
// TODO: derive Debug once const generics are stable
pub struct FrameUniforms {
    // TODO: pack frame values into a [Vector4<f32>; 16],
    lightmap_anim_frames: [UniformArrayFloat; 64],
    camera_pos: Vector4<f32>,
    time: f32,
}

#[repr(C, align(256))]
#[derive(Clone, Copy, Debug)]
pub struct EntityUniforms {
    transform: Matrix4<f32>,
}

enum EntityRenderer<'a> {
    Alias(AliasRenderer),
    Brush(BrushRenderer<'a>),
    Sprite(SpriteRenderer),
    None,
}

/// Top-level renderer.
pub struct WorldRenderer<'a> {
    state: Rc<GraphicsState<'a>>,

    worldmodel_renderer: BrushRenderer<'a>,
    entity_renderers: Vec<EntityRenderer<'a>>,

    world_uniform_block: DynamicUniformBufferBlock<'a, EntityUniforms>,
    entity_uniform_blocks: RefCell<Vec<DynamicUniformBufferBlock<'a, EntityUniforms>>>,
}

impl<'a> WorldRenderer<'a> {
    pub fn new(
        models: &[Model],
        worldmodel_id: usize,
        state: Rc<GraphicsState<'a>>,
    ) -> WorldRenderer<'a> {
        let mut worldmodel_renderer = None;
        let mut entity_renderers = Vec::new();

        let world_uniform_block = state.entity_uniform_buffer_mut().allocate(EntityUniforms {
            transform: Matrix4::identity(),
        });

        for (i, model) in models.iter().enumerate() {
            if i == worldmodel_id {
                match *model.kind() {
                    ModelKind::Brush(ref bmodel) => {
                        worldmodel_renderer = Some(
                            BrushRendererBuilder::new(bmodel, state.clone(), true)
                                .build()
                                .unwrap(),
                        );
                    }
                    _ => panic!("Invalid worldmodel"),
                }
            } else {
                match *model.kind() {
                    ModelKind::Alias(ref amodel) => entity_renderers.push(EntityRenderer::Alias(
                        AliasRenderer::new(state.clone(), amodel).unwrap(),
                    )),

                    ModelKind::Brush(ref bmodel) => {
                        entity_renderers.push(EntityRenderer::Brush(
                            BrushRendererBuilder::new(bmodel, state.clone(), false)
                                .build()
                                .unwrap(),
                        ));
                    }

                    ModelKind::Sprite(ref smodel) => {
                        entity_renderers
                            .push(EntityRenderer::Sprite(SpriteRenderer::new(&state, smodel)));
                    }

                    _ => {
                        warn!("Non-brush renderers not implemented!");
                        entity_renderers.push(EntityRenderer::None);
                    }
                }
            }
        }

        WorldRenderer {
            state: state.clone(),
            worldmodel_renderer: worldmodel_renderer.unwrap(),
            entity_renderers,
            world_uniform_block,
            entity_uniform_blocks: RefCell::new(Vec::new()),
        }
    }

    pub fn update_uniform_buffers<'b, I>(
        &'b self,
        camera: &Camera,
        time: Duration,
        entities: I,
        lightstyle_values: &[f32],
    ) where
        I: Iterator<Item = &'b ClientEntity>,
    {
        let _guard = flame::start_guard("Renderer::update_uniform");

        println!("time = {:?}", engine::duration_to_f32(time));
        trace!("Updating frame uniform buffer");
        self.state
            .queue()
            .write_buffer(self.state.frame_uniform_buffer(), 0, unsafe {
                any_as_bytes(&FrameUniforms {
                    lightmap_anim_frames: {
                        let mut frames = [UniformArrayFloat::new(0.0); 64];
                        for i in 0..64 {
                            frames[i] = UniformArrayFloat::new(lightstyle_values[i]);
                        }
                        frames
                    },
                    camera_pos: camera.origin.extend(1.0),
                    time: engine::duration_to_f32(time),
                })
            });

        trace!("Updating entity uniform buffer");
        let world_uniforms = EntityUniforms {
            transform: camera.transform(),
        };
        self.state
            .entity_uniform_buffer_mut()
            .write_block(&self.world_uniform_block, world_uniforms);

        for (ent_pos, ent) in entities.into_iter().enumerate() {
            let ent_uniforms = EntityUniforms {
                transform: self.calculate_transform(camera, ent),
            };

            if ent_pos >= self.entity_uniform_blocks.borrow().len() {
                // if we don't have enough blocks, get a new one
                let block = self
                    .state
                    .entity_uniform_buffer_mut()
                    .allocate(ent_uniforms);
                self.entity_uniform_blocks.borrow_mut().push(block);
            } else {
                self.state
                    .entity_uniform_buffer_mut()
                    .write_block(&self.entity_uniform_blocks.borrow()[ent_pos], ent_uniforms);
            }
        }

        self.state.entity_uniform_buffer().flush(self.state.queue());
    }

    pub fn render_pass<'b, I>(
        &'b self,
        pass: &mut wgpu::RenderPass<'b>,
        camera: &Camera,
        time: Duration,
        entities: I,
        lightstyle_values: &[f32],
    ) where
        I: Iterator<Item = &'b ClientEntity> + Clone,
    {
        let _guard = flame::start_guard("Renderer::render_pass");
        {
            info!("Updating uniform buffers");
            self.update_uniform_buffers(camera, time, entities.clone(), lightstyle_values);

            pass.set_bind_group(
                BindGroupLayoutId::PerFrame as u32,
                &self.state.world_bind_groups()[BindGroupLayoutId::PerFrame as usize],
                &[],
            );

            // draw world
            info!("Drawing world");
            pass.set_bind_group(
                BindGroupLayoutId::PerEntity as u32,
                &self.state.world_bind_groups()[BindGroupLayoutId::PerEntity as usize],
                &[self.world_uniform_block.offset()],
            );
            self.worldmodel_renderer.record_draw(pass, camera);

            // draw entities
            info!("Drawing entities");
            for (ent_pos, ent) in entities.enumerate() {
                pass.set_bind_group(
                    BindGroupLayoutId::PerEntity as u32,
                    &self.state.world_bind_groups()[BindGroupLayoutId::PerEntity as usize],
                    &[self.entity_uniform_blocks.borrow()[ent_pos].offset()],
                );

                match self.renderer_for_entity(&ent) {
                    EntityRenderer::Brush(ref bmodel) => bmodel.record_draw(pass, camera),
                    EntityRenderer::Alias(ref alias) => alias.record_draw(
                        &self.state,
                        pass,
                        time,
                        ent.get_frame_id(),
                        ent.get_skin_id(),
                    ),
                    EntityRenderer::Sprite(ref sprite) => {
                        sprite.record_draw(&self.state, pass, ent.get_frame_id(), time)
                    }
                    _ => warn!("non-brush renderers not implemented!"),
                    // _ => unimplemented!(),
                }
            }
        }
    }

    fn renderer_for_entity(&self, ent: &ClientEntity) -> &EntityRenderer<'a> {
        // subtract 1 from index because world entity isn't counted
        &self.entity_renderers[ent.get_model_id() - 1]
    }

    fn calculate_transform(&self, camera: &Camera, entity: &ClientEntity) -> Matrix4<f32> {
        let origin = entity.get_origin();
        let angles = entity.get_angles();
        let euler = match self.renderer_for_entity(entity) {
            EntityRenderer::Sprite(ref sprite) => match sprite.kind() {
                // used for decals
                SpriteKind::Oriented => Euler::new(angles.x, angles.y, angles.z),

                _ => {
                    // keep sprite facing player, but preserve roll
                    let inv_cam_angles = -camera.angles();
                    Euler::new(inv_cam_angles.x, inv_cam_angles.y, angles.z)
                }
            },

            _ => Euler::new(angles.x, angles.y, angles.z),
        };

        camera.transform()
            * Matrix4::from_translation(Vector3::new(-origin.y, origin.z, -origin.x))
            * Matrix4::from(euler)
    }
}
