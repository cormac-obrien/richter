use std::collections::HashMap;

use crate::{
    client::{
        render::{
            ui::{
                glyph::GlyphRendererCommand,
                layout::{Anchor, Layout, ScreenPosition, Size},
                quad::{QuadRendererCommand, QuadTexture},
            },
            GraphicsState,
        },
        IntermissionKind,
    },
    common::{
        console::Console,
        net::{ClientStat, ItemFlags},
        wad::QPic,
    },
};

use chrono::Duration;
use num::FromPrimitive as _;
use strum::IntoEnumIterator as _;
use strum_macros::EnumIter;

// intermission overlay size
const OVERLAY_WIDTH: i32 = 320;
const OVERLAY_HEIGHT: i32 = 200;

const OVERLAY_X_OFS: i32 = -OVERLAY_WIDTH / 2;
const OVERLAY_Y_OFS: i32 = -OVERLAY_HEIGHT / 2;

const OVERLAY_ANCHOR: Anchor = Anchor::CENTER;

pub enum HudState<'a> {
    InGame {
        items: ItemFlags,
        item_pickup_time: &'a [Duration],
        stats: &'a [i32],
        face_anim_time: Duration,
        console: &'a Console,
    },
    Intermission {
        kind: &'a IntermissionKind,
        completion_duration: Duration,
        stats: &'a [i32],
        console: &'a Console,
    },
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

    // these are not in gfx.wad
    Complete,
    Intermission,
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

            // these are not in gfx.wad
            Complete => write!(f, "gfx/complete.lmp"),
            Intermission => write!(f, "gfx/inter.lmp"),
        }
    }
}

