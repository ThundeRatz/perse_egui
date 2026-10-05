use crate::editor::{
    models::{MissionData, MissionPoint, Obstacle, ObstacleKind, ObstacleShape},
    sidebar::Selection,
    toolbar::{ActiveTool, ShapeType},
};
use egui::{
    pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Response, RichText, Sense, Stroke,
    StrokeKind, Ui, Vec2,
};
use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ViewVisibilityOptions {
    pub show_margins: bool,
    pub show_path_segments: bool,
    pub show_positive_axes: bool,
    pub show_negative_axes: bool,
    pub show_axis_numbers: bool,
    #[serde(default = "default_true")]
    pub show_parameter_labels: bool,
    #[serde(default)]
    pub lock_obstacles: bool,
    pub visible_param_keys: std::collections::HashSet<String>,
}

impl Default for ViewVisibilityOptions {
    fn default() -> Self {
        Self {
            show_margins: true,
            show_path_segments: true,
            show_positive_axes: true,
            show_negative_axes: true,
            show_axis_numbers: true,
            show_parameter_labels: true,
            lock_obstacles: false,
            visible_param_keys: std::collections::HashSet::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CanvasState {
    pub pan: Vec2, // Posição no mundo (m) correspondente ao centro da tela
    pub zoom: f32, // Pixels por metro
    pub drag_target: Option<Selection>, // Objeto capturado no início do clique/arraste
    pub poly_building: Vec<[f32; 2]>, // Vértices temporários para criação de polígono
    pub view_options: ViewVisibilityOptions, // Opções de visibilidade dos elementos gráficos
    pub is_display_options_open: bool, // Estado expandido/recolhido da janela Exibição
    pub box_select_start: Option<Vec2>, // Posição (mundo) onde a seleção por caixa começou
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            pan: Vec2::new(3.0, 3.0),
            zoom: 40.0, // 40 pixels = 1 metro por padrão
            drag_target: None,
            poly_building: Vec::new(),
            view_options: ViewVisibilityOptions::default(),
            is_display_options_open: true,
            box_select_start: None,
        }
    }
}

impl CanvasState {
    /// Converte Coordenada do Mundo (m) -> Coordenada da Tela (pixels)
    pub fn world_to_screen(&self, world_pos: Vec2, rect: Rect) -> Pos2 {
        let center = rect.center();
        let delta = world_pos - self.pan;
        // No mundo: +Y é para cima. Na tela egui: +Y é para baixo.
        pos2(
            center.x + delta.x * self.zoom,
            center.y - delta.y * self.zoom,
        )
    }

    /// Converte Coordenada da Tela (pixels) -> Coordenada do Mundo (m)
    pub fn screen_to_world(&self, screen_pos: Pos2, rect: Rect) -> Vec2 {
        let center = rect.center();
        let delta_screen = screen_pos - center;
        Vec2::new(
            self.pan.x + delta_screen.x / self.zoom,
            self.pan.y - delta_screen.y / self.zoom,
        )
    }
}

pub struct EditorCanvas;

impl EditorCanvas {
    pub fn show(
        ui: &mut Ui,
        state: &mut CanvasState,
        data: &mut MissionData,
        selection: &mut Selection,
        active_tool: &mut ActiveTool,
        show_sidebar: &mut bool,
        robot_pose: Option<[f32; 3]>, // [x, y, theta]
        zoom_speed: f32,
    ) -> Response {
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let rect = response.rect;
        let painter = painter.with_clip_rect(rect);

        // 1. Tratar Zoom (Scroll Wheel)
        let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
        if response.hovered() && scroll_delta != 0.0 {
            let step = zoom_speed.clamp(0.005, 0.2);
            let zoom_factor = if scroll_delta > 0.0 {
                1.0 + step
            } else {
                1.0 - step
            };
            if let Some(hover_pos) = response.hover_pos() {
                let world_before = state.screen_to_world(hover_pos, rect);
                state.zoom = (state.zoom * zoom_factor).clamp(5.0, 500.0);
                let world_after = state.screen_to_world(hover_pos, rect);
                state.pan += world_before - world_after;
            } else {
                state.zoom = (state.zoom * zoom_factor).clamp(5.0, 500.0);
            }
        }

        // 2. Navegação Pan via Botão do Meio ou Botão Direito
        let is_middle_drag = response.dragged_by(egui::PointerButton::Middle);
        let is_right_drag = response.dragged_by(egui::PointerButton::Secondary);

        if is_middle_drag || is_right_drag {
            let delta = response.drag_delta();
            state.pan.x -= delta.x / state.zoom;
            state.pan.y += delta.y / state.zoom;
        }

        // 3. Renderizar Grade Infinita, Eixos (Positivos e Negativos) e Números Dinâmicos
        draw_grid(&painter, rect, state);

        // 4. Renderizar Obstáculos
        draw_obstacles(&painter, rect, state, &data.obstacles, selection);

        // 5. Renderizar Trajetória (Origem -> Marco 0 -> Marco 1 ... Marco N) e Pontos
        draw_points_and_path(&painter, rect, state, &data.points, selection);

        // 6. Renderizar Robô (Pose)
        let robot = robot_pose.unwrap_or([0.0, 0.0, 0.0]);
        draw_robot(&painter, rect, state, robot);

        // 7. Renderizar Polígono ou Linha em Construção
        if !state.poly_building.is_empty() {
            draw_poly_building(&painter, rect, state, active_tool, &state.poly_building);
        }

        // 8. Atalhos de Teclado (quando nenhum campo de texto possui foco)
        if !ui.memory(|m| m.focused().is_some()) {
            ui.input(|i| {
                if i.key_pressed(egui::Key::Backspace) || i.key_pressed(egui::Key::Delete) {
                    if !selection.is_empty() {
                        let pts_to_remove: Vec<usize> =
                            selection.points.iter().copied().rev().collect();
                        for p in pts_to_remove {
                            if p < data.points.len() {
                                data.points.remove(p);
                            }
                        }
                        let obs_to_remove: Vec<usize> =
                            selection.obstacles.iter().copied().rev().collect();
                        for o in obs_to_remove {
                            if o < data.obstacles.len() {
                                data.obstacles.remove(o);
                            }
                        }
                        selection.clear();
                    }
                } else if i.key_pressed(egui::Key::A) {
                    *active_tool = ActiveTool::AddPoint;
                    state.poly_building.clear();
                } else if i.key_pressed(egui::Key::Escape) || i.key_pressed(egui::Key::S) {
                    *active_tool = ActiveTool::Select;
                    state.poly_building.clear();
                } else if i.key_pressed(egui::Key::P) {
                    *active_tool =
                        ActiveTool::AddObstacle(ObstacleKind::Physical, ShapeType::Polygon);
                    state.poly_building.clear();
                } else if i.key_pressed(egui::Key::L) {
                    *active_tool = ActiveTool::AddObstacle(ObstacleKind::Physical, ShapeType::Line);
                    state.poly_building.clear();
                } else if i.key_pressed(egui::Key::D) {
                    *active_tool = ActiveTool::AddObstacle(ObstacleKind::Cosmetic, ShapeType::Line);
                    state.poly_building.clear();
                } else if i.key_pressed(egui::Key::R) {
                    *active_tool =
                        ActiveTool::AddObstacle(ObstacleKind::Physical, ShapeType::Rectangle);
                    state.poly_building.clear();
                } else if i.key_pressed(egui::Key::C) {
                    *active_tool =
                        ActiveTool::AddObstacle(ObstacleKind::Physical, ShapeType::Circle);
                    state.poly_building.clear();
                }
            });
        }

        // 9. Lógica de Seleção e Arraste Direto
        let mouse_pos = response.hover_pos().unwrap_or(rect.center());
        let mouse_world = state.screen_to_world(mouse_pos, rect);

        // Regiões de UI flutuantes no canvas que não devem disparar ações de clique no mundo
        let island_rect =
            Rect::from_min_size(rect.left_bottom() + vec2(16.0, -56.0), vec2(320.0, 40.0));
        let indicator_rect =
            Rect::from_min_size(rect.left_bottom() + vec2(360.0, -48.0), vec2(190.0, 24.0));
        let display_options_rect =
            Rect::from_min_size(rect.right_top() + vec2(-210.0, 5.0), vec2(205.0, 200.0));

        let is_over_ui_overlay = island_rect.contains(mouse_pos)
            || indicator_rect.contains(mouse_pos)
            || display_options_rect.contains(mouse_pos);

        // Finalização da construção de Polígono ou Linha via Enter, Espaço ou Botão Direito do Mouse
        let key_complete = !ui.memory(|m| m.focused().is_some())
            && ui.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Space));
        let right_click_complete = response.hovered()
            && !is_over_ui_overlay
            && ui.input(|i| i.pointer.secondary_pressed());

