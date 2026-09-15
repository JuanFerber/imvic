//! 2D camera engine and viewport transformations.
//!
//! Handles focal zoom-to-cursor math, continuous panning deltas,
//! aspect ratio fitting, and viewport margin clamping.

use crate::decoder::{CropRect, ImageSource};
use image::RgbaImage;
use std::sync::Arc;

/// Represents interactive camera state projecting an image onto a terminal grid.
#[derive(Clone)]
pub struct ViewportState {
    pub zoom: f32,
    pub offset_x: f32,
    pub offset_y: f32,
    pub term_cols: u16,
    pub term_rows: u16,
    pub source: Arc<dyn ImageSource>,
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
        };

        viewport.fit_to_screen();
        viewport
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
    pub fn zoom_at(&mut self, cursor_col: u16, cursor_row: u16, factor: f32) {
        if factor <= 0.0 {
            return;
        }

        let new_zoom = (self.zoom * factor).clamp(0.05, 100.0);
        let effective_factor = new_zoom / self.zoom;

        let (img_w, img_h) = self.source.dimensions();
        let img_w = img_w as f32;
        let img_h = img_h as f32;

        let u_c = (cursor_col as f32 / self.term_cols as f32).clamp(0.0, 1.0);
        let v_c = (cursor_row as f32 / self.term_rows as f32).clamp(0.0, 1.0);

        // Focal zoom formula: keep point under cursor invariant
        self.offset_x += (u_c * img_w / self.zoom) * (1.0 - 1.0 / effective_factor);
        self.offset_y += (v_c * img_h / self.zoom) * (1.0 - 1.0 / effective_factor);

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

    /// Calculates current visible crop rectangle in source image coordinates.
    pub fn current_crop(&self) -> CropRect {
        let (img_w, img_h) = self.source.dimensions();
        let img_w = img_w as f32;
        let img_h = img_h as f32;

        let visible_w = img_w / self.zoom;
        let visible_h = img_h / self.zoom;

        CropRect {
            x: self.offset_x,
            y: self.offset_y,
            width: visible_w,
            height: visible_h,
        }
    }

    /// Renders current camera frame to target pixel dimensions.
    pub fn render_frame(&self, target_w: u32, target_h: u32) -> RgbaImage {
        let crop = self.current_crop();
        self.source.render_crop(crop, target_w, target_h)
    }

    /// Clamps offsets to prevent panning completely away from the canvas.
    fn clamp_offsets(&mut self) {
        let (img_w, img_h) = self.source.dimensions();
        let img_w = img_w as f32;
        let img_h = img_h as f32;

        let visible_w = img_w / self.zoom;
        let visible_h = img_h / self.zoom;

        // Allow panning with a safety margin (at least 10% visible)
        let margin_x = visible_w * 0.9;
        let margin_y = visible_h * 0.9;

        let min_x = -margin_x;
        let max_x = img_w - visible_w * 0.1;
        let min_y = -margin_y;
        let max_y = img_h - visible_h * 0.1;

        self.offset_x = self.offset_x.clamp(min_x, max_x);
        self.offset_y = self.offset_y.clamp(min_y, max_y);
    }
}
