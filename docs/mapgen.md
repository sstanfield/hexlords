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
(knobs, presets, sizes, files, pictures), the terrain (land and water,
mountains and hills, rivers and lakes, forest, swamp, desert, grass,
cleanup) and the cities (capitals, neutrals, sea and air cities, fairness,
names, walls, what they build and who stands in them). To come, in order:
temples and ruins; roads, bridges and passes; the rest of scoring. **Until
the roads bring bridges, a river is a wall to every walker**: the game
plays a generated map, but armies on either side of a river never meet.

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
| `lakes`, `rivers` | 1/1500, 1/2500 | per hex (`1/1500`) or a count |
| `river_width` | `2..3` | a river's width, from its source to its mouth |
| `min_island` | 30 | land pieces smaller become water |
| `mix` | `grass 53.5 mountain 13 hill 15.5 forest 14 swamp 3 desert 0` | shares of the land, percent; grass also holds the cities and sites (the target map has 17 mountain; our thinner ranges spread about as far with 13) |
| `feature_scale` | 1 | how big the blobs and ranges are |
| `roughness` | 0.5 | 0 smooth to 1 ragged |
| `range_style` | `ridges` | long ranges, or `massifs`, round lumps |
| `hill_ring` | 0.85 | the share of mountain edges that border hills |
| `city_min_gap` | 0.38 x the square root of the density, at least 4 (5 at `normal`) | the least hex distance between cities |
| `city_gap_target` | 0.6 x the square root of the density (8) | the middle distance between neighbours aimed at |
| `coastal_cities` | 0.25 | the share of cities by the water aimed at |
| `edge_margin` | 2 | no city nearer the map's edge |
| `walls` | `3:8 4:58 5:18 6:16` | the neutral cities' walls and their shares, percent; capitals get the highest |
| `sea_cities`, `sea_city_count` | off, 0.04 of the cities | cities on islands only ships reach; the units template needs a ship |
| `fly_cities`, `fly_city_count` | off, one for every 40 cities (1 to 3) | cities in mountain rings only flyers reach; the template needs a flyer |
| `ports`, `ports_per_side` | `auto`, 1 | cities by water that reaches the sea build the template's ship |
| `fairness` | `normal` | `off`, `loose`, `normal`, `strict` (below) |
| `attempts` | 30 | maps made before the fairest is given |

Of size, cities and density, two given say the third (`size` and `cities`:
the density; `size` alone: the cities at the density; neither: the size for
the cities). It warns of a density outside 100 to 300, or fewer than 3
cities a side. The knobs of the stages to come (cities, sites, roads,
fairness, ships and flyers, retries) are read and written already, and do
nothing yet: `temples`, `ruins`, `site_city_gap`, `site_gap`, `roads`,
`road_loops`, `bridges`, `max_bridge`, `passes`, `symmetry`, `accept`. A
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
   Its best inland hexes are the mountains, **thinned**: while a mountain
   lies deeper than 3 hexes into its range, the mountains at the range's
   edge near it go back to what they were, the weakest first and from both
   sides, each only if that breaks no range and walls nothing in; so a
   thick mass becomes a range along its spine, as long as it was. As many
   are laid before thinning as leave the mix's share after it, and the
   count is settled a hex at a time at the end. (The owner, on the first
   maps with cities: thick masses of mountain looked odd and wasted land.
   The target map's 17% of mountain is in thinner ranges than our ridge
   score gives; with ours thinned, 13% spreads about as far as 17 did
   thick, so `classic` has 13.) A ring of hills is laid along
   them by the same score (so what is left out is left out in stretches)
   until `hill_ring` of their edges border hills, thickened outward, and the
   rest of the hills grow as free patches of a noise of their own. Land
   walled in by mountains is of no use: a hollow of a few hexes fills with
   mountain, a bigger pocket gets a way out where its wall is lowest (cut
   into hills). (The owner's eye, on the judged maps of
   `tests/mapgen/judged`: such pockets, and ranges in straight parallel
   stripes, were what the worst maps had.)
