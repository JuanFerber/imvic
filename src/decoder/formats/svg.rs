//! SVG format decoder plugin powered by resvg and tiny-skia.
//!
//! Provides dynamic vector rasterization at arbitrary zoom levels and viewports.

use crate::decoder::{CropRect, FormatDecoder, ImageSource};
use anyhow::{Context, Result};
use image::RgbaImage;
use image::imageops::crop_imm;
use resvg::tiny_skia::{PixmapMut, Transform};
use resvg::usvg::{Options, Tree, fontdb};
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Cushion overdraw scale during stabilized continuous panning.
const PAN_CUSHION_FACTOR: f32 = 2.0;

/// Cushion scale during active zoom (1.0x avoids wasting rasterization on transient frames).
const ZOOM_CUSHION_FACTOR: f32 = 1.0;

/// Delay required after a zoom event before allocating the 2.0x pan cushion.
const ZOOM_STABILIZATION_DELAY: Duration = Duration::from_millis(150);

/// Margin in canvas pixels to avoid clipping anti-aliased geometry during frustum culling.
const FRUSTUM_PADDING_PX: f32 = 2.0;

/// Embedded fallback font assets covering Sans, Serif, and Monospace (Regular & Bold).
const EMBEDDED_FONTS: &[&[u8]] = &[
    include_bytes!("../../../assets/fonts/NotoSans-Regular.ttf"),
    include_bytes!("../../../assets/fonts/NotoSans-Bold.ttf"),
    include_bytes!("../../../assets/fonts/NotoSerif-Regular.ttf"),
    include_bytes!("../../../assets/fonts/NotoSerif-Bold.ttf"),
    include_bytes!("../../../assets/fonts/NotoSansMono-Regular.ttf"),
    include_bytes!("../../../assets/fonts/NotoSansMono-Bold.ttf"),
];

/// SVG vector format decoder plugin.
pub struct SvgDecoder {
    fontdb: Arc<fontdb::Database>,
}

impl SvgDecoder {
    /// Creates a new instance of the SVG decoder.
    pub fn new() -> Self {
        let mut fontdb = fontdb::Database::new();
        fontdb.load_system_fonts();

        for font_bytes in EMBEDDED_FONTS {
            fontdb.load_font_data(font_bytes.to_vec());
        }

        fontdb.set_sans_serif_family("Noto Sans");
        fontdb.set_serif_family("Noto Serif");
        fontdb.set_monospace_family("Noto Sans Mono");

        Self {
            fontdb: Arc::new(fontdb),
        }
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

        // Header sniffing: must contain '<svg' tag or '<!doctype svg' declaration
        let contains_svg_tag = header.windows(4).any(|w| w.eq_ignore_ascii_case(b"<svg"));
        let contains_svg_doctype = header
            .windows(13)
            .any(|w| w.eq_ignore_ascii_case(b"<!doctype svg"));
        let has_svg_magic = contains_svg_tag || contains_svg_doctype;

        has_svg_extension || has_svg_magic
    }

    fn decode(&self, path: &Path) -> Result<Arc<dyn ImageSource>> {
        let svg_bytes = std::fs::read(path)
            .with_context(|| format!("Failed to read SVG file at {:?}", path))?;

        let opt = Options {
            fontdb: self.fontdb.clone(),
            ..Default::default()
        };
        let tree = Tree::from_data(&svg_bytes, &opt)
            .with_context(|| format!("Failed to parse SVG data from {:?}", path))?;

        let size = tree.size();
        let width = size.width().ceil() as u32;
        let height = size.height().ceil() as u32;

        Ok(Arc::new(SvgImageSource {
            tree,
            width: width.max(1),
            height: height.max(1),
            state: Mutex::new(SvgRasterState::default()),
        }))
    }
}

