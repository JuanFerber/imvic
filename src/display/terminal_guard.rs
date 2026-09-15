//! RAII-based terminal state guard with hermetic 4-stage teardown.
//!
//! Guarantees raw mode restoration, graphics cleanup, cursor visibility,
//! and active stdin drainage to eliminate shell corruption (e.g. SGR escape leaks).

use super::transport::{TransportAdapter, detect_transport};
use anyhow::Result;
use crossterm::cursor::{Hide, Show};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture, poll, read};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use std::io::{Write, stdout};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

static PANIC_HOOK_SET: AtomicBool = AtomicBool::new(false);

/// RAII guard managing raw mode, alternate screen, mouse capture, and safe teardown.
pub struct TerminalGuard {
    transport: Box<dyn TransportAdapter>,
}

impl TerminalGuard {
    /// Initializes terminal environment: enters alternate screen, enables raw mode,
    /// hides cursor, enables mouse capture, and registers custom panic hook.
    pub fn new() -> Result<Self> {
        Self::setup_panic_hook();

        enable_raw_mode()?;

        let mut out = stdout();
        crossterm::execute!(out, EnterAlternateScreen, Hide, EnableMouseCapture)?;
        out.flush()?;

        let transport = detect_transport();

        Ok(Self { transport })
    }

    /// Accesses the active transport adapter (e.g. TMUX or Direct).
    pub fn transport(&self) -> &dyn TransportAdapter {
        self.transport.as_ref()
    }

    /// Registers custom panic hook to restore terminal state before printing backtrace.
    fn setup_panic_hook() {
        if !PANIC_HOOK_SET.swap(true, Ordering::SeqCst) {
            let default_hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |panic_info| {
                // Emergency cleanup: restore cooked mode and leave alternate screen
                let _ = disable_raw_mode();
                let mut out = stdout();
                let _ = crossterm::execute!(out, DisableMouseCapture, Show, LeaveAlternateScreen);
                let _ = out.flush();

                // Forward to standard panic logger
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

        // Stage 2: Clear placed GPU graphics and make cursor visible
        let clear_cmd = b"\x1b_Ga=d,d=a\x1b\\";
        let wrapped_clear = self.transport.wrap_escape(clear_cmd);
        let _ = out.write_all(&wrapped_clear);
        let _ = crossterm::execute!(out, Show);
        let _ = out.flush();

        // Stage 3: ACTIVELY DRAIN STDIN to prevent trailing mouse packets (e.g. ;23M)
        // from leaking into the user's shell (zsh/bash).
        while let Ok(true) = poll(Duration::from_millis(15)) {
            let _ = read();
        }

        // Stage 4: Leave alternate screen and restore cooked mode
        let _ = crossterm::execute!(out, LeaveAlternateScreen);
        let _ = out.flush();
        let _ = disable_raw_mode();
    }
}
