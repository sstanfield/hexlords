//! What the panels say: the line about the hex under the cursor; the selected
//! hex, its units' numbers under their icons, the selected unit in a line and
//! any unit in full (the box the pointer opens); the selected city beside the
//! map. Words only, from the game's state; `session` puts them into widgets.
//!
//! Not a port: the C++ had no panels.

use crate::game::{CityState, Game, Ghost, Sight, StackState, Task, describe_terrain};
use crate::orders::{Event, Fallen, Record};
use crate::state::scenario::side_name;
use crate::state::{City, Hex, Map, MoveCost, Patrol, QueueEnd, Sends, Side, TileType, Travel, Unit, UnitProd};

/// The line about the hex under the cursor: the turn, the hex's terrain and
/// road, its city's name, how many units stand there, and, if the selected
/// units can go there, what it costs (one unit) or that they can (a stack),
/// and the odds of winning if going there is an attack.
pub fn cursor_line(game: &Game) -> String {
    cursor_line_for(game, &Sight::all())
}

/// [`cursor_line`] as a side knows it (`sight`): of a hex in the fog, no
/// units, and no odds, but what was seen there last, if anything; of land
/// never seen, nothing.
pub fn cursor_line_for(game: &Game, sight: &Sight) -> String {
    let (map, hex) = (&game.map, game.map.cursor);
    let seen = sight.sees(hex);
    let mut parts = vec![format!("Turn {}", game.turn())];
    if !sight.knows(hex) {
        parts.push(format!("row {}, column {}: {NEVER_SEEN}", hex.row, hex.col));
    } else if let Some(tile) = map.tile(hex) {
        parts.push(format!("row {}, column {}: {}", hex.row, hex.col, describe_terrain(tile.kind, tile.road)));
        if let Some(city) = map.city_at(hex) {
            parts.push(city.name.clone());
        }
        match tile.units().len() {
            _ if !seen => parts.push(match sight.ghost_at(hex) {
                Some(g) => format!("not seen: {} there in round {}", last_seen(g), g.round),
                None => "not seen".to_string(),
            }),
            0 => {}
            1 => parts.push("1 unit".to_string()),
            n => parts.push(format!("{n} units")),
        }
    }
    let selected = game.selected().and_then(|id| map.units().get(id));
    if let (Some(unit), Some(cost)) = (selected, game.range().and_then(|r| r.cost(hex))) {
        match game.battle_odds(game.selection(), hex).filter(|_| seen) {
            Some(odds) => parts.push(format!("attack: {odds}% likely to win")),
            None if game.selection().len() > 1 => {
                parts.push(format!("the {} units can move here", game.selection().len()))
            }
            None => parts.push(format!("{cost} of {} points to move here", unit.movement)),
        }
    }
    parts.join("  |  ")
}

/// What a record of the game's history (`Game::history`) says to `side`, a
/// line or two: a battle it fought, as the attacker or the defender, with who
/// died; a city it took or lost; a side out of the game, the winner. Nothing
/// if it is not about `side` (the other sides' battles and cities, the units
/// built). Units that left it unpaid are told too, and its marches that
/// stopped short.
pub fn record_lines(map: &Map, record: &Record, side: Side) -> Vec<String> {
    match &record.event {
        Event::Battle { at, attacker, defender, attackers, defenders, won, dead, .. }
            if *attacker == side || *defender == side =>
        {
            let place = place(map, *at);
            let first = if *attacker == side {
                let how = if *won { "won" } else { "lost" };
                format!("You attacked {} at {place}, {attackers} against {defenders}, and {how}.", whose(*defender))
            } else {
                let how = if *won { "you were beaten" } else { "you held" };
                format!(
                    "{} attacked you at {place}, {attackers} against {defenders}: {how}.",
                    sentence(side_name(*attacker))
                )
            };
            let ours: Vec<&Fallen> = dead.iter().filter(|f| f.side == side).collect();
            let theirs: Vec<&Fallen> = dead.iter().filter(|f| f.side != side).collect();
            vec![first, format!("You lost {}; they lost {}.", counted(&ours), counted(&theirs))]
        }
        Event::Taken { city, from, by, walls } if *by == side || *from == side => {
            let name = map.cities().get(*city).map_or("a city".to_string(), |c| c.name.clone());
            let fell = walls.map_or(String::new(), |w| format!(" Its walls fell to {w}."));
            if *by == side {
                vec![format!("You took {name} from {}.{fell}", who(*from))]
            } else {
                vec![format!("{} took {name} from you.{fell}", sentence(side_name(*by)))]
            }
        }
        Event::Promoted { side: whose, name, level, commander, .. } if *whose == side => {
            vec![format!("Your {name} rose to {}.", rank(*level, *commander))]
        }
        Event::Blessed { side: whose, name, blessing, .. } if *whose == side => {
            vec![format!("Your {name} were blessed at a temple: {}.", blessing.words())]
        }
        Event::Emerged { side: whose, at, name, .. } if *whose == side => {
            vec![format!("From the ranks at {}, a leader rose: {name}.", place(map, *at))]
        }
        // Units met on the way that nobody had seen (fog of war).
        Event::Surprise { side: whose, at } if *whose == side => {
            vec![format!("At {}, your units ran into others they had not seen there.", place(map, *at))]
        }
        Event::Drowned { side: whose, at, dead } if *whose == side => {
            let dead: Vec<&Fallen> = dead.iter().collect();
            vec![format!("At {}, {} went down with your ships.", place(map, *at), counted(&dead))]
        }
        // A standing order that stopped short, at the end of a turn.
        Event::Stalled { side: whose, units, at, to } if *whose == side => {
            let (who, waits) = match units.len() {
                1 => ("One of your units".to_string(), "It waits"),
                n => (format!("{n} of your units"), "They wait"),
            };
            vec![format!(
                "{who} stopped at {} on the way to {}: something is in the way. {waits} for orders.",
                place(map, *at),
                place(map, *to)
            )]
        }
        Event::Deserted { side: whose, name } if *whose == side => {
            vec![format!("Unpaid, your {name} left you: the treasury could not pay their upkeep.")]
        }
        Event::WallsRaised { city, side: whose, walls, price } if *whose == side => {
            let name = map.cities().get(*city).map_or("a city".to_string(), |c| c.name.clone());
            vec![format!("You raised the walls of {name} to {walls}, for {price} gold.")]
        }
        Event::Disbanded { side: whose, names } if *whose == side => {
            vec![format!("You sent home {}.", names.join(", "))]
        }
        Event::Out { side: out } if *out == side => vec!["You are out of the game.".to_string()],
        Event::Out { side: out } => vec![format!("{} is out of the game.", sentence(side_name(*out)))],
        Event::Won { side: winner } if *winner == side => vec!["You have won the game.".to_string()],
        Event::Won { side: winner } => vec![format!("{} has won the game.", sentence(side_name(*winner)))],
        _ => Vec::new(),
    }
}

