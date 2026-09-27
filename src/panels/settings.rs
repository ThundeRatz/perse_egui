use egui::Context;

use crate::config::AppConfig;

#[derive(Debug, Default)]
pub struct SettingsPanel {
    pub open: bool,
}

impl SettingsPanel {
    pub fn ui(&mut self, ctx: &Context, config: &mut AppConfig) {
        if !self.open {
            return;
        }

        let mut open = self.open;
        egui::Window::new("Configurações")
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label("Endpoint do Relay:");
                ui.text_edit_singleline(&mut config.endpoint);

                ui.add_space(8.0);
                ui.checkbox(&mut config.show_sidebar, "Exibir barra lateral por padrão");

                ui.add_space(8.0);
                ui.label("Sensibilidade do Zoom (Editor 2D):");
                ui.add(egui::Slider::new(&mut config.zoom_speed, 0.01..=0.15).text("Velocidade do Scroll"));
            });
        self.open = open;
    }
}
