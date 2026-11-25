# Hexlords

A hex map strategy game foundation in Rust. Phase 1 (done) is a port of the C++
hexGame engine demo (<https://github.com/sstanfield/hexGame>, read at `99edad5`):
a 100 x 100 hex map, one unit, a movement range, a minimap. The UI is Turbo
Ferret (<https://github.com/sstanfield/turbo-ferret>) and graphics go through
miniquad. (The port's brief, `hexgame-rust-port-plan.md`, is no longer in the
tree: the port is done. It is in the history, `git log -- hexgame-rust-port-plan.md`,
for whoever wants to know why the window is built as it is.) Phase 2 has begun:
there is a documented map format (`docs/map-format.md`: terrain, roads, the
map's own kinds of unit, its cities), which the game plays with `--map`; a map
editor (`tools/mapeditor`); and the tiles, the roads and the units' signs are
drawn by a program (`tools/assetgen`, `docs/tiles.md`). Units move turn by
turn, one side after the other (a new game screen sets the game up: who
plays each side, a person or the computer, hotseat for several people), and
cities build what their holder sets; an empty city is taken by moving in;
stacks move together (ships carry land units, flyers fly, a captain with
flyers flies: `Game::group`; flyers with units that walk go one way all of
them can pay for, `state::reach_together`, `Game::ground_together`), and moving into another side's units is a battle
(`state::combat`: how much chance and which system are set with the game;
commanders lead stacks); each side has a treasury, its cities' taxes come
in and its units' upkeep goes out each turn, and what a city builds is paid
for when it is done; a side that has lost its cities is out. Orders can
last (a stack sent far goes over turns, a city sends what it builds), a
stack can be put aside for the turn, and a list says what is left to do; a
side sees the others' stacks only as banners with their size, and fog of
war is a choice made with the game: off, on, or total, where land never
seen is black and shut until explored (`Game::sight`, `Sight`).
**`docs/gameplay.md` is the plan for the rules**: two modes (turns one side
after the other, and rounds where all sides give orders at once and the
round then plays out on a clock, the faster units first: both built) on one set of rules made
of orders. The computer's players are agents, which also play
without a window (`docs/agents.md`).

**The C++ is history, not a specification.** The owner's word (2026-11-18):
the port is done and the game now goes its own way, so nothing has to match the
C++ any more. `docs/deviations.md` records how the port differed from it and
what has changed since; it is closed, and new work does not add to it. Read the
C++ to understand why something is the way it is, not to decide what to do.

**Read before changing anything substantial:** this file, and
`docs/gameplay.md` before touching the rules.

## Layout

| Path | What |
|---|---|
| `src/state/` | Map, tiles, roads, units and their kinds, cities, movement range; `combat`, the battles (pure); `rng`, the game's chance; `scenario`, the reader of a map's files. No UI, no graphics |
| `src/files.rs`, `src/paths.rs`, `build.rs` | Reading the game's files (`paths`: where the assets are, `--assets`): from the disk, or in a browser (the WebAssembly build) from the copy of `assets/` inside the program, which `build.rs` lists. Everything that reads a file of the game's asks here, not `std::fs`. `build.rs` also stamps the build: the commit, the profile, when (`src/about.rs`) |
| `src/game.rs`, `src/game/patrol.rs`, `src/game/rounds.rs`, `src/game/forecast.rs`, `src/orders.rs`, `src/setup.rs` | `Game`: the map, whose turn it is, and the rules: `Game::apply` carries out an `Order` and says what came of it (`Event`s); the selected unit and its range for the screen; `game::patrol`: patrols, and marauding's flag, as rules; `game::rounds`: the second mode (an order to move is a plan with its way, and the round plays out on a clock, each step taking a unit its cost over its movement a turn; a stack attacks who it finds on its way; stacks join to go as one; two of a side that meet with no room for both pass each other, `Map::change_places`; a plan is sent on from where it ends, a waypoint, `Order::Onward`; `Game::play_out`); `game::forecast`: a side's next pay days, its pay day's rules played forward on copies of its cities, read only (`Game::forecast`). `Setup`: a game before it starts (the mode, the order of turns, who plays each side, the seed) |
| `src/agents.rs`, `src/planner.rs`, `src/planner/wary.rs`, `src/planner/rounds.rs`, `src/marauder.rs`, `src/headless.rs` | The computer's players (`Agent`: `passive`, `simple`, `cmd:<program>` over a text protocol; `planner`: `steady`, `bold`, `careful`, which read the rules in effect, keep clear of stacks they could not beat (`src/planner/wary.rs`), and play a game of rounds in a way of their own, `src/planner/rounds.rs`: `<style>/turns` is one that does not, to measure against); with fog of war each is given its side's view (`Game::view_for`); `marauder`: the computer leading a side's marauding stacks; a game played without a window (`--headless`) |
| `src/view.rs`, `src/minimap_view.rs` | Pure math: zoom, scale factors, visible hexes, picking, panning, recentering; the minimap's fit and marker |
| `src/render/` | miniquad drawing: hex meshes, shaders, textures, the map and minimap renderers, the PNG writer; the units' icons for the panels (`unit_icon`, on the CPU) |
| `src/gui/` | Turbo Ferret widgets: `MapWidget`, `MiniMapWidget`, `RootWindow`. They record commands; they hold no game state. `build`: windows made from code (the help, the cities'); `menu`: a menu bar (an item is a button: its click is its id) |
| `src/platform.rs` | What miniquad leaves out: bringing the window to the front on macOS (the crate's one `unsafe` block); and what differs in a browser (`WEB`): the clock, the time of day for a seed, where a complaint goes |
| `src/screen.rs` | `MapScreen`: the `Ui`, a map and the two views together, with no window: what the game and the tools share. `Scene`: what the window can run |
| `src/new_game.rs`, `src/shell.rs` | The new game screen (a scene on a map screen of its own: the maps, or a random map made there with `mapgen`), and the window's scene from it to the game and back |
| `src/session.rs`, `src/showing.rs`, `src/info.rs`, `src/guide.rs`, `src/city_windows.rs`, `src/log_window.rs`, `src/battle_window.rs`, `src/tasks_window.rs`, `src/temple_window.rs`, `src/disband_window.rs`, `src/patrol_window.rs`, `src/rally_window.rs`, `src/options_window.rs`, `src/production_window.rs`, `src/about.rs` | `Session`: the game on a map screen (its layout, the menus and their keys, what a click and Enter do, what a click picks after Go to, the panels around the map: the treasury, the stack's boxes, the box the pointer opens over a unit, the city in the sidebar; what the person at the screen sees). `info`: what the panels say, as plain functions of the game's state (and what the game's history says to a side). `city_windows`: a city in full (in a window, or smaller in the sidebar), where its holder sets what it builds and where its new units go, and the lists of cities. `log_window`: a side's log of battles and cities. `battle_window`: a battle played out. `tasks_window`: what a side has left to do this turn. `temple_window`: whether a stack on a temple seeks its blessing, and what each was given. `disband_window`: whether the selected units go home for good. `patrol_window`: whether a patrol attacks, and at what odds. `rally_window`: a side's rally points, placed on the map, where cities send units and stacks go. `options_window`: what the screen does for the person as a turn goes (selects, asks, tells): not rules. `about`: the version and what the game was built from (About Hexlords, `--version`). `production_window`: a side's production in one place (its cities, what the next rounds bring, the gold, its stacks), whose clicks are the city window's and X's orders. `showing` (pure): what the screen shows happening, the stacks going hex by hex and the battles in their turn (the game keeps what happens only while watched, `Game::watch`). `guide`: the Help menu's topics, written from the game in play. Tests drive them all |
| `src/replay.rs` | What happened, kept to be looked at again: the game as it was and the steps done to it since (orders, `Game::record`; in rounds, beats of the round), which make it happen again on a frozen copy (`Game::freeze`). The session's Replay button and slider |
| `src/save.rs` | A game that is not over, kept in a file to be played on (`docs/saves.md`): the texts its map was read from, the setup, and every step done to it from the start (`Game::record`, `Step`), which done again make the same game (`Game::redo`), checked by a number (`Game::digest`). The session writes it as the turns pass and when the window closes; the new game screen lists the games in progress, plays them on and deletes them; `platform::saves_dir` says where (nowhere in a browser) |
| `src/script.rs` | Scripted input (`--script`) for any scene, for screenshots and tests |
| `src/app.rs`, `src/main.rs` | The miniquad window (`App<S: Scene>`, `app::run`): forwards input to the scene, draws the frame. The interface's size: the system's DPI times `--ui-scale` (`app::ui_dpi`); the skin lists the DPIs it is drawn at. `main.rs` is the game's command line |
| `tools/mapeditor/` | The map editor, a workspace member on the game's library: `Editor` (a scene: the sidebar, painting, files), `brush`, `history` (undo), `check` (`--check`) |
| `src/mapgen/` | The random map generator, pure (`docs/mapgen.md`): `config` (the knobs, the presets, the size), `land`, `relief`, `rivers`, `biomes`, `cleanup`, `cities`, `sites`, `roads` (the stages), `names`, `grid`, `noise`, `rng`, `reach` (movement classes, who gets where), `stats` (what any map measures), `check`, `validate` (the hard checks a generated map must pass), `score`, `preview` (pictures, contact sheets) |
| `tools/mapgen/` | Its command line, a workspace member on the game's library: `gen`, `batch`, `stats`, `check` |
| `tools/web/` | The browser build, a workspace member with no dependencies: `cargo run -p web` builds the game for `wasm32-unknown-unknown` and fills `target/web` (`index.html`, the page; `hexlords.wasm`; miniquad's `gl.js`, copied from the miniquad in use; the licences), `--serve` gives it out on localhost |
| `tools/assetgen/` | The picture generator, a workspace member with no dependencies: `terrain` (a function for each terrain's art), `noise` and `scatter` (what repeats on the hex grid), `tile` (the image, the atlas), `overlay`, `road` and `icons` (the pictures that go over a tile), `skin` (the windows' and buttons' pictures), `preview` (maps drawn without the game), `png` (the writer) |
| `assets/` | `layouts/main.tb.txt` (the game), `layouts/editor.tb.txt` (the editor), Turbo Badger's default skin and `skin/hexlords.tb.txt` (our changes, loaded on top), `tiles/`, `roads/`, the signs in `units/` and the `hg_*.png` of `skin/` (what `assetgen` makes, and nothing else), `units/default_unit.png` (the C++ demo's), `maps/` (`*.map` files and map directories), `mapgen/` (the generator's presets and units template) |
| `tests/`, `tools/*/tests/` | Headless tests of the layouts, the widgets and the interaction; of the generator against the game |
| `docs/` | `map-format.md` (the map file, for people and agents who write maps); `tiles.md` (the tiles, for those who draw them); `gameplay.md` (the plan for the rules); `saves.md` (a saved game's file); `deviations.md`; `screenshots/` |
| `reference/hexGame/` | The C++ original, for reference. **Read-only, never edit it.** Gitignored: if it is missing, `git clone https://github.com/sstanfield/hexGame reference/hexGame` |
| `encumbered/` | The owner's private maps that are not ours to give away (one built from an existing game, for nostalgia). Gitignored: **never commit anything from it, and never copy its names, maps, numbers or pictures into the game.** The game plays them with `--map encumbered/<map>` |

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
cargo run                                   # the new game screen, in a window
cargo run -- --frames 30                    # smoke test: draws 30 frames and quits
cargo run -- --map assets/maps/continent --screenshot target/shot.png   # draws the start view into a PNG and quits
cargo run -- --map assets/maps/continent --screenshot target/shot.png --script "enter,minus,cursor:0:0"   # after some input
cargo run -- --map assets/maps/example.map  # the game on a map file
cargo run --release -- --headless --map assets/maps/continent --seed 7 --log   # agents against agents, no window
cargo run -- --saves target/saves           # keep the games in progress there (without it: the system's place for a program's data)
cargo run -p mapeditor -- assets/maps/example.map                # the map editor
cargo run -p mapeditor -- --check assets/maps/*.map              # report on map files, no window
cargo run -p mapeditor -- x.map --screenshot target/ed.png --script "press:brush_7,key:w"
cargo run -- --map assets/maps/continent    # the game on a whole map: roads, cities, units
cargo run -p assetgen --release                                  # draw the game's pictures again
cargo run -p assetgen --release -- --previews target/previews    # and maps drawn with the tiles
cargo run -p mapgen --release -- batch --seeds 1..16 --size medium --out target/maps --sheet   # random maps, a contact sheet
cargo run -p mapgen --release -- stats assets/maps/continent     # what a map measures
FUZZ_N=300 cargo test --release --test fuzz -- --nocapture   # games of random orders, many (the tests play a few): the rules' promises held to
cargo +1.86 check --workspace --all-targets # MSRV
cargo run -p web -- --serve                 # the game in a browser: http://127.0.0.1:8080 (once: rustup target add wasm32-unknown-unknown)
cargo clippy --target wasm32-unknown-unknown -p hexlords -- -D warnings   # the browser build, checked
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
  help window, the screenshot mode, scripts) goes into `MapScreen`, `App` or
  `script`, not into one scene. A layout file for a map screen has widgets
  with the ids `main_layout`, `main_map` and `mini_map`.
- **The help says every key** (F1 or `?`, `src/help.rs`): the keys every
  window shares are in `help::map_section` and `help::window_section`, and a
  scene gives its own with `MapScreen::set_help`. A new key or click goes into
  the help in the same change. macOS keeps F11 (and often F1) for itself:
  every key of the window has another that works there (Ctrl+Cmd+F, `?`).
- **Moving the map is the screen's business:** the wheel, a trackpad's two
  fingers and a drag with the right or the middle button never reach a
  widget. `App` turns miniquad's numbers into pixels (`app::Wheel`, which
  knows each system's units) and calls `MapScreen::scroll`, `zoom_at`, `grab`,
  `drag` and `let_go`; those say false when the pointer is on neither map,
  and then the wheel is the UI's. Tests and scripts call the same methods.
  The view goes the same way on both maps (the owner tried the other and
  found it backwards): the map follows the fingers on the main map, and on
  the minimap the view's marker goes against them, at the minimap's scale.
  The view stops at the map's edges: `MapView::set_middle` keeps the widget
  on the map (a map smaller than the widget along an axis sits in its
  middle), whatever moved it (a scroll, a zoom, a resize, the cursor), so
  nothing past the edge is shown.
- **The pictures are generated** (`docs/tiles.md`): `assets/tiles`,
  `assets/roads`, the signs in `assets/units` and the skin's `hg_*.png` hold what
  `cargo run -p assetgen --release` writes and nothing else, and tests draw
  every picture again to compare. To change the art, change the generator
  (`terrain.rs`, `road.rs`, `icons.rs`) and run it; do not edit a PNG. The
  game and the generator each know the 14 shapes of a road and how one is
  turned (`src/render/roads.rs`, `tools/assetgen/src/road.rs`); a test holds
  the two together.
  A new terrain is a `TileType`, a `Terrain` in the generator with the same
  name and letter (a test compares the two lists), a tile, and a minimap
  color. A city has a tile for each side (`city_<side>_tile.png`), in the
  sides' colors, which the generator (`sides.rs`) and the game (`side_color`)
  each know; a test holds them together. Everything generated has to stay original: no shapes, palettes or
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
- **Orders are the one way the game goes on.** A person's clicks and the
  computer's agents alike give orders (`src/orders.rs`), and `Game::apply`
  carries them out under the rules, or refuses them with a reason, and says
  what came of them as events. A new rule goes there, not into the session
  or an agent. The rules are **deterministic**: their chance comes from the
  game's own generator (`state::Rng`, seeded by the setup), never from the
  clock or another source, so a seed and a set of orders always make the
  same game. Keep the agents and `--headless` working for every rule: they
  are how the game is balanced. A saved game depends on it (`src/save.rs`):
  it is the map, the setup and the steps done since, done again, and
  `Game::digest` says it came out the same: what the rules keep in a new
  field goes into it. An
  order that is refused changes nothing: refuse before the first change
  (a refusal after `see_along` once made a saved game another game). So
  whatever changes a game goes through `Game::apply` (or is a `Step` of
  its own that `Game::record` keeps and `Game::redo` does again), a new
  order is written and read by `agents::order_text` and `parse_order` (a
  test goes through every kind), and a run with a script or a screenshot
  keeps no game unless given `--saves`.
- **One way to change the map:** `Map` owns its units and cities, and they
  change only through its methods (`new_unit`, `place_unit`, `move_unit`,
  `kill_unit`, `unit_mut`, `tile_mut`, `add_city` ...), which keep the tiles'
  unit lists and the units' positions in step and count revisions, and
  refuse a unit a hex that is full (8 and a commander): no rule can put
  more on one, so none needs to try (`change_places` for two stacks that
  swap). What is
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
- **The game also runs in a browser** (`cargo run -p web`, WebAssembly on
  miniquad's WebGL 1), so the window's code keeps to what exists there. No
  `std::fs` for the game's files: `crate::files` reads them (the disk, or the
  assets inside the program), and a new place that reads one goes through
  it. No `std::time` (`Instant` and `SystemTime` panic in a browser):
  `platform::Clock` and `platform::clock_nanos`. No threads, and nothing that
  waits: a frame returns. What goes wrong is said with `platform::complain`
  (the console there), not `eprintln!`. A browser keeps Ctrl+T, Ctrl+N, F11
  and closing for itself and passes Ctrl but not Cmd: where that changes what
  the player is told, ask `platform::WEB` (`help::window_section_for`, the
  Game and View menus). No JavaScript of our own beyond the page
  (`tools/web/index.html`), and no `unsafe` for the browser: miniquad's
  `gl.js` is all the game calls. After a change to the window, the files or
  the input, build it and look at it in a browser.
- **Tests:** a fix comes with a test that fails without it. Anything a player
  sees gets a screenshot, and you look at the picture.
- **Commits:** on `main`, small, described by what changed and why.
- **The version** (the owner's word, 2026-11-24): `version` in the root
  `Cargo.toml` is the game's, `0.<minor>.<patch>`, shown by About Hexlords
  (the Help menu) and `--version` with the commit it was built from
  (`build.rs`, `src/about.rs`). Bump it in the same commit as the change
  (`Cargo.lock` records it too):
  - **A rule change** bumps the minor number and puts the patch back to 0
    (0.1.3 to 0.2.0): anything that makes a seed and a set of orders give
    another game, or that a saved game may no longer load over (`Game::apply`
    and what it carries out, the round's play out, pay day, `Game::digest`,
    the save format; a way of fighting or an option of the setup taken out
    or changed).
  - **Any other change to the game** bumps the patch (0.1.0 to 0.1.1): the
    screen, the windows, the help, the agents' play (their orders are what a
    save keeps, so it still loads), the art, speed.
  - A commit with both is a rule change. One that changes nothing the game
    is built from (docs, tests, the tools alone) bumps nothing. The tools
    keep their own versions.
- **Later phases** take the feel of 1990s fantasy strategy games but must not
  clone one: no names, factions, lore, maps, tables or art from an existing game.
  (The README says where the feel comes from: credit, not content.)

## Things that bite

1. **Hex geometry.** Flat-topped hexes in offset columns ("odd-q"): odd columns
   sit half a hex *higher* on screen, and map rows grow *downward* while the GL
   y axis grows upward. The neighbour rule, the visible range and the pick
   formula all depend on it; `src/state/hex.rs` and `src/view.rs` say how.
2. **The map is drawn by the host, not by a widget.** Turbo Ferret widgets paint
   through a batched, read-only `PaintCtx`, so `MapWidget` and `MiniMapWidget`
   paint nothing: `App::draw` draws the map into their rectangles first and the
   UI on top. A map that must sit *between* UI layers (in
   a window) cannot be drawn so: the list of cities shows a picture of the
   minimap drawn on the CPU (`render::minimap_image`) in an `ImageWidget`.
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
   a hex is 16 to 215 pixels wide. That mixes each pixel with its neighbours,
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
