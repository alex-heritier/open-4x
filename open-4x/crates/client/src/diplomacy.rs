//! The diplomacy window: another nation's leader across a table, in the manner of Civ3's
//! leader screen.
//!
//! A dark window over the dimmed map. On the left the leader's portrait (the nation's flag in the
//! pack, which is also its leader art) in a gilt frame, with the leader's name and nation under
//! it; on the right what the leader says, what is known of the nation, and the ways to answer.
//! The game has no treaties yet, so the only answer that does anything is to take one's leave;
//! the others are drawn and dimmed, so the window already has the shape the treaties will fill.
//! Music plays under it for as long as it is open (see [`music`]).
use super::*;
use crate::presentation::{nation_art, nation_color};
use crate::theme::{
    GAIN, LOSS, ON_PLATE, TRIM, Tone, bold, button, framed_flag, glyph, lift, rounded, text,
};
use bevy::ui::FocusPolicy;

/// The window's surface and the lighter wells set into it.
const PANEL: Color = Color::srgb(0.071, 0.098, 0.145);
const INSET: Color = Color::srgb(0.043, 0.063, 0.100);
/// Words on the dark surface that are not the main line.
const MUTED: Color = Color::srgba(0.965, 0.957, 0.925, 0.62);

/// Marks the music under the window, so that [`music`] can stop it.
#[derive(Component)]
pub(super) struct Music;

/// Play the pack's diplomacy music in a loop while the window is open, and stop it when it closes.
pub(super) fn music(
    session: Res<Session>,
    assets: Res<AssetServer>,
    mut commands: Commands,
    playing: Query<Entity, With<Music>>,
) {
    let wanted = session.diplomacy.is_some() && !session.pack.visuals.diplomacy_music.is_empty();
    match (wanted, playing.single()) {
        (true, Err(_)) => {
            commands.spawn((
                AudioPlayer::new(assets.load(format!(
                    "{}{}",
                    session.asset_prefix, session.pack.visuals.diplomacy_music
                ))),
                PlaybackSettings::LOOP,
                Music,
            ));
        }
        (false, Ok(entity)) => commands.entity(entity).despawn(),
        _ => {}
    }
}

/// What the leader says on opening.
fn greeting(game: &Game, me: Id, them: Id) -> String {
    let faction = &game.factions[&them];
    let leader = leader_name(faction);
    if game.at_war(me, them) {
        format!(
            "{leader}: So you come to speak while our armies are in the field. \
             Say what you have to say, and be quick."
        )
    } else {
        format!(
            "{leader}: Welcome. The {} people have long wished to know the mind of your nation. \
             What brings you to me?",
            faction.adjective
        )
    }
}

fn leader_name(faction: &fourx_sim::Faction) -> String {
    let name = faction.leader.trim();
    if name.is_empty() {
        format!("The {} leader", faction.adjective)
    } else {
        name.to_string()
    }
}

