//! Heads-Up Display (HUD) status bar and syntax error warning layer.
//!
//! Renders real-time camera coordinates, zoom factor, and non-fatal
//! syntax corruption alerts on the terminal's bottom status line.

use anyhow::Result;
use std::io::Write;

/// State representation for the bottom HUD status bar.
#[derive(Debug, Clone)]
pub struct HudState {
    pub zoom_factor: f32,
    pub offset: (f32, f32),
    pub dimensions: (u32, u32),
    pub error_message: Option<String>,
}

impl HudState {
    pub fn new(dimensions: (u32, u32)) -> Self {
        Self {
            zoom_factor: 1.0,
            offset: (0.0, 0.0),
            dimensions,
            error_message: None,
        }
    }

    pub fn set_error(&mut self, msg: String) {
        self.error_message = Some(msg);
    }

    pub fn clear_error(&mut self) {
        self.error_message = None;
    }

    /// Formats and writes the HUD bar to the given writer at the bottom row.
    pub fn render(&self, writer: &mut dyn Write, rows: u16, cols: u16) -> Result<()> {
        if rows == 0 || cols == 0 {
            return Ok(());
        }

        // Position cursor at beginning of the bottom row (\x1b[{rows};1H)
        // and clear entire line (\x1b[2K)
        write!(writer, "\x1b[{};1H\x1b[2K", rows)?;

        // Ensure we never write to the bottom-right terminal cell (rows, cols)
        // to prevent terminals from triggering an unwanted auto-scroll line wrap.
        let max_cols = cols.saturating_sub(1).max(1) as usize;

        if let Some(err) = &self.error_message {
            // Error alert style: bold red background
            let error_text = format!(
                " [ERROR] {} | Preserving last valid frame | 'q' to quit",
                err
            );
            let truncated = truncate_str(&error_text, max_cols);
            write!(writer, "\x1b[1;37;41m{}\x1b[0m", truncated)?;
        } else {
            // Normal status style: inverted dark bar
            let zoom_percent = (self.zoom_factor * 100.0).round() as u32;
            let (w, h) = self.dimensions;
            let (ox, oy) = (self.offset.0.round() as i32, self.offset.1.round() as i32);

            let status_text = format!(
                " [IMAGE] {}x{} | Zoom: {}% | Pos: ({}, {}) | 'c' to center | 'q' to quit",
                w, h, zoom_percent, ox, oy
            );
            let truncated = truncate_str(&status_text, max_cols);
            write!(writer, "\x1b[7m{}\x1b[0m", truncated)?;
        }

        Ok(())
    }
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        format!("{:<width$}", s, width = max_len)
    } else {
        let mut truncated: String = s.chars().take(max_len.saturating_sub(3)).collect();
        truncated.push_str("...");
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hud_rendering_normal() {
        let mut hud = HudState::new((1000, 800));
        hud.zoom_factor = 1.5;
        hud.offset = (10.0, -20.0);

        let mut output = Vec::new();
        let res = hud.render(&mut output, 40, 80);
        assert!(res.is_ok());

        let text = String::from_utf8_lossy(&output);
        assert!(text.contains("1000x800"));
        assert!(text.contains("150%"));
    }

    #[test]
    fn test_hud_rendering_error() {
        let mut hud = HudState::new((1000, 800));
        hud.set_error("XML closing tag missing".to_string());

        let mut output = Vec::new();
        let res = hud.render(&mut output, 40, 80);
        assert!(res.is_ok());

        let text = String::from_utf8_lossy(&output);
        assert!(text.contains("[ERROR]"));
        assert!(text.contains("XML closing tag missing"));
        assert!(text.contains("Preserving last valid frame"));
    }
}
