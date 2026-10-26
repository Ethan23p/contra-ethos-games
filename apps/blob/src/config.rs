//! Every tunable, mirroring `blob::Physics`, except that pair spacing is set
//! relative to the blob, so the radius can change without the blobs
//! overlapping. `Default` holds the shipped values; paste Ethan's "Copy
//! values" here. "Standard" is `Physics::default()`, the old build's fluid;
//! "Spec fluid" is that tuned for the spec's launch; the rest are the old
//! build's other fluids, converted from its per-16 ms units: they set the
//! fluid, not the well or the launch.

use blob::{Physics, Rules};
use engine::{Schema, Tunable};

#[derive(Clone, PartialEq)]
pub struct Config {
    /// 0: bonded (m = 3), 1: unbonded (m = 2).
    pub mode: usize,
    pub radius: f32,
    /// Where a pair rests, centre to centre, × the diameter; sets σ.
    pub spacing: f32,
    pub epsilon: f32,
    pub cutoff: f32,
    pub max_force: f32,
    pub damping: f32,
    pub well: f32,
    pub well_radius: f32,
    pub centre_follow: f32,
    pub spring: f32,
    pub bond_rest: f32,
    pub touch: f32,
    pub merge_delay: f32,
    pub delay_clump: bool,
    pub chain_delay: f32,
    pub merge_hold: f32,
    pub merge_secs: f32,
    pub merge_push: f32,
    pub ring: f32,
    pub rim: f32,
    pub launch_reach: f32,
    pub max_speed: f32,
    pub substeps: i32,
    pub show_bonds: bool,
    pub ceiling: i32,
    pub window: i32,
    pub widen_every: i32,
    pub falloff: f32,
    pub queue: i32,
    pub combo_launches: i32,
    pub nova_bonus: i32,
    pub nova_blobs: i32,
    pub grace: f32,
    /// Drawn size × the physical radius: touching blobs of different kinds
    /// show a gap.
    pub drawn: f32,
    /// Look: how far apart identical surfaces start to fuse, × radius.
    pub blend: f32,
    pub link_width: f32,
    pub highlight: f32,
    pub shading: f32,
    pub glimmer: bool,
    pub glimmer_period: f32,
    pub glimmer_strength: f32,
    pub sparkles: i32,
    pub glint_rate: f32,
    pub wobble: f32,
    pub droplets: i32,
}

/// Pairs rest at 2^(1/6) σ.
const REST: f32 = 1.122_462;

impl Default for Config {
    /// Ethan's first round on Spec fluid (2026-10-24), with the blobs grown to
    /// the old build's size against the ring: σ, ε and the force cap scaled
    /// with them so the fluid feels the same.
    fn default() -> Self {
        Self {
            mode: 1,
            radius: 24.5,
            spacing: 0.94,
            epsilon: 295.0,
            cutoff: 2.5,
            max_force: 23_500.0,
            damping: 2.0,
            well: 1570.0,
            well_radius: 760.0,
            centre_follow: 0.5,
            spring: 9.5,
            bond_rest: 1.05,
            touch: 1.2,
            merge_delay: 0.42,
            delay_clump: true,
            chain_delay: 0.3,
            merge_hold: 2.4,
            merge_secs: 0.28,
            merge_push: 150.0,
            ring: 195.0,
            rim: 120.0,
            launch_reach: 1.4,
            max_speed: 3000.0,
            substeps: 4,
            show_bonds: true,
            ..Self::from_physics(Physics::default())
        }
    }
}

impl Config {
    /// A config from a physics set, with the spec's rules.
    fn from_physics(p: Physics) -> Self {
        let r = Rules::default();
        Self {
            mode: if p.m == 2 { 1 } else { 0 },
            radius: p.radius,
            spacing: p.sigma * REST / (2.0 * p.radius),
            epsilon: p.epsilon,
            cutoff: p.cutoff,
            max_force: p.max_force,
            damping: p.damping,
            well: p.well,
            well_radius: p.well_radius,
            centre_follow: p.centre_follow,
            spring: p.spring,
            bond_rest: p.bond_rest,
            touch: p.touch,
            merge_delay: p.merge_delay,
            delay_clump: p.delay_clump,
            chain_delay: p.chain_delay,
            merge_hold: p.merge_hold,
            merge_secs: p.merge_secs,
            merge_push: p.merge_push,
            ring: p.ring,
            rim: p.rim,
            launch_reach: p.launch_reach,
            max_speed: p.max_speed,
            substeps: p.substeps as i32,
            show_bonds: true,
            drawn: 0.9,
            blend: 0.5,
            link_width: 0.34,
            highlight: 0.38,
            shading: 0.3,
            glimmer: true,
            glimmer_period: 3.2,
            glimmer_strength: 0.6,
            sparkles: 3,
            glint_rate: 0.6,
            wobble: 0.22,
            droplets: 7,
            ceiling: r.ceiling as i32,
            window: r.window as i32,
            widen_every: r.widen_every as i32,
            falloff: r.falloff as f32,
            queue: r.queue as i32,
            combo_launches: r.combo_launches as i32,
            nova_bonus: r.nova_bonus as i32,
            nova_blobs: r.nova_blobs as i32,
            grace: r.grace,
        }
    }

