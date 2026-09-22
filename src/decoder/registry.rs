//! Decoder registry and automatic format detection dispatcher.
//!
//! Inspects file headers and extensions to route files to the correct decoder plugin.

use super::formats::raster::RasterDecoder;
use super::formats::svg::SvgDecoder;
use super::{DecodeMatch, FormatDecoder, ImageSource};
use anyhow::{Context, Result, bail};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

/// Number of initial header bytes probed to sniff magic signatures (e.g. `<?xml`, `<svg`, PNG header).
const HEADER_PROBE_SIZE: usize = 512;

/// Central registry managing all registered image format decoders.
pub struct DecoderRegistry {
    decoders: Vec<Box<dyn FormatDecoder>>,
}

impl DecoderRegistry {
    /// Creates a registry initialized with all built-in format decoders.
    pub fn new() -> Self {
        let mut registry = Self {
            decoders: Vec::new(),
        };

        // Register default plugins
        registry.register(Box::new(SvgDecoder::new()));
        registry.register(Box::new(RasterDecoder::new()));

        registry
    }

    /// Registers an additional format decoder plugin.
    pub fn register(&mut self, decoder: Box<dyn FormatDecoder>) {
        self.decoders.push(decoder);
    }

    /// Inspects file path and initial header bytes to locate the best matching decoder.
    pub fn find_decoder(&self, path: &Path) -> Result<&dyn FormatDecoder> {
        let mut file = File::open(path).with_context(|| format!("Failed to open {:?}", path))?;

        let mut header = [0u8; HEADER_PROBE_SIZE];
        let bytes_read = file
            .read(&mut header)
            .with_context(|| format!("Failed to read header from {:?}", path))?;

        let valid_header = &header[..bytes_read];

        // Pick decoder with highest match score
        let best_decoder = self
            .decoders
            .iter()
            .map(|d| (d.as_ref(), d.match_score(path, valid_header)))
            .filter(|(_, score)| *score > DecodeMatch::None)
            .max_by_key(|(_, score)| *score);

        if let Some((decoder, _)) = best_decoder {
            Ok(decoder)
        } else {
            bail!(
                "Unsupported file format for {:?}. No decoder plugin recognized the file extension or header.",
                path
            )
        }
    }

    /// Finds candidate decoders sorted by confidence and decodes the file with fallback.
    pub fn decode(&self, path: &Path) -> Result<Arc<dyn ImageSource>> {
        let mut file = File::open(path).with_context(|| format!("Failed to open {:?}", path))?;

        let mut header = [0u8; HEADER_PROBE_SIZE];
        let bytes_read = file
            .read(&mut header)
            .with_context(|| format!("Failed to read header from {:?}", path))?;

        let valid_header = &header[..bytes_read];

        // Gather all matching decoders and sort descending by confidence (MagicBytes before ExtensionOnly)
        let mut candidates: Vec<(&dyn FormatDecoder, DecodeMatch)> = self
            .decoders
            .iter()
            .map(|d| (d.as_ref(), d.match_score(path, valid_header)))
            .filter(|(_, score)| *score > DecodeMatch::None)
            .collect();

        candidates.sort_by_key(|a| std::cmp::Reverse(a.1));

        if candidates.is_empty() {
            bail!(
                "Unsupported file format for {:?}. No decoder plugin recognized the file extension or header.",
                path
            );
        }

        // Attempt decoding in order of match quality (cascade fallback)
        let mut last_error = None;
        for (decoder, _) in candidates {
            match decoder.decode(path) {
                Ok(source) => return Ok(source),
                Err(err) => {
                    last_error = Some(err.context(format!(
                        "Plugin '{}' failed to decode {:?}",
                        decoder.name(),
                        path
                    )));
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Decoding failed for {:?}", path)))
    }
}

impl Default for DecoderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_initialization() {
        let registry = DecoderRegistry::new();
        assert!(!registry.decoders.is_empty());
    }

    #[test]
    fn test_registry_finds_svg() {
        let registry = DecoderRegistry::new();
        let temp_path = std::env::temp_dir().join("imvic_test_drawing.svg");
        std::fs::write(&temp_path, b"<svg></svg>").unwrap();

        let decoder = registry.find_decoder(&temp_path);
        assert!(decoder.is_ok());
        assert_eq!(decoder.unwrap().name(), "SVG Vector Decoder");

        let _ = std::fs::remove_file(&temp_path);
    }

    #[test]
    fn test_registry_unsupported_format() {
        let registry = DecoderRegistry::new();
        let temp_path = std::env::temp_dir().join("imvic_unsupported.xyz");
        std::fs::write(&temp_path, b"unsupported raw content").unwrap();

        let result = registry.find_decoder(&temp_path);
        let Err(err) = result else {
            panic!("Expected unsupported format error");
        };
        assert!(err.to_string().contains("Unsupported file format"));

        let _ = std::fs::remove_file(&temp_path);
    }

    #[test]
    fn test_registry_nonexistent_file() {
        let registry = DecoderRegistry::new();
        let path = Path::new("definitely_nonexistent_file_12345.xyz");
        let result = registry.find_decoder(path);
        let Err(err) = result else {
            panic!("Expected I/O open error");
        };
        let err_msg = err.to_string();
        assert!(err_msg.contains("Failed to open"));
        assert!(!err_msg.contains("Unsupported file format"));
    }

    #[test]
    fn test_registry_svg_named_as_png_falls_back_correctly() {
        let registry = DecoderRegistry::new();
        let temp_path = std::env::temp_dir().join("imvic_trick_drawing.png");
        // Archivo SVG válido pero con extensión .png
        std::fs::write(
            &temp_path,
            b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100\" height=\"100\"></svg>",
        )
        .unwrap();

        let source = registry.decode(&temp_path);
        assert!(
            source.is_ok(),
            "Failed to decode SVG with .png extension: {:?}",
            source.err()
        );

        let _ = std::fs::remove_file(&temp_path);
    }

    #[test]
    fn test_registry_png_named_as_svg_falls_back_correctly() {
        let registry = DecoderRegistry::new();
        let temp_path = std::env::temp_dir().join("imvic_trick_raster.svg");
        // PNG de 1x1 píxel válido pero con extensión .svg
        let png_1x1 = [
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        std::fs::write(&temp_path, png_1x1).unwrap();

        let source = registry.decode(&temp_path);
        assert!(
            source.is_ok(),
            "Failed to decode PNG with .svg extension: {:?}",
            source.err()
        );

        let _ = std::fs::remove_file(&temp_path);
    }
}
