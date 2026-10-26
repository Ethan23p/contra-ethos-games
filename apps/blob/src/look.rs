//! How blobs look: identical neighbours drawn as one goo, glimmer on the
//! peak, and the hand-rolled effects (pop, purge, nova, spawn).

use std::collections::HashMap;
use std::f32::consts::{PI, TAU};

use blob::Rng;
use engine::glam::Vec2;
use engine::{Goo, Shapes, rgb, rgba};

use crate::config::Config;

/// The old build's group colours: azure, emerald, crimson.
pub const GROUP: [u32; 3] = [0x38bdf8, 0x34d399, 0xfb7185];
const GOLD: u32 = 0xf5c542;
const SHINE: u32 = 0xfff6d6;
/// Digits: a hair off pure white, so they sit on the colour instead of glaring.
const INK: u32 = 0xfbfaf6;
/// The digits' outline and shadow, off the peak.
const EDGE: u32 = 0x0b0d12;

pub fn blob_color(group: u8, value: u32) -> [f32; 4] {
    // Higher values lighten toward white, a step per doubling.
    let k = (value.max(1).ilog2() as f32 * 0.07).min(0.6);
    let base = rgb(GROUP[group as usize % GROUP.len()]);
    [base[0] + (1.0 - base[0]) * k, base[1] + (1.0 - base[1]) * k, base[2] + (1.0 - base[2]) * k, 1.0]
}

fn mix(a: [f32; 4], b: [f32; 4], k: f32) -> [f32; 4] {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * k)
}

/// One blob as it draws this frame.
pub struct Drawn {
    pub id: u32,
    pub group: u8,
    pub value: u32,
    pub pos: Vec2,
    /// Drawn radius, effects included.
    pub r: f32,
    pub merging: Option<u32>,
}

/// The goo style for a blob. Gold marks the peak.
pub fn style(c: &Config, group: u8, value: u32, gold: bool, r: f32) -> Goo {
    let mut g = Goo {
        fill: blob_color(group, value),
        blend: r * c.blend,
        link_radius: r * c.link_width,
        ..Goo::default()
    };
    if gold && c.glimmer {
        g.sheen = c.glimmer_strength;
        g.sheen_color = rgb(SHINE);
    }
    g
}

/// A goo group: indices into the drawn blobs, and its tethers between them.
type Group = (Vec<usize>, Vec<(usize, usize)>);

/// Groups that draw as one goo: identical blobs within the blend of each
/// other, blobs merging together, and bonded pairs (with their tethers).
fn groups(blobs: &[Drawn], bonds: &[(u32, u32)], blend: f32) -> Vec<Group> {
    let mut parent: Vec<usize> = (0..blobs.len()).collect();
    fn root(p: &mut [usize], mut i: usize) -> usize {
        while p[i] != i {
            p[i] = p[p[i]];
            i = p[i];
        }
        i
    }
    let join = |p: &mut Vec<usize>, a: usize, b: usize| {
        let (ra, rb) = (root(p, a), root(p, b));
        p[ra.max(rb)] = ra.min(rb);
    };
    let index: HashMap<u32, usize> = blobs.iter().enumerate().map(|(i, b)| (b.id, i)).collect();
    for i in 0..blobs.len() {
        for j in i + 1..blobs.len() {
            let (a, b) = (&blobs[i], &blobs[j]);
            if a.group != b.group || a.value != b.value {
                continue;
            }
            let near = a.pos.distance(b.pos) - a.r - b.r < blend * (a.r + b.r) * 0.5;
            let merging = a.merging.is_some() && a.merging == b.merging;
            if near || merging {
                join(&mut parent, i, j);
            }
        }
    }
    let links: Vec<(usize, usize)> = bonds.iter().filter_map(|(x, y)| Some((*index.get(x)?, *index.get(y)?))).collect();
    for &(a, b) in &links {
        join(&mut parent, a, b);
    }
    let mut out: Vec<Group> = Vec::new();
    let mut slot: HashMap<usize, usize> = HashMap::new();
    for i in 0..blobs.len() {
        let r = root(&mut parent, i);
        let k = *slot.entry(r).or_insert_with(|| {
            out.push((Vec::new(), Vec::new()));
            out.len() - 1
        });
        out[k].0.push(i);
    }
    for (a, b) in links {
        let k = slot[&root(&mut parent, a)];
        let local = |i: usize| out[k].0.iter().position(|&m| m == i).unwrap();
        let link = (local(a), local(b));
        out[k].1.push(link);
    }
    out
}

