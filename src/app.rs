use eframe::{App, Frame, Storage};
use egui::{Context, Ui};

use crate::{
    config::AppConfig,
    panels::{
        launchfiles::LaunchfilesPanel, parameters::ParametersPanel, settings::SettingsPanel,
        terminals::TerminalsPanel,
    },
    rewire_integration::RewireIntegration,
    state::AppState,
};

pub struct MyApp {
    rewire: RewireIntegration,
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
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let mut config = AppConfig::default();
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
        } else {
            config.endpoint = endpoint;
        }

        let rewire = RewireIntegration::create(cc, &config.endpoint)?;

        Ok(Self {
            rewire,
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
                        self.parameters_panel.ui(ui, &self.state);
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

        // Visualizador Rewire no painel central
        self.rewire.show(ui, frame);
    }
}
