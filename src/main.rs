//! HexGame in a window, or without one.
//!
//! Without `--map` the window opens on the new game screen. Options:
//! - `--map <file or directory>`: play this map at once (one person, the first
//!   side; the computer plays the others);
//! - `--players red=person,blue=simple`: who plays which side (`person`, or an
//!   agent: `passive`, `simple`, `cmd:<program>`);
//! - `--order fixed|drawn`: the sides play in the same order each round, or
//!   one drawn anew; `--seed <n>`: the seed of the game's chance (without
//!   it, one from the clock in a window, each game its own; 1 headless, or
//!   with a script or a screenshot, so a run can be made again);
//! - `--chance <0-10>`: how much chance there is in a battle (5); `--battles
//!   lines|strength`: how a battle goes (lines); `--wounds`: a unit's hits
//!   last until its side's turn ends, and it fights at half strength while
//!   wounded (they do not); `--gold <n>`: the gold each
//!   side starts with (50); `--fog`: fog of war; `--total-fog`: fog of war,
//!   and the land a side has never seen is black;
//! - `--headless`: no window: the computer plays every side of `--map` until
//!   one is left or `--rounds <n>` (200) have gone by, then says how it went
//!   (`--log`: and everything that happened on the way);
//! - `--assets <dir>`: use this asset directory;
//! - `--frames <n>`: quit after drawing `n` frames (a smoke test);
//! - `--screenshot <file>`: draw a frame into a PNG file and quit (the last frame
//!   of `--frames`, or the second);
//! - `--script <steps>`: play some input first (see `hexgame::script`), for
//!   example `--script "enter,right*3,enter"`.

#![forbid(unsafe_code)]

use std::path::PathBuf;

use hexgame::app::{self, OPTIONS_USAGE, Options, WINDOW_SIZE, WINDOW_TITLE, WindowSpec};
use hexgame::game::Game;
use hexgame::headless;
use hexgame::new_game::COMPUTER;
use hexgame::script;
use hexgame::session::Session;
use hexgame::setup::{Controller, Fog, Setup, TurnOrder};
use hexgame::shell::Shell;
use hexgame::state::combat::{MAX_CHANCE, System};

/// The command line.
#[derive(Clone, Debug, Default)]
struct Args {
    window: Options,
    /// `--map <file or directory>`: play this map at once.
    map: Option<PathBuf>,
    /// `--script <steps>`: input to play before the first frame (see `script`).
    script: Option<String>,
    /// `--players <side>=<who>,...`
    players: Option<String>,
    /// `--order fixed|drawn`
    order: Option<TurnOrder>,
    seed: Option<u64>,
    headless: bool,
    rounds: Option<u32>,
    log: bool,
    chance: Option<u8>,
    battles: Option<System>,
    wounds: bool,
    gold: Option<u32>,
    fog: Fog,
}

fn usage() -> String {
    format!(
        "usage: hexgame [--map <file.map or map directory>] [--players <side>=<person|agent>,...] [--order fixed|drawn] \
         [--seed <n>] [--chance <0-10>] [--battles lines|strength] [--wounds] [--gold <n>] [--fog | --total-fog] [--headless [--rounds <n>] \
         [--log]] \
         [--script <steps>] {OPTIONS_USAGE}"
    )
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut parsed = Args::default();
    let mut args = args;
    while let Some(arg) = args.next() {
        if parsed.window.take(&arg, &mut args)? {
            continue;
        }
        let mut value = |what: &str| args.next().ok_or(format!("{arg} needs {what}"));
        match arg.as_str() {
            "--map" => parsed.map = Some(PathBuf::from(value("a map")?)),
            "--script" => parsed.script = Some(value("steps")?),
            "--players" => parsed.players = Some(value("who plays which side")?),
            "--order" => {
                parsed.order = Some(match value("fixed or drawn")?.as_str() {
                    "fixed" => TurnOrder::Fixed,
                    "drawn" => TurnOrder::Drawn,
                    other => return Err(format!("--order: \"{other}\" is not fixed or drawn")),
                })
            }
            "--seed" => {
                let n = value("a number")?;
                parsed.seed = Some(n.parse().map_err(|_| format!("--seed: \"{n}\" is not a number"))?);
            }
            "--rounds" => {
                let n = value("a number")?;
                parsed.rounds = Some(n.parse().map_err(|_| format!("--rounds: \"{n}\" is not a number"))?);
            }
            "--headless" => parsed.headless = true,
            "--fog" => parsed.fog = parsed.fog.max(Fog::On),
            "--wounds" => parsed.wounds = true,
            "--total-fog" => parsed.fog = Fog::Total,
            "--chance" => {
                let n = value("a number from 0 to 10")?;
                let chance = n.parse::<u8>().ok().filter(|c| *c <= MAX_CHANCE);
                parsed.chance = Some(chance.ok_or(format!("--chance: \"{n}\" is not a number from 0 to 10"))?);
            }
            "--gold" => {
                let n = value("a number")?;
                parsed.gold = Some(n.parse().map_err(|_| format!("--gold: \"{n}\" is not a number"))?);
            }
            "--battles" => {
                let name = value("lines or strength")?;
                parsed.battles =
                    Some(System::named(&name).ok_or(format!("--battles: \"{name}\" is not lines or strength"))?);
            }
            "--log" => parsed.log = true,
            "-h" | "--help" => return Err(String::new()),
            _ => return Err(format!("unknown option {arg}")),
        }
    }
    Ok(parsed)
}

