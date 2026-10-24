//! Every tunable: the rules (one field per spec constant) and the feel.
//! `Default` holds the shipped values; paste Ethan's "Copy values" here.

use dice::{Dir, Rules, Survivor};
use engine::{Schema, Tunable};

#[derive(Clone, PartialEq)]
pub struct Config {
    // Rules, mirroring `dice::Rules`.
    pub size: i32,
    pub base: f32,
    pub threshold: i32,
    pub score_per_mass: f32,
    pub gravity: bool,
    pub gravity_dir: usize,
    pub weight_1: f32,
    pub weight_2: f32,
    pub weight_3: f32,
    pub temperature: f32,
    pub max_value: i32,
    pub no_repeat: bool,
    pub one_pair: bool,
    pub queue_len: i32,
    pub oldest: bool,

    // Weight: durations and intensities scale with mass^this.
    pub mass_exponent: f32,
    // Rotation.
    pub rotate_ms: f32,
    pub rotate_max_ms: f32,
    pub spin_ms: f32,
    // Placing and gravity.
    pub place_ms: f32,
    pub hop_ms: f32,
    pub hop_max_ms: f32,
    pub settle_beat_ms: f32,
    // The merge's pulse train: dice leave for the survivor one at a time and
    // each landing sends out a ring.
    pub beat_ms: f32,
    pub speed_up: f32,
    pub build: f32,
    pub ring_ms: f32,
    pub ring_travel: f32,
    pub ring_width: f32,
    pub beat_scale: f32,
    // The merged die's pop.
    pub pop_ms: f32,
    pub pop_base: f32,
    pub pop_per_impact: f32,
    pub pop_cap: f32,
    pub glow: f32,
    // Dragging.
    pub lift: f32,
    pub lift_ms: f32,
    pub snap_ms: f32,
    pub snap_bounce: f32,
    pub shake: f32,
    pub shake_ms: f32,
    pub entrance_scale: f32,
    pub entrance_ms: f32,
    // Look.
    pub die_size: f32,
    pub die_corner: f32,
    pub pip_size: f32,
}

impl Default for Config {
    fn default() -> Self {
        let rules = Rules::default();
        Self {
            size: rules.size as i32,
            base: rules.base as f32,
            threshold: rules.threshold as i32,
            score_per_mass: rules.score_per_mass as f32,
            gravity: false,
            gravity_dir: 0,
            weight_1: rules.size_weights[0] as f32,
            weight_2: rules.size_weights[1] as f32,
            weight_3: rules.size_weights[2] as f32,
            temperature: rules.temperature as f32,
            max_value: rules.max_value as i32,
            no_repeat: rules.no_repeat,
            one_pair: rules.one_pair,
            queue_len: rules.queue_len as i32,
            oldest: false,

            mass_exponent: 0.5,
            rotate_ms: 60.0,
            rotate_max_ms: 200.0,
            spin_ms: 420.0,
            place_ms: 0.0,
            hop_ms: 170.0,
            hop_max_ms: 260.0,
            settle_beat_ms: 70.0,
            beat_ms: 110.0,
            speed_up: 10.0,
            build: 60.0,
            ring_ms: 380.0,
            ring_travel: 5.5,
            ring_width: 0.75,
            beat_scale: 1.1,
            pop_ms: 360.0,
            pop_base: 1.15,
            pop_per_impact: 0.09,
            pop_cap: 1.6,
            glow: 2.0,
            lift: 2.0,
            lift_ms: 100.0,
            snap_ms: 440.0,
            snap_bounce: 0.4,
            shake: 1.0,
            shake_ms: 220.0,
            entrance_scale: 0.85,
            entrance_ms: 160.0,
            die_size: 0.92,
            die_corner: 0.16,
            pip_size: 0.125,
        }
    }
}

impl Tunable for Config {
    const KEY: &'static str = "dice";

