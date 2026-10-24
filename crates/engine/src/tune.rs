//! Live tuning, one schema for every game. A game's config is a plain struct
//! whose `Default` is the shipped value of every tunable; the schema lists
//! which fields the config panel shows and how.
//!
//! A [`Tuner`] holds the **current** values the game reads (e.g.
//! `tuner.current.gravity`) and a list of **profiles**: named value sets.
//! "Shipped" (the code's `Default`) and the game's presets are built in; the
//! player adds their own, which are kept on the device. One profile can be the
//! **device default** (what this device starts with) and one can be marked
//! **to ship** (what Ethan intends as everyone's default; its values go into
//! the config's `Default` by hand). Any number of fields can be **pinned**:
//! pinned fields can be shown alone and stay reachable in a small floating
//! widget while the panel is shut. The panel's layout lives in `panel.rs`; the
//! cards and the option island are drawn here.
//!
//! Profiles, pins and both marks persist (localStorage on the web, a file
//! natively); the current values don't, so a reload starts from the device
//! default.

use egui::{Color32, RichText, Ui};

use crate::screen::EM;

/// Height of a touchable row in the panel.
const ROW: f32 = 1.875 * EM;

/// A config the panel can tune.
pub trait Tunable: Clone + Default + 'static {
    /// Names the saved profiles and pins, e.g. "dice". Change it to discard old saves.
    const KEY: &'static str;
    fn schema(s: &mut Schema<Self>);
    /// Built-in profiles offered besides Shipped.
    fn presets() -> Vec<(&'static str, Self)> {
        Vec::new()
    }
}

pub enum Control<C> {
    Float { get: fn(&mut C) -> &mut f32, min: f32, max: f32, step: f32 },
    Int { get: fn(&mut C) -> &mut i32, min: i32, max: i32 },
    Toggle { get: fn(&mut C) -> &mut bool },
    Choice { get: fn(&mut C) -> &mut usize, options: &'static [&'static str] },
}

pub struct Field<C> {
    /// Stable name in saves and copied text; match the struct field.
    pub id: &'static str,
    pub group: &'static str,
    pub label: &'static str,
    pub unit: &'static str,
    pub control: Control<C>,
}

impl<C> Field<C> {
    fn get(&self, c: &mut C) -> f64 {
        match &self.control {
            Control::Float { get, .. } => *get(c) as f64,
            Control::Int { get, .. } => *get(c) as f64,
            Control::Toggle { get } => *get(c) as u8 as f64,
            Control::Choice { get, .. } => *get(c) as f64,
        }
    }

    /// Sets from a saved or pasted number, clamped into range.
    fn set(&self, c: &mut C, v: f64) {
        if !v.is_finite() {
            return;
        }
        match &self.control {
            Control::Float { get, min, max, .. } => *get(c) = (v as f32).clamp(*min, *max),
            Control::Int { get, min, max } => *get(c) = (v.round() as i32).clamp(*min, *max),
            Control::Toggle { get } => *get(c) = v != 0.0,
            Control::Choice { get, options } => *get(c) = (v.max(0.0) as usize).min(options.len() - 1),
        }
    }

    /// The value as it reads in copied text: shortest form, no f32 noise.
    fn text(&self, c: &mut C) -> String {
        match &self.control {
            Control::Float { get, .. } => format!("{}", *get(c)),
            _ => format!("{}", self.get(c)),
        }
    }

    /// The value as the panel shows it, with its unit.
    fn show(&self, v: f64) -> String {
        let unit = if self.unit.is_empty() { String::new() } else { format!(" {}", self.unit) };
        match &self.control {
            Control::Float { step, .. } => {
                let decimals = if *step >= 1.0 { 0 } else { (-step.log10()).ceil() as usize };
                format!("{v:.decimals$}{unit}")
            }
            Control::Int { .. } => format!("{v}{unit}"),
            Control::Toggle { .. } => (if v != 0.0 { "On" } else { "Off" }).into(),
            Control::Choice { options, .. } => options[(v.max(0.0) as usize).min(options.len() - 1)].into(),
        }
    }
}

