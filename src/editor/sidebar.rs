use crate::{
    editor::{
        canvas::ViewVisibilityOptions,
        models::{MissionData, MissionSetCollection, ObstacleKind, ObstacleShape},
    },
    panels::components::simple_section_header,
};
use egui::{Color32, Margin, RichText, Ui};
use indexmap::IndexMap;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Selection {
    pub points: std::collections::BTreeSet<usize>,
    pub obstacles: std::collections::BTreeSet<usize>,
    pub obstacle_vertex: Option<(usize, usize)>,
}

impl Selection {
    pub fn is_empty(&self) -> bool {
        self.points.is_empty() && self.obstacles.is_empty() && self.obstacle_vertex.is_none()
    }

    pub fn clear(&mut self) {
        self.points.clear();
        self.obstacles.clear();
        self.obstacle_vertex = None;
    }

    pub fn select_single_point(&mut self, idx: usize) {
        self.clear();
        self.points.insert(idx);
    }

    pub fn select_single_obstacle(&mut self, idx: usize) {
        self.clear();
        self.obstacles.insert(idx);
    }

    pub fn select_single_vertex(&mut self, obs_idx: usize, v_idx: usize) {
        self.clear();
        self.obstacles.insert(obs_idx);
        self.obstacle_vertex = Some((obs_idx, v_idx));
    }

    pub fn contains_point(&self, idx: usize) -> bool {
        self.points.contains(&idx)
    }

    pub fn contains_obstacle(&self, idx: usize) -> bool {
        self.obstacles.contains(&idx)
    }

    pub fn is_vertex_selected(&self, obs_idx: usize, v_idx: usize) -> bool {
        self.obstacle_vertex == Some((obs_idx, v_idx))
    }

    pub fn count(&self) -> usize {
        self.points.len() + self.obstacles.len()
    }

    pub fn remove_point_and_adjust(&mut self, idx: usize) {
        self.points.remove(&idx);
        let old_points = std::mem::take(&mut self.points);
        for p in old_points {
            if p > idx {
                self.points.insert(p - 1);
            } else if p < idx {
                self.points.insert(p);
            }
        }
    }

