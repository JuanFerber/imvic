# imvic (Image Viewer CLI)

[![CI](https://github.com/JuanFerber/imvic/actions/workflows/ci.yml/badge.svg)](https://github.com/JuanFerber/imvic/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)
[![Rust: 2024 Edition](https://img.shields.io/badge/Rust-2024_Edition-orange.svg)](https://www.rust-lang.org/)
[![Clippy: Clean](https://img.shields.io/badge/Clippy-0%20warnings-brightgreen.svg)](<>)

> **High-performance, modular terminal image viewer with terminal-native graphics presentation and adaptive CPU rasterization.**

`imvic` is an interactive image and vector viewer built in Rust for modern terminal emulators supporting graphics protocols. It renders frames at the terminal's reported cell-pixel resolution, featuring smooth focal zoom, continuous panning, and robust terminal safety invariants.

---

## Key Architectural Highlights

- **Terminal-Native Graphics Protocol:** Targets high-resolution graphics presentation using the **Kitty Graphics Protocol** on supported terminals. Never falls back to degraded half-block Unicode characters.
- **Transparent Multiplexer Passthrough (TMUX):** Automatically detects `$TMUX` and wraps escape payloads in DCS passthrough sequences (`\x1bPtmux;\x1b...`), utilizing Unicode placeholders (`U+10EEEE`) and querying pane origin/cell geometry dynamically upon terminal resize.
- **Multi-Format Extensible Decoder Registry:**
  - **Vector (SVG):** Crisp mathematical rendering at arbitrary zoom levels via `resvg` and `tiny-skia` with frustum culling and adaptive overdraw cache.
  - **Raster:** Crop-and-resize camera rendering for **PNG**, **JPEG**, **WebP**, **GIF**, and **BMP** via `image`.
- **Sub-Pixel Camera & Logarithmic Focal Zoom:** Reciprocal multiplicative zoom steps provide smooth, reversible zooming around the cursor while preserving sub-pixel focal invariance, pan deltas scaled to zoom factor, and boundary clamping.
- **Terminal Safety & Hermetic Teardown:**
  - `TerminalGuard` with transactional rollback to cooked mode on initialization failure.
  - 4-stage RAII teardown on exit (disabling mouse tracking, purging terminal graphics, draining pending `stdin` escape sequences, and restoring cooked mode).
  - Panic hook that restores terminal state and drains `stdin` to prevent leaked escape sequences (such as `;23M` in `zsh`) before printing the backtrace.
- **Hot-Path Memory Optimization:** Reusable scratch buffers for PNG compression and Base64 encoding reduce repeated per-frame allocations in the encoding path. Frame output uses buffered writing with a single top-level flush per rendered frame.
- **Resilient Live Reload (`--watch`):** Watches the parent directory with a 200 ms event rate limiter to survive atomic file replacements. If a file is saved with syntax errors, `imvic` preserves the last valid frame and alerts the user via the HUD.

---

## Verified Environments

| Environment / Transport | Mode                     | Status       | Notes                                                    |
| :---------------------- | :----------------------- | :----------- | :------------------------------------------------------- |
| **Kitty**               | Native Graphics Protocol | **Verified** | Primary reference implementation and verified target.    |
| **Kitty + TMUX**        | DCS Passthrough          | **Verified** | Verified inside TMUX with `set -g allow-passthrough on`. |

_Note: Other terminal emulators implementing the Kitty Graphics Protocol (such as Ghostty or WezTerm) are target platforms whose specific conformance is undergoing progressive testing._

---

## Installation & Requirements

Ensure you have a modern Rust toolchain installed (Rust 1.88+ / 2024 edition):

```bash
# Clone the repository
git clone https://github.com/JuanFerber/imvic.git
cd imvic

# Build with release optimizations (Thin LTO, codegen-units = 1)
cargo build --release

# Install locally
cargo install --path .
```

---

## Usage

```bash
# View a vector or raster image
imvic photo.png
imvic diagram.svg

# Enable live reload (opt-in: automatically updates when the file changes on disk)
imvic --watch drawing.svg

# Override canvas background color using hex format (#RGB, #RGBA, #RRGGBB, #RRGGBBAA)
imvic --bg "#1e1e2e" illustration.png
imvic --bg "#ffffff" icon.svg

# Set initial zoom factor
imvic --scale 2.0 blueprint.svg

# Cap interactive frame rate
imvic --fps 60 animation.png
```

### Command-Line Arguments

| Argument  | Flag           | Description                                                                   |
| :-------- | :------------- | :---------------------------------------------------------------------------- |
| `<FILE>`  | _(Positional)_ | Path to the image or vector file to display.                                  |
| `--watch` | `-w`           | Watch file for changes and reload view automatically (Live Reload).           |
| `--scale` | `-s <FACTOR>`  | Initial zoom or scale factor override (e.g. `1.5`, `2.0`).                    |
| `--bg`    | `-b <HEX>`     | Canvas background color in `#RGB`, `#RGBA`, `#RRGGBB`, or `#RRGGBBAA` format. |
| `--fps`   | `--fps <FPS>`  | Target framerate limit in FPS (e.g. `30`, `60`, `144`). Defaults to uncapped. |

---

## Interactive Controls

| Input Gesture                          | Action             | Description                                                                     |
| :------------------------------------- | :----------------- | :------------------------------------------------------------------------------ |
| **Ctrl + Scroll / Ctrl + Mouse Wheel** | **Focal Zoom**     | Smooth zoom centered around current cursor position with reciprocal steps.      |
| **Mouse / Trackpad Scroll**            | **Continuous Pan** | Smooth 2D directional panning across the canvas.                                |
| **Mouse Left-Click Drag**              | **Drag Pan**       | Drags and shifts the canvas following cursor displacement.                      |
| **`c`** / **`C`** / **`Home`**         | **Center View**    | Recenters the image and resets camera offset.                                   |
| **`q`** / **`Esc`** / **`Ctrl+C`**     | **Quit**           | Gracefully cleans up terminal graphics, drains stdin, and restores cooked mode. |

---

## Testing & Verification Discipline

`imvic` enforces strict verification invariants: zero compiler warnings, zero linter warnings, and a deterministic test suite.

```bash
# 1. Type check and borrow check
cargo check

# 2. Strict linter (zero warnings policy)
cargo clippy -- -D warnings

# 3. Comprehensive automated test suite
cargo test
```

### Automated Tests Overview

- **Format Decoders & Protocol Units:** Format sniffing, SVG prologue validation, raster alpha blending, CLI hex parsing fallback cascade, HUD bounding box margins, and TMUX escape wrapping.
- **Mathematical Camera Invariants (`tests/viewport_math_tests.rs`):**
  - Sub-pixel focal invariance for arbitrary cursor coordinates.
  - Zoom-to-cursor invariance at center and edge positions.
  - Logarithmic reversibility cycles (reciprocal symmetry: `zoom_in * zoom_out ≈ 1.0`).
  - Pan delta scaling strictly proportional to camera zoom factor.
  - Clamping symmetry and boundary stability preventing lost canvas states.

---

## Current Limitations

- **Graphics Backend:** The Kitty Graphics Protocol is currently the sole graphics backend implemented.
- **Terminal Conformance:** Verified primarily on Kitty; Ghostty and WezTerm compatibility is undergoing progressive testing.
- **CPU Rasterization:** Frame rendering and cropping currently execute on the CPU; persistent terminal GPU placements are planned for a subsequent release.

---

## License

This project is licensed under the [MIT License](LICENSE).
