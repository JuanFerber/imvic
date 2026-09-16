//! Graphics backends for high-resolution terminal rendering.

pub mod kitty;

use crate::display::transport::TransportAdapter;
use anyhow::{Result, bail};
use image::RgbaImage;
use std::io::Write;

pub use kitty::KittyBackend;

/// Contract for high-resolution graphics backend plugins.
pub trait GraphicsBackend: Send + Sync {
    /// Human-readable backend name (e.g. "Kitty Graphics Protocol").
    fn name(&self) -> &'static str;

    /// Criterion 1: Checks if the host terminal matches this backend plugin.
    fn is_detected(&self) -> bool;

    /// Criterion 2: Checks if the graphics protocol is natively supported and active in the host terminal.
    fn is_protocol_available(&self) -> bool;

    /// Draws a frame to the terminal using native GPU pixel rendering.
    fn draw_image(
        &mut self,
        writer: &mut dyn Write,
        transport: &dyn TransportAdapter,
        frame: &RgbaImage,
        target_cells: (u16, u16),
    ) -> Result<()>;

    /// Clears graphics placed on the terminal screen/GPU.
    fn clear_graphics(
        &mut self,
        writer: &mut dyn Write,
        transport: &dyn TransportAdapter,
    ) -> Result<()>;

    /// Clones this backend into a boxed trait object.
    fn clone_box(&self) -> Box<dyn GraphicsBackend>;
}

/// Central registry managing all registered graphics backend plugins.
pub struct BackendRegistry {
    backends: Vec<Box<dyn GraphicsBackend>>,
}

impl BackendRegistry {
    /// Creates a registry pre-loaded with all built-in backend plugins.
    pub fn new() -> Self {
        let mut registry = Self {
            backends: Vec::new(),
        };

        // Register default plugins (Ghostty, WezTerm, Sixel will register here)
        registry.register(Box::new(KittyBackend::new()));

        registry
    }

    /// Registers an additional terminal graphics backend plugin.
    pub fn register(&mut self, backend: Box<dyn GraphicsBackend>) {
        self.backends.push(backend);
    }

    /// Validates both the host terminal and the active multiplexer transport tunnel.
    pub fn select_backend(
        &self,
        transport: &dyn TransportAdapter,
    ) -> Result<Box<dyn GraphicsBackend>> {
        // 1. Locate a backend matching the host terminal
        let mut matched_backend: Option<&dyn GraphicsBackend> = None;

        for backend in &self.backends {
            if backend.is_detected() {
                matched_backend = Some(backend.as_ref());
                break;
            }
        }

        let backend = match matched_backend {
            Some(b) => b,
            None => {
                bail!(
                    "High-resolution graphics protocol not detected in current terminal.\n\
                         Imvic requires native GPU graphics rendering to guarantee pin-sharp vector display.\n\n\
                         Supported terminals:\n\
                           - Kitty (https://sw.kovidgoyal.net/kitty/) [standalone or inside TMUX]\n\
                           - (Note: Ghostty and WezTerm plugins can be registered in BackendRegistry)"
                );
            }
        };

        // 2. Validate that the host terminal's protocol is operational
        if !backend.is_protocol_available() {
            bail!(
                "Terminal '{}' detected, but its graphics protocol is not available or supported.",
                backend.name()
            );
        }

        // 3. Validate that the multiplexer (if any) allows passthrough
        if !transport.is_passthrough_enabled() {
            let hint = transport
                .passthrough_enable_hint()
                .unwrap_or("enable passthrough");
            bail!(
                "Compatible terminal '{}' detected, but multiplexer '{}' has passthrough disabled.\n\
                     Enable passthrough by executing:\n\
                       {}\n\
                     or add 'set -g allow-passthrough on' to your multiplexer configuration.",
                backend.name(),
                transport.name(),
                hint
            );
        }

        // Return a clean clone of the selected backend
        Ok(backend.clone_box())
    }
}

impl Default for BackendRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience function to select the active backend using the global registry.
pub fn select_backend(transport: &dyn TransportAdapter) -> Result<Box<dyn GraphicsBackend>> {
    let registry = BackendRegistry::new();
    registry.select_backend(transport)
}
