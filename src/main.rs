//! Hexlords in a window, or without one.
//!
//! Without `--map` the window opens on the new game screen. `--version`
//! says which version this is and what it was built from, and quits.
//! Options:
//! - `--map <file or directory>`: play this map at once (one person, the first
//!   side; the computer plays the others);
//! - `--players red=person,blue=bold`: who plays which side (`person`, or an
//!   agent: `steady`, `bold`, `careful`, `simple`, `passive`, `cmd:<program>`);
//! - `--mode turns|rounds`: the sides play one after the other (turns), or
//!   all give their orders and the round plays out at once (rounds);
//! - `--order fixed|drawn`: the sides play in the same order each round, or
//!   one drawn anew (in rounds always drawn, `Setup::drawn`); `--seed <n>`: the seed of the game's chance (without
//!   it, one from the clock in a window, each game its own; 1 headless, or
//!   with a script or a screenshot, so a run can be made again);
//! - `--chance <0-10>`: how much chance there is in a battle (5); `--battles
//!   lines|strength`: how a battle goes (lines); `--wounds`: a unit's hits last until its side's turn
//!   ends, and it fights at half strength while wounded (they do not);
//!   `--blows-back`: the loser of an exchange may hit back (it does not);
//!   `--cap <n>`: no unit fights stronger than `n`, its bonuses counted
//!   (none);
//!   `--gold <n>`: the gold each
//!   side starts with (50); `--fog`: fog of war; `--total-fog`: fog of war,
//!   and the land a side has never seen is black;
//! - `--headless`: no window: the computer plays every side of `--map` until
//!   one is left or `--rounds <n>` (200) have gone by, then says how it went
//!   (`--log`: and everything that happened on the way);
//! - `--saves <dir>`: keep the games that are not over in this directory
//!   (without it, where the system keeps a program's data: a game is saved as
//!   its turns pass and when the window closes, and the new game screen
//!   lists those to play on; not with `--frames`, `--screenshot` or
//!   `--script`, which keep nothing unless told where);
//! - `--assets <dir>`: use this asset directory;
//! - `--frames <n>`: quit after drawing `n` frames (a smoke test);
//! - `--screenshot <file>`: draw a frame into a PNG file and quit (the last frame
//!   of `--frames`, or the second);
//! - `--script <steps>`: play some input first (see `hexlords::script`), for
//!   example `--script "enter,right*3,enter"`.

#![forbid(unsafe_code)]

use std::path::PathBuf;

use hexlords::app::{self, OPTIONS_USAGE, Options, WINDOW_SIZE, WINDOW_TITLE, WindowSpec};
use hexlords::game::Game;
use hexlords::headless;
use hexlords::new_game::COMPUTER;
use hexlords::script;
use hexlords::session::Session;
use hexlords::setup::{Controller, Fog, Mode, Setup, TurnOrder};
use hexlords::shell::Shell;
use hexlords::state::combat::{MAX_CHANCE, System};

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
    /// `--mode turns|rounds`
    mode: Option<Mode>,
    /// `--order fixed|drawn`
    order: Option<TurnOrder>,
    seed: Option<u64>,
    headless: bool,
    rounds: Option<u32>,
    log: bool,
    chance: Option<u8>,
    battles: Option<System>,
    wounds: bool,
    blows_back: bool,
    cap: Option<u32>,
    gold: Option<u32>,
    fog: Fog,
    /// `--saves <dir>`
    saves: Option<PathBuf>,
    /// `--version`
    version: bool,
}

