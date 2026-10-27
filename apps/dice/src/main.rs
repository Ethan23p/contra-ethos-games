//! Dice Merge: the rules from `crates/dice`, drawn and animated with the engine.
//!
//! Drag any of the staged clusters below the board onto it; tap one to rotate
//! it. Its slot restocks once it's placed. A placed turn plays back as a timeline (see `player.rs`), and placing again
//! mid-animation skips to its end, so fast play never waits.
//!
//! Native: `cargo run -p dice-app`, or `-- --snapshot out.png` for a headless
//! strip of a cascading merge (`--snapshot-drag` for the drag preview).

mod config;
mod look;
mod player;

use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, TAU};

use dice::{Cluster, Game, Pos, Step};
use engine::glam::Vec2;
use engine::{App, Frame, Input, Screen, Shapes, Tuner, View, rgb, rgba};

use config::Config;
use look::*;
use player::{Fx, Player};

const BEST_KEY: &str = "dice-best";

enum Hand {
    Idle,
    /// Holding a slot's cluster by its cell `grab`; `finger` is in world units.
    Dragging {
        slot: usize,
        grab: usize,
        finger: Vec2,
        since: f32,
    },
    /// Springing back to its slot after a rejected drop.
    Returning {
        slot: usize,
        pos: Vec2,
        vel: Vec2,
        pitch: f32,
        dist: f32,
    },
}

/// A visual quarter-turn (or a single die's spin) easing out to rest.
struct Turn {
    from: f32,
    start: f32,
    secs: f32,
}

struct DiceApp {
    tuner: Tuner<Config>,
    game: Game,
    player: Player,
    view: View,
    staging: Staging,
    hand: Hand,
    /// Each slot's latest turn.
    turns: Vec<Turn>,
    /// When each slot was last restocked, for the entrance pop.
    entered: Vec<f32>,
    best: f64,
    /// Whether the best score is saved; off for headless snapshots.
    persist: bool,
    time: f32,
}

impl DiceApp {
    fn new(tuner: Tuner<Config>, seed: u64) -> Self {
        let game = Game::new(tuner.current.rules(), seed);
        let best = engine::tune::storage::load(BEST_KEY).and_then(|s| s.trim().parse().ok()).unwrap_or(0.0);
        let n = game.slots().len();
        let mut app = Self {
            player: Player::new(game.board().clone()),
            game,
            tuner,
            view: View::PIXELS,
            staging: Staging { centers: Vec::new(), half: Vec2::ZERO, pitch: 0.0 },
            hand: Hand::Idle,
            turns: (0..n).map(|_| Turn { from: 0.0, start: 0.0, secs: 0.0 }).collect(),
            entered: vec![0.0; n],
            best,
            persist: true,
            time: 0.0,
        };
        // A stand-in until the first frame lays out the real screen.
        app.hud(Screen::new(Vec2::new(375.0, 812.0), 1.0));
        app
    }

    /// Lays out the screen, keeping the view and the slots for input.
    fn hud(&mut self, screen: Screen) -> Hud {
        let hud = Hud::new(screen, self.game.slots().len(), self.layout().pitch, &self.tuner.current);
        self.view = hud.view;
        self.staging = hud.staging.clone();
        hud
    }

    fn new_game(&mut self) {
        self.game = Game::new(self.tuner.current.rules(), fresh_seed());
        self.player = Player::new(self.game.board().clone());
        self.hand = Hand::Idle;
        let n = self.game.slots().len();
        self.turns = (0..n).map(|_| Turn { from: 0.0, start: 0.0, secs: 0.0 }).collect();
        self.entered = vec![self.time; n];
    }

    fn layout(&self) -> Layout {
        Layout::new(self.game.rules.size)
    }

