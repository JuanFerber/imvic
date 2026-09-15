//! SVG format decoder plugin powered by resvg and tiny-skia.
//!
//! Provides dynamic vector rasterization at arbitrary zoom levels and viewports.

use crate::decoder::{CropRect, FormatDecoder, ImageSource};
use anyhow::{Context, Result};
use image::RgbaImage;
use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{Options, Tree};
use std::path::Path;
use std::sync::Arc;

/// SVG vector format decoder plugin.
pub struct SvgDecoder;

impl SvgDecoder {
    /// Creates a new instance of the SVG decoder.
    pub fn new() -> Self {
        Self
    }
}

impl Default for SvgDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl FormatDecoder for SvgDecoder {
    fn name(&self) -> &'static str {
        "SVG Vector Decoder"
    }

    fn can_decode(&self, path: &Path, header: &[u8]) -> bool {
        let has_svg_extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("svg"))
            .unwrap_or(false);

        let has_svg_magic =
            header.windows(4).any(|w| w == b"<svg") || header.windows(5).any(|w| w == b"<?xml");

        has_svg_extension || has_svg_magic
    }

    fn decode(&self, path: &Path) -> Result<Arc<dyn ImageSource>> {
        let svg_bytes = std::fs::read(path)
            .with_context(|| format!("Failed to read SVG file at {:?}", path))?;

        let opt = Options::default();
        let tree = Tree::from_data(&svg_bytes, &opt)
            .with_context(|| format!("Failed to parse SVG data from {:?}", path))?;

        let size = tree.size();
        let width = size.width().ceil() as u32;
        let height = size.height().ceil() as u32;

        Ok(Arc::new(SvgImageSource {
            tree,
            width: width.max(1),
            height: height.max(1),
        }))
    }
}

/// In-memory SVG image surface capable of dynamic resolution re-rendering.
pub struct SvgImageSource {
    tree: Tree,
    width: u32,
    height: u32,
}

impl ImageSource for SvgImageSource {
    fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn render_crop(&self, crop: CropRect, target_w: u32, target_h: u32) -> RgbaImage {
        if target_w == 0 || target_h == 0 || crop.width <= 0.0 || crop.height <= 0.0 {
            return RgbaImage::new(1, 1);
        }

        let scale_x = target_w as f32 / crop.width;
        let scale_y = target_h as f32 / crop.height;

        let transform = Transform::from_translate(-crop.x, -crop.y).post_scale(scale_x, scale_y);

        let mut pixmap = match Pixmap::new(target_w, target_h) {
            Some(p) => p,
            None => return RgbaImage::new(target_w, target_h),
        };

        resvg::render(&self.tree, transform, &mut pixmap.as_mut());

        // Convert premultiplied pixmap data into straight RGBA bytes for image buffer
        let rgba_bytes = pixmap.take_demultiplied();
        RgbaImage::from_raw(target_w, target_h, rgba_bytes)
            .unwrap_or_else(|| RgbaImage::new(target_w, target_h))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
            <rect width="100" height="100" fill="red"/>
        </svg>"#;

    #[test]
    fn test_svg_detection() {
        let decoder = SvgDecoder::new();
        let path = Path::new("test.svg");
        assert!(decoder.can_decode(path, SAMPLE_SVG));
    }

    #[test]
    fn test_svg_render_crop() {
        let opt = Options::default();
        let tree = Tree::from_data(SAMPLE_SVG, &opt).expect("Valid SVG");
        let source = SvgImageSource {
            tree,
            width: 100,
            height: 100,
        };

        assert_eq!(source.dimensions(), (100, 100));

        let crop = CropRect {
            x: 0.0,
            y: 0.0,
            width: 50.0,
            height: 50.0,
        };

        let frame = source.render_crop(crop, 200, 200);
        assert_eq!(frame.width(), 200);
        assert_eq!(frame.height(), 200);
    }
}