/// Internal cache storing a pre-rendered overdraw cushion to accelerate continuous panning.
struct SvgCache {
    /// The world-coordinate bounding box covered by this pre-rendered cushion.
    cushion_crop: CropRect,
    /// Horizontal scale factor used to render the cushion.
    scale_x: f32,
    /// Vertical scale factor used to render the cushion.
    scale_y: f32,
    /// Background color applied to the cushion.
    bg_color: Option<[u8; 4]>,
    /// The pre-rendered pixel buffer held in memory.
    image: RgbaImage,
}

/// Internal state managing the pan cache, reusable scratch buffer, and zoom metrics.
#[derive(Default)]
struct SvgRasterState {
    /// Cached pre-rendered cushion for fast pan hits.
    cache: Option<SvgCache>,
    /// Pre-allocated reusable pixel buffer avoiding heap churn across frames.
    scratch: Vec<u8>,
    /// Scale factor of the previous frame to detect active zoom vs panning.
    last_scale: Option<(f32, f32)>,
    /// Timestamp of the last detected zoom event for stabilization delay.
    last_zoom_time: Option<Instant>,
}

/// In-memory SVG image surface capable of dynamic resolution re-rendering.
pub struct SvgImageSource {
    tree: Tree,
    width: u32,
    height: u32,
    state: Mutex<SvgRasterState>,
}

impl ImageSource for SvgImageSource {
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