    fn rotate(&mut self, slot: usize) {
        let c = &self.tuner.current;
        let cluster = self.game.slot(slot);
        let heaviest = cluster.cells.iter().map(|d| d.1).max().unwrap_or(1);
        let (from, ms) = if cluster.len() == 1 {
            (-TAU, c.spin_ms)
        } else {
            (-FRAC_PI_2, (c.rotate_ms * c.weight(heaviest)).min(c.rotate_max_ms))
        };
        self.game.rotate(slot);
        self.turns[slot] = Turn { from, start: self.time, secs: ms / 1000.0 };
    }

    fn turn_angle(&self, slot: usize) -> f32 {
        let t = &self.turns[slot];
        let k = if t.secs > 0.0 { (self.time - t.start) / t.secs } else { 1.0 };
        t.from * (1.0 - ease_out(k))
    }

    /// Where the dragged cluster's grabbed die is drawn: above the finger, lifted in.
    fn grabbed_at(&self, grab_finger: Vec2, since: f32) -> Vec2 {
        let c = &self.tuner.current;
        let k = ease_out((self.time - since) / (c.lift_ms / 1000.0).max(1e-3));
        grab_finger - Vec2::new(0.0, c.lift * self.layout().pitch * k)
    }

    /// Board cell a slot's `grab` cell would land on, and whether it's legal.
    fn target(&self, slot: usize, grab: usize, finger: Vec2, since: f32) -> (Pos, bool) {
        let at = self.layout().cell_at(self.grabbed_at(finger, since));
        (at, self.game.can_place(slot, grab, at))
    }

    fn drop_cluster(&mut self, slot: usize, grab: usize, finger: Vec2, since: f32) {
        let layout = self.layout();
        let (at, ok) = self.target(slot, grab, finger, since);
        let held = self.game.slot(slot).clone();
        if ok {
            let grabbed = self.grabbed_at(finger, since);
            let dropped: HashMap<Pos, Vec2> = held
                .cells_at(grab, at)
                .zip(&held.cells)
                .map(|((p, _), (off, _))| {
                    let rel = cell(*off) - cell(held.cells[grab].0);
                    (p, layout.cell_of(grabbed + rel * layout.pitch))
                })
                .collect();
            let turn = self.game.place(slot, grab, at).expect("checked legal");
            for step in &turn.steps {
                if let Step::Spawn { index, .. } | Step::Replace { index, .. } = step {
                    self.entered[*index] = self.time;
                    self.turns[*index].secs = 0.0;
                }
            }
            self.player.play(self.time, turn.steps, dropped, &self.tuner.current);
            self.hand = Hand::Idle;
            if self.game.score() > self.best {
                self.best = self.game.score();
                if self.persist {
                    engine::tune::storage::save(BEST_KEY, &self.best.to_string());
                }
            }
        } else {
            let cells: Vec<Pos> =
                held.cells_at(grab, at).map(|(p, _)| p).filter(|&p| self.game.board().contains(p)).collect();
            if !cells.is_empty() {
                self.player.shake(self.time, cells);
            }
            let pos = self.cluster_center_from_grab(slot, grab, self.grabbed_at(finger, since), layout.pitch);
            let dist = (pos - self.staging.centers[slot]).length().max(1e-3);
            self.hand = Hand::Returning { slot, pos, vel: Vec2::ZERO, pitch: layout.pitch, dist };
        }
    }

    fn cluster_center_from_grab(&self, slot: usize, grab: usize, grabbed: Vec2, pitch: f32) -> Vec2 {
        let cluster = self.game.slot(slot);
        grabbed - (cell(cluster.cells[grab].0) - mid(cluster)) * pitch
    }

    /// Keeps the game's rules in step with the panel. Board size and the
    /// number of slots start a new game; the rest apply mid-game.
    fn sync_rules(&mut self) {
        let rules = self.tuner.current.rules();
        if rules.size != self.game.rules.size || rules.slots != self.game.rules.slots {
            self.new_game();
        } else if rules != self.game.rules {
            self.game.rules = rules;
        }
    }

