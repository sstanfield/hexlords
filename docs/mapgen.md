# mapgen: the random map generator

`mapgen` makes a map directory in the game's format (`docs/map-format.md`)
from a seed and a few knobs, measures any map, and checks one. The same seed
and knobs make the same map, byte for byte, on every machine. The generator
is `hexgame::mapgen` (pure: no files, no clock, so the game can call it one
day); `tools/mapgen` is its command line.

Its targets are a hand-built map's statistics ("the target map", the
`classic` preset): long mountain ranges ringed by hills, big forests, wide
rivers that split the land, a city about every 8 hexes, roads that link them.
That map is not in the repository and never will be; only its aggregate
numbers are used.

**Where it stands.** Built: the ruler (stats, reach, check), the skeleton
(knobs, presets, sizes, files, pictures) and the terrain (land and water,
mountains and hills, forest, swamp, desert, grass, cleanup). To come, in
order: rivers and lakes; cities (capitals, neutrals, sea and air cities,
fairness, names, production); temples and ruins; roads, bridges and passes;
scoring with retries. Until cities come, a generated map has terrain only:
the game opens it, but there is nobody to play.

## Commands

```sh
cargo run -p mapgen --release -- gen --seed 7 --size medium --out target/maps/a --preview --layers
cargo run -p mapgen --release -- batch --seeds 1..16 --size medium --out target/maps/b --sheet
cargo run -p mapgen --release -- stats assets/maps/continent
cargo run -p mapgen --release -- check assets/maps/continent
```

- `gen` writes one map: `terrain.map`, `cities.txt`, `units.txt` (the units
  template's, word for word), `mapgen.txt` (the whole config, seed, size and
  cities included: `--config mapgen.txt` makes the map again), `stats.txt`
  (everything `stats` says, and each target with its value and a mark),
  `preview.png` (with `--preview`) and `layers/` (with `--layers`: the
  elevation, the moisture, the ridge score, in grey). Without `--seed`, a
  seed from the clock, printed.
