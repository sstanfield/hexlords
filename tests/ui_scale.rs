//! The interface's size from the Options window (the owner's word,
//! 2026-11-24: the text tiny on a small high resolution screen under Linux;
//! "If there is an option we can add to increase the DPI ... that would be
//! great"): chosen there, the screen is built again at it around the same
//! game, and the size is kept in the settings.

mod common;

use hexlords::options_window::ui_scale_id;
use hexlords::script;
use hexlords::shell::Shell;

fn dpi_of(shell: &mut Shell) -> i32 {
    shell.session().unwrap().screen.ui.dimension_converter().dp_to_px(96)
}

#[test]
fn a_size_chosen_in_options_builds_the_screen_again_around_the_same_game() {
    let session = common::session_at(1536, 1152, 96);
    let mut shell = Shell::playing(&common::assets(), session);
    shell.set_ui_scale(100, 96);
    let dir = std::env::temp_dir().join(format!("hexlords-ui-scale-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    shell.keep_games_in(dir.clone());
    // Some of the game, to find again after.
    script::run(&mut shell, "turn").unwrap();
    let (turn, units) = {
        let game = &shell.session().unwrap().screen.game;
        (game.turn(), game.map.units().iter().count())
    };
    assert_eq!(dpi_of(&mut shell), 96);
    let kept = shell.session().unwrap().kept_in().map(|p| p.to_path_buf());
    // Options (the Game menu), then 150%.
    script::run(&mut shell, "press:menu_game,press:options").unwrap();
    assert!(shell.session().unwrap().options_window().is_some(), "the options");
    script::run(&mut shell, &format!("press:{}", ui_scale_id(150))).unwrap();
    // Built again, at half again the size; the same game; its windows gone.
    assert_eq!(dpi_of(&mut shell), 144);
    let session = shell.session().unwrap();
    assert_eq!((session.screen.game.turn(), session.screen.game.map.units().iter().count()), (turn, units));
    assert_eq!(session.options().ui_scale, 150);
    assert!(session.options_window().is_none());
    assert_eq!(session.kept_in().map(|p| p.to_path_buf()), kept, "kept where it was (if it was)");
    // Kept in the settings, for the next start.
    assert_eq!(hexlords::settings::ui_scale(&dir), Some(150));
    // And back to 100%.
    script::run(&mut shell, "press:menu_game,press:options").unwrap();
    script::run(&mut shell, &format!("press:{}", ui_scale_id(100))).unwrap();
    assert_eq!(dpi_of(&mut shell), 96);
    assert_eq!(hexlords::settings::ui_scale(&dir), Some(100));
    let _ = std::fs::remove_dir_all(&dir);
}
