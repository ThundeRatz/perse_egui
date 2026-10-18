#[cfg(feature = "viewer")]
use egui::{Color32, Frame, Margin, RichText, Ui};

pub mod key_mapper;
#[cfg(not(target_arch = "wasm32"))]
pub mod pty_manager;
pub mod session;
pub mod widget;

#[cfg(feature = "viewer")]
use crate::config::{AppConfig, LaunchTargetMode};
#[cfg(feature = "viewer")]
use crate::net::client::ControlClient;
#[cfg(feature = "viewer")]
use crate::net::protocol::{
    ControlMessage, Domain, TerminalCloseRequest, TerminalCreateRequest, TerminalInputMessage,
    TerminalListSyncResponse, TerminalOutputMessage, TerminalResizeMessage, TerminalStatusMessage,
    TerminalTarget,
};
#[cfg(feature = "viewer")]
use crate::panels::components::{re_icon_button, section_header};
#[cfg(feature = "viewer")]
use session::TerminalSession;
#[cfg(feature = "viewer")]
use widget::show_terminal_widget;

/// Painel de Terminais (posicionado na Sidebar Direita Resizable)
#[cfg(feature = "viewer")]
pub struct TerminalsPanel {
    pub sessions: Vec<TerminalSession>,
    pub active_tab_index: usize,
    pub search_active: bool,
    pub search_query: String,
    pub selected_target: TerminalTarget,
    next_counter: usize,
    initialized_target_from_config: bool,
    #[cfg(not(target_arch = "wasm32"))]
    local_pty_manager: pty_manager::PtyManager,
    #[cfg(not(target_arch = "wasm32"))]
    local_events_rx: std::sync::mpsc::Receiver<LocalPtyEvent>,
    #[cfg(not(target_arch = "wasm32"))]
    local_events_tx: std::sync::mpsc::Sender<LocalPtyEvent>,
}

#[cfg(all(feature = "viewer", not(target_arch = "wasm32")))]
enum LocalPtyEvent {
    Output(String, Vec<u8>),
    Exit(String),
}

#[cfg(feature = "viewer")]
impl Default for TerminalsPanel {
    fn default() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        let (tx, rx) = std::sync::mpsc::channel();

        Self {
            sessions: Vec::new(),
            active_tab_index: 0,
            search_active: false,
            search_query: String::new(),
            selected_target: TerminalTarget::Host,
            next_counter: 1,
            initialized_target_from_config: false,
            #[cfg(not(target_arch = "wasm32"))]
            local_pty_manager: pty_manager::PtyManager::new(),
            #[cfg(not(target_arch = "wasm32"))]
            local_events_rx: rx,
            #[cfg(not(target_arch = "wasm32"))]
            local_events_tx: tx,
        }
    }
}

#[cfg(feature = "viewer")]
impl TerminalsPanel {
    /// Sincroniza a lista de sessões ativas recebida do daemon (ex: ao abrir ou dar F5 no navegador).
    pub fn apply_list_sync(&mut self, sync: TerminalListSyncResponse, now_ms: u64) {
        self.sessions.clear();
        for summary in sync.sessions {
            let mut session = TerminalSession::with_size(
                summary.id.clone(),
                summary.title,
                summary.target,
                summary.cols,
                summary.rows,
            );
            session.is_running = summary.is_running;
            if !summary.initial_bytes.is_empty() {
                session.process_bytes(&summary.initial_bytes, now_ms);
            }
            self.sessions.push(session);
        }
        self.next_counter = self.sessions.len() + 1;
        if self.active_tab_index >= self.sessions.len() {
            self.active_tab_index = self.sessions.len().saturating_sub(1);
        }
    }

    /// Processa bytes de saída recebidos do daemon para um terminal específico.
    pub fn handle_output(&mut self, msg: TerminalOutputMessage, now_ms: u64) {
        if let Some(session) = self.sessions.iter_mut().find(|s| s.id == msg.id) {
            session.process_bytes(&msg.data, now_ms);
        }
    }

    /// Atualiza o status de execução (rodando/finalizado) ou cria a sessão caso tenha sido aberta em outro cliente.
    pub fn handle_status(&mut self, msg: TerminalStatusMessage) {
        if let Some(session) = self.sessions.iter_mut().find(|s| s.id == msg.id) {
            session.is_running = msg.is_running;
            session.title = msg.title;
        } else {
            let mut session = TerminalSession::new(msg.id, msg.title, msg.target);
            session.is_running = msg.is_running;
            self.sessions.push(session);
            self.active_tab_index = self.sessions.len() - 1;
        }
    }

    /// Remove uma aba caso tenha sido fechada remotamente.
    pub fn handle_closed(&mut self, id: &str) {
        if let Some(pos) = self.sessions.iter().position(|s| s.id == id) {
            self.sessions.remove(pos);
            if self.active_tab_index >= self.sessions.len() {
                self.active_tab_index = self.sessions.len().saturating_sub(1);
            }
        }
    }

