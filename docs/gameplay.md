# Gameplay: the plan

This is the plan for the game's rules: what a side can do, what a turn is,
and how the two ways to play (below) share one set of rules. It is a plan,
not a description, though its first steps are built ("Where we are"). It takes the place of
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

Built (2026-11-19):

- the map with terrain and roads; a map's kinds of unit (strength, moves,
  upkeep, price, what each terrain costs to enter); cities (holder, walls,
  what they can build at what cost, garrison);
- **a new game screen**: the map, the mode (rounds are shown, to come), the
  order of turns (the same each round, or drawn anew), who plays each side
  (a person, or the computer); several people play hotseat;
- **turns**: one side after the other, in the round's order; only that
  side's units move and its cities can be set; a side's turn begins with
  its units' points back (up to 2 kept) and its cities' work (from the
  second round); a screen covers the map between two people's turns;
- **orders** (`src/orders.rs`) and **events**: everything a side does goes
  through `Game::apply`, a person's clicks and the computer's alike;
- **taking cities**: another side's (or nobody's) city with nobody in it is
  taken by moving in;
- **stacks, battles and commanders** (below, "Fighting"): the units of a hex
  are selected together and move together, or some of them; units sent into
  another side's fight them, and go in if they win; the chance (0 to 10)
  and the system (lines, or strength against strength) are set with the
  game; the ground, walls and a commander make units stronger; the odds
  are shown before an attack, a report after it, and the news to whoever
  was attacked while they waited;
- **the end**: a side that has lost its last city is out (its units go, in
  one place, `Game::put_out`, so that this can change); the last side wins;
- **agents and games without a window** (`docs/agents.md`): the computer's
  players, `--headless` to play agents against agents, and a text protocol
  for a program of one's own to play.

- **gold**: a treasury for each side, its cities' taxes, its units'
  upkeep (those it cannot pay leave), and the price of what its cities
  build, paid when it is done (it waits for the gold);
- **orders that last** (below, "Easy to play"): a stack sent somewhere far
  goes there over turns, a city sends what it builds where its holder
  says, a stack can be put aside for the turn, and a list of tasks says
  what is left to do;
- **what a side sees**: the other sides' stacks only as banners with their
  size, their cities only from outside; and **fog of war**, set with the
  game (below, 9).

Not built: rounds, saved games.

## One set of rules, two modes

