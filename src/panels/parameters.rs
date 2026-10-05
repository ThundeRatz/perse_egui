use egui::{Color32, Frame, Margin, RichText, Ui};
use serde::{Deserialize, Serialize};

use crate::panels::components::section_header;
use crate::state::AppState;

/// Tipos de dados de parâmetros suportados pelo ROS 2
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ParameterValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    ByteArray(Vec<u8>),
    BoolArray(Vec<bool>),
    IntArray(Vec<i64>),
    FloatArray(Vec<f64>),
    StringArray(Vec<String>),
}

impl ParameterValue {
    pub fn type_name(&self) -> &'static str {
        match self {
            ParameterValue::Bool(_) => "bool",
            ParameterValue::Int(_) => "int",
            ParameterValue::Float(_) => "float",
            ParameterValue::String(_) => "string",
            ParameterValue::ByteArray(_) => "byte[]",
            ParameterValue::BoolArray(_) => "bool[]",
            ParameterValue::IntArray(_) => "int[]",
            ParameterValue::FloatArray(_) => "float[]",
            ParameterValue::StringArray(_) => "string[]",
        }
    }
}

/// Representa um único parâmetro ROS 2
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    pub description: String,
    pub saved_value: ParameterValue,
    pub edited_value: ParameterValue,
    // Buffer temporário para edição de textos de arrays em formato string
    pub array_text_buf: Option<String>,
}

impl Parameter {
    pub fn new(name: impl Into<String>, desc: impl Into<String>, val: ParameterValue) -> Self {
        Self {
            name: name.into(),
            description: desc.into(),
            saved_value: val.clone(),
            edited_value: val,
            array_text_buf: None,
        }
    }

    pub fn is_modified(&self) -> bool {
        self.saved_value != self.edited_value
    }

    pub fn apply(&mut self) {
        self.saved_value = self.edited_value.clone();
        self.array_text_buf = None;
    }

    pub fn discard(&mut self) {
        self.edited_value = self.saved_value.clone();
        self.array_text_buf = None;
    }
}

/// Escopo hierárquico (pode conter sub-escopos recursivamente e parâmetros)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scope {
    pub name: String,
    pub sub_scopes: Vec<Scope>,
    pub parameters: Vec<Parameter>,
}

impl Scope {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            sub_scopes: Vec::new(),
            parameters: Vec::new(),
        }
    }

    pub fn count_modified(&self) -> usize {
        let mut count = self.parameters.iter().filter(|p| p.is_modified()).count();
        for sub in &self.sub_scopes {
            count += sub.count_modified();
        }
        count
    }

    pub fn apply_all(&mut self) {
        for p in &mut self.parameters {
            p.apply();
        }
        for sub in &mut self.sub_scopes {
            sub.apply_all();
        }
    }

    pub fn discard_all(&mut self) {
        for p in &mut self.parameters {
            p.discard();
        }
        for sub in &mut self.sub_scopes {
            sub.discard_all();
        }
    }

    pub fn matches_search(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        let q = query.to_lowercase();
        if self.name.to_lowercase().contains(&q) {
            return true;
        }
        if self.parameters.iter().any(|p| {
            p.name.to_lowercase().contains(&q) || p.description.to_lowercase().contains(&q)
        }) {
            return true;
        }
        self.sub_scopes.iter().any(|sub| sub.matches_search(query))
    }

    pub fn has_parameter_match(&self, query: &str) -> bool {
        if query.is_empty() {
            return false;
        }
        let q = query.to_lowercase();
        if self.parameters.iter().any(|p| {
            p.name.to_lowercase().contains(&q) || p.description.to_lowercase().contains(&q)
        }) {
            return true;
        }
        self.sub_scopes
            .iter()
            .any(|sub| sub.has_parameter_match(query))
    }
}

/// Arquivo de parâmetros (.yaml / .yml)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamFile {
    pub filename: String,
    pub scopes: Vec<Scope>,
}

impl ParamFile {
    pub fn count_modified(&self) -> usize {
        self.scopes.iter().map(|s| s.count_modified()).sum()
    }

