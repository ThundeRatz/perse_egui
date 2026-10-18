#[cfg(feature = "viewer")]
use egui::{Color32, Frame, Margin, RichText, Ui};
use serde::{Deserialize, Serialize};

#[cfg(feature = "viewer")]
use crate::panels::components::{re_icon_button, re_icon_toggle_button, section_header};
#[cfg(feature = "viewer")]
use crate::state::AppState;

/// Tipo de arquivo de launch ROS 2
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LaunchKind {
    Python,
    Xml,
}

impl LaunchKind {
    pub fn badge(&self) -> &'static str {
        match self {
            LaunchKind::Python => "🐍 py",
            LaunchKind::Xml => "📄 xml",
        }
    }
}

/// Status de execução do launch file
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LaunchStatus {
    Stopped,
    Running,
}

/// Argumento configurável para execução de um launchfile
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LaunchArgument {
    pub name: String,
    pub default_value: String,
    pub value: String,
    pub from_file: bool,
}

impl LaunchArgument {
    pub fn new_from_file(name: impl Into<String>, default_value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            default_value: default_value.into(),
            value: String::new(),
            from_file: true,
        }
    }

    pub fn new_custom(name: impl Into<String>, value: impl Into<String>) -> Self {
        let val = value.into();
        Self {
            name: name.into(),
            default_value: String::new(),
            value: val,
            from_file: false,
        }
    }

    pub fn is_modified(&self) -> bool {
        if self.from_file {
            !self.value.trim().is_empty()
        } else {
            !self.name.trim().is_empty() || !self.value.trim().is_empty()
        }
    }
}

/// Estrutura para um arquivo de launch (suporta hierarquia recursiva, ex: XML incluindo outros launchs)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchFile {
    pub name: String,
    pub kind: LaunchKind,
    pub status: LaunchStatus,
    pub description: String,
    pub children: Vec<LaunchFile>,
    #[serde(default)]
    pub args: Vec<LaunchArgument>,
    #[serde(default)]
    pub args_open: bool,
}

impl LaunchFile {
    pub fn new(name: impl Into<String>, kind: LaunchKind, desc: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind,
            status: LaunchStatus::Stopped,
            description: desc.into(),
            children: Vec::new(),
            args: Vec::new(),
            args_open: false,
        }
    }

    pub fn with_children(mut self, children: Vec<LaunchFile>) -> Self {
        self.children = children;
        self
    }

    pub fn with_arguments(mut self, args: Vec<LaunchArgument>) -> Self {
        self.args = args;
        self
    }

    pub fn matches_search(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        let q = query.to_lowercase();
        if self.name.to_lowercase().contains(&q) || self.description.to_lowercase().contains(&q) {
            return true;
        }
        self.children
            .iter()
            .any(|child| child.matches_search(query))
    }
}

/// Pacote que contém launch files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchPackage {
    pub name: String,
    pub launch_files: Vec<LaunchFile>,
}

impl LaunchPackage {
    pub fn matches_search(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        let q = query.to_lowercase();
        if self.name.to_lowercase().contains(&q) {
            return true;
        }
        self.launch_files.iter().any(|f| f.matches_search(query))
    }

