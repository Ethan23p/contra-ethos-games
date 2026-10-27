# Implementation plan

The living plan for rebuilding Dice Merge and Blob Merge in Rust + WebGPU and publishing them on contra-ethos.com. Tick steps as they land, and log decisions at the bottom.

**Current phase: 3, Blob Merge (look and feel).** `crates/blob` has the field, its physics and every spec rule (`Game`, `Rules`), with policy tests. `apps/blob` at play.contra-ethos.com/blob/ is playable end to end; Ethan's phone pass (2026-10-26) found the feel good, rapid fire included. The gloopy look, effects and glimmer are in (2026-10-26). Next: Ethan's phone pass on the look, then the overflow warning.

## Goal state

- Both games are playable at contra-ethos.com, in the browser and on a phone, rendered with WebGPU and matching their specs.
- One shared engine crate covers the GPU, the frame loop, input and 2D drawing, so a third game costs only its rules and its look.
- The games later move into one custom shared UI (menus, settings, tuning) built on the same engine. That comes in later sessions; the plan only keeps the path open.

## Architecture

```
contra-ethos-games/            Cargo workspace
├─ crates/
│  ├─ engine/                  wgpu device + surface, frame loop, unified pointer/touch input,
│  │                           instanced SDF shape renderer (circles, rounded rects), text later
│  ├─ dice/                    Dice Merge rules: pure Rust, no rendering deps
│  └─ blob/                    Blob Merge rules + fixed-step physics: pure Rust
├─ apps/
│  ├─ sandbox/                 engine playground: rendering experiments, pipeline checks
│  ├─ dice/                    dice logic + engine → native window and wasm
│  └─ blob/                    blob logic + engine → native window and wasm
├─ web/                        HTML shells, the build script, output for the site
├─ CLAUDE.md
└─ PLAN.md
```

Why it's split this way:
- **Logic sits apart from the renderer.** The rules can then be tested natively and quickly against the spec, and they survive any renderer or UI change. It also matches how the specs separate Entities and Capabilities from presentation.
- **Every app runs natively as well as in the browser.** Native builds iterate faster and give clearer errors. They also let us render headless to PNG, so Claude can check visuals without a browser (the screenshot-strip approach that works well in Phosphor-Sand).
- **The site stays static.** contra-ethos.com stays plain HTML and CSS. This repo builds static bundles (`.wasm` + JS glue + HTML), and they get copied in under something like `contra-ethos.com/play/<game>/`.

## Phases

### 0. Foundations: prove the whole pipeline with a trivial frame
- [x] 0.1 Toolchain: update Rust, add the `wasm32-unknown-unknown` target, and install Trunk.
- [x] 0.2 Workspace skeleton: the crates and apps above, all empty, with `cargo test --workspace` green.
- [x] 0.3 `engine`: a wgpu surface that clears to a colour and draws animated SDF circles, natively and in the browser, with resize and device-pixel-ratio handling. Shown in `apps/sandbox`.
- [x] 0.4 Phone check: Ethan opens https://play.contra-ethos.com/sandbox/ on his phone and confirms it renders.
- [x] 0.5 Deploy smoke test: a push to `main` builds the site and publishes it to play.contra-ethos.com through GitHub Actions and Pages.
- [x] 0.6 Headless snapshot: render a frame offscreen to PNG natively, for visual checks.
- [x] 0.7 Fill in the Commands section of `CLAUDE.md`.