/// Builds a field list; fields join the most recent [`Schema::group`].
pub struct Schema<C> {
    fields: Vec<Field<C>>,
    group: &'static str,
}

impl<C> Schema<C> {
    pub fn group(&mut self, name: &'static str) -> &mut Self {
        self.group = name;
        self
    }

    pub fn float(
        &mut self,
        id: &'static str,
        label: &'static str,
        unit: &'static str,
        range: std::ops::RangeInclusive<f32>,
        step: f32,
        get: fn(&mut C) -> &mut f32,
    ) -> &mut Self {
        let control = Control::Float { get, min: *range.start(), max: *range.end(), step };
        self.push(id, label, unit, control)
    }

    pub fn int(
        &mut self,
        id: &'static str,
        label: &'static str,
        unit: &'static str,
        range: std::ops::RangeInclusive<i32>,
        get: fn(&mut C) -> &mut i32,
    ) -> &mut Self {
        self.push(id, label, unit, Control::Int { get, min: *range.start(), max: *range.end() })
    }

    pub fn toggle(&mut self, id: &'static str, label: &'static str, get: fn(&mut C) -> &mut bool) -> &mut Self {
        self.push(id, label, "", Control::Toggle { get })
    }

    pub fn choice(
        &mut self,
        id: &'static str,
        label: &'static str,
        options: &'static [&'static str],
        get: fn(&mut C) -> &mut usize,
    ) -> &mut Self {
        self.push(id, label, "", Control::Choice { get, options })
    }

    fn push(&mut self, id: &'static str, label: &'static str, unit: &'static str, control: Control<C>) -> &mut Self {
        self.fields.push(Field { id, group: self.group, label, unit, control });
        self
    }
}

pub struct Profile<C> {
    pub name: String,
    pub values: C,
    /// Shipped and the game's presets: always there, never overwritten.
    pub builtin: bool,
}

/// What the runner's panel needs from a game's tuner, whatever its config type.
pub trait Config {
    /// The field of parameters: a header, then the scrollable cards.
    fn field(&mut self, ui: &mut Ui);
    /// The option island: profiles and what to do with them.
    fn island(&mut self, ui: &mut Ui);
    /// The floating widget's contents: the pinned fields' cards.
    fn widget(&mut self, ui: &mut Ui);
    fn has_pins(&self) -> bool;
}

pub struct Tuner<C: Tunable> {
    pub current: C,
    fields: Vec<Field<C>>,
    profiles: Vec<Profile<C>>,
    selected: usize,
    pinned: Vec<bool>,
    pinned_only: bool,
    /// The profile loaded at startup on this device.
    startup: Option<String>,
    /// The profile Ethan means to ship as everyone's default.
    ship: Option<String>,
    picking: bool,
    persist: bool,
    notice: Option<(String, f64)>,
}

impl<C: Tunable> Default for Tuner<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: Tunable> Tuner<C> {
    /// Loads saved profiles and pins, and starts from the default profile.
    pub fn new() -> Self {
        let mut tuner = Self::unsaved();
        tuner.persist = true;
        if let Some(text) = storage::load(C::KEY) {
            tuner.load_store(&text);
        }
        tuner
    }

    /// Shipped values and built-in profiles only; saves nothing (tests, snapshots).
    pub fn unsaved() -> Self {
        let mut schema = Schema { fields: Vec::new(), group: "General" };
        C::schema(&mut schema);
        let mut profiles = vec![Profile { name: "Shipped".into(), values: C::default(), builtin: true }];
        for (name, values) in C::presets() {
            profiles.push(Profile { name: name.into(), values, builtin: true });
        }
        Self {
            current: C::default(),
            pinned: vec![false; schema.fields.len()],
            fields: schema.fields,
            profiles,
            selected: 0,
            pinned_only: false,
            startup: None,
            ship: None,
            picking: false,
            persist: false,
            notice: None,
        }
    }

    /// The current values as `id = value` lines.
    pub fn to_text(&mut self) -> String {
        values_text(&self.fields, &mut self.current)
    }

    /// Applies `id = value` lines to current; unknown ids and bad lines are skipped.
    pub fn apply_text(&mut self, text: &str) {
        apply_values(&self.fields, &mut self.current, text);
    }

