use std::{env, net::SocketAddr, path::PathBuf, sync::Arc};

use agent_ide_lib::{event_bus::EventBus, AppState, lsp, pty_client};
use axum::{
    extract::{Path, Request, State, WebSocketUpgrade},
    http::{header::AUTHORIZATION, StatusCode},
    middleware::{from_fn_with_state, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::Value;
use tokio::sync::broadcast;
use tower_http::{
    cors::CorsLayer,
    services::{ServeDir, ServeFile},
};

#[path = "../server/dispatcher.rs"]
mod dispatcher;
use dispatcher::dispatch;

struct ServerState {
    app_state: Arc<AppState>,
    auth_token: String,
    event_bus: broadcast::Sender<agent_ide_lib::event_bus::ServerEvent>,
}

async fn auth_middleware(
    State(state): State<Arc<ServerState>>,
    request: Request,
    next: Next,
) -> Response {
    let provided = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "));

    match provided {
        Some(token) if token == state.auth_token => next.run(request).await,
        _ => (StatusCode::UNAUTHORIZED, "Unauthorized").into_response(),
    }
}

async fn invoke_command(
    State(state): State<Arc<ServerState>>,
    Path(command): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    match dispatch(&state.app_state, &command, body).await {
        Ok(value) => Json(value).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err).into_response(),
    }
}

async fn events_websocket(
    State(state): State<Arc<ServerState>>,
    ws: WebSocketUpgrade,
    req: Request,
) -> Response {
    let provided = req
        .uri()
        .query()
        .and_then(|q| {
            q.split('&')
                .find_map(|pair| pair.strip_prefix("token="))
        })
        .map(|t| t.to_string());

    match provided {
        Some(token) if token == state.auth_token => {}
        _ => return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response(),
    }

    let mut rx = state.event_bus.subscribe();
    ws.on_upgrade(move |mut socket| async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if let Ok(json) = serde_json::to_string(&event) {
                        if socket
                            .send(axum::extract::ws::Message::Text(json.into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                }
                Err(_) => break,
            }
        }
    })
}

async fn async_main() -> Result<(), Box<dyn std::error::Error>> {
    if env::var("AGENT_IDE_CONFIG_DIR").is_err() {
        if let Some(dir) = dirs::config_dir() {
            env::set_var("AGENT_IDE_CONFIG_DIR", dir.join("agent-ide"));
        }
    }

    let auth_token = env::var("AGENT_IDE_AUTH_TOKEN")
        .unwrap_or_else(|_| "agent-ide-dev-token".to_string());

    let static_dir: PathBuf = env::var("AGENT_IDE_STATIC_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/app/dist"));

    let port: u16 = env::var("AGENT_IDE_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    let (event_tx, _event_rx) = broadcast::channel::<agent_ide_lib::event_bus::ServerEvent>(1024);
    let event_bus = EventBus::Broadcast(event_tx.clone());

    let lsp_manager = Arc::new(lsp::LspManager::default());
    let app_state = AppState::new(event_bus.clone(), lsp_manager);

    let pty_client = Arc::new(
        pty_client::PtyClient::new(pty_client::daemon_socket_path(), event_bus.clone(), false).await?,
    );
    let _ = app_state.pty_client.set(pty_client);

    let server_state = Arc::new(ServerState {
        app_state,
        auth_token,
        event_bus: event_tx,
    });

    let api = Router::new()
        .route("/-/invoke/{command}", post(invoke_command))
        .route("/-/events", get(events_websocket))
        .route_layer(from_fn_with_state(server_state.clone(), auth_middleware));

    let index_html = static_dir.join("index.html");
    let app = Router::new()
        .merge(api)
        .fallback_service(
            ServeDir::new(&static_dir)
                .append_index_html_on_directories(true)
                .fallback(ServeFile::new(index_html)),
        )
        .layer(CorsLayer::permissive())
        .with_state(server_state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("agent-ide-server listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--pty-daemon") {
        let daemonize = args.iter().any(|a| a == "--daemonize");
        return agent_ide_lib::run_pty_daemon(daemonize).map_err(|e| e.into());
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async_main())
}
