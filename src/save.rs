//! A game that is not over, kept in a file to be played on later (the
//! owner's word, 2026-11-22: "save the current game's state in a file with
//! start conditions and logs to deterministically recreate the current
//! game ... a simple save game, when exiting make sure the current game is
//! saved").
//!
//! The file holds how the game began (the texts its map was read from, the
//! setup with its seed) and everything done to it since, in its order (the
//! orders carried out, each beat of a round playing out: `Game::record`).
//! The rules are deterministic, so that is the game: to load it is to begin
//! it again and do it all again (`Game::redo`). Nothing of the game's
//! state itself is written, but a number that says how it stood
//! (`Game::digest`): a game that comes out otherwise (the rules have
//! changed since it was saved) is not played on, and says so.
//!
//! The file is text, a line for each thing (`docs/saves.md`); an order is
//! written as the agents' protocol writes it (`agents::order_text`).
//!
//! Pure but for the last few functions, which read and write the files of
//! a directory of saved games (`platform::saves_dir`; a browser has none).

use std::path::{Path, PathBuf};

use crate::agents;
use crate::files;
use crate::game::Game;
use crate::orders::Step;
use crate::setup::{Controller, Fog, Mode, Seat, Setup, TurnOrder};
use crate::state::MapSource;
use crate::state::Side;
use crate::state::combat::System;
use crate::state::scenario::{side_name, side_named};

/// The first line of a saved game, with the format's number.
pub const HEADER: &str = "hexlords save 1";
/// What a saved game's file is called after its name.
pub const EXTENSION: &str = "save";

/// What a saved game says of itself before it is loaded, for a list.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Summary {
    /// The map's name.
    pub map: String,
    /// The round it is in.
    pub round: u32,
    pub mode: Mode,
    pub seats: Vec<Seat>,
    /// When it was saved, in seconds since 1970 (the newest first).
    pub stamp: u64,
}

impl Summary {
    /// In a line: `continent, round 12, rounds`.
    pub fn line(&self) -> String {
        let mode = if self.mode == Mode::Rounds { "rounds" } else { "turns" };
        format!("{}, round {}, {mode}", self.map, self.round)
    }

    /// Who plays it, in a few words: `red and 3 computers`.
    pub fn players(&self) -> String {
        let people: Vec<&str> =
            self.seats.iter().filter(|s| s.controller == Controller::Person).map(|s| side_name(s.side)).collect();
        let computers = match self.seats.len() - people.len() {
            0 => String::new(),
            1 => "1 computer".to_string(),
            n => format!("{n} computers"),
        };
        match (people.is_empty(), computers.is_empty()) {
            (true, _) => computers,
            (false, true) => people.join(", "),
            (false, false) => format!("{} and {computers}", people.join(", ")),
        }
    }
}

/// A saved game, loaded: the game as it was, what was done to it from its
/// start (to go on keeping it), and what each person at the screen had seen
/// of its history (`Session`).
pub struct Loaded {
    pub game: Game,
    pub steps: Vec<Step>,
    pub seen: Vec<(Side, usize)>,
}

/// A step in a line.
fn step_text(game: &Game, step: &Step) -> String {
    match step {
        Step::Order(order) => agents::order_text(game, order),
        Step::Beat => "beat".to_string(),
        Step::Ease(side) => format!("ease {}", side_name(*side)),
    }
}

/// The step a line is, in `game` (the game as it was when it was done).
fn parse_step(game: &Game, line: &str) -> Result<Step, String> {
    let words: Vec<&str> = line.split_whitespace().collect();
    match words[..] {
        ["beat"] => Ok(Step::Beat),
        ["ease", side] => Ok(Step::Ease(side_named(side).ok_or(format!("\"{side}\" is not a side"))?)),
        _ => agents::parse_order(game, line)?.map(Step::Order).ok_or("not a step".to_string()),
    }
}

