//! The game on top of the field: the launcher's queue and the pool it draws
//! from, Purge, scoring and the combo, nova, and the ending.

use crate::{Event, Field, GROUPS, Physics, Rng};

/// Every gameplay constant. `Default` is the spec's shipped set.
#[derive(Clone, Debug, PartialEq)]
pub struct Rules {
    /// The pool's ceiling never drops below this value; past it, the ceiling
    /// is half the peak.
    pub ceiling: u32,
    /// How many values the pool holds at first.
    pub window: usize,
    /// The pool widens by one value per this many doublings of the peak past
    /// the ceiling's minimum.
    pub widen_every: u32,
    /// Draw weight falls by this factor per step above the pool's floor.
    pub falloff: f64,
    /// Queue length, the blob on the launcher included.
    pub queue: usize,
    /// The combo counts merges over this many launches, the current one
    /// included.
    pub combo_launches: usize,
    /// Points for a nova.
    pub nova_bonus: u64,
    /// How many blobs a nova seeds.
    pub nova_blobs: usize,
    /// The game ends when a blob stays outside the rim this long (s).
    pub grace: f32,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            ceiling: 8,
            window: 3,
            widen_every: 3,
            falloff: 1.25,
            queue: 2,
            combo_launches: 3,
            nova_bonus: 2000,
            nova_blobs: 12,
            grace: 6.5,
        }
    }
}

impl Rules {
    /// Pool window: the values the launcher may draw, lowest first. Ceiling =
    /// max(8, peak ÷ 2); 3 + floor((log₂ peak − 3) ÷ 3) consecutive values
    /// counting down from it, never below 2. Below a peak of 8 the window
    /// stays at 3 rather than narrowing.
    pub fn pool(&self, peak: u32) -> Vec<u32> {
        let ceiling = self.ceiling.max(2).max(peak / 2).ilog2();
        let past = peak.max(1).ilog2().saturating_sub(self.ceiling.max(1).ilog2());
        let count = (self.window.max(1) + (past / self.widen_every.max(1)) as usize).min(ceiling as usize);
        (ceiling + 1 - count as u32..=ceiling).map(|k| 1 << k).collect()
    }
}

#[derive(Clone, Debug)]
pub struct Game {
    pub field: Field,
    pub rules: Rules,
    /// The launcher's queue of (group, value), the head on the launcher.
    pub queue: Vec<(u8, u32)>,
    pub score: u64,
    pub combo: u32,
    /// Combo credit per launch window, oldest first, the current one last.
    /// A window runs from one launch to the moment before the next.
    pub windows: Vec<u32>,
    /// Blobs outside the rim, and for how long.
    pub outside: Vec<(u32, f32)>,
    pub over: bool,
    rng: Rng,
    events: Vec<Event>,
}

impl Game {
    /// A fresh game: Seed, then fill the queue from the pool.
    pub fn new(p: &Physics, rules: Rules, seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let mut field = Field::new();
        field.seed(p, &mut rng);
        let mut game = Self {
            field,
            rules,
            queue: Vec::new(),
            score: 0,
            combo: 1,
            windows: vec![0],
            outside: Vec::new(),
            over: false,
            rng,
            events: Vec::new(),
        };
        game.refill();
        game
    }

    /// Field events and the game's own, since the last call.
    pub fn take_events(&mut self) -> Vec<Event> {
        let mut events = std::mem::take(&mut self.events);
        events.extend(self.field.take_events());
        events
    }

    /// The highest value on the field.
    pub fn peak(&self) -> u32 {
        self.field.blobs.iter().map(|b| b.value).max().unwrap_or(1)
    }

    pub fn pool(&self) -> Vec<u32> {
        self.rules.pool(self.peak())
    }

    /// Draw: a uniform group, and a value from the pool weighted
    /// 1 / falloff^i, i counting steps above the floor.
    fn draw(&mut self, pool: &[u32]) -> u32 {
        let weights: Vec<f64> = (0..pool.len()).map(|i| self.rules.falloff.powi(-(i as i32))).collect();
        pool[self.rng.weighted(&weights)]
    }

    /// Tops the queue up to length and re-rolls the value of any entry the
    /// pool no longer allows.
    fn refill(&mut self) {
        let pool = self.pool();
        for k in 0..self.queue.len() {
            if !pool.contains(&self.queue[k].1) {
                self.queue[k].1 = self.draw(&pool);
            }
        }
        self.queue.truncate(self.rules.queue.max(1));
        while self.queue.len() < self.rules.queue.max(1) {
            let group = self.rng.below(GROUPS as usize) as u8;
            let value = self.draw(&pool);
            self.queue.push((group, value));
        }
    }