/// An event in a side's log: its lines, and whether it is new (it came after
/// the history was the length the log was asked from).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogEntry {
    pub new: bool,
    pub lines: Vec<String>,
    /// Its record's place in the history, and whether it is a battle (one
    /// that can be watched, while the game keeps it: `Game::fight`).
    pub record: usize,
    pub battle: bool,
}

/// What the game's history says to `side`, a round at a time, the latest
/// round first and each round's events in the order they happened: the
/// round, and each event's lines (`record_lines`), new if it came after the
/// history was `new_from` long. Rounds with nothing for `side` are left out.
pub fn log(game: &Game, side: Side, new_from: usize) -> Vec<(u32, Vec<LogEntry>)> {
    let mut rounds: Vec<(u32, Vec<LogEntry>)> = Vec::new();
    for (i, record) in game.history().iter().enumerate() {
        let lines = record_lines(&game.map, record, side);
        if lines.is_empty() {
            continue;
        }
        let battle = matches!(record.event, Event::Battle { .. });
        let entry = LogEntry { new: i >= new_from, lines, record: i, battle };
        match rounds.last_mut() {
            Some((round, entries)) if *round == record.round => entries.push(entry),
            _ => rounds.push((record.round, vec![entry])),
        }
    }
    rounds.reverse();
    rounds
}

/// How a battle went, for `side` (who fought it): the battle's record's
/// lines, and those of what came of it in the same order (levels won, a
/// captain from the ranks, a city taken, a side out, the winner).
pub fn battle_outcome(game: &Game, record: usize, side: Side) -> Vec<String> {
    let history = game.history();
    let Some(battle) = history.get(record) else { return Vec::new() };
    let mut lines = record_lines(&game.map, battle, side);
    for next in history.iter().skip(record + 1) {
        let after = matches!(
            next.event,
            Event::Taken { .. }
                | Event::Out { .. }
                | Event::Won { .. }
                | Event::Promoted { .. }
                | Event::Emerged { .. }
        );
        if !after || next.round != battle.round || next.turn != battle.turn {
            break;
        }
        lines.extend(record_lines(&game.map, next, side));
    }
    // The same line again and again (three units risen alike) once, counted.
    let mut counted: Vec<(String, usize)> = Vec::new();
    for line in lines {
        match counted.last_mut() {
            Some((last, n)) if *last == line => *n += 1,
            _ => counted.push((line, 1)),
        }
    }
    counted.into_iter().map(|(line, n)| if n > 1 { format!("{line} (\u{d7}{n})") } else { line }).collect()
}

/// The fallen by kind, counted, and captains by name: "2 Militia, 1
/// Riders, Captain Aldric", in the order they first come; "nothing" for
/// none.
fn counted(fallen: &[&Fallen]) -> String {
    let mut counts: Vec<(&str, usize, bool)> = Vec::new();
    for f in fallen {
        match counts.iter_mut().find(|(n, _, person)| *n == f.name && !*person) {
            Some((_, count, _)) => *count += 1,
            None => counts.push((&f.name, 1, f.person)),
        }
    }
    if counts.is_empty() {
        return "nothing".to_string();
    }
    let each = |(name, count, person): &(&str, usize, bool)| {
        if *person { name.to_string() } else { format!("{count} {name}") }
    };
    counts.iter().map(each).collect::<Vec<_>>().join(", ")
}

/// A unit's level in words: a commander's is a number ("level 2"), the
/// others' a rank ("veteran").
pub fn rank(level: u8, commander: bool) -> String {
    match (commander, level) {
        (true, level) => format!("level {level}"),
        (false, 0 | 1) => "regular".to_string(),
        (false, 2) => "veteran".to_string(),
        (false, _) => "elite".to_string(),
    }
}

/// Another side's units, in a sentence: "blue", or "nobody's units".
fn whose(side: Side) -> String {
    if side == Side::Grey { "nobody's units".to_string() } else { side_name(side).to_string() }
}

/// A hex in words (where a battle was, where units go): its city's name,
/// or "row 5, column 9".
pub fn place(map: &Map, at: Hex) -> String {
    map.city_at(at).map_or(format!("row {}, column {}", at.row, at.col), |c| c.name.clone())
}

/// A standing order's stops in words, in order: "the temple at row 9,
/// column 12, then Redhold".
pub fn route_words(map: &Map, route: &[Hex]) -> String {
    let stop = |at: &Hex| match map.tile(*at) {
        Some(tile) if tile.kind == TileType::Temple && map.city_at(*at).is_none() => {
            format!("the temple at {}", place(map, *at))
        }
        _ => place(map, *at),
    };
    route.iter().map(stop).collect::<Vec<_>>().join(", then ")
}

/// A route in words, as `route_words`, its end named as the rally point it
/// is if the units on it are `bound` to one: "the temple at row 4, column
/// 9, then rally point 2 (row 5, column 20)".
pub fn route_words_to(map: &Map, route: &[Hex], bound: Option<u8>) -> String {
    match (bound, route.split_last()) {
        (Some(n), Some((end, []))) => format!("rally point {n} ({})", place(map, *end)),
        (Some(n), Some((end, stops))) => {
            format!("{}, then rally point {n} ({})", route_words(map, stops), place(map, *end))
        }
        _ => route_words(map, route),
    }
}

/// Where a city's new units go, in words after "new units": "stay in the
/// city", "defend the city", "go to row 5, column 20", "go to rally point
/// 2, by a temple on the way".
pub fn sends_words(map: &Map, sends: &Sends) -> String {
    match sends {
        Sends::Stay => "stay in the city".to_string(),
        Sends::Defend => "defend the city".to_string(),
        Sends::Route(route) => format!("go to {}", route_words(map, route)),
        Sends::Point { n, bless: false } => format!("go to rally point {n}"),
        Sends::Point { n, bless: true } => format!("go to rally point {n}, by a temple on the way"),
    }
}

/// A line with its first letter a capital.
fn sentence(line: &str) -> String {
    let mut chars = line.chars();
    chars.next().map_or(String::new(), |first| first.to_uppercase().chain(chars).collect())
}