    pub fn has_file_match(&self, query: &str) -> bool {
        if query.is_empty() {
            return false;
        }
        self.launch_files.iter().any(|f| f.matches_search(query))
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub mod process_manager;
pub mod workspace;

/// Pedido pendente para abrir um launchfile como terminal interativo
#[derive(Debug, Clone)]
pub struct PendingLaunchTerminal {
    pub package: String,
    pub filename: String,
    pub formatted_args: Vec<String>,
}

/// Constrói o comando de execução do launchfile com os devidos sources de ROS 2 e workspace
pub fn build_launch_command_string(
    mode: crate::config::LaunchTargetMode,
    entrypoint: &str,
    package: &str,
    filename: &str,
    launch_args: &[String],
) -> String {
    let entry_script = if !entrypoint.trim().is_empty() {
        format!(
            "if [ -f \"{0}\" ]; then source \"{0}\" 2>/dev/null || \"{0}\"; fi; ",
            entrypoint.trim()
        )
    } else {
        String::new()
    };

    let args_suffix = if launch_args.is_empty() {
        String::new()
    } else {
        format!(" {}", launch_args.join(" "))
    };

    match mode {
        crate::config::LaunchTargetMode::Host => {
            format!(
                "for s in /opt/ros/*/setup.bash install/setup.bash ~/perse_ws/install/setup.bash; do [ -f \"$s\" ] && source \"$s\" 2>/dev/null; done; {0}exec ros2 launch {1} {2}{3}",
                entry_script, package, filename, args_suffix
            )
        }
        crate::config::LaunchTargetMode::Docker => {
            format!(
                "for s in /perse_entrypoint.sh /opt/ros/*/setup.bash /home/*/*/install/setup.bash ~/perse_ws/install/setup.bash; do [ -f \"$s\" ] && source \"$s\" 2>/dev/null; done; {0}exec ros2 launch {1} {2}{3}",
                entry_script, package, filename, args_suffix
            )
        }
    }
}

/// Painel de Launchfiles
#[cfg(feature = "viewer")]
#[derive(Debug)]
pub struct LaunchfilesPanel {
    pub packages: Vec<LaunchPackage>,
    pub search_active: bool,
    pub search_query: String,
    pub first_frame: bool,
    pub request_open_settings: bool,
    pub pending_terminal_launch: Option<PendingLaunchTerminal>,
}

#[cfg(feature = "viewer")]
impl Default for LaunchfilesPanel {
    fn default() -> Self {
        Self {
            packages: Vec::new(),
            search_active: false,
            search_query: String::new(),
            first_frame: true,
            request_open_settings: false,
            pending_terminal_launch: None,
        }
    }
}

#[cfg(feature = "viewer")]
impl LaunchfilesPanel {
    pub fn update_from_remote(&mut self, remote_packages: Vec<LaunchPackage>) {
        if self.packages.is_empty() {
            self.packages = remote_packages;
            return;
        }

        for remote_pkg in remote_packages {
            if let Some(local_pkg) = self.packages.iter_mut().find(|p| p.name == remote_pkg.name) {
                for remote_file in remote_pkg.launch_files {
                    merge_launch_file(&mut local_pkg.launch_files, remote_file);
                }
            } else {
                self.packages.push(remote_pkg);
            }
        }
    }

    pub fn update_file_status(&mut self, pkg_name: &str, filename: &str, running: bool) {
        let new_status = if running {
            LaunchStatus::Running
        } else {
            LaunchStatus::Stopped
        };

        for pkg in &mut self.packages {
            if pkg.name == pkg_name {
                update_status_recursive(&mut pkg.launch_files, filename, new_status);
            }
        }
    }

    pub fn ui(
        &mut self,
        ui: &mut Ui,
        state: &mut AppState,
        client: Option<&crate::net::client::ControlClient>,
        mut config: Option<&mut crate::config::AppConfig>,
    ) {
        let is_first_frame = self.first_frame;
        self.first_frame = false;

        section_header(
            ui,
            "Launchfiles",
            &mut self.search_active,
            &mut self.search_query,
            |_ui| {},
        );

        Frame::new()
            .inner_margin(Margin {
                left: 6,
                right: 6,
                top: 4,
                bottom: 4,
            })
            .show(ui, |ui| {
                egui::ScrollArea::both()
                    .id_salt("launchfiles_scroll")
                    .min_scrolled_height(0.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.packages.is_empty() {
                            ui.add_space(16.0);
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    RichText::new("Nenhum launch file encontrado.")
                                        .strong()
                                        .color(Color32::from_gray(210)),
                                );
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new("Configure o caminho da workspace ROS nas configurações para carregar os launch files dos pacotes.")
                                        .color(Color32::from_gray(160)),
                                );
                                ui.add_space(12.0);
                                if crate::panels::components::re_icon_text_button(
                                    ui,
                                    &re_ui::icons::SETTINGS,
                                    "Abrir Configurações",
                                )
                                .on_hover_text("Abrir janela de Configurações para definir o caminho da workspace")
                                .clicked()
                                {
                                    self.request_open_settings = true;
                                }
                            });
                            return;
                        }

                        let query = self.search_query.trim().to_lowercase();

                        let mut visible_packages: Vec<&mut LaunchPackage> = self
                            .packages
                            .iter_mut()
                            .filter(|pkg| pkg.matches_search(&query))
                            .collect();

                        if visible_packages.is_empty() {
                            ui.add_space(8.0);
                            ui.label(
                                RichText::new("Nenhum launch file corresponde à busca")
                                    .italics()
                                    .color(Color32::GRAY),
                            );
                            return;
                        }

                        // Itens marcados com estrela no topo
                        if let Some(ref cfg) = config {
                            visible_packages.sort_by(|a, b| {
                                let a_has_star = a
                                    .launch_files
                                    .iter()
                                    .any(|f| cfg.is_launch_file_starred(&a.name, &f.name));
                                let b_has_star = b
                                    .launch_files
                                    .iter()
                                    .any(|f| cfg.is_launch_file_starred(&b.name, &f.name));
                                b_has_star.cmp(&a_has_star)
                            });
                        }

                        for pkg in visible_packages {
                            render_launch_package(
                                ui,
                                pkg,
                                &query,
                                state,
                                is_first_frame,
                                client,
                                config.as_deref_mut(),
                                &mut self.pending_terminal_launch,
                            );
                        }
                    });
            });
    }
}

