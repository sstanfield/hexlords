//! Two things for the game's build.
//!
//! The list of the files in `assets/` for `src/files.rs`: in a browser
//! there are no files to read, so the WebAssembly build carries the assets
//! inside itself (`cargo run -p web`). Other builds read the list only in a
//! test, which holds it against the directory. The list is a Rust
//! expression in `$OUT_DIR/embedded.rs`: a slice of
//! `("assets/tiles/grass_tile.png", include_bytes!("<the file>"))`, sorted by
//! name, with `/` between a name's parts on every system. Files and
//! directories whose names begin with a dot are left out.
//!
//! And what the program is (the owner's word, 2026-11-24: "the git version a
//! binary was built from will be stamped into it"), for the About window
//! and `--version` (`src/about.rs`): `HEXLORDS_COMMIT`, as `main 9056171+`,
//! the branch and the commit, `+` if what the game is built from differs
//! from it (git's word, when the sources are in a repository and git is
//! there; else empty); and `HEXLORDS_BUILD`, as `release,
//! aarch64-apple-darwin, 2026-11-24 18:05 UTC, rustc 1.91.0`: the profile,
//! the target, when (UTC; `SOURCE_DATE_EPOCH` if it is set, for a build to be
//! made again the same) and the compiler. (After the way of sl-sh's `slosh/build.rs`.)

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

/// What the game is built from: what makes the build "+" when it differs
/// from the commit, and what runs this script again when it changes.
const SOURCES: [&str; 5] = ["src", "assets", "build.rs", "Cargo.toml", "Cargo.lock"];

fn main() {
    let root = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR"));
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    // A directory here means all that is in it.
    for source in SOURCES {
        println!("cargo:rerun-if-changed={source}");
    }
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
    println!("cargo:rustc-env=HEXLORDS_COMMIT={}", commit(&root).unwrap_or_default());
    println!("cargo:rustc-env=HEXLORDS_BUILD={}", build());

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

/// What `HEXLORDS_BUILD` says (the module's doc).
fn build() -> String {
    let mut parts = Vec::new();
    parts.push(std::env::var("PROFILE").unwrap_or_default());
    parts.push(std::env::var("TARGET").unwrap_or_default());
    parts.push(when());
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    if let Some(version) = output(Command::new(rustc).arg("--version")) {
        // "rustc 1.91.0 (f8297e351 2025-10-28)": the name and the number.
        parts.push(version.split(" (").next().unwrap_or(&version).to_string());
    }
    parts.retain(|p| !p.is_empty());
    parts.join(", ")
}

/// The branch and the commit the sources are at, `+` after it if what the
/// game is built from has changes not committed (the files git does not
/// track count for nothing); None without git or a repository. The script
/// runs again when the commit does.
fn commit(root: &Path) -> Option<String> {
    let git = |args: &[&str]| output(Command::new("git").current_dir(root).args(args));
    let hash = git(&["rev-parse", "--short", "HEAD"])?;
    let branch = git(&["rev-parse", "--abbrev-ref", "HEAD"]).filter(|b| b != "HEAD");
    // (Without the optional locks git does not write its index here, which
    // would run this script again at every build.)
    let mut status = vec!["--no-optional-locks", "status", "--porcelain", "--untracked-files=no", "--"];
    status.extend(SOURCES);
    let changed = git(&status).is_some_and(|s| !s.is_empty());
    if let Some(dir) = git(&["rev-parse", "--absolute-git-dir"]) {
        for file in ["HEAD", "index", "refs", "packed-refs"] {
            let path = Path::new(&dir).join(file);
            if path.exists() {
                println!("cargo:rerun-if-changed={}", path.display());
            }
        }
    }
    let plus = if changed { "+" } else { "" };
    Some(match branch {
        Some(branch) => format!("{branch} {hash}{plus}"),
        None => format!("{hash}{plus}"),
    })
}

/// What a command prints, trimmed, if it ran and said it went well.
fn output(command: &mut Command) -> Option<String> {
    let out = command.output().ok().filter(|o| o.status.success())?;
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// When the build is made, to the minute, in UTC.
fn when() -> String {
    let epoch = std::env::var("SOURCE_DATE_EPOCH").ok().and_then(|s| s.parse::<u64>().ok());
    let secs = epoch.unwrap_or_else(|| {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
    });
    let (days, rest) = ((secs / 86_400) as i64, secs % 86_400);
    let (year, month, day) = civil(days);
    format!("{year}-{month:02}-{day:02} {:02}:{:02} UTC", rest / 3600, rest % 3600 / 60)
}

/// The date `days` after 1970-01-01 (Howard Hinnant's `civil_from_days`).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}
