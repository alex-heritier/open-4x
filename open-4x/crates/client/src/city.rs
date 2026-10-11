//! The city view: a window over the map that shows what a city gathers and what it builds.
//!
//! A light card over a dimmed map, in the interface's own language (see `theme`): readout tiles
//! with flat glyphs for the economy (people, food, shields, gold, industry, land) in the left
//! column, and production in the right, with a card for every design the pack offers. Foreign
//! cities show the left column only.
use super::*;
use crate::presentation::{city_sprite, nation_color};
use crate::theme::{
    ACCENT, BAD, CARD, EDGE, GOOD, INK, INK_SOFT, Tone, WELL, bold, button, caption, glyph, lift,
    meter, nation_flag, rounded, rule, text,
};
use bevy::ui::FocusPolicy;
use fourx_sim::City;

/// Cost of one step of industrial development, in gold (see `Command::Develop`).
const DEVELOP_COST: i32 = 40;

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
        caption(p, label);
        p.spawn((
            Node {
                flex_grow: 1.0,
                height: px(1),
                ..default()
            },
            BackgroundColor(crate::theme::RULE),
        ));
    });
}

/// A small pill: a glyph and a few words.
fn chip(p: &mut ChildSpawnerCommands, kit: &Kit, name: &str, words: impl Into<String>) {
    p.spawn((
        Node {
            align_items: AlignItems::Center,
            column_gap: px(6),
            padding: UiRect::axes(px(10), px(4)),
            border_radius: rounded(999.0),
            ..default()
        },
        BackgroundColor(WELL),
    ))
    .with_children(|p| {
        kit.glyph(p, name, 15.0, INK_SOFT);
        bold(p, words, 12.0, INK);
    });
}