#[cfg(feature = "viewer")]
fn render_launch_package(
    ui: &mut Ui,
    pkg: &mut LaunchPackage,
    query: &str,
    state: &mut AppState,
    is_first_frame: bool,
    client: Option<&crate::net::client::ControlClient>,
    mut config: Option<&mut crate::config::AppConfig>,
    pending_launch: &mut Option<PendingLaunchTerminal>,
) {
    let running_count = pkg
        .launch_files
        .iter()
        .filter(|f| f.status == LaunchStatus::Running)
        .count();

    let title = if running_count > 0 {
        format!("📦 {} (● {})", pkg.name, running_count)
    } else {
        format!("📦 {}", pkg.name)
    };

    let file_match = pkg.has_file_match(query);
    let pkg_self_match = !query.is_empty() && pkg.name.to_lowercase().contains(query);
    let force_show_all = pkg_self_match && !file_match;

    let open_override = if is_first_frame { Some(true) } else { None };

    let pkg_name = pkg.name.clone();

    // Ordena os launch files colocando os marcados com estrela no topo
    if let Some(ref cfg) = config {
        pkg.launch_files.sort_by(|a, b| {
            let a_star = cfg.is_launch_file_starred(&pkg.name, &a.name);
            let b_star = cfg.is_launch_file_starred(&pkg.name, &b.name);
            b_star.cmp(&a_star)
        });
    }

    egui::CollapsingHeader::new(RichText::new(title).strong().size(13.0))
        .id_salt(format!("launch_pkg_collapsing_{}", pkg.name))
        .default_open(true)
        .open(open_override)
        .show(ui, |ui| {
            for file in &mut pkg.launch_files {
                if force_show_all || file.matches_search(query) {
                    render_launch_file(
                        ui,
                        &pkg_name,
                        file,
                        query,
                        state,
                        force_show_all,
                        is_first_frame,
                        client,
                        config.as_deref_mut(),
                        0,
                        pending_launch,
                    );
                }
            }
        });
}

