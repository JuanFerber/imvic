//! Central input registry managing and multiplexing active hardware input plugins.

use super::touchpad::TouchpadInputAdapter;
use super::{AppEvent, InputAdapter};
use crossterm::event::Event;

/// Central input registry managing active hardware input adapters.
pub struct InputRegistry {
    adapters: Vec<Box<dyn InputAdapter>>,
}

impl InputRegistry {
    /// Creates an input registry initialized with all detected and available hardware adapters.
    pub fn new() -> Self {
        let mut registry = Self {
            adapters: Vec::new(),
        };

        // Candidate hardware input plugins
        let candidates: Vec<Box<dyn InputAdapter>> = vec![
            Box::new(TouchpadInputAdapter::new()),
            // Future plugins (MouseInputAdapter, VimInputAdapter) register here
        ];

        // Auto-detect and activate only available hardware devices
        for adapter in candidates {
            if adapter.is_available() {
                registry.register(adapter);
            }
        }

        // Fallback safety: if hardware detection is uncertain, ensure at least touchpad is active
        if registry.adapters.is_empty() {
            registry.register(Box::new(TouchpadInputAdapter::new()));
        }

        registry
    }

    /// Registers an additional input adapter plugin.
    pub fn register(&mut self, adapter: Box<dyn InputAdapter>) {
        self.adapters.push(adapter);
    }

    /// Translates raw crossterm events by dispatching through all active input adapters.
    pub fn handle_event(&mut self, event: &Event) -> Option<AppEvent> {
        for adapter in &mut self.adapters {
            if let Some(app_event) = adapter.handle_event(event) {
                return Some(app_event);
            }
        }
        None
    }

    /// Returns a human-readable list of active input drivers (e.g. "Touchpad", "Touchpad + Mouse").
    pub fn active_drivers_summary(&self) -> String {
        self.adapters
            .iter()
            .map(|a| a.name())
            .collect::<Vec<_>>()
            .join(" + ")
    }
}

impl Default for InputRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_registry_initialization() {
        let registry = InputRegistry::new();
        assert!(!registry.adapters.is_empty());
        assert!(registry.active_drivers_summary().contains("Touchpad"));
    }
}
