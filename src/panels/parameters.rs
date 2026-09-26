use egui::{Frame, Margin, Ui};

use crate::panels::components::section_header;
use crate::state::AppState;

#[derive(Debug, Default)]
pub struct ParametersPanel;

impl ParametersPanel {
    pub fn ui(&mut self, ui: &mut Ui, state: &AppState) {
        section_header(ui, "Parâmetros", None, |_ui| {});

        Frame::new()
            .inner_margin(Margin {
                left: 8,
                right: 8,
                top: 6,
                bottom: 6,
            })
            .show(ui, |ui| {
                ui.label(format!("Ações executadas: {}", state.action_count));
                ui.add_space(4.0);
                if !state.status_message.is_empty() {
                    ui.label(format!("Último status: {}", state.status_message));
                } else {
                    ui.label("Status: OK");
                }
            });
    }
}
