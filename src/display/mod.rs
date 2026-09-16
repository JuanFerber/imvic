//! Display subsystem: terminal guards, graphics backends, and transport tunnels.

pub mod backend;
pub mod hud;
pub mod terminal_guard;
pub mod transport;

pub use backend::{GraphicsBackend, KittyBackend, select_backend};
pub use hud::HudState;
pub use terminal_guard::TerminalGuard;
