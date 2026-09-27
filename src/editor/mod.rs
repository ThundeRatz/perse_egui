pub mod canvas;
pub mod models;
pub mod sidebar;
pub mod space_view;
pub mod toolbar;

use canvas::{CanvasState, EditorCanvas};
use models::{MissionData, MissionSetCollection};
use sidebar::{EditorSidebar, Selection};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use toolbar::{ActiveTool, ToolBar};

static SHARED_MISSION_SETS: OnceLock<Arc<Mutex<MissionSetCollection>>> = OnceLock::new();
static SHARED_ZOOM_SPEED: AtomicU32 = AtomicU32::new(0);

pub fn get_shared_mission_sets() -> Arc<Mutex<MissionSetCollection>> {
    SHARED_MISSION_SETS
        .get_or_init(|| Arc::new(Mutex::new(MissionSetCollection::default())))
        .clone()
}

pub fn init_shared_mission_sets(sets: MissionSetCollection) {
    let shared = get_shared_mission_sets();
    let mut guard = shared.lock().unwrap_or_else(|e| e.into_inner());
    *guard = sets;
}

pub fn get_shared_zoom_speed() -> f32 {
    let bits = SHARED_ZOOM_SPEED.load(Ordering::Relaxed);
    if bits == 0 {
        0.005
    } else {
        f32::from_bits(bits)
    }
}

pub fn set_shared_zoom_speed(speed: f32) {
    SHARED_ZOOM_SPEED.store(speed.to_bits(), Ordering::Relaxed);
}

pub struct MissionEditor {
    pub canvas_state: CanvasState,
    pub selection: Selection,
    pub active_tool: ActiveTool,
    pub sidebar: EditorSidebar,
    pub show_sidebar: bool,
    pub file_path: String,
    pub status_msg: String,
    pub is_renaming_set: bool,
    pub rename_input: String,
}

impl Default for MissionEditor {
    fn default() -> Self {
        let file_path = "mission_points.yaml".to_string();
        let status_msg = format!("Carregado de {}", file_path);

        Self {
            canvas_state: CanvasState::default(),
            selection: Selection::default(),
            active_tool: ActiveTool::Select,
            sidebar: EditorSidebar::default(),
            show_sidebar: true,
            file_path,
            status_msg,
            is_renaming_set: false,
            rename_input: String::new(),
        }
    }
}

impl MissionEditor {
    pub fn ui(&mut self, ui: &mut egui::Ui, zoom_speed: f32) {
        let panel_bg = ui.visuals().panel_fill;
        let sets_arc = get_shared_mission_sets();
        let mut sets_guard = sets_arc.lock().unwrap_or_else(|e| e.into_inner());

        let default_disk_data = MissionData::load_from_file("mission_points.yaml").unwrap_or_default();
        let is_different = if let Some(active_data) = sets_guard.active_data() {
            active_data != &default_disk_data
        } else {
            false
        };

        // Tratamento do Atalho Ctrl+S (apenas no editor 2D)
        let ctrl_s_pressed = ui.input_mut(|i| {
            i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::COMMAND,
                egui::Key::S,
            ))
        });

        if ctrl_s_pressed {
            if let Some(active_data) = sets_guard.active_data() {
                let save_target = if self.file_path.trim().is_empty() {
                    "mission_points.yaml"
                } else {
                    &self.file_path
                };
                match active_data.save_to_file(save_target) {
                    Ok(_) => {
                        self.status_msg = format!("Conjunto salvo em '{}'! (Ctrl+S)", save_target);
                    }
                    Err(e) => {
                        self.status_msg = format!("Erro ao salvar via Ctrl+S: {}", e);
                    }
                }
            }
        }

        let sidebar_id = ui.make_persistent_id("editor_properties_sidebar");

        // Painel Lateral Direito de Propriedades do Editor (Collapsible)
        egui::Panel::right(sidebar_id)
            .resizable(true)
            .default_size(280.0)
            .frame(egui::Frame::new().fill(panel_bg).inner_margin(0.0))
            .show_collapsible(ui, &mut self.show_sidebar, |ui| {
                self.sidebar.show(
                    ui,
                    &mut sets_guard,
                    &mut self.selection,
                    &mut self.canvas_state.view_options,
                    &mut self.file_path,
                    &mut self.status_msg,
                );
            });

        // Área Central do Canvas 2D
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(panel_bg).inner_margin(0.0))
            .show(ui, |ui| {
                let canvas_rect = ui.available_rect_before_wrap();

                if let Some(data) = sets_guard.active_data_mut() {
                    // Renderizar Canvas 2D
                    EditorCanvas::show(
                        ui,
                        &mut self.canvas_state,
                        data,
                        &mut self.selection,
                        &mut self.active_tool,
                        &mut self.show_sidebar,
                        None,
                        zoom_speed,
                    );
                }

                // Overlay: Ilha de Botões Flutuantes ("Button Island") no Canto Inferior Esquerdo
                let island_rect = egui::Rect::from_min_size(
                    canvas_rect.left_bottom() + egui::vec2(16.0, -56.0),
                    egui::vec2(320.0, 40.0),
                );

                ui.put(island_rect, |ui: &mut egui::Ui| {
                    ToolBar::show(ui, &mut self.active_tool);
                    ui.allocate_response(egui::Vec2::ZERO, egui::Sense::hover())
                });

                // Indicador ao lado da button island indicando se o conjunto difere de mission_points.yaml
                if is_different {
                    let indicator_rect = egui::Rect::from_min_size(
                        canvas_rect.left_bottom() + egui::vec2(360.0, -48.0),
                        egui::vec2(190.0, 24.0),
                    );
                    ui.put(indicator_rect, |ui: &mut egui::Ui| {
                        egui::Frame::new()
                            .fill(egui::Color32::from_black_alpha(210))
                            .corner_radius(6.0)
                            .inner_margin(egui::Margin::symmetric(2, 4))
                            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgba_premultiplied(220, 160, 40, 100)))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new("Diferente de mission_points.yaml")
                                        .size(11.0)
                                        .strong()
                                        .color(egui::Color32::from_rgb(255, 195, 75)),
                                );
                            });
                        ui.allocate_response(egui::Vec2::ZERO, egui::Sense::hover())
                    });
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shared_mission_data_across_editor_instances() {
        let _editor1 = MissionEditor::default();
        let _editor2 = MissionEditor::default();

        // Modificar o ponto no editor 1 via shared sets
        {
            let sets_arc = get_shared_mission_sets();
            let mut guard = sets_arc.lock().unwrap();
            let data = guard.active_data_mut().unwrap();
            data.points.push(models::MissionPoint::new(99.0, 88.0));
        }

        // Verificar que o editor 2 enxerga a mesma modificação imediatamente
        {
            let sets_arc = get_shared_mission_sets();
            let guard = sets_arc.lock().unwrap();
            let data = guard.active_data().unwrap();
            let last_pt = data.points.last().expect("Ponto adicionado no editor 1 deve refletir no editor 2");
            assert_eq!(last_pt.x, 99.0);
            assert_eq!(last_pt.y, 88.0);
        }
    }

    #[test]
    fn test_shared_zoom_speed() {
        set_shared_zoom_speed(0.08);
        assert_eq!(get_shared_zoom_speed(), 0.08);
    }
}
