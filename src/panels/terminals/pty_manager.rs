use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};

use crate::net::protocol::{TerminalSessionSummary, TerminalTarget};

/// Maximum number of raw bytes buffered per session to replay upon WebSocket reconnection.
const MAX_HISTORY_BYTES: usize = 256 * 1024;

struct PtyWriterArc(Arc<Mutex<Box<dyn Write + Send>>>);

impl Write for PtyWriterArc {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Ok(mut w) = self.0.lock() {
            w.write(buf)
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Lock poisoned",
            ))
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if let Ok(mut w) = self.0.lock() {
            w.flush()
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Lock poisoned",
            ))
        }
    }
}

struct PtyHandle {
    title: String,
    target: TerminalTarget,
    cols: u16,
    rows: u16,
    is_running: Arc<AtomicBool>,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    output_history: Arc<Mutex<Vec<u8>>>,
}

/// Manages active PTY sessions on the host or inside a Docker container.
pub struct PtyManager {
    sessions: Arc<Mutex<HashMap<String, PtyHandle>>>,
}

impl Default for PtyManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PtyManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Spawns a new interactive PTY session (`Host` `/bin/bash` or `Docker` `docker exec -it ...`).
    ///
    /// `on_output` is invoked from a dedicated reader thread whenever the PTY emits bytes.
    /// `on_exit` is invoked when the PTY stream reaches EOF / process exits.
    pub fn create_session<FOut, FExit>(
        &self,
        id: String,
        title: String,
        target: TerminalTarget,
        cols: u16,
        rows: u16,
        docker_container: &str,
        docker_entrypoint: &str,
        workspace_path: &str,
        command: Option<&str>,
        on_output: FOut,
        on_exit: FExit,
    ) -> Result<(), String>
    where
        FOut: Fn(String, Vec<u8>) + Send + Sync + 'static,
        FExit: Fn(String) + Send + Sync + 'static,
    {
        let pty_system = native_pty_system();
        let size = PtySize {
            rows: rows.max(2),
            cols: cols.max(10),
            pixel_width: 0,
            pixel_height: 0,
        };

        let pair = pty_system
            .openpty(size)
            .map_err(|e| format!("Falha ao abrir PTY: {}", e))?;

        let mut cmd = match target {
            TerminalTarget::Host => {
                let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
                let mut c = CommandBuilder::new(shell);
                if !workspace_path.trim().is_empty()
                    && std::path::Path::new(workspace_path.trim()).is_dir()
                {
                    c.cwd(workspace_path.trim());
                }
                if let Some(cmd_str) = command {
                    c.arg("-c");
                    c.arg(cmd_str);
                }
                c
            }
            TerminalTarget::Docker => {
                let container = if docker_container.trim().is_empty() {
                    "perse_container"
                } else {
                    docker_container.trim()
                };
                let mut c = CommandBuilder::new("docker");
                c.arg("exec");
                c.arg("-it");
                c.arg(container);
                c.arg("bash");

                let entry = docker_entrypoint.trim();
                let init_cmd = if !entry.is_empty() {
                    format!(
                        "if [ -f \"{0}\" ]; then source \"{0}\" 2>/dev/null; fi; ",
                        entry
                    )
                } else {
                    String::new()
                };

                if let Some(cmd_str) = command {
                    c.arg("-c");
                    c.arg(format!("{}{}", init_cmd, cmd_str));
                } else if !entry.is_empty() {
                    c.arg("-c");
                    c.arg(format!("{}exec bash -i", init_cmd));
                }
                c
            }
        };

        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("Falha ao iniciar processo no PTY: {}", e))?;

        // Drop slave in parent so EOF is detected when child exits
        drop(pair.slave);

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| format!("Falha ao clonar leitor do PTY: {}", e))?;

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| format!("Falha ao obter escritor do PTY: {}", e))?;
        let writer = Arc::new(Mutex::new(writer));

        let is_running = Arc::new(AtomicBool::new(true));
        let output_history = Arc::new(Mutex::new(Vec::new()));

        let handle = PtyHandle {
            title,
            target,
            cols: size.cols,
            rows: size.rows,
            is_running: is_running.clone(),
            master: pair.master,
            writer: Box::new(PtyWriterArc(writer.clone())),
            child,
            output_history: output_history.clone(),
        };

        if let Ok(mut map) = self.sessions.lock() {
            map.insert(id.clone(), handle);
        }

        let id_reader = id.clone();
        let writer_for_dsr = writer.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let chunk = buf[..n].to_vec();

                        // Intercept ANSI DSR (Device Status Report: \x1b[6n ou \x9b6n)
                        // Enviado por utilitários como atuin ou starship para consultar a posição do cursor.
                        // Sem esta resposta imediata no stdin/PTY, ferramentas como o atuin estouram o timeout
                        // e imprimem mensagens de erro como "The cursor position could not be read within a normal duration".
                        if chunk.windows(4).any(|w| w == b"\x1b[6n")
                            || chunk.windows(3).any(|w| w == b"\x9b6n")
                        {
                            if let Ok(mut w) = writer_for_dsr.lock() {
                                let _ = w.write_all(b"\x1b[1;1R");
                                let _ = w.flush();
                            }
                        }

                        if let Ok(mut hist) = output_history.lock() {
                            hist.extend_from_slice(&chunk);
                            if hist.len() > MAX_HISTORY_BYTES {
                                let drain_len = hist.len() - MAX_HISTORY_BYTES;
                                hist.drain(0..drain_len);
                            }
                        }
                        on_output(id_reader.clone(), chunk);
                    }
                    Err(_) => break,
                }
            }
            is_running.store(false, Ordering::SeqCst);
            on_exit(id_reader);
        });

        Ok(())
    }

    /// Writes user keyboard/paste bytes to the PTY master of session `id`.
    pub fn write_input(&self, id: &str, data: &[u8]) -> Result<(), String> {
        let mut map = self
            .sessions
            .lock()
            .map_err(|_| "Lock error on sessions".to_string())?;
        let Some(handle) = map.get_mut(id) else {
            return Err(format!("Sessão de terminal '{}' não encontrada", id));
        };
        handle
            .writer
            .write_all(data)
            .map_err(|e| format!("Erro ao escrever no PTY: {}", e))?;
        let _ = handle.writer.flush();
        Ok(())
    }

    /// Resizes the PTY window (`TIOCSWINSZ`) for session `id`.
    pub fn resize_session(&self, id: &str, cols: u16, rows: u16) -> Result<(), String> {
        let mut map = self
            .sessions
            .lock()
            .map_err(|_| "Lock error on sessions".to_string())?;
        let Some(handle) = map.get_mut(id) else {
            return Err(format!("Sessão de terminal '{}' não encontrada", id));
        };
        let c = cols.max(10);
        let r = rows.max(2);
        handle.cols = c;
        handle.rows = r;
        handle
            .master
            .resize(PtySize {
                rows: r,
                cols: c,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("Erro ao redimensionar PTY: {}", e))?;
        Ok(())
    }

    /// Terminates the child process and removes the PTY session.
    pub fn close_session(&self, id: &str) -> bool {
        let removed = self.sessions.lock().ok().and_then(|mut map| map.remove(id));
        if let Some(mut handle) = removed {
            handle.is_running.store(false, Ordering::SeqCst);
            let _ = handle.child.kill();
            let _ = handle.child.wait();
            true
        } else {
            false
        }
    }

    /// Returns a list of all active/open terminal sessions with their buffered output history
    /// so reconnecting clients can reconstruct the exact terminal state.
    pub fn list_sessions(&self) -> Vec<TerminalSessionSummary> {
        let Ok(map) = self.sessions.lock() else {
            return Vec::new();
        };
        let mut list: Vec<TerminalSessionSummary> = map
            .iter()
            .map(|(id, handle)| {
                let initial_bytes = handle
                    .output_history
                    .lock()
                    .map(|h| h.clone())
                    .unwrap_or_default();
                TerminalSessionSummary {
                    id: id.clone(),
                    title: handle.title.clone(),
                    target: handle.target,
                    cols: handle.cols,
                    rows: handle.rows,
                    is_running: handle.is_running.load(Ordering::SeqCst),
                    initial_bytes,
                }
            })
            .collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        list
    }
}
