//! The help's topics (the Help menu): how battles go in this game, movement,
//! cities and gold, units and captains, orders and turns, fog of war. Each is
//! a list of sections of rows, a word and what it says, as the help window
//! shows them (`MapScreen::show_topic`), written from the game: the rules it
//! was set up with, the map's kinds of unit. Plain functions of the game's
//! state, as `info` is.
//!
//! Not a port: the C++ had no help.

use crate::game::{CITY_SIGHT, Game, HARD_ODDS, UNIT_SIGHT, emerge_chance};
use crate::help::HelpSection;
use crate::info;
use crate::setup::{Fog, TurnOrder};
use crate::state::combat::{BLOW_BACK_MOST, HITS, MAX_CHANCE, System, walls_bonus};
use crate::state::{Blessing, LEVEL_EXPERIENCE, MAX_STACK, Travel, WALL_PRICE, WALLS_CAP};

/// How long a line of a topic's words is, at most, in characters.
const WIDTH: usize = 64;

/// The topics, in the Help menu's order: the id of its item, its title.
pub const TOPICS: [(&str, &str); 6] = [
    ("help_battles", "Battles"),
    ("help_movement", "Movement"),
    ("help_cities", "Cities and gold"),
    ("help_units", "Units and captains"),
    ("help_orders", "Orders and turns"),
    ("help_fog", "What you see"),
];

/// The topic with this id: its title and its sections.
pub fn topic(game: &Game, id: &str) -> Option<(&'static str, Vec<HelpSection>)> {
    let (_, title) = TOPICS.iter().find(|(topic, _)| *topic == id)?;
    let sections = match id {
        "help_battles" => battles(game),
        "help_movement" => movement(game),
        "help_cities" => cities(game),
        "help_units" => units(game),
        "help_orders" => orders(game),
        _ => sight(game),
    };
    Some((title, sections))
}

/// A section: rows of a word and what it says, the words cut into lines (a
/// row each, the word only on the first).
fn section(title: &str, rows: &[(&str, String)]) -> HelpSection {
    let mut out = Vec::new();
    for (word, text) in rows {
        for (i, line) in wrap(text, WIDTH).into_iter().enumerate() {
            out.push((if i == 0 { word.to_string() } else { String::new() }, line));
        }
    }
    HelpSection { title: title.to_string(), rows: out }
}

/// Text in lines of at most `width` characters, cut between words.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}

/// How often a strength twice another's wins an exchange, in percent, with
/// `chance` (0 to 10): `2^k : 1`, `k = 2 (10 - chance) / 5`.
pub fn twice_wins(chance: u8) -> u32 {
    let k = 2.0 * f64::from(MAX_CHANCE.saturating_sub(chance)) / 5.0;
    let a = 2f64.powf(k);
    (100.0 * a / (a + 1.0)).round() as u32
}

