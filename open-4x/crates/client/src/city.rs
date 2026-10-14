//! The city view: a window over the map that shows what a city gathers and what it builds.
//!
//! A sheet of aged parchment in a gilt frame over a dimmed map, set in two serif faces (Playfair
//! Display for names and figures, EB Garamond for the rest) and engraved in sepia. A header holds
//! the city's picture, its names and a harbour engraving; under it the economy (people, food,
//! shields, gold, industry, land, and the granary over a field engraving) fills the left column,
//! and production (what is being built, development, and a card for every design the pack
//! offers) the right. Foreign cities show the left column and what they are building.
//!
//! The window keeps its own palette and controls rather than the rest of the interface's flat
//! cards, so everything it needs lives here: the faces ([`install_fonts`]) and the hover colours
//! of its controls ([`Paper`], [`paint`]).
use super::*;
use crate::presentation::{city_sprite, nation_color};
use crate::theme::{glyph, nation_flag, rounded};
use bevy::asset::uuid_handle;
use bevy::text::{Font, FontWeight};
use bevy::ui::FocusPolicy;
use fourx_sim::City;

/// Cost of one step of industrial development, in gold (see `Command::Develop`).
const DEVELOP_COST: i32 = 40;

// --- faces -----------------------------------------------------------------------------------
/// Playfair Display, SIL OFL 1.1: `fonts/PlayfairDisplay-OFL.txt`.
const DISPLAY: Handle<Font> = uuid_handle!("7a1c0b9e-52d4-4f1b-9d6e-3b1f6a0c8d21");
/// EB Garamond, SIL OFL 1.1: `fonts/EBGaramond-OFL.txt`.
const SERIF: Handle<Font> = uuid_handle!("c4e2f7a8-1b36-4a5d-8e90-62d7b13f4a55");

/// Make the two serif faces available to the city view.
pub(super) fn install_fonts(app: &mut App) {
    let mut fonts = app.world_mut().resource_mut::<Assets<Font>>();
    for (handle, bytes) in [
        (
            DISPLAY,
            &include_bytes!("../fonts/PlayfairDisplay-Variable.ttf")[..],
        ),
        (
            SERIF,
            &include_bytes!("../fonts/EBGaramond-Variable.ttf")[..],
        ),
    ] {
        let font = Font::try_from_bytes(bytes.to_vec()).expect("the bundled font is a font");
        fonts
            .insert(handle.id(), font)
            .expect("a font slot takes a font");
    }
}

// --- palette ---------------------------------------------------------------------------------
/// Dark brown ink, and the same pulled back for what is secondary.
const INK: Color = Color::srgb(0.169, 0.125, 0.086);
const INK_SOFT: Color = Color::srgba(0.235, 0.169, 0.102, 0.70);
/// The red-brown of headings and the close button.
const RED: Color = Color::srgb(0.545, 0.176, 0.133);
/// Gilt: the frame, the rules and the trim of controls.
const GILT: Color = Color::srgb(0.718, 0.545, 0.208);
const GILT_SOFT: Color = Color::srgba(0.718, 0.545, 0.208, 0.55);
/// The dark edge of the window, and the hairline round a tile.
const FRAME: Color = Color::srgb(0.290, 0.188, 0.098);
const HAIR: Color = Color::srgba(0.443, 0.310, 0.141, 0.42);
/// Words on the navy button.
const ON_NAVY: Color = Color::srgb(0.965, 0.945, 0.878);
/// Good news and bad news on paper.
const GROWTH: Color = Color::srgb(0.251, 0.541, 0.196);
const FAMINE: Color = Color::srgb(0.690, 0.176, 0.133);

/// How a control of the window is filled, and how that changes as the pointer crosses it.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(super) enum Paper {
    /// A lighter patch of paper: tiles and every ordinary control.
    Tile,
    /// The design that is already being built.
    Chosen,
    /// Cannot be used now.
    Faint,
    /// Deep blue: the one thing that spends gold.
    Navy,
    /// The red of the close button.
    Red,
}

