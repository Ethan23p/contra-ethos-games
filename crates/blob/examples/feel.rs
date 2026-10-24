//! Headless feel measurements for a physics set, for tuning presets before
//! they reach a phone: `cargo run -p blob --example feel -- damping=3 well=900`.
//! Any `Physics` field can be overridden as `name=value`.

use blob::{Field, Physics, Rng};
use glam::Vec2;

const DT: f32 = 1.0 / 120.0;

fn physics(args: &[String]) -> Physics {
    let mut p = Physics::default();
    for arg in args {
        let Some((k, v)) = arg.split_once('=') else { continue };
        let v: f32 = v.parse().expect("number");
        match k {
            "m" => p.m = v as usize,
            "radius" => p.radius = v,
            "sigma" => p.sigma = v,
            "epsilon" => p.epsilon = v,
            "cutoff" => p.cutoff = v,
            "max_force" => p.max_force = v,
            "damping" => p.damping = v,
            "well" => p.well = v,
            "well_radius" => p.well_radius = v,
            "centre_follow" => p.centre_follow = v,
            "spring" => p.spring = v,
            "bond_rest" => p.bond_rest = v,
            "touch" => p.touch = v,
            "merge_delay" => p.merge_delay = v,
            "delay_clump" => p.delay_clump = v != 0.0,
            "chain_delay" => p.chain_delay = v,
            "merge_hold" => p.merge_hold = v,
            "merge_secs" => p.merge_secs = v,
            "merge_push" => p.merge_push = v,
            "ring" => p.ring = v,
            "rim" => p.rim = v,
            "launch_reach" => p.launch_reach = v,
            "max_speed" => p.max_speed = v,
            "substeps" => p.substeps = v as u32,
            _ => panic!("unknown field {k}"),
        }
    }
    p
}

/// A seeded clump left to settle, so launches meet something at rest.
fn settled(p: &Physics, seed: u64) -> Field {
    let mut f = Field::new();
    f.seed(p, &mut Rng::new(seed));
    for _ in 0..240 {
        f.step(p, DT);
    }
    f.take_events();
    f
}

fn max_speed(f: &Field) -> f32 {
    f.blobs.iter().map(|b| b.vel.length()).fold(0.0, f32::max)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let p = physics(&args);
    println!("launch speed {:.0} u/s", p.launch_speed());

    // Settling: a fresh seed; the last moment anything moved faster than 5 u/s.
    let mut f = Field::new();
    f.seed(&p, &mut Rng::new(7));
    let mut t = 0.0;
    for i in 1..=600 {
        f.step(&p, DT);
        if max_speed(&f) > 5.0 {
            t = i as f32 * DT;
        }
    }
    println!("seed settles     {t:.2} s");

    // Launches from eight angles into a settled clump, with a value nothing
    // else has, so nothing merges.
    let (mut flight, mut impact, mut rebound, mut knock, mut spread) = (vec![], vec![], vec![], vec![], vec![]);
    for k in 0..8 {
        let mut f = settled(&p, 7);
        let angle = k as f32 * std::f32::consts::TAU / 8.0 + 0.2;
        let id = f.launch(&p, angle, 0, 1024);
        let start = f.blobs.iter().map(|b| (b.id, b.pos)).collect::<Vec<_>>();
        let contact = 2.0 * p.radius * p.touch;
        let mut t = 0.0;
        let mut hit: Option<(f32, f32, f32)> = None;
        let mut out = 0.0f32;
        let mut moved = 0.0f32;
        let mut far = 0.0f32;
        while t < 3.0 {
            f.step(&p, DT);
            t += DT;
            let me = f.blob(id).unwrap().clone();
            let r = (me.pos - f.centre).length();
            if hit.is_none() && f.blobs.iter().any(|b| b.id != id && b.pos.distance(me.pos) < contact) {
                hit = Some((t, me.vel.length(), r));
            }
            if let Some((_, _, r0)) = hit {
                out = out.max(r - r0);
            }
            for b in &f.blobs {
                if b.id == id {
                    continue;
                }
                if let Some((_, p0)) = start.iter().find(|(i, _)| *i == b.id) {
                    moved = moved.max(b.pos.distance(*p0));
                }
                far = far.max((b.pos - f.centre).length());
            }
        }
        if let Some((t, v, _)) = hit {
            flight.push(t);
            impact.push(v);
        }
        rebound.push(out);
        knock.push(moved);
        spread.push(far);
    }
    let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len().max(1) as f32;
    let max = |v: &[f32]| v.iter().copied().fold(0.0, f32::max);
    println!("flight to contact {:.2} s (of {} launches)", mean(&flight), flight.len());
    println!("impact speed     {:.0} u/s", mean(&impact));
    println!("bounce back      {:.0} u mean, {:.0} max", mean(&rebound), max(&rebound));
    println!("clump knocked    {:.0} u mean, {:.0} max", mean(&knock), max(&knock));
    println!("farthest blob    {:.0} u (rim {:.0})", max(&spread), p.rim);

    // A stray 200 out from a settled clump: time to rejoin.
    let mut f = settled(&p, 7);
    let id = f.add(0, 1024, f.centre + Vec2::new(200.0, 0.0));
    let mut t = 0.0;
    while t < 10.0 {
        f.step(&p, DT);
        t += DT;
        let me = f.blob(id).unwrap().pos;
        if f.blobs.iter().any(|b| b.id != id && b.pos.distance(me) < 2.0 * p.radius * p.touch) {
            break;
        }
    }
    println!("stray rejoins    {t:.2} s");

    // Two overlapping blobs, 10 apart: how far they fly.
    let mut f = Field::new();
    let a = f.add(0, 1, Vec2::new(-5.0, 0.0));
    f.add(1, 1, Vec2::new(5.0, 0.0));
    let mut far = 0.0f32;
    for _ in 0..240 {
        f.centre = Vec2::ZERO;
        f.step(&Physics { well: 0.0, ..p.clone() }, DT);
        far = far.max(f.blob(a).unwrap().pos.length());
    }
    println!("overlap fling    {far:.0} u");
}
