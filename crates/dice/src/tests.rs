//! One test group per spec policy, named after it.

use crate::cluster::{self, value_weights};
use crate::game::fits_anywhere;
use crate::rng::Rng;
use crate::*;

fn cluster(cells: &[(i32, i32, u32)]) -> Cluster {
    Cluster { cells: cells.iter().map(|&(x, y, v)| (Pos::new(x, y), v)).collect() }
}

fn single(v: u32) -> Cluster {
    cluster(&[(0, 0, v)])
}

/// A game on `rows` whose first slot holds `head`.
fn game(rules: Rules, rows: &[&str], head: Cluster) -> Game {
    let mut g = Game::with_board(rules, 7, Board::parse(rows));
    let mut slots = g.slots().to_vec();
    slots[0] = head;
    g.set_slots(slots);
    g
}

fn value_at(g: &Game, x: i32, y: i32) -> Option<u32> {
    g.board().get(Pos::new(x, y)).map(|d| d.value)
}

fn merges(turn: &Turn) -> Vec<&Vec<Merge>> {
    turn.steps.iter().filter_map(|s| if let Step::Merge(m) = s { Some(m) } else { None }).collect()
}

const EMPTY5: [&str; 5] = [". . . . .", ". . . . .", ". . . . .", ". . . . .", ". . . . ."];

// Mass law

#[test]
fn mass_law_is_b_to_the_v_minus_one() {
    let rules = Rules::default();
    assert_eq!([1, 2, 3, 4, 8].map(|v| rules.mass(v)), [1.0, 2.0, 4.0, 8.0, 128.0]);
    let rules = Rules { base: 3.0, ..Rules::default() };
    assert_eq!(rules.mass(3), 9.0);
}

// Legality

#[test]
fn legality_needs_every_cell_in_bounds_and_empty() {
    let g = game(
        Rules::default(),
        &[". . . . .", ". 1 . . .", ". . . . .", ". . . . .", ". . . . ."],
        cluster(&[(0, 0, 1), (1, 0, 2)]),
    );
    assert!(g.can_place(0, 0, Pos::new(2, 1)));
    assert!(!g.can_place(0, 0, Pos::new(0, 1)), "second cell is occupied");
    assert!(!g.can_place(0, 0, Pos::new(4, 0)), "second cell is off the board");
    assert!(g.can_place(0, 1, Pos::new(4, 0)), "anchored on the other cell it fits");
    assert!(!g.can_place(0, 0, Pos::new(-1, 0)));
    assert!(!g.can_place(0, 2, Pos::new(2, 2)), "no such cell");
}

#[test]
fn legality_rejects_an_illegal_place_without_changing_anything() {
    let mut g = game(Rules::default(), &EMPTY5, cluster(&[(0, 0, 1), (1, 0, 2)]));
    let before = (g.board().clone(), g.slots().to_vec());
    assert_eq!(g.place(0, 0, Pos::new(4, 4)), Err(PlaceError::Illegal));
    assert_eq!((g.board().clone(), g.slots().to_vec()), before);
}

#[test]
fn legality_rotation_is_free_and_clockwise() {
    let l = cluster(&[(0, 0, 1), (0, 1, 2), (1, 1, 3)]);
    let mut g = game(Rules::default(), &EMPTY5, l.clone());
    let before = g.board().clone();
    g.rotate(0);
    assert_eq!(g.slot(0), &cluster(&[(1, 0, 1), (0, 0, 2), (0, 1, 3)]));
    for _ in 0..3 {
        g.rotate(0);
    }
    assert_eq!(g.slot(0), &l, "four quarter-turns are the identity");
    assert_eq!(g.board(), &before);

    let domino = cluster(&[(0, 0, 1), (1, 0, 2)]);
    assert_eq!(domino.rotated(), cluster(&[(0, 0, 1), (0, 1, 2)]), "right becomes down");
}

// Threshold

