use egui::Context;
#[cfg(not(target_arch = "wasm32"))]
use egui_file_dialog::FileDialog;

use crate::config::AppConfig;

pub struct SettingsPanel {
    pub open: bool,
    #[cfg(not(target_arch = "wasm32"))]
    file_dialog: FileDialog,
}

impl Default for SettingsPanel {
    fn default() -> Self {
        Self {
            open: false,
            #[cfg(not(target_arch = "wasm32"))]
            file_dialog: FileDialog::new(),
        }
    }
}

impl SettingsPanel {
    pub fn ui(&mut self, ctx: &Context, config: &mut AppConfig) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.file_dialog.update(ctx);

            if let Some(path) = self.file_dialog.take_picked() {
                config.custom_config_path = Some(path.to_string_lossy().to_string());
            }
        }

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
                ui.label("Sensibilidade do Zoom (Editor 2D):");
                ui.add(
                    egui::Slider::new(&mut config.zoom_speed, 0.005..=0.1)
                        .text("Velocidade do Scroll"),
                );

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);
                ui.checkbox(
                    &mut config.color_parameter_hierarchy,
                    "Colorir hierarquia de parâmetros (pacote, arquivo e escopo)",
                );

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);
                ui.label("Caminho do Arquivo de Persistência / Configuração:");
                ui.horizontal(|ui| {
                    let mut path_str = config.custom_config_path.clone().unwrap_or_default();
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut path_str)
                                .hint_text("ex: /caminho/para/config.json"),
                        )
                        .changed()
                    {
                        if path_str.trim().is_empty() {
                            config.custom_config_path = None;
                        } else {
                            config.custom_config_path = Some(path_str);
                        }
                    }

                    #[cfg(not(target_arch = "wasm32"))]
                    if ui.button("📂 Procurar...").clicked() {
                        self.file_dialog.pick_file();
                    }
                });
                ui.label(
                    egui::RichText::new("Deixe em branco para usar o caminho padrão do egui")
                        .small()
                        .color(egui::Color32::from_gray(140)),
                );
            });
        self.open = open;
    }
}
