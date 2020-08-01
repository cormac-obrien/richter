// Copyright © 2020 Cormac O'Brien.
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

use crate::client::render::{
    Extent2d, DEPTH_ATTACHMENT_FORMAT, DIFFUSE_ATTACHMENT_FORMAT, LIGHT_ATTACHMENT_FORMAT,
    NORMAL_ATTACHMENT_FORMAT,
};

// TODO: collapse these into a single definition
/// Create a texture suitable for use as a color attachment.
///
/// The resulting texture will have the OUTPUT_ATTACHMENT flag as well as
/// any flags specified by `usage`.
pub fn create_color_attachment(
    device: &wgpu::Device,
    size: Extent2d,
    sample_count: u32,
    usage: wgpu::TextureUsage,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("color attachment"),
        size: size.into(),
        mip_level_count: 1,
        sample_count,
        dimension: wgpu::TextureDimension::D2,
        format: DIFFUSE_ATTACHMENT_FORMAT,
        usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT | usage,
    })
}

/// Create a texture suitable for use as a normal attachment.
///
/// The resulting texture will have the OUTPUT_ATTACHMENT flag as well as
/// any flags specified by `usage`.
pub fn create_normal_attachment(
    device: &wgpu::Device,
    size: Extent2d,
    sample_count: u32,
    usage: wgpu::TextureUsage,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("normal attachment"),
        size: size.into(),
        mip_level_count: 1,
        sample_count,
        dimension: wgpu::TextureDimension::D2,
        format: NORMAL_ATTACHMENT_FORMAT,
        usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT | usage,
    })
}

/// Create a texture suitable for use as a light attachment.
///
/// The resulting texture will have the OUTPUT_ATTACHMENT flag as well as
/// any flags specified by `usage`.
pub fn create_light_attachment(
    device: &wgpu::Device,
    size: Extent2d,
    sample_count: u32,
    usage: wgpu::TextureUsage,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("light attachment"),
        size: size.into(),
        mip_level_count: 1,
        sample_count,
        dimension: wgpu::TextureDimension::D2,
        format: LIGHT_ATTACHMENT_FORMAT,
        usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT | usage,
    })
}

/// Create a texture suitable for use as a depth attachment.
///
/// The underlying texture will have the OUTPUT_ATTACHMENT flag as well as
/// any flags specified by `usage`.
pub fn create_depth_attachment(
    device: &wgpu::Device,
    size: Extent2d,
    sample_count: u32,
    usage: wgpu::TextureUsage,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth attachment"),
        size: size.into(),
        mip_level_count: 1,
        sample_count,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_ATTACHMENT_FORMAT,
        usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT | usage,
    })
}

/// Intermediate object that can generate `RenderPassDescriptor`s.
pub struct RenderPassBuilder<'a> {
    color_attachments: Vec<wgpu::RenderPassColorAttachmentDescriptor<'a>>,
    depth_attachment: Option<wgpu::RenderPassDepthStencilAttachmentDescriptor<'a>>,
}

impl<'a> RenderPassBuilder<'a> {
    pub fn descriptor(&self) -> wgpu::RenderPassDescriptor {
        wgpu::RenderPassDescriptor {
            color_attachments: &self.color_attachments,
            depth_stencil_attachment: self.depth_attachment.clone(),
        }
    }
}

/// A trait describing a render target.
///
/// A render target consists of a series of color attachments and an optional depth-stencil
/// attachment.
pub trait RenderTarget {
    fn render_pass_builder<'a>(&'a self) -> RenderPassBuilder<'a>;
}

/// A trait describing a render target with a built-in resolve attachment.
pub trait RenderTargetResolve: RenderTarget {
    fn resolve_attachment(&self) -> &wgpu::Texture;
    fn resolve_view(&self) -> &wgpu::TextureView;
}

// TODO: use ArrayVec<wgpu::TextureView> in concrete types so it can be passed
// as Cow::Borrowed in RenderPassDescriptor

/// Render target for the initial world pass.
pub struct InitialPassTarget {
    size: Extent2d,
    sample_count: u32,
    diffuse_attachment: wgpu::Texture,
    diffuse_view: wgpu::TextureView,
    normal_attachment: wgpu::Texture,
    normal_view: wgpu::TextureView,
    light_attachment: wgpu::Texture,
    light_view: wgpu::TextureView,
    depth_attachment: wgpu::Texture,
    depth_view: wgpu::TextureView,
}

impl InitialPassTarget {
    pub fn new(device: &wgpu::Device, size: Extent2d, sample_count: u32) -> InitialPassTarget {
        let diffuse_attachment =
            create_color_attachment(device, size, sample_count, wgpu::TextureUsage::SAMPLED);
        let normal_attachment =
            create_normal_attachment(device, size, sample_count, wgpu::TextureUsage::SAMPLED);
        let light_attachment =
            create_light_attachment(device, size, sample_count, wgpu::TextureUsage::SAMPLED);
        let depth_attachment =
            create_depth_attachment(device, size, sample_count, wgpu::TextureUsage::SAMPLED);

        let diffuse_view = diffuse_attachment.create_default_view();
        let normal_view = normal_attachment.create_default_view();
        let light_view = light_attachment.create_default_view();
        let depth_view = depth_attachment.create_default_view();

        InitialPassTarget {
            size,
            sample_count,
            diffuse_attachment,
            diffuse_view,
            normal_attachment,
            normal_view,
            light_attachment,
            light_view,
            depth_attachment,
            depth_view,
        }
    }

    pub fn size(&self) -> Extent2d {
        self.size
    }

