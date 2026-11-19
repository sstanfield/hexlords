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
green's turn`, `red.4.0 moves from (5, 5) to (10, 4) for 12`, `red attacks
blue at (22, 11), 5 against 2: red loses 1, blue 2; red wins`, `red takes
Fenwyke from blue`, `blue is out`), and the orders the game refused, with
why. One seed and one set of agents always make the same game: the rules
take their chance from the game's own generator (`state::rng`), never from
anywhere else.

The same options set up a game in the window: `--map` with `--players`,
`--order fixed|drawn`, `--seed`, `--chance <0-10>`, `--battles
lines|strength` and `--gold <n>` (without `--map`, the new game screen sets
them). To compare
two ways of fighting, play the same seeds with each:

```sh
for seed in 1 2 3 4 5 6; do
  cargo run -q --release -- --headless --map assets/maps/continent --seed $seed --battles strength | head -1
done
```

## The agents

| Name | What it does |
|---|---|
| `passive` | Ends every turn at once |
| `simple` | Builds a commander where it can when its side has none; sets each idle city to build what gives the most strength for its turns, of what its taxes can keep (after its units' upkeep and that of what its cities are already building), and stops the dearest work when they cannot; keeps two units in each of its cities; attacks a city that is not its own, or another side's stack near its free units, with the strongest stack's worth of the free units that can reach it this turn, when the odds are at least 60 in 100 (or 15, when more than a stack can reach it: the first wave); else sends each stack's free units together toward the nearest such city, where they wait for the others |
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
battles lines chance 5
gold 34 income 40 upkeep 32
terrain
46
72
wwwwwwww...                      (the map file's rows: docs/map-format.md)
end terrain
kind riders strength 4 moves 18 upkeep 3 price 10 commander 0 enter grass 2 forest 5 city 1 road 1 bridge 1
kind captain strength 3 moves 14 upkeep 4 price 20 commander 1 enter grass 2 forest 3 city 1 road 1 bridge 1
city 13 7 red walls 2 income 20 builds 2 progress 1 name Brindlewick
can 13 7 0 militia 1 4
can 13 7 1 archers 2 6
can 13 7 2 riders 3 10
city 19 42 green walls 2 income 20 builds none progress 0 name Eskerfall
unit red.0.0 red 13 7 militia strength 2 moves 10/10
unit green.3.0 green 19 42 archers strength 3 moves 12/12
end
```

- `turn <round> <side>`: whose turn it is, which is the side the program plays.
- `order`: this round's order of turns.
- `battles <lines or strength> chance <0 to 10>`: how the game's battles go
  (`docs/gameplay.md`, "Fighting").
- `gold <n> income <n> upkeep <n>`: the side's treasury, what its cities
  bring in a turn, what its units cost a turn.
- `terrain` ... `end terrain`: the map, as in a map file (rows, columns, a
  letter per hex; a capital is a road).
- `kind`: each kind of unit: its numbers, whether it is a commander (the
  bonus it gives its stack, 0 if it is not one), and what each terrain costs
  it to enter.
- `city <row> <col> <side> walls <n> income <n> builds <index or none>
  progress <turns done> name <name>`: every city; `can <row> <col> <index> <kind> <turns>
  <cost>`: what each of the side's own cities can build.
- `unit <name> <side> <row> <col> <kind> strength <n> moves <left>/<a turn's>`:
  every unit on the map. A unit's name (`red.0.0`) stays the same from turn
  to turn while the unit lives.

### What it writes

A line each, in the order they are to be carried out:

| Order | What it does |
|---|---|
| `move <units> <row> <col>` | Move to a hex every one of the units can reach this turn; into another side's units, an attack (by 8 units and a commander at most) |
| `toward <units> <row> <col>` | Move together as far as the slowest gets this turn on its cheapest way to that hex, and where there is room for all of them |
| `build <row> <col> <index or none>` | Set what a city of its side builds |
| `undo` | Take back the order before, if it can be (not a battle, nor one that put a side out) |
| `end` | The turn is over (the end of the output says so too) |

`<units>` is one unit's name or several, with commas between and no spaces
(`red.0.0,red.3.0`); they may stand on different hexes. Empty lines and
lines starting with `#` are skipped. Units of another side can be attacked
but not gone through: a move into them is a battle, and the winners go in. A
city of another side, or nobody's, ends a move: with nobody in it, it is
taken by moving in.
