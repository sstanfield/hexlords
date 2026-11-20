# The map format

A map is plain text. It comes in two sizes:

- **A file**, `name.map`: the terrain and the roads. Enough to draw a map, to
  look at and to edit; with no sides on it, there is nothing to play.
- **A directory**, `name/`: the same terrain file, plus the kinds of unit the
  map has and its cities, and pictures of its own if it wants them. A whole
  map, which the game starts on with its cities and their garrisons.

```
name/
  terrain.map    the terrain and the roads              (must be there)
  units.txt      the kinds of unit                      (optional)
  cities.txt     the cities                             (optional)
  assets/        pictures instead of the game's         (optional)
```

Maps live in `assets/maps/`. `assets/maps/example.map` is the file at the
bottom of this page, and `assets/maps/continent/` is a whole map to copy from.
This page is everything needed to write either by hand or with a script.

```sh
cargo run -- --map assets/maps/continent                # play it
cargo run -p mapeditor -- assets/maps/continent         # paint its terrain and roads
cargo run -p mapeditor -- --check assets/maps/continent # what is in it, and what is wrong with it
```

## The terrain file

```
<rows>
<columns>
<row 0: one letter per hex, column 0 first>
<row 1>
...
<the last row>
```

- **Line 1:** the number of rows. **Line 2:** the number of columns. Each is a
  whole number from **20 to 600**; they need not be equal.
- **Then one line per row**, the top row of the map first. A line has exactly
  as many letters as the map has columns, the leftmost hex first, with nothing
  between the letters.
- Lines end in `\n` (`\r\n` is read too). End the last line with one as well.

### The letters

| Letter | Terrain | For a unit on foot |
|---|---|---|
| `g` | grass | open ground: cheap to cross |
| `f` | forest | slow |
| `h` | hill | slow |
| `s` | swamp | slowest |
| `d` | desert | a little slower than grass |
| `m` | mountain | cannot be entered |
| `w` | water | cannot be entered |
| `c` | city | a place; quick to enter |
| `t` | temple | a place; crossed like grass |
| `r` | ruin | a place; crossed like grass |

What a terrain costs is not in this file: every kind of unit has its own
prices (`units.txt`, below). The third column is how the example map's units
have them, and the idea will stay: grass is open, forest, hill and swamp slow
an army down, mountains and water are walls for anything that walks.

### Roads: capital letters

**A capital letter is that terrain with a road on it.** `G` is grass with a
road, `F` a road through a forest, and

- `W`, a road over water, is a **bridge**;
- `M`, a road over a mountain, is a **pass**.

A road joins the hexes around it that have a road too: it is drawn from the
middle of each such side, through the hex, and a unit that steps *along* it,
from one road hex to the next, pays its road price (its bridge price over
water) instead of the terrain's. So:

- A bridge and a pass can be entered only along the road, from the road hex
  before them. From the bank beside a bridge, the water is water.
- Every two road hexes that touch are joined. Two roads that run side by side
  are one wide tangle; keep a hex between roads that are not meant to meet.
- **A city needs no capital.** Roads run up to a city's gates by themselves: a
  road hex beside a `c` joins it. (`C` is read and means the same.)
- A temple or a ruin is joined only when its letter is a capital (`T`, `R`):
  then the roads beside it run up to it. No road is drawn over the picture of
  a place.
- A road that joins nothing is drawn as a small round place.

### What the reader forgives, and what it refuses

- A row with **too few** letters is filled up with grass, and rows that are
  **missing** at the end are all grass. This is almost always a mistake in a
  hand-written file: check for it (below).
- A row with **too many** letters, a letter that is no terrain, a size that is
  not a number or is outside 20 to 600: an error that names the line.
- Spaces at the end of a line are ignored. Spaces between letters are an error.
- Lines after the last row are ignored.

## The kinds of unit: `units.txt`

A list of records. A record starts with `unit <id>`, and the lines after it,
up to the next record, are `key value`. Empty lines and lines that start with
`#` are skipped; how a line is indented does not matter.

```
# Fast on open ground and on roads, slow in rough country, no good in a swamp.
unit riders
  name     Riders
  image    units/riders.png
  strength 4
  moves    18
  upkeep   3
  price    10
  enter    grass 2, forest 5, hill 5, desert 3
  enter    city 1, road 1, bridge 1
```

