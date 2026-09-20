//! Terminal transport adapters and multiplexer tunneling.
//!
//! Handles transparent escape sequence passthrough for multiplexers like TMUX and Zellij.

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

    /// Indicates whether this transport represents a multiplexer (e.g. Tmux, Zellij)
    /// that requires virtual pane isolation and text-cell anchoring.
    fn is_multiplexer(&self) -> bool {
        false
    }

    /// Checks if this transport permits passthrough escape sequences to the host terminal.
    fn is_passthrough_enabled(&self) -> bool;

    /// Command or guidance to enable passthrough if currently disabled.
    fn passthrough_enable_hint(&self) -> Option<&'static str>;

    /// Wraps a raw escape sequence into the required multiplexer escape envelope.
    fn wrap_escape(&self, raw: &[u8]) -> Vec<u8>;

    /// Physical terminal coordinates (row, col) where the current pane/session begins (1-indexed).
    fn physical_origin(&self) -> (u16, u16) {
        (1, 1)
    }

    /// Physical pixel dimensions of a single terminal cell (width, height), if reported by the transport.
    fn cell_size(&self) -> Option<(u16, u16)> {
        None
    }

    /// Clones this transport into a trait object.
    fn clone_box(&self) -> Box<dyn TransportAdapter>;
}

/// Registry managing all supported terminal multiplexer transport plugins.
pub struct TransportRegistry {
    transports: Vec<Box<dyn TransportAdapter>>,
}

impl TransportRegistry {
    /// Creates a registry pre-loaded with supported multiplexer plugins.
    pub fn new() -> Self {
        let mut registry = Self {
            transports: Vec::new(),
        };

        // Register multiplexer plugins (Zellij, Screen, etc. register here)
        registry.register(Box::new(TmuxTransport::new()));

        registry
    }

    /// Registers an additional multiplexer transport plugin.
    pub fn register(&mut self, transport: Box<dyn TransportAdapter>) {
        self.transports.push(transport);
    }

    /// Iterates over registered multiplexers and returns the active one,
    /// or falls back to DirectTransport if running directly in the terminal.
    pub fn detect_transport(&self) -> Box<dyn TransportAdapter> {
        for transport in &self.transports {
            if transport.is_active() {
                return transport.clone_box();
            }
        }

        Box::new(DirectTransport::new())
    }
}

impl Default for TransportRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Global detection entry point using the TransportRegistry.
pub fn detect_transport() -> Box<dyn TransportAdapter> {
    let registry = TransportRegistry::new();
    registry.detect_transport()
}
