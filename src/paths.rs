//! Where the assets live.
//!
//! C++: src/settings.cpp Settings (a singleton holding the asset and shader
//! directories) and src/main.cpp (the asset path from `argv[0]/../assets/`).

use std::path::{Path, PathBuf};

/// The asset directory, by the first rule that applies:
///
/// 1. `--assets <dir>`, if given;
/// 2. `<exe dir>/../assets`, if it exists (the C++ rule, for installed builds);
/// 3. `assets` next to `Cargo.toml`, so that `cargo run` works.
pub fn asset_root(arg: Option<&Path>) -> PathBuf {
    let exe = std::env::current_exe().ok();
    resolve(arg, exe.as_deref(), Path::new(env!("CARGO_MANIFEST_DIR")), &|p| p.is_dir())
}

/// [`asset_root`] with its inputs passed in.
pub fn resolve(arg: Option<&Path>, exe: Option<&Path>, manifest_dir: &Path, is_dir: &dyn Fn(&Path) -> bool) -> PathBuf {
    if let Some(dir) = arg {
        return dir.to_path_buf();
    }
    if let Some(bin_dir) = exe.and_then(Path::parent) {
        let installed = bin_dir.join("..").join("assets");
        if is_dir(&installed) {
            return installed;
        }
    }
    manifest_dir.join("assets")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_argument_wins() {
        let got = resolve(Some(Path::new("elsewhere")), Some(Path::new("bin/hexgame")), Path::new("repo"), &|_| true);
        assert_eq!(got, Path::new("elsewhere"));
    }

    #[test]
    fn next_to_the_executable_if_it_exists() {
        let exe = Path::new("opt").join("bin").join("hexgame");
        let want = Path::new("opt").join("bin").join("..").join("assets");
        assert_eq!(resolve(None, Some(&exe), Path::new("repo"), &|p| p == want), want);
    }

    #[test]
    fn the_repository_otherwise() {
        let exe = Path::new("target").join("debug").join("hexgame");
        let want = Path::new("repo").join("assets");
        assert_eq!(resolve(None, Some(&exe), Path::new("repo"), &|_| false), want);
        assert_eq!(resolve(None, None, Path::new("repo"), &|_| true), want);
    }

    #[test]
    fn the_repository_has_the_assets() {
        let root = resolve(None, None, Path::new(env!("CARGO_MANIFEST_DIR")), &|_| false);
        assert!(root.join("skin").join("skin.tb.txt").is_file());
        assert!(root.join("tiles").join("grass_tile.png").is_file());
    }
}
