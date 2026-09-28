//! Pure geometry on kurbo types, with no dependency on evaluation: segment
//! realization (quad elevation, cube passthrough, arc radius fit and
//! realization), the curvature check, stroking with join patches, filled
//! contours, Bézier clipping, contour roles, and winding (spec §6, §7,
//! §8).
//!
//! Implemented in M3: segment realization is complete and exact — there
//! is no Hobby's-algorithm-style approximation left to defer, since the
//! spec's `quad`/`cube`/`arc` segments fix their own geometry with no
//! free directions to solve for (see [`skeleton`]) — plus skeleton
//! bounding boxes and line-involving path intersection. M4 adds the
//! curvature check, stroking, join patches, filled contours, full
//! curve–curve Bézier clipping, contour roles, and winding.

pub mod skeleton;
