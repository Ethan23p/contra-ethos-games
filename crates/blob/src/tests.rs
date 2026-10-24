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
fn seeding_opens_with_10_to_13_blobs_of_values_1_to_8() {
    let p = Physics::default();
    for seed in 0..20 {
        let mut f = Field::new();
        f.seed(&p, &mut Rng::new(seed));
        assert!((10..=13).contains(&f.blobs.len()));
        assert!(f.blobs.iter().all(|b| [1, 2, 4, 8].contains(&b.value) && b.group < GROUPS));
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
