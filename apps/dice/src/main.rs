//! Dice Merge: the rules from `crates/dice`, drawn and animated with the engine.
//!
//! Drag the piece from below the board onto it; tap anywhere to rotate it. A
//! placed turn plays back as a timeline (see `player.rs`), and placing again
//! mid-animation skips to its end, so fast play never waits.
//!
//! Native: `cargo run -p dice-app`, or `-- --snapshot out.png` for a headless
//! strip of a cascading merge (`--snapshot-drag` for the drag preview).

mod config;
mod look;
mod player;

use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, TAU};

use dice::{Cluster, Game, Pos};
use engine::glam::Vec2;
use engine::{App, Frame, Input, Shapes, Tuner, View, rgb, rgba};

use config::Config;
use look::*;
use player::{Fx, Player};

const BEST_KEY: &str = "dice-best";

enum Hand {
    Idle,
    /// Holding the head by its cell `grab`; `finger` is in world units.
    Dragging {
        grab: usize,
        finger: Vec2,
        since: f32,
    },
    /// Springing back to the queue after a rejected drop.
    Returning {
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
    hand: Hand,
    turn: Turn,
    /// When the queue last changed, for the entrance pop.
    entered: f32,
    best: f64,
    /// Whether the best score is saved; off for headless snapshots.
    persist: bool,
    time: f32,
}

impl DiceApp {
    fn new(tuner: Tuner<Config>, seed: u64) -> Self {
        let game = Game::new(tuner.current.rules(), seed);
        let best = engine::tune::storage::load(BEST_KEY).and_then(|s| s.trim().parse().ok()).unwrap_or(0.0);
        Self {
            player: Player::new(game.board().clone()),
            game,
            tuner,
            view: View::PIXELS,
            hand: Hand::Idle,
            turn: Turn { from: 0.0, start: 0.0, secs: 0.0 },
            entered: 0.0,
            best,
            persist: true,
            time: 0.0,
        }
    }

    fn new_game(&mut self) {
        self.game = Game::new(self.tuner.current.rules(), fresh_seed());
        self.player = Player::new(self.game.board().clone());
        self.hand = Hand::Idle;
        self.entered = self.time;
    }

    fn layout(&self) -> Layout {
        Layout::new(self.game.rules.size)
    }

    fn rotate(&mut self) {
        let c = &self.tuner.current;
        let heaviest = self.game.head().cells.iter().map(|d| d.1).max().unwrap_or(1);
        let (from, ms) = if self.game.head().len() == 1 {
            (-TAU, c.spin_ms)
        } else {
            (-FRAC_PI_2, (c.rotate_ms * c.weight(heaviest)).min(c.rotate_max_ms))
        };
        self.game.rotate();
        self.turn = Turn { from, start: self.time, secs: ms / 1000.0 };
    }

    fn turn_angle(&self) -> f32 {
        let k = if self.turn.secs > 0.0 { (self.time - self.turn.start) / self.turn.secs } else { 1.0 };
        self.turn.from * (1.0 - ease_out(k))
    }

    /// Where the dragged head's grabbed die is drawn: above the finger, lifted in.
    fn grabbed_at(&self, grab_finger: Vec2, since: f32) -> Vec2 {
        let c = &self.tuner.current;
        let k = ease_out((self.time - since) / (c.lift_ms / 1000.0).max(1e-3));
        grab_finger - Vec2::new(0.0, c.lift * self.layout().pitch * k)
    }

    /// Board cell the head's `grab` cell would land on, and whether it's legal.
    fn target(&self, grab: usize, finger: Vec2, since: f32) -> (Pos, bool) {
        let at = self.layout().cell_at(self.grabbed_at(finger, since));
        (at, self.game.can_place(grab, at))
    }