Everything a side does is an **order**, a value: move this stack along this
path, set this city to build that, split a stack, hold. The rules apply an
order to the game's state in one place (`Game::apply`, with no UI; the orders
are in `src/orders.rs`), and say what came of it as **events** (the stack moved to here, a battle was
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
3. **Stacks** (built). A stack is the units on a hex: a click on the hex
   selects those of them with moves left, and they go together where all
   of them can (each pays its own way); a click on a unit under the map
   takes it into the selection or leaves it out, so a stack splits by
   sending some of it, and two join by going to one hex. A hex holds eight
   units and a commander. `Order::Move` and `Order::Toward` take a list of
   units (`Toward`: as far as the slowest gets).
4. **Fighting** (built, `state::combat`, pure and in integers, with tests
   that run it many times): our own design, **made to be experimented
   with** (decided). Units sent into another side's units attack them; the
   two stacks fight until one is gone; the dead are gone; the attackers that
   live go in (and take the city), or, if they lost, the defenders stay. An
   attack is a stack: as many units as a hex holds (8 and a commander),
   from one hex or from several.
   - **An exchange** between strengths `a` (the attacker's) and `b`: `a`
     wins with the odds `a^k : b^k`, where `k` is `2 × (10 - chance) / 5`.
     Only the ratio of the strengths counts. The loser of an exchange takes
     a hit; a unit dies at its second (`combat::HITS`), and is whole again
     after the battle. (In integers: `k` is one of the two whole numbers
     around it, drawn in the share that makes the odds right, so every
     machine fights the same battle.) Before (2026-11-19, first version):
     each side scored its strength plus luck, and the steep edge that gave
     the stronger, with walls in full, made cities near fortresses (a full
     stack of good units took a walled neutral city one time in eight).
   - **How much chance**, 0 to 10, set on the new game screen (or
     `--chance`); **5 unless it is set** (decided). At 0 the stronger always
     wins (a tie is the defender's). At 5, `k` is 2: a strength twice
     another's wins an exchange four times in five, 4 against 3 about 64
     in 100; the less chance, the more strength counts (at 3, 4 against 2
     about 87; at 7, about 70). At 10 strength counts for nothing.
   - **How a battle goes**, a choice of systems behind one function
     (`combat::fight(attackers, defenders, rules, the game's chance) -> who
     died`), so that more can be added and each played without a window
     against the others (`--battles`):
     - *Lines* (`lines`, the default for now): each stack stands in a line
       from its weakest to its strongest, its commander last; the two at
       the front fight; a unit out of hits dies and the next of its line
       steps up.
     - *Strength against strength* (`strength`): each exchange matches the
       two stacks' whole strengths, and the hit falls on a unit of the
       loser drawn by chance, so that cheap units in front do not shield the
       dear ones behind. Many weak units beat one strong one here, where in
       lines they lose one by one.
   - **Bonuses**, in either system: a defender is 1 stronger on hills and
     in forests, 2 on mountains, and three quarters of a point for each
     level of its city's walls (a battle's strengths are in quarter points,
     `combat::QUARTER`); every unit of a stack with a commander is as much
     stronger as the commander says (`commander 1`). The attackers get no
     ground and no walls. With chance 5, eight good units take a city held
     by one behind walls 1 nearly always, the strongest neutral city on the
     continent (two wardens behind walls 3) about half the time, a side's
     capital with its captain four times in five; equal stacks in the field
     are even.
   - **The odds** before an attack (`Game::battle_odds`) are 200 battles
     fought with a chance of their own, so asking does not change the game;
     the line under the map shows them ("attack: 82% likely to win").
   - The tests check the odds over many seeds (the stronger mostly wins,
     the weaker sometimes, at the middle settings; never at 0), and that one
     seed always gives the same battle.
5. **Cities change hands.** A city is taken by moving into it when no enemy
   stands in it (after a battle, or empty). A neutral city has a small
   garrison of its own.
6. **Gold** (built). Each side has a treasury (`--gold`, or the new game
   screen: 50 unless set). At the start of each of its turns (from the
   second round): its cities' taxes come in (a city's `income`, 10 unless
   its map says); its units' upkeep goes out, and if the treasury cannot pay
   it all, units leave, unpaid: as few as make up what it is short (the
   least dear that does it alone, or else the dearest; a commander last);
   then what its cities finish is paid for, and a unit whose price the
   treasury cannot pay waits in its city, done, until it can. **The price is
   the city's** (decided): what its `produce` line says, or, if it says
   nothing, the unit's `price`. (The unit's price is also what it would
   cost in a city that is given a new kind to build, later.) The sidebar
   shows the treasury, its taxes and its upkeep; the log says who left
   unpaid. Still to try: gold from a city taken, a tax rate to set.
7. **Commanders**, in the first version of the rules (decided), even if
   they do little at first. Built: a kind of unit is a commander (the
   `commander <n>` key in `units.txt`, the bonus it gives); a hex holds one
   besides its eight units (the ninth slot under the map); every other unit
   of its stack fights `n` stronger; in lines it fights last. The continent
   has a captain in each side's first city, and those cities can build
   captains (4 turns, 20 gold), to replace one that is lost; a captain done
   in a city that has one waits for room. The simple agent builds one when
   its side has none. Captains have names (put together by the game's chance
   from syllables of our own: "Captain Aldric"), given when the game starts,
   when one is built, or when one rises from the ranks (below), and they
   rise in level like other units (a captain of the highest level commands a
   point more). Later: they carry things, lead bigger stacks.
