# One Brush — Project Roadmap

A Flash CS6-inspired 2D animation editor built on a Rust desktop stack,
started fresh (not a CrossAir rewrite) to get away from Paper.js.

**This file is the single source of truth.** Multiple sessions/tools
(Claude, opencode, Kilo Code) have worked on this project in parallel;
stray copies of earlier roadmap drafts exist in Downloads and elsewhere
and should be treated as stale. Always check the real committed version
in the repo before trusting any other summary of project state.

## Architecture (decided)

**Stack:** Pure Rust, targeting native Windows desktop first, browser/Wasm
pipeline deferred to M8.

**Repos:** Two, separate on purpose —
- `github.com/F1reAceYT/rosin` — a fork of TimTom's Rosin GUI toolkit
  (tracks `upstream`), gaining a real Windows platform backend.
- `github.com/F1reAceYT/One-Brush` — the app itself, with
  `quad-engine-core` (the ECS/Ledger/FLA-import crate) as a workspace
  member, not a separate repo, since it only serves this one app.

**Desktop pipeline (dual-GPU-engine):**
- **Lyon + wgpu** — canvas/display engine, VRAM-cached tessellated
  triangles for locked 144Hz stylus tracking. Confirmed via real
  benchmarking to be necessary — a Vello-only canvas consistently missed
  the 144Hz budget under realistic multi-stroke load.
- **Vello (via the Rosin fork)** — UI layer: toolbox, inspector, timeline
  chrome. Also provides text layout for free (`parley`/`fontique`/
  `skrifa`/`harfrust` come in transitively).
- **`i_curve`** — boolean path operations (union/intersection/diff/xor)
  directly on line/quad/cubic edges. **Chosen over the earlier
  `contourklip`+`cxx` C++ bridge plan** after a real evaluation: ~35 LOC
  pure-Rust integration vs. an estimated 150-300 lines of unsafe FFI plus
  a C++ toolchain dependency. Geometric correctness confirmed (area
  identities hold to <0.02%, translation invariance verified). One open
  item: a small corner-join artifact from the default curve re-fit,
  flagged but not yet checked at real editing zoom levels.
- **`linesweeper`** — curve splits and flood fills. Division of labor
  vs. `i_curve` not yet explicitly re-confirmed since `i_curve` was
  adopted — worth checking they don't overlap.
- **`flo_curves`** — fitting a smooth Bézier curve to raw freehand/
  stylus point sequences (a gap none of the above cover). Real
  evaluation blocked until the M4 canvas engine exists to produce raw
  stroke data.
- **`tiny-skia` + Rayon** — multi-threaded offline export rasterization.
- **`glam`** — math primitives (vectors/matrices).
- **`rapier2D`** — joint/constraint solving for armature/Bone tween
  posing. Scope (pure IK vs. also physics-based secondary motion like
  hair/cloth) not yet decided.
- **`splines`** — Motion tween's multi-keyframe `PropertyTrack`
  interpolation. Shape/Classic/Bone tweens use hand-rolled lerp+easing
  instead (required regardless, since `Easing` matches Flash's own
  `-100..100` slider formula, which no crate replicates).