    fn schema(s: &mut Schema<Self>) {
        s.group("Rules")
            .int("size", "Board size (restarts)", "N", 4..=6, |c| &mut c.size)
            .float("base", "Mass base b", "×", 1.1..=6.0, 0.1, |c| &mut c.base)
            .int("threshold", "Merge threshold m", "dice", 2..=8, |c| &mut c.threshold)
            .float("score_per_mass", "Score per mass", "", 0.0..=100.0, 1.0, |c| &mut c.score_per_mass)
            .toggle("oldest", "Merge into the oldest die (experiment)", |c| &mut c.oldest)
            .int("queue_len", "Queue length (restarts)", "", 1..=4, |c| &mut c.queue_len);
        s.group("Gravity").toggle("gravity", "Gravity", |c| &mut c.gravity).choice(
            "gravity_dir",
            "Direction",
            &["Down", "Up", "Left", "Right"],
            |c| &mut c.gravity_dir,
        );
        s.group("Spawn")
            .float("weight_1", "Weight: 1 die", "", 0.0..=100.0, 1.0, |c| &mut c.weight_1)
            .float("weight_2", "Weight: 2 dice", "", 0.0..=100.0, 1.0, |c| &mut c.weight_2)
            .float("weight_3", "Weight: 3 dice", "", 0.0..=100.0, 1.0, |c| &mut c.weight_3)
            .float("temperature", "Rarity temperature T", "", 0.1..=20.0, 0.1, |c| &mut c.temperature)
            .int("max_value", "Highest spawned value", "", 2..=12, |c| &mut c.max_value)
            .toggle("no_repeat", "No repeated value in a cluster", |c| &mut c.no_repeat)
            .toggle("one_pair", "3 dice: exactly one pair", |c| &mut c.one_pair);
        // Durations scale with the dice's mass, which grows exponentially with
        // value, so each mass-scaled duration has a cap: without one a value-12
        // piece would take over a second to turn.
        s.group("Weight")
            .float("mass_exponent", "Mass → feel exponent", "", -1.0..=2.0, 0.05, |c| &mut c.mass_exponent);
        s.group("Rotation")
            .float("rotate_ms", "Quarter turn, lightest piece", "ms", 0.0..=1000.0, 10.0, |c| &mut c.rotate_ms)
            .float("rotate_max_ms", "Quarter turn cap (heavy pieces)", "ms", 0.0..=2000.0, 10.0, |c| {
                &mut c.rotate_max_ms
            })
            .float("spin_ms", "Single-die spin", "ms", 0.0..=2000.0, 10.0, |c| &mut c.spin_ms);
        s.group("Place and fall")
            .float("place_ms", "Drop settle", "ms", 0.0..=1000.0, 10.0, |c| &mut c.place_ms)
            .float("hop_ms", "Hop per cell, lightest die", "ms", 0.0..=1000.0, 10.0, |c| &mut c.hop_ms)
            .float("hop_max_ms", "Hop per cell cap (heavy dice)", "ms", 0.0..=2000.0, 10.0, |c| &mut c.hop_max_ms)
            .float("settle_beat_ms", "Pause between waves", "ms", 0.0..=1000.0, 10.0, |c| &mut c.settle_beat_ms);
        s.group("Merge pulse")
            .float("beat_ms", "Time between departures", "ms", 10.0..=600.0, 5.0, |c| &mut c.beat_ms)
            .float("speed_up", "Speed-up per beat", "%", 0.0..=60.0, 1.0, |c| &mut c.speed_up)
            .float("build", "First ring strength", "%", 0.0..=100.0, 5.0, |c| &mut c.build)
            .float("ring_ms", "Ring lifetime", "ms", 40.0..=2000.0, 10.0, |c| &mut c.ring_ms)
            .float("ring_travel", "Ring travel", "u", 0.0..=30.0, 0.25, |c| &mut c.ring_travel)
            .float("ring_width", "Ring width", "u", 0.1..=4.0, 0.05, |c| &mut c.ring_width)
            .float("beat_scale", "Survivor beat scale", "×", 1.0..=2.0, 0.01, |c| &mut c.beat_scale);
        s.group("Merge pop")
            .float("pop_ms", "Pop duration", "ms", 0.0..=2000.0, 10.0, |c| &mut c.pop_ms)
            .float("pop_base", "Pop scale", "×", 0.5..=3.0, 0.05, |c| &mut c.pop_base)
            .float("pop_per_impact", "Pop scale per impact", "×", 0.0..=1.0, 0.01, |c| &mut c.pop_per_impact)
            .float("pop_cap", "Pop scale cap", "×", 1.0..=4.0, 0.05, |c| &mut c.pop_cap)
            .float("glow", "Pop glow", "u", 0.0..=12.0, 0.25, |c| &mut c.glow);
        s.group("Drag")
            .float("lift", "Lift above finger", "cells", 0.0..=4.0, 0.1, |c| &mut c.lift)
            .float("lift_ms", "Lift duration", "ms", 0.0..=1000.0, 10.0, |c| &mut c.lift_ms)
            .float("snap_ms", "Snapback time", "ms", 50.0..=2000.0, 10.0, |c| &mut c.snap_ms)
            .float("snap_bounce", "Snapback bounce", "", 0.0..=1.0, 0.05, |c| &mut c.snap_bounce)
            .float("shake", "Rejected shake", "u", 0.0..=6.0, 0.1, |c| &mut c.shake)
            .float("shake_ms", "Shake duration", "ms", 0.0..=1000.0, 10.0, |c| &mut c.shake_ms)
            .float("entrance_scale", "New piece start scale", "×", 0.0..=2.0, 0.01, |c| &mut c.entrance_scale)
            .float("entrance_ms", "New piece entrance", "ms", 0.0..=1000.0, 10.0, |c| &mut c.entrance_ms);
        s.group("Look")
            .float("die_size", "Die size", "× cell", 0.5..=1.0, 0.01, |c| &mut c.die_size)
            .float("die_corner", "Die corner", "× die", 0.0..=0.5, 0.01, |c| &mut c.die_corner)
            .float("pip_size", "Pip size", "× die", 0.05..=0.3, 0.005, |c| &mut c.pip_size);
    }
}

