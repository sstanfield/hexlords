# The map's pictures: tiles, roads, units

Every terrain of the map has a tile: a picture the game draws on each hex of
that terrain. Over it go the hex's road, if it has one, and the sign of a unit
that stands there. All of these are drawn by a program, `tools/assetgen`, not
by hand: noise, shapes and a little light, all computed. So the art is
original, it has no licence but the game's, and it can be made again, at any
size, by anyone:

```sh
cargo run -p assetgen --release                                # the game's assets, as it ships them
cargo run -p assetgen --release -- --previews target/previews  # and pictures of maps drawn with them
```

| Directory | What | Size |
|---|---|---|
| `assets/tiles/` | `<terrain>_tile.png`: the ten terrains; `city_<side>_tile.png`: a city for each side | 256 x 256, no transparency |
| `assets/roads/` | `road_NN.png` and `bridge_NN.png`: a road over land and over water, in each of its 14 shapes | 256 x 256, transparent |
| `assets/units/` | `<name>.png`: the units' signs (and `default_unit.png`, the C++ demo's) | 128 x 128, transparent |

This page is for whoever changes these pictures, or draws others: how a
picture lies in its file, what the game does with it, and how to change the
art. A map can bring pictures of its own that replace the game's
(`docs/map-format.md`, "Pictures of its own"); they follow the same rules.

## A tile's picture

`assets/tiles/<name>_tile.png`, where `<name>` is the terrain's name: `grass`,
`forest`, `hill`, `swamp`, `desert`, `mountain`, `water`, `city`, `temple`,
`ruin` (the terrains of `docs/map-format.md`).

A city is drawn with the picture of the side that holds it:
`city_<side>_tile.png`, `<side>` one of `black`, `white`, `red`, `green`,
`blue`, `orange`, `yellow`, `purple`, and `city_tile.png` for nobody's. They
are the same town, with its roofs (of the houses, the towers and the keep)
in the side's color and its wall's stone tinted with it, so that whose a city
is shows at a glance, even when a hex is a few pixels wide. The colors are
the game's (`side_color` in `src/render/map_renderer.rs`, `sides.rs` in the
generator; a test holds them together). A map that draws its own cities
gives all nine.

- **Square, 256 x 256, RGBA, no transparency.** A side has to be a power of
  two (the game makes smaller copies of the picture by halving it).
- **The hex fills the square.** It is flat-topped: its points are at the
  middle of the left and the right edge, its flat edges run along the top and
  the bottom from a quarter to three quarters of the width. The picture's
  first row is the top of the hex.

  ```
       (64,0) ________ (192,0)
             /        \
            /          \
    (0,128)              (256,128)
            \          /
             \________/
     (64,256)          (192,256)
  ```

  In the game's hex model (2 wide and 2 high, x right, y up, the middle at 0)
  a point is at `u = (x + 1) / 2`, `v = (1 - y) / 2` of the picture.
- **The four corners outside the hex are painted too**, with the ground of the
  neighbouring hexes: the terrain simply goes on. The game never draws them,
  but it mixes neighbouring pixels when it scales a tile (below), and next to
  the hex's rim those must not be empty.
- **The art is squeezed.** The game draws y at 0.863 of x, so that hexes are
  regular on the screen. A tile is drawn as it will look on the screen and then
  stretched to the square's height: in the file a round dome is an ellipse
  taller than it is wide, and on the map it is round again.

## What the game does with it

- A hex is drawn with its tile, then a border over its outer twentieth (black,
  red under the cursor). The border hides where two terrains meet, which is a
  hard edge.
- On the screen a hex is anything from about 270 pixels wide (the closest zoom
  on a high-density display) to about 20 (the farthest on an ordinary one). The
  tiles are scaled smoothly, with mipmaps: `Sampling::Smooth` in
  `src/render/textures.rs`. That is why they have no transparency.
