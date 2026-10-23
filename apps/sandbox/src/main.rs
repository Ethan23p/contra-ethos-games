//! Engine playground. Phase 1: every engine primitive in one scene, so each can
//! be checked by eye on desktop, on a phone and headless.
//!
//! - A row of dice (rounded rects, outlines, SDF numbers) and a row of blobs.
//! - A ball on a fixed-step simulation, drawn interpolated.
//! - Tap for a ripple, drag the tile, hold for a glow.
//! - The dev panel (tap the dots top right, or F1) tunes all of it live.
//!
//! Native: `cargo run -p sandbox`, or `cargo run -p sandbox -- --snapshot out.png`
//! for a headless strip of frames.

use engine::glam::Vec2;
use engine::{App, Frame, Input, Schema, Shapes, Tunable, Tuner, View, rgb, rgba};

/// The world is 100 units wide and 160 high, centred on the origin.
const WORLD: Vec2 = Vec2::new(100.0, 160.0);

#[derive(Clone)]
struct Config {
    gravity: f32,
    bounce: f32,
    corner: f32,
    outline: f32,
    glow: f32,
    text_size: f32,
    ripple_time: f32,
    palette: usize,
    interpolate: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            gravity: 420.0,
            bounce: 0.82,
            corner: 3.0,
            outline: 0.9,
            glow: 6.0,
            text_size: 9.0,
            ripple_time: 0.5,
            palette: 0,
            interpolate: true,
        }
    }
}

impl Tunable for Config {
    const KEY: &'static str = "sandbox";

    fn schema(s: &mut Schema<Self>) {
        s.group("Ball")
            .float("gravity", "Gravity", "u/s²", 0.0..=1000.0, 5.0, |c| &mut c.gravity)
            .float("bounce", "Bounce", "×", 0.0..=1.0, 0.01, |c| &mut c.bounce)
            .toggle("interpolate", "Interpolate between steps", |c| &mut c.interpolate);
        s.group("Look")
            .float("corner", "Die corner radius", "u", 0.0..=6.0, 0.1, |c| &mut c.corner)
            .float("outline", "Outline width", "u", 0.0..=3.0, 0.05, |c| &mut c.outline)
            .float("glow", "Glow width", "u", 0.0..=20.0, 0.5, |c| &mut c.glow)
            .float("text_size", "Number size", "u", 3.0..=20.0, 0.25, |c| &mut c.text_size)
            .choice("palette", "Palette", &["Warm", "Cool"], |c| &mut c.palette);
        s.group("Input").float("ripple_time", "Ripple lifetime", "s", 0.1..=2.0, 0.05, |c| &mut c.ripple_time);
    }

    fn presets() -> Vec<(&'static str, Self)> {
        vec![("Floaty", Self { gravity: 60.0, bounce: 0.95, ..Self::default() })]
    }
}

#[derive(Default)]
struct Ball {
    pos: Vec2,
    prev: Vec2,
    vel: Vec2,
}

struct Sandbox {
    tuner: Tuner<Config>,
    view: View,
    ball: Ball,
    ripples: Vec<(Vec2, f32)>,
    tile: Vec2,
    dragging: Option<Vec2>,
    holding: Option<(Vec2, f32)>,
    taps: u32,
    time: f32,
}

impl Sandbox {
    fn new(tuner: Tuner<Config>) -> Self {
        let start = Vec2::new(-30.0, -40.0);
        Self {
            tuner,
            view: View::PIXELS,
            ball: Ball { pos: start, prev: start, vel: Vec2::new(38.0, 0.0) },
            ripples: Vec::new(),
            tile: Vec2::new(0.0, 55.0),
            dragging: None,
            holding: None,
            taps: 0,
            time: 0.0,
        }
    }
}

impl App for Sandbox {
    fn input(&mut self, input: Input, _: &Frame) {
        let world = |p: Vec2| self.view.to_world(p);
        match input {
            Input::Tap(p) => {
                self.ripples.push((world(p), self.time));
                self.taps += 1;
            }
            Input::Hold(p) => self.holding = Some((world(p), self.time)),
            Input::DragStart(p) => {
                let p = world(p);
                if (p - self.tile).abs().max_element() < 12.0 {
                    self.dragging = Some(self.tile - p);
                }
            }
            Input::Drag(p) => {
                if let Some(grab) = self.dragging {
                    self.tile = world(p) + grab;
                }
            }
            Input::DragEnd(_) => self.dragging = None,
            Input::Pointer(p) if p.phase != engine::Phase::Down && p.phase != engine::Phase::Move => {
                self.holding = None;
            }
            _ => {}
        }
    }

    fn update(&mut self, dt: f32) {
        let c = &self.tuner.current;
        self.time += dt;
        let b = &mut self.ball;
        b.prev = b.pos;
        b.vel.y += c.gravity * dt;
        b.pos += b.vel * dt;
        let r = 6.0;
        let floor = WORLD.y * 0.5 - 30.0;
        let wall = WORLD.x * 0.5 - r;
        if b.pos.y > floor - r {
            b.pos.y = floor - r;
            b.vel.y = -b.vel.y.abs() * c.bounce;
        }
        if b.pos.x.abs() > wall {
            b.pos.x = b.pos.x.clamp(-wall, wall);
            b.vel.x = -b.vel.x;
        }
        let life = c.ripple_time;
        let now = self.time;
        self.ripples.retain(|&(_, t)| now - t < life);
    }