/// A side's name in a sentence: nobody's units are "nobody".
fn who(side: Side) -> &'static str {
    if side == Side::Grey { "nobody" } else { side_name(side) }
}

/// The heading of the selected hex: where it is, what it is, who is there.
pub fn hex_title(map: &Map, hex: Hex) -> String {
    hex_title_for(map, hex, &Sight::all())
}

/// [`hex_title`] of a hex that is `seen` or not (fog of war: in the fog, who
/// is there is not told; what was seen there last, `ghost`, is).
pub fn hex_title_for(map: &Map, hex: Hex, sight: &Sight) -> String {
    let Some(tile) = map.tile(hex) else { return String::new() };
    if !sight.knows(hex) {
        return format!("Row {}, column {}: {NEVER_SEEN}", hex.row, hex.col);
    }
    let seen = sight.sees(hex);
    let mut title = format!("Row {}, column {}: {}", hex.row, hex.col, describe_terrain(tile.kind, tile.road));
    if let Some(city) = map.city_at(hex) {
        title.push_str(&format!(" ({})", city.name));
    }
    // Each side once, in the order its first unit arrived.
    let mut sides: Vec<&str> = Vec::new();
    for side in tile.units().iter().filter_map(|id| map.units().get(*id)).map(|u| side_name(u.side())) {
        if !sides.contains(&side) {
            sides.push(side);
        }
    }
    match tile.units().len() {
        _ if !seen => match sight.ghost_at(hex) {
            Some(g) => title.push_str(&format!(", not seen: {} there in round {}", last_seen(g), g.round)),
            None => title.push_str(", not seen"),
        },
        0 => title.push_str(", no units"),
        1 => title.push_str(&format!(", 1 unit ({})", sides.join(", "))),
        n => title.push_str(&format!(", {n} units ({})", sides.join(", "))),
    }
    title
}

/// What land never seen is called (total fog of war).
pub const NEVER_SEEN: &str = "never seen";

/// Who held a city in the fog when it was seen last, as more words for its
/// name: "; held by blue in round 3", "; nobody's in round 1" (nothing if
/// it was never seen).
pub fn last_held(sight: &Sight, city: &City) -> String {
    match sight.held(city.pos) {
        Some((side, round)) => format!("; {} in round {round}", holder(side).to_lowercase()),
        None => String::new(),
    }
}

/// A stack remembered (fog of war) in words: "2 units (blue)", "1 unit
/// (nobody's)".
fn last_seen(ghost: &Ghost) -> String {
    let units = if ghost.count == 1 { "1 unit".to_string() } else { format!("{} units", ghost.count) };
    let whose = if ghost.side == Side::Grey { "nobody's".to_string() } else { side_name(ghost.side).to_string() };
    format!("{units} ({whose})")
}

/// A unit in numbers, under its icon in the stack: its strength, and the
/// movement points it has left of those it gets a turn ("4 · 7/10");
/// wounded, the strength its wounds leave it, starred ("2* · 7/10"; the
/// font has no arrows).
pub fn unit_numbers(unit: &Unit) -> String {
    let strength = if unit.wounds > 0 { format!("{}*", unit.fighting_power()) } else { unit.power().to_string() };
    format!("{strength} \u{b7} {}/{}", unit.movement, unit.base_move_points)
}

/// A unit's name with its side, and its rank if it has one to tell: "Riders
/// (red)", "Riders (red), veteran", "Captain Aldric (red), level 1".
fn named(map: &Map, unit: &Unit) -> String {
    let commander = map.is_commander(unit.id());
    let head = format!("{} ({})", unit.title(), side_name(unit.side()));
    if commander || unit.level > 1 { format!("{head}, {}", rank(unit.level, commander)) } else { head }
}

/// A unit's strength in words: "4", or with what its level and a blessing
/// add, "5 (4 +1)".
/// Wounded, what its wounds leave it: "2, wounded (whole, 4)".
fn strength_words(unit: &Unit) -> String {
    let (power, own) = (unit.power(), unit.strength);
    let whole = if power > own { format!("{power} ({own} +{})", power - own) } else { own.to_string() };
    if unit.wounds > 0 { format!("{}, wounded (whole, {whole})", crate::state::combat::wounded(power)) } else { whole }
}

/// What a unit has won and been given: its experience and how far the next
/// level is, the temples' blessings. None for a unit with neither.
fn growth_line(unit: &Unit) -> Option<String> {
    let mut parts = Vec::new();
    if unit.experience > 0 || unit.level > 1 {
        let next = crate::state::LEVEL_EXPERIENCE.get(usize::from(unit.level.max(1)) - 1);
        parts.push(match next {
            Some(need) => format!("Experience {} (next level at {need})", unit.experience),
            None => format!("Experience {} (the highest level)", unit.experience),
        });
    }
    if !unit.blessings.is_empty() {
        let words: Vec<String> = unit.blessings.iter().map(|(_, b)| b.words()).collect();
        let temples = if words.len() == 1 { "a temple".to_string() } else { format!("{} temples", words.len()) };
        parts.push(format!("blessed at {temples}: {}", words.join(", ")));
    }
    (!parts.is_empty()).then(|| sentence(&parts.join("; ")))
}

/// The selected unit in a line: who it is, its numbers, and its price (and
/// what it does for its stack, if it is a commander).
pub fn unit_line(map: &Map, unit: &Unit) -> String {
    let mut line = format!(
        "{}: strength {}, moves {}/{}, upkeep {}",
        named(map, unit),
        strength_words(unit),
        unit.movement,
        unit.base_move_points,
        unit.upkeep
    );
    if let Some(kind) = map.units().kind_of(unit) {
        line.push_str(&format!(", price {}", kind.price));
        let command = crate::state::command_bonus(kind.commander, unit.level);
        if command > 0 {
            line.push_str(&format!(", commands (+{command})"));
        }
        match kind.travel {
            Travel::Sea => line.push_str(&format!(", carries {}", kind.carries)),
            Travel::Air => line.push_str(", flies"),
            Travel::Land => {}
        }
    }
    line
}

/// A patrol in words: "patrolling 2 waypoints, never attacks",
/// "patrolling 4 waypoints, attacks at 65% or better".
pub fn patrol_words(patrol: &Patrol) -> String {
    let attack = match patrol.attack {
        None => "never attacks".to_string(),
        Some(odds) => format!("attacks at {odds}% or better"),
    };
    format!("patrolling {} waypoints, {attack}", patrol.stops.len())
}

