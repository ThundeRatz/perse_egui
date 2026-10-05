use crate::net::protocol::ControlMessage;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[cfg(not(target_arch = "wasm32"))]
use futures_util::{SinkExt, StreamExt};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::closure::Closure;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use web_sys::{ErrorEvent, MessageEvent, WebSocket};

/// Cliente WebSocket para interação entre o Visualizador (Web e Nativo) e o Host do Robô
#[derive(Clone)]
pub struct ControlClient {
    is_connected: Arc<AtomicBool>,
    status_message: Arc<Mutex<String>>,
    incoming_queue: Arc<Mutex<Vec<ControlMessage>>>,
    #[cfg(not(target_arch = "wasm32"))]
    native_tx: Arc<Mutex<Option<tokio::sync::mpsc::UnboundedSender<String>>>>,
    #[cfg(target_arch = "wasm32")]
    wasm_ws: Arc<Mutex<Option<WebSocket>>>,
}

impl Default for ControlClient {
    fn default() -> Self {
        Self {
            is_connected: Arc::new(AtomicBool::new(false)),
            status_message: Arc::new(Mutex::new("Desconectado".to_string())),
            incoming_queue: Arc::new(Mutex::new(Vec::new())),
            #[cfg(not(target_arch = "wasm32"))]
            native_tx: Arc::new(Mutex::new(None)),
            #[cfg(target_arch = "wasm32")]
            wasm_ws: Arc::new(Mutex::new(None)),
        }
    }
}

impl ControlClient {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_connected(&self) -> bool {
        self.is_connected.load(Ordering::SeqCst)
    }

    pub fn status_message(&self) -> String {
        self.status_message
            .lock()
            .map(|s| s.clone())
            .unwrap_or_else(|_| "Desconhecido".to_string())
    }

    pub fn push_incoming(&self, msg: ControlMessage) {
        if let Ok(mut queue) = self.incoming_queue.lock() {
            queue.push(msg);
        }
    }

    pub fn drain_messages(&self) -> Vec<ControlMessage> {
        if let Ok(mut queue) = self.incoming_queue.lock() {
            std::mem::take(&mut *queue)
        } else {
            Vec::new()
        }
    }

