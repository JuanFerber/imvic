//! Raster/Bitmap static image format decoder plugin (PNG, JPEG, WebP, GIF, BMP).
//!
//! Decodes static raster formats into an in-memory RGBA surface and performs
//! fast, filtered sub-rectangle cropping for the viewport camera engine.

use crate::decoder::{CropRect, FormatDecoder, ImageSource};
use anyhow::{Context, Result};
use image::imageops::{FilterType, crop_imm, resize};
use image::{GenericImageView, RgbaImage};
use std::path::Path;
use std::sync::Arc;

/// Decoder plugin for common static raster image formats.
pub struct RasterDecoder;

impl RasterDecoder {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RasterDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl FormatDecoder for RasterDecoder {
    fn name(&self) -> &'static str {
        "Static Raster Image Decoder (PNG, JPEG, WebP, GIF, BMP)"
    }

    fn can_decode(&self, path: &Path, header: &[u8]) -> bool {
        // 1. Check file extensions
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let ext_lower = ext.to_ascii_lowercase();
            if matches!(
                ext_lower.as_str(),
                "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp"
            ) {
                return true;
            }
        }

        // 2. Check magic bytes
        if header.len() >= 8 {
            // PNG signature: 89 50 4E 47 0D 0A 1A 0A
            if header.starts_with(b"\x89PNG\r\n\x1a\n") {
                return true;
            }
            // JPEG signature: FF D8 FF
            if header.starts_with(b"\xff\xd8\xff") {
                return true;
            }
            // GIF signature: GIF87a or GIF89a
            if header.starts_with(b"GIF87a") || header.starts_with(b"GIF89a") {
                return true;
            }
            // BMP signature: BM
            if header.starts_with(b"BM") {
                return true;
            }
            // WebP signature: RIFF....WEBP
            if header.starts_with(b"RIFF") && header.len() >= 12 && &header[8..12] == b"WEBP" {
                return true;
            }
        }

        false
    }

    fn decode(&self, path: &Path) -> Result<Arc<dyn ImageSource>> {
        let reader = image::ImageReader::open(path)
            .with_context(|| format!("Failed to open image file at {:?}", path))?
            .with_guessed_format()
            .with_context(|| format!("Failed to determine format for {:?}", path))?;

        let dynamic_img = reader
            .decode()
            .with_context(|| format!("Failed to decode raster image data from {:?}", path))?;

        let (width, height) = dynamic_img.dimensions();
        let rgba = dynamic_img.to_rgba8();

        Ok(Arc::new(RasterImageSource {
            image: Arc::new(rgba),
            width: width.max(1),
            height: height.max(1),
        }))
    }
}

/// In-memory static bitmap image surface supporting sub-pixel viewport cropping and scaling.
pub struct RasterImageSource {
    image: Arc<RgbaImage>,
    width: u32,
    height: u32,
}

impl ImageSource for RasterImageSource {
    fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn render_crop(&self, crop: CropRect, target_w: u32, target_h: u32) -> RgbaImage {
        if target_w == 0 || target_h == 0 || crop.width <= 0.0 || crop.height <= 0.0 {
            return RgbaImage::new(1, 1);
        }

        let img_w = self.width as f32;
        let img_h = self.height as f32;

        // Calculate visible bounding box in image pixel space
        let crop_x = crop.x.clamp(0.0, img_w);
        let crop_y = crop.y.clamp(0.0, img_h);
        let crop_w = (crop.width).min(img_w - crop_x).max(1.0);
        let crop_h = (crop.height).min(img_h - crop_y).max(1.0);

        // Crop the source sub-rectangle
        let cropped = crop_imm(
            self.image.as_ref(),
            crop_x.floor() as u32,
            crop_y.floor() as u32,
            crop_w.ceil() as u32,
            crop_h.ceil() as u32,
        );

        // Rescale to target pixel dimensions using bilinear filtering
        resize(
            &cropped.to_image(),
            target_w,
            target_h,
            FilterType::Triangle,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raster_magic_detection() {
        let decoder = RasterDecoder::new();
        let png_header = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR";
        assert!(decoder.can_decode(Path::new("test.unknown"), png_header));

        let jpeg_header = b"\xff\xd8\xff\xe0\x00\x10JFIF";
        assert!(decoder.can_decode(Path::new("test.unknown"), jpeg_header));

        let text_file = b"Hello, World!";
        assert!(!decoder.can_decode(Path::new("test.txt"), text_file));
    }
}