### 1. Engine minimum
- [x] Instanced shape renderer: circles and rounded rects with fill, outline, glow, rotation and scale; colour and alpha per instance; one pass, one draw call (`shapes.rs`, `shapes.wgsl`).
- [x] Unified pointer input: mouse and touch, tap, drag, hold, mapped to world coordinates through `View` (`input.rs`).
- [x] Frame loop with a fixed simulation step (120 Hz default) and interpolated rendering via `Frame::alpha`; headless snapshots step the same way (`runner.rs`, `headless.rs`).
- [x] Text: SDF glyph atlas built at startup from Fredoka SemiBold, drawn by the shape shader, so text gets outlines and glow too; digits are tabular (`text.rs`).
- [x] Dev tuning panel in egui: `Tunable` + `Tuner` (initial / default / current, presets, per-field reset, Copy values as text, saved to localStorage). Open with the dots top right or F1 (`tune.rs`).
- [x] Config panel v2 (Ethan's flexible config panel spec): floating corner button, parameter cards with pins, option island with profiles (load/save all or pinned, keep on device, make default), pinned-cards widget. Fixes the freeze and input leaking of v1 (`panel.rs`, `tune.rs`).
- [x] Practical pass on the dice settings: snapback is time + bounce instead of raw stiffness/damping (it could oscillate forever at zero damping); the pop glow stops growing past impact 4 (it swamped the board at high values). Mass-scaled durations keep their caps: weight is exponential, so a value-12 piece would turn for 1.35 s.
- [x] Config panel to Ethan's revised spec: cards read `Label  current (profile)`, the device default and "to ship" are separate marks, pinned cards in the widget are the full cards. Ethan's tuned values shipped as the dice defaults.
- [x] Layout standard: `engine::Screen` anchors and units; the dice score band is anchored to the top in ems with the play area fitted below; the panel is sized in ems; the pinned widget is compact rows at the top for dice (`App::config_widget_top`).
- [x] Phone check of the Phase 1 sandbox: tap, drag, hold, panel, Copy values. Ethan's tuned gravity (420) is now the sandbox default.

### 2. Dice Merge
The discrete game comes first: it's the cheapest way to prove the spec → code → test loop.
- [x] `crates/dice`: entities, the capabilities (Spawn, Rotate, Place, Merge) and every policy, with one test group per policy. All constants in `Rules`; the oldest-survivor experiment is `Rules::survivor`.
- [x] Settling runs as a synchronous fixed-point loop that returns a step timeline for presentation (`Turn::steps`: Place, Fall, Merge per generation, Spawn, Replace, End).
- [x] `apps/dice`: board, queue, drag-to-place with rotate, merge and gravity animation weighted by mass. Dice show pips up to 6 and digits from 7. The merge plays as the old build's pulse train: dice leave one at a time along the neighborhood, each landing rings, then the survivor pops. Placing mid-animation skips the rest of the last turn.
- [x] Tunables for every policy constant, board size, gravity on/off and its direction, plus every timing and look constant (`apps/dice/src/config.rs`).
- [x] Spec open experiment: a toggle so merges converge on the oldest die instead of the youngest (Rules panel).
- [x] Feel pass on the phone against the old build (Ethan, 2026-10-23).

### 3. Blob Merge
- [x] `crates/blob`: blobs, bonds, merges and the field; Move (Lennard-Jones with a soft core and force cap, the saturating well at the tracked centre, damping, bond springs), Merge (convergence on the shared centre, joins, the light push), Launch and Seed, at a fixed dt with substeps.
- [x] Purge: blobs below the pool's floor leave the field, unless they're merging or about to.
- [x] Policies with tests: motion, identity, threshold and m, bonding, merging (convergence on the shared centre, joins during convergence, bigger jumps for bigger merges), launching, seeding.
- [x] Policies with tests: pool window, draw, scoring and combo, nova, ending (`Game`, `Rules`).
- [x] The gloopy merge look: `engine::Goo`, a smooth-union SDF pass. Identical blobs fuse within the blend, merging blobs flow together, bonds draw as tethers; each pixel is drawn by its nearest circle only, so groups render once without seams, in one draw call (`goo.rs`, `goo.wgsl`, `apps/blob/src/look.rs`).
- [ ] The overflow warning, beyond the rim's red pulse.
- [x] Physics presets and live tuning: every `Physics` constant in the panel; the old build's Light / Flowey, Thick Liquid and Clay Clumps as presets.
- [x] Physics sandbox (`apps/blob`): plain circles, bonds as bridges, aim and release to launch. Tuned by Ethan (2026-10-24).
- [x] The rules in the app: score and combo readout, the pool, points rising from merges, purge and nova placeholders, the rim pulsing as a blob runs down the grace, game over and tap to play again.
- [x] Feel pass on the phone, including rapid-fire launching (Ethan, 2026-10-26). Game over and its 6.5 s grace feel right.
- [x] Pool feel: the floor rose a bit too quickly, so the pool now widens every 3 doublings instead of 6.
- [x] Combo by merges, not time: it counts merges over the last 3 launch windows, replacing the 1.4 s reset.
- [x] Effects, hand-rolled: a merge lands big and wobbles to size, with a splash ring and droplets; a purge greys and deflates, shedding drops; a nova flashes, sends a gold shockwave and droplets, and the new seed grows in outward. The opening seed grows in too.
- [x] Glimmer: peak blobs get a gold rim, a sheen that sweeps across and waits out of sight before looping, and glints that move only while dark. Every look constant is in the panel's Look and Glimmer groups.

### 4. Publish
- [ ] Release builds: size budget, loading screen, a message when WebGPU isn't available.
- [ ] Both games under `contra-ethos.com/play/`, linked from the home page.

### 5. Shared UI (later sessions)
- [ ] Design pass: menus, settings, tuning, game switching, one visual language.
- [ ] Build it on `engine`, replacing the dev panel for players.

### Later: Phosphor-Sand joins the platform
A separate undertaking under the same umbrella. It's plain JS + WGSL today, so joining means porting the host to Rust/wgpu; the WGSL shaders can largely carry over. Plan it once phases 0–1 have settled the engine.

## Open decisions

None right now.

## Decision log

| Date | Decision | Why |
| --- | --- | --- |
| 2026-10-22 | Rebuild both games in Rust + WebGPU (wgpu) in a new monorepo, `contra-ethos-games`. | A shared engine and later a shared UI; contra-ethos.com stays a static site. |
| 2026-10-22 | The specs are the source of truth; the old builds are reference for feel only. | The specs were deliberately revised ahead of the code. |
| 2026-10-22 | Game logic crates have no rendering dependencies. | Native tests against the spec, and freedom to change the renderer or UI. |
| 2026-10-22 | WebGPU only; no WebGL2 fallback. | Ethan doesn't need older browsers, and WebGPU now ships in every major browser. One backend means one set of shaders to tune. |
| 2026-10-22 | Trunk is the web build tool. | Dev server with rebuild on save, and a static `dist/` that drops into any host. |
| 2026-10-22 | egui for the dev tuning panel, standardised across both games on the dice schema (initial / default / current per tunable). | Fast to build and works on the web; Ethan already used in-game tuning in both old builds to find good values. |
| 2026-10-22 | Host at play.contra-ethos.com: GitHub Pages from this repo (public), built by Actions on every push to `main`. | The same pattern as sand.contra-ethos.com; each push puts a build on Ethan's phone with no tunnel. |
| 2026-10-22 | Everything players see is drawn with our own primitives: no DOM or CSS UI. | Full control over animation and feel; CSS fought the dice build. |
| 2026-10-22 | Rendering feedback loop: headless PNG strips for Claude, the live phone link for Ethan, and every look/feel constant in the dev panel. Ethan tunes on the phone and pastes "Copy values" text back; those become the new initials. | Ethan judges feel on a phone; Claude needs to see frames without a browser. |
| 2026-10-22 | One SDF pipeline for shapes and text; text from a runtime-built distance-field atlas of Fredoka SemiBold (OFL, the old dice display font). | Outlines, glow and any size for free, in one draw call that keeps draw order. The font is a placeholder until the visual design pass. |
| 2026-10-22 | The egui panel uses the game font, not egui's bundled fonts, and ships in every build for now. | Saves about 2 MB of wasm; Ethan tunes on the deployed build. Gate it out of player builds in phase 4. |
| 2026-10-23 | Config panel v2: egui sees every window event and runs every frame; the overlay decides who owns a press when it goes down (open panel: all; shut: only the button and widget, from last frame's rects). "Make default" sets this device's startup profile and copies its values for Claude to ship. New profile names come from the browser's prompt, since the canvas can't raise a phone keyboard. | v1 fed egui only while open, so a press that straddled opening or closing left egui with a stuck pointer (the freeze), and ownership came from last frame's `consumed`, so fresh taps leaked to the game. |
| 2026-10-23 | UI is placed by viewport anchor and sized in ems or viewport units, never raw pixels. | Ethan's standard, borrowed from game UI: one layout reads right on every phone and desktop, and text never shrinks below readable. |
| 2026-10-22 | Place ages dice by Manhattan distance from the anchor. | For 1–3-cell clusters it's the path distance through the cluster; Chebyshev would make an L's far end and corner tie. |
| 2026-10-22 | Every Place spawns, the final one included; `End` follows `Spawn` in the timeline. | Supply says each Place removes the head and appends a new cluster, so the queue length is always fixed. |
| 2026-10-22 | A dead queued cluster is re-rolled from the normal distribution up to 64 times, then falls back to a single die. | Keeps the spawn distribution where possible; a single die fits any empty cell, and a full board ends the game anyway. |
| 2026-10-22 | `rustfmt.toml`: 120 columns, `use_small_heuristics = "Max"`. | Matches how the engine was already written; `cargo fmt --all` keeps it consistent. |
| 2026-10-23 | Dice Spawn draws shapes uniformly up to rotation (I and L trominoes equally likely), then turns each a random number of quarter-turns. | Ethan's call over the old build's fixed-orientation odds; spec updated. |
| 2026-10-23 | Both spawn variety rules are on by default: exactly one pair in every 3-die cluster, no repeated value otherwise. | Ethan's call; spec updated. |
| 2026-10-23 | Dice show traditional pips up to 6, digits from 7. | Ethan's call. |
| 2026-10-23 | Placing while a turn still animates jumps that animation to its end. | Rapid play never waits on animation, and the board on screen never disagrees with the rules for long. |
| 2026-10-23 | The dice app's package is `dice-app` (binary `dice`), since the rules crate is `dice`. | Cargo package names must be unique in a workspace. |
| 2026-10-23 | Blob physics: a custom integrator in `crates/blob`, not a physics crate. | The forces are bespoke (Lennard-Jones, a saturating well, bond springs), and determinism matters for tests. |
| 2026-10-23 | Blob world units are the old build's pixels on a 600-unit field; its per-16 ms constants convert by ÷ 0.016 (damping: −ln(keep) ÷ 0.016). | The old presets carry over exactly as starting points. |
| 2026-10-23 | The launch speed is set by its reach: speed = reach × ring × damping. | Under linear damping a blob coasts speed ÷ damping, so the launch lands where intended whatever the fluid preset. |
| 2026-10-24 | Pool window: below a peak of 8 the window stays at 3 values rather than narrowing (the spec's floor((log₂ peak − 3) / 6) goes negative there). | A narrower window would offer only 4s and 8s on a field whose peak is lower. |
| 2026-10-24 | A queued entry the pool no longer allows re-rolls its value and keeps its group. | The player may already be aiming for that colour. |
| 2026-10-24 | Purge skips blobs that are merging or held for a merge; the merge finishes first. | Removing a blob mid-merge would cancel a merge the player already earned. |
| 2026-10-24 | Nova seeds around the last blob; a field emptied outright (only possible by purging) reseeds as a new game, without the bonus. | The spec covers one blob remaining, not none. |
| 2026-10-24 | Ending times each blob separately, by its centre's distance from the field centre; once over, the field keeps moving but the rules and launches stop. | Matches "any blob stays outside the rim for a continuous grace period". |
| 2026-10-26 | Seed opens with values 2–8. | Seeded 1s fell below the opening pool and were purged at once; a 1 also stood out too much visually. |
| 2026-10-26 | Values start at 2 everywhere: Seed, the pool's floor, and a nova's half value. | Ethan: a 1 stands out too much. The spec now says so. |
| 2026-10-26 | The pool's ceiling stays at half the peak; it widens every 3 doublings instead of 6. | Ethan's restatement describes the same shape; his floor felt like it rose too fast. |
| 2026-10-26 | The combo is 1 plus the merge credit (n − 1 per merge) over the last 3 launch windows; a window runs from one launch to the moment before the next. Merges before the first launch count in an opening window. | Ethan: combo by merges, not time. Waiting never resets it, and spamming launches moves old merges out rather than inflating it. |
| 2026-10-26 | Draw stays 1 / 1.25^i: over doubling values that is a power law, weight ∝ value^−0.32. | Ethan asked for a power law; the geometric weight already is one. |
| 2026-10-26 | Glimmer shows only once the peak is above the pool; gold blobs keep their group colour, warmed, with a gold rim. | At the start the peak (8) is in the pool and half the field would glimmer. Only identical blobs merge, so the group must stay readable. |
| 2026-10-26 | Goo groups fuse identical blobs whose surfaces come within the blend, plus merging and bonded ones; other blobs keep their gap. | Fusing only what can merge keeps the look honest to the rules. |
| 2026-10-27 | Dice supply is three staging slots, any of which may be placed; a placed slot restocks in place (`Rules::slots`). The board fills the screen width (or the height left after a minimum staging band), and staged clusters size to fit their slot in any rotation, never larger than on the board. | Ethan's call: choice instead of a fixed queue, and no next-in-queue preview. |
