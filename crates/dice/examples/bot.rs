//! A greedy bot playing whole games, for tuning the spawn rules:
//! `cargo run --release -p dice --example bot -- mature=0.2`. Each turn it takes
//! the placement that scores most, then the one that leaves the fewest dice.

use dice::{Game, Pos, Rules};

const GAMES: u64 = 60;
const LIMIT: usize = 3000;

/// The best placement's slot, turns and cell, if there's one.
fn best(g: &Game) -> Option<(usize, usize, Pos)> {
    let mut top: Option<(f64, usize, (usize, usize, Pos))> = None;
    for slot in 0..g.slots().len() {
        let mut turned = g.clone();
        for turns in 0..4 {
            for at in g.board().positions() {
                let mut trial = turned.clone();
                let Ok(turn) = trial.place(slot, 0, at) else { continue };
                let dice = trial.board().dice().count();
                if top.is_none_or(|(s, d, _)| turn.score > s || (turn.score == s && dice < d)) {
                    top = Some((turn.score, dice, (slot, turns, at)));
                }
            }
            turned.rotate(slot);
        }
    }
    top.map(|t| t.2)
}

fn main() {
    let mut r = Rules::default();
    for arg in std::env::args().skip(1) {
        let Some((k, v)) = arg.split_once('=') else { continue };
        let v: f64 = v.parse().expect("number");
        match k {
            "mature" => r.mature = v,
            "temperature" => r.temperature = v,
            _ => panic!("unknown rule {k}"),
        }
    }
    let mut rows = Vec::new();
    for seed in 0..GAMES {
        let mut g = Game::new(r.clone(), seed);
        let mut turns = 0;
        while !g.is_over() && turns < LIMIT {
            let Some((slot, rotate, at)) = best(&g) else { break };
            for _ in 0..rotate {
                g.rotate(slot);
            }
            g.place(slot, 0, at).unwrap();
            turns += 1;
        }
        let top = g.board().dice().map(|(_, d)| d.value).max().unwrap_or(0);
        rows.push((turns as f64, g.score(), top as f64));
    }
    let mid = |f: fn(&(f64, f64, f64)) -> f64| {
        let mut v: Vec<f64> = rows.iter().map(f).collect();
        v.sort_by(f64::total_cmp);
        v[v.len() / 2]
    };
    println!("median of {GAMES}: {} turns, score {}, top die {}", mid(|r| r.0), mid(|r| r.1), mid(|r| r.2));
}
