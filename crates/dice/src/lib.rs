//! Dice Merge rules, as defined by the Dice Merge abstract specification.
//! Pure Rust: no rendering or platform dependencies (see CLAUDE.md).
//!
//! The entities are [`Die`], [`Board`], [`Cluster`], the slots and the score,
//! all held by [`Game`]. The player's capabilities are [`Game::rotate`] and
//! [`Game::place`]; the system's, Merge and Spawn, run inside `place`, which
//! returns the turn as a timeline of [`Step`]s for presentation to animate.
//! Every policy constant lives in [`Rules`].

mod board;
mod cluster;
mod game;
mod rng;

pub use board::{Board, Die, Dir, Move, Pos};
pub use cluster::{Cluster, value_weights};
pub use game::{Game, Merge, PlaceError, Step, Turn};

/// Which die of a neighborhood the merged die lands on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Survivor {
    /// The spec's rule.
    Youngest,
    /// The spec's open experiment.
    Oldest,
}

/// Every policy constant. `Default` is the shipped set.
#[derive(Clone, Debug, PartialEq)]
pub struct Rules {
    /// Board: N, the side of the N × N grid (4, 5 or 6).
    pub size: usize,
    /// Mass law: b in mass(v) = b^(v−1).
    pub base: f64,
    /// Threshold: m, the fewest dice a neighborhood merges with.
    pub threshold: usize,
    /// Yield: score gained per unit of released mass.
    pub score_per_mass: f64,
    /// Gravity: off, or the direction dice fall.
    pub gravity: Option<Dir>,
    /// Spawn distribution: weights of cluster sizes 1, 2 and 3.
    pub size_weights: [f64; 3],
    /// Spawn distribution: T in P(v) ∝ exp(−mass(v) / T).
    pub temperature: f64,
    /// Spawn distribution: the highest value spawned.
    pub max_value: u32,
    /// Spawn distribution, variety: no value repeats within a cluster.
    pub no_repeat: bool,
    /// Spawn distribution, variety: a 3-cell cluster holds exactly one pair.
    /// Overrides `no_repeat`.
    pub one_pair: bool,
    /// Supply: how many clusters are on offer at once. Any may be placed, and
    /// a placed one's slot is restocked with a fresh spawn.
    pub slots: usize,
    /// Survivor: youngest (the rule) or oldest (the open experiment).
    pub survivor: Survivor,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            size: 5,
            base: 2.0,
            threshold: 3,
            score_per_mass: 10.0,
            gravity: None,
            size_weights: [40.0, 35.0, 25.0],
            temperature: 2.0,
            max_value: 8,
            no_repeat: true,
            one_pair: true,
            slots: 3,
            survivor: Survivor::Youngest,
        }
    }
}

impl Rules {
    /// Mass law: mass(v) = b^(v−1). Scoring, spawn rarity and animation weight
    /// all read from this one function.
    pub fn mass(&self, value: u32) -> f64 {
        self.base.powi(value as i32 - 1)
    }

    /// Yield: the mass a neighborhood of `n` dice of value `v` releases when
    /// it merges. Never negative.
    pub fn released(&self, n: usize, value: u32) -> f64 {
        (n as f64 * self.mass(value) - self.mass(value + 1)).max(0.0)
    }
}

#[cfg(test)]
mod tests;
