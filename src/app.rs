use eframe::{App, Frame, Storage};
use egui::{Context, Ui};

use crate::{
    config::AppConfig,
    panels::{launchfiles::LaunchfilesPanel, parameters::ParametersPanel, settings::SettingsPanel},
    rewire_integration::RewireIntegration,
    state::AppState,
};

pub struct MyApp {
    rewire: RewireIntegration,
    config: AppConfig,
    state: AppState,
    launchfiles_panel: LaunchfilesPanel,
    parameters_panel: ParametersPanel,
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
            launchfiles_panel: LaunchfilesPanel,
            parameters_panel: ParametersPanel,
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
            "perse_egui.theme_preference",
            self.config.theme_preference.clone(),
        );
    }

    fn logic(&mut self, ctx: &Context, frame: &mut Frame) {
        self.rewire.logic(ctx, frame);
    }

    fn ui(&mut self, ui: &mut Ui, frame: &mut Frame) {
        egui::Panel::top("perse_toolbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Perse Egui");
                ui.separator();
                ui.label(format!("Relay: {}", self.config.endpoint));
                ui.separator();

                let toggle_text = if self.config.show_sidebar {
                    "Ocultar Sidebar"
                } else {
                    "Exibir Sidebar"
                };
                if ui.button(toggle_text).clicked() {
                    self.config.show_sidebar = !self.config.show_sidebar;
                }

                if ui.button("Configurações").clicked() {
                    self.settings_panel.open = !self.settings_panel.open;
                }
            });
        });

        if self.config.show_sidebar {
            let panel_bg = ui.visuals().panel_fill;
            egui::Panel::left("perse_sidebar")
                .resizable(true)
                .default_size(260.0)
                .frame(egui::Frame::new().fill(panel_bg).inner_margin(0.0))
                .show(ui, |ui| {
                    egui::Panel::top("sidebar_parameters_subpanel")
                        .resizable(true)
                        .default_size(150.0)
                        .frame(egui::Frame::new().fill(panel_bg).inner_margin(0.0))
                        .show(ui, |ui| {
                            self.parameters_panel.ui(ui, &self.state);
                            ui.allocate_space(ui.available_size());
                        });

                    egui::ScrollArea::vertical()
                        .id_salt("launchfiles_scroll")
                        .show(ui, |ui| {
                            self.launchfiles_panel
                                .ui(ui, &mut self.config, &mut self.state);
                        });
                });
        }

        self.settings_panel.ui(ui.ctx(), &mut self.config);

        self.rewire.show(ui, frame);
    }
}
