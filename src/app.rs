use eframe::{App, Frame, Storage};
use egui::{Context, Ui};

use crate::{
    config::AppConfig,
    editor::MissionEditor,
    panels::{
        launchfiles::LaunchfilesPanel, parameters::ParametersPanel, settings::SettingsPanel,
        terminals::TerminalsPanel,
    },
    rewire_integration::RewireIntegration,
    state::AppState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    #[default]
    RerunViewer,
    MissionEditor,
}

pub struct MyApp {
    rewire: RewireIntegration,
    mission_editor: MissionEditor,
    view_mode: ViewMode,
    config: AppConfig,
    state: AppState,
    parameters_panel: ParametersPanel,
    launchfiles_panel: LaunchfilesPanel,
    terminals_panel: TerminalsPanel,
    settings_panel: SettingsPanel,
    control_client: crate::net::client::ControlClient,
    was_connected: bool,
    last_reconnect_attempt: f64,
}

impl MyApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        endpoint: String,
        cli_config_path: Option<String>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let mut config = AppConfig::default();
        let mut loaded_mission_sets: Option<crate::editor::models::MissionSetCollection> = None;

        // 1. Priorizar endpoint explicitamente informado; caso vazio, buscar no storage
        if !endpoint.is_empty() {
            config.endpoint = endpoint;
        } else if let Some(storage) = cc.storage {
            if let Some(ep) = storage.get_string("perse_egui.endpoint") {
                config.endpoint = ep;
            }
        }

        if let Some(storage) = cc.storage {
            if let Some(sb) = storage.get_string("perse_egui.show_sidebar") {
                config.show_sidebar = sb.parse().unwrap_or(true);
            }
            if let Some(rsb) = storage.get_string("perse_egui.show_right_sidebar") {
                config.show_right_sidebar = rsb.parse().unwrap_or(true);
            }
            if let Some(theme) = storage.get_string("perse_egui.theme_preference") {
                config.theme_preference = theme;
            }
            if let Some(zs) = storage.get_string("perse_egui.zoom_speed") {
                config.zoom_speed = zs.parse().unwrap_or(0.04);
            }
            if let Some(cp) = storage.get_string("perse_egui.custom_config_path") {
                config.custom_config_path = Some(cp);
            }
            if let Some(open) = storage.get_string("perse_egui.is_display_options_open") {
                config.is_display_options_open = open.parse().unwrap_or(true);
            }
            if let Some(opts_json) = storage.get_string("perse_egui.canvas_display_options") {
                if let Ok(opts) = serde_json::from_str(&opts_json) {
                    config.canvas_display_options = opts;
                }
            }
            if let Some(mfp) = storage.get_string("perse_egui.mission_file_path") {
                if !mfp.trim().is_empty() {
                    config.mission_file_path = mfp;
                }
            }
            if let Some(ws) = storage.get_string("perse_egui.workspace_path") {
                if !ws.trim().is_empty() {
                    config.workspace_path = ws;
                }
            }
            if let Some(sets_json) = storage.get_string("perse_egui.mission_sets") {
                if let Ok(loaded_sets) =
                    serde_json::from_str::<crate::editor::models::MissionSetCollection>(&sets_json)
                {
                    if !loaded_sets.sets.is_empty() {
                        loaded_mission_sets = Some(loaded_sets);
                    }
                }
            }
        }

        // 2. Resolver o caminho de configuração customizado (CLI override tem prioridade)
        let target_path = cli_config_path
            .clone()
            .or_else(|| config.custom_config_path.clone());

        // 3. Tentar carregar do arquivo no caminho customizado se fornecido
        if let Some(ref path) = target_path {
            match crate::config::PersistentAppState::load_from_file(path) {
                Ok(loaded_state) => {
                    re_log::info!("Configurações e estado carregados com sucesso de: {}", path);
                    config = loaded_state.config;
                    config.custom_config_path = Some(path.clone());
                    loaded_mission_sets = Some(loaded_state.mission_sets);
                }
                Err(err) => {
                    re_log::warn!(
                        "Não foi possível carregar o arquivo de configuração em '{}': {}. Permanecendo com as configurações do caminho padrão.",
                        path,
                        err
                    );
                    if cli_config_path.is_some() {
                        config.custom_config_path = cli_config_path;
                    }
                }
            }
        }

        // 4. Se não veio do storage ou arquivo customizado, verificar disco (nativo)
        let mission_sets = loaded_mission_sets.unwrap_or_else(|| {
            #[cfg(not(target_arch = "wasm32"))]
            {
                if let Ok(col) =
                    crate::editor::models::MissionSetCollection::load_from_file("mission_sets.json")
                {
                    if !col.sets.is_empty() {
                        return col;
                    }
                }
                let target_file = if config.mission_file_path.trim().is_empty() {
                    "mission_points.yaml"
                } else {
                    &config.mission_file_path
                };
                let initial_data = crate::editor::models::MissionData::load_from_file(target_file)
                    .unwrap_or_default();
                let default_set = crate::editor::models::MissionSet::new(
                    "default",
                    "Missão Padrão",
                    initial_data,
                );
                crate::editor::models::MissionSetCollection {
                    active_set_id: "default".to_string(),
                    sets: vec![default_set],
                }
            }
            #[cfg(target_arch = "wasm32")]
            {
                crate::editor::models::MissionSetCollection::default()
            }
        });

        crate::editor::init_shared_mission_sets(mission_sets);
        crate::editor::set_shared_zoom_speed(config.zoom_speed);

        let rewire = RewireIntegration::create(cc, &config.endpoint)?;

        let mut mission_editor = MissionEditor::default();
        mission_editor.canvas_state.is_display_options_open = config.is_display_options_open;
        mission_editor.canvas_state.view_options = config.canvas_display_options.clone();
        mission_editor.file_path = config.mission_file_path.clone();

        #[allow(unused_mut)]
        let mut parameters_panel = ParametersPanel::default();
        #[allow(unused_mut)]
        let mut launchfiles_panel = LaunchfilesPanel::default();
        #[cfg(not(target_arch = "wasm32"))]
        {
            let scanned = crate::panels::parameters::workspace::scan_workspace_packages(
                &config.workspace_path,
            );
            if !scanned.is_empty() {
                re_log::info!(
                    "Carregados {} pacotes da workspace ROS '{}'",
                    scanned.len(),
                    config.workspace_path
                );
                parameters_panel.packages = scanned;
            }

            let scanned_launches =
                crate::panels::launchfiles::workspace::scan_workspace_launchfiles(
                    &config.workspace_path,
                );
            if !scanned_launches.is_empty() {
                re_log::info!(
                    "Carregados {} pacotes com launchfiles da workspace ROS '{}'",
                    scanned_launches.len(),
                    config.workspace_path
                );
                launchfiles_panel.packages = scanned_launches;
            }
        }

        let control_client = crate::net::client::ControlClient::new();
        let ws_url = crate::net::client::ControlClient::default_url(&config.endpoint);
        control_client.connect(&ws_url);

        Ok(Self {
            rewire,
            mission_editor,
            view_mode: ViewMode::default(),
            config,
            state: AppState::default(),
            parameters_panel,
            launchfiles_panel,
            terminals_panel: TerminalsPanel::default(),
            settings_panel: SettingsPanel::default(),
            control_client,
            was_connected: false,
            last_reconnect_attempt: 0.0,
        })
    }

    /// Dispara reconexão no cliente de controle e no visualizador Rerun/Rewire
    pub fn reconnect(&mut self) {
        re_log::info!("Disparando reconexão manual/automática...");
        self.control_client.reconnect();
        self.rewire.reconnect();
    }
}

