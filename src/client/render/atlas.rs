use std::{cmp::Ordering, mem::size_of};

use crate::client::render::Palette;

use failure::Error;

const DEFAULT_ATLAS_DIM: u32 = 1024;

struct Rect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

fn area_order(t1: &TextureData, t2: &TextureData) -> Ordering {
    (t1.width * t1.height).cmp(&(t2.width * t2.height))
}

#[derive(Clone, Debug)]
pub struct TextureData {
    width: u32,
    height: u32,
    indexed: Vec<u8>,
}

impl TextureData {
    fn empty(width: u32, height: u32) -> TextureData {
        let len = (width * height) as usize;
        let mut indexed = Vec::with_capacity(len);
        indexed.resize(len, 0);

        TextureData {
            width,
            height,
            indexed,
        }
    }

    fn subtexture(&mut self, other: &TextureData, xy: [u32; 2]) -> Result<(), Error> {
        let [x, y] = xy;
        ensure!(x + other.width <= self.width);
        ensure!(y + other.height <= self.height);

        for r in 0..other.height {
            for c in 0..other.width {
                self.indexed[(self.width * (y + r) + x + c) as usize] =
                    other.indexed[(other.width * r + c) as usize];
            }
        }

        Ok(())
    }
}

pub struct TextureAtlasBuilder {
    textures: Vec<TextureData>,
}

impl TextureAtlasBuilder {
    pub fn new() -> TextureAtlasBuilder {
        TextureAtlasBuilder {
            textures: Vec::new(),
        }
    }

    pub fn add(&mut self, texture: TextureData) -> Result<usize, Error> {
        self.textures.push(texture);
        Ok(self.textures.len() - 1)
    }

    /// Constructs a TextureAtlas by efficiently packing multiple textures together.
    ///
    /// - Enumerate and sort the textures by total area, returning the sorted
    ///   list of textures and a corresponding list of each texture's original index
    /// - Create a list of available rectangular spaces in the atlas, starting with
    ///   the entire space.
    /// - For each texture, find a large enough space. Remove the space from the list,
    ///   splitting off any unnecessary space and returning that to the list. Add the
    ///   coordinates to a list of texture locations.
    /// - Sort the list of texture locations back into the original texture order.
    pub fn build(
        self,
        label: Option<&str>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        palette: &Palette,
    ) -> Result<TextureAtlas, Error> {
        let TextureAtlasBuilder { textures } = self;
        let mut enumerated_textures = textures
            .into_iter()
            .enumerate()
            .collect::<Vec<(usize, TextureData)>>();
        enumerated_textures.sort_unstable_by(|e1, e2| area_order(&e1.1, &e2.1));
        let (indices, textures): (Vec<usize>, Vec<TextureData>) =
            enumerated_textures.into_iter().unzip();

        let mut atlas = TextureData::empty(DEFAULT_ATLAS_DIM, DEFAULT_ATLAS_DIM);
        let mut spaces = vec![Rect {
            x: 0,
            y: 0,
            width: atlas.width,
            height: atlas.height,
        }];

        let mut subtextures: Vec<TextureAtlasSubtexture> = Vec::with_capacity(textures.len());

        // iterate in reverse: largest textures first
        for tex in textures.iter().rev() {
            let mut coords: Option<(u32, u32)> = None;

            // find a large enough space
            for i in (0..spaces.len()).rev() {
                use std::cmp::Ordering::*;

                // - find a large enough space to fit the current texture
                // - copy the texture into the space
                // - remove the space from the list of candidates
                // - split off any unused space and return it to the list
                let subtex = match (
                    spaces[i].width.cmp(&tex.width),
                    spaces[i].height.cmp(&tex.height),
                ) {
                    // if either dimension is too small, keep looking
                    (Less, _) | (_, Less) => continue,

                    // perfect fit!
                    (Equal, Equal) => {
                        let Rect { x, y, .. } = spaces.remove(i);
                        (x, y)
                    }

                    // split off the right side
                    (Greater, Equal) => {
                        let space = spaces.remove(i);
                        spaces.push(Rect {
                            x: space.x + tex.width,
                            y: space.y,
                            width: space.width - tex.width,
                            height: space.height,
                        });
                        (space.x, space.y)
                    }

                    // split off the bottom
                    (Equal, Greater) => {
                        let space = spaces.remove(i);
                        spaces.push(Rect {
                            x: space.x,
                            y: space.y + tex.height,
                            width: space.width,
                            height: space.height - tex.height,
                        });
                        (space.x, space.y)
                    }

                    // split off two spaces, maximizing the size of the large one
                    (Greater, Greater) => {
                        let space = spaces.remove(i);
                        let w_diff = space.width - tex.width;
                        let h_diff = space.height - tex.height;

                        let (space_a, space_b) = if w_diff > h_diff {
                            // =============
                            // |     |     |
                            // | tex |     |
                            // |     |  A  |
                            // |-----|     |
                            // |  B  |     |
                            // =============
                            (
                                Rect {
                                    // A
                                    x: space.x + tex.width,
                                    y: space.y,
                                    width: space.width - tex.width,
                                    height: space.height,
                                },
                                Rect {
                                    // B
                                    x: space.x,
                                    y: space.y + tex.height,
                                    width: tex.width,
                                    height: space.height - tex.height,
                                },
                            )
                        } else {
                            // =============
                            // |  tex  | B |
                            // |-----------|
                            // |           |
                            // |     A     |
                            // |           |
                            // =============
                            (
                                Rect {
                                    // A
                                    x: space.x,
                                    y: space.y + tex.height,
                                    width: space.width,
                                    height: space.height - tex.height,
                                },
                                Rect {
                                    // B
                                    x: space.x + tex.width,
                                    y: space.y,
                                    width: space.width - tex.width,
                                    height: tex.height,
                                },
                            )
                        };

                        // put the smaller space closer to the end
                        spaces.push(space_a);
                        spaces.push(space_b);

                        (space.x, space.y)
                    }
                };

                coords = Some(subtex);
            }

            match coords {
                Some((x, y)) => {
                    let base_s = x as f32 / atlas.width as f32;
                    let base_t = y as f32 / atlas.height as f32;
                    let subtex_w = tex.width as f32 / atlas.width as f32;
                    let subtex_h = tex.height as f32 / atlas.height as f32;
                    subtextures.push(TextureAtlasSubtexture {
                        base_xy: [x, y],
                        base_st: [base_s, base_t],
                        width: subtex_w,
                        height: subtex_h,
                    });
                }
                None => bail!("Can't pack all textures in an atlas this size!"),
            }
        }

        // copy the textures into the atlas
        for (subtex, tex) in subtextures.iter().rev().zip(textures.iter()) {
            atlas.subtexture(tex, subtex.base_xy)?;
        }

        let mut enumerated_subtextures: Vec<(usize, TextureAtlasSubtexture)> = indices
            .into_iter()
            .zip(subtextures.into_iter().rev())
            .collect();

        // sort back into the original order
        enumerated_subtextures.sort_unstable_by(|e1, e2| e1.0.cmp(&e2.0));
        let (_, subtextures): (Vec<usize>, Vec<TextureAtlasSubtexture>) =
            enumerated_subtextures.into_iter().unzip();

        let (rgba, fullbright) = palette.translate(&atlas.indexed);

        let diffuse_buffer = device.create_buffer_with_data(&rgba, wgpu::BufferUsages::COPY_SRC);
        let diffuse_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: atlas.width,
                height: atlas.height,
                depth_or_array_layers: 1,
            },
            array_layer_count: 1,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::NONE,
        });
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.copy_buffer_to_texture(
            wgpu::BufferCopyView {
                buffer: &diffuse_buffer,
                offset: 0,
                bytes_per_row: atlas.width * atlas.height * size_of::<[u8; 4]> as u32,
                rows_per_image: 1,
            },
            wgpu::ImageCopyTexture {
                texture: &diffuse_texture,
                mip_level: 1,
                array_layer: 0,
                origin: wgpu::Origin3d::ZERO,
            },
            wgpu::Extent3d {
                width: atlas.width,
                height: atlas.height,
                depth_or_array_layers: 1,
            },
        );
        let cmd_buffer = encoder.finish();
        queue.submit(&[cmd_buffer]);

        Ok(TextureAtlas {
            atlas: diffuse_texture,
            width: atlas.width,
            height: atlas.height,
            subtextures,
        })
    }
}

