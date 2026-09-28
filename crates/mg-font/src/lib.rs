//! Slant, extrema, cu2qu, zone snap and quantization, glyf/cmap/metrics
//! assembly, kerning, and instances (spec §10–§12).
//!
//! [`outline`] holds the per-contour preparation stages and [`prepare`]
//! runs them over a whole instance (M5). [`assemble`] turns prepared
//! glyphs into tables and [`build`] drives one build per instance (M6).
//! Kerning (M7) is still to come.

pub mod assemble;
pub mod build;
pub mod outline;
pub mod prepare;

pub use build::{BuildOptions, BuiltFont, build_fonts};
pub use prepare::{PreparedComponent, PreparedGlyph, prepare_font};