impl Paper {
    fn fill(self, interaction: Interaction) -> Color {
        let (rest, hover, pressed) = match self {
            Paper::Tile => (
                Color::srgb(0.957, 0.918, 0.812),
                Color::srgb(0.984, 0.953, 0.859),
                Color::srgb(0.914, 0.867, 0.741),
            ),
            Paper::Chosen => (
                Color::srgb(0.929, 0.855, 0.643),
                Color::srgb(0.949, 0.880, 0.682),
                Color::srgb(0.894, 0.816, 0.604),
            ),
            Paper::Faint => return Color::srgba(0.957, 0.918, 0.812, 0.45),
            Paper::Navy => (
                Color::srgb(0.114, 0.239, 0.396),
                Color::srgb(0.157, 0.306, 0.486),
                Color::srgb(0.078, 0.176, 0.310),
            ),
            Paper::Red => (
                Color::srgb(0.635, 0.141, 0.110),
                Color::srgb(0.725, 0.188, 0.149),
                Color::srgb(0.502, 0.102, 0.078),
            ),
        };
        match interaction {
            Interaction::Pressed => pressed,
            Interaction::Hovered => hover,
            Interaction::None => rest,
        }
    }
}

/// Repaint the window's controls as the pointer moves over them.
pub(super) fn paint(
    mut controls: Query<(&Interaction, &Paper, &mut BackgroundColor), Changed<Interaction>>,
) {
    for (interaction, paper, mut fill) in &mut controls {
        fill.0 = paper.fill(*interaction);
    }
}

/// Marks the window of the city view so that the wheel and a finger can scroll it when it is
/// taller than the screen.
#[derive(Component)]
pub(super) struct Scrolled;

/// Scroll the city view with the wheel or a drag.
pub(super) fn scroll(
    mut wheel: MessageReader<bevy::input::mouse::MouseWheel>,
    touches: Res<Touches>,
    mut view: Query<(&ComputedNode, &mut ScrollPosition), With<Scrolled>>,
) {
    let mut delta: f32 = wheel.read().map(|e| -e.y * 30.0).sum();
    delta -= touches.iter().map(|t| t.delta().y).sum::<f32>();
    if delta == 0.0 {
        return;
    }
    for (node, mut scroll) in &mut view {
        let max = (node.content_size().y - node.size().y) * node.inverse_scale_factor();
        scroll.y = (scroll.y + delta).clamp(0.0, max.max(0.0));
    }
}

// --- text ------------------------------------------------------------------------------------
fn write(
    p: &mut ChildSpawnerCommands,
    value: impl Into<String>,
    font: &Handle<Font>,
    size: f32,
    weight: i32,
    color: Color,
) {
    p.spawn((
        Node {
            flex_shrink: 0.0,
            ..default()
        },
        Text::new(value),
        TextFont {
            font: font.clone(),
            font_size: size,
            weight: FontWeight(weight as u16),
            ..default()
        },
        TextColor(color),
    ));
}

/// Names, figures and headings.
fn display(
    p: &mut ChildSpawnerCommands,
    value: impl Into<String>,
    size: f32,
    weight: i32,
    color: Color,
) {
    write(p, value, &DISPLAY, size, weight, color);
}

/// Running text.
fn serif(p: &mut ChildSpawnerCommands, value: impl Into<String>, size: f32, color: Color) {
    write(p, value, &SERIF, size, 500, color);
}

/// A small label in capitals over a figure.
fn label(p: &mut ChildSpawnerCommands, value: &str) {
    write(p, value.to_uppercase(), &SERIF, 12.0, 700, INK_SOFT);
}

/// What the view's widgets need to find the pack's pictures.
struct Kit<'a> {
    assets: &'a AssetServer,
    prefix: &'a str,
}
impl Kit<'_> {
    /// A glyph of the given side, tinted `color`.
    fn glyph(&self, p: &mut ChildSpawnerCommands, name: &str, side: f32, color: Color) {
        glyph(p, self.assets, self.prefix, name, side, color);
    }

    fn image(&self, path: &str) -> Handle<Image> {
        self.assets.load(format!("{}{path}", self.prefix))
    }
}

// --- pieces ----------------------------------------------------------------------------------
/// A hairline that takes the rest of its row.
fn line(p: &mut ChildSpawnerCommands, color: Color) {
    p.spawn((
        Node {
            flex_grow: 1.0,
            height: px(1),
            ..default()
        },
        BackgroundColor(color),
    ));
}

/// A small diamond, the way the window sets its rules and corners.
fn diamond(p: &mut ChildSpawnerCommands, side: f32) {
    p.spawn((
        Node {
            width: px(side),
            height: px(side),
            flex_shrink: 0.0,
            border: UiRect::all(px(1)),
            ..default()
        },
        BackgroundColor(GILT),
        BorderColor::all(FRAME),
        UiTransform::from_rotation(Rot2::degrees(45.0)),
    ));
}