impl Config {
    pub fn rules(&self) -> Rules {
        Rules {
            size: self.size as usize,
            base: self.base as f64,
            threshold: self.threshold as usize,
            score_per_mass: self.score_per_mass as f64,
            gravity: self.gravity.then(|| Dir::ALL[self.gravity_dir.min(3)]),
            size_weights: [self.weight_1 as f64, self.weight_2 as f64, self.weight_3 as f64],
            temperature: self.temperature as f64,
            max_value: self.max_value as u32,
            no_repeat: self.no_repeat,
            one_pair: self.one_pair,
            queue_len: self.queue_len as usize,
            survivor: if self.oldest { Survivor::Oldest } else { Survivor::Youngest },
        }
    }

    /// The one place mass turns into duration or intensity: 1 for a value-1 die.
    pub fn weight(&self, value: u32) -> f32 {
        (self.base.powi(value as i32 - 1)).powf(self.mass_exponent)
    }

    /// The snapback spring's stiffness and damping, from its settle time and
    /// bounce: critically damped at bounce 0, wobbling for a while at 1. Never
    /// stuck oscillating, whatever the sliders say.
    pub fn snap_spring(&self) -> (f32, f32) {
        let omega = 5.8 / (self.snap_ms / 1000.0).max(0.05);
        let zeta = 1.0 - 0.95 * self.snap_bounce.clamp(0.0, 1.0);
        (omega * omega, 2.0 * zeta * omega)
    }

    /// Seconds per cell of travel for a die of `value`.
    pub fn hop(&self, value: u32) -> f32 {
        (self.hop_ms * self.weight(value)).min(self.hop_max_ms) / 1000.0
    }
}