#[test]
fn threshold_merges_at_m_and_not_below() {
    let mut g = game(Rules::default(), &[". . . . .", "1 . . . .", ". . . . .", ". . . . .", ". . . . ."], single(1));
    let turn = g.place(0, 0, Pos::new(1, 1)).unwrap();
    assert!(merges(&turn).is_empty(), "a pair stays");
    let mut slots = g.slots().to_vec();
    slots[0] = single(1);
    g.set_slots(slots);
    let turn = g.place(0, 0, Pos::new(2, 1)).unwrap();
    assert_eq!(merges(&turn).len(), 1, "three merge");
    assert_eq!(g.board().dice().count(), 1);
}

#[test]
fn threshold_consumes_the_whole_neighborhood() {
    let rows = [". . . . .", "2 2 . . .", "2 . . . .", "2 . . . .", ". . . . ."];
    let mut g = game(Rules::default(), &rows, single(2));
    let turn = g.place(0, 0, Pos::new(1, 2)).unwrap();
    let merge = &merges(&turn)[0][0];
    assert_eq!(merge.cells.len(), 5);
    assert_eq!(g.board().dice().count(), 1, "five dice became one");
    assert_eq!(value_at(&g, 1, 2), Some(3));
}

#[test]
fn threshold_m_is_tunable() {
    let rules = Rules { threshold: 4, ..Rules::default() };
    let mut g = game(rules, &[". . . . .", "1 1 . . .", ". . . . .", ". . . . .", ". . . . ."], single(1));
    let turn = g.place(0, 0, Pos::new(2, 1)).unwrap();
    assert!(merges(&turn).is_empty());
}

// Yield

#[test]
fn yield_releases_n_mass_minus_the_merged_mass() {
    let rules = Rules::default();
    assert_eq!(rules.released(3, 1), 1.0);
    assert_eq!(rules.released(5, 2), 6.0);
    assert_eq!(rules.released(3, 4), 8.0);

    let rows = [". . . . .", "2 2 . . .", "2 . . . .", "2 . . . .", ". . . . ."];
    let mut g = game(Rules::default(), &rows, single(2));
    let turn = g.place(0, 0, Pos::new(1, 2)).unwrap();
    assert_eq!(turn.score, 60.0, "released mass × 10");
    assert_eq!(g.score(), 60.0);
}

#[test]
fn yield_never_costs_score() {
    let rules = Rules { threshold: 2, ..Rules::default() };
    assert_eq!(rules.released(2, 3), 0.0, "2 · 4 − 8");
    let rules = Rules { threshold: 2, base: 3.0, ..Rules::default() };
    assert_eq!(rules.released(2, 1), 0.0, "2 · 1 − 3 clamps to 0");
    let mut g = game(rules, &[". . . . .", "1 . . . .", ". . . . .", ". . . . .", ". . . . ."], single(1));
    let turn = g.place(0, 0, Pos::new(1, 1)).unwrap();
    assert_eq!(merges(&turn).len(), 1);
    assert_eq!(g.score(), 0.0);
}

// Survivor

#[test]
fn survivor_place_ages_dice_furthest_first_and_anchor_last() {
    // An I-tromino anchored in the middle: the two ends are equally far, so
    // they share an age, and the anchor is younger than both.
    let mut g = game(Rules::default(), &EMPTY5, cluster(&[(0, 0, 5), (1, 0, 6), (2, 0, 7)]));
    g.place(0, 1, Pos::new(2, 2)).unwrap();
    let born = |x| g.board().get(Pos::new(x, 2)).unwrap().born;
    assert_eq!(born(1), born(3));
    assert!(born(2) > born(1));

    // An L anchored at one end: the far end, then the corner, then the anchor.
    let mut g = game(Rules::default(), &EMPTY5, cluster(&[(0, 0, 5), (0, 1, 6), (1, 1, 7)]));
    g.place(0, 0, Pos::new(1, 1)).unwrap();
    let born = |x, y| g.board().get(Pos::new(x, y)).unwrap().born;
    assert!(born(2, 2) < born(1, 2) && born(1, 2) < born(1, 1));
}

