# CLAUDE.md

This file guides Claude Code when working in this repository. Read it, then `PLAN.md`, before starting any session.

contra-ethos-games is the Rust + WebGPU rebuild of two small merge games, **Dice Merge** and **Blob Merge**. They'll be published on contra-ethos.com. Later they'll share one custom UI, also built in Rust and WebGPU. The falling-sand game, [Phosphor-Sand](https://github.com/Ethan23p/Phosphor-Sand), joins this platform later as a separate effort. It's plain JS + WGSL today and is served at sand.contra-ethos.com. The work happens across many sessions, so `PLAN.md` is the coordination point. Each session picks up the next open step there and leaves the plan accurate.

## Sources of truth

1. **The abstract specifications define what the games are.** They're Claude Docs; read them with the docs connector:
   - Dice: https://claude.ai/artifact/Nc16s8DQxjU3PZMAAyTLrx
   - Blob: https://claude.ai/artifact/KNAFDfw4TWWauZNejsusDe

   If code and spec disagree, the spec wins. If the spec is silent or ambiguous, ask Ethan (the user), then update the spec, not just the code. Each spec ends with open questions; don't settle those in code alone.
2. **`PLAN.md` holds the phases, the current step, and the decision log.**
3. **The old JS/TS builds are reference for feel, not for rules.**
   - [Blob-Merge-Game](https://github.com/Ethan23p/Blob-Merge-Game) is React + Canvas, with an in-game config list and top bar.
   - [Dice-Merge-Testbed](https://github.com/Ethan23p/Dice-Merge-Testbed) is checked out at `../Dice-Game`. Its newest work (the main menu, game modes, the Tuning panel and the Sonar merge effect) is on the `claude/merge-pulse-train` branch, not on `main`.

   Use them to match timings, easing, colours and the gloopy merge look. Don't port their structure. The specs deliberately diverge from them in places.
4. **Ethan's own spec.** Ethan will keep a project-wide spec in Logseq, covering instructions and expectations. Once it exists, it ranks alongside the game specs.

## Roles

Ethan is the designer and project manager, and owns the specs. Claude is the engineer, and owns the implementation: architecture, code, tooling, the dev environment and how things get verified. Make engineering calls without asking. Bring design questions (feel, rules, look) back to Ethan as concrete options.

## Vocabulary

Code names follow the specs exactly, so a reader can go from a spec row straight to the code that implements it.

| Term | Meaning |
| --- | --- |
| **value** | The one word for a piece's tier or worth. Dice: 1, 2, 3, … Blobs: 1, 2, 4, 8, … Never "power", "rank" or "level". |
| **mass** | Value as the physics sees it. Dice: `mass(v) = b^(v−1)`; scoring, spawn rarity and animation weight all read it. Blobs: currently uniform, so a high value never gets sluggish. |
| **neighborhood** | A maximal connected set of identical pieces: orthogonal on the dice board; touch or bond on the blob field. |
| **cluster** | Dice only: the 1–3-die polyomino in the queue. Once placed it's just dice. |
| **m** | Merge size, in both games. |
| **survivor** | Dice only: the youngest die in a neighborhood. Blob merges converge on the neighborhood's shared centre instead. |

Prefer fewer variables. Add a value-like quantity beyond value and mass only when the spec needs it.

## Architecture rules

- **Game logic has no rendering or platform dependencies.** `crates/dice` and `crates/blob` are plain Rust: state, rules, a fixed-timestep sim. No wgpu, no winit, no web-sys. They compile and test natively with `cargo test`.
- **Presentation reads logic and never changes rules.** The app crates turn game state and events into animation and draw calls. Animation code never branches on *why* something moved.
- **Determinism:** every bit of randomness comes from a seeded RNG owned by the game state, and the blob physics steps at a fixed dt. With the same seed and the same inputs you get the same game, which is what makes tests and replays possible.
- **The config is the one source of truth for tunables.** Every constant a spec calls tunable lives in one typed config per game, which the config panel and the shaders both read. The code's `Default` is the shipped values; the panel adds named profiles on top. Slider ranges go past what seems reasonable and stop just short of where the game would technically break: overly conservative limits are frustrating when tuning. Expose more controls rather than fewer; guard real breakage (an endless spring, say) in the code, not by narrowing the range.
- **Tests are named after spec policies:** `threshold_*`, `survivor_*`, `yield_*`, … When a policy changes in the spec, its tests change in the same commit.

- **We own the primitives.** Everything on screen, including UI, text and animation, is drawn by our own renderer from shapes, shaders and springs. There's no DOM, no CSS and no layout engine to negotiate with. The HTML page is only a canvas and a loader. The config panel (egui) is the one exception, and it's a scaffold.
- **Anchor, then size relatively.** As in game UI, place each element by an anchor point of the viewport (`engine::Anchor`, via `frame.screen()`) plus an offset, and size it in ems (one em is 16 logical px, the height of body text) or viewport units (`vw`, `vh`, `vmin`), never raw pixels. Text is sized in ems so it stays readable on any screen. A game's play area is a world fitted into the space the HUD leaves (`View::fit_area`); things that belong to the play area (dice, the board) stay in world units.
- **WebGPU only.** No WebGL fallback; a browser without WebGPU gets a short message.

The workspace layout and the reasoning behind it are in `PLAN.md` (Architecture).

## Working with Ethan

- Ethan plays on a phone. Judge feel on a touch screen and under rapid input (spamming launches is a real strategy), not just on desktop.
- Show, don't just tell. After a visual change, capture screenshots or a short recording and send them.
- Ethan likes being offered two or three concrete design options with a recommendation, and dislikes skepticism without a reason behind it.
- The environment is Windows with PowerShell 7 and Git Bash. Commit and push (which deploys) when the changes are ones Ethan explicitly asked for and we're in close communication; never ship anything he isn't aware of without asking.

## Session protocol

1. Read `PLAN.md`. Find the first unchecked step of the current phase.
2. Re-read the spec sections that step touches.
3. Work in small, verified increments: tests for logic, screenshots for visuals.
4. Before ending: tick finished steps in `PLAN.md`, add any decision to the decision log with its date and reason, and note what the next session should do first.

## Commands

Run these from the repo root unless noted. The toolchain is Rust stable plus the `wasm32-unknown-unknown` target and Trunk (`cargo install --locked trunk`). Trunk fetches a matching `wasm-bindgen` by itself.

- Tests: `cargo test --workspace`. Lint: `cargo clippy --workspace --all-targets`. Format: `cargo fmt --all` (120 columns, from `rustfmt.toml`).
- Native window: `cargo run -p sandbox`, `cargo run -p dice-app` for Dice Merge, or `cargo run -p blob-app` for Blob Merge.
- Headless snapshot: `cargo run -p sandbox -- --snapshot <file.png>` renders a strip of frames at fixed times. Look at it after any visual change. Dice: `cargo run -p dice-app -- --snapshot <file.png>` plays a scripted two-step cascade; `--snapshot-drag` shows the drop preview. Blob: `cargo run -p blob-app -- --snapshot <file.png>` shows a seeded field taking launches, the last one in flight.
- Web dev server: `trunk serve` from `apps/<app>/`. The sandbox serves at http://127.0.0.1:8765, dice at :8766 and blob at :8767; port 8080 belongs to Steam on this machine. Trunk must run from the app's folder, because it can't find the crate from the repo root. Each app's `Trunk.toml` watches `../../crates` too, so engine edits rebuild.
- Config panel: the translucent sliders button in a bottom corner (long press moves it to the other corner), or F1 / backquote. A game exposes tunables by implementing `engine::Tunable` on its config struct (its `Default` is the shipped values) and returning its `Tuner` from `App::config`; `App::config_actions` adds buttons such as "New game". The panel (`engine/src/panel.rs`) is modal while open; shut, only the button and the pinned-cards widget take touches. Pinned fields show in the widget as the same cards as in the panel. Profiles, pins, the device default and the profile marked to ship persist in localStorage; current values don't. "Copy values" and "Mark to ship" copy `id = value` lines; paste Ethan's into the config's `Default` to ship them.
- Web release build: `trunk build --release` from `apps/<app>/`, written to that app's `dist/`. The phase-1 sandbox is about 2.7 MB of wasm (1.1 MB gzipped), mostly egui; phase 0 without egui was 295 KB.
- Site build: `bash web/build.sh` builds every app into `target/site/`, which is exactly what Pages serves. Add each new app to the `apps` list there and link it from `web/index.html`.
- Deploy: every push to `main` runs `.github/workflows/pages.yml` (tests, then the site build, then GitHub Pages), taking about 5 minutes. The site is https://play.contra-ethos.com, with each app at `/<app>/`. This is also how Ethan tries builds on his phone. DNS is a Cloudflare CNAME, `play` → `ethan23p.github.io`, set to DNS only.

Gotchas:
- wgpu 30 differs from most examples online. `get_current_texture()` returns a `CurrentSurfaceTexture` enum, presenting is `queue.present(frame)`, pipeline layouts and vertex buffers take `Option`s, and the field is `multiview_mask`. When unsure, read the source in `~/.cargo/registry/src/*/wgpu-30.*`.
- Git Bash rewrites arguments that look like absolute paths, such as `/sandbox/`, into Windows paths. `web/build.sh` sets `MSYS_NO_PATHCONV=1` for Trunk, and keeps `--dist` relative for the same reason.
- epaint panics in debug builds if a `TexturesDelta` is dropped without `clear()`; the runner clears it after applying.
- In the in-app browser, screenshots after a click on the canvas can come back zoomed on the focused canvas. It's the screenshot tool, not the render; check with `getBoundingClientRect` before chasing it.
- Surfaces use a non-sRGB format on purpose, so colours are authored and blended in sRGB the same way on desktop and on the web.
