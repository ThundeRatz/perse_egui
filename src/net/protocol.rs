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
use crate::panels::terminals::TerminalTab;

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalsListResponse {
    pub tabs: Vec<TerminalTab>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchfileActionRequest {
    pub package: String,
    pub filename: String,
    pub action: String, // "start" ou "stop"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchfileStatusNotification {
    pub package: String,
    pub filename: String,
    pub running: bool,
    pub pid: Option<u32>,
}

// --- Payloads do Controle de Terminal ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalDataMessage {
    pub terminal_id: usize,
    pub text: String,
    pub is_input: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalCreateRequest {
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalCloseRequest {
    pub terminal_id: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalResizeRequest {
    pub terminal_id: usize,
    pub cols: u16,
    pub rows: u16,
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
}
