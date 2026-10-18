use crate::editor::models::MissionSetCollection;
use crate::panels::parameters::{PackageParams, ParameterValue};
use serde::{Deserialize, Serialize};

/// Envelope padrão para mensagens no canal de controle `/ws/control`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ControlMessage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub domain: Domain,
    pub action: String,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub payload: serde_json::Value,
}

impl ControlMessage {
    pub fn new(domain: Domain, action: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            id: None,
            domain,
            action: action.into(),
            payload,
        }
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }
}

/// Domínios de mensagens suportados pelo plano de controle
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Domain {
    Parameters,
    Launchfiles,
    Terminal,
    Mission,
    System,
}

// --- Payloads do Domínio de Parâmetros ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParametersTreeResponse {
    pub packages: Vec<PackageParams>,
    #[serde(default)]
    pub workspace: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateParameterRequest {
    pub package: String,
    pub file: String,
    pub scope_path: Vec<String>,
    pub param_name: String,
    pub edited_value: ParameterValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyParametersRequest {
    #[serde(default)]
    pub package: Option<String>,
    #[serde(default)]
    pub packages: Vec<PackageParams>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyParametersResponse {
    pub applied_count: usize,
}

// --- Payloads do Domínio de Launchfiles ---

use crate::panels::launchfiles::LaunchPackage;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LaunchfileInfo {
    pub package: String,
    pub filename: String,
    pub running: bool,
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchfilesListResponse {
    pub packages: Vec<LaunchPackage>,
    #[serde(default)]
    pub workspace: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchfileActionRequest {
    pub package: String,
    pub filename: String,
    pub action: String, // "start" ou "stop"
    #[serde(default)]
    pub target_mode: Option<crate::config::LaunchTargetMode>,
    #[serde(default)]
    pub container_name: Option<String>,
    #[serde(default)]
    pub entrypoint: Option<String>,
    #[serde(default)]
    pub args: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetLaunchConfigRequest {
    pub target_mode: crate::config::LaunchTargetMode,
    pub container_name: String,
    pub entrypoint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchfileStatusNotification {
    pub package: String,
    pub filename: String,
    pub running: bool,
    pub pid: Option<u32>,
}

// --- Payloads do Controle de Terminal ---

/// Alvo de execução do shell no host do daemon
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalTarget {
    Host,
    Docker,
}

impl Default for TerminalTarget {
    fn default() -> Self {
        Self::Host
    }
}

fn default_term_cols() -> u16 {
    80
}

fn default_term_rows() -> u16 {
    24
}

/// Requisição para criar uma nova sessão de terminal PTY no daemon
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TerminalCreateRequest {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub target: TerminalTarget,
    #[serde(default = "default_term_cols")]
    pub cols: u16,
    #[serde(default = "default_term_rows")]
    pub rows: u16,
    #[serde(default)]
    pub command: Option<String>,
}

/// Envio de bytes de entrada (stdin / teclas ANSI) do cliente para o PTY do daemon
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TerminalInputMessage {
    pub id: String,
    pub data: Vec<u8>,
}

/// Envio de bytes de saída (stdout/stderr do PTY) do daemon para os clientes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TerminalOutputMessage {
    pub id: String,
    pub data: Vec<u8>,
    #[serde(default)]
    pub timestamp_ms: u64,
}

/// Redimensionamento da janela do PTY (colunas x linhas)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TerminalResizeMessage {
    pub id: String,
    pub cols: u16,
    pub rows: u16,
}

/// Requisição para encerrar e fechar uma sessão de terminal no daemon
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TerminalCloseRequest {
    pub id: String,
}

/// Notificação de mudança de status de execução de uma sessão de terminal
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TerminalStatusMessage {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub target: TerminalTarget,
    pub is_running: bool,
}

/// Resumo de uma sessão ativa no daemon (incluindo buffer recente para restauração em reconexão)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TerminalSessionSummary {
    pub id: String,
    pub title: String,
    pub is_running: bool,
    #[serde(default)]
    pub target: TerminalTarget,
    #[serde(default = "default_term_cols")]
    pub cols: u16,
    #[serde(default = "default_term_rows")]
    pub rows: u16,
    #[serde(default)]
    pub initial_bytes: Vec<u8>,
}

/// Lista de sessões sincronizadas do daemon para o cliente
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TerminalListSyncResponse {
    pub sessions: Vec<TerminalSessionSummary>,
}

// --- Payloads do Domínio de Missão (Mission Editor) ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionDataResponse {
    pub collection: MissionSetCollection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveMissionDataRequest {
    pub collection: MissionSetCollection,
    pub target_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveMissionDataResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListFilesRequest {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteFileEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListFilesResponse {
    pub current_path: String,
    pub parent_path: Option<String>,
    pub entries: Vec<RemoteFileEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadMissionFileRequest {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadMissionFileResponse {
    pub path: String,
    pub success: bool,
    pub data: Option<crate::editor::models::MissionData>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveMissionFileRequest {
    pub path: String,
    pub data: crate::editor::models::MissionData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveMissionFileResponse {
    pub path: String,
    pub success: bool,
    pub error: Option<String>,
}

// --- Payloads de Sistema ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStatusNotification {
    pub message: String,
    pub host_time_secs: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_control_message_roundtrip() {
        let msg = ControlMessage::new(
            Domain::Parameters,
            "get_tree",
            serde_json::json!({"filter": "all"}),
        )
        .with_id("req-123");

        let json = serde_json::to_string(&msg).expect("Serialize message");
        let decoded: ControlMessage = serde_json::from_str(&json).expect("Deserialize message");

        assert_eq!(msg, decoded);
        assert_eq!(decoded.id, Some("req-123".to_string()));
        assert_eq!(decoded.domain, Domain::Parameters);
        assert_eq!(decoded.action, "get_tree");
    }

    #[test]
    fn test_mission_message_roundtrip() {
        let coll = MissionSetCollection::default();
        let payload = serde_json::to_value(MissionDataResponse {
            collection: coll.clone(),
        })
        .unwrap();

        let msg = ControlMessage::new(Domain::Mission, "data_response", payload);
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: ControlMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.domain, Domain::Mission);
        let resp: MissionDataResponse = serde_json::from_value(decoded.payload).unwrap();
        assert_eq!(resp.collection.active_set_id, coll.active_set_id);
    }

    #[test]
    fn test_terminal_messages_roundtrip() {
        let create_req = TerminalCreateRequest {
            id: "term_1".to_string(),
            title: "Docker Shell".to_string(),
            target: TerminalTarget::Docker,
            cols: 120,
            rows: 36,
            command: Some("ros2 launch my_pkg test.launch.py".to_string()),
        };
        let msg = ControlMessage::new(
            Domain::Terminal,
            "create",
            serde_json::to_value(&create_req).unwrap(),
        );
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: ControlMessage = serde_json::from_str(&json).unwrap();
        let decoded_req: TerminalCreateRequest = serde_json::from_value(decoded.payload).unwrap();
        assert_eq!(decoded_req, create_req);

        let input_msg = TerminalInputMessage {
            id: "term_1".to_string(),
            data: vec![0x03, b'l', b's', b'\r'],
        };
        let input_json = serde_json::to_string(&input_msg).unwrap();
        let decoded_input: TerminalInputMessage = serde_json::from_str(&input_json).unwrap();
        assert_eq!(decoded_input, input_msg);
    }
}
