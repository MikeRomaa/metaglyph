//! Spec §14's named constants, in one place rather than as magic numbers
//! at their use sites. Every tolerance scales with the em size
//! (`k · font.em / 1000`), so they come as one [`Tolerances`] value built
//! from the font's `em`; the two dimensionless limits are plain consts.

/// `MITER_LIMIT` (spec §6.4, §14): a `"miter"` join whose apex lies
/// further than this many half-widths from the vertex falls back to a
/// bevel.
pub const MITER_LIMIT: f64 = 4.0;

/// `COMPONENT_DEPTH` (spec §10.1, §14): the deepest a component reference
/// chain may nest.
pub const COMPONENT_DEPTH: usize = 5;

/// The em-scaled tolerances of spec §14, all in design units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tolerances {
    /// `OFFSET_TOLERANCE`: stroke offset accuracy (spec §7.1).
    pub offset: f64,
    /// `CU2QU_TOLERANCE`: cubic-to-quadratic conversion accuracy (spec
    /// §10.3).
    pub cu2qu: f64,
    /// `ZONE_SNAP_TOLERANCE`: how far an on-curve point may sit from a
    /// metric and still snap to it (spec §10.4).
    pub zone_snap: f64,
    /// `ARC_TOLERANCE`: radii-mode `arc` diameter-chord slack (spec §6.3).
    pub arc: f64,
}

impl Tolerances {
    pub fn for_em(em: f64) -> Self {
        let scale = em / 1000.0;
        Self {
            offset: 0.05 * scale,
            cu2qu: 0.5 * scale,
            zone_snap: scale,
            arc: 0.01 * scale,
        }
    }
}