fn battles(game: &Game) -> Vec<HelpSection> {
    let rules = game.setup().combat;
    let system = match rules.system {
        System::Lines => "Lines. Each stack stands in a line from its weakest to its strongest, its commander \
                          last; the two at the front fight, the loser of each exchange takes a hit, and when a \
                          unit dies the next of its line steps up."
            .to_string(),
        System::Strength => "Strength against strength. Each exchange matches the two stacks' whole strengths, \
                             and the loser takes the hit on one of its units drawn by chance: cheap units do not \
                             shield the dear ones."
            .to_string(),
    };
    let chance = match rules.chance {
        0 => "0 of 10: the stronger always wins (a tie is the defender's).".to_string(),
        c if c >= MAX_CHANCE => {
            format!("{c} of 10: strength counts for nothing; every exchange is a coin toss.")
        }
        c => format!(
            "{c} of 10. Only the ratio of the strengths counts: one twice as strong as the other wins {}% of \
             the exchanges. (Less chance: strength counts more.)",
            twice_wins(c)
        ),
    };
    let walls = format!(
        "{} point behind walls 1 or 2, {} behind 3 to 5, {} behind 6 (walls 0: nothing)",
        walls_bonus(1),
        walls_bonus(3),
        walls_bonus(6)
    );
    // What this game's kinds fight the better for: their ground, a siege.
    let knacks: Vec<String> = game
        .map
        .units()
        .kinds()
        .iter()
        .filter_map(|k| info::knack_line(k).map(|line| format!("{}: {}", k.name, line.to_lowercase())))
        .collect();
    let knacks = if knacks.is_empty() {
        "No kind of unit in this game fights better on some ground, nor breaks walls.".to_string()
    } else {
        format!(
            "Some kinds fight better on some ground, attacking or defending, and a siege engine makes the \
             walls of a city its stack attacks count less: {}.",
            knacks.join("; ")
        )
    };
    vec![
        section("In this game", &[("How", system), ("Chance", chance)]),
        section(
            "What makes a unit stronger",
            &[
                (
                    "Its own",
                    "Its kind's strength, a point for each level above the first, and the temples' \
                             blessings."
                        .to_string(),
                ),
                (
                    "A commander",
                    "Every other unit of a stack with a commander is as much stronger as it gives \
                                 (a point more when it is at its highest level). In lines the commander fights last; by \
                                 strength a hit may fall on it as on any other."
                        .to_string(),
                ),
                ("The ground", "A defender is a point stronger on hills and in forests, two on mountains.".to_string()),
                ("Walls", format!("A defender in a city is stronger by its walls: {walls}.")),
                ("Their own ground", knacks),
                (
                    "At the most",
                    match rules.cap {
                        Some(cap) => {
                            format!("No unit fights stronger than {cap} in this game, all its bonuses counted.")
                        }
                        None => "No cap: a unit fights with all its bonuses, however many.".to_string(),
                    },
                ),
                (
                    "Attacking",
                    "Attackers get neither the ground's bonus nor walls; their kind's own bonus on the \
                     ground they attack, they do."
                        .to_string(),
                ),
            ],
        ),
        section(
            "A battle",
            &[
                (
                    "Who fights",
                    format!(
                        "An attack is at most {MAX_STACK} units and a commander, from one hex or from several. \
                     All who stand on the hex attacked defend it."
                    ),
                ),
                (
                    "Hits",
                    format!(
                        "A unit dies at its {} hit; {}",
                        if HITS == 2 { "second".to_string() } else { format!("{HITS}th") },
                        if rules.wounds {
                            "wounds last until its side's turn ends (attacked in the others' turns, it stays \
                             wounded through its own), and a wounded unit fights at half its strength, \
                             rounded up."
                        } else {
                            "one that lives is whole again after the battle."
                        }
                    ),
                ),
                (
                    "Hitting back",
                    if rules.blows_back {
                        format!(
                            "The loser of an exchange may hit back: one of the winner's units (in lines, the \
                             one it fought) takes a hit too, with a chance of the loser's share of the \
                             strengths that fought, never more than {BLOW_BACK_MOST}%. A side the exchange \
                             left with nobody does not."
                        )
                    } else {
                        "The winner of an exchange is never hit in it.".to_string()
                    },
                ),
                ("The odds", "Before you attack, the line under the map says how likely you are to win.".to_string()),
                (
                    "After",
                    "If the attackers win, those that live go in, and take the city. A battle cannot be \
                           undone."
                        .to_string(),
                ),
                (
                    "Captains",
                    format!(
                        "A stack with no captain that wins may see one rise from its ranks: the chance is half \
                     of how unlikely the win was ({}% for a fight at even odds).",
                        emerge_chance(50)
                    ),
                ),
            ],
        ),
    ]
}

