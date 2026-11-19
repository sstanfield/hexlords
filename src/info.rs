//! What the panels say: the line about the hex under the cursor; the selected
//! hex, its units' numbers under their icons, the selected unit in a line and
//! any unit in full (the box the pointer opens); the selected city beside the
//! map. Words only, from the game's state; `session` puts them into widgets.
//!
//! Not a port: the C++ had no panels.

use crate::game::{Game, describe_terrain};
use crate::orders::Event;
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

/// A battle, a line at a time, for the one who fought it: who attacked whom
/// where, with how many, what each side lost, and who won (and the city, if
/// one was taken). Nothing for an event that is not a battle.
pub fn battle_lines(map: &Map, events: &[Event]) -> Vec<String> {
    let mut lines = Vec::new();
    for event in events {
        if let Event::Battle { at, attacker, defender, attackers, defenders, attackers_lost, defenders_lost, won } =
            event
        {
            let (a, d) = (side_name(*attacker), who(*defender));
            lines.push(sentence(&format!("{a} attacked {d} at {}, {attackers} against {defenders}.", place(map, *at))));
            lines.push(sentence(&format!("{a} lost {attackers_lost}, {d} {defenders_lost}.")));
            lines.push(sentence(&if *won { format!("{a} won.") } else { format!("{d} held.") }));
        }
        if let Event::Taken { city, by, .. } = event {
            if let Some(city) = map.cities().get(*city) {
                lines.push(sentence(&format!("{} took {}.", side_name(*by), city.name)));
            }
        }
    }
    lines
}

/// What happened to `side` while others played, in a line, if the event was
/// about it: a battle it fought, a city it lost, a side that is out.
pub fn news_line(map: &Map, event: &Event, side: Side) -> Option<String> {
    let line = match event {
        Event::Battle { at, attacker, defender, attackers_lost, defenders_lost, won, .. } if *defender == side => {
            let how = if *won { "lost it" } else { "held" };
            let (by, at) = (side_name(*attacker), place(map, *at));
            format!("{by} attacked you at {at}: you {how}, losing {defenders_lost}; they lost {attackers_lost}.")
        }
        Event::Battle { at, attacker, defender, attackers_lost, defenders_lost, won, .. } if *attacker == side => {
            let how = if *won { "won" } else { "lost" };
            let (whom, at) = (who(*defender), place(map, *at));
            format!("You attacked {whom} at {at}: you {how}, losing {attackers_lost}; they lost {defenders_lost}.")
        }
        Event::Taken { city, from, by } if *from == side => {
            let name = map.cities().get(*city).map_or("a city".to_string(), |c| c.name.clone());
            format!("{} took {name} from you.", side_name(*by))
        }
        Event::Out { side: out } if *out != side => format!("{} is out of the game.", side_name(*out)),
        _ => return None,
    };
    Some(sentence(&line))
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
    format!("{} \u{b7} {}/{}", unit.strength, unit.movement, unit.base_move_points)
}

/// The selected unit in a line: who it is, its numbers, and its price (and
/// what it does for its stack, if it is a commander).
pub fn unit_line(map: &Map, unit: &Unit) -> String {
    let mut line = format!(
        "{} ({}): strength {}, moves {}/{}, upkeep {}",
        unit.name,
        side_name(unit.side()),
        unit.strength,
        unit.movement,
        unit.base_move_points,
        unit.upkeep
    );
    if let Some(kind) = map.units().kind_of(unit) {
        line.push_str(&format!(", price {}", kind.price));
        if kind.commander > 0 {
            line.push_str(&format!(", commands (+{})", kind.commander));
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
    let mut lines = vec![format!("{} ({})", unit.name, side_name(unit.side()))];
    let mut numbers = format!(
        "Strength {}   Moves {}/{}   Upkeep {}",
        unit.strength, unit.movement, unit.base_move_points, unit.upkeep
    );
    let mut leads = None;
    if let Some(kind) = map.units().kind_of(unit) {
        numbers.push_str(&format!("   Price {}", kind.price));
        if kind.commander > 0 {
            leads = Some(format!("Commands: its stack fights {} stronger; it fights last", kind.commander));
        }
    }
    lines.push(numbers);
    lines.extend(leads);
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

/// What a city is building and how far it has got: "Building Riders, 1 of 3
/// turns done", or "Building nothing".
pub fn progress_line(map: &Map, city: &City) -> String {
    match city.production() {
        Some(prod) => format!("Building {}, {} of {} done", kind_name(map, prod), city.progress, turns(prod.time)),
        None => "Building nothing".to_string(),
    }
}

/// What a city is building, short, for the sidebar: "Building Riders, 1/3".
pub fn progress_short(map: &Map, city: &City) -> String {
    match city.production() {
        Some(prod) => format!("Building {}, {}/{}", kind_name(map, prod), city.progress, prod.time),
        None => "Building nothing".to_string(),
    }
}

/// A city's holder, walls and place in a line: "Held by red, walls 2, at row
/// 13, column 7".
pub fn city_summary(city: &City) -> String {
    format!("{}, walls {}, at row {}, column {}", holder(city.side), city.walls, city.pos.row, city.pos.col)
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
    lines.push(format!("Walls {}", city.walls));
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
                "Walls 2",
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
            ["Low Tarn", "Nobody's", "Walls 0", "No garrison", "Builds nothing"]
        );
        // Less room than lines: the last says how many are left out.
        assert_eq!(
            city_lines(&game.map, wrenfold, 6),
            ["Wrenfold", "Held by blue", "Walls 2", "Garrison: 3 units", "Building nothing", "  ... and 3 more"]
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
        game.map.produce(Side::Blue);
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