/// The rule under the header: gilt, with a diamond at its middle.
fn ornament(p: &mut ChildSpawnerCommands) {
    p.spawn(Node {
        width: percent(100),
        align_items: AlignItems::Center,
        column_gap: px(10),
        flex_shrink: 0.0,
        ..default()
    })
    .with_children(|p| {
        line(p, GILT_SOFT);
        diamond(p, 7.0);
        line(p, GILT_SOFT);
    });
}

/// A section heading with its hairline.
fn section(p: &mut ChildSpawnerCommands, title: &str) {
    p.spawn(Node {
        align_items: AlignItems::Center,
        column_gap: px(12),
        flex_shrink: 0.0,
        ..default()
    })
    .with_children(|p| {
        display(p, title, 19.0, 600, RED);
        line(p, GILT_SOFT);
    });
}

/// The look of a patch of paper set into the sheet.
fn tile(radius: f32) -> Node {
    Node {
        border: UiRect::all(px(1)),
        border_radius: rounded(radius),
        ..default()
    }
}

/// A small pill: a glyph and a few words.
fn chip(
    p: &mut ChildSpawnerCommands,
    kit: &Kit,
    name: &str,
    colour: Color,
    words: impl Into<String>,
) {
    p.spawn((
        Node {
            align_items: AlignItems::Center,
            column_gap: px(7),
            padding: UiRect::axes(px(11), px(5)),
            ..tile(5.0)
        },
        BackgroundColor(Paper::Tile.fill(Interaction::None)),
        BorderColor::all(HAIR),
    ))
    .with_children(|p| {
        kit.glyph(p, name, 16.0, colour);
        write(p, words, &SERIF, 15.0, 600, INK);
    });
}

/// One readout: a glyph, a figure, what it counts, and optionally how it adds up.
fn readout(
    p: &mut ChildSpawnerCommands,
    kit: &Kit,
    name: &str,
    colour: Color,
    (figure, figure_colour): (String, Color),
    what: &str,
    detail: Option<String>,
) {
    p.spawn((
        Node {
            align_items: AlignItems::Center,
            column_gap: px(12),
            padding: UiRect::axes(px(14), px(8)),
            ..tile(6.0)
        },
        BackgroundColor(Paper::Tile.fill(Interaction::None)),
        BorderColor::all(HAIR),
    ))
    .with_children(|p| {
        kit.glyph(p, name, 36.0, colour);
        p.spawn(Node {
            flex_direction: FlexDirection::Column,
            ..default()
        })
        .with_children(|p| {
            display(p, figure, 25.0, 700, figure_colour);
            label(p, what);
            if let Some(detail) = detail {
                serif(p, detail, 13.0, INK_SOFT);
            }
        });
    });
}

/// Whole days until `needed` is gathered at `per_day`, or `None` if it never will be.
pub(crate) fn days(needed: i32, per_day: i32) -> Option<i32> {
    (per_day > 0).then(|| (needed.max(0) + per_day - 1) / per_day)
}
fn in_days(n: i32) -> String {
    if n == 1 {
        "1 week".into()
    } else {
        format!("{n} weeks")
    }
}

/// A bar in a dark case, `fraction` full.
fn bar(p: &mut ChildSpawnerCommands, fraction: f32, tint: Color) {
    p.spawn((
        Node {
            flex_grow: 1.0,
            height: px(16),
            border: UiRect::all(px(2)),
            border_radius: rounded(4.0),
            overflow: Overflow::clip(),
            ..default()
        },
        BackgroundColor(Color::srgb(0.20, 0.17, 0.14)),
        BorderColor::all(FRAME),
    ))
    .with_children(|p| {
        p.spawn((
            Node {
                width: percent(fraction.clamp(0.0, 1.0) * 100.0),
                height: percent(100),
                ..default()
            },
            BackgroundColor(tint),
        ));
    });
}

/// A button of the window: a label (and anything `inside` adds) on a patch of paper.
fn control(
    p: &mut ChildSpawnerCommands,
    action: Option<Action>,
    paper: Paper,
    node: Node,
    inside: impl FnOnce(&mut ChildSpawnerCommands),
) {
    let mut button = p.spawn((
        Button,
        paper,
        Node {
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..node
        },
        BackgroundColor(paper.fill(Interaction::None)),
        BorderColor::all(if paper == Paper::Faint { HAIR } else { GILT }),
    ));
    if let Some(action) = action {
        button.insert(action);
    }
    button.with_children(inside);
}