    pub fn apply_all(&mut self) {
        for s in &mut self.scopes {
            s.apply_all();
        }
    }

    pub fn discard_all(&mut self) {
        for s in &mut self.scopes {
            s.discard_all();
        }
    }

    pub fn matches_search(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        let q = query.to_lowercase();
        if self.filename.to_lowercase().contains(&q) {
            return true;
        }
        self.scopes.iter().any(|s| s.matches_search(query))
    }

    pub fn has_parameter_match(&self, query: &str) -> bool {
        if query.is_empty() {
            return false;
        }
        self.scopes.iter().any(|s| s.has_parameter_match(query))
    }
}

/// Pacote ROS 2 (Sem agrupação de workspace, os pacotes são o topo da árvore)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageParams {
    pub name: String,
    pub files: Vec<ParamFile>,
}

impl PackageParams {
    pub fn count_modified(&self) -> usize {
        self.files.iter().map(|f| f.count_modified()).sum()
    }

    pub fn apply_all(&mut self) {
        for f in &mut self.files {
            f.apply_all();
        }
    }

    pub fn discard_all(&mut self) {
        for f in &mut self.files {
            f.discard_all();
        }
    }

    pub fn matches_search(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        let q = query.to_lowercase();
        if self.name.to_lowercase().contains(&q) {
            return true;
        }
        self.files.iter().any(|f| f.matches_search(query))
    }

    pub fn has_parameter_match(&self, query: &str) -> bool {
        if query.is_empty() {
            return false;
        }
        self.files.iter().any(|f| f.has_parameter_match(query))
    }
}

/// Painel de Parâmetros
#[derive(Debug)]
pub struct ParametersPanel {
    pub packages: Vec<PackageParams>,
    pub search_active: bool,
    pub search_query: String,
    pub first_frame: bool,
    pub is_panel_focused: bool,
}

impl Default for ParametersPanel {
    fn default() -> Self {
        Self {
            packages: create_mock_parameters(),
            search_active: false,
            search_query: String::new(),
            first_frame: true,
            is_panel_focused: false,
        }
    }
}

impl ParametersPanel {
    pub fn count_modified(&self) -> usize {
        self.packages.iter().map(|pkg| pkg.count_modified()).sum()
    }

    pub fn apply_all(&mut self) {
        for pkg in &mut self.packages {
            pkg.apply_all();
        }
    }

    pub fn discard_all(&mut self) {
        for pkg in &mut self.packages {
            pkg.discard_all();
        }
    }

    pub fn update_from_remote(&mut self, remote_packages: Vec<PackageParams>) {
        if self.count_modified() == 0 {
            self.packages = remote_packages;
            return;
        }

        for remote_pkg in remote_packages {
            if let Some(local_pkg) = self.packages.iter_mut().find(|p| p.name == remote_pkg.name) {
                for remote_file in remote_pkg.files {
                    if let Some(local_file) = local_pkg
                        .files
                        .iter_mut()
                        .find(|f| f.filename == remote_file.filename)
                    {
                        for mut remote_scope in remote_file.scopes {
                            if let Some(local_scope) = local_file
                                .scopes
                                .iter_mut()
                                .find(|s| s.name == remote_scope.name)
                            {
                                preserve_scope_edits(local_scope, &mut remote_scope);
                                *local_scope = remote_scope;
                            } else {
                                local_file.scopes.push(remote_scope);
                            }
                        }
                    } else {
                        local_pkg.files.push(remote_file);
                    }
                }
            } else {
                self.packages.push(remote_pkg);
            }
        }
    }