3. **Rivers and lakes** (`rivers.rs`): the land is flooded from the water
   up (a priority flood over all but the mountains, on the elevation with
   a wandering noise, higher by the map's edges and at the foot of a
   range), which gives every hex the neighbour it drains to. Rivers start
   by the mountains and, by turns, at the map's land edge (a river from
   beyond it, which cuts the land in two), far from the sea and from each
   other; each follows the drainage to the sea or another river (joining
   one it comes alongside), so it never crosses a range, and widens toward
   its mouth (`river_width`). It is kept as its list of hexes, for the
   bridges. Then the lakes, mostly small, in the deepest hollows. The sea
   is made smaller by what they took and the land made again (at most three
   times), and its shore makes the water share exact.
4. **Forest, swamp, desert** (`biomes.rs`): a moisture from a slow noise and
   the water near. Swamps grow from one to three seeds where the land is low
   and wet by the water, round about their seed; forest is the wettest
   grass; desert the driest, far from water and swamps.
5. **Cleanup** (`cleanup.rs`): the ring of hills put back where rivers took
   it, two passes of a majority filter (a hex with
   five or six neighbours of one other terrain becomes it; water and
   mountains stay; some lone trees too), then each terrain back on its
   quota. Last, **land walled off on foot** (by mountains and the sea or a
   lake; a river is bridged later) gets a way in, biggest piece first: the
   range cut where its ridge is lowest, the hexes made hills (a saddle), or,
   where only water walls it, a neck of grass across at most two hexes of
   it. Land farther out than that is an island; pieces under 12 hexes are
   left. No city goes where no army can, and the owner, looking at the
   first maps with cities, found such land "wasted real estate". The final
   pass over the mix leaves these ways alone.