/// A labelled, gilt-edged button.
fn labelled(p: &mut ChildSpawnerCommands, title: impl Into<String>, action: Action) {
    control(
        p,
        Some(action),
        Paper::Tile,
        Node {
            min_width: px(116),
            height: px(40),
            flex_shrink: 0.0,
            padding: UiRect::horizontal(px(20)),
            ..tile(5.0)
        },
        |p| display(p, title, 15.0, 600, INK),
    );
}

/// The window of the city view over the map. `narrow` is a phone-shaped screen, where the two
/// columns stack.
pub(super) fn spawn(
    commands: &mut Commands,
    assets: &AssetServer,
    session: &Session,
    game: &Game,
    id: Id,
    narrow: bool,
) {
    let c = &game.cities[&id];
    let kit = Kit {
        assets,
        prefix: &session.asset_prefix,
    };
    let mine = c.owner == session.player;
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.03, 0.05, 0.08, 0.58)),
            FocusPolicy::Block,
            GlobalZIndex(50),
            UiRoot,
        ))
        .with_children(|p| {
            p.spawn((
                Node {
                    width: if narrow { percent(96) } else { px(960) },
                    max_width: percent(96),
                    max_height: percent(94),
                    padding: UiRect::axes(px(if narrow { 20 } else { 34 }), px(26)),
                    border: UiRect::all(px(3)),
                    border_radius: rounded(8.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(12),
                    ..default()
                },
                ImageNode::new(kit.image("ui/paper.png")).with_mode(NodeImageMode::Tiled {
                    tile_x: true,
                    tile_y: true,
                    stretch_value: 0.5,
                }),
                BorderColor::all(FRAME),
                BoxShadow::new(
                    Color::srgba(0.0, 0.0, 0.0, 0.5),
                    px(0),
                    px(10),
                    px(0),
                    px(30),
                ),
                FocusPolicy::Block,
            ))
            .with_children(|p| {
                frame(p, &kit);
                // Everything but the footer scrolls when the window is short.
                p.spawn((
                    Node {
                        flex_shrink: 1.0,
                        min_height: px(0),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(14),
                        overflow: Overflow::scroll_y(),
                        ..default()
                    },
                    ScrollPosition::default(),
                    Scrolled,
                ))
                .with_children(|p| {
                    header(p, &kit, game, session, c, narrow);
                    ornament(p);
                    p.spawn(Node {
                        flex_direction: if narrow {
                            FlexDirection::Column
                        } else {
                            FlexDirection::Row
                        },
                        column_gap: px(34),
                        row_gap: px(18),
                        align_items: AlignItems::Stretch,
                        flex_shrink: 0.0,
                        ..default()
                    })
                    .with_children(|body| {
                        body.spawn(Node {
                            width: if narrow { percent(100) } else { auto() },
                            flex_grow: 1.0,
                            flex_basis: px(0),
                            min_width: px(0),
                            flex_direction: FlexDirection::Column,
                            row_gap: px(10),
                            ..default()
                        })
                        .with_children(|p| economy(p, &kit, game, c));
                        body.spawn(Node {
                            width: if narrow { percent(100) } else { auto() },
                            flex_grow: 1.1,
                            flex_basis: px(0),
                            min_width: px(0),
                            flex_direction: FlexDirection::Column,
                            row_gap: px(10),
                            ..default()
                        })
                        .with_children(|p| production(p, &kit, game, session, c, mine));
                    });
                });
                p.spawn(Node {
                    width: percent(100),
                    justify_content: JustifyContent::FlexEnd,
                    align_items: AlignItems::Center,
                    column_gap: px(10),
                    flex_shrink: 0.0,
                    ..default()
                })
                .with_children(|p| {
                    if !mine && let Some(faction) = game.factions.get(&c.owner) {
                        let leader = faction.leader.trim();
                        labelled(
                            p,
                            if leader.is_empty() {
                                "Talk".to_string()
                            } else {
                                format!("Talk to {leader}")
                            },
                            Action::Talk(c.owner),
                        );
                    }
                    labelled(p, "Close", Action::CloseCity);
                });
                close_button(p, &kit);
            });
        });
}

