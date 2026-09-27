//! Pure geometry on kurbo types, with no dependency on evaluation: segment
//! realization, Hobby splines, the curvature check, stroking with join
//! patches, filled contours, Bézier clipping, contour roles, and winding
//! (spec §6, §7, §8).
//!
//! Implemented in M3 (Bézier clipping) and M4 (everything else).
