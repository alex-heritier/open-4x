use super::*;
use fourx_sim::terrain::Terrain;

const PAPER: Color = Color::srgb(0.88, 0.83, 0.69);
const GOLD: Color = Color::srgb(0.78, 0.64, 0.38);
const INK: Color = Color::srgb(0.09, 0.15, 0.16);
const MUTED: Color = Color::srgb(0.50, 0.58, 0.55);
const RED: Color = Color::srgb(0.75, 0.24, 0.20);
fn text(parent: &mut ChildSpawnerCommands, value: impl Into<String>, size: f32, color: Color) {
    parent.spawn((
        Node {
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
fn button(parent: &mut ChildSpawnerCommands, title: impl Into<String>, action: Action) {
    parent
        .spawn((
            Button,
            action,
            Node {
                min_height: px(40),
                flex_shrink: 0.0,
                padding: UiRect::axes(px(12), px(8)),
                margin: UiRect::bottom(px(5)),
                border: UiRect::all(px(1)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(INK),
            BorderColor::all(GOLD),
        ))
        .with_children(|p| text(p, title, 14.0, PAPER));
}
fn panel() -> Node {
    Node {
        position_type: PositionType::Absolute,
        padding: UiRect::all(px(18)),
        border: UiRect::all(px(2)),
        flex_direction: FlexDirection::Column,
        row_gap: px(9),
        ..default()
    }
}
fn image_sprite(
    commands: &mut Commands,
    assets: &AssetServer,
    prefix: &str,
    path: &str,
    position: Vec3,
    size: Vec2,
    color: Color,
) {
    let mut sprite = Sprite::from_image(assets.load(format!("{prefix}{path}")));
    sprite.custom_size = Some(size);
    sprite.color = color;
    commands.spawn((sprite, Transform::from_translation(position), WorldVisual));
}

pub(super) fn refresh(
    mut commands: Commands,
    mut session: ResMut<Session>,
    assets: Res<AssetServer>,
    art: Res<Art>,
    world: Query<Entity, With<WorldVisual>>,
    ui: Query<Entity, With<UiRoot>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut camera: Query<&mut Transform, With<Camera2d>>,
    mut previous_size: Local<Vec2>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let size = Vec2::new(window.width(), window.height());
    if *previous_size != size {
        if previous_size.x == 0.0 || (previous_size.x < 900.0) != (size.x < 900.0) {
            if let Some(game) = &session.game {
                if let Some(city) = game
                    .cities
                    .values()
                    .find(|c| c.owner == session.player.max(1))
                {
                    let (x, y) = city.position.screen();
                    if let Ok(mut transform) = camera.single_mut() {
                        transform.translation =
                            Vec3::new(x + if size.x < 900.0 { 0.0 } else { 240.0 }, y, 0.0);
                    }
                }
            }
        }
        session.dirty = true;
        *previous_size = size;
    }
    if !session.dirty {
        return;
    }
    session.dirty = false;
    for e in &world {
        commands.entity(e).despawn();
    }
    for e in &ui {
        commands.entity(e).despawn();
    }
    let Some(game) = &session.game else {
        commands
            .spawn((
                Node {
                    width: percent(100),
                    height: percent(100),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                UiRoot,
            ))
            .with_children(|p| text(p, &session.message, 24.0, PAPER));
        return;
    };
    let prefix = &session.asset_prefix;
    let texture: Handle<Image> = assets.load(format!("{prefix}{}", session.pack.visuals.terrain));
    let explored = game.factions.get(&session.player).map(|f| &f.explored);
    let is_known = |p: Coord| session.player == 0 || explored.is_some_and(|e| e.contains(&p));
    for y in -1..game.map.height {
        for x in -1..game.map.width {
            let p = Coord::new(x, y);
            let (wx, wy) = p.screen();
            let mut sprite = Sprite::from_atlas_image(
                texture.clone(),
                TextureAtlas {
                    layout: art.atlas.clone(),
                    index: game.map.blend_cell(p),
                },
            );
            sprite.custom_size = Some(Vec2::new(128.0, 64.0));
            if !is_known(p) {
                sprite.color = Color::srgb(0.30, 0.39, 0.42);
            }
            commands.spawn((sprite, Transform::from_xyz(wx, wy - 32.0, 0.0), WorldVisual));
        }
    }
    for tile in &game.map.tiles {
        if !is_known(tile.position) {
            continue;
        }
        let (x, y) = tile.position.screen();
        let z = 10.0 - y / 1000.0;
        if tile.mountain {
            image_sprite(
                &mut commands,
                &assets,
                prefix,
                &session.pack.visuals.mountain,
                Vec3::new(x, y + 24.0, z),
                Vec2::new(128.0, 112.0),
                Color::WHITE,
            );
        } else if tile.forest {
            image_sprite(
                &mut commands,
                &assets,
                prefix,
                &session.pack.visuals.forest,
                Vec3::new(x, y + 24.0, z),
                Vec2::new(128.0, 112.0),
                Color::WHITE,
            );
        }
    }
    for city in game.cities.values() {
        let (x, y) = city.position.screen();
        let z = 20.0 - y / 1000.0;
        image_sprite(
            &mut commands,
            &assets,
            prefix,
            &session.pack.visuals.city,
            Vec3::new(x, y + 24.0, z),
            Vec2::new(128.0, 112.0),
            if city.owner == 1 {
                Color::WHITE
            } else {
                Color::srgb(0.75, 0.85, 1.0)
            },
        );
        commands.spawn((
            Sprite::from_color(INK, Vec2::new(134.0, 24.0)),
            Transform::from_xyz(x, y - 24.0, z + 0.1),
            WorldVisual,
        ));
        commands.spawn((
            Text2d::new(format!("{}   {}", city.population, city.name)),
            TextFont {
                font_size: 16.0,
                ..default()
            },
            TextColor(PAPER),
            Transform::from_xyz(x, y - 24.0, z + 0.2),
            WorldVisual,
        ));
    }
    for army in game.armies.values() {
        let Some(regiment) = army.regiments.first() else {
            continue;
        };
        let Some(def) = session.rules.units.get(&regiment.kind) else {
            continue;
        };
        let (mut x, mut y) = army.position.screen();
        if game.cities.values().any(|c| c.position == army.position) {
            x += 62.0;
            y += 24.0;
        }
        let stack = game
            .armies
            .values()
            .filter(|a| a.position == army.position && a.id < army.id)
            .count();
        x += stack as f32 * 24.0;
        let z = 30.0 - y / 1000.0;
        image_sprite(
            &mut commands,
            &assets,
            prefix,
            &def.sprite,
            Vec3::new(x, y + 12.0, z),
            if def.naval {
                Vec2::new(112.0, 84.0)
            } else {
                Vec2::splat(70.0)
            },
            if army.owner == 1 {
                Color::WHITE
            } else {
                Color::srgb(0.75, 0.80, 1.0)
            },
        );
        let selected_here = match session.selection {
            Selection::Army(id) => game
                .armies
                .get(&id)
                .filter(|a| a.position == army.position)
                .map(|a| a.id),
            _ => None,
        };
        if selected_here.is_some_and(|id| id != army.id) || selected_here.is_none() && stack > 0 {
            continue;
        }
        commands.spawn((
            Sprite::from_color(
                if army.owner == 1 {
                    RED
                } else {
                    Color::srgb(0.35, 0.48, 0.78)
                },
                Vec2::new(48.0, 16.0),
            ),
            Transform::from_xyz(x, y - 21.0, z + 0.1),
            WorldVisual,
        ));
        commands.spawn((
            Text2d::new(format!("{:.1}k", army.strength() as f32 / 1000.0)),
            TextFont {
                font_size: 12.0,
                ..default()
            },
            TextColor(Color::WHITE),
            Transform::from_xyz(x, y - 21.0, z + 0.2),
            WorldVisual,
        ));
    }
    // Unhurried map typography echoes printed campaign charts.
    for (label, pos) in [
        ("THE JADE STRAITS", Coord::new(14, 2)),
        ("SOUTHERN SEA", Coord::new(4, 15)),
    ] {
        let (x, y) = pos.screen();
        commands.spawn((
            Text2d::new(label),
            TextFont {
                font_size: 24.0,
                ..default()
            },
            TextColor(Color::srgba(0.67, 0.81, 0.81, 0.55)),
            Transform::from_xyz(x, y, 2.0),
            WorldVisual,
        ));
    }
    build_ui(&mut commands, &session, size);
}

fn build_ui(commands: &mut Commands, session: &Session, size: Vec2) {
    let game = session.game.as_ref().unwrap();
    let narrow = size.x < 900.0;
    let faction = &game.factions[&session.player.max(1)];
    let top = Node {
        position_type: PositionType::Absolute,
        top: px(0),
        width: percent(100),
        height: px(72),
        padding: UiRect::axes(px(20), px(10)),
        border: UiRect::bottom(px(2)),
        justify_content: JustifyContent::SpaceBetween,
        align_items: AlignItems::Center,
        column_gap: px(12),
        ..default()
    };
    commands
        .spawn((top, BackgroundColor(INK), BorderColor::all(GOLD), UiRoot))
        .with_children(|p| {
            p.spawn(Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                ..default()
            })
            .with_children(|p| {
                text(
                    p,
                    if narrow {
                        "DAWN / STRAITS"
                    } else {
                        "DAWN OVER THE STRAITS"
                    },
                    if narrow { 15.0 } else { 22.0 },
                    PAPER,
                );
                if !narrow {
                    text(
                        p,
                        format!("{} / INDUSTRIAL AGE", faction.name.to_uppercase()),
                        11.0,
                        GOLD,
                    );
                }
            });
            text(
                p,
                if narrow {
                    format!("{}g  |  Day {}", faction.gold, game.turn)
                } else {
                    format!(
                        "TREASURY  {}     RESEARCH  {}/{}     INDUSTRY  {}/{}",
                        faction.gold,
                        faction.research,
                        session.rules.research_cost * (faction.technology as i32 + 1),
                        faction.industry,
                        session.rules.victory_industry
                    )
                },
                if narrow { 13.0 } else { 15.0 },
                PAPER,
            );
            button(
                p,
                if session.pending {
                    "Waiting...".into()
                } else {
                    format!("DAY {}  >", game.turn)
                },
                Action::EndTurn,
            );
        });
    let mut sidebar = panel();
    if narrow {
        sidebar.left = px(8);
        sidebar.right = px(8);
        sidebar.bottom = px(52);
        sidebar.height = px(inspector_height(size.y));
        sidebar.padding = UiRect::all(px(10));
        sidebar.overflow = Overflow::scroll_y();
    } else {
        sidebar.left = px(16);
        sidebar.top = px(88);
        sidebar.width = px(266);
        sidebar.bottom = px(166);
        sidebar.overflow = Overflow::scroll_y();
    }
    commands.spawn((sidebar,BackgroundColor(PAPER),BorderColor::all(GOLD),UiRoot,Inspector,ScrollPosition::default())).with_children(|p|{
        text(p,"CAMPAIGN COMMAND",13.0,INK);
        match session.selection{
            Selection::Army(id) if game.armies.contains_key(&id)=>{
                let a=&game.armies[&id];text(p,&a.name,22.0,INK);
                text(p,format!("{} soldiers | Morale {}%\nMovement {}  /  General +{}",a.strength(),a.morale(),a.movement,a.general),14.0,INK);
                for r in &a.regiments{text(p,format!("{}  {}",session.rules.units[&r.kind].name,r.strength),13.0,INK);}
                if a.regiments.iter().any(|r|session.rules.units[&r.kind].settler){button(p,"FOUND A CITY",Action::Found);}
                text(p,"Click another tile to march. Right-click also orders movement. Mountains cost two steps.",13.0,INK);
            }
            Selection::City(id) if game.cities.contains_key(&id)=>{
                let c=&game.cities[&id];text(p,&c.name,24.0,INK);
                text(p,format!("Population {} | Industry {}\nProduction: {} ({})",c.population,c.industry,c.production.as_deref().unwrap_or("idle"),c.progress),14.0,INK);
                if c.owner==session.player{
                    button(p,"DEVELOP INDUSTRY (40g)",Action::Develop);
                    for (i,u) in session.rules.units.values().enumerate(){button(p,format!("{}: {} / {}",i+1,u.name,u.cost),Action::Produce(u.id.clone()));}
                }
            }
            Selection::Tile(pos)=>{
                let t=game.map.get(pos).unwrap();text(p,format!("{:?}",t.terrain),22.0,INK);
                text(p,format!("Chart {}, {}\n{}{}",pos.x,pos.y,if t.forest{"Forest  "}else{""},if t.mountain{"Mountains"}else{""}),14.0,INK);
            }
            _=>{
                text(p,"An empire at dawn",24.0,INK);
                text(p,"Explore the straits. Establish new cities. Build an army and an industrial economy. Defeat the League or reach 500 industry.",15.0,INK);
            }
        }
        if !narrow{
            text(p,"YOUR CITIES",12.0,INK);
            for c in game.cities.values().filter(|c|c.owner==session.player){button(p,format!("{}  |  Industry {}",c.name,c.industry),Action::SelectCity(c.id));}
            text(p,"YOUR EXPEDITIONS",12.0,INK);
            for a in game.armies.values().filter(|a|a.owner==session.player).take(5){button(p,format!("{}  |  {:.1}k",a.name,a.strength()as f32/1000.0),Action::SelectArmy(a.id));}
            button(p,"SAVE CAMPAIGN",Action::Save);button(p,"CENTER MAP",Action::Center);
            text(p,"WASD / arrows: pan\nWheel: zoom | Tab: next army\nC: next city | 1-5: recruit\nSpace: next day | F: settle\nE: develop | F5: save\nEsc: clear selection",12.0,INK);
        }else{
            p.spawn(Node{flex_direction:FlexDirection::Row,column_gap:px(5),flex_wrap:FlexWrap::Wrap,..default()}).with_children(|p|{
                for c in game.cities.values().filter(|c|c.owner==session.player){button(p,&c.name,Action::SelectCity(c.id));}
                for a in game.armies.values().filter(|a|a.owner==session.player).take(3){button(p,format!("Army {}",a.id),Action::SelectArmy(a.id));}
                button(p,"Save",Action::Save);
            });
        }
    });
    if !narrow {
        let mut chart = panel();
        chart.left = px(16);
        chart.bottom = px(64);
        chart.width = px(266);
        chart.height = px(90);
        chart.padding = UiRect::all(px(6));
        commands
            .spawn((chart, BackgroundColor(INK), BorderColor::all(GOLD), UiRoot))
            .with_children(|p| {
                for tile in &game.map.tiles {
                    let known = session.player == 0
                        || game.factions[&session.player]
                            .explored
                            .contains(&tile.position);
                    let color = if !known {
                        Color::srgb(0.08, 0.13, 0.15)
                    } else {
                        match tile.terrain {
                            Terrain::Water => Color::srgb(0.13, 0.33, 0.39),
                            Terrain::Grass => Color::srgb(0.43, 0.48, 0.26),
                            Terrain::Sand => GOLD,
                        }
                    };
                    p.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(6.0 + tile.position.x as f32 * 10.4),
                            top: px(5.0 + tile.position.y as f32 * 4.2),
                            width: px(10.4),
                            height: px(4.2),
                            ..default()
                        },
                        BackgroundColor(color),
                    ));
                }
                for c in game.cities.values() {
                    p.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(6.0 + c.position.x as f32 * 10.4),
                            top: px(5.0 + c.position.y as f32 * 4.2),
                            width: px(5),
                            height: px(5),
                            ..default()
                        },
                        BackgroundColor(if c.owner == 1 { RED } else { Color::WHITE }),
                    ));
                }
            });
        let mut report = panel();
        report.right = px(16);
        report.bottom = px(66);
        report.width = px(345);
        report.padding = UiRect::all(px(16));
        commands
            .spawn((
                report,
                BackgroundColor(Color::srgba(0.06, 0.12, 0.14, 0.95)),
                BorderColor::all(GOLD),
                UiRoot,
            ))
            .with_children(|p| {
                if let Some(b) = game.battles.first() {
                    text(
                        p,
                        format!("BATTLE OF THE STRAITS  /  DAY {}", b.day),
                        15.0,
                        GOLD,
                    );
                    text(
                        p,
                        format!(
                            "{} PHASE  |  Width {}",
                            b.phase.to_uppercase(),
                            session.rules.combat_width
                        ),
                        13.0,
                        PAPER,
                    );
                    for id in [b.attacker, b.defender] {
                        let a = &game.armies[&id];
                        text(
                            p,
                            format!(
                                "{}\n{} soldiers  /  {}% morale",
                                a.name,
                                a.strength(),
                                a.morale()
                            ),
                            14.0,
                            PAPER,
                        );
                    }
                    text(
                        p,
                        format!(
                            "Losses  {} / {}\nAdvance days to resolve the engagement.",
                            b.attacker_losses, b.defender_losses
                        ),
                        13.0,
                        MUTED,
                    );
                } else {
                    text(p, "DISPATCHES FROM THE FRONT", 13.0, GOLD);
                }
                for line in game.log.iter().rev().take(3) {
                    text(p, line, 13.0, PAPER);
                }
            });
    }
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: px(0),
                width: percent(100),
                height: px(48),
                padding: UiRect::axes(px(20), px(8)),
                align_items: AlignItems::Center,
                border: UiRect::top(px(1)),
                ..default()
            },
            BackgroundColor(INK),
            BorderColor::all(GOLD),
            UiRoot,
        ))
        .with_children(|p| text(p, &session.message, if narrow { 12.0 } else { 14.0 }, PAPER));
    if let Some(winner) = game.winner {
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
                BackgroundColor(Color::srgba(0.02, 0.08, 0.10, 0.75)),
                UiRoot,
            ))
            .with_children(|p| {
                p.spawn((
                    Node {
                        width: percent(70),
                        max_width: px(600),
                        padding: UiRect::all(px(40)),
                        border: UiRect::all(px(3)),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(16),
                        ..default()
                    },
                    BackgroundColor(PAPER),
                    BorderColor::all(GOLD),
                ))
                .with_children(|p| {
                    text(
                        p,
                        if winner == session.player {
                            "THE DAWN IS YOURS"
                        } else {
                            "THE EMPIRE FALLS"
                        },
                        30.0,
                        INK,
                    );
                    text(
                        p,
                        format!(
                            "{} has won the campaign on day {}.",
                            game.factions[&winner].name, game.turn
                        ),
                        20.0,
                        INK,
                    );
                    button(p, "SAVE THIS CAMPAIGN", Action::Save);
                });
            });
    }
}

