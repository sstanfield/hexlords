# HexGame

A hex map strategy game foundation in Rust. Phase 1 (done) is a port of the C++
hexGame engine demo (<https://github.com/sstanfield/hexGame>, read at `99edad5`):
a 100 x 100 hex map, one unit, a movement range, a minimap. The UI is Turbo
Ferret (<https://github.com/sstanfield/turbo-ferret>) and graphics go through
miniquad. `hexgame-rust-port-plan.md` is the brief: the acceptance spec of the
demo, the design, and the direction for the phases after it.

**Fidelity to the C++ is not a goal.** The owner's word (2026-11-18): the C++
was an in-progress demo, and the port exists to be the foundation of a new game,
so it need not be pixel or bug perfect. Where the C++ behaviour is a bug or
plainly worse, do the sane thing and record it in `docs/deviations.md`.

**Read before changing anything substantial:** `hexgame-rust-port-plan.md`
(sections 2 and 3 for what exists, section 8 for what comes next) and
`docs/deviations.md` (where the Rust differs from the C++ on purpose, numbered).

## Layout

| Path | What |
|---|---|
| `src/state/` | Map, tiles, units, cities, movement range. No UI, no graphics |
| `src/game.rs` | `Game`: the map plus the selected unit and its range; the demo setup; `select_hex` |
| `src/view.rs`, `src/minimap_view.rs` | Pure math: zoom, scale factors, visible hexes, picking, recentering; the minimap's fit and marker |
| `src/render/` | miniquad drawing: hex meshes, shaders, textures, the map and minimap renderers, the PNG writer |
| `src/gui/` | Turbo Ferret widgets: `MapWidget`, `MiniMapWidget`, `RootWindow`. They record commands; they hold no game state |
| `src/session.rs` | `Session`: the `Ui`, the `Game` and the views together, with no window. Tests drive it |
| `src/script.rs` | Scripted input (`--script`), for screenshots and tests |
| `src/app.rs`, `src/main.rs` | The miniquad window: forwards input to the session, draws the frame |
| `assets/` | `layouts/main.tb.txt`, Turbo Badger's default skin, tiles, the unit image |
| `tests/` | Headless tests of the layout, the widgets and the interaction |
| `docs/` | `deviations.md`; `screenshots/` (what the game looked like when phase 1 was done) |
| `reference/hexGame/` | The C++ original, for reference. **Read-only, never edit it.** Gitignored: if it is missing, `git clone https://github.com/sstanfield/hexGame reference/hexGame` |

## Commands

Every change ends with all of these passing:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Other useful ones:

```sh
cargo run                                   # the game in a window
cargo run -- --frames 30                    # smoke test: draws 30 frames and quits
cargo run -- --screenshot target/shot.png   # draws the start view into a PNG and quits
cargo run -- --screenshot target/shot.png --script "enter,1,1,cursor:0:0"   # after some input
cargo +1.86 check --all-targets             # MSRV
```

`cargo test` needs no GPU and no display. The screenshot mode needs both (on
Linux CI: `xvfb-run`); it is how an agent sees the game, since nobody can click
the live window for you. Look at the PNG you made.

## Rules

- **Dependencies:** the runtime dependencies are `miniquad`, `turbo-ferret` and
  `turbo-ferret-miniquad`. Nothing else: no `image`, `png`, `glam`, `serde`,
  `log`, `rand`. PNGs are decoded by Turbo Ferret's decoder and written by the
  small encoder in `src/render/png_out.rs`. A dev-dependency is allowed when a
  test really needs one. Turbo Ferret is pinned by `rev`; bump it on purpose. A
  `[patch]` to a local checkout is fine for local work, but don't commit it.
- **Rust:** edition 2024, MSRV 1.86 (no let-chains), rustfmt width 120.
  `#![forbid(unsafe_code)]`.
- **Credit and licence:** ported code cites its origin in a comment,
  `// C++: src/state/mapmove.cpp Map::selectUnit`. Keep `LICENSE` (the zlib
  notice of the C++) and `NOTICE` (Turbo Badger, the skin, the DejaVu font
  licence, which must ship with binaries).
- **Deviations:** when behaviour differs from the C++ on purpose, add a
  numbered entry to `docs/deviations.md`.
- **Layering:** `state` and `game` know nothing of the UI or the GPU. Widgets
  never hold game state: Turbo Ferret factories are `fn() -> Box<dyn Widget>`,
  so a widget records commands and the session applies them (the UI produces
  orders, the game consumes them). Keep it that way; phase 3 builds on it.
- **Platforms:** macOS and Linux are the test targets; Windows is best effort.
  Use `std::path`, no Unix-only APIs, no `#[cfg(unix)]`-only features. Only
  miniquad's OpenGL backend is supported (the game's shaders are GLSL 100).
- **Tests:** a fix comes with a test that fails without it. Anything a player
  sees gets a screenshot, and you look at the picture.
- **Commits:** on `main`, small, described by what changed and why.
- **Later phases** take the feel of 1990s fantasy strategy games but must not
  clone one: no names, factions, lore, maps, tables or art from an existing game
  (plan, section 8).

## Things that bite

1. **Hex geometry.** Flat-topped hexes in offset columns ("odd-q"): odd columns
   sit half a hex *higher* on screen, and map rows grow *downward* while the GL
   y axis grows upward. The neighbour rule, the visible range and the pick
   formula all depend on it; `src/state/hex.rs` and `src/view.rs` say how.
2. **The map is drawn by the host, not by a widget.** Turbo Ferret widgets paint
   through a batched, read-only `PaintCtx`, so `MapWidget` and `MiniMapWidget`
   paint nothing: `App::draw` draws the map into their rectangles first and the
   UI on top (plan, section 3.3). A map that must sit *between* UI layers needs
   a new hook in Turbo Ferret.
3. **Pixels are framebuffer pixels.** On a HiDPI screen the window is 1024 x 768
   points but 2048 x 1536 pixels, and the UI, the layout and the mouse all use
   the pixels.
4. **A widget is busy inside its own hook.** `ui.get::<T>(id)` and
   `ui.with_widget` on the widget whose hook is running give `None`. Read
   Turbo Ferret's `CLAUDE.md` ("The five things that bite") before writing
   widget code.
5. **Keys go to the focused widget only**, and bubble up from there. The map is
   focused at start; a click on the minimap or the button moves the focus, and
   then the arrows do nothing until the map is clicked again (as in the C++).
