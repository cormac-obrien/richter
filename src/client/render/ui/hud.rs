use std::collections::HashMap;

use crate::{
    client::render::{
        glyph::GlyphRendererCommand,
        quad::{QuadRendererCommand, QuadTexture},
        ui::layout::{Anchor, Layout, ScreenPosition, Size},
        GraphicsState,
    },
    common::net::{ClientStat, ItemFlags},
};

use chrono::Duration;
use num::FromPrimitive as _;
use strum::IntoEnumIterator as _;
use strum_macros::EnumIter;

pub struct HudState<'a> {
    pub items: ItemFlags,
    pub item_pickup_time: &'a [Duration],
    pub stats: &'a [i32],
    pub face_anim_time: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum HudTextureId {
    Digit { alt: bool, value: usize },
    Minus { alt: bool },
    Colon,
    Slash,
    Weapon { id: WeaponId, frame: WeaponFrame },
    Ammo { id: AmmoId },
    Armor { id: usize },
    Item { id: ItemId },
    Sigil { id: usize },
    Face { id: FaceId },
    StatusBar,
    InvBar,
    ScoreBar,
}

impl std::fmt::Display for HudTextureId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use HudTextureId::*;
        match *self {
            Digit { alt, value } => write!(f, "{}NUM_{}", if alt { "A" } else { "" }, value),
            Minus { alt } => write!(f, "{}NUM_MINUS", if alt { "A" } else { "" }),
            Colon => write!(f, "NUM_COLON"),
            Slash => write!(f, "NUM_SLASH"),
            Weapon { id, frame } => write!(f, "INV{}_{}", frame, id),
            Ammo { id } => write!(f, "SB_{}", id),
            Armor { id } => write!(f, "SB_ARMOR{}", id + 1),
            Item { id } => write!(f, "SB_{}", id),
            Sigil { id } => write!(f, "SB_SIGIL{}", id + 1),
            Face { id } => write!(f, "{}", id),
            StatusBar => write!(f, "SBAR"),
            InvBar => write!(f, "IBAR"),
            ScoreBar => write!(f, "SCOREBAR"),
        }
    }
}

const WEAPON_ID_NAMES: [&'static str; 7] = [
    "SHOTGUN", "SSHOTGUN", "NAILGUN", "SNAILGUN", "RLAUNCH", "SRLAUNCH", "LIGHTNG",
];
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, FromPrimitive, EnumIter)]
enum WeaponId {
    Shotgun = 0,
    SuperShotgun = 1,
    Nailgun = 2,
    SuperNailgun = 3,
    RocketLauncher = 4,
    GrenadeLauncher = 5,
    LightningGun = 6,
}

impl std::fmt::Display for WeaponId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", WEAPON_ID_NAMES[*self as usize])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum WeaponFrame {
    Inactive,
    Active,
    Pickup { frame: usize },
}

impl std::fmt::Display for WeaponFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            WeaponFrame::Inactive => write!(f, ""),
            WeaponFrame::Active => write!(f, "2"),
            WeaponFrame::Pickup { frame } => write!(f, "A{}", frame + 1),
        }
    }
}

const AMMO_ID_NAMES: [&'static str; 4] = ["SHELLS", "NAILS", "ROCKET", "CELLS"];
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, FromPrimitive, EnumIter)]
enum AmmoId {
    Shells = 0,
    Nails = 1,
    Rockets = 2,
    Cells = 3,
}

impl std::fmt::Display for AmmoId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", AMMO_ID_NAMES[*self as usize])
    }
}

const ITEM_ID_NAMES: [&'static str; 6] = ["KEY1", "KEY2", "INVIS", "INVULN", "SUIT", "QUAD"];
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, FromPrimitive, EnumIter)]
enum ItemId {
    Key1 = 0,
    Key2 = 1,
    Invisibility = 2,
    Invulnerability = 3,
    BioSuit = 4,
    QuadDamage = 5,
}

impl std::fmt::Display for ItemId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", ITEM_ID_NAMES[*self as usize])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Hash, Eq)]
enum FaceId {
    Normal { pain: bool, frame: usize },
    Invisible,
    Invulnerable,
    InvisibleInvulnerable,
    QuadDamage,
}

