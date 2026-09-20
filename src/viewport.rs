//! 2D camera engine and viewport transformations.
//!
//! Handles focal zoom-to-cursor math, continuous panning deltas,
//! aspect ratio fitting, and viewport margin clamping.

use crate::decoder::{CropRect, ImageSource};
use image::RgbaImage;
use std::sync::Arc;

pub const MIN_ZOOM: f32 = 0.50;
pub const MAX_ZOOM: f32 = 10.0;

/// Minimum percentage of the canvas dimension that must remain visible on screen when panning.
pub const MIN_CANVAS_VISIBILITY_RATIO: f32 = 0.10;

/// Represents interactive camera state projecting an image onto a terminal grid.
#[derive(Clone)]
pub struct ViewportState {
    pub zoom: f32,
    pub offset_x: f32,
    pub offset_y: f32,
    pub term_cols: u16,
    pub term_rows: u16,
    pub source: Arc<dyn ImageSource>,
    pub bg_color: Option<[u8; 4]>,
}

impl ViewportState {
    /// Creates a new viewport initialized with fit-to-screen scaling.
    pub fn new(source: Arc<dyn ImageSource>, term_cols: u16, term_rows: u16) -> Self {
        let mut viewport = Self {
            zoom: 1.0,
            offset_x: 0.0,
            offset_y: 0.0,
            term_cols: term_cols.max(1),
            term_rows: term_rows.max(1),
            source,
            bg_color: None,
        };

        viewport.fit_to_screen();
        viewport
    }

    /// Sets or clears the optional canvas background color.
    pub fn set_bg_color(&mut self, bg_color: Option<[u8; 4]>) {
        self.bg_color = bg_color;
    }

    /// Updates the underlying image source while preserving camera position.
    pub fn set_source(&mut self, source: Arc<dyn ImageSource>) {
        self.source = source;
        self.clamp_offsets();
    }

    /// Updates terminal grid dimensions and clamps camera offsets.
    pub fn set_terminal_size(&mut self, cols: u16, rows: u16) {
        self.term_cols = cols.max(1);
        self.term_rows = rows.max(1);
        self.clamp_offsets();
    }

    /// Pans camera by relative delta in terminal cell units.
    pub fn pan(&mut self, delta_cols: f32, delta_rows: f32) {
        let (img_w, img_h) = self.source.dimensions();
        let img_w = img_w as f32;
        let img_h = img_h as f32;

        let delta_u = delta_cols / self.term_cols as f32;
        let delta_v = delta_rows / self.term_rows as f32;

        let world_dx = -(delta_u * img_w / self.zoom);
        let world_dy = -(delta_v * img_h / self.zoom);

        self.offset_x += world_dx;
        self.offset_y += world_dy;

        self.clamp_offsets();
    }

    /// Zooms camera anchored at specific terminal cursor coordinates (zoom-to-cursor invariance).
    ///
    /// Preserves sub-pixel focal invariance using the center-relative coordinate space
    /// and guards against catastrophic cancellation / division-by-zero near boundary limits.
    pub fn zoom_at(&mut self, cursor_col: u16, cursor_row: u16, factor: f32) {
        if factor <= 0.0 || (factor - 1.0).abs() < 1e-6 {
            return;
        }

        let new_zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        if (new_zoom - self.zoom).abs() < 1e-6 {
            return;
        }

        let (img_w, img_h) = self.source.dimensions();
        let img_w = img_w as f32;
        let img_h = img_h as f32;

        let u_c = (cursor_col as f32 / self.term_cols as f32).clamp(0.0, 1.0);
        let v_c = (cursor_row as f32 / self.term_rows as f32).clamp(0.0, 1.0);

        // Center-relative focal zoom formula: keeps point under cursor strictly stationary
        // Delta offset = (cursor - 0.5) * img_dimension * (1/z_old - 1/z_new)
        let inv_diff = 1.0 / self.zoom - 1.0 / new_zoom;
        self.offset_x += (u_c - 0.5) * img_w * inv_diff;
        self.offset_y += (v_c - 0.5) * img_h * inv_diff;

        self.zoom = new_zoom;
        self.clamp_offsets();
    }

    /// Centers and fits the entire image within the current terminal grid.
    pub fn fit_to_screen(&mut self) {
        self.zoom = 1.0;
        self.offset_x = 0.0;
        self.offset_y = 0.0;
    }

    /// Resets camera zoom to 100% (1:1) and centers offsets.
    pub fn reset_view(&mut self) {
        self.zoom = 1.0;
        self.offset_x = 0.0;
        self.offset_y = 0.0;
    }

    /// Centers the camera view on the image without changing the current zoom level.
    pub fn center_view(&mut self) {
        self.offset_x = 0.0;
        self.offset_y = 0.0;
    }

    /// Calculates current visible crop rectangle in source image coordinates,
    /// preserving the aspect ratio of the target render screen (Contain / Letterboxing).
    pub fn current_crop(&self, target_w: u32, target_h: u32) -> CropRect {
        let (img_w, img_h) = self.source.dimensions();
        let img_w = img_w as f32;
        let img_h = img_h as f32;

        let target_w = target_w.max(1) as f32;
        let target_h = target_h.max(1) as f32;

        let base_scale = (target_w / img_w).min(target_h / img_h);
        let effective_scale = base_scale * self.zoom;

        let visible_w = target_w / effective_scale;
        let visible_h = target_h / effective_scale;

        let center_x = (img_w - visible_w) / 2.0;
        let center_y = (img_h - visible_h) / 2.0;

        CropRect {
            x: center_x + self.offset_x,
            y: center_y + self.offset_y,
            width: visible_w,
            height: visible_h,
        }
    }

    /// Renders current camera frame to target pixel dimensions.
    pub fn render_frame(&self, target_w: u32, target_h: u32) -> RgbaImage {
        let crop = self.current_crop(target_w, target_h);
        self.source
            .render_crop(crop, target_w, target_h, self.bg_color)
    }

    /// Clamps offsets to prevent panning completely away from the canvas.
    fn clamp_offsets(&mut self) {
        let (img_w, img_h) = self.source.dimensions();
        let img_w = img_w as f32;
        let img_h = img_h as f32;

        let visible_w = img_w / self.zoom;
        let visible_h = img_h / self.zoom;

        // Keep at least the minimum percentage of the canvas visible on screen when panning
        let min_visible_x = (visible_w.min(img_w) * MIN_CANVAS_VISIBILITY_RATIO).max(1.0);
        let min_visible_y = (visible_h.min(img_h) * MIN_CANVAS_VISIBILITY_RATIO).max(1.0);

        let max_offset_x = ((img_w + visible_w) / 2.0 - min_visible_x).max(0.0);
        let max_offset_y = ((img_h + visible_h) / 2.0 - min_visible_y).max(0.0);

        self.offset_x = self.offset_x.clamp(-max_offset_x, max_offset_x);
        self.offset_y = self.offset_y.clamp(-max_offset_y, max_offset_y);
    }
}