/// The gilt line inside the edge of the window, and a bracket in each corner.
fn frame(p: &mut ChildSpawnerCommands, kit: &Kit) {
    p.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(7),
            right: px(7),
            top: px(7),
            bottom: px(7),
            border: UiRect::all(px(1.5)),
            border_radius: rounded(4.0),
            ..default()
        },
        BorderColor::all(GILT_SOFT),
    ));
    for (left, top, flip_x, flip_y) in [
        (true, true, false, false),
        (false, true, true, false),
        (true, false, false, true),
        (false, false, true, true),
    ] {
        let edge = |on: bool| if on { px(3) } else { auto() };
        let mut image = ImageNode::new(kit.image("ui/flourish.png"));
        image.flip_x = flip_x;
        image.flip_y = flip_y;
        p.spawn((
            image,
            Node {
                position_type: PositionType::Absolute,
                width: px(48),
                height: px(48),
                left: edge(left),
                right: edge(!left),
                top: edge(top),
                bottom: edge(!top),
                ..default()
            },
        ));
    }
}

/// The red button in the corner that closes the window.
fn close_button(p: &mut ChildSpawnerCommands, kit: &Kit) {
    p.spawn((
        Button,
        Action::CloseCity,
        Paper::Red,
        Node {
            position_type: PositionType::Absolute,
            top: px(-16),
            right: px(-16),
            width: px(40),
            height: px(40),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            border: UiRect::all(px(3)),
            border_radius: rounded(999.0),
            ..default()
        },
        BackgroundColor(Paper::Red.fill(Interaction::None)),
        BorderColor::all(GILT),
        BoxShadow::new(Color::srgba(0.0, 0.0, 0.0, 0.4), px(0), px(3), px(0), px(8)),
    ))
    .with_children(|p| kit.glyph(p, "cross", 16.0, ON_NAVY));
}

/// The portrait, the names, and an engraving of the harbour.
fn header(
    p: &mut ChildSpawnerCommands,
    kit: &Kit,
    game: &Game,
    session: &Session,
    c: &City,
    narrow: bool,
) {
    let nation = game.factions.get(&c.owner);
    p.spawn(Node {
        width: percent(100),
        align_items: AlignItems::Center,
        column_gap: px(20),
        flex_shrink: 0.0,
        ..default()
    })
    .with_children(|h| {
        // The engraving sits behind the words and fades out to the left.
        if !narrow {
            h.spawn((
                ImageNode::new(kit.image("ui/city_panorama.png")),
                Node {
                    position_type: PositionType::Absolute,
                    right: px(0),
                    top: px(0),
                    width: px(540),
                    height: px(112),
                    ..default()
                },
            ));
        }
        // The city as the map draws it, in a gilt frame of its own.
        let (width, height) = if narrow {
            (104.0, 74.0)
        } else {
            (160.0, 108.0)
        };
        h.spawn((
            Node {
                width: px(width),
                height: px(height),
                flex_shrink: 0.0,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border: UiRect::all(px(3)),
                border_radius: rounded(4.0),
                overflow: Overflow::clip(),
                ..default()
            },
            BorderColor::all(GILT),
            BoxShadow::new(
                Color::srgba(0.1, 0.05, 0.0, 0.35),
                px(0),
                px(2),
                px(0),
                px(6),
            ),
        ))
        .with_children(|p| {
            p.spawn((
                ImageNode::new(kit.image("ui/city_backdrop.png")).with_mode(NodeImageMode::Stretch),
                Node {
                    position_type: PositionType::Absolute,
                    width: percent(100),
                    height: percent(100),
                    ..default()
                },
            ));
            let sprite = city_sprite(&session.pack.visuals, game, c.owner);
            p.spawn((
                ImageNode::new(kit.image(sprite)),
                Node {
                    height: percent(110),
                    margin: UiRect::top(px(8)),
                    ..default()
                },
            ));
        });
        h.spawn(Node {
            flex_shrink: 0.0,
            flex_direction: FlexDirection::Column,
            row_gap: px(4),
            ..default()
        })
        .with_children(|p| {
            p.spawn(Node {
                align_items: AlignItems::Center,
                column_gap: px(8),
                ..default()
            })
            .with_children(|p| {
                nation_flag(p, nation_color(game, c.owner, 1.0), 22.0);
                label(p, nation.map_or("Unknown", |f| f.name.as_str()));
            });
            display(p, &c.name, if narrow { 30.0 } else { 46.0 }, 700, INK);
            p.spawn(Node {
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(8),
                row_gap: px(6),
                margin: UiRect::top(px(2)),
                ..default()
            })
            .with_children(|p| {
                if c.capital {
                    chip(p, kit, "star", GILT, "Capital");
                }
                chip(
                    p,
                    kit,
                    "hud_people",
                    INK_SOFT,
                    format!("Population {}", c.population),
                );
                chip(
                    p,
                    kit,
                    "globe",
                    INK_SOFT,
                    format!("Border level {}", c.border),
                );
            });
        });
    });
}