    pub fn ui(
        &mut self,
        ui: &mut Ui,
        _state: &AppState,
        color_hierarchy: bool,
        client: Option<&crate::net::client::ControlClient>,
    ) {
        let is_first_frame = self.first_frame;
        self.first_frame = false;

        let is_hovered = ui.rect_contains_pointer(ui.max_rect());
        if ui.input(|i| i.pointer.any_pressed()) {
            self.is_panel_focused = is_hovered;
        }

        let panel_active = is_hovered || self.is_panel_focused;
        if panel_active {
            let ctrl_s_pressed = ui.input_mut(|i| {
                i.consume_shortcut(&egui::KeyboardShortcut::new(
                    egui::Modifiers::COMMAND,
                    egui::Key::S,
                ))
            });
            if ctrl_s_pressed {
                self.apply_all();
                if let Some(c) = client {
                    c.send(crate::net::protocol::ControlMessage::new(
                        crate::net::protocol::Domain::Parameters,
                        "apply_params",
                        serde_json::to_value(crate::net::protocol::ApplyParametersRequest {
                            package: None,
                            packages: self.packages.clone(),
                        })
                        .unwrap_or_default(),
                    ));
                }
            }
        }

        let modified_count = self.count_modified();

        enum HeaderAction {
            None,
            Apply,
            Discard,
        }

        let mut header_action = HeaderAction::None;

        // Cabeçalho com busca e botão de Aplicar condicional
        section_header(
            ui,
            "Parâmetros",
            &mut self.search_active,
            &mut self.search_query,
            |ui| {
                if modified_count > 0 {
                    if crate::panels::components::re_icon_text_button(
                        ui,
                        &re_ui::icons::CHECKED,
                        &format!("Aplicar ({})", modified_count),
                    )
                    .on_hover_text("Aplicar alterações de parâmetros (Ctrl+S)")
                    .clicked()
                    {
                        header_action = HeaderAction::Apply;
                    }

                    if crate::panels::components::re_icon_text_button(
                        ui,
                        &re_ui::icons::RESET,
                        "Desfazer",
                    )
                    .clicked()
                    {
                        header_action = HeaderAction::Discard;
                    }
                }
            },
        );

        match header_action {
            HeaderAction::Apply => {
                self.apply_all();
                if let Some(c) = client {
                    c.send(crate::net::protocol::ControlMessage::new(
                        crate::net::protocol::Domain::Parameters,
                        "apply_params",
                        serde_json::to_value(crate::net::protocol::ApplyParametersRequest {
                            package: None,
                            packages: self.packages.clone(),
                        })
                        .unwrap_or_default(),
                    ));
                }
            }
            HeaderAction::Discard => self.discard_all(),
            HeaderAction::None => {}
        }

        // Corpo do painel com a árvore de pacotes, escopos e parâmetros
        Frame::new()
            .inner_margin(Margin {
                left: 4,
                right: 4,
                top: 2,
                bottom: 2,
            })
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;

                egui::ScrollArea::both()
                    .id_salt("parameters_tree_scroll")
                    .min_scrolled_height(0.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        let query = self.search_query.trim().to_lowercase();

                        let visible_packages: Vec<&mut PackageParams> = self
                            .packages
                            .iter_mut()
                            .filter(|pkg| pkg.matches_search(&query))
                            .collect();

                        if visible_packages.is_empty() {
                            ui.add_space(8.0);
                            ui.label(
                                RichText::new("Nenhum parâmetro encontrado")
                                    .italics()
                                    .color(Color32::GRAY),
                            );
                            return;
                        }

                        for pkg in visible_packages {
                            render_package(ui, pkg, &query, is_first_frame, color_hierarchy);
                        }
                    });
            });
    }
}

fn preserve_scope_edits(local: &Scope, remote: &mut Scope) {
    for remote_param in &mut remote.parameters {
        if let Some(local_param) = local
            .parameters
            .iter()
            .find(|p| p.name == remote_param.name)
        {
            if local_param.is_modified() {
                remote_param.edited_value = local_param.edited_value.clone();
                remote_param.array_text_buf = local_param.array_text_buf.clone();
            }
        }
    }
    for remote_sub in &mut remote.sub_scopes {
        if let Some(local_sub) = local.sub_scopes.iter().find(|s| s.name == remote_sub.name) {
            preserve_scope_edits(local_sub, remote_sub);
        }
    }
}

