use crate::{
    editor::models::{MissionData, ObstacleKind, ObstacleShape},
    panels::components::simple_section_header,
};
use egui::{Color32, Margin, RichText, Ui};
use indexmap::IndexMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    None,
    Point(usize),
    Obstacle(usize),
}

pub struct EditorSidebar {
    pub new_param_key: String,
    pub new_param_val: String,
    pub show_add_param_popup: bool,
}

impl Default for EditorSidebar {
    fn default() -> Self {
        Self {
            new_param_key: String::new(),
            new_param_val: String::new(),
            show_add_param_popup: false,
        }
    }
}

impl EditorSidebar {
    pub fn show(
        &mut self,
        ui: &mut Ui,
        data: &mut MissionData,
        selection: &mut Selection,
        file_path: &mut String,
        status_msg: &mut String,
    ) {
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    match selection {
                        Selection::None => {
                            simple_section_header(ui, "Propriedades", |_| {});
                            self.show_global_info(ui, data, file_path, status_msg);
                        }
                        Selection::Point(idx) => {
                            let idx = *idx;
                            if idx < data.points.len() {
                                self.show_point_properties(ui, data, idx, selection, status_msg);
                            } else {
                                *selection = Selection::None;
                            }
                        }
                        Selection::Obstacle(idx) => {
                            let idx = *idx;
                            if idx < data.obstacles.len() {
                                self.show_obstacle_properties(ui, data, idx, selection);
                            } else {
                                *selection = Selection::None;
                            }
                        }
                    }
                });
            });
    }

    fn show_global_info(
        &mut self,
        ui: &mut Ui,
        data: &mut MissionData,
        file_path: &mut String,
        status_msg: &mut String,
    ) {
        pad_content(ui, |ui| {
            ui.label(RichText::new("Arquivo de Missão").strong());
            ui.text_edit_singleline(file_path);

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("Carregar").clicked() {
                    match MissionData::load_from_file(&file_path) {
                        Ok(loaded) => {
                            *data = loaded;
                            *status_msg = format!("Arquivo '{}' carregado!", file_path);
                        }
                        Err(e) => {
                            *status_msg = format!("Erro ao carregar: {}", e);
                        }
                    }
                }
                if ui.button("Salvar").clicked() {
                    match data.save_to_file(&file_path) {
                        Ok(_) => {
                            *status_msg = format!("Arquivo '{}' salvo!", file_path);
                        }
                        Err(e) => {
                            *status_msg = format!("Erro ao salvar: {}", e);
                        }
                    }
                }
            });

            if !status_msg.is_empty() {
                ui.add_space(4.0);
                ui.weak(status_msg.as_str());
            }

            ui.add_space(12.0);
            ui.label(RichText::new("Resumo").strong());
            ui.label(format!("Pontos de missão: {}", data.points.len()));
            ui.label(format!("Obstáculos definidos: {}", data.obstacles.len()));
        });
    }

    fn show_point_properties(
        &mut self,
        ui: &mut Ui,
        data: &mut MissionData,
        idx: usize,
        selection: &mut Selection,
        status_msg: &mut String,
    ) {
        // Cabeçalho com o nome do marco (ex: "Marco 4") e botão Deselecionar à direita (largura total)
        simple_section_header(ui, &format!("Marco {}", idx), |ui| {
            if ui.small_button("❌ Deselecionar").clicked() {
                *selection = Selection::None;
            }
        });

        let mut move_up = false;
        let mut move_down = false;
        let mut delete_pt = false;
        let points_len = data.points.len();

        {
            let pt = &mut data.points[idx];

            // Conteúdo da posição e ações com padding interno em Grid de 2 colunas
            pad_content(ui, |ui| {
                egui::Grid::new(ui.id().with("point_core_grid"))
                    .num_columns(2)
                    .spacing([16.0, 6.0])
                    .min_col_width(100.0)
                    .show(ui, |ui| {
                        ui.label(RichText::new("Posição").color(Color32::from_gray(200)));
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("X:").color(Color32::from_gray(180)));
                            ui.add(egui::DragValue::new(&mut pt.x).speed(0.05));
                            ui.add_space(12.0);
                            ui.label(RichText::new("Y:").color(Color32::from_gray(180)));
                            ui.add(egui::DragValue::new(&mut pt.y).speed(0.05));
                        });
                        ui.end_row();
                    });

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if idx > 0 && ui.small_button("⬆ Mover para Cima").clicked() {
                        move_up = true;
                    }
                    if idx + 1 < points_len && ui.small_button("⬇ Mover para Baixo").clicked() {
                        move_down = true;
                    }
                    if ui.small_button("🗑 Excluir Ponto").clicked() {
                        delete_pt = true;
                    }
                });
            });

            // Cabeçalho de Parâmetros (largura total)
            simple_section_header(ui, "Parâmetros", |_| {});

            // Conteúdo dos parâmetros com padding interno
            pad_content(ui, |ui| {
                render_aligned_parameters(
                    ui,
                    &mut pt.extra,
                    &mut self.new_param_key,
                    &mut self.new_param_val,
                    &mut self.show_add_param_popup,
                );
            });
        }

        if move_up {
            data.points.swap(idx, idx - 1);
            *selection = Selection::Point(idx - 1);
        } else if move_down {
            data.points.swap(idx, idx + 1);
            *selection = Selection::Point(idx + 1);
        } else if delete_pt {
            data.points.remove(idx);
            *selection = Selection::None;
            *status_msg = format!("Marco {} removido.", idx);
        }
    }

    fn show_obstacle_properties(
        &mut self,
        ui: &mut Ui,
        data: &mut MissionData,
        idx: usize,
        selection: &mut Selection,
    ) {
        let obs_id = data.obstacles[idx].id.clone();
        // Cabeçalho com o nome do obstáculo e botão Deselecionar à direita (largura total)
        simple_section_header(ui, &format!("Obstáculo #{} ({})", idx, obs_id), |ui| {
            if ui.small_button("❌ Deselecionar").clicked() {
                *selection = Selection::None;
            }
        });

        let mut delete_obs = false;

        {
            let obs = &mut data.obstacles[idx];

            pad_content(ui, |ui| {
                egui::Grid::new(ui.id().with("obstacle_core_grid"))
                    .num_columns(2)
                    .spacing([16.0, 6.0])
                    .min_col_width(100.0)
                    .show(ui, |ui| {
                        ui.label(RichText::new("ID").color(Color32::from_gray(200)));
                        ui.text_edit_singleline(&mut obs.id);
                        ui.end_row();

                        ui.label(RichText::new("Tipo").color(Color32::from_gray(200)));
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut obs.kind, ObstacleKind::Physical, "Físico");
                            ui.selectable_value(&mut obs.kind, ObstacleKind::Cosmetic, "Cosmético");
                        });
                        ui.end_row();

                        match &mut obs.shape {
                            ObstacleShape::Polygon { vertices } => {
                                ui.label(RichText::new("Formato").color(Color32::from_gray(200)));
                                ui.label(format!("Polígono ({} vértices)", vertices.len()));
                                ui.end_row();

                                let mut vert_to_remove = None;
                                let vert_len = vertices.len();
                                for (i, v) in vertices.iter_mut().enumerate() {
                                    ui.label(RichText::new(format!("Vértice {}", i)).color(Color32::from_gray(180)));
                                    ui.horizontal(|ui| {
                                        ui.label("X:");
                                        ui.add(egui::DragValue::new(&mut v[0]).speed(0.05));
                                        ui.add_space(8.0);
                                        ui.label("Y:");
                                        ui.add(egui::DragValue::new(&mut v[1]).speed(0.05));
                                        if vert_len > 3 && ui.small_button("🗑").clicked() {
                                            vert_to_remove = Some(i);
                                        }
                                    });
                                    ui.end_row();
                                }
                                if let Some(vi) = vert_to_remove {
                                    vertices.remove(vi);
                                }
                            }
                            ObstacleShape::Rectangle {
                                x,
                                y,
                                width,
                                height,
                                rotation,
                            } => {
                                ui.label(RichText::new("Posição").color(Color32::from_gray(200)));
                                ui.horizontal(|ui| {
                                    ui.label("X:");
                                    ui.add(egui::DragValue::new(x).speed(0.05));
                                    ui.add_space(8.0);
                                    ui.label("Y:");
                                    ui.add(egui::DragValue::new(y).speed(0.05));
                                });
                                ui.end_row();

                                ui.label(RichText::new("Dimensões").color(Color32::from_gray(200)));
                                ui.horizontal(|ui| {
                                    ui.label("L:");
                                    ui.add(egui::DragValue::new(width).speed(0.05).range(0.01..=100.0));
                                    ui.add_space(8.0);
                                    ui.label("A:");
                                    ui.add(egui::DragValue::new(height).speed(0.05).range(0.01..=100.0));
                                });
                                ui.end_row();

                                ui.label(RichText::new("Rotação").color(Color32::from_gray(200)));
                                ui.horizontal(|ui| {
                                    ui.add(egui::DragValue::new(rotation).speed(0.05));
                                    ui.label("rad");
                                });
                                ui.end_row();
                            }
                            ObstacleShape::Circle { center, radius } => {
                                ui.label(RichText::new("Centro").color(Color32::from_gray(200)));
                                ui.horizontal(|ui| {
                                    ui.label("X:");
                                    ui.add(egui::DragValue::new(&mut center[0]).speed(0.05));
                                    ui.add_space(8.0);
                                    ui.label("Y:");
                                    ui.add(egui::DragValue::new(&mut center[1]).speed(0.05));
                                });
                                ui.end_row();

                                ui.label(RichText::new("Raio").color(Color32::from_gray(200)));
                                ui.add(egui::DragValue::new(radius).speed(0.05).range(0.01..=100.0));
                                ui.end_row();
                            }
                        }
                    });

                if matches!(obs.shape, ObstacleShape::Polygon { .. }) {
                    ui.add_space(6.0);
                    if ui.button("+ Adicionar Vértice").clicked() {
                        if let ObstacleShape::Polygon { vertices } = &mut obs.shape {
                            let last = vertices.last().cloned().unwrap_or([0.0, 0.0]);
                            vertices.push([last[0] + 0.5, last[1] + 0.5]);
                        }
                    }
                }

                ui.add_space(8.0);
                if ui.button("🗑 Excluir Obstáculo").clicked() {
                    delete_obs = true;
                }
            });

            simple_section_header(ui, "Parâmetros", |_| {});

            pad_content(ui, |ui| {
                render_aligned_parameters(
                    ui,
                    &mut obs.extra,
                    &mut self.new_param_key,
                    &mut self.new_param_val,
                    &mut self.show_add_param_popup,
                );
            });
        }

        if delete_obs {
            data.obstacles.remove(idx);
            *selection = Selection::None;
        }
    }
}