/// `game` as a saved game's text: `steps` is all that was done to it from
/// its start, `seen` what each person had seen of its history, `stamp` the
/// time of day in seconds. `None` if the game does not know the texts its
/// map was read from (`Game::source`: a map made by hand, in a test).
pub fn text(game: &Game, steps: &[Step], seen: &[(Side, usize)], stamp: u64) -> Option<String> {
    let source = game.source()?;
    let setup = game.setup();
    let mut out = format!("{HEADER}\n");
    let mut line = |words: String| {
        out.push_str(&words);
        out.push('\n');
    };
    line(format!("map {}", source.name.replace(['\n', '\r'], " ")));
    line(format!("round {}", game.turn()));
    line(format!("saved {stamp}"));
    if let Some(assets) = &game.assets {
        line(format!("assets {}", assets.display()));
    }
    line(format!("mode {}", if setup.mode == Mode::Rounds { "rounds" } else { "turns" }));
    line(format!("order {}", if setup.order == TurnOrder::Drawn { "drawn" } else { "fixed" }));
    line(format!("seed {}", setup.seed));
    line(format!("gold {}", setup.gold));
    line(format!("fog {}", setup.fog.word()));
    let combat = &setup.combat;
    line(format!(
        "battles {} chance {} wounds {} back {} cap {}",
        combat.system.name(),
        combat.chance,
        u8::from(combat.wounds),
        u8::from(combat.blows_back),
        combat.cap.map_or("none".to_string(), |cap| cap.to_string())
    ));
    for seat in &setup.seats {
        let who = match &seat.controller {
            Controller::Person => "person",
            Controller::Computer(agent) => agent.as_str(),
        };
        line(format!("seat {} {who}", side_name(seat.side)));
    }
    for (side, length) in seen {
        line(format!("seen {} {length}", side_name(*side)));
    }
    let texts =
        [("terrain", Some(&source.terrain)), ("units", source.units.as_ref()), ("cities", source.cities.as_ref())];
    for (name, text) in texts {
        let Some(text) = text else { continue };
        line(format!("{name} {}", text.lines().count()));
        text.lines().for_each(|l| line(l.to_string()));
    }
    line(format!("steps {}", steps.len()));
    steps.iter().for_each(|step| line(step_text(game, step)));
    line(format!("check {:016x}", game.digest()));
    Some(out)
}

/// A saved game's text, read as far as its lines go: what it says of how
/// the game began, and the lines of its steps.
struct Read<'a> {
    summary: Summary,
    source: MapSource,
    assets: Option<PathBuf>,
    setup: Setup,
    seen: Vec<(Side, usize)>,
    /// The steps, each with its line's number.
    steps: Vec<(usize, &'a str)>,
    check: u64,
}

