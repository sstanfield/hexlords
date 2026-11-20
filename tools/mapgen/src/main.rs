//! `mapgen`: make random maps, measure maps, check maps (`docs/mapgen.md`).
//!
//! ```text
//! mapgen gen   [--preset NAME] [--config FILE] [--units PATH] [--seed N] [--KNOB VALUE ...]
//!              --out DIR [--preview] [--layers]
//! mapgen batch [the same] --seeds A..B --out DIR [--sheet]
//! mapgen stats DIR
//! mapgen check DIR
//! ```
//!
//! Not a port: the C++ had no maps but one.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use hexgame::mapgen::config::Config;
use hexgame::mapgen::preview::{self, Image};
use hexgame::mapgen::stats::Stats;
use hexgame::mapgen::{Generated, check, generate, score};
use hexgame::state::scenario;

const USAGE: &str = "\
usage: mapgen gen   [--preset NAME] [--config FILE] [--units PATH] [--seed N] [--KNOB VALUE ...]
                    --out DIR [--preview] [--layers]
       mapgen batch [the same] --seeds A..B --out DIR [--sheet]
       mapgen stats DIR
       mapgen check DIR
The knobs are docs/mapgen.md's (--water 0.25, --sides 6, --sea-cities, ...).";

/// The units template unless told otherwise.
const UNITS: &str = "assets/mapgen/units.txt";
/// A hex's half width in a preview, in pixels.
const PREVIEW: f64 = 4.0;

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
            let map = scenario::load(&PathBuf::from(dir)).map_err(|e| e.to_string())?.map;
            print!("{}", Stats::of(&map).report());
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
        [cmd, rest @ ..] if cmd == "gen" || cmd == "batch" => make(cmd == "batch", rest),
        _ => Err(USAGE.to_string()),
    }
}

