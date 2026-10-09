//! The city view: a window over the map that shows what a city gathers and what it builds.
//!
//! It is dressed from the pack's UI kit rather than flat colours: a nine-sliced navy frame with
//! brass corners, engraved icons for the readouts, brass buttons, dark cards for what can be
//! built, and the kit's meter for the granary and for production. The left column is the
//! economy (people, food, shields, gold, industry, land); the right column is production, with
//! a card for every design the pack offers. Foreign cities show the left column only.
use super::*;
use crate::presentation::{GOLD, GOLD_LINE, INK, PAPER, city_sprite, nation_color, rounded, text};
use bevy::sprite::{BorderRect, SliceScaleMode, TextureSlicer};
use bevy::ui::{FocusPolicy, widget::NodeImageMode};
use fourx_sim::City;

/// Cost of one step of industrial development, in gold (see `Command::Develop`).
const DEVELOP_COST: i32 = 40;
/// What a control that cannot be used right now looks like.
const DIM: Color = Color::srgb(0.5, 0.5, 0.5);
/// Secondary text on the dark frame.
const MUTED: Color = Color::srgba(0.89, 0.85, 0.74, 0.62);
/// A recessed field on the frame: readout tiles, the portrait, chips.
const WELL: Color = Color::srgba(0.01, 0.05, 0.07, 0.55);
/// Food that is eaten faster than it is grown, shields at a standstill: bad news.
const WARN: Color = Color::srgb(0.94, 0.45, 0.38);

/// Hover and press feedback for a control that wears a picture: the picture is tinted.
#[derive(Component)]
pub(super) struct Skin {
    rest: Color,
    hover: Color,
    pressed: Color,
}
impl Skin {
    /// The usual look: slightly quiet at rest, full colour under the pointer, darker pressed.
    fn lit() -> Self {
        Self {
            rest: Color::srgb(0.9, 0.9, 0.9),
            hover: Color::WHITE,
            pressed: Color::srgb(0.7, 0.7, 0.7),
        }
    }
    /// The card of what the city is building: always warm.
    fn chosen() -> Self {
        Self {
            rest: Color::srgb(1.0, 0.9, 0.62),
            hover: Color::srgb(1.0, 0.96, 0.78),
            pressed: Color::srgb(0.8, 0.7, 0.45),
        }
    }
}

/// Tint the pictures of the city view's controls as the pointer moves over them.
pub(super) fn update(
    mut changed: Query<(&Interaction, &Skin, &mut ImageNode), Changed<Interaction>>,
) {
    for (interaction, skin, mut image) in &mut changed {
        image.color = match interaction {
            Interaction::Pressed => skin.pressed,
            Interaction::Hovered => skin.hover,
            Interaction::None => skin.rest,
        };
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

/// An image cut in nine, so that its corners keep their shape at any size. `border` is the
/// inset in the picture's own pixels (see `ui/nine_slice.json`).
fn sliced(image: Handle<Image>, border: f32) -> ImageNode {
    ImageNode::new(image).with_mode(NodeImageMode::Sliced(TextureSlicer {
        border: BorderRect::all(border),
        center_scale_mode: SliceScaleMode::Stretch,
        sides_scale_mode: SliceScaleMode::Stretch,
        max_corner_scale: 1.0,
    }))
}

/// The pictures the view is made of.
struct Kit<'a> {
    assets: &'a AssetServer,
    prefix: &'a str,
}
impl Kit<'_> {
    fn ui(&self, name: &str) -> Handle<Image> {
        self.assets.load(format!("{}ui/{name}.png", self.prefix))
    }
    fn icon(&self, name: &str) -> Handle<Image> {
        // `hud_*` are the top-bar badges; everything else is an engraved `icon_*`.
        let file = if name.starts_with("hud_") {
            name.to_string()
        } else {
            format!("icon_{name}")
        };
        self.assets
            .load(format!("{}ui/icons/{file}.png", self.prefix))
    }
    /// A glyph of the given side.
    fn glyph(&self, p: &mut ChildSpawnerCommands, name: &str, side: f32) {
        p.spawn((
            ImageNode::new(self.icon(name)),
            Node {
                width: px(side),
                height: px(side),
                flex_shrink: 0.0,
                ..default()
            },
        ));
    }
}

