//! The in-game menu: a small button, and a paused overlay with New game and the
//! Dev mode toggle. Drawn from our own primitives, so it reads as part of the game.

use glam::Vec2;

use crate::input::{Input, Phase};
use crate::screen::{Anchor, Screen};
use crate::shapes::{Shapes, View, rgb, rgba};
use crate::tune::storage;

/// What the menu made of an input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reply {
    /// Not the menu's; the game may use it.
    Pass,
    /// The menu's; the game ignores it.
    Taken,
    NewGame,
}

pub struct Menu {
    /// While open, the game pauses.
    pub open: bool,
    /// Whether the config overlay shows. Per device, across games.
    pub dev: bool,
    persist: bool,
    anchor: Anchor,
    /// The button's centre from the anchor, in ems.
    offset: Vec2,
    ink: u32,
    surface: u32,
    /// Whether the latest press went to the menu, so the gesture it ends in
    /// stays out of the game too.
    taken: bool,
    pressing: bool,
}

/// Half the button's touch target, in ems.
const REACH: f32 = 1.5;
const CHOICE: Vec2 = Vec2::new(13.0, 3.25);

impl Menu {
    pub fn new(anchor: Anchor, offset: Vec2, ink: u32, surface: u32) -> Self {
        let dev = storage::load("dev-mode").is_some_and(|s| s.trim() == "on");
        Self { dev, persist: true, ..Self::unsaved(anchor, offset, ink, surface) }
    }

    /// Dev mode off, and a toggle isn't remembered (tests).
    fn unsaved(anchor: Anchor, offset: Vec2, ink: u32, surface: u32) -> Self {
        Self { open: false, dev: false, persist: false, anchor, offset, ink, surface, taken: false, pressing: false }
    }

    fn button(&self, s: &Screen) -> Vec2 {
        s.at(self.anchor, self.offset * s.em)
    }

    /// The choices' centres: New game, Resume, Dev mode.
    fn choices(s: &Screen) -> [Vec2; 3] {
        let step = Vec2::new(0.0, s.em(CHOICE.y + 0.8));
        let mid = s.at(Anchor::CENTER, Vec2::ZERO);
        [mid - step, mid, mid + step]
    }

    fn on_choice(s: &Screen, center: Vec2, p: Vec2) -> bool {
        let d = (p - center).abs();
        d.x <= s.em(CHOICE.x * 0.5) && d.y <= s.em(CHOICE.y * 0.5)
    }

    /// Feed every input through this first. A press acts on its release.
    pub fn input(&mut self, input: Input, screen: Screen) -> Reply {
        let pass = |taken: bool| if taken { Reply::Taken } else { Reply::Pass };
        let Input::Pointer(p) = input else { return pass(self.taken) };
        match p.phase {
            Phase::Down => {
                let d = (p.pos - self.button(&screen)).abs();
                self.taken = self.open || d.max_element() <= screen.em(REACH);
                self.pressing = true;
                pass(self.taken)
            }
            Phase::Move => pass(self.open || (self.taken && self.pressing)),
            Phase::Cancel => {
                self.pressing = false;
                pass(self.taken)
            }
            Phase::Up => {
                self.pressing = false;
                if !self.taken {
                    return Reply::Pass;
                }
                if !self.open {
                    let d = (p.pos - self.button(&screen)).abs();
                    self.open = d.max_element() <= screen.em(REACH);
                    return Reply::Taken;
                }
                let choices = Self::choices(&screen);
                if Self::on_choice(&screen, choices[2], p.pos) {
                    // Stays open, so the switch is seen to move.
                    self.dev = !self.dev;
                    if self.persist {
                        storage::save("dev-mode", if self.dev { "on" } else { "off" });
                    }
                    return Reply::Taken;
                }
                self.open = false;
                if Self::on_choice(&screen, choices[0], p.pos) { Reply::NewGame } else { Reply::Taken }
            }
        }
    }

