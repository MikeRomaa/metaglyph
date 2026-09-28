//! Pure geometry on kurbo types, with no dependency on evaluation: segment
//! realization (quad elevation, cube passthrough, arc radius fit and
//! realization, [`skeleton`]), curve–curve intersection ([`intersect`]),
//! the curvature limit check ([`curvature`]), stroking with join splicing
//! ([`stroke`]), filled contours and their self-intersection check
//! ([`fill`]), and contour roles and winding direction ([`winding`]) —
//! spec §6, §7, §8.
//!
//! Segment realization (M3) is complete and exact — there is no Hobby's-
//! algorithm-style approximation left to defer, since the spec's
//! `quad`/`cube`/`arc` segments fix their own geometry with no free
//! directions to solve for. The rest of this crate (M4) is what turns
//! that skeleton into a renderable outline: [`stroke`] and [`fill`] are
//! the two entry points a rendering path's `.bbox` and `mg svg` actually
//! call, and lean on the other modules underneath.

pub mod curvature;
pub mod fill;
pub mod intersect;
pub mod skeleton;
pub mod stroke;
pub mod winding;