/// One readout: a glyph, a figure, what it counts, and optionally how it adds up.
fn readout(
    p: &mut ChildSpawnerCommands,
    kit: &Kit,
    name: &str,
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
            column_gap: px(12),
            padding: UiRect::axes(px(14), px(10)),
            border_radius: rounded(14.0),
            ..default()
        },
        BackgroundColor(WELL),
    ))
    .with_children(|p| {
        kit.glyph(p, name, 28.0, ACCENT);
        p.spawn(Node {
            flex_direction: FlexDirection::Column,
            ..default()
        })
        .with_children(|p| {
            bold(p, figure, 21.0, colour);
            caption(p, what);
            if let Some(detail) = detail {
                text(p, detail, 11.5, INK_SOFT);
            }
        });
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

/// A figure on the left and what it counts on the right, over a meter.
fn gauge(p: &mut ChildSpawnerCommands, label: &str, figure: String, fraction: f32, tint: Color) {
    p.spawn(Node {
        justify_content: JustifyContent::SpaceBetween,
        align_items: AlignItems::Center,
        flex_shrink: 0.0,
        ..default()
    })
    .with_children(|p| {
        caption(p, label);
        bold(p, figure, 12.5, INK);
    });
    meter(p, fraction, tint, 10.0);
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
            BackgroundColor(Color::srgba(0.03, 0.06, 0.10, 0.55)),
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
                    padding: UiRect::axes(px(if narrow { 18 } else { 28 }), px(24)),
                    border: UiRect::all(px(1)),
                    border_radius: rounded(20.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(16),
                    ..default()
                },
                BackgroundColor(CARD),
                BorderColor::all(EDGE),
                lift(),
                FocusPolicy::Block,
            ))
            .with_children(|p| {
                // Everything but the footer scrolls when the window is short.
                p.spawn((
                    Node {
                        flex_shrink: 1.0,
                        min_height: px(0),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(16),
                        overflow: Overflow::scroll_y(),
                        ..default()
                    },
                    ScrollPosition::default(),
                    Scrolled,
                ))
                .with_children(|p| {
                    header(p, &kit, game, session, c, narrow);
                    rule(p);
                    p.spawn(Node {
                        flex_direction: if narrow {
                            FlexDirection::Column
                        } else {
                            FlexDirection::Row
                        },
                        column_gap: px(28),
                        row_gap: px(18),
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
                            INK_SOFT,
                        );
                    });
                    p.spawn(Node {
                        column_gap: px(8),
                        ..default()
                    })
                    .with_children(|p| {
                        if !mine && let Some(faction) = game.factions.get(&c.owner) {
                            let leader = faction.leader.trim();
                            button(
                                p,
                                if leader.is_empty() {
                                    "Talk".to_string()
                                } else {
                                    format!("Talk to {leader}")
                                },
                                Action::Talk(c.owner),
                                Tone::Quiet,
                            );
                        }
                        button(p, "Close", Action::CloseCity, Tone::Quiet);
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
                border_radius: rounded(16.0),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(Color::srgb(0.22, 0.38, 0.42)),
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
            row_gap: px(6),
            ..default()
        })
        .with_children(|p| {
            p.spawn(Node {
                align_items: AlignItems::Center,
                column_gap: px(8),
                ..default()
            })
            .with_children(|p| {
                nation_flag(p, nation_color(game, c.owner, 1.0), 20.0);
                caption(p, nation.map_or("Unknown", |f| f.name.as_str()));
            });
            bold(p, &c.name, if narrow { 26.0 } else { 32.0 }, INK);
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
            Tone::Quiet,
            Node {
                width: px(38),
                height: px(38),
                flex_shrink: 0.0,
                align_self: AlignSelf::FlexStart,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border_radius: rounded(999.0),
                ..default()
            },
            BackgroundColor(Tone::Quiet.fill(Interaction::None)),
        ))
        .with_children(|p| kit.glyph(p, "cross", 16.0, INK));
    });
}

/// The left column: what the land gives and what the people eat.
fn economy(p: &mut ChildSpawnerCommands, kit: &Kit, game: &Game, c: &City) {
    section(p, "Economy");
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
            INK,
            "Citizens",
            Some(format!("{} eat {} food", c.population, c.food_eaten())),
        );
        readout(
            p,
            kit,
            "wheat",
            format!("{surplus:+}"),
            if surplus < 0 { BAD } else { INK },
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
            INK,
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
            INK,
            "Gold a day",
            None,
        );
        readout(
            p,
            kit,
            "hud_industry",
            c.industry.to_string(),
            INK,
            "Industry",
            None,
        );
        readout(
            p,
            kit,
            "globe",
            game.territory_of(c.id).len().to_string(),
            INK,
            "Squares held",
            Some(format!("border level {}", c.border)),
        );
    });
    // The granary: a citizen is born when it fills, and one starves when it runs dry.
    let size = c.granary_size();
    p.spawn(Node {
        margin: UiRect::top(px(6)),
        flex_direction: FlexDirection::Column,
        row_gap: px(6),
        ..default()
    })
    .with_children(|p| {
        gauge(
            p,
            "Granary",
            format!("{} / {}", c.granary, size),
            c.granary as f32 / size.max(1) as f32,
            if surplus < 0 { BAD } else { GOOD },
        );
    });
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
        if surplus < 0 { BAD } else { INK_SOFT },
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
    section(p, "Production");
    let building = c
        .production
        .as_ref()
        .and_then(|id| session.rules.units.get(id));
    // The current work, with how far along it is.
    p.spawn((
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: px(8),
            padding: UiRect::axes(px(14), px(12)),
            border_radius: rounded(14.0),
            flex_shrink: 0.0,
            ..default()
        },
        BackgroundColor(WELL),
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
                    bold(p, def.name.clone(), 16.0, INK);
                    let left = days(def.cost - c.progress, c.shields());
                    text(
                        p,
                        match left {
                            Some(n) => format!("Ready in {}", in_days(n.max(1))),
                            None => "Nothing is gathered to build it".to_string(),
                        },
                        11.5,
                        if left.is_some() { INK_SOFT } else { BAD },
                    );
                });
            });
            gauge(
                p,
                "Shields",
                format!("{} / {}", c.progress, def.cost),
                c.progress as f32 / def.cost.max(1) as f32,
                ACCENT,
            );
        }
        None => {
            bold(p, "Idle", 16.0, INK);
            text(
                p,
                if mine {
                    "Nothing is being built. Choose a design below."
                } else {
                    "Nothing is being built."
                },
                11.5,
                INK_SOFT,
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
    let tone = if rich { Tone::Primary } else { Tone::Dormant };
    let mut develop = p.spawn((
        Button,
        tone,
        Node {
            width: percent(100),
            height: px(44),
            flex_shrink: 0.0,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            column_gap: px(8),
            padding: UiRect::horizontal(px(14)),
            border_radius: rounded(12.0),
            ..default()
        },
        BackgroundColor(tone.fill(Interaction::None)),
    ));
    if rich {
        develop.insert(Action::Develop);
    }
    develop.with_children(|p| {
        kit.glyph(p, "hud_industry", 20.0, tone.ink());
        bold(p, "Develop industry  +2", 13.5, tone.ink());
        kit.glyph(p, "hud_gold", 18.0, tone.ink());
        bold(
            p,
            if rich {
                DEVELOP_COST.to_string()
            } else {
                format!("{DEVELOP_COST}: not enough gold")
            },
            13.5,
            tone.ink(),
        );
    });
    section(p, "Build");
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
    let tone = if needs_coast {
        Tone::Dormant
    } else if chosen {
        Tone::Chosen
    } else {
        Tone::Quiet
    };
    let mut card = p.spawn((
        Button,
        tone,
        Node {
            flex_basis: percent(46.0),
            flex_grow: 1.0,
            height: px(70),
            align_items: AlignItems::Center,
            column_gap: px(8),
            padding: UiRect::axes(px(12), px(6)),
            border: UiRect::all(px(if chosen { 2 } else { 0 })),
            border_radius: rounded(14.0),
            ..default()
        },
        BackgroundColor(tone.fill(Interaction::None)),
        BorderColor::all(ACCENT),
    ));
    if !needs_coast {
        card.insert(Action::Produce(def.id.clone()));
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
            bold(
                p,
                def.name.clone(),
                13.0,
                if needs_coast { INK_SOFT } else { INK },
            );
            p.spawn(Node {
                align_items: AlignItems::Center,
                column_gap: px(5),
                ..default()
            })
            .with_children(|p| {
                if needs_coast {
                    text(p, "Needs a coast", 11.5, BAD);
                } else {
                    kit.glyph(p, "tools", 14.0, INK_SOFT);
                    bold(p, def.cost.to_string(), 13.0, INK);
                    if chosen {
                        bold(p, "BUILDING", 9.5, ACCENT);
                    }
                }
            });
        });
        if index < 6 && !needs_coast {
            p.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    top: px(6),
                    right: px(10),
                    ..default()
                },
                Text::new((index + 1).to_string()),
                TextFont {
                    font_size: 11.0,
                    weight: bevy::text::FontWeight::SEMIBOLD,
                    ..default()
                },
                TextColor(INK_SOFT),
            ));
        }
    });
}
