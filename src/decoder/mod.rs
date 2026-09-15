//! Image format decoding plugin contracts and abstraction layer.
//!
//! Provides traits and types for extensible, zero-cost image decoding.

use anyhow::Result;
use image::RgbaImage;
use std::path::Path;
use std::sync::Arc;

pub mod formats;
pub mod registry;
pub use registry::DecoderRegistry;

/// Rectangular crop area in source image coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CropRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Abstract contract for a loaded image surface in memory.
pub trait ImageSource: Send + Sync {
    /// Returns natural width and height of the image in source pixels.
    fn dimensions(&self) -> (u32, u32);

    /// Renders a sub-region (crop) adapted to target pixel dimensions.
    fn render_crop(&self, crop: CropRect, target_w: u32, target_h: u32) -> RgbaImage;
}

/// Contract for image format decoder plugins.
pub trait FormatDecoder: Send + Sync {
    /// Human-readable plugin name (e.g. "SVG Vector Decoder").
    fn name(&self) -> &'static str;

    /// Inspects path and header bytes to determine if this plugin can decode the file.
    fn can_decode(&self, path: &Path, header: &[u8]) -> bool;

    /// Decodes the file into an in-memory image source.
    fn decode(&self, path: &Path) -> Result<Arc<dyn ImageSource>>;
}