/// Renderiza um pacote ROS 2 no nível superior
fn render_package(
    ui: &mut Ui,
    pkg: &mut PackageParams,
    query: &str,
    is_first_frame: bool,
    color_hierarchy: bool,
) {
    let mod_count = pkg.count_modified();
    let title = if mod_count > 0 {
        format!("📦 {} (*{})", pkg.name, mod_count)
    } else {
        format!("📦 {}", pkg.name)
    };

    let param_match = pkg.has_parameter_match(query);
    let pkg_self_match = !query.is_empty() && pkg.name.to_lowercase().contains(query);
    let force_show_all = pkg_self_match && !param_match;

    let open_override = if is_first_frame { Some(true) } else { None };

    let header_text = if color_hierarchy {
        RichText::new(title)
            .strong()
            .size(13.0)
            .color(Color32::from_rgb(130, 200, 255))
    } else {
        RichText::new(title).strong().size(13.0)
    };

    egui::CollapsingHeader::new(header_text)
        .id_salt(format!("pkg_collapsing_{}", pkg.name))
        .default_open(true)
        .open(open_override)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            for file in &mut pkg.files {
                if force_show_all || file.matches_search(query) {
                    render_param_file(
                        ui,
                        &pkg.name,
                        file,
                        query,
                        force_show_all,
                        is_first_frame,
                        color_hierarchy,
                    );
                }
            }
        });
}

/// Renderiza um arquivo de parâmetros dentro do pacote
fn render_param_file(
    ui: &mut Ui,
    pkg_name: &str,
    file: &mut ParamFile,
    query: &str,
    parent_force_all: bool,
    is_first_frame: bool,
    color_hierarchy: bool,
) {
    let mod_count = file.count_modified();
    let title = if mod_count > 0 {
        format!("📄 {} (*{})", file.filename, mod_count)
    } else {
        format!("📄 {}", file.filename)
    };

    let path_prefix = format!("{}/{}", pkg_name, file.filename);

    let param_match = file.has_parameter_match(query);
    let file_self_match = !query.is_empty() && file.filename.to_lowercase().contains(query);
    let force_show_all = parent_force_all || (file_self_match && !param_match);

    let open_override = if is_first_frame { Some(true) } else { None };

    let header_text = if color_hierarchy {
        RichText::new(title)
            .strong()
            .size(12.5)
            .color(Color32::from_rgb(240, 200, 110))
    } else {
        RichText::new(title).color(Color32::from_gray(200))
    };

    egui::CollapsingHeader::new(header_text)
        .id_salt(format!("file_collapsing_{}", path_prefix))
        .default_open(true)
        .open(open_override)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            for scope in &mut file.scopes {
                if force_show_all || scope.matches_search(query) {
                    render_scope(
                        ui,
                        &path_prefix,
                        scope,
                        query,
                        0,
                        force_show_all,
                        is_first_frame,
                        color_hierarchy,
                    );
                }
            }
        });
}

/// Renderiza escopos de forma recursiva com suporte a simplificação de caminhos de filho único (ex: nav/params/planner)
fn render_scope(
    ui: &mut Ui,
    path_prefix: &str,
    scope: &mut Scope,
    query: &str,
    depth: usize,
    parent_force_all: bool,
    is_first_frame: bool,
    color_hierarchy: bool,
) {
    // Se o escopo atual não tiver parâmetros próprios E tiver exatamente 1 sub-escopo,
    // comprime a cadeia em um único caminho visual (ex: "nav/params/planner")
    let mut current_name = scope.name.clone();
    let mut current_scope = scope;

    while current_scope.parameters.is_empty() && current_scope.sub_scopes.len() == 1 {
        let child_name = current_scope.sub_scopes[0].name.clone();
        current_name = format!("{}/{}", current_name, child_name);
        current_scope = &mut current_scope.sub_scopes[0];
    }

    let mod_count = current_scope.count_modified();
    let title = if mod_count > 0 {
        format!("📁 {} (*{})", current_name, mod_count)
    } else {
        format!("📁 {}", current_name)
    };

    let current_path = format!("{}/{}", path_prefix, current_name);

    let param_match = current_scope.has_parameter_match(query);
    let scope_self_match = !query.is_empty() && current_name.to_lowercase().contains(query);
    let force_show_all = parent_force_all || (scope_self_match && !param_match);

    let open_override = if is_first_frame { Some(true) } else { None };

    let header_text = if color_hierarchy {
        RichText::new(title)
            .strong()
            .size(12.0)
            .color(Color32::from_rgb(195, 160, 245))
    } else {
        RichText::new(title).size(12.5)
    };

    egui::CollapsingHeader::new(header_text)
        .id_salt(format!("scope_collapsing_{}", current_path))
        .default_open(true)
        .open(open_override)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            // Renderiza sub-escopos do escopo comprimido recursivamente
            for sub in &mut current_scope.sub_scopes {
                if force_show_all || sub.matches_search(query) {
                    render_scope(
                        ui,
                        &current_path,
                        sub,
                        query,
                        depth + 1,
                        force_show_all,
                        is_first_frame,
                        color_hierarchy,
                    );
                }
            }

            // Renderiza parâmetros do escopo comprimido
            for param in &mut current_scope.parameters {
                if force_show_all
                    || query.is_empty()
                    || param.name.to_lowercase().contains(query)
                    || param.description.to_lowercase().contains(query)
                {
                    render_parameter_row(ui, param, color_hierarchy);
                }
            }
        });
}