#[cfg(feature = "viewer")]
fn render_launch_file(
    ui: &mut Ui,
    pkg_name: &str,
    file: &mut LaunchFile,
    query: &str,
    state: &mut AppState,
    parent_force_all: bool,
    is_first_frame: bool,
    client: Option<&crate::net::client::ControlClient>,
    mut config: Option<&mut crate::config::AppConfig>,
    nesting_level: usize,
    pending_launch: &mut Option<PendingLaunchTerminal>,
) {
    let is_running = file.status == LaunchStatus::Running;
    let has_children = !file.children.is_empty();

    Frame::new()
        .fill(if is_running {
            Color32::from_rgb(20, 50, 30)
        } else {
            Color32::from_gray(24)
        })
        .stroke(if is_running {
            egui::Stroke::new(1.0, Color32::from_rgb(40, 180, 80))
        } else {
            egui::Stroke::NONE
        })
        .inner_margin(Margin {
            left: 6,
            right: 6,
            top: 4,
            bottom: 4,
        })
        .corner_radius(4.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // Botão de Estrela (Favorito) - Apenas para launchfiles de nível superior
                if nesting_level == 0 {
                    let is_starred = config
                        .as_ref()
                        .map(|c| c.is_launch_file_starred(pkg_name, &file.name))
                        .unwrap_or(false);
                    let star_text = if is_starred { "★" } else { "☆" };
                    let star_color = if is_starred {
                        Color32::from_rgb(255, 205, 50)
                    } else {
                        Color32::from_gray(120)
                    };
                    if ui
                        .add(
                            egui::Button::new(RichText::new(star_text).size(13.0).color(star_color))
                                .frame(false),
                        )
                        .on_hover_text(if is_starred {
                            "Remover dos favoritos"
                        } else {
                            "Fixar no topo (Favorito)"
                        })
                        .clicked()
                    {
                        if let Some(ref mut cfg) = config {
                            cfg.toggle_launch_file_starred(pkg_name, &file.name);
                        }
                    }
                }

                // Indicador visual de status
                if is_running {
                    ui.label(RichText::new("●").color(Color32::from_rgb(50, 220, 100)));
                } else {
                    ui.label(RichText::new("○").color(Color32::GRAY));
                }

                // Nome do launch file
                ui.label(
                    RichText::new(&file.name)
                        .strong()
                        .size(12.0)
                        .color(if is_running {
                            Color32::WHITE
                        } else {
                            Color32::from_gray(220)
                        }),
                );

                // Botão de argumentos (substitui o badge de tipo python/xml)
                let arg_count = file.args.len();
                let btn_text = if arg_count > 0 {
                    format!("args ({})", arg_count)
                } else {
                    "args".to_string()
                };

                if re_icon_toggle_button(
                    ui,
                    &re_ui::icons::SETTINGS,
                    &btn_text,
                    file.args_open,
                )
                .on_hover_text("Configurar argumentos de execução")
                .clicked()
                {
                    file.args_open = !file.args_open;
                }

                // Botões de Ação de Play/Stop usando os ícones do re_ui::icons
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if is_running {
                        // Botão de Parar (Stop) usando ícone do re_ui
                        if re_icon_button(ui, &re_ui::icons::PAUSE, "Parar execução").clicked() {
                            file.status = LaunchStatus::Stopped;
                            state.increment_action(format!("Parou {}", file.name));
                            if let Some(c) = client {
                                c.send(crate::net::protocol::ControlMessage::new(
                                    crate::net::protocol::Domain::Launchfiles,
                                    "stop",
                                    serde_json::to_value(
                                        crate::net::protocol::LaunchfileActionRequest {
                                            package: pkg_name.to_string(),
                                            filename: file.name.clone(),
                                            action: "stop".to_string(),
                                            target_mode: config.as_ref().map(|c| c.launch_target_mode),
                                            container_name: config.as_ref().map(|c| c.docker_container_name.clone()),
                                            entrypoint: config.as_ref().and_then(|c| {
                                                if c.docker_entrypoint.trim().is_empty() {
                                                    None
                                                } else {
                                                    Some(c.docker_entrypoint.clone())
                                                }
                                            }),
                                            args: None,
                                        },
                                    )
                                    .unwrap_or_default(),
                                ));
                            }
                        }
                    } else {
                        // Botão de Iniciar (Play) usando ícone do re_ui: abre como terminal interativo
                        if re_icon_button(ui, &re_ui::icons::PLAY, "Iniciar launch file no terminal").clicked()
                        {
                            file.status = LaunchStatus::Running;
                            state.increment_action(format!("Iniciou {}", file.name));

                            let formatted_args: Vec<String> = file
                                .args
                                .iter()
                                .filter(|a| {
                                    if a.from_file {
                                        !a.value.trim().is_empty()
                                    } else {
                                        !a.name.trim().is_empty()
                                    }
                                })
                                .map(|a| format!("{}:={}", a.name.trim(), a.value.trim()))
                                .collect();

                            *pending_launch = Some(PendingLaunchTerminal {
                                package: pkg_name.to_string(),
                                filename: file.name.clone(),
                                formatted_args,
                            });
                        }
                    }
                });
            });

            if !file.description.is_empty() {
                ui.label(
                    RichText::new(&file.description)
                        .size(10.0)
                        .italics()
                        .color(Color32::from_gray(140)),
                );
            }

            // Painel / collapsible de argumentos de execução
            if file.args_open {
                ui.add_space(4.0);
                Frame::new()
                    .fill(Color32::from_gray(18))
                    .stroke(egui::Stroke::new(1.0, Color32::from_gray(40)))
                    .inner_margin(Margin {
                        left: 8,
                        right: 8,
                        top: 6,
                        bottom: 6,
                    })
                    .corner_radius(4.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("Argumentos de execução")
                                    .strong()
                                    .size(11.0)
                                    .color(Color32::from_gray(200)),
                            );

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if re_icon_button(ui, &re_ui::icons::ADD, "Adicionar argumento customizado").clicked() {
                                    file.args.push(LaunchArgument::new_custom("", ""));
                                }
                            });
                        });

                        ui.add_space(2.0);

                        if file.args.is_empty() {
                            ui.label(
                                RichText::new("Nenhum argumento definido. Clique em '+' para adicionar.")
                                    .size(10.0)
                                    .italics()
                                    .color(Color32::from_gray(130)),
                            );
                        } else {
                            let mut to_remove = None;
                            for (idx, arg) in file.args.iter_mut().enumerate() {
                                ui.horizontal(|ui| {
                                    let font_id = egui::TextStyle::Body.resolve(ui.style());

                                    if arg.from_file {
                                        // Argumento definido no arquivo: não pode ser removido, apenas alterado
                                        ui.label(
                                            RichText::new(&arg.name)
                                                .strong()
                                                .size(11.0)
                                                .color(Color32::from_gray(210)),
                                        ).on_hover_text("Argumento declarado no arquivo (não pode ser removido)");

                                        ui.label(
                                            RichText::new(":=")
                                                .size(11.0)
                                                .color(Color32::from_gray(130)),
                                        );

                                        let display_str = if arg.value.is_empty() {
                                            &arg.default_value
                                        } else {
                                            &arg.value
                                        };
                                        let text_width = ui.painter().layout_no_wrap(display_str.to_string(), font_id.clone(), Color32::WHITE).size().x;
                                        let min_width = 160.0;
                                        let desired_width = (text_width + 24.0).max(min_width);

                                        ui.add(
                                            egui::TextEdit::singleline(&mut arg.value)
                                                .hint_text(&arg.default_value)
                                                .desired_width(desired_width),
                                        );

                                        if !arg.value.trim().is_empty() {
                                            if re_icon_button(
                                                ui,
                                                &re_ui::icons::RESET,
                                                &format!("Restaurar padrão ({})", arg.default_value),
                                            )
                                            .clicked()
                                            {
                                                arg.value.clear();
                                            }
                                        }
                                    } else {
                                        // Argumento customizado: nome e valor editáveis + botão de excluir
                                        let name_str = if arg.name.is_empty() { "nome" } else { &arg.name };
                                        let name_text_width = ui.painter().layout_no_wrap(name_str.to_string(), font_id.clone(), Color32::WHITE).size().x;
                                        let name_width = (name_text_width + 20.0).max(100.0);

                                        ui.add(
                                            egui::TextEdit::singleline(&mut arg.name)
                                                .hint_text("nome")
                                                .desired_width(name_width),
                                        );

                                        ui.label(
                                            RichText::new(":=")
                                                .size(11.0)
                                                .color(Color32::from_gray(130)),
                                        );

                                        let val_str = if arg.value.is_empty() { "valor" } else { &arg.value };
                                        let val_text_width = ui.painter().layout_no_wrap(val_str.to_string(), font_id.clone(), Color32::WHITE).size().x;
                                        let val_width = (val_text_width + 24.0).max(160.0);

                                        ui.add(
                                            egui::TextEdit::singleline(&mut arg.value)
                                                .hint_text("valor")
                                                .desired_width(val_width),
                                        );

                                        if re_icon_button(
                                            ui,
                                            &re_ui::icons::TRASH,
                                            "Remover argumento customizado",
                                        )
                                        .clicked()
                                        {
                                            to_remove = Some(idx);
                                        }
                                    }
                                });
                            }

                            if let Some(idx) = to_remove {
                                file.args.remove(idx);
                            }
                        }
                    });
            }

            // Se for um arquivo de launch XML que inclui sub-launchfiles, renderiza os filhos
            if has_children {
                ui.add_space(2.0);
                let is_first_sub_level = nesting_level == 0;
                let open_override = if is_first_frame {
                    Some(is_first_sub_level)
                } else {
                    None
                };

                egui::CollapsingHeader::new(
                    RichText::new("↳ Sub-launchfiles incluídos")
                        .size(11.0)
                        .color(Color32::from_gray(170)),
                )
                .id_salt(format!("sub_launch_collapsing_{}", file.name))
                .default_open(is_first_sub_level)
                .open(open_override)
                .show(ui, |ui| {
                    for child in &mut file.children {
                        if parent_force_all || child.matches_search(query) {
                            render_launch_file(
                                ui,
                                pkg_name,
                                child,
                                query,
                                state,
                                parent_force_all,
                                is_first_frame,
                                client,
                                config.as_deref_mut(),
                                nesting_level + 1,
                                pending_launch,
                            );
                        }
                    }
                });
            }
        });

    ui.add_space(4.0);
}