/// How a patrol goes round its waypoints: "round and round" (a loop), or
/// "back and forth".
pub fn patrol_way(patrol: &Patrol) -> &'static str {
    if patrol.round { "round and round" } else { "back and forth" }
}

/// What a unit on patrol or marauding does, in words ("patrolling 3
/// waypoints ...", "marauding: the computer leads it"); `None` for one on
/// neither.
pub fn duty_words(unit: &Unit) -> Option<String> {
    match (&unit.patrol, unit.marauding) {
        (Some(patrol), _) => Some(patrol_words(patrol)),
        (None, true) => Some("marauding: the computer leads it".to_string()),
        (None, false) => None,
    }
}

/// What a stack on a standing order (`units`, its player's) is doing, in
/// words after "This unit is" or "These units are": "defending rally point
/// 0", "on the way to rally point 0 (row 25, column 5)", "patrolling 3
/// waypoints, ...", "marauding", "defending Redhold". `None` if none of them
/// is on one.
pub fn mission(game: &Game, units: &[&Unit]) -> Option<String> {
    let map = &game.map;
    if let Some(unit) = units.iter().find(|u| u.on_duty()) {
        return duty_words(unit).map(|w| w.replace(": the computer leads it", ", led by the computer"));
    }
    if let Some(unit) = units.iter().find(|u| !u.route.is_empty()) {
        let to = route_words_to(map, &unit.route, unit.bound);
        return Some(if unit.stalled {
            format!("on the way to {to} (stopped short)")
        } else {
            format!("on the way to {to}")
        });
    }
    let unit = units.iter().find(|u| u.defending)?;
    let at = unit.pos();
    let point =
        (0..crate::orders::RALLY_POINTS as u8).find(|n| game.rally_point(unit.side(), *n).is_some_and(|p| p.at == at));
    Some(match (point, map.city_at(at)) {
        (Some(n), _) => format!("defending rally point {n}"),
        (None, Some(city)) => format!("defending {}", city.name),
        (None, None) => "defending where they stand".to_string(),
    })
}

/// The line under the stack about a selected unit (of `count` selected)
/// that patrols or marauds: what it does, and what the keys do with it.
pub fn duty_line(map: &Map, unit: &Unit, count: usize) -> String {
    let who = if count > 1 { format!("{count} units") } else { named(map, unit) };
    match (&unit.patrol, unit.fought) {
        (Some(_), true) => format!("{who} fought on patrol: D lets it go on; another order ends the patrol."),
        (None, true) => format!("{who} fought, marauding: D lets it go on; U calls it back."),
        (Some(patrol), false) if unit.stalled => {
            format!(
                "{who}, {}: stopped short, something is in the way (D: try again at the turn's end).",
                patrol_words(patrol)
            )
        }
        (Some(patrol), false) => {
            format!("{who}, {}, {} (P: another patrol; a move ends it).", patrol_words(patrol), patrol_way(patrol))
        }
        (None, false) => format!("{who}, marauding: the computer leads it (U: call it back; a move ends it)."),
    }
}

/// What each terrain costs to enter, in words, a few to a line: "grass 2,
/// forest 3". What cannot be entered is left out.
pub fn enters(cost: &MoveCost, per_line: usize) -> Vec<String> {
    let words: Vec<String> = cost
        .prices()
        .iter()
        .filter(|(_, points)| *points > 0)
        .map(|(name, points)| format!("{name} {points}"))
        .collect();
    if words.is_empty() {
        return vec!["Enters nothing".to_string()];
    }
    let mut lines: Vec<String> = words.chunks(per_line.max(1)).map(|chunk| chunk.join(", ")).collect();
    lines[0].insert_str(0, "Enters: ");
    lines
}

/// Everything about a unit, a line at a time (the box over its icon): who it
/// is, its numbers, and what every terrain costs it to enter.
pub fn unit_details(map: &Map, unit: &Unit) -> Vec<String> {
    let mut lines = vec![named(map, unit)];
    let mut numbers = format!(
        "Strength {}   Moves {}/{}   Upkeep {}",
        strength_words(unit),
        unit.movement,
        unit.base_move_points,
        unit.upkeep
    );
    let mut leads = None;
    let mut travel = None;
    let mut knack = None;
    if let Some(kind) = map.units().kind_of(unit) {
        numbers.push_str(&format!("   Price {}", kind.price));
        let command = crate::state::command_bonus(kind.commander, unit.level);
        if command > 0 {
            leads = Some(format!("Commands: +{command} to its stack; fights last"));
        }
        travel = travel_line(kind);
        knack = knack_line(kind);
    }
    lines.push(numbers);
    lines.extend(leads);
    lines.extend(knack);
    lines.extend(travel);
    lines.extend(growth_line(unit));
    if unit.wounds > 0 {
        lines.push("Wounded: half as strong until its side's turn ends, when it heals".to_string());
    }
    if unit.defending {
        lines.push("Defending: it stays here until it is sent somewhere".to_string());
    }
    if let Some(duty) = duty_words(unit) {
        lines.push(format!("{}, until it is given another order", sentence(&duty)));
    }
    match map.units().move_cost(unit.move_cost) {
        Some(cost) => lines.extend(enters(cost, 5)),
        None => lines.push("Enters nothing".to_string()),
    }
    lines
}

/// Who holds a city: "Held by red", or "Nobody's".
pub fn holder(side: Side) -> String {
    match side_name(side) {
        "neutral" => "Nobody's".to_string(),
        side => format!("Held by {side}"),
    }
}

/// The name of the kind a city builds, if the map has the kind (else its id).
fn kind_name<'a>(map: &'a Map, prod: &'a UnitProd) -> &'a str {
    let kinds = map.units();
    kinds.kind_named(&prod.basename).and_then(|k| kinds.kind(k)).map_or(&prod.basename, |k| &k.name)
}

fn turns(n: u32) -> String {
    if n == 1 { "1 turn".to_string() } else { format!("{n} turns") }
}

/// One of what a city can build: "Riders: 3 turns, price 10".
pub fn production_option(map: &Map, prod: &UnitProd) -> String {
    format!("{}: {}, price {}", kind_name(map, prod), turns(prod.time), prod.cost)
}

/// What a city can build, in a few words for its button: the unit's name,
/// and its strength and moves a turn ("Riders  4 · 18").
pub fn production_button(map: &Map, prod: &UnitProd) -> String {
    format!("{}  {} \u{b7} {}", kind_name(map, prod), prod.strength, prod.movement)
}