/// Conteúdo de seção envolvido por Frame de Padding Interno de 10px
fn pad_content(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .inner_margin(Margin {
            left: 10,
            right: 10,
            top: 6,
            bottom: 6,
        })
        .show(ui, add_contents);
}

/// Renderiza os parâmetros organizados em colunas perfeitamente alinhadas:
/// `nome      valor      [botão de excluir no hover no extremo direito]`
fn render_aligned_parameters(
    ui: &mut Ui,
    extra: &mut IndexMap<String, serde_yaml::Value>,
    new_key: &mut String,
    new_val: &mut String,
    show_popup: &mut bool,
) {
    let mut to_remove = None;
    let grid_id = ui.id().with("params_grid");

    egui::Grid::new(grid_id)
        .num_columns(2)
        .spacing([16.0, 6.0])
        .min_col_width(100.0)
        .show(ui, |ui| {
            for (k, v) in extra.iter_mut() {
                let row_min_y = ui.cursor().min.y - 2.0;
                let is_hovered = ui.input(|i| i.pointer.hover_pos()).map_or(false, |pos| {
                    pos.y >= row_min_y && pos.y <= row_min_y + 24.0
                });

                // Coluna 1: Nome do parâmetro alinhado à esquerda
                ui.label(RichText::new(k).color(Color32::from_gray(200)));

                // Coluna 2: Valor do parâmetro (alinhado verticalmente para todas as linhas) + Botão de exclusão à direita
                ui.horizontal(|ui| {
                    render_dynamic_param_val(ui, v);

                    if is_hovered {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("🗑").on_hover_text("Remover parâmetro").clicked() {
                                to_remove = Some(k.clone());
                            }
                        });
                    }
                });

                ui.end_row();
            }
        });

    if let Some(k) = to_remove {
        extra.shift_remove(&k);
    }

    ui.add_space(8.0);
    render_add_param_controls(ui, extra, new_key, new_val, show_popup);
}