    /// Pins, both marks and the player's profiles, as text.
    fn store_text(&mut self) -> String {
        let pins: Vec<&str> = self.fields.iter().zip(&self.pinned).filter(|(_, p)| **p).map(|(f, _)| f.id).collect();
        let mut text = format!("pins = {}\n", pins.join(", "));
        if let Some(name) = &self.startup {
            text += &format!("default = {name}\n");
        }
        if let Some(name) = &self.ship {
            text += &format!("ship = {name}\n");
        }
        for p in self.profiles.iter_mut().filter(|p| !p.builtin) {
            text += &format!("[profile {}]\n{}", p.name, values_text(&self.fields, &mut p.values));
        }
        text
    }

    fn load_store(&mut self, text: &str) {
        let mut profile: Option<usize> = None;
        for line in text.lines() {
            if let Some(name) = line.strip_prefix("[profile ").and_then(|l| l.strip_suffix(']')) {
                profile = Some(self.add_profile(name, C::default()));
                continue;
            }
            let Some((key, value)) = line.split_once('=') else { continue };
            let (key, value) = (key.trim(), value.trim());
            match profile {
                Some(i) => apply_values(&self.fields, &mut self.profiles[i].values, line),
                None if key == "pins" => {
                    for id in value.split(',').map(str::trim) {
                        if let Some(i) = self.fields.iter().position(|f| f.id == id) {
                            self.pinned[i] = true;
                        }
                    }
                }
                None if key == "default" => self.startup = Some(value.to_string()),
                None if key == "ship" => self.ship = Some(value.to_string()),
                None => {}
            }
        }
        if let Some(i) = self.startup.as_ref().and_then(|name| self.profiles.iter().position(|p| &p.name == name)) {
            self.selected = i;
            self.current = self.profiles[i].values.clone();
        }
    }

    fn save(&mut self) {
        if self.persist {
            let text = self.store_text();
            storage::save(C::KEY, &text);
        }
    }

    /// Adds a profile, or overwrites the player's own one of that name.
    /// Returns its index.
    fn add_profile(&mut self, name: &str, values: C) -> usize {
        if let Some(i) = self.profiles.iter().position(|p| p.name == name) {
            if !self.profiles[i].builtin {
                self.profiles[i].values = values;
            }
            return i;
        }
        self.profiles.push(Profile { name: name.into(), values, builtin: false });
        self.profiles.len() - 1
    }

    fn say(&mut self, ui: &Ui, text: impl Into<String>) {
        self.notice = Some((text.into(), ui.ctx().input(|i| i.time)));
    }

    /// Copies `from`'s values into `to`, every field or only the pinned ones.
    fn copy_values(fields: &[Field<C>], pinned: &[bool], from: &mut C, to: &mut C, only_pinned: bool) {
        for (f, &pin) in fields.iter().zip(pinned) {
            if pin || !only_pinned {
                let v = f.get(from);
                f.set(to, v);
            }
        }
    }
}

