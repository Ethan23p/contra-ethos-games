//! One test group per spec policy, named after it.

use glam::Vec2;

use crate::*;

const DT: f32 = 1.0 / 120.0;

/// No well, so tests see only the forces they're about.
fn calm() -> Physics {
    Physics { well: 0.0, ..Physics::default() }
}

fn run(field: &mut Field, p: &Physics, secs: f32) -> Vec<Event> {
    let mut events = Vec::new();
    for _ in 0..(secs / DT).round() as usize {
        field.step(p, DT);
        events.extend(field.take_events());
    }
    events
}

/// Blobs placed in a row, `gap` apart centre to centre.
fn row(field: &mut Field, blobs: &[(u8, u32)], gap: f32) -> Vec<u32> {
    let start = -gap * (blobs.len() as f32 - 1.0) / 2.0;
    blobs.iter().enumerate().map(|(i, &(g, v))| field.add(g, v, Vec2::new(start + gap * i as f32, 0.0))).collect()
}

fn values(field: &Field) -> Vec<u32> {
    let mut v: Vec<u32> = field.blobs.iter().map(|b| b.value).collect();
    v.sort();
    v
}

#[test]
fn motion_a_pair_settles_near_the_lennard_jones_rest_distance() {
    let p = calm();
    let mut f = Field::new();
    // The default fluid is viscous: from just outside rest it takes seconds.
    row(&mut f, &[(0, 1), (1, 1)], 45.0);
    run(&mut f, &p, 8.0);
    let d = f.blobs[0].pos.distance(f.blobs[1].pos);
    let rest = p.sigma * 2f32.powf(1.0 / 6.0);
    assert!((d - rest).abs() < 1.0, "rest {rest}, got {d}");
}

#[test]
fn motion_overlapping_blobs_push_apart_without_exploding() {
    let p = calm();
    let mut f = Field::new();
    row(&mut f, &[(0, 1), (1, 1)], 2.0);
    run(&mut f, &p, 2.0);
    let d = f.blobs[0].pos.distance(f.blobs[1].pos);
    assert!(d.is_finite() && d > p.sigma && d < 2.0 * p.ring, "{d}");
}

#[test]
fn motion_the_well_pulls_a_lone_blob_toward_the_centre() {
    let p = Physics::default();
    let mut f = Field::new();
    f.add(0, 1, Vec2::new(150.0, 0.0));
    // The centre follows the blobs, so pin it to watch the pull alone.
    let mut x = 150.0;
    for _ in 0..120 {
        f.centre = Vec2::ZERO;
        f.step(&p, DT);
        assert!(f.blobs[0].pos.x < x);
        x = f.blobs[0].pos.x;
    }
}

#[test]
fn motion_the_centre_tracks_the_centre_of_mass() {
    let p = calm();
    let mut f = Field::new();
    f.add(0, 1, Vec2::new(100.0, 40.0));
    run(&mut f, &p, 2.0);
    assert!(f.centre.distance(Vec2::new(100.0, 40.0)) < 1.0, "{}", f.centre);
}

#[test]
fn motion_same_seed_same_field() {
    let p = Physics::default();
    let make = || {
        let mut f = Field::new();
        f.seed(&p, &mut Rng::new(42));
        f.launch(&p, 1.0, 0, 2);
        run(&mut f, &p, 2.0);
        f
    };
    let (a, b) = (make(), make());
    assert_eq!(a.blobs, b.blobs);
}

#[test]
fn identity_different_groups_never_bond_or_merge() {
    let p = calm();
    let mut f = Field::new();
    row(&mut f, &[(0, 1), (1, 1), (2, 1)], 39.0);
    let events = run(&mut f, &p, 2.0);
    assert!(events.is_empty(), "{events:?}");
    assert!(f.bonds.is_empty());
}

#[test]
fn identity_different_values_never_bond() {
    let p = calm();
    let mut f = Field::new();
    row(&mut f, &[(0, 1), (0, 2)], 39.0);
    run(&mut f, &p, 1.0);
    assert!(f.bonds.is_empty());
}