- **Firewheel** (+ default `cpal` backend) — audio engine. **Chosen over
  Kira and Oddio** specifically for its ECS-friendly parameter API,
  matching the Ledger's architecture. `symphonia` used directly
  (alongside Firewheel's own `symphonium`-based file loading) for
  waveform-display decoding, a different access pattern than playback.
- **`image`** — bitmap decode/encode for `FillStyle::Bitmap` and
  imported reference art.
- **Export encoding**: `muxide` (pure-Rust MP4 muxer) + `openh264`
  (H.264 encoding) for video export; `gif` crate for animated GIF.
- **`tracing`/`tracing-subscriber`** — structured logging, replacing the
  ad-hoc `println!` debug pattern used earlier in this project.
- **`proptest`** — property-based testing, aimed at the class of Ledger
  undo/redo bugs already found manually once.

**Project file format:** zip of JSON (mirrors the structure already
proven for FLA import: zip + serde), chosen over a compact binary format
(bincode/postcard) specifically for human-inspectability, given this
project's real, costly experience with unverifiable/opaque state this
session (see M1 history below).

**Alternatives considered and ruled out:** Pathfinder (unmaintained,
non-`wgpu` graphics backend — Vello is its closer, maintained spiritual
successor), skia-safe/femtovg/vger-rs (canvas engine), React/JS/Tauri and
C++ (overall stack), Kira/Oddio (audio — see above), `keyframe`
(interpolation — see `splines` above).

**Still open / not yet decided:** `rapier2D`'s exact scope; whether
`linesweeper` and `i_curve` have genuinely non-overlapping jobs;
`parley`'s support for mixed-style text runs within one field (needed
for Flash-style rich text) not yet verified against real data;
`i_curve`'s corner-artifact at real editing zoom and its actual runtime
latency (only compile time has been measured so far).

## Milestones

### ✅ M0 — Architecture & stack decisions
Complete. See "Architecture" above for the full, current list — this
section superseded an earlier, since-reversed plan that had locked in a
C++ (`bezier`+`contourklip` via `cxx`) math engine; that plan was
evaluated against `i_curve` and dropped.

### ✅ M1 — Rosin Windows fork: functionally complete, one active bug
- [x] Windowing & input (message loop, WndProc, mouse/kb translation)
- [x] Fixed the `hit_test` layout crash (`layout_cache` empty-vec panic)
      — a genuine, Windows-agnostic bug in `rosin-core`, worth an
      upstream PR
- [x] Full wgpu/Vello rendering pipeline, including the custom-overlay
      compositor path (`OverlayPipeline`) — found and fixed a real
      surface-format bug along the way (`compatible_surface: None`
      caused empty adapter capabilities)
- [x] Full IME composition pipeline (IMM32, `WM_IME_*`, candidate window
      positioning) — visually confirmed working for CJK input
- [x] Widget suite validated (`widgets_demo`: textbox, dropdown,
      scrollarea, slider, dragvalue, progressbar, checkbox, tabs) — also
      fixed a real machine-specific Vulkan-loader issue (Epic EOS
      overlay) via a `WGPU_BACKEND` env override
- [x] Multi-window display sync (`VirtualFrames` — detached
      Color Mixer/Navigator panels as versioned virtual sub-layouts)
- [ ] **Active bug**: a Flash-8-light-theme re-skin of `app_shell_demo`
      left the menu bar and mode-tab row still dark/black — the rest of
      the shell (toolbox, canvas, inspector, timeline) correctly
      re-themed, but the header didn't. Needs investigation (CSS
      specificity/selector mismatch, or hardcoded colors overriding it).
- [ ] Remaining Phase-4 feature parity: clipboard, file/alert dialogs,
      native context menus, AccessKit accessibility