fn main() {
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(message) => {
            if !message.is_empty() {
                eprintln!("hexgame: {message}");
            }
            eprintln!("{}", usage());
            std::process::exit(2);
        }
    };
    let Args { window, map, script, players, order, seed, headless, rounds, log, chance, battles, wounds, gold, fog } =
        args;
    // A map that cannot be read ends the run before a window opens.
    let game = match &map {
        Some(path) => match Game::load(path) {
            Ok((game, warnings)) => {
                for warning in warnings {
                    eprintln!("hexgame: --map: {}: {warning}", path.display());
                }
                Some(game)
            }
            Err(e) => {
                eprintln!("hexgame: --map: {e}");
                std::process::exit(1);
            }
        },
        None => None,
    };
    // How the game is set up: one person (the first side; without a window,
    // nobody), the computer for the others, and what the options say.
    let game = match game {
        Some(game) => {
            let person = game.setup().seats.iter().find(|s| s.controller == Controller::Person).map(|s| s.side);
            let mut setup = Setup::solo(&game.map, person, COMPUTER);
            if headless {
                setup.seats.iter_mut().for_each(|seat| seat.controller = Controller::Computer(COMPUTER.to_string()));
            }
            if let Some(spec) = &players {
                if let Err(e) = setup.set_players(spec) {
                    eprintln!("hexgame: --players: {e}");
                    std::process::exit(2);
                }
            }
            setup.order = order.unwrap_or(setup.order);
            // (A script or a screenshot is to show the same game each time.)
            let again = headless || script.is_some() || window.screenshot.is_some();
            setup.seed = game_seed(seed, again, setup.seed);
            setup.combat.chance = chance.unwrap_or(setup.combat.chance);
            setup.combat.system = battles.unwrap_or(setup.combat.system);
            setup.combat.wounds |= wounds;
            setup.gold = gold.unwrap_or(setup.gold);
            if fog.on() {
                setup.fog = fog;
            }
            Some(game.with_setup(setup))
        }
        None => None,
    };
    if headless {
        let Some(mut game) = game else {
            eprintln!("hexgame: --headless needs a --map");
            std::process::exit(2);
        };
        let mut print = |_: &Game, line: &str| {
            if log {
                println!("{line}");
            }
        };
        match headless::play(&mut game, rounds.unwrap_or(200), &mut print) {
            Ok(outcome) => print!("{}", headless::summary(&outcome)),
            Err(e) => {
                eprintln!("hexgame: --headless: {e}");
                std::process::exit(2);
            }
        }
        return;
    }
    let spec = WindowSpec { program: "hexgame", title: WINDOW_TITLE, size: WINDOW_SIZE };
    app::run(spec, window, move |assets, viewport, dpi| {
        let mut shell = match game {
            Some(game) => Shell::playing(assets, Session::with_game(assets, viewport, dpi, game)?),
            None => Shell::new_game(assets, viewport, dpi, None)?,
        };
        if let Some(steps) = &script {
            script::run(&mut shell, steps).map_err(|e| format!("--script: {e}"))?;
        }
        Ok(shell)
    });
}