        let scale_x = target_w as f32 / crop.width;
        let scale_y = target_h as f32 / crop.height;

        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());

        // 1. Check for cache hit: identical scale, same background, and requested crop inside cushion
        if let Some(cache) = state.cache.as_ref() {
            let scale_match =
                (cache.scale_x - scale_x).abs() < 1e-4 && (cache.scale_y - scale_y).abs() < 1e-4;
            let bg_match = cache.bg_color == bg_color;

            let src_x = (crop.x - cache.cushion_crop.x) * scale_x;
            let src_y = (crop.y - cache.cushion_crop.y) * scale_y;

            let in_bounds = src_x >= 0.0
                && src_y >= 0.0
                && (src_x + target_w as f32) <= cache.image.width() as f32
                && (src_y + target_h as f32) <= cache.image.height() as f32;

            if scale_match && bg_match && in_bounds {
                let px = (src_x.round() as u32).min(cache.image.width().saturating_sub(target_w));
                let py = (src_y.round() as u32).min(cache.image.height().saturating_sub(target_h));
                return crop_imm(&cache.image, px, py, target_w, target_h).to_image();
            }
        }

        // 2. Adaptive Dynamic Cushion: Detect active zoom vs stabilized pan
        let now = Instant::now();

        let scale_changed = match state.last_scale {
            Some((lx, ly)) => (lx - scale_x).abs() > 1e-4 || (ly - scale_y).abs() > 1e-4,
            None => true,
        };

        let is_zooming = if scale_changed {
            state.last_scale = Some((scale_x, scale_y));
            state.last_zoom_time = Some(now);
            true
        } else if let Some(last_time) = state.last_zoom_time {
            now.duration_since(last_time) < ZOOM_STABILIZATION_DELAY
        } else {
            false
        };

        // Render exact 1.0x screen during active zoom (saving 75% pixel rasterization),
        // and 2.0x overdraw cushion only when zooming stabilizes for pan hits.
        let cushion_factor: f32 = if is_zooming {
            ZOOM_CUSHION_FACTOR
        } else {
            PAN_CUSHION_FACTOR
        };
        let margin_ratio = (cushion_factor - 1.0) * 0.5;

        let margin_x = crop.width * margin_ratio;
        let margin_y = crop.height * margin_ratio;

        let cushion_crop = CropRect {
            x: crop.x - margin_x,
            y: crop.y - margin_y,
            width: crop.width + margin_x * 2.0,
            height: crop.height + margin_y * 2.0,
        };

        let cushion_w = (cushion_crop.width * scale_x).round().max(1.0) as u32;
        let cushion_h = (cushion_crop.height * scale_y).round().max(1.0) as u32;

        let needed_bytes = match (cushion_w as usize)
            .checked_mul(cushion_h as usize)
            .and_then(|px| px.checked_mul(4))
        {
            Some(bytes) => bytes,
            None => return RgbaImage::new(target_w, target_h),
        };

        // 3. Reusable Scratch Buffer with Ping-Pong Recycling
        if let Some(old_cache) = state.cache.take() {
            let old_vec = old_cache.image.into_raw();
            if old_vec.capacity() > state.scratch.capacity() {
                state.scratch = old_vec;
            }
        }

        if state.scratch.len() < needed_bytes {
            state.scratch.resize(needed_bytes, 0);
        }

        let transform = Transform::from_translate(-cushion_crop.x, -cushion_crop.y)
            .post_scale(scale_x, scale_y);

        let mut pixmap =
            match PixmapMut::from_bytes(&mut state.scratch[..needed_bytes], cushion_w, cushion_h) {
                Some(p) => p,
                None => return RgbaImage::new(target_w, target_h),
            };

        // Hardware-accelerated background fill
        let is_opaque_bg = matches!(bg_color, Some([_, _, _, 255]));
        if let Some([r, g, b, a]) = bg_color {
            let c = resvg::tiny_skia::Color::from_rgba8(r, g, b, a);
            pixmap.fill(c);
        } else {
            pixmap.fill(resvg::tiny_skia::Color::TRANSPARENT);
        }

        // 4. Frustum Culling & Viewport Rendering
        render_tree_or_culled(&self.tree, cushion_crop, transform, &mut pixmap);

        // 5. Alpha Demultiplication Bypass (only if canvas has semi-transparent pixels)
        if !is_opaque_bg {
            for chunk in state.scratch[..needed_bytes].as_chunks_mut::<4>().0 {
                let a = chunk[3];
                if a > 0 && a < 255 {
                    let a_f = a as f32 / 255.0;
                    chunk[0] = ((chunk[0] as f32 / a_f).round()).min(255.0) as u8;
                    chunk[1] = ((chunk[1] as f32 / a_f).round()).min(255.0) as u8;
                    chunk[2] = ((chunk[2] as f32 / a_f).round()).min(255.0) as u8;
                }
            }
        }

        // Sub-crop requested view from the freshly rendered cushion
        let src_x = ((crop.x - cushion_crop.x) * scale_x).round().max(0.0) as u32;
        let src_y = ((crop.y - cushion_crop.y) * scale_y).round().max(0.0) as u32;

        let output_image = if cushion_w == target_w
            && cushion_h == target_h
            && src_x == 0
            && src_y == 0
        {
            match RgbaImage::from_raw(target_w, target_h, state.scratch[..needed_bytes].to_vec()) {
                Some(img) => img,
                None => RgbaImage::new(target_w, target_h),
            }
        } else {
            let cushion_view = match RgbaImage::from_raw(
                cushion_w,
                cushion_h,
                state.scratch[..needed_bytes].to_vec(),
            ) {
                Some(img) => img,
                None => return RgbaImage::new(target_w, target_h),
            };
            let px = src_x.min(cushion_view.width().saturating_sub(target_w));
            let py = src_y.min(cushion_view.height().saturating_sub(target_h));
            crop_imm(&cushion_view, px, py, target_w, target_h).to_image()
        };

        // Cache 2.0x cushion for subsequent pan hits when stabilized
        if !is_zooming
            && let Some(cushion_image) =
                RgbaImage::from_raw(cushion_w, cushion_h, state.scratch[..needed_bytes].to_vec())
        {
            state.cache = Some(SvgCache {
                cushion_crop,
                scale_x,
                scale_y,
                bg_color,
                image: cushion_image,
            });
        }

        output_image
    }
}

