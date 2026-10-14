//! The interface's design language: gilt-framed parchment and slate over the map, one dark-green
//! action, and parchment plates for the labels that sit on the map itself.
//!
//! Panels are dressed by [`crate::chrome`]: a tile of aged paper (or dark slate) under a bronze
//! and gold frame with a boss or a scroll in each corner and a cartouche in the middle of each
//! side. Text is a single dark brown ink on paper and cream on slate, set in two weights of one
//! old-style serif (EB Garamond); icons are either white glyphs tinted at the place of use or
//! painted emblems (see `art/forge/emblems.py`); the one primary control (ending the day) is
//! dark green with a gilt edge. On the map, names and counts sit on small parchment banners, and
//! a nation is its flag in a gold-rimmed disc.
//!
//! The map labels' shapes are made at start-up (see [`Shapes`]); the frames, paper and emblems
//! are files of the pack (`ui/chrome/`), and a pack without them still gets flat panels in the
//! colours below.
use super::*;
use bevy::asset::AssetId;
use bevy::sprite::{BorderRect, SliceScaleMode, SpriteImageMode, TextureSlicer};
use bevy::text::{Font, FontWeight};
use std::f32::consts::TAU;

/// Room the two cards along the top edge take: their height plus the margin around them. The
/// pane under them is kept clear of it.
pub(crate) const TOP_BAR: f32 = 74.0;
/// Distance of the top cards from the edges of the window.
pub(crate) const TOP_MARGIN: f32 = 8.0;

/// The face every `Text` uses (see [`install_font`]). EB Garamond, SIL OFL 1.1:
/// `fonts/EBGaramond-OFL.txt`.
const SERIF: &[u8] = include_bytes!("../fonts/EBGaramond-Variable.ttf");

/// Old-style serifs are small for their size: every text is drawn this much larger than asked.
const TYPE_SCALE: f32 = 1.16;

/// Make EB Garamond the face of every text that names no font of its own.
pub(crate) fn install_font(app: &mut App) {
    let font = Font::try_from_bytes(SERIF.to_vec()).expect("the bundled font is a font");
    let mut fonts = app.world_mut().resource_mut::<Assets<Font>>();
    fonts
        .insert(AssetId::default(), font)
        .expect("the default font slot takes a font");
}

// --- palette ---------------------------------------------------------------------------------
/// Aged paper: what a panel's surface is where its picture has not loaded, and the colour of the
/// plain cards of a pack without chrome.
pub(crate) const CARD: Color = Color::srgb(0.918, 0.851, 0.694);
/// Dark slate, the surface of the nation's panel and the minimap's frame.
pub(crate) const SLATE: Color = Color::srgb(0.145, 0.157, 0.180);
/// A control or a recessed field on paper, with its hover and pressed fills.
pub(crate) const WELL: Color = Color::srgb(0.855, 0.776, 0.608);
pub(crate) const WELL_HOVER: Color = Color::srgb(0.906, 0.835, 0.663);
pub(crate) const WELL_PRESSED: Color = Color::srgb(0.773, 0.690, 0.529);
/// The hairline around a control, and the rules inside a panel: brown, soft.
pub(crate) const EDGE: Color = Color::srgba(0.298, 0.196, 0.071, 0.42);
pub(crate) const RULE: Color = Color::srgba(0.337, 0.235, 0.098, 0.26);
/// Text on paper: dark brown, and the same pulled back for what is secondary.
pub(crate) const INK: Color = Color::srgb(0.118, 0.086, 0.055);
pub(crate) const INK_SOFT: Color = Color::srgba(0.32, 0.239, 0.141, 0.82);
/// Dark green: the one primary action, and the chosen thing among alternatives.
pub(crate) const ACCENT: Color = Color::srgb(0.137, 0.282, 0.224);
pub(crate) const ACCENT_HOVER: Color = Color::srgb(0.184, 0.357, 0.278);
pub(crate) const ACCENT_PRESSED: Color = Color::srgb(0.094, 0.208, 0.161);
/// Text and glyphs on the accent.
pub(crate) const ON_ACCENT: Color = Color::srgb(0.965, 0.929, 0.816);
/// Dark plates for labels over the map, and the cream text on them (and on slate).
pub(crate) const PLATE: Color = Color::srgba(0.102, 0.110, 0.125, 0.94);
pub(crate) const ON_PLATE: Color = Color::srgb(0.949, 0.910, 0.804);
/// Good news, bad news, and a caution, as read on paper.
pub(crate) const GOOD: Color = Color::srgb(0.169, 0.490, 0.145);
pub(crate) const BAD: Color = Color::srgb(0.690, 0.176, 0.125);
pub(crate) const AMBER: Color = Color::srgb(0.808, 0.553, 0.098);
/// An income, and a loss, read on the dark panel of the nation.
pub(crate) const GAIN: Color = Color::srgb(0.455, 0.792, 0.376);
pub(crate) const LOSS: Color = Color::srgb(0.929, 0.455, 0.384);
/// A spent health pip on a dark track: near-black, so it reads as missing.
pub(crate) const SPENT: Color = Color::srgba(0.012, 0.020, 0.032, 0.92);
/// The gap between two neighbouring health pips, in px.
pub(crate) const PIP_GAP: f32 = 2.0;
/// The vivid green of a healthy unit's pips on the map, bright enough to read over terrain.
pub(crate) const MAP_HEALTHY: Color = Color::srgb(0.38, 0.88, 0.24);
/// The most hit points still drawn as separate pips. Past that the pips would be slivers,
/// so callers fall back to a continuous bar.
pub(crate) const MAX_PIPS: i32 = 24;
/// The white of a selection on the map, and of the route that leads out of it.
pub(crate) const SELECT: Color = Color::srgb(0.965, 0.957, 0.925);
/// Gilt: the trim of every frame, and the headings on slate.
pub(crate) const TRIM: Color = Color::srgb(0.847, 0.667, 0.298);
/// The dark bronze outline of a frame, and the ink of the labels on the map's banners.
pub(crate) const BRONZE: Color = Color::srgb(0.110, 0.067, 0.035);
/// The parchment of a banner on the map, and its dark rim.
pub(crate) const BANNER: Color = Color::srgb(0.929, 0.855, 0.690);
pub(crate) const BANNER_RIM: Color = Color::srgb(0.337, 0.224, 0.098);

