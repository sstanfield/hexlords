# Agents: the computer's players, and games without a window

A side the computer plays is played by an **agent**: something that is
asked, again and again, for the next order of its side, until it says the
turn is over (`src/agents.rs`). The same agents play in the window and
without one, so a game can be played many times over, with many seeds, to
see what the rules do: who wins, how fast, with what.

## Without a window

```sh
cargo run --release -- --headless --map assets/maps/continent --seed 7 --rounds 100
cargo run --release -- --headless --map assets/maps/continent --players red=simple,blue=passive --log
```

The computer plays every side (`simple`, unless `--players` says otherwise;
a person cannot play without a window). The game goes on until one side is
left or `--rounds` (200) have gone by, and then says how it stands:

```
red wins in round 23.
  red: 9 cities, 41 units
  green: 0 cities, 0 units
```

`--log` prints everything that happened on the way, a line each (`round 3:
green's turn`, `red.4.0 moves from (5, 5) to (10, 4) for 12`, `red takes
Waste from nobody`, `blue is out`), and the orders the game refused, with
why. One seed and one set of agents always make the same game: the rules
take their chance from the game's own generator (`state::rng`), never from
anywhere else.

The same options set up a game in the window: `--map` with `--players`,
`--order fixed|drawn` and `--seed` (without `--map`, the new game screen
sets them).

## The agents

| Name | What it does |
|---|---|
| `passive` | Ends every turn at once |
| `simple` | Sets each idle city to build the cheapest of what it can; keeps one unit in each of its cities; sends the others toward the nearest city that is not its own and has nobody in it |
| `cmd:<program> [<argument> ...]` | A program that plays (below) |

A new agent is a type that implements `Agent` and a name in `agents::by_name`.

## A program that plays: the text protocol

`--players red=cmd:./my-bot` has red played by a program of your own: a
script, or a bridge to some other player. At the start of each of red's turns
the program is run (no shell: the words after `cmd:` are the program and its
arguments), reads the game on its standard input, writes its orders on its
standard output, and exits. What it gets wrong is said on the game's error
output, and the turn goes on without that order.

### What it reads

```
turn 3 red
order red green blue yellow
terrain
46
72
wwwwwwww...                      (the map file's rows: docs/map-format.md)
end terrain
kind riders strength 4 moves 18 upkeep 3 price 10 enter grass 2 forest 5 city 1 road 1 bridge 1
city 13 7 red walls 2 builds 2 progress 1 name Brindlewick
can 13 7 0 militia 1 4
can 13 7 1 archers 2 6
can 13 7 2 riders 3 10
city 19 42 green walls 2 builds none progress 0 name Eskerfall
unit red.0.0 red 13 7 militia strength 2 moves 10/10
unit green.3.0 green 19 42 archers strength 3 moves 12/12
end
```

- `turn <round> <side>`: whose turn it is, which is the side the program plays.
- `order`: this round's order of turns.
- `terrain` ... `end terrain`: the map, as in a map file (rows, columns, a
  letter per hex; a capital is a road).
- `kind`: each kind of unit, with what each terrain costs it to enter.
- `city <row> <col> <side> walls <n> builds <index or none> progress <turns
  done> name <name>`: every city; `can <row> <col> <index> <kind> <turns>
  <cost>`: what each of the side's own cities can build.
- `unit <name> <side> <row> <col> <kind> strength <n> moves <left>/<a turn's>`:
  every unit on the map. A unit's name (`red.0.0`) stays the same from turn
  to turn while the unit lives.

### What it writes

A line each, in the order they are to be carried out:

| Order | What it does |
|---|---|
| `move <unit> <row> <col>` | Move to a hex the unit can reach this turn |
| `toward <unit> <row> <col>` | Move as far as it gets this turn on its cheapest way to that hex |
| `build <row> <col> <index or none>` | Set what a city of its side builds |
| `end` | The turn is over (the end of the output says so too) |

Empty lines and lines starting with `#` are skipped. A unit of another side
blocks a hex (there are no battles yet); a city of another side, or nobody's,
with nobody in it is taken by moving in, and the move ends there.
