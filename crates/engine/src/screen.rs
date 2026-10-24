//! Placing things the way game UI does: pin each element to an anchor point
//! of the viewport, then offset and size it in viewport units or in ems,
//! never in raw pixels. An em is the height of a line of body text, 16
//! logical pixels, so text stays readable on every screen; viewport units
//! scale with the screen itself.

use glam::Vec2;

/// One em in logical pixels: the browser's default text size.
pub const EM: f32 = 16.0;

/// A point of the viewport as fractions of its size: (0, 0) top left, (1, 1)
/// bottom right.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor(pub Vec2);

impl Anchor {
    pub const TOP_LEFT: Self = Self(Vec2::new(0.0, 0.0));
    pub const TOP: Self = Self(Vec2::new(0.5, 0.0));
    pub const TOP_RIGHT: Self = Self(Vec2::new(1.0, 0.0));
    pub const LEFT: Self = Self(Vec2::new(0.0, 0.5));
    pub const CENTER: Self = Self(Vec2::new(0.5, 0.5));
    pub const RIGHT: Self = Self(Vec2::new(1.0, 0.5));
    pub const BOTTOM_LEFT: Self = Self(Vec2::new(0.0, 1.0));
    pub const BOTTOM: Self = Self(Vec2::new(0.5, 1.0));
    pub const BOTTOM_RIGHT: Self = Self(Vec2::new(1.0, 1.0));
}

/// The viewport and its units, in physical pixels (what [`crate::View::PIXELS`] draws in).
#[derive(Clone, Copy, Debug)]
pub struct Screen {
    pub size: Vec2,
    /// One em, in physical pixels.
    pub em: f32,
}

impl Screen {
    pub fn new(size: Vec2, scale: f32) -> Self {
        Self { size, em: EM * scale }
    }

    /// The anchor's point, moved by `offset` (build it from the units below).
    pub fn at(&self, anchor: Anchor, offset: Vec2) -> Vec2 {
        anchor.0 * self.size + offset
    }

    pub fn em(&self, n: f32) -> f32 {
        n * self.em
    }

    /// Percent of the viewport's width, as CSS's `vw`.
    pub fn vw(&self, percent: f32) -> f32 {
        self.size.x * percent / 100.0
    }

    pub fn vh(&self, percent: f32) -> f32 {
        self.size.y * percent / 100.0
    }

    pub fn vmin(&self, percent: f32) -> f32 {
        self.size.min_element() * percent / 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchors_and_units() {
        let s = Screen::new(Vec2::new(800.0, 1600.0), 2.0);
        assert_eq!(s.at(Anchor::TOP, Vec2::new(0.0, s.em(1.0))), Vec2::new(400.0, 32.0));
        assert_eq!(s.at(Anchor::BOTTOM_RIGHT, Vec2::ZERO), Vec2::new(800.0, 1600.0));
        assert_eq!((s.vw(50.0), s.vh(10.0), s.vmin(25.0)), (400.0, 160.0, 200.0));
    }
}