impl<C: Tunable> Config for Tuner<C> {
    fn field(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.heading("Config");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Copy values").clicked() {
                    let text = self.to_text();
                    ui.ctx().copy_text(text);
                    self.say(ui, "Current values copied.");
                }
                let any = self.has_pins();
                ui.add_enabled_ui(any || self.pinned_only, |ui| ui.toggle_value(&mut self.pinned_only, "Pinned only"));
            });
        });
        ui.add_space(4.0);

        let Self { fields, pinned, current, profiles, selected, pinned_only, .. } = self;
        let profile = &mut profiles[*selected];
        let mut pins_changed = false;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let mut group = "";
            for (i, field) in fields.iter().enumerate() {
                if *pinned_only && !pinned[i] {
                    continue;
                }
                if field.group != group {
                    group = field.group;
                    ui.add_space(6.0);
                    ui.label(RichText::new(group.to_uppercase()).small().color(DIM));
                }
                let out = card(ui, field, current, Some((&profile.name, &mut profile.values)), Some(&mut pinned[i]));
                pins_changed |= out.pin_toggled;
            }
            if *pinned_only && !pinned.iter().any(|&p| p) {
                ui.label(RichText::new("Nothing pinned.").color(DIM));
            }
            ui.add_space(8.0);
        });
        if pins_changed {
            if !self.has_pins() {
                self.pinned_only = false;
            }
            self.save();
        }
    }

    fn island(&mut self, ui: &mut Ui) {
        let width = ui.available_width();
        let mut dirty = false;

        // Profile selector: tap to open the list, pick one or start a new one.
        if self.picking {
            egui::Frame::new().fill(CARD).corner_radius(8.0).inner_margin(6.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                for i in 0..self.profiles.len() {
                    let p = &self.profiles[i];
                    let mut label = p.name.clone();
                    if self.startup.as_deref() == Some(p.name.as_str()) {
                        label += " · device default";
                    }
                    if self.ship.as_deref() == Some(p.name.as_str()) {
                        label += " · to ship";
                    }
                    if p.builtin {
                        label += " · built-in";
                    }
                    if ui.selectable_label(i == self.selected, label).clicked() {
                        self.selected = i;
                        self.picking = false;
                    }
                }
                if ui.button("New profile…").clicked() {
                    let suggested = format!("Profile {}", self.profiles.iter().filter(|p| !p.builtin).count() + 1);
                    if let Some(name) = ask_name(&suggested) {
                        let values = self.current.clone();
                        self.selected = self.add_profile(&name, values);
                        self.say(ui, format!("Made \"{name}\" from the current values."));
                        dirty = true;
                    }
                    self.picking = false;
                }
            });
        }
        let name = self.profiles[self.selected].name.clone();
        let builtin = self.profiles[self.selected].builtin;
        let marker = if self.picking { "close" } else { "change" };
        if ui.add(egui::Button::new(format!("Profile: {name}   ({marker})")).min_size([width, ROW].into())).clicked() {
            self.picking = !self.picking;
        }

        let half = [(width - ui.spacing().item_spacing.x - 2.75 * EM) / 2.0, ROW];
        let Self { fields, pinned, current, profiles, selected, .. } = self;
        let profile = &mut profiles[*selected];
        let any_pins = pinned.iter().any(|&p| p);
        let mut note = None;
        ui.horizontal(|ui| {
            ui.add_sized([2.5 * EM, ROW], egui::Label::new("Load"));
            if ui.add(egui::Button::new("All").min_size(half.into())).clicked() {
                Self::copy_values(fields, pinned, &mut profile.values, current, false);
                note = Some(format!("Loaded \"{}\".", profile.name));
            }
            if ui.add_enabled(any_pins, egui::Button::new("Pinned").min_size(half.into())).clicked() {
                Self::copy_values(fields, pinned, &mut profile.values, current, true);
                note = Some(format!("Loaded the pinned values of \"{}\".", profile.name));
            }
        });
        ui.horizontal(|ui| {
            ui.add_sized([2.5 * EM, ROW], egui::Label::new("Save"));
            if ui.add_enabled(!builtin, egui::Button::new("All").min_size(half.into())).clicked() {
                Self::copy_values(fields, pinned, current, &mut profile.values, false);
                note = Some(format!("Saved into \"{}\".", profile.name));
                dirty = true;
            }
            if ui.add_enabled(!builtin && any_pins, egui::Button::new("Pinned").min_size(half.into())).clicked() {
                Self::copy_values(fields, pinned, current, &mut profile.values, true);
                note = Some(format!("Saved the pinned values into \"{}\".", profile.name));
                dirty = true;
            }
        });
        let half = [(width - ui.spacing().item_spacing.x) / 2.0, ROW];
        let is_startup = self.startup.as_deref() == Some(name.as_str());
        let is_ship = self.ship.as_deref() == Some(name.as_str());
        ui.horizontal(|ui| {
            let label = "Device default";
            if ui.add(egui::Button::new(label).selected(is_startup).min_size(half.into())).clicked() {
                self.startup = if is_startup { None } else { Some(name.clone()) };
                note = Some(if is_startup {
                    "This device starts from Shipped again.".into()
                } else {
                    format!("This device now starts from \"{name}\".")
                });
                dirty = true;
            }
            let label = if is_ship { "To ship (copy)" } else { "Mark to ship" };
            if ui.add(egui::Button::new(label).selected(is_ship).min_size(half.into())).clicked() {
                let text = values_text(&self.fields, &mut self.profiles[self.selected].values);
                ui.ctx().copy_text(text);
                self.ship = Some(name.clone());
                note = Some(format!(
                    "\"{name}\" is marked to ship and its values are copied: send them to Claude to make them everyone's default."
                ));
                dirty = true;
            }
        });
        if let Some(note) = note {
            self.say(ui, note);
        }

        if let Some((text, at)) = &self.notice
            && ui.ctx().input(|i| i.time) - at < 6.0
        {
            ui.add(egui::Label::new(RichText::new(text).small().color(DIM)).wrap());
        }
        if dirty {
            self.save();
        }
    }

    fn widget(&mut self, ui: &mut Ui) {
        let Self { fields, pinned, current, profiles, selected, .. } = self;
        let profile = &mut profiles[*selected];
        let mut pins_changed = false;
        egui::ScrollArea::vertical().max_height(ui.ctx().content_rect().height() * 0.5).show(ui, |ui| {
            for (i, field) in fields.iter().enumerate() {
                if pinned[i] {
                    let out =
                        card(ui, field, current, Some((&profile.name, &mut profile.values)), Some(&mut pinned[i]));
                    pins_changed |= out.pin_toggled;
                }
            }
        });
        if pins_changed {
            self.save();
        }
    }

    fn has_pins(&self) -> bool {
        self.pinned.iter().any(|&p| p)
    }
}