/// Renders an SVG tree onto a pixmap, applying broad-phase viewport frustum culling
/// across top-level elements to skip out-of-bounds paths and layers.
fn render_tree_or_culled(
    tree: &Tree,
    cushion_crop: CropRect,
    transform: Transform,
    pixmap: &mut PixmapMut,
) {
    let root = tree.root();
    let children = root.children();

    // If root contains only a single group or no children, delegate directly to resvg
    if children.len() <= 1 {
        resvg::render(tree, transform, pixmap);
        return;
    }

    // Broad-phase AABB test in SVG canvas coordinates with 2px margin for anti-aliasing
    let pad = FRUSTUM_PADDING_PX;
    let min_x = cushion_crop.x - pad;
    let min_y = cushion_crop.y - pad;
    let max_x = cushion_crop.x + cushion_crop.width + pad;
    let max_y = cushion_crop.y + cushion_crop.height + pad;

    for node in children {
        if let Some(bbox) = node.abs_layer_bounding_box() {
            let intersects = bbox.right() >= min_x
                && bbox.left() <= max_x
                && bbox.bottom() >= min_y
                && bbox.top() <= max_y;

            if !intersects {
                // Frustum culling: skip out-of-bounds node completely
                continue;
            }

            // Offset compensation: resvg::render_node internally subtracts (bbox.x, bbox.y).
            // Pre-translating by (bbox.x, bbox.y) restores the exact net transform.
            let node_transform = transform.pre_translate(bbox.x(), bbox.y());
            let _ = resvg::render_node(node, node_transform, pixmap);
        } else {
            // If bounding box cannot be computed, render conservatively
            resvg::render(tree, transform, pixmap);
            return;
        }
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

        // Standard extension
        let path = Path::new("test.svg");
        assert!(decoder.can_decode(path, SAMPLE_SVG));

        // Unknown extension with pure <svg> header
        let unk_path = Path::new("test_drawing.unknown");
        assert!(decoder.can_decode(unk_path, SAMPLE_SVG));

        // SVG with XML prologue
        let xml_svg =
            b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg width=\"100\" height=\"100\"></svg>";
        assert!(decoder.can_decode(unk_path, xml_svg));

        // Generic XML without <svg> tag must be rejected!
        let generic_xml =
            b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<feed><title>RSS</title></feed>";
        assert!(!decoder.can_decode(unk_path, generic_xml));
    }

    #[test]
    fn test_svg_render_crop() {
        let opt = Options::default();
        let tree = Tree::from_data(SAMPLE_SVG, &opt).expect("Valid SVG");
        let source = SvgImageSource {
            tree,
            width: 100,
            height: 100,
            state: Mutex::new(SvgRasterState::default()),
        };

        assert_eq!(source.dimensions(), (100, 100));

        let crop = CropRect {
            x: 0.0,
            y: 0.0,
            width: 50.0,
            height: 50.0,
        };

        let frame = source.render_crop(crop, 200, 200, None);
        assert_eq!(frame.width(), 200);
        assert_eq!(frame.height(), 200);
    }

    #[test]
    fn test_svg_text_rendering() {
        let decoder = SvgDecoder::new();
        let svg_with_text = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 100">
                <text x="10" y="50" font-family="sans-serif" font-size="20" fill="black">Hello</text>
            </svg>"#;

        let opt = Options {
            fontdb: decoder.fontdb.clone(),
            ..Default::default()
        };
        let tree = Tree::from_data(svg_with_text, &opt).expect("Valid SVG with text");
        let source = SvgImageSource {
            tree,
            width: 200,
            height: 100,
            state: Mutex::new(SvgRasterState::default()),
        };

        let crop = CropRect {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 100.0,
        };

        let frame = source.render_crop(crop, 200, 100, None);
        let has_rendered_text_pixels = frame.pixels().any(|p| p[3] > 0);
        assert!(
            has_rendered_text_pixels,
            "SVG text must render non-transparent glyph pixels"
        );
    }
}