    pub fn rules(&self) -> Rules {
        Rules {
            ceiling: self.ceiling.max(1) as u32,
            window: self.window.max(1) as usize,
            widen_every: self.widen_every.max(1) as u32,
            falloff: self.falloff as f64,
            queue: self.queue.max(1) as usize,
            combo_launches: self.combo_launches.max(1) as usize,
            nova_bonus: self.nova_bonus.max(0) as u64,
            nova_blobs: self.nova_blobs.max(1) as usize,
            grace: self.grace,
        }
    }

    pub fn physics(&self) -> Physics {
        Physics {
            m: if self.mode == 1 { 2 } else { 3 },
            radius: self.radius,
            sigma: self.spacing * 2.0 * self.radius / REST,
            epsilon: self.epsilon,
            cutoff: self.cutoff,
            max_force: self.max_force,
            damping: self.damping,
            well: self.well,
            well_radius: self.well_radius,
            centre_follow: self.centre_follow,
            spring: self.spring,
            bond_rest: self.bond_rest,
            touch: self.touch,
            merge_delay: self.merge_delay,
            delay_clump: self.delay_clump,
            chain_delay: self.chain_delay,
            merge_hold: self.merge_hold,
            merge_secs: self.merge_secs,
            merge_push: self.merge_push,
            ring: self.ring,
            rim: self.rim,
            launch_reach: self.launch_reach,
            max_speed: self.max_speed,
            substeps: self.substeps.max(1) as u32,
        }
    }

    /// An old-build fluid preset, from its per-16 ms values.
    #[allow(clippy::too_many_arguments)]
    fn fluid(
        epsilon: f32,
        sigma: f32,
        cutoff: f32,
        max_force: f32,
        substeps: i32,
        keep: f32,
        spring: f32,
        merge: f32,
    ) -> Self {
        const FRAME: f32 = 0.016;
        let standard = Physics::default();
        Self {
            epsilon: epsilon / FRAME,
            spacing: sigma * REST / (2.0 * standard.radius),
            cutoff,
            max_force: max_force / FRAME,
            substeps,
            damping: -keep.ln() / FRAME,
            spring: spring / FRAME,
            merge_secs: merge,
            ..Self::from_physics(standard)
        }
    }
}

impl Tunable for Config {
    const KEY: &'static str = "blob";

