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

The computer plays every side (`steady`, unless `--players` says otherwise;
a person cannot play without a window). `--mode rounds` plays in rounds
(every side gives its orders, then the round plays out at once), where the
agents' moves are plans for the round. The game goes on until one side is
left or `--rounds` (200) have gone by, and then says how it stands:

```
red wins in round 23.
  red: 9 cities, 41 units
  green: 0 cities, 0 units
```

`--log` prints everything that happened on the way, a line each (`round 3:
green's turn`, `red.4.0 moves from (5, 5) to (10, 4) for 12`, `red attacks
blue at (22, 11), 5 against 2: red loses 1, blue 2; red wins; blue's Captain
Aldric falls`, `red takes Fenwyke from blue; its walls fall to 1`, `blue is
out`), and the orders the game refused, with
why. One seed and one set of agents always make the same game: the rules
take their chance from the game's own generator (`state::rng`), never from
anywhere else.

The same options set up a game in the window: `--map` with `--players`,
`--mode turns|rounds`, `--order fixed|drawn`, `--seed`, `--chance <0-10>`,
`--battles lines|strength|dice`, `--dice-exact hits|misses|defenders`,
`--wounds`, `--blows-back`, `--cap <n>`, `--gold <n>`, and `--fog` or
`--total-fog` (without `--map`, the new game screen sets them). To compare
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
| `simple` | Builds a commander where it can when its side has none; takes a stack to a temple it can reach this turn that has not blessed some of its units, and seeks the blessing there (`bless`); sets each idle city to build what gives the most strength for its turns, of what its taxes can keep (after its units' upkeep and that of what its cities are already building), and stops the dearest work when they cannot; keeps two units in each of its cities; attacks a city that is not its own, or another side's stack near its free units, with the strongest stack's worth of the free units that can reach it this turn, when the odds are at least 60 in 100 (or 15, when more than a stack can reach it: the first wave; or, a siege, when its side has three times the units and cities of the city's holder, 5, or likely to kill a defender); else sends each stack's free units together toward the nearest such city, where they wait for the others |
| `steady` | The computer's player unless another is chosen (`src/planner.rs`, below): armies around a commander, cities defended as they are threatened, attacks at fair odds |
| `bold` | The planner, fast and many: grabs the empty and weak cities first, attacks at lower odds |
| `careful` | The planner, slow and sure: answers threats with more defenders, big armies, high odds |
| `steady/turns`, `bold/turns`, `careful/turns` | The planner playing a game of rounds as it plays turns, without what is its own in rounds (below): for playing the two against each other |
| `cmd:<program> [<argument> ...]` | A program that plays (below) |

A side's turn played by an agent (`agents::play_turn`) ends with the
computer leading the side's marauding stacks, if it has any, after the
agent's orders (`marauder::maraud`, the same as for a person:
`docs/gameplay.md`, "Marauders"); then its marches and patrols go. The
computer's own players hand none to it.

A new agent is a type that implements `Agent` and a name in `agents::by_name`.
It is shown its side's view of the game for each order (`Game::view_for`,
whose chance is a generator of its own: a battle tried in it says nothing
of the game's), and hears of each order the game refused
(`Agent::refused`): under fog, something it could not see is in the way.
The new game screen gives each computer's seat a style: a click on its
Computer button again takes the next (`new_game::COMPUTERS`).

### The planner