/// The seed of a game from the command line: `--seed`'s; without it, one
/// from the clock in a window (each game its own chance), and the setup's
/// own (`default`) when the run is to be made again (`again`: headless, a
/// script, a screenshot).
fn game_seed(given: Option<u64>, again: bool, default: u64) -> u64 {
    given.unwrap_or_else(|| {
        if again {
            return default;
        }
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
        now.map_or(default, |d| (d.as_nanos() as u64).max(2))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Args, String> {
        parse_args(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn a_game_in_a_window_has_its_own_seed() {
        // In a window, without --seed, one from the clock: the same seed each
        // time gave the same battles and blessings in every game.
        assert_ne!(game_seed(None, false, 1), 1);
        // Headless, scripted or a screenshot, the setup's, so a run can be
        // made again; --seed wins.
        assert_eq!(game_seed(None, true, 1), 1);
        assert_eq!((game_seed(Some(9), false, 1), game_seed(Some(9), true, 1)), (9, 9));
    }

    #[test]
    fn no_arguments_are_the_defaults() {
        let o = parse(&[]).unwrap();
        assert!(o.window.assets.is_none() && o.window.frames.is_none() && o.window.screenshot.is_none());
        assert!(o.map.is_none() && o.script.is_none());
    }

    #[test]
    fn every_option() {
        let all = ["--assets", "a dir", "--frames", "30", "--screenshot", "shot.png", "--script", "enter,1"];
        let o = parse(&[&all[..], &["--map", "maps/one.map"]].concat()).unwrap();
        assert_eq!(o.window.assets, Some(PathBuf::from("a dir")));
        assert_eq!(o.window.frames, Some(30));
        assert_eq!(o.window.screenshot, Some(PathBuf::from("shot.png")));
        assert_eq!(o.script.as_deref(), Some("enter,1"));
        assert_eq!(o.map, Some(PathBuf::from("maps/one.map")));
    }

    #[test]
    fn the_games_options() {
        let o =
            parse(&["--headless", "--map", "m", "--players", "red=simple", "--order", "drawn", "--seed", "9"]).unwrap();
        assert!(o.headless && !o.log);
        assert_eq!((o.players.as_deref(), o.order, o.seed), (Some("red=simple"), Some(TurnOrder::Drawn), Some(9)));
        let o = parse(&["--rounds", "5", "--log", "--order", "fixed"]).unwrap();
        assert_eq!((o.rounds, o.log, o.order), (Some(5), true, Some(TurnOrder::Fixed)));
        assert_eq!(parse(&["--order", "sometimes"]).unwrap_err(), "--order: \"sometimes\" is not fixed or drawn");
        assert_eq!(parse(&["--seed", "x"]).unwrap_err(), "--seed: \"x\" is not a number");
        assert_eq!(parse(&["--rounds"]).unwrap_err(), "--rounds needs a number");
        let o = parse(&["--chance", "0", "--battles", "strength"]).unwrap();
        assert_eq!((o.chance, o.battles), (Some(0), Some(System::Strength)));
        assert_eq!(parse(&["--chance", "11"]).unwrap_err(), "--chance: \"11\" is not a number from 0 to 10");
        assert_eq!(parse(&["--battles", "dice"]).unwrap_err(), "--battles: \"dice\" is not lines or strength");
        assert!(parse(&["--wounds"]).unwrap().wounds && !parse(&[]).unwrap().wounds);
        assert_eq!(parse(&["--gold", "120"]).unwrap().gold, Some(120));
        assert_eq!(parse(&["--gold", "lots"]).unwrap_err(), "--gold: \"lots\" is not a number");
    }

    #[test]
    fn mistakes_are_reported() {
        assert_eq!(parse(&["--bogus"]).unwrap_err(), "unknown option --bogus");
        assert_eq!(parse(&["--frames"]).unwrap_err(), "--frames needs a number");
        assert_eq!(parse(&["--frames", "many"]).unwrap_err(), "--frames: \"many\" is not a number");
        assert_eq!(parse(&["--frames", "0"]).unwrap_err(), "--frames: at least 1");
        assert_eq!(parse(&["--assets"]).unwrap_err(), "--assets needs a directory");
        assert_eq!(parse(&["--map"]).unwrap_err(), "--map needs a map");
        // Help is the usage alone.
        assert_eq!(parse(&["--help"]).unwrap_err(), "");
    }
}
