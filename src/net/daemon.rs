use axum::{
    body::Body,
    extract::{
        ws::{WebSocket, WebSocketUpgrade},
        Request, State,
    },
    http::{header, HeaderValue, StatusCode},
    middleware::Next,
    response::Response,
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
use crate::panels::terminals::pty_manager::PtyManager;

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
    pub workspace_path: Arc<Mutex<String>>,
    pub launch_packages: Arc<Mutex<Vec<LaunchPackage>>>,
    pub launch_target_mode: Arc<Mutex<crate::config::LaunchTargetMode>>,
    pub docker_container_name: Arc<Mutex<String>>,
    pub docker_entrypoint: Arc<Mutex<String>>,
    pub mission_collection: Arc<Mutex<MissionSetCollection>>,
    pub pty_manager: Arc<PtyManager>,
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

        let default_ws = crate::config::default_workspace_path();

        let scanned_pkgs =
            crate::panels::parameters::workspace::scan_workspace_packages(&default_ws);
        let initial_packages = if !scanned_pkgs.is_empty() {
            re_log::info!(
                "Daemon: Carregados {} pacotes ROS da workspace '{}'",
                scanned_pkgs.len(),
                default_ws
            );
            scanned_pkgs
        } else {
            Vec::new()
        };

        let scanned_launches =
            crate::panels::launchfiles::workspace::scan_workspace_launchfiles(&default_ws);
        let initial_launches = if !scanned_launches.is_empty() {
            re_log::info!(
                "Daemon: Carregados {} pacotes com launchfiles da workspace '{}'",
                scanned_launches.len(),
                default_ws
            );
            scanned_launches
        } else {
            Vec::new()
        };

        let default_cfg = crate::config::AppConfig::default();

        Self {
            packages: Arc::new(Mutex::new(initial_packages)),
            workspace_path: Arc::new(Mutex::new(default_ws)),
            launch_packages: Arc::new(Mutex::new(initial_launches)),
            launch_target_mode: Arc::new(Mutex::new(default_cfg.launch_target_mode)),
            docker_container_name: Arc::new(Mutex::new(default_cfg.docker_container_name)),
            docker_entrypoint: Arc::new(Mutex::new(default_cfg.docker_entrypoint)),
            mission_collection: Arc::new(Mutex::new(initial_missions)),
            pty_manager: Arc::new(PtyManager::new()),
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

    // Sincronização inicial de status de launchfiles
    sync_launch_status(&host_state).await;

    // Background polling: monitora e sincroniza processos de launchfiles a cada 1.5s
    let poll_state = host_state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(1500));
        loop {
            interval.tick().await;
            sync_launch_status(&poll_state).await;
        }
    });

    let web_dir = resolve_web_directory(custom_web_dir.as_deref());
    re_log::info!("Servindo interface web a partir de: {:?}", web_dir);

    let proxy_state = ProxyState {
        client: reqwest::Client::new(),
        target_base: normalize_proxy_target(&connect_endpoint),
    };

    let app = Router::new()
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
            let mut response_builder = axum::http::Response::builder().status(status);

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