#[test]
fn bonding_a_touching_identical_pair_bonds_below_m() {
    let p = calm();
    let mut f = Field::new();
    let ids = row(&mut f, &[(0, 4), (0, 4)], 39.0);
    let events = run(&mut f, &p, 0.1);
    assert_eq!(f.bonds, vec![(ids[0], ids[1])]);
    assert!(events.contains(&Event::Bonded { a: ids[0], b: ids[1] }));
}

#[test]
fn bonding_a_bond_holds_a_pair_together_past_cohesion_range() {
    let p = calm();
    let mut f = Field::new();
    let ids = row(&mut f, &[(0, 4), (0, 4)], 39.0);
    run(&mut f, &p, 0.1);
    // Fling them apart, beyond where Lennard-Jones would let go.
    f.blobs[0].vel = Vec2::new(-1500.0, 0.0);
    f.blobs[1].vel = Vec2::new(1500.0, 0.0);
    run(&mut f, &p, 4.0);
    assert!(f.bonded(ids[0], ids[1]));
    let d = f.blobs[0].pos.distance(f.blobs[1].pos);
    assert!(d < 2.0 * p.radius * p.touch * 1.5, "spring pulled them back: {d}");
}

#[test]
fn bonding_with_m_2_a_touching_pair_merges_at_once_so_bonds_never_form() {
    let p = Physics { m: 2, ..calm() };
    let mut f = Field::new();
    row(&mut f, &[(0, 4), (0, 4)], 39.0);
    let events = run(&mut f, &p, 1.0);
    assert!(!events.iter().any(|e| matches!(e, Event::Bonded { .. })));
    assert_eq!(values(&f), vec![8]);
}

#[test]
fn threshold_a_neighborhood_reaching_m_merges() {
    let p = calm();
    let mut f = Field::new();
    row(&mut f, &[(0, 2), (0, 2), (0, 2)], 39.0);
    run(&mut f, &p, 1.0);
    assert_eq!(values(&f), vec![4]);
}

#[test]
fn threshold_a_neighborhood_below_m_does_not_merge() {
    let p = calm();
    let mut f = Field::new();
    row(&mut f, &[(0, 2), (0, 2)], 39.0);
    run(&mut f, &p, 1.0);
    assert_eq!(values(&f), vec![2, 2]);
}

#[test]
fn threshold_the_whole_neighborhood_merges_whatever_its_size() {
    // Four in a chain reach m = 3 together: all four merge, and the one over
    // m doubles once more.
    let p = calm();
    let mut f = Field::new();
    row(&mut f, &[(1, 1), (1, 1), (1, 1), (1, 1)], 39.0);
    run(&mut f, &p, 1.5);
    assert_eq!(values(&f), vec![4]);
}

#[test]
fn threshold_bonds_connect_a_neighborhood() {
    // Two bonded pairs; one partner of each touches. The bonds carry the
    // neighborhood past m, so all four merge.
    let p = calm();
    let mut f = Field::new();
    let a = f.add(0, 1, Vec2::new(-200.0, 0.0));
    let b = f.add(0, 1, Vec2::new(-161.0, 0.0));
    let c = f.add(0, 1, Vec2::new(200.0, 0.0));
    let d = f.add(0, 1, Vec2::new(239.0, 0.0));
    run(&mut f, &p, 0.05);
    assert_eq!(f.bonds.len(), 2, "{:?}", (a, b, c, d));
    // Bring b and c together.
    let left = f.blobs.iter().position(|x| x.id == b).unwrap();
    let right = f.blobs.iter().position(|x| x.id == c).unwrap();
    f.blobs[left].pos = Vec2::new(0.0, 0.0);
    f.blobs[right].pos = Vec2::new(39.0, 0.0);
    run(&mut f, &p, 2.0);
    assert_eq!(values(&f), vec![4]);
}

#[test]
fn merging_value_doubles_at_m_and_again_per_extra_blob() {
    assert_eq!(merged_value(4, 3, 3), 8);
    assert_eq!(merged_value(4, 4, 3), 16);
    assert_eq!(merged_value(4, 5, 3), 32);
    assert_eq!(merged_value(4, 2, 2), 8);
    assert_eq!(merged_value(4, 3, 2), 16);
}

