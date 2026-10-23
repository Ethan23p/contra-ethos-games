//! The game state and a turn: Rotate* → Place → Merge* → Spawn.

use crate::board::{Board, Die, Move, Pos};
use crate::cluster::{self, Cluster};
use crate::rng::Rng;
use crate::{Rules, Survivor};

/// One neighborhood merging.
#[derive(Clone, Debug, PartialEq)]
pub struct Merge {
    /// Every cell the neighborhood covered, the survivor's included.
    pub cells: Vec<Pos>,
    /// Where the merged die lands.
    pub survivor: Pos,
    /// The merged die, of value v + 1.
    pub die: Die,
    pub released: f64,
    pub score: f64,
}

/// One beat of a turn, in order, for presentation to animate.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    /// The head cluster's dice written into the board.
    Place(Vec<(Pos, Die)>),
    /// Dice sliding under gravity.
    Fall(Vec<Move>),
    /// One settling generation: every qualifying neighborhood, merging at once.
    Merge(Vec<Merge>),
    /// A newly spawned cluster appended to the queue.
    Spawn(Cluster),
    /// A queued cluster that no longer fit anywhere, replaced by one that does.
    Replace { index: usize, cluster: Cluster },
    /// The board is full after settling: the game is over.
    End,
}

/// What a Place did.
#[derive(Clone, Debug, PartialEq)]
pub struct Turn {
    pub steps: Vec<Step>,
    pub score: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaceError {
    /// The game is over.
    Over,
    /// A target cell is off the board or occupied.
    Illegal,
}

#[derive(Clone, Debug)]
pub struct Game {
    pub rules: Rules,
    board: Board,
    queue: Vec<Cluster>,
    score: f64,
    over: bool,
    rng: Rng,
    /// Ages: each distance ring of each Place takes the next tick.
    tick: u64,
}

/// How many fresh spawns to try before replacing a dead cluster with a single die.
const RESPAWN_TRIES: usize = 64;

impl Game {
    pub fn new(rules: Rules, seed: u64) -> Self {
        let board = Board::new(rules.size);
        Self::with_board(rules, seed, board)
    }

    /// A game starting from `board`, for tests and set-up positions. Its dice
    /// are all older than anything placed from here on.
    pub fn with_board(rules: Rules, seed: u64, board: Board) -> Self {
        let tick = board.dice().map(|(_, d)| d.born).max().unwrap_or(0);
        let mut game = Self { rules, board, queue: Vec::new(), score: 0.0, over: false, rng: Rng::new(seed), tick };
        while game.queue.len() < game.rules.queue_len.max(1) {
            let c = cluster::spawn(&game.rules, &mut game.rng);
            game.queue.push(c);
        }
        game.replace_dead();
        game
    }

    pub fn board(&self) -> &Board {
        &self.board
    }

    pub fn queue(&self) -> &[Cluster] {
        &self.queue
    }

    /// The head cluster, the one in play.
    pub fn head(&self) -> &Cluster {
        &self.queue[0]
    }

    pub fn score(&self) -> f64 {
        self.score
    }

    pub fn is_over(&self) -> bool {
        self.over
    }

    /// Replaces the queue, for tests and set-up positions.
    pub fn set_queue(&mut self, queue: Vec<Cluster>) {
        self.queue = queue;
    }

    /// Rotate: a clockwise quarter-turn of the head cluster. Always free.
    pub fn rotate(&mut self) {
        self.queue[0] = self.queue[0].rotated();
    }

    /// Legality: whether the head may go with its cell `anchor` at `at`.
    pub fn can_place(&self, anchor: usize, at: Pos) -> bool {
        fits_at(&self.board, self.head(), anchor, at)
    }

    /// Place, then Merge to a fixed point, then Spawn: the rest of a turn.
    pub fn place(&mut self, anchor: usize, at: Pos) -> Result<Turn, PlaceError> {
        if self.over {
            return Err(PlaceError::Over);
        }
        if !self.can_place(anchor, at) {
            return Err(PlaceError::Illegal);
        }
        let start_score = self.score;
        let head = self.queue.remove(0);
        let mut steps = vec![Step::Place(self.write(&head, anchor, at))];
        self.settle(&mut steps);

        let c = cluster::spawn(&self.rules, &mut self.rng);
        self.queue.push(c.clone());
        steps.push(Step::Spawn(c));
        if self.board.is_full() {
            self.over = true;
            steps.push(Step::End);
        } else {
            for (index, cluster) in self.replace_dead() {
                steps.push(Step::Replace { index, cluster });
            }
        }
        Ok(Turn { steps, score: self.score - start_score })
    }