| Key | Value | If left out |
|---|---|---|
| `unit` | The kind's id: lower-case letters, digits and `_`. `cities.txt` names it by this | (starts the record) |
| `name` | What the player reads | the id |
| `image` | The kind's picture, a path in an asset directory (below) | `units/default_unit.png` |
| `strength` | How hard it fights (`docs/gameplay.md`, "Fighting") | 1 |
| `moves` | Its movement points each turn, at least 1 | an error: it must be given |
| `upkeep` | What it costs each turn | 0 |
| `price` | What it costs a city to build one, unless the city's `produce` line says otherwise | 0 |
| `enter` | What a step costs it: `<what> <points>`, with commas between; may be given on several lines | an error: at least one |
| `commander` | The kind is a commander: every other unit of its stack fights this much stronger, and it fights last. A hex holds one commander besides its 8 units. At least 1 | not a commander |

What `enter` can name: `grass`, `forest`, `swamp`, `desert`, `hill`,
`mountain`, `water`, `city`, and `road` and `bridge`, the price of a step along
a road over land and over water. **What is left out cannot be entered**: a
unit without `water` does not swim, one without `bridge` cannot cross a
bridge, one without `road` walks on a road as on the ground under it. Temples
and ruins cost what grass costs. A road is never dearer than the ground under
it.

The game has no kinds of unit of its own: a map brings them. (A map with none,
or a single terrain file, has no units, and nobody to play it.)

## The cities: `cities.txt`

The same kind of list. A record starts with `city <row> <column>`, and **that
hex must be a city in the terrain file** (the letter `c`).

```
city 13 7
  name     Brindlewick
  side     red
  walls    2
  income   20
  produce  militia 1
  produce  riders 3 12
  garrison militia 2
  garrison riders
```

| Key | Value | If left out |
|---|---|---|
| `city` | Its row and its column, counted from 0 | (starts the record) |
| `name` | The city's name; no two cities share one | an error: it must be given |
| `side` | Who holds it at the start: `black`, `white`, `red`, `green`, `blue`, `orange`, `yellow`, `purple`, or `neutral` | `neutral` |
| `walls` | How strong its walls are | 0 |
| `income` | The gold it brings its holder at the start of each of the holder's turns (its taxes) | 10 |
| `trains` | The level the units it builds start at: `regular`, `veteran` or `elite` (`docs/gameplay.md`, levels) | `regular` |
| `produce` | A kind of unit it can build, the turns one takes, and what one costs this city: `produce <unit> <turns> [<cost>]`; without a cost, the unit's `price`. The cost is paid from its holder's treasury when the unit is done; until it can be, the unit waits. A line for each. The game starts with the city building none of them: its holder chooses | it can build nothing |
| `garrison` | Units that stand in it at the start, on its side: `garrison <unit> [<how many>]`. A line for each kind; at most 8 units in a city, and a commander besides | nobody |

A city tile without a record is still a city, with no name: `--check` warns of
it. A record on a hex that is not a city, a unit that `units.txt` does not
have, two cities on a hex: errors that name the file and the line.

## Pictures of its own: `assets/`

A picture is looked for in the map's `assets/` directory first, then in the
game's. So a map brings a unit's picture by having the file its `image` names
(`assets/units/dragon.png` for `image units/dragon.png`), and it can replace
any of the game's the same way: a terrain's tile (`assets/tiles/grass_tile.png`),
a side's city (`assets/tiles/city_red_tile.png`), a road
(`assets/roads/road_09.png`). `docs/tiles.md` says what each picture
has to look like.

## How the grid lies on the screen

The hexes are flat-topped (a flat edge at the top and the bottom, a point to
the left and the right) and stand in columns. **The odd columns (1, 3, 5 ...)
sit half a hex higher than the even ones.** So a "row" of the file is a zigzag
on the screen, and a coastline that is a straight line of letters in the file
is a toothed edge in the game, while one that looks toothed in the file
(`gwgwgw`) can be the straight one.

The six neighbours of the hex at row `r`, column `c`:

| | Even column | Odd column |
|---|---|---|
| Above and below | `(r-1, c)`, `(r+1, c)` | `(r-1, c)`, `(r+1, c)` |
| To the left | `(r, c-1)`, `(r+1, c-1)` | `(r-1, c-1)`, `(r, c-1)` |
| To the right | `(r, c+1)`, `(r+1, c+1)` | `(r-1, c+1)`, `(r, c+1)` |

A unit moves from a hex to one of these six, and a road goes on to those of
them that have a road. A letter in the file is therefore connected to the
letters above and below it, to its left and right, and to two of its four
diagonal letters, which two depending on its column. **Lay a road hex by hex
along this table**, not by drawing a line of capitals: two capitals that are
diagonal in the file may not touch on the map.

