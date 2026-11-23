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
- **a new game screen**: the map, the mode (turns or rounds), the
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

- **rounds** (below, "Rounds"): every side gives its orders, then the
  round plays out on a clock, the faster units first, on the turns' own
  rules.

- **saved games** (below, "Saved games"): a game that is not over is kept
  in a file as it goes, and played on later from the new game screen.

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
     after the battle, unless wounds last. (In integers: `k` is one of the
     two whole numbers around it, drawn in the share that makes the odds
     right, so every machine fights the same battle.) Before (2026-11-19,
     first version): each side scored its strength plus luck, and the steep
     edge that gave the stronger, with walls in full, made cities near
     fortresses (a full stack of good units took a walled neutral city one
     time in eight).
   - **Wounds that last** (built, a choice made with the game, off unless
     chosen: the new game screen's Wounds last, `--wounds`; the owner's
     word 2026-11-21): a unit's hits stay with it (`Unit::wounds`) until
     its side's turn ends, when its units heal (`Game::heal`; the units of
     a side with no turn, nobody's garrisons, heal as a round ends), so a stack
     attacked in waves in the others' turns is worn down, and stays
     battered through its own turn; and a wounded unit fights at half its
     own strength, rounded up (`combat::wounded`: 5 fights as 3; the
     bonuses for its stack, the ground and the walls stay whole), from its
     first hit, in the battle it was hit in too. A battle's odds and the
     agents count the wounds (`Unit::fighting_power`). Measured (100 seeds of the continent, each
     way): the simple agents' games hardly change in either system; the
     planners' games in strength against strength last a little longer
     (35.5 rounds to 37.2) with more cities taken and taken back (15.8 to
     16.8, 8.1 to 9.1), and in lines hardly change. The agents do not
     attack in waves to wear a stack down; and a stack that wins strength
     against strength loses about 4 in 100 of its units, so it has little
     to carry over. What makes strength against strength one-sided is the
     exchange (whole stack against whole stack: the bigger wins nearly
     every one), not the healing (still open).
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
     - *Dice* (`dice`, the owner's design 2026-11-21, to try): each
       exchange the two stacks' whole strengths are brought to numbers from
       1 to 9 (`combat::dice_numbers`: as they are when neither is over 9;
       else the greater to 9 and the other in the same measure, to the
       nearest, never under 1), and each side rolls a ten-sided die: the
       side whose roll is under its number while the other's is over wins
       the exchange (both under or both over: they roll again; drawn at once
       as their odds, `p (10 - q) : q (10 - p)`), and the loser takes the
       hit on one of its units drawn by chance, as by strength. The game's
       chance plays no part. A roll of exactly a side's number (the owner's
       three, `combat::Exact`, chosen with the game, `--dice-exact`): it
       wins (9 against 1, 99 in 100; 9 against 8, 69; even, even), it
       misses (a 1 never wins a roll), or it wins for the defenders only
       (even numbers are theirs: 5 against 5, 60 in 100; 9 against 9, 69).
   - **Losers hit back** (built, a choice made with the game, off unless
     chosen: the new game screen's Losers hit back, `--blows-back`; the
     owner's word 2026-11-21: "the strong stack taking some hits is
     probably the right idea but we also dont want a small or weak stack
     leaving a strong stack in ruins", and, since an even battle would
     have its loser hit back every other exchange, at most a quarter of the
     time): in any system, after an exchange the loser hits back with a
     chance of its share of the strengths that fought (in lines, the two at
     the front), at most `BLOW_BACK_MOST` (25) in 100, and one of the
     winner's units takes a hit (in lines, the one at the front); not if the
     exchange left the loser with nobody. A weak stack has few hits to
     spend and a small share, so it lands few: a pair of 2s against eight
     5s, never a death in 20,000 battles. The battle window shows each
     ("Blue hits back.").
   - **Measured** (2026-11-21; single battles, 20,000 each, at chance 5:
     the winner's units lost when the attackers win, of 8):

     | Battle | Lines | Strength | + hit back | Dice | + hit back |
     |---|---|---|---|---|---|
     | 8×5 against a pair of 2s | 0.16 | 0 | 0 | 0 | 0 |
     | 8×5 against 8×2 | 1.05 | 0.12 | 0.92 | 0.03 | 0.67 |
     | 8×3 against 8×3 (wins half) | 5.5 | 1.8 | 3.1 | 0.8 | 2.1 |
     | 8×4 against 8×3: wins | 95% | 90% | 86% | 99% | 98% |
     | 3×6 against 8×2: wins | 99% | 14% | 3% | 48% | 13% |

     On the dice the greater side is always a 9 and misses one roll in
     ten, so a lead runs away: an even battle ends one-sided, and quality
     counts more than by strength (three 6s against eight 2s, about even;
     lines 99 in 100, strength 14). A roll of exactly the number winning for
     the defenders only makes an even battle the attackers' 9 times in 100.
     Whole games (the continent, the steady planners, 100 seeds each; with
     wounds that last in brackets): by strength, the winner of a battle
     loses 5 in 100 of its units (4), with losers hitting back 11 (9); cities
     taken 14.8 a game (15.3), with hitting back 17.4 (18.0), and taken back
     7.5 (7.8) and 9.9 (10.3); 33.6 rounds (35.0), with hitting back 34.9
     (39.4). On the dice: the winner loses 2 in 100 (1), with hitting back
     6 (6); 31.6 rounds, 2 games of 100 unended after 200 (exact rolls
     missing: 35.5 rounds, all ended; winning for the defenders: 41.8, 8
     unended). In lines, hitting back shortens the games (49.8 rounds to
     33.1): the strong unit at the front is worn down by the weak ones
     before it.
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
   - **A cap** on a unit's strength (built, a choice made with the game,
     none unless chosen: the new game screen's Cap 9, `--cap <n>`; the
     owner's idea 2026-11-21, "cap each individual unit (with bonusses) to 9
     in strength ... That might control stacks a bit but still allow a strong
     stack to shine"): no unit fights stronger than the cap, all its bonuses
     counted (`combat::Rules::capped`; wounded, what is left of it, no more
     than the cap either). The odds, the planners' weighing of a unit and
     the battle window ("9 (cap)") count it. Measured (the continent, the
     steady planners, 100 seeds, strength against strength with losers
     hitting back and wounds that last): its units are 2 to 6, and with
     every bonus a unit fought over 9 one time in a thousand (by a point), so
     a cap of 9 changes nothing there (39.4 rounds, 38.9 with it; the
     winner of a battle loses 9 in 100 of its units either way); caps of 8
     and 7 hardly more (36.2 and 39.0 rounds; 10 in 100). It is for maps
     whose kinds are stronger.
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
   (`Event::Surprise`, and a line in the log). A side sees what its units
   and cities see where they are, and no more (the owner's word,
   2026-11-22, in both modes: "you should only see what the unit can see
   at the time and leave ghosts vs leave a lingering visibility trail";
   until then what units saw on their way stayed in sight until the turn
   ended, and through the next for standing orders): as a stack is shown
   going, what it sees goes with it (`Showing::sight`), and what it has
   passed is dimmed again; what was there is noted as it passes
   (`Game::note_sightings`), so a side
   remembers the other sides' stacks it saw where it saw them last (where
   one went out of its sight, if it saw it go), until the hex is in sight
   again (`Game::ghosts`): the map shows a faint banner and the pips there,
   the line under the map how many and in which round. A move that showed
   what was not seen before (another side's units, a city's holder) cannot
   be undone. Every side has its own memory, and the computer's
   players are shown theirs in their view (`Game::view_for`), no other's.
   While another side plays, the screen only looks: a click selects
   nothing, Next stack and the lists of cities are the person's own, and
   their city's buttons give no orders. Rounds need fog far more than turns
   do.
