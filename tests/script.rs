//! The input scripts of the screenshot mode (`--script`).

mod common;

use common::session;
use hexgame::script;
use hexgame::state::Hex;

#[test]
fn keys_go_through_the_ui() {
    let mut s = session();
    script::run(&mut s, "left*3, up, down*2,right").unwrap();
    assert_eq!(s.screen.game.map.cursor, Hex::new(51, 48));
    script::run(&mut s, "1,1").unwrap();
    assert_eq!((s.screen.view.zoom_level(), s.screen.view.center()), (3, Hex::new(51, 48)));
    script::run(&mut s, "2*9").unwrap();
    assert_eq!(s.screen.view.zoom_level(), 0);
    // Esc does nothing, and an empty script is fine.
    script::run(&mut s, "esc").unwrap();
    script::run(&mut s, "").unwrap();
    script::run(&mut s, " , ").unwrap();
    assert_eq!(s.screen.game.map.cursor, Hex::new(51, 48));
}

#[test]
fn the_plans_flow_as_a_script() {
    let mut s = session();
    let &[cav] = s.screen.game.map.units_at(Hex::new(50, 50)) else { panic!() };
    script::run(&mut s, "enter,right*3,enter").unwrap();
    assert_eq!(s.screen.game.map.units_at(Hex::new(50, 53)), [cav]);
    assert_eq!(s.screen.game.map.units().get(cav).unwrap().movement, 14);
    assert!(s.screen.game.selected().is_none());
}

#[test]
fn clicks_and_hovers_aim_at_hexes() {
    let mut s = session();
    let &[cav] = s.screen.game.map.units_at(Hex::new(50, 50)) else { panic!() };
    script::run(&mut s, "hover:47:55").unwrap();
    assert_eq!(s.screen.game.map.cursor, Hex::new(47, 55));
    assert!(s.screen.game.selected().is_none());
    script::run(&mut s, "click:50:50").unwrap();
    assert_eq!(s.screen.game.selected(), Some(cav));
    script::run(&mut s, "click:52:48").unwrap();
    assert_eq!(s.screen.game.map.units_at(Hex::new(52, 48)), [cav]);
}

#[test]
fn cursor_jumps_and_centers_the_view() {
    let mut s = session();
    script::run(&mut s, "cursor:50:30").unwrap();
    assert_eq!((s.screen.game.map.cursor, s.screen.view.center()), (Hex::new(50, 30), Hex::new(50, 30)));
    // In a corner the view goes as far as the map's edges let it.
    script::run(&mut s, "cursor:0:0").unwrap();
    assert_eq!(s.screen.game.map.cursor, Hex::new(0, 0));
    assert!(s.screen.view.shows(Hex::new(0, 0)) && s.screen.view.center() != Hex::new(0, 0));
    script::run(&mut s, "cursor:99:99,left").unwrap();
    assert_eq!(s.screen.game.map.cursor, Hex::new(99, 98));
    assert!(s.screen.view.shows(Hex::new(99, 99)));
}

#[test]
fn hex_pixels_are_inside_the_map_or_none() {
    let s = session();
    let r = s.screen.map_rect();
    let p = s.screen.hex_pixel(Hex::new(50, 50)).unwrap();
    assert!((p.x - r.w / 2).abs() <= 1 && (p.y - r.h / 2).abs() <= 1, "{p:?}");
    assert!(r.contains(s.screen.hex_pixel(Hex::new(45, 41)).unwrap()));
    // Out of view.
    assert_eq!(s.screen.hex_pixel(Hex::new(50, 80)), None);
    assert_eq!(s.screen.hex_pixel(Hex::new(10, 50)), None);
}

#[test]
fn characters_drags_and_buttons() {
    let mut s = session();
    // `key:` types any character, with modifiers if asked: Ctrl+Q quits.
    script::run(&mut s, "key:1,key:2,key:x,key:+,space").unwrap();
    assert_eq!(s.screen.view.zoom_level(), 1);
    assert!(!s.screen.quit_requested());
    script::run(&mut s, "key:ctrl+q").unwrap();
    assert!(s.screen.quit_requested());
    s.screen.cancel_quit();
    script::run(&mut s, "key:shift+cmd+Q").unwrap();
    assert!(s.screen.quit_requested());

    // A drag: the button goes down on the first hex and comes up on the
    // second, where the cursor then is. (To the game that is a click on the
    // second hex, and there is nothing on it to select.)
    script::run(&mut s, "drag:50:48:50:53").unwrap();
    assert_eq!(s.screen.game.map.cursor, Hex::new(50, 53));

    // `press:` clicks a widget by its id.
    assert_eq!(s.screen.game.turn(), 1);
    script::run(&mut s, "press:end_turn").unwrap();
    assert_eq!(s.screen.game.turn(), 2, "the button ends the turn");
    assert_eq!(s.screen.game.map.cursor, Hex::new(50, 53));
    // A count goes with the last `*` only, so that `*` itself can be typed.
    script::run(&mut s, "key:**2").unwrap();
}

#[test]
fn bad_scripts_say_what_is_wrong() {
    let mut s = session();
    for (script, want) in [
        ("jump", "unknown step"),
        ("left*x", "not a count"),
        ("click:5", "needs a column"),
        ("click:a:5", "not a row"),
        ("click:50:80", "not in view"),
        ("cursor:100:0", "not on the map"),
        ("cursor:1:2:3", "too many parts"),
        ("left:1", "too many parts"),
        ("drag:50:50:50", "needs a column"),
        ("drag:50:50:50:99", "not in view"),
        ("key:", "is not one character"),
        ("key:ab", "is not one character"),
        ("key:meta+a", "is not a modifier"),
        ("press:", "needs a widget id"),
        ("press:no_such_widget", "no widget with id no_such_widget"),
    ] {
        let err = script::run(&mut s, script).unwrap_err();
        assert!(err.contains(want) && err.starts_with(script), "{script}: {err}");
    }
    // The steps before the bad one have happened.
    let before = s.screen.game.map.cursor;
    assert!(script::run(&mut s, "left,nonsense,left").is_err());
    assert_eq!(s.screen.game.map.cursor, Hex::new(before.row, before.col - 1));
}