const WEAPON_ID_NAMES: [&str; 7] = [
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

const AMMO_ID_NAMES: [&str; 4] = ["SHELLS", "NAILS", "ROCKET", "CELLS"];
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

const ITEM_ID_NAMES: [&str; 6] = ["KEY1", "KEY2", "INVIS", "INVULN", "SUIT", "QUAD"];
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
    /// Construct a new `HudRenderer`.
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

        // new id list for textures not in gfx.wad
        let ids = vec![Complete, Intermission];
        for id in ids.into_iter() {
            debug!("Opening {}", id);
            let qpic = QPic::load(state.vfs().open(&format!("{}", id)).unwrap()).unwrap();
            textures.insert(id, QuadTexture::from_qpic(state, &qpic));
        }

        HudRenderer { textures }
    }

    fn cmd_number<'a>(
        &'a self,
        number: i32,
        alt_color: bool,
        max_digits: usize,
        screen_anchor: Anchor,
        screen_x_ofs: i32,
        screen_y_ofs: i32,
        quad_anchor: Anchor,
        scale: f32,
        quad_cmds: &mut Vec<QuadRendererCommand<'a>>,
    ) {
        use HudTextureId::*;

        let number_str = format!("{}", number);
        let number_chars = number_str.chars().collect::<Vec<_>>();

        let mut skip = 0;
        let mut place_ofs = 0;
        if number_chars.len() > max_digits {
            skip = number_chars.len() - max_digits;
        } else if max_digits > number_chars.len() {
            place_ofs = (max_digits - number_chars.len()) as i32 * 24;
        }

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
                    anchor: quad_anchor,
                    size: Size::Scale { factor: scale },
                },
            });
        }
    }

    // Draw a quad on the status bar.
    //
    // `x_ofs` and `y_ofs` are specified relative to the bottom-left corner of
    // the status bar.
    fn cmd_sbar_quad<'a>(
        &'a self,
        texture_id: HudTextureId,
        x_ofs: i32,
        y_ofs: i32,
        scale: f32,
        quad_cmds: &mut Vec<QuadRendererCommand<'a>>,
    ) {
        quad_cmds.push(QuadRendererCommand {
            texture: self.textures.get(&texture_id).unwrap(),
            layout: Layout {
                position: ScreenPosition::Relative {
                    anchor: Anchor::BOTTOM_CENTER,
                    x_ofs: OVERLAY_X_OFS + x_ofs,
                    y_ofs,
                },
                anchor: Anchor::BOTTOM_LEFT,
                size: Size::Scale { factor: scale },
            },
        });
    }

    // Draw a quad on the status bar.
    //
    // `x_ofs` and `y_ofs` are specified relative to the bottom-left corner of
    // the status bar.
    fn cmd_sbar_number<'a>(
        &'a self,
        number: i32,
        alt_color: bool,
        max_digits: usize,
        x_ofs: i32,
        y_ofs: i32,
        scale: f32,
        quad_cmds: &mut Vec<QuadRendererCommand<'a>>,
    ) {
        self.cmd_number(
            number,
            alt_color,
            max_digits,
            Anchor::BOTTOM_CENTER,
            OVERLAY_X_OFS + x_ofs,
            y_ofs,
            Anchor::BOTTOM_LEFT,
            scale,
            quad_cmds,
        );
    }

    // Draw the status bar.
    fn cmd_sbar<'a>(
        &'a self,
        time: Duration,
        items: ItemFlags,
        item_pickup_time: &'a [Duration],
        stats: &'a [i32],
        face_anim_time: Duration,
        scale: f32,
        quad_cmds: &mut Vec<QuadRendererCommand<'a>>,
        glyph_cmds: &mut Vec<GlyphRendererCommand>,
    ) {
        use HudTextureId::*;

        let sbar = self.textures.get(&StatusBar).unwrap();
        let sbar_x_ofs = -(sbar.width() as i32) / 2;

        // status bar background
        self.cmd_sbar_quad(StatusBar, 0, 0, scale, quad_cmds);

        // inventory bar background
        self.cmd_sbar_quad(InvBar, 0, sbar.height() as i32, scale, quad_cmds);

        // weapon slots
        for i in 0..7 {
            if items.contains(ItemFlags::from_bits(ItemFlags::SHOTGUN.bits() << i).unwrap()) {
                let id = WeaponId::from_usize(i).unwrap();
                let pickup_time = item_pickup_time[i];
                let delta = time - pickup_time;
                let frame = if delta >= Duration::milliseconds(100) {
                    if stats[ClientStat::ActiveWeapon as usize] as u32
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

                self.cmd_sbar_quad(
                    Weapon { id, frame },
                    24 * i as i32,
                    sbar.height() as i32,
                    scale,
                    quad_cmds,
                );
            }
        }

        // ammo counters
        for i in 0..4 {
            let ammo_str = format!("{: >3}", stats[ClientStat::Shells as usize + i]);
            for (chr_id, chr) in ammo_str.chars().enumerate() {
                if chr != ' ' {
                    glyph_cmds.push(GlyphRendererCommand::Glyph {
                        glyph_id: 18 + chr as u8 - b'0',
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

        // items (keys and powerups)
        for i in 0..6 {
            if items.contains(ItemFlags::from_bits(ItemFlags::KEY_1.bits() << i).unwrap()) {
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
            if items.contains(ItemFlags::from_bits(ItemFlags::SIGIL_1.bits() << i).unwrap()) {
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
        let armor_width = self.textures.get(&Armor { id: 0 }).unwrap().width() as i32;
        if items.contains(ItemFlags::INVULNERABILITY) {
            self.cmd_sbar_number(666, true, 3, armor_width, 0, scale, quad_cmds);
        // TODO draw_disc
        } else {
            let armor = stats[ClientStat::Armor as usize];
            self.cmd_sbar_number(armor, armor <= 25, 3, armor_width, 0, scale, quad_cmds);

            let mut armor_id = None;
            for i in (0..3).rev() {
                if items.contains(ItemFlags::from_bits(ItemFlags::ARMOR_1.bits() << i).unwrap()) {
                    armor_id = Some(Armor { id: i });
                    break;
                }
            }

            if let Some(a) = armor_id {
                self.cmd_sbar_quad(a, 0, 0, scale, quad_cmds);
            }
        }

        // health
        let health = stats[ClientStat::Health as usize];
        self.cmd_sbar_number(health, health <= 25, 3, 136, 0, scale, quad_cmds);

        let ammo = stats[ClientStat::Ammo as usize];
        self.cmd_sbar_number(ammo, ammo <= 10, 3, 248, 0, scale, quad_cmds);

        let face = if items.contains(ItemFlags::INVISIBILITY | ItemFlags::INVULNERABILITY) {
            FaceId::InvisibleInvulnerable
        } else if items.contains(ItemFlags::QUAD) {
            FaceId::QuadDamage
        } else if items.contains(ItemFlags::INVISIBILITY) {
            FaceId::Invisible
        } else if items.contains(ItemFlags::INVULNERABILITY) {
            FaceId::Invulnerable
        } else {
            let health = stats[ClientStat::Health as usize];
            let frame = 4 - if health >= 100 {
                4
            } else {
                health.max(0) as usize / 20
            };

            FaceId::Normal {
                pain: face_anim_time > time,
                frame,
            }
        };

        self.cmd_sbar_quad(Face { id: face }, 112, 0, scale, quad_cmds);

        // crosshair
        glyph_cmds.push(GlyphRendererCommand::Glyph {
            glyph_id: b'+',
            position: ScreenPosition::Absolute(Anchor::CENTER),
            anchor: Anchor::TOP_LEFT,
            scale,
        });
    }

    // Draw a quad on the intermission overlay.
    //
    // `x_ofs` and `y_ofs` are specified relative to the top-left corner of the
    // overlay.
    fn cmd_intermission_quad<'a>(
        &'a self,
        texture_id: HudTextureId,
        x_ofs: i32,
        y_ofs: i32,
        scale: f32,
        quad_cmds: &mut Vec<QuadRendererCommand<'a>>,
    ) {
        quad_cmds.push(QuadRendererCommand {
            texture: self.textures.get(&texture_id).unwrap(),
            layout: Layout {
                position: ScreenPosition::Relative {
                    anchor: Anchor::CENTER,
                    x_ofs: OVERLAY_X_OFS + x_ofs,
                    y_ofs: OVERLAY_Y_OFS + y_ofs,
                },
                anchor: Anchor::TOP_LEFT,
                size: Size::Scale { factor: scale },
            },
        });
    }

    // Draw a number on the intermission overlay.
    //
    // `x_ofs` and `y_ofs` are specified relative to the top-left corner of the
    // overlay.
    fn cmd_intermission_number<'a>(
        &'a self,
        number: i32,
        max_digits: usize,
        x_ofs: i32,
        y_ofs: i32,
        scale: f32,
        quad_cmds: &mut Vec<QuadRendererCommand<'a>>,
    ) {
        self.cmd_number(
            number,
            false,
            max_digits,
            OVERLAY_ANCHOR,
            OVERLAY_X_OFS + x_ofs,
            OVERLAY_Y_OFS + y_ofs,
            Anchor::TOP_LEFT,
            scale,
            quad_cmds,
        );
    }

    // Draw the intermission overlay.
    fn cmd_intermission_overlay<'a>(
        &'a self,
        _kind: &'a IntermissionKind,
        completion_duration: Duration,
        stats: &'a [i32],
        scale: f32,
        quad_cmds: &mut Vec<QuadRendererCommand<'a>>,
    ) {
        use HudTextureId::*;

        // TODO: check gametype

        self.cmd_intermission_quad(Complete, 64, OVERLAY_HEIGHT - 24, scale, quad_cmds);
        self.cmd_intermission_quad(Intermission, 0, OVERLAY_HEIGHT - 56, scale, quad_cmds);

        // TODO: zero-pad number of seconds
        let time_y_ofs = OVERLAY_HEIGHT - 64;
        let minutes = completion_duration.num_minutes() as i32;
        let seconds = completion_duration.num_seconds() as i32 - 60 * minutes;
        self.cmd_intermission_number(minutes, 3, 160, time_y_ofs, scale, quad_cmds);
        self.cmd_intermission_quad(Colon, 234, time_y_ofs, scale, quad_cmds);
        self.cmd_intermission_number(seconds, 2, 246, time_y_ofs, scale, quad_cmds);

        // secrets
        let secrets_y_ofs = OVERLAY_HEIGHT - 104;
        let secrets_found = stats[ClientStat::FoundSecrets as usize];
        let secrets_total = stats[ClientStat::TotalSecrets as usize];
        self.cmd_intermission_number(secrets_found, 3, 160, secrets_y_ofs, scale, quad_cmds);
        self.cmd_intermission_quad(Slash, 232, secrets_y_ofs, scale, quad_cmds);
        self.cmd_intermission_number(secrets_total, 3, 240, secrets_y_ofs, scale, quad_cmds);

        // monsters
        let monsters_y_ofs = OVERLAY_HEIGHT - 144;
        let monsters_killed = stats[ClientStat::KilledMonsters as usize];
        let monsters_total = stats[ClientStat::TotalMonsters as usize];
        self.cmd_intermission_number(monsters_killed, 3, 160, monsters_y_ofs, scale, quad_cmds);
        self.cmd_intermission_quad(Slash, 232, monsters_y_ofs, scale, quad_cmds);
        self.cmd_intermission_number(monsters_total, 3, 240, monsters_y_ofs, scale, quad_cmds);
    }

    /// Generate render commands to draw the HUD in the specified state.
    pub fn generate_commands<'a>(
        &'a self,
        hud_state: &HudState<'a>,
        time: Duration,
        quad_cmds: &mut Vec<QuadRendererCommand<'a>>,
        glyph_cmds: &mut Vec<GlyphRendererCommand>,
    ) {
        // TODO: get from cvar
        let scale = 2.0;
        let console_timeout = Duration::seconds(3);

        match hud_state {
            HudState::InGame {
                items,
                item_pickup_time,
                stats,
                face_anim_time,
                console,
            } => {
                self.cmd_sbar(
                    time,
                    *items,
                    item_pickup_time,
                    stats,
                    *face_anim_time,
                    scale,
                    quad_cmds,
                    glyph_cmds,
                );

                let output = console.output();
                for (id, line) in output.recent_lines(console_timeout, 100, 10).enumerate() {
                    for (chr_id, chr) in line.iter().enumerate() {
                        glyph_cmds.push(GlyphRendererCommand::Glyph {
                            glyph_id: *chr as u8,
                            position: ScreenPosition::Relative {
                                anchor: Anchor::TOP_LEFT,
                                x_ofs: 8 * chr_id as i32,
                                y_ofs: -8 * id as i32,
                            },
                            anchor: Anchor::TOP_LEFT,
                            scale,
                        });
                    }
                }
            }
            HudState::Intermission {
                kind,
                completion_duration,
                stats,
                console,
            } => {
                self.cmd_intermission_overlay(kind, *completion_duration, stats, scale, quad_cmds);

                // TODO: dedup this code
                let output = console.output();
                for (id, line) in output.recent_lines(console_timeout, 100, 10).enumerate() {
                    for (chr_id, chr) in line.iter().enumerate() {
                        glyph_cmds.push(GlyphRendererCommand::Glyph {
                            glyph_id: *chr as u8,
                            position: ScreenPosition::Relative {
                                anchor: Anchor::TOP_LEFT,
                                x_ofs: 8 * chr_id as i32,
                                y_ofs: -8 * id as i32,
                            },
                            anchor: Anchor::TOP_LEFT,
                            scale,
                        });
                    }
                }
            }
        }
    }
}