    fn drop_head(&mut self, grab: usize, finger: Vec2, since: f32) {
        let layout = self.layout();
        let (at, ok) = self.target(grab, finger, since);
        if ok {
            let grabbed = self.grabbed_at(finger, since);
            let dropped: HashMap<Pos, Vec2> = self
                .game
                .head()
                .cells_at(grab, at)
                .zip(&self.game.head().cells)
                .map(|((p, _), (off, _))| {
                    let rel = cell(*off) - cell(self.game.head().cells[grab].0);
                    (p, layout.cell_of(grabbed + rel * layout.pitch))
                })
                .collect();
            let turn = self.game.place(grab, at).expect("checked legal");
            self.player.play(self.time, turn.steps, dropped, &self.tuner.current);
            self.entered = self.time;
            self.hand = Hand::Idle;
            if self.game.score() > self.best {
                self.best = self.game.score();
                if self.persist {
                    engine::tune::storage::save(BEST_KEY, &self.best.to_string());
                }
            }
        } else {
            let cells: Vec<Pos> = self
                .game
                .head()
                .cells_at(grab, at)
                .map(|(p, _)| p)
                .filter(|&p| self.game.board().contains(p))
                .collect();
            if !cells.is_empty() {
                self.player.shake(self.time, cells);
            }
            let pos = self.cluster_center_from_grab(grab, self.grabbed_at(finger, since), layout.pitch);
            let dist = (pos - HEAD_SLOT).length().max(1e-3);
            self.hand = Hand::Returning { pos, vel: Vec2::ZERO, pitch: layout.pitch, dist };
        }
    }

    fn cluster_center_from_grab(&self, grab: usize, grabbed: Vec2, pitch: f32) -> Vec2 {
        let head = self.game.head();
        grabbed - (cell(head.cells[grab].0) - mid(head)) * pitch
    }

    /// Keeps the game's rules in step with the panel. Board size and queue
    /// length start a new game; the rest apply mid-game.
    fn sync_rules(&mut self) {
        let rules = self.tuner.current.rules();
        if rules.size != self.game.rules.size || rules.queue_len != self.game.rules.queue_len {
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
            Input::Tap(_) => {
                if self.game.is_over() && !self.player.busy() {
                    self.new_game();
                } else if matches!(self.hand, Hand::Idle) && !self.game.is_over() {
                    self.rotate();
                }
            }
            Input::DragStart(p) => {
                let p = world(p);
                // Anywhere below the board picks up the head, by its nearest die.
                if !self.game.is_over() && p.y > self.layout().bottom() {
                    let head = self.game.head();
                    let nearest = (0..head.len())
                        .min_by(|&a, &b| {
                            let d =
                                |i: usize| (HEAD_SLOT + (cell(head.cells[i].0) - mid(head)) * HEAD_PITCH - p).length();
                            d(a).total_cmp(&d(b))
                        })
                        .unwrap();
                    self.turn.secs = 0.0;
                    self.hand = Hand::Dragging { grab: nearest, finger: p, since: self.time };
                }
            }
            Input::Drag(p) => {
                if let Hand::Dragging { finger, .. } = &mut self.hand {
                    *finger = world(p);
                }
            }
            Input::DragEnd(p) => {
                if let Hand::Dragging { grab, since, .. } = self.hand {
                    self.drop_head(grab, world(p), since);
                }
            }
            _ => {}
        }
    }

    fn update(&mut self, dt: f32) {
        self.time += dt;
        self.sync_rules();
        let c = &self.tuner.current;
        if let Hand::Returning { pos, vel, .. } = &mut self.hand {
            let accel = -c.snap_stiffness * (*pos - HEAD_SLOT) - c.snap_damping * *vel;
            *vel += accel * dt;
            *pos += *vel * dt;
            if (*pos - HEAD_SLOT).length() < 0.05 && vel.length() < 0.5 {
                self.hand = Hand::Idle;
            }
        }
        self.player.update(self.time, c);
    }