#[test]
fn merging_converges_on_the_shared_centre() {
    let p = calm();
    let mut f = Field::new();
    // An L: the shared centre isn't any blob's position.
    f.add(2, 8, Vec2::new(0.0, 0.0));
    f.add(2, 8, Vec2::new(39.0, 0.0));
    f.add(2, 8, Vec2::new(0.0, 39.0));
    let events = run(&mut f, &p, 1.0);
    let at = events.iter().find_map(|e| if let Event::Merged { at, .. } = e { Some(*at) } else { None }).unwrap();
    assert!(at.distance(Vec2::new(13.0, 13.0)) < 6.0, "{at}");
}

#[test]
fn merging_takes_about_the_convergence_time() {
    let p = calm();
    let mut f = Field::new();
    row(&mut f, &[(0, 2), (0, 2), (0, 2)], 39.0);
    let mut t = 0.0;
    while f.blobs.len() > 1 {
        f.step(&p, DT);
        t += DT;
        assert!(t < 1.0, "merge never finished");
    }
    assert!(t > 0.1 && t < 2.0 * p.merge_secs, "{t}");
}

#[test]
fn merging_an_identical_blob_touching_a_converging_blob_joins() {
    let p = calm();
    let mut f = Field::new();
    row(&mut f, &[(0, 2), (0, 2), (0, 2)], 39.0);
    f.step(&p, DT);
    assert_eq!(f.merges.len(), 1);
    // A fourth arrives next to the merge as it converges.
    f.add(0, 2, Vec2::new(0.0, 40.0));
    run(&mut f, &p, 1.0);
    assert_eq!(values(&f), vec![8], "four blobs: doubled twice");
}

#[test]
fn merging_the_new_blob_pushes_its_neighbours_away() {
    let p = Physics { epsilon: 0.0, ..calm() };
    let mut f = Field::new();
    row(&mut f, &[(0, 2), (0, 2), (0, 2)], 39.0);
    let other = f.add(1, 1, Vec2::new(0.0, 45.0));
    let mut pushed = false;
    for _ in 0..240 {
        f.step(&p, DT);
        for e in f.take_events() {
            if matches!(e, Event::Merged { .. }) {
                pushed = f.blob(other).unwrap().vel.y > 0.0;
            }
        }
    }
    assert!(pushed);
}

#[test]
fn launching_moves_toward_the_centre_at_the_fixed_speed() {
    let p = Physics::default();
    let mut f = Field::new();
    let id = f.launch(&p, 0.0, 0, 1);
    let b = f.blob(id).unwrap();
    assert!((b.pos - Vec2::new(p.ring, 0.0)).length() < 1e-3);
    assert!((b.vel - Vec2::new(-p.launch_speed(), 0.0)).length() < 1e-2);
}

#[test]
fn launching_an_unobstructed_blob_coasts_about_its_reach() {
    let p = Physics { well: 0.0, ..Physics::default() };
    let mut f = Field::new();
    f.launch(&p, 0.0, 0, 1);
    // Pin the centre, so the launch alone moves the blob.
    for _ in 0..600 {
        f.centre = Vec2::ZERO;
        f.step(&p, DT);
    }
    let x = f.blobs[0].pos.x;
    let expected = p.ring * (1.0 - p.launch_reach);
    assert!((x - expected).abs() < p.ring * 0.05, "{x} vs {expected}");
}

#[test]
fn seeding_opens_with_10_to_13_blobs_of_values_2_to_8() {
    let p = Physics::default();
    for seed in 0..20 {
        let mut f = Field::new();
        f.seed(&p, &mut Rng::new(seed));
        assert!((10..=13).contains(&f.blobs.len()));
        assert!(f.blobs.iter().all(|b| [2, 4, 8].contains(&b.value) && b.group < GROUPS));
    }
}

