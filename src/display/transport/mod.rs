//! Terminal transport adapters and multiplexer tunneling.
//!
//! Handles transparent escape sequence passthrough for environments like TMUX.

pub mod direct;
pub mod tmux;

pub use direct::DirectTransport;
pub use tmux::TmuxTransport;

/// Contract for terminal escape sequence transports and multiplexer tunnels.
pub trait TransportAdapter: Send + Sync {
    /// Human-readable transport name (e.g. "TMUX DCS Passthrough").
    fn name(&self) -> &'static str;

    /// Detects if this transport/multiplexer is active in the current session.
    fn is_active(&self) -> bool;

    /// Wraps a raw escape sequence into the required multiplexer escape envelope.
    fn wrap_escape(&self, raw: &[u8]) -> Vec<u8>;
}

/// Detects and returns the appropriate active transport adapter for the current environment.
pub fn detect_transport() -> Box<dyn TransportAdapter> {
    let tmux = TmuxTransport::new();
    if tmux.is_active() {
        Box::new(tmux)
    } else {
        Box::new(DirectTransport::new())
    }
}
