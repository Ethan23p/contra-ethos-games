//! Every tunable, mirroring `blob::Physics`. `Default` holds the shipped
//! values; paste Ethan's "Copy values" here. "Spec fluid" is tuned for the
//! spec's launch; the rest are the old build's,
//! converted from its per-16 ms units: they set the fluid, not the well or the
//! launch.

use blob::Physics;
use engine::{Schema, Tunable};

#[derive(Clone, PartialEq)]
pub struct Config {
    /// 0: bonded (m = 3), 1: unbonded (m = 2).
    pub mode: usize,
    pub radius: f32,
    pub sigma: f32,
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
    pub merge_secs: f32,
    pub merge_push: f32,
    pub ring: f32,
    pub rim: f32,
    pub launch_reach: f32,
    pub max_speed: f32,
    pub substeps: i32,
    pub show_bonds: bool,
}

impl Default for Config {
    fn default() -> Self {
        let p = Physics::default();
        Self {
            mode: if p.m == 2 { 1 } else { 0 },
            radius: p.radius,
            sigma: p.sigma,
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
            merge_secs: p.merge_secs,
            merge_push: p.merge_push,
            ring: p.ring,
            rim: p.rim,
            launch_reach: p.launch_reach,
            max_speed: p.max_speed,
            substeps: p.substeps as i32,
            show_bonds: true,
        }
    }
}

impl Config {
    pub fn physics(&self) -> Physics {
        Physics {
            m: if self.mode == 1 { 2 } else { 3 },
            radius: self.radius,
            sigma: self.sigma,
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
        Self {
            epsilon: epsilon / FRAME,
            sigma,
            cutoff,
            max_force: max_force / FRAME,
            substeps,
            damping: -keep.ln() / FRAME,
            spring: spring / FRAME,
            merge_secs: merge,
            ..Self::default()
        }
    }
}

impl Tunable for Config {
    const KEY: &'static str = "blob";

    fn schema(s: &mut Schema<Self>) {
        s.group("Rules").choice("mode", "Merge size m", &["Bonded (3)", "Unbonded (2)"], |c| &mut c.mode);
        s.group("Cohesion (Lennard-Jones)")
            .float("epsilon", "Cohesion strength ε", "u²/s²", 0.0..=5000.0, 5.0, |c| &mut c.epsilon)
            .float("sigma", "Pair size σ (rest ≈ 1.12 σ)", "u", 5.0..=120.0, 0.5, |c| &mut c.sigma)
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
        s.group("Merging").float("merge_secs", "Convergence time", "s", 0.02..=3.0, 0.01, |c| &mut c.merge_secs).float(
            "merge_push",
            "Push on neighbours",
            "u/s",
            0.0..=3000.0,
            10.0,
            |c| &mut c.merge_push,
        );
        s.group("Field")
            .float("radius", "Blob radius", "u", 4.0..=60.0, 0.5, |c| &mut c.radius)
            .float("ring", "Launcher ring radius", "u", 100.0..=600.0, 5.0, |c| &mut c.ring)
            .float("rim", "Rim radius", "u", 50.0..=600.0, 5.0, |c| &mut c.rim)
            .float("launch_reach", "Launch reach", "× ring", 0.0..=3.0, 0.05, |c| &mut c.launch_reach);
    }

    fn presets() -> Vec<(&'static str, Self)> {
        vec![
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
                    ..Self::default()
                },
            ),
            ("Light / Flowey", Self::fluid(1.2, 38.0, 2.0, 1200.0, 3, 0.94, 0.05, 0.16)),
            ("Thick Liquid", Self::fluid(9.0, 34.0, 3.5, 3000.0, 4, 0.72, 0.22, 0.34)),
            ("Clay Clumps", Self::fluid(18.0, 31.0, 1.8, 6000.0, 6, 0.55, 0.55, 0.5)),
        ]
    }
}