### ✅ M2 — Core Animation Data Layer: fully spec'd and decided
- [x] Path/shape model spec: `Shape` → `Contour`s → `Edge`s
      (`Line`/`Quad`/`Cubic`, all kept — not normalized to cubic-only —
      to preserve CS6 data losslessly); dual-sided fills
      (`fill_left`/`fill_right`, matching Flash's `fillStyle1`/`2`);
      twips as the stored unit.
      **Real reconciliation finding**: `edge.rs`'s actual implementation
      normalizes to cubic-only (discarding the quadratic fallback tail),
      which conflicts with the spec's stated dual-representation intent.
      Resolved as acceptable: the quadratic tail is a lossy derived
      approximation, not additional source information, so discarding it
      loses nothing about the curve's real shape — only a byte-perfect
      re-export of the original quadratic fallback would need it, which
      isn't a real goal. Spec should be updated to match the code.
- [x] Timeline model spec: per-layer independent frame length; all four
      tween types (Shape/Classic/Motion/Bone) as distinct data — Shape
      tween scoped as the first to actually implement (matching-topology
      only, hints deferred), since it's the only type not blocked on
      Symbols.
- [x] Symbols/instances model spec: Graphic/MovieClip/Button, uniform
      `Instance` placement, MovieClip as home for Armature/Bone/
      JointConstraints (resolving `BoneChainId = ArmatureId` and the
      Classic tween's `sync` flag), Button as pure four-state data,
      interactivity explicitly out of scope.
- [x] Project file format: zip of JSON (see Architecture above).

**Real implementation status** (as of the last directly-witnessed build):
`quad-engine-core` builds and tests clean — **17/17 tests passing**
(10 lib + 7 edge parser), verified via real terminal output after fixing
two real, separate problems: a missing Windows SDK (blocking all native
linking on this machine) and a regressed/never-actually-fixed borrow
error in `ledger.rs`'s `undo_entity`/`redo_entity`. Treat any higher
test-count claims (19, 27, 28) from earlier unverified session summaries
with caution until re-confirmed the same way.

### ✅ M3 — App shell: built, screenshot-verified, mid re-skin
The full shell layout exists and has been directly confirmed via
screenshots, not just claimed: menu bar (11 items), mode tabs (Project/
Audio/Paint/Animate/Composite/Edit — confirmed as the real six; Toggle/
Mixer/Mixer:Float were leftover dev buttons, since removed), left tool
strip (now a 2×3 grid with grouping), centered stage/artboard, right
dock (Color Mixer with real RGB/hex/alpha, Swatches, Align/Transform
with real X/Y/W/H/Rotation fields), and a timeline dock (layer rows with
working eye/lock/outline icon columns, frame ruler, draggable playhead —
manual drag confirmation still pending from the user).

Currently mid-reskin from the original dark "Animate CC" theme to a
classic Flash 8 light silver chrome — mostly complete, but the menu bar
and mode-tab row are still rendering dark (see M1's active bug above,
same root cause, tracked once so it isn't duplicated).

### 🔲 M4 — Canvas engine (next major milestone)
- [ ] Lyon+wgpu canvas surface inside the real (not synthetic-benchmark)
      Rosin-hosted shell
- [ ] Re-validate stylus latency with real UI competing for frame time
      (the original benchmark used a standalone `winit` prototype, not
      the actual app)
- [ ] Basic drawing tools (pen, brush) writing into the path model
- [ ] This is also where `flo_curves` (stroke fitting) and the
      `i_curve`/`linesweeper` division of labor get their first real
      test against actual usage, not synthetic data

### 🔲 M5 — Vector logic
- [ ] `i_curve` integration for boolean fills (superseding the original
      `cxx`/`contourklip` plan referenced in earlier drafts of this
      roadmap — see M0)
- [ ] Real editing operations: reshaping paths, boolean-style fills

### 🔲 M6 — Timeline & animation
- [ ] Frame-by-frame authoring
- [ ] Shape tween (first), then Classic/Motion/Bone per the spec's
      dependency order
- [ ] Symbols/instances on the timeline
- [ ] Audio timeline sync via Firewheel (Ledger stores plain placement/
      sync-mode data; Firewheel only gets touched at playback time)

### 🔲 M7 — Export
- [ ] tiny-skia + Rayon offline rendering pipeline
- [ ] Video export via `muxide` + `openh264`; GIF via the `gif` crate

### 🔲 M8 — Browser pipeline (deferred)
- [ ] Vello/WebGL2/Wasm build; `linesweeper` (and `i_curve`, which is
      `no_std`/Wasm-friendly) compiled to Wasm; Firewheel's `cpal`
      backend already supports `wasm32` via Web Audio API

---
**Current position:** M0-M2 fully settled. M1 and M3 are functionally
complete with one shared active bug (the light-theme header re-skin) and
some verification/feature-parity items remaining. **M4 (the real canvas
engine) is the next major milestone** — everything since the shell UI
polish has been architecture/engine-selection work, not new app
functionality, so this is where the app starts being something you can
actually draw in.
