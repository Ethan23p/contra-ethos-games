//! The config overlay: a floating button in a bottom corner, the config panel
//! it opens, and a small widget of pinned cards while the panel is shut.
//!
//! The button stays put whether the panel is open or not, so it toggles in
//! place; a long press moves it to the other bottom corner. The panel's option
//! island sits in the corner opposite the button. With nothing pinned, the
//! button is the only thing over the game.

use egui::{Align2, Color32, Context, Pos2, Rect, Ui, vec2};

use crate::runner::App;
use crate::tune::storage;

const BUTTON: f32 = 48.0;
const MARGIN: f32 = 12.0;
const LONG_PRESS: f64 = 0.5;
/// Narrower than this (logical px), the panel takes the whole width.
const PHONE: f32 = 640.0;

pub(crate) struct Panel {
    pub open: bool,
    /// The button's corner: bottom right, or bottom left.
    right: bool,
    held: Option<f64>,
    swapped: bool,
    island_height: f32,
    /// Where the overlay took the screen last frame while shut, in points: a
    /// press landing here is the overlay's, not the game's.
    zones: Vec<Rect>,
    persist: bool,
}

impl Panel {
    pub fn new() -> Self {
        let right = storage::load("config-button").is_none_or(|s| s.trim() != "left");
        Self { right, persist: true, ..Self::unsaved() }
    }

    /// Bottom right, and a long press isn't remembered (tests).
    fn unsaved() -> Self {
        Self {
            open: false,
            right: true,
            held: None,
            swapped: false,
            island_height: 0.0,
            zones: Vec::new(),
            persist: false,
        }
    }

    /// Larger touch targets and text than egui's desktop defaults.
    pub fn style(ctx: &Context) {
        ctx.all_styles_mut(|style| {
            use egui::{FontId, TextStyle};
            style.visuals = egui::Visuals::dark();
            style.spacing.interact_size.y = 30.0;
            style.spacing.button_padding = vec2(10.0, 6.0);
            style.spacing.item_spacing = vec2(8.0, 6.0);
            style.spacing.slider_rail_height = 6.0;
            style.text_styles = [
                (TextStyle::Heading, FontId::proportional(20.0)),
                (TextStyle::Body, FontId::proportional(16.0)),
                (TextStyle::Button, FontId::proportional(16.0)),
                (TextStyle::Small, FontId::proportional(13.0)),
                (TextStyle::Monospace, FontId::proportional(15.0)),
            ]
            .into();
        });
    }

    /// Whether a press at `pos` (points) belongs to the overlay.
    pub fn takes(&self, pos: Pos2) -> bool {
        self.open || self.zones.iter().any(|z| z.contains(pos))
    }

    pub fn show<A: App>(&mut self, ctx: &Context, app: &mut A) {
        self.zones.clear();
        if app.config().is_none() {
            return;
        }
        let screen = ctx.content_rect();
        let side = if self.right { 1.0 } else { -1.0 };
        let corner = if self.right { Align2::RIGHT_BOTTOM } else { Align2::LEFT_BOTTOM };
        let opposite = if self.right { Align2::LEFT_BOTTOM } else { Align2::RIGHT_BOTTOM };

        if self.open {
            // A light veil says the game is paused under the panel.
            ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::unique("config veil")))
                .rect_filled(screen, 0.0, Color32::from_black_alpha(90));

            let phone = screen.width() < PHONE;
            let width = if phone { screen.width() } else { 420.0 };
            let x = if phone || !self.right { screen.left() } else { screen.right() - width };
            // Side by side with the island when wide; above it on a phone.
            let below = if phone { self.island_height.max(BUTTON) } else { BUTTON };
            let field_height = screen.height() - below - MARGIN * 3.0;
            egui::Area::new(egui::Id::unique("config field"))
                .fixed_pos(Pos2::new(x, screen.top()))
                .order(egui::Order::Middle)
                .show(ctx, |ui| {
                    let frame = egui::Frame::new()
                        .fill(Color32::from_rgba_premultiplied(14, 13, 12, 240))
                        .inner_margin(egui::Margin { left: 12, right: 12, top: 10, bottom: 0 });
                    frame.show(ui, |ui| {
                        ui.set_width(width - 24.0);
                        ui.set_height(field_height);
                        if let Some(config) = app.config() {
                            config.field(ui);
                        }
                    });
                });

