use crate::net::protocol::TerminalTarget;

/// Default number of scrollback lines kept in the `vt100::Parser`.
pub const DEFAULT_SCROLLBACK_LEN: usize = 5000;

/// Intervalo de seleção de texto no terminal em coordenadas `(linha, coluna)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalSelection {
    pub start: (usize, usize),
    pub end: (usize, usize),
}

impl TerminalSelection {
    /// Retorna o par `(início, fim)` ordenado no sentido de leitura normal (de cima para baixo, esquerda para direita).
    pub fn normalized(&self) -> ((usize, usize), (usize, usize)) {
        if self.start.0 < self.end.0 || (self.start.0 == self.end.0 && self.start.1 <= self.end.1) {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        }
    }

    /// Verifica se uma célula `(row, col)` está compreendida no intervalo de seleção atual.
    pub fn contains(&self, row: usize, col: usize) -> bool {
        let (start, end) = self.normalized();
        if row < start.0 || row > end.0 {
            return false;
        }
        if start.0 == end.0 {
            col >= start.1 && col <= end.1
        } else if row == start.0 {
            col >= start.1
        } else if row == end.0 {
            col <= end.1
        } else {
            true
        }
    }
}

/// Represents the state of a single terminal tab on the frontend (or local viewer).
pub struct TerminalSession {
    /// Unique identifier for the terminal session (matches daemon ID).
    pub id: String,
    /// Display title of the tab (e.g., "Terminal 1 (Host)").
    pub title: String,
    /// Whether the underlying PTY process is currently alive.
    pub is_running: bool,
    /// Execution target (Host or Docker).
    pub target: TerminalTarget,
    /// VT100 terminal state machine and grid buffer.
    pub parser: vt100::Parser,
    /// Current terminal width in character columns.
    pub cols: u16,
    /// Current terminal height in character rows.
    pub rows: u16,
    /// Historical byte chunks tagged with a monotonic timestamp (in ms).
    /// Prepared for future Rerun timeline synchronization so a parser can be
    /// reconstructed at any arbitrary point in time.
    pub chunks_history: Vec<(u64, Vec<u8>)>,
    /// Whether this terminal tab is currently focused by the user.
    pub is_focused: bool,
    /// Active text selection range within the visible screen.
    pub selection: Option<TerminalSelection>,
}

impl TerminalSession {
    /// Creates a new terminal session with specified dimensions.
    pub fn with_size(id: String, title: String, target: TerminalTarget, cols: u16, rows: u16) -> Self {
        let cols = cols.max(10);
        let rows = rows.max(2);
        Self {
            id,
            title,
            is_running: true,
            target,
            parser: vt100::Parser::new(rows, cols, DEFAULT_SCROLLBACK_LEN),
            cols,
            rows,
            chunks_history: Vec::new(),
            is_focused: false,
            selection: None,
        }
    }

    /// Creates a new terminal session with default dimensions (`80x24`).
    pub fn new(id: String, title: String, target: TerminalTarget) -> Self {
        Self::with_size(id, title, target, 80, 24)
    }

    /// Retorna o texto atualmente selecionado no buffer visível do terminal.
    pub fn get_selected_text(&self) -> String {
        let Some(sel) = self.selection else {
            return String::new();
        };
        let (start, end) = sel.normalized();
        let screen = self.parser.screen();
        let (rows, cols) = screen.size();
        let mut lines = Vec::new();

        let max_r = (rows as usize).saturating_sub(1);
        let max_c = (cols as usize).saturating_sub(1);

        for row in start.0..=end.0.min(max_r) {
            let col_start = if row == start.0 { start.1.min(max_c) } else { 0 };
            let col_end = if row == end.0 { end.1.min(max_c) } else { max_c };

            let mut line = String::new();
            for col in col_start..=col_end {
                if let Some(cell) = screen.cell(row as u16, col as u16) {
                    if cell.is_wide_continuation() {
                        continue;
                    }
                    let c = cell.contents();
                    if c.is_empty() {
                        line.push(' ');
                    } else {
                        line.push_str(c);
                    }
                } else {
                    line.push(' ');
                }
            }
            lines.push(line.trim_end().to_string());
        }

        lines.join("\n")
    }

