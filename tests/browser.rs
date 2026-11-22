//! The game also runs in a browser (`cargo run -p web`), where there are no
//! files, no clock of the standard library's and no terminal. Nothing here
//! runs a browser: these hold the code to what exists there, since a slip
//! builds everywhere and only stops the game once it is on a page.

use std::path::{Path, PathBuf};

/// The game's own source files.
fn sources(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sources(&path, found);
        } else if path.extension().is_some_and(|x| x == "rs") {
            found.push(path);
        }
    }
}

/// A file's code without its tests (they run on a desktop) and its comments.
fn code(text: &str) -> String {
    let code = text.split("#[cfg(test)]").next().unwrap_or(text);
    code.lines().filter(|line| !line.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n")
}

#[test]
fn the_game_reads_its_files_and_the_time_in_ways_a_browser_has() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    sources(&src, &mut files);
    assert!(files.len() > 50, "the sources were not found");
    // What panics or finds nothing in a browser, and who may use it: the
    // modules that stand between the game and the system.
    let between = ["files.rs", "platform.rs", "paths.rs"];
    let not_there = [
        ("std::fs::read", "crate::files::read"),
        ("fs::read_dir", "crate::files::list"),
        ("fs::read_to_string", "crate::files::read_to_string"),
        (".is_file()", "crate::files::is_file"),
        (".is_dir()", "crate::files::is_dir"),
        ("FsLoader", "screen::AssetLoader"),
        ("Instant::now", "platform::Clock"),
        ("SystemTime::now", "platform::clock_nanos"),
        ("std::thread", "nothing: a page has one thread"),
    ];
    let mut wrong = Vec::new();
    for file in &files {
        let name = file.strip_prefix(&src).unwrap().display().to_string();
        if between.contains(&name.as_str()) {
            continue;
        }
        let code = code(&std::fs::read_to_string(file).unwrap());
        for (what, instead) in not_there {
            if code.contains(what) {
                wrong.push(format!("src/{name} uses {what}: in a browser that fails (use {instead})"));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn the_asset_loader_reads_what_the_ui_asks_for() {
    // The skin and the layouts come through the game's own reader of files
    // (in a browser, from inside the program), with Turbo Ferret's errors.
    use hexlords::screen::AssetLoader;
    use turbo_ferret::{LoadError, ResourceLoader};
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
    let mut loader = AssetLoader::new(&assets);
    let skin = loader.load("skin/skin.tb.txt").unwrap();
    assert_eq!(&skin[..], &std::fs::read(assets.join("skin").join("skin.tb.txt")).unwrap()[..]);
    assert_eq!(loader.load("skin/none.tb.txt"), Err(LoadError::NotFound("skin/none.tb.txt".to_string())));
}

#[test]
fn the_page_and_the_build_agree_on_the_canvas_color() {
    // While the game loads, the page is the color the game clears to.
    let page = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tools/web/index.html")).unwrap();
    let (r, g, b, _) = hexlords::app::CLEAR_COLOR;
    let hex = |v: f32| (v * 255.0).round() as u8;
    let color = format!("background: #{:02x}{:02x}{:02x};", hex(r), hex(g), hex(b));
    assert!(page.contains(&color), "the page's background is not {color}");
}