/// A section label with its hairline.
fn section(p: &mut ChildSpawnerCommands, label: &str) {
    p.spawn(Node {
        align_items: AlignItems::Center,
        column_gap: px(10),
        flex_shrink: 0.0,
        ..default()
    })
    .with_children(|p| {
        text(p, label, 12.0, GOLD);
        p.spawn((
            Node {
                flex_grow: 1.0,
                height: px(1),
                ..default()
            },
            BackgroundColor(GOLD_LINE),
        ));
    });
}

/// A small pill: a glyph and a few words.
fn chip(p: &mut ChildSpawnerCommands, kit: &Kit, glyph: &str, words: impl Into<String>) {
    p.spawn((
        Node {
            align_items: AlignItems::Center,
            column_gap: px(5),
            padding: UiRect::axes(px(9), px(3)),
            border: UiRect::all(px(1)),
            border_radius: rounded(11.0),
            ..default()
        },
        BackgroundColor(WELL),
        BorderColor::all(GOLD_LINE),
    ))
    .with_children(|p| {
        kit.glyph(p, glyph, 15.0);
        text(p, words, 11.5, PAPER);
    });
}

/// One readout: a glyph, a figure, what it counts, and optionally how it adds up.
fn readout(
    p: &mut ChildSpawnerCommands,
    kit: &Kit,
    glyph: &str,
    figure: String,
    colour: Color,
    what: &str,
    detail: Option<String>,
) {
    p.spawn((
        Node {
            flex_basis: percent(46.0),
            flex_grow: 1.0,
            align_items: AlignItems::Center,
            column_gap: px(10),
            padding: UiRect::axes(px(11), px(8)),
            border: UiRect::all(px(1)),
            border_radius: rounded(8.0),
            ..default()
        },
        BackgroundColor(WELL),
        BorderColor::all(GOLD_LINE),
    ))
    .with_children(|p| {
        kit.glyph(p, glyph, 32.0);
        p.spawn(Node {
            flex_direction: FlexDirection::Column,
            ..default()
        })
        .with_children(|p| {
            text(p, figure, 22.0, colour);
            text(p, what.to_uppercase(), 9.5, GOLD);
            if let Some(detail) = detail {
                text(p, detail, 10.5, MUTED);
            }
        });
    });
}

/// A meter in the kit's frame, `fraction` full.
fn meter(p: &mut ChildSpawnerCommands, kit: &Kit, fraction: f32, tint: Color, height: f32) {
    p.spawn((
        Node {
            width: percent(100),
            height: px(height),
            padding: UiRect::all(px(3)),
            flex_shrink: 0.0,
            ..default()
        },
        sliced(kit.ui("bar_frame"), 14.0),
    ))
    .with_children(|p| {
        p.spawn((
            Node {
                width: percent(fraction.clamp(0.0, 1.0) * 100.0),
                height: percent(100),
                border_radius: rounded(3.0),
                ..default()
            },
            ImageNode::new(kit.ui("bar_fill_green")).with_color(tint),
        ));
    });
}

/// Whole days until `needed` is gathered at `per_day`, or `None` if it never will be.
fn days(needed: i32, per_day: i32) -> Option<i32> {
    (per_day > 0).then(|| (needed.max(0) + per_day - 1) / per_day)
}
fn in_days(n: i32) -> String {
    if n == 1 {
        "1 day".into()
    } else {
        format!("{n} days")
    }
}

