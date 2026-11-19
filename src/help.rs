//! The help window's words: every key and what the mouse does, in sections.
//! `MapScreen` opens and closes the window (F1, or `?`); a scene gives the
//! sections of its own (`MapScreen::set_help`), and the sections every map
//! screen shares (moving around the map, the window's keys) come from here.
//!
//! Not a port: the C++ had no help.

/// A heading and its rows: what to press or do, and what it does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HelpSection {
    pub title: String,
    pub rows: Vec<(String, String)>,
}

impl HelpSection {
    pub fn new(title: &str, rows: &[(&str, &str)]) -> HelpSection {
        let rows = rows.iter().map(|(keys, what)| (keys.to_string(), what.to_string())).collect();
        HelpSection { title: title.to_string(), rows }
    }
}

/// What this system calls the key that goes with a letter for a command: Cmd
/// on macOS, Ctrl elsewhere. (Either works everywhere.)
pub fn command_key() -> &'static str {
    if cfg!(target_os = "macos") { "Cmd" } else { "Ctrl" }
}

/// The keys for fullscreen, as this system has them: macOS keeps F11 for
/// itself, and its own key for fullscreen is Ctrl+Cmd+F. (Both work
/// everywhere; F11 is the others' usual key.)
pub fn fullscreen_keys() -> &'static str {
    if cfg!(target_os = "macos") { "Ctrl+Cmd+F" } else { "F11" }
}

/// Moving around the map, the same in every window with one.
pub fn map_section() -> HelpSection {
    HelpSection::new(
        "The map",
        &[
            ("Pointer on the map", "The hex under it is the cursor"),
            ("Arrow keys", "Move the cursor"),
            ("1  /  2", "Zoom out  /  in"),
            ("Wheel, two fingers", "Move the map (with Shift: sideways)"),
            ("Ctrl or Cmd + wheel", "Zoom in or out at the pointer"),
            ("Right or middle drag", "Pull the map along"),
            ("Click the minimap", "Go there (drag to keep going)"),
        ],
    )
}

/// The window's own keys.
pub fn window_section() -> HelpSection {
    let quit = format!("{}+Q", command_key());
    HelpSection::new(
        "The window",
        &[("F1  or  ?", "Open and close this help"), (fullscreen_keys(), "Fullscreen on and off"), (&quit, "Quit")],
    )
}

/// The help of a window whose scene has `own` sections: the map's first, then
/// the scene's, then the window's.
pub fn sections(own: &[HelpSection]) -> Vec<HelpSection> {
    let mut all = vec![map_section()];
    all.extend(own.iter().cloned());
    all.push(window_section());
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_map_comes_first_and_the_window_last() {
        let own = HelpSection::new("Units", &[("Click a unit", "Select it")]);
        let all = sections(std::slice::from_ref(&own));
        let titles: Vec<&str> = all.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["The map", "Units", "The window"]);
        assert_eq!(all[1], own);
        // Every row says what to press and what it does.
        assert!(all.iter().flat_map(|s| &s.rows).all(|(keys, what)| !keys.is_empty() && !what.is_empty()));
    }

    #[test]
    fn the_keys_are_named_as_the_system_names_them() {
        let window = window_section();
        let keys: Vec<&str> = window.rows.iter().map(|(keys, _)| keys.as_str()).collect();
        if cfg!(target_os = "macos") {
            assert_eq!(keys, ["F1  or  ?", "Ctrl+Cmd+F", "Cmd+Q"]);
        } else {
            assert_eq!(keys, ["F1  or  ?", "F11", "Ctrl+Q"]);
        }
    }
}