async fn ws_control_handler(ws: WebSocketUpgrade, host_state: HostState) -> Response {
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
        let sessions = host_state.pty_manager.list_sessions();

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
                "list_sync",
                serde_json::to_value(TerminalListSyncResponse { sessions }).unwrap_or_default(),
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

async fn sync_launch_status(host_state: &HostState) {
    let mode = host_state
        .launch_target_mode
        .lock()
        .map(|m| *m)
        .unwrap_or_default();
    let container = host_state
        .docker_container_name
        .lock()
        .map(|c| c.clone())
        .unwrap_or_default();

    let running = crate::panels::launchfiles::process_manager::scan_running_launches_async(
        mode,
        &container,
    )
    .await;

    let mut changed = false;
    let mut changed_notifications = Vec::new();

    if let Ok(mut pkgs) = host_state.launch_packages.lock() {
        for pkg in pkgs.iter_mut() {
            fn check_file(
                pkg_name: &str,
                file: &mut crate::panels::launchfiles::LaunchFile,
                running_list: &[crate::panels::launchfiles::process_manager::RunningLaunch],
                changed: &mut bool,
                notifs: &mut Vec<LaunchfileStatusNotification>,
            ) {
                let is_running = running_list.iter().any(|rl| {
                    rl.package == pkg_name
                        && (rl.filename == file.name
                            || rl.filename.trim_end_matches(".xml")
                                == file.name.trim_end_matches(".xml"))
                });
                let target_status = if is_running {
                    LaunchStatus::Running
                } else {
                    LaunchStatus::Stopped
                };

                if file.status != target_status {
                    file.status = target_status;
                    *changed = true;
                    let pid = if is_running {
                        running_list
                            .iter()
                            .find(|rl| {
                                rl.package == pkg_name
                                    && (rl.filename == file.name
                                        || rl.filename.trim_end_matches(".xml")
                                            == file.name.trim_end_matches(".xml"))
                            })
                            .map(|rl| rl.pid)
                    } else {
                        None
                    };
                    notifs.push(LaunchfileStatusNotification {
                        package: pkg_name.to_string(),
                        filename: file.name.clone(),
                        running: is_running,
                        pid,
                    });
                }

                for child in &mut file.children {
                    check_file(pkg_name, child, running_list, changed, notifs);
                }
            }

            for file in &mut pkg.launch_files {
                check_file(
                    &pkg.name,
                    file,
                    &running,
                    &mut changed,
                    &mut changed_notifications,
                );
            }
        }

        if changed {
            for notif in changed_notifications {
                let msg = ControlMessage::new(
                    Domain::Launchfiles,
                    "status",
                    serde_json::to_value(notif).unwrap_or_default(),
                );
                let _ = host_state.tx_broadcast.send(msg);
            }

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
            "refresh" => {
                let ws_val = host_state
                    .workspace_path
                    .lock()
                    .map(|g| g.clone())
                    .unwrap_or_else(|_| crate::config::default_workspace_path());
                let scanned = crate::panels::parameters::workspace::scan_workspace_packages(&ws_val);
                if !scanned.is_empty() {
                    if let Ok(mut pkgs) = host_state.packages.lock() {
                        *pkgs = scanned.clone();
                    }
                    re_log::info!(
                        "Daemon: Workspace recarregada de '{}' ({} pacotes)",
                        ws_val,
                        scanned.len()
                    );
                    let resp = ControlMessage::new(
                        Domain::Parameters,
                        "tree_response",
                        serde_json::to_value(ParametersTreeResponse {
                            packages: scanned,
                        })
                        .unwrap_or_default(),
                    );
                    let _ = host_state.tx_broadcast.send(resp);
                } else if let Ok(pkgs) = host_state.packages.lock() {
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
            }
            "apply_params" => {
                let mut applied_count = 0;
                if let Ok(req) =
                    serde_json::from_value::<ApplyParametersRequest>(msg.payload.clone())
                {
                    if !req.packages.is_empty() {
                        if let Ok(mut pkgs) = host_state.packages.lock() {
                            for req_pkg in &req.packages {
                                if let Some(host_pkg) = pkgs.iter_mut().find(|p| p.name == req_pkg.name) {
                                    for req_file in &req_pkg.files {
                                        if let Some(host_file) = host_pkg.files.iter_mut().find(|f| f.filename == req_file.filename) {
                                            if !req_file.path.is_empty() {
                                                host_file.path = req_file.path.clone();
                                            }
                                            host_file.scopes = req_file.scopes.clone();
                                        } else {
                                            host_pkg.files.push(req_file.clone());
                                        }
                                    }
                                } else {
                                    pkgs.push(req_pkg.clone());
                                }
                            }
                            if pkgs.is_empty() {
                                *pkgs = req.packages;
                            }
                            match crate::panels::parameters::workspace::save_modified_packages(&mut pkgs) {
                                Ok(saved) => {
                                    applied_count = saved;
                                    re_log::info!("Daemon: Salvos com sucesso {} arquivos de parâmetros no disco.", saved);
                                }
                                Err(err) => {
                                    re_log::error!("Daemon: Erro ao salvar parâmetros: {}", err);
                                }
                            }
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
                        match crate::panels::parameters::workspace::save_modified_packages(&mut pkgs) {
                            Ok(saved) => {
                                applied_count = saved;
                                re_log::info!("Daemon: Salvos com sucesso {} arquivos de parâmetros no disco.", saved);
                            }
                            Err(err) => {
                                re_log::error!("Daemon: Erro ao salvar parâmetros: {}", err);
                            }
                        }
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
                    match crate::panels::parameters::workspace::save_modified_packages(&mut pkgs) {
                        Ok(saved) => {
                            applied_count = saved;
                            re_log::info!("Daemon: Salvos com sucesso {} arquivos de parâmetros no disco.", saved);
                        }
                        Err(err) => {
                            re_log::error!("Daemon: Erro ao salvar parâmetros: {}", err);
                        }
                    }
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
                let resp = ControlMessage::new(
                    Domain::Parameters,
                    "apply_response",
                    serde_json::to_value(ApplyParametersResponse { applied_count })
                        .unwrap_or_default(),
                );
                let _ = host_state.tx_broadcast.send(resp);
            }
            "set_workspace" => {
                if let Some(ws_val) = msg.payload.get("workspace").and_then(|v| v.as_str()) {
                    if let Ok(mut ws_lock) = host_state.workspace_path.lock() {
                        *ws_lock = ws_val.to_string();
                    }
                    let scanned = crate::panels::parameters::workspace::scan_workspace_packages(ws_val);
                    if !scanned.is_empty() {
                        if let Ok(mut pkgs) = host_state.packages.lock() {
                            *pkgs = scanned.clone();
                        }
                        re_log::info!(
                            "Daemon: Workspace atualizada para '{}' ({} pacotes)",
                            ws_val,
                            scanned.len()
                        );
                        let resp = ControlMessage::new(
                            Domain::Parameters,
                            "tree_response",
                            serde_json::to_value(ParametersTreeResponse {
                                packages: scanned,
                            })
                            .unwrap_or_default(),
                        );
                        let _ = host_state.tx_broadcast.send(resp);
                    }

                    let scanned_launches =
                        crate::panels::launchfiles::workspace::scan_workspace_launchfiles(ws_val);
                    if let Ok(mut lpkgs) = host_state.launch_packages.lock() {
                        *lpkgs = scanned_launches.clone();
                    }
                    let launch_resp = ControlMessage::new(
                        Domain::Launchfiles,
                        "list_response",
                        serde_json::to_value(LaunchfilesListResponse {
                            packages: scanned_launches,
                        })
                        .unwrap_or_default(),
                    );
                    let _ = host_state.tx_broadcast.send(launch_resp);
                }
            }
            _ => {}
        },
        Domain::Launchfiles => match msg.action.as_str() {
            "get_list" => {
                sync_launch_status(host_state).await;
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
            "set_config" => {
                if let Ok(req) =
                    serde_json::from_value::<SetLaunchConfigRequest>(msg.payload.clone())
                {
                    if let Ok(mut mode_lock) = host_state.launch_target_mode.lock() {
                        *mode_lock = req.target_mode;
                    }
                    if let Ok(mut container_lock) = host_state.docker_container_name.lock() {
                        *container_lock = req.container_name;
                    }
                    if let Ok(mut entry_lock) = host_state.docker_entrypoint.lock() {
                        *entry_lock = req.entrypoint;
                    }
                    sync_launch_status(host_state).await;
                }
            }
            "start" | "stop" => {
                if let Ok(req) =
                    serde_json::from_value::<LaunchfileActionRequest>(msg.payload.clone())
                {
                    if let Some(target_mode) = req.target_mode {
                        if let Ok(mut mode_lock) = host_state.launch_target_mode.lock() {
                            *mode_lock = target_mode;
                        }
                    }
                    if let Some(container) = req.container_name {
                        if let Ok(mut c_lock) = host_state.docker_container_name.lock() {
                            *c_lock = container;
                        }
                    }
                    if let Some(entrypoint) = req.entrypoint {
                        if let Ok(mut e_lock) = host_state.docker_entrypoint.lock() {
                            *e_lock = entrypoint;
                        }
                    }

                    let mode = host_state
                        .launch_target_mode
                        .lock()
                        .map(|m| *m)
                        .unwrap_or_default();
                    let container = host_state
                        .docker_container_name
                        .lock()
                        .map(|c| c.clone())
                        .unwrap_or_default();
                    let entrypoint = host_state
                        .docker_entrypoint
                        .lock()
                        .map(|e| e.clone())
                        .unwrap_or_default();

                    let is_start = msg.action == "start" || req.action == "start";

                    if is_start {
                        let launch_args = req.args.as_deref().unwrap_or(&[]);
                        re_log::info!(
                            "Daemon: Iniciando launchfile '{}/{}' (modo: {:?}, container: '{}', args: {:?})",
                            req.package,
                            req.filename,
                            mode,
                            container,
                            launch_args
                        );
                        if let Err(e) = crate::panels::launchfiles::process_manager::start_launch_async(
                            mode,
                            &container,
                            &entrypoint,
                            &req.package,
                            &req.filename,
                            launch_args,
                        )
                        .await
                        {
                            re_log::error!("Daemon: Erro ao iniciar launchfile: {}", e);
                        }
                    } else {
                        re_log::info!(
                            "Daemon: Parando launchfile '{}/{}' (modo: {:?}, container: '{}')",
                            req.package,
                            req.filename,
                            mode,
                            container
                        );
                        if let Err(e) = crate::panels::launchfiles::process_manager::stop_launch_async(
                            mode,
                            &container,
                            &req.package,
                            &req.filename,
                        )
                        .await
                        {
                            re_log::error!("Daemon: Erro ao parar launchfile: {}", e);
                        }
                    }

                    // Sincroniza o status dos processos atualizados e notifica os clientes
                    sync_launch_status(host_state).await;
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
                "get_list" => {
                    let sessions = host_state.pty_manager.list_sessions();
                    let resp = ControlMessage::new(
                        Domain::Terminal,
                        "list_sync",
                        serde_json::to_value(TerminalListSyncResponse { sessions })
                            .unwrap_or_default(),
                    );
                    let _ = host_state.tx_broadcast.send(resp);
                }
                "create" => {
                    if let Ok(req) =
                        serde_json::from_value::<TerminalCreateRequest>(msg.payload.clone())
                    {
                        let container = host_state
                            .docker_container_name
                            .lock()
                            .map(|c| c.clone())
                            .unwrap_or_default();
                        let entrypoint = host_state
                            .docker_entrypoint
                            .lock()
                            .map(|e| e.clone())
                            .unwrap_or_default();
                        let ws_path = host_state
                            .workspace_path
                            .lock()
                            .map(|w| w.clone())
                            .unwrap_or_default();

                        let tx_out = host_state.tx_broadcast.clone();
                        let tx_exit = host_state.tx_broadcast.clone();
                        let title_for_exit = req.title.clone();
                        let target_for_exit = req.target;

                        match host_state.pty_manager.create_session(
                            req.id.clone(),
                            req.title.clone(),
                            req.target,
                            req.cols,
                            req.rows,
                            &container,
                            &entrypoint,
                            &ws_path,
                            move |sid, data| {
                                let ts = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_millis() as u64)
                                    .unwrap_or(0);
                                let out_msg = ControlMessage::new(
                                    Domain::Terminal,
                                    "output",
                                    serde_json::to_value(TerminalOutputMessage {
                                        id: sid,
                                        data,
                                        timestamp_ms: ts,
                                    })
                                    .unwrap_or_default(),
                                );
                                let _ = tx_out.send(out_msg);
                            },
                            move |sid| {
                                let status_msg = ControlMessage::new(
                                    Domain::Terminal,
                                    "status",
                                    serde_json::to_value(TerminalStatusMessage {
                                        id: sid,
                                        title: title_for_exit.clone(),
                                        target: target_for_exit,
                                        is_running: false,
                                    })
                                    .unwrap_or_default(),
                                );
                                let _ = tx_exit.send(status_msg);
                            },
                        ) {
                            Ok(()) => {
                                let status_msg = ControlMessage::new(
                                    Domain::Terminal,
                                    "status",
                                    serde_json::to_value(TerminalStatusMessage {
                                        id: req.id,
                                        title: req.title,
                                        target: req.target,
                                        is_running: true,
                                    })
                                    .unwrap_or_default(),
                                );
                                let _ = host_state.tx_broadcast.send(status_msg);
                            }
                            Err(err) => {
                                re_log::error!("Daemon: Erro ao criar sessão PTY: {}", err);
                                let err_bytes = format!(
                                    "\r\n\x1b[31m[Daemon: Falha ao criar PTY: {}]\x1b[0m\r\n",
                                    err
                                )
                                .into_bytes();
                                let out_msg = ControlMessage::new(
                                    Domain::Terminal,
                                    "output",
                                    serde_json::to_value(TerminalOutputMessage {
                                        id: req.id.clone(),
                                        data: err_bytes,
                                        timestamp_ms: 0,
                                    })
                                    .unwrap_or_default(),
                                );
                                let _ = host_state.tx_broadcast.send(out_msg);
                                let status_msg = ControlMessage::new(
                                    Domain::Terminal,
                                    "status",
                                    serde_json::to_value(TerminalStatusMessage {
                                        id: req.id,
                                        title: req.title,
                                        target: req.target,
                                        is_running: false,
                                    })
                                    .unwrap_or_default(),
                                );
                                let _ = host_state.tx_broadcast.send(status_msg);
                            }
                        }
                    }
                }
                "input" => {
                    if let Ok(req) =
                        serde_json::from_value::<TerminalInputMessage>(msg.payload.clone())
                    {
                        if let Err(e) = host_state.pty_manager.write_input(&req.id, &req.data) {
                            re_log::warn!("Daemon: Erro ao escrever input no PTY '{}': {}", req.id, e);
                        }
                    }
                }
                "resize" => {
                    if let Ok(req) =
                        serde_json::from_value::<TerminalResizeMessage>(msg.payload.clone())
                    {
                        let _ = host_state
                            .pty_manager
                            .resize_session(&req.id, req.cols, req.rows);
                    }
                }
                "close" => {
                    if let Ok(req) =
                        serde_json::from_value::<TerminalCloseRequest>(msg.payload.clone())
                    {
                        host_state.pty_manager.close_session(&req.id);
                        let closed_notif = ControlMessage::new(
                            Domain::Terminal,
                            "closed",
                            serde_json::to_value(TerminalCloseRequest { id: req.id })
                                .unwrap_or_default(),
                        );
                        let _ = host_state.tx_broadcast.send(closed_notif);
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
    use crate::panels::parameters::ParameterValue;
    use axum::http::{header, Request};
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_grpc_route_matching() {
        let proxy_state = ProxyState {
            client: reqwest::Client::new(),
            target_base: "http://127.0.0.1:9876".to_string(),
        };
        let (tx, _) = broadcast::channel(16);
        let host_state = HostState::new(tx);

        let app = Router::new()
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
            .fallback_service(ServeDir::new("web").append_index_html_on_directories(true))
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

    #[tokio::test]
    async fn test_root_serves_index_from_filesystem() {
        let temp_dir = std::env::temp_dir().join(format!("test_root_index_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        std::fs::write(temp_dir.join("index.html"), "<html>from-disk</html>").unwrap();

        let app = Router::new()
            .fallback_service(ServeDir::new(&temp_dir).append_index_html_on_directories(true));

        let req = Request::builder()
            .method("GET")
            .uri("/")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body_str = std::str::from_utf8(&body).unwrap();
        assert!(body_str.contains("from-disk"));

        let _ = std::fs::remove_dir_all(temp_dir);
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

        let blueprint_candidate = default_resolved.join("default_blueprint.rbl");
        if blueprint_candidate.exists() {
            assert!(blueprint_candidate.is_file());
        }
    }

    #[tokio::test]
    async fn test_daemon_handle_apply_params_and_refresh() {
        let temp_dir = std::env::temp_dir().join(format!("test_daemon_params_{}", std::process::id()));
        let pkg_dir = temp_dir.join("test_pkg");
        let config_dir = pkg_dir.join("config");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::write(pkg_dir.join("package.xml"), "<package><name>test_pkg</name></package>").unwrap();
        let param_file = config_dir.join("params.yaml");
        std::fs::write(&param_file, "test_node:\n  ros__parameters:\n    linear_speed: 1.0\n").unwrap();

        let (tx, mut rx) = broadcast::channel(16);
        let host_state = HostState::new(tx);
        *host_state.workspace_path.lock().unwrap() = temp_dir.to_string_lossy().to_string();

        // 1. Test Refresh
        let refresh_msg = ControlMessage::new(Domain::Parameters, "refresh", serde_json::Value::Null);
        handle_client_message(&refresh_msg, &host_state).await;

        let broadcasted = rx.recv().await.unwrap();
        assert_eq!(broadcasted.domain, Domain::Parameters);
        assert_eq!(broadcasted.action, "tree_response");
        let resp: ParametersTreeResponse = serde_json::from_value(broadcasted.payload).unwrap();
        assert_eq!(resp.packages.len(), 1);
        assert_eq!(resp.packages[0].name, "test_pkg");

        // 2. Test Apply Params
        let mut modified_packages = resp.packages.clone();
        let param = &mut modified_packages[0].files[0].scopes[0].parameters[0];
        param.edited_value = ParameterValue::Float(2.5);

        let apply_msg = ControlMessage::new(
            Domain::Parameters,
            "apply_params",
            serde_json::to_value(ApplyParametersRequest {
                package: None,
                packages: modified_packages,
            })
            .unwrap(),
        );
        handle_client_message(&apply_msg, &host_state).await;

        // Verify the file was written to disk!
        let updated_content = std::fs::read_to_string(&param_file).unwrap();
        assert!(updated_content.contains("linear_speed: 2.5"));

        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
