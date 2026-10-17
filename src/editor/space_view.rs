use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use re_chunk_store::external::re_chunk::Chunk;
use re_chunk_store::LatestAtQuery;
use re_log_types::{EntityPath, StoreId, TimeInt, TimePoint, Timeline, Timestamp};
use re_sdk_types::archetypes::{InstancePoses3D, LineStrips3D, Points2D, Scalars, TextDocument, Transform3D};
use re_sdk_types::components::{
    Color, LineStrip3D, Position2D, Radius, RotationAxisAngle, RotationQuat, Scalar, Text,
    TransformMat3x3, Translation3D,
};
use re_sdk_types::reflection::{ViewApplicability, ViewReflection};
use re_sdk_types::ViewClassIdentifier;
use re_viewer::external::re_entity_db::EntityDb;
use re_viewer_context::{
    CommandSender, SystemCommand, SystemCommandSender as _, SystemExecutionOutput, ViewClass,
    ViewClassLayoutPriority, ViewClassRegistryError, ViewClassUiOutput, ViewId, ViewQuery,
    ViewSpawnHeuristics, ViewState, ViewStateExt as _, ViewSystemExecutionError,
    ViewSystemRegistrator, ViewerContext,
};

use crate::editor::canvas::{CanvasState, EditorCanvas};
use crate::editor::models::{MissionData, MissionPoint};

pub const MISSION_POINTS_ENTITY_PATH: &str = "/mission/points";

struct MissionLogSyncState {
    last_store_id: Option<StoreId>,
    last_save_generation: u64,
    seeded_timelines: BTreeSet<String>,
}

static MISSION_LOG_SYNC: OnceLock<Mutex<MissionLogSyncState>> = OnceLock::new();
static MISSION_LOG_TICK: AtomicU64 = AtomicU64::new(1);

fn get_sync_state() -> &'static Mutex<MissionLogSyncState> {
    MISSION_LOG_SYNC.get_or_init(|| {
        Mutex::new(MissionLogSyncState {
            last_store_id: None,
            last_save_generation: crate::editor::mission_save_generation(),
            seeded_timelines: BTreeSet::new(),
        })
    })
}

/// Sincroniza os pontos de missão com a gravação ativa do Rerun sempre que:
/// 1. Entramos em uma nova gravação (ou surgem novas timelines na gravação atual sem log inicial), ou
/// 2. O arquivo de missão é salvo (`notify_mission_saved`).
pub fn sync_mission_points_to_rerun(rerun_app: &re_viewer::App) {
    let Some(recording) = rerun_app.recording_db() else {
        return;
    };
    ensure_mission_points_logged(recording, &rerun_app.command_sender);
}

pub fn ensure_mission_points_logged(
    recording: &EntityDb,
    command_sender: &CommandSender,
) {
    let store_id = recording.store_id().clone();
    let current_gen = crate::editor::mission_save_generation();

    let mut sync_guard = get_sync_state().lock().unwrap_or_else(|e| e.into_inner());

    let is_new_recording = sync_guard.last_store_id.as_ref() != Some(&store_id);
    let is_saved = current_gen != sync_guard.last_save_generation;

    let current_timeline_names: BTreeSet<String> = recording
        .timelines()
        .keys()
        .map(|k| k.as_str().to_string())
        .collect();
    let has_unseeded_timelines = current_timeline_names
        .iter()
        .any(|tl| !sync_guard.seeded_timelines.contains(tl));

    if !is_new_recording && !is_saved && !has_unseeded_timelines {
        return;
    }

    let sets_arc = crate::editor::get_shared_mission_sets();
    let mission_data = {
        let guard = sets_arc.lock().unwrap_or_else(|e| e.into_inner());
        guard.active_data().cloned().unwrap_or_default()
    };

    if is_new_recording {
        sync_guard.last_store_id = Some(store_id.clone());
        sync_guard.last_save_generation = current_gen;
        sync_guard.seeded_timelines = current_timeline_names;
        log_mission_data_chunk(recording, command_sender, &mission_data, true);
    } else if is_saved {
        sync_guard.last_save_generation = current_gen;
        sync_guard.seeded_timelines.extend(current_timeline_names);
        log_mission_data_chunk(recording, command_sender, &mission_data, false);
    } else if has_unseeded_timelines {
        sync_guard.seeded_timelines.extend(current_timeline_names);
        log_mission_data_chunk(recording, command_sender, &mission_data, true);
    }
}

