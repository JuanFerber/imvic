//! Pure mathematical unit tests for 2D viewport and camera transformations.

use image::RgbaImage;
use imvic::decoder::{CropRect, ImageSource};
use imvic::input::touchpad::{ZOOM_STEP_IN, ZOOM_STEP_OUT};
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

    fn render_crop(
        &self,
        _crop: CropRect,
        target_w: u32,
        target_h: u32,
        _bg_color: Option<[u8; 4]>,
    ) -> RgbaImage {
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

    // Cursor at exact center of terminal (col 50, row 25 -> u=0.5, v=0.5)
    let cursor_col = 50;
    let cursor_row = 25;

    let crop_before = vp.current_crop(1000, 800);
    let world_x_before = crop_before.x + 0.5 * crop_before.width;
    let world_y_before = crop_before.y + 0.5 * crop_before.height;

    // Apply 3.0x zoom
    vp.zoom_at(cursor_col, cursor_row, 3.0);

    let crop_after = vp.current_crop(1000, 800);
    let world_x_after = crop_after.x + 0.5 * crop_after.width;
    let world_y_after = crop_after.y + 0.5 * crop_after.height;

    // Center offset must remain strictly 0.0
    assert!(
        vp.offset_x.abs() < 1e-4,
        "Offset X drifted from center: {}",
        vp.offset_x
    );
    assert!(
        vp.offset_y.abs() < 1e-4,
        "Offset Y drifted from center: {}",
        vp.offset_y
    );

    // Point under cursor must not shift
    assert!((world_x_before - world_x_after).abs() < 1e-3);
    assert!((world_y_before - world_y_after).abs() < 1e-3);
}

#[test]
fn test_subpixel_focal_invariance_arbitrary_points() {
    let test_points = [(10, 10), (80, 20), (25, 40), (95, 45)];

    for &(col, row) in &test_points {
        let mut vp = create_test_viewport(1920, 1080, 100, 50);
        let u = col as f32 / vp.term_cols as f32;
        let v = row as f32 / vp.term_rows as f32;

        let crop_before = vp.current_crop(1920, 1080);
        let world_x_before = crop_before.x + u * crop_before.width;
        let world_y_before = crop_before.y + v * crop_before.height;

        vp.zoom_at(col, row, 1.75);

        let crop_after = vp.current_crop(1920, 1080);
        let world_x_after = crop_after.x + u * crop_after.width;
        let world_y_after = crop_after.y + v * crop_after.height;

        assert!(
            (world_x_before - world_x_after).abs() < 1e-3,
            "Focal drift at ({}, {}): before={}, after={}",
            col,
            row,
            world_x_before,
            world_x_after
        );
        assert!(
            (world_y_before - world_y_after).abs() < 1e-3,
            "Focal drift at ({}, {}): before={}, after={}",
            col,
            row,
            world_y_before,
            world_y_after
        );
    }
}

#[test]
fn test_logarithmic_zoom_reversibility_cycle() {
    let mut vp = create_test_viewport(1000, 1000, 100, 100);
    let step_in = ZOOM_STEP_IN;
    let step_out = ZOOM_STEP_OUT;

    // Zoom in 50 times, then out 50 times
    for _ in 0..50 {
        vp.zoom_at(60, 40, step_in);
    }
    assert!(vp.zoom > 3.0);

    for _ in 0..50 {
        vp.zoom_at(60, 40, step_out);
    }

    assert!(
        (vp.zoom - 1.0).abs() < 1e-3,
        "Zoom failed to reverse: {}",
        vp.zoom
    );
    assert!(
        vp.offset_x.abs() < 1e-2 && vp.offset_y.abs() < 1e-2,
        "Offsets failed to reverse: ({}, {})",
        vp.offset_x,
        vp.offset_y
    );
}