        if !state.poly_building.is_empty() && (key_complete || right_click_complete) {
            match active_tool {
                ActiveTool::AddObstacle(kind, ShapeType::Polygon) => {
                    if state.poly_building.len() >= 3 {
                        let id = format!("obs_{}", data.obstacles.len() + 1);
                        let obs = Obstacle::new_polygon(id, *kind, state.poly_building.clone());
                        data.obstacles.push(obs);
                        selection.select_single_obstacle(data.obstacles.len() - 1);
                    }
                    state.poly_building.clear();
                    *active_tool = ActiveTool::Select;
                }
                ActiveTool::AddObstacle(kind, ShapeType::Line) => {
                    if state.poly_building.len() >= 2 {
                        let id = format!("obs_{}", data.obstacles.len() + 1);
                        let obs = Obstacle::new_line(id, *kind, state.poly_building.clone());
                        data.obstacles.push(obs);
                        selection.select_single_obstacle(data.obstacles.len() - 1);
                    }
                    state.poly_building.clear();
                    *active_tool = ActiveTool::Select;
                }
                _ => {}
            }
        }

        let primary_pressed = ui.input(|i| i.pointer.primary_pressed());
        let primary_released = ui.input(|i| i.pointer.primary_released());

        if primary_pressed
            && response.hovered()
            && !is_over_ui_overlay
            && !(is_middle_drag || is_right_drag)
        {
            let shift_pressed = ui.input(|i| i.modifiers.shift);
            match active_tool {
                ActiveTool::Select => {
                    let mut hit_vert = None;
                    let mut hit_point = None;
                    let mut hit_obstacle = None;

                    // 1. Hit test nos vértices de qualquer obstáculo polígono/linha
                    for (i, obs) in data.obstacles.iter().enumerate() {
                        match &obs.shape {
                            ObstacleShape::Polygon { vertices }
                            | ObstacleShape::Line { vertices } => {
                                for (v_idx, v) in vertices.iter().enumerate() {
                                    let v_screen = state.world_to_screen(vec2(v[0], v[1]), rect);
                                    if mouse_pos.distance(v_screen) <= 12.0 {
                                        hit_vert = Some((i, v_idx));
                                        break;
                                    }
                                }
                            }
                            _ => {}
                        }
                        if hit_vert.is_some() {
                            break;
                        }
                    }

                    // 2. Hit test para pontos de missão
                    if hit_vert.is_none() {
                        for (i, pt) in data.points.iter().enumerate() {
                            let pt_screen = state.world_to_screen(vec2(pt.x, pt.y), rect);
                            let is_already_selected = selection.contains_point(i);

                            let hit_radius =
                                if is_already_selected && state.view_options.show_margins {
                                    14.0_f32.max(pt.get_margin() * state.zoom)
                                } else {
                                    14.0
                                };

                            if mouse_pos.distance(pt_screen) <= hit_radius {
                                hit_point = Some(i);
                                break;
                            }
                        }
                    }

                    // 3. Hit test para corpo dos obstáculos (Polígono, Linha, Retângulo, Círculo)
                    if hit_vert.is_none() && hit_point.is_none() {
                        for (i, obs) in data.obstacles.iter().enumerate() {
                            if hit_test_obstacle(obs, mouse_world) {
                                hit_obstacle = Some(i);
                                break;
                            }
                        }
                    }

                    if let Some((obs_idx, v_idx)) = hit_vert {
                        selection.select_single_vertex(obs_idx, v_idx);
                        state.drag_target = Some(selection.clone());
                        state.box_select_start = None;
                    } else if let Some(p_idx) = hit_point {
                        if shift_pressed {
                            if selection.contains_point(p_idx) {
                                selection.points.remove(&p_idx);
                            } else {
                                selection.points.insert(p_idx);
                            }
                        } else {
                            if !selection.contains_point(p_idx) {
                                selection.select_single_point(p_idx);
                            }
                        }
                        state.drag_target = Some(selection.clone());
                        state.box_select_start = None;
                    } else if let Some(o_idx) = hit_obstacle {
                        if shift_pressed {
                            if selection.contains_obstacle(o_idx) {
                                selection.obstacles.remove(&o_idx);
                            } else {
                                selection.obstacles.insert(o_idx);
                            }
                        } else {
                            if !selection.contains_obstacle(o_idx) {
                                selection.select_single_obstacle(o_idx);
                            }
                        }
                        state.drag_target = Some(selection.clone());
                        state.box_select_start = None;
                    } else {
                        // Clique em área vazia do canvas
                        if !shift_pressed {
                            selection.clear();
                        }
                        state.drag_target = None;
                        state.box_select_start = Some(mouse_world);
                    }
                }
                ActiveTool::AddPoint => {
                    let new_pt = MissionPoint::new(mouse_world.x, mouse_world.y);
                    let insert_idx =
                        if selection.points.len() == 1 && selection.obstacles.is_empty() {
                            let sel_idx = *selection.points.iter().next().unwrap();
                            if sel_idx < data.points.len() {
                                sel_idx + 1
                            } else {
                                data.points.len()
                            }
                        } else {
                            data.points.len()
                        };

                    if insert_idx < data.points.len() {
                        data.points.insert(insert_idx, new_pt);
                    } else {
                        data.points.push(new_pt);
                    }

                    selection.select_single_point(insert_idx);
                    state.drag_target = Some(selection.clone());
                }
                ActiveTool::AddObstacle(kind, shape) => match shape {
                    ShapeType::Polygon => {
                        let mut closed = false;
                        if state.poly_building.len() >= 3 {
                            let first_screen = state.world_to_screen(
                                vec2(state.poly_building[0][0], state.poly_building[0][1]),
                                rect,
                            );
                            if mouse_pos.distance(first_screen) <= 15.0 {
                                let id = format!("obs_{}", data.obstacles.len() + 1);
                                let obs =
                                    Obstacle::new_polygon(id, *kind, state.poly_building.clone());
                                data.obstacles.push(obs);
                                state.poly_building.clear();
                                selection.select_single_obstacle(data.obstacles.len() - 1);
                                *active_tool = ActiveTool::Select;
                                closed = true;
                            }
                        }
                        if !closed {
                            state.poly_building.push([mouse_world.x, mouse_world.y]);
                        }
                    }
                    ShapeType::Line => {
                        state.poly_building.push([mouse_world.x, mouse_world.y]);
                    }
                    ShapeType::Rectangle => {
                        let id = format!("obs_{}", data.obstacles.len() + 1);
                        let obs = Obstacle::new_rectangle(
                            id,
                            *kind,
                            mouse_world.x,
                            mouse_world.y,
                            2.0,
                            1.5,
                        );
                        data.obstacles.push(obs);
                        selection.select_single_obstacle(data.obstacles.len() - 1);
                        *active_tool = ActiveTool::Select;
                    }
                    ShapeType::Circle => {
                        let id = format!("obs_{}", data.obstacles.len() + 1);
                        let obs =
                            Obstacle::new_circle(id, *kind, [mouse_world.x, mouse_world.y], 1.0);
                        data.obstacles.push(obs);
                        selection.select_single_obstacle(data.obstacles.len() - 1);
                        *active_tool = ActiveTool::Select;
                    }
                },
            }
        }