7b. **Levels, blessings, captains from the ranks** (built, the owner's word
   2026-11-19: "a tiny bit of role-playing feel"):
   - **Levels** 1 to 3 (`MAX_LEVEL`): regular, veteran, elite; a captain's
     are numbers. Each level above the first makes a unit a point stronger
     (`Unit::power`). They come with experience: each unit that lives
     through a battle its side won gains a point, two if its side's odds
     were below half (`HARD_ODDS`); 2 make a veteran, 5 elite
     (`LEVEL_EXPERIENCE`).
   - **Training**: a city whose record says `trains veteran` (or `elite`)
     makes its units at that level (the continent's Gorsehaven trains
     veterans).
   - **Blessings**: a unit that stops on a temple is blessed, once in its
     life: +1 strength or +2 moves a turn, chance says which. A move that
     brings a blessing cannot be undone. (The continent has a temple near
     each side's first city.)
   - **Captains from the ranks** (the owner's word, 2026-11-19): when a
     stack with no commander wins a battle, a captain of the map's first
     kind of commander may rise among its survivors, named, with no moves
     that turn. The chance, in percent, is half of how unlikely the win was:
     (100 - the odds the game gave before the attack) / 2
     (`emerge_chance`): one in two for a win nobody expected, none for a
     sure one.
   - **A captain rides with its stack** (the owner's word): at the start of
     its side's turn, a commander gets the moves of the fastest unit on its
     hex, if they are more than its own: a captain with riders rides
     (`Map::reset_move_points`).
   - What shows: a unit's ring is silver for a veteran (or a captain of
     level 2) and gold for elite (level 3), on the map and on its icons; the
     panels say its rank, its experience and what it needs for the next
     level, and its blessing; the log tells of each level, blessing and
     captain. A captain's card in a battle shows its name.
8. **The end** (decided): a side that has lost all its cities is out of the
   game, and its units with it; the game ends when one side is left. (More
   ways to win can come later: a share of the cities, a number of rounds.)
9. **What a side sees** (built, the owner's word 2026-11-19: "you can
   only see the details of your side's units and cities"). The other
   sides' stacks are banners in their color on the map, with a pip for each
   unit (as a city's garrison is), and their panels say how many there are
   and whose, not what; their cities show their name, holder, walls, taxes
   and how many stand in them, never what they build or where its units go.
   **Fog of war** is a choice when the game is set up (the new game screen,
   `--fog`, `Setup::fog`): a side sees what is within 2 hexes of its units
   (`UNIT_SIGHT`) and 3 of its cities (`CITY_SIGHT`) (`Game::sight`); the
   rest of the map is dimmed: its terrain and where its cities are show,
   but no units, and no city's holder (on the map, the minimap and the
   lists of cities). It is worked out anew whenever the map changes. With
   nobody at the screen (the computer against itself) all is seen. The
   reach the map marks for the selected units is as far as the person
   knows: units unseen do not stop it (`Game::range_seen`); a move the
   rules refuse because of one says "Something in the fog is in the way."
   While another side plays, the screen only looks: a click selects
   nothing, Next stack and the lists of cities are the person's own, and
   their city's buttons give no orders. The
   agents still read the whole game (they do not respect the fog yet).
   Rounds need fog far more than turns do.
10. **Saved games.** The state and the orders, in text, like the maps.
11. **Movement points.** A unit that does not use all its points keeps up
    to 2 of them for the next turn (decided: as now, from the C++).

## Turns

- The sides play in an order, chosen when the game is set up (decided):
  always the order of the seats, which the new game screen shows, or one
  drawn at the start and again after every round. A **round** is every side
  once. (Built.)
- A side's turn begins with its cities' work and its gold: income, upkeep,
  production. (Built for production.)
- Moving into an enemy stack or city is an attack: the battle is fought then
  and there, and shown.
- Hotseat: between two people's turns a screen hides the map until the next
  one says they are there. (Built.)

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

## Easy to play

The owner's concern: late in a big game, managing many stacks and cities
can become a slog, and what helps most is a clean interface that is easy to
get around, and good defaults, so that little has to be said for the usual
thing to happen. Ideas to try, as the rules grow:

- **Orders that last** (built): Go to (G, or the Go to button, then a
  hex: `Order::GoTo`) sends the selected units to a far hex: they take this
  turn's part at once, and the rest at the start of each of their side's
  turns, before the player plays (`Game::marches`), as far as the slowest
  of them goes, until they are there. It is taken if each unit could get
  there over the land in as many turns as it takes (`way_over_land`:
  whoever stands on the way now does not count, nor do the moves left this
  turn); units on several hexes go as a stack from each. A march never attacks: if its next
  step is into another side's units, or there is no way, the stack stops
  (`Event::Stalled`, and a line in the log), keeps its order, and goes
  back among the stacks that wait for orders (Next stack, the tasks); left
  alone, it tries again next turn. A move of the player's own ends the
  order (an undo gives it back). In rounds, the same: a stopped march is
  in the list again, and goes on if it is not given another order. Later:
  a stack follows another.
- **Rally points** (built): a city's window has "Send new units to..."
  (then a hex) and "Keep them here" (`Order::Rally`): what it builds is
  sent there with a standing order the moment it is done. The hex has to
  be one that something the city builds can get to over the land; a city
  taken forgets its old holder's rally point. What a turn's start brings
  (the marches, the units built, what they take) is that turn's in the
  history.
- **Done for the turn** (built): D, or the Done button (`Order::Done`),
  puts the selected units aside: Next stack passes them by, as if they had
  no moves left, and the selection goes on to the next stack that waits.
  The next turn they are back.
- **A menu bar** (built, the owner's word 2026-11-20: "start cleaning up the
  interface"): Game, Units, Cities, View and Help, each item with its keys
  (every one but the help's topics has one), so the sidebar keeps only End
  turn, Undo and the city. The treasury sits at the bar's right end ("34
  +40 -32"). The stack under the map is a box for its leader and one for
  each of the eight units a hex holds, empty ones dimmed, with Go to, Done
  and Next beside them. One's own city on the selected hex is in full in
  the sidebar (what it builds, where its new units go; its window has more
  room), another's as it shows from outside. In hotseat each person's turn
  starts at their capital (`Game::capital`: the side's first city in the
  map's order). The Help menu has the rules of the game in play
  (`guide`): battles as this game was set up, what each of the map's kinds
  of unit pays to move, cities and gold, units and captains, orders and
  turns, what one sees.
- **A list of tasks** (built): T, or the Tasks button, opens what the
  player has left to do this turn (`Game::tasks`), most pressing first:
  stacks that have had no orders (or whose march stopped short) and cities
  that build nothing; then stacks that can go further, or were put aside
  with moves left, and cities whose work is done and waits (for gold, or
  for room); then the rest (marching, spent, building). A row is a button:
  a stack's selects it, a city's opens it; the list follows the game, so
  the player can work down it.
- **Production that repeats** until told otherwise (it does), and the lists
  of cities to see and change all of it in one place (they do).
- **The next stack with points left**, a key away (built: N, and the Next
  stack button; C and Next city for the cities); End turn could warn of
  stacks that have done nothing.
- **A log** (built): every battle a side fought, attacking or attacked,
  with who won and who died; the cities it took and lost; the units that
  left it unpaid; the sides out. The game keeps the history
  (`Game::history`); the log is what it says to the side at the screen.
  End turn opens it, and while the others play (the computer's turns a
  little apart, `COMPUTER_TURN_MS`) it fills, what is new in a color of its
  own, a line on top saying whose turn it is; when the person's turn comes,
  the button says Start turn, which closes it. (Fog of war will decide what
  more a side may hear.)
- **Battles watched** (built): a battle is played out in a window, both
  stacks with each unit's strength and where its bonuses come from, then
  the exchanges a few a second (who fights lit, who is hit in red with an
  X, the dead dimmed). It opens on the person's own attack; their log has
  a Watch button for each battle they fought, so the computer's attacks
  and, in hotseat, another person's can be watched too. The game keeps
  each battle's fight (who fought with what, each exchange, `Game::fight`)
  for `REPLAY_ROUNDS` (5) rounds; a battle is deterministic, but the fight
  is kept as it happened rather than fought again.
- **What a city can build, before it is built** (built): the city's window
  has a button for each, with the unit's icon, name, strength and moves,
  and all about the one under the pointer (turns, price, upkeep, what each
  terrain costs it).
- **Mistakes cost little** (built): a click that is not a move drops the
  selection, Esc drops it too, a drag moves the map rather than a unit, and
  the last order can be taken back (`Order::Undo`), unless chance had a say
  in it (a battle) or it put a side out.
- **Sub-orders** (decided: wanted, can wait): what a stack does when
  something happens in the turn (an enemy comes in sight, it is attacked,
  a city it passes is empty): attack, stop, go round, carry on; with a
  default for each, so that most stacks never need one.

## Multiplayer and the AI

- Hotseat first, in both modes: one screen, the players take turns at it.
- Network later, on the orders: turns send each order as it is given;
  rounds send a side's orders once a round. Since the rules are
  deterministic, every machine computes the same game (lockstep), and a
  replay is the orders again.
- The AI makes orders like a player (an agent, `docs/agents.md`). The first
  one, `simple`, builds what gives the most strength for its time, keeps
  two units in each city, and attacks the nearest cities that are not its
  own when the odds are good, gathering its units first. Agents play without a window, against each other,
  to try the rules and balance the numbers (decided): keep that working for
  every rule that is added.

## The order of work (a proposal)

Each step ends playable, with tests and screenshots, and the next builds on
it; the rules are written once for both modes from the first step.

1. **A new game, players and turns.** A screen to start a game: the map,
   the mode, who plays each side. Whose turn it is; only that side's units
   move and its cities build; End turn passes to the next side; hotseat's
   hiding screen. The session gives orders instead of changing the map.
   Taking empty cities, a side out, the winner; agents and games without a
   window. **Done (2026-11-19).**
2. **Stacks, battles and commanders.** Stacks move together; moving into an
   enemy is a battle (turns), with chance in it; units die; cities are
   taken; a commander leads a stack. **Done (2026-11-19).**
3. **Gold.** Income, the cities' prices, upkeep; a treasury in the sidebar.
   **Done (2026-11-19).**
4. **The end, and an AI.** A side that has lost its cities is out; the last
   one wins; a first AI.
5. **Fog of war.** **Done (2026-11-19)**, with orders that last and the
   tasks.
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

7. **The order of turns** is a choice when the game is set up: always the
   same, or drawn anew after every round.
8. **Sub-orders** for what happens in a turn are wanted, but can wait; what
   matters more is an interface that is easy, with good defaults (see
   "Easy to play").
9. **A side that is out** loses its units, for now; the code keeps the one
   place where that happens (`WhenOut`, `Game::put_out`), so that other
   ways can be tried: they join the side that took the last city, they
   fight on as mercenaries, they have a few turns to take a city back.
10. **Agents play the game** without a window, so that it can be balanced
    by playing it many times; that is kept working as the rules grow.
11. **Combat is to be experimented with**: a setting from no chance at all
    to nothing but chance, and more than one system (lines of duels,
    strength against strength).
12. **The chance a game starts with is 5**, until play says otherwise.

## Still open

1. Rounds: what happens when two stacks meet halfway, and the other cases
   of the resolution; a sketch is above, to be tried once the battles exist.
2. Combat: which system a game starts with (lines for now). Agents
   against agents without a window can compare them (`--battles`, with
   many seeds).