/// Renderiza a linha de um único parâmetro com controle específico para seu tipo de dado e destaque de estado não salvo
fn render_parameter_row(ui: &mut Ui, param: &mut Parameter, color_hierarchy: bool) {
    let param_name = param.name.clone();
    ui.push_id(&param_name, |ui| {
        ui.add_space(0.5);

        let is_modified = param.is_modified();

        // Moldura com cor diferenciada se estiver em estado não salvo (unsaved state)
        let frame_bg = if is_modified {
            Color32::from_rgb(60, 45, 15) // Tom ambar/amarelo para estado modificado
        } else {
            Color32::TRANSPARENT
        };

        let stroke = if is_modified {
            egui::Stroke::new(1.0, Color32::from_rgb(220, 160, 40))
        } else {
            egui::Stroke::NONE
        };

        let name_color = if is_modified {
            Color32::from_rgb(255, 210, 100)
        } else if color_hierarchy {
            Color32::from_rgb(175, 235, 195)
        } else {
            Color32::from_gray(220)
        };

        Frame::new()
            .fill(frame_bg)
            .stroke(stroke)
            .inner_margin(Margin {
                left: 4,
                right: 4,
                top: 1,
                bottom: 1,
            })
            .corner_radius(3.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Nome do parâmetro e badge de tipo
                    ui.label(
                        RichText::new(&param.name)
                            .strong()
                            .size(11.5)
                            .color(name_color),
                    );

                    ui.label(
                        RichText::new(format!("[{}]", param.edited_value.type_name()))
                            .size(9.5)
                            .color(Color32::from_gray(130)),
                    );

                    // Controle de entrada específico para o tipo de dado.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        render_value_input(ui, param);

                        if is_modified
                            && crate::panels::components::re_icon_button(
                                ui,
                                &re_ui::icons::RESET,
                                "Desfazer alteração",
                            )
                            .clicked()
                        {
                            param.discard();
                        }
                    });
                });

                if !param.description.is_empty() {
                    ui.label(
                        RichText::new(&param.description)
                            .size(9.5)
                            .italics()
                            .color(Color32::from_gray(135)),
                    );
                }
            });
    });
}