        // Arrastar elemento capturado
        if response.dragged_by(egui::PointerButton::Primary)
            && matches!(active_tool, ActiveTool::Select)
        {
            let delta_world = vec2(
                response.drag_delta().x / state.zoom,
                -response.drag_delta().y / state.zoom,
            );

            if let Some(ref target) = state.drag_target {
                if let Some((obs_idx, v_idx)) = target.obstacle_vertex {
                    if !state.view_options.lock_obstacles {
                        if let Some(obs) = data.obstacles.get_mut(obs_idx) {
                            match &mut obs.shape {
                                ObstacleShape::Polygon { vertices }
                                | ObstacleShape::Line { vertices } => {
                                    if let Some(v) = vertices.get_mut(v_idx) {
                                        v[0] += delta_world.x;
                                        v[1] += delta_world.y;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                } else {
                    for &p_idx in &target.points {
                        if let Some(pt) = data.points.get_mut(p_idx) {
                            pt.x += delta_world.x;
                            pt.y += delta_world.y;
                        }
                    }
                    if !state.view_options.lock_obstacles {
                        for &o_idx in &target.obstacles {
                            if let Some(obs) = data.obstacles.get_mut(o_idx) {
                                match &mut obs.shape {
                                    ObstacleShape::Polygon { vertices }
                                    | ObstacleShape::Line { vertices } => {
                                        for v in vertices.iter_mut() {
                                            v[0] += delta_world.x;
                                            v[1] += delta_world.y;
                                        }
                                    }
                                    ObstacleShape::Rectangle { x, y, .. } => {
                                        *x += delta_world.x;
                                        *y += delta_world.y;
                                    }
                                    ObstacleShape::Circle { center, .. } => {
                                        center[0] += delta_world.x;
                                        center[1] += delta_world.y;
                                    }
                                }
                            }
                        }
                    }
                }
            } else if state.box_select_start.is_none() {
                state.pan.x -= response.drag_delta().x / state.zoom;
                state.pan.y += response.drag_delta().y / state.zoom;
            }
        }

        // Renderizar retângulo de seleção (Marquee Box) enquanto arrasta na ferramenta de seleção
        if matches!(active_tool, ActiveTool::Select) {
            if let Some(start_world) = state.box_select_start {
                let start_screen = state.world_to_screen(start_world, rect);
                let box_rect = Rect::from_two_pos(start_screen, mouse_pos);

                let fill = Color32::from_rgba_unmultiplied(0, 150, 255, 35);
                let stroke = Stroke::new(1.2, Color32::from_rgb(0, 150, 255));
                painter.rect(box_rect, 0.0, fill, stroke, StrokeKind::Outside);
            }
        }

        if primary_released {
            if let Some(start_world) = state.box_select_start.take() {
                let min_x = start_world.x.min(mouse_world.x);
                let max_x = start_world.x.max(mouse_world.x);
                let min_y = start_world.y.min(mouse_world.y);
                let max_y = start_world.y.max(mouse_world.y);

                if (max_x - min_x) > 0.05 || (max_y - min_y) > 0.05 {
                    let shift_pressed = ui.input(|i| i.modifiers.shift);
                    if !shift_pressed {
                        selection.clear();
                    }

                    for (i, pt) in data.points.iter().enumerate() {
                        if pt.x >= min_x && pt.x <= max_x && pt.y >= min_y && pt.y <= max_y {
                            selection.points.insert(i);
                        }
                    }

                    for (i, obs) in data.obstacles.iter().enumerate() {
                        if obstacle_intersects_box(obs, min_x, max_x, min_y, max_y) {
                            selection.obstacles.insert(i);
                        }
                    }
                }
            }
            state.drag_target = None;
        }

        // Overlay: Janela Ancorada de Opções de Exibição no Canto Superior Direito
        egui::Window::new("Exibição")
            .id(ui.make_persistent_id("canvas_display_options_window"))
            .anchor(egui::Align2::RIGHT_TOP, vec2(-10.0, 10.0))
            .title_bar(false)
            .resizable(false)
            .movable(false)
            .constrain_to(rect)
            .show(ui.ctx(), |ui| {
                // Animação suave de abertura/fechamento (openness de 0.0 a 1.0)
                let openness = ui.ctx().animate_bool(
                    ui.make_persistent_id("canvas_display_options_openness"),
                    state.is_display_options_open,
                );

                // Forçar largura dinâmica animada (120px colapsado -> 195px expandido)
                let target_width = egui::lerp(120.0..=195.0, openness);
                ui.set_min_width(target_width);
                ui.set_max_width(target_width);

                // Barra de Título Customizada
                ui.horizontal(|ui| {
                    ui.set_height(20.0);

                    // 1. Esquerda: Ícone de recolher/expandir janela de Exibição
                    let collapse_label = if state.is_display_options_open {
                        "⏷"
                    } else {
                        "⏵"
                    };
                    if ui
                        .small_button(collapse_label)
                        .on_hover_text("Recolher / Expandir Exibição")
                        .clicked()
                    {
                        state.is_display_options_open = !state.is_display_options_open;
                    }

                    ui.add_space(2.0);

                    // 2. Título "Exibição"
                    ui.label(
                        RichText::new("Exibição")
                            .strong()
                            .size(12.0)
                            .color(Color32::from_gray(230)),
                    );

                    // 3. Extrema Direita: Botão com ícone RIGHT_PANEL_TOGGLE com cor de acordo com o estado da sidebar
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let tint = if *show_sidebar {
                            Color32::WHITE
                        } else {
                            Color32::from_gray(110)
                        };

                        let icon_img = re_ui::icons::RIGHT_PANEL_TOGGLE
                            .as_image()
                            .fit_to_exact_size(vec2(13.0, 13.0))
                            .tint(tint);

                        let btn = ui
                            .add(egui::Button::image(icon_img).fill(Color32::TRANSPARENT))
                            .on_hover_text("Alternar Painel de Propriedades");

                        if btn.clicked() {
                            *show_sidebar = !*show_sidebar;
                        }
                    });
                });

                // Conteúdo das opções de exibição com animação suave de fade/openness
                if openness > 0.0 {
                    ui.scope_builder(egui::UiBuilder::new(), |ui| {
                        if openness < 1.0 {
                            ui.set_opacity(openness);
                        }

                        ui.checkbox(
                            &mut state.view_options.show_margins,
                            "Visualização das margens",
                        );
                        ui.checkbox(
                            &mut state.view_options.show_path_segments,
                            "Segmentos de retas",
                        );
                        ui.checkbox(
                            &mut state.view_options.show_positive_axes,
                            "Eixos positivos (X, Y)",
                        );
                        ui.checkbox(
                            &mut state.view_options.show_negative_axes,
                            "Eixos negativos (-X, -Y)",
                        );
                        ui.checkbox(
                            &mut state.view_options.show_axis_numbers,
                            "Números nos eixos",
                        );
                        ui.checkbox(
                            &mut state.view_options.show_parameter_labels,
                            "Valores dos parâmetros",
                        );
                        ui.checkbox(&mut state.view_options.lock_obstacles, "Travar obstáculos");
                    });
                }
            });

        // Indicador de coordenadas
        let coord_text = format!("X: {:.2} m, Y: {:.2} m", mouse_world.x, mouse_world.y);
        painter.text(
            rect.left_top() + vec2(10.0, 10.0),
            Align2::LEFT_TOP,
            coord_text,
            FontId::monospace(13.0),
            Color32::from_gray(180),
        );

        response
    }
}

fn draw_grid(painter: &Painter, rect: Rect, state: &CanvasState) {
    let bg_color = Color32::from_rgb(17, 19, 23); // Paleta Dark Rerun
    painter.rect_filled(rect, 0.0, bg_color);

    let grid_step_minor = 1.0;
    let grid_step_major = 5.0;

    let min_world = state.screen_to_world(rect.left_top(), rect);
    let max_world = state.screen_to_world(rect.right_bottom(), rect);

    let min_x = min_world.x.min(max_world.x);
    let max_x = min_world.x.max(max_world.x);
    let min_y = min_world.y.min(max_world.y);
    let max_y = min_world.y.max(max_world.y);

    let start_x = (min_x / grid_step_minor).floor() * grid_step_minor;
    let end_x = (max_x / grid_step_minor).ceil() * grid_step_minor;

    let start_y = (min_y / grid_step_minor).floor() * grid_step_minor;
    let end_y = (max_y / grid_step_minor).ceil() * grid_step_minor;

    let minor_stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 10));
    let major_stroke = Stroke::new(1.2, Color32::from_rgba_unmultiplied(255, 255, 255, 20));