/// The left column: what the land gives and what the people eat.
fn economy(p: &mut ChildSpawnerCommands, kit: &Kit, game: &Game, c: &City) {
    section(p, "Economy");
    let surplus = c.food_surplus();
    p.spawn(Node {
        display: Display::Grid,
        grid_template_columns: vec![RepeatedGridTrack::flex(2, 1.0)],
        column_gap: px(10),
        row_gap: px(10),
        ..default()
    })
    .with_children(|p| {
        let brown = Color::srgb(0.36, 0.24, 0.14);
        let iron = Color::srgb(0.36, 0.34, 0.33);
        let gold = Color::srgb(0.80, 0.57, 0.10);
        readout(
            p,
            kit,
            "hud_people",
            brown,
            (c.population.to_string(), INK),
            "Citizens",
            Some(format!("{} eat {} food", c.population, c.food_eaten())),
        );
        readout(
            p,
            kit,
            "wheat",
            gold,
            (
                format!("{surplus:+}"),
                if surplus < 0 { FAMINE } else { INK },
            ),
            "Food a week",
            Some(format!(
                "{} grown, {} eaten",
                c.harvest.food,
                c.food_eaten()
            )),
        );
        readout(
            p,
            kit,
            "tools",
            iron,
            (c.shields().to_string(), INK),
            "Shields a week",
            Some(format!(
                "{} land + {} industry",
                c.harvest.shields, c.industry
            )),
        );
        readout(
            p,
            kit,
            "hud_gold",
            gold,
            (format!("{:+}", c.harvest.gold), INK),
            "Gold a week",
            None,
        );
        readout(
            p,
            kit,
            "hud_industry",
            iron,
            (c.industry.to_string(), INK),
            "Industry",
            None,
        );
        readout(
            p,
            kit,
            "globe",
            Color::srgb(0.27, 0.48, 0.22),
            (game.territory_of(c.id).len().to_string(), INK),
            "Squares held",
            Some(format!("border level {}", c.border)),
        );
    });
    // The granary: a citizen is born when it fills, and one starves when it runs dry.
    let size = c.granary_size();
    p.spawn(Node {
        margin: UiRect::top(px(6)),
        ..default()
    })
    .with_children(|p| section(p, "Granary"));
    p.spawn(Node {
        align_items: AlignItems::Center,
        column_gap: px(12),
        flex_shrink: 0.0,
        ..default()
    })
    .with_children(|p| {
        bar(
            p,
            c.granary as f32 / size.max(1) as f32,
            if surplus < 0 { FAMINE } else { GROWTH },
        );
        display(p, format!("{} / {}", c.granary, size), 15.0, 600, INK);
    });
    serif(
        p,
        match surplus {
            s if s > 0 => format!(
                "A citizen is born in {}.",
                in_days(days(size - c.granary, s).unwrap_or(0))
            ),
            s if s < 0 => {
                let left = (c.granary + -s - 1) / -s;
                format!("Famine: a citizen starves in {}.", in_days(left.max(1)))
            }
            _ => "The city is neither growing nor starving.".to_string(),
        },
        14.0,
        if surplus < 0 { FAMINE } else { INK_SOFT },
    );
    // An engraving of the fields, with the rule of borders written across its foot.
    p.spawn(Node {
        position_type: PositionType::Relative,
        width: percent(100),
        height: px(112),
        flex_grow: 1.0,
        min_height: px(100),
        flex_shrink: 0.0,
        justify_content: JustifyContent::Center,
        align_items: AlignItems::FlexEnd,
        overflow: Overflow::clip(),
        ..default()
    })
    .with_children(|p| {
        p.spawn((
            ImageNode::new(kit.image("ui/city_fields.png")).with_mode(NodeImageMode::Stretch),
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                ..default()
            },
        ));
        serif(
            p,
            "Its border never changes: whoever holds the city holds the land.",
            13.5,
            INK,
        );
    });
}