6. **Cities** (`cities.rs`). Travel is the reference unit's on foot (grass
   2, forest 4, hill and swamp 5, desert 3), a river crossed as if bridged.
   - **Capitals**: of the grass well in from the edges, off the water, open
     around and with land enough within 3 turns, a spread sample; from a
     few starts (the farthest points by travel), moves that make the set
     better: each capital to the best place in its own land, then swaps.
     Better is each capital in the middle of its own land (the least
     travel from the land to its capital, on the whole), within what
     fairness allows for the land and the way to the nearest enemy (beyond
     it weighs most), and **none hemmed in**: seen from each capital, the
     others lie within a half turn, so each has a side of the map to itself.
     (The plan had them as far apart as can be; that pushed them into
     corners with unequal land, and the fairness put right after it bunched
     them in the middle. The owner did not like a capital in the middle of
     the others, nor the four bunched together.)
   - **Neutral cities**: a Poisson disc from the capitals over all the land
     (grass liked best, the water's edge more while the cities by it are
     fewer than `coastal_cities`), thinned where they crowd most, one at a
     time, to the count (or more in the emptiest places if it made too
     few); then evened out (while it brings the land nearer its cities on
     the whole, the most crowded city moves to the emptiest place); three
     passes moving each a hex toward the middle of its cell. After the
     fairness pass, the holes it leaves are filled the same way, as far as
     fairness allows. Nineteen hexes of land in twenty are within 9 or 10 of
     a city on a medium map (the target map's cities leave far more land
     empty; the owner wants none wasted). The cities are farther apart
     than the target map's, so: a median of about 10 to the nearest, not 8.
   - **At sea and in the air** (the checkboxes): cities on the islands
     raised for them, the first of each by the water; and neutral cities
     three turns from every capital, in or by a range, ringed by mountains
     two hexes out (the mountains taken from ranges' edges elsewhere).
   - **Fairness**: while the neutral cities are shared out unfairly (the
     table below), the side with the most gives up the city it holds most
     narrowly and the side with the fewest gets one in its emptiest place;
     likewise within 3 turns of the capitals, and for ports. What only the
     capitals can change is left to six tries of capitals, and what the
     terrain will not allow to a new map: the seed and the try scrambled
     (`mapgen.txt` says the seed that made it), at most `attempts` times.
   - **Room**: a mountain beside a city becomes a hill, a swamp grass, and
     three of its neighbours at least are grass; the mix is put back round
     them.
   - **Records**: a name from our own syllables (`assets/mapgen/names.txt`;
     a capital's with a grand ending), its holder (a capital's side, in the
     order black, white, red, green, blue, orange, yellow, purple), walls
     (a capital the highest of `walls`; a neutral drawn from it, one more by
     the water or in the hills), taxes (a capital 20, a neutral 4 and twice
     its walls), what it builds (from the template by role: a capital its
     two cheapest, its fastest and its commander; a city in the hills its
     two strongest; the others the cheapest and one more; a port the
     template's ship; a city in the air a flyer first, now and then a strong
     one too), and its garrison (a capital its commander and three of what
     it builds, a neutral half its walls of the cheapest).
   - **Worth**, on the map as it ends: a neutral city hard to come at is
     worth the taking (the owner's wish). Its approach is the travel into
     it from the land 4 hexes round, over the same across open grass (land
     across the sea or a lake left out; land no walker reaches it from
     counted as 6 turns). At 1.9 or more (hills, forest, a river, a range
     to go round; about one city in ten) it is hard: its units start
     veterans, it pays 4 more a turn, it builds the template's strongest
     unit on foot, and one in four builds a flyer too. At 2.8 or more, and
     every city at sea or in the air, it is remote: elites, 8 more, a flyer
     one in two.

Fairness, by level (`fairness`); travel by the cheapest way, a river
crossed:

| Measure | loose | normal | strict |
|---|---|---|---|
| Neutral cities in each capital's cell, most less fewest | 3 | 2 | 1 |
| Neutral cities within 3 turns of each capital, most less fewest | 2 | 1 | 1 |
| Land in each cell, most over least | 1.5 | 1.3 | 1.15 |
| The way to the nearest enemy capital, longest over shortest | 1.5 | 1.3 | 1.15 |
| Ports in each cell (that has water a ship reaches the sea on), most less fewest; and none short of `ports_per_side` | 3 | 2 | 1 |
| Cities at sea nearest one capital | 2 | 1 | 1 |

Every stage uses only `+ - * /`, `floor` and `sqrt` where results pick
terrain (no `sin`, `exp` or `powf`, whose last bits differ between
platforms), breaks ties by the hex's index, and draws its chance from its own
stream (`rng.rs`, SplitMix64).

## How well it does

Over 20 seeds each of medium and huge maps (`tests/mapgen_terrain.rs`):
water and every land share within a point of the config on every map; the
mountains' edges on hills within 0.08 of `hill_ring` on every map; and how
each terrain keeps to itself, on average, within 0.08 of the target map's
(grass 0.84, forest 0.79, hill 0.60, mountain 0.80, swamp 0.82, water
0.87); no land walled in by mountains. Over 10 medium seeds, every river runs
from its source to water, one hex beside the next, all of it water at the
end; a good part of the water is narrow. Tiny maps come within twice the
tolerance, on the whole. Over 20 medium seeds the cities are as many as
asked, a capital a side, apart and off the edge, named each its own, and
shared out fairly at `normal` (a few take a second or third map); no land
of size is walled off from the main land but islands, 95% of it lies
within half again the aimed spacing of a city, and no capital is hemmed in.

The tests make many maps; they are built a little optimized (`[profile.test]
opt-level = 1` in `Cargo.toml`), or they would take minutes.

`tests/mapgen_target.rs` holds the ruler to the target map's numbers. It
reads the map from `MAPGEN_TARGET_DIR` and is ignored unless asked for:

```sh
MAPGEN_TARGET_DIR=<the map's directory> cargo test --test mapgen_target -- --ignored
```