- `batch` makes `seed-N/` for each seed, each with its preview, and with
  `--sheet` a contact sheet of them all (`sheet.png`, the seed in each
  corner) and `sheet.txt` (each seed's score and what it misses).
- `stats DIR` measures any map: water (the sea, lakes, open and narrow
  water, the edge, the coast, the land's pieces), the land mix, how each
  terrain lies (how much of a hex's neighbourhood is its own terrain against
  a random scatter, its patches, what it borders), the cities (spacing in
  hexes and by travel, by the water, walls, reach, ports), sites, roads
  (networks, lines, dead ends, bridges, passes).
- `check DIR` says what the game's reader does not refuse: a bridge or pass
  whose road does not run on at both ends, and each city's reach class (on
  foot from the capitals, by sea, by air) with a warning for a city only a
  ship or a flyer reaches when the map has none, or no way to leave it.

Options of `gen` and `batch`: `--preset NAME` (or a file), `--config FILE`
(on top of the preset), `--units PATH` (the units template; default
`assets/mapgen/units.txt`, the game's own kinds), `--seed N`, and any knob as
`--knob value` (`--water 0.25`, `--sides 6`, `--layout twin`); the
checkboxes are flags (`--sea-cities`, `--fly-cities`).

## Tuning

1. `batch --seeds 1..16 --sheet --out runs/a` and look at `sheet.png`.
2. A bad seed: `gen --seed 7 --preview --layers`; the layers show which stage
   went wrong.
3. Change a knob in a copy of a preset and run the batch again with the same
   seeds into `runs/b`. Each stage draws on its own stream of chance, so only
   what comes after the change moves.
4. Play the good ones (once there are cities): `hexgame --map runs/b/seed-3`.

## The knobs

A config is `key value` lines, `#` for comments, as `cities.txt` is. The
presets are in `assets/mapgen/presets/` (and built into the library):
`classic` (the default), `varied` (some desert), `open` (much grass, fast
games), `rugged` (mountains and hills, more passes), `islands` (an
archipelago; cities only ships reach; the units template needs a ship).

| Knob | Default | Meaning |
|---|---|---|
| `seed` | from the clock | 64-bit |
| `sides` | 4 | 2 to 8 |
| `side_colours` | the first of black, white, red, green, blue, orange, yellow, purple | or a list |
| `cities`, `cities_per_side` | 4 x 10 | the cities, capitals included |
| `density` | `normal` | land hexes per city: `sparse` 230, `normal` 170, `dense` 120, or a number |
| `size` | from the cities | `ROWSxCOLS`, or `tiny` 38 x 44, `small` 54 x 62, `medium` 77 x 88, `large` 94 x 108, `huge` 121 x 140 |
| `aspect` | `square` | rows per column: `square` 0.87 (looks square), `portrait` 1.24, `landscape` 0.6, or a number |
| `water` | 0.19 | the share of all hexes that are water |
| `layout` | `coast` | `coast` (the sea from `sea_edges`), `continent`, `inland`, `twin` (two masses, a land bridge), `archipelago` |
| `sea_edges` | `west,north` | for `coast` |
| `lakes`, `rivers` | 1/1500, 1/2500 | per hex (`1/1500`) or a count (to come) |
| `min_island` | 30 | land pieces smaller become water |
| `mix` | `grass 49.5 mountain 17 hill 15.5 forest 14 swamp 3 desert 0` | shares of the land, percent; grass also holds the cities and sites |
| `feature_scale` | 1 | how big the blobs and ranges are |
| `roughness` | 0.5 | 0 smooth to 1 ragged |
| `range_style` | `ridges` | long ranges, or `massifs`, round lumps |
| `hill_ring` | 0.85 | the share of mountain edges that border hills |

Of size, cities and density, two given say the third (`size` and `cities`:
the density; `size` alone: the cities at the density; neither: the size for
the cities). It warns of a density outside 100 to 300, or fewer than 3
cities a side. The knobs of the stages to come (cities, sites, roads,
fairness, ships and flyers, retries) are read and written already, and do
nothing yet: `river_width`, `city_min_gap`, `city_gap_target`,
`coastal_cities`, `edge_margin`, `walls`, `temples`, `ruins`,
`site_city_gap`, `site_gap`, `roads`, `road_loops`, `bridges`, `max_bridge`,
`passes`, `sea_cities`, `sea_city_count`, `fly_cities`, `fly_city_count`,
`ports`, `ports_per_side`, `fairness`, `symmetry`, `attempts`, `accept`. A
checkbox on with a units template that has no such unit stops the generator
with an error that says so.

## How it makes the terrain

1. **Land and water** (`land.rs`): an elevation from noise and the layout's
   shape (a slope to the sea edges, with bays; a rounded square; two masses
   with a channel and a land bridge; home islands). The lowest hexes are the
   sea, exactly as many as `water` asks (every share is hit by taking the
   best-scoring hexes, ties by index, not by guessing thresholds); specks of
   land under `min_island` sink, specks of sea fill, and the share is put
   back. With `sea_cities`, islands rise in the open sea.
2. **Mountains and hills** (`relief.rs`): a ridge score from ridged noise
   drawn out along two directions, each holding sway in broad regions, bent
   by a slow warp, and gathered by a mask into mountain country and plains.
   Its best inland hexes are the mountains; a ring of hills is laid along
   them by the same score (so what is left out is left out in stretches)
   until `hill_ring` of their edges border hills, thickened outward, and the
   rest of the hills grow as free patches of a noise of their own. Land
   walled in by mountains is of no use: a hollow of a few hexes fills with
   mountain, a bigger pocket gets a way out where its wall is lowest (cut
   into hills). (The owner's eye, on the judged maps of
   `tests/mapgen/judged`: such pockets, and ranges in straight parallel
   stripes, were what the worst maps had.)
3. **Forest, swamp, desert** (`biomes.rs`): a moisture from a slow noise and
   the water near. Swamps grow from one to three seeds where the land is low
   and wet by the water, round about their seed; forest is the wettest
   grass; desert the driest, far from water and swamps.
4. **Cleanup** (`cleanup.rs`): two passes of a majority filter (a hex with
   five or six neighbours of one other terrain becomes it; water and
   mountains stay; some lone trees too), then each terrain back on its
   quota.

Every stage uses only `+ - * /`, `floor` and `sqrt` where results pick
terrain (no `sin`, `exp` or `powf`, whose last bits differ between
platforms), breaks ties by the hex's index, and draws its chance from its own
stream (`rng.rs`, SplitMix64).

## How well it does

Over 20 seeds each of medium and huge maps (`tests/mapgen_terrain.rs`):
water and every land share within a point of the config on every map; the
mountains' edges on hills within 0.08 of `hill_ring` on every map; and how
each terrain keeps to itself, on average, within 0.08 of the target map's
(grass 0.84, forest 0.79, hill 0.60, mountain 0.80, swamp 0.82). Water keeps
to itself more (0.95 against 0.87) until rivers narrow it. Tiny maps come
within twice that.

`tests/mapgen_target.rs` holds the ruler to the target map's numbers. It
reads the map from `MAPGEN_TARGET_DIR` and is ignored unless asked for:

```sh
MAPGEN_TARGET_DIR=<the map's directory> cargo test --test mapgen_target -- --ignored
```
