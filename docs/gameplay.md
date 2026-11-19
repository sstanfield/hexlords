# Gameplay: the plan

This is the plan for the game's rules: what a side can do, what a turn is,
and how the two ways to play (below) share one set of rules. It is a plan,
not a description: almost none of it is built yet. It takes the place of
phase 3 of `hexgame-rust-port-plan.md`, section 8, which proposed one mode
(simultaneous turns); the owner's word (2026-11-19) is that the game supports
two, since they feel very different, above all with several players:

- **Turns** ("I go, you go"): the sides play one after the other. A side's
  orders happen at once, and it sees what they did before it gives the next.
- **Rounds** ("we go"): every side gives all its orders for the round, without
  seeing the others'; then the round plays out at once, for everyone.

The players choose the mode when a game starts, on any map (decided,
2026-11-19).

The feel stays the one of section 8: 1990s fantasy strategy on a map of
cities, cheap troops against dear ones, gold from cities, stacks of eight.
Nothing is taken from an existing game: the rules and the numbers are ours.

## Where we are

Built (2026-11-19): the map with terrain and roads; a map's kinds of unit
(strength, moves, upkeep, price, what each terrain costs to enter); cities
(holder, walls, what they can build, garrison); units that move within their
movement points, turn by turn, with up to 2 points carried over; cities that
build what they are set to, a unit at a time, at the end of each turn; the
side the screen plays (`Game::player`, the holder of the city the game starts
in), whose cities it can set to build; a city's own cost for what it builds
(`produce <unit> <turns> <cost>` in `cities.txt`, else the unit's price). Not
built: whose turn it is (anyone can move any unit), fighting, taking a city,
gold, commanders, the end of the game.

## One set of rules, two modes

Everything a side does is an **order**, a value: move this stack along this
path, set this city to build that, split a stack, hold. The rules apply an
order to the game's state in one place (a `state::rules` module, with no UI),
and say what came of it as **events** (the stack moved to here, a battle was
fought there, a city changed hands). The modes differ only in *when* the
orders are applied:

- In **turns**, an order is applied as soon as it is given, and its events
  are shown at once.
- In **rounds**, the orders are kept until every side has finished giving
  them; then the round's **resolution** applies them all together, and
  every side is shown the events it could see.

The UI already only records commands and leaves them to the session (CLAUDE.md,
"Layering"); orders are the next step of that: the session turns a click into
an order, never changes the map itself. With orders as data, the rest follows:
an AI is something that makes orders; a saved game is the map and the orders
since; a network game sends orders. The rules must be **deterministic**: the
same state and the same orders give the same result on every machine. Any
chance comes from a random number generator whose seed is part of the state,
and the rules use integers, not floats.

| | Turns | Rounds |
|---|---|---|
| Feel | Careful, tactical: see, then decide | Guessing the other side: plans, feints, surprises |
| With several players | Each waits for the others' turns | All plan at once; nobody waits but for the slowest |
| What an order does | Happens now | Happens at the round's end, with the others' |
| Fighting | When a stack moves into an enemy | Where moving stacks meet, as the round plays out |
| What the UI needs | Moves and battles shown as they happen | Planned paths drawn on the map, then a replay of the round |
| AI | Easier: it reacts to what it sees | Harder: it plans without knowing |
| Hidden information | Fog of war is nice to have | Fog of war is most of the game |

## What both modes need first

1. **Players.** Each side is played by a person at this screen, by the AI,
   or by nobody (neutral cities). Only a side's own units and cities take
   its orders.
2. **Orders and events** (`state::orders`, `state::rules`): applying an
   order checks it is allowed (it is that side's unit, the path is within
   reach) and changes the state through `Map`'s methods.
3. **Stacks.** A stack is the units on a hex that move together: as far as
   the slowest of them can go. Orders to split a stack and to join two.
4. **Fighting.** Our own design, kept in a pure `combat` module with tests
   that run it many times. **Chance is always in it** (decided): the weaker
   side can win, and either side can lose more, or less, than the numbers
   foretell. A proposal: a battle is a series of duels between the front
   units of the two stacks; a duel's odds come from the two strengths, with
   bonuses for terrain, a city's walls and a commander; the loser of a duel
   dies; the battle ends when a stack is gone. "Quality beats quantity": a
   strong unit should win against several weak ones more often than their
   numbers suggest, but not always. The tests check the odds, run over many
   seeds (that the stronger side mostly wins, that the weaker sometimes
   does), and that one seed always gives the same battle.
5. **Cities change hands.** A city is taken by moving into it when no enemy
   stands in it (after a battle, or empty). A neutral city has a small
   garrison of its own.
6. **Gold.** Every city brings in gold each turn; what a city builds is paid
   for (proposal: when it is finished, or it waits); every unit costs its
   upkeep each turn; a side that cannot pay loses units. **The price is the
   city's** (decided): what its `produce` line says, or, if it says nothing,
   the unit's `price`. (The unit's price is also what it would cost in a
   city that is given a new kind to build, later.)
