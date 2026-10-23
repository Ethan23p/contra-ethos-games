//! One pointer model for mouse and touch, plus the gestures both games use.
//!
//! Positions are physical pixels, like [`crate::Frame::size`]; map them into the
//! game with [`crate::View::to_world`]. The first finger down (or the mouse) is
//! the primary pointer, and only it makes gestures; every pointer still arrives
//! as raw [`Input::Pointer`] events, so multi-touch stays possible.

use glam::Vec2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Down,
    Move,
    Up,
    /// The platform took the pointer away (a system gesture, the tab losing focus).
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pointer {
    /// Stable while the pointer is down. The mouse is always 0.
    pub id: u64,
    pub phase: Phase,
    pub pos: Vec2,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Input {
    Pointer(Pointer),
    /// Down and up without moving past the slop or reaching the hold time.
    Tap(Vec2),
    /// Held still for the hold time. A drag may still follow; a tap won't.
    Hold(Vec2),
    /// Moved past the slop. Carries where the pointer went down, so the
    /// drag can pick up what was under the finger.
    DragStart(Vec2),
    Drag(Vec2),
    DragEnd(Vec2),
}

/// Gesture thresholds. Apps return these from [`crate::App::gestures`], so they
/// can be bound to tunables.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GestureConfig {
    /// Distance in logical pixels a pointer may wander and still count as still.
    pub slop: f32,
    /// Seconds a still pointer must stay down to become a hold.
    pub hold: f32,
}

impl Default for GestureConfig {
    fn default() -> Self {
        Self { slop: 8.0, hold: 0.45 }
    }
}

#[derive(Debug)]
struct Press {
    id: u64,
    start: Vec2,
    since: f32,
    held: bool,
    dragging: bool,
}

/// Turns raw pointer events into gestures.
#[derive(Debug, Default)]
pub struct Gestures {
    press: Option<Press>,
}

impl Gestures {
    /// Feeds one raw event; `scale` is physical pixels per logical pixel.
    pub fn pointer(&mut self, p: Pointer, now: f32, scale: f32, config: GestureConfig, out: &mut Vec<Input>) {
        out.push(Input::Pointer(p));
        match p.phase {
            Phase::Down => {
                if self.press.is_none() {
                    self.press = Some(Press { id: p.id, start: p.pos, since: now, held: false, dragging: false });
                }
            }
            Phase::Move => {
                let Some(press) = self.press.as_mut().filter(|press| press.id == p.id) else { return };
                if !press.dragging && p.pos.distance(press.start) > config.slop * scale {
                    press.dragging = true;
                    out.push(Input::DragStart(press.start));
                }
                if press.dragging {
                    out.push(Input::Drag(p.pos));
                }
            }
            Phase::Up | Phase::Cancel => {
                if self.press.as_ref().is_none_or(|press| press.id != p.id) {
                    return;
                }
                let press = self.press.take().unwrap();
                if press.dragging {
                    out.push(Input::DragEnd(p.pos));
                } else if p.phase == Phase::Up && !press.held {
                    out.push(Input::Tap(press.start));
                }
            }
        }
    }

    /// Called once per frame, so a hold fires even when nothing moves.
    pub fn tick(&mut self, now: f32, config: GestureConfig, out: &mut Vec<Input>) {
        if let Some(press) = self.press.as_mut()
            && !press.held
            && !press.dragging
            && now - press.since >= config.hold
        {
            press.held = true;
            out.push(Input::Hold(press.start));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(g: &mut Gestures, id: u64, phase: Phase, x: f32, now: f32) -> Vec<Input> {
        let mut out = Vec::new();
        g.pointer(Pointer { id, phase, pos: Vec2::new(x, 0.0) }, now, 2.0, GestureConfig::default(), &mut out);
        out.retain(|i| !matches!(i, Input::Pointer(_)));
        out
    }

    #[test]
    fn short_still_press_is_a_tap() {
        let mut g = Gestures::default();
        feed(&mut g, 1, Phase::Down, 10.0, 0.0);
        assert!(feed(&mut g, 1, Phase::Move, 20.0, 0.05).is_empty(), "within slop (8 logical = 16 px)");
        assert_eq!(feed(&mut g, 1, Phase::Up, 20.0, 0.1), vec![Input::Tap(Vec2::new(10.0, 0.0))]);
    }

    #[test]
    fn moving_past_slop_drags_and_never_taps() {
        let mut g = Gestures::default();
        feed(&mut g, 1, Phase::Down, 0.0, 0.0);
        let start = feed(&mut g, 1, Phase::Move, 30.0, 0.1);
        assert_eq!(start, vec![Input::DragStart(Vec2::ZERO), Input::Drag(Vec2::new(30.0, 0.0))]);
        assert_eq!(feed(&mut g, 1, Phase::Up, 40.0, 0.2), vec![Input::DragEnd(Vec2::new(40.0, 0.0))]);
    }

    #[test]
    fn hold_fires_once_and_suppresses_the_tap() {
        let mut g = Gestures::default();
        let mut out = Vec::new();
        feed(&mut g, 1, Phase::Down, 0.0, 0.0);
        g.tick(0.2, GestureConfig::default(), &mut out);
        assert!(out.is_empty());
        g.tick(0.5, GestureConfig::default(), &mut out);
        g.tick(0.6, GestureConfig::default(), &mut out);
        assert_eq!(out, vec![Input::Hold(Vec2::ZERO)]);
        assert!(feed(&mut g, 1, Phase::Up, 0.0, 0.7).is_empty());
    }

    #[test]
    fn second_finger_makes_no_gestures() {
        let mut g = Gestures::default();
        feed(&mut g, 1, Phase::Down, 0.0, 0.0);
        feed(&mut g, 2, Phase::Down, 100.0, 0.0);
        assert!(feed(&mut g, 2, Phase::Move, 300.0, 0.1).is_empty());
        assert!(feed(&mut g, 2, Phase::Up, 300.0, 0.1).is_empty());
        assert_eq!(feed(&mut g, 1, Phase::Up, 0.0, 0.2), vec![Input::Tap(Vec2::ZERO)]);
    }

    #[test]
    fn cancel_ends_a_drag_but_never_taps() {
        let mut g = Gestures::default();
        feed(&mut g, 1, Phase::Down, 0.0, 0.0);
        assert!(feed(&mut g, 1, Phase::Cancel, 0.0, 0.1).is_empty());
    }
}