#[test]
fn test_boundary_clamping_stability() {
    let mut vp = create_test_viewport(1000, 1000, 100, 100);
    vp.zoom = imvic::viewport::MAX_ZOOM;

    let offset_before = (vp.offset_x, vp.offset_y);
    // Attempt zooming further past 10.0x
    vp.zoom_at(20, 20, 1.5);

    assert_eq!(vp.zoom, imvic::viewport::MAX_ZOOM);
    assert_eq!(vp.offset_x, offset_before.0);
    assert_eq!(vp.offset_y, offset_before.1);
    assert!(!vp.offset_x.is_nan() && !vp.offset_y.is_nan());
}

#[test]
fn test_coalesced_zoom_equivalence() {
    let mut vp1 = create_test_viewport(1000, 1000, 100, 100);
    let mut vp2 = create_test_viewport(1000, 1000, 100, 100);

    let factor = ZOOM_STEP_IN;
    let cursor = (70, 30);

    // Simulated uncoalesced: 10 individual calls
    for _ in 0..10 {
        vp1.zoom_at(cursor.0, cursor.1, factor);
    }

    // Coalesced: single accumulated call F = factor^10
    let coalesced_factor = factor.powi(10);
    vp2.zoom_at(cursor.0, cursor.1, coalesced_factor);

    assert!((vp1.zoom - vp2.zoom).abs() < 1e-4);
    assert!((vp1.offset_x - vp2.offset_x).abs() < 1e-3);
    assert!((vp1.offset_y - vp2.offset_y).abs() < 1e-3);
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

#[test]
fn test_clamping_symmetry_high_and_low_zoom() {
    let mut vp = create_test_viewport(9660, 2213, 100, 50);

    // High zoom (e.g. 7.3x / 730%)
    vp.zoom = 7.3;
    // Panning with positive delta moves camera towards the left edge (negative offset)
    vp.pan(100000.0, 0.0);
    let left_offset = vp.offset_x;
    // Panning with negative delta moves camera towards the right edge (positive offset)
    vp.pan(-200000.0, 0.0);
    let right_offset = vp.offset_x;

    assert!(
        (left_offset + right_offset).abs() < 1e-4,
        "High zoom clamping must be symmetric"
    );
    assert!(left_offset < 0.0 && right_offset > 0.0);

    // Verify user can reach the extreme left edge
    vp.pan(200000.0, 0.0);
    let crop_at_left = vp.current_crop(1000, 500);
    assert!(
        crop_at_left.x <= 0.0,
        "User must be able to pan to left edge at high zoom"
    );

    // Verify user can reach the extreme right edge
    vp.pan(-200000.0, 0.0);
    let crop_at_right = vp.current_crop(1000, 500);
    assert!(
        crop_at_right.x + crop_at_right.width >= 9660.0,
        "User must be able to pan to right edge at high zoom"
    );

    // Low zoom (e.g. MIN_ZOOM = 0.50x / 50%)
    vp.zoom = imvic::viewport::MIN_ZOOM;
    vp.pan(100000.0, 0.0);
    let left_offset_low = vp.offset_x;
    vp.pan(-200000.0, 0.0);
    let right_offset_low = vp.offset_x;
    assert!(
        (left_offset_low + right_offset_low).abs() < 1e-4,
        "Low zoom clamping must be symmetric"
    );
    assert!(
        right_offset_low > 0.0,
        "User must be able to pan in positive direction at low zoom"
    );
}

#[test]
fn test_zoom_limits_enforced() {
    let mut vp = create_test_viewport(1000, 1000, 100, 100);

    // Attempt zooming way out below MIN_ZOOM (0.50)
    for _ in 0..50 {
        vp.zoom_at(50, 50, 0.5);
    }
    assert!((vp.zoom - imvic::viewport::MIN_ZOOM).abs() < 1e-4);

    // Attempt zooming way in above MAX_ZOOM (10.0)
    for _ in 0..50 {
        vp.zoom_at(50, 50, 2.0);
    }
    assert!((vp.zoom - imvic::viewport::MAX_ZOOM).abs() < 1e-4);
}
