use egui::{Color32, Frame, Margin, RichText, Ui};

use crate::panels::components::{re_icon_button, section_header};

/// Uma aba / processo de terminal
#[derive(Debug, Clone)]
pub struct TerminalTab {
    pub id: usize,
    pub title: String,
    pub is_running: bool,
    pub auto_scroll: bool,
    pub output_lines: Vec<String>,
}

impl TerminalTab {
    pub fn new(id: usize, title: impl Into<String>, lines: Vec<&str>) -> Self {
        Self {
            id,
            title: title.into(),
            is_running: true,
            auto_scroll: true,
            output_lines: lines.into_iter().map(|s| s.to_string()).collect(),
        }
    }
}

/// Painel de Terminais (posicionado na Sidebar Direita Resizable)
#[derive(Debug)]
pub struct TerminalsPanel {
    pub tabs: Vec<TerminalTab>,
    pub active_tab_index: usize,
    pub search_active: bool,
    pub search_query: String,
    pub command_input: String,
}

impl Default for TerminalsPanel {
    fn default() -> Self {
        Self {
            tabs: create_mock_terminals(),
            active_tab_index: 0,
            search_active: false,
            search_query: String::new(),
            command_input: String::new(),
        }
    }
}

impl TerminalsPanel {
    pub fn ui(&mut self, ui: &mut Ui) {
        // Cabeçalho com modo de busca inline
        section_header(
            ui,
            "Terminais",
            &mut self.search_active,
            &mut self.search_query,
            |ui| {
                if re_icon_button(ui, &re_ui::icons::ADD, "Novo Terminal").clicked() {
                    let next_id = self.tabs.len() + 1;
                    self.tabs.push(TerminalTab::new(
                        next_id,
                        format!("terminal_{}", next_id),
                        vec!["[INFO] [bash]: Terminal interativo iniciado."],
                    ));
                    self.active_tab_index = self.tabs.len() - 1;
                }
            },
        );

        if self.tabs.is_empty() {
            ui.label("Nenhum terminal ativo.");
            return;
        }

        // Barra de abas dos terminais (flex-wrap tiling ocupando a largura do painel)
        Frame::new()
            .fill(Color32::from_gray(22))
            .inner_margin(Margin {
                left: 6,
                right: 6,
                top: 4,
                bottom: 4,
            })
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                    let mut tab_to_close = None;
                    let can_close = self.tabs.len() > 1;

                    for (idx, tab) in self.tabs.iter().enumerate() {
                        let is_active = idx == self.active_tab_index;
                        let (clicked_tab, clicked_close) =
                            render_tab_pill(ui, &tab.title, tab.is_running, is_active, can_close);

                        if clicked_tab {
                            self.active_tab_index = idx;
                        }

                        if clicked_close {
                            tab_to_close = Some(idx);
                        }
                    }

                    if let Some(close_idx) = tab_to_close {
                        self.tabs.remove(close_idx);
                        if self.active_tab_index >= self.tabs.len() {
                            self.active_tab_index = self.tabs.len().saturating_sub(1);
                        }
                    }
                });
            });

        let query = self.search_query.trim().to_lowercase();
        let tab_idx = self.active_tab_index.min(self.tabs.len().saturating_sub(1));
        let tab = &mut self.tabs[tab_idx];

        // Barra de ferramentas do terminal ativo (Play/Stop, Clear, Auto-scroll)
        Frame::new()
            .fill(Color32::from_gray(28))
            .inner_margin(Margin {
                left: 6,
                right: 6,
                top: 2,
                bottom: 2,
            })
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if tab.is_running {
                        if re_icon_button(ui, &re_ui::icons::PAUSE, "Interromper processo (SIGINT)").clicked() {
                            tab.is_running = false;
                            tab.output_lines.push("[WARN] [sys]: Processo interrompido pelo usuário.".into());
                        }
                    } else {
                        if re_icon_button(ui, &re_ui::icons::PLAY, "Reiniciar processo").clicked() {
                            tab.is_running = true;
                            tab.output_lines.push("[INFO] [sys]: Processo reiniciado.".into());
                        }
                    }

                    ui.separator();

                    if crate::panels::components::re_icon_text_button(
                        ui,
                        &re_ui::icons::TRASH,
                        "Limpar",
                    )
                    .clicked()
                    {
                        tab.output_lines.clear();
                    }

                    ui.checkbox(&mut tab.auto_scroll, "Auto-scroll");
                });
            });

        // Área de exibição de log/saída do terminal (Monospaced output)
        Frame::new()
            .fill(Color32::from_gray(16))
            .inner_margin(Margin {
                left: 6,
                right: 6,
                top: 4,
                bottom: 4,
            })
            .show(ui, |ui| {
                let scroll = egui::ScrollArea::both()
                    .id_salt(format!("terminal_scroll_{}", tab.id))
                    .auto_shrink([false, false])
                    .min_scrolled_height(0.0)
                    .stick_to_bottom(tab.auto_scroll);

                scroll.show(ui, |ui| {
                    let lines: Vec<&String> = tab
                        .output_lines
                        .iter()
                        .filter(|line| query.is_empty() || line.to_lowercase().contains(&query))
                        .collect();

                    if lines.is_empty() {
                        ui.label(
                            RichText::new("Nenhuma saída para exibir...")
                                .italics()
                                .color(Color32::DARK_GRAY),
                        );
                    } else {
                        for line in lines {
                            render_log_line(ui, line);
                        }
                    }
                });
            });

        // Linha de entrada de comando (Prompt)
        Frame::new()
            .fill(Color32::from_gray(24))
            .inner_margin(Margin {
                left: 6,
                right: 6,
                top: 3,
                bottom: 3,
            })
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("$").strong().color(Color32::from_rgb(50, 200, 100)));

                    let available_for_text =
                        (ui.available_width() - 60.0 - ui.spacing().item_spacing.x).max(30.0);

                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.command_input)
                            .hint_text("Digite um comando ros2...")
                            .desired_width(available_for_text),
                    );

                    if (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                        || ui.small_button("Enviar").clicked()
                    {
                        if !self.command_input.trim().is_empty() {
                            let cmd = self.command_input.clone();
                            tab.output_lines.push(format!("$ {}", cmd));
                            tab.output_lines.push(format!("[INFO] [exec]: Executando '{}'...", cmd));
                            self.command_input.clear();
                        }
                    }
                });
            });
    }
}

