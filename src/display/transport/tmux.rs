//! TMUX DCS passthrough transport adapter.
//!
//! Wraps terminal graphics escape sequences in DCS envelopes with doubled escape bytes.

use super::TransportAdapter;

/// TMUX DCS passthrough adapter.
#[derive(Clone, Copy)]
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

    fn is_passthrough_enabled(&self) -> bool {
        if !self.is_active() {
            return false;
        }

        // Query the running tmux server for its allow-passthrough setting
        let output = std::process::Command::new("tmux")
            .args(["show", "-gv", "allow-passthrough"])
            .output();

        match output {
            Ok(out) if out.status.success() => {
                let val = String::from_utf8_lossy(&out.stdout).trim().to_lowercase();
                val == "on" || val == "all"
            }
            _ => false,
        }
    }

    fn passthrough_enable_hint(&self) -> Option<&'static str> {
        Some("tmux set -g allow-passthrough on")
    }

    fn wrap_escape(&self, raw: &[u8]) -> Vec<u8> {
        // DCS prefix: \x1bPtmux;
        // Inside content: every \x1b must be escaped as \x1b\x1b
        // DCS suffix: \x1b\\
        let mut wrapped = Vec::with_capacity(raw.len() + 32);
        wrapped.extend_from_slice(b"\x1bPtmux;");

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

    fn clone_box(&self) -> Box<dyn TransportAdapter> {
        Box::new(*self)
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

        assert!(wrapped.starts_with(b"\x1bPtmux;"));
        assert!(wrapped.ends_with(b"\x1b\\"));
        assert_eq!(wrapped, b"\x1bPtmux;\x1b\x1b_Ga=T;\x1b\x1b\\\x1b\\");
    }
}
