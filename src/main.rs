//! Imvic: High-performance, modular terminal image viewer with terminal-native graphics presentation.

use anyhow::Result;
use clap::Parser;
use crossterm::terminal::{WindowSize, size, window_size};
use imvic::cli::{CliArgs, parse_bg_color};
use imvic::decoder::DecoderRegistry;
use imvic::display::backend::select_backend;
use imvic::display::{HudState, TerminalGuard};
use imvic::input::AppEvent;
use imvic::input::InputRegistry;
use imvic::viewport::ViewportState;
use imvic::watcher::FileWatcher;
use std::io::{BufWriter, Write, stdout};
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

/// Capacity of the standard output buffer in bytes (256 KB).
///
/// Pre-allocating a large buffer ensures high-throughput graphics payloads flush in single system calls.
const STDOUT_BUFFER_CAPACITY: usize = 256 * 1024;

/// Maximum number of crossterm input events to drain within a single frame tick.
///
/// Prevents event starvation during high-frequency touchpad scrolling.
const MAX_EVENTS_PER_FRAME: usize = 128;

/// Idle polling timeout when no visual updates are pending (~60 Hz idle tick).
const IDLE_POLL_TIMEOUT: Duration = Duration::from_millis(16);

/// Fallback character cell dimensions in pixels (width, height).
///
/// Used when neither the terminal emulator nor multiplexer reports physical cell dimensions.
/// Assumes a standard 1:2 aspect ratio monospace font matrix.
const DEFAULT_CELL_PIXEL_SIZE: (u16, u16) = (11, 22);

/// Coalesces high-frequency touchpad and crossterm events within a single 144 Hz frame tick.
#[derive(Debug, Default)]
struct InputCoalescer {
    /// Accumulated horizontal pan displacement in terminal columns.
    pan_dx: f32,
    /// Accumulated vertical pan displacement in terminal rows.
    pan_dy: f32,
    /// Multiplicative compound zoom factor (calibrated to Weber-Fechner scale).
    zoom_factor: f32,
    /// Anchor cursor cell coordinates for the compounded focal zoom.
    last_zoom_cursor: Option<(u16, u16)>,
    /// Pending terminal window dimension update (cols, rows).
    resize: Option<(u16, u16)>,
    /// Flag indicating whether camera should re-center on image.
    center_view: bool,
    /// Flag indicating whether the user requested to terminate the session.
    quit: bool,
}

impl InputCoalescer {
    /// Creates a new coalescer initialized with neutral multipliers.
    fn new() -> Self {
        Self {
            zoom_factor: 1.0,
            ..Default::default()
        }
    }

    /// Records an incoming high-level event, accumulating continuous gestures.
    fn record(&mut self, event: AppEvent) {
        match event {
            AppEvent::Quit => self.quit = true,
            AppEvent::Pan { delta_x, delta_y } => {
                self.pan_dx += delta_x;
                self.pan_dy += delta_y;
            }
            AppEvent::ZoomIn {
                cursor_x,
                cursor_y,
                factor,
            }
            | AppEvent::ZoomOut {
                cursor_x,
                cursor_y,
                factor,
            } => {
                self.zoom_factor *= factor;
                self.last_zoom_cursor = Some((cursor_x, cursor_y));
            }
            AppEvent::Resize { cols, rows } => {
                self.resize = Some((cols, rows));
            }
            AppEvent::CenterView => {
                self.center_view = true;
            }
            AppEvent::FileModified => {}
        }
    }

    /// Applies accumulated pan deltas, logarithmic zoom, and resize events to the viewport.
    /// Returns `true` if the viewport state changed and requires a redraw.
    fn apply(self, viewport: &mut ViewportState) -> bool {
        let mut dirty = false;

        if let Some((cols, rows)) = self.resize {
            viewport.set_terminal_size(cols, rows.saturating_sub(1).max(1));
            dirty = true;
        }

        if self.center_view {
            viewport.center_view();
            dirty = true;
        }

        if self.pan_dx != 0.0 || self.pan_dy != 0.0 {
            viewport.pan(self.pan_dx, self.pan_dy);
            dirty = true;
        }

        if (self.zoom_factor - 1.0).abs() > 1e-6
            && let Some((cx, cy)) = self.last_zoom_cursor
        {
            viewport.zoom_at(cx, cy, self.zoom_factor);
            dirty = true;
        }

        dirty
    }

    /// Checks if any terminal resize event was recorded in this frame tick.
    fn has_resize(&self) -> bool {
        self.resize.is_some()
    }
}