fn movement(game: &Game) -> Vec<HelpSection> {
    let mut kinds = Vec::new();
    for kind in game.map.units().kinds() {
        let costs = info::enters(&kind.cost, usize::MAX).join(", ");
        let costs = costs.strip_prefix("Enters: ").unwrap_or(&costs).to_string();
        kinds.push((kind.name.as_str(), format!("{} moves a turn. {costs}.", kind.moves)));
    }
    let mut out = vec![section(
        "Moving",
        &[
            (
                "Points",
                "Each unit has its moves to spend each turn; a step costs what that terrain costs it. Up \
                        to 2 points it does not use are kept for the next turn."
                    .to_string(),
            ),
            (
                "Roads",
                "A step along a road costs what a road costs it; a bridge is a road over water, a pass a \
                       road over a mountain."
                    .to_string(),
            ),
            (
                "Stacks",
                "The units selected go together where every one of them can reach; a march goes at \
                        the pace of the slowest. Flyers with units that walk stay with them: they go one \
                        way all of them can pay for, not where the others cannot walk."
                    .to_string(),
            ),
            (
                "Commanders",
                "At the start of each turn a commander has the moves of the fastest unit on its hex, \
                            if they are more than its own: a captain with riders rides."
                    .to_string(),
            ),
            (
                "Others",
                "Another side's units cannot be gone through: moving into them is an attack, which \
                        needs 2 moves left beside them and ends the attackers' moves for the turn. Another \
                        side's city, or nobody's, ends a move; an empty one is taken."
                    .to_string(),
            ),
            (
                "Temples",
                format!(
                    "A stack on a temple may seek its blessing (asked when it goes there; B): each unit \
                     the temple has not blessed is blessed, {} or {} a turn, and stops there for the turn. \
                     Each temple blesses a unit once; another temple blesses it again.",
                    Blessing::Strength.words(),
                    Blessing::Moves.words()
                ),
            ),
        ],
    )];
    let travels = |t: Travel| game.map.units().kinds().iter().any(|k| k.travel == t);
    if travels(Travel::Sea) || travels(Travel::Air) {
        out.push(section(
            "Ships and flyers",
            &[
                (
                    "Ships",
                    "A ship goes over water and into its own side's cities. A land unit steps aboard a ship of \
                     its side that has room (it carries as many as its kind says, and a commander besides): \
                     that ends its move. The ship takes them where it goes, for nothing, and they land on \
                     their own points. Select the ship and those aboard together to sail: nobody is left in \
                     the water without a ship."
                        .to_string(),
                ),
                (
                    "At sea",
                    "Ships and flyers fight there; who rides does not, and goes down with the ships that \
                     sink. In a port, ships do not fight: they are lost if it falls."
                        .to_string(),
                ),
                (
                    "Flyers",
                    "A flyer goes over whatever its prices let it, water and mountains too. A commander that \
                     goes with flyers and no one else flies with them."
                        .to_string(),
                ),
            ],
        ));
    }
    if !kinds.is_empty() {
        out.push(section("What each kind of unit pays, on this map", &kinds));
    }
    out
}