/// Where the sheen band sits at `time`: it sweeps across in the first part
/// of each period and waits past the far edge for the rest, so the jump back
/// to the start always happens out of sight.
fn sweep(time: f32, period: f32, extent: f32) -> f32 {
    let u = (time / period.max(0.1)).fract();
    let e = extent + 1.0;
    const PASS: f32 = 0.4;
    if u < PASS {
        let x = u / PASS;
        -e + 2.0 * e * x * x * (3.0 - 2.0 * x)
    } else {
        e
    }
}

/// The field's blobs as goo, their values on top, and glimmer on the peak.
pub fn draw_field(s: &mut Shapes, c: &Config, blobs: &[Drawn], bonds: &[(u32, u32)], peak: u32, time: f32) {
    for (members, links) in groups(blobs, bonds, c.blend) {
        let first = &blobs[members[0]];
        let gold = first.value == peak;
        let mut st = style(c, first.group, first.value, gold, c.radius * c.drawn);
        if gold {
            let extent =
                members.iter().map(|&i| (blobs[i].pos.distance(first.pos) + blobs[i].r) / first.r).fold(1.0, f32::max);
            st.sheen_at = sweep(time, c.glimmer_period, extent);
        }
        let circles: Vec<(Vec2, f32)> = members.iter().map(|&i| (blobs[i].pos, blobs[i].r)).collect();
        s.goo(&circles, &links, &st);
    }
    for b in blobs {
        // Merging blobs fade their numbers as they flow together.
        let fade = if b.merging.is_some() { 0.3 } else { 1.0 };
        number(s, c, b.pos, b.r, b.value, b.value == peak && c.glimmer, fade);
    }
    if c.glimmer {
        for b in blobs.iter().filter(|b| b.value == peak) {
            sparkles(s, b.id, b.pos, b.r, time, c.sparkles.max(0) as u32, c.glint_rate, c.glimmer_strength);
        }
    }
}

/// A lone blob, as in the launcher or the queue.
#[allow(clippy::too_many_arguments)]
pub fn draw_one(s: &mut Shapes, c: &Config, pos: Vec2, r: f32, group: u8, value: u32, gold: bool, time: f32) {
    let mut st = style(c, group, value, gold, r);
    if gold {
        st.sheen_at = sweep(time, c.glimmer_period, 1.0);
    }
    s.goo(&[(pos, r)], &[], &st);
    number(s, c, pos, r, value, gold && c.glimmer, 1.0);
}

/// A blob's value, with the configured shadow and outline: dark, or gold on
/// the peak. Glyph outlines sit outside the stroke.
fn number(s: &mut Shapes, c: &Config, pos: Vec2, r: f32, value: u32, gold: bool, fade: f32) {
    let text = value.to_string();
    let size = r * if text.len() >= 3 { 0.68 } else { 0.94 };
    let edge = if gold { GOLD } else { EDGE };
    if c.digit_shadow > 0.0 {
        let at = pos + Vec2::new(0.0, size * c.digit_shadow_offset);
        let width = if c.digit_outline > 0.0 { size * c.digit_outline_width } else { 0.0 };
        let color = rgba(edge, c.digit_shadow * fade);
        // The shadow takes the outline's footprint too, so it reads as one piece.
        s.text(at, size, &text, color).outline(width, color);
    }
    let digits = s.text(pos, size, &text, rgba(INK, 0.95 * fade));
    if c.digit_outline > 0.0 {
        digits.outline(size * c.digit_outline_width, rgba(edge, c.digit_outline * fade));
    }
}

fn hash(a: u32, b: u32, c: u32) -> f32 {
    let mut x = (a as u64) << 40 ^ (b as u64) << 20 ^ c as u64;
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    ((x ^ (x >> 31)) >> 40) as f32 / (1u64 << 24) as f32
}

/// Glints that twinkle on a blob. Each slot lives a cycle at a time and moves
/// only between cycles, while it's dark, so nothing ever pops. `rate` is
/// glints per second for one slot; the others run a little slower, so they
/// never fall into step.
#[allow(clippy::too_many_arguments)]
fn sparkles(s: &mut Shapes, id: u32, pos: Vec2, r: f32, time: f32, count: u32, rate: f32, strength: f32) {
    for slot in 0..count {
        let period = (1.0 + 0.25 * slot as f32) / rate.max(0.01);
        let t = time / period + hash(id, slot, 0);
        let (n, u) = (t.floor(), t.fract());
        const LIT: f32 = 0.45;
        if u >= LIT {
            continue;
        }
        let w = (PI * u / LIT).sin().powi(2);
        let n = n as i64 as u32;
        let at = pos + Vec2::from_angle(hash(id, slot, n * 2 + 1) * TAU) * r * (0.2 + 0.55 * hash(id, slot, n * 2 + 2));
        star(s, at, r * 0.4 * w, 0.8 * u, rgba(SHINE, strength.min(1.0) * w));
    }
}

