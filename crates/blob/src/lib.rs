//! Blob Merge rules and physics, as defined by the Blob Merge abstract specification.
//! Pure Rust: no rendering or platform dependencies (see CLAUDE.md).
//!
//! [`Field`] holds the blobs, their bonds and the merges in progress, and
//! [`Field::step`] runs the spec's continuous Move → Merge loop at a fixed dt
//! with substeps; [`Field::launch`] and [`Field::seed`] add blobs. [`Game`]
//! plays the rules on top: the queue and its pool, Purge, scoring, nova and
//! the ending. Physics constants live in [`Physics`], gameplay ones in [`Rules`].
//!
//! Units: world units are the old build's pixels on a 600-unit field, and
//! times are seconds, so the old presets convert directly.

mod field;
mod game;
mod rng;

#[cfg(test)]
mod tests;

pub use field::{Blob, Event, Field, Merge, Pending, merged_value};
pub use game::{Game, Rules};
pub use rng::Rng;

/// Colour classes. Only identical blobs (same group, same value) interact.
pub const GROUPS: u8 = 3;

/// Every physics and field constant. `Default` is the shipped set.
#[derive(Clone, Debug, PartialEq)]
pub struct Physics {
    /// Merge size m: 3 ("bonded") or 2 ("unbonded").
    pub m: usize,
    /// Every blob's radius.
    pub radius: f32,

    // Lennard-Jones: short-range repulsion, mid-range cohesion.
    /// Pair size scale; pairs rest at about 1.12 σ.
    pub sigma: f32,
    /// Cohesion strength (units²/s²).
    pub epsilon: f32,
    /// Range of the force, × σ.
    pub cutoff: f32,
    /// The most one pair can push or pull (units/s²); keeps overlaps from exploding.
    pub max_force: f32,

    /// Viscous damping rate (1/s): speed falls by e every 1/damping seconds.
    pub damping: f32,
    /// The well's pull at full strength (units/s²).
    pub well: f32,
    /// Distance from the centre where the well's pull saturates.
    pub well_radius: f32,
    /// How fast the centre follows the blobs' centre of mass (1/s).
    pub centre_follow: f32,

    /// Bond spring stiffness (1/s²), pulling once a bond stretches past rest.
    pub spring: f32,
    /// Bond rest length, × the blob diameter.
    pub bond_rest: f32,
    /// Blobs touch within this many diameters, centre to centre.
    pub touch: f32,

    /// How long a launched blob waits, from when it first touches anything,
    /// before a merge it's in converges, so its jolt carries through (s).
    pub merge_delay: f32,
    /// Whether that delay also holds every neighborhood that forms during it,
    /// so the jolt can't trigger merges elsewhere in the clump first.
    pub delay_clump: bool,
    /// How long a blob made by a merge waits before merging again (s), on
    /// top of the convergence it came from.
    pub chain_delay: f32,
    /// While a merge waits, members stay in while within this many touch
    /// distances of another member, so a blob that ricochets off still merges.
    pub merge_hold: f32,
    /// The gloopy convergence: roughly how long a merge takes (s).
    pub merge_secs: f32,
    /// Speed the merged blob gives the blobs around it (units/s), fading to
    /// nothing at three radii.
    pub merge_push: f32,

    /// Launcher ring radius, from the centre.
    pub ring: f32,
    /// The rim, inside the ring; Ending watches it.
    pub rim: f32,
    /// How far an unobstructed launch would coast, as a fraction of the ring
    /// radius: 1 stops at the centre. Sets the fixed launch speed.
    pub launch_reach: f32,
    /// Speed cap (units/s), against tunnelling.
    pub max_speed: f32,

    /// Substeps per step: the 1/r¹² core is stiff.
    pub substeps: u32,
}

impl Default for Physics {
    fn default() -> Self {
        // The old build's "Standard Fluid", converted from per-16 ms units.
        Self {
            m: 3,
            radius: 20.0,
            sigma: 35.0,
            epsilon: 212.5,
            cutoff: 2.5,
            max_force: 125_000.0,
            damping: 12.8,
            well: 600.0,
            well_radius: 450.0,
            centre_follow: 5.0,
            spring: 9.4,
            bond_rest: 1.05,
            touch: 1.1,
            merge_delay: 0.0,
            delay_clump: false,
            chain_delay: 0.0,
            merge_hold: 1.5,
            merge_secs: 0.28,
            merge_push: 150.0,
            ring: 280.0,
            rim: 235.0,
            launch_reach: 1.1,
            max_speed: 15_000.0,
            substeps: 4,
        }
    }
}

impl Physics {
    /// The fixed launch speed: with linear damping a blob coasts speed ÷ damping.
    pub fn launch_speed(&self) -> f32 {
        self.launch_reach * self.ring * self.damping
    }
}
