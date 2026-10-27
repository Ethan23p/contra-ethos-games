//! Colours, the board's geometry, and how a die is drawn.

use dice::Pos;
use engine::glam::Vec2;
use engine::{Anchor, Screen, Shapes, View, rgb, rgba};

use crate::config::Config;

pub const BG: u32 = 0x0a0908;
pub const LINE: u32 = 0x3c3a37;
pub const CELL: u32 = 0x0f0e0d;
pub const DIM: u32 = 0x948d81;
pub const TEXT: u32 = 0xf4f1ec;
pub const ACCENT: u32 = 0x5b8ef2;
pub const RED: u32 = 0xd9564f;

/// The board in world units. The screen fits the board as large as it can
/// go, and the staging area takes the space under it (see `Hud`).
const BOARD_SIDE: f32 = 90.0;
const BOARD_CENTER: Vec2 = Vec2::new(0.0, -15.0);
const BOARD_PAD: f32 = 1.5;
/// The gap between cells, in world units.
pub const GAP: f32 = 0.75;
/// A cluster spans at most this many cells, in any rotation.
const CLUSTER_SPAN: f32 = 3.0;

pub fn die_color(value: u32) -> [f32; 4] {
    const BASE: [u32; 6] = [0xcac4b8, 0x5b8ef2, 0x57b56a, 0xe8a23c, 0xd9564f, 0x9c6fd6];
    match value {
        1..=6 => rgb(BASE[value as usize - 1]),
        _ => hsl(((value - 1) * 47 % 360) as f32, 0.55, 0.52),
    }
}

fn hsl(h: f32, s: f32, l: f32) -> [f32; 4] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c * 0.5;
    [r + m, g + m, b + m, 1.0]
}

pub fn with_alpha(mut color: [f32; 4], alpha: f32) -> [f32; 4] {
    color[3] *= alpha;
    color
}

/// Traditional pip layouts, on a grid from −1 to 1.
fn pips(value: u32) -> &'static [(f32, f32)] {
    match value {
        1 => &[(0.0, 0.0)],
        2 => &[(-1.0, -1.0), (1.0, 1.0)],
        3 => &[(-1.0, -1.0), (0.0, 0.0), (1.0, 1.0)],
        4 => &[(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)],
        5 => &[(-1.0, -1.0), (1.0, -1.0), (0.0, 0.0), (-1.0, 1.0), (1.0, 1.0)],
        6 => &[(-1.0, -1.0), (-1.0, 0.0), (-1.0, 1.0), (1.0, -1.0), (1.0, 0.0), (1.0, 1.0)],
        _ => &[],
    }
}

/// A die of `size` (its side, world units): pips up to 6, digits from 7.
pub fn draw_die(s: &mut Shapes, c: &Config, center: Vec2, size: f32, value: u32, angle: f32, alpha: f32) {
    let corner = size * c.die_corner;
    s.rounded_rect(center + Vec2::new(0.0, size * 0.08), Vec2::splat(size), corner, rgba(0, 0.35 * alpha))
        .soft(size * 0.15)
        .rotate(angle);
    s.rounded_rect(center, Vec2::splat(size), corner, with_alpha(die_color(value), alpha))
        .outline(size * 0.045, rgba(0, 0.18 * alpha))
        .rotate(angle);
    let ink = rgba(0, 0.68 * alpha);
    if value <= 6 {
        let r = Vec2::from_angle(angle);
        for &(x, y) in pips(value) {
            s.circle(center + r.rotate(Vec2::new(x, y) * size * 0.25), size * c.pip_size * 0.5, ink);
        }
    } else {
        s.text(center, size * 0.42, &value.to_string(), rgba(0, 0.72 * alpha)).rotate(angle);
    }
}

/// The board's geometry for an N × N grid. Cell coordinates are fractional:
/// integer coordinates are cell centres.
#[derive(Clone, Copy)]
pub struct Layout {
    pub n: usize,
    pub pitch: f32,
    origin: Vec2,
}

impl Layout {
    pub fn new(n: usize) -> Self {
        let pitch = (BOARD_SIDE - 2.0 * BOARD_PAD) / n as f32;
        Self { n, pitch, origin: BOARD_CENTER - Vec2::splat(BOARD_SIDE * 0.5 - BOARD_PAD) }
    }

    pub fn at(&self, cell: Vec2) -> Vec2 {
        self.origin + (cell + 0.5) * self.pitch
    }

    pub fn center(&self, p: Pos) -> Vec2 {
        self.at(cell(p))
    }

    /// World point to fractional cell coordinates.
    pub fn cell_of(&self, world: Vec2) -> Vec2 {
        (world - self.origin) / self.pitch - 0.5
    }

    pub fn cell_at(&self, world: Vec2) -> Pos {
        let c = self.cell_of(world).round();
        Pos::new(c.x as i32, c.y as i32)
    }

    pub fn die(&self, c: &Config) -> f32 {
        (self.pitch - GAP) * c.die_size
    }

    pub fn draw(&self, s: &mut Shapes) {
        s.rounded_rect(BOARD_CENTER + Vec2::new(0.0, 3.0), Vec2::splat(BOARD_SIDE), 2.5, rgba(0, 0.5)).soft(8.0);
        s.rounded_rect(BOARD_CENTER, Vec2::splat(BOARD_SIDE), 2.5, rgb(LINE));
        for y in 0..self.n as i32 {
            for x in 0..self.n as i32 {
                self.cell_rect(s, Pos::new(x, y), Vec2::ZERO, rgb(CELL));
            }
        }
    }