/// A brass button: `face` over the kit's button, `width` wide.
fn brass(
    p: &mut ChildSpawnerCommands,
    kit: &Kit,
    width: Val,
    action: Option<Action>,
    content: impl FnOnce(&mut ChildSpawnerCommands),
) {
    let mut button = p.spawn((
        Button,
        Node {
            width,
            height: px(44),
            flex_shrink: 0.0,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            column_gap: px(8),
            padding: UiRect::horizontal(px(14)),
            ..default()
        },
        sliced(kit.ui("button_brass"), 28.0).with_color(if action.is_some() {
            Skin::lit().rest
        } else {
            DIM
        }),
    ));
    if let Some(action) = action {
        button.insert((action, Skin::lit()));
    }
    button.with_children(content);
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
            BackgroundColor(Color::srgba(0.01, 0.05, 0.07, 0.68)),
            FocusPolicy::Block,
            GlobalZIndex(50),
            UiRoot,
        ))
        .with_children(|p| {
            p.spawn((
                Node {
                    width: if narrow { percent(96) } else { px(900) },
                    max_width: percent(96),
                    max_height: percent(92),
                    padding: UiRect::axes(px(if narrow { 24 } else { 34 }), px(28)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(14),
                    ..default()
                },
                sliced(kit.ui("panel_navy"), 44.0),
                FocusPolicy::Block,
            ))
            .with_children(|p| {
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
                    p.spawn((
                        Node {
                            width: percent(100),
                            height: px(1),
                            flex_shrink: 0.0,
                            ..default()
                        },
                        BackgroundColor(GOLD_LINE),
                    ));
                    p.spawn(Node {
                        flex_direction: if narrow {
                            FlexDirection::Column
                        } else {
                            FlexDirection::Row
                        },
                        column_gap: px(24),
                        row_gap: px(16),
                        align_items: AlignItems::FlexStart,
                        flex_shrink: 0.0,
                        ..default()
                    })
                    .with_children(|body| {
                        body.spawn(Node {
                            width: if narrow { percent(100) } else { px(372) },
                            flex_shrink: 0.0,
                            flex_direction: FlexDirection::Column,
                            row_gap: px(10),
                            ..default()
                        })
                        .with_children(|p| economy(p, &kit, game, c));
                        body.spawn(Node {
                            width: if narrow { percent(100) } else { auto() },
                            flex_grow: 1.0,
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
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    column_gap: px(16),
                    flex_shrink: 0.0,
                    ..default()
                })
                .with_children(|p| {
                    p.spawn(Node {
                        flex_shrink: 1.0,
                        ..default()
                    })
                    .with_children(|p| {
                        text(
                            p,
                            "Its border never changes: whoever holds the city holds the land.",
                            11.5,
                            MUTED,
                        );
                    });
                    brass(p, &kit, px(150), Some(Action::CloseCity), |p| {
                        text(p, "CLOSE", 14.0, INK);
                    });
                });
            });
        });
}

/// The portrait, the names, and the way out.
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
        column_gap: px(18),
        flex_shrink: 0.0,
        ..default()
    })
    .with_children(|h| {
        // The city as the map draws it, in a little window of its own.
        let (width, height) = if narrow { (84.0, 74.0) } else { (132.0, 116.0) };
        h.spawn((
            Node {
                width: px(width),
                height: px(height),
                flex_shrink: 0.0,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border: UiRect::all(px(1)),
                border_radius: rounded(10.0),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(WELL),
            BorderColor::all(GOLD),
        ))
        .with_children(|p| {
            let sprite = city_sprite(&session.pack.visuals, game, c.owner);
            p.spawn((
                ImageNode::new(kit.assets.load(format!("{}{sprite}", kit.prefix))),
                Node {
                    height: percent(100),
                    ..default()
                },
            ));
        });
        h.spawn(Node {
            flex_grow: 1.0,
            flex_basis: px(0),
            min_width: px(0),
            flex_direction: FlexDirection::Column,
            row_gap: px(5),
            ..default()
        })
        .with_children(|p| {
            p.spawn(Node {
                align_items: AlignItems::Center,
                column_gap: px(8),
                ..default()
            })
            .with_children(|p| {
                p.spawn((
                    Node {
                        width: px(11),
                        height: px(11),
                        border: UiRect::all(px(1)),
                        border_radius: rounded(6.0),
                        ..default()
                    },
                    BackgroundColor(nation_color(game, c.owner, 1.0)),
                    BorderColor::all(PAPER.with_alpha(0.8)),
                ));
                text(
                    p,
                    nation.map_or("Unknown".to_string(), |f| f.name.to_uppercase()),
                    11.5,
                    GOLD,
                );
            });
            text(p, &c.name, if narrow { 26.0 } else { 34.0 }, PAPER);
            p.spawn(Node {
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(8),
                row_gap: px(6),
                ..default()
            })
            .with_children(|p| {
                if c.capital {
                    chip(p, kit, "star", "Capital");
                }
                chip(p, kit, "hud_people", format!("Population {}", c.population));
                chip(p, kit, "globe", format!("Border level {}", c.border));
            });
        });
        h.spawn((
            Button,
            Action::CloseCity,
            Skin::lit(),
            Node {
                width: px(40),
                height: px(40),
                flex_shrink: 0.0,
                align_self: AlignSelf::FlexStart,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            sliced(kit.ui("button_brass"), 28.0).with_color(Skin::lit().rest),
        ))
        .with_children(|p| kit.glyph(p, "cross", 24.0));
    });
}

/// The left column: what the land gives and what the people eat.
fn economy(p: &mut ChildSpawnerCommands, kit: &Kit, game: &Game, c: &City) {
    section(p, "ECONOMY");
    let surplus = c.food_surplus();
    p.spawn(Node {
        flex_wrap: FlexWrap::Wrap,
        column_gap: px(8),
        row_gap: px(8),
        ..default()
    })
    .with_children(|p| {
        readout(
            p,
            kit,
            "hud_people",
            c.population.to_string(),
            PAPER,
            "Citizens",
            Some(format!("{} eat {} food", c.population, c.food_eaten())),
        );
        readout(
            p,
            kit,
            "wheat",
            format!("{surplus:+}"),
            if surplus < 0 { WARN } else { PAPER },
            "Food a day",
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
            c.shields().to_string(),
            PAPER,
            "Shields a day",
            Some(format!(
                "{} land + {} industry",
                c.harvest.shields, c.industry
            )),
        );
        readout(
            p,
            kit,
            "hud_gold",
            format!("{:+}", c.harvest.gold),
            PAPER,
            "Gold a day",
            None,
        );
        readout(
            p,
            kit,
            "hud_industry",
            c.industry.to_string(),
            PAPER,
            "Industry",
            None,
        );
        readout(
            p,
            kit,
            "globe",
            game.territory_of(c.id).len().to_string(),
            PAPER,
            "Squares held",
            Some(format!("border level {}", c.border)),
        );
    });
    // The granary: a citizen is born when it fills, and one starves when it runs dry.
    let size = c.granary_size();
    p.spawn(Node {
        justify_content: JustifyContent::SpaceBetween,
        align_items: AlignItems::Center,
        margin: UiRect::top(px(4)),
        ..default()
    })
    .with_children(|p| {
        text(p, "GRANARY", 9.5, GOLD);
        text(p, format!("{} / {}", c.granary, size), 12.0, PAPER);
    });
    meter(
        p,
        kit,
        c.granary as f32 / size.max(1) as f32,
        if surplus < 0 {
            Color::srgb(1.0, 0.45, 0.4)
        } else {
            Color::WHITE
        },
        22.0,
    );
    text(
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
        11.5,
        if surplus < 0 { WARN } else { MUTED },
    );
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
    section(p, "PRODUCTION");
    let building = c
        .production
        .as_ref()
        .and_then(|id| session.rules.units.get(id));
    // The current work, with how far along it is.
    p.spawn((
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: px(7),
            padding: UiRect::axes(px(12), px(10)),
            border: UiRect::all(px(1)),
            border_radius: rounded(8.0),
            flex_shrink: 0.0,
            ..default()
        },
        BackgroundColor(WELL),
        BorderColor::all(GOLD_LINE),
    ))
    .with_children(|p| match building {
        Some(def) => {
            p.spawn(Node {
                align_items: AlignItems::Center,
                column_gap: px(12),
                ..default()
            })
            .with_children(|p| {
                sprite(p, kit, &def.sprite, 46.0);
                p.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    ..default()
                })
                .with_children(|p| {
                    text(p, def.name.clone(), 17.0, PAPER);
                    let left = days(def.cost - c.progress, c.shields());
                    text(
                        p,
                        match left {
                            Some(n) => format!("Ready in {}", in_days(n.max(1))),
                            None => "Nothing is gathered to build it".to_string(),
                        },
                        11.5,
                        if left.is_some() { MUTED } else { WARN },
                    );
                });
            });
            p.spawn(Node {
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            })
            .with_children(|p| {
                text(p, "SHIELDS", 9.5, GOLD);
                text(p, format!("{} / {}", c.progress, def.cost), 12.0, PAPER);
            });
            meter(
                p,
                kit,
                c.progress as f32 / def.cost.max(1) as f32,
                Color::srgb(1.0, 0.82, 0.45),
                20.0,
            );
        }
        None => {
            text(p, "Idle", 17.0, PAPER);
            text(
                p,
                if mine {
                    "Nothing is being built. Choose a design below."
                } else {
                    "Nothing is being built."
                },
                11.5,
                MUTED,
            );
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
    brass(p, kit, percent(100), rich.then_some(Action::Develop), |p| {
        kit.glyph(p, "hud_industry", 24.0);
        text(p, "DEVELOP INDUSTRY  +2", 13.5, INK);
        kit.glyph(p, "hud_gold", 22.0);
        text(
            p,
            if rich {
                DEVELOP_COST.to_string()
            } else {
                format!("{DEVELOP_COST}: not enough gold")
            },
            13.5,
            INK,
        );
    });
    section(p, "BUILD");
    p.spawn(Node {
        flex_wrap: FlexWrap::Wrap,
        column_gap: px(8),
        row_gap: px(8),
        ..default()
    })
    .with_children(|p| {
        let coastal = game.coastal(c.position);
        for (i, def) in session.rules.units.values().enumerate() {
            let chosen = c.production.as_deref() == Some(def.id.as_str());
            let wanted_coast = def.is_naval() && !coastal;
            card(p, kit, def, i, chosen, wanted_coast);
        }
    });
}

/// A unit's picture at the given height; the width follows the picture.
fn sprite(p: &mut ChildSpawnerCommands, kit: &Kit, path: &str, height: f32) {
    p.spawn(Node {
        width: px(height * 1.3),
        height: px(height),
        flex_shrink: 0.0,
        align_items: AlignItems::Center,
        justify_content: JustifyContent::Center,
        overflow: Overflow::clip(),
        ..default()
    })
    .with_children(|p| {
        p.spawn((
            ImageNode::new(kit.assets.load(format!("{}{path}", kit.prefix))),
            Node {
                height: px(height),
                ..default()
            },
        ));
    });
}

/// One design on offer: its picture, name and cost, and the key that picks it.
fn card(
    p: &mut ChildSpawnerCommands,
    kit: &Kit,
    def: &fourx_sim::UnitDef,
    index: usize,
    chosen: bool,
    needs_coast: bool,
) {
    let skin = if chosen { Skin::chosen() } else { Skin::lit() };
    let tint = if needs_coast { DIM } else { skin.rest };
    let mut card = p.spawn((
        Button,
        Node {
            flex_basis: percent(46.0),
            flex_grow: 1.0,
            height: px(70),
            align_items: AlignItems::Center,
            column_gap: px(8),
            padding: UiRect::axes(px(12), px(6)),
            border: UiRect::all(px(if chosen { 2 } else { 0 })),
            border_radius: rounded(9.0),
            ..default()
        },
        BorderColor::all(GOLD),
        sliced(kit.ui("button_dark"), 36.0).with_color(tint),
    ));
    if !needs_coast {
        card.insert((Action::Produce(def.id.clone()), skin));
    }
    card.with_children(|p| {
        sprite(p, kit, &def.sprite, 44.0);
        p.spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(2),
            flex_shrink: 1.0,
            ..default()
        })
        .with_children(|p| {
            text(
                p,
                def.name.clone(),
                13.0,
                if needs_coast { MUTED } else { PAPER },
            );
            p.spawn(Node {
                align_items: AlignItems::Center,
                column_gap: px(4),
                ..default()
            })
            .with_children(|p| {
                if needs_coast {
                    text(p, "Needs a coast", 11.5, WARN);
                } else {
                    kit.glyph(p, "tools", 15.0);
                    text(p, def.cost.to_string(), 13.0, GOLD);
                    if chosen {
                        text(p, "  BUILDING", 9.5, GOLD);
                    }
                }
            });
        });
        if index < 6 && !needs_coast {
            p.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    top: px(5),
                    right: px(7),
                    padding: UiRect::axes(px(4), px(0)),
                    border_radius: rounded(3.0),
                    ..default()
                },
                BackgroundColor(INK.with_alpha(0.75)),
                Text::new((index + 1).to_string()),
                TextFont {
                    font_size: 10.0,
                    ..default()
                },
                TextColor(PAPER),
            ));
        }
    });
}