/// The right column: what is being built, development, and the designs to choose from.
fn production(
    p: &mut ChildSpawnerCommands,
    kit: &Kit,
    game: &Game,
    session: &Session,
    c: &City,
    mine: bool,
) {
    section(p, "Production");
    if session.rules.economy.enabled {
        serif(
            p,
            if game.home_city(c.id) {
                "Home territory"
            } else {
                "Colony: goods must be shipped home"
            },
            14.0,
            INK_SOFT,
        );
        for b in [fourx_sim::Building::Factory, fourx_sim::Building::Port] {
            if c.buildings.contains(&b) {
                serif(p, format!("{} operating", b.name()), 14.0, INK_SOFT);
            } else if mine && (b != fourx_sim::Building::Port || game.coastal(c.position)) {
                crate::theme::button(
                    p,
                    &format!(
                        "Build {} · {} production",
                        b.name(),
                        session.rules.economy.building_cost(b)
                    ),
                    Action::Build(c.id, b),
                    crate::theme::Tone::Quiet,
                );
            }
        }
        if let Some(b) = c.construction {
            serif(
                p,
                format!(
                    "{}: {}/{} production",
                    b.name(),
                    c.construction_progress,
                    session.rules.economy.building_cost(b)
                ),
                14.0,
                INK_SOFT,
            );
        }
    }
    let building = c
        .production
        .as_ref()
        .and_then(|id| session.rules.units.get(id));
    // The current work, with how far along it is.
    p.spawn((
        Node {
            width: percent(100),
            align_items: AlignItems::Center,
            column_gap: px(14),
            padding: UiRect::axes(px(14), px(11)),
            flex_shrink: 0.0,
            ..tile(6.0)
        },
        BackgroundColor(Paper::Tile.fill(Interaction::None)),
        BorderColor::all(HAIR),
    ))
    .with_children(|p| match building {
        Some(def) => {
            sprite(p, kit, &def.sprite, 46.0);
            p.spawn(Node {
                flex_grow: 1.0,
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                ..default()
            })
            .with_children(|p| {
                display(p, def.name.clone(), 18.0, 700, INK);
                let left = days(def.cost - c.progress, c.shields());
                serif(
                    p,
                    match left {
                        Some(n) => format!("Ready in {}", in_days(n.max(1))),
                        None => "Nothing is gathered to build it".to_string(),
                    },
                    14.0,
                    if left.is_some() { INK_SOFT } else { FAMINE },
                );
                p.spawn(Node {
                    align_items: AlignItems::Center,
                    column_gap: px(10),
                    ..default()
                })
                .with_children(|p| {
                    bar(
                        p,
                        c.progress as f32 / def.cost.max(1) as f32,
                        Color::srgb(0.17, 0.35, 0.57),
                    );
                    display(p, format!("{} / {}", c.progress, def.cost), 14.0, 600, INK);
                });
            });
        }
        None => {
            kit.glyph(p, "gear", 32.0, Color::srgb(0.55, 0.40, 0.17));
            p.spawn(Node {
                flex_direction: FlexDirection::Column,
                ..default()
            })
            .with_children(|p| {
                display(p, "Idle", 19.0, 700, INK);
                serif(
                    p,
                    if mine {
                        "Nothing is being built. Choose a project below."
                    } else {
                        "Nothing is being built."
                    },
                    14.0,
                    INK_SOFT,
                );
            });
        }
    });
    if !mine {
        return;
    }
    // Development: gold into permanent shields.
    let rich = game
        .factions
        .get(&c.owner)
        .is_some_and(|f| f.gold >= DEVELOP_COST);
    let paper = if rich { Paper::Navy } else { Paper::Faint };
    let ink = if rich { ON_NAVY } else { INK_SOFT };
    control(
        p,
        rich.then_some(Action::Develop),
        paper,
        Node {
            width: percent(100),
            height: px(48),
            flex_shrink: 0.0,
            column_gap: px(10),
            padding: UiRect::horizontal(px(14)),
            ..tile(5.0)
        },
        |p| {
            kit.glyph(p, "gear", 22.0, ink);
            display(p, "Develop industry", 16.0, 600, ink);
            display(p, "+2", 15.0, 700, ink);
            kit.glyph(p, "tools", 18.0, ink);
            p.spawn(Node {
                width: px(8),
                ..default()
            });
            display(
                p,
                if rich {
                    DEVELOP_COST.to_string()
                } else {
                    format!("{DEVELOP_COST}: not enough gold")
                },
                15.0,
                700,
                ink,
            );
            kit.glyph(
                p,
                "hud_gold",
                20.0,
                if rich {
                    Color::srgb(0.96, 0.78, 0.25)
                } else {
                    ink
                },
            );
        },
    );
    section(p, "Build");
    p.spawn(Node {
        display: Display::Grid,
        grid_template_columns: vec![RepeatedGridTrack::flex(2, 1.0)],
        column_gap: px(10),
        row_gap: px(10),
        ..default()
    })
    .with_children(|p| {
        let coastal = game.coastal(c.position);
        for (i, def) in session
            .rules
            .units
            .values()
            .filter(|d| d.id != "crate")
            .enumerate()
        {
            let chosen = c.production.as_deref() == Some(def.id.as_str());
            let wanted_coast = def.is_naval() && !coastal;
            let goods = crate::production::goods(game, &session.rules, c, def);
            card(p, kit, def, i, chosen, wanted_coast, goods);
        }
    });
}