    fn spawn_new_terminal(
        &mut self,
        target: TerminalTarget,
        client: Option<&ControlClient>,
        _config: Option<&AppConfig>,
    ) {
        let idx = self.next_counter;
        self.next_counter += 1;

        let target_label = match target {
            TerminalTarget::Host => "Host",
            TerminalTarget::Docker => "Docker",
        };
        let id = format!("term_{}_{}", idx, current_time_ms() % 100_000);
        let title = format!("term_{} ({})", idx, target_label);
        let cols = 80;
        let rows = 24;

        let session = TerminalSession::new(id.clone(), title.clone(), target);
        self.sessions.push(session);
        self.active_tab_index = self.sessions.len() - 1;

        if let Some(c) = client {
            c.send(ControlMessage::new(
                Domain::Terminal,
                "create",
                serde_json::to_value(TerminalCreateRequest {
                    id,
                    title,
                    target,
                    cols,
                    rows,
                })
                .unwrap_or_default(),
            ));
        } else {
            #[cfg(not(target_arch = "wasm32"))]
            {
                let (container, entrypoint, ws_path) = if let Some(cfg) = _config {
                    (
                        cfg.docker_container_name.as_str(),
                        cfg.docker_entrypoint.as_str(),
                        cfg.workspace_path.as_str(),
                    )
                } else {
                    ("perse_container", "", "")
                };
                let tx_out = self.local_events_tx.clone();
                let tx_exit = self.local_events_tx.clone();
                if let Err(err) = self.local_pty_manager.create_session(
                    id.clone(),
                    title,
                    target,
                    cols,
                    rows,
                    container,
                    entrypoint,
                    ws_path,
                    move |sid, data| {
                        let _ = tx_out.send(LocalPtyEvent::Output(sid, data));
                    },
                    move |sid| {
                        let _ = tx_exit.send(LocalPtyEvent::Exit(sid));
                    },
                ) {
                    if let Some(s) = self.sessions.iter_mut().find(|s| s.id == id) {
                        let msg = format!("\r\n\x1b[31m[Erro ao iniciar PTY: {}]\x1b[0m\r\n", err);
                        s.process_bytes(msg.as_bytes(), current_time_ms());
                        s.is_running = false;
                    }
                }
            }
        }
    }

    pub fn ui(
        &mut self,
        ui: &mut Ui,
        client: Option<&ControlClient>,
        config: Option<&AppConfig>,
    ) {
        // Inicializa o seletor padrão de acordo com a configuração de LaunchTargetMode
        if !self.initialized_target_from_config {
            if let Some(cfg) = config {
                self.selected_target = match cfg.launch_target_mode {
                    LaunchTargetMode::Host => TerminalTarget::Host,
                    LaunchTargetMode::Docker => TerminalTarget::Docker,
                };
                self.initialized_target_from_config = true;
            }
        }

        // Drena eventos do PTY local caso esteja rodando nativo sem daemon remoto
        #[cfg(not(target_arch = "wasm32"))]
        {
            let now = current_time_ms();
            let mut got_local_output = false;
            while let Ok(ev) = self.local_events_rx.try_recv() {
                match ev {
                    LocalPtyEvent::Output(id, data) => {
                        if let Some(s) = self.sessions.iter_mut().find(|s| s.id == id) {
                            s.process_bytes(&data, now);
                            got_local_output = true;
                        }
                    }
                    LocalPtyEvent::Exit(id) => {
                        if let Some(s) = self.sessions.iter_mut().find(|s| s.id == id) {
                            s.is_running = false;
                            s.process_bytes(
                                b"\r\n\x1b[33m[Processo finalizado]\x1b[0m\r\n",
                                now,
                            );
                            got_local_output = true;
                        }
                    }
                }
            }
            if got_local_output {
                ui.ctx().request_repaint();
            }
        }

        let mut create_requested: Option<TerminalTarget> = None;

        // Cabeçalho com seletor Host/Docker e botão + Novo Terminal
        section_header(
            ui,
            "Terminais",
            &mut self.search_active,
            &mut self.search_query,
            |ui| {
                if re_icon_button(ui, &re_ui::icons::ADD, "Novo Terminal").clicked() {
                    create_requested = Some(self.selected_target);
                }

                // Seletor compacto do alvo (Host vs Container Docker)
                egui::ComboBox::from_id_salt("terminal_target_selector")
                    .width(78.0)
                    .selected_text(match self.selected_target {
                        TerminalTarget::Host => "Host",
                        TerminalTarget::Docker => "Docker",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.selected_target,
                            TerminalTarget::Host,
                            "Host (Bash)",
                        );
                        ui.selectable_value(
                            &mut self.selected_target,
                            TerminalTarget::Docker,
                            "Docker Container",
                        );
                    });
            },
        );

