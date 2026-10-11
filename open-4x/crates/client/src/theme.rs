//! The interface's design language: light cards over the map, one steel-blue action, and dark
//! plates for the labels that sit on the map itself.
//!
//! Everything is flat. Surfaces are warm off-white cards with large radii, a hairline edge and a
//! soft drop shadow; text is a single dark slate in two weights of one sans face (Inter); icons
//! are white glyphs tinted at the place of use; the one primary control (ending the day) is
//! steel blue and nothing else is. On the map, names and counts sit on dark navy pills so they
//! read over any terrain, and a nation is a colour disc, the stand-in for its flag.
//!
//! Nothing here depends on the pack: no UI picture is needed but the glyphs, and the shapes the
//! map labels use are made at start-up (see [`Shapes`]).
use super::*;
use bevy::asset::AssetId;
use bevy::sprite::{BorderRect, SliceScaleMode, SpriteImageMode, TextureSlicer};
use bevy::text::{Font, FontWeight};

/// Height of the bar along the top edge; map clicks under it belong to the bar.
pub(crate) const TOP_BAR: f32 = 50.0;

/// The face every `Text` uses (see [`install_font`]). Inter, SIL OFL 1.1: `fonts/Inter-OFL.txt`.
const INTER: &[u8] = include_bytes!("../fonts/Inter-Variable.ttf");

/// Make Inter the face of every text that names no font of its own, which is all of them.
pub(crate) fn install_font(app: &mut App) {
    let font = Font::try_from_bytes(INTER.to_vec()).expect("the bundled font is a font");
    let mut fonts = app.world_mut().resource_mut::<Assets<Font>>();
    fonts
        .insert(AssetId::default(), font)
        .expect("the default font slot takes a font");
}

// --- palette ---------------------------------------------------------------------------------
/// A card: the surface panels, windows and the inspector are made of.
pub(crate) const CARD: Color = Color::srgba(0.969, 0.953, 0.914, 0.97);
/// The top bar: the card, a little brighter and a little less opaque.
pub(crate) const BAR: Color = Color::srgba(0.984, 0.976, 0.949, 0.95);
/// A control or a recessed field on a card, with its hover and pressed fills.
pub(crate) const WELL: Color = Color::srgb(0.925, 0.906, 0.855);
pub(crate) const WELL_HOVER: Color = Color::srgb(0.878, 0.855, 0.792);
pub(crate) const WELL_PRESSED: Color = Color::srgb(0.820, 0.796, 0.725);
/// The hairline around a card, and the rules inside one.
pub(crate) const EDGE: Color = Color::srgba(0.07, 0.10, 0.14, 0.10);
pub(crate) const RULE: Color = Color::srgba(0.07, 0.10, 0.14, 0.13);
/// Text: slate, and the same slate pulled back for what is secondary.
pub(crate) const INK: Color = Color::srgb(0.110, 0.149, 0.188);
pub(crate) const INK_SOFT: Color = Color::srgba(0.110, 0.149, 0.188, 0.60);
/// Steel blue: the one primary action, and the chosen thing among alternatives.
pub(crate) const ACCENT: Color = Color::srgb(0.263, 0.435, 0.592);
pub(crate) const ACCENT_HOVER: Color = Color::srgb(0.322, 0.502, 0.665);
pub(crate) const ACCENT_PRESSED: Color = Color::srgb(0.204, 0.357, 0.494);
/// Text and glyphs on the accent.
pub(crate) const ON_ACCENT: Color = Color::srgb(0.984, 0.980, 0.961);
/// Dark navy plates for labels over the map, and the text on them.
pub(crate) const PLATE: Color = Color::srgba(0.086, 0.133, 0.200, 0.92);
pub(crate) const ON_PLATE: Color = Color::srgb(0.965, 0.957, 0.925);
/// Good news, bad news, and a caution.
pub(crate) const GOOD: Color = Color::srgb(0.239, 0.576, 0.361);
pub(crate) const BAD: Color = Color::srgb(0.765, 0.235, 0.216);
pub(crate) const AMBER: Color = Color::srgb(0.835, 0.600, 0.157);
/// The white of a selection on the map, and of the route that leads out of it.
pub(crate) const SELECT: Color = Color::srgb(0.965, 0.957, 0.925);

