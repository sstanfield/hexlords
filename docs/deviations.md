# Deviations from the C++

Where the Rust port behaves differently from the C++ hexGame demo
(`reference/hexGame`, commit `99edad5`) on purpose. D1 to D11 are the ones the
plan (`hexgame-rust-port-plan.md`, section 6) asked for; the rest were found
while porting.

**This record is closed.** Since 2026-11-18 the game no longer follows the C++
(the owner's word), so later changes are not deviations and are not listed. The
last section names the first of them, because they undo things this document
says were kept.

There is no C++ reference build to compare against: milestone M0 (building the
C++ under Xvfb and capturing screenshots) was skipped at the owner's word, since
pixel and bug parity with an in-progress demo is not the goal. The Rust build was
checked by eye against section 2 of the plan; the pictures are in
`docs/screenshots/`.

## Bugs fixed

| # | C++ behaviour | Rust behaviour | Where |
|---|---|---|---|
| D1 | `Map::Map(file)` writes through a null `map` pointer | Loading a map file works | `state/map.rs` `Map::parse` |
| D2 | `selectHex` moves the selected unit to any hex whose cost is ≤ its movement, and hexes the flood fill never reached have cost 0, so a unit can go anywhere, mountains included, for free | A unit moves only to hexes it can reach. Choosing any other hex does nothing and keeps the selection | `game.rs` `Game::select_hex` |
| D3 | The flood fill makes one pass (its multi-pass loop is commented out), so a cost can be higher than the cheapest path's | Dijkstra: always the cheapest cost. The overlay may cover a few more hexes than the C++ showed; without a C++ build this was not compared | `state/movement.rs` |
| D4 | `selectHex` selects `units[0]`, which is null after a removal leaves a hole in the stack | A stack has no holes; the first unit on the hex is selected | `state/map.rs`, `game.rs` |
| D5 | The minimap marker is a 30% black quad as large as the whole minimap, at a center that never changes (with the half-row and half-column terms swapped), so it only darkens the minimap | The marker is the part of the map the main view shows, and follows it | `minimap_view.rs` `MiniMapView::marker` |
| D6 | The recenter checks subtract in unsigned numbers: near the left and top edges the bound wraps around and every key press recenters | Signed math: the view recenters only when the cursor leaves it | `view.rs` `MapView::keep_cursor_visible` |
| D7 | `resetMovePoints` dereferences empty unit slots | It skips them | `state/units.rs` |
| D8 | The minimap frees an uninitialized pointer on its first resize | Not applicable | |
| D18 | `placeUnit` silently does nothing on a full stack, and `moveUnit` has already taken the unit off its old tile by then: the unit is lost | Placing and moving return an error and change nothing if the unit cannot go there | `state/map.rs` |

## Behaviour changed

| # | C++ behaviour | Rust behaviour | Why |
|---|---|---|---|
| D9 | The root widget swallows plain `f` and `q` key presses | Only Alt+F and Ctrl+Q (and Cmd+Q) are consumed | It would eat typing later |
| D11 | The root has the keyboard at start, so the arrows do nothing until the map is clicked | The map is focused at start | Nicer, and nothing depends on the old way |
| D12 | Units are kept in lists for the eight player sides; a unit of the neutral side (grey) would index past the array | Grey has a unit list too | Neutral garrisons need it (phase 3) |
| D13 | The minimap is scaled by the smaller of `1/(rows·hf)` and `1/((cols/2)·1.5·wf)` around hex (rows/2, cols/2), which cuts half a row off the top and leaves a gap at the bottom | The map's exact extent is fitted and centered | The cut shows on small maps |
| D14 | `main.lua` sizes the map and the minimap once at startup from the layout's rectangle, with `distribution: available` everywhere (which centers the map vertically in its row and stretches the "SLS" button to the row's height), and makes the minimap as wide as the rest of the window (5 px too wide once the spacing is counted) | The sizes are set from the window's size, and again whenever it changes. The map sits top left, the button keeps its own size, and the minimap's width allows for the spacing | A layout that survives a resize |
| D15 | The map acts on `1` and `2` but returns "not handled" (a fall-through in a `switch`) | It consumes the presses it acts on | The intent |
| D16 | Ctrl+Q quits | Ctrl+Q and Cmd+Q quit | Cmd+Q is the usual key on macOS, and miniquad does not handle it itself |
| D17 | F11 calls a fullscreen stub that does nothing | F11 switches fullscreen on and off (`miniquad::window::set_fullscreen`). Alt+F ("next fullscreen mode") is still consumed and does nothing | The plan allowed either |
| D19 | The map file reader ignores letters it does not know and anything past the last column | Both are errors that name the line. Short and missing lines still leave grass, and `\r\n` line ends parse | A wrong file should say so |

## Replaced (the point of the port)

| # | C++ | Rust |
|---|---|---|
| D10 | Turbo Badger, Lua (`main.lua`), stb_image, GLEW, glm, GLFW | Turbo Ferret, `assets/layouts/main.tb.txt` plus a few lines of Rust, Turbo Ferret's PNG decoder, miniquad |
| D20 | One to three draw calls per visible hex, each with its own uniforms; the minimap cached by drawing it into the window and reading it back with `glReadPixels` | The visible hexes are built into a mesh per layer on the CPU (terrain by texture, then units, then the overlay): about ten draw calls a frame. Hexes do not overlap, so the layers look the same as the per-hex order. The minimap is drawn into a render target, and again only when its size or the terrain changes |
| D21 | DroidSans 14 dp through a TrueType renderer; a language file (`lang/lng_en.tb.txt`, missing from the repository) | Turbo Ferret's built-in font (DejaVu Sans) at 14 dp; no language file |
| D22 | The mouse is scaled from window to framebuffer coordinates by hand | The UI works in framebuffer pixels (Turbo Ferret's miniquad backend), on HiDPI screens too |

## Not ported

- The gamepad, Emscripten and GLEW paths, `glcapabilities`, `distancemap`,
  `util/string`, and Turbo Badger's own GL renderer.
- The root widget's count of dimmers (the demo has none).
- The map editor hotkeys that are commented out in `main.cpp` (phase 2).
- The shortcut mapping in `Application::InvokeShortcut`: Turbo Ferret's
  `InputAdapter` produces `Shortcut` events itself.

## Kept at the end of the port

These look like bugs or leftovers but are what the C++ does, and the port kept
cosmetic quirks:

- City, temple, ruin and desert tiles all use `citycenter_tile.png`; desert and
  city are black on the minimap. `border_tile.png` and `city1..6_tile.png` are
  in `assets/tiles` but never drawn.
- The hexes cut off at the map's top and bottom edges have no border along the
  cut (every model uses the whole hex's rim weights).
- The "SLS" button does nothing.

## Changed after the port

Kept by the port, then changed once parity stopped mattering:

- **Picking is exact.** The C++ formula treated a hex as the rectangle around
  it, so a point near a slanted edge picked the neighbour in the next column.
  `view::hex_at` tests the hex's real shape, and the cut-off halves of the
  map's edge hexes pick nothing.
- **The minimap navigates.** A click on it sends the main map (and the cursor)
  to that place, and a drag scrolls. In the C++ it took clicks and did nothing.
- **The keyboard stays with the map.** A click on the minimap or the button
  used to take the focus, and the arrows then did nothing until the map was
  clicked again. Now only the map takes the focus.
- **Tile images are the right way up.** The C++ texture coordinates had v = 1 at
  the top of a hex, which drew every image upside down relative to its file
  (it only showed on the unit image).
- **The window comes to the front on macOS** (`platform::bring_to_front`), where
  miniquad opens it behind the terminal.