fn log_mission_data_chunk(
    recording: &EntityDb,
    command_sender: &CommandSender,
    data: &MissionData,
    is_initial: bool,
) {
    let mut timepoint = TimePoint::default();
    let now_nanos = Timestamp::now().nanos_since_epoch();

    if is_initial {
        timepoint.insert(Timeline::log_time(), TimeInt::new_temporal(now_nanos));
        timepoint.insert(Timeline::log_tick(), TimeInt::new_temporal(0));
        for (tl_name, timeline) in recording.timelines() {
            if let Some(range) = recording.time_range_for(&tl_name) {
                timepoint.insert(timeline, range.min());
            }
        }
    } else {
        let tick = MISSION_LOG_TICK.fetch_add(1, Ordering::Relaxed) as i64;
        timepoint.insert(Timeline::log_time(), TimeInt::new_temporal(now_nanos));
        timepoint.insert(Timeline::log_tick(), TimeInt::new_temporal(tick));
        for (tl_name, timeline) in recording.timelines() {
            if let Some(range) = recording.time_range_for(&tl_name) {
                timepoint.insert(timeline, range.max());
            }
        }
    }

    let positions: Vec<[f32; 2]> = data.points.iter().map(|p| [p.x, p.y]).collect();
    let radii: Vec<f32> = data.points.iter().map(|p| p.get_margin()).collect();
    let labels: Vec<String> = data
        .points
        .iter()
        .enumerate()
        .map(|(i, _)| format!("M{}", i))
        .collect();

    let points_arch = Points2D::new(positions)
        .with_radii(radii)
        .with_labels(labels);

    let json_payload = serde_json::to_string(data).unwrap_or_default();
    let doc_arch = TextDocument::new(json_payload);

    let entity_path = EntityPath::from(MISSION_POINTS_ENTITY_PATH);
    let chunk_res = Chunk::builder(entity_path)
        .with_archetype_auto_row(timepoint.clone(), &points_arch)
        .with_archetype_auto_row(timepoint, &doc_arch)
        .build();

    if let Ok(chunk) = chunk_res {
        command_sender.send_system(SystemCommand::AppendToStore(
            recording.store_id().clone(),
            vec![chunk],
        ));
    }
}

pub struct MissionSpaceViewState {
    pub canvas_state: CanvasState,
    pub points_entity: String,
    pub pose_entity: String,
    pub route_entity: String,
    pub glow_entity: String,
    pub last_known_param_keys: BTreeSet<String>,
}

impl Default for MissionSpaceViewState {
    fn default() -> Self {
        Self {
            canvas_state: CanvasState::default(),
            points_entity: MISSION_POINTS_ENTITY_PATH.to_string(),
            pose_entity: String::new(),
            route_entity: String::new(),
            glow_entity: String::new(),
            last_known_param_keys: BTreeSet::new(),
        }
    }
}

impl ViewState for MissionSpaceViewState {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn heap_size_bytes(&self) -> u64 {
        0
    }
}

#[derive(Default)]
pub struct MissionSpaceView;

impl MissionSpaceView {
    pub fn register(app: &mut re_viewer::App) -> Result<(), ViewClassRegistryError> {
        app.add_view_class::<Self>(ViewReflection {
            applicability: ViewApplicability::Archetypes(Default::default()),
        })
    }
}

impl ViewClass for MissionSpaceView {
    fn identifier() -> ViewClassIdentifier
    where
        Self: Sized,
    {
        "MissionEditor".into()
    }