    fn frame(&mut self, f: &Frame, s: &mut Shapes) {
        let c = self.tuner.current.clone();
        s.clear = rgb(0x14121a);
        self.view = View::fit(WORLD, f.size);
        s.view = self.view;

        // The world's bounds, faintly.
        s.rect(Vec2::ZERO, WORLD, rgba(0xffffff, 0.03)).corner(4.0);

        let palette: [u32; 6] = match c.palette {
            0 => [0xe8c547, 0xe0884f, 0xd96c75, 0xb06ec9, 0x6f8fd2, 0x6fc2b0],
            _ => [0x6fb3d2, 0x6f8fd2, 0x8c7ed9, 0x6fc2b0, 0x8fcf7a, 0xcfe0e8],
        };

        // Dice: value 1 to 6, the last one glowing.
        let die = 13.0;
        for (i, hex) in palette.iter().enumerate() {
            let center = Vec2::new((i as f32 - 2.5) * (die + 2.5), -62.0);
            if i == 5 {
                s.rounded_rect(center, Vec2::splat(die), c.corner, rgba(*hex, 0.6)).soft(c.glow);
            }
            s.rounded_rect(center, Vec2::splat(die), c.corner, rgb(*hex)).outline(c.outline, rgba(0x000000, 0.35));
            s.text(center, c.text_size, &(i + 1).to_string(), rgb(0x1b1720));
        }

        // Blobs: value 1, 2, 4, … with outlined numbers, the largest slowly turning.
        let radii = [4.5, 6.0, 7.5, 9.0, 10.5];
        let mut x = -(radii.iter().sum::<f32>() * 2.0 + 4.0 * 2.5) * 0.5;
        for (i, radius) in radii.into_iter().enumerate() {
            let value = 1u32 << (i * 2);
            let center = Vec2::new(x + radius, -38.0);
            x += radius * 2.0 + 2.5;
            s.circle(center, radius, rgb(palette[i])).outline(c.outline, rgba(0xffffff, 0.5));
            let text = s.text(center, c.text_size * (0.7 + i as f32 * 0.1), &value.to_string(), rgb(0xffffff));
            let text = text.outline(0.5, rgba(0x000000, 0.45));
            if i == 4 {
                text.rotate((f.time * 0.8).sin() * 0.4);
            }
        }

        // The fixed-step ball, interpolated between its last two steps.
        let b = &self.ball;
        let pos = if c.interpolate { b.prev.lerp(b.pos, f.alpha) } else { b.pos };
        let floor = WORLD.y * 0.5 - 30.0;
        s.rect(Vec2::new(0.0, floor + 0.5), Vec2::new(WORLD.x, 1.0), rgba(0xffffff, 0.15));
        s.circle(pos, 6.0, rgb(0xe8c547)).outline(c.outline, rgba(0x000000, 0.3));

        // Input: ripples, the hold glow and the draggable tile.
        for &(p, t) in &self.ripples {
            let k = ((self.time - t) / c.ripple_time).clamp(0.0, 1.0);
            s.circle(p, 3.0 + 14.0 * k, rgba(0xffffff, 0.0)).outline(0.8, rgba(0xffffff, 1.0 - k));
        }
        if let Some((p, t)) = self.holding {
            let k = ((self.time - t) * 3.0).min(1.0);
            s.circle(p, 6.0 + 4.0 * k, rgba(0xe8c547, 0.5 * k)).soft(c.glow + 4.0);
        }
        let lifted = self.dragging.is_some();
        if lifted {
            s.rounded_rect(self.tile + Vec2::new(0.0, 2.0), Vec2::splat(20.0), c.corner, rgba(0, 0.4)).soft(4.0);
        }
        let tile = s.rounded_rect(self.tile, Vec2::splat(20.0), c.corner, rgb(0x2d2838));
        tile.outline(c.outline, rgb(if lifted { 0xe8c547 } else { 0x4a4458 })).scale(if lifted { 1.08 } else { 1.0 });
        s.text(self.tile, c.text_size * 0.55, "drag", rgba(0xffffff, 0.6));

        // Tabular digits keep the counter steady as it climbs.
        s.text(Vec2::new(0.0, -74.0 - 1.0), 6.0, &format!("taps {}", self.taps), rgba(0xffffff, 0.5));
    }

    fn dev_ui(&mut self, ui: &mut engine::egui::Ui) {
        ui.heading("Sandbox");
        self.tuner.ui(ui);
    }
}

fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let args: Vec<String> = std::env::args().collect();
        if let Some(i) = args.iter().position(|a| a == "--snapshot") {
            let path = args.get(i + 1).map_or("snapshot.png", String::as_str);
            let mut app = Sandbox::new(Tuner::unsaved());
            engine::headless::snapshot_strip(&mut app, 300, 480, &[0.0, 0.4, 0.8, 1.6], path.as_ref());
            println!("wrote {path}");
            return;
        }
    }
    engine::run(Sandbox::new(Tuner::new()));
}