#[test]
fn seeding_no_two_blobs_start_overlapping() {
    let p = Physics::default();
    let mut f = Field::new();
    f.seed(&p, &mut Rng::new(3));
    for a in &f.blobs {
        for b in &f.blobs {
            if a.id != b.id {
                assert!(a.pos.distance(b.pos) > p.sigma, "{} {}", a.pos, b.pos);
            }
        }
    }
}

#[test]
fn merging_a_launched_blob_landing_waits_the_delay_before_converging() {
    let p = Physics { merge_delay: 0.2, ..calm() };
    let mut f = Field::new();
    row(&mut f, &[(0, 2), (0, 2), (0, 2)], 39.0);
    f.blobs[2].launched = true;
    run(&mut f, &p, 0.15);
    assert!(f.merges.is_empty(), "still under ordinary physics");
    assert_eq!(f.pending.len(), 1);
    run(&mut f, &p, 1.0);
    assert_eq!(values(&f), vec![4]);
}

#[test]
fn merging_without_a_launched_blob_starts_at_once_whatever_the_delay() {
    let p = Physics { merge_delay: 0.2, ..calm() };
    let mut f = Field::new();
    row(&mut f, &[(0, 2), (0, 2), (0, 2)], 39.0);
    f.step(&p, DT);
    assert_eq!(f.merges.len(), 1);
}

/// A bonded pair, and a launched blob that lands beside it and ricochets.
fn ricochet(hold: f32) -> Field {
    let p = Physics { merge_delay: 0.15, merge_hold: hold, ..calm() };
    let mut f = Field::new();
    row(&mut f, &[(0, 2), (0, 2)], 39.0);
    run(&mut f, &p, 0.05);
    let id = f.add(0, 2, Vec2::new(19.5 + 39.0, 0.0));
    let k = f.blobs.iter().position(|b| b.id == id).unwrap();
    f.blobs[k].launched = true;
    f.blobs[k].vel = Vec2::new(400.0, 0.0);
    run(&mut f, &p, 0.25);
    f
}

#[test]
fn merging_a_launched_blob_that_ricochets_within_the_hold_distance_still_merges() {
    let f = ricochet(2.5);
    assert!(f.merges.len() == 1 || values(&f) == vec![4], "{:?}", values(&f));
}

#[test]
fn merging_a_launched_blob_that_ricochets_past_the_hold_distance_is_let_go() {
    let f = ricochet(1.0);
    assert!(f.merges.is_empty() && values(&f) == vec![2, 2, 2], "{:?}", values(&f));
}

#[test]
fn merging_a_launched_blob_that_grazes_another_colour_first_still_waits() {
    let p = Physics { merge_delay: 0.3, ..calm() };
    let mut f = Field::new();
    f.add(1, 1, Vec2::new(0.0, 100.0));
    let id = f.add(0, 2, Vec2::new(0.0, 139.0));
    f.blobs[1].launched = true;
    f.step(&p, DT);
    // Then it rolls in beside two of its own.
    row(&mut f, &[(0, 2), (0, 2)], 39.0);
    let k = f.blobs.iter().position(|b| b.id == id).unwrap();
    f.blobs[k].pos = Vec2::new(0.0, 39.0);
    run(&mut f, &p, 0.2);
    assert!(f.merges.is_empty(), "still within the delay");
    run(&mut f, &p, 1.0);
    assert!(values(&f).contains(&4), "{:?}", values(&f));
}

#[test]
fn merging_with_the_delay_on_the_whole_clump_the_jolt_triggers_nothing_early() {
    let p = Physics { merge_delay: 0.3, delay_clump: true, ..calm() };
    let mut f = Field::new();
    f.add(1, 1, Vec2::new(0.0, 200.0));
    let id = f.add(0, 64, Vec2::new(0.0, 239.0));
    f.blobs[1].launched = true;
    f.step(&p, DT);
    assert!(!f.blob(id).unwrap().launched);
    // Elsewhere, three come together during the jolt.
    row(&mut f, &[(2, 4), (2, 4), (2, 4)], 39.0);
    run(&mut f, &p, 0.2);
    assert!(f.merges.is_empty());
    run(&mut f, &p, 1.0);
    assert!(values(&f).contains(&8), "{:?}", values(&f));
}