    fn display_name(&self) -> &'static str {
        "Visualizador de Missão 2D"
    }

    fn icon(&self) -> &'static re_ui::Icon {
        &re_ui::icons::VIEW_2D
    }

    fn help(&self, _os: egui::os::OperatingSystem) -> re_ui::Help {
        re_ui::Help::new("Painel 2D somente leitura para visualização de marcos da missão, pose do robô e rota sincronizados com a timeline do Rerun")
    }

    fn on_register(
        &self,
        _system_registry: &mut ViewSystemRegistrator<'_>,
    ) -> Result<(), ViewClassRegistryError> {
        Ok(())
    }

    fn new_state(&self) -> Box<dyn ViewState> {
        Box::<MissionSpaceViewState>::default()
    }

    fn layout_priority(&self) -> ViewClassLayoutPriority {
        ViewClassLayoutPriority::Medium
    }

    fn spawn_heuristics(
        &self,
        _ctx: &ViewerContext<'_>,
        _include_entity: &dyn Fn(&re_log_types::EntityPath) -> bool,
    ) -> ViewSpawnHeuristics {
        ViewSpawnHeuristics::empty()
    }

    fn selection_ui(
        &self,
        ctx: &ViewerContext<'_>,
        ui: &mut egui::Ui,
        state: &mut dyn ViewState,
        _space_origin: &EntityPath,
        _view_id: ViewId,
    ) -> Result<(), ViewSystemExecutionError> {
        let state = state.downcast_mut::<MissionSpaceViewState>()?;

        ui.add_space(4.0);
        ui.strong("Exibição");
        ui.separator();

        let opts = &mut state.canvas_state.view_options;
        ui.checkbox(&mut opts.show_margins, "Visualização das margens");
        ui.checkbox(&mut opts.show_path_segments, "Segmentos de retas");
        ui.checkbox(&mut opts.show_positive_axes, "Eixos positivos (X, Y)");
        ui.checkbox(&mut opts.show_negative_axes, "Eixos negativos (-X, -Y)");
        ui.checkbox(&mut opts.show_axis_numbers, "Números nos eixos");
        ui.checkbox(&mut opts.show_parameter_labels, "Valores dos parâmetros");

        if opts.show_parameter_labels && !state.last_known_param_keys.is_empty() {
            ui.indent("mission_view_visible_params", |ui| {
                ui.label(
                    egui::RichText::new("Parâmetros visíveis no mapa:")
                        .small()
                        .weak(),
                );
                for key in &state.last_known_param_keys {
                    let mut is_vis = opts.visible_param_keys.contains(key);
                    if ui.checkbox(&mut is_vis, key).changed() {
                        if is_vis {
                            opts.visible_param_keys.insert(key.clone());
                        } else {
                            opts.visible_param_keys.remove(key);
                        }
                    }
                }
            });
        }

        ui.add_space(10.0);
        ui.strong("Entidades da Timeline (Archetypes)");
        ui.separator();

        let recording = ctx.recording();
        let pose_candidates = find_entities_with_any_component(
            recording,
            &[
                InstancePoses3D::descriptor_translations().component,
                InstancePoses3D::descriptor_quaternions().component,
                Transform3D::descriptor_translation().component,
            ],
        );
        let route_candidates = find_entities_with_any_component(
            recording,
            &[LineStrips3D::descriptor_strips().component],
        );
        let glow_candidates = find_entities_with_any_component(
            recording,
            &[Scalars::descriptor_scalars().component],
        );
        let points_candidates = find_entities_with_any_component(
            recording,
            &[
                Points2D::descriptor_positions().component,
                TextDocument::descriptor_text().component,
            ],
        );

        egui::Grid::new("mission_space_view_entities_grid")
            .num_columns(2)
            .spacing([8.0, 6.0])
            .show(ui, |ui| {
                ui.label("Marcos (Points2D):");
                entity_selector_combo(
                    ui,
                    "points_entity_combo",
                    &mut state.points_entity,
                    &points_candidates,
                    MISSION_POINTS_ENTITY_PATH,
                );
                ui.end_row();

                ui.label("Robô (InstancePoses3D):");
                entity_selector_combo(
                    ui,
                    "pose_entity_combo",
                    &mut state.pose_entity,
                    &pose_candidates,
                    "Auto",
                );
                ui.end_row();

                ui.label("Rota (LineStrips3D):");
                entity_selector_combo(
                    ui,
                    "route_entity_combo",
                    &mut state.route_entity,
                    &route_candidates,
                    "Auto (Todas)",
                );
                ui.end_row();

                ui.label("Brilho (Scalar):");
                entity_selector_combo(
                    ui,
                    "glow_entity_combo",
                    &mut state.glow_entity,
                    &glow_candidates,
                    "Auto",
                );
                ui.end_row();
            });

        Ok(())
    }

    fn ui(
        &self,
        ctx: &ViewerContext<'_>,
        _missing_chunk_reporter: &re_chunk_store::MissingChunkReporter,
        ui: &mut egui::Ui,
        state: &mut dyn ViewState,
        _query: &ViewQuery<'_>,
        _system_output: SystemExecutionOutput,
    ) -> Result<ViewClassUiOutput, ViewSystemExecutionError> {
        let state = state.downcast_mut::<MissionSpaceViewState>()?;

        // Garante que os pontos atuais estão publicados caso tenhamos entrado numa nova gravação ou salvado
        ensure_mission_points_logged(ctx.recording(), ctx.command_sender());

        let latest_query = ctx.current_query();
        let recording = ctx.recording();

        // 1. Consultar pontos da missão na timeline (com fallback para o conjunto ativo em memória)
        let mission_data = query_mission_data_at_timeline(
            recording,
            &latest_query,
            &state.points_entity,
        )
        .unwrap_or_else(|| {
            let sets_arc = crate::editor::get_shared_mission_sets();
            let guard = sets_arc.lock().unwrap_or_else(|e| e.into_inner());
            guard.active_data().cloned().unwrap_or_default()
        });

        for pt in &mission_data.points {
            for k in pt.extra.keys() {
                state.last_known_param_keys.insert(k.clone());
            }
        }

        // 2. Consultar pose do robô (InstancePoses3D) na timeline
        let robot_pose =
            query_robot_pose_at_timeline(recording, &latest_query, &state.pose_entity);

        // 3. Consultar brilho do robô (Scalars: 0 ou 1) na timeline
        let robot_glowing =
            query_robot_glow_at_timeline(recording, &latest_query, &state.glow_entity);

        // 4. Consultar rota (LineStrips3D) na timeline
        let route_strips =
            query_route_strips_at_timeline(recording, &latest_query, &state.route_entity);

        let zoom_speed = crate::editor::get_shared_zoom_speed();
        EditorCanvas::show_read_only(
            ui,
            &mut state.canvas_state,
            &mission_data,
            robot_pose,
            robot_glowing,
            &route_strips,
            zoom_speed,
        );

        Ok(ViewClassUiOutput::default())
    }
}

