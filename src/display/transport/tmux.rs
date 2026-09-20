//! TMUX DCS passthrough transport adapter.
//!
//! Wraps terminal graphics escape sequences in DCS envelopes with doubled escape bytes.

use super::TransportAdapter;

/// TMUX DCS passthrough adapter with dynamic pane, status bar, and cell dimensions tracking.
#[derive(Clone, Copy)]
pub struct TmuxTransport {
    /// 1-indexed (row, col) physical terminal coordinates where this tmux pane begins.
    origin: (u16, u16),
    /// Cell pixel dimensions (width, height) queried from tmux, if supported by the client.
    cell_size: Option<(u16, u16)>,
}

impl TmuxTransport {
    pub fn new() -> Self {
        let (origin, cell_size) = Self::query_info();
        Self { origin, cell_size }
    }

    /// Queries the active tmux server for pane coordinates, status bar position,
    /// and host terminal cell pixel dimensions.
    pub fn query_info() -> ((u16, u16), Option<(u16, u16)>) {
        if std::env::var_os("TMUX").is_none() {
            return ((1, 1), None);
        }

        let mut cmd = std::process::Command::new("tmux");
        cmd.arg("display-message");
        if let Ok(pane_id) = std::env::var("TMUX_PANE") {
            cmd.args(["-t", &pane_id]);
        }
        cmd.args([
            "-p",
            "#{pane_top},#{pane_left},#{status-position},#{status},#{client_cell_width},#{client_cell_height}",
        ]);
        let output = cmd.output();

        if let Ok(out) = output
            && out.status.success()
        {
            let s = String::from_utf8_lossy(&out.stdout);
            let parts: Vec<&str> = s.trim().split(',').collect();
            if parts.len() >= 4 {
                let pane_top: u16 = parts[0].parse().unwrap_or(0);
                let pane_left: u16 = parts[1].parse().unwrap_or(0);
                let status_pos = parts[2].trim();
                let status_on = parts[3].trim() == "on";

                let status_offset = if status_on && status_pos == "top" {
                    1
                } else {
                    0
                };
                let phys_row = 1 + status_offset + pane_top;
                let phys_col = 1 + pane_left;

                let cell_size = if parts.len() >= 6 {
                    let cw: u16 = parts[4].parse().unwrap_or(0);
                    let ch: u16 = parts[5].parse().unwrap_or(0);
                    if cw > 0 && ch > 0 {
                        Some((cw, ch))
                    } else {
                        None
                    }
                } else {
                    None
                };

                return ((phys_row, phys_col), cell_size);
            }
        }

        ((1, 1), None)
    }

    /// Convenience helper returning the cached physical origin coordinates.
    pub fn query_origin() -> (u16, u16) {
        Self::query_info().0
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

    fn is_multiplexer(&self) -> bool {
        self.is_active()
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

    fn physical_origin(&self) -> (u16, u16) {
        self.origin
    }

    fn cell_size(&self) -> Option<(u16, u16)> {
        self.cell_size
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
