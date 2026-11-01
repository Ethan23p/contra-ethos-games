//! The game on top of the field: the launcher's queue and the pool it draws
//! from, Exclude, scoring and the combo, nova, and the ending.

use std::fmt::Write;

use glam::Vec2;

use crate::field::take;
use crate::{Event, Field, GROUPS, Physics, Rng};

/// Every gameplay constant. `Default` is the spec's shipped set.
#[derive(Clone, Debug, PartialEq)]
pub struct Rules {
    /// The pool's ceiling never drops below this value; past it, the ceiling
    /// is half the peak.
    pub ceiling: u32,
    /// How many values the pool holds at first.
    pub window: usize,
    /// Whether blobs below the pool are excluded from the field. Off, a low
    /// value stays in the pool until its last blob leaves the field.
    pub exclude: bool,
    /// Difficulty rises each time the record peak doubles this many times.
    pub interval: u32,
    /// How likely a rise is to add a group, against the other levers.
    pub group_lever: f64,
    /// How likely a rise is to widen the pool by one value.
    pub pool_lever: f64,
    /// Groups never grow past this.
    pub max_groups: u8,
    /// Draw weight falls by this factor per step above the pool's floor.
    pub falloff: f64,
    /// Queue length, the blob on the launcher included.
    pub queue: usize,
    /// The combo counts merges over this many launches, the current one
    /// included.
    pub combo_launches: usize,
    /// Points for a nova.
    pub nova_bonus: u64,
    /// How many blobs a nova seeds, per group in play.
    pub nova_blobs: usize,
    /// Whether a nova takes one blob left of a group, or of the whole field.
    pub group_nova: bool,
    /// The game ends when a blob stays outside the rim this long (s).
    pub grace: f32,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            ceiling: 8,
            window: 3,
            exclude: true,
            interval: 2,
            group_lever: 1.0,
            pool_lever: 1.0,
            max_groups: GROUPS,
            falloff: 1.25,
            queue: 2,
            combo_launches: 3,
            nova_bonus: 2000,
            nova_blobs: 4,
            group_nova: true,
            grace: 6.5,
        }
    }
}

impl Rules {
    /// Pool window: the values the launcher may draw, lowest first. Ceiling =
    /// max(8, peak ÷ 2); `window` + `widened` consecutive values counting down
    /// from it, never below 2.
    pub fn pool(&self, peak: u32, widened: u32) -> Vec<u32> {
        let ceiling = self.ceiling.max(2).max(peak / 2).ilog2();
        let count = (self.window.max(1) + widened as usize).min(ceiling as usize);
        (ceiling + 1 - count as u32..=ceiling).map(|k| 1 << k).collect()
    }
}