- The minimap does not use the tiles. It gives every terrain one color
  (`terrain_color` in `src/render/minimap_renderer.rs`): for open country the
  mean color of its tile (a test compares them), for the three places a color
  that stands out. Change a tile's colors much and that test says to change
  the minimap's. A city is a square over its hex, a few pixels across
  whatever the map's size, in its holder's color on a dark one.

## Roads and bridges

A hex with a road (a capital letter in the map file) gets a second picture
over its tile, transparent but for the road. The road leaves the hex through
the middle of each side whose neighbour has a road too, or is a city, and
crosses the side squarely, so that it goes on in the neighbour without a step.

Which sides those are is a number from 0 to 63: a bit for each side,
clockwise from the top (1 north, 2 north-east, 4 south-east, 8 south, 16
south-west, 32 north-west). Turning a road by a sixth of a turn gives another
of the 64, so there are only **14 shapes** to draw, and the game draws each
one turned as the map needs it:

| File | Sides | The road |
|---|---|---|
| `road_00` | none | a small round place |
| `road_01` | N | a dead end |
| `road_03` | N, NE | a tight bend |
| `road_05` | N, SE | a wide bend |
| `road_09` | N, S | straight through |
| `road_07`, `road_11`, `road_13`, `road_21` | three sides | forks |
| `road_15`, `road_23`, `road_27` | four | crossings |
| `road_31`, `road_63` | five, six | where everything meets |

`bridge_NN.png` is the same shape over water: planks between two rails, with
a shadow on the water. A picture is the hex's square like a tile, and it is
**drawn for its lowest turn**: `road_03` has the road leaving through the top
and the upper right. Because the game turns it, the art must not have a
light from one side or anything else that looks wrong turned.

Transparent pixels are not black: they have the color of the road's rim and
no opacity. The game scales these pictures smoothly, which mixes a pixel with
its neighbours, and a black neighbour would darken the road's edge.

No road is drawn over a city, a temple or a ruin: the roads beside one run up
to it.

## Units

A unit is drawn as a disc in its side's color, half a hex wide, with the
picture of its kind on it (`image` in the map's `units.txt`). The game's
signs are a sword (`militia.png`), a spear (`lightinfantry.png`), a shield
(`wardens.png`), a helmet with a visor (`heavyinfantry.png`), a bow
(`archers.png`), a pine (`rangers.png`), two peaks (`highlanders.png`), a
catapult (`siegeengine.png`), a horseshoe (`lightcavalry.png`), a stirrup
(`cavalry.png`), a banner (`captain.png`), a
longship with shields along its side (`warship.png`), a round-bellied cog
(`transport.png`), a feather (`hawkriders.png`) and a pair of wings
(`wyverns.png`): light, with a dark outline, on nothing, and without a color
of a side, since the disc has that. `riders.png`, which maps made before
the riders were split still name, is the horseshoe too.

A unit's picture is the hex's square like a tile, **with the sign inside the
disc**: within 0.45 of the middle, where the hex is 1 from its middle to its
points (in a 128 pixel picture, within 29 pixels of the middle sideways and
33 up and down). Its transparent pixels have the outline's color, for the
same reason as the roads'. A city shows its own picture, never a unit's:
its garrison is a small pip for each unit, in the unit's color, in rows of
four over the gate at the bottom of the tile, so keep that part of a city
plain enough for them to show.

In the panel under the map the same picture is the unit's icon, on a round
disc: what is drawn of it is scaled to fit inside the disc's ring, wherever it
lies in its square, so a picture of a map's own that fills its square (as
`default_unit.png` does) shows whole.

## No seams

Two tiles of the same terrain side by side look like one piece of ground. The
ground's texture and everything strewn on it (tufts, trees, hills, peaks,
reeds, waves, pebbles) repeat on the hex grid itself: what leaves a tile
through one edge comes in through the opposite one, and a tree cut by an edge
has its other half on the neighbour. City, temple and ruin are a picture in
the middle of a grass tile, so they join grass, and each other, the same way.

