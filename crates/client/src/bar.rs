//! The action bar: what the selected unit can be ordered to do, as separate circle
//! buttons floating at the foot of the map, the way Civ3 lays out its unit commands.
//!
//! Two rows, spaced apart and centred. Above go the orders of the unit's own trade:
//! found a city, build a road, a mine or a farm, and every blow it can strike from where it
//! stands. Below go the standing orders every unit can be given: fortify, cancel its orders,
//! disband it. A button appears for every order the design can give at all; one that cannot be
//! given right now is dimmed and does nothing. The pointer resting on a button names it above
//! the bar, and says what is missing when it is dimmed.
//!
//! Go to chooses a movement destination; strike buttons give explicit combat orders.
use super::*;
use crate::theme::{BAD, INK, INK_SOFT, ON_PLATE, PLATE, TRIM, Tone, glyph, lift_small, rounded};
use fourx_sim::{Domain, Order, Unit};

/// Diameter of a button on screen.
const BUTTON: f32 = 48.0;
/// Side of the glyph on it. The glyphs are 64 px, drawn at under half for crisp scaling.
const GLYPH: f32 = 26.0;

/// One button of the bar.
#[derive(Clone)]
pub(super) struct Entry {
    pub action: Action,
    /// The glyph, a file of the pack's `ui/icons/`.
    pub icon: &'static str,
    /// The key that gives the same order, printed on the button.
    pub key: Option<&'static str>,
    /// What the pointer resting on the button reads.
    pub label: String,
    /// Why the order cannot be given now, or `None` when it can.
    pub blocked: Option<String>,
}

/// The two rows of the bar for a unit, top row first. Empty rows are left out. Only the
/// player's own units have orders.
pub(super) fn rows(game: &Game, rules: &Rules, unit: &Unit, player: Id) -> Vec<Vec<Entry>> {
    if unit.owner != player {
        return Vec::new();
    }
    let def = rules.def(unit);
    let ashore = unit.carrier.is_none();
    let aboard = |blocked: Option<String>| {
        if ashore {
            blocked
        } else {
            Some("Come ashore first".to_string())
        }
    };
    let mut own = Vec::new();
    if def.settler {
        own.push(Entry {
            action: Action::Found,
            icon: "city",
            key: Some("B"),
            label: "Found a city".into(),
            blocked: aboard(
                game.can_found_city(player, unit.position)
                    .err()
                    .map(String::from),
            ),
        });
    }
    if def.can_work() {
        let options = game.job_options(unit.position);
        for (job, icon, key) in [
            (next_road_job(game, unit), None, "R"),
            (Job::Mine, Some("pickaxe"), "M"),
            (Job::Farm, Some("wheat"), "I"),
        ] {
            let icon = icon.unwrap_or(if job == Job::Rail { "rails" } else { "road" });
            let (label, blocked) = match options[&job] {
                Ok(work) => (
                    format!(
                        "Build a {} ({} turns)",
                        job.name().to_lowercase(),
                        work.div_ceil(def.work)
                    ),
                    None,
                ),
                Err(why) => (
                    format!("Build a {}", job.name().to_lowercase()),
                    Some(why.to_string()),
                ),
            };
            own.push(Entry {
                action: Action::Work(job),
                icon,
                key: Some(key),
                label,
                blocked: aboard(blocked),
            });
        }
    }
    for strike in strikes(game, rules, unit) {
        own.push(Entry {
            action: Action::Strike(strike.target),
            icon: if strike.bombard { "fire" } else { "shock" },
            key: None,
            label: strike.label(),
            blocked: None,
        });
    }
    let mut standing = vec![Entry {
        action: Action::GoTo,
        icon: "road",
        key: Some("G"),
        label: "Go to".into(),
        blocked: None,
    }];
    if unit.carrier.is_some() || (def.is_carrier() && !game.passengers(unit.id).is_empty()) {
        standing.push(Entry {
            action: Action::Unload,
            icon: "tools",
            key: None,
            label: "Unload passengers in port".into(),
            blocked: None,
        });
    }
    if unit.carrier.is_none() && def.domain == Domain::Land {
        for ship in game
            .units_at(unit.position)
            .into_iter()
            .filter(|s| s.owner == player && rules.def(s).is_carrier())
        {
            standing.push(Entry {
                action: Action::Load(ship.id),
                icon: "tools",
                key: None,
                label: format!("Load aboard {}", rules.def(ship).name),
                blocked: None,
            });
        }
    }
    if def.domain == Domain::Land && def.defense > 0 {
        standing.push(Entry {
            action: Action::Fortify,
            icon: "shield",
            key: Some("F"),
            label: "Fortify".into(),
            blocked: aboard(match unit.order {
                Order::Fortified => Some("Already fortified".to_string()),
                Order::Fortifying => Some("Already digging in".to_string()),
                _ => None,
            }),
        });
    }
    standing.push(Entry {
        action: Action::Cancel,
        icon: "cancel",
        key: Some("Del"),
        label: "Cancel orders".into(),
        blocked: (unit.order.is_none() && unit.goto.is_none())
            .then(|| "It has no orders".to_string()),
    });
    standing.push(Entry {
        action: Action::Disband,
        icon: "cross",
        key: None,
        label: "Disband".into(),
        blocked: None,
    });
    [own, standing]
        .into_iter()
        .filter(|row| !row.is_empty())
        .collect()
}

