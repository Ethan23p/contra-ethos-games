# Implementation plan

The living plan for rebuilding Dice Merge and Blob Merge in Rust + WebGPU and publishing them on contra-ethos.com. Tick steps as they land, and log decisions at the bottom.

**Current phase: 1, engine minimum (built; awaiting Ethan's phone check).** Next step: Ethan tries the Phase 1 sandbox on his phone (tap, drag, hold, the dev panel). Then phase 2, starting with `crates/dice`.

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
- [ ] Phone check of the Phase 1 sandbox: tap, drag, hold, panel, Copy values.

### 2. Dice Merge
The discrete game comes first: it's the cheapest way to prove the spec → code → test loop.
- [ ] `crates/dice`: entities, the capabilities (Spawn, Rotate, Place, Merge) and every policy, with one test group per policy.
- [ ] Settling runs as a synchronous fixed-point loop that returns a step timeline for presentation.
- [ ] `apps/dice`: board, queue, drag-to-place with rotate, merge and gravity animation weighted by mass.
- [ ] Tunables for every policy constant, board size, gravity on/off and its direction.
- [ ] Spec open experiment: a toggle so merges converge on the oldest die instead of the youngest.
- [ ] Feel pass on the phone against the old build.

### 3. Blob Merge
- [ ] `crates/blob`: entities, then Move (Lennard-Jones forces, the well, damping, bond springs), Merge, Purge and Seed, at a fixed dt with substeps.
- [ ] Policies with tests: threshold and m, bonding, merging (convergence on the shared centre, joins during convergence, bigger jumps for bigger merges), pool window, draw, scoring and combo, seeding and nova, ending.
- [ ] `apps/blob`: the launcher ring and aiming, near-instant reload, the gloopy merge look (likely a metaball or smooth-union SDF pass), the rim and the overflow warning.
- [ ] Physics presets and live tuning.
- [ ] Feel pass on the phone, including rapid-fire launching.

### 4. Publish
- [ ] Release builds: size budget, loading screen, a message when WebGPU isn't available.
- [ ] Both games under `contra-ethos.com/play/`, linked from the home page.

### 5. Shared UI (later sessions)
- [ ] Design pass: menus, settings, tuning, game switching, one visual language.
- [ ] Build it on `engine`, replacing the dev panel for players.

### Later: Phosphor-Sand joins the platform
A separate undertaking under the same umbrella. It's plain JS + WGSL today, so joining means porting the host to Rust/wgpu; the WGSL shaders can largely carry over. Plan it once phases 0–1 have settled the engine.

## Open decisions

- **Physics:** a custom integrator, ported from the spec, or a crate like `rapier2d`. I lean custom: the forces (Lennard-Jones, a saturating well, springs) are bespoke, and determinism matters.

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
