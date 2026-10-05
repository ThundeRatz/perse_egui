use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

use crate::net::protocol::ControlMessage;

/// Servidor WebSocket rodando no host do robô para gerenciar os painéis
pub struct ControlServer {
    addr: SocketAddr,
    tx_broadcast: broadcast::Sender<ControlMessage>,
}

impl ControlServer {
    pub fn new(addr: SocketAddr) -> Self {
        let (tx, _) = broadcast::channel(256);
        Self {
            addr,
            tx_broadcast: tx,
        }
    }

    pub fn sender(&self) -> broadcast::Sender<ControlMessage> {
        self.tx_broadcast.clone()
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let listener = TcpListener::bind(self.addr).await?;
        re_log::info!("Servidor WebSocket Host rodando em ws://{}", self.addr);

        loop {
            let (stream, peer_addr) = listener.accept().await?;
            let tx_broadcast = self.tx_broadcast.clone();

            tokio::spawn(async move {
                if let Err(e) = handle_connection(stream, peer_addr, tx_broadcast).await {
                    re_log::warn!("Erro na conexão de {}: {}", peer_addr, e);
                }
            });
        }
    }
}

async fn handle_connection(
    stream: tokio::net::TcpStream,
    peer_addr: SocketAddr,
    tx_broadcast: broadcast::Sender<ControlMessage>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut ws_stream = accept_async(stream).await?;
    re_log::debug!("Cliente WebSocket conectado: {}", peer_addr);

    let mut rx_broadcast = tx_broadcast.subscribe();

    loop {
        tokio::select! {
            // Recebe mensagem enviada pelo cliente
            msg = ws_stream.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(ctrl_msg) = serde_json::from_str::<ControlMessage>(&text) {
                            re_log::debug!("Host recebeu mensagem de controle de {}: {:?}", peer_addr, ctrl_msg);
                            let _ = tx_broadcast.send(ctrl_msg);
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
            // Envia mensagens em broadcast (atualizações de status para os clientes)
            b_msg = rx_broadcast.recv() => {
                if let Ok(msg) = b_msg {
                    if let Ok(json) = serde_json::to_string(&msg) {
                        ws_stream.send(Message::Text(json.into())).await?;
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_control_server_bind() {
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let listener = TcpListener::bind(addr).await.unwrap();
        let local_addr = listener.local_addr().unwrap();
        assert!(local_addr.port() > 0);
    }
}
