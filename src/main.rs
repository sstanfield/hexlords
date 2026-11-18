//! HexGame in a window.
//!
//! Options:
//! - `--assets <dir>`: use this asset directory;
//! - `--frames <n>`: quit after drawing `n` frames (a smoke test);
//! - `--screenshot <file>`: draw one frame into a PNG file and quit;
//! - `--script <steps>`: play some input first (see `hexgame::script`), for
//!   example `--script "enter,right*3,enter"`.

#![forbid(unsafe_code)]

use std::path::PathBuf;

use hexgame::app::{App, Options, WINDOW_SIZE, WINDOW_TITLE};
use miniquad::conf::Conf;

const USAGE: &str = "usage: hexgame [--assets <dir>] [--frames <n>] [--screenshot <file.png>] [--script <steps>]";

fn parse_args(args: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut options = Options::default();
    let mut args = args;
    while let Some(arg) = args.next() {
        let mut value = |what: &str| args.next().ok_or(format!("{arg} needs {what}"));
        match arg.as_str() {
            "--assets" => options.assets = Some(PathBuf::from(value("a directory")?)),
            "--frames" => {
                let n = value("a number")?;
                options.frames = Some(n.parse().map_err(|_| format!("--frames: \"{n}\" is not a number"))?);
            }
            "--screenshot" => options.screenshot = Some(PathBuf::from(value("a file")?)),
            "--script" => options.script = Some(value("steps")?),
            "-h" | "--help" => return Err(String::new()),
            _ => return Err(format!("unknown option {arg}")),
        }
    }
    Ok(options)
}

fn main() {
    let options = match parse_args(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            if !message.is_empty() {
                eprintln!("hexgame: {message}");
            }
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    };
    let conf = Conf {
        window_title: WINDOW_TITLE.to_string(),
        window_width: WINDOW_SIZE.0,
        window_height: WINDOW_SIZE.1,
        high_dpi: true,
        ..Default::default()
    };
    miniquad::start(conf, move || match App::new(options) {
        Ok(app) => Box::new(app),
        Err(e) => {
            eprintln!("hexgame: {e}");
            std::process::exit(1);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Options, String> {
        parse_args(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn no_arguments_are_the_defaults() {
        let o = parse(&[]).unwrap();
        assert!(o.assets.is_none() && o.frames.is_none() && o.screenshot.is_none() && o.script.is_none());
    }

    #[test]
    fn every_option() {
        let o =
            parse(&["--assets", "a dir", "--frames", "30", "--screenshot", "shot.png", "--script", "enter,1"]).unwrap();
        assert_eq!(o.assets, Some(PathBuf::from("a dir")));
        assert_eq!(o.frames, Some(30));
        assert_eq!(o.screenshot, Some(PathBuf::from("shot.png")));
        assert_eq!(o.script.as_deref(), Some("enter,1"));
    }

    #[test]
    fn mistakes_are_reported() {
        assert_eq!(parse(&["--bogus"]).unwrap_err(), "unknown option --bogus");
        assert_eq!(parse(&["--frames"]).unwrap_err(), "--frames needs a number");
        assert_eq!(parse(&["--frames", "many"]).unwrap_err(), "--frames: \"many\" is not a number");
        assert_eq!(parse(&["--assets"]).unwrap_err(), "--assets needs a directory");
        // Help is the usage alone.
        assert_eq!(parse(&["--help"]).unwrap_err(), "");
    }
}
