//! Command-line argument parsing and configuration specification for `imvic`.
//!
//! Defines the structure and flags accepted by the application at startup.

use clap::Parser;
use std::path::PathBuf;

/// High-performance, modular, GPU-accelerated terminal image and vector viewer
#[derive(Parser, Debug, Clone)]
#[command(
    name = "imvic",
    version,
    about = "High-performance, modular, GPU-accelerated terminal image and vector viewer."
)]
pub struct CliArgs {
    /// Path to the image or vector file to display
    #[arg(value_name = "FILE")]
    pub file: PathBuf,

    /// Watch file for changes and reload view automatically (Live Reload)
    #[arg(short = 'w', long = "watch", default_value_t = true)]
    pub watch: bool,

    /// Initial zoom or scale factor override
    #[arg(short = 's', long = "scale")]
    pub scale: Option<f32>,

    /// Optional canvas background color in hex format (#RGB, #RGBA, #RRGGBB, #RRGGBBAA).
    /// If passed without a value or with an invalid format, falls back to white.
    #[arg(
            long = "bg",
            value_name = "COLOR",
            num_args = 0..=1,
            default_missing_value = "default",
            require_equals = true
        )]
    pub bg: Option<String>,

    /// Target framerate limit in FPS (e.g. 30, 60, 144). Defaults to uncapped/native.
    #[arg(long = "fps", value_name = "FPS")]
    pub fps: Option<u32>,

    /// Print verbose debugging traces and terminal protocol events
    #[arg(short = 'v', long = "verbose")]
    pub verbose: bool,
}

/// Default solid white canvas background fallback [R, G, B, A].
pub const DEFAULT_FALLBACK_BG: [u8; 4] = [255, 255, 255, 255];

/// Parses an optional CLI background string into an RGBA byte array.
///
/// Accepts `#RGB`, `#RGBA`, `#RRGGBB`, and `#RRGGBBAA`.
/// Falls back to solid white `[255, 255, 255, 255]` if `#` is missing or hex is invalid.
/// Returns `None` if no background argument was passed.
pub fn parse_bg_color(arg: Option<&str>) -> Option<[u8; 4]> {
    let raw = arg?.trim();

    // Standard fallback to solid white if '#' prefix is missing
    if !raw.starts_with('#') {
        return Some(DEFAULT_FALLBACK_BG);
    }

    let hex = &raw[1..];
    match hex.len() {
        3 => {
            // #RGB -> #RRGGBB
            let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
            Some([r, g, b, 255])
        }
        4 => {
            // #RGBA -> #RRGGBBAA
            let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
            let a = u8::from_str_radix(&hex[3..4].repeat(2), 16).ok()?;
            Some([r, g, b, a])
        }
        6 => {
            // #RRGGBB
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some([r, g, b, 255])
        }
        8 => {
            // #RRGGBBAA
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
            Some([r, g, b, a])
        }
        // Fallback to white on any invalid length
        _ => Some(DEFAULT_FALLBACK_BG),
    }
    .or(Some(DEFAULT_FALLBACK_BG))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bg_color() {
        // No argument -> transparent
        assert_eq!(parse_bg_color(None), None);

        // Missing '#' prefix or invalid -> fallback white
        assert_eq!(parse_bg_color(Some("default")), Some(DEFAULT_FALLBACK_BG));
        assert_eq!(parse_bg_color(Some("white")), Some(DEFAULT_FALLBACK_BG));
        assert_eq!(parse_bg_color(Some("invalid")), Some(DEFAULT_FALLBACK_BG));

        // #RGB
        assert_eq!(parse_bg_color(Some("#fff")), Some([255, 255, 255, 255]));
        assert_eq!(parse_bg_color(Some("#000")), Some([0, 0, 0, 255]));

        // #RGBA
        assert_eq!(parse_bg_color(Some("#0000")), Some([0, 0, 0, 0]));

        // #RRGGBB
        assert_eq!(parse_bg_color(Some("#1e1e2e")), Some([30, 30, 46, 255]));

        // #RRGGBBAA
        assert_eq!(parse_bg_color(Some("#1e1e2eff")), Some([30, 30, 46, 255]));
        assert_eq!(parse_bg_color(Some("#00000000")), Some([0, 0, 0, 0]));
    }
}
