use axum::{
    body::Body,
    extract::{
        ws::{WebSocket, WebSocketUpgrade},
        Request, State,
    },
    http::{header, HeaderValue, Response, StatusCode},
    middleware::Next,
    response::Html,
    routing::{any, get},
    Router,
};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use crate::editor::models::MissionSetCollection;
use crate::net::protocol::*;
use crate::panels::launchfiles::{LaunchPackage, LaunchStatus};
use crate::panels::parameters::PackageParams;
use crate::panels::terminals::TerminalTab;

static INDEX_HTML: &str = include_str!("../../web/index.html");
const VIEWER_ASSET_CACHE_CONTROL: &str = "public, max-age=31536000, immutable";

#[derive(Clone)]
struct ProxyState {
    client: reqwest::Client,
    target_base: String,
}

fn normalize_proxy_target(endpoint: &str) -> String {
    let clean = endpoint
        .trim_start_matches("rerun+http://")
        .trim_start_matches("rerun+https://")
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_start_matches("ws://")
        .trim_start_matches("wss://");
    let host_port = clean.split('/').next().unwrap_or("127.0.0.1:9876");
    format!("http://{}", host_port)
}

/// Estado mestre residente no robô/host gerenciado pelo Daemon
#[derive(Clone)]
pub struct HostState {
    pub packages: Arc<Mutex<Vec<PackageParams>>>,
    pub launch_packages: Arc<Mutex<Vec<LaunchPackage>>>,
    pub mission_collection: Arc<Mutex<MissionSetCollection>>,
    pub terminal_tabs: Arc<Mutex<Vec<TerminalTab>>>,
    pub tx_broadcast: broadcast::Sender<ControlMessage>,
}

impl HostState {
    pub fn new(tx_broadcast: broadcast::Sender<ControlMessage>) -> Self {
        let initial_missions =
            if let Ok(col) = MissionSetCollection::load_from_file("mission_sets.json") {
                if !col.sets.is_empty() {
                    re_log::info!(
                        "Conjunto de missões restaurado de mission_sets.json ({} conjuntos)",
                        col.sets.len()
                    );
                    col
                } else {
                    MissionSetCollection::default()
                }
            } else {
                MissionSetCollection::default()
            };

        Self {
            packages: Arc::new(Mutex::new(
                crate::panels::parameters::create_mock_parameters(),
            )),
            launch_packages: Arc::new(Mutex::new(
                crate::panels::launchfiles::create_mock_launchfiles(),
            )),
            mission_collection: Arc::new(Mutex::new(initial_missions)),
            terminal_tabs: Arc::new(Mutex::new(crate::panels::terminals::create_mock_terminals())),
            tx_broadcast,
        }
    }
}

/// Localiza o diretório dos artefatos WebAssembly
pub fn resolve_web_directory(custom: Option<&str>) -> std::path::PathBuf {
    if let Some(custom_path) = custom {
        let p = std::path::PathBuf::from(custom_path);
        if p.exists() {
            return p;
        }
    }

    if let Ok(env_dir) = std::env::var("PERSE_WEB_DIR") {
        let p = std::path::PathBuf::from(env_dir);
        if p.exists() {
            return p;
        }
    }

    // 1. Diretório de trabalho atual (./web)
    let cwd_web = std::path::PathBuf::from("web");
    if cwd_web.exists() {
        return cwd_web;
    }

    // 2. Relativo ao binário executável
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            // <exe_dir>/web
            let exe_web = exe_dir.join("web");
            if exe_web.exists() {
                return exe_web;
            }

            // Padrão FHS (<prefix>/bin/perse_egui -> <prefix>/share/perse_egui/web)
            if let Some(prefix) = exe_dir.parent() {
                let share_web = prefix.join("share/perse_egui/web");
                if share_web.exists() {
                    return share_web;
                }

                // Padrão ament/colcon (<prefix>/lib/perse_egui/perse_egui -> <prefix>/share/perse_egui/web)
                if let Some(ament_prefix) = prefix.parent() {
                    let ament_share_web = ament_prefix.join("share/perse_egui/web");
                    if ament_share_web.exists() {
                        return ament_share_web;
                    }
                }
            }
        }
    }

    // 3. XDG Data Home (~/.local/share/perse_egui/web)
    if let Ok(home) = std::env::var("HOME") {
        let user_share = std::path::PathBuf::from(home).join(".local/share/perse_egui/web");
        if user_share.exists() {
            return user_share;
        }
    }

    // 4. Locais padrão do sistema (/usr/local/share e /usr/share)
    let usr_local_share = std::path::PathBuf::from("/usr/local/share/perse_egui/web");
    if usr_local_share.exists() {
        return usr_local_share;
    }
    let usr_share = std::path::PathBuf::from("/usr/share/perse_egui/web");
    if usr_share.exists() {
        return usr_share;
    }

    cwd_web
}