    let mut x = start_x;
    while x <= end_x {
        let is_major = (x / grid_step_major).round() * grid_step_major == x;
        let stroke = if is_major { major_stroke } else { minor_stroke };
        let p1 = state.world_to_screen(vec2(x, min_y), rect);
        let p2 = state.world_to_screen(vec2(x, max_y), rect);
        painter.line_segment([p1, p2], stroke);
        x += grid_step_minor;
    }

    let mut y = start_y;
    while y <= end_y {
        let is_major = (y / grid_step_major).round() * grid_step_major == y;
        let stroke = if is_major { major_stroke } else { minor_stroke };
        let p1 = state.world_to_screen(vec2(min_x, y), rect);
        let p2 = state.world_to_screen(vec2(max_x, y), rect);
        painter.line_segment([p1, p2], stroke);
        y += grid_step_minor;
    }

    // Origem Cartesiana (0,0)
    let origin_screen = state.world_to_screen(vec2(0.0, 0.0), rect);

    // Eixos Positivos (X, Y)
    if state.view_options.show_positive_axes {
        // Eixo +X (Azul Rerun)
        let x_pos_end = pos2(rect.right(), origin_screen.y);
        if origin_screen.y >= rect.top() && origin_screen.y <= rect.bottom() {
            painter.line_segment(
                [origin_screen, x_pos_end],
                Stroke::new(2.0, Color32::from_rgb(60, 140, 240)),
            );
            painter.text(
                x_pos_end - vec2(15.0, 15.0),
                Align2::RIGHT_BOTTOM,
                "X",
                FontId::proportional(14.0),
                Color32::from_rgb(60, 140, 240),
            );
        }

        // Eixo +Y (Verde Rerun) até o topo da tela
        let y_pos_end = pos2(origin_screen.x, rect.top());
        if origin_screen.x >= rect.left() && origin_screen.x <= rect.right() {
            painter.line_segment(
                [origin_screen, y_pos_end],
                Stroke::new(2.0, Color32::from_rgb(60, 220, 100)),
            );
            painter.text(
                y_pos_end + vec2(10.0, 5.0),
                Align2::LEFT_TOP,
                "Y",
                FontId::proportional(14.0),
                Color32::from_rgb(60, 220, 100),
            );
        }
    }