/// The window over the map. `narrow` is a phone-shaped screen, where the two columns stack.
pub(super) fn spawn(
    commands: &mut Commands,
    assets: &AssetServer,
    session: &Session,
    game: &Game,
    them: Id,
    narrow: bool,
) {
    let Some(faction) = game.factions.get(&them) else {
        return;
    };
    let me = if session.player == 0 {
        game.commander
    } else {
        session.player
    };
    let prefix = session.asset_prefix.as_str();
    let at_war = game.at_war(me, them);
    let cities = game.cities.values().filter(|c| c.owner == them).count();
    let (portrait_w, portrait_h) = if narrow {
        (122.0, 146.0)
    } else {
        (210.0, 250.0)
    };
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
            BackgroundColor(Color::srgba(0.02, 0.03, 0.06, 0.72)),
            FocusPolicy::Block,
            GlobalZIndex(55),
            UiRoot,
        ))
        .with_children(|p| {
            p.spawn((
                Node {
                    width: if narrow { percent(96) } else { px(860) },
                    max_height: percent(94),
                    padding: UiRect::all(px(if narrow { 14 } else { 22 })),
                    border: UiRect::all(px(2)),
                    border_radius: rounded(18.0),
                    flex_direction: if narrow {
                        FlexDirection::Column
                    } else {
                        FlexDirection::Row
                    },
                    column_gap: px(26),
                    row_gap: px(14),
                    overflow: Overflow::clip(),
                    ..default()
                },
                BackgroundColor(PANEL),
                BorderColor::all(TRIM.with_alpha(0.75)),
                lift(),
                FocusPolicy::Block,
            ))
            .with_children(|p| {
                // The leader.
                p.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: px(10),
                    flex_shrink: 0.0,
                    ..default()
                })
                .with_children(|p| {
                    framed_flag(
                        p,
                        nation_color(game, them, 1.0),
                        &faction.tag,
                        nation_art(session, assets, &faction.tag),
                        portrait_w,
                        portrait_h,
                    );
                    bold(
                        p,
                        leader_name(faction),
                        if narrow { 15.0 } else { 17.0 },
                        ON_PLATE,
                    );
                    text(p, faction.name.clone(), 13.0, MUTED);
                    // How things stand between the two nations.
                    p.spawn((
                        Node {
                            padding: UiRect::axes(px(12), px(4)),
                            border_radius: rounded(999.0),
                            ..default()
                        },
                        BackgroundColor(INSET),
                    ))
                    .with_children(|p| {
                        bold(
                            p,
                            if at_war { "At war" } else { "At peace" },
                            12.0,
                            if at_war { LOSS } else { GAIN },
                        );
                    });
                });
                // The talk.
                p.spawn(Node {
                    flex_grow: 1.0,
                    flex_basis: px(0),
                    min_width: px(0),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(14),
                    ..default()
                })
                .with_children(|p| {
                    p.spawn(Node {
                        justify_content: JustifyContent::SpaceBetween,
                        align_items: AlignItems::Center,
                        ..default()
                    })
                    .with_children(|p| {
                        p.spawn(Node {
                            flex_direction: FlexDirection::Column,
                            ..default()
                        })
                        .with_children(|p| {
                            bold(
                                p,
                                faction.name.clone(),
                                if narrow { 20.0 } else { 24.0 },
                                ON_PLATE,
                            );
                            caption_dark(p, "Diplomacy");
                        });
                        p.spawn((
                            Button,
                            Action::CloseTalk,
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
                        .with_children(|p| {
                            glyph(p, assets, prefix, "cross", 14.0, Tone::Quiet.ink())
                        });
                    });
                    // What the leader says.
                    p.spawn((
                        Node {
                            padding: UiRect::all(px(16)),
                            border: UiRect::all(px(1)),
                            border_radius: rounded(12.0),
                            ..default()
                        },
                        BackgroundColor(INSET),
                        BorderColor::all(TRIM.with_alpha(0.4)),
                    ))
                    .with_children(|p| {
                        // Wraps within the box, unlike the theme's `text`, which keeps its line.
                        p.spawn((
                            Node {
                                width: percent(100),
                                ..default()
                            },
                            Text::new(greeting(game, me, them)),
                            TextFont {
                                font_size: if narrow { 14.0 } else { 16.0 },
                                ..default()
                            },
                            TextColor(ON_PLATE),
                        ));
                    });
                    // What is known of the nation.
                    p.spawn(Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(6),
                        ..default()
                    })
                    .with_children(|p| {
                        fact(p, "Government", faction.government.clone());
                        fact(p, "Cities known", cities.to_string());
                        fact(p, "Industry", faction.industry.to_string());
                    });
                    // How to answer.
                    p.spawn(Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(6),
                        margin: UiRect::top(px(4)),
                        ..default()
                    })
                    .with_children(|p| {
                        button(p, "Propose a treaty", Action::Idle, Tone::Dormant);
                        button(p, "Farewell", Action::CloseTalk, Tone::Quiet);
                    });
                    text(p, "Treaties are not yet part of the game.", 11.5, MUTED);
                });
            });
        });
}

/// The small label under the nation's name.
fn caption_dark(p: &mut ChildSpawnerCommands, label: &str) {
    crate::theme::bold(p, label.to_uppercase(), 10.5, TRIM);
}

/// A fact about the nation: its name on the left and the figure on the right.
fn fact(p: &mut ChildSpawnerCommands, label: &str, value: String) {
    p.spawn(Node {
        width: percent(100),
        justify_content: JustifyContent::SpaceBetween,
        align_items: AlignItems::Center,
        padding: UiRect::bottom(px(5)),
        border: UiRect::bottom(px(1)),
        ..default()
    })
    .insert(BorderColor::all(Color::WHITE.with_alpha(0.10)))
    .with_children(|p| {
        text(p, label, 13.0, MUTED);
        bold(p, value, 13.0, ON_PLATE);
    });
}