    /// URL padrão de WebSocket de controle para o ambiente atual
    pub fn default_url(endpoint: &str) -> String {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = endpoint;
            if let Some(win) = web_sys::window() {
                if let Ok(loc) = win.location().host() {
                    if !loc.is_empty() {
                        let proto = win.location().protocol().unwrap_or_default();
                        let ws_proto = if proto == "https:" { "wss://" } else { "ws://" };
                        return format!("{}{}/ws/control", ws_proto, loc);
                    }
                }
            }
            "ws://127.0.0.1:8080/ws/control".to_string()
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let clean = endpoint
                .trim_start_matches("rerun+http://")
                .trim_start_matches("rerun+https://")
                .trim_start_matches("http://")
                .trim_start_matches("https://")
                .trim_start_matches("ws://")
                .trim_start_matches("wss://");
            let host_port = clean.split('/').next().unwrap_or("127.0.0.1:8080");
            let ws_host_port = if host_port.ends_with(":9876") {
                host_port.replace(":9876", ":8080")
            } else if !host_port.contains(':') {
                format!("{}:8080", host_port)
            } else {
                host_port.to_string()
            };
            format!("ws://{}/ws/control", ws_host_port)
        }
    }

    /// Conecta ao endpoint WebSocket `/ws/control`
    pub fn connect(&self, url: &str) {
        let url_str = url.to_string();

        #[cfg(not(target_arch = "wasm32"))]
        {
            let incoming = self.incoming_queue.clone();
            let is_connected = self.is_connected.clone();
            let status_message = self.status_message.clone();
            let (tx_out, mut rx_out) = tokio::sync::mpsc::unbounded_channel::<String>();

            if let Ok(mut g) = self.native_tx.lock() {
                *g = Some(tx_out);
            }

            let runner = async move {
                if let Ok(mut st) = status_message.lock() {
                    *st = format!("Conectando a {}...", url_str);
                }
                match tokio_tungstenite::connect_async(&url_str).await {
                    Ok((mut ws_stream, _)) => {
                        is_connected.store(true, Ordering::SeqCst);
                        if let Ok(mut st) = status_message.lock() {
                            *st = format!("Conectado a {}", url_str);
                        }
                        re_log::info!("ControlClient (Nativo) conectado com sucesso a {}", url_str);

                        // Ao conectar, envia requisição de dados iniciais para sincronizar painéis
                        let initial_reqs = vec![
                            ControlMessage::new(
                                crate::net::protocol::Domain::Parameters,
                                "get_tree",
                                serde_json::Value::Null,
                            ),
                            ControlMessage::new(
                                crate::net::protocol::Domain::Launchfiles,
                                "get_list",
                                serde_json::Value::Null,
                            ),
                            ControlMessage::new(
                                crate::net::protocol::Domain::Mission,
                                "get_data",
                                serde_json::Value::Null,
                            ),
                        ];
                        for req in initial_reqs {
                            if let Ok(j) = serde_json::to_string(&req) {
                                let _ = ws_stream
                                    .send(tokio_tungstenite::tungstenite::Message::Text(j.into()))
                                    .await;
                            }
                        }

                        loop {
                            tokio::select! {
                                Some(txt) = rx_out.recv() => {
                                    use tokio_tungstenite::tungstenite::Message;
                                    if ws_stream.send(Message::Text(txt.into())).await.is_err() {
                                        break;
                                    }
                                }
                                msg = ws_stream.next() => {
                                    use tokio_tungstenite::tungstenite::Message;
                                    match msg {
                                        Some(Ok(Message::Text(txt))) => {
                                            if let Ok(ctrl_msg) = serde_json::from_str::<ControlMessage>(&txt) {
                                                if let Ok(mut q) = incoming.lock() {
                                                    q.push(ctrl_msg);
                                                }
                                            }
                                        }
                                        Some(Ok(Message::Close(_))) | None => break,
                                        _ => {}
                                    }
                                }
                            }
                        }
                        is_connected.store(false, Ordering::SeqCst);
                        if let Ok(mut st) = status_message.lock() {
                            *st = "Conexão encerrada".to_string();
                        }
                    }
                    Err(e) => {
                        is_connected.store(false, Ordering::SeqCst);
                        if let Ok(mut st) = status_message.lock() {
                            *st = format!("Falha na conexão: {}", e);
                        }
                        re_log::warn!("ControlClient falhou ao conectar em {}: {}", url_str, e);
                    }
                }
            };

            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(runner);
            } else {
                std::thread::spawn(move || {
                    if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                    {
                        rt.block_on(runner);
                    }
                });
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            if let Ok(mut st) = self.status_message.lock() {
                *st = format!("Conectando a {}...", url_str);
            }
            match WebSocket::new(&url_str) {
                Ok(ws) => {
                    let is_connected_open = self.is_connected.clone();
                    let status_msg_open = self.status_message.clone();
                    let url_for_open = url_str.clone();
                    let ws_for_open = ws.clone();

                    let onopen = Closure::<dyn FnMut()>::new(move || {
                        is_connected_open.store(true, Ordering::SeqCst);
                        if let Ok(mut st) = status_msg_open.lock() {
                            *st = format!("Conectado a {}", url_for_open);
                        }
                        re_log::info!(
                            "ControlClient (WASM) conectado com sucesso a {}",
                            url_for_open
                        );

                        // Requisitar dados iniciais dos painéis
                        let initial_reqs = vec![
                            ControlMessage::new(
                                crate::net::protocol::Domain::Parameters,
                                "get_tree",
                                serde_json::Value::Null,
                            ),
                            ControlMessage::new(
                                crate::net::protocol::Domain::Launchfiles,
                                "get_list",
                                serde_json::Value::Null,
                            ),
                            ControlMessage::new(
                                crate::net::protocol::Domain::Mission,
                                "get_data",
                                serde_json::Value::Null,
                            ),
                        ];
                        for req in initial_reqs {
                            if let Ok(j) = serde_json::to_string(&req) {
                                let _ = ws_for_open.send_with_str(&j);
                            }
                        }
                    });
                    ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
                    onopen.forget();

                    let incoming = self.incoming_queue.clone();
                    let onmessage =
                        Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
                            if let Some(txt) = e.data().as_string() {
                                if let Ok(ctrl_msg) = serde_json::from_str::<ControlMessage>(&txt) {
                                    if let Ok(mut q) = incoming.lock() {
                                        q.push(ctrl_msg);
                                    }
                                }
                            }
                        });
                    ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
                    onmessage.forget();

                    let is_connected_close = self.is_connected.clone();
                    let status_msg_close = self.status_message.clone();
                    let onclose = Closure::<dyn FnMut(web_sys::CloseEvent)>::new(move |_| {
                        is_connected_close.store(false, Ordering::SeqCst);
                        if let Ok(mut st) = status_msg_close.lock() {
                            *st = "Desconectado".to_string();
                        }
                    });
                    ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
                    onclose.forget();

                    let is_connected_err = self.is_connected.clone();
                    let status_msg_err = self.status_message.clone();
                    let onerror = Closure::<dyn FnMut(ErrorEvent)>::new(move |_| {
                        is_connected_err.store(false, Ordering::SeqCst);
                        if let Ok(mut st) = status_msg_err.lock() {
                            *st = "Erro no WebSocket".to_string();
                        }
                    });
                    ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));
                    onerror.forget();

                    if let Ok(mut g) = self.wasm_ws.lock() {
                        *g = Some(ws);
                    }
                }
                Err(e) => {
                    self.is_connected.store(false, Ordering::SeqCst);
                    if let Ok(mut st) = self.status_message.lock() {
                        *st = format!("Erro ao criar WebSocket: {:?}", e);
                    }
                }
            }
        }
    }

    /// Envia uma mensagem de controle através da rede
    pub fn send(&self, msg: ControlMessage) -> bool {
        let json = match serde_json::to_string(&msg) {
            Ok(j) => j,
            Err(_) => return false,
        };

        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Ok(g) = self.native_tx.lock() {
                if let Some(ref tx) = *g {
                    return tx.send(json).is_ok();
                }
            }
            false
        }

        #[cfg(target_arch = "wasm32")]
        {
            if let Ok(g) = self.wasm_ws.lock() {
                if let Some(ref ws) = *g {
                    if ws.ready_state() == WebSocket::OPEN {
                        return ws.send_with_str(&json).is_ok();
                    }
                }
            }
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::protocol::Domain;

    #[test]
    fn test_client_queue_operations() {
        let client = ControlClient::new();
        assert_eq!(client.drain_messages().len(), 0);

        let msg = ControlMessage::new(Domain::System, "ping", serde_json::Value::Null);
        client.push_incoming(msg.clone());

        let drained = client.drain_messages();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0], msg);
        assert_eq!(client.drain_messages().len(), 0);
    }
}