/// What a city can build, in full: the unit's strength and moves (and what it
/// does for its stack, a commander); the turns it takes here, its price
/// here, its upkeep; what each terrain costs it to enter.
pub fn production_details(map: &Map, prod: &UnitProd, trains: u8) -> Vec<String> {
    let kind = map.units().kind_named(&prod.basename).and_then(|k| map.units().kind(k));
    let name = kind_name(map, prod);
    let mut lines = vec![format!("{name}: strength {}, moves {}", prod.strength, prod.movement)];
    lines.push(format!("Builds in {}, price {}; upkeep {}", turns(prod.time), prod.cost, prod.upkeep));
    if trains > 1 {
        let commander = kind.is_some_and(|k| k.commander > 0);
        lines.push(format!("Trained here: it starts {}", rank(trains, commander)));
    }
    if let Some(kind) = kind {
        if kind.commander > 0 {
            lines.push(format!("Commands: +{} to its stack; fights last", kind.commander));
        }
        lines.extend(knack_line(kind));
        lines.extend(travel_line(kind));
        lines.extend(enters(&kind.cost, 4));
    }
    lines
}

/// What a kind fights the better for, in a line: "Fights +2 in forests, +1
/// in swamps; breaks 1 of the walls it attacks". `None` for one with
/// nothing.
pub fn knack_line(kind: &crate::state::UnitKind) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if !kind.bonus.is_empty() {
        let grounds: Vec<String> = kind
            .bonus
            .iter()
            .map(|(g, n)| {
                let place = match g.as_str() {
                    "city" => "in a city",
                    "grass" => "on grass",
                    "water" => "on water",
                    "forest" => "in forests",
                    "swamp" => "in swamps",
                    "desert" => "in deserts",
                    "hill" => "in the hills",
                    "mountain" => "in the mountains",
                    other => other,
                };
                format!("+{n} {place}")
            })
            .collect();
        parts.push(format!("Fights {}", grounds.join(", ")));
    }
    if kind.siege > 0 {
        parts.push(format!("breaks {} of the walls it attacks", kind.siege));
    }
    if parts.is_empty() {
        return None;
    }
    let line = parts.join("; ");
    let mut chars = line.chars();
    chars.next().map(|first| first.to_uppercase().chain(chars).collect())
}

/// What a ship or a flyer does that others do not, in a line: "A ship:
/// carries 6 and a commander", "Flies: a commander with it flies too".
pub fn travel_line(kind: &crate::state::UnitKind) -> Option<String> {
    match kind.travel {
        Travel::Land => None,
        Travel::Sea if kind.carries == 1 => Some("A ship: carries 1 and a commander".to_string()),
        Travel::Sea => Some(format!("A ship: carries {} and a commander", kind.carries)),
        Travel::Air => Some("Flies: a commander with it flies too".to_string()),
    }
}

/// What a city is building and how far it has got: "Building Riders, 1 of 3
/// turns done", or "Building nothing". A unit that is done and still in the
/// works waits for its price, or for room in the city: "Riders done: they
/// wait for 10 gold".
pub fn progress_line(map: &Map, city: &City) -> String {
    match city.production() {
        Some(prod) if city.progress >= prod.time => {
            format!("{} done: {}{}", kind_name(map, prod), waits(map, city, prod), queue_short(city))
        }
        Some(prod) => format!(
            "Building {}, {} of {} done{}",
            kind_name(map, prod),
            city.progress,
            turns(prod.time),
            queue_short(city)
        ),
        None if city.resting => "Building nothing, as you said".to_string(),
        None => "Building nothing".to_string(),
    }
}

/// Where a city is in its queue, if it has one, after what it builds: " (2
/// of 3; item 1 of 4)".
pub fn queue_short(city: &City) -> String {
    let queue = &city.queue;
    let Some(item) = queue.current() else { return String::new() };
    format!(" ({} of {}; item {} of {})", queue.made + 1, item.count, queue.at + 1, queue.items.len())
}

/// An item of a city's queue in a line, short: "3 Light Infantry: defend",
/// "1 Riders: to point 0, blessed" (by a temple on the way), "2 Militia: to
/// 5, 20" (a row and a column).
pub fn queue_item_line(map: &Map, city: &City, i: usize) -> String {
    let Some(item) = city.queue.items.get(i) else { return String::new() };
    let name = city.units.get(item.what).map_or("?", |prod| kind_name(map, prod));
    let to = match &item.sends {
        Sends::Stay => "stay".to_string(),
        Sends::Defend => "defend".to_string(),
        Sends::Route(route) => match route.split_last() {
            Some((end, [])) => format!("to {}, {}", end.row, end.col),
            Some((end, stops)) => format!("to {}, {} by {} stops", end.row, end.col, stops.len()),
            None => "nowhere".to_string(),
        },
        Sends::Point { n, bless: false } => format!("to point {n}"),
        Sends::Point { n, bless: true } => format!("to point {n}, blessed"),
    };
    format!("{} {name}: {to}", item.count)
}

/// What a city's queue does after its last item, in a line.
pub fn queue_end_line(city: &City) -> String {
    match city.queue.end {
        QueueEnd::Stop => "Then it stops, and waits for orders.".to_string(),
        QueueEnd::Repeat { from, times: None } => format!("Then again from item {}, for ever.", from + 1),
        QueueEnd::Repeat { from, times: Some(t) } => match t.saturating_sub(city.queue.rounds) {
            0 => format!("It is going round from item {} the last time; then it stops.", from + 1),
            1 => format!("Then again from item {}, once more; then it stops.", from + 1),
            left => format!("Then again from item {}, {left} times more; then it stops.", from + 1),
        },
    }
}

/// What a city is building, short, for the sidebar: "Building Riders, 1/3",
/// or "Riders: wait for 10 gold".
pub fn progress_short(map: &Map, city: &City) -> String {
    match city.production() {
        Some(prod) if city.progress >= prod.time => format!("{}: {}", kind_name(map, prod), waits(map, city, prod)),
        Some(prod) if city.queue.is_empty() => {
            format!("Building {}, {}/{}", kind_name(map, prod), city.progress, prod.time)
        }
        Some(prod) => format!(
            "{} {}/{}, item {}/{}",
            kind_name(map, prod),
            city.progress,
            prod.time,
            city.queue.at + 1,
            city.queue.items.len()
        ),
        None if city.resting => "Building nothing, on purpose".to_string(),
        None => "Building nothing".to_string(),
    }
}

