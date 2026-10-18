#[cfg(feature = "viewer")]
use egui::{Event, ImeEvent, Key, Modifiers};

/// Converte uma lista de eventos do `egui` capturados enquanto o terminal possui foco
/// em uma sequência de bytes ANSI pronta para ser enviada ao stdin do PTY.
#[cfg(feature = "viewer")]
pub fn extract_terminal_input_bytes(
    events: &[Event],
    app_cursor: bool,
    has_selection: bool,
) -> Vec<u8> {
    let mut out = Vec::new();

    for event in events {
        match event {
            Event::Text(text) => {
                // Em navegadores/Winit no Linux e Web, o pressionamento de teclas mortas (acentos como ~, ^, ´, `)
                // ou combinações incompletas de IME podem gerar eventos Event::Text("Dead") ou texto não intencional.
                // Filtramos "Dead" para evitar que a palavra seja digitada no prompt.
                if text != "Dead" {
                    out.extend_from_slice(text.as_bytes());
                }
            }
            Event::Ime(ImeEvent::Commit(text)) => {
                // Caractere composto gerado por IME ou dead keys (ex: ã, é, ç) no navegador via text_agent
                if text != "Dead" {
                    out.extend_from_slice(text.as_bytes());
                }
            }
            Event::Paste(text) => {
                out.extend_from_slice(text.as_bytes());
            }
            Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } => {
                if let Some(bytes) = map_key_press(*key, *modifiers, app_cursor, has_selection) {
                    out.extend_from_slice(&bytes);
                }
            }
            _ => {}
        }
    }

    out
}