// --- text --------------------------------------------------------------------------------------
pub(crate) fn font(size: f32, weight: FontWeight) -> TextFont {
    TextFont {
        font_size: size * TYPE_SCALE,
        weight,
        ..default()
    }
}

/// Body text.
pub(crate) fn text(
    parent: &mut ChildSpawnerCommands,
    value: impl Into<String>,
    size: f32,
    color: Color,
) {
    spawn_text(parent, value, font(size, FontWeight::NORMAL), color);
}

/// Titles, figures and anything that should stand out of its line.
pub(crate) fn bold(
    parent: &mut ChildSpawnerCommands,
    value: impl Into<String>,
    size: f32,
    color: Color,
) {
    spawn_text(parent, value, font(size, FontWeight::SEMIBOLD), color);
}

/// A small label over a figure or a group: soft, semi-bold, and in capitals.
pub(crate) fn caption(parent: &mut ChildSpawnerCommands, label: &str) {
    bold(parent, label.to_uppercase(), 10.5, INK_SOFT);
}

fn spawn_text(
    parent: &mut ChildSpawnerCommands,
    value: impl Into<String>,
    font: TextFont,
    color: Color,
) {
    parent.spawn((
        Node {
            flex_shrink: 0.0,
            ..default()
        },
        Text::new(value),
        font,
        TextColor(color),
    ));
}

// --- surfaces ------------------------------------------------------------------------------------
/// A rounded corner, for the `border_radius` of a node.
pub(crate) fn rounded(radius: f32) -> BorderRadius {
    BorderRadius::all(px(radius))
}

/// A card floating over the map: a long, soft shadow.
pub(crate) fn lift() -> BoxShadow {
    BoxShadow::new(
        Color::srgba(0.02, 0.03, 0.05, 0.50),
        px(0),
        px(6),
        px(0),
        px(16),
    )
}

/// A small thing floating over the map: a button, a tray, a pill.
pub(crate) fn lift_small() -> BoxShadow {
    BoxShadow::new(
        Color::srgba(0.02, 0.05, 0.09, 0.26),
        px(0),
        px(3),
        px(0),
        px(10),
    )
}

/// The base of a floating card, placed absolutely by the caller.
pub(crate) fn card() -> Node {
    Node {
        position_type: PositionType::Absolute,
        // The frame's band and the shade inside it take the first twenty pixels or so.
        padding: UiRect::axes(px(24), px(20)),
        flex_direction: FlexDirection::Column,
        row_gap: px(4),
        ..default()
    }
}

/// A hairline across the width of its container.
pub(crate) fn rule(parent: &mut ChildSpawnerCommands) {
    parent.spawn((
        Node {
            width: percent(100),
            height: px(1),
            flex_shrink: 0.0,
            ..default()
        },
        BackgroundColor(RULE),
    ));
}