    pub fn remove_obstacle_and_adjust(&mut self, idx: usize) {
        self.obstacles.remove(&idx);
        if let Some((o_idx, _)) = self.obstacle_vertex {
            if o_idx == idx {
                self.obstacle_vertex = None;
            } else if o_idx > idx {
                self.obstacle_vertex = Some((o_idx - 1, self.obstacle_vertex.unwrap().1));
            }
        }
        let old_obs = std::mem::take(&mut self.obstacles);
        for o in old_obs {
            if o > idx {
                self.obstacles.insert(o - 1);
            } else if o < idx {
                self.obstacles.insert(o);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteDialogMode {
    Open,
    Save,
}

#[derive(Debug, Clone)]
pub struct RemoteFileDialogState {
    pub is_open: bool,
    pub mode: RemoteDialogMode,
    pub current_path: String,
    pub parent_path: Option<String>,
    pub entries: Vec<crate::net::protocol::RemoteFileEntry>,
    pub selected_file: Option<String>,
    pub save_filename: String,
    pub error_msg: Option<String>,
}

impl Default for RemoteFileDialogState {
    fn default() -> Self {
        Self {
            is_open: false,
            mode: RemoteDialogMode::Open,
            current_path: ".".to_string(),
            parent_path: None,
            entries: Vec::new(),
            selected_file: None,
            save_filename: "mission_points.yaml".to_string(),
            error_msg: None,
        }
    }
}

#[derive(Default)]
pub struct EditorSidebar {
    pub new_param_key: String,
    pub new_param_val: String,
    pub show_add_param_popup: bool,
    pub remote_dialog: RemoteFileDialogState,
    pub is_renaming_set: bool,
    pub rename_input: String,
    pub pending_overwrite_data: Option<(String, MissionData)>,
}

impl EditorSidebar {
    pub fn notify_sets_updated(
        client: Option<&crate::net::client::ControlClient>,
        sets: &MissionSetCollection,
        file_path: &str,
    ) {
        if let Some(c) = client {
            if c.is_connected() {
                c.send(crate::net::protocol::ControlMessage::new(
                    crate::net::protocol::Domain::Mission,
                    "save_data",
                    serde_json::to_value(crate::net::protocol::SaveMissionDataRequest {
                        collection: sets.clone(),
                        target_path: Some(file_path.to_string()),
                    })
                    .unwrap_or_default(),
                ));
            }
        }
    }

    pub fn request_list_files(
        &mut self,
        client: Option<&crate::net::client::ControlClient>,
        path: &str,
    ) {
        if let Some(c) = client {
            if c.is_connected() {
                c.send(crate::net::protocol::ControlMessage::new(
                    crate::net::protocol::Domain::Mission,
                    "list_files",
                    serde_json::to_value(crate::net::protocol::ListFilesRequest {
                        path: path.to_string(),
                    })
                    .unwrap_or_default(),
                ));
                return;
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let raw_path = if path.trim().is_empty() || path == "." {
                std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
            } else {
                std::path::PathBuf::from(path)
            };
            let target = raw_path.canonicalize().unwrap_or(raw_path);

            match std::fs::read_dir(&target) {
                Ok(read_dir) => {
                    let mut entries = Vec::new();
                    for entry in read_dir.flatten() {
                        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                        let name = entry.file_name().to_string_lossy().to_string();
                        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);

                        if is_dir || name.ends_with(".yaml") || name.ends_with(".yml") {
                            entries.push(crate::net::protocol::RemoteFileEntry {
                                name,
                                is_dir,
                                size,
                            });
                        }
                    }

                    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
                        (true, false) => std::cmp::Ordering::Less,
                        (false, true) => std::cmp::Ordering::Greater,
                        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                    });

                    self.remote_dialog.current_path = target.to_string_lossy().to_string();
                    self.remote_dialog.parent_path =
                        target.parent().map(|p| p.to_string_lossy().to_string());
                    self.remote_dialog.entries = entries;
                    self.remote_dialog.error_msg = None;
                }
                Err(e) => {
                    self.remote_dialog.error_msg =
                        Some(format!("Erro ao acessar diretório local: {}", e));
                }
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            self.remote_dialog.error_msg = Some(
                "Host desconectado. Conecte ao robô para navegar no sistema de arquivos."
                    .to_string(),
            );
        }
    }

    pub fn execute_load(
        &mut self,
        client: Option<&crate::net::client::ControlClient>,
        sets_guard: &mut MissionSetCollection,
        path: &str,
        status_msg: &mut String,
        selection: &mut Selection,
        file_path: &str,
    ) {
        if let Some(c) = client {
            if c.is_connected() {
                c.send(crate::net::protocol::ControlMessage::new(
                    crate::net::protocol::Domain::Mission,
                    "load_file",
                    serde_json::to_value(crate::net::protocol::LoadMissionFileRequest {
                        path: path.to_string(),
                    })
                    .unwrap_or_default(),
                ));
                *status_msg = format!("Requisitado carregamento de '{}' ao host...", path);
                return;
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            match MissionData::load_from_file(path) {
                Ok(loaded) => {
                    self.apply_loaded_data(
                        sets_guard,
                        path.to_string(),
                        loaded,
                        status_msg,
                        selection,
                        client,
                        file_path,
                    );
                }
                Err(e) => {
                    *status_msg = format!("Erro ao carregar: {}", e);
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (sets_guard, selection, file_path);
            *status_msg = "Host desconectado. Conecte ao robô para carregar o arquivo.".to_string();
        }
    }

    pub fn execute_save(
        &mut self,
        client: Option<&crate::net::client::ControlClient>,
        sets_guard: &mut MissionSetCollection,
        path: &str,
        status_msg: &mut String,
    ) {
        let Some(data) = sets_guard.active_data() else {
            return;
        };

        if let Some(c) = client {
            if c.is_connected() {
                c.send(crate::net::protocol::ControlMessage::new(
                    crate::net::protocol::Domain::Mission,
                    "save_file",
                    serde_json::to_value(crate::net::protocol::SaveMissionFileRequest {
                        path: path.to_string(),
                        data: data.clone(),
                    })
                    .unwrap_or_default(),
                ));
                c.send(crate::net::protocol::ControlMessage::new(
                    crate::net::protocol::Domain::Mission,
                    "save_data",
                    serde_json::to_value(crate::net::protocol::SaveMissionDataRequest {
                        collection: sets_guard.clone(),
                        target_path: Some(path.to_string()),
                    })
                    .unwrap_or_default(),
                ));
                *status_msg = format!("Salvando '{}' no host...", path);
                return;
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            match data.save_to_file(path) {
                Ok(_) => {
                    let _ = sets_guard.save_to_file("mission_sets.json");
                    *status_msg = format!("Arquivo '{}' salvo com sucesso!", path);
                }
                Err(e) => {
                    *status_msg = format!("Erro ao salvar: {}", e);
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            *status_msg = "Host desconectado. Conecte ao robô para salvar o arquivo.".to_string();
        }
    }

    fn show_remote_file_dialog(
        &mut self,
        ui: &mut Ui,
        client: Option<&crate::net::client::ControlClient>,
        sets_guard: &mut MissionSetCollection,
        file_path: &mut String,
        status_msg: &mut String,
        selection: &mut Selection,
    ) {
        if !self.remote_dialog.is_open {
            return;
        }

        let mut is_open = self.remote_dialog.is_open;
        let title = match self.remote_dialog.mode {
            RemoteDialogMode::Open => "📂 Explorador Remoto de Arquivos (Host) - Abrir",
            RemoteDialogMode::Save => "💾 Explorador Remoto de Arquivos (Host) - Salvar Como...",
        };

        egui::Window::new(title)
            .id(ui.make_persistent_id("remote_file_dialog_window"))
            .open(&mut is_open)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .default_size(egui::vec2(540.0, 420.0))
            .resizable(true)
            .collapsible(false)
            .show(ui.ctx(), |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Caminho no Host:").strong());
                    let has_parent = self.remote_dialog.parent_path.is_some();
                    if ui
                        .add_enabled(has_parent, egui::Button::new("⬆ Subir"))
                        .clicked()
                    {
                        if let Some(parent) = self.remote_dialog.parent_path.clone() {
                            self.request_list_files(client, &parent);
                        }
                    }
                    if ui.button("🔄 Atualizar").clicked() {
                        let cur = self.remote_dialog.current_path.clone();
                        self.request_list_files(client, &cur);
                    }
                });

                ui.add(
                    egui::TextEdit::singleline(&mut self.remote_dialog.current_path)
                        .desired_width(ui.available_width())
                        .interactive(false),
                );

                ui.add_space(4.0);
                ui.separator();

                if let Some(ref err) = self.remote_dialog.error_msg {
                    ui.colored_label(Color32::from_rgb(250, 100, 100), err);
                }

                let scroll_height = if self.remote_dialog.mode == RemoteDialogMode::Save {
                    ui.available_height() - 75.0
                } else {
                    ui.available_height() - 45.0
                };

                let mut dir_to_open = None;
                let mut file_to_select = None;
                let mut double_click_open_file = None;

                egui::ScrollArea::vertical()
                    .max_height(scroll_height.max(140.0))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.remote_dialog.entries.is_empty() {
                            ui.weak("Nenhum arquivo ou pasta encontrado neste diretório.");
                        } else {
                            for entry in &self.remote_dialog.entries {
                                let is_selected = self.remote_dialog.selected_file.as_deref()
                                    == Some(&entry.name);
                                let icon = if entry.is_dir { "📁" } else { "📄" };
                                let size_info = if entry.is_dir {
                                    "<DIR>".to_string()
                                } else if entry.size < 1024 {
                                    format!("{} B", entry.size)
                                } else if entry.size < 1024 * 1024 {
                                    format!("{:.1} KB", entry.size as f64 / 1024.0)
                                } else {
                                    format!("{:.1} MB", entry.size as f64 / (1024.0 * 1024.0))
                                };

                                let row_text = format!("{}  {} ({})", icon, entry.name, size_info);
                                let response = ui.selectable_label(is_selected, row_text);

                                if response.clicked() {
                                    if entry.is_dir {
                                        dir_to_open = Some(entry.name.clone());
                                    } else {
                                        file_to_select = Some(entry.name.clone());
                                    }
                                }
                                if response.double_clicked() {
                                    if entry.is_dir {
                                        dir_to_open = Some(entry.name.clone());
                                    } else {
                                        double_click_open_file = Some(entry.name.clone());
                                    }
                                }
                            }
                        }
                    });

                if let Some(folder_name) = dir_to_open {
                    let current = &self.remote_dialog.current_path;
                    let new_path =
                        if current == "/" || current.ends_with('/') || current.ends_with('\\') {
                            format!("{}{}", current, folder_name)
                        } else {
                            format!("{}/{}", current, folder_name)
                        };
                    self.request_list_files(client, &new_path);
                    self.remote_dialog.selected_file = None;
                }

                if let Some(file_name) = file_to_select {
                    self.remote_dialog.selected_file = Some(file_name.clone());
                    if self.remote_dialog.mode == RemoteDialogMode::Save {
                        self.remote_dialog.save_filename = file_name;
                    }
                }

                ui.add_space(6.0);
                ui.separator();
                ui.add_space(4.0);

                match self.remote_dialog.mode {
                    RemoteDialogMode::Open => {
                        let sel_label = self
                            .remote_dialog
                            .selected_file
                            .clone()
                            .unwrap_or_else(|| "Nenhum selecionado".to_string());
                        let can_open = self.remote_dialog.selected_file.is_some()
                            || double_click_open_file.is_some();
                        let target_to_open = double_click_open_file
                            .or_else(|| self.remote_dialog.selected_file.clone());
                        let cur_dir = self.remote_dialog.current_path.clone();

                        let mut should_open = false;
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Arquivo:").weak());
                            ui.label(RichText::new(&sel_label).strong());

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.button("Cancelar").clicked() {
                                        self.remote_dialog.is_open = false;
                                    }

                                    let open_clicked = ui
                                        .add_enabled(can_open, egui::Button::new("📂 Abrir"))
                                        .clicked();
                                    if open_clicked && can_open {
                                        should_open = true;
                                    }
                                },
                            );
                        });

                        if should_open {
                            if let Some(fname) = target_to_open {
                                let full_path = if cur_dir == "." {
                                    fname
                                } else if cur_dir.ends_with('/') || cur_dir.ends_with('\\') {
                                    format!("{}{}", cur_dir, fname)
                                } else {
                                    format!("{}/{}", cur_dir, fname)
                                };

                                *file_path = full_path.clone();
                                let cur_fp = file_path.clone();
                                self.execute_load(
                                    client, sets_guard, &full_path, status_msg, selection, &cur_fp,
                                );
                                self.remote_dialog.is_open = false;
                            }
                        }
                    }
                    RemoteDialogMode::Save => {
                        let cur_dir = self.remote_dialog.current_path.clone();
                        let mut should_save = false;

                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Nome do arquivo:").weak());
                            ui.text_edit_singleline(&mut self.remote_dialog.save_filename);

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.button("Cancelar").clicked() {
                                        self.remote_dialog.is_open = false;
                                    }

                                    let can_save =
                                        !self.remote_dialog.save_filename.trim().is_empty();
                                    if ui
                                        .add_enabled(can_save, egui::Button::new("💾 Salvar"))
                                        .clicked()
                                    {
                                        should_save = true;
                                    }
                                },
                            );
                        });

                        if should_save {
                            let mut fname = self.remote_dialog.save_filename.trim().to_string();
                            if !fname.ends_with(".yaml") && !fname.ends_with(".yml") {
                                fname.push_str(".yaml");
                            }
                            let full_path = if cur_dir == "." {
                                fname
                            } else if cur_dir.ends_with('/') || cur_dir.ends_with('\\') {
                                format!("{}{}", cur_dir, fname)
                            } else {
                                format!("{}/{}", cur_dir, fname)
                            };

                            *file_path = full_path.clone();
                            self.execute_save(client, sets_guard, &full_path, status_msg);
                            self.remote_dialog.is_open = false;
                        }
                    }
                }
            });

        self.remote_dialog.is_open = is_open;
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        sets_guard: &mut MissionSetCollection,
        selection: &mut Selection,
        view_options: &mut ViewVisibilityOptions,
        file_path: &mut String,
        status_msg: &mut String,
        client: Option<&crate::net::client::ControlClient>,
    ) {
        self.show_remote_file_dialog(ui, client, sets_guard, file_path, status_msg, selection);

        // Modal de confirmação para sobrescrever conjunto não-vazio
        if let Some((path_str, loaded)) = self.pending_overwrite_data.clone() {
            let mut show_modal = true;
            egui::Window::new("⚠️ Confirmar Sobrescrita")
                .id(ui.make_persistent_id("confirm_overwrite_modal"))
                .open(&mut show_modal)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .resizable(false)
                .collapsible(false)
                .show(ui.ctx(), |ui| {
                    ui.label("O conjunto de missão atual não está vazio.");
                    ui.label("Deseja substituir o conteúdo atual pelo arquivo carregado?");
                    ui.add_space(4.0);
                    ui.weak(format!("Arquivo: {}", path_str));

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Confirmar / Sobrescrever").clicked() {
                            if let Some(data) = sets_guard.active_data_mut() {
                                *data = loaded;
                                *status_msg =
                                    format!("Conjunto atual substituído por '{}'!", path_str);
                                selection.clear();
                                Self::notify_sets_updated(client, sets_guard, file_path);
                            }
                            self.pending_overwrite_data = None;
                        }

                        if ui.button("Cancelar").clicked() {
                            self.pending_overwrite_data = None;
                        }
                    });
                });

            if !show_modal {
                self.pending_overwrite_data = None;
            }
        }

        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    if selection.is_empty() {
                        simple_section_header(ui, "Propriedades", |_| {});
                        self.show_global_info(
                            ui, sets_guard, file_path, status_msg, selection, client,
                        );
                    } else if selection.points.len() == 1 && selection.obstacles.is_empty() {
                        let idx = *selection.points.iter().next().unwrap();
                        if let Some(data) = sets_guard.active_data_mut() {
                            if idx < data.points.len() {
                                self.show_point_properties(
                                    ui,
                                    data,
                                    idx,
                                    selection,
                                    &mut view_options.visible_param_keys,
                                    status_msg,
                                );
                            } else {
                                selection.clear();
                            }
                        }
                    } else if selection.obstacles.len() == 1 && selection.points.is_empty() {
                        let idx = *selection.obstacles.iter().next().unwrap();
                        if let Some(data) = sets_guard.active_data_mut() {
                            if idx < data.obstacles.len() {
                                self.show_obstacle_properties(
                                    ui,
                                    data,
                                    idx,
                                    selection,
                                    view_options,
                                );
                            } else {
                                selection.clear();
                            }
                        }
                    } else {
                        // Seleção Múltipla
                        simple_section_header(
                            ui,
                            &format!(
                                "Seleção Múltipla ({} marcos, {} obstáculos)",
                                selection.points.len(),
                                selection.obstacles.len()
                            ),
                            |ui| {
                                if ui.small_button("❌ Deselecionar Tudo").clicked() {
                                    selection.clear();
                                }
                            },
                        );

                        // Renderizar propriedades de todos os marcos selecionados
                        let sel_pts: Vec<usize> = selection.points.iter().copied().collect();
                        for idx in sel_pts {
                            if let Some(data) = sets_guard.active_data_mut() {
                                if idx < data.points.len() {
                                    self.show_point_properties(
                                        ui,
                                        data,
                                        idx,
                                        selection,
                                        &mut view_options.visible_param_keys,
                                        status_msg,
                                    );
                                    ui.add_space(8.0);
                                    ui.separator();
                                }
                            }
                        }

                        // Renderizar propriedades de todos os obstáculos selecionados
                        let sel_obs: Vec<usize> = selection.obstacles.iter().copied().collect();
                        for idx in sel_obs {
                            if let Some(data) = sets_guard.active_data_mut() {
                                if idx < data.obstacles.len() {
                                    self.show_obstacle_properties(
                                        ui,
                                        data,
                                        idx,
                                        selection,
                                        view_options,
                                    );
                                    ui.add_space(8.0);
                                    ui.separator();
                                }
                            }
                        }
                    }
                });
            });
    }

    pub fn apply_loaded_data(
        &mut self,
        sets_guard: &mut MissionSetCollection,
        path_str: String,
        loaded: MissionData,
        status_msg: &mut String,
        selection: &mut Selection,
        client: Option<&crate::net::client::ControlClient>,
        file_path: &str,
    ) {
        let is_non_empty = sets_guard
            .active_data()
            .is_some_and(|d| !d.points.is_empty() || !d.obstacles.is_empty());

        if is_non_empty {
            self.pending_overwrite_data = Some((path_str, loaded));
        } else {
            if let Some(data) = sets_guard.active_data_mut() {
                *data = loaded;
                *status_msg = format!("Conjunto atual substituído por '{}'!", path_str);
                selection.clear();
                Self::notify_sets_updated(client, sets_guard, file_path);
            }
        }
    }

    fn show_global_info(
        &mut self,
        ui: &mut Ui,
        sets_guard: &mut MissionSetCollection,
        file_path: &mut String,
        status_msg: &mut String,
        selection: &mut Selection,
        client: Option<&crate::net::client::ControlClient>,
    ) {
        pad_content(ui, |ui| {
            ui.label(RichText::new("Conjunto de Missão").strong());

            let active_name = sets_guard
                .sets
                .iter()
                .find(|s| s.id == sets_guard.active_set_id)
                .map(|s| s.name.clone())
                .unwrap_or_else(|| "Padrão".to_string());

            let mut current_id = sets_guard.active_set_id.clone();
            egui::ComboBox::from_id_salt(ui.make_persistent_id("sidebar_mission_set_select_combo"))
                .selected_text(&active_name)
                .show_ui(ui, |ui| {
                    for s in &sets_guard.sets {
                        ui.selectable_value(&mut current_id, s.id.clone(), &s.name);
                    }
                });

            if current_id != sets_guard.active_set_id {
                sets_guard.set_active(&current_id);
                selection.clear();
                Self::notify_sets_updated(client, sets_guard, file_path);
            }

            ui.add_space(4.0);

            if self.is_renaming_set {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.rename_input).desired_width(120.0));
                    if ui
                        .small_button("✓")
                        .on_hover_text("Confirmar nome")
                        .clicked()
                    {
                        if !self.rename_input.trim().is_empty() {
                            sets_guard.rename_active(self.rename_input.trim());
                            Self::notify_sets_updated(client, sets_guard, file_path);
                        }
                        self.is_renaming_set = false;
                    }
                    if ui.small_button("❌").on_hover_text("Cancelar").clicked() {
                        self.is_renaming_set = false;
                    }
                });
            } else {
                ui.horizontal(|ui| {
                    if ui
                        .button("➕ Novo")
                        .on_hover_text("Criar novo conjunto de pontos e obstáculos")
                        .clicked()
                    {
                        let count = sets_guard.sets.len() + 1;
                        let new_id =
                            sets_guard.add_set(format!("Missão {}", count), MissionData::default());
                        sets_guard.set_active(&new_id);
                        selection.clear();
                        Self::notify_sets_updated(client, sets_guard, file_path);
                    }

                    if ui
                        .button("📋 Duplicar")
                        .on_hover_text("Duplicar o conjunto atual")
                        .clicked()
                    {
                        sets_guard.duplicate_active();
                        selection.clear();
                        Self::notify_sets_updated(client, sets_guard, file_path);
                    }

                    if ui
                        .button("✏️ Renomear")
                        .on_hover_text("Renomear o conjunto atual")
                        .clicked()
                    {
                        self.rename_input = active_name;
                        self.is_renaming_set = true;
                    }

                    let can_delete = sets_guard.sets.len() > 1;
                    if ui
                        .add_enabled(can_delete, egui::Button::new("🗑 Excluir"))
                        .on_hover_text("Excluir o conjunto atual")
                        .clicked()
                    {
                        sets_guard.delete_active();
                        selection.clear();
                        Self::notify_sets_updated(client, sets_guard, file_path);
                    }
                });
            }

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            ui.label(RichText::new("Arquivo de Missão (YAML)").strong());
            ui.text_edit_singleline(file_path);

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui
                    .button("📥 Carregar")
                    .on_hover_text("Carregar arquivo e substituir conjunto atual")
                    .clicked()
                {
                    let cur_path = file_path.clone();
                    self.execute_load(
                        client, sets_guard, &cur_path, status_msg, selection, file_path,
                    );
                }

                if ui
                    .button("💾 Salvar")
                    .on_hover_text("Salvar conjunto atual no arquivo especificado")
                    .clicked()
                {
                    let cur_path = file_path.clone();
                    self.execute_save(client, sets_guard, &cur_path, status_msg);
                }
            });

            if !status_msg.is_empty() {
                ui.add_space(4.0);
                ui.weak(status_msg.as_str());
            }

            ui.add_space(12.0);
            ui.label(RichText::new("Resumo do Conjunto Ativo").strong());
            if let Some(data) = sets_guard.active_data() {
                ui.label(format!("Pontos de missão: {}", data.points.len()));
                ui.label(format!("Obstáculos definidos: {}", data.obstacles.len()));
            }
        });
    }

    fn show_point_properties(
        &mut self,
        ui: &mut Ui,
        data: &mut MissionData,
        idx: usize,
        selection: &mut Selection,
        visible_param_keys: &mut std::collections::HashSet<String>,
        status_msg: &mut String,
    ) {
        ui.push_id(("point_props_scope", idx), |ui| {
            // Cabeçalho com o nome do marco (ex: "Marco 4") e botão Deselecionar à direita (largura total)
            simple_section_header(ui, &format!("Marco {}", idx), |ui| {
                if ui.small_button("❌ Deselecionar").clicked() {
                    selection.points.remove(&idx);
                }
            });

            let mut move_up = false;
            let mut move_down = false;
            let mut delete_pt = false;
            let points_len = data.points.len();

            {
                let pt = &mut data.points[idx];

                // Conteúdo da posição e ações com padding interno em Grid de 2 colunas
                pad_content(ui, |ui| {
                    egui::Grid::new("point_core_grid")
                        .num_columns(2)
                        .spacing([16.0, 6.0])
                        .min_col_width(100.0)
                        .show(ui, |ui| {
                            ui.label(RichText::new("Posição").color(Color32::from_gray(200)));
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("X:").color(Color32::from_gray(180)));
                                ui.add(egui::DragValue::new(&mut pt.x).speed(0.05));
                                ui.add_space(12.0);
                                ui.label(RichText::new("Y:").color(Color32::from_gray(180)));
                                ui.add(egui::DragValue::new(&mut pt.y).speed(0.05));
                            });
                            ui.end_row();
                        });

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if idx > 0 && ui.small_button("⬆ Mover para Cima").clicked() {
                            move_up = true;
                        }
                        if idx + 1 < points_len && ui.small_button("⬇ Mover para Baixo").clicked()
                        {
                            move_down = true;
                        }
                        if ui.small_button("🗑 Excluir Ponto").clicked() {
                            delete_pt = true;
                        }
                    });
                });

                // Cabeçalho de Parâmetros (largura total)
                simple_section_header(ui, "Parâmetros", |_| {});

                // Conteúdo dos parâmetros com padding interno
                pad_content(ui, |ui| {
                    render_aligned_parameters(
                        ui,
                        &mut pt.extra,
                        visible_param_keys,
                        &mut self.new_param_key,
                        &mut self.new_param_val,
                        &mut self.show_add_param_popup,
                    );
                });
            }

            if move_up {
                data.points.swap(idx, idx - 1);
                let has_curr = selection.points.contains(&idx);
                let has_prev = selection.points.contains(&(idx - 1));
                if has_curr {
                    selection.points.insert(idx - 1);
                } else {
                    selection.points.remove(&(idx - 1));
                }
                if has_prev {
                    selection.points.insert(idx);
                } else {
                    selection.points.remove(&idx);
                }
            } else if move_down {
                data.points.swap(idx, idx + 1);
                let has_curr = selection.points.contains(&idx);
                let has_next = selection.points.contains(&(idx + 1));
                if has_curr {
                    selection.points.insert(idx + 1);
                } else {
                    selection.points.remove(&(idx + 1));
                }
                if has_next {
                    selection.points.insert(idx);
                } else {
                    selection.points.remove(&idx);
                }
            } else if delete_pt {
                data.points.remove(idx);
                selection.remove_point_and_adjust(idx);
                *status_msg = format!("Marco {} removido.", idx);
            }
        });
    }

    fn show_obstacle_properties(
        &mut self,
        ui: &mut Ui,
        data: &mut MissionData,
        idx: usize,
        selection: &mut Selection,
        view_options: &mut ViewVisibilityOptions,
    ) {
        ui.push_id(("obstacle_props_scope", idx), |ui| {
            let obs_id = data.obstacles[idx].id.clone();
            // Cabeçalho com o nome do obstáculo e botão Deselecionar à direita (largura total)
            simple_section_header(ui, &format!("Obstáculo #{} ({})", idx, obs_id), |ui| {
                if ui.small_button("❌ Deselecionar").clicked() {
                    selection.obstacles.remove(&idx);
                }
            });

            if view_options.lock_obstacles {
                pad_content(ui, |ui| {
                    egui::Frame::new()
                        .fill(Color32::from_rgba_unmultiplied(220, 50, 50, 35))
                        .corner_radius(4.0)
                        .inner_margin(Margin::same(6))
                        .stroke(egui::Stroke::new(1.0, Color32::from_rgb(240, 80, 80)))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new("🔒 Edição/movimentação de obstáculos está travada (Menu Exibição)")
                                    .size(11.0)
                                    .color(Color32::from_rgb(255, 160, 160)),
                            );
                        });
                });
            }

            let mut delete_obs = false;

            {
                let obs = &mut data.obstacles[idx];

                pad_content(ui, |ui| {
                    egui::Grid::new("obstacle_core_grid")
                        .num_columns(2)
                        .spacing([16.0, 6.0])
                        .min_col_width(100.0)
                        .show(ui, |ui| {
                            ui.label(RichText::new("ID").color(Color32::from_gray(200)));
                            ui.text_edit_singleline(&mut obs.id);
                            ui.end_row();

                            ui.label(RichText::new("Tipo").color(Color32::from_gray(200)));
                            ui.horizontal(|ui| {
                                ui.selectable_value(&mut obs.kind, ObstacleKind::Physical, "Físico");
                                ui.selectable_value(&mut obs.kind, ObstacleKind::Cosmetic, "Cosmético");
                            });
                            ui.end_row();

                            match &mut obs.shape {
                                ObstacleShape::Polygon { vertices } => {
                                    ui.label(RichText::new("Formato").color(Color32::from_gray(200)));
                                    ui.label(format!("Polígono ({} vértices)", vertices.len()));
                                    ui.end_row();

                                    let mut vert_to_remove = None;
                                    let vert_len = vertices.len();
                                    for (i, v) in vertices.iter_mut().enumerate() {
                                        ui.label(RichText::new(format!("Vértice {}", i)).color(Color32::from_gray(180)));
                                        ui.horizontal(|ui| {
                                            ui.label("X:");
                                            ui.add(egui::DragValue::new(&mut v[0]).speed(0.05));
                                            ui.add_space(8.0);
                                            ui.label("Y:");
                                            ui.add(egui::DragValue::new(&mut v[1]).speed(0.05));
                                            if vert_len > 3 && ui.small_button("🗑").clicked() {
                                                vert_to_remove = Some(i);
                                            }
                                        });
                                        ui.end_row();
                                    }
                                    if let Some(vi) = vert_to_remove {
                                        vertices.remove(vi);
                                    }
                                }
                                ObstacleShape::Line { vertices } => {
                                    ui.label(RichText::new("Formato").color(Color32::from_gray(200)));
                                    ui.label(format!("Linhas ({} vértices)", vertices.len()));
                                    ui.end_row();

                                    let mut vert_to_remove = None;
                                    let vert_len = vertices.len();
                                    for (i, v) in vertices.iter_mut().enumerate() {
                                        ui.label(RichText::new(format!("Vértice {}", i)).color(Color32::from_gray(180)));
                                        ui.horizontal(|ui| {
                                            ui.label("X:");
                                            ui.add(egui::DragValue::new(&mut v[0]).speed(0.05));
                                            ui.add_space(8.0);
                                            ui.label("Y:");
                                            ui.add(egui::DragValue::new(&mut v[1]).speed(0.05));
                                            if vert_len > 2 && ui.small_button("🗑").clicked() {
                                                vert_to_remove = Some(i);
                                            }
                                        });
                                        ui.end_row();
                                    }
                                    if let Some(vi) = vert_to_remove {
                                        vertices.remove(vi);
                                    }
                                }
                                ObstacleShape::Rectangle {
                                    x,
                                    y,
                                    width,
                                    height,
                                    rotation,
                                } => {
                                    ui.label(RichText::new("Posição").color(Color32::from_gray(200)));
                                    ui.horizontal(|ui| {
                                        ui.label("X:");
                                        ui.add(egui::DragValue::new(x).speed(0.05));
                                        ui.add_space(8.0);
                                        ui.label("Y:");
                                        ui.add(egui::DragValue::new(y).speed(0.05));
                                    });
                                    ui.end_row();

                                    ui.label(RichText::new("Dimensões").color(Color32::from_gray(200)));
                                    ui.horizontal(|ui| {
                                        ui.label("L:");
                                        ui.add(egui::DragValue::new(width).speed(0.05).range(0.01..=100.0));
                                        ui.add_space(8.0);
                                        ui.label("A:");
                                        ui.add(egui::DragValue::new(height).speed(0.05).range(0.01..=100.0));
                                    });
                                    ui.end_row();

                                    ui.label(RichText::new("Rotação").color(Color32::from_gray(200)));
                                    ui.horizontal(|ui| {
                                        ui.add(egui::DragValue::new(rotation).speed(0.05));
                                        ui.label("rad");
                                    });
                                    ui.end_row();
                                }
                                ObstacleShape::Circle { center, radius } => {
                                    ui.label(RichText::new("Centro").color(Color32::from_gray(200)));
                                    ui.horizontal(|ui| {
                                        ui.label("X:");
                                        ui.add(egui::DragValue::new(&mut center[0]).speed(0.05));
                                        ui.add_space(8.0);
                                        ui.label("Y:");
                                        ui.add(egui::DragValue::new(&mut center[1]).speed(0.05));
                                    });
                                    ui.end_row();

                                    ui.label(RichText::new("Raio").color(Color32::from_gray(200)));
                                    ui.add(egui::DragValue::new(radius).speed(0.05).range(0.01..=100.0));
                                    ui.end_row();
                                }
                            }
                        });

                    if matches!(obs.shape, ObstacleShape::Polygon { .. } | ObstacleShape::Line { .. }) {
                        ui.add_space(6.0);
                        if ui.button("+ Adicionar Vértice").clicked() {
                            match &mut obs.shape {
                                ObstacleShape::Polygon { vertices } | ObstacleShape::Line { vertices } => {
                                    let last = vertices.last().cloned().unwrap_or([0.0, 0.0]);
                                    vertices.push([last[0] + 0.5, last[1] + 0.5]);
                                }
                                _ => {}
                            }
                        }
                    }

                    ui.add_space(8.0);
                    if ui.button("🗑 Excluir Obstáculo").clicked() {
                        delete_obs = true;
                    }
                });

                simple_section_header(ui, "Parâmetros", |_| {});

                pad_content(ui, |ui| {
                    render_aligned_parameters(
                        ui,
                        &mut obs.extra,
                        &mut view_options.visible_param_keys,
                        &mut self.new_param_key,
                        &mut self.new_param_val,
                        &mut self.show_add_param_popup,
                    );
                });
            }

            if delete_obs {
                data.obstacles.remove(idx);
                selection.remove_obstacle_and_adjust(idx);
            }
        });
    }
}

