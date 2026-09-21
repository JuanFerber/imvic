# imvic (Image Viewer CLI)

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)
[![Rust: 2024 Edition](https://img.shields.io/badge/Rust-2024_Edition-orange.svg)](https://www.rust-lang.org/)
[![Clippy: Clean](https://img.shields.io/badge/Clippy-0%20warnings-brightgreen.svg)]()

> **High-performance, modular terminal image viewer with terminal-native graphics presentation and adaptive CPU rasterization.**

`imvic` is an interactive image and vector viewer built from the ground up in Rust for modern terminal emulators supporting graphics protocols. It delivers monitor-native resolution rendering, smooth focal zoom, continuous panning, and robust terminal safety invariants.

---

## Key Architectural Highlights

* **Terminal-Native Graphics Protocol:** Targets high-resolution graphics presentation using the **Kitty Graphics Protocol** on supported terminals. Never falls back to degraded half-block Unicode characters.
* **Transparent Multiplexer Passthrough (TMUX):** Automatically detects `$TMUX` and wraps escape payloads in DCS passthrough sequences (`\x1bPtmux;\x1b...`), utilizing Unicode placeholders (`U+10EEEE`) and querying pane origin/cell geometry dynamically upon terminal resize.
* **Multi-Format Extensible Decoder Registry:**
  * **Vector (SVG):** Crisp mathematical rendering at arbitrary zoom levels via `resvg` and `tiny-skia` with frustum culling and adaptive overdraw cache.
  * **Raster:** Instant decoding and affine-transformed camera projection for **PNG**, **JPEG**, **WebP**, **GIF**, and **BMP** via `image-rs`.
* **Sub-Pixel Camera & Focal Zoom:** Mathematically verified zoom-to-cursor invariance based on the Weber-Fechner law, logarithmic reversibility cycles, pan deltas proportional to zoom scale, and boundary clamping.
* **Terminal Safety & Hermetic RAII Invariants:**
  * `TerminalGuard` with transactional rollback to cooked mode on initialization failure.
  * 4-stage panic hook that restores the cursor, purges terminal graphic resources, drains pending `stdin` escape sequences (preventing leaked bytes like `;23M` in `zsh`), and leaves the alternate screen before printing the backtrace.
* **Hot-Path Memory Optimization:** Reusable scratch buffers for PNG compression and Base64 encoding eliminate per-frame allocations in interactive loops. Frame output is written via a buffered system call in a single atomic flush.
* **Resilient Live Reload (`--watch`):** Watches the parent directory with a 200 ms debounce to survive atomic file replacements. If a file is saved with syntax errors, `imvic` preserves the last valid frame and alerts the user via the HUD.

---

## Verified Environments

| Environment | Mode | Status | Notes |
| :--- | :--- | :--- | :--- |
| **Kitty** | Native Graphics | **Verified** | Primary reference implementation and verified target. |
| **TMUX** | DCS Passthrough | **Verified** | Verified inside TMUX with `set -g allow-passthrough on`. |

*Note: Other terminal emulators implementing the Kitty Graphics Protocol (such as Ghostty or WezTerm) are target platforms whose specific conformance is undergoing progressive testing.*

---

## Installation & Requirements

Ensure you have a modern Rust toolchain installed (Rust 1.85+ / 2024 edition):

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

# Enable live reload (automatically updates when the file changes on disk)
imvic --watch drawing.svg

# Override canvas background color (supports #RGB, #RGBA, #RRGGBB, #RRGGBBAA, or named colors)
imvic --bg "#1e1e2e" illustration.png
imvic --bg white icon.svg

# Set initial zoom factor
imvic --scale 2.0 blueprint.svg

# Cap interactive frame rate
imvic --fps 60 animation.png
```

### Command-Line Arguments

| Argument | Flag | Description |
| :--- | :--- | :--- |
| `<FILE>` | *(Positional)* | Path to the image or vector file to display. |
| `--watch` | `-w` | Watch file for changes and reload view automatically (Live Reload). |
| `--scale` | `-s <FACTOR>` | Initial zoom or scale factor override (e.g. `1.5`, `2.0`). |
| `--bg` | `-b <COLOR>` | Canvas background color in hex format or `white`/`black`/`transparent`. |
| `--fps` | `--fps <FPS>` | Target framerate limit in FPS (e.g. `30`, `60`, `144`). Defaults to uncapped. |

---

## Interactive Controls

| Input Gesture | Action | Description |
| :--- | :--- | :--- |
| **Touchpad Pinch / Ctrl + Mouse Wheel** | **Focal Zoom** | Smooth zoom centered around current cursor position (Weber-Fechner logarithmic steps). |
| **Touchpad 2-Finger Scroll** | **Continuous Pan** | Smooth 2D directional panning across the canvas. |
| **Mouse Left-Click Drag** | **Drag Pan** | Drags and shifts the canvas following cursor displacement. |
| **`c`** / **`C`** / **`Home`** | **Center View** | Recenters the image and resets camera offset. |
| **`q`** / **`Esc`** / **`Ctrl+C`** | **Quit** | Gracefully cleans up terminal graphics, drains stdin, and restores cooked mode. |

---

## Testing & Verification Discipline

`imvic` enforces strict verification invariants: zero compiler warnings, zero linter warnings, and a pure mathematical test suite.

```bash
# 1. Type check and borrow check
cargo check

# 2. Strict linter (zero warnings policy)
cargo clippy -- -D warnings

# 3. Comprehensive automated test suite
cargo test
```

### Automated Tests Overview

* **Unit Tests:** Format sniffing, SVG prologue validation, raster alpha blending, CLI hex parsing fallback cascade, HUD bounding box margins, and TMUX escape doubling.
* **Mathematical Camera Invariants (`tests/viewport_math_tests.rs`):**
  * Sub-pixel focal invariance for arbitrary cursor coordinates.
  * Zoom-to-cursor invariance at center and edge positions.
  * Logarithmic reversibility cycles (Weber-Fechner symmetry: `zoom_in * zoom_out ≈ 1.0`).
  * Pan delta scaling strictly proportional to camera zoom factor.
  * Clamping symmetry and boundary stability preventing lost canvas states.

---

## License

This project is licensed under the [MIT License](LICENSE).
