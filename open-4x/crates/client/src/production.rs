//! The production window: a modal over the map for choosing what a city builds.
//!
//! The right-click menu only offers a way in ("Change production"); the list of designs, with
//! their pictures and costs, lives here so the menu stays short however many designs a pack
//! ships. The window is open while `Session::production` names a city. Choosing a design
//! orders it and closes the window; Escape or the close button leaves it unchanged.

use super::*;
use crate::chrome::{Frame, Skin, dress};
use crate::theme::{EDGE, INK, INK_SOFT, Tone, bold, caption, glyph, lift, rounded, rule, text};
use bevy::ui::FocusPolicy;

/// The scrolling list of designs.
#[derive(Component)]
pub(super) struct DesignList;

/// Scroll the list with the wheel. The window is a modal, so the pointer need not be over it.
pub(super) fn scroll(
    session: Res<Session>,
    mut wheel: MessageReader<MouseWheel>,
    mut list: Query<(&ComputedNode, &mut ScrollPosition), With<DesignList>>,
) {
    let delta: f32 = wheel.read().map(|e| -e.y * 30.0).sum();
    if delta == 0.0 || session.production.is_none() {
        return;
    }
    for (node, mut scroll) in &mut list {
        let max = (node.content_size().y - node.size().y) * node.inverse_scale_factor();
        scroll.y = (scroll.y + delta).clamp(0.0, max.max(0.0));
    }
}

/// The window over the map, for the city `id`.
pub(super) fn spawn(
    commands: &mut Commands,
    assets: &AssetServer,
    session: &Session,
    game: &Game,
    id: Id,
    narrow: bool,
) {
    let Some(city) = game.cities.get(&id) else {
        return;
    };
    let prefix = session.asset_prefix.as_str();
    let coastal = game.coastal(city.position);
    let building = city
        .production
        .as_ref()
        .and_then(|d| session.rules.units.get(d));
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
            BackgroundColor(Color::srgba(0.02, 0.03, 0.06, 0.60)),
            FocusPolicy::Block,
            GlobalZIndex(52),
            UiRoot,
        ))
        .with_children(|p| {
            p.spawn((
                Node {
                    width: if narrow { percent(94) } else { px(460) },
                    max_height: percent(88),
                    padding: UiRect::axes(px(26), px(22)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(8),
                    overflow: Overflow::clip(),
                    ..default()
                },
                BackgroundColor(crate::theme::CARD),
                BorderColor::all(EDGE),
                lift(),
                FocusPolicy::Block,
            ))
            .with_children(|p| {
                dress(p, assets, prefix, Skin::Paper, Frame::Grand, false);
                // Title row: the city and a way out.
                p.spawn(Node {
                    width: percent(100),
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    flex_shrink: 0.0,
                    ..default()
                })
                .with_children(|p| {
                    p.spawn(Node {
                        flex_direction: FlexDirection::Column,
                        ..default()
                    })
                    .with_children(|p| {
                        caption(p, "Change production");
                        bold(p, city.name.clone(), 20.0, INK);
                    });
                    p.spawn((
                        Button,
                        Action::CloseProduction,
                        Tone::Quiet,
                        Node {
                            width: px(34),
                            height: px(34),
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            border_radius: rounded(999.0),
                            flex_shrink: 0.0,
                            ..default()
                        },
                        BackgroundColor(Tone::Quiet.fill(Interaction::None)),
                    ))
                    .with_children(|p| glyph(p, assets, prefix, "cross", 14.0, Tone::Quiet.ink()));
                });
                text(
                    p,
                    match building {
                        Some(def) => {
                            format!("Building {} ({} of {})", def.name, city.progress, def.cost)
                        }
                        None => "Nothing is being built.".to_string(),
                    },
                    13.0,
                    INK_SOFT,
                );
                rule(p);
                // The designs.
                p.spawn((
                    Node {
                        width: percent(100),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(6),
                        overflow: Overflow::scroll_y(),
                        // Shrinks to the window's height when there are too many to show.
                        min_height: px(0),
                        flex_shrink: 1.0,
                        ..default()
                    },
                    DesignList,
                    ScrollPosition::default(),
                ))
                .with_children(|p| {
                    for def in session.rules.units.values() {
                        if def.is_naval() && !coastal {
                            continue;
                        }
                        let chosen = city.production.as_deref() == Some(def.id.as_str());
                        let tone = if chosen { Tone::Primary } else { Tone::Quiet };
                        p.spawn((
                            Button,
                            Action::ProduceAt {
                                city: id,
                                unit: def.id.clone(),
                            },
                            tone,
                            Node {
                                width: percent(100),
                                min_height: px(46),
                                flex_shrink: 0.0,
                                padding: UiRect::axes(px(12), px(6)),
                                column_gap: px(10),
                                border_radius: rounded(10.0),
                                align_items: AlignItems::Center,
                                ..default()
                            },
                            BackgroundColor(tone.fill(Interaction::None)),
                        ))
                        .with_children(|p| {
                            p.spawn(Node {
                                width: px(42),
                                height: px(34),
                                flex_shrink: 0.0,
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::Center,
                                overflow: Overflow::clip(),
                                ..default()
                            })
                            .with_children(|p| {
                                p.spawn((
                                    ImageNode::new(assets.load(format!("{prefix}{}", def.sprite))),
                                    Node {
                                        height: px(34),
                                        ..default()
                                    },
                                ));
                            });
                            p.spawn(Node {
                                flex_grow: 1.0,
                                ..default()
                            })
                            .with_children(|p| bold(p, def.name.clone(), 14.0, tone.ink()));
                            bold(
                                p,
                                if chosen {
                                    format!("{} · building", def.cost)
                                } else {
                                    def.cost.to_string()
                                },
                                13.0,
                                tone.ink(),
                            );
                        });
                    }
                });
                rule(p);
                p.spawn(Node {
                    width: percent(100),
                    flex_shrink: 0.0,
                    ..default()
                })
                .with_children(|p| {
                    crate::theme::button(p, "Cancel", Action::CloseProduction, Tone::Quiet)
                });
            });
        });
}
