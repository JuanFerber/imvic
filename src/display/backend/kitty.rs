//! Kitty Graphics Protocol backend with 4096-byte chunking and in-memory PNG compression.
//!
//! Transmits high-resolution images directly to the terminal GPU using APC escape sequences.

use super::GraphicsBackend;
use crate::display::transport::TransportAdapter;
use anyhow::{Context, Result};
use base64::prelude::*;
use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder, RgbaImage};
use std::io::Write;

const CHUNK_SIZE: usize = 4096;

/// Graphics backend targeting Kitty, Ghostty, and WezTerm via the Kitty Graphics Protocol.
pub struct KittyBackend;

impl KittyBackend {
    pub fn new() -> Self {
        Self
    }
}

impl Default for KittyBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphicsBackend for KittyBackend {
    fn name(&self) -> &'static str {
        "Kitty Graphics Protocol"
    }

    fn is_supported(&self) -> bool {
        // Check standard Kitty environment variables
        if std::env::var_os("KITTY_WINDOW_ID").is_some() || std::env::var_os("KITTY_PID").is_some()
        {
            return true;
        }

        // Check if TERM identifies as kitty
        std::env::var("TERM")
            .map(|term| term.to_ascii_lowercase().contains("kitty"))
            .unwrap_or(false)
    }

    fn draw_image(
        &mut self,
        writer: &mut dyn Write,
        transport: &dyn TransportAdapter,
        frame: &RgbaImage,
        target_cells: (u16, u16),
    ) -> Result<()> {
        if frame.is_empty() {
            return Ok(());
        }

        // 1. Encode raw RGBA bytes into PNG in memory with zero unnecessary frame clones
        let mut png_bytes = Vec::new();
        let encoder = PngEncoder::new(&mut png_bytes);
        encoder
            .write_image(
                frame.as_raw(),
                frame.width(),
                frame.height(),
                ExtendedColorType::Rgba8,
            )
            .context("Failed to encode frame to in-memory PNG")?;

        // 2. Base64 encode PNG payload
        let b64 = BASE64_STANDARD.encode(&png_bytes);
        let b64_bytes = b64.as_bytes();
        let total_len = b64_bytes.len();

        let (cols, rows) = target_cells;

        // 3. Transmit in chunks of at most 4096 bytes
        let mut offset = 0;
        let mut is_first = true;

        while offset < total_len {
            let end = (offset + CHUNK_SIZE).min(total_len);
            let chunk = &b64_bytes[offset..end];
            let is_last = end == total_len;
            let m = if is_last { 0 } else { 1 };

            let raw_escape = if is_first {
                // First chunk includes control headers
                format!(
                    "\x1b_Ga=T,f=100,t=d,c={},r={},q=2,m={};{}\x1b\\",
                    cols,
                    rows,
                    m,
                    std::str::from_utf8(chunk).unwrap_or("")
                )
            } else {
                // Subsequent chunks only need the continuation flag
                format!(
                    "\x1b_Gm={};{}\x1b\\",
                    m,
                    std::str::from_utf8(chunk).unwrap_or("")
                )
            };

            let wrapped = transport.wrap_escape(raw_escape.as_bytes());
            writer.write_all(&wrapped)?;

            is_first = false;
            offset = end;
        }

        writer.flush()?;
        Ok(())
    }

    fn clear_graphics(
        &mut self,
        writer: &mut dyn Write,
        transport: &dyn TransportAdapter,
    ) -> Result<()> {
        let clear_cmd = b"\x1b_Ga=d,d=a\x1b\\";
        let wrapped = transport.wrap_escape(clear_cmd);
        writer.write_all(&wrapped)?;
        writer.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::transport::DirectTransport;

    #[test]
    fn test_kitty_chunking() {
        let mut backend = KittyBackend::new();
        let transport = DirectTransport::new();
        let frame = RgbaImage::new(10, 10);
        let mut output = Vec::new();

        let result = backend.draw_image(&mut output, &transport, &frame, (40, 20));
        assert!(result.is_ok());
        assert!(!output.is_empty());
        assert!(output.starts_with(b"\x1b_Ga=T,f=100"));
        assert!(output.ends_with(b"\x1b\\"));
    }

    #[test]
    fn test_kitty_clear_graphics() {
        let mut backend = KittyBackend::new();
        let transport = DirectTransport::new();
        let mut output = Vec::new();

        let result = backend.clear_graphics(&mut output, &transport);
        assert!(result.is_ok());
        assert_eq!(output, b"\x1b_Ga=d,d=a\x1b\\");
    }
}