#[cfg(feature = "viewer")]
fn update_status_recursive(
    files: &mut [LaunchFile],
    filename: &str,
    new_status: LaunchStatus,
) -> bool {
    for f in files {
        if f.name == filename {
            f.status = new_status;
            return true;
        }
        if update_status_recursive(&mut f.children, filename, new_status) {
            return true;
        }
    }
    false
}

#[cfg(feature = "viewer")]
fn merge_launch_file(local_files: &mut Vec<LaunchFile>, remote_file: LaunchFile) {
    if let Some(local_file) = local_files.iter_mut().find(|f| f.name == remote_file.name) {
        local_file.status = remote_file.status;
        local_file.description = remote_file.description;
        local_file.kind = remote_file.kind;

        // Atualiza argumentos sem sobrescrever modificações locais do usuário
        for remote_arg in remote_file.args {
            if let Some(local_arg) = local_file
                .args
                .iter_mut()
                .find(|a| a.name == remote_arg.name)
            {
                local_arg.default_value = remote_arg.default_value;
                local_arg.from_file = remote_arg.from_file;
                // Mantém local_arg.value! Se o usuário digitou algo, permanece salvo.
            } else {
                local_file.args.push(remote_arg);
            }
        }

        for remote_child in remote_file.children {
            merge_launch_file(&mut local_file.children, remote_child);
        }
    } else {
        local_files.push(remote_file);
    }
}

