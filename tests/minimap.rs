//! The minimap moves the main map: a click goes to a place, a drag scrolls. No
//! window and no GPU: a `Session` on the real assets.

mod common;

use common::{click, hover, press, session, session_at};
use hexgame::gui::{MiniMapCommand, MiniMapWidget};
use hexgame::session::Session;
use hexgame::state::Hex;
use turbo_ferret::{Modifiers, Point, Size, SpecialKey};

/// How far two hexes are apart, in rows and columns.
fn apart(a: Hex, b: Hex) -> (u16, u16) {
    (a.row.abs_diff(b.row), a.col.abs_diff(b.col))
}

/// On the demo's minimap a hex is 2 pixels wide, so a pixel names a hex to
/// within one.
fn near(a: Hex, b: Hex) -> bool {
    let (rows, cols) = apart(a, b);
    rows <= 1 && cols <= 1
}

fn minimap_commands(s: &mut Session) -> Vec<MiniMapCommand> {
    let id = s.screen.minimap_widget();
    s.screen.ui.with_widget::<MiniMapWidget, _>(id, |w, _| w.take_commands()).expect("the minimap widget")
}

#[test]
fn a_click_on_the_minimap_goes_there() {
    let mut s = session();
    for target in [Hex::new(20, 80), Hex::new(75, 10), Hex::new(50, 50), Hex::new(5, 5)] {
        let p = s.screen.minimap_pixel(target);
        click(&mut s, p);
        // The cursor goes there, so the keys carry on from there, and the
        // view shows it: in its middle, as far as the map's edges let it.
        let cursor = s.screen.game.map.cursor;
        assert!(near(cursor, target), "the cursor is at {cursor}, not {target}");
        assert!(s.screen.view.shows(cursor), "{target}");
        if target == Hex::new(50, 50) {
            assert_eq!(s.screen.view.center(), cursor);
        }
    }
    // It selects nothing and keeps the zoom.
    assert!(s.screen.game.selected().is_none());
    assert_eq!(s.screen.view.zoom_level(), 1);
}

#[test]
fn the_corners_of_the_minimap_are_the_corners_of_the_map() {
    let mut s = session();
    let r = s.screen.minimap_rect();
    for (p, corner) in [
        (Point::new(r.x, r.y), Hex::new(0, 0)),
        (Point::new(r.right() - 1, r.bottom() - 1), Hex::new(99, 99)),
        (Point::new(r.right() - 1, r.y), Hex::new(0, 99)),
    ] {
        click(&mut s, p);
        assert_eq!(s.screen.game.map.cursor, corner);
        assert!(s.screen.view.shows(corner), "{corner}");
    }
}

#[test]
fn a_drag_on_the_minimap_scrolls_the_map() {
    let mut s = session();
    let from = s.screen.minimap_pixel(Hex::new(30, 30));
    s.screen.ui.pointer_move(from, Modifiers::NONE);
    s.screen.ui.pointer_down(from, 1, Modifiers::NONE);
    s.pump();
    assert!(near(s.screen.view.center(), Hex::new(30, 30)), "the press already goes there: {}", s.screen.view.center());

    // Each move with the button held scrolls on.
    for target in [Hex::new(32, 40), Hex::new(45, 55), Hex::new(70, 60)] {
        let p = s.screen.minimap_pixel(target);
        s.screen.ui.pointer_move(p, Modifiers::NONE);
        s.pump();
        assert!(near(s.screen.view.center(), target), "{} is not near {target}", s.screen.view.center());
        assert_eq!(s.screen.game.map.cursor, s.screen.view.center());
    }

    // The drag goes on outside the minimap, over the main map even: the view
    // stops at the map's edge, and the main map does not take the pointer.
    let r = s.screen.minimap_rect();
    s.screen.ui.pointer_move(Point::new(r.x - 300, r.y + r.h / 2), Modifiers::NONE);
    s.pump();
    assert_eq!(s.screen.game.map.cursor.col, 0);
    assert!(s.screen.view.shows(s.screen.game.map.cursor));
    s.screen.ui.pointer_move(Point::new(r.x + r.w / 2, r.bottom() + 400), Modifiers::NONE);
    s.pump();
    assert_eq!(s.screen.game.map.cursor.row, 99);
    assert!(s.screen.view.shows(s.screen.game.map.cursor));

    // After the release, moves over the minimap do nothing.
    let last = s.screen.minimap_pixel(Hex::new(70, 60));
    s.screen.ui.pointer_move(last, Modifiers::NONE);
    s.screen.ui.pointer_up(last, Modifiers::NONE);
    s.pump();
    let rest = (s.screen.view.center(), s.screen.game.map.cursor);
    let p = s.screen.minimap_pixel(Hex::new(10, 10));
    hover(&mut s, p);
    let p = s.screen.minimap_pixel(Hex::new(90, 90));
    hover(&mut s, p);
    assert_eq!((s.screen.view.center(), s.screen.game.map.cursor), rest);
}

