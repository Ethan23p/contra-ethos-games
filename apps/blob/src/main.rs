//! Blob Merge: the game from `crates/blob`, drawn as goo (see `look`), with
//! every force, rule and look constant live in the config panel.
//!
//! Move or drag to aim the launcher around the ring; release (or click) to
//! launch.
//!
//! Native: `cargo run -p blob-app`, or `-- --snapshot out.png` for a headless
//! strip of a seeded field taking a few launches.

mod config;
mod look;

use std::collections::HashMap;

use blob::{Event, Game, Rng};
use engine::glam::Vec2;
use engine::{Anchor, App, Frame, Input, Menu, Phase, Reply, Screen, Shapes, Tuner, View, rgb, rgba};

use config::Config;
use look::{Birth, Drawn, Drops, RED};

const BG: u32 = 0x08090c;
const LINE: u32 = 0x2c2f38;
const DIM: u32 = 0x8b8f9c;
const TEXT: u32 = 0xf2f4f8;

/// Height of the band at the top, in ems.
const BAND: f32 = 3.5;
const SAVE_KEY: &str = "blob-game";
/// How often a game in play is saved, s.
const SAVE_EVERY: f32 = 0.5;

struct BlobApp {
    tuner: Tuner<Config>,
    game: Game,
    /// Seeds each new game.
    rng: Rng,
    view: View,
    /// Launcher angle, radians from +x.
    aim: f32,
    /// Whether a press started on the game, so its release launches.
    pressed: bool,
    /// Positions before the last step, for drawing between steps.
    prev: HashMap<u32, Vec2>,
    prev_centre: Vec2,
    /// Effects in play, with when they started.
    flashes: Vec<(Flash, f32)>,
    time: f32,
    /// When the game ended, so a tap can't restart it by accident.
    ended: Option<f32>,
    /// Blobs still making their entrance, with when they were born.
    born: HashMap<u32, (Birth, f32)>,
    drops: Drops,
    /// Randomness for effects only, apart from the game's.
    fx: Rng,
    menu: Menu,
    /// Whether the game is saved as it goes; off for headless snapshots.
    persist: bool,
    saved: f32,
}

enum Flash {
    /// A merge finished: a splash, and the points it scored.
    Pop {
        at: Vec2,
        points: u64,
        combo: u32,
    },
    /// A blob below the pool shrinks away.
    Purge {
        at: Vec2,
        group: u8,
        value: u32,
    },
    Nova {
        at: Vec2,
        bonus: u64,
    },
    /// A merge's splash ring, apart from the points so it can be brief.
    Ring {
        at: Vec2,
        color: [f32; 4],
        size: f32,
    },
}

impl Flash {
    fn life(&self) -> f32 {
        match self {
            Flash::Pop { .. } => 0.8,
            Flash::Purge { .. } => 0.5,
            Flash::Nova { .. } => 1.6,
            Flash::Ring { .. } => 0.45,
        }
    }
}

impl BlobApp {
    fn new(tuner: Tuner<Config>, seed: u64) -> Self {
        let c = &tuner.current;
        let game = Game::new(&c.physics(), c.rules(), seed);
        let mut app = Self {
            tuner,
            game,
            rng: Rng::new(seed),
            view: View::PIXELS,
            aim: std::f32::consts::FRAC_PI_2,
            pressed: false,
            prev: HashMap::new(),
            prev_centre: Vec2::ZERO,
            flashes: Vec::new(),
            time: 0.0,
            ended: None,
            born: HashMap::new(),
            drops: Drops(Vec::new()),
            fx: Rng::new(seed ^ 0x5eed),
            menu: Menu::new(Anchor::TOP, Vec2::new(0.0, 1.75), TEXT, 0x16181f),
            persist: false,
            saved: 0.0,
        };
        app.reset();
        app
    }

    /// Picks up the saved game, if there's one, and saves from here on.
    fn resume(mut self) -> Self {
        self.persist = true;
        let rules = self.tuner.current.rules();
        let saved = engine::tune::storage::load(SAVE_KEY).and_then(|text| Game::from_save(rules, &text));
        if let Some(game) = saved {
            self.game = game;
            self.born.clear();
            self.prev.clear();
            self.prev_centre = self.game.field.centre;
        }
        self
    }