/// Inicia o servidor HTTP/WebSocket em modo daemon (sem interface nativa)
pub async fn run_daemon_server(
    port: u16,
    connect_endpoint: String,
    custom_web_dir: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (tx_broadcast, _) = broadcast::channel::<ControlMessage>(256);
    let host_state = HostState::new(tx_broadcast);

    let web_dir = resolve_web_directory(custom_web_dir.as_deref());
    re_log::info!("Servindo interface web a partir de: {:?}", web_dir);

    let proxy_state = ProxyState {
        client: reqwest::Client::new(),
        target_base: normalize_proxy_target(&connect_endpoint),
    };

    let app = Router::new()
        .route("/", get(index_handler))
        .route("/index.html", get(index_handler))
        .route(
            "/ws/control",
            get({
                let state = host_state.clone();
                move |ws: WebSocketUpgrade| ws_control_handler(ws, state)
            }),
        )
        .route("/proxy", any(proxy_handler))
        .route("/proxy/{*path}", any(proxy_handler))
        .route("/rerun.{*path}", any(proxy_handler))
        .fallback_service(ServeDir::new(&web_dir).append_index_html_on_directories(true))
        .with_state(proxy_state.clone())
        .layer(axum::middleware::from_fn(cache_viewer_assets_middleware))
        .layer(CorsLayer::permissive());

    let js_path = web_dir.join("rewire_viewer.js");
    let wasm_path = web_dir.join("rewire_viewer_bg.wasm");
    if !js_path.exists() || !wasm_path.exists() {
        re_log::warn!("⚠️ Atenção: Os artefatos WebAssembly ('rewire_viewer.js' ou 'rewire_viewer_bg.wasm') não foram encontrados em '{:?}'!", web_dir);
        re_log::warn!("👉 Para compilar o visualizador WebAssembly, execute o script: ./build_web.sh ou 'make download'");
    }

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    re_log::info!("Servidor perse_egui --daemon rodando em http://{}", addr);
    re_log::info!("Proxy de requisições gRPC -> {}", proxy_state.target_base);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn index_handler() -> Html<&'static str> {
    Html(INDEX_HTML)
}

fn should_cache_viewer_asset(path: &str) -> bool {
    matches!(path, "/rewire_viewer.js" | "/rewire_viewer_bg.wasm")
}

async fn cache_viewer_assets_middleware(req: Request, next: Next) -> Response {
    let should_cache = should_cache_viewer_asset(req.uri().path());
    let response = next.run(req).await;
    if !should_cache || !response.status().is_success() {
        return response;
    }

    let (mut parts, body) = response.into_parts();
    parts.headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(VIEWER_ASSET_CACHE_CONTROL),
    );
    Response::from_parts(parts, body)
}

