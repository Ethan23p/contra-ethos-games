//! The field: blobs, bonds and merges in progress, and the Move → Merge loop.

use glam::Vec2;

use crate::{GROUPS, Physics, Rng};

#[derive(Clone, Debug, PartialEq)]
pub struct Blob {
    /// Stable for the blob's life; never reused.
    pub id: u32,
    pub group: u8,
    /// 1, 2, 4, 8, …
    pub value: u32,
    pub pos: Vec2,
    pub vel: Vec2,
    /// The merge it's converging in, if any.
    pub merging: Option<u32>,
    /// Launched and yet to touch anything: a merge it lands in waits out the
    /// merge delay.
    pub launched: bool,
}

impl Blob {
    pub fn identical(&self, other: &Blob) -> bool {
        self.group == other.group && self.value == other.value
    }
}

/// A neighborhood converging on its shared centre.
#[derive(Clone, Debug, PartialEq)]
pub struct Merge {
    pub id: u32,
    pub members: Vec<u32>,
    /// Seconds since it began.
    pub age: f32,
}

/// A neighborhood that reached m by a launched blob landing, held under
/// ordinary physics for the merge delay so the launch's jolt can carry
/// through before it converges. Any identical blob that touches it meanwhile
/// joins.
#[derive(Clone, Debug, PartialEq)]
pub struct Pending {
    pub members: Vec<u32>,
    /// Seconds since it formed.
    pub age: f32,
}

/// What happened during a step, for presentation.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Bonded {
        a: u32,
        b: u32,
    },
    MergeBegan {
        merge: u32,
    },
    /// The merge's blobs met: `blob` is the new one, made of `count` blobs.
    Merged {
        merge: u32,
        blob: u32,
        value: u32,
        count: usize,
        at: Vec2,
    },
}

#[derive(Clone, Debug, Default)]
pub struct Field {
    pub blobs: Vec<Blob>,
    /// Bonded pairs, smaller id first.
    pub bonds: Vec<(u32, u32)>,
    pub merges: Vec<Merge>,
    /// Neighborhoods waiting out the merge delay.
    pub pending: Vec<Pending>,
    /// Tracks the blobs' centre of mass; the well sits here.
    pub centre: Vec2,
    /// Since the last [`Field::take_events`].
    events: Vec<Event>,
    next_id: u32,
}

impl Field {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    pub fn blob(&self, id: u32) -> Option<&Blob> {
        self.blobs.iter().find(|b| b.id == id)
    }

    fn id(&mut self) -> u32 {
        self.next_id += 1;
        self.next_id
    }

    /// Adds a blob at rest. Returns its id.
    pub fn add(&mut self, group: u8, value: u32, pos: Vec2) -> u32 {
        let id = self.id();
        self.blobs.push(Blob { id, group, value, pos, vel: Vec2::ZERO, merging: None, launched: false });
        id
    }

    /// Launch: a blob from the ring at `angle`, moving toward the centre at the
    /// fixed launch speed. It's a field blob from the start, under the same
    /// forces as every other.
    pub fn launch(&mut self, p: &Physics, angle: f32, group: u8, value: u32) -> u32 {
        let dir = Vec2::from_angle(angle);
        let id = self.add(group, value, self.centre + dir * p.ring);
        let blob = self.blobs.last_mut().unwrap();
        blob.vel = -dir * p.launch_speed();
        blob.launched = true;
        id
    }

    /// Seed: 10–13 blobs of values 1–8 in random groups, packed loosely around
    /// the centre on a sunflower spiral so none start overlapping.
    pub fn seed(&mut self, p: &Physics, rng: &mut Rng) {
        let count = 10 + rng.below(4);
        let golden = std::f32::consts::PI * (3.0 - 5f32.sqrt());
        for k in 0..count {
            // Neighbours on the spiral sit about 1.15 σ apart, near rest.
            let r = p.sigma * 0.65 * (k as f32 + 0.5).sqrt();
            let pos = self.centre + Vec2::from_angle(k as f32 * golden) * r;
            let value = 1 << rng.below(4);
            let group = rng.below(GROUPS as usize) as u8;
            self.add(group, value, pos);
        }
    }