const CARD: Color32 = Color32::from_rgba_premultiplied(30, 30, 34, 235);
const DIM: Color32 = Color32::from_rgb(150, 145, 135);
const ACCENT: Color32 = Color32::from_rgb(91, 142, 242);

struct CardOut {
    pin_toggled: bool,
}

/// One parameter card: label, then the current value, the selected profile's
/// value in brackets (tap to use it) and the pin; the control, with its range.
fn card<C>(
    ui: &mut Ui,
    field: &Field<C>,
    current: &mut C,
    profile: Option<(&str, &mut C)>,
    pinned: Option<&mut bool>,
) -> CardOut {
    let mut out = CardOut { pin_toggled: false };
    egui::Frame::new().fill(CARD).corner_radius(10.0).inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            // Values first, from the right; the label takes what's left and
            // truncates rather than running under them.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(pinned) = pinned
                    && pin_button(ui, *pinned).clicked()
                {
                    *pinned = !*pinned;
                    out.pin_toggled = true;
                }
                // The selected profile's value in brackets; tap it to use it.
                if let Some((name, values)) = profile {
                    let theirs = field.get(values);
                    let differs = (field.get(current) - theirs).abs() > 1e-4 * theirs.abs().max(1.0);
                    let text = RichText::new(format!("({})", field.show(theirs))).color(DIM);
                    if differs {
                        let tip = format!("The value in {name}. Tap to use it.");
                        if ui.add(egui::Button::new(text).frame(false)).on_hover_text(tip).clicked() {
                            field.set(current, theirs);
                        }
                    } else {
                        ui.label(text).on_hover_text(format!("Same as {name}."));
                    }
                }
                ui.label(RichText::new(field.show(field.get(current))).color(ACCENT));
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add(egui::Label::new(RichText::new(field.label).strong()).truncate()).on_hover_text(field.label);
                });
            });
        });
        controls(ui, field, current);
    });
    ui.add_space(4.0);
    out
}