`steady`, `bold` and `careful` are one agent, `planner::Planner`, in three
styles (`planner::Style`: how many it keeps home, the odds it attacks at,
how much of a threat it answers, how big an army it gathers, what its
cities' work weighs). Each turn, order by order, it:

1. keeps its treasury in hand: what its cities build is what it can keep
   (within its taxes, and a twelfth of its gold a turn besides: the
   treasury is not a hoard) and pay for, and what is worth most under the
   rules of this game; late
   (with two stacks' worth of soldiers, more than it fields at once) the
   strongest it can keep (a city that builds only what it would send home
   for the best waits), and it disbands its weakest (never a siege
   engine), up to four a turn, to make room for one three points the
   stronger (the owner: late in the game, quality stacks, not junk); and,
   late, it builds a siege engine when it has none
   (`planner::worth`: in lines a unit's worth goes as its strength to the
   power the chance gives, so with little chance it builds strong units and
   with much it builds many; in strength against strength, strengths add
   up); and it keeps a commander;
2. defends: a city that the other sides' units could reach next turn (with
   a whole turn's moves, roads and all) keeps defenders for the threat,
   weighed by worth too, and free units near it come in; its capital keeps
   one more;
3. strikes where the game's own odds (`Game::battle_forecast`, with the
   rules in them) are good enough for its style (against walls, a siege
   engine leads its stack), waves at walls when more
   than a stack can follow, lays siege to the cities of a side it
   outnumbers three times in units and cities (waves at 5 in 100 or better,
   or likely to kill a defender: a last city held by a full stack behind
   high walls fell to nobody before, and a wave that kills nobody only
   raises the defenders' levels), and takes on stacks in the field near its
   cities and units; a threatened city it holds with too few, behind walls
   under 3, raises them a level a turn when its treasury has twice the
   price;
4. gathers the rest around its commander (where it stood at the turn's
   start), sends small stacks to the army or keeps them home until there is
   a force (three units; the careful, four; but a city of nobody's it is
   likely to take as it is, it goes for), stops at temples on the way that
   have a blessing for half of the stack or more (free of others, with
   room) and seeks it there, and sends each army toward the city most
   worth the trouble (its taxes against the way there and what defends
   it), a step at a time as far as it knows the way (never into a stack it
   has not chosen to attack);
5. and on its way **keeps clear of what it could not beat**
   (`src/planner/wary.rs`): every stack of another side it sees could be
   anywhere a turn's movement takes it, and a stack of its own that
   gathers, looks, goes to a temple or marches on a city does not end its
   move where one it fears could come to it, unless where it stands is no
   better (it fears a stack worth more than its own, by its style: careful
   80 in 100 of its own worth, steady 100, bold 130; more as a long game
   goes on). An attack is another matter: that, it means.

The treasury spent, the smaller army, the temple for half a stack and the
wariness were made for rounds (below) and tried in turns after (the owner,
2026-11-22: "you mentioned some AI improvements that would help turn mode,
you can do them"). Measured in turns on the continent (2026-11-22; the
planner with them on two seats against the planner as it was on the other
two, both ways round, 240 seeds, 480 games): steady won 356 and lost 124,
careful 326 and 146 (8 not ended in 200 rounds), bold 285 and 195; steady
with fog of war 293 and 187, with total fog 162 and 76 of 240. One at a
time, steady against steady as it was: the smaller army 292 to 188, the
gold spent 270 to 210 (a sixth of it a turn 286, a twenty-fourth 259), the
wariness 261 to 219, the temple rule 243 to 237 (nothing either way: it is
kept for being the same in both modes); the first three and the temple
rule without the wariness, 303 to 177. Among themselves in turns now,
steady beats careful (326 to 154) and edges bold (255 to 225).

Against `simple`, the other yardstick (the same day, the continent, steady
on two seats and `simple` on two, both ways round): in turns the planner
as it was lost, 102 games to 138 of 240; it now wins 133 to 107, and with
fog of war 181 to 59; in rounds 82 to 38 of 120, with fog of war the
same. **Under total fog `simple` still wins**: 74 to 46 of 120 in turns
(it was 92 to 28), 68 to 52 in rounds. Why is not known. Tried for it and
left out: small stacks going to look instead of waiting at home (nothing
either way; with the army's own doing it too, worse); an army of two
(nothing against `simple`; 259 to 221 of 480 against the army of three
with no fog, not enough to tell); attacking a city it knows of and cannot
see (as `simple` does: it lost 154 to 86 against itself; only with more
units than it remembers there, 131 to 109 against itself and nothing
against `simple`).

With fog of war it never attacks a city it cannot see: it goes where it can
see it first, and weighs it by the garrison it saw there last. A stack it
saw near one of its cities in the last two rounds, now out of its sight, is
a threat to that city still. A place the game refuses to send it, it leaves
be for three rounds, and a way that is shut shows itself as it goes. With
total fog it goes by the land it has seen: while it knows of no city to go
for, every stack goes to see more, a few steps a turn, toward the nearest
edge of that land that all of it can get to (riders do not climb where
foot does) and away from where its others went; and while there is land it
has not seen, its fastest spare unit goes on looking. In rounds, where a
stack has one plan a round, it is sent on past an edge it gets to that
round, into the land not seen, if its way there is by the edge; else to
the edge itself, along the way it knows (`planner::outward`). (It was sent
past the edge whatever the way, and the game, which takes a stack toward
land not seen by the nearest hex it knows as the crow flies, took it
elsewhere or refused: of forty games of rounds under total fog thirteen
ended in 200 rounds, most of the others at eight cities to one with the
ninth never seen. All forty end now, in 88 rounds on average; in turns,
in 81.)
Under total fog its styles won 76 to 80 games in 100 against `simple`
(which explores too), and games lasted half again as long (on the
continent, before the rules of 2026-11-20). On generated maps of four
sides they now last far longer: 3 or 4 in 24 end within 300 rounds (most
go past 240; mountains, walls for anything that walks, make the ways
round long through land not yet seen). Its sense of worth
is plain arithmetic (no `powf`), so a seed plays the same game on every
machine.

With much chance in battle it waits for better odds and keeps fewer at home
(walls and strength do little for defenders then); a long game makes it
bolder, so two that wait for each other do not wait for ever.

How the styles were chosen: tournaments without a window on the continent,
every arrangement of two styles on the four seats, over seeds, under each
set of rules (chance 1, 5, 7, 9; lines and strength against strength; fog of
war). Against `simple`, two seats each, the three styles win most games
under every set of rules: 56 to 67 in 100 with chance 9 (where luck decides
most), 61 to 71 with strength against strength, 72 to 82 at the default
rules, nearly all with little chance or fog. They finish off a passive
side, an all-out rusher and a turtle.
Among themselves no style beats another by much, and each has rules it
does best under.

Measured again (2026-11-21) on twelve generated maps, under the rules of
now (upkeep half a unit's price, every neutral city garrisoned, walls by
steps, the new kinds), each style on two seats against `simple` on two,
both ways round: at the default rules steady won 13 games of 24 and lost
7, careful 15 and 7, bold 11 and 13; with strength against strength, 15
and 8, 10 and 10, 9 and 12. Bold, the quickest, does no better than
`simple` now; all four styles on a map together, bold wins most (14 of
46, simple 12, careful 11, steady 9).

### The planner in rounds

A game of rounds is another game (the owner's word, 2026-11-22: "Round
based games probably need their own AI since they are very different from
turn based"): every side's orders are given before anything moves, the
faster stacks are there first, and a stack fights whoever it finds on its
way. In one the planner plays in a way of its own (`src/planner/rounds.rs`;
what its cities build, who stays home, what its armies go for and what
they keep clear of are as in turns):

- Its wariness is **a matter of time**: a stack of another side could be
  on a hex at the moment its speed gets it there, and a stack of its own
  does not pass a hex where one it fears could be by then (in turns
  nobody else moves while it does).
- **An attack is one stack**: the units of one hex if they are enough (the
  hex that gets them there first), units of several hexes only where the
  game makes one stack of them (they gather on the way). Stacks around a
  city that could take it together but not as one stack this round **join
  first**, and attack as one the round after.
- A stack in the field has moved on by the time anyone gets to it: it is
  attacked only at odds 15 in 100 above those a city is attacked at.

`steady/turns`, `bold/turns` and `careful/turns` play a game of rounds
without these, as the planner plays turns. Measured on the continent
(2026-11-22; a style in its way for rounds on two seats against the same
style playing rounds as turns on the other two, both ways round, 240
seeds, 480 games): steady won 261 and lost 219, bold 274 and 206, careful
263 and 206 (11 not ended in 200 rounds); steady with fog of war 127 and
113 of 240, with total fog 66 and 54 of 120. Among themselves in rounds,
steady beats careful (187 to 52 of 240) and bold (138 to 102). Four steady
planners end every game of forty, in 61 rounds on average (in turns, 52).

How it came to this. The way for rounds was made first, against the
planner as it then played turns, and it won far more: steady 367 of 480,
careful 324, bold 305. Most of that was not about rounds at all: what the
parts were worth then, steady against the best of the old three over 480
games, as they were put together: setting out at three, wary, its attacks
one stack, 242; with stacks joining for an assault, 270 (256 of those
without the wariness); with the gold spent, 318, and 328 with the temple
left to half a stack; with the attack tried in its order (one stack that
is enough, else join, else a wave), 355. The smaller army, the gold, the
temple rule and the wariness then went into the planner in turns (above),
where they win as much, and so into `<style>/turns`: what is left between
the two is what is the rounds' own. Its battles, measured then: it won 61
in 100 of its attacks (the planner playing rounds as turns, 47), and 20 in
100 of them were one unit's (36).

Tried and left out, for winning nothing over 480 games: holding an attack
on a city back for the stacks of its holder that could be in it first;
sending a threatened city only the units that get in before the others
could; more defenders at home (a second unit kept in every city lost two
games in three).

## A program that plays: the text protocol

`--players red=cmd:./my-bot` has red played by a program of your own: a
script, or a bridge to some other player. At the start of each of red's turns
the program is run (no shell: the words after `cmd:`, up to the next comma,
are the program and its arguments), reads the game on its standard input,
writes its orders on its standard output, and exits. A line that is no order
is said on the game's error output, and the turn goes on without it. An
order the game refuses changes nothing, and the turn goes on: without a
window `--log` prints it (`refused: <the order>: <why>`, the order as the
game's code writes it, not as the protocol does); in the window nothing says
so; and the program is not told.

### What it reads

The game as the program's side knows it (`Game::view_for`; every agent is
given the same). With fog of war, the other sides' units out of its sight
are not in it, and a city in the fog is held by who held it when the side
saw it last (nobody, if it never did), behind the walls it had then; what
its side remembers of stacks
seen before is in the view the computer's own players are given
(`Sight::ghosts`; the text has no line for them: a program remembers what
it saw itself), and its orders go as far as
it knows the way (a stack it could not see, met on the way, is a battle
there). With total fog, land the side has never seen is `?` in the terrain
and its cities are not listed; no order can go there or through it (but a
`toward` in rounds, below). Fog or
not, it is not told what the other sides' cities build or can build, their
gold, their units' standing orders or the moves they have left.

```
turn 1 red
order red green blue yellow
battles lines chance 5
fog off
gold 50 income 40 upkeep 18
terrain
46
72
wwwwwwww...                      (the map file's rows: docs/map-format.md; `?`: never seen)
end terrain
kind archers strength 3 moves 10 upkeep 3 price 6 commander 0 travels land carries 0 siege 0 bonus city:1 enter grass 2 forest 2 swamp 5 desert 3 hill 3 city 1 road 1 bridge 1
kind lightcavalry strength 3 moves 20 upkeep 4 price 8 commander 0 travels land carries 0 siege 0 bonus none enter grass 2 forest 5 desert 3 hill 5 city 1 road 1 bridge 1
kind captain strength 3 moves 14 upkeep 10 price 20 commander 1 travels land carries 0 siege 0 bonus none enter grass 2 forest 3 swamp 5 desert 3 hill 4 city 1 road 1 bridge 1
kind transport strength 1 moves 12 upkeep 5 price 10 commander 0 travels sea carries 6 siege 0 bonus none enter water 2 city 2
city 13 7 red walls 2 income 40 builds none progress 0 name Brindlewick
can 13 7 0 militia 1 3
can 13 7 1 lightinfantry 1 6
can 13 7 2 archers 1 6
can 13 7 3 lightcavalry 2 8
city 19 42 green walls 2 income 40 builds none progress 0 name Eskerfall
unit red.0.0 red 13 7 militia strength 2 moves 10/10 level 1
unit green.1.0 green 19 42 archers strength 3 moves 10/10 level 1
end
```

- `turn <round> <side>`: whose turn it is, which is the side the program plays.
- `mode rounds`: only in a game played in rounds (`--mode rounds`,
  `docs/gameplay.md`, "Rounds"): the program's orders to move are plans
  (nothing moves until every side has given its orders; then the round
  plays out on a clock: each stack goes the way it was given, the faster
  ones are there first, and a stack that gets to a hex another side
  stands on attacks it, wherever it was going), so the game it is shown
  is the one at the round's start, whatever it has ordered. A `move`
  naming units on several hexes is one stack: they gather first on the
  one of their hexes nearest to where they go, and go on together (with
  no hex of theirs that all can gather on and go on from, the units of
  each hex go on their own); to
  have stacks attack one after the other, order each by itself. In turns
  the line is
  not there. In rounds `toward` may name land the side has never seen
  (total fog of war): the units set off toward it and find their way as
  they come to see more. With `--order drawn` the `order` line is the
  order the orders are given in; who goes first of stacks that get
  somewhere at the same moment is drawn after them.
- `order`: this round's order of turns.
- `battles <lines, strength or dice> chance <0 to 10>`: how the game's
  battles go (`docs/gameplay.md`, "Fighting"; on the dice the chance plays
  no part); then, on the dice, `exact <hits, misses or defenders>`: what a
  roll of exactly a side's number does (`--dice-exact`); `wounds last` when
  a unit's hits stay with it until its side's turn ends (`--wounds`); and
  `blows back` when the loser of an exchange may hit back (`--blows-back`);
  `cap <n>` when no unit fights stronger than `n`, its bonuses counted
  (`--cap`): `battles dice chance 5 exact hits wounds last blows back cap 9`.
- `fog <off, on or total>`: fog of war (`docs/gameplay.md`).
- `gold <n> income <n> upkeep <n>`: the side's treasury, what its cities
  bring in a turn, what its units cost a turn.
- `terrain` ... `end terrain`: the map, as in a map file (rows, columns, a
  letter per hex; a capital is a road), but `?` for a hex never seen (total
  fog of war).
- `kind`: each kind of unit: its numbers, whether it is a commander (the
  bonus it gives its stack, 0 if it is not one), the walls it breaks as a
  siege engine (`siege`, 0 if none), where it fights the stronger (`bonus
  forest:1,city:1`, or `none`), how it travels (`land`,
  `sea`: a ship, carrying as many land units as it says; `air`: a flyer),
  and what each terrain costs it to enter (`docs/gameplay.md`, "Ships and
  flyers", says how they go).
- `city <row> <col> <side> walls <n> income <n> builds <index or none>
  progress <turns done> name <name>`: every city the side knows of (`<side>` is `neutral` for nobody's, here and for a unit); `can <row> <col> <index> <kind> <turns>
  <cost>`: what each of the side's own cities can build.
- `unit <name> <side> <row> <col> <kind> strength <n> moves <left>/<a turn's>
  level <n>`: every unit on the map (with fog of war: every unit the side sees); its strength is what it fights with
  before its stack, the ground and walls (its kind's, a point for each level
  above 1, a temple's blessing), its level 1 to 3 (`docs/gameplay.md`); and,
  at its end, `wounds <n>` for a unit wounded (wounds that last: it has
  that many hits fewer, and fights at half its strength, rounded up). A
  unit's name (`red.0.0`) stays the same from turn to turn while the unit
  lives.

### What it writes

A line each, in the order they are to be carried out (a saved game keeps
the orders that were carried out as the same lines, `docs/saves.md`;
`agents::order_text` writes them, and writes `-` for no units at all):

| Order | What it does |
|---|---|
| `move <units> <row> <col>` | Move to a hex every one of the units can reach this turn; into another side's units, an attack (by 8 units and a commander at most) |
| `toward <units> <row> <col>` | Move together as far as the slowest gets this turn on its cheapest way to that hex, and where there is room for all of them. Units that go at different prices (flyers with units that walk) take a way all of them can go, and go together or not at all; so do they on a `goto`, a patrol and in a round |
| `onward <units> <row> <col>` | In rounds: units with a plan for the round (`move`) go on from where it ends to that hex, which all of them can reach from there with what the plan's way leaves them: a waypoint (the plan goes by the first hex, then on). Refused with no plan, or after a plan that ends in a battle, in a city to take, or on a temple that has yet to bless them (it stops them for the round). In turns it is a `move` |
| `goto <units> <row> <col> [<row> <col> ...]` | A standing order: go to that hex over as many turns as it takes; with more than one hex, by way of each in order (stops; the last is where it ends). At a stop it goes on at once with what movement is left; at a stop on a temple, those the temple has not blessed seek its blessing and the stack stops there for the turn. This turn's part is taken at once (as `toward`), the rest at the end of each of the side's turns, with the movement left (after `end`); it never attacks: a step into another side's units stops it (it keeps the order, and tries again next turn). A `move` or `toward` of the same units ends it |
| `done <units>` | Put the units aside for this turn (Next stack and the player's tasks pass them by); it changes nothing else, but for a patrol or marauders that fought (below) |
| `defend <units>`, `undefend <units>` | Mark the units as defenders, staying where they are turn after turn (Next stack and the player's tasks pass them by) until they are sent somewhere; or no longer |
| `bless <units>` | Units on a temple seek its blessing: each the temple has not blessed yet is blessed (chance says how) and its movement is spent for the turn; one it blessed before keeps its movement |
| `disband <units>` | Send units home for good: they are gone, and their upkeep with them (a ship at sea only with those it carries, or with ships enough left to carry them) |
| `build <row> <col> <index or none>` | Set what a city of its side builds |
| `walls <row> <col>` | Raise the walls of a city of its side a level, paid at once: once a turn, up to 6, 25 gold for each level it raises them to |
| `rally <row> <col> <row> <col> [<row> <col> ...]`, `rally <row> <col> none`, `rally <row> <col> defend`, `rally <row> <col> point <n> [bless]` | Where a city of its side sends what it builds: along a route (with a standing order, as `goto`, by way of the stops before the last); nowhere (it stays in the city); nowhere, defending the city; or to the side's rally point `n` (0 to 9), with `bless` by way of a temple near the way first |
| `queue <row> <col> <item> [<item> ...] [repeat <from> [<times>]]`, `queue <row> <col> none` | A city's production queue: its items in turn, each `<what>x<count>[@<where>]` (`what` an index into what it builds, as `build`; where its units go: `defend`, `point<n>`, `point<n>+bless`, or a route `<row>.<col>[,<row>.<col> ...]`; `stay` or none, they stay: a saved game writes `@stay`); after the last it stops, or with `repeat` goes back to item `from` (0 the first) and on, `times` times or for ever. At most 8 items. It goes on from the item it was building if that is still there; `none` takes the queue away. A queue that runs out leaves the city building nothing, needing orders; `build ... none` is nothing on purpose |
| `point <n> <row> <col> [defend]`, `point <n> none` | Place (or move) the side's rally point `n` (0 to 9), on land the side has seen and in no city but its own, where units are sent to gather: once it is full they stand beside it; with `defend` those who get there defend it. `none` removes it: the units on their way there stop, and the cities that sent theirs there keep them. Units sent to a point's hex with `goto` follow it if it moves |
| `patrol <units> <row> <col> <row> <col> [<row> <col> ...] [attack <percent>]` | A standing order for units on one hex: go round the waypoints in order, the first first (give where they stand first, to start from there), turn after turn: round again after the last if it is the first again, else back the way they came. As with `goto`, this turn's part at once, the rest at the end of each of the side's turns, on from a waypoint with what movement is left, never into another side's units seen. With `attack <percent>`, they attack the other sides' stacks they see and can reach at those odds or better, once a turn at most; without, never. Waypoints are on land the side has seen, and in no city but its own |
| `maraud <units>`, `unmaraud <units>` | Hand the units to the computer (`src/marauder.rs`), which leads them at the end of each of the side's turns, after the program's orders: it strikes the stacks it can beat and takes the cities it can, at the `bold` odds; or no longer |
| `roam <units> <row> <col>` | The computer's move for marauding units: as `move`, and they go on marauding (refused for units that are not) |
| `undo` | Take back the order before, if it was a move, a `goto` or `patrol` that has gone one leg at most, a plan in rounds, a `build` or a `walls` (not a battle, nor a move that put a side out or, with fog of war, showed something new; a `queue`, `rally`, `point`, `maraud`, `bless` or `disband` is not taken back; a `done` or `defend` given since does not count) |
| `end` | The turn is over (the end of the output says so too) |

`<units>` is one unit's name or several, with commas between and no spaces
(`red.0.0,red.3.0`); they may stand on different hexes, but for `bless`,
`patrol` and `onward`, which take the units of one hex. Empty lines and
lines starting with `#` are skipped. Units of another side can be attacked
but not gone through: a move into them is a battle, and the winners go in. A
city of another side, or nobody's, ends a move: with nobody in it, it is
taken by moving in.

A stack on patrol or marauding that fights (attacking, or attacked and
holding) waits for its player's word: `done` lets it go on (a patrol at
once, if it can); another order for it (`move`, `toward`, `goto`, `defend`,
`bless`, `patrol`, `maraud`) ends its patrol or its marauding; left alone,
it goes on the turn after. The `unit` lines do not say which of the side's
units patrol or maraud: a program keeps that itself.