async fn proxy_handler(
    State(state): State<ProxyState>,
    req: Request,
) -> Result<Response<Body>, StatusCode> {
    let raw_path = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str().to_string())
        .unwrap_or_default();

    let sub_path = if let Some(stripped) = raw_path.strip_prefix("/proxy") {
        if stripped.is_empty() {
            "/".to_string()
        } else {
            stripped.to_string()
        }
    } else {
        raw_path.clone()
    };

    let target_url = format!("{}{}", state.target_base, sub_path);

    let method = req.method().clone();
    let mut headers = req.headers().clone();
    headers.remove(axum::http::header::HOST);

    let body_bytes = match axum::body::to_bytes(req.into_body(), usize::MAX).await {
        Ok(b) => b,
        Err(_) => return Err(StatusCode::BAD_REQUEST),
    };

    re_log::info!("Proxying {} {} -> {}", method, raw_path, target_url);

    let mut builder = state.client.request(method, &target_url);
    for (name, val) in headers.iter() {
        builder = builder.header(name, val);
    }
    builder = builder.body(body_bytes);

    match builder.send().await {
        Ok(resp) => {
            let status = resp.status();
            let mut response_builder = Response::builder().status(status);

            for (name, val) in resp.headers().iter() {
                if name == axum::http::header::TRANSFER_ENCODING
                    || name == axum::http::header::CONNECTION
                    || name == "keep-alive"
                {
                    continue;
                }
                response_builder = response_builder.header(name, val);
            }

            response_builder = response_builder.header(
                "access-control-expose-headers",
                "grpc-status, grpc-message, grpc-encoding, grpc-accept-encoding, *",
            );

            let stream = resp.bytes_stream();
            let body = Body::from_stream(stream);
            response_builder
                .body(body)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
        }
        Err(err) => {
            re_log::error!("Proxy request para {} falhou: {}", target_url, err);
            Err(StatusCode::BAD_GATEWAY)
        }
    }
}

async fn ws_control_handler(ws: WebSocketUpgrade, host_state: HostState) -> Response<Body> {
    ws.on_upgrade(move |socket| handle_control_socket(socket, host_state))
}