#[test]
fn survivor_is_the_youngest_die_and_keeps_its_age() {
    // A domino of 1s anchored on its right cell, beside an older 1.
    let mut g = game(
        Rules::default(),
        &[". . . . .", "1 . . . .", ". . . . .", ". . . . .", ". . . . ."],
        cluster(&[(0, 0, 1), (1, 0, 1)]),
    );
    let turn = g.place(0, 1, Pos::new(2, 1)).unwrap();
    let m = &merges(&turn)[0][0];
    assert_eq!(m.survivor, Pos::new(2, 1), "the anchor is the youngest");
    let placed_anchor = match &turn.steps[0] {
        Step::Place(dice) => dice.iter().find(|(p, _)| *p == Pos::new(2, 1)).unwrap().1,
        _ => unreachable!(),
    };
    assert_eq!(m.die.born, placed_anchor.born);
    assert_eq!(value_at(&g, 2, 1), Some(2));
}

#[test]
fn survivor_equal_ages_are_a_coin_flip() {
    // Three 1s of equal age merge when an unrelated die lands.
    let rows = ["1 1 1 . .", ". . . . .", ". . . . .", ". . . . .", ". . . . ."];
    let mut landed = [0; 3];
    for seed in 0..300 {
        let mut g = Game::with_board(Rules::default(), seed, Board::parse(&rows));
        let mut slots = g.slots().to_vec();
        slots[0] = single(5);
        g.set_slots(slots);
        let turn = g.place(0, 0, Pos::new(4, 4)).unwrap();
        landed[merges(&turn)[0][0].survivor.x as usize] += 1;
    }
    for (x, &n) in landed.iter().enumerate() {
        assert!(n > 60, "cell {x} won {n} of 300: {landed:?}");
    }
}

#[test]
fn survivor_oldest_experiment() {
    let rules = Rules { survivor: Survivor::Oldest, ..Rules::default() };
    let mut g = game(
        rules,
        &[". . . . .", "1 . . . .", ". . . . .", ". . . . .", ". . . . ."],
        cluster(&[(0, 0, 1), (1, 0, 1)]),
    );
    let turn = g.place(0, 1, Pos::new(2, 1)).unwrap();
    assert_eq!(merges(&turn)[0][0].survivor, Pos::new(0, 1));
}

// Settling

#[test]
fn settling_chains_to_a_fixed_point() {
    // The new 1 completes three 1s; their survivor, now a 2, completes three 2s.
    let rows = ["2 2 . . .", ". 1 1 . .", ". . . . .", ". . . . .", ". . . . ."];
    let mut g = game(Rules::default(), &rows, single(1));
    let turn = g.place(0, 0, Pos::new(2, 0)).unwrap();
    let gens = merges(&turn);
    assert_eq!(gens.len(), 2);
    assert_eq!(gens[0][0].die.value, 2);
    assert_eq!(gens[1][0].die.value, 3);
    assert_eq!(value_at(&g, 2, 0), Some(3));
    assert_eq!(g.board().dice().count(), 1);
    assert_eq!(turn.score, 10.0 + 20.0);
}

#[test]
fn settling_merges_every_qualifying_neighborhood_at_once() {
    let rows = ["1 1 . . .", ". . . . .", "3 3 . . .", ". . . . .", ". . . . ."];
    let mut g = game(Rules::default(), &rows, cluster(&[(0, 0, 1), (0, 1, 5), (0, 2, 3)]));
    let turn = g.place(0, 1, Pos::new(2, 1)).unwrap();
    let gens = merges(&turn);
    assert_eq!(gens.len(), 1, "one generation");
    assert_eq!(gens[0].len(), 2, "both neighborhoods in it");
}

#[test]
fn settling_reads_a_frozen_board() {
    // A vertical domino completes three 1s and three 2s at once. The 1s' new 2
    // lands beside the 2s, but the generation already read their neighborhood,
    // so it doesn't join it.
    let rows = ["1 1 . . .", ". . . 2 2", ". . . . .", ". . . . .", ". . . . ."];
    let mut g = game(Rules::default(), &rows, cluster(&[(0, 0, 1), (0, 1, 2)]));
    let turn = g.place(0, 1, Pos::new(2, 1)).unwrap();
    let gens = merges(&turn);
    assert_eq!(gens.len(), 1);
    assert!(gens[0].iter().all(|m| m.cells.len() == 3));
    assert_eq!((value_at(&g, 2, 0), value_at(&g, 2, 1)), (Some(2), Some(3)));
}

// Gravity