fn cities(game: &Game) -> Vec<HelpSection> {
    vec![
        section(
            "Cities",
            &[
                (
                    "Taking one",
                    "Move into a city with nobody in it (after a battle, or empty) and it is yours.".to_string(),
                ),
                (
                    "Building",
                    "Its holder sets what it builds (its window shows each unit's numbers, turns and \
                              price). It goes on building the same until told otherwise."
                        .to_string(),
                ),
                (
                    "Paying",
                    "What a city finishes is paid for then; if the gold is short it waits until it can be. \
                     A full city keeps what stays in it waiting for room; what it sends elsewhere steps out \
                     onto the best hex beside it on its way that has room and nobody else's units, if \
                     one has."
                        .to_string(),
                ),
                (
                    "New units",
                    "A city can send what it builds to a hex of your choosing: Send to... in \
                               its window (Send... in the sidebar)."
                        .to_string(),
                ),
                (
                    "Walls",
                    format!(
                        "Walls make those who defend the city stronger (see Battles). Its holder raises them \
                         a level a turn, up to {WALLS_CAP}, from the city's window: {WALL_PRICE} gold for each \
                         level they rise to. A city that is taken loses a level."
                    ),
                ),
                ("Training", "Some cities train their units: they come out veterans, or elite.".to_string()),
                (
                    "Production",
                    "E: all your cities in one window: what each builds and where its units go (those \
                     idle or waiting first), what the next 5 rounds bring, and your gold: whether it \
                     will go on paying the upkeep, and if not from which round. From there you stop \
                     a city, or all of them, send a city's new units to a rally point or to another \
                     of your cities, and open a city's window for what it builds. Your stacks are \
                     there too, the dearest first, to send home what you can do without."
                        .to_string(),
                ),
                (
                    "The forecast",
                    "It takes things to stay as they are: your cities held, no battles, no new \
                     orders. Units sent from a city are taken to leave it, and none to arrive."
                        .to_string(),
                ),
                (
                    "The end",
                    "A side that has lost its last city is out of the game, its units with it; the last \
                             side left wins."
                        .to_string(),
                ),
            ],
        ),
        section(
            "Gold",
            &[
                ("This game", format!("Each side started with {} gold.", game.setup().gold)),
                (
                    "Taxes",
                    "At the start of each of your turns (from the second round) each of your cities \
                           brings in its taxes."
                        .to_string(),
                ),
                (
                    "Upkeep",
                    "Then your units' upkeep goes out. Units you cannot pay leave you: as few as make up \
                            what is short, a commander last."
                        .to_string(),
                ),
                (
                    "At a glance",
                    "The menu bar shows your gold, what comes in each turn (+) and what goes out (-).".to_string(),
                ),
            ],
        ),
    ]
}

fn units(_game: &Game) -> Vec<HelpSection> {
    let [veteran, elite] = LEVEL_EXPERIENCE;
    vec![
        section(
            "Units",
            &[
                (
                    "A stack",
                    format!(
                        "The units on a hex: at most {MAX_STACK}, and a commander besides. The boxes under the map \
                     show yours: the commander's on the left, then a box for each unit."
                    ),
                ),
                (
                    "Levels",
                    format!(
                        "Regular, veteran, elite: each level above the first is a point of strength. A unit that \
                     lives through a battle its side won gains a point of experience, two if the odds were \
                     below {HARD_ODDS}%; {veteran} make a veteran, {elite} elite. A silver ring marks a \
                     veteran, a gold one elite."
                    ),
                ),
                (
                    "Blessings",
                    format!(
                        "A temple blesses each unit once, if its stack seeks it there (B): {} or {} a turn; \
                         it stops there for the turn. Another temple blesses it again.",
                        Blessing::Strength.words(),
                        Blessing::Moves.words()
                    ),
                ),
                ("Upkeep", "Each unit costs its upkeep every turn (see Cities and gold).".to_string()),
            ],
        ),
        section(
            "Captains",
            &[
                (
                    "Commanding",
                    "A captain makes the other units of its stack stronger and, in lines, fights last. It has a \
                                name, and rises in level too; at the highest it commands a point more."
                        .to_string(),
                ),
                ("Speed", "It moves as fast as the fastest unit of its stack.".to_string()),
                (
                    "New ones",
                    "Some cities can build captains; and a stack with no captain that wins a battle may \
                              see one rise from its ranks, the likelier the harder the fight was."
                        .to_string(),
                ),
            ],
        ),
    ]
}

