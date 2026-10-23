//! Plays a turn's step timeline on a board of its own, one step after another.
//! The game's board is already final; this one catches up with it on screen.
//! Every step is played the same way whatever caused it.

use std::collections::{HashMap, VecDeque};

use dice::{Board, Merge, Pos, Step};
use engine::glam::Vec2;

use crate::config::Config;
use crate::look::cell;

pub enum Fx {
    /// A ring leaving the survivor as a die lands on it.
    Ring { at: Pos, value: u32, amp: f32 },
    /// The survivor's short beat as a die lands on it.
    Beat { at: Pos, amp: f32 },
    /// The merged die's pop.
    Pop { at: Pos, impact: f32 },
    /// Score rising off a merge.
    Float { at: Pos, text: String },
    /// Cells shaking after a rejected drop.
    Shake { cells: Vec<Pos> },
}

pub struct Effect {
    pub start: f32,
    pub fx: Fx,
}

/// A die drawn away from its cell, in fractional cell coordinates.
pub struct Mover {
    pub value: u32,
    pub cell: Vec2,
}

/// One die's trip to its merge's survivor.
struct Flight {
    value: u32,
    path: Vec<Pos>,
    depart: f32,
    hop: f32,
}

impl Flight {
    fn landing(&self) -> f32 {
        self.depart + self.hop * (self.path.len() - 1) as f32
    }
}

pub struct Player {
    /// The board as shown before the current step.
    pub base: Board,
    steps: VecDeque<Step>,
    start: f32,
    /// Where each placed die was let go, in cell coordinates.
    dropped: HashMap<Pos, Vec2>,
    pub effects: Vec<Effect>,
}

impl Player {
    pub fn new(board: Board) -> Self {
        Self { base: board, steps: VecDeque::new(), start: 0.0, dropped: HashMap::new(), effects: Vec::new() }
    }

    pub fn busy(&self) -> bool {
        !self.steps.is_empty()
    }

    /// Starts a turn. A turn still playing jumps to its end first, so rapid
    /// placing never waits on animation.
    pub fn play(&mut self, now: f32, steps: Vec<Step>, dropped: HashMap<Pos, Vec2>, c: &Config) {
        self.skip();
        self.steps =
            steps.into_iter().filter(|s| matches!(s, Step::Place(_) | Step::Fall(_) | Step::Merge(_))).collect();
        self.dropped = dropped;
        self.start = now;
        self.schedule(c);
    }

    pub fn skip(&mut self) {
        while let Some(step) = self.steps.pop_front() {
            apply(&mut self.base, &step);
        }
    }

    pub fn shake(&mut self, now: f32, cells: Vec<Pos>) {
        self.effects.push(Effect { start: now, fx: Fx::Shake { cells } });
    }

    pub fn update(&mut self, now: f32, c: &Config) {
        while let Some(step) = self.steps.front() {
            let d = self.duration(step, c);
            if now - self.start < d {
                break;
            }
            let step = self.steps.pop_front().unwrap();
            apply(&mut self.base, &step);
            self.start += d;
            self.schedule(c);
        }
        self.effects.retain(|e| now - e.start < 3.0);
    }

    fn duration(&self, step: &Step, c: &Config) -> f32 {
        match step {
            Step::Place(_) => c.place_ms / 1000.0,
            Step::Fall(moves) => {
                let value = moves.iter().map(|m| m.die.value).max().unwrap_or(1);
                let cells = moves.iter().map(|m| m.from.manhattan(m.to)).max().unwrap_or(0);
                c.hop(value) * cells as f32
            }
            Step::Merge(merges) => {
                let last = merges.iter().flat_map(|m| self.flights(m, c)).map(|f| f.landing()).fold(0.0, f32::max);
                last + c.settle_beat_ms / 1000.0
            }
            _ => 0.0,
        }
    }

    /// Effects for the step that just became current.
    fn schedule(&mut self, c: &Config) {
        let Some(Step::Merge(merges)) = self.steps.front() else { return };
        let mut fx = Vec::new();
        for m in merges {
            let flights = self.flights(m, c);
            let n = flights.len();
            for (k, f) in flights.iter().enumerate() {
                let amp =
                    if n > 1 { c.build / 100.0 + (1.0 - c.build / 100.0) * k as f32 / (n - 1) as f32 } else { 1.0 };
                let at = self.start + f.landing();
                fx.push(Effect { start: at, fx: Fx::Ring { at: m.survivor, value: f.value, amp } });
                fx.push(Effect { start: at, fx: Fx::Beat { at: m.survivor, amp } });
            }
            let end = self.start + flights.iter().map(Flight::landing).fold(0.0, f32::max);
            let impact = m.cells.len() as f32 / 3.0 * c.weight(m.die.value - 1);
            fx.push(Effect { start: end, fx: Fx::Pop { at: m.survivor, impact } });
            if m.score > 0.0 {
                fx.push(Effect { start: end, fx: Fx::Float { at: m.survivor, text: format!("+{}", m.score.round()) } });
            }
        }
        self.effects.extend(fx);
    }