fn read(text: &str) -> Result<Read<'_>, String> {
    let mut lines = text.lines().enumerate().map(|(i, line)| (i + 1, line));
    if lines.next().map(|(_, line)| line.trim_end()) != Some(HEADER) {
        return Err(format!("not a saved game (its first line is not \"{HEADER}\")"));
    }
    let number =
        |n: usize, word: &str| word.parse::<u64>().map_err(|_| format!("line {n}: \"{word}\" is not a number"));
    let (mut map, mut round, mut stamp, mut assets) = (None, 1, 0, None);
    let (mut mode, mut order, mut seed, mut gold, mut fog) = (Mode::Turns, TurnOrder::Fixed, None, None, Fog::Off);
    let mut combat = crate::state::combat::Rules::default();
    let (mut seats, mut seen, mut steps, mut check) = (Vec::new(), Vec::new(), None, None);
    let (mut terrain, mut units, mut cities) = (None, None, None);
    while let Some((n, line)) = lines.next() {
        let (key, rest) = line.split_once(' ').unwrap_or((line, ""));
        // So many lines that follow, as they are.
        let mut block = |count: &str| -> Result<Vec<(usize, &str)>, String> {
            let count = number(n, count)? as usize;
            let block: Vec<(usize, &str)> = lines.by_ref().take(count).collect();
            if block.len() == count { Ok(block) } else { Err(format!("line {n}: the file ends before its {key}")) }
        };
        let joined = |block: Vec<(usize, &str)>| block.iter().map(|(_, l)| format!("{l}\n")).collect::<String>();
        match key {
            "map" => map = Some(rest.to_string()),
            "round" => round = number(n, rest)? as u32,
            "saved" => stamp = number(n, rest)?,
            "assets" => assets = Some(PathBuf::from(rest)),
            "mode" => mode = if rest == "rounds" { Mode::Rounds } else { Mode::Turns },
            "order" => order = if rest == "drawn" { TurnOrder::Drawn } else { TurnOrder::Fixed },
            "seed" => seed = Some(number(n, rest)?),
            "gold" => gold = Some(number(n, rest)? as u32),
            "fog" => {
                fog = [Fog::Off, Fog::On, Fog::Total]
                    .into_iter()
                    .find(|f| f.word() == rest)
                    .ok_or(format!("line {n}: \"{rest}\" is not a fog"))?
            }
            "battles" => {
                let words: Vec<&str> = rest.split_whitespace().collect();
                // (A file from before the dice were taken out, 2026-11-24,
                // says what an exact roll of them did: nothing now. One
                // whose battles were on the dice is refused.)
                let words = match words[..] {
                    [system, "chance", chance, "exact", _, ref rest @ ..] => {
                        [&[system, "chance", chance][..], rest].concat()
                    }
                    _ => words,
                };
                let [system, "chance", chance, "wounds", wounds, "back", back, "cap", cap] = words[..] else {
                    return Err(format!("line {n}: not the battles' rules"));
                };
                combat.system = System::named(system).ok_or(format!("line {n}: \"{system}\" is not a way to fight"))?;
                combat.chance = number(n, chance)? as u8;
                (combat.wounds, combat.blows_back) = (wounds == "1", back == "1");
                combat.cap = if cap == "none" { None } else { Some(number(n, cap)? as u32) };
            }
            "seat" => {
                let (side, who) = rest.split_once(' ').ok_or(format!("line {n}: seat <side> <who>"))?;
                let side = side_named(side).ok_or(format!("line {n}: \"{side}\" is not a side"))?;
                let controller = match who {
                    "person" => Controller::Person,
                    agent => Controller::Computer(agent.to_string()),
                };
                seats.push(Seat { side, controller });
            }
            "seen" => {
                let (side, length) = rest.split_once(' ').ok_or(format!("line {n}: seen <side> <length>"))?;
                let side = side_named(side).ok_or(format!("line {n}: \"{side}\" is not a side"))?;
                seen.push((side, number(n, length)? as usize));
            }
            "terrain" => terrain = Some(joined(block(rest)?)),
            "units" => units = Some(joined(block(rest)?)),
            "cities" => cities = Some(joined(block(rest)?)),
            "steps" => steps = Some(block(rest)?),
            "check" => {
                check = Some(u64::from_str_radix(rest, 16).map_err(|_| format!("line {n}: \"{rest}\" is not a check"))?)
            }
            // (A line a later version writes: not this one's to mind.)
            _ => {}
        }
    }
    let missing = |what: &str| format!("the file is cut short: it has no {what}");
    let name = map.ok_or_else(|| missing("map"))?;
    let source = MapSource { name: name.clone(), terrain: terrain.ok_or_else(|| missing("terrain"))?, units, cities };
    let mut setup = Setup {
        mode,
        order,
        seats,
        seed: seed.ok_or_else(|| missing("seed"))?,
        when_out: crate::setup::WhenOut::Disband,
        combat,
        gold: gold.ok_or_else(|| missing("gold"))?,
        fog,
    };
    if setup.seats.is_empty() {
        return Err(missing("seats"));
    }
    setup.seats.dedup_by_key(|seat| seat.side);
    let summary = Summary { map: name, round, mode, seats: setup.seats.clone(), stamp };
    let steps = steps.ok_or_else(|| missing("steps"))?;
    // The check is the last line: without it the file was not written whole.
    let check = check.ok_or_else(|| missing("check"))?;
    Ok(Read { summary, source, assets, setup, seen, steps, check })
}