fn entity_selector_combo(
    ui: &mut egui::Ui,
    id_salt: &str,
    selected: &mut String,
    candidates: &[EntityPath],
    empty_label: &str,
) {
    let display_text = if selected.is_empty() {
        empty_label.to_string()
    } else {
        selected.clone()
    };

    egui::ComboBox::from_id_salt(id_salt)
        .selected_text(display_text)
        .width(170.0)
        .show_ui(ui, |ui| {
            let is_auto = selected.is_empty()
                || (empty_label == MISSION_POINTS_ENTITY_PATH
                    && selected == MISSION_POINTS_ENTITY_PATH);
            if empty_label == MISSION_POINTS_ENTITY_PATH {
                if ui
                    .selectable_label(
                        selected == MISSION_POINTS_ENTITY_PATH,
                        MISSION_POINTS_ENTITY_PATH,
                    )
                    .clicked()
                {
                    *selected = MISSION_POINTS_ENTITY_PATH.to_string();
                }
            } else if ui.selectable_label(is_auto, empty_label).clicked() {
                selected.clear();
            }

            for cand in candidates {
                let path_str = cand.to_string();
                if path_str == MISSION_POINTS_ENTITY_PATH
                    && empty_label == MISSION_POINTS_ENTITY_PATH
                {
                    continue;
                }
                if ui
                    .selectable_label(*selected == path_str, &path_str)
                    .clicked()
                {
                    *selected = path_str;
                }
            }
        });
}