    fn frame(&mut self, f: &Frame, s: &mut Shapes) {
        let c = self.tuner.current.clone();
        let now = self.time;
        s.clear = rgb(BG);
        self.view = View::fit(WORLD, f.size);
        s.view = self.view;
        let layout = self.layout();
        let die = layout.die(&c);

        // Score.
        s.text(Vec2::new(0.0, -74.5), 11.0, &format!("{}", self.game.score().round()), rgb(ACCENT));
        s.text(Vec2::new(0.0, -65.0), 2.6, &format!("BEST {}", self.best.round()), rgb(DIM));

        layout.draw(s);

        // Drop preview.
        if let Hand::Dragging { grab, finger, since } = self.hand {
            let (at, ok) = self.target(grab, finger, since);
            let tint = if ok { ACCENT } else { RED };
            for (p, _) in self.game.head().cells_at(grab, at) {
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
                            glow = (1.0 - k) * (c.glow * (1.0 + 0.3 * (impact - 1.0)));
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

        // Hint and queue.
        s.text(Vec2::new(0.0, 37.0), 2.4, "DRAG TO PLACE · TAP TO ROTATE", rgb(DIM));
        let entrance = ease_out((now - self.entered) / (c.entrance_ms / 1000.0).max(1e-3));
        let enter_scale = c.entrance_scale + (1.0 - c.entrance_scale) * entrance;
        let queue = self.game.queue();
        let rest = &queue[1..];
        for (i, cluster) in rest.iter().enumerate() {
            let x = (i as f32 - (rest.len() as f32 - 1.0) * 0.5) * 24.0;
            self.draw_cluster(s, cluster, Vec2::new(x, NEXT_Y), NEXT_PITCH * enter_scale, 0.0, 0.7);
        }
        match self.hand {
            Hand::Idle if !self.game.is_over() => {
                self.draw_cluster(s, &queue[0], HEAD_SLOT, HEAD_PITCH * enter_scale, self.turn_angle(), 1.0);
            }
            Hand::Returning { pos, pitch, dist, .. } => {
                let k = ((pos - HEAD_SLOT).length() / dist).min(1.0);
                self.draw_cluster(s, &queue[0], pos, HEAD_PITCH + (pitch - HEAD_PITCH) * k, 0.0, 1.0);
            }
            Hand::Dragging { grab, finger, since } => {
                let center = self.cluster_center_from_grab(grab, self.grabbed_at(finger, since), layout.pitch);
                self.draw_cluster(s, &queue[0], center, layout.pitch, 0.0, 1.0);
            }
            _ => {}
        }

        // Game over.
        if self.game.is_over() && !self.player.busy() {
            s.rect(Vec2::ZERO, WORLD * 2.0, rgba(0x050403, 0.82));
            s.rounded_rect(Vec2::new(0.0, -10.0), Vec2::new(64.0, 40.0), 4.0, rgb(0x16140f)).outline(0.4, rgb(LINE));
            s.text(Vec2::new(0.0, -20.0), 6.5, "Game over", text_color(1.0));
            s.text(Vec2::new(0.0, -9.0), 9.0, &format!("{}", self.game.score().round()), rgb(ACCENT));
            s.text(Vec2::new(0.0, 2.5), 2.6, "TAP TO PLAY AGAIN", rgb(DIM));
        }
    }

    fn config(&mut self) -> Option<&mut dyn engine::tune::Config> {
        Some(&mut self.tuner)
    }

    fn config_actions(&mut self, ui: &mut engine::egui::Ui) {
        if ui.add(engine::egui::Button::new("New game").min_size([ui.available_width(), 30.0].into())).clicked() {
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
    let mut queue = app.game.queue().to_vec();
    queue[0] = Cluster { cells: vec![(Pos::new(0, 0), 1)] };
    app.game.set_queue(queue);
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
            app.drop_head(0, layout.center(Pos::new(2, 2)), -1.0);
            let times = [0.0, 0.2, 0.35, 0.5, 0.65, 0.9, 1.3];
            engine::headless::snapshot_strip(&mut app, 300, 510, &times, path(i).as_ref());
            println!("wrote {}", path(i));
            return;
        }
        if let Some(i) = args.iter().position(|a| a == "--snapshot-drag") {
            let mut app = scripted();
            let layout = app.layout();
            app.hand = Hand::Dragging { grab: 0, finger: layout.center(Pos::new(4, 3)), since: -1.0 };
            app.tuner.current.lift = 0.0;
            engine::headless::snapshot_strip(&mut app, 300, 510, &[0.0], path(i).as_ref());
            println!("wrote {}", path(i));
            return;
        }
    }
    engine::run(DiceApp::new(Tuner::new(), fresh_seed()));
}