    fn save(&mut self) {
        self.saved = self.time;
        if self.persist {
            engine::tune::storage::save(SAVE_KEY, &self.game.to_save());
        }
    }

    fn reset(&mut self) {
        let c = &self.tuner.current;
        self.game = Game::new(&c.physics(), c.rules(), self.rng.next_u64());
        self.prev.clear();
        self.flashes.clear();
        self.drops.0.clear();
        self.ended = None;
        // The opening blobs grow in, outward from the centre.
        let centre = self.game.field.centre;
        self.born = self
            .game
            .field
            .blobs
            .iter()
            .map(|b| (b.id, (Birth::Spawn { delay: b.pos.distance(centre) / 700.0 }, self.time)))
            .collect();
        self.save();
    }

    fn launch(&mut self) {
        self.game.launch(&self.tuner.current.physics(), self.aim);
    }

    /// A release launches; once the game is over, it starts the next after a
    /// moment.
    fn release(&mut self) {
        match self.ended {
            Some(t) if self.time - t > 1.0 => self.reset(),
            Some(_) => {}
            None => self.launch(),
        }
    }

    fn aim_at(&mut self, screen: Vec2) {
        let d = self.view.to_world(screen) - self.game.field.centre;
        if d.length() > 1e-3 {
            self.aim = d.y.atan2(d.x);
        }
    }

    /// Fits the ring, and the launcher blob on it, into the screen below the
    /// band, with a little room to spare.
    fn fit(&self, screen: &Screen, centre: Vec2) -> View {
        let c = &self.tuner.current;
        let top = screen.em(BAND);
        let area = screen.size - Vec2::new(0.0, top + screen.em(0.5));
        View::fit_area(Vec2::splat(2.0 * (c.ring + c.radius * 1.4)), centre, Vec2::new(0.0, top), area)
    }
}

impl App for BlobApp {
    fn input(&mut self, input: Input, frame: &Frame) {
        match self.menu.input(input, frame.screen()) {
            Reply::Pass => {}
            Reply::Taken => {
                self.pressed = false;
                return;
            }
            Reply::NewGame => {
                self.pressed = false;
                self.reset();
                return;
            }
        }
        if let Input::Pointer(p) = input {
            match p.phase {
                Phase::Down => {
                    self.pressed = true;
                    self.aim_at(p.pos);
                }
                Phase::Move => self.aim_at(p.pos),
                Phase::Up => {
                    if std::mem::take(&mut self.pressed) {
                        self.aim_at(p.pos);
                        self.release();
                    }
                }
                Phase::Cancel => self.pressed = false,
            }
        }
    }

    fn update(&mut self, dt: f32) {
        if self.menu.open {
            // Paused: nothing left to draw between.
            self.prev.clear();
            self.prev_centre = self.game.field.centre;
            return;
        }
        self.time += dt;
        let physics = self.tuner.current.physics();
        self.game.rules = self.tuner.current.rules();
        self.prev = self.game.field.blobs.iter().map(|b| (b.id, b.pos)).collect();
        self.prev_centre = self.game.field.centre;
        self.game.step(&physics, dt);
        let c = self.tuner.current.clone();
        let r = c.radius * c.drawn;
        for e in self.game.take_events() {
            let flash = match e {
                Event::Merged { blob, count, at, .. } => {
                    self.born.insert(blob, (Birth::Pop, self.time));
                    let Some(b) = self.game.field.blob(blob) else { continue };
                    let color = look::blob_color(b.group, b.value);
                    Flash::Ring { at, color, size: r * (1.0 + 0.25 * count as f32) }
                }
                Event::Scored { points, combo, at, .. } => Flash::Pop { at, points, combo },
                Event::Purged { group, value, at, .. } => {
                    let color = look::grey(look::blob_color(group, value), 0.6);
                    self.drops.burst(&mut self.fx, at, r * 0.7, 5, (25.0, 70.0), (0.12, 0.2), color, self.time, 0.6);
                    Flash::Purge { at, group, value }
                }
                Event::Nova { at, bonus, .. } => {
                    self.drops.burst(
                        &mut self.fx,
                        at,
                        r,
                        22,
                        (260.0, 560.0),
                        (0.12, 0.26),
                        look::gold(),
                        self.time,
                        0.8,
                    );
                    // The new seed grows in, outward from the last blob.
                    for b in &self.game.field.blobs {
                        let delay = 0.15 + b.pos.distance(at) / 900.0;
                        self.born.insert(b.id, (Birth::Spawn { delay }, self.time));
                    }
                    Flash::Nova { at, bonus }
                }
                Event::Ended { .. } => {
                    self.ended = Some(self.time);
                    self.save();
                    continue;
                }
                _ => continue,
            };
            self.flashes.push((flash, self.time));
        }
        let now = self.time;
        self.flashes.retain(|(f, t)| now - t < f.life());
        self.drops.step(dt, now);
        self.born.retain(|_, (b, t)| !b.done(now - *t));
        if now - self.saved >= SAVE_EVERY && self.ended.is_none() {
            self.save();
        }
    }