    pub fn sample_count(&self) -> u32 {
        self.sample_count
    }

    pub fn diffuse_attachment(&self) -> &wgpu::Texture {
        &self.diffuse_attachment
    }

    pub fn diffuse_view(&self) -> &wgpu::TextureView {
        &self.diffuse_view
    }

    pub fn normal_attachment(&self) -> &wgpu::Texture {
        &self.normal_attachment
    }

    pub fn normal_view(&self) -> &wgpu::TextureView {
        &self.normal_view
    }

    pub fn light_attachment(&self) -> &wgpu::Texture {
        &self.light_attachment
    }

    pub fn light_view(&self) -> &wgpu::TextureView {
        &self.light_view
    }

    pub fn depth_attachment(&self) -> &wgpu::Texture {
        &self.depth_attachment
    }

    pub fn depth_view(&self) -> &wgpu::TextureView {
        &self.depth_view
    }
}

impl RenderTarget for InitialPassTarget {
    fn render_pass_builder<'a>(&'a self) -> RenderPassBuilder {
        RenderPassBuilder {
            color_attachments: vec![
                wgpu::RenderPassColorAttachmentDescriptor {
                    attachment: self.diffuse_view(),
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: true,
                    },
                },
                wgpu::RenderPassColorAttachmentDescriptor {
                    attachment: self.normal_view(),
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: true,
                    },
                },
                wgpu::RenderPassColorAttachmentDescriptor {
                    attachment: self.light_view(),
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: true,
                    },
                },
            ],
            depth_attachment: Some(wgpu::RenderPassDepthStencilAttachmentDescriptor {
                attachment: self.depth_view(),
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: true,
                }),
                stencil_ops: None,
            }),
        }
    }
}

pub struct DeferredPassTarget {
    size: Extent2d,
    sample_count: u32,
    color_attachment: wgpu::Texture,
    color_view: wgpu::TextureView,
}

impl DeferredPassTarget {
    pub fn new(device: &wgpu::Device, size: Extent2d, sample_count: u32) -> DeferredPassTarget {
        let color_attachment =
            create_color_attachment(device, size, sample_count, wgpu::TextureUsage::SAMPLED);
        let color_view = color_attachment.create_default_view();

        DeferredPassTarget {
            size,
            sample_count,
            color_attachment,
            color_view,
        }
    }

    pub fn size(&self) -> Extent2d {
        self.size
    }

    pub fn sample_count(&self) -> u32 {
        self.sample_count
    }

    pub fn color_attachment(&self) -> &wgpu::Texture {
        &self.color_attachment
    }

    pub fn color_view(&self) -> &wgpu::TextureView {
        &self.color_view
    }
}

impl RenderTarget for DeferredPassTarget {
    fn render_pass_builder<'a>(&'a self) -> RenderPassBuilder {
        RenderPassBuilder {
            color_attachments: vec![wgpu::RenderPassColorAttachmentDescriptor {
                attachment: self.color_view(),
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: true,
                },
            }],
            depth_attachment: None,
        }
    }
}

pub struct FinalPassTarget {
    size: Extent2d,
    sample_count: u32,
    color_attachment: wgpu::Texture,
    color_view: wgpu::TextureView,
    resolve_attachment: wgpu::Texture,
    resolve_view: wgpu::TextureView,
}

impl FinalPassTarget {
    pub fn new(device: &wgpu::Device, size: Extent2d, sample_count: u32) -> FinalPassTarget {
        let color_attachment =
            create_color_attachment(device, size, sample_count, wgpu::TextureUsage::empty());
        let color_view = color_attachment.create_default_view();
        // add COPY_SRC so we can copy to a buffer for capture and SAMPLED so we
        // can blit to the swap chain
        let resolve_attachment = create_color_attachment(
            device,
            size,
            1,
            wgpu::TextureUsage::COPY_SRC | wgpu::TextureUsage::SAMPLED,
        );
        let resolve_view = resolve_attachment.create_default_view();

        FinalPassTarget {
            size,
            sample_count,
            color_attachment,
            color_view,
            resolve_attachment,
            resolve_view,
        }
    }

    pub fn size(&self) -> Extent2d {
        self.size
    }

    pub fn sample_count(&self) -> u32 {
        self.sample_count
    }
}

impl RenderTarget for FinalPassTarget {
    fn render_pass_builder<'a>(&'a self) -> RenderPassBuilder {
        RenderPassBuilder {
            color_attachments: vec![wgpu::RenderPassColorAttachmentDescriptor {
                attachment: &self.color_view,
                resolve_target: Some(self.resolve_view()),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: true,
                },
            }],
            depth_attachment: None,
        }
    }
}

impl RenderTargetResolve for FinalPassTarget {
    fn resolve_attachment(&self) -> &wgpu::Texture {
        &self.resolve_attachment
    }

    fn resolve_view(&self) -> &wgpu::TextureView {
        &self.resolve_view
    }
}

pub struct SwapChainTarget<'a> {
    swap_chain_view: &'a wgpu::TextureView,
}

impl<'a> SwapChainTarget<'a> {
    pub fn with_swap_chain_view(swap_chain_view: &'a wgpu::TextureView) -> SwapChainTarget<'a> {
        SwapChainTarget { swap_chain_view }
    }
}

impl<'a> RenderTarget for SwapChainTarget<'a> {
    fn render_pass_builder(&self) -> RenderPassBuilder {
        RenderPassBuilder {
            color_attachments: vec![wgpu::RenderPassColorAttachmentDescriptor {
                attachment: self.swap_chain_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: true,
                },
            }],
            depth_attachment: None,
        }
    }
}