#[test]
fn merging_without_the_whole_clump_delay_merges_elsewhere_start_at_once() {
    let p = Physics { merge_delay: 0.3, delay_clump: false, ..calm() };
    let mut f = Field::new();
    f.add(1, 1, Vec2::new(0.0, 200.0));
    f.add(0, 64, Vec2::new(0.0, 239.0));
    f.blobs[1].launched = true;
    f.step(&p, DT);
    row(&mut f, &[(2, 4), (2, 4), (2, 4)], 39.0);
    f.step(&p, DT);
    assert_eq!(f.merges.len(), 1);
}

#[test]
fn merging_a_launched_blob_does_not_join_a_converging_merge_until_its_delay_ends() {
    let p = Physics { merge_delay: 0.3, ..calm() };
    let mut f = Field::new();
    row(&mut f, &[(0, 2), (0, 2), (0, 2)], 39.0);
    f.step(&p, DT);
    assert_eq!(f.merges.len(), 1);
    let id = f.add(0, 2, Vec2::new(0.0, 40.0));
    let k = f.blobs.iter().position(|b| b.id == id).unwrap();
    f.blobs[k].launched = true;
    f.step(&p, DT);
    assert!(f.blob(id).unwrap().merging.is_none());
}

#[test]
fn merging_a_merged_blob_waits_the_chain_delay_before_merging_again() {
    let p = Physics { chain_delay: 0.4, merge_push: 0.0, ..calm() };
    let mut f = Field::new();
    row(&mut f, &[(0, 2), (0, 2), (0, 2)], 39.0);
    // Two 4s waiting just below and above where the new 4 will appear.
    f.add(0, 4, Vec2::new(0.0, 39.0));
    f.add(0, 4, Vec2::new(0.0, -39.0));
    let mut made = None;
    let mut began_after = None;
    let mut t = 0.0;
    while t < 2.0 {
        f.step(&p, DT);
        t += DT;
        for e in f.take_events() {
            match e {
                Event::Merged { value: 4, .. } => made = Some(t),
                Event::MergeBegan { .. } if made.is_some() => began_after = Some(t),
                _ => {}
            }
        }
    }
    let (made, began) = (made.unwrap(), began_after.expect("the 4s merged in the end"));
    assert!(began - made >= 0.4 - DT, "{made} → {began}");
}

// The game: pool window, draw, scoring, purge, nova, ending.

/// A game on a hand-built field, with the queue filled for its peak.
fn game(p: &Physics, blobs: &[(u8, u32, Vec2)]) -> Game {
    let mut g = Game::new(p, Rules::default(), 1);
    g.field = Field::new();
    for &(group, value, pos) in blobs {
        g.field.add(group, value, pos);
    }
    g.queue.clear();
    g.step(p, DT);
    g
}

fn play(g: &mut Game, p: &Physics, secs: f32) -> Vec<Event> {
    let mut events = Vec::new();
    for _ in 0..(secs / DT).round() as usize {
        g.step(p, DT);
        events.extend(g.take_events());
    }
    events
}

/// Out of the way: blobs that keep a field from emptying, none identical.
fn bystanders() -> Vec<(u8, u32, Vec2)> {
    vec![(0, 2, Vec2::new(-150.0, 0.0)), (1, 4, Vec2::new(150.0, 0.0)), (2, 8, Vec2::new(0.0, -150.0))]
}

#[test]
fn pool_window_opens_at_8_4_2() {
    let r = Rules::default();
    assert_eq!(r.pool(8), vec![2, 4, 8]);
    // Below a peak of 8 the ceiling holds at 8 and the window at 3.
    assert_eq!(r.pool(1), vec![2, 4, 8]);
    assert_eq!(r.pool(16), vec![2, 4, 8]);
}

#[test]
fn pool_window_slides_up_with_the_ceiling_at_half_the_peak() {
    let r = Rules::default();
    assert_eq!(r.pool(32), vec![4, 8, 16]);
    assert_eq!(r.pool(256), vec![32, 64, 128]);
}