    fn frame(&mut self, f: &Frame, s: &mut Shapes) {
        let c = self.tuner.current.clone();
        let a = f.alpha;
        let screen = f.screen();
        let centre = self.prev_centre.lerp(self.game.field.centre, a);
        self.view = self.fit(&screen, centre);
        s.clear = rgb(BG);
        s.view = self.view;
        let r = c.radius;

        // Ring, rim and the aim line.
        s.circle(centre, c.ring, rgba(0, 0.0)).outline(1.5, rgb(LINE));
        // The rim warms and thickens as a blob outside it runs down the grace.
        let danger = self.game.danger();
        let pulse = 0.5 + 0.5 * (self.time * (4.0 + 8.0 * danger)).sin();
        let warn = if danger > 0.0 { 0.35 + 0.65 * danger * (0.6 + 0.4 * pulse) } else { 0.25 };
        s.circle(centre, c.rim, rgba(0, 0.0)).outline(1.0 + 3.0 * danger, rgba(RED, warn));
        let dir = Vec2::from_angle(self.aim);
        let launcher = centre + dir * c.ring;
        for i in 1..9 {
            let k = i as f32 / 9.0;
            s.circle(launcher - dir * (c.ring - c.rim * 0.2) * k, 2.0, rgba(TEXT, 0.18 * (1.0 - k)));
        }

        let at = |id: u32, pos: Vec2| self.prev.get(&id).map_or(pos, |p| p.lerp(pos, a));
        let now = self.time;
        let drawn = r * c.drawn;
        // Glimmer marks the peak once it's out of the launcher's reach, so it
        // means something: at the start the peak is still in the pool.
        let peak = self.game.peak();
        let peak = if self.game.pool().last().is_some_and(|&top| peak > top) { peak } else { 0 };

        // Under the blobs: splash rings and the nova's shockwave.
        for (flash, t) in &self.flashes {
            let k = (now - t) / flash.life();
            let ease = 1.0 - (1.0 - k).powi(3);
            match *flash {
                Flash::Ring { at, color, size } => {
                    let fade = 1.0 - k;
                    let mut glow = color;
                    glow[3] = 0.35 * fade * fade;
                    s.circle(at, size * (1.0 + 0.9 * ease), rgba(0, 0.0))
                        .outline(drawn * 0.22 * fade + 0.5, glow)
                        .soft(drawn * 0.18);
                }
                Flash::Nova { at, .. } => {
                    let fade = 1.0 - k;
                    let gold = look::gold();
                    s.circle(at, drawn * (1.0 + 11.0 * ease), rgba(0, 0.0))
                        .outline(drawn * 0.3 * fade + 0.5, [gold[0], gold[1], gold[2], 0.7 * fade])
                        .soft(drawn * 0.25);
                    let k2 = ((now - t - 0.12) / 1.2).clamp(0.0, 1.0);
                    let ease2 = 1.0 - (1.0 - k2).powi(3);
                    s.circle(at, drawn * (1.0 + 7.0 * ease2), rgba(0, 0.0)).outline(1.5, rgba(TEXT, 0.5 * (1.0 - k2)));
                    // A flash where the last blob was.
                    let f = ((now - t) / 0.3).min(1.0);
                    let shine = look::shine();
                    s.circle(at, drawn * (1.0 + 1.6 * f), [shine[0], shine[1], shine[2], 0.8 * (1.0 - f)]).soft(drawn);
                }
                Flash::Purge { at, group, value } => {
                    // Deflates as it greys out.
                    let color = look::grey(look::blob_color(group, value), k);
                    let size = drawn * (1.0 - k * k).max(0.0);
                    let mut st = look::style(&c, group, value, false, drawn);
                    st.fill = [color[0], color[1], color[2], 1.0 - k * k];
                    s.goo(&[(at, size)], &[], &st);
                }
                Flash::Pop { .. } => {}
            }
        }

        let field = &self.game.field;
        let blobs: Vec<Drawn> = field
            .blobs
            .iter()
            .filter_map(|b| {
                let scale = self.born.get(&b.id).map_or(1.0, |&(birth, t)| birth.scale(now - t, c.wobble));
                (scale > 0.01).then(|| Drawn {
                    id: b.id,
                    group: b.group,
                    value: b.value,
                    pos: at(b.id, b.pos),
                    r: drawn * scale,
                    merging: b.merging,
                })
            })
            .collect();
        let bonds: &[(u32, u32)] = if c.show_bonds { &field.bonds } else { &[] };
        look::draw_field(s, &c, &blobs, bonds, peak, now);
        self.drops.draw(s, now);

        // The overflow warning: which blobs, how long, and how close overall.
        let mut urgent = None::<(f32, f32)>;
        if self.ended.is_none() {
            for &(id, t) in &self.game.outside {
                let Some(k) = look::warning(&c, t) else { continue };
                if let Some(b) = blobs.iter().find(|b| b.id == id) {
                    look::warning_ring(s, &c, b.pos, b.r, t);
                }
                if urgent.is_none_or(|(most, _)| k > most) {
                    urgent = Some((k, c.grace - t));
                }
            }
        }
        if let Some((_, left)) = urgent {
            look::countdown(s, &c, centre, left);
        }

        if self.ended.is_none() {
            let (g, v) = self.game.queue[0];
            look::draw_one(s, &c, launcher, drawn, g, v, v == peak, now);
        }

        // Points rise from where they were scored.
        for (flash, t) in &self.flashes {
            let k = (now - t) / flash.life();
            let (at, text, size) = match *flash {
                Flash::Pop { at, points, combo } if combo > 1 => (at, format!("+{points} ×{combo}"), r * 0.8),
                Flash::Pop { at, points, .. } => (at, format!("+{points}"), r * 0.8),
                Flash::Nova { at, bonus } => (at, format!("NOVA +{bonus}"), r * 1.3),
                _ => continue,
            };
            let ease = 1.0 - (1.0 - k).powi(3);
            s.text(at - Vec2::new(0.0, r * (1.2 + 1.5 * ease)), size, &text, rgba(TEXT, 1.0 - k * k));
        }

        // The band: a plain readout on the left, the next blob on the right.
        s.view = View::PIXELS;
        if let Some((k, _)) = urgent.filter(|_| c.warn_edge > 0.0) {
            let alpha = c.warn_edge * (0.3 + 0.7 * k) * (0.7 + 0.3 * pulse);
            look::edge_glow(s, screen.size, screen.em(c.warn_edge_width), alpha);
        }
        let left = |s: &Shapes, size: f32, text: &str, y: f32| {
            screen.at(Anchor::TOP_LEFT, Vec2::new(screen.em(1.0) + s.text_width(size, text) * 0.5, screen.em(y)))
        };
        let g = &self.game;
        let line = if g.combo > 1 { format!("{}  ×{}", g.score, g.combo) } else { g.score.to_string() };
        let size = screen.em(1.4);
        s.text(left(s, size, &line, 1.4), size, &line, rgb(TEXT));
        let pool: Vec<String> = g.pool().iter().map(u32::to_string).collect();
        let hint = format!("POOL {}", pool.join(" · "));
        let size = screen.em(0.7);
        s.text(left(s, size, &hint, 2.75), size, &hint, rgb(DIM));
        if let Some(&(g, v)) = g.queue.get(1) {
            let next = screen.at(Anchor::TOP_RIGHT, Vec2::new(-screen.em(2.0), screen.em(1.75)));
            look::draw_one(s, &c, next, screen.em(1.0), g, v, v == peak, now);
            let size = screen.em(0.8);
            s.text(next - Vec2::new(screen.em(1.6) + s.text_width(size, "NEXT") * 0.5, 0.0), size, "NEXT", rgb(DIM));
        }

        if let Some(t) = self.ended {
            let k = ((self.time - t) / 0.4).min(1.0);
            s.rounded_rect(screen.size * 0.5, screen.size, 0.0, rgba(BG, 0.7 * k));
            let mid = screen.size * 0.5;
            s.text(mid - Vec2::new(0.0, screen.em(2.2)), screen.em(1.0), "OVERFLOW", rgba(RED, k));
            s.text(mid, screen.em(2.6), &self.game.score.to_string(), rgba(TEXT, k));
            if self.time - t > 1.0 {
                s.text(mid + Vec2::new(0.0, screen.em(2.4)), screen.em(0.8), "TAP TO PLAY AGAIN", rgb(DIM));
            }
        }
        self.menu.draw(s, screen);
        s.view = self.view;
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
            self.reset();
        }
    }
}