/// A section heading: a line running into a diamond at either side of a small label.
pub(crate) fn heading(parent: &mut ChildSpawnerCommands, label: &str) {
    let line = || Node {
        flex_grow: 1.0,
        height: px(1),
        ..default()
    };
    let diamond = || {
        (
            Node {
                width: px(5),
                height: px(5),
                flex_shrink: 0.0,
                ..default()
            },
            BackgroundColor(INK_SOFT),
            UiTransform::from_rotation(Rot2::degrees(45.0)),
        )
    };
    parent
        .spawn(Node {
            width: percent(100),
            flex_shrink: 0.0,
            align_items: AlignItems::Center,
            column_gap: px(7),
            margin: UiRect::vertical(px(6)),
            ..default()
        })
        .with_children(|p| {
            p.spawn((line(), BackgroundColor(INK_SOFT.with_alpha(0.5))));
            p.spawn(diamond());
            caption(p, label);
            p.spawn(diamond());
            p.spawn((line(), BackgroundColor(INK_SOFT.with_alpha(0.5))));
        });
}

/// A name on the left and its figure on the right, over a hairline.
pub(crate) fn row(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    value: impl Into<String>,
    tint: Color,
) {
    parent
        .spawn((
            Node {
                width: percent(100),
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                column_gap: px(12),
                flex_shrink: 0.0,
                padding: UiRect::vertical(px(4)),
                border: UiRect::bottom(px(1)),
                ..default()
            },
            BorderColor::all(RULE),
        ))
        .with_children(|p| {
            text(p, label, 13.0, INK_SOFT);
            bold(p, value, 13.0, tint);
        });
}

/// The colour of lit health pips at `fraction` of full strength: green while healthy,
/// amber while hurt, red when nearly dead. One helper so the map, the inspector and the
/// combat stage agree.
pub(crate) fn hp_color(fraction: f32) -> Color {
    if fraction > 0.66 {
        GOOD
    } else if fraction > 0.33 {
        AMBER
    } else {
        BAD
    }
}

/// One health pip: its centre's offset from the bar's centre, its width, and whether it is
/// still lit (one pip per hit point, like the reference).
pub(crate) struct Pip {
    pub centre: f32,
    pub width: f32,
    pub lit: bool,
}

/// Lay `max` pips out across `width`, the first `hp` of them lit. The pips tile the width
/// with [`PIP_GAP`] between neighbours. Returns an empty vector when there is nothing to
/// draw (`max <= 0`) or the pips would be unreadably thin (`max > MAX_PIPS`); the caller
/// then falls back to a continuous bar.
pub(crate) fn pip_layout(hp: i32, max: i32, width: f32) -> Vec<Pip> {
    pip_layout_gapped(hp, max, width, PIP_GAP)
}

/// [`pip_layout`] with `gap` px between neighbouring pips instead of [`PIP_GAP`].
pub(crate) fn pip_layout_gapped(hp: i32, max: i32, width: f32, gap: f32) -> Vec<Pip> {
    if max <= 0 || max > MAX_PIPS {
        return Vec::new();
    }
    let lit = hp.clamp(0, max);
    let pip_width = (width - gap * (max - 1) as f32) / max as f32;
    if pip_width < 2.0 {
        return Vec::new();
    }
    (0..max)
        .map(|i| {
            let left = -width / 2.0 + i as f32 * (pip_width + gap);
            Pip {
                centre: left + pip_width / 2.0,
                width: pip_width,
                lit: i < lit,
            }
        })
        .collect()
}