// --- text --------------------------------------------------------------------------------------
fn font(size: f32, weight: FontWeight) -> TextFont {
    TextFont {
        font_size: size,
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
        Color::srgba(0.02, 0.05, 0.09, 0.30),
        px(0),
        px(8),
        px(0),
        px(24),
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
        padding: UiRect::all(px(16)),
        border: UiRect::all(px(1)),
        flex_direction: FlexDirection::Column,
        row_gap: px(8),
        border_radius: rounded(16.0),
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

/// A section heading: the small label, the way a card sets its parts apart.
pub(crate) fn heading(parent: &mut ChildSpawnerCommands, label: &str) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            flex_shrink: 0.0,
            row_gap: px(6),
            margin: UiRect::top(px(2)),
            ..default()
        })
        .with_children(|p| {
            caption(p, label);
            rule(p);
        });
}

/// A name on the left and its figure on the right, the way the reference sets its numbers.
pub(crate) fn row(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    value: impl Into<String>,
    tint: Color,
) {
    parent
        .spawn(Node {
            width: percent(100),
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            column_gap: px(12),
            flex_shrink: 0.0,
            ..default()
        })
        .with_children(|p| {
            text(p, label, 13.0, INK_SOFT);
            bold(p, value, 13.0, tint);
        });
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

/// A nation's flag, as far as the game draws one: its colour in a rounded tab with a hairline.
pub(crate) fn nation_flag(parent: &mut ChildSpawnerCommands, color: Color, width: f32) {
    parent.spawn((
        Node {
            width: px(width),
            height: px(width * 0.7),
            flex_shrink: 0.0,
            border: UiRect::all(px(1)),
            border_radius: rounded(width * 0.14),
            ..default()
        },
        BackgroundColor(color),
        BorderColor::all(Color::WHITE.with_alpha(0.85)),
        BoxShadow::new(
            Color::srgba(0.0, 0.0, 0.0, 0.25),
            px(0),
            px(1),
            px(0),
            px(3),
        ),
    ));
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
    /// Steel blue, white text: the one action that moves the game on.
    Primary,
    /// A quiet well on a card: every other control.
    Quiet,
    /// The alternative that is already chosen: blue-tinted, so it reads as on.
    Chosen,
    /// Cannot be used now: drawn faint, and the pointer changes nothing.
    Dormant,
}

impl Tone {
    /// The fill under the pointer's current state.
    pub(crate) fn fill(self, interaction: Interaction) -> Color {
        let (rest, hover, pressed) = match self {
            Tone::Primary => (ACCENT, ACCENT_HOVER, ACCENT_PRESSED),
            Tone::Quiet => (WELL, WELL_HOVER, WELL_PRESSED),
            Tone::Chosen => (
                Color::srgb(0.847, 0.894, 0.941),
                Color::srgb(0.800, 0.859, 0.918),
                Color::srgb(0.749, 0.820, 0.894),
            ),
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
            Tone::Quiet | Tone::Chosen => INK,
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
    parent
        .spawn((
            Button,
            action,
            tone,
            Node {
                min_height: px(34),
                flex_shrink: 0.0,
                padding: UiRect::axes(px(14), px(7)),
                border_radius: rounded(10.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
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
}

/// Side of the disc picture, in pixels.
const DISC: u32 = 64;

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
        }
    }

    /// A pill-shaped sprite, for the bars and plates the map's pieces carry.
    pub(crate) fn bar(&self, size: Vec2, color: Color) -> Sprite {
        self.sprite(size, color, true)
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

/// A nation's flag on the map: its colour in a disc with a white rim.
pub(crate) fn map_flag(
    commands: &mut Commands,
    shapes: &Shapes,
    at: Vec3,
    diameter: f32,
    color: Color,
) {
    disc(commands, shapes, at, diameter, Color::WHITE);
    disc(commands, shapes, at + Vec3::Z * 0.05, diameter - 4.0, color);
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

/// How wide a label of `chars` characters is at `size`, near enough to size a pill around it.
pub(crate) fn text_width(chars: usize, size: f32) -> f32 {
    chars as f32 * size * 0.56
}
