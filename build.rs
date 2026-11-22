//! Writes the list of the files in `assets/` for `src/files.rs`: in a browser
//! there are no files to read, so the WebAssembly build carries the assets
//! inside itself (`cargo run -p web`). Other builds read the list only in a
//! test, which holds it against the directory.
//!
//! The list is a Rust expression in `$OUT_DIR/embedded.rs`: a slice of
//! `("assets/tiles/grass_tile.png", include_bytes!("<the file>"))`, sorted by
//! name, with `/` between a name's parts on every system. Files and
//! directories whose names begin with a dot are left out.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn main() {
    let root = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR"));
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    // A directory here means all that is in it.
    println!("cargo:rerun-if-changed=assets");
    println!("cargo:rerun-if-changed=build.rs");

    let mut files = Vec::new();
    walk(&root.join("assets"), "assets", &mut files);
    files.sort();
    let mut text = String::from("&[\n");
    for (name, path) in &files {
        // `{:?}` writes a path as a string literal (a backslash doubled).
        let _ = writeln!(text, "    ({name:?}, include_bytes!({:?}) as &[u8]),", path.display().to_string());
    }
    text.push_str("]\n");
    std::fs::write(out.join("embedded.rs"), text).expect("write embedded.rs");
}

fn walk(dir: &Path, name: &str, files: &mut Vec<(String, PathBuf)>) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    for entry in entries {
        let entry = entry.unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
        let file = entry.file_name().to_string_lossy().into_owned();
        if file.starts_with('.') {
            continue;
        }
        let (path, name) = (entry.path(), format!("{name}/{file}"));
        if path.is_dir() {
            walk(&path, &name, files);
        } else {
            files.push((name, path));
        }
    }
}