fn orders(game: &Game) -> Vec<HelpSection> {
    let order = match (game.rounds(), game.setup().order) {
        (false, TurnOrder::Fixed) => "The sides play one after the other, in the same order every round.",
        (false, TurnOrder::Drawn) => "The sides play one after the other, in an order drawn anew every round.",
        (true, _) => {
            "Rounds: every side gives its orders, in the same order every round, then the round plays out \
             at once. Of stacks that get somewhere at the same moment, the sides go in an order drawn anew \
             as each round plays out (always, in rounds)."
        }
    };
    // In rounds: how a round goes.
    let rounds: Vec<(&str, String)> = if game.rounds() {
        vec![
            (
                "Your orders",
                "A click on where a stack goes is a plan: nothing moves yet, and the map outlines the way. \
                 While the stack has moves left where its plan ends it stays selected, and another click is a \
                 waypoint: it goes on from there (a click on the stack itself: afresh). Undo takes the last \
                 plan, or its last leg, back. What moves nobody happens at once: what a city builds, its \
                 walls, Defend, a blessing. Then Orders done (where End turn is)."
                    .to_string(),
            ),
            (
                "The round",
                "Once every side's orders are done the round plays out, every stack along the way the map \
                 outlined. It ends when nobody moves any more; then every side's taxes come in and its cities \
                 build, together."
                    .to_string(),
            ),
            (
                "Speed",
                "The faster a unit, the sooner it is there: a step takes it the share of the round that its \
                 cost is of the unit's moves a turn. Riders with 20 moves are three hexes of grass on when \
                 soldiers with 12 are two; a stack goes at the pace of its slowest. So riders sent to a city \
                 are in it before foot soldiers as far away."
                    .to_string(),
            ),
            (
                "Meeting",
                "A stack that gets to a hex another side stands on attacks it, there and then, wherever it \
                 was going: it does not go round who has come to stand on its way, and it does not wait. So \
                 two stacks making for the same hex fight there, the one that got there first defending; two \
                 that cross fight where they meet; a stack sent against one that has left finds the hex \
                 empty, and takes it. A march (Go to) fights on its way too. A patrol goes round, and \
                 attacks at its odds. A battle is the round of whoever wins it, attackers or attacked: they \
                 go no further."
                    .to_string(),
            ),
            (
                "Joining",
                "Each stack attacks when it gets there. To have two go, and fight, as one: send one onto the \
                 other (Join it); the stack there is then selected with those on their way to it (marked + \
                 in their boxes, which are blue); send it on. Those there wait for the others, and they go on together, at \
                 the pace of the slowest, with what moves the way there left."
                    .to_string(),
            ),
            (
                "Your own",
                "A stack waits behind others of yours that are on their way and leave it no room, and goes \
                 through those that stay. Two of yours that come toward each other with no room for both \
                 on one hex change places, each paying its step: a hex never holds more than a stack."
                    .to_string(),
            ),
            (
                "Battles",
                "A battle of yours is played out in its window as the round comes to it, and the round \
                 waits until you close the window (Close, Enter or Esc)."
                    .to_string(),
            ),
            (
                "Below",
                "What the rest of this page says happens at once or as your turn ends (a move, Go to, a \
                 patrol, marauders) happens, in rounds, as the round plays out; a plan that ends on a temple \
                 seeks its blessing there; and End turn is Orders done."
                    .to_string(),
            ),
            (
                "Into the dark",
                "With total fog of war, a click on land you have never seen sends the stack toward it, to \
                 look: as the round plays out it goes over the land it has seen, and finds its way on as it \
                 sees more."
                    .to_string(),
            ),
        ]
    } else {
        Vec::new()
    };
    let mut sections = Vec::new();
    if !rounds.is_empty() {
        sections.push(section("Rounds", &rounds));
    }
    sections.extend(orders_sections(order));
    sections
}

