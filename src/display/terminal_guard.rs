//! RAII-based terminal state guard with hermetic 4-stage teardown.
//!
//! Guarantees raw mode restoration, graphics cleanup, cursor visibility,
//! and active stdin drainage to eliminate shell corruption (e.g. SGR escape leaks).

use super::transport::{TransportAdapter, detect_transport};
use anyhow::Result;
use crossterm::cursor::{Hide, Show};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture, poll, read};
use crossterm::terminal::{
    DisableLineWrap, EnableLineWrap, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
    enable_raw_mode,
};
use std::io::{Write, stdout};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

static PANIC_HOOK_SET: AtomicBool = AtomicBool::new(false);

/// Kitty Graphics Protocol escape sequence to universally purge all placed terminal graphic resources.
///
/// Breakdown:
/// - `\x1b_G`: Kitty APC (Application Program Command) graphics payload initiator.
/// - `a=d`: Action is Delete.
/// - `d=A`: Delete all images across all virtual screens and z-indexes.
/// - `\x1b\\`: Standard ANSI ST (String Terminator).
///
/// Emitted during teardown as a fail-safe to guarantee no lingering visual artifacts
/// remain in the terminal buffer after process exit.
const KITTY_CLEAR_ALL_GRAPHICS: &[u8] = b"\x1b_Ga=d,d=A\x1b\\";

/// Maximum time to poll `stdin` during teardown to consume in-flight mouse escape sequences.
///
/// 15 milliseconds is calibrated to exceed terminal emulator packet latency for trailing
/// mouse release events (e.g. SGR 1006 escape sequences) while remaining unnoticeable to the user.
const STDIN_DRAIN_TIMEOUT: Duration = Duration::from_millis(15);

/// RAII guard managing raw mode, alternate screen, mouse capture, and safe teardown.
pub struct TerminalGuard {
    transport: Box<dyn TransportAdapter>,
}

impl TerminalGuard {
    /// Initializes terminal environment: enters alternate screen, enables raw mode,
    /// hides cursor, enables mouse capture, and registers custom panic hook.
    ///
    /// Guarantees safe rollback to cooked mode if any intermediate initialization step fails.
    pub fn new() -> Result<Self> {
        Self::setup_panic_hook();

        enable_raw_mode()?;

        let mut out = stdout();
        if let Err(err) = crossterm::execute!(
            out,
            EnterAlternateScreen,
            Hide,
            EnableMouseCapture,
            // Disable auto line-wrap to prevent scrolling artifacts when drawing on the rightmost edge
            DisableLineWrap
        )
        .and_then(|_| out.flush())
        {
            // Rollback: restore cooked mode and reset any partially set terminal state
            let _ = crossterm::execute!(
                out,
                Show,
                DisableMouseCapture,
                EnableLineWrap,
                LeaveAlternateScreen
            );
            let _ = out.flush();
            let _ = disable_raw_mode();
            return Err(err.into());
        }

        let transport = detect_transport();

        Ok(Self { transport })
    }

    /// Accesses the active transport adapter used for escape wrapping.
    pub fn transport(&self) -> &dyn TransportAdapter {
        self.transport.as_ref()
    }

    /// Refreshes dynamic properties of the active transport adapter (e.g. on window/pane resize).
    pub fn refresh_transport(&mut self) {
        self.transport.refresh();
    }

    /// Registers custom panic hook to restore terminal state before printing backtrace.
    ///
    /// Reproduces the full hermetic teardown sequence (mouse disable, terminal graphics purge,
    /// stdin drainage, alternate screen exit, and cooked mode restoration) even under `panic = "abort"`.
    fn setup_panic_hook() {
        if !PANIC_HOOK_SET.swap(true, Ordering::SeqCst) {
            let default_hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |panic_info| {
                let mut out = stdout();

                // Stage 1: Disable mouse tracking escape sequences immediately
                let _ = crossterm::execute!(out, DisableMouseCapture);
                let _ = out.flush();

                // Stage 2: Clear placed terminal graphics and make cursor visible
                let transport = detect_transport();
                let wrapped_clear = transport.wrap_escape(KITTY_CLEAR_ALL_GRAPHICS);
                let _ = out.write_all(&wrapped_clear);
                let _ = crossterm::execute!(out, Show);
                let _ = out.flush();

                // Stage 3: Drain pending stdin packets to prevent shell leakage
                while let Ok(true) = poll(STDIN_DRAIN_TIMEOUT) {
                    let _ = read();
                }

                // Stage 4: Leave alternate screen, restore line wrap, and restore cooked mode
                let _ = crossterm::execute!(out, EnableLineWrap, LeaveAlternateScreen);
                let _ = out.flush();
                let _ = disable_raw_mode();

                // Forward to standard panic logger to print clean backtrace to stderr
                default_hook(panic_info);
            }));
        }
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let mut out = stdout();

        // Stage 1: Disable mouse tracking escape sequences immediately
        let _ = crossterm::execute!(out, DisableMouseCapture);
        let _ = out.flush();

        // Stage 2: Clear placed terminal graphics and make cursor visible
        let wrapped_clear = self.transport.wrap_escape(KITTY_CLEAR_ALL_GRAPHICS);
        let _ = out.write_all(&wrapped_clear);
        let _ = crossterm::execute!(out, Show);
        let _ = out.flush();

        // Stage 3: ACTIVELY DRAIN STDIN to prevent trailing mouse packets (e.g. ;23M)
        // from leaking into the user's shell (zsh/bash).
        while let Ok(true) = poll(STDIN_DRAIN_TIMEOUT) {
            let _ = read();
        }

        // Stage 4: Leave alternate screen, restore line wrap, and restore cooked mode
        let _ = crossterm::execute!(out, EnableLineWrap, LeaveAlternateScreen);
        let _ = out.flush();
        let _ = disable_raw_mode();
    }
}
