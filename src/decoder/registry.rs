//! Decoder registry and automatic format detection dispatcher.
//!
//! Inspects file headers and extensions to route files to the correct decoder plugin.

use super::formats::raster::RasterDecoder;
use super::formats::svg::SvgDecoder;
use super::{FormatDecoder, ImageSource};
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

    /// Inspects file path and initial header bytes to locate a capable decoder.
    pub fn find_decoder(&self, path: &Path) -> Result<&dyn FormatDecoder> {
        let mut file = File::open(path).with_context(|| format!("Failed to open {:?}", path))?;

        let mut header = [0u8; HEADER_PROBE_SIZE];
        let bytes_read = file
            .read(&mut header)
            .with_context(|| format!("Failed to read header from {:?}", path))?;

        let valid_header = &header[..bytes_read];

        for decoder in &self.decoders {
            if decoder.can_decode(path, valid_header) {
                return Ok(decoder.as_ref());
            }
        }

        bail!(
            "Unsupported file format for {:?}. No decoder plugin recognized the file extension or header.",
            path
        )
    }

    /// Finds the appropriate decoder and decodes the file into an in-memory image source.
    pub fn decode(&self, path: &Path) -> Result<Arc<dyn ImageSource>> {
        let decoder = self.find_decoder(path)?;
        decoder.decode(path).with_context(|| {
            format!(
                "Failed to decode {:?} using plugin '{}'",
                path,
                decoder.name()
            )
        })
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
}