/// Why a unit that is done is not out yet: no room in the city (a hex holds
/// eight units, and a commander besides), or no gold.
fn waits(map: &Map, city: &City, prod: &UnitProd) -> String {
    let units = map.units();
    let commander = units.kind_named(&prod.basename).and_then(|k| units.kind(k)).is_some_and(|k| k.commander > 0);
    let here = map.units_at(city.pos);
    let leaders = here.iter().filter(|u| map.is_commander(**u)).count();
    let full = if commander { leaders > 0 } else { here.len() - leaders >= crate::state::MAX_STACK };
    if full { "they wait for room".to_string() } else { format!("they wait for {} gold", prod.cost) }
}

/// A side's treasury in two lines, for the sidebar: "Gold 34", and what comes
/// in and goes out a turn, "Taxes +40, upkeep -32".
pub fn treasury_lines(game: &Game, side: Side) -> [String; 2] {
    let (income, upkeep) = (game.income(side), game.upkeep(side));
    [format!("Gold {}", game.gold(side)), format!("Taxes +{income}, upkeep -{upkeep}")]
}

/// A city's holder, walls and place in a line: "Held by red, walls 2, at row
/// 13, column 7".
pub fn city_summary(city: &City) -> String {
    format!("{}, walls {}, at row {}, column {}", holder(city.side), city.walls, city.pos.row, city.pos.col)
}

/// What a city brings its holder: "Taxes: 20 gold a turn".
pub fn income_line(city: &City) -> String {
    format!("Taxes: {} gold a turn", city.income)
}

/// Who stands in a city, kind by kind in the order they stand: "Garrison: 2
/// Militia, 1 Riders", or "No garrison".
pub fn garrison_line(map: &Map, city: &City) -> String {
    let mut kinds: Vec<(&str, usize)> = Vec::new();
    for unit in map.units_at(city.pos).iter().filter_map(|id| map.units().get(*id)) {
        match kinds.iter_mut().find(|(name, _)| *name == unit.name) {
            Some((_, n)) => *n += 1,
            None => kinds.push((&unit.name, 1)),
        }
    }
    if kinds.is_empty() {
        return "No garrison".to_string();
    }
    let kinds: Vec<String> = kinds.iter().map(|(name, n)| format!("{n} {name}")).collect();
    format!("Garrison: {}", kinds.join(", "))
}

/// A city's row in a list of cities: its name and who holds it, or, in a list
/// of one's own, what it is building ("Brindlewick: Riders, 1 of 3").
pub fn city_row(map: &Map, city: &City, own: bool) -> String {
    if !own {
        return match side_name(city.side) {
            "neutral" => format!("{} (nobody's)", city.name),
            side => format!("{} ({side})", city.name),
        };
    }
    match city.production() {
        Some(prod) => format!("{}: {}, {} of {}", city.name, kind_name(map, prod), city.progress, prod.time),
        None => format!("{}: building nothing", city.name),
    }
}

/// A city, a line at a time, in at most `room` lines: its name, who holds it,
/// its walls and garrison, what it is building, and what it can build (the
/// unit, the turns one takes, its price).
/// A task (`Game::tasks`) in a line, for the list of tasks: what it is,
/// where, and what is left to do with it. "Riders at row 5, column 9: no
/// orders yet", "3 units in Brindlewick: moved, with moves left", "Brindlewick:
/// builds nothing".
pub fn task_line(game: &Game, task: &Task) -> String {
    let map = &game.map;
    match task {
        Task::Stack { at, units, state } => {
            let here: Vec<&Unit> = map.units_at(*at).iter().filter_map(|id| map.units().get(*id)).collect();
            let who = match (here.first(), units) {
                (Some(unit), 1) => unit.title(),
                _ => format!("{units} units"),
            };
            let here_words = match map.city_at(*at) {
                Some(city) => format!("in {}", city.name),
                None => format!("at row {}, column {}", at.row, at.col),
            };
            let goal = here
                .iter()
                .find(|u| !u.route.is_empty())
                .map_or(String::new(), |u| route_words_to(map, &u.route, u.bound));
            // On patrol, or marauding.
            let patrol = here.iter().find_map(|u| u.patrol.as_ref());
            let duty = here.iter().find_map(|u| duty_words(u)).unwrap_or_default();
            let what = match state {
                StackState::Waiting => "no orders yet".to_string(),
                StackState::Stalled if goal.is_empty() && patrol.is_some() => {
                    format!(
                        "stopped on patrol, on the way to {}",
                        patrol.map_or(String::new(), |p| place(map, p.target()))
                    )
                }
                StackState::Stalled => format!("stopped on the way to {goal}"),
                StackState::MovesLeft => "moved, with moves left".to_string(),
                StackState::Done => "put aside, with moves left".to_string(),
                StackState::Marching => format!("on the way to {goal}"),
                StackState::Defending => "defending: stays put".to_string(),
                StackState::Spent => "no moves left".to_string(),
                StackState::Patrolling | StackState::Marauding => duty,
                StackState::Fought if patrol.is_some() => "fought on patrol: D lets it go on".to_string(),
                StackState::Fought => "fought, marauding: D lets it go on".to_string(),
            };
            format!("{who} {here_words}: {what}")
        }
        Task::City { city, state } => {
            let Some(city) = map.cities().get(*city) else { return String::new() };
            let what = match (state, city.production()) {
                (CityState::Waiting, Some(prod)) => {
                    format!("{} done, {}", kind_name(map, prod), waits(map, city, prod))
                }
                (CityState::Building, Some(prod)) => {
                    format!("building {}, {}/{}{}", kind_name(map, prod), city.progress, prod.time, queue_short(city))
                }
                (CityState::Resting, _) => "builds nothing, as you said".to_string(),
                _ => "builds nothing".to_string(),
            };
            match city.sends {
                Sends::Stay => format!("{}: {what}", city.name),
                _ => format!("{}: {what}; new units {}", city.name, sends_words(map, &city.sends)),
            }
        }
    }
}

/// What a side knows of a city: all of it (its own), what shows from
/// outside (another's, or nobody's, in sight), or where it is (in the fog),
/// and who held it, in which round, when it was seen last.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Known {
    All,
    Outside,
    /// In the fog: who held it and when it was seen last, and its walls
    /// then.
    Fog(Option<(Side, u32)>, Option<u32>),
}