/// What a saved game's text says of itself, without loading it.
pub fn summary(text: &str) -> Result<Summary, String> {
    read(text).map(|read| read.summary)
}

/// The game a saved game's text is: begun again, and all that was done to
/// it done again. An error says what is wrong with the text, or that the
/// game came out otherwise than it was saved.
pub fn load(text: &str) -> Result<Loaded, String> {
    let Read { source, assets, setup, seen, steps: lines, check, .. } = read(text)?;
    // (Its own pictures, if the map's directory is still there.)
    let assets = assets.filter(|dir| files::is_dir(dir));
    let mut game = Game::from_source(source, assets).map_err(|e| format!("its map: {e}"))?.with_setup(setup);
    // (Kept, as it was when it was played: a round then plays out a beat
    // at a time, each a step of the file's.)
    game.record(true);
    let other = "this game was saved under other rules, and cannot be played on";
    let mut steps = Vec::with_capacity(lines.len());
    for (n, line) in lines {
        let step = parse_step(&game, line).map_err(|e| format!("line {n}: {e}"))?;
        game.redo(&step).map_err(|e| format!("line {n} ({line}): {e}: {other}"))?;
        steps.push(step);
    }
    game.take_steps();
    if game.digest() != check {
        return Err(format!("{other} (it came out another way)"));
    }
    Ok(Loaded { game, steps, seen })
}

/// A map's name as part of a file's: its letters and digits, `_` for the
/// rest.
fn file_word(name: &str) -> String {
    let word: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    if word.is_empty() { "game".to_string() } else { word }
}

/// A file for a new saved game of a map called `name` in `dir`: the first
/// of `<name>-1.save`, `<name>-2.save` ... that is not there.
pub fn fresh(dir: &Path, name: &str) -> PathBuf {
    let word = file_word(name);
    (1..).map(|n| dir.join(format!("{word}-{n}.{EXTENSION}"))).find(|path| !files::is_file(path)).unwrap_or_default()
}

/// The saved games in `dir`, the last saved first, with what each says of
/// itself (or what is wrong with it).
pub fn list(dir: &Path) -> Vec<(PathBuf, Result<Summary, String>)> {
    let mut found: Vec<(PathBuf, Result<Summary, String>)> = files::list(dir)
        .unwrap_or_default()
        .into_iter()
        .filter(|path| path.extension().is_some_and(|x| x == EXTENSION))
        .map(|path| {
            let summary = files::read_to_string(&path).map_err(|e| e.to_string()).and_then(|text| summary(&text));
            (path, summary)
        })
        .collect();
    found.sort_by_key(|(_, summary)| std::cmp::Reverse(summary.as_ref().map_or(0, |s| s.stamp)));
    found
}

/// Write a saved game's text to `path` (its directory is made if it is not
/// there): whole, or not at all.
pub fn write(path: &Path, text: &str) -> Result<(), String> {
    files::write_whole(path, text.as_bytes()).map_err(|e| format!("{}: {e}", path.display()))
}