            let island_width = (screen.width() - BUTTON - MARGIN * 3.0).min(360.0);
            let island = egui::Area::new(egui::Id::unique("config island"))
                .anchor(opposite, vec2(-side * MARGIN, -MARGIN))
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(Color32::from_rgba_premultiplied(24, 23, 22, 250))
                        .stroke(egui::Stroke::new(1.0, Color32::from_gray(60)))
                        .corner_radius(14.0)
                        .inner_margin(10.0)
                        .show(ui, |ui| {
                            ui.set_width(island_width - 20.0);
                            app.config_actions(ui);
                            if let Some(config) = app.config() {
                                config.island(ui);
                            }
                        });
                });
            self.island_height = island.response.rect.height();
        } else if app.config().is_some_and(|c| c.has_pins()) {
            let width = (screen.width() - MARGIN * 2.0).min(320.0);
            let widget = egui::Area::new(egui::Id::unique("config widget"))
                .anchor(corner, vec2(-side * MARGIN, -(MARGIN * 2.0 + BUTTON)))
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(Color32::from_rgba_premultiplied(14, 13, 12, 200))
                        .corner_radius(12.0)
                        .inner_margin(6.0)
                        .show(ui, |ui| {
                            ui.set_width(width - 12.0);
                            if let Some(config) = app.config() {
                                config.widget(ui);
                            }
                        });
                });
            self.zones.push(widget.response.rect);
        }

        let button = egui::Area::new(egui::Id::unique("config button"))
            .anchor(corner, vec2(-side * MARGIN, -MARGIN))
            .order(egui::Order::Tooltip)
            .show(ctx, |ui| self.button(ui));
        self.zones.push(button.response.rect);
    }

    /// Tap: open or shut. Hold: move to the other corner.
    fn button(&mut self, ui: &mut Ui) {
        let (rect, response) = ui.allocate_exact_size(vec2(BUTTON, BUTTON), egui::Sense::click());
        let now = ui.input(|i| i.time);
        let down = response.is_pointer_button_down_on();
        if down {
            let start = *self.held.get_or_insert(now);
            if !self.swapped && now - start > LONG_PRESS {
                self.right = !self.right;
                self.swapped = true;
                if self.persist {
                    storage::save("config-button", if self.right { "right" } else { "left" });
                }
            }
        }
        if response.clicked() && !self.swapped {
            self.open = !self.open;
        }
        if !down {
            self.held = None;
            self.swapped = false;
        }

        // Three slider rails with knobs, on a translucent disc.
        let p = ui.painter();
        let c = rect.center();
        let lit = self.open || down;
        p.circle_filled(c, BUTTON * 0.5, Color32::from_black_alpha(if lit { 170 } else { 90 }));
        let ink = Color32::from_white_alpha(if lit { 230 } else { 120 });
        for (i, knob) in [(-1.0, 0.35), (0.0, -0.3), (1.0, 0.1)] {
            let y = c.y + i * 7.0;
            p.line_segment([Pos2::new(c.x - 11.0, y), Pos2::new(c.x + 11.0, y)], egui::Stroke::new(1.6, ink));
            p.circle_filled(Pos2::new(c.x + knob * 11.0, y), 2.8, ink);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tune::{Schema, Tunable, Tuner};
    use crate::{Frame, Shapes};
    use egui::{Event, PointerButton, RawInput};

    #[derive(Clone, Default, PartialEq, Debug)]
    struct Knobs {
        a: f32,
        b: i32,
        c: bool,
    }

    impl Tunable for Knobs {
        const KEY: &'static str = "panel-test";
        fn schema(s: &mut Schema<Self>) {
            s.float("a", "A", "", 0.0..=10.0, 0.5, |k| &mut k.a).int("b", "B", "", 0..=9, |k| &mut k.b);
            s.toggle("c", "C", |k| &mut k.c);
        }
    }

    struct Game {
        tuner: Tuner<Knobs>,
    }

    impl App for Game {
        fn frame(&mut self, _: &Frame, _: &mut Shapes) {}
        fn config(&mut self) -> Option<&mut dyn crate::tune::Config> {
            Some(&mut self.tuner)
        }
    }

    /// Drives the overlay through egui alone, a frame per call.
    struct Rig {
        ctx: Context,
        panel: Panel,
        game: Game,
        time: f64,
    }

    impl Rig {
        fn new() -> Self {
            let ctx = Context::default();
            Panel::style(&ctx);
            let mut rig = Self { ctx, panel: Panel::unsaved(), game: Game { tuner: Tuner::unsaved() }, time: 0.0 };
            rig.frame(vec![]);
            rig.frame(vec![]);
            rig
        }

        fn frame(&mut self, events: Vec<Event>) {
            self.time += 1.0 / 60.0;
            let raw = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(390.0, 800.0))),
                time: Some(self.time),
                events,
                ..Default::default()
            };
            let (panel, game) = (&mut self.panel, &mut self.game);
            let mut out = self.ctx.run_ui(raw, |ui| panel.show(ui.ctx(), game));
            out.textures_delta.clear();
        }

        fn press(pos: Pos2, pressed: bool) -> Event {
            Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Default::default() }
        }

        /// A tap spread over frames, as a finger makes it.
        fn tap(&mut self, pos: Pos2) {
            self.frame(vec![Event::PointerMoved(pos), Self::press(pos, true)]);
            self.frame(vec![]);
            self.frame(vec![Self::press(pos, false), Event::PointerGone]);
            self.frame(vec![]);
        }

        fn button(&self) -> Pos2 {
            self.panel.zones.last().unwrap().center()
        }
    }

    #[test]
    fn the_button_toggles_the_panel_every_time() {
        let mut rig = Rig::new();
        for i in 0..20 {
            let at = rig.button();
            rig.tap(at);
            assert_eq!(rig.panel.open, i % 2 == 0, "tap {i}");
        }
    }

    #[test]
    fn a_press_and_release_in_one_frame_still_counts() {
        let mut rig = Rig::new();
        let at = rig.button();
        rig.frame(vec![Event::PointerMoved(at), Rig::press(at, true), Rig::press(at, false)]);
        assert!(rig.panel.open);
    }

    #[test]
    fn a_long_press_moves_the_button_and_does_not_toggle() {
        let mut rig = Rig::new();
        let at = rig.button();
        rig.frame(vec![Event::PointerMoved(at), Rig::press(at, true)]);
        for _ in 0..45 {
            rig.frame(vec![]);
        }
        rig.frame(vec![Rig::press(at, false), Event::PointerGone]);
        rig.frame(vec![]);
        assert!(!rig.panel.right);
        assert!(!rig.panel.open);
        assert!(rig.button().x < 100.0, "now bottom left");
        let at = rig.button();
        rig.tap(at);
        assert!(rig.panel.open, "and still taps open from there");
    }

    #[test]
    fn the_panel_keeps_answering_after_many_tweaks() {
        let mut rig = Rig::new();
        let at = rig.button();
        rig.tap(at);
        // Tap and drag all over the field: buttons, sliders, toggles, pins.
        for y in (60..520).step_by(14) {
            for x in (20..380).step_by(45) {
                let p = Pos2::new(x as f32, y as f32);
                rig.tap(p);
            }
            let (from, to) = (Pos2::new(40.0, y as f32), Pos2::new(340.0, y as f32));
            rig.frame(vec![Event::PointerMoved(from), Rig::press(from, true)]);
            rig.frame(vec![Event::PointerMoved(to)]);
            rig.frame(vec![Rig::press(to, false), Event::PointerGone]);
        }
        assert_ne!(rig.game.tuner.current, Knobs::default(), "the taps tweaked something");
        assert!(rig.panel.open);
        let at = rig.button();
        rig.tap(at);
        assert!(!rig.panel.open, "the button still shuts the panel");
    }

    #[test]
    fn shut_only_the_button_and_widget_take_presses() {
        let mut rig = Rig::new();
        assert!(rig.panel.takes(rig.button()));
        assert!(!rig.panel.takes(Pos2::new(200.0, 300.0)));
        let at = rig.button();
        rig.tap(at);
        assert!(rig.panel.takes(Pos2::new(200.0, 300.0)), "open, the panel is modal");
    }
}
