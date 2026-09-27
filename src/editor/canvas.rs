use crate::editor::{
    models::{MissionData, MissionPoint, Obstacle, ObstacleKind, ObstacleShape},
    sidebar::Selection,
    toolbar::{ActiveTool, ShapeType},
};
use egui::{
    pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Response, Sense, Stroke,
    StrokeKind, Ui, Vec2,
};

#[derive(Debug, Clone)]
pub struct ViewVisibilityOptions {
    pub show_margins: bool,
    pub show_path_segments: bool,
    pub show_positive_axes: bool,
    pub show_negative_axes: bool,
    pub show_axis_numbers: bool,
}

impl Default for ViewVisibilityOptions {
    fn default() -> Self {
        Self {
            show_margins: true,
            show_path_segments: true,
            show_positive_axes: true,
            show_negative_axes: true,
            show_axis_numbers: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CanvasState {
    pub pan: Vec2,                      // Posição no mundo (m) correspondente ao centro da tela
    pub zoom: f32,                     // Pixels por metro
    pub drag_target: Option<Selection>, // Objeto capturado no início do clique/arraste
    pub poly_building: Vec<[f32; 2]>,   // Vértices temporários para criação de polígono
    pub view_options: ViewVisibilityOptions, // Opções de visibilidade dos elementos gráficos
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            pan: Vec2::new(3.0, 3.0),
            zoom: 40.0, // 40 pixels = 1 metro por padrão
            drag_target: None,
            poly_building: Vec::new(),
            view_options: ViewVisibilityOptions::default(),
        }
    }
}

impl CanvasState {
    /// Converte Coordenada do Mundo (m) -> Coordenada da Tela (pixels)
    pub fn world_to_screen(&self, world_pos: Vec2, rect: Rect) -> Pos2 {
        let center = rect.center();
        let delta = world_pos - self.pan;
        // No mundo: +Y é para cima. Na tela egui: +Y é para baixo.
        pos2(center.x + delta.x * self.zoom, center.y - delta.y * self.zoom)
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
            let zoom_factor = if scroll_delta > 0.0 { 1.0 + step } else { 1.0 - step };
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

        // 7. Renderizar Polígono em Construção
        if !state.poly_building.is_empty() {
            draw_poly_building(&painter, rect, state, &state.poly_building);
        }

        // 8. Lógica de Seleção e Arraste Direto
        let mouse_pos = response.hover_pos().unwrap_or(rect.center());
        let mouse_world = state.screen_to_world(mouse_pos, rect);

        let primary_pressed = ui.input(|i| i.pointer.primary_pressed());
        let primary_released = ui.input(|i| i.pointer.primary_released());

        if primary_pressed && response.hovered() && !(is_middle_drag || is_right_drag) {
            match active_tool {
                ActiveTool::Select => {
                    let mut hit = Selection::None;

                    // Hit test para pontos
                    for (i, pt) in data.points.iter().enumerate() {
                        let pt_screen = state.world_to_screen(vec2(pt.x, pt.y), rect);
                        let is_already_selected = matches!(selection, Selection::Point(s_idx) if *s_idx == i);

                        let hit_radius = if is_already_selected && state.view_options.show_margins {
                            14.0_f32.max(pt.get_margin() * state.zoom)
                        } else {
                            14.0
                        };

                        if mouse_pos.distance(pt_screen) <= hit_radius {
                            hit = Selection::Point(i);
                            break;
                        }
                    }

                    // Hit test para obstáculos
                    if hit == Selection::None {
                        for (i, obs) in data.obstacles.iter().enumerate() {
                            if hit_test_obstacle(obs, mouse_world) {
                                hit = Selection::Obstacle(i);
                                break;
                            }
                        }
                    }

                    *selection = hit.clone();
                    state.drag_target = if hit != Selection::None { Some(hit) } else { None };
                }
                ActiveTool::AddPoint => {
                    let new_pt = MissionPoint::new(mouse_world.x, mouse_world.y);
                    data.points.push(new_pt);
                    let new_idx = data.points.len() - 1;
                    *selection = Selection::Point(new_idx);
                    state.drag_target = Some(Selection::Point(new_idx));
                }
                ActiveTool::AddObstacle(kind, shape) => match shape {
                    ShapeType::Polygon => {
                        state.poly_building.push([mouse_world.x, mouse_world.y]);
                        if state.poly_building.len() >= 3 && response.double_clicked() {
                            let id = format!("obs_{}", data.obstacles.len() + 1);
                            let obs = Obstacle::new_polygon(id, kind.clone(), state.poly_building.clone());
                            data.obstacles.push(obs);
                            state.poly_building.clear();
                            *selection = Selection::Obstacle(data.obstacles.len() - 1);
                            *active_tool = ActiveTool::Select;
                        }
                    }
                    ShapeType::Rectangle => {
                        let id = format!("obs_{}", data.obstacles.len() + 1);
                        let obs = Obstacle::new_rectangle(id, kind.clone(), mouse_world.x, mouse_world.y, 2.0, 1.5);
                        data.obstacles.push(obs);
                        *selection = Selection::Obstacle(data.obstacles.len() - 1);
                        *active_tool = ActiveTool::Select;
                    }
                    ShapeType::Circle => {
                        let id = format!("obs_{}", data.obstacles.len() + 1);
                        let obs = Obstacle::new_circle(id, kind.clone(), [mouse_world.x, mouse_world.y], 1.0);
                        data.obstacles.push(obs);
                        *selection = Selection::Obstacle(data.obstacles.len() - 1);
                        *active_tool = ActiveTool::Select;
                    }
                },
            }
        }

        // Arrastar elemento capturado
        if response.dragged_by(egui::PointerButton::Primary) && matches!(active_tool, ActiveTool::Select) {
            let delta_world = vec2(response.drag_delta().x / state.zoom, -response.drag_delta().y / state.zoom);

            match state.drag_target {
                Some(Selection::Point(idx)) => {
                    if let Some(pt) = data.points.get_mut(idx) {
                        pt.x += delta_world.x;
                        pt.y += delta_world.y;
                    }
                }
                Some(Selection::Obstacle(idx)) => {
                    if let Some(obs) = data.obstacles.get_mut(idx) {
                        match &mut obs.shape {
                            ObstacleShape::Polygon { vertices } => {
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
                _ => {
                    state.pan.x -= response.drag_delta().x / state.zoom;
                    state.pan.y += response.drag_delta().y / state.zoom;
                }
            }
        }

        if primary_released {
            state.drag_target = None;
        }

        // Overlay: Janela Ancorada de Opções de Exibição no Canto Superior Direito
        egui::Window::new("Exibição")
            .id(egui::Id::new("canvas_display_options_window"))
            .anchor(egui::Align2::RIGHT_TOP, vec2(-10.0, 10.0))
            .title_bar(true)
            .collapsible(true)
            .movable(false)
            .resizable(false)
            .constrain_to(rect)
            .show(ui.ctx(), |ui| {
                ui.checkbox(&mut state.view_options.show_margins, "Visualização das margens");
                ui.checkbox(&mut state.view_options.show_path_segments, "Segmentos de retas");
                ui.checkbox(&mut state.view_options.show_positive_axes, "Eixos positivos (X, Y)");
                ui.checkbox(&mut state.view_options.show_negative_axes, "Eixos negativos (-X, -Y)");
                ui.checkbox(&mut state.view_options.show_axis_numbers, "Números nos eixos");
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
            painter.line_segment([origin_screen, x_pos_end], Stroke::new(2.0, Color32::from_rgb(60, 140, 240)));
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
            painter.line_segment([origin_screen, y_pos_end], Stroke::new(2.0, Color32::from_rgb(60, 220, 100)));
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
            painter.line_segment([origin_screen, x_neg_end], Stroke::new(1.8, Color32::from_rgb(30, 75, 140)));
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
            painter.line_segment([origin_screen, y_neg_end], Stroke::new(1.8, Color32::from_rgb(30, 120, 60)));
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
                if (is_pos && state.view_options.show_positive_axes) || (!is_pos && state.view_options.show_negative_axes) {
                    let p = state.world_to_screen(vec2(x_val, 0.0), rect);
                    if p.x >= rect.left() && p.x <= rect.right() {
                        let tick_color = if is_pos { Color32::from_rgb(60, 140, 240) } else { Color32::from_rgb(30, 75, 140) };
                        painter.line_segment([p - vec2(0.0, 4.0), p + vec2(0.0, 4.0)], Stroke::new(1.2, tick_color));
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
                if (is_pos && state.view_options.show_positive_axes) || (!is_pos && state.view_options.show_negative_axes) {
                    let p = state.world_to_screen(vec2(0.0, y_val), rect);
                    if p.y >= rect.top() && p.y <= rect.bottom() {
                        let tick_color = if is_pos { Color32::from_rgb(60, 220, 100) } else { Color32::from_rgb(30, 120, 60) };
                        painter.line_segment([p - vec2(4.0, 0.0), p + vec2(4.0, 0.0)], Stroke::new(1.2, tick_color));
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
                let num_arrows = if dist > 120.0 { 3 } else if dist > 50.0 { 2 } else { 1 };
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
        let is_selected = matches!(selection, Selection::Point(s_idx) if *s_idx == i);

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
        painter.circle_stroke(pos, pt_radius + 2.0, Stroke::new(1.5, Color32::from_rgb(17, 19, 23)));

        // Rótulo "Marco i"
        let label = format!("Marco {}", i);
        painter.text(
            pos + vec2(0.0, pt_radius + 12.0),
            Align2::CENTER_TOP,
            label,
            FontId::proportional(12.0),
            Color32::WHITE,
        );
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
        let is_selected = matches!(selection, Selection::Obstacle(s_idx) if *s_idx == i);

        let (fill_color, stroke_color) = match obs.kind {
            ObstacleKind::Physical => (
                Color32::from_rgba_unmultiplied(230, 90, 40, 60),
                if is_selected { Color32::WHITE } else { Color32::from_rgb(230, 90, 40) },
            ),
            ObstacleKind::Cosmetic => (
                Color32::from_rgba_unmultiplied(60, 180, 240, 50),
                if is_selected { Color32::WHITE } else { Color32::from_rgb(60, 180, 240) },
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

fn draw_poly_building(painter: &Painter, rect: Rect, state: &CanvasState, vertices: &[[f32; 2]]) {
    let pts: Vec<Pos2> = vertices
        .iter()
        .map(|v| state.world_to_screen(vec2(v[0], v[1]), rect))
        .collect();

    for p in &pts {
        painter.circle_filled(*p, 4.0, Color32::from_rgb(255, 200, 50));
    }

    if pts.len() > 1 {
        for i in 0..pts.len() - 1 {
            painter.line_segment([pts[i], pts[i + 1]], Stroke::new(1.5, Color32::from_rgb(255, 200, 50)));
        }
    }
}

fn hit_test_obstacle(obs: &Obstacle, mouse_world: Vec2) -> bool {
    match &obs.shape {
        ObstacleShape::Polygon { vertices } => point_in_polygon(mouse_world, vertices),
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