/// Read a saved game's file and load it (`load`).
pub fn load_file(path: &Path) -> Result<Loaded, String> {
    let text = files::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    load(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Take a saved game's file away.
pub fn delete(path: &Path) -> Result<(), String> {
    files::remove(path).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orders::{Order, RallyPoint};
    use crate::state::{CityId, Hex, QueueEnd, QueueItem, Sends, UnitId};

    const TERRAIN: &str = "g";
    const UNITS: &str = "unit soldiers\nname Soldiers\nstrength 5\nmoves 12\nenter grass 2, city 1\n\
                         unit levy\nname Levy\nstrength 1\nmoves 12\nenter grass 2, city 1\n";
    const CITIES: &str = "city 5 2\nname Redhold\nside red\nproduce soldiers 1\ngarrison soldiers 2\n\
                          city 5 17\nname Bluemoor\nside blue\nproduce levy 1\ngarrison levy 2\n";

    /// 20 x 20 grass, red's and blue's cities with two units each, both
    /// sides the computer's (passive: the test gives the orders).
    fn game(mode: Mode) -> Game {
        let mut rows = vec![TERRAIN.repeat(20); 20];
        rows[5] = format!("{}c{}c{}", "g".repeat(2), "g".repeat(14), "g".repeat(2));
        let terrain = format!("20\n20\n{}\n", rows.join("\n"));
        let source = MapSource {
            name: "two cities".to_string(),
            terrain,
            units: Some(UNITS.to_string()),
            cities: Some(CITIES.to_string()),
        };
        let game = Game::from_source(source, None).unwrap();
        let mut setup = Setup::solo(&game.map, None, "passive");
        setup.set_players("red=passive,blue=person").unwrap();
        (setup.mode, setup.seed, setup.fog, setup.gold) = (mode, 77, Fog::On, 120);
        setup.combat.wounds = true;
        setup.combat.cap = Some(9);
        let mut game = game.with_setup(setup);
        game.record(true);
        game
    }

    fn units_of(game: &Game, side: Side) -> Vec<UnitId> {
        game.map.units().of_side(side).map(|u| u.id()).collect()
    }

    fn places(game: &Game) -> Vec<(UnitId, Hex, u32)> {
        game.map.units().iter().map(|u| (u.id(), u.pos(), u.movement)).collect()
    }

    /// A few turns: moves, what the cities build, an order taken back, a
    /// battle. Saved and loaded, it is the same game: every unit where it
    /// was, the same history, the same chance from here on.
    #[test]
    fn a_game_saved_and_loaded_is_the_same_game() {
        let mut game = game(Mode::Turns);
        let (red, blue) = (units_of(&game, Side::Red), units_of(&game, Side::Blue));
        let red_city = game.map.city_at(Hex::new(5, 2)).unwrap().id;
        for order in [
            Order::Build { city: red_city, what: Some(0) },
            Order::Move { units: vec![red[0]], to: Hex::new(5, 5) },
            Order::Undo,
            Order::GoTo { units: red.clone(), to: Hex::new(5, 14), via: vec![Hex::new(8, 9)] },
            Order::EndTurn,
            Order::Defend { units: vec![blue[0]], on: true },
            Order::EndTurn,
            Order::EndTurn,
            Order::Move { units: vec![blue[1]], to: Hex::new(6, 16) },
            Order::EndTurn,
            Order::EndTurn,
            Order::EndTurn,
        ] {
            game.apply(order.clone()).unwrap_or_else(|why| panic!("{order:?}: {why}"));
        }
        assert!(game.history().len() >= 3, "{:?}", game.history());
        let steps = game.take_steps();
        let seen = vec![(Side::Blue, 2)];
        let text = text(&game, &steps, &seen, 1234).unwrap();
        let loaded = load(&text).unwrap();
        assert_eq!(loaded.steps, steps);
        assert_eq!(loaded.seen, seen);
        let again = loaded.game;
        assert_eq!(again.setup(), game.setup());
        assert_eq!(again.source(), game.source());
        assert_eq!(places(&again), places(&game));
        assert_eq!(again.history(), game.history());
        assert_eq!((again.turn(), again.player(), again.digest()), (game.turn(), game.player(), game.digest()));
        // What it says of itself, for a list.
        let summary = summary(&text).unwrap();
        assert_eq!((summary.map.as_str(), summary.round, summary.stamp), ("two cities", game.turn(), 1234));
        assert_eq!(summary.line(), format!("two cities, round {}, turns", game.turn()));
        assert_eq!(summary.players(), "blue and 1 computer");
        // And saved again, it is the same text.
        assert_eq!(super::text(&again, &loaded.steps, &seen, 1234).unwrap(), text);
    }

    /// Rounds: saved while the round plays out, it is loaded there, and
    /// plays on the same.
    #[test]
    fn a_round_half_played_out_is_saved_there() {
        let mut game = game(Mode::Rounds);
        let (red, blue) = (units_of(&game, Side::Red), units_of(&game, Side::Blue));
        game.apply(Order::Move { units: red.clone(), to: Hex::new(5, 8) }).unwrap();
        game.stand_easy(Side::Red);
        game.apply(Order::EndTurn).unwrap();
        game.apply(Order::Move { units: vec![blue[0]], to: Hex::new(8, 15) }).unwrap();
        game.apply(Order::EndTurn).unwrap();
        assert!(game.playing_out(), "{:?} {:?} {}", game.player(), game.order(), game.turn());
        game.play_out();
        assert!(game.playing_out(), "the round takes more than a beat");
        let steps = game.take_steps();
        assert!(steps.contains(&Step::Beat) && steps.contains(&Step::Ease(Side::Red)), "{steps:?}");
        let text = text(&game, &steps, &[], 0).unwrap();
        let mut again = load(&text).unwrap().game;
        assert!(again.playing_out());
        assert_eq!(places(&again), places(&game));
        while game.playing_out() {
            game.play_out();
            again.play_out();
        }
        assert!(!again.playing_out());
        assert_eq!((places(&again), again.digest()), (places(&game), game.digest()));
    }

    /// Every kind of order is written as a line that reads as the same
    /// order.
    #[test]
    fn every_order_is_written_as_it_is_read() {
        let game = game(Mode::Turns);
        let (red, city) = (units_of(&game, Side::Red), CityId(0));
        let (a, b) = (Hex::new(3, 4), Hex::new(10, 11));
        let item = |sends: Sends| QueueItem { what: 0, count: 2, sends };
        let orders = [
            Order::Move { units: red.clone(), to: a },
            Order::Toward { units: vec![red[0]], to: b },
            Order::Onward { units: red.clone(), to: b },
            Order::Build { city, what: Some(0) },
            Order::Build { city, what: None },
            Order::Queue {
                city,
                items: vec![
                    item(Sends::Stay),
                    item(Sends::Defend),
                    item(Sends::Route(vec![a, b])),
                    item(Sends::Point { n: 3, bless: false }),
                    item(Sends::Point { n: 4, bless: true }),
                ],
                end: QueueEnd::Repeat { from: 1, times: Some(3) },
            },
            Order::Queue { city, items: vec![item(Sends::Stay)], end: QueueEnd::Repeat { from: 0, times: None } },
            Order::Queue { city, items: vec![item(Sends::Stay)], end: QueueEnd::Stop },
            Order::Queue { city, items: Vec::new(), end: QueueEnd::Stop },
            Order::GoTo { units: red.clone(), to: b, via: Vec::new() },
            Order::GoTo { units: red.clone(), to: b, via: vec![a, Hex::new(7, 7)] },
            Order::Done { units: red.clone() },
            Order::Done { units: Vec::new() },
            Order::Defend { units: red.clone(), on: true },
            Order::Defend { units: red.clone(), on: false },
            Order::Bless { units: red.clone() },
            Order::Rally { city, to: Sends::Stay },
            Order::Rally { city, to: Sends::Defend },
            Order::Rally { city, to: Sends::Route(vec![a, b]) },
            Order::Rally { city, to: Sends::Point { n: 9, bless: true } },
            Order::Rally { city, to: Sends::Point { n: 0, bless: false } },
            Order::RallyPoint { n: 2, point: Some(RallyPoint { at: a, defend: true }) },
            Order::RallyPoint { n: 2, point: Some(RallyPoint { at: a, defend: false }) },
            Order::RallyPoint { n: 2, point: None },
            Order::Walls { city },
            Order::Disband { units: vec![red[1]] },
            Order::Undo,
            Order::EndTurn,
            Order::Patrol { units: red.clone(), waypoints: vec![a, b], attack: Some(60) },
            Order::Patrol { units: red.clone(), waypoints: vec![a, b, a], attack: None },
            Order::Maraud { units: red.clone(), on: true },
            Order::Maraud { units: red.clone(), on: false },
            Order::Roam { units: red.clone(), to: a },
        ];
        for order in orders {
            let line = step_text(&game, &Step::Order(order.clone()));
            assert_eq!(parse_step(&game, &line), Ok(Step::Order(order)), "{line}");
        }
        assert_eq!(parse_step(&game, "beat"), Ok(Step::Beat));
        assert_eq!(parse_step(&game, &step_text(&game, &Step::Ease(Side::Blue))), Ok(Step::Ease(Side::Blue)));
    }

    /// A file that is not one, that was cut short, or whose game comes out
    /// otherwise, says so and gives no game.
    #[test]
    fn a_bad_file_gives_no_game() {
        let mut game = game(Mode::Turns);
        let red = units_of(&game, Side::Red);
        game.apply(Order::Move { units: red.clone(), to: Hex::new(5, 5) }).unwrap();
        game.apply(Order::EndTurn).unwrap();
        let steps = game.take_steps();
        let text = text(&game, &steps, &[], 0).unwrap();
        assert!(load(&text).is_ok());
        let why = |text: &str| load(text).err().unwrap_or_default();
        assert_eq!(why("hello\n"), "not a saved game (its first line is not \"hexlords save 1\")");
        // Cut short (the check is its last line).
        let cut = text.lines().take(text.lines().count() - 1).collect::<Vec<_>>().join("\n");
        assert_eq!(why(&cut), "the file is cut short: it has no check");
        let cut = text.lines().take(30).collect::<Vec<_>>().join("\n");
        assert!(why(&cut).contains("the file ends before its"), "{}", why(&cut));
        // A move the rules now refuse.
        let moved = text.replace(" 5 5\n", " 19 19\n");
        assert!(
            why(&moved).ends_with("this game was saved under other rules, and cannot be played on"),
            "{}",
            why(&moved)
        );
        // The same orders, another outcome.
        let other = text.replace("seed 77", "seed 78");
        assert!(why(&other).contains("it came out another way"), "{}", why(&other));
        // Written before the dice were taken out (2026-11-24): what an exact
        // roll did is passed over; battles on the dice are refused.
        let battles = text.lines().find(|l| l.starts_with("battles ")).unwrap();
        let old = battles.replace(" wounds ", " exact hits wounds ");
        let before = text.replace(battles, &old);
        assert!(before != text && load(&before).is_ok());
        let dice = before.replace(&old, &old.replacen("battles lines", "battles dice", 1));
        assert!(why(&dice).contains("\"dice\" is not a way to fight"), "{}", why(&dice));
        // A map that is no map.
        let no_map = text.replace("terrain 22\n20\n", "terrain 22\n2x\n");
        assert!(why(&no_map).starts_with("its map: "), "{}", why(&no_map));
    }

    #[test]
    fn saved_games_are_files_in_a_directory() {
        let dir = std::env::temp_dir().join(format!("hexlords-save-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(list(&dir).is_empty(), "no directory: no games");
        let mut game = game(Mode::Turns);
        let first = fresh(&dir, "two cities");
        assert_eq!(first, dir.join("two_cities-1.save"));
        write(&first, &text(&game, &[], &[], 10).unwrap()).unwrap();
        let second = fresh(&dir, "two cities");
        assert_eq!(second, dir.join("two_cities-2.save"));
        game.apply(Order::EndTurn).unwrap();
        game.apply(Order::EndTurn).unwrap();
        let steps = game.take_steps();
        write(&second, &text(&game, &steps, &[], 20).unwrap()).unwrap();
        std::fs::write(dir.join("broken.save"), "nothing").unwrap();
        std::fs::write(dir.join("notes.txt"), "not a save").unwrap();
        // The last saved first; one that cannot be read last, with why.
        let found = list(&dir);
        let names: Vec<_> = found.iter().map(|(p, _)| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["two_cities-2.save", "two_cities-1.save", "broken.save"]);
        assert_eq!(found[0].1.as_ref().map(|s| s.round), Ok(2));
        assert!(found[2].1.is_err());
        // Written over, loaded, taken away.
        assert_eq!(load_file(&second).unwrap().game.turn(), 2);
        delete(&second).unwrap();
        assert_eq!(list(&dir).len(), 2);
        assert_eq!(fresh(&dir, "two cities"), second, "its name is free again");
        assert!(load_file(&second).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
