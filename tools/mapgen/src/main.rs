//! `mapgen`: make random maps, measure maps, check maps (`docs/mapgen.md`).
//!
//! ```text
//! mapgen stats DIR     what a map measures
//! mapgen check DIR     what is wrong with a map, and each city's reach
//! ```
//!
//! Not a port: the C++ had no maps but one.

use std::path::PathBuf;
use std::process::ExitCode;

use hexgame::mapgen::{check, stats};
use hexgame::state::scenario;

const USAGE: &str = "usage: mapgen stats DIR | mapgen check DIR";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("mapgen: {e}");
            ExitCode::from(2)
        }
    }
}

fn run(args: &[String]) -> Result<ExitCode, String> {
    match args {
        [cmd, dir] if cmd == "stats" => {
            let map = read(dir)?;
            print!("{}", stats::Stats::of(&map).report());
            Ok(ExitCode::SUCCESS)
        }
        [cmd, dir] if cmd == "check" => {
            let scenario = scenario::load(&PathBuf::from(dir)).map_err(|e| e.to_string())?;
            for w in &scenario.warnings {
                println!("warning: {w}");
            }
            let c = check::check(&scenario.map, false);
            print!("{}", c.report());
            Ok(if c.problems.is_empty() { ExitCode::SUCCESS } else { ExitCode::FAILURE })
        }
        _ => Err(USAGE.to_string()),
    }
}

fn read(dir: &str) -> Result<hexgame::state::Map, String> {
    scenario::load(&PathBuf::from(dir)).map(|s| s.map).map_err(|e| e.to_string())
}