#[test]
fn gravity_is_off_by_default() {
    let mut g = game(Rules::default(), &EMPTY5, single(4));
    g.place(0, 0, Pos::new(2, 0)).unwrap();
    assert_eq!(value_at(&g, 2, 0), Some(4));
}

#[test]
fn gravity_settles_after_place_and_after_merge() {
    for dir in Dir::ALL {
        let rules = Rules { gravity: Some(dir), ..Rules::default() };
        let mut g = game(rules, &EMPTY5, cluster(&[(0, 0, 1), (1, 0, 2)]));
        let turn = g.place(0, 0, Pos::new(2, 2)).unwrap();
        assert!(turn.steps.iter().any(|s| matches!(s, Step::Fall(_))), "{dir:?}");
        assert!(g.board().is_settled(dir), "{dir:?}: {:?}", g.board());
    }
}

#[test]
fn gravity_falls_then_merges_then_falls_again() {
    let rules = Rules { gravity: Some(Dir::Down), ..Rules::default() };
    // Column 0, bottom up: 1, 1, 3. A 1 dropped beside them falls next to the
    // bottom 1, the three 1s merge into a 2, and the 3 falls onto it.
    let rows = [". . . . .", ". . . . .", "3 . . . .", "1 . . . .", "1 . . . ."];
    let mut g = game(rules, &rows, single(1));
    let turn = g.place(0, 0, Pos::new(1, 0)).unwrap();
    let kinds: Vec<&str> = turn
        .steps
        .iter()
        .map(|s| match s {
            Step::Place(_) => "place",
            Step::Fall(_) => "fall",
            Step::Merge(_) => "merge",
            Step::Spawn { .. } => "spawn",
            Step::Replace { .. } => "replace",
            Step::End => "end",
        })
        .collect();
    assert_eq!(&kinds[..4], ["place", "fall", "merge", "fall"]);
    assert!(g.board().is_settled(Dir::Down), "{:?}", g.board());
    assert_eq!(g.board().dice().count(), 2);
}

// Spawn distribution

fn spawns(rules: &Rules, n: usize) -> Vec<Cluster> {
    let mut rng = Rng::new(42);
    (0..n).map(|_| cluster::spawn(rules, &mut rng)).collect()
}

#[test]
fn spawn_distribution_sizes_are_weighted() {
    let all = spawns(&Rules::default(), 20_000);
    for (size, expect) in [(1, 0.40), (2, 0.35), (3, 0.25)] {
        let share = all.iter().filter(|c| c.len() == size).count() as f64 / all.len() as f64;
        assert!((share - expect).abs() < 0.015, "size {size}: {share}");
    }
}

#[test]
fn spawn_distribution_shapes_are_uniform_up_to_rotation() {
    let all = spawns(&Rules { size_weights: [0.0, 0.0, 1.0], ..Rules::default() }, 12_000);
    let horizontal = all.iter().filter(|c| c.cells.iter().all(|(p, _)| p.y == 0)).count() as f64;
    let vertical = all.iter().filter(|c| c.cells.iter().all(|(p, _)| p.x == 0)).count() as f64;
    let n = all.len() as f64;
    assert!(((horizontal + vertical) / n - 0.5).abs() < 0.02, "I-trominoes: {}", (horizontal + vertical) / n);
    assert!((horizontal / n - 0.25).abs() < 0.02, "both orientations of the I");
    // Each of the L's four orientations, by its missing corner.
    for missing in [(0, 0), (1, 0), (0, 1), (1, 1)] {
        let share = all
            .iter()
            .filter(|c| c.len() == 3 && !c.cells.iter().any(|(p, _)| (p.x, p.y) == missing))
            .filter(|c| c.cells.iter().all(|(p, _)| p.x <= 1 && p.y <= 1))
            .count() as f64
            / n;
        assert!((share - 0.125).abs() < 0.015, "L missing {missing:?}: {share}");
    }
}

