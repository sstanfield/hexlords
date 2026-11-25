//! The person's settings that are no game's own, kept in a small text file
//! beside the saved games (`platform::saves_dir`; a browser has no files,
//! and keeps them for the page's life only): a line each, a key and its
//! value. Today one: the interface's size (`ui-scale 150`, the Options
//! window's; the owner's word, 2026-11-24, small high resolution screens
//! under Linux). A line this version does not know is passed over.

use std::path::Path;

use crate::files;

/// The file's name, in the directory of saved games.
pub const FILE: &str = "settings.txt";

/// The interface's size kept in `dir`'s settings, if it is there and a
/// size (`app::parse_ui_scale`).
pub fn ui_scale(dir: &Path) -> Option<u32> {
    let text = files::read_to_string(&dir.join(FILE)).ok()?;
    text.lines()
        .filter_map(|line| line.trim().strip_prefix("ui-scale "))
        .find_map(|value| crate::app::parse_ui_scale(value).ok())
}

/// Keep the interface's size in `dir`'s settings (the file made, or
/// written again whole).
pub fn set_ui_scale(dir: &Path, percent: u32) -> Result<(), String> {
    let path = dir.join(FILE);
    files::write_whole(&path, format!("ui-scale {percent}\n").as_bytes())
        .map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_interface_size_is_kept_and_read_back() {
        let dir = std::env::temp_dir().join(format!("hexlords-settings-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(ui_scale(&dir), None, "no file: none");
        set_ui_scale(&dir, 150).unwrap();
        assert_eq!(ui_scale(&dir), Some(150));
        std::fs::write(dir.join(FILE), "colour blue\nui-scale 175\n").unwrap();
        assert_eq!(ui_scale(&dir), Some(175), "a line it does not know is passed over");
        std::fs::write(dir.join(FILE), "ui-scale big\n").unwrap();
        assert_eq!(ui_scale(&dir), None, "no size: none");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
