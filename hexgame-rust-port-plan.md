# HexGame: Rust port plan

A brief for Claude Code. Phase 1 ports the existing hexGame engine demo
(https://github.com/sstanfield/hexGame, last commit `99edad5`) to Rust. The GUI is
Turbo Ferret (https://github.com/sstanfield/turbo-ferret, read at `a37cb83`) and
graphics go through miniquad. Phases 2 and later are sketched at the end; they are
direction, not a spec.

Everything below comes from reading both repositories. Where something could not be
confirmed without running the C++ build, it says so.

---

## 0. Ground rules

- **Phase 1 is a faithful port of the demo.** It should look and behave like the C++
  build. There are no new features. Bugs that crash, cause undefined behaviour or let
  a unit teleport get fixed, and each fix is listed under "Deviations" (section 6).
  Cosmetic quirks are kept unless section 6 says otherwise.
- **Dependencies.** Runtime dependencies are `miniquad`, `turbo-ferret` and
  `turbo-ferret-miniquad`. Nothing else. No Lua, glm, GLEW, GLFW, stb_image, `image`,
  `png`, `glam`, `serde`, `log` or `rand`. PNG decoding uses Turbo Ferret's own decoder.
  Dev-dependencies are allowed when a test needs one.
- **Rust.** Edition 2024, MSRV 1.86 (no let-chains), rustfmt width 120, the same as
  Turbo Ferret. Use `#![forbid(unsafe_code)]` in the game crate.
- **Every change** ends with `cargo fmt --check`,
  `cargo clippy --all-targets -- -D warnings` and `cargo test` passing.
- **Credit.** Ported code cites its origin, for example `// C++: src/state/mapmove.cpp Map::selectUnit`.
  Keep the zlib notice from `COPYING.txt`.
- Do not copy C++ comments that mention gamepads, Emscripten or GLEW. Those paths go away.

---

## 1. What exists today (C++ inventory)

About 3,000 lines are the game's own code. The rest is vendored third-party code.

| Path | Lines | What it is | Port? |
|---|---|---|---|
| `src/main.cpp` | 526 | GLFW window (1024x768, "HEX Game!"), GL 3.3 core, GLEW, key/mouse/scroll callbacks into TB, main loop, asset path from `argv[0]/../assets/` | Replace with a miniquad `EventHandler` |
| `src/Application.{h,cpp}` | 300 | TB init (skin, font "Sans" = DroidSans 14dp), root widget, **builds the demo map**, runs `layouts/main.lua`, forwards input, TB shortcut mapping | Port the setup. Input glue comes from `turbo-ferret-miniquad::InputAdapter` |
| `src/settings.{h,cpp}` | 57 | Singleton holding the asset and shader dirs | Replace with a plain `Paths` struct |
| `src/hexlua.{h,cpp}` + `src/lua/` | 230 + Lua 5.3 | Embeds Lua only so `main.lua` can build the layout (`tb_addWidget`, `tb_getWidth`, `tb_getHeight`) | **Drop** (see 3.6) |
| `assets/layouts/main.lua` | 22 | Builds the layout: map at 80% width × 70% height, sidebar with minimap and a button "SLS" | Replace with a `.tb.txt` layout and a little Rust |
| `src/state/map.{h,cpp}` | 311 | `Map`, `Tile`, `TileType`, map file load/save, place/move/kill units, cities | Port |
| `src/state/mapmove.cpp` | 232 | Movement-range flood fill (`selectUnit`, `deSelectUnit`) | Port as Dijkstra (see 6) |
| `src/state/unit.{h,cpp}` | 222 | `Side` (8 + neutral grey), `Unit`, `MoveCost` (interned per terrain-cost tuple), `UnitManager`, `resetMovePoints` | Port |
| `src/state/city.{h,cpp}` | 172 | `City`, `UnitProd`, `CityManager` | Port (unused by the demo, but cheap and next phase needs it) |
| `src/gui/glwidget.*` | 65 | TB widget that calls `BeginNativeRender`, sets `glViewport` to its padding rect, calls `render()` | Different approach (see 3.3) |
| `src/gui/mapwidget.*` | 168 | Map input: hover, click, arrows, Enter, `1`/`2` zoom | Port |
| `src/gui/minimapwidget.*` | 123 | Minimap widget (input is a no-op) | Port |
| `src/gui/root_widget.*` | 84 | id `RootWindow`, F11 / Alt+F fullscreen (stubs), Ctrl+Q quit, dimmer count | Port the keys. Drop the dimmer count (no dimmers in the demo) |
| `src/render/maprender.*` | 777 | Main hex map renderer | Port to miniquad |
| `src/render/minimaprender.*` | 739 | Minimap renderer, cached via `glReadPixels` into a texture | Port to miniquad (render-to-texture) |
| `src/render/shaders.*` | 223 | Shader loading and caching | miniquad `new_shader` |
| `src/render/imageutils.*` | 104 | PNG to GL texture via stb_image | `turbo_ferret::image::png::decode` + miniquad texture |
| `src/render/tbrenderer.*`, `glcapabilities.*` | 389 | TB's GL renderer | **Drop**. `MiniquadRenderer` replaces it |
| `src/render/distancemap.*`, `src/util/string.*`, `src/include/*`, `src/glm/`, `src/render/pc/` (GLEW) | | Unused or replaced | **Drop** |
| `src/gui/tb/` | | Vendored Turbo Badger | **Drop**. Turbo Ferret replaces it |
| `assets/shaders/{330_core,es_2.0}/` | | Hex, overlay, minimap, location marker, tex-pass, text and TB shaders | Rewrite the six game shaders in GLSL 100 (see 3.4). Drop the TB and text shaders |
| `assets/tiles/*.png` | | 64×64 and 256×256 RGBA, not interlaced | Keep |
| `assets/units/default_unit.png` | | 64×64 | Keep |
| `assets/skin/` | | Turbo Badger's default skin (public domain). `skin.hex.txt` is an empty override | Keep the PNGs and `skin.tb.txt`. Leave out the `.psd` files |
| `assets/fonts/Droid*.ttf` | | Apache 2.0 | Not needed: use Turbo Ferret's built-in font (see 3.6) |
| `lang/lng_en.tb.txt` | | Loaded by `Application.cpp` but **missing** from the repo | Drop the load |

Hex geometry facts that the rest of the plan relies on (from `maprender.cpp` and `mapmove.cpp`):

- **Flat-topped hexes in offset columns ("odd-q").** A hex model has its center at 0,0,
  vertices at (±1, 0) and (±0.5, ±1), and 6 triangles around the center (indices
  `0,2,1  0,3,2  0,4,3  0,5,4  0,6,5  0,1,6`).
- **Placement.** In model units, column `c` is at x = `1.5 * c`. Row `r` is at
  y = `2 * r`, and odd columns are drawn one unit higher (y + 1) on screen. Map rows grow
  **downward** on screen (`mrow = centerRow - row`).
- **Neighbours** of (r, c): (r−1, c), (r+1, c), (r, c−1), (r, c+1), and
  (r + (c odd ? −1 : +1), c ± 1).
- **Aspect.** `HEX_HEIGHT_ADJUST = 0.86330935` scales y (about √3/2).
- **Map edges are straight.** Edge hexes use clipped models: column 0 loses its left
  point, the last column loses its right point, odd columns in row 0 lose their top
  half, and even columns in the last row lose their bottom half. Corners combine
  these. There are 7 vertex variants (`hex`, `left`, `right`, `top`, `bottom`,
  `top_right`, `bottom_left`, `bottom_right`) with matching UV tables. Copy the
  tables exactly.

---

## 2. What the demo does: the acceptance spec

The Rust build is done when all of this holds.

**Window.** 1024×768, title "HEX Game!", clear color (0.5, 0.5, 0.5, 1), alpha
blending (`SRC_ALPHA, ONE_MINUS_SRC_ALPHA`) for the map. Vsync, and it redraws every frame.

**Layout** (from `main.lua`):

```
RootWindow (root)
└─ main_layout      TBLayout axis y, distribution available, spacing 5
   ├─ main_layout1  TBLayout axis x, distribution available, spacing 5
   │  ├─ main_map     MapWidget, is-focusable, lp width = floor(W*0.8) px, height = floor(H*0.7) px
   │  └─ main_sidebar TBLayout axis y, distribution available, spacing 5
   │     ├─ mini_map  MiniMapWidget, is-focusable, lp width = W - mw px, height = H - mh px
   │     └─ button    TBButton text "SLS" (does nothing)
   └─ main_layout2  TBLayout axis x (empty)
```

W and H are `main_layout`'s size when the script runs, which is computed once at
startup. *Unconfirmed:* in TB, the layout's rect may still be 0×0 when the script
asks for it. If the C++ reference build (section 5, M0) shows something else, match
what it shows. Otherwise use the root viewport size.

**Demo state** (`Application.cpp`):

- A 100×100 map, all Grass. A Mountain diagonal: `tile(i, i) = Mountain` for i in 0..100.
- Move cost for the test unit: grass 2, forest 4, swamp 5, desert 3, hill 4,
  mountain 0, water 0, shore 0, bridge 1, road 2, city 1. **0 means impassable.**
  Temple and Ruin cost the same as grass.
- One unit: side Black, name "Test Cav", upkeep 3, strength 5, movement 20, at (row 50, col 50).
  That hex is on the mountain diagonal, so the unit starts on a tile it could not enter.
  Its movement range must still work from there.
- The cursor (`map.row/col`) starts at (50, 50). The map view centers on it.

**Main map rendering** (`MapRenderer::renderMap` / `drawHex`):

- Zoom levels are `[0.2, 0.1, 0.075, 0.05, 0.03]`, and it starts at index 1.
- Factors (`updateMapDimensions`): start with hf = widget height and wf = widget width.
  If hf < wf, then wf = (hf/wf)·scale and hf = scale. Otherwise hf = (wf/hf)·scale and
  wf = scale. Then multiply hf by 0.86330935. The vertex position in the widget's
  viewport NDC is `(pos.x*wf + center.x*wf, pos.y*hf + center.y*hf)`.
- Visible range: numX = int(1/wf) + 1, then numX += numX/4 + 6. numY = int(1/hf) + 2.
  Loop over col in −numX/2..=numX/2 and row in −numY/2..=numY/2 around (centerRow,
  centerCol), and skip hexes that are off the map.
- For each hex, in this order:
  1. Draw the tile texture with a border. Each fragment has a barycentric value: 1 at
     the center, 0 at the rim (attribute `BarryCoord`; the fragment shader tests `.y`).
     When `bary.y > threshold`, draw the border color.
     - The cursor hex has a red (1,0,0,1) border and threshold `0.85 − zoomLevel*0.01`.
     - Other hexes have a black border and threshold `0.95 − zoomLevel*0.01`.
  2. If the tile has units, draw `default_unit.png` over the hex with the same model
     and border.
  3. If a unit is selected and the tile's move cost is greater than 0 and no more than
     the unit's movement, draw a translucent red overlay (1, 0, 0, 0.3) over the hex.
- Texture by terrain: grass, forest, swamp, hill, mountain and water use their own
  `*_tile.png`. City, Temple, Ruin and Desert all use `citycenter_tile.png`.
  `border_tile.png` and `city1..6_tile.png` are loaded or present but never drawn.
- Textures use nearest filtering and clamp-to-edge.

**Minimap rendering** (`MiniMapRenderer`):

- Fit the whole map into the widget. Factors are computed as for the main map, with
  scale 1, then multiplied by min(1/(rows·hf), 1/((cols/2)·1.5·wf)).
- Fill the widget with black first, then draw each hex as a flat color:
  - Grass (0, .76, 0)
  - Forest (.22, .416, .122)
  - Swamp (0, .76, .522)
  - Desert (0, 0, 0)
  - Hill (.863, .667, .208)
  - Mountain (.475, .467, .443)
  - Water (.09, .886, .902)
  - Temple (.5, .5, .5)
  - Ruin (.6, .6, .6)
  - City (0, 0, 0)
- The image is cached and redrawn only when marked dirty. In the C++, only a resize
  marks it dirty.
- A translucent black (alpha 0.3) location marker is drawn on top. See deviation D5.

**Input** (`MapWidget::OnEvent`, `RootWidget::OnEvent`):

- **Pointer move over the map:** the hex under the pointer becomes the cursor.
  `setCenterMap` is misnamed: it moves the cursor, not the view.
- **Click on the map:** the hex under the pointer becomes the cursor, then
  `selectHex(col, row)` runs:
  - If a unit is selected and this is its own hex, deselect it.
  - If a unit is selected and the hex is reachable, subtract the hex's cost from the
    unit's movement, move the unit there and deselect it.
  - If nothing is selected and the hex has a unit, select the first unit on it. This
    computes the movement range.
- **Pick math**, relative to the widget. Let relX = (x − w/2)/(w/2) and
  relY = −(y − h/2)/(h/2). Then:
  - col = centerCol + trunc(relX/(1.5·wf) + (relX<0 ? −.5 : .5))
  - row = centerRow − trunc(relY/(2·hf) + (relY<0 ? −.5 : .5) − (col odd ? .5 : 0))
  - The pick is accepted only if (row, col) is on the map.
- **Keys** (when the map has focus):
  - Left/Right move the cursor column, Up/Down move the cursor row. If the cursor
    leaves the view (its column is outside centerCol ± colsDisplayed/2, or its row is
    outside centerRow ± rowsDisplayed/2), the view recenters on that axis. Here
    rowsDisplayed = int(1/hf) and colsDisplayed = int(1.25/wf).
  - Enter does `selectHex` at the cursor.
  - `1` zooms out and `2` zooms in. Both recenter the view on the cursor.
  - The map consumes the key-ups of arrows, Esc and Enter.
- **Root:** Ctrl+Q quits. F11 and Alt+F call fullscreen stubs that do nothing. Port
  them as no-ops, or wire F11 to `miniquad::window::set_fullscreen`. Either is fine;
  say which you did.
- **Minimap:** consumes clicks and arrows and does nothing else.
- Wheel does nothing. The "SLS" button does nothing.

**Unit rules already present** (they matter for phase 3):

- Stacks hold at most 8 units.
- `resetMovePoints`: new MP = base + carry-over, where the carry-over is 0 if 0 MP
  were left, 1 if 1 MP was left, and 2 otherwise.
- Map sizes are 20..=600 on each axis.
- Map file format: line 1 is rows, line 2 is cols, then one line per row with one
  character per tile: `g h f m w s d t r c`.

---

## 3. Rust design

### 3.1 Repository layout

**A new repository** (Steve's decision). Steve starts Claude Code in a new directory
with the old C++ code checked out in a subdirectory for reference, called
`reference/hexGame/` below; use whatever name Steve actually gave it. Treat it like
turbo-ferret's `turbobadger/`:
- It is read-only. Never edit it.
- Put it in `.gitignore`, so it is not committed.
- If it is missing, run `git clone https://github.com/sstanfield/hexGame reference/hexGame`.

Copy the assets the port needs (`tiles/`, `units/`, the skin PNGs and `skin.tb.txt`)
from it into `assets/`. Don't reference the copy in place.

```
Cargo.toml            # single package `hexgame` (bin + lib); a workspace only when a second crate appears
CLAUDE.md             # short agent brief, modeled on turbo-ferret's
README.md
assets/
  layouts/main.tb.txt
  skin/               # TB default skin PNGs + skin.tb.txt (no .psd)
  tiles/  units/
  shaders/            # optional: or embed GLSL as Rust string consts (preferred, see 3.4)
src/
  main.rs             # miniquad::start, Conf, args (--assets, --frames N, --screenshot PATH)
  lib.rs              # pub mod state, view, render, gui, app
  app.rs              # App: owns Ui, MiniquadRenderer, InputAdapter, Game, renderers; EventHandler impl
  paths.rs            # asset root resolution
  state/
    mod.rs
    hex.rs            # Hex {row, col}, neighbours (odd-q), bounds
    terrain.rs        # TileType, char codes, MoveCost
    map.rs            # Map, Tile, load/save, place/move/kill
    units.rs          # Side, Unit, UnitId, Units (slot vec per side), MoveCost interning, reset_move_points
    cities.rs         # City, UnitProd, Cities
    movement.rs       # reachable-cost computation (Dijkstra), MoveRange
  view.rs             # MapView: zoom, factors, center, visible range, pick, recenter rules (pure math)
  minimap_view.rs     # MiniMapView: fit factors, marker rect (pure math)
  render/
    mod.rs
    hex_mesh.rs       # the 7 vertex/UV variants + barycentric data, choose_variant(row, col, rows, cols)
    shaders.rs        # GLSL 100 sources + ShaderMeta
    textures.rs       # load PNG via turbo_ferret::image::png → miniquad texture (nearest, clamp)
    map_renderer.rs   # batched hex drawing for the main map
    minimap_renderer.rs # render-to-texture cache + blit + marker
  gui/
    mod.rs            # register factories
    map_widget.rs     # MapWidget: input → MapCommand queue
    minimap_widget.rs
    root.rs           # root key handling (Ctrl+Q, F11, Alt+F)
tests/
  state_*.rs  view_*.rs  layout.rs
```

### 3.2 Dependencies (`Cargo.toml`)

```toml
[dependencies]
miniquad = "=0.4.11"   # the version turbo-ferret-miniquad is built against
turbo-ferret = { git = "https://github.com/sstanfield/turbo-ferret", rev = "a37cb83" }
turbo-ferret-miniquad = { git = "https://github.com/sstanfield/turbo-ferret", rev = "a37cb83" }
```

Turbo Ferret's default features (`std`, `png`, `default-font`) are what we want. Pin
`rev` and bump it on purpose. If both repositories end up checked out side by side, a
`[patch]` to a path is fine for local work, but don't commit it.

### 3.3 How the GL map and the UI share a frame

Turbo Ferret has no `BeginNativeRender`. Widgets paint through a batched, read-only
`PaintCtx`, and the `Renderer` trait only draws quads. **Do not add a native-render
hook to Turbo Ferret in phase 1.** Instead:

1. `MapWidget` and `MiniMapWidget` are ordinary Turbo Ferret widgets with no skin and
   no paint. They exist for layout, focus and input.
2. Each frame, the host (`App::draw`) does the following:
   1. If the minimap is dirty, render it into its offscreen render target (miniquad
      `new_render_pass` with a color texture). This has to happen **before**
      `begin_default_pass`, which is why it replaces the C++ `glReadPixels` trick.
   2. `begin_default_pass(clear 0.5 grey)`.
   3. For each GL widget, find its screen rect with
      `ui.to_root(id, Point::ZERO)` + `ui.padding_rect(id)` (check which one gives the
      padding rect in local coordinates). Convert to miniquad's bottom-left origin
      (`y = fb_h - (top + h)`) and call `apply_viewport` and `apply_scissor_rect`.
      Then draw the map (or blit the minimap texture and draw the marker).
   4. Reset the viewport and scissor to the full framebuffer.
   5. `ui.paint(&mut renderer)`. The UI draws on top. The map widgets paint nothing,
      and later popups and menus correctly cover the map.
   6. `end_render_pass`, `commit_frame`.
3. `MiniquadRenderer` owns the miniquad context. Reach it with `renderer.context_mut()`
   for the game's own pipelines, buffers and textures. Create those in `App::new`
   after the renderer exists.

If the map ever needs to sit *between* UI layers (for example a semi-transparent
panel over it that the map must not cover), that is the time to propose a small
"native region" hook in Turbo Ferret. Not now.

### 3.4 Shaders and drawing

Write the shaders in **GLSL 100** (`#version 100`, `attribute`/`varying`,
`gl_FragColor`, `precision mediump float;` in fragment shaders). That runs on GL 2.1+,
GLES 2 and WebGL 1, and it is what turbo-ferret-miniquad uses. Embed them as Rust
`const &str`. Only the OpenGL backend is supported: if `ctx.info().backend ==
Backend::Metal`, fail with a clear message. miniquad defaults to GL on macOS.

Batch instead of making one draw call per hex. The C++ issues 1 to 3 draw calls per
visible hex, each with uniform changes. In Rust:

- Each frame (or only when the view, cursor, selection or map changes), build CPU-side
  vertices for every visible hex: `pos: [f32; 2]` already in widget NDC (apply the
  factor and center math on the CPU, using the formulas in section 2), `uv: [f32; 2]`,
  `bary: [f32; 2]` (x = center weight, y = rim weight, as in the C++ data), and
  `border: [f32; 4]` per vertex (rgb + threshold), so the cursor hex needs no separate
  draw.
- Group the vertices into one index range per texture: one per terrain texture, one
  for `default_unit.png`, and one for the overlay pass. Use one dynamic vertex buffer
  and one dynamic index buffer per frame, created with `BufferUsage::Stream`. Grow them
  when needed.
- There are two pipelines:
  - `hex`: texture × border test, alpha blend `SRC_ALPHA, ONE_MINUS_SRC_ALPHA`.
  - `flat`: a solid uniform color. It is used for the overlay (1,0,0,.3), the minimap
    hexes and backing, and the minimap marker.
  
  Plus `blit` to draw the cached minimap texture.
- Draw order must match the C++ order: all terrain, then units, then overlays. Hexes
  don't overlap, so drawing grouped by texture looks the same as the C++ per-hex order.
  Check this with the screenshot comparison anyway.
- **The 16-bit index limit:** at the widest zoom, about 40×35 hexes × 7 vertices ×
  3 layers is well under 65,536 vertices. Assert it anyway.
- Note that miniquad's Metal backend can only draw from index 0. We are GL-only (macOS, Linux, and Windows on its GL backend), so
  `base_element` offsets are fine, but keep each draw's indices in its own small
  buffer if that turns out simpler. turbo-ferret-miniquad's renderer does it that way.

### 3.5 State, view and input: who owns what

- `App` owns `Game { map: Map, selected: Option<UnitId>, range: Option<MoveRange> }`,
  `MapView`, `MiniMapView` and the renderers. There is no global singleton and no
  `Rc<RefCell<…>>` for game state.
- Turbo Ferret factories are `fn() -> Box<dyn Widget>`, so widgets must be `Default`
  and cannot capture state. `MapWidget` therefore **records commands**:
  `enum MapCommand { Hover(Point, Size), Click(Point, Size), Key(SpecialKey|char) }`.
  It pushes them to a `Vec` inside itself. `on_event` returns `true` for exactly the
  events the C++ consumed.
- After each input call and before `update`, `App` drains the commands with
  `ui.with_widget::<MapWidget, _>(map_id, |w, _| w.take_commands())` and applies them to
  `Game` and `MapView`. Hit-testing (`MapView::pick`) is pure math on the view, so the
  widget needs no access to it.
- This keeps `state` and `view` free of UI and GL code and fully unit-testable. It is
  also the shape phase 3 wants: the UI produces orders and the game consumes them.
- `MapView` holds `zoom_level`, `center: Hex` and the widget size, and derives
  `wf`/`hf`. Its methods: `resize`, `zoom_in`, `zoom_out`, `center_on`,
  `visible_hexes()`, `pick(rel_x, rel_y) -> Option<Hex>`,
  `keep_cursor_visible(cursor, axis)`.
- Use `usize`/`u16` for coordinates in `state`, and do signed math in `view`. Avoid the
  C++ unsigned underflow in the recenter checks (D6).

### 3.6 Things deliberately dropped or replaced

- **Lua.** It existed only to build the layout from code. Replace it with
  `assets/layouts/main.tb.txt` (the same tree, written with `lp: width: Npx` lines that
  `App::new` fills in from the viewport before inflating) or with a fixed layout plus
  `ui.set_layout_params` after inflation. Pick whichever is less code. This drops a
  C dependency (`mlua` would need Lua built from C) for 22 lines of script. If data
  files are needed later, Turbo Ferret's `.tb.txt` parser (`turbo_ferret::res`) can
  read plain data nodes, so units and maps can use that format with no new dependency.
- **Fonts.** Use Turbo Ferret's built-in DejaVu font at 14 dp
  (`ui.dimension_converter().dp_to_px(14)`). Its licence notice must ship with
  binaries; put it in `README.md`/`NOTICE`. `turbo-ferret-ttf` with DroidSans is
  optional later, at no extra external cost.
- **Skin.** Load `skin/skin.tb.txt` through `Services::with_loader(FsLoader::new(asset_root))`.
  Skip the empty `skin.hex.txt` override, or pass it. Either works.
- **TB shortcut mapping** in `Application::InvokeShortcut`: the `InputAdapter` already
  produces `Shortcut` events.
- **The gamepad, Emscripten, GLEW and `glcapabilities` code** is not ported. A web
  build is a later option (miniquad supports wasm). Keep asset loading behind Turbo
  Ferret's loader so it can swap later.
- **Asset path.** `--assets <dir>` if given. Otherwise `<exe dir>/../assets` if it
  exists (the C++ rule). Otherwise `env!("CARGO_MANIFEST_DIR")/assets`, so that
  `cargo run` works.

---

## 4. Ported data structures (sketch)

```rust
// state/terrain.rs
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum TileType { Grass, Forest, Swamp, Desert, Hill, Mountain, Water, Temple, Ruin, City }
impl TileType { pub fn from_char(c: char) -> Option<Self>; pub fn to_char(self) -> char; }

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct MoveCost { pub grass: u16, forest, swamp, desert, hill, mountain, water, shore, bridge, road, city }
impl MoveCost { pub fn entry(&self, t: TileType) -> Option<u16> /* None = impassable (0) */ }

// state/units.rs
pub enum Side { Black, White, Red, Green, Blue, Orange, Yellow, Purple, Grey /* neutral */ }
pub const NUM_SIDES: usize = 8;
pub struct UnitId { pub side: Side, pub index: u32 }      // slot index, reused like the C++
pub struct Unit { id, side, name: String, upkeep, strength, movement, move_cost: MoveCostId, base_move_points, pos: Hex }
pub struct Units { per_side: [Vec<Option<Unit>>; NUM_SIDES], move_costs: Vec<MoveCost> }

// state/map.rs
pub const MAX_STACK: usize = 8;
pub struct Tile { pub kind: TileType, pub frozen: bool, pub road: bool, pub units: Vec<UnitId> /* ≤ MAX_STACK */ }
pub struct Map { rows: u16, cols: u16, tiles: Vec<Tile>, pub cursor: Hex, pub units: Units, pub cities: Cities }

// state/movement.rs
pub struct MoveRange { origin: Hex, cost: Vec<Option<u32>> }   // cost to enter each hex; None = unreachable
pub fn compute_range(map: &Map, unit: &Unit) -> MoveRange;     // Dijkstra over odd-q neighbours
```

`Tile::currentMoveCost` and `Tile::stacked` were scratch fields for the flood fill.
They move into `MoveRange`, so `Map` is no longer mutated by selection.

---

## 5. Milestones

Each milestone is one or more small commits with tests, and should be reviewable on
its own.

**M0: Reference capture (optional, recommended).** Build the C++ demo under Xvfb with
Mesa (llvmpipe has GL 3.3 core). It needs `libx11-dev libxrandr-dev libxinerama-dev
libxcursor-dev libxi-dev libgl1-mesa-dev libfreetype-dev libpng-dev libjpeg-dev
xvfb`. The `glfw` submodule uses an SSH URL (`git@github.com:sstanfield/glfw.git`);
switch it to HTTPS or clone upstream GLFW 3.x into `glfw/`. Capture screenshots of:
the start view; after clicking the unit (range overlay); after `1` and after `2`;
the cursor at a map edge and corner. Store them in `docs/reference/`. If the build
fights back for more than about an hour, skip it and say so. Section 2 is detailed
enough to work from.

**M1: Skeleton.** In the new repository, add `.gitignore` (including the reference checkout), the Cargo package, the copied assets, `CLAUDE.md`,
the miniquad window (1024×768, "HEX Game!", `high_dpi: true`), the grey clear, Turbo
Ferret `Ui` + `MiniquadRenderer` + `InputAdapter` wired as in turbo-ferret's
`demo/src/main.rs`, the skin and the built-in font. Add `--frames N` (quit after N
frames, for smoke tests). Done when `cargo run` shows a grey window and Ctrl+Q quits.

**M2: State.** `state/` complete, with tests:
- the neighbours of odd and even columns and at the edges;
- that `TileType` char codes round-trip;
- that a map file round-trips (write, read, compare) and that bad dimensions are
  rejected;
- the stack limit of 8;
- unit slot reuse after a kill;
- `MoveCost` interning returns the same id for the same tuple;
- the `reset_move_points` carry-over (0→0, 1→1, ≥2→2);
- `compute_range` on hand-made maps (grass costs, a mountain wall, an impassable
  pocket, and agreement with brute-force BFS on random small maps).

**M3: Layout and widgets.** `MapWidget`, `MiniMapWidget` and root keys are registered.
Inflate the layout and check widget rects in a headless test (`Ui` with `FsLoader` on
`assets/`, viewport 1024×768, `update()`, assert the `main_map` rect is about 819×537
and that the sidebar and button are present). Test command recording: feed
`pointer_move`, `pointer_down`/`up` and `key` into the `Ui` and assert the commands
the widget recorded and which events it consumed.

**M4: MapView math.** Port the factors, visible range, pick and recenter rules. Tests:
- `pick` of each hex center returns that hex at every zoom level;
- the pick formula matches the C++ formula on a grid of sample points (copy the
  formula into the test as the oracle);
- zoom clamps at both ends;
- recentering only happens when the cursor leaves the view.

**M5: Main map renderer.** Hex meshes (the 7 variants), GLSL 100 shaders, texture
loading, batched drawing, border thresholds, unit sprite, overlay. Add a
`--screenshot PATH` mode: render one frame into an offscreen render target of the
window size, read it with `texture_read_pixels`, and write a PNG with a tiny
hand-written PNG encoder (stored deflate blocks are fine, about 60 lines) or a
dev-only `png` dependency behind a feature. Done when the screenshots match M0's (or
section 2) by eye: the diagonal mountains, the red cursor border, the unit, and the
clipped edge hexes.

**M6: Minimap.** Fit math, render-to-texture cache, dirty flag, blit, location marker
(D5). The minimap is dirty on resize and on any terrain change (no terrain changes in
phase 1).

**M7: Interaction.** Apply the commands:
- hover moves the cursor;
- click and Enter run `select_hex` (select, move within range, deselect);
- arrows move the cursor with recentering;
- `1`/`2` zoom;
- Ctrl+Q quits; F11 toggles fullscreen or does nothing.

The range overlay follows the selected unit. Write a headless test that drives `Game`
through: select the unit → move it to (50, 53), three grass hexes east → check the remaining movement
is 14 → deselect.

**M8: Parity pass and docs.** Compare against the reference screenshots. Write the
`README.md` (build, run, controls, licences), keep `CLAUDE.md` current, list every
deviation in `docs/deviations.md`, and run on Linux by hand: resize, HiDPI, keys and
mouse.

Testing notes:
- `cargo test` must not need a GPU. Everything in `state/`, `view.rs`,
  `minimap_view.rs`, `render/hex_mesh.rs` (pure data and variant choice) and the
  headless `Ui` tests runs without one.
- The screenshot mode needs a display. On Linux CI use `xvfb-run`; it is not part of
  `cargo test`.

---

## 6. Deviations from the C++ (fix and record)

| # | C++ behaviour | Rust behaviour | Why |
|---|---|---|---|
| D1 | `Map::Map(file)` writes through a null `map` pointer | Load works | Crash |
| D2 | `selectHex` moves the unit to any hex whose cost is ≤ movement, and **unreached hexes have cost 0**, so a unit can teleport anywhere, mountains included, for free | A unit moves only to reachable hexes (cost > 0 and ≤ movement) | A rule bug that phase 3 must not inherit |
| D3 | The flood fill does one pass (its multi-pass loop is commented out), so costs may be higher than the cheapest path | Dijkstra: always the cheapest cost | The overlay can show a few more hexes than the C++ did. Note any visible difference in the parity pass |
| D4 | `selectHex` selects `units[0]`, which can be null after a removal leaves a hole | Select the first unit present | Crash |
| D5 | The minimap marker is a 30% black quad as large as the whole minimap, at a stale center with swapped half-row and half-column terms, so it just darkens the minimap | The marker is the main map's visible region, translucent black, and follows the main view | It was clearly meant to |
| D6 | Unsigned underflow in the recenter checks near the left and top edges (recenters every key press) | Signed math. Recenter only when the cursor leaves the view | Same intent, no wrap |
| D7 | `resetMovePoints` dereferences empty unit slots | It skips empty slots | Crash (not reached in the demo) |
| D8 | The minimap uninitialized pointer is freed on first resize | n/a in Rust | UB |
| D9 | Root swallows plain `f`/`q` key presses even without modifiers | Only Alt+F and Ctrl+Q are consumed | Harmless now, but would eat typing later. Say if you'd rather keep it |
| D10 | Lua layout, TB, stb, GLEW, glm, GLFW | `.tb.txt`, Turbo Ferret, miniquad | The point of the port |
| D11 | Optional: the map widget does not have focus at start, so arrows do nothing until it is clicked | Optional: focus `main_map` at startup | Only if the M0 build shows the arrows dead at start. Otherwise keep it and note it |

---

## 7. Decisions (answered by Steve, 2026-11-18)

1. **Repository:** a new repository. The old C++ sits in a subdirectory for reference
   only (see 3.1).
2. **Lua:** dropped. If scripting comes back later, it will be something other than Lua,
   so don't design for a Lua layer.
3. **Platforms:**
   - **macOS and Linux** are the test targets. Steve tests on both, so both must build,
     run and pass the hand checks in M8.
   - **Windows** is best effort. Steve has no Windows machine, so keep it possible:
     - use `std::path` and no Unix-only APIs or path strings in game code;
     - no `#[cfg(unix)]`-only features;
     - keep line endings out of the map file parser's logic, so `\r\n` parses;
     - add a GitHub Actions job that runs `cargo build` and `cargo test` on
       `windows-latest`. A failure there is worth fixing but doesn't block a milestone.
   - **Web:** a later option, not a target.
4. **Storage (future):** phase 1 needs no database. When one is needed (save games,
   scenario libraries, the turn and event log), use the LMDB-style key-value store Steve
   wrote. It currently lives inside another project, at
   `/Users/sstanf/work/telcoin-network/crates/tn-lmdb` on Steve's machine, so it is not
   reachable from this repository. Ask Steve for access, or to have it split out as its
   own crate, before planning around its API.

   First decide whether it is overkill. Plain versioned save files, written with the
   same `.tb.txt` format or a small binary format, may be enough until there are many
   saves or the event logs get large. Recommend one or the other to Steve.

---

## 8. Later phases (direction only)

These come after the demo is ported. They are inspired by the feel of the 1990s SSG
*Warlords*: a fantasy map of cities, cheap troops against expensive elite units,
"quality beats quantity", gold from cities paying for production and upkeep, stacks
of up to 8, carry-over movement points. They must **not** clone it. No Warlords names,
factions, lore, maps, artefacts, tables or art. The manual is for mood only. Use hexes,
simultaneous turns and our own unit set and numbers.

### Phase 2: Better placeholder assets and editor basics
- Write a tiny `tools/assetgen` (Rust, no dependencies) that generates hex-shaped
  terrain tiles that match the flat-top mesh, plus unit counters (a simple
  silhouette or icon on a side-colored disc for each of the 8 sides) and city and
  road markers. All of it is generated, so the licensing is clean and it can be
  regenerated. **Done (2026-11-18):** `tools/assetgen` draws the ten
  terrain tiles, roads and bridges, and four signs for units; the game draws a
  unit as its kind's sign on a disc of its side's color, and a banner on an
  owned city (`docs/tiles.md`). A map brings its own kinds of unit and its
  cities (`docs/map-format.md`).
- Show a stack-size indicator on hexes with units (a count, or pips).
- ~~Restore the map editor hotkeys that are commented out in `main.cpp` (G/F/W/H/M/S
  paint terrain, C places a city, Ctrl+S saves, Ctrl+T ends the turn), behind a
  `--edit` flag. The minimap marks itself dirty on edits.~~ **Done differently
  (2026-11-18, the owner's word):** the editor is its own program,
  `tools/mapeditor`, on the game's library. The letter keys paint, Ctrl+S saves;
  ending the turn belongs to the game, and is still to do. The map file format
  is in `docs/map-format.md`, and the game plays a map file with `--map`.
- A sidebar info panel built from Turbo Ferret widgets: the cursor hex's terrain and
  its units.

### Phase 3: Core gameplay, simultaneous turns ("WEGO")
- **The turn loop.** There is an orders phase, in which every player gives orders
  in secret: move along a path, attack, hold, garrison, produce in a city. Then a
  resolution phase plays all orders out at once. Start with hotseat play, where
  players hand off with a hidden-orders screen. An AI comes later.
- **Resolution model** (a starting proposal to iterate on):
  - Each unit's path is split into steps, and each step costs MP.
  - Resolution advances in ticks. In each tick, every unit that can afford its next
    step takes it at the same time.
  - When enemy units would enter the same hex, or swap hexes, they fight.
  - A stack that reaches an enemy-held hex attacks it.
  - Ties are broken deterministically with a seeded RNG that the game state owns.
  - The result is a replayable event log, which drives the animation and later
    network play.
- **Economy.** Cities produce income. Production costs gold and takes turns. Upkeep
  is about half the build cost per turn. City walls are a defence value. A city is
  captured by defeating or being unopposed by its garrison. Neutral cities are
  garrisoned weakly.
- **A vanilla common unit set**, the same for every side. Sample roles (the names and
  numbers are placeholders):
  - Militia: cheap and slow.
  - Spearmen: defensive, with a bonus against cavalry.
  - Swordsmen: line infantry.
  - Archers: a bonus when defending and in forests.
  - Light cavalry: fast scouts.
  - Heavy cavalry: shock troops, poor in forest and swamp.
  - Siege engine: slow, with a bonus against walls.
  - Transport ship: water only, carries a stack.
  - Commander: a hero-like unit that gives a stack bonus.

  Movement classes (foot, mounted, siege, naval) map to `MoveCost` tables, which the
  C++ already models.
- **Combat.** Our own design. A stack-against-stack model where unit quality matters
  more than numbers, with modifiers for terrain, walls and commanders. Keep it in a
  pure `combat.rs` with property tests.
- **Data.** Unit types, terrain costs and city lists live in `.tb.txt` data files,
  read with Turbo Ferret's resource parser, so there is no new dependency.

### Phase 4 and later
AI opponents, fog of war, scenario and map files with cities and starting positions,
save and load, roads and bridges (the fields exist already), heroes or commanders
with simple items, network play built on the order and event log, and a web build.
