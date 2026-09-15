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

    /// Checks if the current terminal environment supports this graphics protocol.
    fn is_supported(&self) -> bool;

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
}

/// Detects and returns an active graphics backend, or fails with a diagnostic message.
pub fn select_backend() -> Result<Box<dyn GraphicsBackend>> {
    let kitty = KittyBackend::new();
    if kitty.is_supported() {
        return Ok(Box::new(kitty));
    }

    bail!(
        "High-resolution graphics protocol not detected in current terminal.\n\
             Imvic requires native GPU graphics rendering to guarantee pin-sharp vector display.\n\n\
             Currently supported terminal:\n\
               - Kitty (https://sw.kovidgoyal.net/kitty/) [standalone or inside TMUX]\n\n\
             Note: Support for other high-resolution terminals (Ghostty, WezTerm) is planned\n\
             for future releases as modular backend plugins."
    )
}
