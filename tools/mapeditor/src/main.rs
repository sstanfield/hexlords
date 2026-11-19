#![forbid(unsafe_code)]

use std::path::PathBuf;

use hexgame::app::{self, OPTIONS_USAGE, Options, WindowSpec};
use hexgame::script;
use hexgame::state::{Map, TileType};
use mapeditor::check::check_path;
use mapeditor::editor::Editor;

const WINDOW: WindowSpec = WindowSpec { program: "mapeditor", title: "HexGame Map Editor", size: (1280, 800) };
/// The size of a new map when `--new` does not say.
const NEW_SIZE: (u16, u16) = (100, 100);

/// The command line.
#[derive(Clone, Debug, Default, PartialEq)]
struct Args {
    window: Options,
    /// The map file to edit. One that does not exist yet is where a new map goes.
    file: Option<PathBuf>,
    /// `--new <rows>x<cols>`: start a new map of this size.
    new: Option<(u16, u16)>,
    /// `--check`: report on the files and quit, without a window.
    check: Vec<PathBuf>,
    /// `--script <steps>`: input to play before the first frame.
    script: Option<String>,
}

fn usage() -> String {
    format!(
        "usage: mapeditor [<file.map>] [--new <rows>x<cols>] [--script <steps>] {OPTIONS_USAGE}\n       \
         mapeditor --check <map>..."
    )
}

fn parse_size(s: &str) -> Result<(u16, u16), String> {
    let bad = || format!("--new: \"{s}\" is not a size like 60x80 (rows x columns)");
    let (rows, cols) = s.split_once(['x', 'X']).ok_or_else(bad)?;
    Ok((rows.trim().parse().map_err(|_| bad())?, cols.trim().parse().map_err(|_| bad())?))
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut parsed = Args::default();
    let mut checking = false;
    let mut args = args;
    while let Some(arg) = args.next() {
        if parsed.window.take(&arg, &mut args)? {
            continue;
        }
        let mut value = |what: &str| args.next().ok_or(format!("{arg} needs {what}"));
        match arg.as_str() {
            "--new" => parsed.new = Some(parse_size(&value("a size")?)?),
            "--script" => parsed.script = Some(value("steps")?),
            "--check" => checking = true,
            "-h" | "--help" => return Err(String::new()),
            _ if arg.starts_with('-') => return Err(format!("unknown option {arg}")),
            _ if checking => parsed.check.push(PathBuf::from(arg)),
            _ if parsed.file.is_some() => return Err(format!("one map file at a time: {arg}")),
            _ => parsed.file = Some(PathBuf::from(arg)),
        }
    }
    if checking && parsed.check.is_empty() {
        return Err("--check needs a file".to_string());
    }
    Ok(parsed)
}

/// The map to start with: the file's if it exists, else a new one.
fn first_map(file: Option<&PathBuf>, new: Option<(u16, u16)>) -> Result<Map, String> {
    match (file, new) {
        // Of a map directory the editor edits the terrain.
        (Some(path), None) if path.exists() => {
            Map::load(&hexgame::state::scenario::terrain_path(path)).map_err(|e| e.to_string())
        }
        (Some(path), Some(_)) if path.exists() => {
            Err(format!("--new: {} exists already (leave --new out to edit it)", path.display()))
        }
        _ => {
            let (rows, cols) = new.unwrap_or(NEW_SIZE);
            Map::new(rows, cols, TileType::Grass).map_err(|e| format!("--new: {e}"))
        }
    }
}

/// `--check`: print what each map holds, a file or a directory. False if one
/// of them is no map.
fn check_files(files: &[PathBuf], assets: &std::path::Path) -> bool {
    let mut good = true;
    for path in files {
        match check_path(path, Some(assets)) {
            Ok(report) => print!("{}: {report}", path.display()),
            Err(e) => {
                println!("{}: error: {e}", path.display());
                good = false;
            }
        }
    }
    good
}

fn main() {
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(message) => {
            if !message.is_empty() {
                eprintln!("mapeditor: {message}");
            }
            eprintln!("{}", usage());
            std::process::exit(2);
        }
    };
    if !args.check.is_empty() {
        let assets = hexgame::paths::asset_root(args.window.assets.as_deref());
        std::process::exit(if check_files(&args.check, &assets) { 0 } else { 1 });
    }
    let Args { window, file, new, script, .. } = args;
    let map = match first_map(file.as_ref(), new) {
        Ok(map) => map,
        Err(e) => {
            eprintln!("mapeditor: {e}");
            std::process::exit(1);
        }
    };
    app::run(WINDOW, window, move |assets, viewport, dpi| {
        let mut editor = Editor::new(assets, viewport, dpi, map, file)?;
        if let Some(steps) = &script {
            script::run(&mut editor, steps).map_err(|e| format!("--script: {e}"))?;
        }
        Ok(editor)
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
        assert_eq!(parse(&[]).unwrap(), Args::default());
    }

    #[test]
    fn every_option() {
        let a = parse(&["maps/a.map", "--new", "60x80", "--script", "key:f", "--frames", "3"]).unwrap();
        assert_eq!(a.file, Some(PathBuf::from("maps/a.map")));
        assert_eq!(a.new, Some((60, 80)));
        assert_eq!(a.script.as_deref(), Some("key:f"));
        assert_eq!(a.window.frames, Some(3));
        let a = parse(&["--check", "a.map", "b.map"]).unwrap();
        assert_eq!(a.check, [PathBuf::from("a.map"), PathBuf::from("b.map")]);
        assert_eq!(a.file, None);
    }

    #[test]
    fn mistakes_are_reported() {
        assert_eq!(parse(&["--bogus"]).unwrap_err(), "unknown option --bogus");
        assert_eq!(parse(&["a.map", "b.map"]).unwrap_err(), "one map file at a time: b.map");
        assert_eq!(parse(&["--new"]).unwrap_err(), "--new needs a size");
        assert_eq!(parse(&["--new", "60"]).unwrap_err(), "--new: \"60\" is not a size like 60x80 (rows x columns)");
        assert_eq!(parse(&["--new", "ax9"]).unwrap_err(), "--new: \"ax9\" is not a size like 60x80 (rows x columns)");
        assert_eq!(parse(&["--check"]).unwrap_err(), "--check needs a file");
        assert_eq!(parse(&["--help"]).unwrap_err(), "");
    }

    #[test]
    fn the_first_map() {
        let none: Option<&PathBuf> = None;
        let m = first_map(none, None).unwrap();
        assert_eq!((m.rows(), m.cols()), NEW_SIZE);
        let m = first_map(none, Some((30, 40))).unwrap();
        assert_eq!((m.rows(), m.cols()), (30, 40));
        assert!(first_map(none, Some((5, 40))).unwrap_err().starts_with("--new: invalid map dimensions"));
        // A file that is not there yet is where the new map will go.
        let missing = PathBuf::from("no/such/dir/new.map");
        assert_eq!(first_map(Some(&missing), Some((20, 20))).unwrap().rows(), 20);
        // One that is there is read, and --new would overwrite it.
        let here = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        assert!(first_map(Some(&here), None).is_err(), "not a map");
        assert!(first_map(Some(&here), Some((20, 20))).unwrap_err().contains("exists already"));
    }
}
