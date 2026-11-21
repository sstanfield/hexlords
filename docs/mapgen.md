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
cleanup), the cities (capitals, neutrals, sea and air cities, fairness,
names, walls, what they build and who stands in them), the temples and
ruins, the roads (bridges, passes, every city and site meant to be
reached on foot made so), and the hard checks, retries and scores (`accept
best N`), with maps of islands; and on the new game screen, "A random
map" (the plan's G8): its preset, size, sides, cities, temples for each
side, cities at sea and in the air (greyed out when the units template has
no ship or no flyer), the seed, then Make the map, shown before the game
starts. Not built: `symmetry` (below).

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
  corner) and `sheet.txt` (each seed's score and what it misses); last, how
  many maps failed the hard checks, and the tries and the time a map.
- `stats.txt`: what `stats` says; the targets (each soft measure, its
  target or range, its value, a mark, and the score); the fairness measures;
  side by side, a line for each side (its capital, the neutral cities in its
  cell and within 3 turns, its land, the way to the nearest enemy and to
  the nearest temple in turns, the cities in its cell that build ships, its
  temples and ruins); and what the hard checks failed. The preview draws a
  thin line round each capital's cell.
- A map no try made pass the hard checks is written all the same, to look
  at; `gen` and `batch` then say what it fails (`FAILS: ...`) and exit with
  1.
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
| `temples_per_side` | 1 | temples for each side, 0 to 4 (the new game screen offers 0, 0.5, 1, 2, 3, 4): none; one shared by two neighbours; one each, ... (the owner, 2026-11-20: "2 per side is probably a lot") |
| `temples`, `ruins` | none, and 0.24 a city | a count, or a rate of the cities; a rate, with fairness on, is rounded to as many for each side (one at least). `temples`, if given, is used instead of `temples_per_side` |
| `site_city_gap` | 3 | no site nearer a city |
| `site_gap` | 5 | no site nearer another |
| `roads` | `linked` | `none`; `sparse` (about three cities in five on a road, in small networks); `linked` (every city on foot on one network); `dense` (linked, and more loops) |
| `road_loops` | 0.15 | the share of the links left out of the tree that are laid too (`linked`; `dense` two and a half times as many) |
| `bridges`, `max_bridge` | on, 5 | roads cross rivers on bridges, at most so long |
| `passes` | 0.1 a city | the most ranges roads cross over passes |

Of size, cities and density, two given say the third (`size` and `cities`:
the density; `size` alone: the cities at the density; neither: the size for
the cities). It warns of a density outside 100 to 300, or fewer than 3
cities a side. The knobs of the stages to come (cities, sites, roads,
fairness, ships and flyers, retries) are read and written already, and do
nothing yet: `symmetry` (a map built on a half or a quarter and copied:
every stage would have to keep to it; the fairness measures already hold
the sides to a fair share, and the owner wants random maps fun to play,
bespoke ones from the editor). `accept` is `first` (the first map that
passes the hard checks) or `best N` (the lowest score of N that pass). A
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
     raised for them (in the main sea, 5 hexes from land if there is such
     water, else 4 or 3), by the water and off the edge; and neutral cities
     three turns from every capital (on a small map where none is, the
     farthest), in or by a range, ringed by mountains two hexes out (the
     mountains taken from ranges' edges elsewhere, never another air city's
     ring).
   - **To the water**: while fewer cities stand by the water than
     `coastal_cities`, one a hex or two from it steps to its edge (an even
     spread keeps most off the shores).
   - **Maps of islands** (`layout archipelago`, the `islands` preset):
     travel goes by sea too, on the main sea (the biggest water a ship
     sails on), a hex of sea 1, putting to sea a turn: the cells, the
     fairness and the cities' places reach every island. A capital to each
     big island (land between the water), when there are as many as sides;
     neutral cities on every island, those on another island than the
     capitals' reached by sea, by the water (each its island's port) and
     building the template's ship; roads on each island among its own
     cities; temples and ruins on the capitals' islands. Each home island
     with a coast on the main sea needs a port on it (fairness).
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
     the water or in the hills; 1 at least), taxes (a capital 40, a neutral 8 and four times
     its walls), what it builds (from the template by role, among its kinds
     on foot that are neither siege engines nor at home in the woods or the
     high country, unless it has no others: a capital its two cheapest, its
     fastest, its strongest, the template's siege engine and its commander;
     a city in the hills its two strongest; the others the cheapest and one
     more; besides, a city with two forests or swamps around it the kind at
     home there, and one with two hills or mountains around it the kind at
     home in those; a port the template's ship; a city in the air a flyer
     first, now and then a strong one too), in how many turns (a turn for
     each 6 of its price, 1 to 4; a commander 4: `build_turns`), and its
     garrison (a capital its commander and three of what it builds; a
     neutral one unit, the weakest on foot it builds, no siege engine).
   - **Worth**, on the map as it ends: a neutral city hard to come at is
     worth the taking (the owner's wish). Its approach is the travel into
     it from the land 4 hexes round, over the same across open grass (land
     across the sea or a lake left out; land no walker reaches it from
     counted as 6 turns). At 1.9 or more (hills, forest, a river, a range
     to go round; about one city in ten) it is hard: its units start
     veterans, it pays 8 more a turn, it builds the template's strongest
     unit on foot, and one in four builds a flyer too. At 2.8 or more, and
     every city at sea or in the air, it is remote: elites, 16 more, a flyer
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
| Capitals hemmed in (degrees short of a half turn; up to 4 sides: with more, some stand amid the others on any map) | 0 | 0 | 0 |
| Home islands with no port on the main sea (a map of islands, or with cities at sea) | 0 | 0 | 0 |
| Temples in each cell, most less fewest | 2 | 1 | 0 |
| Ruins in each cell, most less fewest | 2 | 1 | 0 |
| The way from each capital to its nearest temple, longest less shortest, in turns | 3 | 2 | 1 |

7. **Temples and ruins** (`sites.rs`). A temple blesses the units that
   seek it, so the sites are shared out evenly, each kind on its own
   (the owner: "make sure they are distributed fairly (both) since temples
   are a definite boon"). Each capital's cell gets as many of each (what
   does not share out evenly goes to cells by lot); a site stands well
   inside its cell (most of a turn nearer its own capital than any
   other's, so the last touches to the terrain do not move it to another),
   on land a walker gets to from the capitals, off the map's edge, not on
   an island nor in an air city's ring, `site_city_gap` from every city
   and `site_gap` from every other site. Each side's first temple stands 2
   to 3.5 turns from its capital (as the continent map has a temple near
   each side's first city); the rest go where the cities and sites are
   farthest off, out to twice `site_gap`, a little more often in wilder
   country (forest, hills, the water's edge) than on the open grass, and
   by chance. Fewer temples than sides (half one each) are shared: where
   the capital worst served is nearest one, the capitals' ways to their
   nearest temples most alike, and all of them shortest (between the
   cells, often where they meet); "temples in a cell" is then no measure.
   Their fairness is judged on the map as it ends.

8. **Roads, bridges, passes** (`roads.rs`).
   - **Links**: each city on foot to its 3 nearest by travel (a capital to
     4) and its 2 nearest as the crow flies (a city across a range is far
     by travel, near by a pass). Each link's road is found by a search that
     costs grass 1, desert 1.5, forest 2, hill 3, swamp 4 a hex (a road
     already laid 0.5, so roads merge), a bridge 6 a hex over a river only
     (never the sea or a lake, never longer than `max_bridge`), a pass 5 a
     hex, and 4 more a hex beside another road or a city (but the hex it
     comes from and the city it goes to: in the game every two road hexes
     that touch are joined, so a new road joins an old one or keeps a hex
     from it, and two roads do not leave a city by neighbouring sides); it
     never sets foot on a sea city's island or in an air city's ring; of
     ways that cost the same it takes the one nearest the straight line
     between its ends. A link through a third city is left out (the
     two links through it do).
   - **Network**: the cheapest tree joining the cities, round the ranges
     where it can be; then `linked`, the cheapest `road_loops` share of the
     other links; `dense`, more; `sparse`, the tree thinned, its longest
     links first, until about three cities in five are on a road. Then
     passes, where a road over one saves a turn's march at least, the most
     saving first, no more ranges crossed than `passes` allows (held to
     while drawing, too).
   - **Passes**: a road may cross a range up to 5 hexes wide; all but its
     middle one or two mountains become hills, a saddle, so a pass is 1 or
     2 hexes long. (The plan priced a pass at 15 grass hexes; on our
     thinner ranges, with gaps cut in them, no road ever took one, so a
     pass costs 5 and is laid only where it saves.)
   - **Tidy**: a road hex on land whose road neighbours stay joined
     without it goes (a corner cut, two roads side by side made one), and
     so does a stub; never the land end of a bridge or a pass. At a city's
     gate, two roads by neighbouring sides (a triangle, drawn so because
     touching road hexes join) become one, or one moves a side round,
     where the roads beyond allow: about one city in fifty keeps one,
     where water or mountains crowd a city with three roads.
   - **Reach repair**: whatever is meant to be reached on foot and is not
     (a city, a temple, a ruin), as the game walks (onto a bridge or a pass
     only along the road), gets a road to one of the nearest cities reached,
     round the ranges if it can be; with no road for it (or `roads none`,
     or `bridges off`), a ford: the fewest river hexes between it and land
     reached, made grass.

Every stage uses only `+ - * /`, `floor` and `sqrt` where results pick
terrain (no `sin`, `exp` or `powf`, whose last bits differ between
platforms), breaks ties by the hex's index, and draws its chance from its own
stream (`rng.rs`, SplitMix64).

## Hard checks, retries, the score

A map must pass the hard checks (`validate.rs`), or it is made again from
another seed (the seed and the try scrambled), at most `attempts` times:

- the game reads it, and `mapgen check` finds no problem (strict: every
  bridge and pass on a road from land to land);
- every city is reached as planned (on foot, by sea, through the air), and
  every temple and ruin off the islands on foot;
- as many cities, capitals, temples and ruins as asked; a city at sea, and
  one in the air, if their checkboxes are on;
- the cities `city_min_gap` apart and `edge_margin` off the edge;
- fair at the level asked (the table above).

`accept best N` makes N maps that pass and keeps the lowest soft score: the
sum over the measures of the miss over the tolerance, squared, at most 4
each. The measures (`score.rs`): the water share (1 point), each land
terrain's share (1 point), how each keeps to itself (the target map's, 0.08),
land walled in by mountains (none; an air city's ring is meant), mountain
edges on hills (`hill_ring`, 0.08); the nearest city's median (0.9 x the
square root of the density, an even spread over the land; 1.5), cities by
the water (`coastal_cities`, 0.08), cities on foot on a road (`linked` and
`dense` all; `sparse` 55 to 70%), road hexes for each city (`linked` 9 to
15, `sparse` 4 to 11, `dense` 11 to 19), road hexes in clean lines (80% and
more), bridges for each city (one for 5 to 8), ports among the cities by
the water (50 to 90%).

## How well it does

Every size and number of sides, 10 seeds each, in release on the owner's
machine (`mapgen batch`; the full 50-seed run is
`cargo test --release --test mapgen_validate -- --ignored`): maps that fail
the hard checks after `attempts` tries; tries and seconds a map.

| Size, sides | Checkboxes off | Both on | `islands` |
|---|---|---|---|
| tiny, 2 | 0; 1.0, 0.02 s | 0; 1.3, 0.03 s | 0; 1.4, 0.02 s |
| tiny, 4 | 0; 1.6, 0.06 s | 0; 4.7, 0.20 s | 10 (5 cities for 4 sides) |
| tiny, 8 | 3; 8.8, 0.8 s | 10 | none (too few cities for 8 capitals) |
| medium, 2 | 0; 1.1, 0.24 s | 0; 1.0, 0.28 s | 0; 1.1, 0.22 s |
| medium, 4 | 0; 1.6, 0.6 s | 0; 1.5, 0.4 s | 0; 2.1, 0.5 s |
| medium, 8 | 0; 3.8, 3.0 s | 0; 4.0, 2.6 s | 0; 9.0, 3.4 s |
| huge, 2 | 0; 1.0, 0.8 s | 0; 1.0, 0.8 s | 0; 1.2, 0.7 s |
| huge, 4 | 0; 1.2, 1.1 s | 0; 1.1, 1.0 s | 0; 1.8, 1.6 s |
| huge, 8 | 0; 4.5, 5.9 s | 0; 8.8, 12.6 s | 0; 10.4, 12.0 s |

A tiny map has a city for some 170 land hexes: with 8 sides (or 4 on the
islands' 45% of water) that is fewer than 3 cities a side, and the
generator warns of it; its land cannot always be shared out fairly, nor a
city found for the sea. With 8 sides, the land in eight cells within 1.3 of
each other takes a few terrains: a try gives a terrain up as soon as two
sets of capitals show it will not share out, but a huge map still takes
some seconds.


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
Over 20 medium seeds the sites are as many as asked, apart, and reached;
over 10 medium and 10 huge, every side has as many temples and as many
ruins, and its nearest temple within 2 turns of the others'. Round the
sites on a medium map: grass 30%, forest 25%, hill 24%, water 17% (the
target map: 35, 22, 15, 13, and swamp 10). Over 20 medium seeds `linked`
puts every city on one network, every bridge and pass runs from land road
to land road (`mapgen check`, strict), no bridge is longer than
`max_bridge`, no pass than 2, the passes are no more than `passes`, and
every city, temple and ruin is reached on foot; `sparse` puts 55 to 70% of
the cities on a road over 10; with `roads none`, fords alone reach them.

The tests make many maps; they are built a little optimized (`[profile.test]
opt-level = 1` in `Cargo.toml`), or they would take minutes.

`tests/mapgen_target.rs` holds the ruler to the target map's numbers. It
reads the map from `MAPGEN_TARGET_DIR` and is ignored unless asked for:

```sh
MAPGEN_TARGET_DIR=<the map's directory> cargo test --test mapgen_target -- --ignored
```