    /// Limpa qualquer seleção ativa de texto.
    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    /// Feeds raw PTY output bytes into the VT100 parser and records the chunk in history.
    pub fn process_bytes(&mut self, data: &[u8], timestamp_ms: u64) {
        if data.is_empty() {
            return;
        }
        self.parser.process(data);
        self.chunks_history.push((timestamp_ms, data.to_vec()));
    }

    /// Resizes the VT100 screen grid if dimensions changed.
    /// Returns `true` if the dimensions actually changed.
    ///
    /// Re-parses the history with the new dimensions so that columns truncated
    /// when the window was narrowed are fully restored when expanded again,
    /// and visible lines are naturally wrapped according to the new viewport size.
    pub fn resize(&mut self, cols: u16, rows: u16) -> bool {
        let cols = cols.max(10);
        let rows = rows.max(2);
        if self.cols != cols || self.rows != rows {
            self.cols = cols;
            self.rows = rows;

            let prev_scrollback = self.parser.screen().scrollback();

            // Recria o parser com as novas dimensões e reprocessa o histórico de bytes
            let mut new_parser = vt100::Parser::new(rows, cols, DEFAULT_SCROLLBACK_LEN);
            for (_, chunk) in &self.chunks_history {
                new_parser.process(chunk);
            }

            if prev_scrollback > 0 {
                new_parser.screen_mut().set_scrollback(prev_scrollback);
            }
            self.parser = new_parser;

            true
        } else {
            false
        }
    }

    /// Adjusts the scrollback offset by `delta_lines` (positive scrolls up into history,
    /// negative scrolls down toward the live prompt).
    pub fn scroll(&mut self, delta_lines: isize) {
        let current = self.parser.screen().scrollback() as isize;
        let next = (current + delta_lines).max(0) as usize;
        self.parser.screen_mut().set_scrollback(next);
    }

    /// Resets the scrollback to `0` (bottom of the terminal / live view).
    pub fn scroll_to_bottom(&mut self) {
        if self.parser.screen().scrollback() != 0 {
            self.parser.screen_mut().set_scrollback(0);
        }
    }