    pub fn cell_rect<'a>(&self, s: &'a mut Shapes, p: Pos, nudge: Vec2, color: [f32; 4]) -> engine::Styled<'a> {
        s.rounded_rect(self.center(p) + nudge, Vec2::splat(self.pitch - GAP), 1.0, color)
    }
}

pub fn cell(p: Pos) -> Vec2 {
    Vec2::new(p.x as f32, p.y as f32)
}

pub fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

pub fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Text colour helper for the HUD.
pub fn text_color(alpha: f32) -> [f32; 4] {
    rgba(TEXT, alpha)
}

/// Where the HUD goes: the score band anchored to the top in ems, the board
/// fitted as large as it can go below it, and the staging area under that.
pub struct Hud {
    pub screen: Screen,
    pub view: View,
    pub staging: Staging,
}

/// The staging area's slots, in world units.
#[derive(Clone)]
pub struct Staging {
    pub centers: Vec<Vec2>,
    /// Each slot's half extent.
    pub half: Vec2,
    /// The cell pitch staged clusters are drawn at: each fits its slot, inside
    /// the slot's padding, in any rotation, and is never larger than on the board.
    pub pitch: f32,
}

impl Staging {
    /// The slot under `p`, if any.
    pub fn slot_at(&self, p: Vec2) -> Option<usize> {
        self.centers.iter().position(|&c| ((p - c).abs() - self.half).max_element() <= 0.0)
    }
}

impl Hud {
    /// Height of the score band at the top, in ems.
    const BAND: f32 = 5.5;
    /// Room for the hint line at the bottom, in ems.
    const HINT: f32 = 2.0;

    pub fn new(screen: Screen, slots: usize, board_pitch: f32, c: &Config) -> Self {
        let top = screen.em(Self::BAND);
        let pad = screen.em(c.board_pad);
        let floor = screen.size.y - screen.em(Self::HINT);
        let room = Vec2::new(screen.size.x - 2.0 * pad, floor - top - screen.em(c.staging_min));
        let side = room.min_element().max(screen.em(4.0));
        let view = View::fit_area(
            Vec2::splat(BOARD_SIDE),
            BOARD_CENTER,
            Vec2::new((screen.size.x - side) * 0.5, top),
            Vec2::splat(side),
        );

        // Slots split the board's width, from just under the board to the hint.
        let stage_top = top + side + pad.min(screen.em(1.0));
        let stage = Vec2::new(side, (floor - stage_top).max(0.0));
        let n = slots.max(1) as f32;
        let slot = Vec2::new(stage.x / n, stage.y);
        let inner = (slot - Vec2::splat(2.0 * screen.em(c.slot_pad))).max(Vec2::ZERO);
        let left = (screen.size.x - side) * 0.5;
        let centers = (0..slots)
            .map(|i| view.to_world(Vec2::new(left + (i as f32 + 0.5) * slot.x, stage_top + stage.y * 0.5)))
            .collect();
        let pitch = (inner.min_element() / CLUSTER_SPAN / view.scale).min(board_pitch);
        let staging = Staging { centers, half: slot * 0.5 / view.scale, pitch };
        Self { screen, view, staging }
    }

    pub fn hint(&self, s: &mut Shapes, text: &str) {
        let sc = &self.screen;
        s.view = View::PIXELS;
        s.text(sc.at(Anchor::BOTTOM, Vec2::new(0.0, -sc.em(Self::HINT * 0.5))), sc.em(0.75), text, rgb(DIM));
        s.view = self.view;
    }

    pub fn score(&self, s: &mut Shapes, score: f64, best: f64) {
        let sc = &self.screen;
        s.view = View::PIXELS;
        s.text(sc.at(Anchor::TOP, Vec2::new(0.0, sc.em(2.4))), sc.em(2.75), &format!("{}", score.round()), rgb(ACCENT));
        s.text(sc.at(Anchor::TOP, Vec2::new(0.0, sc.em(4.4))), sc.em(0.8), &format!("BEST {}", best.round()), rgb(DIM));
        s.view = self.view;
    }

    pub fn game_over(&self, s: &mut Shapes, score: f64) {
        let sc = &self.screen;
        s.view = View::PIXELS;
        s.rect(sc.at(Anchor::CENTER, Vec2::ZERO), sc.size, rgba(0x050403, 0.82));
        let c = self.view.to_screen(Vec2::new(0.0, -10.0));
        s.rounded_rect(c, Vec2::new(sc.em(15.0), sc.em(9.5)), sc.em(1.0), rgb(0x16140f)).outline(sc.em(0.1), rgb(LINE));
        s.text(c - Vec2::new(0.0, sc.em(2.6)), sc.em(1.6), "Game over", text_color(1.0));
        s.text(c + Vec2::new(0.0, sc.em(0.2)), sc.em(2.2), &format!("{}", score.round()), rgb(ACCENT));
        s.text(c + Vec2::new(0.0, sc.em(2.9)), sc.em(0.8), "TAP TO PLAY AGAIN", rgb(DIM));
        s.view = self.view;
    }
}