impl Known {
    /// What `side`, which sees as `sight`, knows of `city`. (Nobody,
    /// `Side::Grey`, a map without sides: all.)
    pub fn of(city: &City, side: Side, sight: &Sight) -> Known {
        if side == Side::Grey || city.side == side {
            Known::All
        } else if sight.sees(city.pos) {
            Known::Outside
        } else {
            Known::Fog(sight.held(city.pos), sight.walls_seen(city.pos))
        }
    }
}

/// [`city_lines`] as much as is `known`: from outside, its name, holder,
/// walls and taxes, and how many stand in it; in the fog, its name, walls
/// and taxes (what it builds is its holder's to know).
pub fn city_lines_for(map: &Map, city: &City, room: usize, known: Known) -> Vec<String> {
    match known {
        Known::All => city_lines(map, city, room),
        Known::Outside => {
            let garrison = match map.units_at(city.pos).len() {
                0 => "No garrison".to_string(),
                1 => "Garrison: 1 unit".to_string(),
                n => format!("Garrison: {n} units"),
            };
            vec![city.name.clone(), holder(city.side), format!("Walls {}, taxes {}", city.walls, city.income), garrison]
        }
        Known::Fog(held, seen) => {
            let walls = format!("Walls {}, taxes {}", seen.unwrap_or(city.walls), city.income);
            match held {
                Some((side, round)) => {
                    vec![
                        city.name.clone(),
                        format!("Not seen since round {round}"),
                        format!("{} then", holder(side)),
                        walls,
                    ]
                }
                None => vec![city.name.clone(), "Not seen".to_string(), walls],
            }
        }
    }
}

