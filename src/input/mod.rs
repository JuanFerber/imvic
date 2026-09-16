//! Modular input handling and hardware adapter contracts.

pub mod registry;
pub mod touchpad;

pub use registry::InputRegistry;
pub use touchpad::TouchpadInputAdapter;

use crossterm::event::Event;

/// Unified high-level application events produced by input adapters.
#[derive(Debug, Clone, PartialEq)]
pub enum AppEvent {
    /// Continuous panning from dragging or 2-finger touchpad gestures
    Pan { delta_x: f32, delta_y: f32 },
    /// Focal zoom centered at cursor coordinates
    ZoomIn {
        cursor_x: u16,
        cursor_y: u16,
        factor: f32,
    },
    ZoomOut {
        cursor_x: u16,
        cursor_y: u16,
        factor: f32,
    },
    /// Terminal window resized
    Resize { cols: u16, rows: u16 },
    /// Live reload signal from file system watcher
    FileModified,
    /// Exit application cleanly
    Quit,
}

/// Contract for hardware input adapters.
pub trait InputAdapter: Send + Sync {
    /// Descriptive name of the input driver (e.g. "Touchpad", "Mouse").
    fn name(&self) -> &'static str;

    /// Checks if the physical hardware for this adapter is detected on the system.
    fn is_available(&self) -> bool;

    /// Translates raw crossterm events into unified AppEvents.
    fn handle_event(&mut self, event: &Event) -> Option<AppEvent>;
}
