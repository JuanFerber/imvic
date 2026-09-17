//! Imvic: High-performance, modular, GPU-accelerated terminal image and vector viewer.

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
use std::time::Duration;

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
    let guard = TerminalGuard::new()?;
    let mut input_registry = InputRegistry::new();

    // 7. Initialize background live reload watcher if enabled
    let (watcher_tx, watcher_rx) = channel();
    let _watcher = if args.watch {
        FileWatcher::new(&args.file, watcher_tx).ok()
    } else {
        None
    };

    let mut out = BufWriter::new(stdout());
    let mut dirty = true;

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
            Duration::from_millis(16)
        };

        let mut should_quit = false;

        if crossterm::event::poll(poll_timeout)? {
            let mut drained = 0;
            while drained < 8 && crossterm::event::poll(Duration::from_millis(0))? {
                let raw_event = crossterm::event::read()?;
                drained += 1;
                if let Some(app_event) = input_registry.handle_event(&raw_event) {
                    match app_event {
                        AppEvent::Quit => {
                            should_quit = true;
                            break;
                        }
                        AppEvent::Pan { delta_x, delta_y } => {
                            viewport.pan(delta_x, delta_y);
                            dirty = true;
                        }
                        AppEvent::ZoomIn {
                            cursor_x,
                            cursor_y,
                            factor,
                        } => {
                            viewport.zoom_at(cursor_x, cursor_y, factor);
                            dirty = true;
                        }
                        AppEvent::ZoomOut {
                            cursor_x,
                            cursor_y,
                            factor,
                        } => {
                            viewport.zoom_at(cursor_x, cursor_y, factor);
                            dirty = true;
                        }
                        AppEvent::Resize {
                            cols: new_cols,
                            rows: new_rows,
                        } => {
                            viewport.set_terminal_size(new_cols, new_rows.saturating_sub(1).max(1));
                            dirty = true;
                        }
                        AppEvent::FileModified => {}
                    }
                }
            }
        }

        if should_quit {
            break;
        }

        // C. Render frame if dirty
        if dirty {
            let (curr_cols, curr_rows) = size()?;
            let view_rows = curr_rows.saturating_sub(1).max(1);

            // Compute exact pixel dimensions using terminal pixel resolution if supported
            let (target_w, target_h) = match window_size() {
                Ok(WindowSize {
                    width,
                    height,
                    columns,
                    rows,
                }) if width > 0 && height > 0 && columns > 0 && rows > 0 => {
                    let cell_w = width as f32 / columns as f32;
                    let cell_h = height as f32 / rows as f32;
                    (
                        (curr_cols as f32 * cell_w).round() as u32,
                        (view_rows as f32 * cell_h).round() as u32,
                    )
                }
                _ => (curr_cols as u32 * 10, view_rows as u32 * 20),
            };

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
