use egui::{Frame, Margin, Ui};

use crate::panels::components::section_header;
use crate::{config::AppConfig, state::AppState};

#[derive(Debug, Default)]
pub struct LaunchfilesPanel;

impl LaunchfilesPanel {
    pub fn ui(&mut self, ui: &mut Ui, config: &mut AppConfig, state: &mut AppState) {
        section_header(ui, "Launchfiles", None, |ui| {
            if ui.small_button("🔍").clicked() {
                state.increment_action("Buscar no Blueprint");
            }
            if ui.small_button("⋯").clicked() {
                state.increment_action("Opções do Blueprint");
            }
        });

        Frame::new()
            .inner_margin(Margin {
                left: 8,
                right: 8,
                top: 6,
                bottom: 6,
            })
            .show(ui, |ui| {
                ui.checkbox(&mut config.show_sidebar, "Manter sidebar aberta");

                ui.add_space(8.0);
                ui.label(format!("Endpoint: {}", config.endpoint));

                ui.add_space(8.0);
                if ui.button("Executar ação rápida").clicked() {
                    state.increment_action("Ação executada no painel de controles");
                }
            });
    }
}