    /// Move, then Merge: advance `dt` seconds in substeps, finish the merges
    /// whose blobs have met, then bond touching pairs and begin new merges.
    pub fn step(&mut self, p: &Physics, dt: f32) {
        let substeps = p.substeps.max(1);
        let h = dt / substeps as f32;
        for _ in 0..substeps {
            self.substep(p, h);
        }
        for merge in &mut self.merges {
            merge.age += dt;
        }
        for pending in &mut self.pending {
            pending.age += dt;
        }
        if !self.blobs.is_empty() {
            let mass = self.blobs.iter().map(|b| b.pos).sum::<Vec2>() / self.blobs.len() as f32;
            self.centre += (mass - self.centre) * (1.0 - (-p.centre_follow * dt).exp());
        }
        self.finish_merges(p);
        self.link(p);
        let blobs = &self.blobs;
        self.pending.retain(|h| h.members.iter().any(|id| blobs.iter().any(|b| b.id == *id)));
    }

    fn index(&self, id: u32) -> Option<usize> {
        self.blobs.iter().position(|b| b.id == id)
    }

    pub fn bonded(&self, a: u32, b: u32) -> bool {
        self.bonds.contains(&(a.min(b), a.max(b)))
    }

    fn substep(&mut self, p: &Physics, h: f32) {
        let n = self.blobs.len();
        let mut acc = vec![Vec2::ZERO; n];

        // The well: a pull toward the centre that grows with distance, then saturates.
        for (b, a) in self.blobs.iter().zip(&mut acc) {
            if b.merging.is_some() {
                continue;
            }
            let d = self.centre - b.pos;
            let dist = d.length();
            if dist > 1e-3 {
                *a += d / dist * p.well * (dist / p.well_radius.max(1.0)).min(1.0);
            }
        }

        // Lennard-Jones between every pair in range: F = 24ε/r · (2(σ/r)¹² − (σ/r)⁶).
        // A bond replaces the cohesive (σ/r)⁶ term with its spring; blobs
        // converging in the same merge pass through each other.
        let cutoff = p.sigma * p.cutoff;
        let floor = p.sigma * 0.35;
        for i in 0..n {
            for j in i + 1..n {
                let (a, b) = (&self.blobs[i], &self.blobs[j]);
                if a.merging.is_some() && a.merging == b.merging {
                    continue;
                }
                let d = a.pos - b.pos;
                let r2 = d.length_squared();
                if r2 >= cutoff * cutoff || r2 < 1e-8 {
                    continue;
                }
                let r = r2.sqrt();
                let rs = r.max(floor);
                let s6 = (p.sigma / rs).powi(6);
                let cohesion = if self.bonded(a.id, b.id) { 0.0 } else { s6 };
                let f = (24.0 * p.epsilon / rs * (2.0 * s6 * s6 - cohesion)).clamp(-p.max_force, p.max_force);
                acc[i] += d / r * f;
                acc[j] -= d / r * f;
            }
        }

        // Bonds: a spring that pulls once stretched past rest.
        let rest = 2.0 * p.radius * p.bond_rest;
        for &(a, b) in &self.bonds {
            let (Some(i), Some(j)) = (self.index(a), self.index(b)) else { continue };
            let d = self.blobs[i].pos - self.blobs[j].pos;
            let r = d.length();
            if r > rest {
                let f = d / r * (r - rest) * p.spring;
                acc[i] -= f;
                acc[j] += f;
            }
        }

        // Merges: each blob springs toward the merge's shared centre, which
        // moves with them, critically damped against their mean velocity.
        let omega = 5.0 / p.merge_secs.max(0.01);
        for merge in &self.merges {
            let members: Vec<usize> = merge.members.iter().filter_map(|&id| self.index(id)).collect();
            if members.is_empty() {
                continue;
            }
            let k = members.len() as f32;
            let centre = members.iter().map(|&i| self.blobs[i].pos).sum::<Vec2>() / k;
            let vel = members.iter().map(|&i| self.blobs[i].vel).sum::<Vec2>() / k;
            for &i in &members {
                let b = &self.blobs[i];
                acc[i] += (centre - b.pos) * omega * omega - (b.vel - vel) * 2.0 * omega;
            }
        }

        let max_acc = p.max_force * 1.5;
        let keep = (-p.damping * h).exp();
        for (b, a) in self.blobs.iter_mut().zip(acc) {
            b.vel = ((b.vel + a.clamp_length_max(max_acc) * h) * keep).clamp_length_max(p.max_speed);
            b.pos += b.vel * h;
        }
    }

