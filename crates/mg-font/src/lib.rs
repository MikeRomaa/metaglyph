//! Slant, extrema, cu2qu, zone snap and quantization, glyf/cmap/metrics
//! assembly, kerning, and instances (spec §10–§12).
//!
//! M5 (outline preparation) is [`outline`]'s per-contour stages and
//! [`prepare`]'s whole-instance driver. M6 (TTF assembly) and M7 (kerning
//! via `write-fonts`' GPOS pair-positioning builders) build on them.

pub mod outline;
pub mod prepare;

pub use prepare::{PreparedComponent, PreparedGlyph, prepare_font};