fn fresh_seed() -> u64 {
    web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64)
}

fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let args: Vec<String> = std::env::args().collect();
        if let Some(i) = args.iter().position(|a| a == "--snapshot-goo") {
            // Close up: a bonded trio merging, a tethered pair, and the peak.
            let path = args.get(i + 1).map_or("goo.png", String::as_str).to_string();
            let mut app = BlobApp::new(Tuner::unsaved(), 7);
            let c = &mut app.tuner.current;
            (c.mode, c.ring, c.rim) = (0, 120.0, 90.0);
            app.reset();
            let f = &mut app.game.field;
            *f = blob::Field::new();
            for (g, v, x, y) in [
                (0, 8, -30.0, -20.0),
                (0, 8, 22.0, -20.0),
                (0, 8, -4.0, 26.0),
                (1, 4, 30.0, 62.0),
                (1, 4, 82.0, 50.0),
                (2, 32, -62.0, 52.0),
                (2, 4, -20.0, 80.0),
                (1, 16, 70.0, -40.0),
            ] {
                f.add(g, v, Vec2::new(x, y));
            }
            app.born.clear();
            let times = [0.02, 0.15, 0.32, 0.6, 1.1];
            engine::headless::snapshot_strip(&mut app, 520, 600, &times, path.as_ref());
            println!("wrote {path}");
            return;
        }
        if let Some(i) = args.iter().position(|a| a == "--snapshot-warn") {
            // No well, so two strays sit outside the rim and run down the grace.
            let path = args.get(i + 1).map_or("warn.png", String::as_str).to_string();
            let mut app = BlobApp::new(Tuner::unsaved(), 7);
            let c = &mut app.tuner.current;
            (c.well, c.centre_follow) = (0.0, 0.0);
            app.reset();
            let f = &mut app.game.field;
            *f = blob::Field::new();
            for (g, v, x, y) in [
                (0, 8, -40.0, -30.0),
                (1, 4, 24.0, -36.0),
                (2, 16, -10.0, 34.0),
                (1, 2, 62.0, 30.0),
                (0, 32, 150.0, -60.0),
                (2, 4, -120.0, 110.0),
            ] {
                f.add(g, v, Vec2::new(x, y));
            }
            app.born.clear();
            app.prev_centre = app.game.field.centre;
            let times = [0.5, 1.2, 3.0, 4.0, 5.2, 6.2];
            engine::headless::snapshot_strip(&mut app, 375, 640, &times, path.as_ref());
            println!("wrote {path}");
            return;
        }
        if let Some(i) = args.iter().position(|a| a == "--snapshot") {
            let path = args.get(i + 1).map_or("snapshot.png", String::as_str).to_string();
            let mut app = BlobApp::new(Tuner::unsaved(), 7);
            for (k, aim) in [0.3f32, 2.4, 4.2, 5.5].into_iter().enumerate() {
                for _ in 0..(30 + 20 * k) {
                    app.update(1.0 / 120.0);
                }
                app.aim = aim;
                app.launch();
            }
            // The last launch, from the left, in flight.
            app.aim = std::f32::consts::PI;
            app.launch();
            let times = [0.0, 0.08, 0.16, 0.3, 0.6, 1.5];
            engine::headless::snapshot_strip(&mut app, 300, 510, &times, path.as_ref());
            println!("wrote {path}");
            return;
        }
    }
    engine::run(BlobApp::new(Tuner::new(), fresh_seed()).resume());
}