pub(super) fn draw_overlays(session: Res<Session>, mut gizmos: Gizmos) {
    let Some(game) = &session.game else {
        return;
    };
    let diamond = |center: Vec2| {
        [
            center + Vec2::new(0.0, 32.0),
            center + Vec2::new(64.0, 0.0),
            center + Vec2::new(0.0, -32.0),
            center + Vec2::new(-64.0, 0.0),
            center + Vec2::new(0.0, 32.0),
        ]
    };
    // Claim boundaries follow isometric diamond edges; internal edges are suppressed.
    let owner = |p: Coord| {
        game.cities
            .values()
            .filter(|c| c.position.distance(p) <= 2)
            .min_by_key(|c| c.position.distance(p))
            .map(|c| c.owner)
    };
    for t in &game.map.tiles {
        let Some(id) = owner(t.position) else {
            continue;
        };
        if t.terrain == Terrain::Water {
            continue;
        }
        let (x, y) = t.position.screen();
        let points = diamond(Vec2::new(x, y));
        let neighbors = [
            Coord::new(t.position.x, t.position.y - 1),
            Coord::new(t.position.x + 1, t.position.y),
            Coord::new(t.position.x, t.position.y + 1),
            Coord::new(t.position.x - 1, t.position.y),
        ];
        for (i, n) in neighbors.iter().enumerate() {
            if owner(*n) != Some(id) || game.map.get(*n).is_none_or(|t| t.terrain == Terrain::Water)
            {
                gizmos.line_2d(
                    points[i],
                    points[i + 1],
                    if id == 1 {
                        Color::srgba(0.94, 0.48, 0.37, 0.85)
                    } else {
                        Color::srgba(0.48, 0.62, 0.93, 0.85)
                    },
                );
            }
        }
    }
    let pos = match session.selection {
        Selection::Army(id) => game.armies.get(&id).map(|a| a.position),
        Selection::City(id) => game.cities.get(&id).map(|c| c.position),
        Selection::Tile(pos) => Some(pos),
        Selection::None => None,
    };
    if let Some(pos) = pos {
        let (x, y) = pos.screen();
        gizmos.linestrip_2d(diamond(Vec2::new(x, y)), Color::srgb(1.0, 0.84, 0.42));
    }
    for b in &game.battles {
        let (x, y) = b.position.screen();
        gizmos.circle_2d(Vec2::new(x, y), 42.0, RED);
    }
}
