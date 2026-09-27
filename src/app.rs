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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    RerunViewer,
    MissionEditor,
}

impl Default for ViewMode {
    fn default() -> Self {
        Self::RerunViewer
    }
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
}

impl MyApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        endpoint: String,
        cli_config_path: Option<String>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let mut config = AppConfig::default();
        let mut mission_sets = crate::editor::models::MissionSetCollection::default();

        // 1. Tentar ler as configurações salvas no armazenamento padrão do egui (cc.storage)
        if let Some(storage) = cc.storage {
            if let Some(ep) = storage.get_string("perse_egui.endpoint") {
                config.endpoint = ep;
            } else {
                config.endpoint = endpoint;
            }
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
            if let Some(sets_json) = storage.get_string("perse_egui.mission_sets") {
                if let Ok(loaded_sets) = serde_json::from_str(&sets_json) {
                    mission_sets = loaded_sets;
                }
            }
        } else {
            config.endpoint = endpoint;
        }

        // 2. Resolver o caminho de configuração customizado (CLI override tem prioridade)
        let target_path = cli_config_path.clone().or_else(|| config.custom_config_path.clone());

        // 3. Tentar carregar do arquivo no caminho customizado se fornecido
        if let Some(ref path) = target_path {
            match crate::config::PersistentAppState::load_from_file(path) {
                Ok(loaded_state) => {
                    re_log::info!("Configurações e estado carregados com sucesso de: {}", path);
                    config = loaded_state.config;
                    config.custom_config_path = Some(path.clone());
                    mission_sets = loaded_state.mission_sets;
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

        crate::editor::init_shared_mission_sets(mission_sets);
        crate::editor::set_shared_zoom_speed(config.zoom_speed);

        let rewire = RewireIntegration::create(cc, &config.endpoint)?;

        let mut mission_editor = MissionEditor::default();
        mission_editor.canvas_state.is_display_options_open = config.is_display_options_open;
        mission_editor.canvas_state.view_options = config.canvas_display_options.clone();

        Ok(Self {
            rewire,
            mission_editor,
            view_mode: ViewMode::default(),
            config,
            state: AppState::default(),
            parameters_panel: ParametersPanel::default(),
            launchfiles_panel: LaunchfilesPanel::default(),
            terminals_panel: TerminalsPanel::default(),
            settings_panel: SettingsPanel::default(),
        })
    }
}

impl App for MyApp {
    fn save(&mut self, storage: &mut dyn Storage) {
        self.rewire.save(storage);

        self.config.is_display_options_open = self.mission_editor.canvas_state.is_display_options_open;
        self.config.canvas_display_options = self.mission_editor.canvas_state.view_options.clone();

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
        storage.set_string(
            "perse_egui.zoom_speed",
            self.config.zoom_speed.to_string(),
        );
        storage.set_string(
            "perse_egui.is_display_options_open",
            self.config.is_display_options_open.to_string(),
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

        // 2. Se um caminho de arquivo customizado estiver configurado, salvar também em arquivo JSON
        if let Some(ref path) = self.config.custom_config_path {
            if !path.trim().is_empty() {
                let persistent_state = crate::config::PersistentAppState {
                    config: self.config.clone(),
                    mission_sets: current_sets,
                };
                if let Err(err) = persistent_state.save_to_file(path) {
                    re_log::error!("Falha ao salvar estado no arquivo customizado em '{}': {}", path, err);
                } else {
                    re_log::info!("Estado salvo com sucesso no arquivo customizado: {}", path);
                }
            }
        }
    }

    fn logic(&mut self, ctx: &Context, frame: &mut Frame) {
        self.rewire.logic(ctx, frame);
    }

    fn ui(&mut self, ui: &mut Ui, frame: &mut Frame) {
        // Barra superior de ferramentas
        egui::Panel::top("perse_toolbar").show(ui, |ui| {
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

                // Alternância do painel direito ("Terminais") com destaque por cor (branco se aberto, cinza se fechado)
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

                ui.separator();

                // Alternância entre Rerun Viewer e Editor 2D de Missão
                let is_editor = self.view_mode == ViewMode::MissionEditor;
                if crate::panels::components::re_icon_toggle_button(
                    ui,
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
        });

        let panel_bg = ui.visuals().panel_fill;

        let window_w = ui.available_width();
        let left_max = (window_w - 60.0).clamp(40.0, 500.0);

        // Sidebar Esquerda (Parâmetros + Launchfiles) usando show_collapsible e size_range
        egui::Panel::left("perse_left_sidebar")
            .resizable(true)
            .default_size(280.0)
            .size_range(40.0..=left_max)
            .frame(egui::Frame::new().fill(panel_bg).inner_margin(0.0))
            .show_collapsible(ui, &mut self.config.show_sidebar, |ui| {
                // Subpainel superior: Parâmetros (max_height garante que o cabeçalho do Launchfiles nunca fique oculto)
                let max_param_h = (ui.available_height() - 32.0).max(40.0);
                egui::Panel::top("sidebar_parameters_subpanel")
                    .resizable(true)
                    .default_size(220.0)
                    .max_size(max_param_h)
                    .frame(egui::Frame::new().fill(panel_bg).inner_margin(0.0))
                    .show(ui, |ui| {
                        self.parameters_panel.ui(ui, &self.state, self.config.color_parameter_hierarchy);
                    });

                // Subpainel inferior: Launchfiles
                self.launchfiles_panel.ui(ui, &mut self.state);
            });

        let right_max = (ui.available_width() - 40.0).clamp(40.0, 600.0);

        // Sidebar Direita (Terminais - Resizable) usando show_collapsible e size_range
        egui::Panel::right("perse_right_sidebar")
            .resizable(true)
            .default_size(320.0)
            .size_range(40.0..=right_max)
            .frame(egui::Frame::new().fill(panel_bg).inner_margin(0.0))
            .show_collapsible(ui, &mut self.config.show_right_sidebar, |ui| {
                self.terminals_panel.ui(ui);
            });

        // Modal de configurações
        self.settings_panel.ui(ui.ctx(), &mut self.config);
        crate::editor::set_shared_zoom_speed(self.config.zoom_speed);

        // Exibição central de acordo com o modo selecionado
        match self.view_mode {
            ViewMode::RerunViewer => {
                self.rewire.show(ui, frame);
            }
            ViewMode::MissionEditor => {
                self.mission_editor.ui(ui, self.config.zoom_speed);
            }
        }
    }
}
