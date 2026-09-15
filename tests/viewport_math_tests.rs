//! Pure mathematical unit tests for 2D viewport and camera transformations.

use image::RgbaImage;
use imvic::decoder::{CropRect, ImageSource};
use imvic::viewport::ViewportState;
use std::sync::Arc;

/// Mock image source for pure mathematical testing without I/O.
struct MockImageSource {
    width: u32,
    height: u32,
}

impl ImageSource for MockImageSource {
    fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn render_crop(&self, _crop: CropRect, target_w: u32, target_h: u32) -> RgbaImage {
        RgbaImage::new(target_w, target_h)
    }
}

fn create_test_viewport(img_w: u32, img_h: u32, term_cols: u16, term_rows: u16) -> ViewportState {
    let source = Arc::new(MockImageSource {
        width: img_w,
        height: img_h,
    });
    ViewportState::new(source, term_cols, term_rows)
}

#[test]
fn test_zoom_to_cursor_invariance_center() {
    let mut vp = create_test_viewport(1000, 800, 100, 50);

    // Position cursor exactly at center of terminal (col 50, row 25)
    let cursor_col = 50;
    let cursor_row = 25;
    let u = cursor_col as f32 / vp.term_cols as f32; // 0.5
    let v = cursor_row as f32 / vp.term_rows as f32; // 0.5

    let world_x_before = vp.offset_x + u * (1000.0 / vp.zoom);
    let world_y_before = vp.offset_y + v * (800.0 / vp.zoom);

    // Apply 2.5x zoom centered at cursor
    vp.zoom_at(cursor_col, cursor_row, 2.5);

    let world_x_after = vp.offset_x + u * (1000.0 / vp.zoom);
    let world_y_after = vp.offset_y + v * (800.0 / vp.zoom);

    // The point under cursor in image space MUST remain invariant
    assert!(
        (world_x_before - world_x_after).abs() < 1e-4,
        "X coordinate shifted under cursor!"
    );
    assert!(
        (world_y_before - world_y_after).abs() < 1e-4,
        "Y coordinate shifted under cursor!"
    );
}

#[test]
fn test_zoom_to_cursor_invariance_corners() {
    let mut vp = create_test_viewport(1000, 800, 100, 50);

    // Top-left corner (0, 0)
    let world_x_before = vp.offset_x;
    let world_y_before = vp.offset_y;

    vp.zoom_at(0, 0, 1.8);

    let world_x_after = vp.offset_x;
    let world_y_after = vp.offset_y;

    assert!((world_x_before - world_x_after).abs() < 1e-4);
    assert!((world_y_before - world_y_after).abs() < 1e-4);
}

#[test]
fn test_pan_delta_scales_with_zoom() {
    let mut vp = create_test_viewport(1000, 1000, 100, 100);

    // Pan 10 columns at 1.0x zoom
    vp.zoom = 1.0;
    let offset_before = vp.offset_x;
    vp.pan(10.0, 0.0);
    let delta_zoom_1 = (vp.offset_x - offset_before).abs();

    // Pan 10 columns at 2.0x zoom (higher magnification)
    vp.zoom = 2.0;
    let offset_before = vp.offset_x;
    vp.pan(10.0, 0.0);
    let delta_zoom_2 = (vp.offset_x - offset_before).abs();

    // At double zoom, panning delta in world units must be half as large (greater precision)
    assert!((delta_zoom_1 - 2.0 * delta_zoom_2).abs() < 1e-4);
}

#[test]
fn test_clamping_prevents_lost_canvas() {
    let mut vp = create_test_viewport(1000, 1000, 100, 100);

    // Attempt massive panning out of bounds
    vp.pan(50000.0, 50000.0);
    assert!(!vp.offset_x.is_nan() && !vp.offset_y.is_nan());
    assert!(vp.offset_x < 1000.0 && vp.offset_y < 1000.0);

    vp.pan(-100000.0, -100000.0);
    assert!(!vp.offset_x.is_nan() && !vp.offset_y.is_nan());
}
