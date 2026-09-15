//! Command-line argument parsing and configuration specification for `imvic`.
//!
//! Defines the structure and flags accepted by the application at startup.

use clap::Parser;
use std::path::PathBuf;

/// High-performance, modular, GPU-accelerated terminal image and vector viewer
#[derive(Parser, Debug, Clone)]
#[command(
    name = "imvic",
    version,
    about = "High-performance, modular, GPU-accelerated terminal image and vector viewer."
)]
pub struct CliArgs {
    /// Path to the image or vector file to display
    #[arg(value_name = "FILE")]
    pub file: PathBuf,

    /// Watch file for changes and reload view automatically (Live Reload)
    #[arg(short = 'w', long = "watch", default_value_t = true)]
    pub watch: bool,

    /// Initial zoom or scale factor override
    #[arg(short = 's', long = "scale")]
    pub scale: Option<f32>,

    /// Print verbose debugging traces and terminal protocol events
    #[arg(short = 'v', long = "verbose")]
    pub verbose: bool,
}