    // Eixos Negativos (-X, -Y) com cores mais escuras
    if state.view_options.show_negative_axes {
        // Eixo -X (Azul Escuro)
        let x_neg_end = pos2(rect.left(), origin_screen.y);
        if origin_screen.y >= rect.top() && origin_screen.y <= rect.bottom() {
            painter.line_segment(
                [origin_screen, x_neg_end],
                Stroke::new(1.8, Color32::from_rgb(30, 75, 140)),
            );
            painter.text(
                x_neg_end + vec2(15.0, -15.0),
                Align2::LEFT_BOTTOM,
                "-X",
                FontId::proportional(12.0),
                Color32::from_rgb(30, 75, 140),
            );
        }

        // Eixo -Y (Verde Escuro) até a base da tela
        let y_neg_end = pos2(origin_screen.x, rect.bottom());
        if origin_screen.x >= rect.left() && origin_screen.x <= rect.right() {
            painter.line_segment(
                [origin_screen, y_neg_end],
                Stroke::new(1.8, Color32::from_rgb(30, 120, 60)),
            );
            painter.text(
                y_neg_end + vec2(10.0, -15.0),
                Align2::LEFT_BOTTOM,
                "-Y",
                FontId::proportional(12.0),
                Color32::from_rgb(30, 120, 60),
            );
        }
    }

    // Números Dinâmicos nos Eixos
    if state.view_options.show_axis_numbers {
        let num_step = if state.zoom >= 80.0 {
            1.0
        } else if state.zoom >= 30.0 {
            2.0
        } else if state.zoom >= 12.0 {
            5.0
        } else {
            10.0
        };

        // Números no Eixo X
        let mut x_val = (min_x / num_step).floor() * num_step;
        while x_val <= max_x {
            if x_val.abs() > 0.001 {
                let is_pos = x_val > 0.0;
                if (is_pos && state.view_options.show_positive_axes)
                    || (!is_pos && state.view_options.show_negative_axes)
                {
                    let p = state.world_to_screen(vec2(x_val, 0.0), rect);
                    if p.x >= rect.left() && p.x <= rect.right() {
                        let tick_color = if is_pos {
                            Color32::from_rgb(60, 140, 240)
                        } else {
                            Color32::from_rgb(30, 75, 140)
                        };
                        painter.line_segment(
                            [p - vec2(0.0, 4.0), p + vec2(0.0, 4.0)],
                            Stroke::new(1.2, tick_color),
                        );
                        let label = format!("{:.0}", x_val);
                        painter.text(
                            p + vec2(0.0, 8.0),
                            Align2::CENTER_TOP,
                            label,
                            FontId::proportional(11.0),
                            Color32::from_gray(160),
                        );
                    }
                }
            }
            x_val += num_step;
        }

        // Números no Eixo Y
        let mut y_val = (min_y / num_step).floor() * num_step;
        while y_val <= max_y {
            if y_val.abs() > 0.001 {
                let is_pos = y_val > 0.0;
                if (is_pos && state.view_options.show_positive_axes)
                    || (!is_pos && state.view_options.show_negative_axes)
                {
                    let p = state.world_to_screen(vec2(0.0, y_val), rect);
                    if p.y >= rect.top() && p.y <= rect.bottom() {
                        let tick_color = if is_pos {
                            Color32::from_rgb(60, 220, 100)
                        } else {
                            Color32::from_rgb(30, 120, 60)
                        };
                        painter.line_segment(
                            [p - vec2(4.0, 0.0), p + vec2(4.0, 0.0)],
                            Stroke::new(1.2, tick_color),
                        );
                        let label = format!("{:.0}", y_val);
                        painter.text(
                            p - vec2(8.0, 0.0),
                            Align2::RIGHT_CENTER,
                            label,
                            FontId::proportional(11.0),
                            Color32::from_gray(160),
                        );
                    }
                }
            }
            y_val += num_step;
        }
    }
}