    /// The dice of a merge other than the survivor, nearest first, each leaving
    /// one beat after the last and travelling cell by cell through the neighborhood.
    fn flights(&self, m: &Merge, c: &Config) -> Vec<Flight> {
        // Breadth-first from the survivor gives every cell its way back.
        let mut parent: HashMap<Pos, Pos> = HashMap::new();
        let mut queue = VecDeque::from([m.survivor]);
        parent.insert(m.survivor, m.survivor);
        while let Some(p) = queue.pop_front() {
            for q in &m.cells {
                if q.manhattan(p) == 1 && !parent.contains_key(q) {
                    parent.insert(*q, p);
                    queue.push_back(*q);
                }
            }
        }
        let value = m.die.value - 1;
        let mut paths: Vec<Vec<Pos>> = m
            .cells
            .iter()
            .filter(|&&p| p != m.survivor)
            .map(|&p| {
                let mut path = vec![p];
                while *path.last().unwrap() != m.survivor {
                    path.push(parent[path.last().unwrap()]);
                }
                path
            })
            .collect();
        paths.sort_by_key(|p| (p.len(), p[0]));

        let hop = c.hop(value);
        let mut depart = 0.0;
        let mut beat = c.beat_ms / 1000.0;
        paths
            .into_iter()
            .map(|path| {
                let f = Flight { value, path, depart, hop };
                depart += beat;
                beat *= 1.0 - c.speed_up / 100.0;
                f
            })
            .collect()
    }

    /// Cells of `base` not to draw this frame: their dice are in flight.
    pub fn hidden(&self) -> Vec<Pos> {
        match self.steps.front() {
            Some(Step::Fall(moves)) => moves.iter().map(|m| m.from).collect(),
            Some(Step::Merge(merges)) => {
                merges.iter().flat_map(|m| m.cells.iter().copied().filter(move |&p| p != m.survivor)).collect()
            }
            _ => Vec::new(),
        }
    }

    /// Survivors already showing their merged value: the last die has landed.
    pub fn merged(&self, now: f32, c: &Config) -> Vec<(Pos, u32)> {
        let Some(Step::Merge(merges)) = self.steps.front() else { return Vec::new() };
        let t = now - self.start;
        merges
            .iter()
            .filter(|m| self.flights(m, c).iter().all(|f| f.landing() <= t))
            .map(|m| (m.survivor, m.die.value))
            .collect()
    }

    pub fn movers(&self, now: f32, c: &Config) -> Vec<Mover> {
        let t = now - self.start;
        match self.steps.front() {
            Some(Step::Place(dice)) => {
                let k = crate::look::ease_out(t / (c.place_ms / 1000.0).max(1e-3));
                dice.iter()
                    .map(|&(p, d)| {
                        let from = self.dropped.get(&p).copied().unwrap_or(cell(p));
                        Mover { value: d.value, cell: from.lerp(cell(p), k) }
                    })
                    .collect()
            }
            Some(Step::Fall(moves)) => {
                let value = moves.iter().map(|m| m.die.value).max().unwrap_or(1);
                let hop = c.hop(value).max(1e-3);
                moves
                    .iter()
                    .map(|m| {
                        let cells = m.from.manhattan(m.to) as f32;
                        let s = (t / hop).min(cells);
                        // Constant speed, easing only into the last cell, so dice
                        // sharing a line never overlap.
                        let s =
                            if s > cells - 1.0 { cells - 1.0 + crate::look::ease_out(s - (cells - 1.0)) } else { s };
                        let dir = (cell(m.to) - cell(m.from)) / cells.max(1.0);
                        Mover { value: m.die.value, cell: cell(m.from) + dir * s }
                    })
                    .collect()
            }
            Some(Step::Merge(merges)) => merges
                .iter()
                .flat_map(|m| self.flights(m, c))
                .filter(|f| t < f.landing())
                .map(|f| {
                    let s = ((t - f.depart) / f.hop.max(1e-3)).max(0.0);
                    let i = (s as usize).min(f.path.len() - 2);
                    let k = crate::look::smooth(s - i as f32);
                    Mover { value: f.value, cell: cell(f.path[i]).lerp(cell(f.path[i + 1]), k) }
                })
                .collect(),
            _ => Vec::new(),
        }
    }
}

fn apply(board: &mut Board, step: &Step) {
    match step {
        Step::Place(dice) => {
            for &(p, d) in dice {
                board.set(p, Some(d));
            }
        }
        Step::Fall(moves) => {
            for m in moves {
                board.set(m.from, None);
            }
            for m in moves {
                board.set(m.to, Some(m.die));
            }
        }
        Step::Merge(merges) => {
            for m in merges {
                for &p in &m.cells {
                    board.set(p, None);
                }
                board.set(m.survivor, Some(m.die));
            }
        }
        _ => {}
    }
}
