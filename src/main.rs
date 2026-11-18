//! HexGame in a window.
//!
//! Options:
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
use hexgame::script;
use hexgame::session::Session;
use hexgame::state::Map;

/// The command line.
#[derive(Clone, Debug, Default)]
struct Args {
    window: Options,
    /// `--map <file>`: play on this map instead of the demo's.
    map: Option<PathBuf>,
    /// `--script <steps>`: input to play before the first frame (see `script`).
    script: Option<String>,
}

fn usage() -> String {
    format!("usage: hexgame [--map <file.map>] [--script <steps>] {OPTIONS_USAGE}")
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
            "--map" => parsed.map = Some(PathBuf::from(value("a file")?)),
            "--script" => parsed.script = Some(value("steps")?),
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
    let Args { window, map, script } = args;
    // A map file that cannot be read ends the run before a window opens.
    let game = match map {
        Some(path) => match Map::load(&path) {
            Ok(map) => Game::on_map(map),
            Err(e) => {
                eprintln!("hexgame: --map: {e}");
                std::process::exit(1);
            }
        },
        None => Game::demo(),
    };
    let spec = WindowSpec { program: "hexgame", title: WINDOW_TITLE, size: WINDOW_SIZE };
    app::run(spec, window, move |assets, viewport, dpi| {
        let mut session = Session::with_game(assets, viewport, dpi, game)?;
        if let Some(steps) = &script {
            script::run(&mut session, steps).map_err(|e| format!("--script: {e}"))?;
        }
        Ok(session)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Args, String> {
        parse_args(args.iter().map(|s| s.to_string()))
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
    fn mistakes_are_reported() {
        assert_eq!(parse(&["--bogus"]).unwrap_err(), "unknown option --bogus");
        assert_eq!(parse(&["--frames"]).unwrap_err(), "--frames needs a number");
        assert_eq!(parse(&["--frames", "many"]).unwrap_err(), "--frames: \"many\" is not a number");
        assert_eq!(parse(&["--frames", "0"]).unwrap_err(), "--frames: at least 1");
        assert_eq!(parse(&["--assets"]).unwrap_err(), "--assets needs a directory");
        assert_eq!(parse(&["--map"]).unwrap_err(), "--map needs a file");
        // Help is the usage alone.
        assert_eq!(parse(&["--help"]).unwrap_err(), "");
    }
}