fn main() -> Result<()> {
    let args = CliArgs::parse();

    // 1. Verify target file exists before entering raw mode
    if !args.file.exists() {
        eprintln!("Error: File not found at {:?}", args.file);
        std::process::exit(1);
    }

    // 2. Preflight check: detect transport tunnel and verify terminal graphics capabilities
    let transport = imvic::display::transport::detect_transport();
    let mut backend = select_backend(transport.as_ref())?;

    // 3. Load initial image source using the decoder registry
    let decoder_registry = DecoderRegistry::new();
    let source = decoder_registry.decode(&args.file)?;

    // 4. Query initial terminal size (reserve bottom row for HUD)
    let (cols, rows) = size()?;
    let view_rows = rows.saturating_sub(1).max(1);

    // 5. Initialize camera viewport and HUD
    let mut viewport = ViewportState::new(source.clone(), cols, view_rows);
    viewport.set_bg_color(parse_bg_color(args.bg.as_deref()));
    if let Some(scale) = args.scale {
        viewport.zoom_at(cols / 2, view_rows / 2, scale);
    }

    let mut hud = HudState::new(source.dimensions());
    hud.zoom_factor = viewport.zoom;
    hud.offset = (viewport.offset_x, viewport.offset_y);

    // 6. Enter raw mode and alternate screen protected by RAII TerminalGuard
    let mut guard = TerminalGuard::new()?;
    let mut input_registry = InputRegistry::new();

    // 7. Initialize background live reload watcher if enabled
    let (watcher_tx, watcher_rx) = channel();
    let _watcher = if args.watch {
        match FileWatcher::new(&args.file, watcher_tx) {
            Ok(watcher) => Some(watcher),
            Err(err) => {
                hud.set_error(format!("Watcher failed: {}", err));
                None
            }
        }
    } else {
        None
    };

    let mut out = BufWriter::with_capacity(STDOUT_BUFFER_CAPACITY, stdout());
    let mut dirty = true;

    let min_frame_duration = args
        .fps
        .map(|fps| Duration::from_secs_f64(1.0 / fps.max(1) as f64));
    let mut last_frame_time = Instant::now();

    // 8. 60 FPS Interactive Event Loop
    loop {
        // A. Check for file modifications from live reload watcher
        while let Ok(AppEvent::FileModified) = watcher_rx.try_recv() {
            match decoder_registry.decode(&args.file) {
                Ok(new_source) => {
                    viewport.set_source(new_source.clone());
                    hud.dimensions = new_source.dimensions();
                    hud.clear_error();
                }
                Err(err) => {
                    // Invariant: NEVER CRASH on corrupt or partial save
                    hud.set_error(err.to_string());
                }
            }
            dirty = true;
        }

        // B. Process crossterm events (drain pending input queue to prevent lag)
        let poll_timeout = if dirty {
            Duration::from_millis(0)
        } else {
            IDLE_POLL_TIMEOUT
        };

        let mut should_quit = false;

        if crossterm::event::poll(poll_timeout)? {
            let mut drained = 0;
            let mut coalescer = InputCoalescer::new();

            while drained < MAX_EVENTS_PER_FRAME
                && crossterm::event::poll(Duration::from_millis(0))?
            {
                let raw_event = crossterm::event::read()?;
                drained += 1;
                if let Some(app_event) = input_registry.handle_event(&raw_event) {
                    coalescer.record(app_event);
                }
            }

            if coalescer.quit {
                should_quit = true;
            }

            if coalescer.has_resize() {
                guard.refresh_transport();
            }

            if coalescer.apply(&mut viewport) {
                dirty = true;
            }
        }

        if should_quit {
            // Graceful exit: request active graphics backend to purge placed textures
            let _ = backend.clear_graphics(&mut out, guard.transport());
            let _ = out.flush();
            break;
        }

        // C. Render frame if dirty
        if dirty {
            if let Some(min_duration) = min_frame_duration {
                let elapsed = last_frame_time.elapsed();
                if elapsed < min_duration {
                    std::thread::sleep(min_duration - elapsed);
                }
            }
            last_frame_time = Instant::now();

            let (curr_cols, curr_rows) = size()?;
            let view_rows = curr_rows.saturating_sub(1).max(1);

            // Compute exact integer cell dimensions to guarantee zero aspect-ratio gap in high-resolution terminal displays
            let (cell_w, cell_h) = match window_size() {
                Ok(WindowSize {
                    width,
                    height,
                    columns,
                    rows,
                }) if width > 0 && height > 0 && columns > 0 && rows > 0 => {
                    ((width / columns).max(1), (height / rows).max(1))
                }
                _ => {
                    if let Some((cw, ch)) = guard.transport().cell_size() {
                        (cw, ch)
                    } else {
                        DEFAULT_CELL_PIXEL_SIZE
                    }
                }
            };

            let target_w = curr_cols as u32 * cell_w as u32;
            let target_h = view_rows as u32 * cell_h as u32;

            // Render camera crop
            let frame = viewport.render_frame(target_w, target_h);

            // Position to (1, 1) and draw image
            write!(out, "\x1b[1;1H")?;
            backend.draw_image(&mut out, guard.transport(), &frame, (curr_cols, view_rows))?;

            // Update and render HUD at bottom row
            hud.zoom_factor = viewport.zoom;
            hud.offset = (viewport.offset_x, viewport.offset_y);
            hud.render(&mut out, curr_rows, curr_cols)?;

            out.flush()?;
            dirty = false;
        }
    }

    Ok(())
}