10. **Saved games.** The map, the setup and the orders, in text, like the
    maps (built: "Saved games", below; the state itself is not written,
    the orders make it again).
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
    with them. **A stack of flyers and units that walk stays together**
    (the owner's word, 2026-11-22: "a mixed stack should always stay and
    move together even with flyers and non-flyers"; the flyers of one went
    over the water alone, the way being theirs): units that go at
    different prices take a way all of them can go
    (`Game::ground_together`: a hex shut to one is shut to all, and the
    way is the quickest for the slowest of them), each paying its own
    price for it, and they go as one or not at all, on a march, a patrol
    and in a round alike. Flyers that are to cross the water are sent
    without those that cannot. The computer builds no ships yet (carrying an army by sea is
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

Built (2026-11-22, the owner's word: "implement the round (simultaneous
turns) feature"; `src/game/rounds.rs`, chosen on the new game screen or
with `--mode rounds`). The rules are the turns' own, applied at another
moment; what the sketch left open is decided as below, to be played and
changed.

- **Planning.** The sides give their orders one after the other (who goes
  first learns nothing by it: nothing moves, and nobody sees another's
  plans). What moves no unit is carried out at once: what a city builds,
  its queue, where it sends its units, its walls, a rally point, Defend,
  Done, a blessing sought, a unit sent home. An order to move is a
  **plan**: `Order::Move`, `Toward` and `Roam` give the units a route to
  that hex for this round (`Unit::planned`: refused as a move is, if they
  cannot get there this turn as things stand, or there is no room for
  them there among those of their side that stay) and **the way they will
  take** (`Unit::way`: the way the side knows when it gives the order,
  round the stacks it sees; the map outlines it); `GoTo` and `Patrol` are
  standing orders as in turns, and take their first step with the round.
  Another plan takes a plan's place; `Order::Undo` takes the last back.
  A side says it has done with `Order::EndTurn` ("Orders done").
