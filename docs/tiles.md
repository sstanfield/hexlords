# The terrain tiles

Every terrain of the map has a tile: a picture the game draws on each hex of
that terrain. The ten in `assets/tiles/` are drawn by a program,
`tools/assetgen`, not by hand: noise, shapes and a little light, all computed.
So the art is original, it has no licence but the game's, and it can be made
again, at any size, by anyone:

```sh
cargo run -p assetgen --release                                # assets/tiles, as the game ships them
cargo run -p assetgen --release -- --previews target/previews  # and pictures of maps drawn with them
```

This page is for whoever changes the tiles, or draws others: how a tile lies
in its picture, what the game does with it, and how to change the art.

## A tile's picture

`assets/tiles/<name>_tile.png`, where `<name>` is the terrain's name: `grass`,
`forest`, `hill`, `swamp`, `desert`, `mountain`, `water`, `city`, `temple`,
`ruin` (the terrains of `docs/map-format.md`).

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
  the minimap's.

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

`assets/tiles` holds what the generator makes and nothing else: a test draws
every tile again and compares. Do not edit a tile's PNG, and do not add other
files there; change the generator and run it. A tile drawn some other way (by
hand, by another program) can replace the generator's, if it follows "A tile's
picture" above; then that test goes, and the tile's origin and licence go
into `NOTICE`.

### The generator's options

| Option | What it does |
|---|---|
| `--out <dir>` | Where the tiles go. Default: `assets/tiles` (run it from the repository's root) |
| `--size <pixels>` | The side of a tile, 8 to 2048. Default: 256 |
| `--cutout` | Leave what is outside the hex transparent (with one pixel of terrain past the rim): tiles to look at, or for a renderer that takes single pixels. Not for the game |
| `--atlas` | Also write `atlas.png` (all ten tiles in a grid of 5 x 2, two transparent pixels around each) and `atlas.txt` (a line for each: `name letter x y w h u0 v0 u1 v1`). The game does not use an atlas |
| `--previews <dir>` | Also draw maps with the tiles: `near.png` and `near_no_border.png` (a mixed map, about ten hexes across), `closeup.png` (256 pixel tiles, a pixel for a pixel), `far.png` (a made-up continent, about 68 hexes across), `seams.png`, and `far.map`, the continent as a map file |

The previews are drawn by the generator itself, the way the game lays a map
out, so the art can be judged without a window. `assets/maps/continent.map` is
`far.map`.

## Not here yet

Units, cities' owners, roads and rivers have no generated art. The one unit
picture, `assets/units/default_unit.png`, is the C++ demo's.