fn find_entities_with_any_component(
    recording: &EntityDb,
    components: &[re_sdk_types::ComponentIdentifier],
) -> Vec<EntityPath> {
    let engine = recording.storage_engine();
    let schema = engine.store().schema();
    let mut out = Vec::new();

    for entity_path in recording.sorted_entity_paths() {
        if let Some(entity_comps) = schema.all_components_for_entity(entity_path) {
            if components.iter().any(|c| entity_comps.contains(c)) {
                out.push(entity_path.clone());
            }
        }
    }
    out
}

fn query_mission_data_at_timeline(
    recording: &EntityDb,
    query: &LatestAtQuery,
    points_entity_override: &str,
) -> Option<MissionData> {
    let path_str = if points_entity_override.trim().is_empty() {
        MISSION_POINTS_ENTITY_PATH
    } else {
        points_entity_override.trim()
    };
    let entity_path = EntityPath::from(path_str);

    let text_comp = TextDocument::descriptor_text().component;
    let pos_comp = Points2D::descriptor_positions().component;
    let rad_comp = Points2D::descriptor_radii().component;

    if let Some(data) =
        query_mission_data_at_entity(recording, query, &entity_path, text_comp, pos_comp, rad_comp)
    {
        return Some(data);
    }

    // Caso o cursor da timeline esteja antes do primeiro ponto salvo nessa timeline,
    // busca o primeiro registro disponível na timeline log_time (TimeInt::MAX) como estado inicial da gravação.
    let fallback_query = LatestAtQuery::new(*Timeline::log_time().name(), TimeInt::MAX);
    query_mission_data_at_entity(
        recording,
        &fallback_query,
        &entity_path,
        text_comp,
        pos_comp,
        rad_comp,
    )
}

fn query_mission_data_at_entity(
    recording: &EntityDb,
    query: &LatestAtQuery,
    entity_path: &EntityPath,
    text_comp: re_sdk_types::ComponentIdentifier,
    pos_comp: re_sdk_types::ComponentIdentifier,
    rad_comp: re_sdk_types::ComponentIdentifier,
) -> Option<MissionData> {
    let results = recording.latest_at(query, entity_path, [text_comp, pos_comp, rad_comp]);

    if let Some(text) = results.component_mono_quiet::<Text>(text_comp) {
        if let Ok(parsed) = serde_json::from_str::<MissionData>(text.0.as_str()) {
            return Some(parsed);
        }
    }

    let positions = results.component_batch_quiet::<Position2D>(pos_comp)?;
    let radii = results
        .component_batch_quiet::<Radius>(rad_comp)
        .unwrap_or_default();

    let points = positions
        .into_iter()
        .enumerate()
        .map(|(i, pos)| {
            let margin = radii
                .get(i)
                .or_else(|| radii.first())
                .map(|r| r.0 .0)
                .unwrap_or(0.5);
            let mut extra = indexmap::IndexMap::new();
            extra.insert(
                "margin".to_string(),
                serde_yaml::Value::from(margin as f64),
            );
            MissionPoint {
                x: pos.0 .0[0],
                y: pos.0 .0[1],
                extra,
            }
        })
        .collect();

    Some(MissionData {
        points,
        obstacles: Vec::new(),
    })
}

