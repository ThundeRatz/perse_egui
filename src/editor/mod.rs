pub mod canvas;
pub mod models;
pub mod sidebar;
pub mod space_view;
pub mod toolbar;

use canvas::{CanvasState, EditorCanvas};
use models::MissionData;
use sidebar::{EditorSidebar, Selection};
use toolbar::{ActiveTool, ToolBar};

pub struct MissionEditor {
    pub data: MissionData,
    pub canvas_state: CanvasState,
    pub selection: Selection,
    pub active_tool: ActiveTool,
    pub sidebar: EditorSidebar,
    pub file_path: String,
    pub status_msg: String,
}

impl Default for MissionEditor {
    fn default() -> Self {
        let file_path = "mission_points.yaml".to_string();
        let mut data = MissionData::default();
        let mut status_msg = String::new();

        if let Ok(loaded) = MissionData::load_from_file(&file_path) {
            data = loaded;
            status_msg = format!("Carregado de {}", file_path);
        }

        Self {
            data,
            canvas_state: CanvasState::default(),
            selection: Selection::None,
            active_tool: ActiveTool::Select,
            sidebar: EditorSidebar::default(),
            file_path,
            status_msg,
        }
    }
}

impl MissionEditor {
    pub fn ui(&mut self, ui: &mut egui::Ui, zoom_speed: f32) {
        let panel_bg = ui.visuals().panel_fill;

        // Painel Lateral Direito de Propriedades do Editor
        egui::Panel::right("editor_properties_sidebar")
            .resizable(true)
            .default_size(280.0)
            .frame(egui::Frame::new().fill(panel_bg).inner_margin(0.0))
            .show(ui, |ui| {
                self.sidebar.show(
                    ui,
                    &mut self.data,
                    &mut self.selection,
                    &mut self.file_path,
                    &mut self.status_msg,
                );
            });

        // Área Central do Canvas 2D
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(panel_bg).inner_margin(0.0))
            .show(ui, |ui| {
            let canvas_rect = ui.available_rect_before_wrap();

            // Renderizar Canvas 2D
            EditorCanvas::show(
                ui,
                &mut self.canvas_state,
                &mut self.data,
                &mut self.selection,
                &mut self.active_tool,
                None,
                zoom_speed,
            );

            // Overlay: Ilha de Botões Flutuantes ("Button Island") no Canto Inferior Esquerdo
            let island_rect = egui::Rect::from_min_size(
                canvas_rect.left_bottom() + egui::vec2(16.0, -56.0),
                egui::vec2(320.0, 40.0),
            );

            ui.put(island_rect, |ui: &mut egui::Ui| {
                ToolBar::show(ui, &mut self.active_tool);
                ui.allocate_response(egui::Vec2::ZERO, egui::Sense::hover())
            });
        });
    }
}