/// A blow the unit could strike now: who stands there and how it would go.
pub(super) struct Strike {
    pub target: Coord,
    pub bombard: bool,
    /// Who holds the square: "Line Infantry in Marlow at 12,5".
    pub who: String,
    /// How it would go: the odds of an attack, or what a bombardment does.
    pub odds: String,
    undefended: bool,
}
impl Strike {
    /// The button's caption.
    fn label(&self) -> String {
        match (self.bombard, self.undefended) {
            (true, _) => format!("Bombard {}", self.who),
            (false, true) => format!("Take {}", self.who),
            (false, false) => format!("Attack {} ({})", self.who, self.odds),
        }
    }
    /// The inspector's line.
    pub fn line(&self) -> String {
        format!("{}: {}", self.who, self.odds)
    }
}

/// Every square the unit can strike from where it stands (see `presentation::strike_targets`),
/// with what stands on it and the odds.
pub(super) fn strikes(game: &Game, rules: &Rules, unit: &Unit) -> Vec<Strike> {
    crate::presentation::strike_targets(game, rules, unit)
        .into_iter()
        .map(|(target, bombard)| {
            let holder = game.hostile_holder(target, unit.owner);
            let defender = game
                .best_defender(target, holder.unwrap_or(0), rules)
                .and_then(|id| game.units.get(&id));
            let city = game.city_at(target).and_then(|id| game.cities.get(&id));
            let at = format!("{},{}", target.x, target.y);
            let who = match (defender, city) {
                (Some(d), Some(city)) => format!("{} in {} at {at}", rules.def(d).name, city.name),
                (Some(d), None) => format!("{} at {at}", rules.def(d).name),
                (None, Some(city)) => format!("{} at {at}, undefended", city.name),
                (None, None) => format!("foreign units at {at}"),
            };
            let estimate = (!bombard)
                .then(|| game.estimate_attack(unit.id, target, rules))
                .flatten();
            let odds = match (bombard, &estimate) {
                (true, _) => "wounds, never kills".to_string(),
                (false, Some(e)) => format!("{:.0}% to win", e.win_chance * 100.0),
                (false, None) => "undefended".to_string(),
            };
            Strike {
                target,
                bombard,
                who,
                odds,
                undefended: !bombard && estimate.is_none(),
            }
        })
        .collect()
}

/// A button on the bar. Its fill follows the pointer (see `theme::paint`).
#[derive(Component)]
pub(super) struct BarButton {
    /// What the pointer resting here reads, with the reason when the order is blocked.
    caption: String,
    /// Cannot be given now: drawn faint, and the click does nothing.
    pub dormant: bool,
}

/// The line above the bar that names the button under the pointer.
#[derive(Component)]
pub(super) struct BarHint;

/// Where the bar sits: the free space of the window, and how far above its foot.
pub(super) struct Placement {
    pub left: f32,
    pub right: f32,
    pub bottom: f32,
}

/// Build the bar. `prefix` is where the pack's files are served from.
pub(super) fn spawn(
    commands: &mut Commands,
    assets: &AssetServer,
    prefix: &str,
    rows: Vec<Vec<Entry>>,
    place: Placement,
) {
    if rows.is_empty() {
        return;
    }
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(place.left),
                right: px(place.right),
                bottom: px(place.bottom),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(8),
                ..default()
            },
            UiRoot,
        ))
        .with_children(|bar| {
            bar.spawn((
                Node {
                    display: Display::None,
                    padding: UiRect::axes(px(14), px(5)),
                    border: UiRect::all(px(1.5)),
                    border_radius: rounded(4.0),
                    ..default()
                },
                BackgroundColor(PLATE),
                BorderColor::all(TRIM.with_alpha(0.55)),
                lift_small(),
                BarHint,
                Text::new(""),
                TextFont {
                    font_size: 12.5,
                    ..default()
                },
                TextColor(ON_PLATE),
            ));
            for row in rows {
                bar.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    justify_content: JustifyContent::Center,
                    column_gap: px(10),
                    row_gap: px(10),
                    ..default()
                })
                .with_children(|row_node| {
                    for entry in row {
                        button(row_node, assets, prefix, entry);
                    }
                });
            }
        });
}

