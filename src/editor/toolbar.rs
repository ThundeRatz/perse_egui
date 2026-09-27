use crate::editor::models::ObstacleKind;
use egui::{CornerRadius, Margin, Ui, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeType {
    Polygon,
    Line,
    Rectangle,
    Circle,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ActiveTool {
    Select,
    AddPoint,
    AddObstacle(ObstacleKind, ShapeType),
}

impl Default for ActiveTool {
    fn default() -> Self {
        Self::Select
    }
}

pub struct ToolBar;

impl ToolBar {
    pub fn show(ui: &mut Ui, active_tool: &mut ActiveTool) {
        let frame = egui::Frame::default()
            .fill(ui.visuals().window_fill)
            .stroke(ui.visuals().window_stroke)
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(6));

        frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);

                // 1. Tool: Select
                let is_select = matches!(active_tool, ActiveTool::Select);
                let btn_select = crate::panels::components::re_icon_toggle_button(
                    ui,
                    &re_ui::icons::INTERNAL_LINK,
                    "Seleção",
                    is_select,
                );
                if btn_select.clicked() {
                    *active_tool = ActiveTool::Select;
                }

                // 2. Tool: Add Point
                let is_add_point = matches!(active_tool, ActiveTool::AddPoint);
                let btn_add_point = crate::panels::components::re_icon_toggle_button(
                    ui,
                    &re_ui::icons::ADD,
                    "Adicionar Ponto",
                    is_add_point,
                );
                if btn_add_point.clicked() {
                    *active_tool = ActiveTool::AddPoint;
                }

                ui.separator();

                // 3. Tool: Add Obstacle (Dropdown)
                let is_add_obs = matches!(active_tool, ActiveTool::AddObstacle(_, _));
                let label_obs = match active_tool {
                    ActiveTool::AddObstacle(kind, shape) => {
                        let k_str = match kind {
                            ObstacleKind::Physical => "Físico",
                            ObstacleKind::Cosmetic => "Cosmético",
                        };
                        let s_str = match shape {
                            ShapeType::Polygon => "Polígono",
                            ShapeType::Line => "Linhas",
                            ShapeType::Rectangle => "Retângulo",
                            ShapeType::Circle => "Círculo",
                        };
                        format!("Obstáculo ({} - {})", k_str, s_str)
                    }
                    _ => "Adicionar Obstáculo ⏷".to_string(),
                };

                let obs_btn_color = if is_add_obs {
                    ui.visuals().strong_text_color()
                } else {
                    ui.visuals().text_color()
                };

                ui.menu_button(egui::RichText::new(label_obs).color(obs_btn_color), |ui| {
                    ui.label(egui::RichText::new("Selecione a variação:").strong());
                    ui.separator();

                    if ui.button("Físico - Polígono").clicked() {
                        *active_tool = ActiveTool::AddObstacle(ObstacleKind::Physical, ShapeType::Polygon);
                        ui.close();
                    }
                    if ui.button("Físico - Linhas").clicked() {
                        *active_tool = ActiveTool::AddObstacle(ObstacleKind::Physical, ShapeType::Line);
                        ui.close();
                    }
                    if ui.button("Físico - Retângulo").clicked() {
                        *active_tool = ActiveTool::AddObstacle(ObstacleKind::Physical, ShapeType::Rectangle);
                        ui.close();
                    }
                    if ui.button("Físico - Círculo").clicked() {
                        *active_tool = ActiveTool::AddObstacle(ObstacleKind::Physical, ShapeType::Circle);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Cosmético - Polígono").clicked() {
                        *active_tool = ActiveTool::AddObstacle(ObstacleKind::Cosmetic, ShapeType::Polygon);
                        ui.close();
                    }
                    if ui.button("Cosmético - Linhas").clicked() {
                        *active_tool = ActiveTool::AddObstacle(ObstacleKind::Cosmetic, ShapeType::Line);
                        ui.close();
                    }
                    if ui.button("Cosmético - Retângulo").clicked() {
                        *active_tool = ActiveTool::AddObstacle(ObstacleKind::Cosmetic, ShapeType::Rectangle);
                        ui.close();
                    }
                    if ui.button("Cosmético - Círculo").clicked() {
                        *active_tool = ActiveTool::AddObstacle(ObstacleKind::Cosmetic, ShapeType::Circle);
                        ui.close();
                    }
                });
            });
        });
    }
}