        if let Some(target) = create_requested {
            self.spawn_new_terminal(target, client, config);
        }

        if self.sessions.is_empty() {
            ui.add_space(16.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("Nenhum terminal aberto.")
                        .color(Color32::from_gray(150)),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("+ Novo Terminal (Host)").clicked() {
                        self.selected_target = TerminalTarget::Host;
                        self.spawn_new_terminal(TerminalTarget::Host, client, config);
                    }
                    if ui.button("+ Novo Terminal (Docker)").clicked() {
                        self.selected_target = TerminalTarget::Docker;
                        self.spawn_new_terminal(TerminalTarget::Docker, client, config);
                    }
                });
            });
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
                    let mut tab_to_activate = None;

                    for (idx, session) in self.sessions.iter().enumerate() {
                        let is_active = idx == self.active_tab_index;
                        let (clicked_tab, clicked_close) =
                            render_tab_pill(ui, &session.title, session.is_running, is_active);

                        if clicked_tab {
                            tab_to_activate = Some(idx);
                        }

                        if clicked_close {
                            tab_to_close = Some(idx);
                        }
                    }

                    if let Some(activate_idx) = tab_to_activate {
                        self.active_tab_index = activate_idx;
                        for (i, s) in self.sessions.iter_mut().enumerate() {
                            s.is_focused = i == activate_idx;
                        }
                    }

                    if let Some(close_idx) = tab_to_close {
                        let closed_id = self.sessions[close_idx].id.clone();
                        self.sessions.remove(close_idx);
                        if self.active_tab_index >= self.sessions.len() {
                            self.active_tab_index = self.sessions.len().saturating_sub(1);
                        }
                        if self.sessions.is_empty() {
                            widget::set_terminal_focus_web(false);
                        }
                        if let Some(c) = client {
                            c.send(ControlMessage::new(
                                Domain::Terminal,
                                "close",
                                serde_json::to_value(TerminalCloseRequest { id: closed_id })
                                    .unwrap_or_default(),
                            ));
                        } else {
                            #[cfg(not(target_arch = "wasm32"))]
                            {
                                self.local_pty_manager.close_session(&closed_id);
                            }
                        }
                    }
                });
            });

        if self.sessions.is_empty() {
            return;
        }

        let tab_idx = self
            .active_tab_index
            .min(self.sessions.len().saturating_sub(1));
        let session = &mut self.sessions[tab_idx];

        // Renderiza o emulador VT100 interativo ocupando todo o espaço restante do painel
        let widget_out = show_terminal_widget(ui, session);

        if let Some((cols, rows)) = widget_out.resized {
            if let Some(c) = client {
                c.send(ControlMessage::new(
                    Domain::Terminal,
                    "resize",
                    serde_json::to_value(TerminalResizeMessage {
                        id: session.id.clone(),
                        cols,
                        rows,
                    })
                    .unwrap_or_default(),
                ));
            } else {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let _ = self
                        .local_pty_manager
                        .resize_session(&session.id, cols, rows);
                }
            }
        }

        if !widget_out.input_bytes.is_empty() {
            if let Some(c) = client {
                c.send(ControlMessage::new(
                    Domain::Terminal,
                    "input",
                    serde_json::to_value(TerminalInputMessage {
                        id: session.id.clone(),
                        data: widget_out.input_bytes,
                    })
                    .unwrap_or_default(),
                ));
            } else {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let _ = self
                        .local_pty_manager
                        .write_input(&session.id, &widget_out.input_bytes);
                }
            }
        }
    }
}

#[cfg(feature = "viewer")]
fn current_time_ms() -> u64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|w| w.performance())
            .map(|p| p.now() as u64)
            .unwrap_or(0)
    }
}

#[cfg(feature = "viewer")]
fn render_tab_pill(
    ui: &mut Ui,
    title: &str,
    is_running: bool,
    is_active: bool,
) -> (bool, bool) {
    let dot_color = if is_running {
        Color32::from_rgb(80, 210, 120)
    } else {
        Color32::from_gray(120)
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

    let mut clicked_tab = false;
    let mut clicked_close = false;

    Frame::new()
        .fill(bg)
        .stroke(stroke)
        .corner_radius(4.0)
        .inner_margin(Margin {
            left: 6,
            right: 4,
            top: 2,
            bottom: 2,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(RichText::new("●").size(10.0).color(dot_color));
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new(title).strong().size(11.5).color(text_color),
                        )
                        .frame(false),
                    )
                    .clicked()
                {
                    clicked_tab = true;
                }
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new("✕")
                                .size(10.5)
                                .color(Color32::from_gray(150)),
                        )
                        .frame(false),
                    )
                    .on_hover_text("Fechar terminal e encerrar processo")
                    .clicked()
                {
                    clicked_close = true;
                }
            });
        });

    (clicked_tab, clicked_close)
}
