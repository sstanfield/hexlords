# The map file format

A map file is plain text: the size of the map, then the terrain, one letter per
hex. This page is everything needed to write one by hand or with a script.
Maps live in `assets/maps/` and end in `.map`; `assets/maps/example.map` is the
map at the bottom of this page.

## The file

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

| Letter | Terrain | In the demo |
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

Lower case only. Any other character is an error.

The third column is what the one unit of today's demo does with each terrain
(its costs are 2 for grass, 3 desert, 4 forest and hill, 5 swamp, 1 city, out
of 20 movement points a turn). Those numbers belong to that unit, not to the
file, and will change; what will stay is the idea: grass is open, forest, hill
and swamp slow an army down, mountains and water are walls for anything that
walks.

### What is not in the file

Only terrain. A map file has no units, no owners, no starting positions, no
roads and no names: a `c` is a city *tile*, and nothing more is known about it
yet. Those will come with the game's rules, **after the last row**: lines there
are ignored today and reserved for them, so a file written now stays valid.

### What the reader forgives, and what it refuses

- A row with **too few** letters is filled up with grass, and rows that are
  **missing** at the end are all grass. This is almost always a mistake in a
  hand-written file: check for it (below).
- A row with **too many** letters, a letter that is no terrain, a size that is
  not a number or is outside 20 to 600: an error that names the line.
- Spaces at the end of a line are ignored. Spaces between letters are an error.

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

A unit moves from a hex to one of these six. A hex of letters in the file is
therefore connected to the letters above and below it, to its left and right,
and to two of its four diagonal letters, which two depending on its column.

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

Any size from 20 x 20 to 600 x 600. For a feel of them: the demo's own map is
100 x 100; at the closest zoom the game's window shows about 6 rows of 10
hexes, at the farthest about 39 rows of 68. 30 x 40 is a skirmish, 60 x 90 a
full game, and anything over 200 x 200 is mostly for testing that it still
runs.

## Making and checking a map

Writing 60 lines of 90 letters by hand goes wrong. Have a script compute the
terrain from the hex centers above and print the file, then check it:

```sh
cargo run -p mapeditor -- --check assets/maps/mine.map
```

prints the size, how many hexes of each terrain there are, and a warning for
every short or missing row. To look at it and touch it up:

```sh
cargo run -p mapeditor -- assets/maps/mine.map      # the editor
cargo run -- --map assets/maps/mine.map             # the game: the demo's unit, on your map
```

Both take `--screenshot shot.png`, which draws the window into a file and
quits (add `--script "1,1,1"` to zoom all the way out first). The editor's
minimap shows the whole map at once.

Every map in `assets/maps/` is read by `cargo test`, which fails if one has an
error or a warning.

### What makes a map good for the demo

- The unit starts on the land nearest the middle of the map, so the middle
  should be on the largest piece of land.
- Land a walking unit cannot leave (an island, a valley closed by mountains)
  is fine in small doses; the whole map being such pockets is not.
- Mix the terrains in patches of a dozen hexes and more, not hex by hex: a
  forest, a range of hills with mountains in it, a swamp along a lake.
- A handful of cities, well apart, on grass, each reachable from the others on
  foot (or the map is about ships, which do not exist yet).
- Use every terrain at least once, so that the map is also a test of the
  drawing.

## An example

20 rows, 30 columns: an island with a forest, hills around a mountain ridge, a
desert, a swamp by a lake, three cities, a temple and a ruin. Note the toothed
bottom row of land (`gwgwgw`), which is a smooth coast on the screen.

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
wwwwggfffffffgghhhmhhhgggggwww
wwwggggfffffgghhmmmmmhhgggggww
wwwggggggggggggthmmmhhggggggww
wwwgggggcgggwwwhhhhhhhggggggww
wwwgggggggggwwwgghhhfffgggggww
wwwgggggggsggwgggggfffffggggww
wwwwggrgsssssggggggfffffgggwww
wwwwwgggsssssggggggfffffggwwww
wwwwwwwgsssssggggcgggfggwwwwww
wwwwwwwwwsssggggggggggwwwwwwww
wwwwwwwwwwwgwgwgwgwgwwwwwwwwww
wwwwwwwwwwwwwwwwwwwwwwwwwwwwww
wwwwwwwwwwwwwwwwwwwwwwwwwwwwww
```