7. **Commanders**, in the first version of the rules (decided), even if
   they do little at first. A proposal: a kind of unit is a commander (a
   `commander` key in `units.txt`); a stack has at most one, in a place of
   its own (the ninth slot under the map is kept for it); in a battle it
   gives its stack better odds; it fights last. Later: they grow with
   battles won, carry things, lead bigger stacks.
8. **The end** (decided): a side that has lost all its cities is out of the
   game, and its units with it; the game ends when one side is left. (More
   ways to win can come later: a share of the cities, a number of rounds.)
9. **Fog of war.** A side sees what is near its units and cities. Rounds
   need it far more than turns do.
10. **Saved games.** The state and the orders, in text, like the maps.
11. **Movement points.** A unit that does not use all its points keeps up
    to 2 of them for the next turn (decided: as now, from the C++).

## Turns

- The sides play in an order (the map's, or drawn at the start). A **round**
  is every side once.
- A side's turn begins with its cities' work and its gold: income, upkeep,
  production. (Today production happens at the end of the round for every
  side; in this mode it moves to the start of each side's turn.)
- Moving into an enemy stack or city is an attack: the battle is fought then
  and there, and shown.
- Hotseat: between two people's turns a screen hides the map until the next
  one says they are there.

## Rounds

- **Planning.** Each side gives its stacks paths and its cities what to
  build. The map shows its own planned paths; the others' stay hidden. A
  side says "done", and the round resolves when every side has.
- **Resolution** (from section 8's proposal, refined):
  - A path is cut into steps, each with its cost in movement points.
  - The round advances in ticks. In each tick every stack that can pay for
    its next step takes it, all at the same moment.
  - Two hostile stacks entering one hex in the same tick fight there. Two
    that would swap hexes meet halfway, and fight. A stack entering a hex an
    enemy stands in attacks it.
  - A stack that fought, or whose way is blocked, stops for the rest of the
    round (unless its orders say to go on: a later refinement).
  - After the movement: cities change hands, then every side's income,
    upkeep and production, together.
  - What happened is a list of events, which each side sees replayed (as far
    as its fog allows).
- Hotseat: each person plans in turn behind the hiding screen; the replay
  is shown to each.

## Multiplayer and the AI

- Hotseat first, in both modes: one screen, the players take turns at it.
- Network later, on the orders: turns send each order as it is given;
  rounds send a side's orders once a round. Since the rules are
  deterministic, every machine computes the same game (lockstep), and a
  replay is the orders again.
- The AI makes orders like a player. A first one: take the nearest weak
  city, defend one's own, build what is affordable.

## The order of work (a proposal)

Each step ends playable, with tests and screenshots, and the next builds on
it; the rules are written once for both modes from the first step.

1. **A new game, players and turns.** A screen to start a game: the map,
   the mode, who plays each side (a person or nobody, the AI later). Whose
   turn it is; only that side's units move and its cities build; End turn
   passes to the next side; hotseat's hiding screen. The session gives
   orders instead of changing the map.
2. **Stacks, battles and commanders.** Stacks move together; moving into an
   enemy is a battle (turns), with chance in it; units die; cities are
   taken; a commander leads a stack.
3. **Gold.** Income, the cities' prices, upkeep; a treasury in the sidebar.
4. **The end, and an AI.** A side that has lost its cities is out; the last
   one wins; a first AI.
5. **Fog of war.**
6. **Rounds.** Planned paths, "done", the resolution, the replay.
7. **Saves, then the network.**

## Decided (2026-11-19, the owner's word)

1. **The mode** is chosen by the players when a game starts.
2. **Fighting** always has chance in it: the weaker side may win, or do
   more harm than was to be expected.
3. **A city's cost** for what it builds is its own (the third number of its
   `produce` line); if it gives none, the unit's `price`. The unit's price
   will also serve when a city is given a new kind to build.
4. **Movement points:** up to 2 that are not used carry over to the next
   turn.
5. **Commanders** come in the first version of the rules, if only with a
   little to do.
6. **The end:** a side is out when all its cities are taken; the game ends
   when one side is left.

## Still open

1. Turns: do the sides play in a fixed order, or one drawn anew each round?
2. Rounds: can a stack's orders say what to do when it meets an enemy
   (attack, stop, go round), or is "stop and fight" enough at first?
3. A side that is out loses its units with its last city (as the proposal
   above has it): or do they fight on, or go over to nobody?