#[test]
fn hovering_over_the_minimap_does_nothing() {
    let mut s = session();
    let before = (s.screen.view.clone(), s.screen.game.map.cursor);
    let p = s.screen.minimap_pixel(Hex::new(10, 90));
    hover(&mut s, p);
    assert_eq!(minimap_commands(&mut s), []);
    assert_eq!((s.screen.view.clone(), s.screen.game.map.cursor), before);
}

#[test]
fn the_widget_records_where_the_player_pointed() {
    let mut s = session();
    let r = s.screen.minimap_rect();
    let size = Size::new(r.w, r.h);
    let p = Point::new(r.x + 40, r.y + 60);
    s.screen.ui.pointer_move(p, Modifiers::NONE);
    assert!(s.screen.ui.pointer_down(p, 1, Modifiers::NONE));
    assert_eq!(minimap_commands(&mut s), [MiniMapCommand::Goto(Point::new(40, 60), size)]);
    s.screen.ui.pointer_move(Point::new(r.x + 41, r.y + 65), Modifiers::NONE);
    // Outside the widget the point is still relative to it.
    s.screen.ui.pointer_move(Point::new(r.x - 10, r.y + 300), Modifiers::NONE);
    assert_eq!(
        minimap_commands(&mut s),
        [MiniMapCommand::Goto(Point::new(41, 65), size), MiniMapCommand::Goto(Point::new(-10, 300), size)]
    );
    s.screen.ui.pointer_up(Point::new(r.x - 10, r.y + 300), Modifiers::NONE);
    assert_eq!(minimap_commands(&mut s), [], "the release records nothing");
}

#[test]
fn a_press_that_is_cancelled_ends_the_drag() {
    let mut s = session();
    let p = s.screen.minimap_pixel(Hex::new(30, 30));
    s.screen.ui.pointer_move(p, Modifiers::NONE);
    s.screen.ui.pointer_down(p, 1, Modifiers::NONE);
    // The window lost the focus with the button down: the release never comes.
    s.screen.ui.pointer_cancel();
    s.pump();
    let rest = s.screen.view.center();
    let p = s.screen.minimap_pixel(Hex::new(80, 80));
    hover(&mut s, p);
    assert_eq!(s.screen.view.center(), rest);
}

#[test]
fn the_keys_carry_on_from_where_the_minimap_went() {
    let mut s = session();
    let p = s.screen.minimap_pixel(Hex::new(20, 80));
    click(&mut s, p);
    let at = s.screen.game.map.cursor;
    assert!(press(&mut s, SpecialKey::Right));
    assert!(press(&mut s, SpecialKey::Down));
    assert_eq!(s.screen.game.map.cursor, Hex::new(at.row + 1, at.col + 1));
    assert_eq!(s.screen.view.center(), at, "a small step stays in view");
    // And the main map picks hexes around the new center.
    let target = Hex::new(at.row - 2, at.col + 3);
    let p = s.screen.hex_pixel(target).expect("in view");
    hover(&mut s, p);
    assert_eq!(s.screen.game.map.cursor, target);
}

#[test]
fn a_selected_unit_stays_selected_while_the_minimap_scrolls() {
    let mut s = session();
    let &[cav] = s.screen.game.map.units_at(Hex::new(50, 50)) else { panic!() };
    press(&mut s, SpecialKey::Enter);
    assert_eq!(s.screen.game.selected(), Some(cav));
    let p = s.screen.minimap_pixel(Hex::new(10, 10));
    click(&mut s, p);
    assert_eq!(s.screen.game.selected(), Some(cav));
    assert_eq!(s.screen.game.map.units_at(Hex::new(50, 50)), [cav], "going somewhere moves no unit");
    assert_eq!(s.screen.game.map.units().get(cav).unwrap().movement, 20);
}

#[test]
fn the_minimap_works_in_framebuffer_pixels() {
    let mut s = session_at(2048, 1536, 192);
    let target = Hex::new(70, 25);
    let p = s.screen.minimap_pixel(target);
    click(&mut s, p);
    assert!(near(s.screen.view.center(), target), "{}", s.screen.view.center());
}

#[test]
fn the_marker_follows_the_minimap_click() {
    let mut s = session();
    let [l0, b0, r0, t0] = s.screen.mini_view.marker(&s.screen.view);
    let p = s.screen.minimap_pixel(Hex::new(20, 80));
    click(&mut s, p);
    let [l1, b1, r1, t1] = s.screen.mini_view.marker(&s.screen.view);
    // Up and to the right, the same size.
    assert!(l1 > l0 && r1 > r0 && b1 > b0 && t1 > t0);
    assert!(((r1 - l1) - (r0 - l0)).abs() < 1e-5);
    // The click was inside what the marker now covers.
    let [x, y] = s.screen.mini_view.to_ndc(s.screen.mini_view.hex_center(Hex::new(20, 80)));
    assert!(l1 < x && x < r1 && b1 < y && y < t1);
}