    /// Draws the button, and the overlay while open. Call this last.
    pub fn draw(&self, s: &mut Shapes, screen: Screen) {
        let view = s.view;
        s.view = View::PIXELS;
        let at = self.button(&screen);
        let line = Vec2::new(screen.em(1.1), screen.em(0.13));
        for k in [-1.0, 0.0, 1.0] {
            s.rounded_rect(at + Vec2::new(0.0, k * screen.em(0.34)), line, line.y, rgba(self.ink, 0.6));
        }
        if self.open {
            s.rect(screen.size * 0.5, screen.size, rgba(0, 0.72));
            let size = CHOICE * screen.em;
            let [new_game, resume, dev] = Self::choices(&screen);
            for center in [new_game, resume, dev] {
                s.rounded_rect(center, size, screen.em(0.8), rgb(self.surface))
                    .outline(screen.em(0.08), rgba(self.ink, 0.25));
            }
            s.text(new_game, screen.em(1.1), "NEW GAME", rgb(self.ink));
            s.text(resume, screen.em(1.1), "RESUME", rgb(self.ink));
            s.text(dev - Vec2::new(screen.em(1.7), 0.0), screen.em(1.1), "DEV MODE", rgb(self.ink));

            // The switch: a pill, with its knob to the right when on.
            let track = Vec2::new(screen.em(3.0), screen.em(1.6));
            let center = dev + Vec2::new(screen.em(4.3), 0.0);
            let side = if self.dev { 1.0 } else { -1.0 };
            s.rounded_rect(center, track, track.y * 0.5, rgba(self.ink, if self.dev { 0.6 } else { 0.15 }));
            let knob = center + Vec2::new(side * (track.x - track.y) * 0.5, 0.0);
            s.circle(knob, track.y * 0.38, rgb(if self.dev { self.surface } else { self.ink }));
        }
        s.view = view;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Pointer;

    fn screen() -> Screen {
        Screen::new(Vec2::new(400.0, 800.0), 1.0)
    }

    fn press(m: &mut Menu, phase: Phase, pos: Vec2) -> Reply {
        m.input(Input::Pointer(Pointer { id: 1, phase, pos }), screen())
    }

    fn tap(m: &mut Menu, pos: Vec2) -> (Reply, Reply) {
        (press(m, Phase::Down, pos), press(m, Phase::Up, pos))
    }

    fn menu() -> (Menu, Vec2) {
        (Menu::unsaved(Anchor::TOP, Vec2::new(0.0, 2.0), 0xffffff, 0), Vec2::new(200.0, 32.0))
    }

    #[test]
    fn taps_elsewhere_pass_through() {
        let (mut m, _) = menu();
        assert_eq!(tap(&mut m, Vec2::new(200.0, 400.0)), (Reply::Pass, Reply::Pass));
        assert!(!m.open);
    }

    #[test]
    fn the_button_opens_it_and_its_tap_stays_out_of_the_game() {
        let (mut m, button) = menu();
        assert_eq!(tap(&mut m, button), (Reply::Taken, Reply::Taken));
        assert!(m.open);
        assert_eq!(m.input(Input::Tap(button), screen()), Reply::Taken);
    }

    #[test]
    fn new_game_is_two_taps_away() {
        let (mut m, button) = menu();
        tap(&mut m, button);
        assert_eq!(tap(&mut m, Menu::choices(&screen())[0]).1, Reply::NewGame);
        assert!(!m.open);
    }

    #[test]
    fn resume_or_a_tap_outside_closes_it() {
        for at in [Menu::choices(&screen())[1], Vec2::new(5.0, 700.0)] {
            let (mut m, button) = menu();
            tap(&mut m, button);
            assert_eq!(tap(&mut m, at), (Reply::Taken, Reply::Taken));
            assert!(!m.open);
        }
    }

    #[test]
    fn dev_mode_toggles_and_keeps_the_menu_open() {
        let (mut m, button) = menu();
        assert!(!m.dev);
        tap(&mut m, button);
        let toggle = Menu::choices(&screen())[2];
        assert_eq!(tap(&mut m, toggle), (Reply::Taken, Reply::Taken));
        assert!(m.dev && m.open);
        tap(&mut m, toggle);
        assert!(!m.dev && m.open);
    }

    #[test]
    fn hover_reaches_the_game_only_while_it_is_shut() {
        let (mut m, button) = menu();
        tap(&mut m, button);
        assert_eq!(press(&mut m, Phase::Move, button), Reply::Taken);
        tap(&mut m, button);
        assert_eq!(press(&mut m, Phase::Move, button), Reply::Pass);
    }
}