    /// Writes the dice furthest from the anchor first and the anchor last, one
    /// tick per distance, so the anchor is the youngest and dice equally far
    /// from it share an age.
    fn write(&mut self, cluster: &Cluster, anchor: usize, at: Pos) -> Vec<(Pos, Die)> {
        let mut cells: Vec<(Pos, u32)> = cluster.cells_at(anchor, at).collect();
        cells.sort_by_key(|&(p, _)| std::cmp::Reverse(p.manhattan(at)));
        let mut placed = Vec::with_capacity(cells.len());
        let mut last = None;
        for (p, value) in cells {
            if last != Some(p.manhattan(at)) {
                self.tick += 1;
                last = Some(p.manhattan(at));
            }
            let die = Die { value, born: self.tick };
            self.board.set(p, Some(die));
            placed.push((p, die));
        }
        placed
    }

    /// Settling: gravity, then synchronous merge generations, each reading a
    /// frozen board, until nothing qualifies.
    fn settle(&mut self, steps: &mut Vec<Step>) {
        loop {
            if let Some(dir) = self.rules.gravity {
                let moves = self.board.fall(dir);
                if !moves.is_empty() {
                    steps.push(Step::Fall(moves));
                }
            }
            let generation = self.merge_generation();
            if generation.is_empty() {
                break;
            }
            steps.push(Step::Merge(generation));
        }
    }

    fn merge_generation(&mut self) -> Vec<Merge> {
        let threshold = self.rules.threshold.max(1);
        let hoods: Vec<Vec<Pos>> = self.board.neighborhoods().into_iter().filter(|h| h.len() >= threshold).collect();
        // Neighborhoods are disjoint, so choosing survivors one by one and then
        // writing every result still reads the frozen board.
        let mut merges = Vec::with_capacity(hoods.len());
        for cells in hoods {
            let survivor = self.survivor(&cells);
            let old = self.board.get(survivor).unwrap();
            let released = self.rules.released(cells.len(), old.value);
            let score = released * self.rules.score_per_mass;
            merges.push(Merge { cells, survivor, die: Die { value: old.value + 1, ..old }, released, score });
        }
        for m in &merges {
            for &p in &m.cells {
                self.board.set(p, None);
            }
            self.board.set(m.survivor, Some(m.die));
            self.score += m.score;
        }
        merges
    }

    /// Survivor: the youngest die (or the oldest, under the experiment). Equal
    /// ages are a coin flip, never scan order.
    fn survivor(&mut self, cells: &[Pos]) -> Pos {
        let born = |p: &Pos| self.board.get(*p).unwrap().born;
        let best = match self.rules.survivor {
            Survivor::Youngest => cells.iter().map(born).max(),
            Survivor::Oldest => cells.iter().map(born).min(),
        }
        .unwrap();
        let tied: Vec<Pos> = cells.iter().copied().filter(|p| born(p) == best).collect();
        tied[self.rng.below(tied.len())]
    }

    /// No dead clusters: replaces each queued cluster that fits nowhere in any
    /// rotation with a fresh one that does. Returns what it replaced.
    fn replace_dead(&mut self) -> Vec<(usize, Cluster)> {
        if self.board.is_full() {
            return Vec::new();
        }
        let mut replaced = Vec::new();
        for index in 0..self.queue.len() {
            if fits_anywhere(&self.board, &self.queue[index]) {
                continue;
            }
            let fresh = (0..RESPAWN_TRIES)
                .map(|_| cluster::spawn(&self.rules, &mut self.rng))
                .find(|c| fits_anywhere(&self.board, c))
                .unwrap_or_else(|| {
                    // A single die fits any empty cell.
                    let rules = Rules { size_weights: [1.0, 0.0, 0.0], ..self.rules.clone() };
                    cluster::spawn(&rules, &mut self.rng)
                });
            self.queue[index] = fresh.clone();
            replaced.push((index, fresh));
        }
        replaced
    }
}

fn fits_at(board: &Board, cluster: &Cluster, anchor: usize, at: Pos) -> bool {
    anchor < cluster.len() && cluster.cells_at(anchor, at).all(|(p, _)| board.is_empty_at(p))
}

/// Whether any rotation of `cluster` has a legal placement.
pub(crate) fn fits_anywhere(board: &Board, cluster: &Cluster) -> bool {
    cluster.rotations().iter().any(|c| board.positions().any(|p| fits_at(board, c, 0, p)))
}