fn query_robot_pose_at_timeline(
    recording: &EntityDb,
    query: &LatestAtQuery,
    pose_entity_override: &str,
) -> Option<[f32; 3]> {
    let inst_trans = InstancePoses3D::descriptor_translations().component;
    let inst_quat = InstancePoses3D::descriptor_quaternions().component;
    let inst_axis = InstancePoses3D::descriptor_rotation_axis_angles().component;
    let inst_mat = InstancePoses3D::descriptor_mat3x3().component;

    let tf_trans = Transform3D::descriptor_translation().component;
    let tf_quat = Transform3D::descriptor_quaternion().component;
    let tf_axis = Transform3D::descriptor_rotation_axis_angle().component;
    let tf_mat = Transform3D::descriptor_mat3x3().component;

    let all_pose_comps = [
        inst_trans, inst_quat, inst_axis, inst_mat, tf_trans, tf_quat, tf_axis, tf_mat,
    ];

    let candidates: Vec<EntityPath> = if !pose_entity_override.trim().is_empty() {
        vec![EntityPath::from(pose_entity_override.trim())]
    } else {
        // Prioriza entidades com InstancePoses3D primeiro, depois Transform3D
        let mut list = find_entities_with_any_component(
            recording,
            &[inst_trans, inst_quat, inst_axis, inst_mat],
        );
        let tf_list =
            find_entities_with_any_component(recording, &[tf_trans, tf_quat, tf_axis, tf_mat]);
        for e in tf_list {
            if !list.contains(&e) {
                list.push(e);
            }
        }
        list
    };

    for entity_path in candidates {
        let results = recording.latest_at(query, &entity_path, all_pose_comps);

        let translation = results
            .component_batch_quiet::<Translation3D>(inst_trans)
            .and_then(|v| v.into_iter().next())
            .or_else(|| results.component_mono_quiet::<Translation3D>(tf_trans));

        let quat = results
            .component_batch_quiet::<RotationQuat>(inst_quat)
            .and_then(|v| v.into_iter().next())
            .or_else(|| results.component_mono_quiet::<RotationQuat>(tf_quat));

        let axis_angle = results
            .component_batch_quiet::<RotationAxisAngle>(inst_axis)
            .and_then(|v| v.into_iter().next())
            .or_else(|| results.component_mono_quiet::<RotationAxisAngle>(tf_axis));

        let mat3 = results
            .component_batch_quiet::<TransformMat3x3>(inst_mat)
            .and_then(|v| v.into_iter().next())
            .or_else(|| results.component_mono_quiet::<TransformMat3x3>(tf_mat));

        if translation.is_none() && quat.is_none() && axis_angle.is_none() && mat3.is_none() {
            continue;
        }

        let [x, y, _z] = translation.map(|t| t.0 .0).unwrap_or([0.0, 0.0, 0.0]);
        let yaw = if let Some(q) = quat {
            let [qx, qy, qz, qw] = q.0 .0;
            f32::atan2(
                2.0 * (qw * qz + qx * qy),
                1.0 - 2.0 * (qy * qy + qz * qz),
            )
        } else if let Some(aa) = axis_angle {
            let axis_z = aa.0.axis.0[2];
            aa.0.angle.radians() * if axis_z >= 0.0 { 1.0 } else { -1.0 }
        } else if let Some(m) = mat3 {
            // Coluna 0 da matriz 3x3 (column-major): [m[0], m[1], m[2]]
            f32::atan2(m.0 .0[1], m.0 .0[0])
        } else {
            0.0
        };

        return Some([x, y, yaw]);
    }

    None
}

