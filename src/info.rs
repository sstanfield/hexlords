//! What the panels say: the line about the hex under the cursor; the selected
//! hex, its units' numbers under their icons, the selected unit in a line and
//! any unit in full (the box the pointer opens); the selected city beside the
//! map. Words only, from the game's state; `session` puts them into widgets.
//!
//! Not a port: the C++ had no panels.

use crate::game::{Game, describe_terrain};
use crate::orders::{Event, Fallen, Record};
use crate::state::scenario::side_name;
use crate::state::{City, Hex, Map, MoveCost, Side, Unit, UnitProd};

/// The line about the hex under the cursor: the turn, the hex's terrain and
/// road, its city's name, how many units stand there, and, if the selected
/// units can go there, what it costs (one unit) or that they can (a stack),
/// and the odds of winning if going there is an attack.
pub fn cursor_line(game: &Game) -> String {
    let (map, hex) = (&game.map, game.map.cursor);
    let mut parts = vec![format!("Turn {}", game.turn())];
    if let Some(tile) = map.tile(hex) {
        parts.push(format!("row {}, column {}: {}", hex.row, hex.col, describe_terrain(tile.kind, tile.road)));
        if let Some(city) = map.city_at(hex) {
            parts.push(city.name.clone());
        }
        match tile.units().len() {
            0 => {}
            1 => parts.push("1 unit".to_string()),
            n => parts.push(format!("{n} units")),
        }
    }
    let selected = game.selected().and_then(|id| map.units().get(id));
    if let (Some(unit), Some(cost)) = (selected, game.range().and_then(|r| r.cost(hex))) {
        match game.battle_odds(game.selection(), hex) {
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
/// built). Units that left it unpaid are told too.
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
        Event::Taken { city, from, by } if *by == side || *from == side => {
            let name = map.cities().get(*city).map_or("a city".to_string(), |c| c.name.clone());
            if *by == side {
                vec![format!("You took {name} from {}.", who(*from))]
            } else {
                vec![format!("{} took {name} from you.", sentence(side_name(*by)))]
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
        Event::Deserted { side: whose, name } if *whose == side => {
            vec![format!("Unpaid, your {name} left you: the treasury could not pay their upkeep.")]
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

/// Where a battle was: its city's name, or the hex.
fn place(map: &Map, at: Hex) -> String {
    map.city_at(at).map_or(format!("row {}, column {}", at.row, at.col), |c| c.name.clone())
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
    let Some(tile) = map.tile(hex) else { return String::new() };
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
        0 => title.push_str(", no units"),
        1 => title.push_str(&format!(", 1 unit ({})", sides.join(", "))),
        n => title.push_str(&format!(", {n} units ({})", sides.join(", "))),
    }
    title
}

/// A unit in numbers, under its icon in the stack: its strength, and the
/// movement points it has left of those it gets a turn ("4 · 7/10").
pub fn unit_numbers(unit: &Unit) -> String {
    format!("{} \u{b7} {}/{}", unit.power(), unit.movement, unit.base_move_points)
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
fn strength_words(unit: &Unit) -> String {
    let (power, own) = (unit.power(), unit.strength);
    if power > own { format!("{power} ({own} +{})", power - own) } else { own.to_string() }
}

/// What a unit has won and been given: its experience and how far the next
/// level is, a temple's blessing. None for a unit with neither.
fn growth_line(unit: &Unit) -> Option<String> {
    let mut parts = Vec::new();
    if unit.experience > 0 || unit.level > 1 {
        let next = crate::state::LEVEL_EXPERIENCE.get(usize::from(unit.level.max(1)) - 1);
        parts.push(match next {
            Some(need) => format!("Experience {} (next level at {need})", unit.experience),
            None => format!("Experience {} (the highest level)", unit.experience),
        });
    }
    if let Some(blessing) = unit.blessing {
        parts.push(format!("blessed: {}", blessing.words()));
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
    }
    line
}

/// What each terrain costs to enter, in words, a few to a line: "grass 2,
/// forest 3". What cannot be entered is left out.
fn enters(cost: &MoveCost, per_line: usize) -> Vec<String> {
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
    if let Some(kind) = map.units().kind_of(unit) {
        numbers.push_str(&format!("   Price {}", kind.price));
        let command = crate::state::command_bonus(kind.commander, unit.level);
        if command > 0 {
            leads = Some(format!("Commands: +{command} to its stack; fights last"));
        }
    }
    lines.push(numbers);
    lines.extend(leads);
    lines.extend(growth_line(unit));
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
        lines.extend(enters(&kind.cost, 4));
    }
    lines
}

/// What a city is building and how far it has got: "Building Riders, 1 of 3
/// turns done", or "Building nothing". A unit that is done and still in the
/// works waits for its price, or for room in the city: "Riders done: they
/// wait for 10 gold".
pub fn progress_line(map: &Map, city: &City) -> String {
    match city.production() {
        Some(prod) if city.progress >= prod.time => {
            format!("{} done: {}", kind_name(map, prod), waits(map, city, prod))
        }
        Some(prod) => format!("Building {}, {} of {} done", kind_name(map, prod), city.progress, turns(prod.time)),
        None => "Building nothing".to_string(),
    }
}

/// What a city is building, short, for the sidebar: "Building Riders, 1/3",
/// or "Riders: wait for 10 gold".
pub fn progress_short(map: &Map, city: &City) -> String {
    match city.production() {
        Some(prod) if city.progress >= prod.time => format!("{}: {}", kind_name(map, prod), waits(map, city, prod)),
        Some(prod) => format!("Building {}, {}/{}", kind_name(map, prod), city.progress, prod.time),
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
