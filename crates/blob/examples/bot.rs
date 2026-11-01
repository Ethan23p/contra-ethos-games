//! A blind bot playing whole games, for tuning the rules before they reach a
//! phone: `cargo run --release -p blob --example bot -- m=2 nova_blobs=6`.
//! It launches at a random angle on a steady beat, on the shipped app's physics
//! (`apps/blob/src/config.rs`). Blind play won't match a person's score; compare
//! runs against each other.

use blob::{Event, Game, Physics, Rng, Rules};

const DT: f32 = 1.0 / 120.0;
/// Seconds between launches.
const BEAT: f32 = 0.7;
/// Games stop here if they haven't ended.
const LIMIT: f32 = 600.0;
const GAMES: u64 = 40;

/// A game's launches, novas, peak, most blobs, groups, pool gain and seconds.
type Row = (f32, f32, f32, f32, f32, f32, f32);

fn shipped() -> Physics {
    let radius = 24.5;
    Physics {
        m: 2,
        radius,
        sigma: 0.94 * 2.0 * radius / 1.122_462,
        epsilon: 295.0,
        max_force: 24_000.0,
        damping: 2.0,
        rest: 30.0,
        well: 2420.0,
        well_radius: 550.0,
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
        ..Physics::default()
    }
}

/// The opening played patiently: a launch every 3 s, two launches only. Then
/// what the nova leaves, 10 s on, untouched.
fn opening(p: &Physics, r: &Rules) {
    let (mut hits, mut most, mut blobs, mut peak) = (0, 0, 0, 0);
    let mut aim = Rng::new(9);
    for seed in 0..GAMES {
        let mut g = Game::new(p, r.clone(), seed);
        let mut novas = 0;
        for tick in 0..(16.0 / DT) as usize {
            if tick == (1.0 / DT) as usize || tick == (4.0 / DT) as usize {
                g.launch(p, aim.unit() as f32 * std::f32::consts::TAU);
            }
            g.step(p, DT);
            novas += g.take_events().iter().filter(|e| matches!(e, Event::Nova { .. })).count();
        }
        hits += (novas > 0) as u32;
        most = most.max(novas);
        blobs = blobs.max(g.field.blobs.len());
        peak = peak.max(g.peak());
    }
    println!(
        "opening: {hits}/{GAMES} novas in two launches, at most {most} in a row, then at most {blobs} blobs, peak {peak}"
    );
}

fn main() {
    let (mut p, mut r) = (shipped(), Rules::default());
    for arg in std::env::args().skip(1) {
        let Some((k, v)) = arg.split_once('=') else { continue };
        let v: f64 = v.parse().expect("number");
        match k {
            "m" => p.m = v as usize,
            "nova_blobs" => r.nova_blobs = v as usize,
            "interval" => r.interval = v as u32,
            "group_lever" => r.group_lever = v,
            "pool_lever" => r.pool_lever = v,
            "exclude" => r.exclude = v != 0.0,
            "group_nova" => r.group_nova = v != 0.0,
            _ => panic!("unknown rule {k}"),
        }
    }

    opening(&p, &r);
    let mut rows = Vec::new();
    for seed in 0..GAMES {
        let mut g = Game::new(&p, r.clone(), seed);
        let mut aim = Rng::new(seed ^ 0xb07);
        let (mut t, mut beat, mut launches, mut novas, mut peak, mut most, mut first) = (0.0, 0.0, 0, 0, 0, 0, None);
        while t < LIMIT && !g.over {
            g.step(&p, DT);
            t += DT;
            beat += DT;
            most = most.max(g.field.blobs.len());
            peak = peak.max(g.peak());
            novas += g.take_events().iter().filter(|e| matches!(e, Event::Nova { .. })).count();
            if first.is_none() && novas > 0 {
                first = Some(t);
            }
            if beat >= BEAT {
                beat = 0.0;
                launches += 1;
                g.launch(&p, aim.unit() as f32 * std::f32::consts::TAU);
            }
        }
        rows.push((launches as f32, novas as f32, peak as f32, most as f32, g.groups as f32, g.widened as f32, t));
        if seed == 0 {
            println!("first nova at {first:?} s");
        }
    }
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mid = |f: fn(&Row) -> f32| {
        let mut v: Vec<f32> = rows.iter().map(f).collect();
        v.sort_by(f32::total_cmp);
        v[v.len() / 2]
    };
    println!(
        "median of {GAMES}: {} launches, {} novas, peak {}, most blobs {}, {} groups, pool +{}, {:.0} s",
        mid(|r| r.0),
        mid(|r| r.1),
        mid(|r| r.2),
        mid(|r| r.3),
        mid(|r| r.4),
        mid(|r| r.5),
        mid(|r| r.6),
    );
}
