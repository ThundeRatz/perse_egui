use egui::{Color32, Frame, Margin, RichText, Ui};
use serde::{Deserialize, Serialize};

use crate::panels::components::{re_icon_button, section_header};
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

/// Estrutura para um arquivo de launch (suporta hierarquia recursiva, ex: XML incluindo outros launchs)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchFile {
    pub name: String,
    pub kind: LaunchKind,
    pub status: LaunchStatus,
    pub description: String,
    pub children: Vec<LaunchFile>,
}

impl LaunchFile {
    pub fn new(name: impl Into<String>, kind: LaunchKind, desc: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind,
            status: LaunchStatus::Stopped,
            description: desc.into(),
            children: Vec::new(),
        }
    }

    pub fn with_children(mut self, children: Vec<LaunchFile>) -> Self {
        self.children = children;
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

/// Painel de Launchfiles
#[derive(Debug)]
pub struct LaunchfilesPanel {
    pub packages: Vec<LaunchPackage>,
    pub search_active: bool,
    pub search_query: String,
    pub first_frame: bool,
}

impl Default for LaunchfilesPanel {
    fn default() -> Self {
        Self {
            packages: create_mock_launchfiles(),
            search_active: false,
            search_query: String::new(),
            first_frame: true,
        }
    }
}

impl LaunchfilesPanel {
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        state: &mut AppState,
        client: Option<&crate::net::client::ControlClient>,
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
                        let query = self.search_query.trim().to_lowercase();

                        let visible_packages: Vec<&mut LaunchPackage> = self
                            .packages
                            .iter_mut()
                            .filter(|pkg| pkg.matches_search(&query))
                            .collect();

                        if visible_packages.is_empty() {
                            ui.add_space(8.0);
                            ui.label(
                                RichText::new("Nenhum launch file encontrado")
                                    .italics()
                                    .color(Color32::GRAY),
                            );
                            return;
                        }

                        for pkg in visible_packages {
                            render_launch_package(ui, pkg, &query, state, is_first_frame, client);
                        }
                    });
            });
    }
}

fn render_launch_package(
    ui: &mut Ui,
    pkg: &mut LaunchPackage,
    query: &str,
    state: &mut AppState,
    is_first_frame: bool,
    client: Option<&crate::net::client::ControlClient>,
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
                    );
                }
            }
        });
}

fn render_launch_file(
    ui: &mut Ui,
    pkg_name: &str,
    file: &mut LaunchFile,
    query: &str,
    state: &mut AppState,
    parent_force_all: bool,
    is_first_frame: bool,
    client: Option<&crate::net::client::ControlClient>,
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

                // Badge do tipo (python / xml)
                ui.label(
                    RichText::new(file.kind.badge())
                        .size(10.0)
                        .color(Color32::from_gray(140)),
                );

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
                                        },
                                    )
                                    .unwrap_or_default(),
                                ));
                            }
                        }
                    } else {
                        // Botão de Iniciar (Play) usando ícone do re_ui
                        if re_icon_button(ui, &re_ui::icons::PLAY, "Iniciar launch file").clicked()
                        {
                            file.status = LaunchStatus::Running;
                            state.increment_action(format!("Iniciou {}", file.name));
                            if let Some(c) = client {
                                c.send(crate::net::protocol::ControlMessage::new(
                                    crate::net::protocol::Domain::Launchfiles,
                                    "start",
                                    serde_json::to_value(
                                        crate::net::protocol::LaunchfileActionRequest {
                                            package: pkg_name.to_string(),
                                            filename: file.name.clone(),
                                            action: "start".to_string(),
                                        },
                                    )
                                    .unwrap_or_default(),
                                ));
                            }
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

            // Se for um arquivo de launch XML que inclui sub-launchfiles, renderiza os filhos
            if has_children {
                ui.add_space(2.0);
                let open_override = if is_first_frame { Some(true) } else { None };
                egui::CollapsingHeader::new(
                    RichText::new("↳ Sub-launchfiles incluídos")
                        .size(11.0)
                        .color(Color32::from_gray(170)),
                )
                .id_salt(format!("sub_launch_collapsing_{}", file.name))
                .default_open(true)
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
                            );
                        }
                    }
                });
            }
        });

    ui.add_space(4.0);
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
                ),
                LaunchFile::new(
                    "master_system.launch.xml",
                    LaunchKind::Xml,
                    "Master XML launch file que agrupa subsistemas",
                )
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
