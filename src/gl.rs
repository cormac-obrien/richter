use engine::*;
use mdl::*;

use glium::Texture2d;
use glium::backend::glutin_backend::GlutinFacade as Window;

pub struct GlMdlSkinSingle {
    pub texture: Texture2d,
}

impl GlMdlSkinSingle {
    fn from_mdlskinsingle(window: &Window, src: &SkinSingle, width: u32, height: u32) -> Option<GlMdlSkinSingle> {
        Some(GlMdlSkinSingle {
            texture: engine::tex_from_indexed(window, &src.indices, width, height),
        })
    }
}

pub struct GlMdlSkinGroup {
    times: Vec<f32>,
    textures: Vec<Texture2d>,
}

impl GlMdlSkinGroup {
    fn from_mdlskingroup(window: &Window, src: &SkinGroup, width: u32, height: u32) -> Option<GlMdlSkinGroup> {
        // TODO: optimize for one map() call
        Some(GlMdlSkinGroup {
            times: src.skins.iter().map(|x| x.time).collect(),
            textures: src.skins.iter().map(|x| engine::tex_from_indexed(window, &x.indices, width, height)).collect(),
        })
    }
}

pub enum GlMdlSkin {
    Single(GlMdlSkinSingle),
    Group(GlMdlSkinGroup),
}

impl GlMdlSkin {
    fn from_mdlskin(window: &Window, src: &Skin, width: u32, height: u32) -> Option<GlMdlSkin> {
        Some(match *src {
            Skin::Single(ref s) => GlMdlSkin::Single(GlMdlSkinSingle::from_mdlskinsingle(window, &s, width, height).expect("")),
            Skin::Group(ref g) => GlMdlSkin::Group(GlMdlSkinGroup::from_mdlskingroup(window, &g, width, height).expect("")),
        })
    }
}

pub struct GlMdl {
    pub skins: Vec<GlMdlSkin>,
    pub texcoords: Vec<f32>,
    pub triangles: Vec<u32>,
    pub frames: Vec<u32>,
}

impl GlMdl {
    pub fn from_mdl(window: &Window, src: &Mdl) -> GlMdl{
        GlMdl {
            skins: src.skins.iter()
                            .map(|skin|
                                 GlMdlSkin::from_mdlskin(
                                        window,
                                        skin,
                                        src.header.skin_w as u32,
                                        src.header.skin_h as u32)
                                .expect(""))
                            .collect(),
            texcoords: Vec::new(),
            triangles: Vec::new(),
            frames: Vec::new(),
        }
    }
}