    /// Merges whose blobs have met become one blob of the new value at their
    /// centre, which lightly pushes the blobs around it away.
    fn finish_merges(&mut self, p: &Physics) {
        let meet = p.radius * 0.25;
        let mut i = 0;
        while i < self.merges.len() {
            let merge = &self.merges[i];
            let members: Vec<&Blob> = self.blobs.iter().filter(|b| b.merging == Some(merge.id)).collect();
            let k = members.len() as f32;
            let centre = members.iter().map(|b| b.pos).sum::<Vec2>() / k.max(1.0);
            let met = members.iter().all(|b| b.pos.distance(centre) <= meet);
            // A merge that can't close (pinned by its neighbours) still ends.
            if !(met || merge.age >= 3.0 * p.merge_secs) {
                i += 1;
                continue;
            }
            let merge = self.merges.remove(i);
            let count = members.len();
            let (group, value) = (members[0].group, members[0].value);
            let vel = members.iter().map(|b| b.vel).sum::<Vec2>() / k;
            let value = merged_value(value, count, p.m);
            self.blobs.retain(|b| b.merging != Some(merge.id));
            let ids: Vec<u32> = merge.members.clone();
            self.bonds.retain(|(a, b)| !ids.contains(a) && !ids.contains(b));

            let range = 3.0 * p.radius;
            for b in &mut self.blobs {
                let d = b.pos - centre;
                let dist = d.length();
                if dist > 1e-3 && dist < range {
                    b.vel += d / dist * p.merge_push * (1.0 - dist / range);
                }
            }
            let blob = self.add(group, value, centre);
            self.blobs.last_mut().unwrap().vel = vel;
            self.events.push(Event::Merged { merge: merge.id, blob, value, count, at: centre });
        }
    }

