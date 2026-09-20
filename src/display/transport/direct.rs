//! Direct terminal transport without multiplexer wrapping.

use super::TransportAdapter;

/// Pass-through transport for standard terminals without multiplexers.
#[derive(Clone, Copy)]
pub struct DirectTransport;

impl DirectTransport {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DirectTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl TransportAdapter for DirectTransport {
    fn name(&self) -> &'static str {
        "Direct Terminal"
    }

    fn is_active(&self) -> bool {
        true
    }

    fn is_passthrough_enabled(&self) -> bool {
        true
    }

    fn passthrough_enable_hint(&self) -> Option<&'static str> {
        None
    }

    fn wrap_escape(&self, raw: &[u8]) -> Vec<u8> {
        raw.to_vec()
    }

    fn physical_origin(&self) -> (u16, u16) {
        (1, 1)
    }

    fn clone_box(&self) -> Box<dyn TransportAdapter> {
        Box::new(*self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_direct_transport_unmodified() {
        let transport = DirectTransport::new();
        let payload = b"\x1b_Ga=d,d=a\x1b\\";
        assert_eq!(transport.wrap_escape(payload), payload);
        assert!(transport.is_passthrough_enabled());
    }
}