#[derive(Clone, Debug)]
pub struct Game {
    pub field: Field,
    pub rules: Rules,
    /// The launcher's queue of (group, value), the head on the launcher.
    pub queue: Vec<(u8, u32)>,
    /// Entries due next, ahead of the draw: the opening, and groups on their
    /// way in. A value of 0 is drawn when its turn comes.
    pub script: Vec<(u8, u32)>,
    /// Groups in play, the first this many. A new one counts from its first
    /// launch, so it reaches the field through the launcher.
    pub groups: u8,
    /// Values the pool has gained from difficulty.
    pub widened: u32,
    /// log₂ of the highest peak so far.
    pub record: u32,
    /// Doublings of the record since difficulty last rose.
    pub raises: u32,
    /// Whether a nova may fire: one per launch, so a seed that merges down
    /// can't set off the next.
    pub charged: bool,
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
    /// A fresh game, opening on one group: two launches teach a merge and a
    /// nova, and the next brings in a second group. For a merge size m, that's
    /// m − 1 2s, m − 2 4s and m − 1 8s on the field: a launched 2 makes the 4s
    /// up to m − 1, a launched 4 makes an 8, and the 8s merge down to one blob.
    pub fn new(p: &Physics, rules: Rules, seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let mut field = Field::new();
        let m = p.m.max(2);
        let mut values = [(2, m - 1), (4, m - 2), (8, m - 1)].into_iter().flat_map(|(v, n)| vec![v; n]);
        field.scatter(p, &mut rng, Vec2::ZERO, 3 * m - 4, 1, |_| values.next().unwrap());
        let mut script = vec![(0, 2), (0, 4)];
        if rules.max_groups > 1 {
            script.push((1, 0));
        }
        let mut game = Self {
            field,
            rules,
            queue: Vec::new(),
            script,
            groups: 1,
            widened: 0,
            record: 3,
            raises: 0,
            charged: true,
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

    /// The whole game as text, to resume later. A finished game saves as
    /// nothing, so the next visit starts fresh.
    pub fn to_save(&self) -> String {
        if self.over {
            return String::new();
        }
        let mut s = format!("blob2 {} {} {}", self.score, self.combo, self.rng.state());
        let charged = self.charged as u8;
        write!(s, " {} {} {} {} {charged}", self.groups, self.widened, self.record, self.raises).unwrap();
        self.field.save(&mut s);
        write!(s, " {}", self.windows.len()).unwrap();
        for w in &self.windows {
            write!(s, " {w}").unwrap();
        }
        write!(s, " {}", self.queue.len()).unwrap();
        for (g, v) in &self.queue {
            write!(s, " {g} {v}").unwrap();
        }
        write!(s, " {}", self.outside.len()).unwrap();
        for (id, t) in &self.outside {
            write!(s, " {id} {t}").unwrap();
        }
        write!(s, " {}", self.script.len()).unwrap();
        for (g, v) in &self.script {
            write!(s, " {g} {v}").unwrap();
        }
        s
    }

    /// A game from [`Game::to_save`], or `None` if the text isn't one.
    pub fn from_save(rules: Rules, text: &str) -> Option<Self> {
        let t = &mut text.split_ascii_whitespace();
        if t.next()? != "blob2" {
            return None;
        }
        let (score, combo, rng) = (take(t)?, take(t)?, Rng::new(take(t)?));
        let (groups, widened, record, raises) = (take(t)?, take(t)?, take(t)?, take(t)?);
        let charged = take::<u8>(t)? != 0;
        let field = Field::load(t)?;
        let mut windows = Vec::new();
        for _ in 0..take::<usize>(t)? {
            windows.push(take(t)?);
        }
        let mut queue = Vec::new();
        for _ in 0..take::<usize>(t)? {
            queue.push((take(t)?, take(t)?));
        }
        let mut outside = Vec::new();
        for _ in 0..take::<usize>(t)? {
            outside.push((take(t)?, take(t)?));
        }
        let mut script = Vec::new();
        for _ in 0..take::<usize>(t)? {
            script.push((take(t)?, take(t)?));
        }
        if field.blobs.is_empty() || windows.is_empty() {
            return None;
        }
        let events = Vec::new();
        let mut game = Self {
            field,
            rules,
            queue,
            script,
            groups,
            widened,
            record,
            raises,
            charged,
            score,
            combo,
            windows,
            outside,
            over: false,
            rng,
            events,
        };
        game.refill();
        Some(game)
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

    /// The pool's window, and with Exclude off, any lower values still on the
    /// field.
    pub fn pool(&self) -> Vec<u32> {
        let mut pool = self.rules.pool(self.peak(), self.widened);
        if !self.rules.exclude {
            let floor = pool[0];
            let mut low: Vec<u32> = self.field.blobs.iter().map(|b| b.value).filter(|&v| v < floor).collect();
            low.sort_unstable();
            low.dedup();
            low.extend(pool);
            pool = low;
        }
        pool
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
            let (group, value) = if self.script.is_empty() {
                (self.rng.below(self.groups.max(1) as usize) as u8, 0)
            } else {
                self.script.remove(0)
            };
            let value = if value == 0 { self.draw(&pool) } else { value };
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
        self.groups = self.groups.max(group + 1);
        self.charged = true;
        let id = self.field.launch(p, angle, group, value);
        self.windows.push(0);
        let excess = self.windows.len().saturating_sub(self.rules.combo_launches.max(1));
        self.windows.drain(..excess);
        self.combo = 1 + self.windows.iter().sum::<u32>();
        self.refill();
        Some(id)
    }

    /// Move → Merge → Exclude, then scoring, nova and the ending. Once over,
    /// the field still moves but the rules stop.
    pub fn step(&mut self, p: &Physics, dt: f32) {
        self.field.step(p, dt);
        let events = self.field.take_events();
        if self.over {
            self.events.extend(events);
            return;
        }
        self.score(&events);
        let merged: Vec<u8> = events
            .iter()
            .filter_map(|e| match e {
                &Event::Merged { blob, .. } => self.field.blob(blob).map(|b| b.group),
                _ => None,
            })
            .collect();
        self.events.extend(events);
        self.exclude();
        self.difficulty();
        self.nova(p, &merged);
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

    /// Exclude: blobs whose value has fallen out of the pool leave the field.
    /// Blobs merging or about to finish that first.
    fn exclude(&mut self) {
        if !self.rules.exclude {
            return;
        }
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
                self.events.push(Event::Excluded { blob: b.id, group: b.group, value: b.value, at: b.pos });
            }
        }
    }

    /// Difficulty: each `interval` doublings of the record peak, one lever,
    /// chosen at random by weight from those with room left.
    fn difficulty(&mut self) {
        let level = self.peak().ilog2();
        // A nova's seed merging down earned nothing.
        if !self.charged {
            self.record = self.record.max(level);
        }
        while self.record < level {
            self.record += 1;
            self.raises += 1;
            if self.raises >= self.rules.interval.max(1) {
                self.raises = 0;
                self.rise();
            }
        }
    }

    fn rise(&mut self) {
        let (r, peak) = (&self.rules, self.peak());
        let next = self.script.iter().chain(&self.queue).map(|e| e.0 + 1).fold(self.groups, u8::max);
        let group = if next < r.max_groups { r.group_lever } else { 0.0 };
        let wider = r.pool(peak, self.widened + 1).len() > r.pool(peak, self.widened).len();
        let pool = if wider { r.pool_lever } else { 0.0 };
        if group + pool <= 0.0 {
            return;
        }
        match self.rng.weighted(&[group, pool]) {
            0 => self.script.push((next, 0)),
            _ => self.widened += 1,
        }
    }

    /// Nova: when a merge leaves one blob of its group, +bonus, and Seed
    /// replaces it with blobs of its value or half of it, in the groups in
    /// play. A group's first blob has merged with nothing, so it's safe. One
    /// nova per launch. A field emptied outright reseeds as a new game would.
    fn nova(&mut self, p: &Physics, merged: &[u8]) {
        let f = &self.field;
        if f.blobs.is_empty() {
            self.field.seed(p, &mut self.rng, self.groups);
            return;
        }
        if !self.charged {
            return;
        }
        let group_nova = self.rules.group_nova;
        let lone = merged.iter().find_map(|&g| {
            let mut of = f.blobs.iter().filter(|b| !group_nova || b.group == g);
            let first = of.next()?;
            of.next().is_none().then_some(first.id)
        });
        let Some(last) = lone else { return };
        let b = self.field.remove(last).unwrap();
        self.score += self.rules.nova_bonus;
        self.charged = false;
        let value = b.value;
        let count = self.rules.nova_blobs * self.groups as usize;
        self.field.scatter(p, &mut self.rng, b.pos, count, self.groups, |rng| {
            if rng.below(2) == 0 { value } else { (value / 2).max(2) }
        });
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