    fn schema(s: &mut Schema<Self>) {
        s.group("Rules")
            .choice("mode", "Merge size m", &["Bonded (3)", "Unbonded (2)"], |c| &mut c.mode)
            .int("ceiling", "Pool ceiling at least", "", 1..=1024, |c| &mut c.ceiling)
            .int("window", "Pool holds at first", "values", 1..=12, |c| &mut c.window)
            .int("widen_every", "Pool widens every", "doublings", 1..=20, |c| &mut c.widen_every)
            .float("falloff", "Draw falloff per step up", "×", 0.5..=4.0, 0.01, |c| &mut c.falloff)
            .int("queue", "Queue length", "", 1..=6, |c| &mut c.queue)
            .int("combo_launches", "Combo counts over", "launches", 1..=20, |c| &mut c.combo_launches)
            .int("nova_bonus", "Nova bonus", "points", 0..=100_000, |c| &mut c.nova_bonus)
            .int("nova_blobs", "Nova seeds", "blobs", 1..=40, |c| &mut c.nova_blobs)
            .float("grace", "Grace outside the rim", "s", 0.5..=60.0, 0.1, |c| &mut c.grace);
        s.group("Cohesion (Lennard-Jones)")
            .float("epsilon", "Cohesion strength ε", "u²/s²", 0.0..=5000.0, 5.0, |c| &mut c.epsilon)
            .float("spacing", "Pair spacing", "× diameter", 0.5..=2.0, 0.01, |c| &mut c.spacing)
            .float("cutoff", "Range", "× σ", 1.0..=10.0, 0.1, |c| &mut c.cutoff)
            .float("max_force", "Force cap per pair", "u/s²", 1000.0..=2_000_000.0, 1000.0, |c| &mut c.max_force);
        s.group("Motion")
            .float("damping", "Damping", "1/s", 0.0..=60.0, 0.1, |c| &mut c.damping)
            .float("max_speed", "Speed cap", "u/s", 100.0..=50_000.0, 100.0, |c| &mut c.max_speed)
            .int("substeps", "Substeps per step", "", 1..=16, |c| &mut c.substeps);
        s.group("Well")
            .float("well", "Pull at full strength", "u/s²", 0.0..=5000.0, 10.0, |c| &mut c.well)
            .float("well_radius", "Distance where the pull saturates", "u", 10.0..=1500.0, 10.0, |c| &mut c.well_radius)
            .float("centre_follow", "Centre follows the blobs", "1/s", 0.0..=30.0, 0.1, |c| &mut c.centre_follow);
        s.group("Bonds")
            .float("spring", "Bond spring", "1/s²", 0.0..=500.0, 0.5, |c| &mut c.spring)
            .float("bond_rest", "Bond rest length", "× diameter", 0.5..=3.0, 0.01, |c| &mut c.bond_rest)
            .float("touch", "Touch distance", "× diameter", 0.8..=2.0, 0.01, |c| &mut c.touch)
            .toggle("show_bonds", "Show bonds", |c| &mut c.show_bonds);
        s.group("Merging")
            .float("merge_delay", "Delay after a launch lands", "s", 0.0..=2.0, 0.01, |c| &mut c.merge_delay)
            .toggle("delay_clump", "Delay holds the whole clump", |c| &mut c.delay_clump)
            .float("chain_delay", "Chain delay after a merge", "s", 0.0..=3.0, 0.01, |c| &mut c.chain_delay)
            .float("merge_hold", "Ricochet hold during a delay", "× touch", 1.0..=6.0, 0.05, |c| &mut c.merge_hold)
            .float("merge_secs", "Convergence time", "s", 0.02..=3.0, 0.01, |c| &mut c.merge_secs)
            .float("merge_push", "Push on neighbours", "u/s", 0.0..=3000.0, 10.0, |c| &mut c.merge_push);
        s.group("Field")
            .float("radius", "Blob radius", "u", 4.0..=60.0, 0.5, |c| &mut c.radius)
            .float("ring", "Launcher ring radius", "u", 100.0..=600.0, 5.0, |c| &mut c.ring)
            .float("rim", "Rim radius", "u", 50.0..=600.0, 5.0, |c| &mut c.rim)
            .float("launch_reach", "Launch reach", "× ring", 0.0..=3.0, 0.05, |c| &mut c.launch_reach);
        s.group("Look")
            .float("drawn", "Drawn size", "× radius", 0.5..=1.5, 0.01, |c| &mut c.drawn)
            .float("blend", "Goo blend", "× radius", 0.0..=2.0, 0.01, |c| &mut c.blend)
            .float("link_width", "Bond tether width", "× radius", 0.0..=1.0, 0.01, |c| &mut c.link_width)
            .float("highlight", "Highlight", "", 0.0..=1.0, 0.01, |c| &mut c.highlight)
            .float("shading", "Edge shading", "", 0.0..=1.5, 0.01, |c| &mut c.shading)
            .float("wobble", "Merge pop wobble", "× size", 0.0..=1.0, 0.01, |c| &mut c.wobble)
            .int("droplets", "Merge droplets", "", 0..=40, |c| &mut c.droplets);
        s.group("Glimmer (peak blobs)")
            .toggle("glimmer", "Glimmer", |c| &mut c.glimmer)
            .float("glimmer_period", "Sheen every", "s", 0.3..=20.0, 0.1, |c| &mut c.glimmer_period)
            .float("glimmer_strength", "Sheen and glint strength", "", 0.0..=1.0, 0.01, |c| &mut c.glimmer_strength)
            .int("sparkles", "Glints per blob", "", 0..=12, |c| &mut c.sparkles)
            .float("glint_rate", "Glint frequency", "per s", 0.05..=8.0, 0.05, |c| &mut c.glint_rate);
    }

    fn presets() -> Vec<(&'static str, Self)> {
        vec![
            ("Standard", Self::from_physics(Physics::default())),
            // Tuned for the spec's launch, where a launched blob feels the same
            // fluid as the clump: lighter damping so it visibly travels (about
            // 0.4 s to contact), a well that gathers strays in about a second,
            // and gentler caps. Measured with `cargo run -p blob --example feel`.
            (
                "Spec fluid",
                Self {
                    damping: 2.0,
                    well: 1200.0,
                    max_force: 20_000.0,
                    max_speed: 3000.0,
                    touch: 1.2,
                    launch_reach: 0.9,
                    ..Self::from_physics(Physics::default())
                },
            ),
            ("Light / Flowey", Self::fluid(1.2, 38.0, 2.0, 1200.0, 3, 0.94, 0.05, 0.16)),
            ("Thick Liquid", Self::fluid(9.0, 34.0, 3.5, 3000.0, 4, 0.72, 0.22, 0.34)),
            ("Clay Clumps", Self::fluid(18.0, 31.0, 1.8, 6000.0, 6, 0.55, 0.55, 0.5)),
        ]
    }
}
