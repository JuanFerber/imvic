//! Imvic: High-performance, modular, GPU-accelerated terminal image and vector viewer.

use clap::Parser;
use imvic::cli::CliArgs;

fn main() {
    // Parse command-line arguments (automatically handles --help and --version)
    let args = CliArgs::parse();

    println!("Target file: {:?}", args.file);
    println!("Watch mode: {}", args.watch);
}