fn query_route_strips_at_timeline(
    recording: &EntityDb,
    query: &LatestAtQuery,
    route_entity_override: &str,
) -> Vec<(egui::Color32, Vec<[f32; 2]>)> {
    let strip_comp = LineStrips3D::descriptor_strips().component;
    let color_comp = LineStrips3D::descriptor_colors().component;

    let candidates: Vec<EntityPath> = if !route_entity_override.trim().is_empty() {
        vec![EntityPath::from(route_entity_override.trim())]
    } else {
        find_entities_with_any_component(recording, &[strip_comp])
    };

    let default_color = egui::Color32::from_rgb(0, 210, 255);
    let mut out = Vec::new();

    for entity_path in candidates {
        let results = recording.latest_at(query, &entity_path, [strip_comp, color_comp]);
        let Some(strips) = results.component_batch_quiet::<LineStrip3D>(strip_comp) else {
            continue;
        };
        let colors = results
            .component_batch_quiet::<Color>(color_comp)
            .unwrap_or_default();

        for (i, strip) in strips.into_iter().enumerate() {
            let color = colors
                .get(i)
                .or_else(|| colors.first())
                .copied()
                .map(egui::Color32::from)
                .unwrap_or(default_color);

            let pts_2d: Vec<[f32; 2]> = strip.0.into_iter().map(|v| [v.0[0], v.0[1]]).collect();
            if pts_2d.len() >= 2 {
                out.push((color, pts_2d));
            }
        }
    }

    out
}