/// The Orders topic's sections that turns and rounds share.
fn orders_sections(order: &str) -> Vec<HelpSection> {
    vec![
        section(
            "Turns",
            &[
                ("This game", order.to_string()),
                (
                    "Your turn",
                    "Move your stacks, set what your cities build, then End turn. When your turn comes \
                     again, your log opens if the others did something to you (Start turn closes it); \
                     else your turn starts at once (with the stack nearest the cursor that waits for orders \
                     selected, if you said so in the Options)."
                        .to_string(),
                ),
                (
                    "Options",
                    "The Game menu's Options: whether the next stack that waits is selected by itself \
                     (as a turn starts, and once the stack you moved can go no further), whether a \
                     battle is told in a line or played out (and how fast), whether moves are shown \
                     as they happen (the computer's an order at a time, the map on what it does), \
                     whether a temple asks, whether End turn asks while something waits for orders, and in \
                     rounds whether the map outlines the selected stack's orders only (V)."
                        .to_string(),
                ),
                (
                    "Hotseat",
                    "Several people at one screen: the map is covered between their turns, and each \
                             turn starts at the side's capital."
                        .to_string(),
                ),
            ],
        ),
        section(
            "Orders",
            &[
                (
                    "Moving",
                    "Click your stack, then where it goes (or click its units under the map to take some \
                            of them). While the whole stack can go on it stays selected, to go on a step at a \
                            time (into the fog, say): D when you are done with it, and the next stack that \
                            waits is selected, as it is once it can go no further, if you choose (Options). \
                            Onto another of your \
                            stacks in reach, it asks: join it, or take that stack. A stack with orders of \
                            its own (marching, defending, on patrol) asks before new ones end them. An order \
                            refused says why. Undo takes back the last move, a city's work or its walls, \
                            unless chance had a say in it."
                        .to_string(),
                ),
                (
                    "Go to",
                    "Sends the selected units somewhere far (G, then a hex; or just click a hex beyond \
                           their reach): this turn's part at once, the rest over the turns, on their own, at the end of each of \
                           yours (until then you can still send them elsewhere); Undo takes it back until they have gone a second leg. A march never attacks: if something is in the way it \
                           stops and waits for orders. Shift+click on the way there makes a stop: they go by \
                           way of it, and on at once; at a temple they seek its blessing and stop for the turn. \
                           A city's new units can go by stops too. Once they get where they go, they wait for \
                           orders."
                        .to_string(),
                ),
                (
                    "Done",
                    "Puts the selected units aside for this turn: Next stack passes them by, and the next \
                     stack that waits is selected."
                        .to_string(),
                ),
                (
                    "A queue",
                    "In a city's window, + beside a kind adds it to the city's queue: so many of it, sent \
                     somewhere (click an item to choose it, then set what it builds and where its units go). \
                     After the last item it stops, or repeats from the chosen item, for ever or so many \
                     times. A city whose queue ran out needs orders; Nothing means nothing, on purpose."
                        .to_string(),
                ),
                (
                    "Rally points",
                    "R: up to ten places where units gather into armies, each a numbered flag on the map. \
                     A city sends its new units to one with its button (with a temple's blessing on the way, \
                     if one is near it); Go to onto a flag sends a stack. Once a point is full, those who \
                     come stand beside it. A point can have them defend there, so nobody need see to them; \
                     a city can keep its new units defending it too."
                        .to_string(),
                ),
                (
                    "Defend",
                    "F: the selected units defend where they stand, turn after turn; Next stack and the \
                     tasks pass them by until you send them somewhere, or press F again."
                        .to_string(),
                ),
                (
                    "Disband",
                    "X: sends the selected units home for good, and their upkeep with them; it asks first, \
                     and cannot be taken back."
                        .to_string(),
                ),
                (
                    "Patrol",
                    "P: the selected stack goes round waypoints, turn after turn. Shift+click each waypoint \
                     on land you have seen, then click the last: back and forth along them, or round and round \
                     if the last is the stack's own hex. Then say whether it attacks: never (a scout keeps out \
                     of fights), or the stacks it sees and can reach at the odds you pick or better, once a \
                     turn at most (never nobody's). A waypoint is in no city but your own. It goes on from a \
                     waypoint at once, and at the end of each of your turns."
                        .to_string(),
                ),
                (
                    "Maraud",
                    "U: the computer leads the selected stack, at once and at the end of each of your \
                     turns: it hunts the stacks it can beat, takes the cities it can, and looks for more. U \
                     again calls it back."
                        .to_string(),
                ),
                (
                    "After a battle",
                    "A stack on patrol or marauding that fights (or is attacked) waits for your word: it needs \
                     orders, and N goes to it. D lets it go on (at once, if it waited since your turn began); \
                     any other order ends its patrol or its marauding; left alone, it goes on at the end of \
                     your turn."
                        .to_string(),
                ),
                (
                    "Tasks",
                    "What you have left to do this turn: stacks with no orders and cities that build \
                           nothing first, then cities held up, then the rest. A stack you moved, or put \
                           aside with D, is done for the turn: N passes it by (click it to move it again). \
                           N goes to the nearest stack that waits, round them all; Shift+N goes back."
                        .to_string(),
                ),
            ],
        ),
    ]
}