#[test]
fn spawn_distribution_values_follow_exp_minus_mass_over_t() {
    // Independent rolls: the variety rules off.
    let rules = Rules { no_repeat: false, one_pair: false, ..Rules::default() };
    let weights = value_weights(&rules);
    let total: f64 = weights.iter().sum();
    let values: Vec<u32> = spawns(&rules, 20_000).iter().flat_map(|c| c.cells.iter().map(|c| c.1)).collect();
    for v in 1..=3u32 {
        let share = values.iter().filter(|&&x| x == v).count() as f64 / values.len() as f64;
        let expect = weights[v as usize - 1] / total;
        assert!((share - expect).abs() < 0.015, "v = {v}: {share} vs {expect}");
    }
    assert!(values.iter().all(|&v| (1..=8).contains(&v)));
    // T = 2: the chance of a 1 is e^-0.5 against the rest.
    assert!((weights[0] - (-0.5f64).exp()).abs() < 1e-12);
}

#[test]
fn spawn_distribution_variety_is_on_by_default() {
    // One pair in every 3-die cluster; otherwise no value repeats.
    for c in spawns(&Rules::default(), 5_000) {
        let mut v: Vec<u32> = c.cells.iter().map(|c| c.1).collect();
        v.sort();
        v.dedup();
        assert_eq!(v.len(), if c.len() == 3 { 2 } else { c.len() }, "{c:?}");
    }
}

#[test]
fn spawn_distribution_no_repeat() {
    let rules = Rules { no_repeat: true, one_pair: false, ..Rules::default() };
    for c in spawns(&rules, 5_000) {
        let mut v: Vec<u32> = c.cells.iter().map(|c| c.1).collect();
        v.sort();
        v.dedup();
        assert_eq!(v.len(), c.len(), "{c:?}");
    }
}

#[test]
fn spawn_distribution_one_pair_overrides_no_repeat() {
    let rules = Rules { no_repeat: true, one_pair: true, ..Rules::default() };
    let mut single_cell = [0; 3];
    for c in spawns(&rules, 6_000).into_iter().filter(|c| c.len() == 3) {
        let v: Vec<u32> = c.cells.iter().map(|c| c.1).collect();
        let pairs = (0..3).filter(|&i| v[i] == v[(i + 1) % 3]).count();
        assert_eq!(pairs, 1, "exactly one pair: {v:?}");
        let odd = (0..3).find(|&i| v[i] != v[(i + 1) % 3] && v[i] != v[(i + 2) % 3]).unwrap();
        single_cell[odd] += 1;
    }
    assert!(single_cell.iter().all(|&n| n > 300), "{single_cell:?}");
}

// Supply

#[test]
fn supply_restocks_the_placed_slot_and_keeps_the_others() {
    for len in [1, 3, 5] {
        let rules = Rules { slots: len, ..Rules::default() };
        let mut g = Game::new(rules, 3);
        assert_eq!(g.slots().len(), len);
        let slot = len / 2;
        let mut rotated = 0;
        let p = loop {
            if let Some(p) = g.board().positions().find(|&p| g.can_place(slot, 0, p)) {
                break p;
            }
            g.rotate(slot);
            rotated += 1;
            assert!(rotated < 4);
        };
        let mut others = g.slots().to_vec();
        let turn = g.place(slot, 0, p).unwrap();
        assert_eq!(g.slots().len(), len);
        assert!(turn.steps.iter().any(|s| matches!(s, Step::Spawn { index, .. } if *index == slot)));
        others.remove(slot);
        let mut kept = g.slots().to_vec();
        kept.remove(slot);
        assert_eq!(kept, others, "the other slots are untouched");
    }
}

/// Every legal (anchor, cell) for the first slot as it's turned now.
fn legal(g: &Game) -> Vec<(usize, Pos)> {
    (0..g.slot(0).len())
        .flat_map(|a| g.board().positions().map(move |p| (a, p)))
        .filter(|&(a, p)| g.can_place(0, a, p))
        .collect()
}

/// Turns the first slot until it has a legal placement, and returns the first one.
fn first_fit(g: &mut Game) -> Pos {
    for _ in 0..4 {
        if let Some(p) = g.board().positions().find(|&p| g.can_place(0, 0, p)) {
            return p;
        }
        g.rotate(0);
    }
    panic!("no legal placement in any rotation: {:?} {:?}", g.board(), g.slot(0));
}

// No dead clusters