- **The round plays out** once every side has done, on a clock
  (`Game::play_out`; the owner's word after playing it, 2026-11-22: "speed
  will very much be a factor in these games. Racing to reinforce a city
  before an attack or racing to take a weakened enemy position before
  reinforcements will matter. So fast units should get to spend the same
  MP as slower units faster than them. Also, combat should probably happen
  on any given subtick"; until then every stack went a hex a tick):
  - A step takes a unit the share of a round that its cost is of the
    unit's movement a turn (`Unit::speed`, not counting points kept from
    the turn before; `rounds::ROUND` is a round on the clock). Riders with
    20 points are three hexes of grass on (6 points) when soldiers with
    12 are two. A stack goes at the pace of its slowest: a step takes it
    as long as it takes the one of them it takes longest; and it goes
    whole or not at all (a step one of it cannot pay for stops it for the
    round).
  - Each stack sets off on the next hex of its way, and is there when the
    step's time is up; the stacks get there in the order of the clock.
    Of those there at the same moment, the sides go in the round's
    order: the one thing the order decides. With the order drawn anew
    each round (the new game screen's choice) it is drawn as the round
    begins to play out, after every side's orders are given: nobody
    knows. (The orders are given in the order the round before was
    played in.)
  - A stack that gets to a hex another side stands on attacks it, there
    and then, wherever its way was taking it (the owner: "stacks that run
    into each other should probably fight vs just block each other. This
    will be brutal and will punish sending powerful units out alone"):
    its way is fixed when it sets off, and it does not go round who has
    come to stand on it, nor wait. So the faster of two stacks sent to a
    city is in it first, and the other attacks it there (the owner's
    example: cavalry three points from a city take it before heavy
    infantry three points from it get there, "leaving them attacking a
    city or backing off"); two stacks making for the same hex fight
    there, the one that got there first defending it (with the ground's
    bonus); two that cross fight on the hex of the one that is slower to
    leave; a stack sent against one that has left finds the hex empty,
    and takes it.
  - A march (Go to) finds its way as it sets off in the round, round the
    stacks it sees then (through them, if there is no way round), and
    fights who comes to stand on it as a plan does (in turns a march
    never attacks). A patrol goes round, as in turns, and attacks at its
    odds (it makes for the stack it would attack, a hex at a time, at its
    speed; with no step to take it rests a twelfth of a round and looks
    again).
  - **Joining.** Each stack attacks when it gets there: stacks sent
    against a hex each by its own order are so many attacks, one after
    the other. To go, and fight, as one, stacks join first (the owner:
    "If you want to consolidate stacks then you would order one stack to
    another then combine them and order them as a new stack. That would
    cause the stack being joined to wait for the other units before
    moving or attacking"): units sent to a hex where others of their side
    stand can be given an order with them (`Order::Move` naming all of
    them; `Game::joining` says who is on the way to a hex). They go there
    first, by the way they had; those that stand there wait for them (for
    as long as they can still come: one that is killed, fights or stops
    on its way is not waited for); then all go on as one stack, at the
    pace of its slowest, each with the movement its way there left it
    (where they can go together is worked out so: `Game::joint_range`).
    One `Order::Move` for units on several hexes that are not sent to
    join each other gathers them itself, on the one of their hexes
    nearest to where they go that all can get on from (`rounds::muster`:
    one order is one stack and one attack, as in turns; the agents order
    their attacks so); if there is none, each hex's units go on their own.
  - **Waypoints** (the owner, 2026-11-22: "we need to be able to use
    goto more than once. This will let setting waypoints to force a stack
    on a path we want and not waste mp/turns ... setting a goto point
    could leave the stack active with MP to spend from that point ...
    always start 'fresh' when a stack is selected"). A stack that has a
    plan can be sent on from where the plan ends (`Order::Onward`), to a
    hex all of it can reach from there with what the way so far leaves
    each: the plan goes by the first hex and on to the next, so the
    player says which way a stack takes, round a stack it would rather
    not meet or by the road, and loses no movement by it. On the screen
    a stack given a plan stays selected while it has movement left where
    the plan ends: the map marks where it can go on to from there, and
    the next click is a waypoint more (`Game::select_onward`). A click
    beyond that sends it there over the rounds, by way of the plan's
    stops (a march). A click on the stack itself, or selecting it again
    later, starts afresh: the next click is a new plan from where it
    stands. Undo takes back the last leg. A plan that ends in a battle or
    in a city to take has nothing after it (the battle ends the
    attackers' round); nor has one that ends on a temple that has yet to
    bless them, which stops them there for the round as any stop on one
    does: nothing is offered from there, and a leg after it is refused.
    Go to (G) for a stack so selected goes by the plan's stops too; a
    click on land never seen (total fog) is a new plan, to look. The
    status line counts a hex's cost of what the plan leaves ("2 of 6
    points to go on to here").
  - A stack passes through its own side's stacks that stay, as a move
    does: past a hex they leave it no room on, its step is to the next
    with room. Behind stacks of its side that are on their way themselves
    it waits until they have gone. (If nobody can step any more and some
    still wait, for others to join them or to get out of the way, they
    wait no longer.)
  - The units of one order go as a stack; those of another order on their
    hex, with the same end, go on their own (`Unit::party`).
  - A plan or a march that stops on a temple seeks its blessing there
    (nobody can be asked as the round plays out).
  - A battle ends the attackers' round: an attack uses up their movement.
    A plan ends with it, and with a city taken on the way.
- **The round ends** when nobody is on a step any more. The plans
  end, got there or not (a stack that did not get there waits for orders
  next round). The standing orders use what movement is left as at a
  turn's end, and say if they are stopped short. Wounds heal. Then the
  next round begins for every side together: its units' movement, its
  taxes, its upkeep, what its cities built (`Event::Began` names the side
  that gives its orders first).
- **What a side sees** (fog of war): what its units and cities see where
  they are, as in turns; the stacks its units passed as the round played
  out are remembered where they were seen (ghosts). (Until 2026-11-22
  what its units saw on their way stayed seen while the next orders were
  given.)
- **Into the dark** (total fog of war). Land a side has never seen is
  shut to it, as in turns: no unit steps onto it. But in turns a stack
  creeps, a step and a look at a time; a round's plan is made before
  anything is seen, and would end at the edge of the known land, two
  hexes on (found by playing it, 2026-11-22: the computer's sides never
  met). So in rounds a stack may be sent **toward** land its side has
  never seen (`Order::Toward`; at the screen, a click on the black): as
  the round plays out it goes over the land it has seen toward the hex
  nearest to where it was sent, sees more, and from there finds the
  way on (`rounds::way_toward`); each step is still onto land it has
  seen. `Move` and `GoTo` to unseen land are refused as ever.
- **At the screen.** A click plans: the stack stays, and the map outlines
  the ways the person's stacks will take this round (their own only). The
  End turn button reads "Orders done". A stack sent onto another of the
  person's (Join it) is then selected with that one, the units on their
  way there after those that stand there, marked `+` in their boxes (a
  box leaves one out, or takes it along); the reach shown is the joined
  stack's, from that hex; a click sends them on as one. The computer's
  sides give their orders at once; then the round plays out before the
  person, a **beat** at a time (`rounds::BEAT`, a sixth of a round, or up
  to a battle): the stacks that went in a beat go at the same time, each
  all the hexes it made in it, so the faster are seen to be faster; a
  battle of the person's is played out in its window as the round comes
  to it, and the round waits until the person closes the window (the
  owner's word, 2026-11-22: "the advancement should stop, resolve the
  combat, and let the user dismiss the box (if using it) before moving
  on. The user can miss details otherwise"; Close, Enter or Esc; so in
  turns too, for a battle the computer fights against the person; before,
  the game went on a moment after the battle was over). The map goes
  to the person's own stacks if none is in view, and back to where they
  were looking when it is over. (While a screen watches, the last side's
  `EndTurn` only begins the round and the screen plays the beats; without
  one it plays the round out whole. The same game either way.)
- **Finding things on a busy map** (the owner's notes from play,
  2026-11-22: "The screen gets very busy ... we will need to crack this or
  it may not be usable"; in turns too, where it applies). The hex of the
  selected stack (a city, if it is in one) is ringed green, over the
  cursor's red. Its own orders are ringed white, the other stacks'
  outlined as before; V (the View menu, Options) switches between every
  stack's orders and the selected stack's only. A stack with a commander
  shows the commander's sign, whoever of it is selected, and a commander
  in one of its side's cities has its pip ringed light. A battle shown as
  it happens rings its hex in orange, which flashes red while the battle
  is played out; in its window each stack stands the same way whatever
  order it was in and however the battle is fought, from its weakest to
  its strongest with its commander on the right (the owner, 2026-11-22;
  `battle_window::line_order`); the battle's window goes clear of the hex: to the first
  of its places that leaves the hex in sight with the map as it is (the
  top of the map, its bottom, its corners), the map moved under it only if
  none does, and at the map's edge or on a map all in view, where the map
  cannot move, to wherever the hex shows, over the sidebar if it must
  (the owner, 2026-11-22: "a battle at the top of the map has the battle
  hex covered by the combat dialog"). With battles told in a line
  (Options) the map goes to the hex all the same and it flashes a moment
  before the game goes on. The **minimap** marks each battle the person
  was in since they last ended a turn (what their log has as new: in
  rounds, the round that played out) with an orange cross, bigger while
  the battle is being shown; in the replay, those fought up to where it
  has got to. In the **stack's boxes** under the map a selected unit's
  box is lighter, inside a thick line of the selected hex's green (told
  by its weight, not its color alone), and the others' numbers are grey;
  the box of a unit that is not on the hex yet (in rounds: one sent to
  join the stack, marked +) is blue ("there now vs will be there soon").
- **What happened, again** (the owner, 2026-11-22: "It is also hard to
  follow all the action but this is the nature of everything happening at
  once. I am thinking that a 'replay' feature of the previous turn would
  help. Maybe a slider on the side bar that would allow the user to 'go
  back' in time and examine movement, battles, etc. You can not give
  orders when 'in the past'"). The screen keeps what happened since the
  person's last turn (`replay::Replay`: the game as it was, and what was
  done to it since): in rounds the round that played out, a beat a step;
  in turns everything from the end of their turn to the start of their
  next (their own marches as it ended, the others' turns), an order a
  step. Under the turn's button, once their turn has started: **Replay**
  (W) plays it again from its start, every move and battle shown as it
  was (pressed again, it stops there); the **slider** beside it goes back
  to any step of it (all the way right: now). In the past the map and the
  panels are the game as it was then: stacks and cities can be looked at,
  nothing can be ordered (the copy takes no orders: `Game::freeze`), the
  turn's button reads "Back to now" (Esc goes back too), and the game as
  it is, the orders already given this turn with it, is as it was left.
  (It works because the rules are deterministic: the steps done again to
  a copy make the same game. The orders are kept by the game itself,
  `Game::record`; a saved game is made of the same record: the setup,
  the map and the orders from the start, "Saved games" below.)
- **Hotseat**: each person plans in turn behind the hiding screen; the
  last of them watches the round play out, and each of the others is
  shown it as their next orders begin, once their log is out of the way
  (what is still there to be shown: their own stacks going, the others'
  where they saw them at the time, their battles, each held until its
  window is closed). A click on the map ends the moves shown (the battles
  still play), and nothing of it goes on under the next person's cover.
- **The agents**: the planners (`steady`, `bold`, `careful`) play rounds
  in a way of their own (`src/planner/rounds.rs`, `docs/agents.md`, "The
  planner in rounds"; the owner's word, 2026-11-22: "Round based games
  probably need their own AI"): they go by where the other sides' stacks
  could be this round and when, do not walk a stack into one it cannot
  beat, and attack as one stack (stacks join first for an assault). (Much
  of what was first made for rounds, they now do in turns too: setting
  out sooner, spending their gold, keeping clear of stacks they could not
  beat.) Their plans are made from what they see when they plan (a
  program is told `mode rounds`); to see more of the map they send a
  stack to the edge of the land they know, and on past it when its way
  there is by the edge (`planner::outward`). `simple` plays rounds as it
  plays turns. Measured (the continent, the steady planners on every
  seat, 40 seeds, 200 rounds at most): every game ends, in 61 rounds;
  with total fog too, in 88 (once, 13 of 40 did: sent past the edge, the
  armies never got to it). Against the planner playing rounds as it plays
  turns (`steady/turns`), two seats each, steady wins 261 games of 480.
- Open: whether a march should fight on its way as a plan does, or stop
  short as in turns (it fights: one rule for every stack on its way);
  whether a stack that would run into far worse odds should hold back;
  plans by way of hexes of the player's choosing.

## Saved games

The owner's word (2026-11-22): "save the current game's state in a file with
start conditions and logs to deterministically recreate the current game ...
a simple save game, when exiting make sure the current game is saved", and
"allow an in-progress game to be restarted or deleted from the new game
screen"; finished games need not be kept for now.

- **What a saved game is**: how the game began (the texts its map was read
  from, so that the map's files may change or go; the setup with its seed)
  and everything done to it since, in its order: the orders carried out,
  each beat of a round playing out, and the one thing done outside an order
  (the computer letting a side's marauders go on, `Game::stand_easy`).
  `Game::record` keeps them as `Step`s, `Game::redo` does one again. The
  rules are deterministic, so that is the game: to load it is to begin it
  again and do it all again. Nothing of the state is written but a number
  that says how it stood (`Game::digest`), so that a game that comes out
  otherwise is known: the rules have changed since it was saved, and it is
  not played on (the screen says so; the file stays until it is deleted).
  `docs/saves.md` is the file's format; `src/save.rs` reads and writes it.
- **When it is written**: by itself. A game gets a file when it starts
  (`<map>-<n>.save`), written again each time a turn passes (a side's, in
  either mode), when the game is left for a new one, and when the window
  closes (Ctrl+Q, the close button): with the orders given so far in the
  turn, and, in rounds, as far as the round has played out. There is no
  Save button and nothing to name. Once a game is over its file is taken
  away. The window that asks before a game is left says that it is kept.
- **Where**: where the system keeps a program's data (macOS:
  `~/Library/Application Support/Hexlords/saves`; Linux:
  `$XDG_DATA_HOME/hexlords/saves` or `~/.local/share/hexlords/saves`;
  Windows: `%APPDATA%\Hexlords\saves`), or `--saves <dir>`. A run with
  `--script`, `--screenshot` or `--frames` keeps nothing unless told where
  (it is a test or a picture, not somebody's game). A browser has no files:
  there no game is kept, and the window says a game left is lost.
- **Playing on**: the new game screen lists the games in progress first,
  the last played first, each in a line (its map, its round, its mode, who
  plays), with Play on and Delete. Delete asks once ("Delete it for good")
  and then takes the file away. A file that cannot be read is listed with
  why, and can only be deleted.
- **What comes back and what does not**: the game, exactly, in the middle
  of a turn or a round as it was left; the log and what each person had
  seen of it. Not kept: the replay of the time before (it begins anew),
  where the map was looked at, the person's options, and what the
  computer's players had in mind (they think again from what they see,
  so the game may go on otherwise than it would have: it is the same game
  up to where it was left, not after).
- A map made by hand in a test has no texts, and is not kept.

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
  old holder sent its units. What a turn's end brings (the marches and
  patrols, what they take) is that turn's in the history, and what its
  start brings (the units built, the deserters) the next one's.
- **Rally points** (built, the owner's word 2026-11-21: gathering new
  units into armies is the drudgery, moving armies and taking cities the
  fun): each side has ten, 0 to 9 (`Game::rally_points`,
  `Order::RallyPoint`), its own (another side's view has none of them),
  placed on land it has seen (not open water, nor a city not its own): R, or the Units menu's Rally points, places,
  moves, looks at and removes them; a numbered flag in the side's colour
  shows each on the map. Units sent to one (by Go to onto its flag, or by a city) are bound
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
  no moves left, and the selection goes on to the next stack that waits
  (a stack moved stays selected while it can go on: D is how the person
  moves on from it). The next turn they are back.
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
  the player is done with this turn: put aside, or moved). A stack's row
  counts the units it is about, not all on the hex (the owner,
  2026-11-22: three defenders and two units that wait are "2 units ... no
  orders yet", `Game::task_units`), and its click selects those; a click
  on a hex whose units have all had their orders takes those that moved
  or are on their way, not the defenders beside them. A stack with a
  plan that was put aside (D) is still on its way, and says where; units
  of one hex sent two ways name both. A stack the
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
  (the nearest to the cursor), and once the stack the person moved can go
  no further this turn, as Done and Defend do (off at first: the owner's
  word, 2026-11-21, "I like to look around before moving on"). Either
  way, the stack moved stays selected while it can go on (the owner's
  word, 2026-11-21: "if a stack is moved and it has more MP leave it
  selected, in FOW games the user may have to creep into the fog ... they
  can hit D if they are really done"; "all modes even when next stack
  after move is off"): the whole stack, never some of it ("the goal is to
  make it easier to creep into fog not to break the stack up"), so a
  stack one of whose units has no moves left is done with; at sea, the
  ship's whole stack, which it carries. A move refused leaves it selected
  as it was. A move that spends it, a battle (an attack uses up the
  attackers' moves, and a battle lost leaves nothing), a march still on
  its way, a patrol and a blessing that stops it leave nothing selected,
  or the next stack with the option; so does Done, which is how the person
  says they are done with a stack that can still go on.
  End turn counts what waits for orders ("End
  turn (3)": the tier-1 tasks), and, as the second option says (on at
  first), asks before the turn ends while anything does, or while the
  gold and the taxes will not pay next turn's upkeep (units would leave);
  its window can show the tasks. The third tells a battle of the
  person's in a line under the stack instead of playing it out in its
  window (the log's Watch plays it); another plays battles out at twice
  the pace (off at first: the slower pace has more tension). The next
  shows moves as they happen (on at first; the owner's word 2026-11-21:
  "the player should see them moving and visibility opening up ... not
  super slow", "the map should center on the action"): the game's state
  changes at once, and the screen shows the stacks that went going from
  hex to hex (`showing`, a step in `showing::STEP_MS`, a way in
  `showing::WAY_MS` at most), the person's own and the others' where the
  person sees them, the land opening up as the person's own go (a stack
  kept selected after its move has its reach marked once it is shown
  where it went, not before: nor into land it has yet to be shown to
  see); the computer plays a turn an order at a time
  (`agents::play_step`), the map brought to what it does; and a battle
  the person's side is in that no order of theirs told (the computer attacking them, a march or a patrol
  at a turn's end) is played out in its window, everything else waiting
  for it, unless battles are told in a line. The window that says who won
  waits for the battle that won it. Without a window nothing is kept for
  this (`Game::watch`): the headless games pay nothing. Another option
  has a stack moved onto a temple seek its blessing unasked, and says in
  that line what each was given. The shell carries them from one game to
  the next.
- **A log** (built): every battle a side fought, attacking or attacked,
  with who won and who died; the cities it took and lost; the units that
  left it unpaid; the sides out. The game keeps the history
  (`Game::history`); the log is what it says to the side at the screen.
  When the person's turn comes it opens if the others did something to
  them, or something went wrong for them as their last turn ended or this
  one began (a march stopped short, a battle on patrol, units left unpaid), what is new in a color of its own;
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
  beyond the selection's reach selects that one; one in reach asks, join
  it or take it, the first button the likelier: take it if the screen
  chose the selected stack, join it if the person did, the owner's word
  2026-11-21: "switching stacks is legit but so is combining them"); a
  click that would end a stack's standing orders (a march, defending, a
  patrol, marauding; not one stopped short, which waits for orders) asks
  first, saying what they do ("New orders?": Enter sends, Esc keeps); an
  order refused says why in a window ("Not done": OK, Enter, Esc or a
  click on the map closes it), and the stack stays selected to try again;
  a question that is no longer the question (another stack selected, a
  pick begun, another window over it) goes. A drag moves the map rather
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
  click: a click that would give it an order (and so end its duty) asks
  first ("New orders?", above). It takes this turn's
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
6. **Rounds.** Planned paths, "done", the resolution, the replay. **Done
   (2026-11-22).**
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

1. Rounds are built ("Rounds", above), and changed once by play (the
   clock, fights on the way, joining: 2026-11-22); what more play says
   of them is open (a march that fights, the computer's players in
   them).
2. Combat: which system a game starts with (lines for now). Agents
   against agents without a window can compare them (`--battles`, with
   many seeds).