/// Formata linhas de log de terminal com destaque por tipo (INFO, WARN, ERROR)
fn render_log_line(ui: &mut Ui, line: &str) {
    let color = if line.contains("[ERROR]") || line.contains("FATAL") || line.contains("Error") {
        Color32::from_rgb(240, 80, 80)
    } else if line.contains("[WARN]") || line.contains("Warning") {
        Color32::from_rgb(240, 180, 50)
    } else if line.contains("[INFO]") {
        Color32::from_rgb(140, 200, 250)
    } else if line.starts_with('$') {
        Color32::from_rgb(100, 230, 140)
    } else {
        Color32::from_gray(210)
    };

    ui.label(RichText::new(line).monospace().size(11.0).color(color));
}

fn create_mock_terminals() -> Vec<TerminalTab> {
    vec![
        TerminalTab::new(
            1,
            "robot_bringup",
            vec![
                "[INFO] [launch]: Process set executed: 'robot_state_publisher'",
                "[INFO] [launch]: Process set executed: 'rplidar_composition'",
                "[INFO] [rplidar_composition]: RPLidar S2 connected on /dev/ttyUSB0 (115200 baud)",
                "[INFO] [diff_drive_controller]: Hardware interface initialized successfully.",
                "[INFO] [diff_drive_controller]: Subscriber created on /cmd_vel",
                "[INFO] [robot_bringup]: All 4 nodes initialized and running in lifecycle state ACTIVE.",
            ],
        ),
        TerminalTab::new(
            2,
            "navigation2",
            vec![
                "[INFO] [amcl]: Initializing particle filter with 1000 particles at pose (0, 0, 0).",
                "[INFO] [planner_server]: GridBased global planner created successfully.",
                "[INFO] [controller_server]: DWB local planner active.",
                "[INFO] [bt_navigator]: Behavior Tree loaded from /opt/ros/humble/share/nav2_bt_navigator/trees/navigate_to_pose.xml",
                "[WARN] [amcl]: Initial pose particle cloud dispersion high (std_dev = 0.45m).",
            ],
        ),
        TerminalTab::new(
            3,
            "rosout",
            vec![
                "[INFO] [1727376000.123] [perse_core]: Node perse_core started.",
                "[INFO] [1727376001.456] [perse_teleop]: Joystick device connected: Sony DualSense",
                "[INFO] [1727376002.789] [perse_vision]: Camera node initialized: RGB-D 1080p @ 30fps",
            ],
        ),
    ]
}

fn render_tab_pill(
    ui: &mut Ui,
    title: &str,
    is_running: bool,
    is_active: bool,
    can_close: bool,
) -> (bool, bool) {
    let dot = if is_running { "●" } else { "○" };
    let text = if can_close && is_active {
        format!("{} {}  ✕", dot, title)
    } else {
        format!("{} {}", dot, title)
    };

    let bg = if is_active {
        Color32::from_gray(50)
    } else {
        Color32::from_gray(30)
    };

    let stroke = if is_active {
        egui::Stroke::new(1.0, Color32::from_gray(90))
    } else {
        egui::Stroke::NONE
    };

    let text_color = if is_active {
        Color32::WHITE
    } else {
        Color32::from_gray(170)
    };

    let btn = egui::Button::new(
        RichText::new(text)
            .strong()
            .size(11.5)
            .color(text_color),
    )
    .fill(bg)
    .stroke(stroke)
    .corner_radius(4.0);

    let response = ui.add(btn);
    let mut clicked_tab = false;
    let mut clicked_close = false;

    if response.clicked() {
        if can_close && is_active {
            if let Some(pos) = response.interact_pointer_pos() {
                if pos.x > response.rect.max.x - 20.0 {
                    clicked_close = true;
                } else {
                    clicked_tab = true;
                }
            } else {
                clicked_tab = true;
            }
        } else {
            clicked_tab = true;
        }
    }

    (clicked_tab, clicked_close)
}
