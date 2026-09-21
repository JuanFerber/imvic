//! Command-line argument parsing and configuration specification for `imvic`.
//!
//! Defines the structure and flags accepted by the application at startup.

use clap::Parser;
use std::path::PathBuf;

/// High-performance, modular terminal image viewer with terminal-native graphics presentation.
#[derive(Parser, Debug)]
#[command(
    name = "imvic",
    version,
    about = "High-performance, modular terminal image viewer with terminal-native graphics presentation."
)]
pub struct CliArgs {
    /// Path to the image or vector file to display
    #[arg(value_name = "FILE")]
    pub file: PathBuf,

    /// Watch file for changes and reload view automatically (Live Reload)
    #[arg(short = 'w', long = "watch")]
    pub watch: bool,

    /// Initial zoom or scale factor override
    #[arg(short = 's', long = "scale", value_parser = parse_scale)]
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
    #[arg(long = "fps", value_name = "FPS", value_parser = parse_fps)]
    pub fps: Option<u32>,
}

/// Default solid white canvas background fallback [R, G, B, A].
pub const DEFAULT_FALLBACK_BG: [u8; 4] = [255, 255, 255, 255];

/// Parses and validates the CLI scale factor.
///
/// Ensures the scale multiplier is strictly positive and finite.
pub fn parse_scale(val: &str) -> Result<f32, String> {
    let scale: f32 = val
        .parse()
        .map_err(|_| format!("Invalid scale value '{}': must be a valid number", val))?;

    if !scale.is_finite() || scale <= 0.0 {
        return Err(format!(
            "Invalid scale factor '{}': scale must be a finite number greater than 0",
            val
        ));
    }

    Ok(scale)
}

/// Parses and validates the CLI framerate limit.
///
/// Ensures the framerate is strictly positive and within sane display boundaries (1..=240).
pub fn parse_fps(val: &str) -> Result<u32, String> {
    let fps: u32 = val
        .parse()
        .map_err(|_| format!("Invalid FPS value '{}': must be a positive integer", val))?;

    if fps == 0 || fps > 240 {
        return Err(format!(
            "Invalid framerate '{}': FPS must be between 1 and 240",
            val
        ));
    }

    Ok(fps)
}

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
    let parsed: Option<[u8; 4]> = (|| {
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
            _ => None,
        }
    })();

    Some(parsed.unwrap_or(DEFAULT_FALLBACK_BG))
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
        assert_eq!(parse_bg_color(Some("#ggg")), Some(DEFAULT_FALLBACK_BG));
        assert_eq!(parse_bg_color(Some("#12")), Some(DEFAULT_FALLBACK_BG));
        assert_eq!(parse_bg_color(Some("#zzzzzz")), Some(DEFAULT_FALLBACK_BG));

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

    #[test]
    fn test_parse_scale_valid() {
        assert_eq!(parse_scale("1.0"), Ok(1.0));
        assert_eq!(parse_scale("0.5"), Ok(0.5));
        assert_eq!(parse_scale("10"), Ok(10.0));
    }

    #[test]
    fn test_parse_scale_invalid() {
        assert!(parse_scale("0").is_err());
        assert!(parse_scale("-1.5").is_err());
        assert!(parse_scale("NaN").is_err());
        assert!(parse_scale("inf").is_err());
        assert!(parse_scale("invalid").is_err());
    }

    #[test]
    fn test_parse_fps_valid() {
        assert_eq!(parse_fps("30"), Ok(30));
        assert_eq!(parse_fps("60"), Ok(60));
        assert_eq!(parse_fps("144"), Ok(144));
        assert_eq!(parse_fps("240"), Ok(240));
    }

    #[test]
    fn test_parse_fps_invalid() {
        assert!(parse_fps("0").is_err());
        assert!(parse_fps("241").is_err());
        assert!(parse_fps("-60").is_err());
        assert!(parse_fps("invalid").is_err());
    }
}