    /// Launch the queue head at `angle`; refill from the pool. Nothing once
    /// the game is over.
    pub fn launch(&mut self, p: &Physics, angle: f32) -> Option<u32> {
        if self.over {
            return None;
        }
        let (group, value) = self.queue.remove(0);
        let id = self.field.launch(p, angle, group, value);
        self.windows.push(0);
        let excess = self.windows.len().saturating_sub(self.rules.combo_launches.max(1));
        self.windows.drain(..excess);
        self.combo = 1 + self.windows.iter().sum::<u32>();
        self.refill();
        Some(id)
    }

    /// Move → Merge → Purge, then scoring, nova and the ending. Once over,
    /// the field still moves but the rules stop.
    pub fn step(&mut self, p: &Physics, dt: f32) {
        self.field.step(p, dt);
        let events = self.field.take_events();
        if self.over {
            self.events.extend(events);
            return;
        }
        self.score(&events);
        self.events.extend(events);
        self.purge();
        self.nova(p);
        self.refill();
        self.watch_rim(p, dt);
    }

    /// Scoring: a merge of n blobs credits n − 1 to the current launch
    /// window, then scores the resulting value × combo. The combo is 1 plus
    /// the credit over the last few launch windows, so waiting never resets
    /// it, but launching moves old merges out of it.
    fn score(&mut self, events: &[Event]) {
        for e in events {
            if let &Event::Merged { blob, value, count, at, .. } = e {
                *self.windows.last_mut().unwrap() += count.saturating_sub(1) as u32;
                self.combo = 1 + self.windows.iter().sum::<u32>();
                let points = value as u64 * self.combo as u64;
                self.score += points;
                self.events.push(Event::Scored { blob, points, combo: self.combo, at });
            }
        }
    }

    /// Purge: blobs whose value has fallen below the pool leave the field.
    /// Blobs merging or about to finish that first.
    fn purge(&mut self) {
        let floor = self.pool()[0];
        let held: Vec<u32> = self.field.pending.iter().flat_map(|h| h.members.iter().copied()).collect();
        let gone: Vec<u32> = self
            .field
            .blobs
            .iter()
            .filter(|b| b.value < floor && b.merging.is_none() && !held.contains(&b.id))
            .map(|b| b.id)
            .collect();
        for id in gone {
            if let Some(b) = self.field.remove(id) {
                self.events.push(Event::Purged { blob: b.id, group: b.group, value: b.value, at: b.pos });
            }
        }
    }

    /// Nova: when one blob remains, +bonus, and Seed replaces it with blobs
    /// of its value or half of it. A field emptied outright reseeds as a new
    /// game would.
    fn nova(&mut self, p: &Physics) {
        let f = &self.field;
        if f.blobs.len() > 1 || !f.merges.is_empty() || !f.pending.is_empty() {
            return;
        }
        let Some(last) = f.blobs.first().map(|b| b.id) else {
            self.field.seed(p, &mut self.rng);
            return;
        };
        let b = self.field.remove(last).unwrap();
        self.score += self.rules.nova_bonus;
        self.field.centre = b.pos;
        let value = b.value;
        self.field.scatter(p, &mut self.rng, self.rules.nova_blobs, |rng| {
            if rng.below(2) == 0 { value } else { (value / 2).max(2) }
        });
        self.outside.clear();
        self.events.push(Event::Nova { value, at: b.pos, bonus: self.rules.nova_bonus });
    }

    /// Ending: the game ends when any blob stays outside the rim for the
    /// whole grace period.
    fn watch_rim(&mut self, p: &Physics, dt: f32) {
        let f = &self.field;
        let out: Vec<u32> = f.blobs.iter().filter(|b| b.pos.distance(f.centre) > p.rim).map(|b| b.id).collect();
        self.outside.retain(|(id, _)| out.contains(id));
        for id in out {
            match self.outside.iter_mut().find(|(k, _)| *k == id) {
                Some((_, t)) => *t += dt,
                None => self.outside.push((id, dt)),
            }
        }
        if self.outside.iter().any(|&(_, t)| t >= self.rules.grace) {
            self.over = true;
            self.events.push(Event::Ended { score: self.score });
        }
    }

    /// How close the field is to ending: the longest stay outside the rim as
    /// a fraction of the grace period, 0 to 1.
    pub fn danger(&self) -> f32 {
        let longest = self.outside.iter().map(|&(_, t)| t).fold(0.0, f32::max);
        (longest / self.rules.grace.max(1e-3)).min(1.0)
    }
}