fn draw_points_and_path(
    painter: &Painter,
    rect: Rect,
    state: &CanvasState,
    points: &[MissionPoint],
    selection: &Selection,
) {
    let origin_screen = state.world_to_screen(vec2(0.0, 0.0), rect);
    let mut waypoints = vec![origin_screen];
    for pt in points {
        waypoints.push(state.world_to_screen(vec2(pt.x, pt.y), rect));
    }

    // Renderizar segmentos de retas pontilhados com setas direcionais se habilitado
    if state.view_options.show_path_segments {
        let path_color = Color32::from_rgba_unmultiplied(100, 180, 255, 140);
        let path_stroke = Stroke::new(1.8, path_color);

        for i in 0..waypoints.len() - 1 {
            let p1 = waypoints[i];
            let p2 = waypoints[i + 1];
            let dist = p1.distance(p2);

            if dist > 4.0 {
                let dash_len = 8.0;
                let gap_len = 5.0;
                let step = dash_len + gap_len;
                let dir = (p2 - p1) / dist;

                let mut current_d = 0.0;
                while current_d < dist {
                    let end_d = (current_d + dash_len).min(dist);
                    let seg_p1 = p1 + dir * current_d;
                    let seg_p2 = p1 + dir * end_d;
                    painter.line_segment([seg_p1, seg_p2], path_stroke);
                    current_d += step;
                }

                let ortho = vec2(-dir.y, dir.x);
                let num_arrows = if dist > 120.0 {
                    3
                } else if dist > 50.0 {
                    2
                } else {
                    1
                };
                for k in 1..=num_arrows {
                    let frac = (k as f32) / ((num_arrows + 1) as f32);
                    let arrow_pos = p1 + dir * (dist * frac);

                    let wing1 = arrow_pos - dir * 9.0 + ortho * 5.0;
                    let wing2 = arrow_pos - dir * 9.0 - ortho * 5.0;

                    painter.line_segment([arrow_pos, wing1], path_stroke);
                    painter.line_segment([arrow_pos, wing2], path_stroke);
                }
            }
        }
    }

    // Renderizar cada ponto de missão
    for (i, pt) in points.iter().enumerate() {
        let pos = state.world_to_screen(vec2(pt.x, pt.y), rect);
        let is_selected = selection.contains_point(i);

        // Círculo de margem de tolerância se habilitado
        if state.view_options.show_margins {
            let margin = pt.get_margin();
            let margin_px = margin * state.zoom;

            let margin_color = if is_selected {
                Color32::from_rgba_unmultiplied(255, 200, 50, 45)
            } else {
                Color32::from_rgba_unmultiplied(60, 140, 240, 30)
            };
            let margin_stroke = if is_selected {
                Stroke::new(1.8, Color32::from_rgb(255, 200, 50))
            } else {
                Stroke::new(1.2, Color32::from_rgba_unmultiplied(60, 140, 240, 180))
            };

            painter.circle(pos, margin_px, margin_color, margin_stroke);
        }

        // Ponto central
        let pt_radius = if is_selected { 8.0 } else { 6.0 };
        let pt_color = if is_selected {
            Color32::from_rgb(255, 200, 50)
        } else {
            Color32::WHITE
        };
        painter.circle_filled(pos, pt_radius, pt_color);
        painter.circle_stroke(
            pos,
            pt_radius + 2.0,
            Stroke::new(1.5, Color32::from_rgb(17, 19, 23)),
        );

        let mut label_y_offset = pt_radius + 12.0;

        // Rótulo "Marco i"
        let label = format!("Marco {}", i);
        painter.text(
            pos + vec2(0.0, label_y_offset),
            Align2::CENTER_TOP,
            label,
            FontId::proportional(12.0),
            Color32::WHITE,
        );
        label_y_offset += 14.0;

        // Renderizar parâmetros com visibilidade habilitada abaixo do nome do marco
        if state.view_options.show_parameter_labels {
            for key in &state.view_options.visible_param_keys {
                if let Some(val) = pt.extra.get(key) {
                    let val_str = match val {
                        serde_yaml::Value::Number(n) => {
                            if let Some(f) = n.as_f64() {
                                format!("{:.2}", f)
                            } else {
                                n.to_string()
                            }
                        }
                        serde_yaml::Value::Bool(b) => b.to_string(),
                        serde_yaml::Value::String(s) => s.clone(),
                        _ => format!("{:?}", val),
                    };

                    let text_color = match val {
                        serde_yaml::Value::Bool(true) => Color32::from_rgb(100, 235, 120),
                        serde_yaml::Value::Bool(false) => Color32::from_rgb(240, 110, 110),
                        _ => Color32::from_rgb(180, 220, 255),
                    };

                    let param_text = format!("{}: {}", key, val_str);
                    painter.text(
                        pos + vec2(0.0, label_y_offset),
                        Align2::CENTER_TOP,
                        param_text,
                        FontId::proportional(11.0),
                        text_color,
                    );
                    label_y_offset += 13.0;
                }
            }
        }
    }
}

