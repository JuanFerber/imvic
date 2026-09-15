//! TMUX DCS passthrough transport adapter.
//!
//! Wraps terminal graphics escape sequences in DCS envelopes with doubled escape bytes.

use super::TransportAdapter;

/// TMUX DCS passthrough adapter.
pub struct TmuxTransport;

impl TmuxTransport {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TmuxTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl TransportAdapter for TmuxTransport {
    fn name(&self) -> &'static str {
        "TMUX DCS Passthrough"
    }

    fn is_active(&self) -> bool {
        std::env::var_os("TMUX").is_some()
    }

    fn wrap_escape(&self, raw: &[u8]) -> Vec<u8> {
        // DCS prefix: \x1bPtmux;\x1b
        // Inside content: every \x1b must be escaped as \x1b\x1b
        // DCS suffix: \x1b\\
        let mut wrapped = Vec::with_capacity(raw.len() + 32);
        wrapped.extend_from_slice(b"\x1bPtmux;\x1b");

        for &byte in raw {
            if byte == 0x1b {
                wrapped.push(0x1b);
                wrapped.push(0x1b);
            } else {
                wrapped.push(byte);
            }
        }

        wrapped.extend_from_slice(b"\x1b\\");
        wrapped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tmux_wrapping_and_escape_doubling() {
        let transport = TmuxTransport::new();
        let raw = b"\x1b_Ga=T;\x1b\\";
        let wrapped = transport.wrap_escape(raw);

        // Expect: \x1bPtmux;\x1b + \x1b\x1b_Ga=T;\x1b\x1b\\ + \x1b\\
        assert!(wrapped.starts_with(b"\x1bPtmux;\x1b"));
        assert!(wrapped.ends_with(b"\x1b\\"));
        assert_eq!(wrapped, b"\x1bPtmux;\x1b\x1b\x1b_Ga=T;\x1b\x1b\\\x1b\\");
    }
}
