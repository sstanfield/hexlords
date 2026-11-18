//! HexGame in a window.
//!
//! Options:
//! - `--assets <dir>`: use this asset directory;
//! - `--frames <n>`: quit after drawing `n` frames (a smoke test).

#![forbid(unsafe_code)]

use std::path::PathBuf;

use hexgame::app::{App, Options, WINDOW_SIZE, WINDOW_TITLE};
use miniquad::conf::Conf;

const USAGE: &str = "usage: hexgame [--assets <dir>] [--frames <n>]";

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
