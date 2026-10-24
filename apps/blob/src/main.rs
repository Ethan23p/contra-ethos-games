//! Blob Merge, so far as a physics sandbox: the field from `crates/blob` drawn
//! as plain circles, with every force live in the config panel.
//!
//! Move or drag to aim the launcher around the ring; release (or click) to
//! launch. The launcher draws from 1–8 for now; the spec's pool comes with the
//! rules.
//!
//! Native: `cargo run -p blob-app`, or `-- --snapshot out.png` for a headless
//! strip of a seeded field taking a few launches.

mod config;

use std::collections::HashMap;

use blob::{Event, Field, GROUPS, Rng};
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
    field: Field,
    rng: Rng,
    view: View,
    /// Launcher angle, radians from +x.
    aim: f32,
    /// Whether a press started on the game, so its release launches.
    pressed: bool,
    queue: [(u8, u32); 2],
    /// Positions before the last step, for drawing between steps.
    prev: HashMap<u32, Vec2>,
    prev_centre: Vec2,
    /// Merges that just finished: where, what colour, when.
    pops: Vec<(Vec2, u8, f32)>,
    time: f32,
}

impl BlobApp {
    fn new(tuner: Tuner<Config>, seed: u64) -> Self {
        let mut app = Self {
            tuner,
            field: Field::new(),
            rng: Rng::new(seed),
            view: View::PIXELS,
            aim: std::f32::consts::FRAC_PI_2,
            pressed: false,
            queue: [(0, 1); 2],
            prev: HashMap::new(),
            prev_centre: Vec2::ZERO,
            pops: Vec::new(),
            time: 0.0,
        };
        app.reset();
        app
    }

    fn reset(&mut self) {
        self.field = Field::new();
        self.field.seed(&self.tuner.current.physics(), &mut self.rng);
        self.queue = [self.draw(), self.draw()];
        self.prev.clear();
        self.pops.clear();
    }

    /// A stand-in draw until the spec's pool: any group, values 1–8 weighted
    /// 1 / 1.25^i toward the low end.
    fn draw(&mut self) -> (u8, u32) {
        let group = self.rng.below(GROUPS as usize) as u8;
        let weights: Vec<f64> = (0..4).map(|i| 1.25f64.powi(-i)).collect();
        (group, 1 << self.rng.weighted(&weights))
    }

    fn launch(&mut self) {
        let (group, value) = self.queue[0];
        self.field.launch(&self.tuner.current.physics(), self.aim, group, value);
        self.queue = [self.queue[1], self.draw()];
    }

    fn aim_at(&mut self, screen: Vec2) {
        let d = self.view.to_world(screen) - self.field.centre;
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
                        self.launch();
                    }
                }
                Phase::Cancel => self.pressed = false,
            }
        }
    }

    fn update(&mut self, dt: f32) {
        self.time += dt;
        let physics = self.tuner.current.physics();
        self.prev = self.field.blobs.iter().map(|b| (b.id, b.pos)).collect();
        self.prev_centre = self.field.centre;
        self.field.step(&physics, dt);
        for e in self.field.take_events() {
            if let Event::Merged { blob, at, .. } = e {
                let group = self.field.blob(blob).map_or(0, |b| b.group);
                self.pops.push((at, group, self.time));
            }
        }
        let now = self.time;
        self.pops.retain(|p| now - p.2 < 0.6);
    }

    fn frame(&mut self, f: &Frame, s: &mut Shapes) {
        let c = self.tuner.current.clone();
        let a = f.alpha;
        let screen = f.screen();
        let centre = self.prev_centre.lerp(self.field.centre, a);
        self.view = self.fit(&screen, centre);
        s.clear = rgb(BG);
        s.view = self.view;
        let r = c.radius;

        // Ring, rim and the aim line.
        s.circle(centre, c.ring, rgba(0, 0.0)).outline(1.5, rgb(LINE));
        s.circle(centre, c.rim, rgba(0, 0.0)).outline(1.0, rgba(RED, 0.25));
        let dir = Vec2::from_angle(self.aim);
        let launcher = centre + dir * c.ring;
        for i in 1..9 {
            let k = i as f32 / 9.0;
            s.circle(launcher - dir * (c.ring - c.rim * 0.2) * k, 2.0, rgba(TEXT, 0.18 * (1.0 - k)));
        }

        let at = |id: u32, pos: Vec2| self.prev.get(&id).map_or(pos, |p| p.lerp(pos, a));

        if c.show_bonds {
            for &(x, y) in &self.field.bonds {
                let (Some(bx), Some(by)) = (self.field.blob(x), self.field.blob(y)) else { continue };
                let (p, q) = (at(bx.id, bx.pos), at(by.id, by.pos));
                let d = q - p;
                let color = blob_color(bx.group, bx.value);
                s.rounded_rect((p + q) * 0.5, Vec2::new(d.length(), r * 0.6), r * 0.3, color).rotate(d.y.atan2(d.x));
            }
        }

        for (pos, group, t) in &self.pops {
            let k = (self.time - t) / 0.6;
            let mut color = rgb(GROUP[*group as usize % GROUP.len()]);
            color[3] = 0.6 * (1.0 - k);
            s.circle(*pos, r * (1.0 + 1.5 * k), rgba(0, 0.0)).outline(2.0, color);
        }

        for b in &self.field.blobs {
            let alpha = if b.merging.is_some() { 0.85 } else { 1.0 };
            draw_blob(s, at(b.id, b.pos), r, b.group, b.value, alpha);
        }

        let (g, v) = self.queue[0];
        draw_blob(s, launcher, r, g, v, 1.0);

        // The band: a plain readout on the left, the next blob on the right.
        s.view = View::PIXELS;
        let left = |s: &Shapes, size: f32, text: &str, y: f32| {
            screen.at(Anchor::TOP_LEFT, Vec2::new(screen.em(1.0) + s.text_width(size, text) * 0.5, screen.em(y)))
        };
        let line = format!("{} BLOBS · {} BONDS", self.field.blobs.len(), self.field.bonds.len());
        let size = screen.em(1.1);
        s.text(left(s, size, &line, 1.4), size, &line, rgb(TEXT));
        let hint = "AIM AND RELEASE TO LAUNCH";
        let size = screen.em(0.7);
        s.text(left(s, size, hint, 2.6), size, hint, rgb(DIM));
        let (g, v) = self.queue[1];
        let next = screen.at(Anchor::TOP_RIGHT, Vec2::new(-screen.em(2.0), screen.em(1.75)));
        draw_blob(s, next, screen.em(1.0) / DRAWN, g, v, 1.0);
        let size = screen.em(0.8);
        s.text(next - Vec2::new(screen.em(1.6) + s.text_width(size, "NEXT") * 0.5, 0.0), size, "NEXT", rgb(DIM));
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
            .add(engine::egui::Button::new("New field").min_size([ui.available_width(), 1.875 * engine::EM].into()))
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