/// Renderiza o controle de input adequado para o tipo de dado do parâmetro ROS 2
fn render_value_input(ui: &mut Ui, param: &mut Parameter) {
    match &mut param.edited_value {
        ParameterValue::Bool(ref mut val) => {
            ui.checkbox(val, "");
        }
        ParameterValue::Int(ref mut val) => {
            ui.add(egui::DragValue::new(val).speed(1));
        }
        ParameterValue::Float(ref mut val) => {
            ui.add(egui::DragValue::new(val).speed(0.01).max_decimals(4));
        }
        ParameterValue::String(ref mut val) => {
            ui.add(egui::TextEdit::singleline(val).desired_width(110.0));
        }
        ParameterValue::ByteArray(ref mut vec) => {
            let mut text = param.array_text_buf.clone().unwrap_or_else(|| {
                vec.iter()
                    .map(|b| b.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            });
            if ui
                .add(egui::TextEdit::singleline(&mut text).desired_width(110.0))
                .changed()
            {
                param.array_text_buf = Some(text.clone());
                let parsed: Vec<u8> = text
                    .split(',')
                    .filter_map(|s| s.trim().parse::<u8>().ok())
                    .collect();
                *vec = parsed;
            }
        }
        ParameterValue::BoolArray(ref mut vec) => {
            ui.horizontal(|ui| {
                for (idx, b) in vec.iter_mut().enumerate() {
                    ui.checkbox(b, format!("{}", idx));
                }
            });
        }
        ParameterValue::IntArray(ref mut vec) => {
            let mut text = param.array_text_buf.clone().unwrap_or_else(|| {
                vec.iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            });
            if ui
                .add(egui::TextEdit::singleline(&mut text).desired_width(110.0))
                .changed()
            {
                param.array_text_buf = Some(text.clone());
                let parsed: Vec<i64> = text
                    .split(',')
                    .filter_map(|s| s.trim().parse::<i64>().ok())
                    .collect();
                *vec = parsed;
            }
        }
        ParameterValue::FloatArray(ref mut vec) => {
            let mut text = param.array_text_buf.clone().unwrap_or_else(|| {
                vec.iter()
                    .map(|v| format!("{:.2}", v))
                    .collect::<Vec<_>>()
                    .join(", ")
            });
            if ui
                .add(egui::TextEdit::singleline(&mut text).desired_width(110.0))
                .changed()
            {
                param.array_text_buf = Some(text.clone());
                let parsed: Vec<f64> = text
                    .split(',')
                    .filter_map(|s| s.trim().parse::<f64>().ok())
                    .collect();
                *vec = parsed;
            }
        }
        ParameterValue::StringArray(ref mut vec) => {
            let mut text = param
                .array_text_buf
                .clone()
                .unwrap_or_else(|| vec.join(", "));
            if ui
                .add(egui::TextEdit::singleline(&mut text).desired_width(110.0))
                .changed()
            {
                param.array_text_buf = Some(text.clone());
                let parsed: Vec<String> = text
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                *vec = parsed;
            }
        }
    }
}

/// Cria dados mock realistas de pacotes e parâmetros do robô Perse
pub fn create_mock_parameters() -> Vec<PackageParams> {
    vec![
        PackageParams {
            name: "perse_navigation".into(),
            files: vec![ParamFile {
                filename: "nav2_params.yaml".into(),
                scopes: vec![
                    // Exemplo 1: nav -> params -> planner (caminho único -> simplifica para nav/params/planner)
                    Scope {
                        name: "nav".into(),
                        sub_scopes: vec![Scope {
                            name: "params".into(),
                            sub_scopes: vec![Scope {
                                name: "planner".into(),
                                sub_scopes: vec![],
                                parameters: vec![
                                    Parameter::new(
                                        "speed",
                                        "Velocidade linear máxima (m/s)",
                                        ParameterValue::Float(1.5),
                                    ),
                                    Parameter::new(
                                        "max_accel",
                                        "Aceleração máxima (m/s²)",
                                        ParameterValue::Float(0.8),
                                    ),
                                    Parameter::new(
                                        "enable_recovery",
                                        "Ativar comportamentos de recuperação",
                                        ParameterValue::Bool(true),
                                    ),
                                ],
                            }],
                            parameters: vec![],
                        }],
                        parameters: vec![],
                    },
                    // Exemplo 2: nav_multi -> (params -> planner) e config (ramificado -> nav_multi / params/planner / config)
                    Scope {
                        name: "nav_multi".into(),
                        sub_scopes: vec![
                            Scope {
                                name: "params".into(),
                                sub_scopes: vec![Scope {
                                    name: "planner".into(),
                                    sub_scopes: vec![],
                                    parameters: vec![Parameter::new(
                                        "planner_type",
                                        "Tipo de algoritmo planejador",
                                        ParameterValue::String("NavFn".into()),
                                    )],
                                }],
                                parameters: vec![],
                            },
                            Scope {
                                name: "config".into(),
                                sub_scopes: vec![],
                                parameters: vec![Parameter::new(
                                    "tolerance",
                                    "Tolerância de chegada ao objetivo (m)",
                                    ParameterValue::Float(0.05),
                                )],
                            },
                        ],
                        parameters: vec![],
                    },
                    Scope {
                        name: "amcl".into(),
                        sub_scopes: vec![],
                        parameters: vec![
                            Parameter::new(
                                "min_particles",
                                "Mínimo de partículas no filtro",
                                ParameterValue::Int(500),
                            ),
                            Parameter::new(
                                "max_particles",
                                "Máximo de partículas no filtro",
                                ParameterValue::Int(2000),
                            ),
                            Parameter::new(
                                "initial_pose",
                                "Posição inicial x, y, theta",
                                ParameterValue::FloatArray(vec![0.0, 0.0, 0.0]),
                            ),
                        ],
                    },
                ],
            }],
        },
        PackageParams {
            name: "perse_control".into(),
            files: vec![ParamFile {
                filename: "controllers.yaml".into(),
                scopes: vec![Scope {
                    name: "diff_drive_controller".into(),
                    sub_scopes: vec![],
                    parameters: vec![
                        Parameter::new(
                            "wheel_separation",
                            "Distância entre rodas (m)",
                            ParameterValue::Float(0.45),
                        ),
                        Parameter::new(
                            "wheel_radius",
                            "Raio das rodas (m)",
                            ParameterValue::Float(0.10),
                        ),
                        Parameter::new(
                            "joint_names",
                            "Nomes das juntas de tração",
                            ParameterValue::StringArray(vec![
                                "left_wheel_joint".into(),
                                "right_wheel_joint".into(),
                            ]),
                        ),
                        Parameter::new(
                            "can_device_ids",
                            "IDs dos controladores CAN",
                            ParameterValue::ByteArray(vec![0x01, 0x02, 0x03]),
                        ),
                        Parameter::new(
                            "encoder_counts",
                            "Pulsos de encoder por rotação [esq, dir]",
                            ParameterValue::IntArray(vec![4096, 4096]),
                        ),
                        Parameter::new(
                            "wheel_inverted",
                            "Inversão dos motores [esquerda, direita]",
                            ParameterValue::BoolArray(vec![false, true]),
                        ),
                    ],
                }],
            }],
        },
        PackageParams {
            name: "perse_sensors".into(),
            files: vec![ParamFile {
                filename: "lidar_params.yaml".into(),
                scopes: vec![Scope {
                    name: "rplidar_node".into(),
                    sub_scopes: vec![],
                    parameters: vec![
                        Parameter::new(
                            "serial_port",
                            "Porta serial do Lidar",
                            ParameterValue::String("/dev/ttyUSB0".into()),
                        ),
                        Parameter::new(
                            "baud_rate",
                            "Taxa de transmissão",
                            ParameterValue::Int(115200),
                        ),
                        Parameter::new(
                            "frame_id",
                            "Frame de referência",
                            ParameterValue::String("laser_frame".into()),
                        ),
                        Parameter::new(
                            "angle_compensate",
                            "Compensação de ângulo",
                            ParameterValue::Bool(true),
                        ),
                    ],
                }],
            }],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parameters_panel_apply_all() {
        let mut panel = ParametersPanel::default();
        assert_eq!(panel.count_modified(), 0);

        // Modify a parameter value
        let param =
            &mut panel.packages[0].files[0].scopes[0].sub_scopes[0].sub_scopes[0].parameters[0];
        param.edited_value = ParameterValue::Float(2.5);
        assert!(param.is_modified());
        assert_eq!(panel.count_modified(), 1);

        panel.apply_all();
        assert_eq!(panel.count_modified(), 0);
        let param_after =
            &panel.packages[0].files[0].scopes[0].sub_scopes[0].sub_scopes[0].parameters[0];
        assert_eq!(param_after.saved_value, ParameterValue::Float(2.5));
    }
}