fn render_dynamic_param_val(ui: &mut Ui, val: &mut serde_yaml::Value) {
    match val {
        serde_yaml::Value::Bool(b) => {
            ui.checkbox(b, "");
        }
        serde_yaml::Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                let mut float_val = f;
                if ui.add(egui::DragValue::new(&mut float_val).speed(0.05)).changed() {
                    *val = serde_yaml::Value::Number(serde_yaml::Number::from(float_val));
                }
            }
        }
        serde_yaml::Value::String(s) => {
            ui.text_edit_singleline(s);
        }
        _ => {
            ui.label(format!("{:?}", val));
        }
    }
}

fn render_add_param_controls(
    ui: &mut Ui,
    extra: &mut IndexMap<String, serde_yaml::Value>,
    new_key: &mut String,
    new_val: &mut String,
    show_popup: &mut bool,
) {
    if *show_popup {
        ui.group(|ui| {
            ui.label(RichText::new("Novo Parâmetro").strong());
            ui.horizontal(|ui| {
                ui.label("Chave:");
                ui.text_edit_singleline(new_key);
            });
            ui.horizontal(|ui| {
                ui.label("Valor:");
                ui.text_edit_singleline(new_val);
            });
            ui.horizontal(|ui| {
                if ui.button("Adicionar").clicked() {
                    let key = new_key.trim().to_string();
                    let val_str = new_val.trim();
                    if !key.is_empty() {
                        let parsed_val = if let Ok(b) = val_str.parse::<bool>() {
                            serde_yaml::Value::Bool(b)
                        } else if let Ok(f) = val_str.parse::<f64>() {
                            serde_yaml::Value::Number(serde_yaml::Number::from(f))
                        } else {
                            serde_yaml::Value::String(val_str.to_string())
                        };
                        extra.insert(key, parsed_val);
                        new_key.clear();
                        new_val.clear();
                        *show_popup = false;
                    }
                }
                if ui.button("Cancelar").clicked() {
                    *show_popup = false;
                }
            });
        });
    } else if ui.button("➕ Adicionar Parâmetro").clicked() {
        *show_popup = true;
    }
}
