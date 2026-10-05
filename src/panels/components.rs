use egui::{Color32, Frame, Margin, RichText, Ui};

/// Cabeçalho customizado com título, modo de busca inline ocupando toda a largura disponível, fundo cinza e área para botões à direita.
pub fn section_header(
    ui: &mut Ui,
    title: &str,
    search_active: &mut bool,
    search_query: &mut String,
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
            if *search_active {
                // Modo de Busca: exibe ícone de busca do re_ui à esquerda, botão de fechar no extremo direito e campo de texto expandido
                ui.horizontal_centered(|ui| {
                    let search_img = re_ui::icons::SEARCH
                        .as_image()
                        .fit_to_exact_size(egui::vec2(12.0, 12.0));
                    ui.add(search_img);

                    let mut close_clicked = false;

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if re_icon_button(ui, &re_ui::icons::CLOSE_SMALL, "Fechar busca").clicked()
                        {
                            close_clicked = true;
                        }

                        let text_width = ui.available_width();
                        let response = ui.add(
                            egui::TextEdit::singleline(search_query)
                                .hint_text("Filtrar...")
                                .desired_width(text_width),
                        );

                        let focus_id = ui.id().with("search_focused");
                        let was_focused: bool =
                            ui.data_mut(|d| d.get_temp(focus_id)).unwrap_or(false);
                        if !was_focused {
                            response.request_focus();
                            ui.data_mut(|d| d.insert_temp(focus_id, true));
                        }
                    });

                    if close_clicked {
                        *search_active = false;
                        search_query.clear();
                        let focus_id = ui.id().with("search_focused");
                        ui.data_mut(|d| d.insert_temp(focus_id, false));
                    }
                });
            } else {
                // Modo Normal: exibe título do painel à esquerda e ações alinhadas à direita
                ui.horizontal_centered(|ui| {
                    ui.label(
                        RichText::new(title)
                            .strong()
                            .size(13.0)
                            .color(Color32::from_gray(230)),
                    );

                    // Botões alinhados à direita (a lupa fica no extremo direito para alinhar com o botão de fechar)
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if re_icon_button(ui, &re_ui::icons::SEARCH, "Buscar neste painel")
                            .clicked()
                        {
                            *search_active = true;
                            let focus_id = ui.id().with("search_focused");
                            ui.data_mut(|d| d.insert_temp(focus_id, false));
                        }

                        add_buttons(ui);
                    });
                });
            }
        });
}

/// Cabeçalho simples de seção estilo Rerun (fundo cinza escuro com título e ações alinhadas à direita).
pub fn simple_section_header(ui: &mut Ui, title: &str, add_buttons: impl FnOnce(&mut Ui)) {
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
            ui.set_height(22.0);
            ui.horizontal_centered(|ui| {
                ui.label(
                    RichText::new(title)
                        .strong()
                        .size(13.0)
                        .color(Color32::from_gray(230)),
                );

                ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    add_buttons,
                );
            });
        });
}

/// Utilitário para renderizar botão simples com ícone do `re_ui::icons`
pub fn re_icon_button(ui: &mut Ui, icon: &re_ui::Icon, tooltip: &str) -> egui::Response {
    let img = icon.as_image().fit_to_exact_size(egui::vec2(12.0, 12.0));
    let res = ui.add(egui::Button::image(img));
    if !tooltip.is_empty() {
        res.on_hover_text(tooltip)
    } else {
        res
    }
}

/// Utilitário para renderizar botão com ícone do `re_ui::icons` e texto
pub fn re_icon_text_button(ui: &mut Ui, icon: &re_ui::Icon, text: &str) -> egui::Response {
    let img = icon.as_image().fit_to_exact_size(egui::vec2(13.0, 13.0));
    ui.add(egui::Button::image_and_text(img, text))
}

/// Utilitário para renderizar botão de alternância (toggle) com ícone do `re_ui::icons` e texto.
/// Quando `is_open == true`, o ícone e o texto ficam brancos.
/// Quando `is_open == false`, ficam atenuados/esmaecidos em cinza.
pub fn re_icon_toggle_button(
    ui: &mut Ui,
    icon: &re_ui::Icon,
    text: &str,
    is_open: bool,
) -> egui::Response {
    let tint = if is_open {
        Color32::WHITE
    } else {
        Color32::from_gray(110)
    };

    let text_color = if is_open {
        Color32::WHITE
    } else {
        Color32::from_gray(130)
    };

    let img = icon
        .as_image()
        .fit_to_exact_size(egui::vec2(13.0, 13.0))
        .tint(tint);

    let btn = egui::Button::image_and_text(img, RichText::new(text).color(text_color).size(12.0))
        .fill(Color32::TRANSPARENT);

    ui.add(btn)
}
