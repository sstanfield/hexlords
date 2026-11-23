# Saved games: the file

A saved game is a text file, `<map>-<n>.save`, in the directory of saved
games (`docs/gameplay.md`, "Saved games"). It holds how a game began and
everything done to it since; the game's state is not in it. To load it is
to begin the game again and do it all again (`src/save.rs`), which gives
the same game because the rules are deterministic.

The format is the game's own, and may change with it: a file is for the
version of the rules that wrote it. One that no longer comes out as it was
saved is refused, not guessed at.

## The lines

The first line is `hexlords save 1`. Then a line for each thing, a key and
its value; a key this version does not know is passed over.

```
hexlords save 1
map continent
round 12
saved 1795380000
assets /home/me/hexlords/assets/maps/continent/assets
mode turns
order fixed
seed 1795379000123456789
gold 50
fog on
battles strength chance 3 exact hits wounds 1 back 1 cap none
seat red person
seat green steady
seat blue bold
seen red 41
terrain 48
... 48 lines: the map's terrain.map ...
units 310
... 310 lines: its units.txt ...
cities 96
... 96 lines: its cities.txt ...
steps 1406
build 20 9 2
move red.0.0,red.1.0 21 11
end
...
check 9f3c2a17d05be481
```

| Key | What |
|---|---|
| `map` | The map's name, for the list |
| `round` | The round the game is in, for the list |
| `saved` | When it was written, in seconds since 1970: the list shows the last first |
| `assets` | The map's own pictures, if it had a directory of them (used if it is still there) |
| `mode`, `order`, `seed`, `gold`, `fog`, `battles` | The setup (`src/setup.rs`): `turns` or `rounds`; `fixed` or `drawn`; the seed of the game's chance; each side's gold at the start; `off`, `on` or `total`; the battles' system, chance, what an exact roll is, whether wounds last, whether losers hit back, the cap or `none` |
| `seat <side> <who>` | A side in the game, in the order of the turns: `person`, or an agent's name |
| `seen <side> <n>` | How much of the game's history that person had seen (the log marks the rest as new) |
| `terrain <n>`, `units <n>`, `cities <n>` | The `n` lines that follow are the map's files as they were read (`docs/map-format.md`); `units` and `cities` only if the map has them |
| `steps <n>` | The `n` lines that follow are what was done, in its order |
| `check` | The last line: a number that says how the game stood (`Game::digest`): the round and whose turn (and how far a round has played out), the order of the sides and who is out, the game's chance, the treasuries and rally points, every unit with its orders and its place in its stack, every city with its work, its queue and where it sends its units, what each side has seen and remembers under fog of war, and how much has happened. A file without it was not written whole |

## The steps

A step is an order that was carried out, written as the agents' protocol
writes orders (`docs/agents.md`: `move <units> <row> <col>`, `end`, ...; a
unit is `red.3.0`, a city its hex, no units `-`), or one of:

- `beat`: in rounds, the round that was playing out went on a beat
  (`Game::play_out`);
- `ease <side>`: the computer, about to lead that side's marauders, let its
  units that had fought and were left alone go on (`Game::stand_easy`).

An order the game refused is not there: it changed nothing.
