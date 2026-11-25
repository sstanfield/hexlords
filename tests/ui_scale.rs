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

/// A hex of the continent with red's units, to find again.
fn red_units(shell: &mut Shell) -> usize {
    let game = &shell.session().unwrap().screen.game;
    game.map.units().iter().filter(|u| u.side() == hexlords::state::Side::Red).count()
}

/// In hotseat, the screen built again in the middle of a person's turn is
/// still theirs: not covered for them again (it did not know whose turn it
/// had shown last).
#[test]
fn in_hotseat_a_new_size_leaves_the_turn_uncovered() {
    let (game, _) = hexlords::game::Game::load(std::path::Path::new("assets/maps/continent")).unwrap();
    let mut setup = hexlords::setup::Setup::solo(&game.map, Some(hexlords::state::Side::Red), "passive");
    setup.set_players("blue=person").unwrap();
    let game = game.with_setup(setup);
    let session = common::with_game(&common::assets(), turbo_ferret::Size::new(1536, 1152), 96, game).unwrap();
    let mut shell = Shell::playing(&common::assets(), session);
    shell.set_ui_scale(100, 96);
    if shell.session().unwrap().covered() {
        script::run(&mut shell, &format!("press:{}", hexlords::session::BEGIN_TURN_ID)).unwrap();
    }
    assert!(!shell.session().unwrap().covered());
    script::run(&mut shell, "press:menu_game,press:options").unwrap();
    script::run(&mut shell, &format!("press:{}", ui_scale_id(150))).unwrap();
    assert_eq!(dpi_of(&mut shell), 144);
    script::run(&mut shell, "wait:100").unwrap();
    assert!(!shell.session().unwrap().covered(), "still red's turn, uncovered");
}

/// A screen that cannot be built again (here: no assets where the shell
/// looks) leaves the game as it was, at the size it was: it had a blank
/// map in its place.
#[test]
fn a_size_that_cannot_be_built_leaves_the_game_as_it_was() {
    let (game, _) = hexlords::game::Game::load(std::path::Path::new("assets/maps/continent")).unwrap();
    let session = common::with_game(&common::assets(), turbo_ferret::Size::new(1536, 1152), 96, game).unwrap();
    let mut shell = Shell::playing(std::path::Path::new("no/such/assets"), session);
    shell.set_ui_scale(100, 96);
    let units = red_units(&mut shell);
    assert!(units > 0);
    script::run(&mut shell, "press:menu_game,press:options").unwrap();
    script::run(&mut shell, &format!("press:{}", ui_scale_id(150))).unwrap();
    assert_eq!(dpi_of(&mut shell), 96, "at the size it was");
    assert_eq!(red_units(&mut shell), units, "the same game");
    assert_ne!(shell.session().unwrap().screen.game.map.rows(), 20, "not the blank map");
    assert_eq!(shell.session().unwrap().options().ui_scale, 100);
}