pub fn city_lines(map: &Map, city: &City, room: usize) -> Vec<String> {
    let mut lines = vec![city.name.clone()];
    lines.push(holder(city.side));
    lines.push(format!("Walls {}, taxes {}", city.walls, city.income));
    lines.push(match map.units_at(city.pos).len() {
        0 => "No garrison".to_string(),
        1 => "Garrison: 1 unit".to_string(),
        n => format!("Garrison: {n} units"),
    });
    if city.units.is_empty() {
        lines.push("Builds nothing".to_string());
    } else {
        lines.push(progress_short(map, city));
        lines.push("Can build:".to_string());
        for prod in &city.units {
            lines.push(format!("  {}", production_option(map, prod)));
        }
    }
    if lines.len() > room && room > 0 {
        let more = lines.len() - (room - 1);
        lines.truncate(room - 1);
        lines.push(format!("  ... and {more} more"));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::SelectOutcome;
    use crate::state::Side;
    use crate::state::scenario::{self, Scenario};

    /// A city with a road east over a bridge, three units in it, and a
    /// second city that is nobody's.
    fn game() -> Game {
        let mut rows = vec!["g".repeat(20); 20];
        rows[5] = "gggggcGGWGgggggcgggg".to_string();
        let terrain = format!("20\n20\n{}\n", rows.join("\n"));
        let units = "unit riders\nname Riders\nstrength 4\nmoves 10\nupkeep 3\nprice 9\nenter grass 2, forest 5, city 1, road 1, bridge 1\n\
                     unit militia\nname Militia\nstrength 2\nmoves 8\nenter grass 2, forest 3, hill 4, swamp 5, desert 3, city 1, road 1\n";
        let cities = "city 5 5\nname Wrenfold\nside blue\nwalls 2\nproduce riders 3\nproduce militia 1\ngarrison riders 2\ngarrison militia\n\
                      city 5 15\nname Low Tarn\n";
        let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
        Game::from_scenario(Scenario { map, assets: None, warnings })
    }

    #[test]
    fn the_line_about_the_hex_under_the_cursor() {
        let mut game = game();
        assert_eq!(cursor_line(&game), "Turn 1  |  row 5, column 5: city  |  Wrenfold  |  3 units");
        game.set_cursor(Hex::new(5, 8));
        assert_eq!(cursor_line(&game), "Turn 1  |  row 5, column 8: water, bridge");
        // With a unit selected: what it costs to go there, of what it has.
        let rider = game.map.units_at(Hex::new(5, 5))[0];
        assert!(game.select_unit(rider));
        assert_eq!(cursor_line(&game), "Turn 1  |  row 5, column 8: water, bridge  |  3 of 10 points to move here");
        game.set_cursor(Hex::new(5, 5));
        assert_eq!(cursor_line(&game), "Turn 1  |  row 5, column 5: city  |  Wrenfold  |  3 units");
        game.set_cursor(Hex::new(18, 18));
        assert_eq!(cursor_line(&game), "Turn 1  |  row 18, column 18: grass", "out of reach: no price");
        game.select_hex(Hex::new(5, 6));
        game.end_turn();
        game.set_cursor(Hex::new(5, 6));
        assert_eq!(cursor_line(&game), "Turn 2  |  row 5, column 6: grass, road  |  1 unit");
        // The stack: whether all of it can go there (the militia has no
        // bridge to cross).
        game.end_turn();
        assert!(matches!(game.select_hex(Hex::new(5, 5)), SelectOutcome::Selected(_)));
        assert_eq!(game.selection().len(), 2);
        game.set_cursor(Hex::new(5, 7));
        assert_eq!(cursor_line(&game), "Turn 3  |  row 5, column 7: grass, road  |  the 2 units can move here");
        game.set_cursor(Hex::new(5, 8));
        assert_eq!(cursor_line(&game), "Turn 3  |  row 5, column 8: water, bridge");
        // Another side's units: an attack, and how likely it is to be won.
        let cost = game.map.acquire_move_cost(MoveCost { grass: 1, ..MoveCost::default() });
        let raider = game.map.new_unit(Side::Red, "Raider", 0, 1, 5, cost);
        game.map.place_unit(raider, Hex::new(5, 7)).unwrap();
        game.set_cursor(Hex::new(5, 7));
        let line = cursor_line(&game);
        let odds: u32 = line.rsplit("attack: ").next().unwrap().split('%').next().unwrap().parse().unwrap();
        assert!(line.ends_with("% likely to win") && odds > 80, "{line}");
    }

    #[test]
    fn the_selected_hexs_heading() {
        let mut game = game();
        assert_eq!(hex_title(&game.map, Hex::new(5, 5)), "Row 5, column 5: city (Wrenfold), 3 units (blue)");
        assert_eq!(hex_title(&game.map, Hex::new(5, 15)), "Row 5, column 15: city (Low Tarn), no units");
        assert_eq!(hex_title(&game.map, Hex::new(5, 7)), "Row 5, column 7: grass, road, no units");
        assert_eq!(hex_title(&game.map, Hex::new(40, 0)), "");
        game.select_unit(game.map.units_at(Hex::new(5, 5))[0]);
        game.select_hex(Hex::new(5, 7));
        assert_eq!(hex_title(&game.map, Hex::new(5, 7)), "Row 5, column 7: grass, road, 1 unit (blue)");
        // Units of two sides on a hex: both are named.
        let cost = game.map.acquire_move_cost(MoveCost { grass: 1, ..MoveCost::default() });
        let other = game.map.new_unit(Side::Red, "Raider", 0, 1, 5, cost);
        game.map.place_unit(other, Hex::new(5, 7)).unwrap();
        assert_eq!(hex_title(&game.map, Hex::new(5, 7)), "Row 5, column 7: grass, road, 2 units (blue, red)");
        // And once each, however the units of the sides came to stand there.
        let third = game.map.new_unit(Side::Blue, "Scout", 0, 1, 5, cost);
        game.map.place_unit(third, Hex::new(5, 7)).unwrap();
        assert_eq!(hex_title(&game.map, Hex::new(5, 7)), "Row 5, column 7: grass, road, 3 units (blue, red)");
    }

    #[test]
    fn a_unit_in_numbers_in_a_line_and_in_full() {
        let mut game = game();
        let city = Hex::new(5, 5);
        let ids = game.map.units_at(city).to_vec();
        let unit = |game: &Game, i: usize| game.map.units().get(ids[i]).unwrap().clone();
        assert_eq!(unit_numbers(&unit(&game, 0)), "4 \u{b7} 10/10");
        assert_eq!(unit_numbers(&unit(&game, 2)), "2 \u{b7} 8/8");
        assert_eq!(unit_line(&game.map, &unit(&game, 0)), "Riders (blue): strength 4, moves 10/10, upkeep 3, price 9");
        assert_eq!(
            unit_details(&game.map, &unit(&game, 0)),
            [
                "Riders (blue)",
                "Strength 4   Moves 10/10   Upkeep 3   Price 9",
                "Enters: grass 2, forest 5, city 1, road 1, bridge 1",
            ]
        );
        // More terrains than fit a line go on to the next.
        assert_eq!(
            unit_details(&game.map, &unit(&game, 2)),
            [
                "Militia (blue)",
                "Strength 2   Moves 8/8   Upkeep 0   Price 0",
                "Enters: grass 2, forest 3, swamp 5, desert 3, hill 4",
                "city 1, road 1",
            ]
        );
        // After a move the points that are left show.
        game.select_unit(ids[0]);
        game.select_hex(Hex::new(5, 8));
        assert_eq!(unit_numbers(&unit(&game, 0)), "4 \u{b7} 7/10");
        assert!(unit_line(&game.map, &unit(&game, 0)).contains("moves 7/10"));
        // A unit made by hand has no kind, and so no price; one that enters
        // nothing says so.
        let cost = game.map.acquire_move_cost(MoveCost::default());
        let stone = game.map.new_unit(Side::Grey, "Stone", 0, 9, 1, cost);
        let stone = game.map.units().get(stone).unwrap().clone();
        assert_eq!(unit_line(&game.map, &stone), "Stone (neutral): strength 9, moves 1/1, upkeep 0");
        assert_eq!(
            unit_details(&game.map, &stone),
            ["Stone (neutral)", "Strength 9   Moves 1/1   Upkeep 0", "Enters nothing"]
        );
    }

    #[test]
    fn a_city_line_by_line() {
        let game = game();
        let wrenfold = game.map.city_at(Hex::new(5, 5)).unwrap();
        assert_eq!(
            city_lines(&game.map, wrenfold, 8),
            [
                "Wrenfold",
                "Held by blue",
                "Walls 2, taxes 10",
                "Garrison: 3 units",
                "Building nothing",
                "Can build:",
                "  Riders: 3 turns, price 9",
                "  Militia: 1 turn, price 0",
            ]
        );
        let tarn = game.map.city_at(Hex::new(5, 15)).unwrap();
        assert_eq!(
            city_lines(&game.map, tarn, 8),
            ["Low Tarn", "Nobody's", "Walls 0, taxes 10", "No garrison", "Builds nothing"]
        );
        // Less room than lines: the last says how many are left out.
        assert_eq!(
            city_lines(&game.map, wrenfold, 6),
            [
                "Wrenfold",
                "Held by blue",
                "Walls 2, taxes 10",
                "Garrison: 3 units",
                "Building nothing",
                "  ... and 3 more"
            ]
        );
    }

    #[test]
    fn a_city_in_full_and_in_a_row_of_a_list() {
        let mut game = game();
        let id = game.map.city_at(Hex::new(5, 5)).unwrap().id;
        let city = |game: &Game| game.map.cities().get(id).unwrap().clone();
        assert_eq!(city_summary(&city(&game)), "Held by blue, walls 2, at row 5, column 5");
        assert_eq!(garrison_line(&game.map, &city(&game)), "Garrison: 2 Riders, 1 Militia");
        assert_eq!(production_option(&game.map, &city(&game).units[0]), "Riders: 3 turns, price 9");
        assert_eq!(production_option(&game.map, &city(&game).units[1]), "Militia: 1 turn, price 0");
        assert_eq!(progress_line(&game.map, &city(&game)), "Building nothing");
        assert_eq!(city_row(&game.map, &city(&game), false), "Wrenfold (blue)");
        assert_eq!(city_row(&game.map, &city(&game), true), "Wrenfold: building nothing");
        // Building riders, a turn done.
        game.map.set_production(id, Some(0));
        game.map.produce(Side::Blue, &mut 1000);
        assert_eq!(progress_line(&game.map, &city(&game)), "Building Riders, 1 of 3 turns done");
        assert_eq!(city_row(&game.map, &city(&game), true), "Wrenfold: Riders, 1 of 3");
        assert_eq!(city_lines(&game.map, &city(&game), 9)[4], "Building Riders, 1/3");
        // Nobody's, and empty.
        let tarn = game.map.city_at(Hex::new(5, 15)).unwrap().clone();
        assert_eq!(city_summary(&tarn), "Nobody's, walls 0, at row 5, column 15");
        assert_eq!(garrison_line(&game.map, &tarn), "No garrison");
        assert_eq!(city_row(&game.map, &tarn, false), "Low Tarn (nobody's)");
    }
}