fn usage() -> String {
    format!(
        "usage: hexlords [--map <file.map or map directory>] [--players <side>=<person|agent>,...] [--mode turns|rounds] [--order fixed|drawn] \
         [--seed <n>] [--chance <0-10>] [--battles lines|strength] [--wounds] [--blows-back] [--cap <n>] [--gold <n>] [--fog | --total-fog] [--saves <dir>] [--headless [--rounds <n>] \
         [--log]] \
         [--script <steps>] {OPTIONS_USAGE} | --version"
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
            "--saves" => parsed.saves = Some(PathBuf::from(value("a directory")?)),
            "--players" => parsed.players = Some(value("who plays which side")?),
            "--mode" => {
                parsed.mode = Some(match value("turns or rounds")?.as_str() {
                    "turns" => Mode::Turns,
                    "rounds" => Mode::Rounds,
                    other => return Err(format!("--mode: \"{other}\" is not turns or rounds")),
                })
            }
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
            "--blows-back" => parsed.blows_back = true,
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
                let system = System::named(&name).ok_or(format!("--battles: \"{name}\" is not lines or strength"));
                parsed.battles = Some(system?);
            }
            "--cap" => {
                let n = value("a number")?;
                let cap = n.parse::<u32>().ok().filter(|n| *n > 0);
                parsed.cap = Some(cap.ok_or(format!("--cap: \"{n}\" is not a number above 0"))?);
            }
            "--log" => parsed.log = true,
            "-h" | "--help" => return Err(String::new()),
            "-V" | "--version" => parsed.version = true,
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
                eprintln!("hexlords: {message}");
            }
            eprintln!("{}", usage());
            std::process::exit(2);
        }
    };
    if args.version {
        println!("{}", hexlords::about::line());
        return;
    }
    let Args {
        window,
        map,
        script,
        players,
        mode,
        order,
        seed,
        headless,
        rounds,
        log,
        chance,
        battles,
        wounds,
        blows_back,
        cap,
        gold,
        fog,
        saves,
        version: _,
    } = args;
    // A map that cannot be read ends the run before a window opens.
    let game = match &map {
        Some(path) => match Game::load(path) {
            Ok((game, warnings)) => {
                for warning in warnings {
                    eprintln!("hexlords: --map: {}: {warning}", path.display());
                }
                Some(game)
            }
            Err(e) => {
                eprintln!("hexlords: --map: {e}");
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
                    eprintln!("hexlords: --players: {e}");
                    std::process::exit(2);
                }
            }
            setup.mode = mode.unwrap_or(setup.mode);
            setup.order = order.unwrap_or(setup.order);
            // (A script or a screenshot is to show the same game each time.)
            let again = headless || script.is_some() || window.screenshot.is_some();
            setup.seed = game_seed(seed, again, setup.seed);
            setup.combat.chance = chance.unwrap_or(setup.combat.chance);
            setup.combat.system = battles.unwrap_or(setup.combat.system);
            setup.combat.wounds |= wounds;
            setup.combat.blows_back |= blows_back;
            setup.combat.cap = cap.or(setup.combat.cap);
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
            eprintln!("hexlords: --headless needs a --map");
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
                eprintln!("hexlords: --headless: {e}");
                std::process::exit(2);
            }
        }
        return;
    }
    // Where the games that are not over are kept: where told, or where the
    // system keeps such things; a run that is a test or a picture keeps
    // nothing unless told where.
    let played = script.is_none() && window.screenshot.is_none() && window.frames.is_none();
    let saves = saves.or_else(|| hexlords::platform::saves_dir().filter(|_| played));
    // The interface's size: --ui-scale's, the environment's, else the one
    // kept in the settings (the Options window's), else 100.
    let mut window = window;
    if window.scale().is_none() {
        window.ui_scale = saves.as_deref().and_then(hexlords::settings::ui_scale);
    }
    let percent = window.scale().unwrap_or(100);
    let spec = WindowSpec { program: "hexlords", title: WINDOW_TITLE, size: WINDOW_SIZE };
    app::run(spec, window, move |assets, viewport, dpi| {
        let mut shell = match game {
            Some(game) => Shell::playing(assets, Session::with_game(assets, viewport, dpi, game)?),
            None => Shell::new_game(assets, viewport, dpi, None)?,
        };
        if let Some(dir) = &saves {
            shell.keep_games_in(dir.clone());
        }
        shell.set_ui_scale(percent, dpi);
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
        (hexlords::platform::clock_nanos() as u64).max(2)
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
        assert_eq!(parse(&["--saves", "my games"]).unwrap().saves, Some(PathBuf::from("my games")));
        assert_eq!(parse(&["--saves"]).unwrap_err(), "--saves needs a directory");
    }

    #[test]
    fn the_games_options() {
        let o =
            parse(&["--headless", "--map", "m", "--players", "red=simple", "--order", "drawn", "--seed", "9"]).unwrap();
        assert!(o.headless && !o.log);
        assert_eq!((o.players.as_deref(), o.order, o.seed), (Some("red=simple"), Some(TurnOrder::Drawn), Some(9)));
        let o = parse(&["--rounds", "5", "--log", "--order", "fixed"]).unwrap();
        assert_eq!((o.rounds, o.log, o.order), (Some(5), true, Some(TurnOrder::Fixed)));
        assert_eq!(parse(&["--mode", "rounds"]).unwrap().mode, Some(Mode::Rounds));
        assert_eq!((parse(&["--mode", "turns"]).unwrap().mode, parse(&[]).unwrap().mode), (Some(Mode::Turns), None));
        assert_eq!(parse(&["--mode", "both"]).unwrap_err(), "--mode: \"both\" is not turns or rounds");
        assert_eq!(parse(&["--order", "sometimes"]).unwrap_err(), "--order: \"sometimes\" is not fixed or drawn");
        assert_eq!(parse(&["--seed", "x"]).unwrap_err(), "--seed: \"x\" is not a number");
        assert_eq!(parse(&["--rounds"]).unwrap_err(), "--rounds needs a number");
        let o = parse(&["--chance", "0", "--battles", "strength"]).unwrap();
        assert_eq!((o.chance, o.battles), (Some(0), Some(System::Strength)));
        assert_eq!(parse(&["--chance", "11"]).unwrap_err(), "--chance: \"11\" is not a number from 0 to 10");
        let lances = "--battles: \"lances\" is not lines or strength";
        assert_eq!(parse(&["--battles", "lances"]).unwrap_err(), lances);
        assert_eq!(parse(&["--battles", "dice"]).unwrap_err(), "--battles: \"dice\" is not lines or strength");
        assert!(parse(&["--wounds"]).unwrap().wounds && !parse(&[]).unwrap().wounds);
        assert!(
            parse(&["--version"]).unwrap().version && parse(&["-V"]).unwrap().version && !parse(&[]).unwrap().version
        );
        assert!(parse(&["--blows-back"]).unwrap().blows_back && !parse(&[]).unwrap().blows_back);
        assert_eq!((parse(&["--cap", "9"]).unwrap().cap, parse(&[]).unwrap().cap), (Some(9), None));
        assert_eq!(parse(&["--cap", "0"]).unwrap_err(), "--cap: \"0\" is not a number above 0");
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