impl std::fmt::Display for FaceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use FaceId::*;
        match *self {
            Normal { pain, frame } => {
                write!(f, "FACE{}{}", if pain { "_P" } else { "" }, frame + 1)
            }
            Invisible => write!(f, "FACE_INVIS"),
            Invulnerable => write!(f, "FACE_INVUL2"),
            InvisibleInvulnerable => write!(f, "FACE_INV2"),
            QuadDamage => write!(f, "FACE_QUAD"),
        }
    }
}

pub struct HudRenderer {
    textures: HashMap<HudTextureId, QuadTexture>,
}

impl HudRenderer {
    pub fn new(state: &GraphicsState) -> HudRenderer {
        use HudTextureId::*;
        let mut ids = Vec::new();

        // digits and minus
        ids.extend((&[false, true]).iter().flat_map(|b| {
            (0..10)
                .map(move |i| Digit { alt: *b, value: i })
                .chain(std::iter::once(Minus { alt: *b }))
        }));

        // weapons
        ids.extend(WeaponId::iter().flat_map(|id| {
            (0..5)
                .map(|frame| WeaponFrame::Pickup { frame })
                .chain(std::iter::once(WeaponFrame::Inactive))
                .chain(std::iter::once(WeaponFrame::Active))
                .map(move |frame| Weapon { id, frame })
        }));

        // ammo
        ids.extend(AmmoId::iter().map(|id| Ammo { id }));

        // armor
        ids.extend((0..3).map(|id| Armor { id }));

        // items
        ids.extend(ItemId::iter().map(|id| Item { id }));

        // sigils
        ids.extend((0..4).map(|id| Sigil { id }));

        // faces
        ids.extend(
            (&[false, true])
                .iter()
                .flat_map(|b| (0..5).map(move |i| FaceId::Normal { pain: *b, frame: i }))
                .chain(
                    vec![
                        FaceId::Invisible,
                        FaceId::Invulnerable,
                        FaceId::InvisibleInvulnerable,
                        FaceId::QuadDamage,
                    ]
                    .into_iter(),
                )
                .map(move |id| Face { id }),
        );

        // unit variants
        ids.extend(vec![Colon, Slash, StatusBar, InvBar, ScoreBar].into_iter());

        let mut textures = HashMap::new();
        for id in ids.into_iter() {
            debug!("Opening {}", id);
            let qpic = state.gfx_wad().open_qpic(id.to_string()).unwrap();
            let texture = QuadTexture::from_qpic(state, &qpic);
            textures.insert(id, texture);
        }

        HudRenderer { textures }
    }