/// Conteúdo de seção envolvido por Frame de Padding Interno de 10px
fn pad_content(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .inner_margin(Margin {
            left: 10,
            right: 10,
            top: 6,
            bottom: 6,
        })
        .show(ui, add_contents);
}

/// Renderiza os parâmetros organizados em colunas perfeitamente alinhadas:
/// `nome      valor      [olho de visibilidade] [botão de excluir]`
fn render_aligned_parameters(
    ui: &mut Ui,
    extra: &mut IndexMap<String, serde_yaml::Value>,
    visible_param_keys: &mut std::collections::HashSet<String>,
    new_key: &mut String,
    new_val: &mut String,
    show_popup: &mut bool,
) {
    let mut to_remove = None;
    let grid_id = ui.id().with("params_grid");

    egui::Grid::new(grid_id)
        .num_columns(2)
        .spacing([16.0, 6.0])
        .min_col_width(100.0)
        .show(ui, |ui| {
            for (k, v) in extra.iter_mut() {
                let row_min_y = ui.cursor().min.y - 2.0;
                let is_hovered = ui
                    .input(|i| i.pointer.hover_pos())
                    .is_some_and(|pos| pos.y >= row_min_y && pos.y <= row_min_y + 24.0);

                let is_visible = visible_param_keys.contains(k);

                // Coluna 1: Nome do parâmetro alinhado à esquerda
                ui.label(RichText::new(k).color(Color32::from_gray(200)));

                // Coluna 2: Valor do parâmetro + Olho (Visibility) e Lixeira no extremo direito
                ui.horizontal(|ui| {
                    render_dynamic_param_val(ui, v);

                    if is_hovered || is_visible {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let icon = if is_visible {
                                &re_ui::icons::VISIBLE
                            } else {
                                &re_ui::icons::INVISIBLE
                            };

                            let tint = if is_visible {
                                Color32::WHITE
                            } else {
                                Color32::from_gray(110)
                            };

                            let eye_img = icon
                                .as_image()
                                .fit_to_exact_size(egui::vec2(13.0, 13.0))
                                .tint(tint);

                            let eye_btn = ui
                                .add(egui::Button::image(eye_img).fill(Color32::TRANSPARENT))
                                .on_hover_text(
                                    "Exibir/ocultar parâmetro no canvas (para todos os marcos)",
                                );

                            if eye_btn.clicked() {
                                if is_visible {
                                    visible_param_keys.remove(k);
                                } else {
                                    visible_param_keys.insert(k.clone());
                                }
                            }

                            if is_hovered
                                && ui
                                    .small_button("🗑")
                                    .on_hover_text("Remover parâmetro")
                                    .clicked()
                            {
                                to_remove = Some(k.clone());
                            }
                        });
                    }
                });

                ui.end_row();
            }
        });

    if let Some(k) = to_remove {
        extra.shift_remove(&k);
    }

    ui.add_space(8.0);
    render_add_param_controls(ui, extra, new_key, new_val, show_popup);
}

