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

use crate::client::render::{Extent2d, DEPTH_ATTACHMENT_FORMAT, DIFFUSE_ATTACHMENT_FORMAT};

/// Create a texture suitable for use as a color attachment.
///
/// This texture can be resolved using a swap chain texture as its target.
///
/// If `sampled` is `true`, then this texture may also be sampled in a shader.
pub fn create_color_attachment(
    device: &wgpu::Device,
    size: Extent2d,
    sample_count: u32,
    sampled: bool,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("color attachment"),
        size: size.into(),
        mip_level_count: 1,
        sample_count,
        dimension: wgpu::TextureDimension::D2,
        format: DIFFUSE_ATTACHMENT_FORMAT,
        usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT
            | if sampled {
                wgpu::TextureUsage::SAMPLED
            } else {
                wgpu::TextureUsage::empty()
            },
    })
}

/// Create a texture suitable for use as a depth attachment.
///
/// If `sampled` is `true`, then this texture may also be sampled in a shader.
pub fn create_depth_attachment(
    device: &wgpu::Device,
    size: Extent2d,
    sample_count: u32,
    sampled: bool,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth attachment"),
        size: size.into(),
        mip_level_count: 1,
        sample_count,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_ATTACHMENT_FORMAT,
        usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT
            | if sampled {
                wgpu::TextureUsage::SAMPLED
            } else {
                wgpu::TextureUsage::empty()
            },
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
    fn render_pass_builder<'a>(
        &'a self,
        resolve_target: Option<&'a wgpu::TextureView>,
    ) -> RenderPassBuilder<'a>;
}

/// Render target for the initial world pass.
pub struct InitialPassTarget {
    size: Extent2d,
    sample_count: u32,
    diffuse_attachment: wgpu::Texture,
    diffuse_view: wgpu::TextureView,
    normal_attachment: wgpu::Texture,
    normal_view: wgpu::TextureView,
    depth_attachment: wgpu::Texture,
    depth_view: wgpu::TextureView,
}

impl InitialPassTarget {
    pub fn new(device: &wgpu::Device, size: Extent2d, sample_count: u32) -> InitialPassTarget {
        let diffuse_attachment = create_color_attachment(device, size, sample_count, true);
        let normal_attachment = create_color_attachment(device, size, sample_count, true);
        let depth_attachment = create_depth_attachment(device, size, sample_count, true);

        let diffuse_view = diffuse_attachment.create_default_view();
        let normal_view = normal_attachment.create_default_view();
        let depth_view = depth_attachment.create_default_view();

        InitialPassTarget {
            size,
            sample_count,
            diffuse_attachment,
            normal_attachment,
            depth_attachment,
            diffuse_view,
            normal_view,
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

    pub fn depth_attachment(&self) -> &wgpu::Texture {
        &self.depth_attachment
    }

    pub fn depth_view(&self) -> &wgpu::TextureView {
        &self.depth_view
    }
}

impl RenderTarget for InitialPassTarget {
    fn render_pass_builder<'a>(
        &'a self,
        resolve_target: Option<&'a wgpu::TextureView>,
    ) -> RenderPassBuilder {
        RenderPassBuilder {
            color_attachments: vec![
                wgpu::RenderPassColorAttachmentDescriptor {
                    attachment: self.diffuse_view(),
                    resolve_target: resolve_target,
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

pub struct FinalPassTarget {
    size: Extent2d,
    sample_count: u32,
    color_attachment: wgpu::Texture,
    color_view: wgpu::TextureView,
}

impl FinalPassTarget {
    pub fn new(device: &wgpu::Device, size: Extent2d, sample_count: u32) -> FinalPassTarget {
        let color_attachment = create_color_attachment(device, size, sample_count, false);
        let color_view = color_attachment.create_default_view();
        FinalPassTarget {
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
}

impl RenderTarget for FinalPassTarget {
    fn render_pass_builder<'a>(
        &'a self,
        resolve_target: Option<&'a wgpu::TextureView>,
    ) -> RenderPassBuilder {
        RenderPassBuilder {
            color_attachments: vec![wgpu::RenderPassColorAttachmentDescriptor {
                attachment: &self.color_view,
                resolve_target: resolve_target,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: true,
                },
            }],
            depth_attachment: None,
        }
    }
}
