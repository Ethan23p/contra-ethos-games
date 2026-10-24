//! Blob Merge: the game from `crates/blob` drawn as plain circles, with every
//! force and rule live in the config panel. Effects are placeholders until the
//! gloopy look.
//!
//! Move or drag to aim the launcher around the ring; release (or click) to
//! launch.
//!
//! Native: `cargo run -p blob-app`, or `-- --snapshot out.png` for a headless
//! strip of a seeded field taking a few launches.

mod config;

use std::collections::HashMap;

use blob::{Event, Game, Rng};
use engine::glam::Vec2;
use engine::{Anchor, App, Frame, Input, Phase, Screen, Shapes, Tuner, View, rgb, rgba};

use config::Config;

const BG: u32 = 0x08090c;
const LINE: u32 = 0x2c2f38;
const DIM: u32 = 0x8b8f9c;
const TEXT: u32 = 0xf2f4f8;
const RED: u32 = 0xd9564f;
/// The old build's group colours: azure, emerald, crimson.
const GROUP: [u32; 3] = [0x38bdf8, 0x34d399, 0xfb7185];

/// Height of the band at the top, in ems.
const BAND: f32 = 3.5;

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
}

enum Flash {
    /// A merge finished: a ring, and the points it scored.
    Pop {
        at: Vec2,
        group: u8,
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
}

impl Flash {
    fn life(&self) -> f32 {
        match self {
            Flash::Pop { .. } => 0.8,
            Flash::Purge { .. } => 0.35,
            Flash::Nova { .. } => 1.6,
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
        };
        app.reset();
        app
    }

    fn reset(&mut self) {
        let c = &self.tuner.current;
        self.game = Game::new(&c.physics(), c.rules(), self.rng.next_u64());
        self.prev.clear();
        self.flashes.clear();
        self.ended = None;
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

fn blob_color(group: u8, value: u32) -> [f32; 4] {
    // Higher values lighten toward white, a step per doubling.
    let k = (value.max(1).ilog2() as f32 * 0.07).min(0.6);
    let base = rgb(GROUP[group as usize % GROUP.len()]);
    [base[0] + (1.0 - base[0]) * k, base[1] + (1.0 - base[1]) * k, base[2] + (1.0 - base[2]) * k, 1.0]
}

/// Blobs draw a little inside their physical radius, so touching ones show a
/// gap and bonds show as bridges across it.
const DRAWN: f32 = 0.9;

fn draw_blob(s: &mut Shapes, pos: Vec2, r: f32, group: u8, value: u32, alpha: f32) {
    let r = r * DRAWN;
    let mut color = blob_color(group, value);
    color[3] = alpha;
    s.circle(pos, r, color).outline(r * 0.08, rgba(0, 0.25 * alpha));
    let text = value.to_string();
    let size = r * if text.len() >= 3 { 0.62 } else { 0.85 };
    s.text(pos, size, &text, rgba(0x101218, 0.85 * alpha));
}

impl App for BlobApp {
    fn input(&mut self, input: Input, _: &Frame) {
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
        self.time += dt;
        let physics = self.tuner.current.physics();
        self.game.rules = self.tuner.current.rules();
        self.prev = self.game.field.blobs.iter().map(|b| (b.id, b.pos)).collect();
        self.prev_centre = self.game.field.centre;
        self.game.step(&physics, dt);
        for e in self.game.take_events() {
            let flash = match e {
                Event::Scored { blob, points, combo, at } => {
                    let group = self.game.field.blob(blob).map_or(0, |b| b.group);
                    Flash::Pop { at, group, points, combo }
                }
                Event::Purged { group, value, at, .. } => Flash::Purge { at, group, value },
                Event::Nova { at, bonus, .. } => Flash::Nova { at, bonus },
                Event::Ended { .. } => {
                    self.ended = Some(self.time);
                    continue;
                }
                _ => continue,
            };
            self.flashes.push((flash, self.time));
        }
        let now = self.time;
        self.flashes.retain(|(f, t)| now - t < f.life());
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

        let field = &self.game.field;
        if c.show_bonds {
            for &(x, y) in &field.bonds {
                let (Some(bx), Some(by)) = (field.blob(x), field.blob(y)) else { continue };
                let (p, q) = (at(bx.id, bx.pos), at(by.id, by.pos));
                let d = q - p;
                let color = blob_color(bx.group, bx.value);
                s.rounded_rect((p + q) * 0.5, Vec2::new(d.length(), r * 0.6), r * 0.3, color).rotate(d.y.atan2(d.x));
            }
        }

        for (flash, t) in &self.flashes {
            let k = (self.time - t) / flash.life();
            match *flash {
                Flash::Pop { at, group, .. } => {
                    let mut color = rgb(GROUP[group as usize % GROUP.len()]);
                    color[3] = 0.6 * (1.0 - k);
                    s.circle(at, r * (1.0 + 1.5 * k), rgba(0, 0.0)).outline(2.0, color);
                }
                Flash::Purge { at, group, value } => {
                    draw_blob(s, at, r * (1.0 - k).max(0.01), group, value, 1.0 - k);
                }
                Flash::Nova { at, .. } => {
                    let fade = 1.0 - k;
                    s.circle(at, r * (1.0 + 10.0 * k), rgba(0, 0.0)).outline(3.0 * fade + 0.5, rgba(TEXT, 0.8 * fade));
                }
            }
        }

        for b in &field.blobs {
            let alpha = if b.merging.is_some() { 0.85 } else { 1.0 };
            draw_blob(s, at(b.id, b.pos), r, b.group, b.value, alpha);
        }

        if self.ended.is_none() {
            let (g, v) = self.game.queue[0];
            draw_blob(s, launcher, r, g, v, 1.0);
        }

        // Points rise from where they were scored.
        for (flash, t) in &self.flashes {
            let k = (self.time - t) / flash.life();
            let (at, text, size) = match *flash {
                Flash::Pop { at, points, combo, .. } if combo > 1 => (at, format!("+{points} ×{combo}"), r * 0.8),
                Flash::Pop { at, points, .. } => (at, format!("+{points}"), r * 0.8),
                Flash::Nova { at, bonus } => (at, format!("NOVA +{bonus}"), r * 1.3),
                Flash::Purge { .. } => continue,
            };
            let ease = 1.0 - (1.0 - k).powi(3);
            s.text(at - Vec2::new(0.0, r * (1.2 + 1.5 * ease)), size, &text, rgba(TEXT, 1.0 - k * k));
        }

        // The band: a plain readout on the left, the next blob on the right.
        s.view = View::PIXELS;
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
            draw_blob(s, next, screen.em(1.0) / DRAWN, g, v, 1.0);
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
    engine::run(BlobApp::new(Tuner::new(), fresh_seed()));
}