fn render_dynamic_param_val(ui: &mut Ui, val: &mut serde_yaml::Value) {
    match val {
        serde_yaml::Value::Bool(b) => {
            ui.checkbox(b, "");
        }
        serde_yaml::Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                let mut float_val = f;
                if ui
                    .add(egui::DragValue::new(&mut float_val).speed(0.05))
                    .changed()
                {
                    *val = serde_yaml::Value::Number(serde_yaml::Number::from(float_val));
                }
            }
        }
        serde_yaml::Value::String(s) => {
            ui.text_edit_singleline(s);
        }
        _ => {
            ui.label(format!("{:?}", val));
        }
    }
}

fn render_add_param_controls(
    ui: &mut Ui,
    extra: &mut IndexMap<String, serde_yaml::Value>,
    new_key: &mut String,
    new_val: &mut String,
    show_popup: &mut bool,
) {
    if *show_popup {
        ui.group(|ui| {
            ui.label(RichText::new("Novo Parâmetro").strong());
            ui.horizontal(|ui| {
                ui.label("Chave:");
                ui.text_edit_singleline(new_key);
            });
            ui.horizontal(|ui| {
                ui.label("Valor:");
                ui.text_edit_singleline(new_val);
            });
            ui.horizontal(|ui| {
                if ui.button("Adicionar").clicked() {
                    let key = new_key.trim().to_string();
                    let val_str = new_val.trim();
                    if !key.is_empty() {
                        let parsed_val = if let Ok(b) = val_str.parse::<bool>() {
                            serde_yaml::Value::Bool(b)
                        } else if let Ok(f) = val_str.parse::<f64>() {
                            serde_yaml::Value::Number(serde_yaml::Number::from(f))
                        } else {
                            serde_yaml::Value::String(val_str.to_string())
                        };
                        extra.insert(key, parsed_val);
                        new_key.clear();
                        new_val.clear();
                        *show_popup = false;
                    }
                }
                if ui.button("Cancelar").clicked() {
                    *show_popup = false;
                }
            });
        });
    } else if ui.button("➕ Adicionar Parâmetro").clicked() {
        *show_popup = true;
    }
}