fn sight(game: &Game) -> Vec<HelpSection> {
    let near = format!("You see what is within {UNIT_SIGHT} hexes of your units and {CITY_SIGHT} of your cities");
    let fog = match game.setup().fog {
        Fog::Off => "Off in this game: the whole map is seen.".to_string(),
        Fog::On => format!(
            "On in this game. {near}; the rest of the map is dimmed: its terrain and where its cities \
             are show, but no units, and who held each city when you saw it last."
        ),
        Fog::Total => format!(
            "Total in this game. {near}. Land you have never seen is black: nothing of it shows, not \
             even its cities. Once you have seen it, it is dimmed when it is out of your sight: its \
             terrain and its cities show, but no units, and who held each city when you saw it last."
        ),
    };
    let mut sections = vec![section(
        "What you see",
        &[
            ("Your own", "Your units and cities, all of them, in full.".to_string()),
            (
                "The others'",
                "Another side's stack is a banner in its color, with a pip for each unit; its \
                             panels say how many and whose, not what. Its cities show their walls, taxes and \
                             how many stand in them, never what they build."
                    .to_string(),
            ),
            ("Fog of war", fog),
        ],
    )];
    if game.setup().fog.on() {
        let black = (game.setup().fog == Fog::Total).then(|| {
            (
                "The black",
                "Your units cannot go into land you have never seen, nor find a way through it: they \
                 go to its edge, and see on. Go to and a city's new units go only by the land you have seen."
                    .to_string(),
            )
        });
        let items: Vec<(&str, String)> = [
            (
                "On the way",
                "You see what your units see where they are: as a stack goes, what it sees goes with it, \
                 and what it has passed is dimmed again. The stacks it passed are remembered."
                    .to_string(),
            ),
            (
                "Remembered",
                "Another side's stack you saw stays on the map, faint, where you saw it last (or where it \
                 went out of your sight), until that hex is in sight again; a city keeps the colors of who \
                 held it then, and the walls it had then (one you have never seen: nobody's colors, and \
                 the walls it began with). The line under the map says how many and when."
                    .to_string(),
            ),
            (
                "Orders",
                "You may send units into the fog: they go as far as you know the way. Units of another \
                 side you could not see, met on the way, stop them there, in a battle."
                    .to_string(),
            ),
        ]
        .into_iter()
        .chain(black)
        .chain([(
            "Undo",
            "A move that showed you what you had not seen (another side's units, who holds a city, \
             land) cannot be taken back."
                .to_string(),
        )])
        .collect();
        sections.push(section("In the fog", &items));
    }
    sections
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_are_cut_into_lines_between_words() {
        assert_eq!(wrap("one two three", 7), ["one two", "three"]);
        assert_eq!(wrap("", 10), [""]);
        assert!(wrap(&"word ".repeat(40), WIDTH).iter().all(|l| l.chars().count() <= WIDTH));
    }

    #[test]
    fn twice_as_strong_wins_as_the_rule_says() {
        // At 5, k is 2: four in five; at 3, about 87 (docs/gameplay.md).
        assert_eq!([twice_wins(3), twice_wins(5), twice_wins(10)], [87, 80, 50]);
    }
}