const HOLES: [&str; 4] = ["1 2 3 4", "5 . 7 .", "2 3 4 5", ". 7 . 1"];

#[test]
fn no_dead_clusters_in_a_new_game() {
    let rules = Rules { size: 4, slots: 4, size_weights: [0.0, 1.0, 1.0], ..Rules::default() };
    let g = Game::with_board(rules, 11, Board::parse(&HOLES));
    for c in g.slots() {
        assert!(fits_anywhere(g.board(), c), "{c:?}");
        assert_eq!(c.len(), 1, "only single dice fit isolated holes");
    }
}

#[test]
fn no_dead_clusters_after_a_turn() {
    let rules = Rules { size: 4, ..Rules::default() };
    let mut g = Game::with_board(rules, 11, Board::parse(&HOLES));
    g.set_slots(vec![single(6), cluster(&[(0, 0, 1), (1, 0, 2)])]);
    let turn = g.place(0, 0, Pos::new(1, 1)).unwrap();
    // The domino stays in slot 1 and fits nowhere on this board.
    assert!(turn.steps.iter().any(|s| matches!(s, Step::Replace { index: 1, .. })), "{:?}", turn.steps);
    assert!(g.slots().iter().all(|c| fits_anywhere(g.board(), c)));
}

// Ending

#[test]
fn ending_when_full_after_settling() {
    let rows = ["1 2 3 4", "5 6 7 8", "2 3 4 5", "6 7 8 ."];
    let mut g = Game::with_board(Rules { size: 4, ..Rules::default() }, 1, Board::parse(&rows));
    g.set_slots(vec![single(1), single(2)]);
    let turn = g.place(0, 0, Pos::new(3, 3)).unwrap();
    assert_eq!(turn.steps.last(), Some(&Step::End));
    assert!(g.is_over());
    assert_eq!(g.place(0, 0, Pos::new(0, 0)), Err(PlaceError::Over));
}

#[test]
fn ending_not_while_a_merge_frees_cells() {
    let rows = ["1 2 3 4", "5 6 7 8", "2 3 4 5", "6 7 5 ."];
    let mut g = Game::with_board(Rules { size: 4, ..Rules::default() }, 1, Board::parse(&rows));
    g.set_slots(vec![single(5), single(2)]);
    let turn = g.place(0, 0, Pos::new(3, 3)).unwrap();
    assert_eq!(merges(&turn).len(), 1);
    assert!(!g.is_over());
}

// Whole games

#[test]
fn same_seed_same_game() {
    let play = |seed| {
        let mut g = Game::new(Rules::default(), seed);
        let mut log = Vec::new();
        while !g.is_over() && log.len() < 200 {
            log.push(
                {
                    let p = first_fit(&mut g);
                    g.place(0, 0, p)
                }
                .unwrap(),
            );
        }
        (log, g.score())
    };
    assert_eq!(play(9), play(9));
    assert_ne!(play(9).1, play(10).1);
}

#[test]
fn invariants_hold_through_random_games() {
    for (seed, gravity) in (0..40).zip([None, Some(Dir::Down), Some(Dir::Left), Some(Dir::Up)].into_iter().cycle()) {
        let rules = Rules { gravity, size: 4 + seed as usize % 3, ..Rules::default() };
        let mut g = Game::new(rules.clone(), seed);
        let mut rng = Rng::new(seed ^ 0xabc);
        for _ in 0..400 {
            if g.is_over() {
                break;
            }
            for _ in 0..rng.below(4) {
                g.rotate(0);
            }
            let mut moves = legal(&g);
            for _ in 0..3 {
                if moves.is_empty() {
                    g.rotate(0);
                    moves = legal(&g);
                }
            }
            assert!(!moves.is_empty(), "no dead clusters");
            let (a, p) = moves[rng.below(moves.len())];
            g.place(0, a, p).unwrap();

            assert_eq!(g.slots().len(), rules.slots);
            assert!(g.board().neighborhoods().iter().all(|h| h.len() < rules.threshold), "settled to a fixed point");
            if let Some(dir) = gravity {
                assert!(g.board().is_settled(dir));
            }
            assert_eq!(g.is_over(), g.board().is_full());
        }
    }
}