    fn draw_cluster(&self, s: &mut Shapes, cluster: &Cluster, center: Vec2, pitch: f32, angle: f32, alpha: f32) {
        let c = &self.tuner.current;
        let r = Vec2::from_angle(angle);
        let size = (pitch - GAP) * c.die_size;
        for &(off, value) in &cluster.cells {
            let p = center + r.rotate((cell(off) - mid(cluster)) * pitch);
            draw_die(s, c, p, size, value, angle, alpha);
        }
    }
}

fn fresh_seed() -> u64 {
    web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64)
}

/// The centre of a cluster's bounding box, in its own cell coordinates.
fn mid(cluster: &Cluster) -> Vec2 {
    let max = cluster.cells.iter().fold(Vec2::ZERO, |m, (p, _)| m.max(cell(*p)));
    max * 0.5
}

impl App for DiceApp {
    fn input(&mut self, input: Input, _: &Frame) {
        let view = self.view;
        let world = |p: Vec2| view.to_world(p);
        match input {
            Input::Tap(p) => {
                if self.game.is_over() && !self.player.busy() {
                    self.new_game();
                } else if let Some(slot) = self.staging.slot_at(world(p))
                    && matches!(self.hand, Hand::Idle)
                    && !self.game.is_over()
                {
                    self.rotate(slot);
                }
            }
            Input::DragStart(p) => {
                let p = world(p);
                // Anywhere in a slot picks up its cluster, by the nearest die.
                if let Some(slot) = self.staging.slot_at(p)
                    && !self.game.is_over()
                {
                    let (cluster, center) = (self.game.slot(slot), self.staging.centers[slot]);
                    let nearest = (0..cluster.len())
                        .min_by(|&a, &b| {
                            let d = |i: usize| {
                                (center + (cell(cluster.cells[i].0) - mid(cluster)) * self.staging.pitch - p).length()
                            };
                            d(a).total_cmp(&d(b))
                        })
                        .unwrap();
                    self.turns[slot].secs = 0.0;
                    self.hand = Hand::Dragging { slot, grab: nearest, finger: p, since: self.time };
                }
            }
            Input::Drag(p) => {
                if let Hand::Dragging { finger, .. } = &mut self.hand {
                    *finger = world(p);
                }
            }
            Input::DragEnd(p) => {
                if let Hand::Dragging { slot, grab, since, .. } = self.hand {
                    self.drop_cluster(slot, grab, world(p), since);
                }
            }
            _ => {}
        }
    }

    fn update(&mut self, dt: f32) {
        self.time += dt;
        self.sync_rules();
        let c = &self.tuner.current;
        if let Hand::Returning { slot, pos, vel, .. } = &mut self.hand {
            let home = self.staging.centers[*slot];
            let (stiffness, damping) = c.snap_spring();
            let accel = -stiffness * (*pos - home) - damping * *vel;
            *vel += accel * dt;
            *pos += *vel * dt;
            if (*pos - home).length() < 0.05 && vel.length() < 0.5 {
                self.hand = Hand::Idle;
            }
        }
        self.player.update(self.time, c);
    }