/// Mapeia uma tecla pressionada com modificadores em sua sequência de escape ANSI/VT100 correspondente.
/// Nota: Caracteres imprimíveis comuns sem `Ctrl`/`Alt` retornam `None` aqui pois já são
/// entregues pelo `egui` como `Event::Text(...)`.
#[cfg(feature = "viewer")]
pub fn map_key_press(
    key: Key,
    modifiers: Modifiers,
    app_cursor: bool,
    has_selection: bool,
) -> Option<Vec<u8>> {
    // Se houver seleção ativa, Ctrl+C / Cmd+C copia para o clipboard e não deve enviar 0x03 (SIGINT)
    if has_selection && (modifiers.ctrl || modifiers.command || modifiers.mac_cmd) && key == Key::C
    {
        return None;
    }

    // Atalhos com Alt (ex: Alt+Backspace -> \x1b\x7f, Alt+Left -> \x1bb, Alt+Right -> \x1bf)
    if modifiers.alt && !modifiers.ctrl {
        match key {
            Key::Backspace => return Some(vec![0x1b, 0x7f]), // Alt+Backspace: apaga palavra anterior (Readline/Zsh)
            Key::ArrowLeft => return Some(b"\x1bb".to_vec()), // Alt+Left: recua palavra
            Key::ArrowRight => return Some(b"\x1bf".to_vec()), // Alt+Right: avança palavra
            Key::ArrowUp => return Some(b"\x1b[1;3A".to_vec()),
            Key::ArrowDown => return Some(b"\x1b[1;3B".to_vec()),
            Key::Home => return Some(b"\x1b[1;3H".to_vec()),
            Key::End => return Some(b"\x1b[1;3F".to_vec()),
            Key::Delete => return Some(b"\x1b[3;3~".to_vec()),
            Key::B => return Some(b"\x1bb".to_vec()),
            Key::F => return Some(b"\x1bf".to_vec()),
            Key::D => return Some(b"\x1bd".to_vec()), // Alt+D: apaga palavra seguinte
            _ => {}
        }
    }

    // Atalhos com Ctrl (ex: Ctrl+C -> 0x03, Ctrl+D -> 0x04, Ctrl+R -> 0x12, etc.)
    if modifiers.ctrl && !modifiers.alt {
        match key {
            Key::A => return Some(vec![0x01]),
            Key::B => return Some(vec![0x02]),
            Key::C => return Some(vec![0x03]),
            Key::D => return Some(vec![0x04]),
            Key::E => return Some(vec![0x05]),
            Key::F => return Some(vec![0x06]),
            Key::G => return Some(vec![0x07]),
            Key::H => return Some(vec![0x08]),
            Key::K => return Some(vec![0x0b]),
            Key::L => return Some(vec![0x0c]),
            Key::N => return Some(vec![0x0e]),
            Key::O => return Some(vec![0x0f]),
            Key::P => return Some(vec![0x10]),
            Key::Q => return Some(vec![0x11]),
            Key::R => return Some(vec![0x12]),
            Key::S => return Some(vec![0x13]),
            Key::T => return Some(vec![0x14]),
            Key::U => return Some(vec![0x15]),
            Key::W => return Some(vec![0x17]),
            Key::X => return Some(vec![0x18]),
            Key::Y => return Some(vec![0x19]),
            Key::Z => return Some(vec![0x1a]),
            Key::OpenBracket => return Some(vec![0x1b]),
            Key::Backslash => return Some(vec![0x1c]),
            Key::CloseBracket => return Some(vec![0x1d]),
            Key::ArrowRight => return Some(b"\x1b[1;5C".to_vec()),
            Key::ArrowLeft => return Some(b"\x1b[1;5D".to_vec()),
            Key::ArrowUp => return Some(b"\x1b[1;5A".to_vec()),
            Key::ArrowDown => return Some(b"\x1b[1;5B".to_vec()),
            Key::Home => return Some(b"\x1b[1;5H".to_vec()),
            Key::End => return Some(b"\x1b[1;5F".to_vec()),
            Key::Delete => return Some(b"\x1b[3;5~".to_vec()),
            Key::Backspace => return Some(vec![0x17]), // Ctrl+Backspace apaga palavra anterior (Ctrl+W)
            _ => {}
        }
    }

    // Atalhos com Shift (ex: Shift+Tab -> \x1b[Z, Shift+Arrows -> \x1b[1;2...)
    if modifiers.shift && !modifiers.ctrl && !modifiers.alt {
        match key {
            Key::Tab => return Some(b"\x1b[Z".to_vec()),
            Key::ArrowUp => return Some(b"\x1b[1;2A".to_vec()),
            Key::ArrowDown => return Some(b"\x1b[1;2B".to_vec()),
            Key::ArrowRight => return Some(b"\x1b[1;2C".to_vec()),
            Key::ArrowLeft => return Some(b"\x1b[1;2D".to_vec()),
            Key::Home => return Some(b"\x1b[1;2H".to_vec()),
            Key::End => return Some(b"\x1b[1;2F".to_vec()),
            _ => {}
        }
    }

    match key {
        Key::Enter => Some(b"\r".to_vec()),
        Key::Backspace => Some(vec![0x7f]),
        Key::Tab => Some(b"\t".to_vec()),
        Key::Escape => Some(vec![0x1b]),
        Key::ArrowUp => {
            if app_cursor {
                Some(b"\x1bOA".to_vec())
            } else {
                Some(b"\x1b[A".to_vec())
            }
        }
        Key::ArrowDown => {
            if app_cursor {
                Some(b"\x1bOB".to_vec())
            } else {
                Some(b"\x1b[B".to_vec())
            }
        }
        Key::ArrowRight => {
            if app_cursor {
                Some(b"\x1bOC".to_vec())
            } else {
                Some(b"\x1b[C".to_vec())
            }
        }
        Key::ArrowLeft => {
            if app_cursor {
                Some(b"\x1bOD".to_vec())
            } else {
                Some(b"\x1b[D".to_vec())
            }
        }
        Key::Home => {
            if app_cursor {
                Some(b"\x1bOH".to_vec())
            } else {
                Some(b"\x1b[H".to_vec())
            }
        }
        Key::End => {
            if app_cursor {
                Some(b"\x1bOF".to_vec())
            } else {
                Some(b"\x1b[F".to_vec())
            }
        }
        Key::PageUp => Some(b"\x1b[5~".to_vec()),
        Key::PageDown => Some(b"\x1b[6~".to_vec()),
        Key::Insert => Some(b"\x1b[2~".to_vec()),
        Key::Delete => Some(b"\x1b[3~".to_vec()),
        Key::F1 => Some(b"\x1bOP".to_vec()),
        Key::F2 => Some(b"\x1bOQ".to_vec()),
        Key::F3 => Some(b"\x1bOR".to_vec()),
        Key::F4 => Some(b"\x1bOS".to_vec()),
        Key::F5 => Some(b"\x1b[15~".to_vec()),
        Key::F6 => Some(b"\x1b[17~".to_vec()),
        Key::F7 => Some(b"\x1b[18~".to_vec()),
        Key::F8 => Some(b"\x1b[19~".to_vec()),
        Key::F9 => Some(b"\x1b[20~".to_vec()),
        Key::F10 => Some(b"\x1b[21~".to_vec()),
        Key::F11 => Some(b"\x1b[23~".to_vec()),
        Key::F12 => Some(b"\x1b[24~".to_vec()),
        _ => None,
    }
}

#[cfg(all(test, feature = "viewer"))]
mod tests {
    use super::*;

