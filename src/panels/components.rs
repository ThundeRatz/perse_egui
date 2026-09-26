use egui::{Color32, Frame, Margin, RichText, Ui};

/// Cabeçalho customizado com título, ícone opcional, fundo cinza e área para botões à direita.
pub fn section_header(
    ui: &mut Ui,
    title: &str,
    icon: Option<&str>,
    add_buttons: impl FnOnce(&mut Ui),
) {
    let bg_color = Color32::from_gray(32);

    Frame::new()
        .fill(bg_color)
        .inner_margin(Margin {
            left: 8,
            right: 8,
            top: 4,
            bottom: 4,
        })
        .show(ui, |ui| {
            ui.set_height(24.0);
            ui.horizontal_centered(|ui| {
                if let Some(ic) = icon {
                    ui.label(RichText::new(ic).size(13.0).color(Color32::from_gray(180)));
                }
                ui.label(
                    RichText::new(title)
                        .strong()
                        .size(13.0)
                        .color(Color32::from_gray(230)),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    add_buttons(ui);
                });
            });
        });
}
