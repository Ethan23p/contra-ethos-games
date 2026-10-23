//! Colours, the board's geometry, and how a die is drawn.

use dice::Pos;
use engine::glam::Vec2;
use engine::{Shapes, rgb, rgba};

use crate::config::Config;

pub const BG: u32 = 0x0a0908;
pub const LINE: u32 = 0x3c3a37;
pub const CELL: u32 = 0x0f0e0d;
pub const DIM: u32 = 0x948d81;
pub const TEXT: u32 = 0xf4f1ec;
pub const ACCENT: u32 = 0x5b8ef2;
pub const RED: u32 = 0xd9564f;

/// The world: 100 units wide, 170 high, centred on the origin.
pub const WORLD: Vec2 = Vec2::new(100.0, 170.0);
const BOARD_SIDE: f32 = 90.0;
const BOARD_CENTER: Vec2 = Vec2::new(0.0, -15.0);
const BOARD_PAD: f32 = 1.5;
/// The gap between cells, in world units.
pub const GAP: f32 = 0.75;

pub const HEAD_SLOT: Vec2 = Vec2::new(0.0, 57.0);
pub const HEAD_PITCH: f32 = 14.0;
pub const NEXT_Y: f32 = 79.0;
pub const NEXT_PITCH: f32 = 7.0;

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

    pub fn bottom(&self) -> f32 {
        BOARD_CENTER.y + BOARD_SIDE * 0.5
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
