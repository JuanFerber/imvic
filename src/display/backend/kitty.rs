//! Kitty Graphics Protocol backend with 4096-byte chunking and in-memory PNG compression.
//!
//! Transmits high-resolution images to terminal emulators with native graphics presentation using APC escape sequences.

use super::GraphicsBackend;
use crate::display::transport::TransportAdapter;
use anyhow::{Context, Result};
use base64::prelude::*;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder, RgbaImage};
use std::io::Write;

const CHUNK_SIZE: usize = 4096;
const INITIAL_PNG_CAPACITY: usize = 256 * 1024;
const INITIAL_B64_CAPACITY: usize = 384 * 1024;

/// Graphics backend implementing the Kitty Graphics Protocol.
#[derive(Clone)]
pub struct KittyBackend {
    /// Preallocated scratch buffer for in-memory PNG compression.
    png_buffer: Vec<u8>,
    /// Preallocated scratch buffer for Base64 payload encoding.
    b64_buffer: String,
    /// Alternating image identifier (1 or 2) to eliminate visual flicker during frame replacement.
    current_image_id: u32,
}

impl KittyBackend {
    pub fn new() -> Self {
        Self {
            png_buffer: Vec::with_capacity(INITIAL_PNG_CAPACITY),
            b64_buffer: String::with_capacity(INITIAL_B64_CAPACITY),
            current_image_id: 1,
        }
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

    fn is_detected(&self) -> bool {
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

    fn is_protocol_available(&self) -> bool {
        self.is_detected()
    }

    fn clone_box(&self) -> Box<dyn GraphicsBackend> {
        Box::new(self.clone())
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

        // 1. Reuse scratch buffer to encode raw RGBA bytes into PNG with zero allocations
        self.png_buffer.clear();
        let encoder = PngEncoder::new_with_quality(
            &mut self.png_buffer,
            CompressionType::Fast,
            FilterType::NoFilter,
        );
        encoder
            .write_image(
                frame.as_raw(),
                frame.width(),
                frame.height(),
                ExtendedColorType::Rgba8,
            )
            .context("Failed to encode frame to in-memory PNG")?;

        /// Kitty Unicode Placeholder combining diacritics mapping for rows and columns (0..296).
        ///
        /// Each cell coordinate (row or col) is mapped to its corresponding combining diacritic
        /// following the official Kitty Graphics Protocol specification.
        #[rustfmt::skip]
        static DIACRITICS: [char; 297] = [
            '\u{0305}', '\u{030D}', '\u{030E}', '\u{0310}', '\u{0312}', '\u{033D}', '\u{033E}', '\u{033F}',
            '\u{0346}', '\u{034A}', '\u{034B}', '\u{034C}', '\u{0350}', '\u{0351}', '\u{0352}', '\u{0357}',
            '\u{035B}', '\u{0363}', '\u{0364}', '\u{0365}', '\u{0366}', '\u{0367}', '\u{0368}', '\u{0369}',
            '\u{036A}', '\u{036B}', '\u{036C}', '\u{036D}', '\u{036E}', '\u{036F}', '\u{0483}', '\u{0484}',
            '\u{0485}', '\u{0486}', '\u{0487}', '\u{0592}', '\u{0593}', '\u{0594}', '\u{0595}', '\u{0597}',
            '\u{0598}', '\u{0599}', '\u{059C}', '\u{059D}', '\u{059E}', '\u{059F}', '\u{05A0}', '\u{05A1}',
            '\u{05A8}', '\u{05A9}', '\u{05AB}', '\u{05AC}', '\u{05AF}', '\u{05C4}', '\u{0610}', '\u{0611}',
            '\u{0612}', '\u{0613}', '\u{0614}', '\u{0615}', '\u{0616}', '\u{0617}', '\u{0657}', '\u{0658}',
            '\u{0659}', '\u{065A}', '\u{065B}', '\u{065D}', '\u{065E}', '\u{06D6}', '\u{06D7}', '\u{06D8}',
            '\u{06D9}', '\u{06DA}', '\u{06DB}', '\u{06DC}', '\u{06DF}', '\u{06E0}', '\u{06E1}', '\u{06E2}',
            '\u{06E4}', '\u{06E7}', '\u{06E8}', '\u{06EB}', '\u{06EC}', '\u{0730}', '\u{0732}', '\u{0733}',
            '\u{0735}', '\u{0736}', '\u{073A}', '\u{073D}', '\u{073F}', '\u{0740}', '\u{0741}', '\u{0743}',
            '\u{0745}', '\u{0747}', '\u{0749}', '\u{074A}', '\u{07EB}', '\u{07EC}', '\u{07ED}', '\u{07EE}',
            '\u{07EF}', '\u{07F0}', '\u{07F1}', '\u{07F3}', '\u{0816}', '\u{0817}', '\u{0818}', '\u{0819}',
            '\u{081B}', '\u{081C}', '\u{081D}', '\u{081E}', '\u{081F}', '\u{0820}', '\u{0821}', '\u{0822}',
            '\u{0823}', '\u{0825}', '\u{0826}', '\u{0827}', '\u{0829}', '\u{082A}', '\u{082B}', '\u{082C}',
            '\u{082D}', '\u{0951}', '\u{0953}', '\u{0954}', '\u{0F82}', '\u{0F83}', '\u{0F86}', '\u{0F87}',
            '\u{135D}', '\u{135E}', '\u{135F}', '\u{17DD}', '\u{193A}', '\u{1A17}', '\u{1A75}', '\u{1A76}',
            '\u{1A77}', '\u{1A78}', '\u{1A79}', '\u{1A7A}', '\u{1A7B}', '\u{1A7C}', '\u{1B6B}', '\u{1B6D}',
            '\u{1B6E}', '\u{1B6F}', '\u{1B70}', '\u{1B71}', '\u{1B72}', '\u{1B73}', '\u{1CD0}', '\u{1CD1}',
            '\u{1CD2}', '\u{1CDA}', '\u{1CDB}', '\u{1CE0}', '\u{1DC0}', '\u{1DC1}', '\u{1DC3}', '\u{1DC4}',
            '\u{1DC5}', '\u{1DC6}', '\u{1DC7}', '\u{1DC8}', '\u{1DC9}', '\u{1DCB}', '\u{1DCC}', '\u{1DD1}',
            '\u{1DD2}', '\u{1DD3}', '\u{1DD4}', '\u{1DD5}', '\u{1DD6}', '\u{1DD7}', '\u{1DD8}', '\u{1DD9}',
            '\u{1DDA}', '\u{1DDB}', '\u{1DDC}', '\u{1DDD}', '\u{1DDE}', '\u{1DDF}', '\u{1DE0}', '\u{1DE1}',
            '\u{1DE2}', '\u{1DE3}', '\u{1DE4}', '\u{1DE5}', '\u{1DE6}', '\u{1DFE}', '\u{20D0}', '\u{20D1}',
            '\u{20D4}', '\u{20D5}', '\u{20D6}', '\u{20D7}', '\u{20DB}', '\u{20DC}', '\u{20E1}', '\u{20E7}',
            '\u{20E9}', '\u{20F0}', '\u{2CEF}', '\u{2CF0}', '\u{2CF1}', '\u{2DE0}', '\u{2DE1}', '\u{2DE2}',
            '\u{2DE3}', '\u{2DE4}', '\u{2DE5}', '\u{2DE6}', '\u{2DE7}', '\u{2DE8}', '\u{2DE9}', '\u{2DEA}',
            '\u{2DEB}', '\u{2DEC}', '\u{2DED}', '\u{2DEE}', '\u{2DEF}', '\u{2DF0}', '\u{2DF1}', '\u{2DF2}',
            '\u{2DF3}', '\u{2DF4}', '\u{2DF5}', '\u{2DF6}', '\u{2DF7}', '\u{2DF8}', '\u{2DF9}', '\u{2DFA}',
            '\u{2DFB}', '\u{2DFC}', '\u{2DFD}', '\u{2DFE}', '\u{2DFF}', '\u{A66F}', '\u{A67C}', '\u{A67D}',
            '\u{A6F0}', '\u{A6F1}', '\u{A8E0}', '\u{A8E1}', '\u{A8E2}', '\u{A8E3}', '\u{A8E4}', '\u{A8E5}',
            '\u{A8E6}', '\u{A8E7}', '\u{A8E8}', '\u{A8E9}', '\u{A8EA}', '\u{A8EB}', '\u{A8EC}', '\u{A8ED}',
            '\u{A8EE}', '\u{A8EF}', '\u{A8F0}', '\u{A8F1}', '\u{AAB0}', '\u{AAB2}', '\u{AAB3}', '\u{AAB7}',
            '\u{AAB8}', '\u{AABE}', '\u{AABF}', '\u{AAC1}', '\u{FE20}', '\u{FE21}', '\u{FE22}', '\u{FE23}',
            '\u{FE24}', '\u{FE25}', '\u{FE26}', '\u{10A0F}', '\u{10A38}', '\u{1D185}', '\u{1D186}',
            '\u{1D187}', '\u{1D188}', '\u{1D189}', '\u{1D1AA}', '\u{1D1AB}', '\u{1D1AC}', '\u{1D1AD}',
            '\u{1D242}', '\u{1D243}', '\u{1D244}',
        ];

        let (cols, rows) = target_cells;
        if cols == 0 || rows == 0 {
            return Ok(());
        }

        // 2. Reuse scratch string for Base64 payload encoding
        self.b64_buffer.clear();
        BASE64_STANDARD.encode_string(&self.png_buffer, &mut self.b64_buffer);
        let b64_bytes = self.b64_buffer.as_bytes();
        let total_len = b64_bytes.len();

        let is_mux = transport.is_multiplexer();

        // 3. Double-buffered flicker-free rendering: alternate image IDs
        let new_id = if self.current_image_id == 1 { 2 } else { 1 };
        let old_id = self.current_image_id;

        // In direct terminal mode (no multiplexer), position cursor at (1, 1).
        // Under multiplexers, positioning is handled cleanly via text grid placeholders.
        if !is_mux {
            let (phys_row, phys_col) = transport.physical_origin();
            let prep_cmd = format!("\x1b[{};{}H", phys_row, phys_col);
            writer.write_all(prep_cmd.as_bytes())?;
        }

        let mut offset = 0;
        let mut is_first = true;
        while offset < total_len {
            let end = (offset + CHUNK_SIZE).min(total_len);
            let chunk = &b64_bytes[offset..end];
            let is_last = end == total_len;
            let m = if is_last { 0 } else { 1 };

            let raw_escape = if is_first {
                if is_mux {
                    // Virtual placement (U=1) for multiplexers: image is stored in Kitty terminal memory
                    // without drawing at arbitrary coordinates, anchored strictly to text buffer cells.
                    format!(
                        "\x1b_Ga=T,f=100,t=d,i={},U=1,C=1,c={},r={},q=2,m={};{}\x1b\\",
                        new_id,
                        cols,
                        rows,
                        m,
                        std::str::from_utf8(chunk).unwrap_or("")
                    )
                } else {
                    format!(
                        "\x1b_Ga=T,f=100,t=d,i={},C=1,c={},r={},q=2,m={};{}\x1b\\",
                        new_id,
                        cols,
                        rows,
                        m,
                        std::str::from_utf8(chunk).unwrap_or("")
                    )
                }
            } else {
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

        // When running inside a multiplexer, emit Unicode placeholders (U+10EEEE).
        // Each placeholder cell carries the image ID in its 24-bit TrueColor foreground,
        // and row/column grid indices encoded via combining diacritics.
        // The multiplexer treats these as normal pane text, enforcing strict pane boundaries and tab isolation.
        if is_mux {
            let r = (new_id >> 16) & 0xff;
            let g = (new_id >> 8) & 0xff;
            let b = new_id & 0xff;
            write!(writer, "\x1b[38;2;{};{};{}m", r, g, b)?;

            for y in 0..rows {
                write!(writer, "\x1b[{};1H", y + 1)?;
                let row_char = DIACRITICS.get(y as usize).copied().unwrap_or(DIACRITICS[0]);
                for x in 0..cols {
                    let col_char = DIACRITICS.get(x as usize).copied().unwrap_or(DIACRITICS[0]);
                    write!(writer, "\u{10EEEE}{}{}", row_char, col_char)?;
                }
            }
            write!(writer, "\x1b[0m")?;
        }

        // Delete the previous graphic instance only AFTER the new frame is placed on screen
        let del_cmd = format!("\x1b_Ga=d,d=i,i={}\x1b\\", old_id);
        let wrapped_del = transport.wrap_escape(del_cmd.as_bytes());
        writer.write_all(&wrapped_del)?;

        self.current_image_id = new_id;
        Ok(())
    }

    fn clear_graphics(
        &mut self,
        writer: &mut dyn Write,
        transport: &dyn TransportAdapter,
    ) -> Result<()> {
        let clear_cmd = b"\x1b_Ga=d,d=A\x1b\\";
        let wrapped_clear = transport.wrap_escape(clear_cmd);
        writer.write_all(&wrapped_clear)?;
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
        assert!(output.starts_with(b"\x1b[1;1H"));
        assert!(output.windows(9).any(|w| w == b"a=T,f=100"));
        assert!(output.ends_with(b"\x1b\\"));
    }

    #[test]
    fn test_kitty_clear_graphics() {
        let mut backend = KittyBackend::new();
        let transport = DirectTransport::new();
        let mut output = Vec::new();

        let result = backend.clear_graphics(&mut output, &transport);
        assert!(result.is_ok());
        assert_eq!(output, b"\x1b_Ga=d,d=A\x1b\\");
    }

    #[derive(Clone, Copy)]
    struct MockMultiplexerTransport;

    impl TransportAdapter for MockMultiplexerTransport {
        fn name(&self) -> &'static str {
            "Mock Multiplexer"
        }
        fn is_active(&self) -> bool {
            true
        }
        fn is_multiplexer(&self) -> bool {
            true
        }
        fn is_passthrough_enabled(&self) -> bool {
            true
        }
        fn passthrough_enable_hint(&self) -> Option<&'static str> {
            None
        }
        fn wrap_escape(&self, raw: &[u8]) -> Vec<u8> {
            raw.to_vec()
        }
        fn clone_box(&self) -> Box<dyn TransportAdapter> {
            Box::new(*self)
        }
    }

    #[test]
    fn test_kitty_unicode_placeholders_with_multiplexer() {
        let mut backend = KittyBackend::new();
        let transport = MockMultiplexerTransport;
        let frame = RgbaImage::new(10, 10);
        let mut output = Vec::new();

        let result = backend.draw_image(&mut output, &transport, &frame, (4, 2));
        assert!(result.is_ok());
        let out_str = String::from_utf8_lossy(&output);
        assert!(out_str.contains("U=1"));
        assert!(out_str.contains('\u{10EEEE}'));
    }
}