Keep it so when changing the art:

- Take noise from `noise::fractal` and `noise::spread`, which repeat on the
  grid, and put small things down with `scatter::scatter`, which copies each
  one to the neighbouring hexes.
- Whatever is drawn at a fixed place (a city's wall) must stay well inside the
  hex. `ICON` in `terrain.rs` scales the three places' pictures.

The generator's tests check both (`open_country_repeats_from_hex_to_hex`,
`the_places_are_grass_at_their_edges`), and `--previews` draws `seams.png`,
each terrain in a block without borders, to see it.

## Changing the art

Each terrain is one function in `tools/assetgen/src/terrain.rs` (`grass`,
`forest`, ...) that gives the color at a point; the colors and the shapes are
there. How many trees, peaks and so on a tile has, and how big, is in
`Features::new` in `scatter.rs`. Then:

```sh
cargo run -p assetgen --release -- --previews target/previews   # draw, and look at target/previews/*.png
cargo test --workspace                                          # the tiles in assets/ are the generator's
cargo run -- --map assets/maps/continent.map                    # and in the game
```

The roads are in `road.rs` (`road`, `bridge`, and `lines`, the way a shape's
road runs), the signs in `icons.rs`, each a shape given by its distance.

`assets/tiles`, `assets/roads` and the signs in `assets/units` hold what the
generator makes and nothing else: tests draw every picture again and compare.
Do not edit a PNG, and do not add other files there; change the generator and
run it. A picture drawn some other way (by hand, by another program) can
replace the generator's, if it follows this page; then its test goes, and the
picture's origin and licence go into `NOTICE`. A map that wants other pictures
does not have to touch the game's: it brings its own.

### The generator's options

| Option | What it does |
|---|---|
| `--out <dir>` | The asset directory: the pictures go into its `tiles`, `roads` and `units`. Default: `assets` (run it from the repository's root) |
| `--size <pixels>` | The side of a tile and of a road, 8 to 2048; the signs are half that. Default: 256 |
| `--cutout` | Leave what is outside the hex transparent in the tiles (with one pixel of terrain past the rim): tiles to look at, or for a renderer that takes single pixels. Not for the game |
| `--atlas` | Also write `tiles/atlas.png` (all ten tiles in a grid of 5 x 2, two transparent pixels around each) and `atlas.txt` (a line for each: `name letter x y w h u0 v0 u1 v1`). The game does not use an atlas |
| `--previews <dir>` | Also draw maps with the pictures: `near.png` and `near_no_border.png` (a mixed map with roads, a bridge and a pass, about ten hexes across), `closeup.png` (256 pixel tiles, a pixel for a pixel), `far.png` (a made-up continent, about 68 hexes across), `seams.png`, and `far.map`, the continent as a map file |

The previews are drawn by the generator itself, the way the game lays a map
out, so the art can be judged without a window. The terrain of
`assets/maps/continent` began as `far.map`.

## Not here yet

Rivers, and anything that moves on water. The minimap shows no roads. The
unit picture `assets/units/default_unit.png` is the C++ demo's, drawn for a
kind of unit that names no picture.

## Windows and buttons

The game's windows, menus, buttons and the box over a unit are drawn with
pictures the generator makes too (`tools/assetgen/src/skin.rs`), named
`hg_*.png` in `assets/skin`, each at the skin's four sizes (`hg_window.png`,
`hg_window@192.png`, `@288`, `@384`), so they are sharp on any screen:
a window is a dark panel inside a thin bronze line, with round corners and
a soft shadow; its title bar darker, the title in gold; a button a raised
plate, sunk and lined in bronze when pressed; and the button a window means
its player to press (`build::primary_button`, the skin's `Primary`) in
bronze. `assets/skin/hexgame.tb.txt` says how the skin cuts each into nine
(the corners stay, the edges and the middle stretch); the rest of the skin
is Turbo Badger's. To change their look, change `skin.rs` and run the
generator.