/// What `gen` and `batch` were asked.
struct Ask {
    config: Config,
    seeded: bool,
    units: PathBuf,
    out: Option<PathBuf>,
    preview: bool,
    layers: bool,
    sheet: bool,
    seeds: Option<(u64, u64)>,
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn parse(args: &[String]) -> Result<Ask, String> {
    let mut ask = Ask {
        config: Config::default(),
        seeded: false,
        units: PathBuf::from(UNITS),
        out: None,
        preview: false,
        layers: false,
        sheet: false,
        seeds: None,
    };
    let mut i = 0;
    let value = |i: usize, flag: &str| args.get(i + 1).cloned().ok_or(format!("{flag} needs a value"));
    while i < args.len() {
        let flag = args[i].as_str();
        match flag {
            "--preset" => {
                let name = value(i, flag)?;
                ask.config = if Path::new(&name).is_file() {
                    Config::default().with_text(&read(Path::new(&name))?)?
                } else {
                    Config::preset(&name)?
                };
                i += 2;
            }
            "--config" => {
                let path = PathBuf::from(value(i, flag)?);
                ask.config = ask.config.with_text(&read(&path)?).map_err(|e| format!("{}: {e}", path.display()))?;
                ask.seeded |= read(&path)?.lines().any(|l| l.trim_start().starts_with("seed"));
                i += 2;
            }
            "--units" => {
                ask.units = PathBuf::from(value(i, flag)?);
                i += 2;
            }
            "--out" => {
                ask.out = Some(PathBuf::from(value(i, flag)?));
                i += 2;
            }
            "--seeds" => {
                let v = value(i, flag)?;
                let (a, b) = v.split_once("..").ok_or(format!("--seeds: \"{v}\" is not A..B"))?;
                let n = |s: &str| s.parse::<u64>().map_err(|_| format!("--seeds: \"{v}\" is not A..B"));
                ask.seeds = Some((n(a)?, n(b)?));
                i += 2;
            }
            "--preview" => (ask.preview, i) = (true, i + 1),
            "--layers" => (ask.layers, i) = (true, i + 1),
            "--sheet" => (ask.sheet, i) = (true, i + 1),
            // A checkbox: on.
            "--sea-cities" | "--fly-cities" => {
                ask.config.set(&flag[2..], "on")?;
                i += 1;
            }
            knob if knob.starts_with("--") => {
                let v = value(i, flag)?;
                ask.config.set(&knob[2..], &v).map_err(|e| format!("{knob}: {e}"))?;
                ask.seeded |= knob == "--seed";
                i += 2;
            }
            other => return Err(format!("\"{other}\"?\n{USAGE}")),
        }
    }
    Ok(ask)
}

fn make(batch: bool, args: &[String]) -> Result<ExitCode, String> {
    let mut ask = parse(args)?;
    let out = ask.out.clone().ok_or("--out DIR is needed")?;
    let units = read(&ask.units)?;
    let template = ask.units.display().to_string();
    if !batch {
        if !ask.seeded {
            // A seed from the clock, said, so the map can be made again.
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos());
            ask.config.seed = (now % 1_000_000_007) as u64;
        }
        println!("seed {}", ask.config.seed);
        let made = generate(&ask.config, &units, &template)?;
        write(&made, &out, ask.preview, ask.layers)?;
        println!("wrote {}", out.display());
        return Ok(ExitCode::SUCCESS);
    }
    let (a, b) = ask.seeds.ok_or("--seeds A..B is needed")?;
    let mut sheet: Vec<(u64, Image)> = Vec::new();
    let mut lines = String::new();
    for seed in a..=b {
        let mut cfg = ask.config.clone();
        cfg.seed = seed;
        let made = generate(&cfg, &units, &template)?;
        let dir = out.join(format!("seed-{seed}"));
        let (img, summary) = write(&made, &dir, true, ask.layers)?;
        lines.push_str(&format!("seed {seed}: {summary}\n"));
        sheet.push((seed, img));
    }
    if ask.sheet {
        let columns = (sheet.len() as f64).sqrt().ceil() as usize;
        let img = preview::sheet(&sheet, columns, 400);
        std::fs::write(out.join("sheet.png"), img.png()).map_err(|e| e.to_string())?;
        std::fs::write(out.join("sheet.txt"), &lines).map_err(|e| e.to_string())?;
    }
    print!("{lines}");
    Ok(ExitCode::SUCCESS)
}

/// Write a map's directory; its preview and a line about it.
fn write(made: &Generated, dir: &Path, picture: bool, layers: bool) -> Result<(Image, String), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let put = |name: &str, bytes: &[u8]| std::fs::write(dir.join(name), bytes).map_err(|e| format!("{name}: {e}"));
    put("terrain.map", made.terrain_text().as_bytes())?;
    put("cities.txt", made.cities_text().as_bytes())?;
    put("units.txt", made.units.as_bytes())?;
    put("mapgen.txt", made.config_text().as_bytes())?;
    let map = made.map()?;
    let stats = Stats::of(&map);
    let metrics = score::terrain(&made.config, &stats);
    let misses: Vec<&str> = metrics.iter().filter(|m| !m.ok()).map(|m| m.name.as_str()).collect();
    let summary = format!(
        "score {:.2}{}",
        score::score(&metrics),
        if misses.is_empty() { String::new() } else { format!("; misses {}", misses.join(", ")) }
    );
    let text = format!("{}\nthe targets:\n{}", stats.report(), score::table(&metrics));
    put("stats.txt", text.as_bytes())?;
    let img = preview::preview(&map, PREVIEW, &[]);
    if picture {
        put("preview.png", &img.png())?;
    }
    if layers {
        let dir = dir.join("layers");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let g = &made.state;
        for (name, grid) in [("elevation", &g.elevation), ("moisture", &g.moisture), ("ridge", &g.ridge)] {
            let png = preview::layer(grid, PREVIEW).png();
            std::fs::write(dir.join(format!("{name}.png")), png).map_err(|e| e.to_string())?;
        }
    }
    Ok((img, summary))
}
