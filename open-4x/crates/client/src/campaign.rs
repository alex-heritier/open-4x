//! Treasury, dispatches and simultaneous stack battle playback.
use super::*;
use crate::chrome::{Frame, Skin, dress};
use crate::theme::{CARD, GAIN, INK, INK_SOFT, LOSS, Tone, bold, button, rule};
use bevy::ui::FocusPolicy;
use fourx_sim::Good;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Panel {
    Treasury,
    Dispatches,
}
#[derive(Component)]
pub(super) struct Ledger;
#[derive(Component)]
pub(super) struct BudgetHint;
pub(super) fn animate(time: Res<Time>, mut session: ResMut<Session>) {
    if let Some((battle, elapsed)) = &mut session.stack_replay {
        let old = (*elapsed / 0.45) as u32;
        *elapsed = (*elapsed + time.delta_secs()).min((battle.rounds + 1) as f32 * 0.45);
        if (*elapsed / 0.45) as u32 != old {
            session.dirty = true;
        }
    }
}
pub(super) fn scroll(
    session: Res<Session>,
    mut wheel: MessageReader<MouseWheel>,
    mut list: Query<(&ComputedNode, &mut ScrollPosition), With<Ledger>>,
) {
    let delta: f32 = wheel.read().map(|e| -e.y * 30.0).sum();
    if session.bureau.is_none() && session.stack_replay.is_none() {
        return;
    }
    for (node, mut pos) in &mut list {
        let max = (node.content_size().y - node.size().y) * node.inverse_scale_factor();
        pos.y = (pos.y + delta).clamp(0.0, max.max(0.0));
    }
}
pub(super) fn budget_hint(
    mut session: ResMut<Session>,
    hints: Query<&Interaction, (With<BudgetHint>, Changed<Interaction>)>,
) {
    if hints.iter().any(|i| *i == Interaction::Hovered) {
        if let Some(game) = &session.game {
            let f = &game.factions[&session.player];
            let interest = (f.debt * session.rules.economy.interest_percent + 99) / 100;
            let message = format!(
                "Weekly treasury: income {} · upkeep {} (interest {}) · net {:+} · debt {}",
                game.income(f.id),
                game.budget_upkeep(f.id, &session.rules),
                interest,
                game.income(f.id) - game.budget_upkeep(f.id, &session.rules),
                f.debt
            );
            if session.message != message {
                session.message = message;
                session.dirty = true;
            }
        }
    }
}
fn overlay<'a>(commands: &'a mut Commands, width: f32) -> EntityCommands<'a> {
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
            BackgroundColor(Color::srgba(0.02, 0.03, 0.06, 0.65)),
            FocusPolicy::Block,
            GlobalZIndex(65),
            UiRoot,
        ))
        .with_children(|_| {});
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: percent(50),
            top: percent(12),
            width: px(width),
            max_width: percent(94),
            max_height: percent(78),
            margin: UiRect::left(px(-width / 2.0)),
            padding: UiRect::all(px(26)),
            flex_direction: FlexDirection::Column,
            row_gap: px(10),
            overflow: Overflow::clip(),
            ..default()
        },
        BackgroundColor(CARD),
        FocusPolicy::Block,
        GlobalZIndex(66),
        UiRoot,
    ))
}
pub(super) fn spawn(
    commands: &mut Commands,
    assets: &AssetServer,
    session: &Session,
    game: &Game,
    size: Vec2,
) {
    let width = (size.x - 32.0).clamp(280.0, 640.0);
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(18),
                top: px(160),
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                ..default()
            },
            UiRoot,
        ))
        .with_children(|p| {
            if session.rules.economy.enabled {
                p.spawn(Node::default())
                    .insert((Interaction::default(), BudgetHint))
                    .with_children(|p| {
                        button(
                            p,
                            "Treasury & market",
                            Action::OpenBureau(Panel::Treasury),
                            Tone::Quiet,
                        )
                    });
            }
            button(
                p,
                "Dispatches",
                Action::OpenBureau(Panel::Dispatches),
                Tone::Quiet,
            );
            for d in game
                .diplomacy
                .demands
                .iter()
                .filter(|d| d.to == session.player)
            {
                button(
                    p,
                    &format!("Demand from {}", game.factions[&d.from].name),
                    Action::Talk(d.from),
                    Tone::Primary,
                );
            }
        });
    if let Some(panel) = session.bureau {
        overlay(commands,width).with_children(|p|{
            dress(p,assets,&session.asset_prefix,Skin::Paper,Frame::Grand,false);
            bold(p,if panel==Panel::Treasury{"National treasury"}else{"Campaign dispatches"},24.0,INK);
            button(p,"Close [Esc]",Action::CloseBureau,Tone::Quiet);rule(p);
            p.spawn((Node{flex_direction:FlexDirection::Column,row_gap:px(8),overflow:Overflow::scroll_y(),min_height:px(0),flex_shrink:1.0,..default()},Ledger,ScrollPosition::default())).with_children(|p|match panel {
                Panel::Treasury=>{
                    let f=&game.factions[&session.player];let r=&session.rules.economy;let upkeep=game.budget_upkeep(f.id,&session.rules);let income=game.income(f.id);
                    bold(p,format!("{} gold · {:+} per week",f.gold,income-upkeep),19.0,INK);
                    text(p,format!("Income {income} · upkeep {upkeep} · debt {} · interest {}% weekly",f.debt,r.interest_percent),14.0,INK_SOFT);
                    for g in Good::ALL {
                        p.spawn(Node{width:percent(100),align_items:AlignItems::Center,justify_content:JustifyContent::SpaceBetween,column_gap:px(8),..default()}).with_children(|p|{
                            bold(p,format!("{}: {}",g.name(),f.stockpile.get(g)),17.0,INK);
                            button(p,&format!("Buy 1 · {}g",r.price(g,true)),Action::Trade(g,1),Tone::Quiet);
                            button(p,&format!("Sell 1 · {}g",r.price(g,false)),Action::Trade(g,-1),Tone::Quiet);
                        });
                    }
                    rule(p);text(p,format!("Factories: Coal + Iron → Steel; Food + Cotton → Supplies. Up to {} of each recipe per factory each week.",r.factory_capacity),14.0,INK_SOFT);
                    text(p,format!("Resource tiles produce every {} weeks. Colonial cargo must reach a home city; cargo arriving by sea needs a built port.",r.resource_period),14.0,INK_SOFT);
                    button(p,&format!("Borrow {} gold",r.loan_principal),Action::Borrow,Tone::Quiet);button(p,"Repay a loan",Action::Repay,Tone::Quiet);
                    let units=game.units.values().filter(|u|u.owner==f.id && u.cargo.is_none()).count();let buildings=game.cities.values().filter(|c|c.owner==f.id).map(|c|c.buildings.len()).sum::<usize>();
                    text(p,format!("{units} units × {}g · {buildings} buildings × {}g · interest {}g",r.unit_upkeep,r.building_upkeep,(f.debt*r.interest_percent+99)/100),14.0,INK_SOFT);
                },
                Panel::Dispatches=>{for entry in game.log.iter().rev(){text(p,entry,15.0,INK);rule(p);}},
            });
            // Command failures stay visible while this modal is open.
            text(p,&session.message,13.0,INK_SOFT);
        });
    }
    if let Some((b, elapsed)) = &session.stack_replay {
        let step = (*elapsed / 0.45) as u32;
        let finished = step > b.rounds;
        overlay(commands, width).with_children(|p| {
            dress(
                p,
                assets,
                &session.asset_prefix,
                Skin::Paper,
                Frame::Grand,
                false,
            );
            bold(
                p,
                format!("Battle at {},{}", b.target.x, b.target.y),
                24.0,
                INK,
            );
            text(
                p,
                format!(
                    "{} against {} · {} lanes · round {}/{}",
                    game.factions[&b.owner].name,
                    game.factions[&b.enemy].name,
                    b.width,
                    step.min(b.rounds),
                    b.rounds
                ),
                15.0,
                INK_SOFT,
            );
            let lost_a = b
                .lanes
                .iter()
                .filter(|l| {
                    l.start + l.rounds.len() as u32 <= step
                        && l.outcome == fourx_sim::Outcome::DefenderWon
                })
                .count();
            let lost_d = b
                .lanes
                .iter()
                .filter(|l| {
                    l.start + l.rounds.len() as u32 <= step
                        && l.outcome == fourx_sim::Outcome::AttackerWon
                })
                .count();
            bold(
                p,
                format!("Losses: {lost_a} attackers · {lost_d} defenders"),
                17.0,
                INK,
            );
            p.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(8),
                    overflow: Overflow::scroll_y(),
                    min_height: px(0),
                    ..default()
                },
                Ledger,
                ScrollPosition::default(),
            ))
            .with_children(|p| {
                for lane in 0..b.width {
                    if let Some(l) = b
                        .lanes
                        .iter()
                        .filter(|l| l.lane == lane && l.start <= step)
                        .max_by_key(|l| l.start)
                    {
                        let n = step.saturating_sub(l.start).min(l.rounds.len() as u32) as usize;
                        let hits = l.rounds[..n].iter().filter(|hit| **hit).count() as i32;
                        p.spawn(Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: px(3),
                            ..default()
                        })
                        .with_children(|p| {
                            text(p, format!("Lane {}", lane + 1), 12.0, INK_SOFT);
                            health(
                                p,
                                &session.rules.units[&l.attacker.kind].name,
                                l.attacker.hp
                                    - (n as i32 - hits)
                                    - i32::from(l.support.as_ref().is_some_and(|s| s.hit)),
                                l.attacker.max_hp,
                                GAIN,
                            );
                            health(
                                p,
                                &session.rules.units[&l.defender.kind].name,
                                l.defender.hp - hits,
                                l.defender.max_hp,
                                LOSS,
                            );
                        });
                    }
                }
            });
            if finished {
                bold(
                    p,
                    if b.advanced {
                        "Victory — the stack advances"
                    } else {
                        "The defending stack holds"
                    },
                    19.0,
                    INK,
                );
            }
            button(
                p,
                if finished {
                    "Continue"
                } else {
                    "Skip battle [Esc]"
                },
                Action::CloseBattle,
                Tone::Primary,
            );
        });
    }
}
fn health(p: &mut ChildSpawnerCommands, name: &str, hp: i32, max: i32, color: Color) {
    text(p, format!("{name}  {}/{max}", hp.max(0)), 14.0, INK);
    p.spawn((
        Node {
            width: percent(100),
            height: px(7),
            ..default()
        },
        BackgroundColor(INK_SOFT.with_alpha(0.15)),
    ))
    .with_children(|p| {
        p.spawn((
            Node {
                width: percent(100.0 * hp.max(0) as f32 / max.max(1) as f32),
                height: percent(100),
                ..default()
            },
            BackgroundColor(color),
        ));
    });
}

fn text(p: &mut ChildSpawnerCommands, value: impl Into<String>, size: f32, color: Color) {
    p.spawn((
        Node {
            width: percent(100),
            flex_shrink: 0.0,
            ..default()
        },
        Text::new(value),
        TextFont {
            font_size: size,
            ..default()
        },
        TextColor(color),
    ));
}
