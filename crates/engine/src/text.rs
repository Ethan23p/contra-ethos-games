//! Text as signed distance fields. At startup the embedded font is rasterised
//! once, each glyph is turned into a distance field, and the fields are packed
//! into one single-channel atlas. The shape shader then draws glyphs exactly like
//! shapes, so text gets the same crisp edges, outlines and glow at any size.

use std::collections::HashMap;
use std::sync::LazyLock;

use glam::Vec2;

pub(crate) const FONT_BYTES: &[u8] = include_bytes!("../assets/Fredoka-SemiBold.ttf");
/// Size the glyphs are rasterised at. Text stays sharp well above this, because
/// the field is interpolated, not the pixels.
const RASTER_PX: f32 = 64.0;
/// How far, in atlas pixels, the field reaches either side of a glyph's edge.
/// This bounds the widest outline or glow text can have, at RASTER_PX.
pub(crate) const SPREAD: f32 = 8.0;
const ATLAS_WIDTH: usize = 1024;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Glyph {
    /// Atlas rectangle, normalised: [u0, v0, u1, v1].
    pub uv: [f32; 4],
    /// Cell size in atlas pixels, padding included.
    pub size: Vec2,
    /// Cell top-left relative to the pen on the baseline, y down, atlas pixels.
    pub offset: Vec2,
    pub advance: f32,
}

pub(crate) struct Font {
    glyphs: HashMap<char, Glyph>,
    /// Height of a capital or a digit above the baseline, atlas pixels.
    pub cap_height: f32,
    pub atlas: Vec<u8>,
    pub atlas_size: (u32, u32),
}

pub(crate) static FONT: LazyLock<Font> = LazyLock::new(Font::build);

impl Font {
    fn build() -> Self {
        let font = fontdue::Font::from_bytes(FONT_BYTES, fontdue::FontSettings::default()).expect("embedded font");
        let pad = SPREAD as usize;

        let mut fields = Vec::new();
        for ch in (' '..='~').chain(['×', '·', '…']) {
            let (m, coverage) = font.rasterize(ch, RASTER_PX);
            let (w, h) = (m.width + 2 * pad, m.height + 2 * pad);
            let field = distance_field(&coverage, m.width, m.height, pad);
            let offset = Vec2::new(m.xmin as f32 - SPREAD, -(m.ymin as f32 + m.height as f32) - SPREAD);
            fields.push((ch, w, h, field, offset, m.advance_width));
        }

        // Shelf packing, tallest first so rows waste little.
        fields.sort_by_key(|f| std::cmp::Reverse(f.2));
        let (mut x, mut y, mut row) = (0, 0, 0);
        let mut placed = Vec::with_capacity(fields.len());
        for (ch, w, h, field, offset, advance) in fields {
            if x + w > ATLAS_WIDTH {
                (x, y, row) = (0, y + row + 1, 0);
            }
            placed.push((ch, x, y, w, h, field, offset, advance));
            x += w + 1;
            row = row.max(h);
        }
        let height = (y + row).next_power_of_two();

        let mut atlas = vec![0u8; ATLAS_WIDTH * height];
        let mut glyphs = HashMap::new();
        let (aw, ah) = (ATLAS_WIDTH as f32, height as f32);
        for (ch, x, y, w, h, field, offset, advance) in placed {
            for row in 0..h {
                atlas[(y + row) * ATLAS_WIDTH + x..][..w].copy_from_slice(&field[row * w..][..w]);
            }
            let uv = [x as f32 / aw, y as f32 / ah, (x + w) as f32 / aw, (y + h) as f32 / ah];
            glyphs.insert(ch, Glyph { uv, size: Vec2::new(w as f32, h as f32), offset, advance });
        }

        // Tabular digits: each takes the widest digit's advance and sits centred
        // in it, so a changing number doesn't shift its neighbours.
        let widest = ('0'..='9').map(|c| glyphs[&c].advance).fold(0.0, f32::max);
        for c in '0'..='9' {
            let g = glyphs.get_mut(&c).unwrap();
            g.offset.x += (widest - g.advance) * 0.5;
            g.advance = widest;
        }

        let cap_height = font.metrics('8', RASTER_PX).height as f32;
        Self { glyphs, cap_height, atlas, atlas_size: (ATLAS_WIDTH as u32, height as u32) }
    }

    pub fn glyph(&self, ch: char) -> Option<&Glyph> {
        self.glyphs.get(&ch).or_else(|| self.glyphs.get(&'?'))
    }

    /// Width of `text` set at font size `px`.
    pub fn width(&self, text: &str, px: f32) -> f32 {
        text.chars().filter_map(|c| self.glyph(c)).map(|g| g.advance).sum::<f32>() * px / RASTER_PX
    }

