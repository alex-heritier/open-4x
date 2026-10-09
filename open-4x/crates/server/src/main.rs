use axum::{
    Json, Router,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
    routing::get,
};
use fourx_content::{Content, Pack, load_directory};
use fourx_runtime::{Host, Start};
use fourx_sim::{Id, Request, Response};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::{Mutex, broadcast};
use tower_http::services::ServeDir;

#[derive(Clone)]
struct Shared {
    host: Arc<Mutex<Host>>,
    /// The nation the commander connection controls.
    commander_id: Id,
    changed: broadcast::Sender<()>,
    commander: Arc<Mutex<bool>>,
    token: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let value = |flag: &str| args.windows(2).find(|a| a[0] == flag).map(|a| a[1].clone());
    if args.iter().any(|a| a == "--help") {
        println!(
            "fourx-server [--bind 127.0.0.1:7878] [--seed 42] [--pack DIRECTORY] [--scenario ID] [--nation ID] [--list] [--load SAVE] [--save SAVE] [--simulate DAYS]\nSet FOURX_TOKEN to require a commander token. GET /health /pack /assets/*; WebSocket /ws."
        );
        return Ok(());
    }
    let asset_dir = value("--pack").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/packs/base")
    });
    let content = load_directory(&asset_dir)?;
    let seed = value("--seed").unwrap_or("42".into()).parse()?;
    let scenario = value("--scenario");
    let nation = value("--nation");
    if args.iter().any(|a| a == "--list") {
        print_catalog(&content, scenario.as_deref())?;
        return Ok(());
    }
    let mut host = if let Some(path) = value("--load") {
        Host::load(&std::fs::read_to_string(path)?)?
    } else {
        Host::start(
            content,
            Start {
                scenario: scenario.as_deref(),
                nation: nation.as_deref(),
                seed,
            },
        )?
    };
    if let Some(days) = value("--simulate") {
        let commander = host.commander();
        for _ in 0..days.parse::<u32>()? {
            if host.game.winner.is_some() {
                break;
            }
            host.command(commander, fourx_sim::Command::EndTurn)?;
        }
        println!("{}", serde_json::to_string_pretty(&host.game)?);
        if let Some(path) = value("--save") {
            std::fs::write(path, host.save()?)?;
        }
        return Ok(());
    }
    // Asset directory and saved pack must agree, otherwise clients would render different rules.
    if serde_json::to_value(&host.pack)? != serde_json::to_value(load_directory(&asset_dir)?.pack)?
    {
        return Err("Saved pack does not match --pack directory".into());
    }
    let bind = value("--bind").unwrap_or("127.0.0.1:7878".into());
    let (changed, _) = broadcast::channel(32);
    let banner = format!(
        "{} ({}) • {} • listening on",
        host.game.scenario_name,
        host.game.scenario,
        host.game.date()
    );
    let shared = Shared {
        commander_id: host.commander(),
        host: Arc::new(Mutex::new(host)),
        changed,
        commander: Arc::new(Mutex::new(false)),
        token: std::env::var("FOURX_TOKEN").ok(),
    };
    let app = Router::new()
        .route(
            "/health",
            get(|| async {
                format!(
                    "open-4x authoritative server / protocol {}",
                    fourx_sim::PROTOCOL_VERSION
                )
            }),
        )
        .route("/pack", get(pack_route))
        .route("/ws", get(ws_route))
        .nest_service("/assets", ServeDir::new(asset_dir))
        .layer(
            tower_http::cors::CorsLayer::new()
                .allow_origin(tower_http::cors::Any)
                .allow_methods([axum::http::Method::GET]),
        )
        .with_state(shared.clone());
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    println!("{banner} {}", listener.local_addr()?);
    let save_path = value("--save");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    if let Some(path) = save_path {
        std::fs::write(path, shared.host.lock().await.save()?)?;
    }
    Ok(())
}
/// Print the pack's scenarios and, for one of them, the nations that can be commanded.
fn print_catalog(
    content: &Content,
    scenario: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "Scenarios in pack {:?} (default: {}):",
        content.pack.id, content.pack.default_scenario
    );
    for (id, s) in &content.scenarios {
        println!(
            "  {id:<16} {} | starts {} | {} nations | {}x{}",
            s.name,
            s.start_date,
            s.nations.len(),
            s.map.width,
            s.map.height
        );
    }
    let s = content.scenario(scenario)?;
    println!(
        "\nNations in {:?} (default commander: {}):",
        s.id, s.commander
    );
    for n in &s.nations {
        println!("  {:<28} {}", n.id, n.name);
    }
    Ok(())
}
async fn pack_route(State(shared): State<Shared>) -> Json<Pack> {
    Json(shared.host.lock().await.pack.clone())
}
async fn ws_route(State(shared): State<Shared>, upgrade: WebSocketUpgrade) -> impl IntoResponse {
    upgrade
        .max_message_size(16 * 1024)
        .max_frame_size(16 * 1024)
        .on_upgrade(move |socket| session(socket, shared))
}
async fn send(socket: &mut WebSocket, response: Response) -> bool {
    socket
        .send(Message::Text(serde_json::to_string(&response).unwrap()))
        .await
        .is_ok()
}
async fn session(mut socket: WebSocket, shared: Shared) {
    // First message is a join handshake. The server assigns authority; requests contain no player id.
    let join = tokio::time::timeout(std::time::Duration::from_secs(10), socket.recv()).await;
    let token = match join {
        Ok(Some(Ok(Message::Text(text)))) => serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| {
                if v["type"] == "join" {
                    Some(v["token"].as_str().unwrap_or("").to_string())
                } else {
                    None
                }
            }),
        _ => None,
    };
    let Some(token) = token else {
        return;
    };
    let authorized = shared
        .token
        .as_ref()
        .is_none_or(|expected| expected == &token);
    let mut occupied = shared.commander.lock().await;
    let commander_id = shared.commander_id;
    let player = if authorized && !*occupied {
        *occupied = true;
        commander_id
    } else {
        0
    };
    drop(occupied);
    let mut changed = shared.changed.subscribe();
    let content = serde_json::json!({"type":"content","pack":shared.host.lock().await.pack});
    if socket
        .send(Message::Text(content.to_string()))
        .await
        .is_err()
    {
        if player == commander_id {
            *shared.commander.lock().await = false;
        }
        return;
    }
    if !send(&mut socket, shared.host.lock().await.snapshot(player)).await {
        if player == commander_id {
            *shared.commander.lock().await = false;
        }
        return;
    }
    let mut last_sequence = 0;
    loop {
        tokio::select! {
            notification=changed.recv() => {
                if matches!(notification,Err(broadcast::error::RecvError::Closed)) {break;}
                if !send(&mut socket,shared.host.lock().await.snapshot(player)).await {break;}
            }
            message=socket.recv() => {
                match message {
                    Some(Ok(Message::Text(text))) => {
                        let parsed=serde_json::from_str::<Request>(&text);
                        let response=match parsed {
                            Ok(request) if player==0 => Response::Rejected{sequence:request.sequence,reason:"Spectators cannot issue orders".into()},
                            Ok(request) if request.sequence<=last_sequence => Response::Rejected{sequence:request.sequence,reason:"Duplicate or out-of-order request".into()},
                            Ok(request) => {last_sequence=request.sequence; shared.host.lock().await.request(player,request)},
                            Err(_) => Response::Rejected{sequence:0,reason:"Invalid command JSON".into()},
                        };
                        let accepted=matches!(response,Response::Snapshot{..});
                        if !send(&mut socket,response).await {break;}
                        if accepted {let _=shared.changed.send(());}
                    }
                    Some(Ok(Message::Ping(data))) => {if socket.send(Message::Pong(data)).await.is_err(){break;}}
                    Some(Ok(Message::Close(_)))|None|Some(Err(_)) => break,
                    _=>{}
                }
            }
        }
    }
    if player == commander_id {
        *shared.commander.lock().await = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::{connect_async, tungstenite::Message as WireMessage};
    type Client = tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >;
    async fn snapshot(socket: &mut Client) -> Response {
        loop {
            let msg = tokio::time::timeout(std::time::Duration::from_secs(3), socket.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            if let WireMessage::Text(t) = msg {
                if serde_json::from_str::<serde_json::Value>(&t).unwrap()["type"] == "content" {
                    continue;
                }
                return serde_json::from_str::<Response>(&t).expect("valid protocol response");
            }
        }
    }
    async fn rejection(socket: &mut Client) -> String {
        loop {
            if let Response::Rejected { reason, .. } = snapshot(socket).await {
                return reason;
            }
        }
    }
    #[tokio::test]
    async fn websocket_authority_revision_replay_and_spectators() {
        let (changed, _) = broadcast::channel(32);
        let shared = Shared {
            host: Arc::new(Mutex::new(Host::base_scenario("dawn-straits", 42).unwrap())),
            commander_id: 1,
            changed,
            commander: Arc::new(Mutex::new(false)),
            token: Some("test-token".into()),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new().route("/ws", get(ws_route)).with_state(shared);
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let (mut host, _) = connect_async(format!("ws://{addr}/ws")).await.unwrap();
        host.send(WireMessage::Text(
            r#"{"type":"join","token":"test-token"}"#.into(),
        ))
        .await
        .unwrap();
        assert!(matches!(
            snapshot(&mut host).await,
            Response::Snapshot { player: 1, .. }
        ));
        let (mut spectator, _) = connect_async(format!("ws://{addr}/ws")).await.unwrap();
        spectator
            .send(WireMessage::Text(
                r#"{"type":"join","token":"wrong"}"#.into(),
            ))
            .await
            .unwrap();
        assert!(matches!(
            snapshot(&mut spectator).await,
            Response::Snapshot { player: 0, .. }
        ));
        let request=serde_json::json!({"version":fourx_sim::PROTOCOL_VERSION,"sequence":1,"revision":0,"command":{"type":"end_turn"}}).to_string();
        spectator
            .send(WireMessage::Text(request.clone()))
            .await
            .unwrap();
        assert!(rejection(&mut spectator).await.contains("Spectators"));
        host.send(WireMessage::Text(request.clone())).await.unwrap();
        assert!(matches!(
            snapshot(&mut host).await,
            Response::Snapshot {
                game: Game { turn: 2, .. },
                ..
            }
        ));
        assert!(matches!(
            snapshot(&mut spectator).await,
            Response::Snapshot {
                game: Game { turn: 2, .. },
                ..
            }
        ));
        host.send(WireMessage::Text(request)).await.unwrap();
        assert!(rejection(&mut host).await.contains("Duplicate"));
        let stale=serde_json::json!({"version":fourx_sim::PROTOCOL_VERSION,"sequence":2,"revision":0,"command":{"type":"end_turn"}}).to_string();
        host.send(WireMessage::Text(stale)).await.unwrap();
        assert!(rejection(&mut host).await.contains("Stale"));
        host.close(None).await.unwrap();
        spectator.close(None).await.unwrap();
        server.abort();
    }
    use fourx_sim::Game;
}