impl App for MyApp {
    fn auto_save_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(5)
    }

    fn save(&mut self, storage: &mut dyn Storage) {
        self.rewire.save(storage);

        self.config.is_display_options_open =
            self.mission_editor.canvas_state.is_display_options_open;
        self.config.canvas_display_options = self.mission_editor.canvas_state.view_options.clone();
        self.config.mission_file_path = self.mission_editor.file_path.clone();

        let current_sets = crate::editor::get_shared_mission_sets()
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default();

        // 1. Salvar no armazenamento padrão do egui (cc.storage)
        storage.set_string("perse_egui.endpoint", self.config.endpoint.clone());
        storage.set_string(
            "perse_egui.show_sidebar",
            self.config.show_sidebar.to_string(),
        );
        storage.set_string(
            "perse_egui.show_right_sidebar",
            self.config.show_right_sidebar.to_string(),
        );
        storage.set_string(
            "perse_egui.theme_preference",
            self.config.theme_preference.clone(),
        );
        storage.set_string("perse_egui.zoom_speed", self.config.zoom_speed.to_string());
        storage.set_string(
            "perse_egui.is_display_options_open",
            self.config.is_display_options_open.to_string(),
        );
        storage.set_string(
            "perse_egui.mission_file_path",
            self.config.mission_file_path.clone(),
        );
        if let Ok(opts_json) = serde_json::to_string(&self.config.canvas_display_options) {
            storage.set_string("perse_egui.canvas_display_options", opts_json);
        }
        if let Some(ref cp) = self.config.custom_config_path {
            storage.set_string("perse_egui.custom_config_path", cp.clone());
        }
        if let Ok(sets_json) = serde_json::to_string(&current_sets) {
            storage.set_string("perse_egui.mission_sets", sets_json);
        }
        storage.set_string(
            "perse_egui.workspace_path",
            self.config.workspace_path.clone(),
        );

        // 2. No desktop nativo, salvar também diretamente em disco
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = current_sets.save_to_file("mission_sets.json");
            if let Some(active) = current_sets.active_data() {
                let target = if self.config.mission_file_path.trim().is_empty() {
                    "mission_points.yaml"
                } else {
                    self.config.mission_file_path.as_str()
                };
                let _ = active.save_to_file(target);
            }
        }

        // 3. Se um caminho de arquivo customizado estiver configurado, salvar também em arquivo JSON
        if let Some(ref path) = self.config.custom_config_path {
            if !path.trim().is_empty() {
                let persistent_state = crate::config::PersistentAppState {
                    config: self.config.clone(),
                    mission_sets: current_sets,
                };
                if let Err(err) = persistent_state.save_to_file(path) {
                    re_log::error!(
                        "Falha ao salvar estado no arquivo customizado em '{}': {}",
                        path,
                        err
                    );
                } else {
                    re_log::info!("Estado salvo com sucesso no arquivo customizado: {}", path);
                }
            }
        }
    }

    fn logic(&mut self, ctx: &Context, frame: &mut Frame) {
        self.rewire.logic(ctx, frame);

        let is_connected = self.control_client.is_connected();
        let now = ctx.input(|i| i.time);

        // Detecta quando a conexão é restabelecida com o Host
        if is_connected && !self.was_connected {
            self.was_connected = true;
            re_log::info!("Conexão com o Host restabelecida. Re-sincronizando Rerun e painéis...");
            self.rewire.reconnect();
            // Requisita dados atualizados do host para popular todos os painéis
            self.control_client
                .send(crate::net::protocol::ControlMessage::new(
                    crate::net::protocol::Domain::Parameters,
                    "get_tree",
                    serde_json::Value::Null,
                ));
            self.control_client
                .send(crate::net::protocol::ControlMessage::new(
                    crate::net::protocol::Domain::Launchfiles,
                    "get_list",
                    serde_json::Value::Null,
                ));
            self.control_client
                .send(crate::net::protocol::ControlMessage::new(
                    crate::net::protocol::Domain::Mission,
                    "get_data",
                    serde_json::Value::Null,
                ));
        } else if !is_connected {
            self.was_connected = false;
            // Garante repaints periódicos para que o temporizador continue rodando mesmo sem interação do mouse
            ctx.request_repaint_after(std::time::Duration::from_millis(1500));

            // Auto-reconexão periódica a cada 3 segundos caso não haja conexão em andamento
            if !self.control_client.is_connecting() && (now - self.last_reconnect_attempt) >= 3.0 {
                self.last_reconnect_attempt = now;
                self.reconnect();
            }
        }
    }

    fn ui(&mut self, ui: &mut Ui, frame: &mut Frame) {
        // 1. Processar e aplicar mensagens recebidas pela rede do plano de controle
        for msg in self.control_client.drain_messages() {
            match msg.domain {
                crate::net::protocol::Domain::Parameters => {
                    if msg.action == "tree_response" {
                        if let Ok(resp) = serde_json::from_value::<
                            crate::net::protocol::ParametersTreeResponse,
                        >(msg.payload)
                        {
                            if self.parameters_panel.refresh_in_progress {
                                self.parameters_panel.packages = resp.packages;
                                self.parameters_panel.refresh_in_progress = false;
                            } else {
                                self.parameters_panel.update_from_remote(resp.packages);
                            }
                        }
                    }
                }
                crate::net::protocol::Domain::Launchfiles => {
                    if msg.action == "list_response" {
                        if let Ok(resp) = serde_json::from_value::<
                            crate::net::protocol::LaunchfilesListResponse,
                        >(msg.payload)
                        {
                            self.launchfiles_panel.update_from_remote(resp.packages);
                        }
                    } else if msg.action == "status" {
                        if let Ok(notif) = serde_json::from_value::<
                            crate::net::protocol::LaunchfileStatusNotification,
                        >(msg.payload)
                        {
                            self.launchfiles_panel.update_file_status(
                                &notif.package,
                                &notif.filename,
                                notif.running,
                            );
                        }
                    }
                }
                crate::net::protocol::Domain::Mission => match msg.action.as_str() {
                    "data_response" | "data_updated" => {
                        if let Ok(resp) = serde_json::from_value::<
                            crate::net::protocol::MissionDataResponse,
                        >(msg.payload)
                        {
                            let local_sets = crate::editor::get_shared_mission_sets()
                                .lock()
                                .map(|g| g.clone())
                                .unwrap_or_default();
                            let local_has_points = local_sets.sets.len() > 1
                                || local_sets.active_data().is_some_and(|d| {
                                    !d.points.is_empty() || !d.obstacles.is_empty()
                                });
                            let remote_has_points = resp.collection.sets.len() > 1
                                || resp.collection.active_data().is_some_and(|d| {
                                    !d.points.is_empty() || !d.obstacles.is_empty()
                                });

                            if msg.action == "data_updated"
                                || remote_has_points
                                || !local_has_points
                            {
                                crate::editor::init_shared_mission_sets(resp.collection);
                            } else {
                                self.control_client.send(
                                    crate::net::protocol::ControlMessage::new(
                                        crate::net::protocol::Domain::Mission,
                                        "save_data",
                                        serde_json::to_value(
                                            crate::net::protocol::SaveMissionDataRequest {
                                                collection: local_sets,
                                                target_path: Some(
                                                    self.config.mission_file_path.clone(),
                                                ),
                                            },
                                        )
                                        .unwrap_or_default(),
                                    ),
                                );
                            }
                        }
                    }
                    "list_files_response" => {
                        if let Ok(resp) = serde_json::from_value::<
                            crate::net::protocol::ListFilesResponse,
                        >(msg.payload)
                        {
                            self.mission_editor.sidebar.remote_dialog.current_path =
                                resp.current_path;
                            self.mission_editor.sidebar.remote_dialog.parent_path =
                                resp.parent_path;
                            self.mission_editor.sidebar.remote_dialog.entries = resp.entries;
                            self.mission_editor.sidebar.remote_dialog.error_msg = None;
                        }
                    }
                    "load_file_response" => {
                        if let Ok(resp) = serde_json::from_value::<
                            crate::net::protocol::LoadMissionFileResponse,
                        >(msg.payload)
                        {
                            if resp.success {
                                if let Some(loaded_data) = resp.data {
                                    self.mission_editor.file_path = resp.path.clone();
                                    let sets_arc = crate::editor::get_shared_mission_sets();
                                    let mut sets_guard =
                                        sets_arc.lock().unwrap_or_else(|e| e.into_inner());
                                    let path = resp.path.clone();
                                    self.mission_editor.sidebar.apply_loaded_data(
                                        &mut sets_guard,
                                        path,
                                        loaded_data,
                                        &mut self.mission_editor.status_msg,
                                        &mut self.mission_editor.selection,
                                        Some(&self.control_client),
                                        &self.mission_editor.file_path,
                                    );
                                }
                            } else {
                                self.mission_editor.status_msg = resp.error.unwrap_or_else(|| {
                                    "Erro ao carregar arquivo do host".to_string()
                                });
                            }
                        }
                    }
                    "save_file_response" => {
                        if let Ok(resp) = serde_json::from_value::<
                            crate::net::protocol::SaveMissionFileResponse,
                        >(msg.payload)
                        {
                            if resp.success {
                                self.mission_editor.file_path = resp.path.clone();
                                crate::editor::notify_mission_saved();
                                self.mission_editor.status_msg =
                                    format!("Arquivo '{}' salvo com sucesso no host!", resp.path);
                            } else {
                                self.mission_editor.status_msg = resp.error.unwrap_or_else(|| {
                                    "Erro ao salvar arquivo no host".to_string()
                                });
                            }
                        }
                    }
                    "save_data_response" | "save_response" => {
                        if let Ok(resp) = serde_json::from_value::<
                            crate::net::protocol::SaveMissionDataResponse,
                        >(msg.payload)
                        {
                            if resp.success {
                                self.mission_editor.status_msg =
                                    "Conjunto de missões sincronizado com o host!".to_string();
                            } else {
                                self.mission_editor.status_msg =
                                    format!("Erro ao sincronizar com o host: {}", resp.message);
                            }
                        }
                    }
                    _ => {}
                },
                crate::net::protocol::Domain::Terminal => {
                    let now_ms = (ui.ctx().input(|i| i.time) * 1000.0) as u64;
                    match msg.action.as_str() {
                        "list_sync" => {
                            if let Ok(resp) = serde_json::from_value::<
                                crate::net::protocol::TerminalListSyncResponse,
                            >(msg.payload)
                            {
                                self.terminals_panel.apply_list_sync(resp, now_ms);
                                ui.ctx().request_repaint();
                            }
                        }
                        "output" => {
                            if let Ok(out_msg) = serde_json::from_value::<
                                crate::net::protocol::TerminalOutputMessage,
                            >(msg.payload)
                            {
                                self.terminals_panel.handle_output(out_msg, now_ms);
                                ui.ctx().request_repaint();
                            }
                        }
                        "status" => {
                            if let Ok(status_msg) = serde_json::from_value::<
                                crate::net::protocol::TerminalStatusMessage,
                            >(msg.payload)
                            {
                                self.terminals_panel.handle_status(status_msg);
                                ui.ctx().request_repaint();
                            }
                        }
                        "closed" => {
                            if let Ok(close_msg) = serde_json::from_value::<
                                crate::net::protocol::TerminalCloseRequest,
                            >(msg.payload)
                            {
                                self.terminals_panel.handle_closed(&close_msg.id);
                                ui.ctx().request_repaint();
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        // Enquanto o painel de terminais estiver aberto e conectado, solicita repaint periódico leve
        // para garantir que a saída assíncrona do PTY chegue imediatamente na tela mesmo sem movimento do mouse.
        if self.config.show_right_sidebar && !self.terminals_panel.sessions.is_empty() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(30));
        }

        // Barra superior de ferramentas
        egui::Panel::top("perse_toolbar").show(ui, |ui| {
            let toolbar_rect = ui.available_rect_before_wrap();
            ui.horizontal(|ui| {
                // Menu de contexto ao clicar em "Perse Egui ⏷" (semelhante ao rerun)
                ui.menu_button("Perse Egui ⏷", |ui| {
                    if crate::panels::components::re_icon_text_button(
                        ui,
                        &re_ui::icons::SETTINGS,
                        "Configurações",
                    )
                    .clicked()
                    {
                        self.settings_panel.open = true;
                        ui.close();
                    }
                });

                ui.separator();

                // Alternância do painel esquerdo ("Painéis") com destaque por cor (branco se aberto, cinza se fechado)
                if crate::panels::components::re_icon_toggle_button(
                    ui,
                    &re_ui::icons::LEFT_PANEL_TOGGLE,
                    "Painéis",
                    self.config.show_sidebar,
                )
                .clicked()
                {
                    self.config.show_sidebar = !self.config.show_sidebar;
                }

                ui.add_space(4.0);

                // Indicador do Host WebSocket à esquerda, logo após Painéis
                let is_conn = self.control_client.is_connected();
                let dot_color = if is_conn {
                    egui::Color32::from_rgb(50, 220, 100)
                } else {
                    egui::Color32::from_rgb(240, 180, 50)
                };

                let label = if is_conn {
                    "● Host Conectado".to_string()
                } else {
                    format!("○ {}", self.control_client.status_message())
                };
                ui.label(egui::RichText::new(label).color(dot_color).size(11.0))
                    .on_hover_text(if is_conn {
                        "Conectado ao Host do robô"
                    } else {
                        "Desconectado do Host. Tentativa automática em segundo plano a cada 3s."
                    });

                if !is_conn {
                    let is_connecting = self.control_client.is_connecting();
                    let btn_text = if is_connecting {
                        "⏳ Conectando..."
                    } else {
                        "🔄 Reconectar"
                    };
                    let reconnect_btn = ui.add_enabled(
                        !is_connecting,
                        egui::Button::new(
                            egui::RichText::new(btn_text)
                                .size(11.0)
                                .color(egui::Color32::from_rgb(220, 220, 220)),
                        ),
                    );
                    if reconnect_btn
                        .on_hover_text(
                            "Tentar reconectar ao Host WebSocket e ao stream Rerun agora",
                        )
                        .clicked()
                    {
                        let now = ui.ctx().input(|i| i.time);
                        self.last_reconnect_attempt = now;
                        self.reconnect();
                    }
                }

                // Extrema Direita: Terminais
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if crate::panels::components::re_icon_toggle_button(
                        ui,
                        &re_ui::icons::RIGHT_PANEL_TOGGLE,
                        "Terminais",
                        self.config.show_right_sidebar,
                    )
                    .clicked()
                    {
                        self.config.show_right_sidebar = !self.config.show_right_sidebar;
                    }
                });
            });

            // Centro: Editor 2D de Missão centralizado na barra superior
            let is_editor = self.view_mode == ViewMode::MissionEditor;
            let center_x = toolbar_rect.center().x;
            let btn_width = 175.0;
            let btn_height = 22.0;
            let center_pos = egui::pos2(
                (center_x - btn_width * 0.5).max(toolbar_rect.min.x),
                toolbar_rect.min.y + (toolbar_rect.height() - btn_height) * 0.5,
            );
            let mut center_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(egui::Rect::from_min_size(
                        center_pos,
                        egui::vec2(btn_width, btn_height),
                    ))
                    .layout(egui::Layout::centered_and_justified(
                        egui::Direction::LeftToRight,
                    )),
            );
            if crate::panels::components::re_icon_toggle_button(
                &mut center_ui,
                &re_ui::icons::VIEW_2D,
                "Editor 2D de Missão",
                is_editor,
            )
            .clicked()
            {
                self.view_mode = if is_editor {
                    ViewMode::RerunViewer
                } else {
                    ViewMode::MissionEditor
                };
            }
        });

        let panel_bg = ui.visuals().panel_fill;

        let window_w = ui.available_width();
        let left_max = (window_w - 60.0).clamp(40.0, 500.0);

        // Sidebar Esquerda (Parâmetros + Launchfiles) usando show_collapsible e size_range
        let mut show_sidebar = self.config.show_sidebar;
        egui::Panel::left("perse_left_sidebar")
            .resizable(true)
            .default_size(280.0)
            .size_range(40.0..=left_max)
            .frame(egui::Frame::new().fill(panel_bg).inner_margin(0.0))
            .show_collapsible(ui, &mut show_sidebar, |ui| {
                // Subpainel superior: Parâmetros (max_height garante que o cabeçalho do Launchfiles nunca fique oculto)
                let max_param_h = (ui.available_height() - 32.0).max(40.0);
                egui::Panel::top("sidebar_parameters_subpanel")
                    .resizable(true)
                    .default_size(220.0)
                    .max_size(max_param_h)
                    .frame(egui::Frame::new().fill(panel_bg).inner_margin(0.0))
                    .show(ui, |ui| {
                        self.parameters_panel.ui(
                            ui,
                            &self.state,
                            self.config.color_parameter_hierarchy,
                            Some(&self.control_client),
                            Some(&self.config.workspace_path),
                        );
                        if self.parameters_panel.request_open_settings {
                            self.parameters_panel.request_open_settings = false;
                            self.settings_panel.open = true;
                        }
                    });

                // Subpainel inferior: Launchfiles
                self.launchfiles_panel.ui(
                    ui,
                    &mut self.state,
                    Some(&self.control_client),
                    Some(&self.config),
                );
                if self.launchfiles_panel.request_open_settings {
                    self.launchfiles_panel.request_open_settings = false;
                    self.settings_panel.open = true;
                }
            });
        self.config.show_sidebar = show_sidebar;

        let right_max = (ui.available_width() - 40.0).clamp(40.0, 600.0);

        // Sidebar Direita (Terminais - Resizable) usando show_collapsible e size_range
        let mut show_right_sidebar = self.config.show_right_sidebar;
        egui::Panel::right("perse_right_sidebar")
            .resizable(true)
            .default_size(320.0)
            .size_range(40.0..=right_max)
            .frame(egui::Frame::new().fill(panel_bg).inner_margin(0.0))
            .show_collapsible(ui, &mut show_right_sidebar, |ui| {
                self.terminals_panel
                    .ui(ui, Some(&self.control_client), Some(&self.config));
            });
        self.config.show_right_sidebar = show_right_sidebar;

        // Modal de configurações
        let old_ws = self.config.workspace_path.clone();
        self.settings_panel.ui(ui.ctx(), &mut self.config);
        if self.config.workspace_path != old_ws {
            #[cfg(not(target_arch = "wasm32"))]
            {
                let scanned = crate::panels::parameters::workspace::scan_workspace_packages(
                    &self.config.workspace_path,
                );
                if !scanned.is_empty() {
                    self.parameters_panel.packages = scanned;
                }

                let scanned_launches =
                    crate::panels::launchfiles::workspace::scan_workspace_launchfiles(
                        &self.config.workspace_path,
                    );
                if !scanned_launches.is_empty() {
                    self.launchfiles_panel.packages = scanned_launches;
                }
            }
            self.control_client
                .send(crate::net::protocol::ControlMessage::new(
                    crate::net::protocol::Domain::Parameters,
                    "set_workspace",
                    serde_json::json!({ "workspace": self.config.workspace_path }),
                ));
        }
        crate::editor::set_shared_zoom_speed(self.config.zoom_speed);

        // Exibição central de acordo com o modo selecionado
        match self.view_mode {
            ViewMode::RerunViewer => {
                self.rewire.show(ui, frame);
            }
            ViewMode::MissionEditor => {
                self.mission_editor
                    .ui(ui, self.config.zoom_speed, Some(&self.control_client));
                self.config.mission_file_path = self.mission_editor.file_path.clone();
            }
        }
    }
}