fn controls<C>(ui: &mut Ui, field: &Field<C>, current: &mut C) {
    let button = |ui: &mut Ui, text: &str| ui.add(egui::Button::new(text).min_size([ROW, ROW].into())).clicked();
    match &field.control {
        Control::Float { get, min, max, step } => {
            ui.horizontal(|ui| {
                let v = get(current);
                if button(ui, "-") {
                    *v = ((*v - step) / step).round() * step;
                }
                let (lo, hi) = (field.show(*min as f64), field.show(*max as f64));
                ui.label(RichText::new(lo.split(' ').next().unwrap_or("")).small().color(DIM));
                ui.spacing_mut().slider_width = (ui.available_width() - 5.5 * EM).max(4.0 * EM);
                ui.add(egui::Slider::new(v, *min..=*max).step_by(*step as f64).show_value(false));
                ui.label(RichText::new(hi.split(' ').next().unwrap_or("")).small().color(DIM));
                if button(ui, "+") {
                    *v = ((*v + step) / step).round() * step;
                }
                *v = v.clamp(*min, *max);
            });
        }
        Control::Int { get, min, max } => {
            ui.horizontal(|ui| {
                let v = get(current);
                if button(ui, "-") {
                    *v -= 1;
                }
                ui.label(RichText::new(min.to_string()).small().color(DIM));
                ui.spacing_mut().slider_width = (ui.available_width() - 5.5 * EM).max(4.0 * EM);
                ui.add(egui::Slider::new(v, *min..=*max).show_value(false));
                ui.label(RichText::new(max.to_string()).small().color(DIM));
                if button(ui, "+") {
                    *v += 1;
                }
                *v = (*v).clamp(*min, *max);
            });
        }
        Control::Toggle { get } => {
            ui.horizontal(|ui| {
                let v = get(current);
                ui.selectable_value(v, false, "Off");
                ui.selectable_value(v, true, "On");
            });
        }
        Control::Choice { get, options } => {
            ui.horizontal_wrapped(|ui| {
                let v = get(current);
                for (i, option) in options.iter().enumerate() {
                    ui.selectable_value(v, i, *option);
                }
            });
        }
    }
}

/// A thumbtack: filled when pinned.
fn pin_button(ui: &mut Ui, pinned: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size([ROW, ROW].into(), egui::Sense::click());
    let color = if pinned { ACCENT } else { DIM };
    let p = ui.painter();
    let c = rect.center();
    let head = c + egui::vec2(0.0, -4.0);
    if pinned {
        p.circle_filled(head, 5.0, color);
    } else {
        p.circle_stroke(head, 5.0, egui::Stroke::new(1.5, color));
    }
    p.line_segment([c + egui::vec2(0.0, 1.0), c + egui::vec2(0.0, 9.0)], egui::Stroke::new(1.8, color));
    response.on_hover_text(if pinned { "Unpin" } else { "Pin" })
}

/// A name for a new profile. On the web the canvas can't raise a phone's
/// keyboard, so this asks with the browser's own prompt.
fn ask_name(suggested: &str) -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        let name = web_sys::window()?.prompt_with_message_and_default("Profile name", suggested).ok()??;
        let name = name.trim().replace(['[', ']', '\n'], "");
        (!name.is_empty()).then_some(name)
    }
    #[cfg(not(target_arch = "wasm32"))]
    Some(suggested.to_string())
}

fn values_text<C>(fields: &[Field<C>], c: &mut C) -> String {
    fields.iter().map(|f| format!("{} = {}\n", f.id, f.text(c))).collect()
}

fn apply_values<C>(fields: &[Field<C>], c: &mut C, text: &str) {
    for line in text.lines() {
        let Some((id, value)) = line.split_once('=') else { continue };
        let (id, value) = (id.trim(), value.trim());
        if let (Some(field), Ok(v)) = (fields.iter().find(|f| f.id == id), value.parse::<f64>()) {
            field.set(c, v);
        }
    }
}