#[test]
fn pool_window_widens_by_one_every_six_doublings() {
    let r = Rules::default();
    assert_eq!(r.pool(256).len(), 3);
    assert_eq!(r.pool(512), vec![32, 64, 128, 256]);
    assert_eq!(r.pool(1 << 15).len(), 5);
}

#[test]
fn pool_window_never_goes_below_1() {
    let r = Rules { ceiling: 2, window: 5, ..Rules::default() };
    assert_eq!(r.pool(2), vec![1, 2]);
}

#[test]
fn draw_values_come_from_the_pool_favouring_low_values() {
    let p = Physics::default();
    let g = Game::new(&p, Rules { queue: 30_000, ..Rules::default() }, 3);
    let pool = g.pool();
    let counts: Vec<usize> = pool.iter().map(|&v| g.queue.iter().filter(|e| e.1 == v).count()).collect();
    assert_eq!(counts.iter().sum::<usize>(), g.queue.len(), "every value from {pool:?}");
    for i in 1..counts.len() {
        let ratio = counts[i] as f64 / counts[i - 1] as f64;
        assert!((ratio - 0.8).abs() < 0.05, "weight 1 / 1.25^i: {counts:?}");
    }
}

#[test]
fn draw_groups_are_uniform() {
    let p = Physics::default();
    let g = Game::new(&p, Rules { queue: 30_000, ..Rules::default() }, 4);
    for group in 0..GROUPS {
        let share = g.queue.iter().filter(|e| e.0 == group).count() as f64 / g.queue.len() as f64;
        assert!((share - 1.0 / GROUPS as f64).abs() < 0.02, "group {group}: {share}");
    }
}

#[test]
fn draw_the_queue_holds_2_and_launching_moves_it_up() {
    let p = Physics::default();
    let mut g = Game::new(&p, Rules::default(), 5);
    assert_eq!(g.queue.len(), 2);
    let (head, next) = (g.queue[0], g.queue[1]);
    let id = g.launch(&p, 0.0).unwrap();
    let b = g.field.blob(id).unwrap();
    assert_eq!((b.group, b.value), head);
    assert_eq!(g.queue.len(), 2);
    assert_eq!(g.queue[0], next);
}

#[test]
fn draw_queued_values_the_pool_no_longer_allows_are_rerolled() {
    let p = calm();
    let mut blobs = bystanders();
    blobs.push((0, 64, Vec2::new(0.0, 150.0)));
    let mut g = game(&p, &blobs);
    g.queue = vec![(1, 2), (2, 16)];
    g.step(&p, DT);
    let pool = g.pool();
    assert_eq!(pool, vec![8, 16, 32]);
    assert_eq!(g.queue[1], (2, 16), "allowed entries stay");
    assert_eq!(g.queue[0].0, 1, "a re-roll keeps the group");
    assert!(pool.contains(&g.queue[0].1));
}

#[test]
fn scoring_a_merge_of_n_raises_the_combo_by_n_minus_1_and_scores_value_times_combo() {
    let p = calm();
    let mut blobs = bystanders();
    blobs.extend([(0, 8, Vec2::new(-40.0, 0.0)), (0, 8, Vec2::ZERO), (0, 8, Vec2::new(40.0, 0.0))]);
    let mut g = game(&p, &blobs);
    let events = play(&mut g, &p, 1.0);
    // Three 8s make a 16 and lift the combo from 1 to 3.
    assert!(events.iter().any(|e| matches!(e, Event::Scored { points: 48, combo: 3, .. })));
    assert_eq!(g.score, 48);
    assert_eq!(g.combo, 3);
}

fn trio(g: &mut Game, value: u32, x: f32) {
    for dx in [-40.0, 0.0, 40.0] {
        g.field.add(1, value, Vec2::new(x + dx, 300.0));
    }
}

#[test]
fn scoring_merges_in_quick_succession_build_the_combo() {
    let p = calm();
    let mut g = game(&p, &bystanders());
    trio(&mut g, 8, -150.0);
    play(&mut g, &p, 0.7);
    trio(&mut g, 16, 150.0);
    play(&mut g, &p, 0.7);
    // 16 × 3, then 32 × 5.
    assert_eq!(g.score, 48 + 160);
}