    /// Reconstructs a `vt100::Parser` containing all bytes received up to `target_timestamp_ms`.
    /// This enables scrubbing the terminal state along the Rerun timeline in the future.
    pub fn replay_at_timestamp(&self, target_timestamp_ms: u64) -> vt100::Parser {
        let mut replay_parser =
            vt100::Parser::new(self.rows, self.cols, DEFAULT_SCROLLBACK_LEN);
        for (ts, chunk) in &self.chunks_history {
            if *ts <= target_timestamp_ms {
                replay_parser.process(chunk);
            } else {
                break;
            }
        }
        replay_parser
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_process_and_replay() {
        let mut session = TerminalSession::new(
            "term-1".to_string(),
            "Terminal 1".to_string(),
            TerminalTarget::Host,
        );

        session.process_bytes(b"Hello ", 100);
        session.process_bytes(b"\x1b[32mWorld\x1b[0m\r\n", 200);
        session.process_bytes(b"Second line", 300);

        assert!(session.parser.screen().contents().contains("Hello World"));
        assert!(session.parser.screen().contents().contains("Second line"));

        // Replay at t=200 should have "Hello World" but not "Second line"
        let replayed = session.replay_at_timestamp(200);
        let text = replayed.screen().contents();
        assert!(text.contains("Hello World"));
        assert!(!text.contains("Second line"));
    }

    #[test]
    fn test_session_resize() {
        let mut session = TerminalSession::new(
            "term-1".to_string(),
            "Terminal 1".to_string(),
            TerminalTarget::Host,
        );
        assert_eq!(session.parser.screen().size(), (24, 80));
        assert!(session.resize(120, 40));
        assert_eq!(session.parser.screen().size(), (40, 120));
        assert!(!session.resize(120, 40));
    }

    #[test]
    fn test_session_resize_preserves_scrollback_history() {
        let mut session = TerminalSession::new(
            "term-1".to_string(),
            "Terminal 1".to_string(),
            TerminalTarget::Host,
        );
        session.resize(80, 24);
        for i in 0..50 {
            session.process_bytes(format!("History Line {:02}\r\n", i).as_bytes(), i as u64);
        }

        // Scrollback acumulou linhas
        assert!(session.parser.screen().scrollback_len() > 0);

        // Rola 10 linhas para cima
        session.scroll(10);
        let scrolled_text = session.parser.screen().contents();
        assert!(scrolled_text.contains("History Line"));

        // Redimensiona para uma janela estreita
        session.resize(40, 20);
        assert_eq!(session.cols, 40);
        assert_eq!(session.rows, 20);
        assert!(session.parser.screen().contents().contains("History Line"));

        // Redimensiona de volta para uma janela larga
        session.resize(100, 30);
        assert_eq!(session.cols, 100);
        assert_eq!(session.rows, 30);
        assert!(session.parser.screen().contents().contains("History Line"));
    }

    #[test]
    fn test_simulate_python_loop_resize() {
        let mut session = TerminalSession::new(
            "term-1".to_string(),
            "Terminal 1".to_string(),
            TerminalTarget::Host,
        );
        session.resize(80, 24);
        for i in 0..100 {
            session.process_bytes(format!("{} {}\r\n", i, "=".repeat(60)).as_bytes(), i as u64);
        }

        println!("--- BEFORE RESIZE (scrollback = {}) ---", session.parser.screen().scrollback());
        let last_lines: Vec<_> = session.parser.screen().contents().lines().rev().take(5).map(|s| s.to_string()).collect();
        println!("Last lines: {:?}", last_lines);

        // Narrow to 40 cols
        session.resize(40, 24);
        println!("--- AFTER NARROW TO 40 COLS ---");
        let last_lines: Vec<_> = session.parser.screen().contents().lines().rev().take(5).map(|s| s.to_string()).collect();
        println!("Last lines: {:?}", last_lines);

        // Expand to 100 cols
        session.resize(100, 24);
        println!("--- AFTER EXPAND TO 100 COLS ---");
        let last_lines: Vec<_> = session.parser.screen().contents().lines().rev().take(5).map(|s| s.to_string()).collect();
        println!("Last lines: {:?}", last_lines);
    }

    #[test]
    fn test_terminal_selection_and_copy() {
        let mut session = TerminalSession::new(
            "term-1".to_string(),
            "Terminal 1".to_string(),
            TerminalTarget::Host,
        );
        session.resize(80, 24);
        session.process_bytes(b"echo hello world\r\nsecond line here", 10);

        // Sem seleção
        assert_eq!(session.get_selected_text(), "");

        // Seleção na primeira linha: "hello world"
        session.selection = Some(TerminalSelection {
            start: (0, 5),
            end: (0, 15),
        });
        assert_eq!(session.get_selected_text(), "hello world");

        // Seleção invertida (arrastada da direita para esquerda): deve normalizar
        session.selection = Some(TerminalSelection {
            start: (0, 15),
            end: (0, 5),
        });
        assert_eq!(session.get_selected_text(), "hello world");

        // Seleção multilinhas
        session.selection = Some(TerminalSelection {
            start: (0, 5),
            end: (1, 10),
        });
        let multiline = session.get_selected_text();
        assert!(multiline.starts_with("hello world"));
        assert!(multiline.contains("second line"));

        // Limpeza de seleção
        session.clear_selection();
        assert!(session.selection.is_none());
        assert_eq!(session.get_selected_text(), "");
    }

    #[test]
    fn test_app_cursor() {
        let mut session = TerminalSession::new("test".into(), "test".into(), TerminalTarget::Host);
        println!("Initial app_cursor: {}", session.parser.screen().application_cursor());
        session.process_bytes(b"\x1b[?1h", 0);
        println!("After \\x1b[?1h app_cursor: {}", session.parser.screen().application_cursor());
        assert!(session.parser.screen().application_cursor());
        session.process_bytes(b"\x1b[?1l", 0);
        println!("After \\x1b[?1l app_cursor: {}", session.parser.screen().application_cursor());
        assert!(!session.parser.screen().application_cursor());
    }
}