    #[test]
    fn test_ctrl_combinations() {
        let ctrl = Modifiers {
            ctrl: true,
            ..Default::default()
        };
        assert_eq!(map_key_press(Key::C, ctrl, false, false), Some(vec![0x03]));
        assert_eq!(map_key_press(Key::C, ctrl, false, true), None);
        assert_eq!(map_key_press(Key::D, ctrl, false, false), Some(vec![0x04]));
        assert_eq!(map_key_press(Key::L, ctrl, false, false), Some(vec![0x0c]));
        assert_eq!(map_key_press(Key::R, ctrl, false, false), Some(vec![0x12]));
        assert_eq!(map_key_press(Key::Z, ctrl, false, false), Some(vec![0x1a]));
        assert_eq!(
            map_key_press(Key::ArrowRight, ctrl, false, false),
            Some(b"\x1b[1;5C".to_vec())
        );
    }

    #[test]
    fn test_navigation_and_special_keys() {
        let none = Modifiers::NONE;
        assert_eq!(
            map_key_press(Key::Enter, none, false, false),
            Some(b"\r".to_vec())
        );
        assert_eq!(
            map_key_press(Key::Backspace, none, false, false),
            Some(vec![0x7f])
        );
        assert_eq!(
            map_key_press(Key::Tab, none, false, false),
            Some(b"\t".to_vec())
        );
        assert_eq!(
            map_key_press(
                Key::Tab,
                Modifiers {
                    shift: true,
                    ..Default::default()
                },
                false,
                false
            ),
            Some(b"\x1b[Z".to_vec())
        );
        assert_eq!(
            map_key_press(Key::ArrowUp, none, false, false),
            Some(b"\x1b[A".to_vec())
        );
        assert_eq!(
            map_key_press(Key::ArrowUp, none, true, false),
            Some(b"\x1bOA".to_vec())
        );
        assert_eq!(
            map_key_press(Key::ArrowDown, none, false, false),
            Some(b"\x1b[B".to_vec())
        );
        assert_eq!(
            map_key_press(Key::ArrowDown, none, true, false),
            Some(b"\x1bOB".to_vec())
        );
        assert_eq!(
            map_key_press(Key::ArrowRight, none, false, false),
            Some(b"\x1b[C".to_vec())
        );
        assert_eq!(
            map_key_press(Key::ArrowRight, none, true, false),
            Some(b"\x1bOC".to_vec())
        );
        assert_eq!(
            map_key_press(Key::ArrowLeft, none, false, false),
            Some(b"\x1b[D".to_vec())
        );
        assert_eq!(
            map_key_press(Key::ArrowLeft, none, true, false),
            Some(b"\x1bOD".to_vec())
        );
        assert_eq!(
            map_key_press(Key::Home, none, false, false),
            Some(b"\x1b[H".to_vec())
        );
        assert_eq!(
            map_key_press(Key::Home, none, true, false),
            Some(b"\x1bOH".to_vec())
        );
        assert_eq!(
            map_key_press(Key::End, none, false, false),
            Some(b"\x1b[F".to_vec())
        );
        assert_eq!(
            map_key_press(Key::End, none, true, false),
            Some(b"\x1bOF".to_vec())
        );
        assert_eq!(
            map_key_press(Key::Delete, none, false, false),
            Some(b"\x1b[3~".to_vec())
        );

        // Alt combinations
        let alt = Modifiers {
            alt: true,
            ..Default::default()
        };
        assert_eq!(
            map_key_press(Key::Backspace, alt, false, false),
            Some(vec![0x1b, 0x7f])
        );
        assert_eq!(
            map_key_press(Key::ArrowLeft, alt, false, false),
            Some(b"\x1bb".to_vec())
        );
        assert_eq!(
            map_key_press(Key::ArrowRight, alt, false, false),
            Some(b"\x1bf".to_vec())
        );
    }

    #[test]
    fn test_extract_terminal_input_bytes_filters_dead() {
        let events = vec![
            Event::Text("Dead".to_string()),
            Event::Text("a".to_string()),
        ];
        let bytes = extract_terminal_input_bytes(&events, false, false);
        assert_eq!(bytes, b"a".to_vec());
    }

    #[test]
    fn test_extract_terminal_input_bytes_ime_commit() {
        let events = vec![
            Event::Ime(ImeEvent::Commit("ã".to_string())),
            Event::Ime(ImeEvent::Commit("é".to_string())),
        ];
        let bytes = extract_terminal_input_bytes(&events, false, false);
        assert_eq!(bytes, "ãé".as_bytes().to_vec());
    }
}
