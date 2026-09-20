//! Raster/Bitmap static image format decoder plugin (PNG, JPEG, WebP, GIF, BMP).
//!
//! Decodes static raster formats into an in-memory RGBA surface and performs
//! fast, filtered sub-rectangle cropping for the viewport camera engine.

use crate::decoder::{CropRect, FormatDecoder, ImageSource};
use anyhow::{Context, Result};
use image::imageops::{FilterType, crop_imm, overlay, resize};
use image::{GenericImageView, Rgba, RgbaImage};
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

    fn render_crop(
        &self,
        crop: CropRect,
        target_w: u32,
        target_h: u32,
        bg_color: Option<[u8; 4]>,
    ) -> RgbaImage {
        if target_w == 0 || target_h == 0 || crop.width <= 0.0 || crop.height <= 0.0 {
            return RgbaImage::new(1, 1);
        }

        // Initialize screen canvas filled with canvas background color or transparent
        let mut canvas = if let Some(bg) = bg_color {
            RgbaImage::from_pixel(target_w, target_h, Rgba(bg))
        } else {
            RgbaImage::new(target_w, target_h)
        };

        let img_w = self.width as f32;
        let img_h = self.height as f32;

        // Calculate intersection between camera frustum and source image domain [0, img_w] x [0, img_h]
        let x1 = crop.x.max(0.0).min(img_w);
        let y1 = crop.y.max(0.0).min(img_h);
        let x2 = (crop.x + crop.width).max(0.0).min(img_w);
        let y2 = (crop.y + crop.height).max(0.0).min(img_h);

        // If the camera frustum overlaps with the image, project and composite the visible slice
        if x2 > x1 && y2 > y1 {
            let src_x = x1.floor() as u32;
            let src_y = y1.floor() as u32;
            let src_w = ((x2 - x1).ceil() as u32)
                .min(self.width.saturating_sub(src_x))
                .max(1);
            let src_h = ((y2 - y1).ceil() as u32)
                .min(self.height.saturating_sub(src_y))
                .max(1);

            let cropped = crop_imm(self.image.as_ref(), src_x, src_y, src_w, src_h);

            // Screen destination coordinates and dimensions preserving exact aspect ratio
            let scale_x = target_w as f32 / crop.width;
            let scale_y = target_h as f32 / crop.height;

            let dst_x = ((x1 - crop.x) * scale_x).round() as i64;
            let dst_y = ((y1 - crop.y) * scale_y).round() as i64;
            let dst_w = ((x2 - x1) * scale_x).round().max(1.0) as u32;
            let dst_h = ((y2 - y1) * scale_y).round().max(1.0) as u32;

            let resized = resize(&cropped.to_image(), dst_w, dst_h, FilterType::Triangle);
            overlay(&mut canvas, &resized, dst_x, dst_y);
        }

        canvas
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

    #[test]
    fn test_raster_bg_color_composition() {
        // Create a 2x2 fully transparent PNG surface
        let transparent_img = Arc::new(RgbaImage::new(2, 2));
        let source = RasterImageSource {
            image: transparent_img,
            width: 2,
            height: 2,
        };

        // Render with red background [255, 0, 0, 255]
        let red_bg = Some([255, 0, 0, 255]);
        let crop = CropRect {
            x: 0.0,
            y: 0.0,
            width: 2.0,
            height: 2.0,
        };
        let frame = source.render_crop(crop, 4, 4, red_bg);

        // Every pixel must have blended with the red background
        for pixel in frame.pixels() {
            assert_eq!(pixel.0, [255, 0, 0, 255]);
        }
    }

    #[test]
    fn test_raster_panning_past_edges_preserves_canvas() {
        // Create a 10x10 image with solid white pixels
        let img = Arc::new(RgbaImage::from_pixel(10, 10, Rgba([255, 255, 255, 255])));
        let source = RasterImageSource {
            image: img,
            width: 10,
            height: 10,
        };

        // Camera frustum looking 50% outside the left edge: x in [-5.0, 5.0]
        let crop = CropRect {
            x: -5.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        };
        // Target screen is 100x100 with red background
        let red_bg = Some([255, 0, 0, 255]);
        let frame = source.render_crop(crop, 100, 100, red_bg);

        // Left half [0..50) must be background (red)
        let left_pixel = frame.get_pixel(10, 50);
        assert_eq!(left_pixel.0, [255, 0, 0, 255]);

        // Right half [50..100) must contain the image (white)
        let right_pixel = frame.get_pixel(75, 50);
        assert_eq!(right_pixel.0, [255, 255, 255, 255]);
    }
}