async fn handle_control_socket(mut socket: WebSocket, host_state: HostState) {
    let mut rx = host_state.tx_broadcast.subscribe();

    // 1. Enviar estado inicial consolidado de todos os painéis ao novo cliente conectado
    let initial_messages = {
        let pkgs = host_state
            .packages
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default();
        let launch_pkgs = host_state
            .launch_packages
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default();
        let missions = host_state
            .mission_collection
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default();
        let tabs = host_state
            .terminal_tabs
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default();

        vec![
            ControlMessage::new(
                Domain::Parameters,
                "tree_response",
                serde_json::to_value(ParametersTreeResponse { packages: pkgs }).unwrap_or_default(),
            ),
            ControlMessage::new(
                Domain::Launchfiles,
                "list_response",
                serde_json::to_value(LaunchfilesListResponse {
                    packages: launch_pkgs,
                })
                .unwrap_or_default(),
            ),
            ControlMessage::new(
                Domain::Mission,
                "data_response",
                serde_json::to_value(MissionDataResponse {
                    collection: missions,
                })
                .unwrap_or_default(),
            ),
            ControlMessage::new(
                Domain::Terminal,
                "list_terminals",
                serde_json::to_value(TerminalsListResponse { tabs }).unwrap_or_default(),
            ),
        ]
    };

    for msg in initial_messages {
        if let Ok(json) = serde_json::to_string(&msg) {
            let _ = socket
                .send(axum::extract::ws::Message::Text(json.into()))
                .await;
        }
    }

    loop {
        tokio::select! {
            msg = socket.recv() => {
                match msg {
                    Some(Ok(axum::extract::ws::Message::Text(text))) => {
                        if let Ok(ctrl_msg) = serde_json::from_str::<ControlMessage>(&text) {
                            handle_client_message(&ctrl_msg, &host_state).await;
                        }
                    }
                    Some(Ok(axum::extract::ws::Message::Close(_))) | None => break,
                    _ => {}
                }
            }
            b_msg = rx.recv() => {
                if let Ok(msg) = b_msg {
                    if let Ok(json) = serde_json::to_string(&msg) {
                        if socket.send(axum::extract::ws::Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                }
            }
        }
    }
}

async fn handle_client_message(msg: &ControlMessage, host_state: &HostState) {
    match msg.domain {
        Domain::Parameters => match msg.action.as_str() {
            "get_tree" => {
                let pkgs = host_state
                    .packages
                    .lock()
                    .map(|g| g.clone())
                    .unwrap_or_default();
                let resp = ControlMessage::new(
                    Domain::Parameters,
                    "tree_response",
                    serde_json::to_value(ParametersTreeResponse { packages: pkgs })
                        .unwrap_or_default(),
                );
                let _ = host_state.tx_broadcast.send(resp);
            }
            "apply_params" => {
                let mut applied_count = 0;
                if let Ok(req) =
                    serde_json::from_value::<ApplyParametersRequest>(msg.payload.clone())
                {
                    if !req.packages.is_empty() {
                        if let Ok(mut pkgs) = host_state.packages.lock() {
                            applied_count = req.packages.len();
                            *pkgs = req.packages;
                            for p in pkgs.iter_mut() {
                                p.apply_all();
                            }
                            let resp = ControlMessage::new(
                                Domain::Parameters,
                                "tree_response",
                                serde_json::to_value(ParametersTreeResponse {
                                    packages: pkgs.clone(),
                                })
                                .unwrap_or_default(),
                            );
                            let _ = host_state.tx_broadcast.send(resp);
                        }
                    } else if let Ok(mut pkgs) = host_state.packages.lock() {
                        for p in pkgs.iter_mut() {
                            applied_count += p.count_modified();
                            p.apply_all();
                        }
                        let resp = ControlMessage::new(
                            Domain::Parameters,
                            "tree_response",
                            serde_json::to_value(ParametersTreeResponse {
                                packages: pkgs.clone(),
                            })
                            .unwrap_or_default(),
                        );
                        let _ = host_state.tx_broadcast.send(resp);
                    }
                } else if let Ok(mut pkgs) = host_state.packages.lock() {
                    for p in pkgs.iter_mut() {
                        applied_count += p.count_modified();
                        p.apply_all();
                    }
                    let resp = ControlMessage::new(
                        Domain::Parameters,
                        "tree_response",
                        serde_json::to_value(ParametersTreeResponse {
                            packages: pkgs.clone(),
                        })
                        .unwrap_or_default(),
                    );
                    let _ = host_state.tx_broadcast.send(resp);
                }
                let resp = ControlMessage::new(
                    Domain::Parameters,
                    "apply_response",
                    serde_json::to_value(ApplyParametersResponse { applied_count })
                        .unwrap_or_default(),
                );
                let _ = host_state.tx_broadcast.send(resp);
            }
            _ => {}
        },
        Domain::Launchfiles => match msg.action.as_str() {
            "get_list" => {
                let launch_pkgs = host_state
                    .launch_packages
                    .lock()
                    .map(|g| g.clone())
                    .unwrap_or_default();
                let resp = ControlMessage::new(
                    Domain::Launchfiles,
                    "list_response",
                    serde_json::to_value(LaunchfilesListResponse {
                        packages: launch_pkgs,
                    })
                    .unwrap_or_default(),
                );
                let _ = host_state.tx_broadcast.send(resp);
            }
            "start" | "stop" => {
                if let Ok(req) =
                    serde_json::from_value::<LaunchfileActionRequest>(msg.payload.clone())
                {
                    let is_start = msg.action == "start" || req.action == "start";
                    let mut found = false;
                    let pid = if is_start {
                        let millis = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis();
                        Some(1001 + (millis % 8000) as u32)
                    } else {
                        None
                    };

                    if let Ok(mut pkgs) = host_state.launch_packages.lock() {
                        for pkg in pkgs.iter_mut() {
                            if pkg.name == req.package {
                                for f in &mut pkg.launch_files {
                                    if f.name == req.filename {
                                        f.status = if is_start {
                                            LaunchStatus::Running
                                        } else {
                                            LaunchStatus::Stopped
                                        };
                                        found = true;
                                        break;
                                    }
                                }
                            }
                        }
                        if found {
                            let notif = ControlMessage::new(
                                Domain::Launchfiles,
                                "status",
                                serde_json::to_value(LaunchfileStatusNotification {
                                    package: req.package,
                                    filename: req.filename,
                                    running: is_start,
                                    pid,
                                })
                                .unwrap_or_default(),
                            );
                            let _ = host_state.tx_broadcast.send(notif);

                            let list_resp = ControlMessage::new(
                                Domain::Launchfiles,
                                "list_response",
                                serde_json::to_value(LaunchfilesListResponse {
                                    packages: pkgs.clone(),
                                })
                                .unwrap_or_default(),
                            );
                            let _ = host_state.tx_broadcast.send(list_resp);
                        }
                    }
                }
            }
            _ => {}
        },
        Domain::Mission => {
            match msg.action.as_str() {
                "get_data" => {
                    let missions = host_state
                        .mission_collection
                        .lock()
                        .map(|g| g.clone())
                        .unwrap_or_default();
                    let resp = ControlMessage::new(
                        Domain::Mission,
                        "data_response",
                        serde_json::to_value(MissionDataResponse {
                            collection: missions,
                        })
                        .unwrap_or_default(),
                    );
                    let _ = host_state.tx_broadcast.send(resp);
                }
                "list_files" => {
                    let req: ListFilesRequest = serde_json::from_value(msg.payload.clone())
                        .unwrap_or(ListFilesRequest {
                            path: ".".to_string(),
                        });
                    let raw_path = if req.path.trim().is_empty() || req.path == "." {
                        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
                    } else {
                        std::path::PathBuf::from(&req.path)
                    };
                    let target_path = raw_path.canonicalize().unwrap_or(raw_path);

                    let mut entries = Vec::new();
                    if let Ok(read_dir) = std::fs::read_dir(&target_path) {
                        for entry in read_dir.flatten() {
                            let file_type = entry.file_type();
                            let is_dir = file_type.as_ref().map(|t| t.is_dir()).unwrap_or(false);
                            let name = entry.file_name().to_string_lossy().to_string();
                            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);

                            if is_dir || name.ends_with(".yaml") || name.ends_with(".yml") {
                                entries.push(RemoteFileEntry { name, is_dir, size });
                            }
                        }
                    }

                    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
                        (true, false) => std::cmp::Ordering::Less,
                        (false, true) => std::cmp::Ordering::Greater,
                        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                    });

                    let parent_path = target_path
                        .parent()
                        .map(|p| p.to_string_lossy().to_string());
                    let current_path = target_path.to_string_lossy().to_string();

                    let resp = ControlMessage::new(
                        Domain::Mission,
                        "list_files_response",
                        serde_json::to_value(ListFilesResponse {
                            current_path,
                            parent_path,
                            entries,
                        })
                        .unwrap_or_default(),
                    );
                    let _ = host_state.tx_broadcast.send(resp);
                }
                "load_file" => {
                    if let Ok(req) =
                        serde_json::from_value::<LoadMissionFileRequest>(msg.payload.clone())
                    {
                        let path = req.path;
                        match crate::editor::models::MissionData::load_from_file(&path) {
                            Ok(loaded_data) => {
                                if let Ok(mut g) = host_state.mission_collection.lock() {
                                    if let Some(active) = g.active_data_mut() {
                                        *active = loaded_data.clone();
                                    }
                                    let _ = g.save_to_file("mission_sets.json");
                                }
                                let resp = ControlMessage::new(
                                    Domain::Mission,
                                    "load_file_response",
                                    serde_json::to_value(LoadMissionFileResponse {
                                        path: path.clone(),
                                        success: true,
                                        data: Some(loaded_data),
                                        error: None,
                                    })
                                    .unwrap_or_default(),
                                );
                                let _ = host_state.tx_broadcast.send(resp);
                            }
                            Err(e) => {
                                let resp = ControlMessage::new(
                                    Domain::Mission,
                                    "load_file_response",
                                    serde_json::to_value(LoadMissionFileResponse {
                                        path: path.clone(),
                                        success: false,
                                        data: None,
                                        error: Some(format!(
                                            "Erro ao ler '{}' no host: {}",
                                            path, e
                                        )),
                                    })
                                    .unwrap_or_default(),
                                );
                                let _ = host_state.tx_broadcast.send(resp);
                            }
                        }
                    }
                }
                "save_file" => {
                    if let Ok(req) =
                        serde_json::from_value::<SaveMissionFileRequest>(msg.payload.clone())
                    {
                        let path = req.path;
                        let mut success = true;
                        let mut err_msg = None;

                        if let Err(e) = req.data.save_to_file(&path) {
                            success = false;
                            err_msg = Some(format!("Erro ao salvar no host: {}", e));
                        } else if let Ok(mut g) = host_state.mission_collection.lock() {
                            if let Some(active) = g.active_data_mut() {
                                *active = req.data.clone();
                            }
                            let _ = g.save_to_file("mission_sets.json");
                        }

                        let resp = ControlMessage::new(
                            Domain::Mission,
                            "save_file_response",
                            serde_json::to_value(SaveMissionFileResponse {
                                path: path.clone(),
                                success,
                                error: err_msg,
                            })
                            .unwrap_or_default(),
                        );
                        let _ = host_state.tx_broadcast.send(resp);

                        let missions = host_state
                            .mission_collection
                            .lock()
                            .map(|g| g.clone())
                            .unwrap_or_default();
                        let notif = ControlMessage::new(
                            Domain::Mission,
                            "data_updated",
                            serde_json::to_value(MissionDataResponse {
                                collection: missions,
                            })
                            .unwrap_or_default(),
                        );
                        let _ = host_state.tx_broadcast.send(notif);
                    }
                }
                "save_data" => {
                    if let Ok(req) =
                        serde_json::from_value::<SaveMissionDataRequest>(msg.payload.clone())
                    {
                        let target = req
                            .target_path
                            .unwrap_or_else(|| "mission_points.yaml".to_string());
                        let mut success = true;
                        let mut err_msg = "Salvo com sucesso".to_string();

                        if let Some(data) = req.collection.active_data() {
                            if let Err(e) = data.save_to_file(&target) {
                                success = false;
                                err_msg = format!("Erro ao salvar no host: {}", e);
                            }
                        }

                        let _ = req.collection.save_to_file("mission_sets.json");

                        if let Ok(mut g) = host_state.mission_collection.lock() {
                            *g = req.collection.clone();
                        }

                        let resp = ControlMessage::new(
                            Domain::Mission,
                            "save_data_response",
                            serde_json::to_value(SaveMissionDataResponse {
                                success,
                                message: err_msg,
                            })
                            .unwrap_or_default(),
                        );
                        let _ = host_state.tx_broadcast.send(resp);

                        // Notifica todos os visualizadores conectados sobre a atualização
                        let notif = ControlMessage::new(
                            Domain::Mission,
                            "data_updated",
                            serde_json::to_value(MissionDataResponse {
                                collection: req.collection,
                            })
                            .unwrap_or_default(),
                        );
                        let _ = host_state.tx_broadcast.send(notif);
                    }
                }
                _ => {}
            }
        }
        Domain::Terminal => {
            match msg.action.as_str() {
                "data" => {
                    if let Ok(data_msg) =
                        serde_json::from_value::<TerminalDataMessage>(msg.payload.clone())
                    {
                        if data_msg.is_input {
                            // Ecoa comando e gera saída de simulação/resposta no host
                            let cmd = data_msg.text.clone();
                            let output_line = format!("[INFO] [host]: Executado '{}'", cmd);

                            if let Ok(mut tabs) = host_state.terminal_tabs.lock() {
                                if let Some(tab) =
                                    tabs.iter_mut().find(|t| t.id == data_msg.terminal_id)
                                {
                                    tab.output_lines.push(format!("$ {}", cmd));
                                    tab.output_lines.push(output_line.clone());
                                }
                            }

                            // Transmite a linha de log resultante para todos os clientes
                            let notif = ControlMessage::new(
                                Domain::Terminal,
                                "data",
                                serde_json::to_value(TerminalDataMessage {
                                    terminal_id: data_msg.terminal_id,
                                    text: format!("$ {}", cmd),
                                    is_input: false,
                                })
                                .unwrap_or_default(),
                            );
                            let _ = host_state.tx_broadcast.send(notif);

                            let notif_res = ControlMessage::new(
                                Domain::Terminal,
                                "data",
                                serde_json::to_value(TerminalDataMessage {
                                    terminal_id: data_msg.terminal_id,
                                    text: output_line,
                                    is_input: false,
                                })
                                .unwrap_or_default(),
                            );
                            let _ = host_state.tx_broadcast.send(notif_res);
                        }
                    }
                }
                "create" => {
                    if let Ok(req) =
                        serde_json::from_value::<TerminalCreateRequest>(msg.payload.clone())
                    {
                        let next_id = {
                            if let Ok(mut tabs) = host_state.terminal_tabs.lock() {
                                let id = tabs.len() + 1;
                                tabs.push(TerminalTab::new(
                                    id,
                                    req.title,
                                    vec!["[INFO] Terminal interativo remoto criado."],
                                ));
                                id
                            } else {
                                1
                            }
                        };
                        let _ = next_id;
                        let tabs = host_state
                            .terminal_tabs
                            .lock()
                            .map(|g| g.clone())
                            .unwrap_or_default();
                        let notif = ControlMessage::new(
                            Domain::Terminal,
                            "list_terminals",
                            serde_json::to_value(TerminalsListResponse { tabs })
                                .unwrap_or_default(),
                        );
                        let _ = host_state.tx_broadcast.send(notif);
                    }
                }
                "close" => {
                    if let Ok(req) =
                        serde_json::from_value::<TerminalCloseRequest>(msg.payload.clone())
                    {
                        if let Ok(mut tabs) = host_state.terminal_tabs.lock() {
                            tabs.retain(|t| t.id != req.terminal_id);
                        }
                        let tabs = host_state
                            .terminal_tabs
                            .lock()
                            .map(|g| g.clone())
                            .unwrap_or_default();
                        let notif = ControlMessage::new(
                            Domain::Terminal,
                            "list_terminals",
                            serde_json::to_value(TerminalsListResponse { tabs })
                                .unwrap_or_default(),
                        );
                        let _ = host_state.tx_broadcast.send(notif);
                    }
                }
                _ => {}
            }
        }
        Domain::System => {
            if msg.action == "ping" {
                let pong = ControlMessage::new(
                    Domain::System,
                    "pong",
                    serde_json::json!({ "pong": true }),
                );
                let _ = host_state.tx_broadcast.send(pong);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{header, Request};
    use tower::ServiceExt;

    #[test]
    fn test_index_html_embedded() {
        assert!(INDEX_HTML.contains("Rewire Viewer"));
        assert!(INDEX_HTML.contains("the_canvas_id"));
    }

    #[tokio::test]
    async fn test_grpc_route_matching() {
        let proxy_state = ProxyState {
            client: reqwest::Client::new(),
            target_base: "http://127.0.0.1:9876".to_string(),
        };
        let (tx, _) = broadcast::channel(16);
        let host_state = HostState::new(tx);

        let app = Router::new()
            .route("/", get(index_handler))
            .route("/index.html", get(index_handler))
            .route(
                "/ws/control",
                get({
                    let state = host_state.clone();
                    move |ws: WebSocketUpgrade| ws_control_handler(ws, state)
                }),
            )
            .route("/proxy", any(proxy_handler))
            .route("/proxy/{*path}", any(proxy_handler))
            .route("/rerun.{*path}", any(proxy_handler))
            .fallback_service(ServeDir::new("web"))
            .layer(axum::middleware::from_fn(cache_viewer_assets_middleware))
            .with_state(proxy_state);

        let req = Request::builder()
            .method("POST")
            .uri("/rerun.sdk_comms.v1alpha1.MessageProxyService/ReadMessages")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_ne!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    #[test]
    fn test_should_cache_viewer_asset() {
        assert!(should_cache_viewer_asset("/rewire_viewer.js"));
        assert!(should_cache_viewer_asset("/rewire_viewer_bg.wasm"));
        assert!(!should_cache_viewer_asset("/index.html"));
        assert!(!should_cache_viewer_asset("/proxy"));
    }

    #[tokio::test]
    async fn test_cache_header_added_for_viewer_assets() {
        let temp_dir =
            std::env::temp_dir().join(format!("test_web_cache_headers_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        std::fs::write(temp_dir.join("rewire_viewer.js"), "console.log('ok');").unwrap();

        let app = Router::new()
            .fallback_service(ServeDir::new(&temp_dir))
            .layer(axum::middleware::from_fn(cache_viewer_assets_middleware));

        let req = Request::builder()
            .method("GET")
            .uri("/rewire_viewer.js")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            VIEWER_ASSET_CACHE_CONTROL
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_resolve_web_directory_custom_and_fallback() {
        let temp_dir = std::env::temp_dir().join(format!("test_web_dir_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let resolved = resolve_web_directory(Some(temp_dir.to_str().unwrap()));
        assert_eq!(resolved, temp_dir);

        let _ = std::fs::remove_dir_all(temp_dir);

        // Sem caminho customizado, deve resolver para um caminho (ex: ./web ou fallback)
        let default_resolved = resolve_web_directory(None);
        assert!(!default_resolved.as_os_str().is_empty());
    }
}