/// A bar of `fraction`, flat, with rounded ends.
pub(crate) fn meter(parent: &mut ChildSpawnerCommands, fraction: f32, color: Color, height: f32) {
    parent
        .spawn((
            Node {
                width: percent(100),
                height: px(height),
                flex_shrink: 0.0,
                border_radius: rounded(height / 2.0),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(INK.with_alpha(0.12)),
        ))
        .with_children(|p| {
            p.spawn((
                Node {
                    width: percent(fraction.clamp(0.0, 1.0) * 100.0),
                    height: percent(100),
                    border_radius: rounded(height / 2.0),
                    ..default()
                },
                BackgroundColor(color),
            ));
        });
}

/// Hit points as separate pips, one per hit point: the lit ones take the health colour,
/// the spent ones sit dark, like the reference's segmented bar. Falls back to [`meter`]
/// when there are too many hit points to segment.
pub(crate) fn hp_pips(parent: &mut ChildSpawnerCommands, hp: i32, max: i32, height: f32) {
    let lit = hp.clamp(0, max.max(1));
    let max = max.max(1);
    if max > MAX_PIPS {
        let fraction = lit as f32 / max as f32;
        meter(parent, fraction, hp_color(fraction), height);
        return;
    }
    let color = hp_color(lit as f32 / max as f32);
    parent
        .spawn(Node {
            width: percent(100),
            height: px(height),
            flex_direction: FlexDirection::Row,
            column_gap: px(PIP_GAP),
            flex_shrink: 0.0,
            ..default()
        })
        .with_children(|p| {
            for i in 0..max {
                p.spawn((
                    Node {
                        flex_grow: 1.0,
                        height: percent(100),
                        border_radius: rounded(2.0),
                        ..default()
                    },
                    BackgroundColor(if i < lit { color } else { INK.with_alpha(0.14) }),
                ));
            }
        });
}

/// A nation's flag, as far as the game draws one: its colour in a rounded tab with a hairline.
pub(crate) fn nation_flag(parent: &mut ChildSpawnerCommands, color: Color, width: f32) {
    parent.spawn((
        Node {
            width: px(width),
            height: px(width * 0.7),
            flex_shrink: 0.0,
            border: UiRect::all(px(1)),
            border_radius: rounded(2.0),
            ..default()
        },
        BackgroundColor(color),
        BorderColor::all(BRONZE),
    ));
}

/// A nation's flag in a gilt frame: the pack's picture of it when there is one, else its colour
/// set in one of five layouts, picked from the nation's tag so the same nation always flies the
/// same flag.
pub(crate) fn framed_flag(
    parent: &mut ChildSpawnerCommands,
    color: Color,
    tag: &str,
    art: Option<Handle<Image>>,
    width: f32,
    height: f32,
) {
    let light = Color::srgb(0.965, 0.957, 0.925);
    let deep = {
        let c = color.to_srgba();
        Color::srgb(c.red * 0.55, c.green * 0.55, c.blue * 0.55)
    };
    let layout = tag
        .bytes()
        .fold(7u32, |h, b| h.wrapping_mul(31) + u32::from(b))
        % 5;
    let fill = |color: Color, left: f32, top: f32, width: f32, height: f32| {
        (
            Node {
                position_type: PositionType::Absolute,
                left: percent(left),
                top: percent(top),
                width: percent(width),
                height: percent(height),
                ..default()
            },
            BackgroundColor(color),
        )
    };
    parent
        .spawn((
            Node {
                width: px(width),
                height: px(height),
                flex_shrink: 0.0,
                padding: UiRect::all(px(3)),
                border: UiRect::all(px(2)),
                border_radius: rounded((width * 0.2).min(14.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.20, 0.16, 0.11)),
            BorderColor::all(TRIM),
            BoxShadow::new(
                Color::srgba(0.0, 0.0, 0.0, 0.35),
                px(0),
                px(2),
                px(0),
                px(5),
            ),
        ))
        .with_children(|p| {
            p.spawn((
                Node {
                    width: percent(100),
                    height: percent(100),
                    border_radius: rounded((width * 0.12).min(9.0)),
                    overflow: Overflow::clip(),
                    ..default()
                },
                BackgroundColor(color),
            ))
            .with_children(|c| match (art, layout) {
                (Some(image), _) => {
                    c.spawn((
                        ImageNode::new(image),
                        Node {
                            width: percent(100),
                            height: percent(100),
                            ..default()
                        },
                    ));
                }
                // A disc on a light field.
                (_, 0) => {
                    c.spawn(fill(light, 0.0, 0.0, 100.0, 100.0));
                    let disc = 0.46 * width;
                    c.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: percent(50),
                            top: percent(50),
                            width: px(disc),
                            height: px(disc),
                            margin: UiRect::all(px(-disc / 2.0)),
                            border_radius: rounded(999.0),
                            ..default()
                        },
                        BackgroundColor(color),
                    ));
                }
                // Three bands, the middle one light.
                (_, 1) => {
                    c.spawn(fill(light, 0.0, 33.0, 100.0, 34.0));
                    c.spawn(fill(deep, 0.0, 67.0, 100.0, 33.0));
                }
                // Three columns, the middle one light.
                (_, 2) => {
                    c.spawn(fill(light, 33.0, 0.0, 34.0, 100.0));
                    c.spawn(fill(deep, 67.0, 0.0, 33.0, 100.0));
                }
                // A light cross on the colour.
                (_, 3) => {
                    c.spawn(fill(light, 28.0, 0.0, 20.0, 100.0));
                    c.spawn(fill(light, 0.0, 40.0, 100.0, 20.0));
                }
                // Two halves with a light canton.
                (_, _) => {
                    c.spawn(fill(deep, 0.0, 50.0, 100.0, 50.0));
                    c.spawn(fill(light, 0.0, 0.0, 40.0, 50.0));
                }
            });
        });
}

