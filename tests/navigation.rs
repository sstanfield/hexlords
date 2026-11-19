//! Moving around the map without the keys: two fingers on a trackpad or the
//! wheel, a zoom at the pointer, and a drag with the right button. No window
//! and no GPU: a `Session` on the real assets, called the way the window calls
//! it.

mod common;

use common::{hover, pixel_of, press, session, widget};
use hexgame::script;
use hexgame::state::Hex;
use turbo_ferret::{Point, Rect, SpecialKey};

fn middle(r: Rect) -> Point {
    Point::new(r.x + r.w / 2, r.y + r.h / 2)
}

/// How far two points of the grid are apart.
fn apart(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).abs().max((a[1] - b[1]).abs())
}

#[test]
fn two_fingers_on_the_map_move_it_with_them() {
    let mut s = session();
    let start = s.screen.view.middle();
    let p = pixel_of(&s, Hex::new(48, 46));
    hover(&mut s, p);
    assert_eq!(s.screen.game.map.cursor, Hex::new(48, 46));

    // A few pixels: less than a hex, and the map moves all the same.
    assert!(s.screen.scroll(p, -7.0, 0.0));
    let moved = s.screen.view.middle();
    assert!(moved[0] > start[0] && moved[0] - start[0] < 0.5, "{start:?} {moved:?}");
    assert_eq!(moved[1], start[1]);
    assert_eq!(s.screen.view.center(), Hex::new(50, 50));

    // The hex that was under the pointer is where the fingers took it.
    let before = pixel_of(&s, Hex::new(48, 46));
    assert!(s.screen.scroll(p, 120.0, -90.0));
    let after = pixel_of(&s, Hex::new(48, 46));
    assert!((after.x - before.x - 120).abs() <= 1 && (after.y - before.y + 90).abs() <= 1, "{before:?} {after:?}");
    // The view went the other way: left and down the map.
    let end = s.screen.view.middle();
    assert!(end[0] < moved[0] && end[1] < moved[1]);
    // The pointer did not move, so another hex is under it, and the cursor is there.
    let cursor = s.screen.game.map.cursor;
    assert_ne!(cursor, Hex::new(48, 46));
    let under = pixel_of(&s, cursor);
    assert!((under.x - p.x).abs() < 30 && (under.y - p.y).abs() < 30, "{cursor} is at {under:?}, not {p:?}");
    // It selects nothing and keeps the zoom.
    assert!(s.screen.game.selected().is_none());
    assert_eq!(s.screen.view.zoom_level(), 1);
}

#[test]
fn the_map_stops_at_its_edges() {
    let mut s = session();
    let p = middle(s.screen.map_rect());
    for _ in 0..40 {
        assert!(s.screen.scroll(p, 500.0, 500.0), "at the edge the wheel is still the map's");
    }
    assert_eq!(s.screen.view.middle(), [0.0, 0.0]);
    assert_eq!(s.screen.game.map.cursor, Hex::new(0, 0));
    for _ in 0..40 {
        s.screen.scroll(p, -500.0, -500.0);
    }
    assert_eq!(s.screen.view.center(), Hex::new(99, 99));
}

#[test]
fn two_fingers_on_the_minimap_move_the_view_the_way_they_do_on_the_map() {
    let mut s = session();
    let p = middle(s.screen.minimap_rect());
    let marker = |s: &hexgame::session::Session| s.screen.mini_view.marker(&s.screen.view);
    let [l0, b0, r0, t0] = marker(&s);
    // The fingers go 20 pixels left and 10 up: on the main map that shows what
    // is to the right and below, and so it does here. The marker goes there,
    // against the fingers, by as many pixels of the minimap.
    assert!(s.screen.scroll(p, -20.0, -10.0));
    let [l1, b1, r1, t1] = marker(&s);
    let (w, h) = s.screen.mini_view.size();
    // A pixel is 2 / w of the minimap's width.
    let (dx, dy) = (40.0 / w as f32, -20.0 / h as f32);
    assert!((l1 - l0 - dx).abs() < 1e-4 && (r1 - r0 - dx).abs() < 1e-4, "{l0} {l1} {dx}");
    assert!((b1 - b0 - dy).abs() < 1e-4 && (t1 - t0 - dy).abs() < 1e-4, "{b0} {b1} {dy}");
    // The view went right and down the map, and the cursor with it.
    let center = s.screen.view.center();
    assert!(center.col > 55 && center.row > 52, "{center}");
    assert_eq!(s.screen.game.map.cursor, center);

    // The same stroke on the main map goes the same way, only not as far.
    let mut on_map = session();
    let (start, p) = (on_map.screen.view.middle(), middle(on_map.screen.map_rect()));
    assert!(on_map.screen.scroll(p, -20.0, -10.0));
    let (near, far) = (on_map.screen.view.middle(), s.screen.view.middle());
    assert!(near[0] > start[0] && far[0] > near[0], "{start:?} {near:?} {far:?}");
    assert!(near[1] < start[1] && far[1] < near[1], "{start:?} {near:?} {far:?}");
}

#[test]
fn the_wheel_is_the_uis_everywhere_else() {
    let mut s = session();
    let view = s.screen.view.clone();
    let button = middle(s.screen.area(widget(&s, "end_turn")));
    assert!(!s.screen.scroll(button, 50.0, 50.0));
    assert!(!s.screen.zoom_at(button, true));
    assert!(!s.screen.zoom_target(button));
    assert!(!s.screen.grab(button));
    // Under the map there is nothing.
    let below = Point::new(100, s.screen.map_rect().y + s.screen.map_rect().h + 40);
    assert!(!s.screen.scroll(below, 50.0, 50.0));
    assert_eq!(s.screen.view, view);
}