    fn generate_number_commands<'a>(
        &'a self,
        number: i32,
        alt_color: bool,
        max_digits: usize,
        screen_anchor: Anchor,
        screen_x_ofs: i32,
        screen_y_ofs: i32,
        scale: f32,
        quad_cmds: &mut Vec<QuadRendererCommand<'a>>,
    ) {
        use HudTextureId::*;

        let number_str = format!("{}", number);
        debug!("number_str = {}", number_str);
        let number_chars = number_str.chars().collect::<Vec<_>>();

        let mut skip = 0;
        let mut place_ofs = 0;
        if number_chars.len() > max_digits {
            skip = number_chars.len() - max_digits;
        } else if max_digits > number_chars.len() {
            place_ofs = (max_digits - number_chars.len()) as i32 * 24;
        }
        debug!("skip = {} | place_ofs = {}", skip, place_ofs);

        for (chr_id, chr) in number_chars.into_iter().skip(skip).enumerate() {
            let tex_id = match chr {
                '-' => Minus { alt: alt_color },
                '0'..='9' => Digit {
                    alt: alt_color,
                    value: chr as usize - '0' as usize,
                },
                _ => unreachable!(),
            };

            quad_cmds.push(QuadRendererCommand {
                texture: self.textures.get(&tex_id).unwrap(),
                layout: Layout {
                    position: ScreenPosition::Relative {
                        anchor: screen_anchor,
                        x_ofs: screen_x_ofs + place_ofs + 24 * chr_id as i32,
                        y_ofs: screen_y_ofs,
                    },
                    anchor: Anchor::BOTTOM_LEFT,
                    size: Size::Scale { factor: scale },
                },
            });
        }
    }

    pub fn generate_commands<'state, 'a>(
        &'a self,
        hud_state: HudState<'a>,
        time: Duration,
        quad_cmds: &mut Vec<QuadRendererCommand<'a>>,
        glyph_cmds: &mut Vec<GlyphRendererCommand>,
    ) {
        use HudTextureId::*;

        // TODO: get from cvar
        let scale = 2.0;

        let sbar = self.textures.get(&StatusBar).unwrap();
        let sbar_x_ofs = -(sbar.width() as i32) / 2;

        // status bar
        quad_cmds.push(QuadRendererCommand {
            texture: sbar,
            layout: Layout {
                position: ScreenPosition::Absolute(Anchor::BOTTOM_CENTER),
                anchor: Anchor::BOTTOM_CENTER,
                size: Size::Scale { factor: scale },
            },
        });

        // inventory bar
        quad_cmds.push(QuadRendererCommand {
            texture: self.textures.get(&InvBar).unwrap(),
            layout: Layout {
                position: ScreenPosition::Relative {
                    anchor: Anchor::BOTTOM_CENTER,
                    x_ofs: 0,
                    y_ofs: sbar.height() as i32,
                },
                anchor: Anchor::BOTTOM_CENTER,
                size: Size::Scale { factor: scale },
            },
        });

        // weapons
        for i in 0..7 {
            if hud_state
                .items
                .contains(ItemFlags::from_bits(ItemFlags::SHOTGUN.bits() << i).unwrap())
            {
                let id = WeaponId::from_usize(i).unwrap();
                let pickup_time = hud_state.item_pickup_time[i];
                let delta = time - pickup_time;
                let frame = if delta >= Duration::milliseconds(100) {
                    if hud_state.stats[ClientStat::ActiveWeapon as usize] as u32
                        == ItemFlags::SHOTGUN.bits() << i
                    {
                        WeaponFrame::Active
                    } else {
                        WeaponFrame::Inactive
                    }
                } else {
                    WeaponFrame::Pickup {
                        frame: (delta.num_milliseconds() * 100) as usize % 5,
                    }
                };

                quad_cmds.push(QuadRendererCommand {
                    texture: self.textures.get(&Weapon { id, frame }).unwrap(),
                    layout: Layout {
                        position: ScreenPosition::Relative {
                            anchor: Anchor::BOTTOM_CENTER,
                            x_ofs: sbar_x_ofs + 24 * i as i32,
                            y_ofs: sbar.height() as i32,
                        },
                        anchor: Anchor::BOTTOM_LEFT,
                        size: Size::Scale { factor: scale },
                    },
                });
            }
        }

        // ammo counters
        for i in 0..4 {
            let ammo_str = format!("{: >3}", hud_state.stats[ClientStat::Shells as usize + i]);
            for (chr_id, chr) in ammo_str.chars().enumerate() {
                if chr != ' ' {
                    glyph_cmds.push(GlyphRendererCommand::Glyph {
                        glyph_id: 18 + chr as u8 - '0' as u8,
                        position: ScreenPosition::Relative {
                            anchor: Anchor::BOTTOM_CENTER,
                            x_ofs: sbar_x_ofs + 8 * (6 * i + chr_id) as i32 + 10,
                            y_ofs: sbar.height() as i32 + 16,
                        },
                        anchor: Anchor::BOTTOM_LEFT,
                        scale,
                    });
                }
            }
        }

        // items
        for i in 0..6 {
            if hud_state
                .items
                .contains(ItemFlags::from_bits(ItemFlags::KEY_1.bits() << i).unwrap())
            {
                quad_cmds.push(QuadRendererCommand {
                    texture: self
                        .textures
                        .get(&Item {
                            id: ItemId::from_usize(i).unwrap(),
                        })
                        .unwrap(),
                    layout: Layout {
                        position: ScreenPosition::Relative {
                            anchor: Anchor::BOTTOM_CENTER,
                            x_ofs: sbar_x_ofs + 16 * i as i32 + 192,
                            y_ofs: sbar.height() as i32,
                        },
                        anchor: Anchor::BOTTOM_LEFT,
                        size: Size::Scale { factor: scale },
                    },
                })
            }
        }

        // sigils
        for i in 0..4 {
            if hud_state
                .items
                .contains(ItemFlags::from_bits(ItemFlags::SIGIL_1.bits() << i).unwrap())
            {
                quad_cmds.push(QuadRendererCommand {
                    texture: self.textures.get(&Sigil { id: i }).unwrap(),
                    layout: Layout {
                        position: ScreenPosition::Relative {
                            anchor: Anchor::BOTTOM_CENTER,
                            x_ofs: sbar_x_ofs + 8 * i as i32 + 288,
                            y_ofs: sbar.height() as i32,
                        },
                        anchor: Anchor::BOTTOM_LEFT,
                        size: Size::Scale { factor: scale },
                    },
                });
            }
        }

        // armor
        if hud_state.items.contains(ItemFlags::INVULNERABILITY) {
            self.generate_number_commands(
                666,
                true,
                3,
                Anchor::BOTTOM_CENTER,
                sbar_x_ofs,
                0,
                scale,
                quad_cmds,
            );
        // TODO draw_disc
        } else {
            let armor = hud_state.stats[ClientStat::Armor as usize];
            self.generate_number_commands(
                armor,
                armor <= 25,
                3,
                Anchor::BOTTOM_CENTER,
                sbar_x_ofs + self.textures.get(&Armor { id: 0 }).unwrap().width() as i32,
                0,
                scale,
                quad_cmds,
            );

            let mut armor_id = None;
            for i in (0..3).rev() {
                if hud_state
                    .items
                    .contains(ItemFlags::from_bits(ItemFlags::ARMOR_1.bits() << i).unwrap())
                {
                    armor_id = Some(Armor { id: i });
                    break;
                }
            }

            if let Some(a) = armor_id {
                quad_cmds.push(QuadRendererCommand {
                    texture: self.textures.get(&a).unwrap(),
                    layout: Layout {
                        position: ScreenPosition::Relative {
                            anchor: Anchor::BOTTOM_CENTER,
                            x_ofs: sbar_x_ofs,
                            y_ofs: 0,
                        },
                        anchor: Anchor::BOTTOM_LEFT,
                        size: Size::Scale { factor: scale },
                    },
                });
            }
        }

        // health
        let health = hud_state.stats[ClientStat::Health as usize];
        self.generate_number_commands(
            health,
            health <= 25,
            3,
            Anchor::BOTTOM_CENTER,
            sbar_x_ofs + 136,
            0,
            scale,
            quad_cmds,
        );

        let ammo = hud_state.stats[ClientStat::Ammo as usize];
        self.generate_number_commands(
            ammo,
            ammo <= 10,
            3,
            Anchor::BOTTOM_CENTER,
            sbar_x_ofs + 248,
            0,
            scale,
            quad_cmds,
        );

        // TODO: render face
        let face = if hud_state
            .items
            .contains(ItemFlags::INVISIBILITY | ItemFlags::INVULNERABILITY)
        {
            FaceId::InvisibleInvulnerable
        } else if hud_state.items.contains(ItemFlags::QUAD) {
            FaceId::QuadDamage
        } else if hud_state.items.contains(ItemFlags::INVISIBILITY) {
            FaceId::Invisible
        } else if hud_state.items.contains(ItemFlags::INVULNERABILITY) {
            FaceId::Invulnerable
        } else {
            let health = hud_state.stats[ClientStat::Health as usize];
            let frame = 4 - if health >= 100 {
                4
            } else {
                health.max(0) as usize / 20
            };

            FaceId::Normal {
                pain: hud_state.face_anim_time > time,
                frame,
            }
        };

        quad_cmds.push(QuadRendererCommand {
            texture: self.textures.get(&Face { id: face }).unwrap(),
            layout: Layout {
                position: ScreenPosition::Relative {
                    anchor: Anchor::BOTTOM_CENTER,
                    x_ofs: sbar_x_ofs + 112,
                    y_ofs: 0,
                },
                anchor: Anchor::BOTTOM_LEFT,
                size: Size::Scale { factor: scale },
            },
        });

        // crosshair
        glyph_cmds.push(GlyphRendererCommand::Glyph {
            glyph_id: '+' as u8,
            position: ScreenPosition::Absolute(Anchor::CENTER),
            anchor: Anchor::CENTER,
            scale,
        });
    }
}
