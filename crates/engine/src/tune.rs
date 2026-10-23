//! Live tuning, one schema for every game. A game's config is a plain struct
//! whose `Default` is the shipped (initial) value of every tunable; the schema
//! lists which fields the dev panel shows and how. A [`Tuner`] keeps three
//! copies, as the old dice Tuning panel did:
//!
//! - **initial**: the shipped values, fixed in code.
//! - **default**: the player's saved baseline.
//! - **current**: the live values the game reads, e.g. `tuner.current.gravity`.
//!
//! Default and current persist (localStorage on the web, a file natively). The
//! panel's Copy button puts the current values on the clipboard as text, so a
//! good set found on the phone can be pasted back into the code as new initials.

use egui::Ui;

/// A config the dev panel can tune.
pub trait Tunable: Clone + Default + 'static {
    /// Names the saved values, e.g. "dice". Change it to discard old saves.
    const KEY: &'static str;
    fn schema(s: &mut Schema<Self>);
    /// Named sets of values offered in the panel, besides Initial.
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

pub struct Tuner<C: Tunable> {
    pub initial: C,
    pub default: C,
    pub current: C,
    fields: Vec<Field<C>>,
    presets: Vec<(&'static str, C)>,
}

impl<C: Tunable> Default for Tuner<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: Tunable> Tuner<C> {
    /// Starts from the shipped values, then applies any saved default and current.
    pub fn new() -> Self {
        let mut tuner = Self::unsaved();
        if let Some(text) = storage::load(C::KEY) {
            tuner.load(&text);
        }
        tuner
    }

    /// Shipped values only, ignoring saves (tests, headless snapshots).
    pub fn unsaved() -> Self {
        let mut schema = Schema { fields: Vec::new(), group: "General" };
        C::schema(&mut schema);
        Self {
            initial: C::default(),
            default: C::default(),
            current: C::default(),
            fields: schema.fields,
            presets: C::presets(),
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

    fn save(&mut self) {
        let text = format!(
            "[default]\n{}[current]\n{}",
            values_text(&self.fields, &mut self.default),
            values_text(&self.fields, &mut self.current)
        );
        storage::save(C::KEY, &text);
    }

    fn load(&mut self, text: &str) {
        let (defaults, current) =
            text.strip_prefix("[default]\n").and_then(|rest| rest.split_once("[current]\n")).unwrap_or(("", text));
        apply_values(&self.fields, &mut self.default, defaults);
        self.current = self.default.clone();
        apply_values(&self.fields, &mut self.current, current);
    }

    /// The panel: global actions, then one collapsible section per group.
    pub fn ui(&mut self, ui: &mut Ui) {
        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            if ui.button("Copy values").on_hover_text("Current values as text").clicked() {
                let text = self.to_text();
                ui.ctx().copy_text(text);
            }
            if ui.button("Save as default").clicked() {
                self.default = self.current.clone();
                changed = true;
            }
            if ui.button("Reset to default").clicked() {
                self.current = self.default.clone();
                changed = true;
            }
            egui::ComboBox::from_id_salt("preset").selected_text("Presets").show_ui(ui, |ui| {
                if ui.selectable_label(false, "Initial (shipped)").clicked() {
                    self.current = self.initial.clone();
                    changed = true;
                }
                for (name, values) in &self.presets {
                    if ui.selectable_label(false, *name).clicked() {
                        self.current = values.clone();
                        changed = true;
                    }
                }
            });
        });
        ui.separator();

        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut i = 0;
            while i < self.fields.len() {
                let group = self.fields[i].group;
                let end = (i..self.fields.len()).find(|&j| self.fields[j].group != group).unwrap_or(self.fields.len());
                egui::CollapsingHeader::new(group).default_open(true).show(ui, |ui| {
                    for field in &self.fields[i..end] {
                        changed |= field_row(ui, field, &mut self.current, &mut self.default, &mut self.initial);
                    }
                });
                i = end;
            }
        });

        if changed {
            self.save();
        }
    }
}

/// One tunable: its control, a reset when it differs from the default, and a
/// dot when it differs from the shipped value. Returns whether it changed.
fn field_row<C>(ui: &mut Ui, field: &Field<C>, current: &mut C, default: &mut C, initial: &mut C) -> bool {
    // Sliders snap to their step, so compare loosely.
    let differs = |a: f64, b: f64| (a - b).abs() > 1e-4 * b.abs().max(1.0);
    let before = field.get(current);
    let off_default = differs(before, field.get(default));
    let off_initial = differs(before, field.get(initial));
    let mut label = field.label.to_string();
    if off_initial {
        label.push_str(" •");
    }
    // Label on its own line, control full width beneath: fits a phone.
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label).on_hover_text(field.id);
        if off_default && ui.small_button("reset").on_hover_text("Back to default").clicked() {
            let v = field.get(default);
            field.set(current, v);
            changed = true;
        }
    });
    // Room for the slider's value box beside it.
    ui.spacing_mut().slider_width = (ui.available_width() - 80.0).max(80.0);
    match &field.control {
        Control::Float { get, min, max, step } => {
            let slider = egui::Slider::new(get(current), *min..=*max).step_by(*step as f64).suffix(unit(field.unit));
            changed |= ui.add(slider).changed();
        }
        Control::Int { get, min, max } => {
            changed |= ui.add(egui::Slider::new(get(current), *min..=*max).suffix(unit(field.unit))).changed();
        }
        Control::Toggle { get } => {
            changed |= ui.checkbox(get(current), "on").changed();
        }
        Control::Choice { get, options } => {
            let value = get(current);
            egui::ComboBox::from_id_salt(field.id).selected_text(options[*value]).show_ui(ui, |ui| {
                for (i, option) in options.iter().enumerate() {
                    changed |= ui.selectable_value(value, i, *option).changed();
                }
            });
        }
    }
    ui.add_space(4.0);
    changed
}

fn unit(unit: &str) -> String {
    if unit.is_empty() { String::new() } else { format!(" {unit}") }
}

fn values_text<C>(fields: &[Field<C>], c: &mut C) -> String {
    fields.iter().map(|f| format!("{} = {}\n", f.id, f.get(c))).collect()
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
    fn pasted_values_are_clamped_and_junk_skipped() {
        let mut t = Tuner::<Config>::unsaved();
        t.apply_text("speed = 99\nsize = 1\nnonsense\nmissing = 3\ngravity = 1\ndirection = 7");
        assert_eq!(t.current, Config { speed: 10.0, size: 4, gravity: true, direction: 1 });
    }

    #[test]
    fn saves_keep_default_and_current_apart() {
        let mut t = Tuner::<Config>::unsaved();
        t.load("[default]\nspeed = 3\n[current]\nspeed = 4\n");
        assert_eq!((t.default.speed, t.current.speed), (3.0, 4.0));
        assert_eq!(t.initial.speed, 1.5);
    }
}