fn draw_obstacles(
    painter: &Painter,
    rect: Rect,
    state: &CanvasState,
    obstacles: &[Obstacle],
    selection: &Selection,
) {
    for (i, obs) in obstacles.iter().enumerate() {
        let is_selected = selection.contains_obstacle(i);

        let (fill_color, stroke_color) = match obs.kind {
            ObstacleKind::Physical => (
                Color32::from_rgba_unmultiplied(230, 90, 40, 60),
                if is_selected {
                    if state.view_options.lock_obstacles {
                        Color32::from_rgb(240, 50, 50)
                    } else {
                        Color32::WHITE
                    }
                } else {
                    Color32::from_rgb(230, 90, 40)
                },
            ),
            ObstacleKind::Cosmetic => (
                Color32::from_rgba_unmultiplied(60, 180, 240, 50),
                if is_selected {
                    if state.view_options.lock_obstacles {
                        Color32::from_rgb(240, 50, 50)
                    } else {
                        Color32::WHITE
                    }
                } else {
                    Color32::from_rgb(60, 180, 240)
                },
            ),
        };

        let stroke = Stroke::new(if is_selected { 2.5 } else { 1.5 }, stroke_color);

        match &obs.shape {
            ObstacleShape::Polygon { vertices } => {
                if vertices.len() >= 3 {
                    let pts: Vec<Pos2> = vertices
                        .iter()
                        .map(|v| state.world_to_screen(vec2(v[0], v[1]), rect))
                        .collect();
                    painter.add(egui::Shape::convex_polygon(pts, fill_color, stroke));
                }
            }
            ObstacleShape::Line { vertices } => {
                if vertices.len() >= 2 {
                    let pts: Vec<Pos2> = vertices
                        .iter()
                        .map(|v| state.world_to_screen(vec2(v[0], v[1]), rect))
                        .collect();
                    let stroke_line =
                        Stroke::new(if is_selected { 3.5 } else { 2.2 }, stroke_color);
                    for i in 0..pts.len() - 1 {
                        painter.line_segment([pts[i], pts[i + 1]], stroke_line);
                    }
                }
            }
            ObstacleShape::Rectangle {
                x,
                y,
                width,
                height,
                ..
            } => {
                let min_p = state.world_to_screen(vec2(x - width / 2.0, y + height / 2.0), rect);
                let max_p = state.world_to_screen(vec2(x + width / 2.0, y - height / 2.0), rect);
                let r = Rect::from_two_pos(min_p, max_p);
                painter.rect(r, 4.0, fill_color, stroke, StrokeKind::Outside);
            }
            ObstacleShape::Circle { center, radius } => {
                let c_screen = state.world_to_screen(vec2(center[0], center[1]), rect);
                let r_px = radius * state.zoom;
                painter.circle(c_screen, r_px, fill_color, stroke);
            }
        }

        // Renderizar handles nos vértices do obstáculo selecionado (Polígono ou Linhas)
        if is_selected {
            match &obs.shape {
                ObstacleShape::Polygon { vertices } | ObstacleShape::Line { vertices } => {
                    for (v_idx, v) in vertices.iter().enumerate() {
                        let v_screen = state.world_to_screen(vec2(v[0], v[1]), rect);
                        let is_vert_selected = selection.is_vertex_selected(i, v_idx);
                        let radius = if is_vert_selected { 6.5 } else { 4.5 };
                        let color = if is_vert_selected {
                            if state.view_options.lock_obstacles {
                                Color32::from_rgb(255, 90, 90)
                            } else {
                                Color32::from_rgb(255, 210, 50)
                            }
                        } else if state.view_options.lock_obstacles {
                            Color32::from_rgb(240, 80, 80)
                        } else {
                            Color32::WHITE
                        };
                        painter.circle_filled(v_screen, radius, color);
                        painter.circle_stroke(
                            v_screen,
                            radius + 1.5,
                            Stroke::new(1.2, Color32::from_rgb(17, 19, 23)),
                        );
                    }
                }
                _ => {}
            }
        }
    }
}

fn draw_robot(painter: &Painter, rect: Rect, state: &CanvasState, pose: [f32; 3]) {
    let pos_world = vec2(pose[0], pose[1]);
    let theta = pose[2];
    let pos_screen = state.world_to_screen(pos_world, rect);

    let size_px = 16.0;

    let dir = vec2(theta.cos(), -theta.sin());
    let ortho = vec2(-dir.y, dir.x);

    let tip = pos_screen + dir * size_px;
    let left = pos_screen - dir * (size_px * 0.5) + ortho * (size_px * 0.5);
    let right = pos_screen - dir * (size_px * 0.5) - ortho * (size_px * 0.5);

    let red_robot = Color32::from_rgb(240, 45, 45); // Vermelho Rerun
    painter.add(egui::Shape::convex_polygon(
        vec![tip, left, pos_screen, right],
        red_robot,
        Stroke::new(1.0, Color32::WHITE),
    ));
}

fn draw_poly_building(
    painter: &Painter,
    rect: Rect,
    state: &CanvasState,
    active_tool: &ActiveTool,
    vertices: &[[f32; 2]],
) {
    let pts: Vec<Pos2> = vertices
        .iter()
        .map(|v| state.world_to_screen(vec2(v[0], v[1]), rect))
        .collect();

    for p in &pts {
        painter.circle_filled(*p, 4.0, Color32::from_rgb(255, 200, 50));
    }

    if pts.len() > 1 {
        for i in 0..pts.len() - 1 {
            painter.line_segment(
                [pts[i], pts[i + 1]],
                Stroke::new(1.8, Color32::from_rgb(255, 200, 50)),
            );
        }
    }

    // Se estiver no modo Polígono e tiver 3+ pontos, destacar o primeiro ponto com um anel de fechamento
    if matches!(active_tool, ActiveTool::AddObstacle(_, ShapeType::Polygon)) && pts.len() >= 3 {
        let p0 = pts[0];
        painter.circle_stroke(p0, 10.0, Stroke::new(2.0, Color32::from_rgb(255, 220, 80)));
    }
}

