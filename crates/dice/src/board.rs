//! The board: an N × N grid of cells, each empty or holding one die.

use std::fmt;

/// A board cell. x grows to the right, y grows downward; (0, 0) is top left.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Pos {
    pub x: i32,
    pub y: i32,
}

impl Pos {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub fn manhattan(self, other: Pos) -> i32 {
        (self.x - other.x).abs() + (self.y - other.y).abs()
    }
}

impl std::ops::Add for Pos {
    type Output = Pos;
    fn add(self, o: Pos) -> Pos {
        Pos::new(self.x + o.x, self.y + o.y)
    }
}

impl std::ops::Sub for Pos {
    type Output = Pos;
    fn sub(self, o: Pos) -> Pos {
        Pos::new(self.x - o.x, self.y - o.y)
    }
}

const ORTHOGONAL: [Pos; 4] = [Pos::new(1, 0), Pos::new(-1, 0), Pos::new(0, 1), Pos::new(0, -1)];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Die {
    pub value: u32,
    /// The die's age, as the tick of the Place that put it on the board: a
    /// larger `born` is a younger die (see the Survivor policy).
    pub born: u64,
}

/// A gravity direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Down,
    Up,
    Left,
    Right,
}

impl Dir {
    pub const ALL: [Dir; 4] = [Dir::Down, Dir::Up, Dir::Left, Dir::Right];

    pub fn step(self) -> Pos {
        match self {
            Dir::Down => Pos::new(0, 1),
            Dir::Up => Pos::new(0, -1),
            Dir::Left => Pos::new(-1, 0),
            Dir::Right => Pos::new(1, 0),
        }
    }
}

/// One die sliding from one cell to another, under gravity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Move {
    pub from: Pos,
    pub to: Pos,
    pub die: Die,
}

#[derive(Clone, PartialEq, Eq)]
pub struct Board {
    n: usize,
    cells: Vec<Option<Die>>,
}

impl Board {
    pub fn new(n: usize) -> Self {
        Self { n, cells: vec![None; n * n] }
    }

    /// A board from rows of values, `.` for empty, separated by whitespace.
    /// Every die gets age 0. Handy in tests and for setting up positions.
    pub fn parse(rows: &[&str]) -> Self {
        let n = rows.len();
        let mut board = Board::new(n);
        for (y, row) in rows.iter().enumerate() {
            let cells: Vec<&str> = row.split_whitespace().collect();
            assert_eq!(cells.len(), n, "row {y} of a {n}×{n} board");
            for (x, c) in cells.into_iter().enumerate() {
                if c != "." {
                    let value = c.parse().expect("a die value or '.'");
                    board.set(Pos::new(x as i32, y as i32), Some(Die { value, born: 0 }));
                }
            }
        }
        board
    }

    pub fn size(&self) -> usize {
        self.n
    }

    pub fn contains(&self, p: Pos) -> bool {
        p.x >= 0 && p.y >= 0 && (p.x as usize) < self.n && (p.y as usize) < self.n
    }

    pub fn get(&self, p: Pos) -> Option<Die> {
        if self.contains(p) { self.cells[p.y as usize * self.n + p.x as usize] } else { None }
    }

    pub fn set(&mut self, p: Pos, die: Option<Die>) {
        assert!(self.contains(p), "{p:?} is off the board");
        self.cells[p.y as usize * self.n + p.x as usize] = die;
    }

    pub fn is_empty_at(&self, p: Pos) -> bool {
        self.contains(p) && self.get(p).is_none()
    }

    pub fn is_full(&self) -> bool {
        self.cells.iter().all(Option::is_some)
    }

    /// Every cell, row by row.
    pub fn positions(&self) -> impl Iterator<Item = Pos> + use<> {
        let n = self.n as i32;
        (0..n).flat_map(move |y| (0..n).map(move |x| Pos::new(x, y)))
    }

    pub fn dice(&self) -> impl Iterator<Item = (Pos, Die)> + '_ {
        self.positions().filter_map(|p| self.get(p).map(|d| (p, d)))
    }

    /// Every neighborhood: each maximal set of same-value dice connected
    /// orthogonally, singletons included.
    pub fn neighborhoods(&self) -> Vec<Vec<Pos>> {
        let mut seen = vec![false; self.cells.len()];
        let mut hoods = Vec::new();
        for start in self.positions() {
            let Some(die) = self.get(start) else { continue };
            let index = |p: Pos| p.y as usize * self.n + p.x as usize;
            if seen[index(start)] {
                continue;
            }
            seen[index(start)] = true;
            let mut hood = vec![start];
            let mut i = 0;
            while i < hood.len() {
                let p = hood[i];
                i += 1;
                for d in ORTHOGONAL {
                    let q = p + d;
                    if self.get(q).is_some_and(|o| o.value == die.value) && !seen[index(q)] {
                        seen[index(q)] = true;
                        hood.push(q);
                    }
                }
            }
            hoods.push(hood);
        }
        hoods
    }

    /// Slides every die as far as it goes in `dir`, keeping their order along
    /// each line. Returns the moves, empty if the board was already settled.
    pub fn fall(&mut self, dir: Dir) -> Vec<Move> {
        let n = self.n as i32;
        let step = dir.step();
        // Each line starts at the far end (where dice come to rest) and walks back.
        let line_starts: Vec<Pos> = match dir {
            Dir::Down => (0..n).map(|x| Pos::new(x, n - 1)).collect(),
            Dir::Up => (0..n).map(|x| Pos::new(x, 0)).collect(),
            Dir::Left => (0..n).map(|y| Pos::new(0, y)).collect(),
            Dir::Right => (0..n).map(|y| Pos::new(n - 1, y)).collect(),
        };
        let mut moves = Vec::new();
        for start in line_starts {
            let mut rest = start;
            let mut p = start;
            while self.contains(p) {
                if let Some(die) = self.get(p) {
                    if p != rest {
                        self.set(p, None);
                        self.set(rest, Some(die));
                        moves.push(Move { from: p, to: rest, die });
                    }
                    rest = rest - step;
                }
                p = p - step;
            }
        }
        moves
    }

    /// True if no die has an empty cell beyond it in `dir`.
    pub fn is_settled(&self, dir: Dir) -> bool {
        self.dice().all(|(p, _)| !self.is_empty_at(p + dir.step()))
    }
}

impl fmt::Debug for Board {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f)?;
        for y in 0..self.n as i32 {
            let row: Vec<String> = (0..self.n as i32)
                .map(|x| self.get(Pos::new(x, y)).map_or(".".into(), |d| d.value.to_string()))
                .collect();
            writeln!(f, "  {}", row.join(" "))?;
        }
        Ok(())
    }
}
