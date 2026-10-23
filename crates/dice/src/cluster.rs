//! Clusters: the 1–3-die polyominoes in the queue, and Spawn, which draws them.

use crate::Rules;
use crate::board::Pos;
use crate::rng::Rng;

/// A polyomino of dice, as offsets from its top-left corner (the smallest x and
/// the smallest y are both 0), each with a value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cluster {
    pub cells: Vec<(Pos, u32)>,
}

/// Every free polyomino of sizes 1 to 3, counted up to rotation: Spawn's
/// "uniform among that size's polyominoes" makes the I- and L-tromino equally
/// likely. Each spawn then turns its shape a random number of quarter-turns.
const SHAPES: [&[&[(i32, i32)]]; 3] =
    [&[&[(0, 0)]], &[&[(0, 0), (1, 0)]], &[&[(0, 0), (1, 0), (2, 0)], &[(0, 0), (0, 1), (1, 1)]]];

impl Cluster {
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Rotate: a clockwise quarter-turn on screen (y down).
    pub fn rotated(&self) -> Cluster {
        let mut c = Cluster { cells: self.cells.iter().map(|&(p, v)| (Pos::new(-p.y, p.x), v)).collect() };
        c.normalize();
        c
    }

    /// The cluster in all four rotations, starting with itself.
    pub fn rotations(&self) -> [Cluster; 4] {
        let r1 = self.rotated();
        let r2 = r1.rotated();
        let r3 = r2.rotated();
        [self.clone(), r1, r2, r3]
    }

    /// Board cells the dice would cover with cell `anchor` at `at`.
    pub fn cells_at(&self, anchor: usize, at: Pos) -> impl Iterator<Item = (Pos, u32)> + '_ {
        let origin = at - self.cells[anchor].0;
        self.cells.iter().map(move |&(p, v)| (origin + p, v))
    }

    fn normalize(&mut self) {
        let min_x = self.cells.iter().map(|c| c.0.x).min().unwrap_or(0);
        let min_y = self.cells.iter().map(|c| c.0.y).min().unwrap_or(0);
        for c in &mut self.cells {
            c.0 = c.0 - Pos::new(min_x, min_y);
        }
    }
}

/// Spawn: draw a size, a shape of that size, and a value for each cell.
pub(crate) fn spawn(rules: &Rules, rng: &mut Rng) -> Cluster {
    let size = rng.weighted(&rules.size_weights) + 1;
    let shapes = SHAPES[size - 1];
    let shape = shapes[rng.below(shapes.len())];
    let values = roll_values(rules, rng, size);
    let mut cluster = Cluster { cells: shape.iter().zip(values).map(|(&(x, y), v)| (Pos::new(x, y), v)).collect() };
    for _ in 0..rng.below(4) {
        cluster = cluster.rotated();
    }
    cluster
}

/// The spawn distribution, P(v) ∝ exp(−mass(v) / T) for v in 1..=max_value,
/// as weights indexed by v − 1.
pub fn value_weights(rules: &Rules) -> Vec<f64> {
    (1..=rules.max_value).map(|v| (-rules.mass(v) / rules.temperature).exp()).collect()
}

fn roll_values(rules: &Rules, rng: &mut Rng, size: usize) -> Vec<u32> {
    let weights = value_weights(rules);
    let roll_except = |rng: &mut Rng, except: &[u32]| {
        let w: Vec<f64> =
            weights.iter().enumerate().map(|(i, &w)| if except.contains(&(i as u32 + 1)) { 0.0 } else { w }).collect();
        rng.weighted(&w) as u32 + 1
    };

    if rules.one_pair && size == 3 {
        // Exactly one pair: a paired value, a different single one, and the
        // single in a random cell.
        let pair = roll_except(rng, &[]);
        let single = roll_except(rng, &[pair]);
        let mut values = vec![pair; 3];
        values[rng.below(3)] = single;
        return values;
    }

    let mut values = Vec::with_capacity(size);
    for _ in 0..size {
        let v = if rules.no_repeat { roll_except(rng, &values) } else { roll_except(rng, &[]) };
        values.push(v);
    }
    values
}
