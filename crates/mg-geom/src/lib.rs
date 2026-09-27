//! Pure geometry on kurbo types, with no dependency on evaluation: segment
//! realization, Hobby splines, the curvature check, stroking with join
//! patches, filled contours, Bézier clipping, contour roles, and winding
//! (spec §6, §7, §8).
//!
//! Implemented in M3: segment realization for every *local* direction
//! rule (see [`skeleton`] for what that excludes), skeleton bounding
//! boxes, and line-involving path intersection. M4 adds Hobby's
//! algorithm, the curvature check, stroking, join patches, filled
//! contours, full curve–curve Bézier clipping, contour roles, and
//! winding.

pub mod skeleton;