fn hit_test_obstacle(obs: &Obstacle, mouse_world: Vec2) -> bool {
    match &obs.shape {
        ObstacleShape::Polygon { vertices } => point_in_polygon(mouse_world, vertices),
        ObstacleShape::Line { vertices } => point_near_polyline(mouse_world, vertices, 0.4),
        ObstacleShape::Rectangle {
            x,
            y,
            width,
            height,
            ..
        } => {
            let half_w = width / 2.0;
            let half_h = height / 2.0;
            mouse_world.x >= x - half_w
                && mouse_world.x <= x + half_w
                && mouse_world.y >= y - half_h
                && mouse_world.y <= y + half_h
        }
        ObstacleShape::Circle { center, radius } => {
            let dx = mouse_world.x - center[0];
            let dy = mouse_world.y - center[1];
            (dx * dx + dy * dy) <= (radius * radius)
        }
    }
}

fn point_near_polyline(pt: Vec2, vertices: &[[f32; 2]], max_dist: f32) -> bool {
    if vertices.len() < 2 {
        return false;
    }
    let max_dist_sq = max_dist * max_dist;
    for i in 0..vertices.len() - 1 {
        let p1 = vec2(vertices[i][0], vertices[i][1]);
        let p2 = vec2(vertices[i + 1][0], vertices[i + 1][1]);
        if dist_sq_point_to_segment(pt, p1, p2) <= max_dist_sq {
            return true;
        }
    }
    false
}

fn dist_sq_point_to_segment(p: Vec2, v: Vec2, w: Vec2) -> f32 {
    let l2 = (v - w).length_sq();
    if l2 == 0.0 {
        return (p - v).length_sq();
    }
    let t = ((p.x - v.x) * (w.x - v.x) + (p.y - v.y) * (w.y - v.y)) / l2;
    let t = t.clamp(0.0, 1.0);
    let projection = v + (w - v) * t;
    (p - projection).length_sq()
}

fn point_in_polygon(pt: Vec2, vertices: &[[f32; 2]]) -> bool {
    let mut inside = false;
    let n = vertices.len();
    let mut j = n - 1;
    for i in 0..n {
        let vi = vertices[i];
        let vj = vertices[j];
        if ((vi[1] > pt.y) != (vj[1] > pt.y))
            && (pt.x < (vj[0] - vi[0]) * (pt.y - vi[1]) / (vj[1] - vi[1]) + vi[0])
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn obstacle_intersects_box(obs: &Obstacle, min_x: f32, max_x: f32, min_y: f32, max_y: f32) -> bool {
    match &obs.shape {
        ObstacleShape::Polygon { vertices } | ObstacleShape::Line { vertices } => vertices
            .iter()
            .any(|v| v[0] >= min_x && v[0] <= max_x && v[1] >= min_y && v[1] <= max_y),
        ObstacleShape::Rectangle {
            x,
            y,
            width,
            height,
            ..
        } => {
            let r_min_x = x - width / 2.0;
            let r_max_x = x + width / 2.0;
            let r_min_y = y - height / 2.0;
            let r_max_y = y + height / 2.0;

            r_min_x <= max_x && r_max_x >= min_x && r_min_y <= max_y && r_max_y >= min_y
        }
        ObstacleShape::Circle { center, radius } => {
            let cx = center[0];
            let cy = center[1];
            cx + radius >= min_x
                && cx - radius <= max_x
                && cy + radius >= min_y
                && cy - radius <= max_y
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canvas_state_default() {
        let state = CanvasState::default();
        assert_eq!(state.zoom, 40.0);
        assert!(state.view_options.show_margins);
        assert!(state.view_options.visible_param_keys.is_empty());
        assert!(state.box_select_start.is_none());
    }

    #[test]
    fn test_selection_multi_operations() {
        let mut selection = Selection::default();
        assert!(selection.is_empty());

        selection.points.insert(1);
        selection.points.insert(3);
        selection.obstacles.insert(0);

        assert_eq!(selection.count(), 3);
        assert!(selection.contains_point(1));
        assert!(selection.contains_point(3));
        assert!(selection.contains_obstacle(0));
        assert!(!selection.contains_point(2));

        selection.remove_point_and_adjust(1);
        assert_eq!(selection.points.len(), 1);
        assert!(selection.contains_point(2)); // former point 3 became 2

        selection.clear();
        assert!(selection.is_empty());
    }

    #[test]
    fn test_obstacle_intersects_box() {
        let rect_obs =
            Obstacle::new_rectangle("r1".to_string(), ObstacleKind::Physical, 0.0, 0.0, 2.0, 2.0);
        assert!(obstacle_intersects_box(&rect_obs, -0.5, 0.5, -0.5, 0.5));
        assert!(!obstacle_intersects_box(&rect_obs, 5.0, 10.0, 5.0, 10.0));
    }

    #[test]
    fn test_insert_point_after_selected() {
        let mut data = MissionData::default();
        data.points.push(MissionPoint::new(0.0, 0.0)); // Pt 0
        data.points.push(MissionPoint::new(1.0, 1.0)); // Pt 1
        data.points.push(MissionPoint::new(2.0, 2.0)); // Pt 2

        let mut selection = Selection::default();
        selection.select_single_point(1); // Select Pt 1

        let insert_idx = if selection.points.len() == 1 && selection.obstacles.is_empty() {
            let sel_idx = *selection.points.iter().next().unwrap();
            if sel_idx < data.points.len() {
                sel_idx + 1
            } else {
                data.points.len()
            }
        } else {
            data.points.len()
        };

        assert_eq!(insert_idx, 2);
        data.points.insert(insert_idx, MissionPoint::new(1.5, 1.5));
        selection.select_single_point(insert_idx);

        assert_eq!(data.points.len(), 4);
        assert_eq!(data.points[2].x, 1.5);
        assert!(selection.contains_point(2));
    }
}
