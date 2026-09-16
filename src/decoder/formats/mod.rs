//! Built-in format decoder plugins.

pub mod raster;
pub mod svg;

pub use raster::RasterDecoder;
pub use svg::SvgDecoder;