#[test]
fn scoring_the_combo_resets_to_1_after_1_4_s_without_a_merge() {
    let p = calm();
    let mut g = game(&p, &bystanders());
    trio(&mut g, 8, -150.0);
    play(&mut g, &p, 2.0);
    assert_eq!(g.combo, 1);
    trio(&mut g, 16, 150.0);
    play(&mut g, &p, 0.7);
    assert_eq!(g.score, 48 + 32 * 3);
}

#[test]
fn purge_blobs_below_the_pool_leave_the_field() {
    let p = calm();
    let mut blobs = bystanders();
    blobs.push((0, 64, Vec2::new(0.0, 150.0)));
    let g = game(&p, &blobs);
    // The pool is 8–32: the 2 and the 4 go, the 8 stays.
    assert_eq!(values(&g.field), vec![8, 64]);
}

#[test]
fn purge_a_merging_blob_finishes_its_merge_first() {
    let p = calm();
    let mut blobs = bystanders();
    blobs.push((0, 64, Vec2::new(0.0, 150.0)));
    let mut g = game(&p, &blobs);
    trio(&mut g, 4, 0.0);
    // Below the floor, but they touch at once: the merge makes an 8, which stays.
    play(&mut g, &p, 1.0);
    assert_eq!(values(&g.field), vec![8, 8, 64]);
}

#[test]
fn nova_one_blob_remaining_scores_2000_and_seeds_12_of_its_value_or_half() {
    let p = calm();
    let mut g = game(&p, &[(0, 16, Vec2::new(30.0, 0.0))]);
    let events = g.take_events();
    assert!(events.iter().any(|e| matches!(e, Event::Nova { value: 16, bonus: 2000, .. })));
    assert_eq!(g.score, 2000);
    assert_eq!(g.field.blobs.len(), 12);
    assert!(g.field.blobs.iter().all(|b| b.value == 16 || b.value == 8));
}

#[test]
fn nova_follows_a_merge_that_leaves_one_blob() {
    let p = calm();
    let mut g = game(&p, &[(0, 8, Vec2::new(-40.0, 0.0)), (0, 8, Vec2::ZERO), (0, 8, Vec2::new(40.0, 0.0))]);
    let events = play(&mut g, &p, 1.0);
    assert!(events.iter().any(|e| matches!(e, Event::Nova { value: 16, .. })));
    assert_eq!(g.score, 48 + 2000);
}

/// A settled field at the centre and one blob well past the rim, which
/// nothing pulls back.
fn straggler(p: &Physics) -> Game {
    let mut blobs = bystanders();
    blobs.push((1, 2, Vec2::ZERO));
    blobs.push((2, 4, Vec2::new(p.rim * 2.5, 0.0)));
    game(p, &blobs)
}

#[test]
fn ending_a_blob_outside_the_rim_for_the_grace_period_ends_the_game() {
    let p = Physics { centre_follow: 0.0, ..calm() };
    let mut g = straggler(&p);
    play(&mut g, &p, 6.3);
    assert!(!g.over);
    let events = play(&mut g, &p, 0.3);
    assert!(g.over);
    assert!(events.iter().any(|e| matches!(e, Event::Ended { .. })));
}

#[test]
fn ending_the_grace_period_must_be_continuous() {
    let p = Physics { centre_follow: 0.0, ..calm() };
    let mut g = straggler(&p);
    play(&mut g, &p, 5.0);
    let far = g.field.blobs.iter().position(|b| b.value == 4 && b.group == 2).unwrap();
    let out = g.field.blobs[far].pos;
    g.field.blobs[far].pos = Vec2::new(0.0, 60.0);
    play(&mut g, &p, 0.1);
    g.field.blobs[far].pos = out;
    play(&mut g, &p, 5.0);
    assert!(!g.over);
}

#[test]
fn ending_launches_stop_once_the_game_is_over() {
    let p = Physics { centre_follow: 0.0, ..calm() };
    let mut g = straggler(&p);
    play(&mut g, &p, 7.0);
    assert!(g.launch(&p, 0.0).is_none());
}