#[test]
fn a_zoom_at_the_pointer_keeps_the_hex_under_it() {
    let mut s = session();
    let hex = Hex::new(47, 57);
    let p = pixel_of(&s, hex);
    assert!(s.screen.zoom_target(p));
    let r = s.screen.map_rect();
    let grid = |s: &hexgame::session::Session| {
        let (half_w, half_h) = (r.w as f32 / 2.0, r.h as f32 / 2.0);
        s.screen
            .view
            .grid_at(((p.x - r.x) as f32 + 0.5 - half_w) / half_w, -((p.y - r.y) as f32 + 0.5 - half_h) / half_h)
    };
    let under = grid(&s);
    assert!(s.screen.zoom_at(p, true));
    assert_eq!(s.screen.view.zoom_level(), 0);
    assert!(apart(grid(&s), under) < 1e-2);
    assert_eq!(s.screen.game.map.cursor, hex);
    // The keys' zoom would have put the cursor's hex in the middle instead.
    assert_ne!(s.screen.view.center(), hex);
    for level in 1..5 {
        assert!(s.screen.zoom_at(p, false));
        assert_eq!(s.screen.view.zoom_level(), level);
        assert!(apart(grid(&s), under) < 1e-2, "level {level}");
        assert_eq!(s.screen.game.map.cursor, hex);
    }
    // At the last level the turn is still the map's, and nothing moves.
    let view = s.screen.view.clone();
    assert!(s.screen.zoom_at(p, false));
    assert_eq!(s.screen.view, view);

    // On the minimap: about the middle of the view.
    let mini = middle(s.screen.minimap_rect());
    let center = s.screen.view.middle();
    assert!(s.screen.zoom_at(mini, true));
    assert_eq!(s.screen.view.zoom_level(), 3);
    assert_eq!(s.screen.view.middle(), center);
}

#[test]
fn a_held_map_goes_with_the_pointer() {
    let mut s = session();
    let hex = Hex::new(50, 50);
    let from = pixel_of(&s, hex);
    assert!(!s.screen.drag(from), "nothing is held yet");
    assert!(s.screen.grab(from));
    for to in [Point::new(from.x + 33, from.y - 21), Point::new(from.x - 150, from.y + 80)] {
        assert!(s.screen.drag(to));
        let now = pixel_of(&s, hex);
        assert!((now.x - to.x).abs() <= 1 && (now.y - to.y).abs() <= 1, "{hex} is at {now:?}, not {to:?}");
    }
    // The pointer may leave the map while it holds it.
    assert!(s.screen.drag(Point::new(-40, from.y)));
    assert!(s.screen.let_go());
    assert!(!s.screen.let_go());
    let view = s.screen.view.clone();
    assert!(!s.screen.drag(from));
    assert_eq!(s.screen.view, view);
    // The minimap has its own drag, with the left button.
    assert!(!s.screen.grab(middle(s.screen.minimap_rect())));
}

#[test]
fn an_arrow_brings_the_view_back_to_a_cursor_it_left() {
    let mut s = session();
    // Pull the map far to the left with the right button: the cursor stays on
    // its hex, which is out of view now.
    let p = middle(s.screen.map_rect());
    hover(&mut s, p);
    let cursor = s.screen.game.map.cursor;
    assert!(s.screen.grab(p));
    s.screen.drag(Point::new(p.x - 3000, p.y));
    s.screen.let_go();
    assert!(!s.screen.view.shows(cursor));
    assert_eq!(s.screen.game.map.cursor, cursor);

    press(&mut s, SpecialKey::Left);
    let stepped = Hex::new(cursor.row, cursor.col - 1);
    assert_eq!(s.screen.game.map.cursor, stepped);
    assert_eq!(s.screen.view.center(), stepped, "the view came back");
    // And from then on it follows as it always did.
    press(&mut s, SpecialKey::Left);
    assert_eq!(s.screen.view.center(), stepped);
}

#[test]
fn a_scroll_does_not_click() {
    let mut s = session();
    let unit = s.screen.game.map.cursor;
    let p = pixel_of(&s, unit);
    s.screen.scroll(p, 40.0, 0.0);
    s.screen.zoom_at(p, false);
    s.screen.grab(p);
    s.screen.drag(Point::new(p.x + 10, p.y));
    s.screen.let_go();
    s.pump();
    assert!(s.screen.game.selected().is_none());
}

#[test]
fn the_scripts_steps() {
    let mut s = session();
    let start = s.screen.view.middle();
    script::run(&mut s, "scroll:-200:0").unwrap();
    let right = s.screen.view.middle();
    assert!(right[0] > start[0] + 3.0 && right[1] == start[1], "{start:?} {right:?}");
    script::run(&mut s, "pull:0:-100*2").unwrap();
    let down = s.screen.view.middle();
    assert!(down[1] < right[1] - 6.0 && down[0] == right[0]);

    script::run(&mut s, "cursor:50:50,miniscroll:-10:0").unwrap();
    assert!(s.screen.view.center().col > 52);

    script::run(&mut s, "cursor:50:50,zoomin:48:53").unwrap();
    assert_eq!(s.screen.view.zoom_level(), 0);
    assert_eq!(s.screen.game.map.cursor, Hex::new(48, 53));
    script::run(&mut s, "zoomout:48:53*3").unwrap();
    assert_eq!(s.screen.view.zoom_level(), 3);

    for (bad, why) in [
        ("scroll:5", "needs DX:DY"),
        ("scroll:a:b", "not a number of pixels"),
        ("pull:1:2:3", "too many parts"),
        ("zoomin:0:0", "not in view"),
        ("zoomout:200:0", "not on the map"),
    ] {
        let e = script::run(&mut s, bad).unwrap_err();
        assert!(e.contains(why), "{bad}: {e}");
    }
}