fn query_robot_glow_at_timeline(
    recording: &EntityDb,
    query: &LatestAtQuery,
    glow_entity_override: &str,
) -> bool {
    let scalar_comp = Scalars::descriptor_scalars().component;

    let candidates: Vec<EntityPath> = if !glow_entity_override.trim().is_empty() {
        vec![EntityPath::from(glow_entity_override.trim())]
    } else {
        let mut all = find_entities_with_any_component(recording, &[scalar_comp]);
        // Ordena priorizando entidades cujo nome sugira relação com o robô/brilho
        all.sort_by_key(|ep| {
            let s = ep.to_string().to_lowercase();
            if s.contains("glow") || s.contains("brilho") || s.contains("blink") {
                0
            } else if s.contains("robot") || s.contains("pose") || s.contains("mission") {
                1
            } else {
                2
            }
        });
        all
    };

    for entity_path in candidates {
        let results = recording.latest_at(query, &entity_path, [scalar_comp]);
        if let Some(scalars) = results.component_batch_quiet::<Scalar>(scalar_comp) {
            if let Some(first) = scalars.first() {
                return first.0 .0 >= 0.5;
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use re_log_types::{StoreKind, TimelineName};
    use re_sdk_types::datatypes::Quaternion;
    use std::sync::Arc;

    fn make_test_db() -> EntityDb {
        EntityDb::new(StoreId::random(StoreKind::Recording, "test_mission_view"))
    }

    #[test]
    fn test_timeline_mission_points_history() {
        let mut db = make_test_db();
        let tl = Timeline::new_sequence("step");

        let mut data_t1 = MissionData::default();
        data_t1.points.push(MissionPoint::new(1.0, 2.0));

        let mut data_t2 = MissionData::default();
        data_t2.points.push(MissionPoint::new(1.0, 2.0));
        data_t2.points.push(MissionPoint::new(5.0, -3.0));

        let tp1 = TimePoint::from([(tl, TimeInt::new_temporal(10))]);
        let tp2 = TimePoint::from([(tl, TimeInt::new_temporal(20))]);

        let chunk1 = Chunk::builder(EntityPath::from(MISSION_POINTS_ENTITY_PATH))
            .with_archetype_auto_row(
                tp1.clone(),
                &Points2D::new([[1.0, 2.0]]).with_radii([0.5]),
            )
            .with_archetype_auto_row(
                tp1,
                &TextDocument::new(serde_json::to_string(&data_t1).unwrap()),
            )
            .build()
            .unwrap();

        let chunk2 = Chunk::builder(EntityPath::from(MISSION_POINTS_ENTITY_PATH))
            .with_archetype_auto_row(
                tp2.clone(),
                &Points2D::new([[1.0, 2.0], [5.0, -3.0]]).with_radii([0.5, 0.5]),
            )
            .with_archetype_auto_row(
                tp2,
                &TextDocument::new(serde_json::to_string(&data_t2).unwrap()),
            )
            .build()
            .unwrap();

        db.add_chunk(&Arc::new(chunk1)).unwrap();
        db.add_chunk(&Arc::new(chunk2)).unwrap();

        let q1 = LatestAtQuery::new(TimelineName::from("step"), TimeInt::new_temporal(15));
        let res1 = query_mission_data_at_timeline(&db, &q1, "").unwrap();
        assert_eq!(res1.points.len(), 1);
        assert_eq!(res1.points[0].x, 1.0);
        assert_eq!(res1.points[0].y, 2.0);

        let q2 = LatestAtQuery::new(TimelineName::from("step"), TimeInt::new_temporal(25));
        let res2 = query_mission_data_at_timeline(&db, &q2, "").unwrap();
        assert_eq!(res2.points.len(), 2);
        assert_eq!(res2.points[1].x, 5.0);
        assert_eq!(res2.points[1].y, -3.0);
    }

    #[test]
    fn test_timeline_pose_route_and_glow_archetypes() {
        let mut db = make_test_db();
        let tl = Timeline::new_sequence("step");
        let tp1 = TimePoint::from([(tl, TimeInt::new_temporal(10))]);
        let tp2 = TimePoint::from([(tl, TimeInt::new_temporal(20))]);

        // Rotação de 90 graus (pi/2) em torno de Z: q = [0, 0, sin(pi/4), cos(pi/4)]
        let half = std::f32::consts::FRAC_PI_4;
        let quat_90 = Quaternion::from_xyzw([0.0, 0.0, half.sin(), half.cos()]);

        let pose_chunk = Chunk::builder(EntityPath::from("/robot/pose"))
            .with_archetype_auto_row(
                tp1.clone(),
                &InstancePoses3D::new()
                    .with_translations([[3.5, -1.5, 0.0]])
                    .with_quaternions([quat_90]),
            )
            .build()
            .unwrap();

        let route_chunk = Chunk::builder(EntityPath::from("/robot/route"))
            .with_archetype_auto_row(
                tp1.clone(),
                &LineStrips3D::new([vec![[0.0, 0.0, 0.0], [3.5, -1.5, 0.0], [7.0, 2.0, 0.0]]]),
            )
            .build()
            .unwrap();

        let glow_off_chunk = Chunk::builder(EntityPath::from("/robot/glow"))
            .with_archetype_auto_row(tp1, &Scalars::single(0.0))
            .build()
            .unwrap();
        let glow_on_chunk = Chunk::builder(EntityPath::from("/robot/glow"))
            .with_archetype_auto_row(tp2, &Scalars::single(1.0))
            .build()
            .unwrap();

        db.add_chunk(&Arc::new(pose_chunk)).unwrap();
        db.add_chunk(&Arc::new(route_chunk)).unwrap();
        db.add_chunk(&Arc::new(glow_off_chunk)).unwrap();
        db.add_chunk(&Arc::new(glow_on_chunk)).unwrap();

        let q1 = LatestAtQuery::new(TimelineName::from("step"), TimeInt::new_temporal(12));
        let pose = query_robot_pose_at_timeline(&db, &q1, "").unwrap();
        assert!((pose[0] - 3.5).abs() < 1e-4);
        assert!((pose[1] - (-1.5)).abs() < 1e-4);
        assert!((pose[2] - std::f32::consts::FRAC_PI_2).abs() < 1e-4);

        let strips = query_route_strips_at_timeline(&db, &q1, "");
        assert_eq!(strips.len(), 1);
        assert_eq!(strips[0].1.len(), 3);
        assert_eq!(strips[0].1[2], [7.0, 2.0]);

        assert!(!query_robot_glow_at_timeline(&db, &q1, ""));

        let q2 = LatestAtQuery::new(TimelineName::from("step"), TimeInt::new_temporal(22));
        assert!(query_robot_glow_at_timeline(&db, &q2, ""));
    }
}