To draw shapes, work with where the hexes are rather than with rows and
columns. The center of hex `(r, c)`, with x to the right and y **upward**:

```
x = 1.5 * c
y = (1 if c is odd, else 0) - 2 * r
```

A hex is 2 wide and 2 high in these units, and the game draws y at 0.863 of x.
A circle in these coordinates is a circle on the map. In letters, a row is
about 1.15 times as high as a column is wide: a map of 100 columns looks
square with 87 rows.

## Sizes

Any size from 20 x 20 to 600 x 600, wide or tall. For a feel of them: at the
closest zoom the game's window shows about 6 rows of 10 hexes, at the farthest
about 39 rows of 68. 30 x 40 is a skirmish, 60 x 90 a full game, and anything
over 200 x 200 is mostly for testing that it still runs.

## Making and checking a map

Writing 60 lines of 90 letters by hand goes wrong. Have a script compute the
terrain from the hex centers above, find the roads' ways with the neighbour
table, and print the file; write `units.txt` and `cities.txt` by hand. Then:

```sh
cargo run -p mapeditor -- --check assets/maps/mine
```

prints the size, how many hexes of each terrain and how many roads, bridges
and passes there are, every kind of unit and every city, and a warning for
every short or missing row, every city tile without a record and every
picture that is nowhere. A mistake is an error with its file and line. To look
at the map and touch it up:

```sh
cargo run -p mapeditor -- assets/maps/mine      # the editor: terrain and roads
cargo run -- --map assets/maps/mine             # the game
```

Both take `--screenshot shot.png`, which draws the window into a file and
quits (add `--script "1,1,1"` to zoom all the way out first). The editor's
minimap shows the whole map at once. The editor paints terrain and lays roads;
the units and the cities are text, to be edited as text, and when the editor
saves a map directory it says what `cities.txt` has to say about the new
terrain.

Every map in `assets/maps/` is read by `cargo test`, which fails if one has an
error or a warning.

### What makes a map good

- Land a walking unit cannot leave (an island, a valley closed by mountains)
  is fine in small doses; the whole map being such pockets is not.
- Mix the terrains in patches of a dozen hexes and more, not hex by hex: a
  forest, a range of hills with mountains in it, a swamp along a lake.
- A handful of cities, well apart, on grass, each reachable from the others on
  foot, and joined by roads: a road is what makes a far city near. A bridge
  or a pass is a place worth holding.
- Every side that plays starts with a city, a garrison in it, and something
  it can build. Leave some cities to nobody.
- Use every terrain at least once, so that the map is also a test of the
  drawing.
- Names are yours to make up. Do not take cities, units or their numbers from
  an existing game.

## An example

20 rows, 30 columns: an island with a forest, hills around a mountain ridge, a
desert, a swamp by a lake, three cities, a temple and a ruin. Roads (the
capitals) join the three cities, with a bridge over the lake's southern tip
(the `W` in row 12). Note the toothed bottom row of land (`gwgwgw`), which is
a smooth coast on the screen.

```
20
30
wwwwwwwwwwwwwwwwwwwwwwwwwwwwww
wwwwwwwwwwwwwwwwwwwwwwwwwwwwww
wwwwwwwwwwwwwwwwwwwwwwwwwwwwww
wwwwwwwwwwfgggggggdddwwwwwwwww
wwwwwwwwfffffgggggdddddwwwwwww
wwwwwwfffffffggggghddddggwwwww
wwwwggfffffffggghhhhhdcggggwww
wwwwggfffffffgghhhmhhhgGGggwww
wwwggggfffffgghhmmmmmhhgGgggww
wwwggggggggggggthmmmhhggGgggww
wwwgggggcgggwwwhhhhhhhggGgggww
wwwggggggGGgwwwgghhhfffgGgggww
wwwgggggggsGGWGggggfffffGgggww
wwwwggrgsssssggGgggfffffGggwww
wwwwwgggsssssggGGgGfffffGgwwww
wwwwwwwgsssssggggcgGGfGGwwwwww
wwwwwwwwwsssgggggggggGwwwwwwww
wwwwwwwwwwwgwgwgwgwgwwwwwwwwww
wwwwwwwwwwwwwwwwwwwwwwwwwwwwww
wwwwwwwwwwwwwwwwwwwwwwwwwwwwww
```