// --- glyphs --------------------------------------------------------------------------------------
/// The glyph file of the pack: `hud_*` are readouts, anything else is an `icon_*`.
pub(crate) fn glyph_image(assets: &AssetServer, prefix: &str, name: &str) -> Handle<Image> {
    let file = if name.starts_with("hud_") {
        name.to_string()
    } else {
        format!("icon_{name}")
    };
    assets.load(format!("{prefix}ui/icons/{file}.png"))
}

/// A glyph of the given side, tinted `color`.
pub(crate) fn glyph(
    parent: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    prefix: &str,
    name: &str,
    side: f32,
    color: Color,
) {
    parent.spawn((
        ImageNode::new(glyph_image(assets, prefix, name)).with_color(color),
        Node {
            width: px(side),
            height: px(side),
            flex_shrink: 0.0,
            ..default()
        },
    ));
}

// --- buttons -------------------------------------------------------------------------------------
/// How a control is filled, and how that changes as the pointer crosses it. A button carries
/// one; [`paint`] does the rest.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Tone {
    /// Dark green with a gilt edge, cream text: the one action that moves the game on.
    Primary,
    /// A quiet well on paper: every other control.
    Quiet,
    /// Cannot be used now: drawn faint, and the pointer changes nothing.
    Dormant,
}

impl Tone {
    /// The fill under the pointer's current state.
    pub(crate) fn fill(self, interaction: Interaction) -> Color {
        let (rest, hover, pressed) = match self {
            Tone::Primary => (ACCENT, ACCENT_HOVER, ACCENT_PRESSED),
            Tone::Quiet => (WELL, WELL_HOVER, WELL_PRESSED),
            Tone::Dormant => return WELL.with_alpha(0.55),
        };
        match interaction {
            Interaction::Pressed => pressed,
            Interaction::Hovered => hover,
            Interaction::None => rest,
        }
    }

    /// The colour of the words and glyphs on it.
    pub(crate) fn ink(self) -> Color {
        match self {
            Tone::Primary => ON_ACCENT,
            Tone::Quiet => INK,
            Tone::Dormant => INK.with_alpha(0.32),
        }
    }
}

/// Repaint a control as the pointer moves over it.
pub(crate) fn paint(
    mut controls: Query<(&Interaction, &Tone, &mut BackgroundColor), Changed<Interaction>>,
) {
    for (interaction, tone, mut fill) in &mut controls {
        fill.0 = tone.fill(*interaction);
    }
}