    /// Joins, bonds and new merges, from who touches whom.
    fn link(&mut self, p: &Physics) {
        let touch = 2.0 * p.radius * p.touch;
        let touching = |a: &Blob, b: &Blob| a.identical(b) && a.pos.distance_squared(b.pos) < touch * touch;
        let n = self.blobs.len();

        // An identical blob that touches a converging blob joins its merge.
        for i in 0..n {
            if self.blobs[i].merging.is_some() {
                continue;
            }
            let join = (0..n).find_map(|j| {
                let other = &self.blobs[j];
                other.merging.filter(|_| touching(&self.blobs[i], other))
            });
            if let Some(merge) = join {
                let id = self.blobs[i].id;
                self.blobs[i].merging = Some(merge);
                self.bonds.retain(|&(a, b)| a != id && b != id);
                if let Some(m) = self.merges.iter_mut().find(|m| m.id == merge) {
                    m.members.push(id);
                }
            }
        }

        // Neighborhoods: identical blobs connected by touch or bond.
        let mut root: Vec<usize> = (0..n).collect();
        fn find(root: &mut [usize], mut i: usize) -> usize {
            while root[i] != i {
                root[i] = root[root[i]];
                i = root[i];
            }
            i
        }
        let mut touches = Vec::new();
        for i in 0..n {
            for j in i + 1..n {
                let (a, b) = (&self.blobs[i], &self.blobs[j]);
                if a.merging.is_some() || b.merging.is_some() {
                    continue;
                }
                let t = touching(a, b);
                if t {
                    touches.push((i, j));
                }
                if t || self.bonded(a.id, b.id) {
                    let (ri, rj) = (find(&mut root, i), find(&mut root, j));
                    root[ri.max(rj)] = ri.min(rj);
                }
            }
        }
        let mut size = vec![0usize; n];
        for i in 0..n {
            size[find(&mut root, i)] += 1;
        }

        // A neighborhood that reaches m joins any pending neighborhood it
        // overlaps. Otherwise it's held for the merge delay if a launched blob
        // just landed in it, and merges now if not.
        let m = p.m.max(2);
        let mut groups: Vec<(usize, Vec<u32>, bool)> = Vec::new();
        for i in 0..n {
            if self.blobs[i].merging.is_some() {
                continue;
            }
            let r = find(&mut root, i);
            if size[r] < m {
                continue;
            }
            let (id, launched) = (self.blobs[i].id, self.blobs[i].launched);
            match groups.iter_mut().find(|(root, ..)| *root == r) {
                Some((_, ids, l)) => {
                    ids.push(id);
                    *l |= launched;
                }
                None => groups.push((r, vec![id], launched)),
            }
        }
        for (_, ids, launched) in groups {
            let mut overlapping =
                self.pending.iter().enumerate().filter(|(_, h)| h.members.iter().any(|id| ids.contains(id)));
            let Some((first, _)) = overlapping.next() else {
                let age = if launched { 0.0 } else { p.merge_delay };
                self.pending.push(Pending { members: ids, age });
                continue;
            };
            let others: Vec<usize> = overlapping.map(|(k, _)| k).collect();
            for &k in others.iter().rev() {
                let other = self.pending.remove(k);
                let held = &mut self.pending[first];
                held.age = held.age.max(other.age);
                held.members.extend(other.members);
            }
            let held = &mut self.pending[first];
            held.members.extend(ids);
            held.members.sort_unstable();
            held.members.dedup();
        }

        // When the delay is up, members still within the hold distance of one
        // another (or bonded) merge whole, in groups of at least m; the rest
        // go back to the field.
        let hold = touch * p.merge_hold.max(1.0);
        let mut i = 0;
        while i < self.pending.len() {
            if self.pending[i].age < p.merge_delay {
                i += 1;
                continue;
            }
            let held = self.pending.remove(i);
            let members: Vec<usize> = held
                .members
                .iter()
                .filter_map(|&id| self.index(id))
                .filter(|&k| self.blobs[k].merging.is_none())
                .collect();
            let mut root: Vec<usize> = (0..members.len()).collect();
            for x in 0..members.len() {
                for y in x + 1..members.len() {
                    let (a, b) = (&self.blobs[members[x]], &self.blobs[members[y]]);
                    if a.pos.distance_squared(b.pos) < hold * hold || self.bonded(a.id, b.id) {
                        let (rx, ry) = (find(&mut root, x), find(&mut root, y));
                        root[rx.max(ry)] = rx.min(ry);
                    }
                }
            }
            let mut size = vec![0usize; members.len()];
            for x in 0..members.len() {
                size[find(&mut root, x)] += 1;
            }
            let mut began: Vec<(usize, u32)> = Vec::new();
            for (x, &k) in members.iter().enumerate() {
                let r = find(&mut root, x);
                if size[r] < m {
                    continue;
                }
                let merge = match began.iter().find(|(root, _)| *root == r) {
                    Some(&(_, merge)) => merge,
                    None => {
                        let merge = self.id();
                        began.push((r, merge));
                        self.merges.push(Merge { id: merge, members: Vec::new(), age: 0.0 });
                        self.events.push(Event::MergeBegan { merge });
                        merge
                    }
                };
                self.blobs[k].merging = Some(merge);
                let id = self.blobs[k].id;
                self.merges.iter_mut().find(|m| m.id == merge).unwrap().members.push(id);
            }
        }
        let merging: Vec<u32> = self.blobs.iter().filter(|b| b.merging.is_some()).map(|b| b.id).collect();
        self.bonds.retain(|(a, b)| !merging.contains(a) && !merging.contains(b));

        // A launched blob has landed once it touches anything.
        for i in 0..n {
            if self.blobs[i].launched {
                let at = self.blobs[i].pos;
                if (0..n).any(|j| j != i && self.blobs[j].pos.distance_squared(at) < touch * touch) {
                    self.blobs[i].launched = false;
                }
            }
        }

        // Touching pairs bond, unless they're merging or about to.
        let held: Vec<u32> = self.pending.iter().flat_map(|h| h.members.iter().copied()).collect();
        for (i, j) in touches {
            let (a, b) = (&self.blobs[i], &self.blobs[j]);
            if a.merging.is_some() || b.merging.is_some() || self.bonded(a.id, b.id) {
                continue;
            }
            if held.contains(&a.id) || held.contains(&b.id) {
                continue;
            }
            let pair = (a.id.min(b.id), a.id.max(b.id));
            self.bonds.push(pair);
            self.events.push(Event::Bonded { a: pair.0, b: pair.1 });
        }
    }
}

/// A merge of `count` blobs multiplies value by 2^(count − m + 1): it doubles at
/// m and doubles again for each blob over m.
pub fn merged_value(value: u32, count: usize, m: usize) -> u32 {
    let doublings = (count + 1).saturating_sub(m).clamp(1, 31) as u32;
    value.saturating_mul(1 << doublings)
}