fn button(row: &mut ChildSpawnerCommands, assets: &AssetServer, prefix: &str, entry: Entry) {
    let dormant = entry.blocked.is_some();
    let caption = match (&entry.key, &entry.blocked) {
        (Some(key), None) => format!("{} ({key})", entry.label),
        (None, None) => entry.label.clone(),
        (_, Some(why)) => format!("{}: {why}", entry.label),
    };
    let tone = if dormant { Tone::Dormant } else { Tone::Quiet };
    // A blow is red, so that the bar reads at a glance as where the fighting is.
    let face = match (&entry.action, dormant) {
        (_, true) => tone.ink(),
        (Action::Strike(_), false) => BAD,
        _ => INK,
    };
    row.spawn((
        Button,
        entry.action,
        BarButton { caption, dormant },
        tone,
        BackgroundColor(tone.fill(Interaction::None)),
        Node {
            width: px(BUTTON),
            height: px(BUTTON),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(px(2.0)),
            border_radius: rounded(BUTTON / 2.0),
            overflow: Overflow::clip(),
            ..default()
        },
        BorderColor::all(if dormant {
            TRIM.with_alpha(0.25)
        } else {
            TRIM.with_alpha(0.9)
        }),
        lift_small(),
    ))
    .with_children(|b| {
        glyph(b, assets, prefix, entry.icon, GLYPH, face);
        if let Some(key) = entry.key {
            b.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    right: px(5),
                    bottom: px(2),
                    ..default()
                },
                Text::new(key),
                TextFont {
                    font_size: 9.5,
                    weight: bevy::text::FontWeight::SEMIBOLD,
                    ..default()
                },
                TextColor(if dormant {
                    INK.with_alpha(0.28)
                } else {
                    INK_SOFT
                }),
            ));
        }
    });
}