struct TextureAtlasSubtexture {
    // base subtexture coordinates in the atlas pixel space
    base_xy: [u32; 2],

    // base subtexture coordinates in the atlas texel space
    base_st: [f32; 2],

    // dimensions of the subtexture in atlas texel space
    width: f32,
    height: f32,
}

impl TextureAtlasSubtexture {
    fn convert_texcoords(&self, st: [f32; 2]) -> [f32; 2] {
        [
            self.base_st[0] + st[0] * self.width,
            self.base_st[1] + st[1] * self.height,
        ]
    }
}

pub struct TextureAtlas {
    /// A handle to the atlas data on the GPU.
    atlas: wgpu::Texture,
    /// The width in texels of the atlas.
    width: u32,
    /// The height in texels of the atlas.
    height: u32,
    subtextures: Vec<TextureAtlasSubtexture>,
}

impl TextureAtlas {
    pub fn convert_texcoords(&self, id: usize, st: [f32; 2]) -> [f32; 2] {
        self.subtextures[id].convert_texcoords(st)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_texture_data_subtexture() {
        let src = TextureData {
            width: 2,
            height: 2,
            #[rustfmt::skip]
            indexed: vec![
                1, 2,
                3, 4,
            ],
        };

        let dst = TextureData {
            width: 4,
            height: 4,
            #[rustfmt::skip]
            indexed: vec![
                0, 0, 0, 0,
                0, 0, 0, 0,
                0, 0, 0, 0,
                0, 0, 0, 0,
            ],
        };

        let mut dst_copy = dst.clone();
        dst_copy.subtexture(&src, [1, 1]).unwrap();

        assert_eq!(
            dst_copy.indexed,
            vec![0, 0, 0, 0, 0, 1, 2, 0, 0, 3, 4, 0, 0, 0, 0, 0,]
        );
    }
}