    fn frame(&mut self, f: &Frame, s: &mut Shapes) {
        let c = self.tuner.current.clone();
        let now = self.time;
        s.clear = rgb(BG);
        let hud = self.hud(f.screen());
        s.view = self.view;
        let layout = self.layout();
        let die = layout.die(&c);

        hud.score(s, self.game.score(), self.best);

        layout.draw(s);

        // Drop preview.
        if let Hand::Dragging { slot, grab, finger, since } = self.hand {
            let (at, ok) = self.target(slot, grab, finger, since);
            let tint = if ok { ACCENT } else { RED };
            for (p, _) in self.game.slot(slot).cells_at(grab, at) {
                if self.game.board().contains(p) {
                    layout
                        .cell_rect(s, p, Vec2::ZERO, rgba(tint, if ok { 0.28 } else { 0.22 }))
                        .outline(0.5, rgb(tint));
                }
            }
        }
        for e in &self.player.effects {
            if let Fx::Shake { cells } = &e.fx {
                let k = (now - e.start) / (c.shake_ms / 1000.0).max(1e-3);
                if (0.0..1.0).contains(&k) {
                    let nudge = Vec2::new((k * TAU * 2.0).sin() * c.shake * (1.0 - k), 0.0);
                    for &p in cells {
                        layout.cell_rect(s, p, nudge, rgba(RED, 0.22 * (1.0 - k))).outline(0.5, rgba(RED, 1.0 - k));
                    }
                }
            }
        }

        // Settled dice, with any beat or pop on them.
        let hidden = self.player.hidden();
        let merged = self.player.merged(now, &c);
        for (p, d) in self.player.base.dice() {
            if hidden.contains(&p) {
                continue;
            }
            let value = merged.iter().find(|m| m.0 == p).map_or(d.value, |m| m.1);
            let mut scale = 1.0;
            let mut glow = 0.0;
            for e in self.player.effects.iter().filter(|e| now >= e.start) {
                let t = now - e.start;
                match e.fx {
                    Fx::Beat { at, amp } if at == p => {
                        let k = t / (c.ring_ms / 1000.0 * 0.6).max(1e-3);
                        if k < 1.0 {
                            let shape = if k < 0.2 { ease_out(k / 0.2) } else { 1.0 - smooth((k - 0.2) / 0.8) };
                            scale *= 1.0 + (c.beat_scale - 1.0) * amp * shape;
                        }
                    }
                    Fx::Pop { at, impact } if at == p => {
                        let k = t / (c.pop_ms / 1000.0).max(1e-3);
                        if k < 1.0 {
                            let peak = (c.pop_base + c.pop_per_impact * (impact - 1.0)).min(c.pop_cap);
                            scale *= if k < 0.45 {
                                0.6 + (peak - 0.6) * ease_out(k / 0.45)
                            } else {
                                peak + (1.0 - peak) * smooth((k - 0.45) / 0.55)
                            };
                            // Impact grows exponentially with value; past 4 the
                            // glow would swamp the board.
                            glow = (1.0 - k) * c.glow * (1.0 + 0.3 * (impact.min(4.0) - 1.0));
                        }
                    }
                    _ => {}
                }
            }
            let center = layout.center(p);
            if glow > 0.0 {
                s.rounded_rect(center, Vec2::splat(die * scale + glow), die * c.die_corner, rgba(0xffffff, 0.35))
                    .soft(glow * 2.0);
            }
            draw_die(s, &c, center, die * scale, value, 0.0, 1.0);
        }
        for m in self.player.movers(now, &c) {
            draw_die(s, &c, layout.at(m.cell), die, m.value, 0.0, 1.0);
        }

        // Rings and score floats ride above the dice.
        for e in self.player.effects.iter().filter(|e| now >= e.start) {
            let t = now - e.start;
            match &e.fx {
                Fx::Ring { at, value, amp } => {
                    let k = t / (c.ring_ms / 1000.0).max(1e-3);
                    if k < 1.0 {
                        let travel = c.ring_travel * (0.6 + 0.4 * amp) * ease_out(k);
                        let color = with_alpha(die_color(*value), (0.55 + 0.45 * amp) * (1.0 - k));
                        let size = die + 2.0 * travel;
                        s.rounded_rect(
                            layout.center(*at),
                            Vec2::splat(size),
                            die * c.die_corner + travel,
                            rgba(0, 0.0),
                        )
                        .outline(c.ring_width, color);
                    }
                }
                Fx::Float { at, text } => {
                    let k = t / 0.8;
                    if k < 1.0 {
                        let p = layout.center(*at) - Vec2::new(0.0, layout.pitch * 0.6 + 8.0 * ease_out(k));
                        s.text(p, 4.0, text, rgba(TEXT, 1.0 - k * k)).outline(0.4, rgba(0, 0.5 * (1.0 - k)));
                    }
                }
                _ => {}
            }
        }

        // Hint and staging. The held cluster is drawn last, above the rest.
        hud.hint(s, "DRAG TO PLACE · TAP TO ROTATE");
        let held = match self.hand {
            Hand::Dragging { slot, .. } | Hand::Returning { slot, .. } => Some(slot),
            Hand::Idle => None,
        };
        let pitch = self.staging.pitch;
        if !self.game.is_over() {
            for (i, cluster) in self.game.slots().iter().enumerate().filter(|&(i, _)| Some(i) != held) {
                let k = ease_out((now - self.entered[i]) / (c.entrance_ms / 1000.0).max(1e-3));
                let scale = c.entrance_scale + (1.0 - c.entrance_scale) * k;
                self.draw_cluster(s, cluster, self.staging.centers[i], pitch * scale, self.turn_angle(i), 1.0);
            }
        }
        match self.hand {
            Hand::Returning { slot, pos, pitch: from, dist, .. } => {
                let k = ((pos - self.staging.centers[slot]).length() / dist).min(1.0);
                self.draw_cluster(s, self.game.slot(slot), pos, pitch + (from - pitch) * k, 0.0, 1.0);
            }
            Hand::Dragging { slot, grab, finger, since } => {
                let center = self.cluster_center_from_grab(slot, grab, self.grabbed_at(finger, since), layout.pitch);
                self.draw_cluster(s, self.game.slot(slot), center, layout.pitch, 0.0, 1.0);
            }
            Hand::Idle => {}
        }

        // Game over.
        if self.game.is_over() && !self.player.busy() {
            hud.game_over(s, self.game.score());
        }
    }

