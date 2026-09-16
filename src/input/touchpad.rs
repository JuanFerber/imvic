//! Touchpad-optimized input adapter (Default - Option 1).
//!
//! Maps 2-finger swipe to smooth 2D panning, pinch/Ctrl+scroll to smooth zoom,
//! and tap-and-drag to continuous panning.

use super::{AppEvent, InputAdapter};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

/// High-resolution touchpad gesture adapter.
pub struct TouchpadInputAdapter {
    last_drag_pos: Option<(u16, u16)>,
}

impl TouchpadInputAdapter {
    pub fn new() -> Self {
        Self {
            last_drag_pos: None,
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) -> Option<AppEvent> {
        match mouse.kind {
            // Pinch-to-zoom or Ctrl + 2-finger scroll: smooth focal zoom
            MouseEventKind::ScrollUp if mouse.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(AppEvent::ZoomIn {
                    cursor_x: mouse.column,
                    cursor_y: mouse.row,
                    factor: 1.05,
                })
            }
            MouseEventKind::ScrollDown if mouse.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(AppEvent::ZoomOut {
                    cursor_x: mouse.column,
                    cursor_y: mouse.row,
                    factor: 0.95,
                })
            }

            // Regular 2-finger scroll on touchpad: smooth 2D panning
            MouseEventKind::ScrollUp => Some(AppEvent::Pan {
                delta_x: 0.0,
                delta_y: 1.0,
            }),
            MouseEventKind::ScrollDown => Some(AppEvent::Pan {
                delta_x: 0.0,
                delta_y: -1.0,
            }),
            MouseEventKind::ScrollLeft => Some(AppEvent::Pan {
                delta_x: 1.0,
                delta_y: 0.0,
            }),
            MouseEventKind::ScrollRight => Some(AppEvent::Pan {
                delta_x: -1.0,
                delta_y: 0.0,
            }),

            // Tap-and-drag or click-and-drag: continuous single-finger pan
            MouseEventKind::Down(MouseButton::Left) => {
                self.last_drag_pos = Some((mouse.column, mouse.row));
                None
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some((prev_col, prev_row)) = self.last_drag_pos {
                    let delta_x = mouse.column as f32 - prev_col as f32;
                    let delta_y = mouse.row as f32 - prev_row as f32;
                    self.last_drag_pos = Some((mouse.column, mouse.row));
                    Some(AppEvent::Pan { delta_x, delta_y })
                } else {
                    self.last_drag_pos = Some((mouse.column, mouse.row));
                    None
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.last_drag_pos = None;
                None
            }

            _ => None,
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Option<AppEvent> {
        match key.code {
            // Only essential keys to exit the raw mode terminal cleanly
            KeyCode::Char('q') | KeyCode::Esc => Some(AppEvent::Quit),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(AppEvent::Quit)
            }
            _ => None,
        }
    }
}

impl Default for TouchpadInputAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl InputAdapter for TouchpadInputAdapter {
    fn name(&self) -> &'static str {
        "Touchpad"
    }

    fn is_available(&self) -> bool {
        #[cfg(target_os = "linux")]
        {
            if let Ok(devices) = std::fs::read_to_string("/proc/bus/input/devices") {
                return devices.to_ascii_lowercase().contains("touchpad");
            }
        }
        true
    }

    fn handle_event(&mut self, event: &Event) -> Option<AppEvent> {
        match event {
            Event::Mouse(mouse) => self.handle_mouse(*mouse),
            Event::Key(key) => self.handle_key(*key),
            Event::Resize(cols, rows) => Some(AppEvent::Resize {
                cols: *cols,
                rows: *rows,
            }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_touchpad_pan_events() {
        let mut adapter = TouchpadInputAdapter::new();
        let scroll_up = Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 10,
            row: 10,
            modifiers: KeyModifiers::empty(),
        });

        let event = adapter.handle_event(&scroll_up);
        assert_eq!(
            event,
            Some(AppEvent::Pan {
                delta_x: 0.0,
                delta_y: 1.0
            })
        );
    }

    #[test]
    fn test_touchpad_ctrl_zoom() {
        let mut adapter = TouchpadInputAdapter::new();
        let pinch_zoom = Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 20,
            row: 15,
            modifiers: KeyModifiers::CONTROL,
        });

        let event = adapter.handle_event(&pinch_zoom);
        assert_eq!(
            event,
            Some(AppEvent::ZoomIn {
                cursor_x: 20,
                cursor_y: 15,
                factor: 1.05
            })
        );
    }

    #[test]
    fn test_quit_key() {
        let mut adapter = TouchpadInputAdapter::new();
        let quit_key = Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::empty()));
        assert_eq!(adapter.handle_event(&quit_key), Some(AppEvent::Quit));
    }
}
