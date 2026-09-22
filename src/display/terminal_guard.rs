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
/// - `d=A`: Purge visible image placements on the active screen buffer.
/// - `\x1b\\`: Standard ANSI ST (String Terminator).
///
/// Emitted during teardown as a fail-safe to guarantee no lingering visual artifacts
/// remain in the terminal buffer after process exit.
const KITTY_CLEAR_ALL_GRAPHICS: &[u8] = b"\x1b_Ga=d,d=A\x1b\\";

/// Maximum wall-clock time allowed for draining pending stdin packets during teardown.
const MAX_DRAIN_DURATION: Duration = Duration::from_millis(50);

/// Maximum number of crossterm events consumed during stdin drain.
const MAX_DRAIN_EVENTS: usize = 64;

/// Timeout for individual event polling during the teardown drain phase.
const DRAIN_POLL_TIMEOUT: Duration = Duration::from_millis(5);

/// Actively drains pending stdin packets with an absolute deadline and event limit.
///
/// Prevents trailing mouse escape sequences (e.g. `;23M`) from leaking into the shell
/// while guaranteeing the process never hangs indefinitely if stdin is flooded.
fn drain_stdin() {
    let deadline = std::time::Instant::now() + MAX_DRAIN_DURATION;
    let mut drained_count = 0;

    while std::time::Instant::now() < deadline && drained_count < MAX_DRAIN_EVENTS {
        match poll(DRAIN_POLL_TIMEOUT) {
            Ok(true) => {
                let _ = read();
                drained_count += 1;
            }
            _ => break,
        }
    }
}

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
                drain_stdin();

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
        drain_stdin();

        // Stage 4: Leave alternate screen, restore line wrap, and restore cooked mode
        let _ = crossterm::execute!(out, EnableLineWrap, LeaveAlternateScreen);
        let _ = out.flush();
        let _ = disable_raw_mode();
    }
}