    fn config(&mut self) -> Option<&mut dyn engine::tune::Config> {
        Some(&mut self.tuner)
    }

    fn config_widget_top(&self) -> bool {
        true
    }

    fn config_actions(&mut self, ui: &mut engine::egui::Ui) {
        if ui
            .add(engine::egui::Button::new("New game").min_size([ui.available_width(), 1.875 * engine::EM].into()))
            .clicked()
        {
            self.new_game();
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn scripted() -> DiceApp {
    // A two-generation cascade: the new 1 joins three 1s, and their 2 joins two 2s.
    let rows = ["1 2 3 4 5", "6 7 8 9 10", "2 2 . 4 .", ". 1 1 1 .", ". . . 3 3"];
    let mut app = DiceApp::new(Tuner::unsaved(), 1);
    (app.persist, app.best) = (false, 0.0);
    app.game = Game::with_board(app.game.rules.clone(), 1, dice::Board::parse(&rows));
    app.player = Player::new(app.game.board().clone());
    let mut slots = app.game.slots().to_vec();
    slots[0] = Cluster { cells: vec![(Pos::new(0, 0), 1)] };
    app.game.set_slots(slots);
    // Settled in, past their entrance pop.
    app.entered.fill(-10.0);
    app
}

fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let args: Vec<String> = std::env::args().collect();
        let path = |i: usize| args.get(i + 1).map_or("snapshot.png", String::as_str).to_string();
        if let Some(i) = args.iter().position(|a| a == "--snapshot") {
            let mut app = scripted();
            let layout = app.layout();
            app.tuner.current.lift = 0.0;
            app.drop_cluster(0, 0, layout.center(Pos::new(2, 2)), -1.0);
            let times = [0.0, 0.2, 0.35, 0.5, 0.65, 0.9, 1.3];
            engine::headless::snapshot_strip(&mut app, 375, 812, &times, path(i).as_ref());
            println!("wrote {}", path(i));
            return;
        }
        if let Some(i) = args.iter().position(|a| a == "--snapshot-drag") {
            let mut app = scripted();
            let layout = app.layout();
            app.hand = Hand::Dragging { slot: 0, grab: 0, finger: layout.center(Pos::new(4, 3)), since: -1.0 };
            app.tuner.current.lift = 0.0;
            engine::headless::snapshot_strip(&mut app, 375, 812, &[0.0], path(i).as_ref());
            println!("wrote {}", path(i));
            return;
        }
    }
    engine::run(DiceApp::new(Tuner::new(), fresh_seed()));
}