    /// Atlas pixels to screen pixels at font size `px`.
    pub fn scale(px: f32) -> f32 {
        px / RASTER_PX
    }
}

/// Turns a coverage bitmap into a padded distance field, encoded so 0.5 is the
/// edge and each unit of 1/255 is SPREAD*2/255 atlas pixels (inside is brighter).
fn distance_field(coverage: &[u8], w: usize, h: usize, pad: usize) -> Vec<u8> {
    let (fw, fh) = (w + 2 * pad, h + 2 * pad);
    let inside = |x: usize, y: usize| {
        x >= pad && y >= pad && x < pad + w && y < pad + h && coverage[(y - pad) * w + (x - pad)] >= 128
    };
    let to_inside = squared_edt(fw, fh, inside);
    let to_outside = squared_edt(fw, fh, |x, y| !inside(x, y));

    let mut field = vec![0u8; fw * fh];
    for y in 0..fh {
        for x in 0..fw {
            let i = y * fw + x;
            // Half a pixel puts the edge between the last inside and first outside pixel.
            let d = if inside(x, y) { 0.5 - to_outside[i].sqrt() } else { to_inside[i].sqrt() - 0.5 };
            field[i] = ((0.5 - d / (2.0 * SPREAD)).clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
    field
}

/// Squared Euclidean distance from every pixel to the nearest pixel where
/// `feature` holds (Felzenszwalb and Huttenlocher, separable in x and y).
fn squared_edt(w: usize, h: usize, feature: impl Fn(usize, usize) -> bool) -> Vec<f32> {
    const FAR: f32 = 1e20;
    let mut grid: Vec<f32> = (0..w * h).map(|i| if feature(i % w, i / w) { 0.0 } else { FAR }).collect();
    let n = w.max(h);
    let (mut f, mut d, mut v, mut z) = (vec![0.0; n], vec![0.0; n], vec![0usize; n], vec![0.0; n + 1]);
    for x in 0..w {
        for y in 0..h {
            f[y] = grid[y * w + x];
        }
        edt_1d(&f[..h], &mut d, &mut v, &mut z);
        for y in 0..h {
            grid[y * w + x] = d[y];
        }
    }
    for y in 0..h {
        f[..w].copy_from_slice(&grid[y * w..][..w]);
        edt_1d(&f[..w], &mut d, &mut v, &mut z);
        grid[y * w..][..w].copy_from_slice(&d[..w]);
    }
    grid
}

/// One-dimensional squared distance transform: the lower envelope of parabolas.
fn edt_1d(f: &[f32], d: &mut [f32], v: &mut [usize], z: &mut [f32]) {
    let n = f.len();
    let sq = |q: usize| (q * q) as f32;
    let mut k = 0;
    v[0] = 0;
    z[0] = f32::NEG_INFINITY;
    z[1] = f32::INFINITY;
    for q in 1..n {
        let intersect = |p: usize| ((f[q] + sq(q)) - (f[p] + sq(p))) / (2.0 * (q - p) as f32);
        // z[0] is -inf, so this never runs past the first parabola.
        let mut s = intersect(v[k]);
        while s <= z[k] {
            k -= 1;
            s = intersect(v[k]);
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f32::INFINITY;
    }
    k = 0;
    for (q, out) in d.iter_mut().enumerate().take(n) {
        while z[k + 1] < q as f32 {
            k += 1;
        }
        let dq = q as f32 - v[k] as f32;
        *out = dq * dq + f[v[k]];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edt_matches_brute_force() {
        let (w, h) = (13, 9);
        let feature = |x: usize, y: usize| (x == 3 && y == 2) || (x == 10 && y == 7);
        let fast = squared_edt(w, h, feature);
        for y in 0..h {
            for x in 0..w {
                let brute = [(3.0, 2.0), (10.0, 7.0)]
                    .iter()
                    .map(|&(fx, fy): &(f32, f32)| (x as f32 - fx).powi(2) + (y as f32 - fy).powi(2))
                    .fold(f32::MAX, f32::min);
                assert_eq!(fast[y * w + x], brute, "at ({x}, {y})");
            }
        }
    }

    #[test]
    fn atlas_holds_digits() {
        let font = &*FONT;
        for c in '0'..='9' {
            let g = font.glyph(c).unwrap();
            assert!(g.size.x > 2.0 * SPREAD && g.advance > 0.0, "{c}");
        }
        // Digits are tabular, so numbers don't jiggle as they change.
        assert_eq!(font.width("1", 10.0), font.width("8", 10.0));
    }
}