/// A unit's picture at the given height; the width follows the picture.
fn sprite(p: &mut ChildSpawnerCommands, kit: &Kit, path: &str, height: f32) {
    p.spawn(Node {
        width: px(height * 1.25),
        height: px(height),
        flex_shrink: 0.0,
        align_items: AlignItems::Center,
        justify_content: JustifyContent::Center,
        overflow: Overflow::clip(),
        ..default()
    })
    .with_children(|p| {
        p.spawn((
            ImageNode::new(kit.image(path)),
            Node {
                height: px(height),
                ..default()
            },
        ));
    });
}

/// The key that picks the design at `index`: 1 to 9, then 0.
fn key_of(index: usize) -> Option<String> {
    match index {
        0..=8 => Some((index + 1).to_string()),
        9 => Some("0".to_string()),
        _ => None,
    }
}

/// One design on offer: its picture, name and cost, and the key that picks it.
fn card(
    p: &mut ChildSpawnerCommands,
    kit: &Kit,
    def: &fourx_sim::UnitDef,
    index: usize,
    chosen: bool,
    needs_coast: bool,
    goods: Option<(fourx_sim::Good, i32, bool)>,
) {
    let missing = goods.is_some_and(|(_, _, ready)| !ready);
    let paper = if needs_coast || missing {
        Paper::Faint
    } else if chosen {
        Paper::Chosen
    } else {
        Paper::Tile
    };
    let mut card = p.spawn((
        Button,
        paper,
        Node {
            height: px(62),
            align_items: AlignItems::Center,
            column_gap: px(8),
            padding: UiRect::axes(px(10), px(6)),
            ..tile(6.0)
        },
        BackgroundColor(paper.fill(Interaction::None)),
        BorderColor::all(if chosen { GILT } else { HAIR }),
    ));
    if !needs_coast && !missing && !chosen {
        card.insert(Action::Produce(def.id.clone()));
    }
    card.with_children(|p| {
        sprite(p, kit, &def.sprite, 50.0);
        p.spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(2),
            flex_shrink: 1.0,
            ..default()
        })
        .with_children(|p| {
            display(
                p,
                def.name.clone(),
                14.0,
                600,
                if needs_coast { INK_SOFT } else { INK },
            );
            p.spawn(Node {
                align_items: AlignItems::Center,
                column_gap: px(5),
                ..default()
            })
            .with_children(|p| {
                if needs_coast {
                    serif(p, "Needs a coast", 14.0, FAMINE);
                } else {
                    kit.glyph(p, "tools", 15.0, Color::srgb(0.36, 0.34, 0.33));
                    write(p, def.cost.to_string(), &SERIF, 16.0, 700, INK);
                    if chosen {
                        write(p, "BUILDING", &SERIF, 11.0, 800, RED);
                    }
                }
            });
            if let Some((g, n, ready)) = goods {
                serif(
                    p,
                    format!("{n} {}{}", g.name(), if ready { "" } else { " · missing" }),
                    12.0,
                    if ready { INK_SOFT } else { FAMINE },
                );
            }
        });
        if let Some(key) = key_of(index).filter(|_| !needs_coast && !missing) {
            p.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    top: px(5),
                    right: px(9),
                    ..default()
                },
                Text::new(key),
                TextFont {
                    font: SERIF,
                    font_size: 12.0,
                    weight: FontWeight(600),
                    ..default()
                },
                TextColor(INK_SOFT),
            ));
        }
    });
}
