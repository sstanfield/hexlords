//! What the program is: its version and what it was built from (the
//! owner's word, 2026-11-24), for the About window (the Help menu) and
//! `--version`.
//!
//! The version is the package's (`Cargo.toml`), kept as `CLAUDE.md` says
//! ("Version"): a change to the rules (saved games may not load) bumps the
//! minor number, any other change to the game the patch. What it was built
//! from is `build.rs`'s stamp: the branch and the commit, `+` if the
//! sources had changes not committed (`HEXLORDS_COMMIT`); the profile, the
//! target, when, and the compiler (`HEXLORDS_BUILD`).

use crate::guide::section;
use crate::help::HelpSection;

/// The game's name.
pub const NAME: &str = "Hexlords";

/// The version: `0.<rules>.<patch>`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The branch and the commit it was built from, `+` after it if the
/// sources had changes not committed ("main 9056171+"); empty if they were
/// not in git (`build.rs`).
pub const COMMIT: &str = env!("HEXLORDS_COMMIT");

/// How it was built: the profile, the target, when, the compiler
/// (`build.rs`).
pub const BUILD: &str = env!("HEXLORDS_BUILD");

/// The About window's id (the item of the Help menu that opens it).
pub const ABOUT_ID: &str = "help_about";

/// The About window's title.
pub const TITLE: &str = "About Hexlords";

/// One line: the name, the version and the build, as `--version` prints it.
pub fn line() -> String {
    let commit = if COMMIT.is_empty() { String::new() } else { format!("{COMMIT}, ") };
    format!("{} {VERSION} ({commit}{BUILD})", NAME.to_lowercase())
}

/// What the About window says, in the help window's lines
/// (`guide::section`).
pub fn sections() -> Vec<HelpSection> {
    let built = match commit() {
        Some((commit, true)) => format!("{commit}, with changes not committed"),
        Some((commit, false)) => commit.to_string(),
        None => "sources outside git".to_string(),
    };
    // The profile and the target; when; the compiler: a row each.
    let parts: Vec<&str> = BUILD.split(", ").collect();
    vec![
        section(
            NAME,
            &[
                ("Version", VERSION.to_string()),
                ("Built from", built),
                ("Build", parts.get(..2).map_or(BUILD.to_string(), |p| p.join(", "))),
                ("Built", parts.get(2).map_or(String::new(), |p| p.to_string())),
                ("Compiler", parts.get(3).map_or(String::new(), |p| p.to_string())),
                (
                    "The game",
                    "A strategy game on a hex map, in turns or in rounds: cities bring in gold and build \
                     units, and stacks of them fight over the map until one side is left."
                        .to_string(),
                ),
            ],
        ),
        section(
            "Made with",
            &[
                ("Turbo Ferret", "The windows and the buttons (a Rust port of Turbo Badger)".to_string()),
                ("miniquad", "The graphics".to_string()),
                ("Licences", "Zlib: LICENSE and NOTICE come with the game".to_string()),
            ],
        ),
    ]
}

/// The commit the game was built from (with its branch), and whether the
/// sources had changes not committed; None if they were not in git.
pub fn commit() -> Option<(&'static str, bool)> {
    (!COMMIT.is_empty()).then(|| (COMMIT.trim_end_matches('+'), COMMIT.ends_with('+')))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_is_the_packages_and_the_build_says_how() {
        assert_eq!(VERSION.split('.').count(), 3, "{VERSION}");
        assert!(VERSION.split('.').all(|n| n.parse::<u32>().is_ok()), "{VERSION}");
        let line = line();
        assert!(line.starts_with(&format!("hexlords {VERSION} (")), "{line}");
        // The profile and the compiler are always there.
        assert!(BUILD.contains("debug") || BUILD.contains("release"), "{BUILD}");
        assert!(BUILD.contains("rustc "), "{BUILD}");
        assert!(BUILD.contains(" UTC"), "{BUILD}");
        let rows: Vec<String> = sections().iter().flat_map(|s| s.rows.iter().map(|(k, _)| k.clone())).collect();
        assert!(rows.contains(&"Version".to_string()) && rows.contains(&"Built from".to_string()), "{rows:?}");
    }
}