/// Small strings that outlive the page: localStorage on the web, a temp-dir
/// file natively. Tunables use it; games may too (a best score, say).
pub mod storage {
    #[cfg(target_arch = "wasm32")]
    fn local() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok()?
    }

    #[cfg(target_arch = "wasm32")]
    pub fn load(key: &str) -> Option<String> {
        local()?.get_item(&format!("contra-ethos:tune:{key}")).ok()?
    }

    #[cfg(target_arch = "wasm32")]
    pub fn save(key: &str, text: &str) {
        if let Some(s) = local() {
            let _ = s.set_item(&format!("contra-ethos:tune:{key}"), text);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn path(key: &str) -> std::path::PathBuf {
        std::env::temp_dir().join("contra-ethos").join(format!("{key}.tune"))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn load(key: &str) -> Option<String> {
        std::fs::read_to_string(path(key)).ok()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn save(key: &str, text: &str) {
        let path = path(key);
        let _ = std::fs::create_dir_all(path.parent().unwrap());
        let _ = std::fs::write(path, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    struct Config {
        speed: f32,
        size: i32,
        gravity: bool,
        direction: usize,
    }

    impl Default for Config {
        fn default() -> Self {
            Self { speed: 1.5, size: 5, gravity: false, direction: 1 }
        }
    }

    impl Tunable for Config {
        const KEY: &'static str = "test";
        fn schema(s: &mut Schema<Self>) {
            s.group("Motion").float("speed", "Speed", "px/s", 0.0..=10.0, 0.1, |c| &mut c.speed);
            s.group("Board").int("size", "Board size", "", 4..=6, |c| &mut c.size);
            s.toggle("gravity", "Gravity", |c| &mut c.gravity);
            s.choice("direction", "Direction", &["Up", "Down"], |c| &mut c.direction);
        }
        fn presets() -> Vec<(&'static str, Self)> {
            vec![("Fast", Self { speed: 9.0, ..Self::default() })]
        }
    }

    #[test]
    fn text_round_trips() {
        let mut t = Tuner::<Config>::unsaved();
        t.current = Config { speed: 2.25, size: 6, gravity: true, direction: 0 };
        let text = t.to_text();
        let mut other = Tuner::<Config>::unsaved();
        other.apply_text(&text);
        assert_eq!(other.current, t.current);
    }

    #[test]
    fn copied_floats_carry_no_f32_noise() {
        let mut t = Tuner::<Config>::unsaved();
        t.current.speed = 0.82;
        assert!(t.to_text().contains("speed = 0.82\n"), "{}", t.to_text());
    }

    #[test]
    fn pasted_values_are_clamped_and_junk_skipped() {
        let mut t = Tuner::<Config>::unsaved();
        t.apply_text("speed = 99\nsize = 1\nnonsense\nmissing = 3\ngravity = 1\ndirection = 7");
        assert_eq!(t.current, Config { speed: 10.0, size: 4, gravity: true, direction: 1 });
    }

    #[test]
    fn kept_profiles_pins_and_default_round_trip() {
        let mut t = Tuner::<Config>::unsaved();
        t.add_profile("Mine", Config { speed: 4.0, ..Config::default() });
        t.add_profile("Other", Config { speed: 6.0, ..Config::default() });
        t.pinned[0] = true;
        t.pinned[3] = true;
        t.startup = Some("Mine".into());
        t.ship = Some("Other".into());
        let text = t.store_text();
        assert!(!text.contains("[profile Shipped]"), "built-ins aren't saved");

        let mut back = Tuner::<Config>::unsaved();
        back.load_store(&text);
        assert_eq!(back.pinned, vec![true, false, false, true]);
        assert_eq!(back.current.speed, 4.0, "the default profile loads first");
        assert_eq!(back.profiles[back.selected].name, "Mine");
        assert_eq!(back.ship.as_deref(), Some("Other"));
        assert_eq!(
            back.profiles.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            ["Shipped", "Fast", "Mine", "Other"]
        );
    }

    #[test]
    fn builtin_profiles_are_never_overwritten() {
        let mut t = Tuner::<Config>::unsaved();
        let i = t.add_profile("Shipped", Config { speed: 7.0, ..Config::default() });
        assert_eq!(i, 0);
        assert_eq!(t.profiles[0].values.speed, 1.5);
    }

    #[test]
    fn loading_pinned_touches_only_pinned_fields() {
        let mut t = Tuner::<Config>::unsaved();
        t.pinned[0] = true;
        let mut from = Config { speed: 9.0, size: 6, ..Config::default() };
        Tuner::copy_values(&t.fields, &t.pinned, &mut from, &mut t.current, true);
        assert_eq!((t.current.speed, t.current.size), (9.0, 5));
    }
}
