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
  what is left to do; a stack patrols waypoints, or is handed to the
  computer to maraud;
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
     in forests, 2 on mountains, and in a city by its walls
     (`combat::walls_bonus`, the owner's table of 2026-11-20): nothing
     behind walls 0, 1 behind 1 or 2, 2 behind 3 to 5, 3 behind 6 (every
     city of our maps starts with walls 1 at least; before, three quarters of
     a point a level: behind walls 6, a couple of cheap units were a meat
     grinder); every unit of a stack with a commander is as much stronger
     as the commander says (`commander 1`). The attackers get no ground and
     no walls. A kind can be at home on some ground (`bonus forest 1`, or
     in a city: the owner's word 2026-11-21, gaps to fill), and fights that
     much stronger in a battle there, attacking or defending; a siege engine
     (`siege 2`) makes the walls of a city its stack attacks count that much
     less (`Game::siege_of`; never below nothing). (A battle's strengths are
     in quarter points, `combat::QUARTER`.)
   - **The odds** before an attack (`Game::battle_odds`) are 200 battles
     fought with a chance of their own, so asking does not change the game;
     the line under the map shows them ("attack: 82% likely to win").
   - The tests check the odds over many seeds (the stronger mostly wins,
     the weaker sometimes, at the middle settings; never at 0), and that one
     seed always gives the same battle.
5. **Cities change hands.** A city is taken by moving into it when no enemy
   stands in it (after a battle, or empty). A neutral city has a small
   garrison of its own (a generated map's: one of the weakest it builds).
   **Walls** (built, the owner's rules 2026-11-20): a city that is taken
   loses a level of its walls; its holder can raise them a level at a time
   (`Order::Walls`, the button in the city's window), once a turn, up to 6
   (`WALLS_CAP`), paid at once: 25 gold for each level it raises them to
   (to 3, 75; to 6, 150: `wall_price`). The log says when walls fall and
   when they are raised.
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
   unpaid. A unit's upkeep is half its price in the game's own units (the
   owner, 2026-11-20). A side can **disband** its units (`Order::Disband`;
   X, or Disband in the Units menu, which asks first): they are gone, and
   their upkeep with them, to make room for better ones; a ship at sea goes
   only with those it carries, or with ships enough left to carry them.
   Still to try: gold from a city taken, a tax rate to set.
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
   - **Blessings** (the owner's rules, 2026-11-20): a stack on a temple
     seeks its blessing (`Order::Bless`; a person is asked, for the whole
     stack, when it goes onto one, or presses B): each unit the temple has
     not blessed is blessed, +1 strength or +2 moves a turn (chance says
     which), and its movement is spent: it stops there for the turn. One
     the temple has blessed keeps its movement. Each temple blesses a unit
     once; each other temple blesses it again, and the blessings add up.
     Going onto a temple blesses nobody. A blessing cannot be undone. (The
     continent has a temple near each side's first city.)
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
   but no units, and a city in the colors of who held it when the side saw
   it last (`Sight::holder`; nobody's, if it never did; a side is told who
   took a city of its own); on the map, the minimap and the lists of
   cities. It is worked out anew whenever the map changes. With nobody at
   the screen (the computer against itself) all is seen.
   **Total fog of war** (`Fog::Total`, `--total-fog`; the owner's word,
   2026-11-20) is the same, and of the land a side has never seen, nothing
   at all: it is black, on the map and the minimap, and its cities are not
   in the lists, until a unit or a city of the side sees it; then it is
   dimmed as above when it is out of sight (`Sight::knows`). Nobody of the
   side can go there, nor find a way through it: a move, Go to, a march and
   a city's rally go only over land the side has seen (`game::UNSEEN` says
   why an order to it is refused), so units explore by going to the edge of
   what they know and seeing on (the owner: "the safe bet is slow
   explore"). A move that showed land never seen cannot be undone.
   The rules go by what the moving side knows (the owner's word, 2026-11-20:
   "giving orders into fog should not be effected by units in the fog"):
   the reach of a selection, an order, a march are worked out as if no unit
   were where the side does not see (`Game::known_range`); units of another
   side met on the way, unseen, stop the move there, in a battle
   (`Event::Surprise`, and a line in the log). What a side's units see on
   their way stays in its sight until its turn ends (its trail); and it
   remembers the other sides' stacks it saw where it saw them last (where
   one went out of its sight, if it saw it go), until the hex is in sight
   again (`Game::ghosts`): the map shows a faint banner and the pips there,
   the line under the map how many and in which round. A move that showed
   what was not seen before (another side's units, a city's holder) cannot
   be undone. Every side has its own trail and memory, and the computer's
   players are shown theirs in their view (`Game::view_for`), no other's.
   While another side plays, the screen only looks: a click selects
   nothing, Next stack and the lists of cities are the person's own, and
   their city's buttons give no orders. Rounds need fog far more than turns
   do.
10. **Saved games.** The state and the orders, in text, like the maps.
11. **Movement points.** A unit that does not use all its points keeps up
    to 2 of them for the next turn (decided: as now, from the C++).
12. **Ships and flyers** (built; the owner's word, 2026-11-20). A kind of
    unit travels by land, by sea or through the air (`travels` in
    `units.txt`). A **ship** goes over water and into its own side's cities
    (its ports), and carries land units: as many as its kind says, no more
    than a stack, and a commander besides, who takes no place aboard. A ship
    takes a place in its stack like any unit. A land unit steps aboard a ship
    of its side that has room, from the shore or a port, for what a step into
    a city costs it, and its move ends there; the ship takes those aboard
    where it goes and they pay nothing; they land on their own points
    (`Game::group` says who goes with whom). A stack at sea is selected
    whole. No move leaves anyone in the water without a ship (or a commander
    without a ship or flyers), and none takes more aboard than the ships
    carry. At sea the ships and the flyers fight; who rides does not, and
    goes down with the ships that sink (beyond what the ships left carry,
    the last aboard first: `Event::Drowned`); a commander still leads. In a
    port ships do not fight, and are lost if it falls. A **flyer** goes
    wherever its prices let it (the continent's cross water and mountains
    at a flat price); a commander that goes with flyers and no one else
    flies with them, at the first flyer's prices, and may stand over water
    with them. The computer builds no ships yet (carrying an army by sea is
    planning of its own, to come), and values a flyer's moves above a
    walker's.

## Turns

- The sides play in an order, chosen when the game is set up (decided):
  always the order of the seats, which the new game screen shows, or one
  drawn at the start and again after every round. A **round** is every side
  once. (Built.)
- A side's turn begins with its cities' work and its gold: income, upkeep,
  production. (Built for production.)
- Moving into an enemy stack or city is an attack: the battle is fought then
  and there, and shown. An attack needs 2 movement points left beside the
  units attacked (the step into them costs 2, `ATTACK_COST`, on any ground
  the attackers could go onto), and it uses up the attackers' movement for
  the turn, won or lost (the owner's rules, 2026-11-20). (Built.)
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
  hex: `Order::GoTo`; or a click on a hex beyond the selection's reach
  this turn, the owner's word 2026-11-21, which says why not if they
  cannot go there and leaves them selected; a hex with the player's own
  stack selects that instead) sends the selected units to a far hex: they take this
  turn's part at once, and the rest at the end of each of their side's
  turns, with the movement they have left (`Game::marches`; the owner's
  word 2026-11-21: in the turn their player can still give them other
  orders, in a pinch, though they need none), as far as the slowest of
  them goes, until they are there. A city's new units, sent somewhere,
  wait in it through the turn they are built, and go at its end. It is taken if each unit could get
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
- **Routes** (built; the owner, 2026-11-21: "march to a temple, get a
  blessing, then march to a rally point"): a standing order is a list of
  stops (`Unit::route`; `Order::GoTo { to, via }`). In Go to, Shift+click
  (or Shift+Enter) adds a stop and a plain click picks where they end; each
  stop is checked as it is picked (`Game::check_route`: every leg one the
  units can make over the land the side knows). At a stop the stack goes
  on to the next at once, with what movement it has left; at a stop on a
  temple, those of it the temple has not blessed seek its blessing and the
  stack stops there for the turn (one it has blessed them all passes on).
  More than one leg in a turn, or a blessing, cannot be undone. Where a
  city sends its new units can be a route too (`Sends::Route`, Shift+click
  after Send to, `Game::check_rally`): what it builds goes by way of the
  stops.
- **Where a city's new units go** (built; `City::sends`, `Order::Rally`):
  they stay in it and wait for orders (Keep here, at first); they stay
  and defend it (Defend here, the owner's word 2026-11-21: a garrison
  nobody need see to); they go somewhere along a route (Send to..., then
  a hex); or they go to one of the side's rally points (its button, or
  Send to... and a click on its flag), by way of a temple near the way
  first if the city says so (the box "A temple's blessing on the way"):
  the temple, of those the side knows that have not blessed the unit,
  that adds the least to its way, if that is no more than a turn's march
  (`Game::temple_on_the_way`; one far off the way is passed by, so the
  player ticks the box where a temple is handy). A unit is sent the
  moment it is done. A route's hexes have to be ones that something the
  city builds can get to over the land; a city taken forgets where its
  old holder sent its units. What a turn's start brings (the marches, the
  units built, what they take) is that turn's in the history.
- **Rally points** (built, the owner's word 2026-11-21: gathering new
  units into armies is the drudgery, moving armies and taking cities the
  fun): each side has ten, 0 to 9 (`Game::rally_points`,
  `Order::RallyPoint`), its own (another side's view has none of them),
  placed on land it has seen: R, or the Units menu's Rally points, places,
  moves, looks at and removes them; a numbered flag shows each on the
  map. Units sent to one (by Go to onto its flag, or by a city) are bound
  to it (`Unit::bound`): if it moves they go to its new place; if it is
  removed they stop and wait for orders, and the cities that sent their
  units there keep them. Once it is full (`MAX_STACK`), those that come
  after stand beside it, on the nearest hex around it with room (out to
  three hexes, in a fixed order, so that one stack fills before the
  next). A point can say "Defend there": those who get there defend it
  (Next stack and the tasks pass them by, as with F) until they are sent
  on; else they wait for orders. Selecting a stack there takes them all,
  defenders too.
- **Done for the turn** (built): D, or the Done button (`Order::Done`),
  puts the selected units aside: Next stack passes them by, as if they had
  no moves left, and the selection goes on to the next stack that waits.
  The next turn they are back.
- **Defending** (built, the owner's word 2026-11-21): F, or Defend here in
  the Units menu (`Order::Defend`), marks the selected units as defenders:
  they stay put turn after turn, Next stack passes them by, and the tasks
  list them last ("defending: stays put"), until they are sent somewhere (a move or
  Go to ends it) or F stands them down. A unit's box says it defends.
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
  that build nothing; then cities whose work is done and waits (for gold,
  or for room); then the rest (marching, spent, building, and the stacks
  the player is done with this turn: put aside, or moved). A stack the
  player moved and that stopped has had its orders, moves left or not:
  Next stack passes it by, as it does one put aside, and a click on its
  hex picks the units that have not moved first. A stack that got where
  a standing order sent it, at the end of the last turn, waits for orders
  again; so does one whose march stopped short then (it stays stopped
  until it gets on). A row is a button:
  a stack's selects it, a city's opens it; the list follows the game, so
  the player can work down it.
- **Production that repeats** until told otherwise (it does), and the lists
  of cities to see and change all of it in one place (they do).
- **The next stack that waits for orders**, a key away (built: N, and the
  Next stack button; C and Next city for the cities): the nearest to the
  stack selected (or the last one Next went to) that it has not gone to
  since it last went round them all, so that N works outward from where
  the player is and never goes back and forth between two; Shift+N goes
  back the way it came.
- **Options** (built, the owner's word 2026-11-21: the turn should not be
  burdensome, nor leave out details): the Game menu's Options (Cmd+, or
  Ctrl+,) are the person's, not rules: they change what the screen does
  for them, never what an order does (`options_window::Options`). The
  first selects the next stack that waits by itself: when a turn starts
  (the nearest to the cursor) and after each move or battle, as Done and
  Defend do (on at first). End turn counts what waits for orders ("End
  turn (3)": the tier-1 tasks), and, as the second option says (on at
  first), asks before the turn ends while anything does, or while the
  gold and the taxes will not pay next turn's upkeep (units would leave);
  its window can show the tasks. The third tells a battle of the
  person's in a line under the stack instead of playing it out in its
  window (the log's Watch plays it); the fourth has a stack moved onto a
  temple seek its blessing unasked, and says in that line what each was
  given. The shell carries them from one game to the next.
- **A log** (built): every battle a side fought, attacking or attacked,
  with who won and who died; the cities it took and lost; the units that
  left it unpaid; the sides out. The game keeps the history
  (`Game::history`); the log is what it says to the side at the screen.
  When the person's turn comes it opens if the others did something to
  them, or something went wrong at the start of their turn (a march
  stopped short, units left unpaid), what is new in a color of its own;
  the button says Start turn, which closes it (so does any order). With
  nothing to tell, the turn starts at once (the owner's word 2026-11-21).
  While the others play (the computer's turns a little apart,
  `COMPUTER_TURN_MS`) the button says whose turn it waits for; L opens the
  log, which fills as they play. A person put out of the game is told in
  their log, once.
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
- **Mistakes cost little** (built): Esc drops the selection (a click on
  the selected stack keeps it; a click on another of the player's stacks
  selects that one: the review of 2026-11-21, since the screen selects
  stacks by itself; stacks join by Go to), a drag moves the map rather
  than a unit, a click beyond the selection's reach (a Go
  to) can be taken back until the stack has gone more than a leg, and the
  last order can be taken back (`Order::Undo`), unless chance had a say in
  it (a battle) or it put a side out.
- **Sub-orders** (decided: wanted, can wait): what a stack does when
  something happens in the turn (an enemy comes in sight, it is attacked,
  a city it passes is empty): attack, stop, go round, carry on; with a
  default for each, so that most stacks never need one.
- **A production queue** (built, the owner's word 2026-11-21): a city
  builds a list (`City::queue`, `Order::Queue`, eight items at most), each
  item so many of a kind and where they go (`Sends`: keep, defend, a route,
  a rally point with or without a blessing on the way): "3 Light Infantry,
  defending; Hawk Riders to rally point 0 by a temple; Cavalry to rally
  point 1". After the last item it stops, or goes back to an item and on
  again, for ever or so many times (`QueueEnd`). A city whose queue ran
  out builds nothing and is a task that needs orders; Nothing (`Order::Build`
  with nothing) is building nothing on purpose (`City::resting`), and is
  no task (late in a game, a city left alone). Build takes the queue away
  (Undo gives it back). A queue changed goes on from the item it was
  building if that is still there. The city's window has two columns: what
  it can build (a + beside each adds one to the queue) and all about the
  one under the pointer; the queue (an item a row, the one building marked,
  one more, one less, out; Then: Stop, or Repeat from the chosen one, and
  how many times) and where the units go. While it has a queue, the
  window's buttons for what it builds and where the units go are about the
  item chosen (the new one, after a +); the sidebar's are about the item it
  builds now.
- **Patrols** (built, the owner's word 2026-11-21: excess troops kept
  useful, and eyes open in the fog): P, or the Units menu's Patrol
  (`Order::Patrol`), then the waypoints, from where the stack stands, as
  Go to picks its stops: Shift+click each (outlined in gold, each checked
  as it is picked, `Game::check_waypoints`: a way over the land the side
  knows, and no city but its own) and a plain click the last; one plain
  click: between here and there. The last on the stack's own hex (the
  first again): round and round; else back and forth along them
  (`state::Patrol`). A small window then asks whether it attacks: never (a
  scout keeps out of fights), or at 90, 75, 60 or 50 in a hundred or
  better (the choice made last is pressed at first); Patrol (or Enter)
  gives the order (the owner's word 2026-11-21: the window wants an OK).
  While it asks the stack is not selected, so that a click or Enter on the
  map gives it no order; Never mind (or Esc) selects it again. A stack on
  patrol or marauding that is selected is not sent elsewhere by one stray
  click: a click that would give it an order (and so end its duty) says
  so, and a second click on the same hex sends it. It takes this turn's
  part at once, and goes on at the end of each of its side's turns, after
  the marches, with the movement it has left (`Game::patrols`): as far as the slowest of it gets,
  on from a waypoint at once with what movement is left (a short patrol
  may go round more than once a turn), never into another side's units it
  sees; with no way on it stops short, waits for orders, and tries again
  next turn. If it attacks: before it steps on, and where its step ends,
  of the other sides' stacks (not nobody's) it sees and can reach now,
  the one it is likeliest to beat, if the odds (`Game::battle_odds`) are
  its least or better; once a turn at most (an attack spends its
  movement); in a city too, which it takes if it wins. One met unseen in
  the fog is a battle there, as for any move. A patrol needs nobody to see
  to it: Next stack passes it by, the tasks list it with the rest
  ("patrolling 3 waypoints, attacks at 75% or better"); a click on it
  selects all of it, and the map outlines its waypoints. Any other order
  for it ends the patrol (as a move ends a Go to; Undo gives it back where
  it takes back a move), and a patrol that has gone no more than a leg
  can be taken back.
- **Marauders** (built, the owner's word 2026-11-21): U, or the Units
  menu's Maraud (`Order::Maraud`), hands the selected stack to the
  computer until its player calls it back (U again) or gives it another
  order. Marauding is not a rule: the rules keep the flag, and the
  computer (`marauder`) gives the stack ordinary orders, at once with what
  movement it has, and at the end of each of its side's turns (the screen
  leads a person's as they end it, hotseat too; an agent's turn leads the
  computer's sides' and a program's after its orders). It is shown the side's
  view, no more, and leads the stack in the bold planner's ways, scoped to
  it: it strikes the other sides' stacks it sees and can reach at the bold
  odds or better, and the cities it can reach that are empty or held by
  few enough (a city first); else it goes toward the city most worth the
  trouble that it could take (one in the fog, to look), or a stack it
  could beat; else, under total fog, to see more; else it waits near the
  nearest city not its side's. It goes as one stack, to a step every unit
  of it can reach, as far as the slowest gets. Its moves are `Order::Roam`:
  a move that keeps the stack marauding (a person's move ends it, so the
  two differ in the order). Its player sets no odds for it: the bold
  ones.
- **After a battle** (patrols and marauders alike, the owner's word
  2026-11-21: "to make sure the user still wants that, or it may be too
  damaged to continue"): a stack on either that fought, attacking, or
  attacked and holding (`Unit::fought`), waits for its player's word: it
  needs orders (tier 1: "fought on patrol: D lets it go on"), Next stack
  goes to it, End turn counts it. Done lets it go on, at once if it can;
  another order ends its patrol or its marauding; left alone, it goes on
  at the end of the turn. One attacked in another side's turn waits
  through its own: its player sees it first. A battle one fights on its
  player's word (the patrol's window, U, D) is told as the player's own;
  those at the end of a turn are in the log, which opens as the next
  begins.
- **Provinces** (shelved, the owner's word 2026-11-21): a group of cities
  handed to the computer under the player's general orders (a share of
  their taxes, units sent to the front). Shelved: multiplayer is meant to
  be people against people, and a province run by the computer blurs
  that; and production queues, rally points, patrols and marauders get
  most of the way there without it. If it comes back it needs more to it
  (an empire forced to break off provinces past a size, provinces that
  rebel or go over to an opponent), and it would be a choice made with
  the game, since it changes the game.

## Multiplayer and the AI

- Hotseat first, in both modes: one screen, the players take turns at it.
- Network later, on the orders: turns send each order as it is given;
  rounds send a side's orders once a round. Since the rules are
  deterministic, every machine computes the same game (lockstep), and a
  replay is the orders again.
- The AI makes orders like a player (an agent, `docs/agents.md`). The first
  one, `simple`, builds what gives the most strength for its time, keeps
  two units in each city, and attacks the nearest cities that are not its
  own when the odds are good, gathering its units first. The planner
  (built, the owner's word 2026-11-20: "a better AI player, possibly
  several flavors"), in three styles, `steady` (the default), `bold` and
  `careful`, reads the rules in effect: what a unit is worth under the
  chance and the system, what threatens its cities, the odds of each
  attack; tournaments chose its numbers. With fog of war every agent is
  shown only what its side sees (`Game::view_for`), as a person would be. Agents play without a window, against each other,
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