/// A four-point glint: two crossed slivers and a soft core.
pub fn star(s: &mut Shapes, at: Vec2, size: f32, angle: f32, color: [f32; 4]) {
    if size <= 0.01 {
        return;
    }
    for k in [0.0, PI * 0.5] {
        s.rect(at, Vec2::new(size * 2.0, size * 0.16), color).corner(size * 0.08).soft(size * 0.1).rotate(angle + k);
    }
    s.circle(at, size * 0.35, color).soft(size * 0.5);
}

/// How a blob came to be, for its entrance.
#[derive(Clone, Copy)]
pub enum Birth {
    /// Made by a merge: it lands big and wobbles to size.
    Pop,
    /// Seeded: it grows in after `delay`.
    Spawn { delay: f32 },
}

impl Birth {
    /// Drawn size as a fraction of full, `t` seconds after birth.
    pub fn scale(self, t: f32, wobble: f32) -> f32 {
        match self {
            Birth::Pop => 1.0 + wobble * (-7.0 * t).exp() * (22.0 * t).cos(),
            Birth::Spawn { delay } => {
                let x = ((t - delay) / 0.34).clamp(0.0, 1.0);
                // Ease out with a little overshoot.
                let (c1, c3) = (1.70158, 2.70158);
                1.0 + c3 * (x - 1.0).powi(3) + c1 * (x - 1.0).powi(2)
            }
        }
    }

    pub fn done(self, t: f32) -> bool {
        match self {
            Birth::Pop => t > 1.0,
            Birth::Spawn { delay } => t > delay + 0.4,
        }
    }
}

/// A droplet thrown off by a merge, a purge or a nova.
pub struct Drop {
    pos: Vec2,
    vel: Vec2,
    r: f32,
    color: [f32; 4],
    born: f32,
    life: f32,
}

pub struct Drops(pub Vec<Drop>);

impl Drops {
    /// `count` droplets from `at`, at speeds in `speed`, sized as a fraction
    /// of `r`.
    #[allow(clippy::too_many_arguments)]
    pub fn burst(
        &mut self,
        rng: &mut Rng,
        at: Vec2,
        r: f32,
        count: usize,
        speed: (f32, f32),
        size: (f32, f32),
        color: [f32; 4],
        now: f32,
        life: f32,
    ) {
        let turn = rng.unit() as f32 * TAU;
        for k in 0..count {
            let a = turn + TAU * (k as f32 + 0.6 * rng.unit() as f32) / count.max(1) as f32;
            let dir = Vec2::from_angle(a);
            let v = speed.0 + (speed.1 - speed.0) * rng.unit() as f32;
            self.0.push(Drop {
                pos: at + dir * r * 0.6,
                vel: dir * v,
                r: r * (size.0 + (size.1 - size.0) * rng.unit() as f32),
                color,
                born: now,
                life: life * (0.75 + 0.5 * rng.unit() as f32),
            });
        }
    }

    pub fn step(&mut self, dt: f32, now: f32) {
        for d in &mut self.0 {
            d.pos += d.vel * dt;
            d.vel *= (-4.0 * dt).exp();
        }
        self.0.retain(|d| now - d.born < d.life);
    }

    pub fn draw(&self, s: &mut Shapes, now: f32) {
        for d in &self.0 {
            let k = ((now - d.born) / d.life).clamp(0.0, 1.0);
            let mut color = d.color;
            color[3] *= (1.0 - k).min(0.3) / 0.3;
            let goo = Goo { fill: color, ..Goo::default() };
            s.goo(&[(d.pos, d.r * (1.0 - k).powf(0.6))], &[], &goo);
        }
    }
}

pub fn grey(color: [f32; 4], k: f32) -> [f32; 4] {
    let l = color[0] * 0.3 + color[1] * 0.59 + color[2] * 0.11;
    mix(color, [l, l, l, color[3]], k)
}

pub fn gold() -> [f32; 4] {
    rgb(GOLD)
}

pub fn shine() -> [f32; 4] {
    rgb(SHINE)
}