/// The name of the button under the pointer.
pub(super) fn update(
    all: Query<(&Interaction, &BarButton)>,
    mut hint: Query<(&mut Text, &mut Node), With<BarHint>>,
) {
    let Ok((mut text, mut node)) = hint.single_mut() else {
        return;
    };
    let under = all
        .iter()
        .find(|(interaction, _)| **interaction != Interaction::None)
        .map(|(_, button)| button.caption.as_str());
    let display = if under.is_some() {
        Display::Flex
    } else {
        Display::None
    };
    if node.display != display {
        node.display = display;
    }
    let shown = under.unwrap_or_default();
    if text.0 != shown {
        text.0 = shown.to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fourx_runtime::Host;

    fn start() -> (Host, Id) {
        let host =
            Host::base_scenario("russo-japanese-war", 42).expect("the starter scenario starts");
        let player = host.commander();
        (host, player)
    }

    fn unit_of(host: &Host, player: Id, kind: &str) -> Id {
        host.game
            .units
            .values()
            .find(|u| u.owner == player && u.kind == kind)
            .unwrap_or_else(|| panic!("the commander starts with a {kind}"))
            .id
    }

    fn bar_of(host: &Host, player: Id, unit: Id) -> Vec<Vec<Entry>> {
        rows(&host.game, &host.rules, &host.game.units[&unit], player)
    }

    fn find(bar: &[Vec<Entry>], action: fn(&Action) -> bool) -> Option<&Entry> {
        bar.iter().flatten().find(|e| action(&e.action))
    }

    #[test]
    fn a_soldier_gets_the_standing_orders_and_no_trade() {
        let (host, player) = start();
        let bar = bar_of(&host, player, unit_of(&host, player, "cavalry"));
        assert_eq!(bar.len(), 1, "a soldier with no one to hit has the one row");
        let actions: Vec<_> = bar[0].iter().map(|e| e.icon).collect();
        assert_eq!(actions, ["road", "shield", "cancel", "cross"]);
    }

    #[test]
    fn a_fortified_unit_cannot_fortify_again_nor_cancel_nothing() {
        let (host, player) = start();
        let infantry = unit_of(&host, player, "infantry");
        let bar = bar_of(&host, player, infantry);
        assert!(host.game.units[&infantry].is_fortified());
        let fortify = find(&bar, |a| matches!(a, Action::Fortify)).unwrap();
        assert_eq!(fortify.blocked.as_deref(), Some("Already fortified"));
        // The starter's infantry stands fortified with no march, but a fortification is an order.
        let cancel = find(&bar, |a| matches!(a, Action::Cancel)).unwrap();
        assert_eq!(cancel.blocked, None);
        let cavalry = bar_of(&host, player, unit_of(&host, player, "cavalry"));
        let cancel = find(&cavalry, |a| matches!(a, Action::Cancel)).unwrap();
        assert_eq!(cancel.blocked.as_deref(), Some("It has no orders"));
    }

    #[test]
    fn a_pioneer_can_found_a_city_only_where_one_may_stand() {
        let (host, player) = start();
        let pioneer = unit_of(&host, player, "pioneer");
        let bar = bar_of(&host, player, pioneer);
        assert_eq!(bar.len(), 2);
        let found = find(&bar, |a| matches!(a, Action::Found)).unwrap();
        // The pioneer starts in its own capital, three tiles from nothing but itself.
        assert!(found.blocked.is_some(), "{:?}", found.blocked);
        assert_eq!(found.key, Some("B"));
        // A pioneer can work the land too, but not inside a city.
        let work: Vec<_> = bar
            .iter()
            .flatten()
            .filter(|e| matches!(e.action, Action::Work(_)))
            .collect();
        assert!(!work.is_empty() && work.iter().all(|e| e.blocked.is_some()));
    }

    #[test]
    fn a_worker_gets_three_jobs_and_a_reason_for_each_it_cannot_do() {
        let (host, player) = start();
        let worker = unit_of(&host, player, "worker");
        let bar = bar_of(&host, player, worker);
        let jobs: Vec<_> = bar[0]
            .iter()
            .filter_map(|e| match e.action {
                Action::Work(job) => Some((job, e.icon, e.blocked.is_some())),
                _ => None,
            })
            .collect();
        assert_eq!(jobs.len(), 3);
        assert_eq!(jobs[0].0, Job::Road);
        assert_eq!(jobs[0].1, "road");
        assert_eq!(jobs[1].0, Job::Mine);
        assert_eq!(jobs[2].0, Job::Farm);
        // In a city nothing needs building.
        assert!(jobs.iter().all(|(_, _, blocked)| *blocked));
    }

    #[test]
    fn the_second_stretch_of_road_is_a_railroad() {
        let (mut host, player) = start();
        let worker = unit_of(&host, player, "worker");
        let at = host
            .game
            .map
            .tiles
            .iter()
            .find(|t| {
                host.game.city_at(t.position).is_none()
                    && host.game.can_improve(t.position, Job::Road).is_ok()
            })
            .expect("the map has open ground")
            .position;
        host.game.set_position(worker, at);
        let reload = |host: &Host| bar_of(host, player, worker);
        let first = reload(&host);
        let road = find(&first, |a| matches!(a, Action::Work(Job::Road))).unwrap();
        assert_eq!((road.icon, road.blocked.as_deref()), ("road", None));
        host.game.map.get_mut(at).unwrap().improvements |= fourx_sim::terrain::Tile::ROAD;
        let second = reload(&host);
        let rail = find(&second, |a| matches!(a, Action::Work(Job::Rail))).unwrap();
        assert_eq!((rail.icon, rail.blocked.as_deref()), ("rails", None));
    }

    #[test]
    fn units_of_other_nations_have_no_bar() {
        let (host, player) = start();
        let other = host
            .game
            .units
            .values()
            .find(|u| u.owner != player)
            .expect("there is a rival")
            .id;
        assert!(bar_of(&host, player, other).is_empty());
    }

    #[test]
    fn an_enemy_in_reach_is_a_blow_on_the_bar_with_its_odds() {
        let (mut host, player) = start();
        let infantry = unit_of(&host, player, "infantry");
        let rival = host
            .game
            .units
            .values()
            .find(|u| u.owner != player && u.kind == "infantry")
            .unwrap()
            .id;
        // Put the rival's infantry on the square next to ours.
        let here = host.game.units[&infantry].position;
        let next = Coord::new(here.x + 1, here.y);
        host.game.set_position(rival, next);
        let bar = bar_of(&host, player, infantry);
        let blow = find(&bar, |a| matches!(a, Action::Strike(_))).expect("a blow is offered");
        assert_eq!(blow.icon, "shock");
        assert!(
            blow.label.starts_with("Attack Line Infantry"),
            "{}",
            blow.label
        );
        assert!(blow.label.contains("% to win"), "{}", blow.label);
        assert_eq!(blow.blocked, None);
        assert!(matches!(blow.action, Action::Strike(t) if t == next));
    }
}