/// A labelled button. Only `Action::EndTurn` is primary; the rest are quiet wells.
pub(crate) fn button(
    parent: &mut ChildSpawnerCommands,
    title: impl Into<String>,
    action: Action,
    tone: Tone,
) {
    let edge = match tone {
        Tone::Primary => TRIM.with_alpha(0.9),
        Tone::Quiet | Tone::Dormant => EDGE,
    };
    parent
        .spawn((
            Button,
            action,
            tone,
            Node {
                min_height: px(34),
                flex_shrink: 0.0,
                padding: UiRect::axes(px(14), px(6)),
                border: UiRect::all(px(1.5)),
                border_radius: rounded(4.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BorderColor::all(edge),
            BackgroundColor(tone.fill(Interaction::None)),
        ))
        .with_children(|p| bold(p, title, 13.0, tone.ink()));
}

// --- the map's own labels --------------------------------------------------------------------------
/// A white disc, made once. Cut in nine it is a pill of any length, so it is the only picture
/// the map's labels need.
#[derive(Resource)]
pub(crate) struct Shapes {
    disc: Handle<Image>,
    selection_tops: Vec<Handle<Image>>,
    selected_square: Handle<Image>,
}

/// Side of the disc picture, in pixels.
const DISC: u32 = 64;
/// Side of the selected unit's marker picture, in pixels.
const TOP_ART: u32 = 64;
const TOP_FRAMES: usize = 16;

fn inside_triangle(p: Vec2, a: Vec2, b: Vec2, c: Vec2) -> bool {
    let side = |a: Vec2, b: Vec2| (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
    let (ab, bc, ca) = (side(a, b), side(b, c), side(c, a));
    (ab >= 0.0 && bc >= 0.0 && ca >= 0.0) || (ab <= 0.0 && bc <= 0.0 && ca <= 0.0)
}

/// One frame of the selected unit's marker: an upside-down square pyramid turning about its
/// upright axis, seen from the map's camera (30 degrees above the ground) and lit from the upper
/// left. A square repeats every quarter turn, so `TOP_FRAMES` frames span 90 degrees and loop.
fn selection_top_image(frame: usize) -> Image {
    const SUPER: u32 = 4; // samples per pixel side
    const RIM: [f32; 3] = [0.30, 0.16, 0.05];
    const TOP: [f32; 3] = [0.84, 0.68, 0.40];
    const SHADE: [f32; 3] = [0.80, 0.33, 0.10];
    const LIT: [f32; 3] = [0.99, 0.80, 0.24];
    let turn = frame as f32 / TOP_FRAMES as f32 * TAU / 4.0;
    let (sin_e, cos_e) = (30f32.to_radians().sin(), 30f32.to_radians().cos());
    // The square top at height 0 and the apex below it, in world units (y up, z toward viewer).
    let corner = |k: usize| {
        let a = turn + k as f32 * TAU / 4.0 + TAU / 8.0;
        Vec3::new(a.cos() * 1.0, 0.0, a.sin() * 1.0)
    };
    let apex = Vec3::new(0.0, -2.0, 0.0);
    let scale = 23.0;
    let project = |p: Vec3| Vec2::new(32.0 + p.x * scale, 18.0 + (-p.y * cos_e + p.z * sin_e) * scale);
    let view = Vec3::new(0.0, sin_e, cos_e);
    let light = Vec3::new(-0.6, 0.55, 0.6).normalize();
    // Faces back to front: the sides that face the camera, then the top over them.
    let mut faces: Vec<([Vec2; 3], [f32; 3])> = Vec::new();
    for k in 0..4 {
        let (a, b) = (corner(k), corner((k + 1) % 4));
        let mut normal = (a - apex).cross(b - apex).normalize();
        if normal.dot((a + b) / 2.0 - Vec3::new(0.0, -1.0, 0.0)) < 0.0 {
            normal = -normal;
        }
        if normal.dot(view) <= 0.0 {
            continue;
        }
        let t = normal.dot(light).clamp(0.0, 1.0);
        let colour = std::array::from_fn(|i| SHADE[i] + (LIT[i] - SHADE[i]) * t);
        faces.push(([project(a), project(b), project(apex)], colour));
    }
    let top: Vec<Vec2> = (0..4).map(|k| project(corner(k))).collect();
    faces.push(([top[0], top[1], top[2]], TOP));
    faces.push(([top[0], top[2], top[3]], TOP));
    // The silhouette, grown a little from its centre, makes a dark rim round the whole marker.
    let centre = Vec2::new(32.0, 18.0 + scale * 0.6);
    let grow = |p: Vec2| centre + (p - centre) * 1.0 + (p - centre).normalize_or_zero() * 1.6;
    let mut data = vec![0u8; (TOP_ART * TOP_ART * 4) as usize];
    for y in 0..TOP_ART {
        for x in 0..TOP_ART {
            let (mut rgb, mut cover) = ([0.0f32; 3], 0.0f32);
            for sy in 0..SUPER {
                for sx in 0..SUPER {
                    let p = Vec2::new(
                        x as f32 + (sx as f32 + 0.5) / SUPER as f32,
                        y as f32 + (sy as f32 + 0.5) / SUPER as f32,
                    );
                    let mut hit = faces
                        .iter()
                        .rev()
                        .find(|(t, _)| inside_triangle(p, t[0], t[1], t[2]))
                        .map(|(_, c)| *c);
                    if hit.is_none()
                        && faces.iter().any(|(t, _)| inside_triangle(p, grow(t[0]), grow(t[1]), grow(t[2])))
                    {
                        hit = Some(RIM);
                    }
                    if let Some(c) = hit {
                        for i in 0..3 {
                            rgb[i] += c[i];
                        }
                        cover += 1.0;
                    }
                }
            }
            if cover > 0.0 {
                let at = ((y * TOP_ART + x) * 4) as usize;
                for i in 0..3 {
                    data[at + i] = (rgb[i] / cover * 255.0).round() as u8;
                }
                data[at + 3] = (cover / (SUPER * SUPER) as f32 * 255.0).round() as u8;
            }
        }
    }
    rgba_image(TOP_ART, TOP_ART, data)
}

/// The outline of the chosen square: a white diamond the size of a tile, 2x for crispness.
fn selected_square_image() -> Image {
    const W: u32 = 256;
    const H: u32 = 128;
    let mut data = vec![0u8; (W * H * 4) as usize];
    for y in 0..H {
        for x in 0..W {
            // distance, in image pixels, to the diamond |dx|/128 + |dy|/64 = 1, drawn just inside it
            let (dx, dy) = ((x as f32 + 0.5 - 128.0).abs(), (y as f32 + 0.5 - 64.0).abs());
            let d = ((dx / 128.0 + dy / 64.0) - 1.0) * (128.0 * 64.0) / (64.0f32.hypot(128.0));
            let cover = (2.6 - (d + 3.2).abs()).clamp(0.0, 1.0);
            let at = ((y * W + x) * 4) as usize;
            data[at..at + 4].copy_from_slice(&[255, 255, 255, (cover * 255.0).round() as u8]);
        }
    }
    rgba_image(W, H, data)
}

fn rgba_image(width: u32, height: u32, data: Vec<u8>) -> Image {
    Image::new(
        bevy::render::render_resource::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        bevy::render::render_resource::TextureDimension::D2,
        data,
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::default(),
    )
}

impl Shapes {
    pub(crate) fn new(images: &mut Assets<Image>) -> Self {
        let centre = (DISC as f32 - 1.0) / 2.0;
        let radius = DISC as f32 / 2.0;
        let mut data = Vec::with_capacity((DISC * DISC * 4) as usize);
        for y in 0..DISC {
            for x in 0..DISC {
                let distance = Vec2::new(x as f32 - centre, y as f32 - centre).length();
                // One pixel of edge, so the rim is soft at any scale.
                let cover = (radius - distance).clamp(0.0, 1.0);
                data.extend_from_slice(&[255, 255, 255, (cover * 255.0 + 0.5) as u8]);
            }
        }
        let image = Image::new(
            bevy::render::render_resource::Extent3d {
                width: DISC,
                height: DISC,
                depth_or_array_layers: 1,
            },
            bevy::render::render_resource::TextureDimension::D2,
            data,
            bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
            bevy::asset::RenderAssetUsages::default(),
        );
        Self {
            disc: images.add(image),
            selected_square: images.add(selected_square_image()),
            selection_tops: (0..TOP_FRAMES)
                .map(|frame| images.add(selection_top_image(frame)))
                .collect(),
        }
    }

    /// A pill-shaped sprite, for the bars and plates the map's pieces carry.
    pub(crate) fn bar(&self, size: Vec2, color: Color) -> Sprite {
        self.sprite(size, color, true)
    }

    /// The turning pyramid drawn above a selected unit.
    pub(crate) fn selection_top(&self) -> Sprite {
        let mut sprite = Sprite::from_image(self.selection_top_frame(0).clone());
        sprite.custom_size = Some(Vec2::splat(28.0));
        sprite
    }

    /// The outline of the chosen square, one tile in size.
    pub(crate) fn selected_square(&self) -> Sprite {
        let mut sprite = Sprite::from_image(self.selected_square.clone());
        sprite.custom_size = Some(Vec2::new(128.0, 64.0));
        sprite.color = SELECT;
        sprite
    }

    /// One orientation of the selected unit's top.
    pub(crate) fn selection_top_frame(&self, frame: usize) -> &Handle<Image> {
        &self.selection_tops[frame % TOP_FRAMES]
    }

    fn sprite(&self, size: Vec2, color: Color, pill: bool) -> Sprite {
        let mut sprite = Sprite::from_image(self.disc.clone());
        sprite.custom_size = Some(size);
        sprite.color = color;
        if pill {
            sprite.image_mode = SpriteImageMode::Sliced(TextureSlicer {
                border: BorderRect::all(DISC as f32 / 2.0 - 1.0),
                center_scale_mode: SliceScaleMode::Stretch,
                sides_scale_mode: SliceScaleMode::Stretch,
                max_corner_scale: 1.0,
            });
        }
        sprite
    }
}

/// A pill of `size` centred on `at`.
pub(crate) fn pill(commands: &mut Commands, shapes: &Shapes, at: Vec3, size: Vec2, color: Color) {
    commands.spawn((
        shapes.sprite(size, color, true),
        Transform::from_translation(at),
        WorldVisual,
    ));
}

/// A disc of `diameter` centred on `at`.
pub(crate) fn disc(
    commands: &mut Commands,
    shapes: &Shapes,
    at: Vec3,
    diameter: f32,
    color: Color,
) {
    commands.spawn((
        shapes.sprite(Vec2::splat(diameter), color, false),
        Transform::from_translation(at),
        WorldVisual,
    ));
}

/// A banner on the map: parchment inside a dark rim, `size` across.
pub(crate) fn label_plate(commands: &mut Commands, shapes: &Shapes, at: Vec3, size: Vec2) {
    pill(commands, shapes, at, size + Vec2::splat(3.0), BANNER_RIM);
    pill(commands, shapes, at + Vec3::Z * 0.02, size, BANNER);
}

/// A round badge on the map: `fill` inside a gold ring, for a count in cream.
pub(crate) fn medallion(
    commands: &mut Commands,
    shapes: &Shapes,
    at: Vec3,
    diameter: f32,
    fill: Color,
) {
    disc(commands, shapes, at, diameter, BRONZE);
    disc(commands, shapes, at + Vec3::Z * 0.03, diameter - 2.0, TRIM);
    disc(commands, shapes, at + Vec3::Z * 0.05, diameter - 5.0, fill);
}

/// Text on the map, centred on `at`.
pub(crate) fn map_text(
    commands: &mut Commands,
    label: impl Into<String>,
    size: f32,
    color: Color,
    at: Vec3,
) {
    commands.spawn((
        Text2d::new(label),
        font(size, FontWeight::SEMIBOLD),
        TextColor(color),
        Transform::from_translation(at),
        WorldVisual,
    ));
}

/// How wide `label` is when set at `size` in the semibold serif of [`map_text`], from the font's
/// own advance widths, so a plate can be cut to fit its name.
pub(crate) fn measure(label: &str, size: f32) -> f32 {
    static FACE: std::sync::OnceLock<Option<ttf_parser::Face<'static>>> = std::sync::OnceLock::new();
    let face = FACE.get_or_init(|| {
        let mut face = ttf_parser::Face::parse(SERIF, 0).ok()?;
        face.set_variation(ttf_parser::Tag::from_bytes(b"wght"), 600.0);
        Some(face)
    });
    let em = size * TYPE_SCALE;
    let Some(face) = face else {
        return text_width(label.chars().count(), size);
    };
    let units = f32::from(face.units_per_em());
    label
        .chars()
        .map(|c| {
            face.glyph_index(c)
                .and_then(|g| face.glyph_hor_advance(g))
                .map_or(em * 0.5, |advance| f32::from(advance) / units * em)
        })
        .sum()
}

/// How wide a label of `chars` characters is at `size`, near enough to size a pill around it.
pub(crate) fn text_width(chars: usize, size: f32) -> f32 {
    chars as f32 * size * TYPE_SCALE * 0.42
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_measured_by_their_letters_not_their_count() {
        // the same number of letters, but wide ones take more room
        assert!(measure("Hamhung", 13.0) > measure("Iilitri", 13.0) * 1.5);
        // and twice the size is twice the width
        let (small, large) = (measure("Seoul", 10.0), measure("Seoul", 20.0));
        assert!(small > 10.0 && (large / small - 2.0).abs() < 1e-3);
    }

    #[test]
    fn pips_are_one_per_hit_point_with_the_wounds_unlit() {
        let pips = pip_layout(2, 4, 42.0);
        assert_eq!(pips.len(), 4, "one pip per hit point");
        assert_eq!(
            pips.iter().filter(|p| p.lit).count(),
            2,
            "the wounds stay dark"
        );
    }

    #[test]
    fn pips_tile_their_width_with_gaps_and_stay_in_order() {
        let pips = pip_layout(3, 3, 42.0);
        assert_eq!(pips.len(), 3);
        assert!(pips.iter().all(|p| p.lit), "full health lights every pip");
        for pair in pips.windows(2) {
            let left_end = pair[0].centre + pair[0].width / 2.0;
            let right_start = pair[1].centre - pair[1].width / 2.0;
            assert!((right_start - left_end - PIP_GAP).abs() < 1e-3);
        }
        let first_start = pips[0].centre - pips[0].width / 2.0;
        let last_end = pips[2].centre + pips[2].width / 2.0;
        assert!((first_start + 21.0).abs() < 1e-3);
        assert!((last_end - 21.0).abs() < 1e-3);
    }

    #[test]
    fn pips_handle_one_hit_point_full_and_dead() {
        let one = pip_layout(1, 1, 42.0);
        assert_eq!(one.len(), 1);
        assert!(one[0].lit, "a lone hit point still shows");
        let dead = pip_layout(0, 4, 42.0);
        assert_eq!(dead.len(), 4);
        assert!(dead.iter().all(|p| !p.lit), "the dead show an empty track");
        let over = pip_layout(9, 4, 42.0);
        assert!(
            over.iter().all(|p| p.lit),
            "overheal clamps to full, never a fifth pip"
        );
    }

    #[test]
    fn pips_give_up_when_there_is_nothing_sane_to_draw() {
        assert!(pip_layout(3, 0, 42.0).is_empty());
        assert!(pip_layout(3, MAX_PIPS + 1, 42.0).is_empty());
    }

    #[test]
    fn the_health_colour_runs_green_amber_red() {
        assert_eq!(hp_color(1.0), GOOD);
        assert_eq!(hp_color(0.67), GOOD);
        assert_eq!(hp_color(0.66), AMBER);
        assert_eq!(hp_color(0.5), AMBER);
        assert_eq!(hp_color(0.33), BAD);
        assert_eq!(hp_color(0.0), BAD);
    }
}