pub fn create_mock_launchfiles() -> Vec<LaunchPackage> {
    vec![
        LaunchPackage {
            name: "perse_bringup".into(),
            launch_files: vec![
                LaunchFile::new(
                    "robot_bringup.launch.py",
                    LaunchKind::Python,
                    "Inicia todos os drivers de sensores e controladores hardware",
                )
                .with_arguments(vec![LaunchArgument::new_custom("use_sim_time", "false")]),
                LaunchFile::new(
                    "master_system.launch.xml",
                    LaunchKind::Xml,
                    "Master XML launch file que agrupa subsistemas",
                )
                .with_arguments(vec![
                    LaunchArgument::new_from_file("sim", "false"),
                    LaunchArgument::new_from_file("use_teleop", "true"),
                ])
                .with_children(vec![
                    LaunchFile::new(
                        "sensors.launch.py",
                        LaunchKind::Python,
                        "Drivers de Lidar e Câmera",
                    ),
                    LaunchFile::new(
                        "teleop.launch.py",
                        LaunchKind::Python,
                        "Interface de controle remoto via joystick",
                    ),
                ]),
            ],
        },
        LaunchPackage {
            name: "perse_navigation".into(),
            launch_files: vec![
                LaunchFile::new(
                    "navigation.launch.py",
                    LaunchKind::Python,
                    "Inicia Nav2 planner, controller e AMCL localization",
                ),
                LaunchFile::new(
                    "slam.launch.py",
                    LaunchKind::Python,
                    "Mapeamento SLAM online com cartographer",
                ),
            ],
        },
        LaunchPackage {
            name: "perse_vision".into(),
            launch_files: vec![
                LaunchFile::new(
                    "camera_driver.launch.py",
                    LaunchKind::Python,
                    "Driver de vídeo e publicação de tópicos de imagem",
                ),
                LaunchFile::new(
                    "object_detection.launch.py",
                    LaunchKind::Python,
                    "Rede neural YOLO para detecção de obstáculos em tempo real",
                ),
            ],
        },
    ]
}
