# HexGame

A hex map strategy game foundation in Rust. Phase 1 (done) is a port of the C++
hexGame engine demo (<https://github.com/sstanfield/hexGame>, read at `99edad5`):
a 100 x 100 hex map, one unit, a movement range, a minimap. The UI is Turbo
Ferret (<https://github.com/sstanfield/turbo-ferret>) and graphics go through
miniquad. `hexgame-rust-port-plan.md` is the brief: the acceptance spec of the
demo, the design, and the direction for the phases after it. Phase 2 has begun:
there is a documented map format (`docs/map-format.md`: terrain, roads, the
map's own kinds of unit, its cities), which the game plays with `--map`; a map
editor (`tools/mapeditor`); and the tiles, the roads and the units' signs are
drawn by a program (`tools/assetgen`, `docs/tiles.md`). Units move turn by
turn; there are no rules for fighting, building or whose turn it is yet.

**The C++ is history, not a specification.** The owner's word (2026-11-18):
the port is done and the game now goes its own way, so nothing has to match the
C++ any more. `docs/deviations.md` records how the port differed from it and
what has changed since; it is closed, and new work does not add to it. Read the
C++ to understand why something is the way it is, not to decide what to do.

**Read before changing anything substantial:** `hexgame-rust-port-plan.md`
(section 3 for the design, section 8 for what comes next; section 2 describes
the C++ demo and is no longer binding).

## Layout

| Path | What |
|---|---|
| `src/state/` | Map, tiles, roads, units and their kinds, cities, movement range; `scenario`, the reader of a map's files. No UI, no graphics |
| `src/game.rs` | `Game`: the map plus the selected unit and its range; the demo setup; `select_hex` |
| `src/view.rs`, `src/minimap_view.rs` | Pure math: zoom, scale factors, visible hexes, picking, panning, recentering; the minimap's fit and marker |
| `src/render/` | miniquad drawing: hex meshes, shaders, textures, the map and minimap renderers, the PNG writer |
| `src/gui/` | Turbo Ferret widgets: `MapWidget`, `MiniMapWidget`, `RootWindow`. They record commands; they hold no game state |
| `src/platform.rs` | What miniquad leaves out: bringing the window to the front on macOS (the crate's one `unsafe` block) |
| `src/screen.rs` | `MapScreen`: the `Ui`, a map and the two views together, with no window: what the game and the tools share. `Scene`: what the window can run |
| `src/session.rs`, `src/info.rs` | `Session`: the game on a map screen (its layout, what a click and Enter do, the panels around the map). `info`: what the panels say, as plain functions of the game's state. Tests drive both |
| `src/script.rs` | Scripted input (`--script`) for any scene, for screenshots and tests |
| `src/app.rs`, `src/main.rs` | The miniquad window (`App<S: Scene>`, `app::run`): forwards input to the scene, draws the frame. `main.rs` is the game's command line |
| `tools/mapeditor/` | The map editor, a workspace member on the game's library: `Editor` (a scene: the sidebar, painting, files), `brush`, `history` (undo), `check` (`--check`) |
| `tools/assetgen/` | The picture generator, a workspace member with no dependencies: `terrain` (a function for each terrain's art), `noise` and `scatter` (what repeats on the hex grid), `tile` (the image, the atlas), `overlay`, `road` and `icons` (the pictures that go over a tile), `preview` (maps drawn without the game), `png` (the writer) |
| `assets/` | `layouts/main.tb.txt` (the game), `layouts/editor.tb.txt` (the editor), Turbo Badger's default skin and `skin/hexgame.tb.txt` (our changes, loaded on top), `tiles/`, `roads/` and the signs in `units/` (what `assetgen` makes, and nothing else), `units/default_unit.png` (the C++ demo's), `maps/` (`*.map` files and map directories) |
| `tests/`, `tools/*/tests/` | Headless tests of the layouts, the widgets and the interaction; of the generator against the game |
| `docs/` | `map-format.md` (the map file, for people and agents who write maps); `tiles.md` (the tiles, for those who draw them); `deviations.md`; `screenshots/` |
| `reference/hexGame/` | The C++ original, for reference. **Read-only, never edit it.** Gitignored: if it is missing, `git clone https://github.com/sstanfield/hexGame reference/hexGame` |

## Commands

Every change ends with all of these passing:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

(The repository is a workspace: the game is the root package, the tools are
members. Without `--all` and `--workspace` the tools are skipped.)

Other useful ones:

```sh
cargo run                                   # the game in a window
cargo run -- --frames 30                    # smoke test: draws 30 frames and quits
cargo run -- --screenshot target/shot.png   # draws the start view into a PNG and quits
cargo run -- --screenshot target/shot.png --script "enter,1,1,cursor:0:0"   # after some input
cargo run -- --map assets/maps/example.map  # the game on a map file
cargo run -p mapeditor -- assets/maps/example.map                # the map editor
cargo run -p mapeditor -- --check assets/maps/*.map              # report on map files, no window
cargo run -p mapeditor -- x.map --screenshot target/ed.png --script "press:brush_7,key:w"
cargo run -- --map assets/maps/continent    # the game on a whole map: roads, cities, units
cargo run -p assetgen --release                                  # draw the game's pictures again
cargo run -p assetgen --release -- --previews target/previews    # and maps drawn with the tiles
cargo +1.86 check --workspace --all-targets # MSRV
```

`cargo test` needs no GPU and no display. The screenshot mode needs both (on
Linux CI: `xvfb-run`); it is how an agent sees the game and the editor, since
nobody can click the live window for you. Look at the PNG you made. The script
steps are listed at the top of `src/script.rs`; `press:ID` clicks a button.

## Rules

- **Dependencies:** the runtime dependencies are `miniquad`, `turbo-ferret` and
  `turbo-ferret-miniquad` (pinned once, in `[workspace.dependencies]`); a tool
  may also depend on the game's library (`assetgen` depends on nothing, and
  has the game only as a dev-dependency, for its tests). Nothing else: no `image`, `png`, `glam`, `serde`,
  `log`, `rand`. PNGs are decoded by Turbo Ferret's decoder and written by the
  small encoder in `src/render/png_out.rs`. A dev-dependency is allowed when a
  test really needs one. Turbo Ferret is pinned by `rev`; bump it on purpose. A
  `[patch]` to a local checkout is fine for local work, but don't commit it.
- **Rust:** edition 2024, MSRV 1.86 (no let-chains), rustfmt width 120.
- **Unsafe:** `#![deny(unsafe_code)]`, and the one kind that is allowed (the
  owner's word, 2026-11-18): a block that calls C or Objective-C for
  something miniquad leaves out. Such a block lives in `src/platform.rs`
  behind a safe function, is as small as the call, and has a `SAFETY` comment.
  Today there is one: `platform::bring_to_front` sends one Objective-C
  message on macOS, because miniquad opens its window behind the terminal
  there. No `unsafe` for anything else (speed, a cast, a shortcut).
- **Credit and licence:** code ported from the C++ cites its origin in a
  comment, `// C++: src/state/mapmove.cpp Map::selectUnit`; keep those when you
  change such code. New code needs none. Keep `LICENSE` (the zlib
  notice of the C++) and `NOTICE` (Turbo Badger, the skin, the DejaVu font
  licence, which must ship with binaries).
- **Layering:** `state` and `game` know nothing of the UI or the GPU. Widgets
  never hold game state: Turbo Ferret factories are `fn() -> Box<dyn Widget>`,
  so a widget records commands and the session applies them (the UI produces
  orders, the game consumes them). Keep it that way; phase 3 builds on it.
  What has no widget of its own to record it arrives at `RootWindow`: a click
  on a button is `RootCommand::Click(id)`, a key nothing used is
  `RootCommand::Key`, and `MapScreen::pump_shared` hands those to the scene.
- **One window host, many scenes:** the game and each tool are a `Scene` on a
  `MapScreen`, run by the same `App`. What every window needs (the map
  widgets, the cursor and zoom keys, the minimap, quit and fullscreen, the
  screenshot mode, scripts) goes into `MapScreen`, `App` or `script`, not into
  one scene. A layout file for a map screen has widgets with the ids
  `main_layout`, `main_map` and `mini_map`.
- **Moving the map is the screen's business:** the wheel, a trackpad's two
  fingers and a drag with the right or the middle button never reach a
  widget. `App` turns miniquad's numbers into pixels (`app::Wheel`, which
  knows each system's units) and calls `MapScreen::scroll`, `zoom_at`, `grab`,
  `drag` and `let_go`; those say false when the pointer is on neither map,
  and then the wheel is the UI's. Tests and scripts call the same methods.
  The view goes the same way on both maps (the owner tried the other and
  found it backwards): the map follows the fingers on the main map, and on
  the minimap the view's marker goes against them, at the minimap's scale.
- **The pictures are generated** (`docs/tiles.md`): `assets/tiles`,
  `assets/roads` and the signs in `assets/units` hold what
  `cargo run -p assetgen --release` writes and nothing else, and tests draw
  every picture again to compare. To change the art, change the generator
  (`terrain.rs`, `road.rs`, `icons.rs`) and run it; do not edit a PNG. The
  game and the generator each know the 14 shapes of a road and how one is
  turned (`src/render/roads.rs`, `tools/assetgen/src/road.rs`); a test holds
  the two together.
  A new terrain is a `TileType`, a `Terrain` in the generator with the same
  name and letter (a test compares the two lists), a tile, and a minimap
  color. Everything generated has to stay original: no shapes, palettes or
  icons taken from an existing game.
- **Maps are any size and shape**, 20 to 600 rows and as many columns, the
  two independent. Nothing may assume a square: `tests/shapes.rs` runs wide,
  tall and odd maps through the file, the views, the renderer's geometry and
  the unit's movement, and new code that takes rows and columns belongs in it.
- **The map format is a promise** (`docs/map-format.md`): maps written today
  must stay readable. A map is a terrain file, or a directory of text files
  (`terrain.map`, `units.txt`, `cities.txt`, `assets/`); `state::scenario`
  reads it, and every mistake is an error with its file and line. A new kind
  of data is a new key in a record or a new file in the directory, with a
  default for maps that do not have it. Change the page, the reader and the
  maps in `assets/maps` together (tests read the page's examples and every
  map), and keep `mapeditor --check` telling the truth about what the reader
  forgives. The game has no units or cities of its own: a map brings them.
- **Roads are terrain.** `Tile::road` (a capital letter in the file) joins
  the roads and cities around it: `Map::road_links` for drawing,
  `movement::step_cost` for what a step along one costs. A bridge is a road
  on water and a pass a road on a mountain; nothing else says so.
- **One way to change the map:** `Map` owns its units and cities, and they
  change only through its methods (`new_unit`, `place_unit`, `move_unit`,
  `kill_unit`, `unit_mut`, `tile_mut`, `add_city` ...), which keep the tiles'
  unit lists and the units' positions in step and count revisions. What is
  derived from the map checks a revision instead of being told: `Game::refresh`
  recomputes the selected unit's range, and the session asks for a new minimap
  picture when `terrain_revision` moves. Add new mutations as `Map` methods
  that call `changed()` or `terrain_changed()`; do not make the fields public.
- **Unit ids are generational.** A deleted unit's `UnitId` names nothing, even
  after its slot is reused: every lookup gives `None`. Orders and selections
  may hold ids across turns; they must cope with `None`.
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
5. **Keys go to the focused widget only**, and bubble up from there. In the
   game the map is the only widget that takes the focus (the minimap, the
   button and the root window do not), so its keys work whatever was clicked
   last and the keys it does not use reach `RootWindow`. A widget that needs
   the keyboard (a text field, a dialog) has to give the focus back to the map
   when it is done: the editor's fields do on Enter and Esc, its buttons are
   `is-focusable: 0`, and it takes letters for terrain keys only while the map
   has the focus.
6. **A tile is scaled smoothly, so it has no transparency.** Terrain
   textures are read with linear filtering and mipmaps (`Sampling::Smooth`):
   a hex is 20 to 270 pixels wide. That mixes each pixel with its neighbours,
   so a tile is painted to the corners of its square, outside the hex too,
   and its side is a power of two. A picture with transparency (the unit
   image) is read by the nearest pixel instead.
7. **The view's middle is a point, not a hex.** `MapView` keeps the nearest
   hex (`center()`) and how far the middle is from it; a scroll leaves part of
   a hex there. Ask `middle()`, `hex_center`, `shows` or `pick`, and do not
   compute a place on screen from `center()` alone. The cursor can be out of
   view after a scroll; an arrow key brings the view back to it.
8. **A press on the map is three commands and a click is a fourth.**
   `MapCommand::Press`, `Hover`s while the button is held (a drag, which may
   leave the map), `Release`, and then `Click` if the release was on the map.
   The game acts on `Click`, the editor on `Press` and the drag: do not act on
   both.
