//! Milestone 7: input through the UI changes the game and the view. No window
//! and no GPU: a `Session` on the real assets, fed what the window would send.

mod common;

use common::{click, hover, pixel_of, press, session, session_at, type_char};
use hexgame::state::Hex;
use turbo_ferret::{Point, SpecialKey};

const START: Hex = Hex::new(50, 50);

#[test]
fn the_view_starts_on_the_cursor() {
    let s = session();
    assert_eq!(s.screen.game.map.cursor, START);
    assert_eq!(s.screen.view.center(), START);
    assert_eq!(s.screen.view.zoom_level(), 1);
    assert!(s.screen.game.selected().is_none());
}

#[test]
fn the_hex_under_the_pointer_becomes_the_cursor() {
    let mut s = session();
    for hex in [Hex::new(48, 52), Hex::new(53, 45), Hex::new(50, 50), Hex::new(47, 57)] {
        let p = pixel_of(&s, hex);
        hover(&mut s, p);
        assert_eq!(s.screen.game.map.cursor, hex);
    }
    // The view does not follow the pointer, and hovering selects nothing.
    assert_eq!(s.screen.view.center(), START);
    assert!(s.screen.game.selected().is_none());
    // Outside the map widget the cursor stays.
    hover(&mut s, Point::new(900, 600));
    assert_eq!(s.screen.game.map.cursor, Hex::new(47, 57));
}

#[test]
fn hovering_works_in_framebuffer_pixels() {
    let mut s = session_at(2048, 1536, 192);
    let hex = Hex::new(52, 47);
    let p = pixel_of(&s, hex);
    hover(&mut s, p);
    assert_eq!(s.screen.game.map.cursor, hex);
}

#[test]
fn click_to_select_click_to_move() {
    let mut s = session();
    let &[cav] = s.screen.game.map.units_at(START) else { panic!("the demo unit") };
    let p = pixel_of(&s, START);
    click(&mut s, p);
    assert_eq!(s.screen.game.selected(), Some(cav));
    assert_eq!(s.screen.game.map.cursor, START);
    // The range follows the selected unit.
    assert!(s.screen.game.can_move_to(Hex::new(50, 53)));
    assert!(!s.screen.game.can_move_to(Hex::new(51, 51)));

    let target = Hex::new(50, 53);
    let p = pixel_of(&s, target);
    click(&mut s, p);
    assert_eq!(s.screen.game.map.cursor, target);
    assert_eq!(s.screen.game.map.units_at(target), [cav]);
    assert_eq!(s.screen.game.map.units().get(cav).unwrap().movement, 14);
    assert!(s.screen.game.selected().is_none(), "a move deselects");
    assert!(!s.screen.game.can_move_to(Hex::new(50, 54)));
}

#[test]
fn clicking_the_selected_unit_again_deselects_it() {
    let mut s = session();
    let p = pixel_of(&s, START);
    click(&mut s, p);
    assert!(s.screen.game.selected().is_some());
    click(&mut s, p);
    assert!(s.screen.game.selected().is_none());
}

#[test]
fn clicking_out_of_range_changes_nothing() {
    let mut s = session();
    let &[cav] = s.screen.game.map.units_at(START) else { panic!() };
    // Zoom out twice: at the start zoom everything in view is within reach.
    type_char(&mut s, '1');
    type_char(&mut s, '1');
    let p = pixel_of(&s, START);
    click(&mut s, p);
    assert_eq!(s.screen.game.selected(), Some(cav));
    // A mountain next door, and grass too far away (the C++ teleported to both).
    for hex in [Hex::new(51, 51), Hex::new(50, 68), Hex::new(39, 50)] {
        let p = pixel_of(&s, hex);
        click(&mut s, p);
        assert_eq!(s.screen.game.map.cursor, hex, "the click still moves the cursor");
        assert_eq!(s.screen.game.selected(), Some(cav), "{hex}");
        assert_eq!(s.screen.game.map.units_at(START), [cav]);
        assert_eq!(s.screen.game.map.units().get(cav).unwrap().movement, 20);
    }
    // The edge of the range is still a move: ten grass steps up.
    let p = pixel_of(&s, Hex::new(40, 50));
    click(&mut s, p);
    assert_eq!(s.screen.game.map.units_at(Hex::new(40, 50)), [cav]);
    assert_eq!(s.screen.game.map.units().get(cav).unwrap().movement, 0);
}

/// The plan's flow, by keyboard: select the unit, go three hexes east, move.
#[test]
fn enter_selects_and_moves_at_the_cursor() {
    let mut s = session();
    let &[cav] = s.screen.game.map.units_at(START) else { panic!() };
    assert!(press(&mut s, SpecialKey::Enter));
    assert_eq!(s.screen.game.selected(), Some(cav));
    for _ in 0..3 {
        press(&mut s, SpecialKey::Right);
    }
    assert_eq!(s.screen.game.map.cursor, Hex::new(50, 53));
    assert!(s.screen.game.can_move_to(s.screen.game.map.cursor));
    press(&mut s, SpecialKey::Enter);
    assert_eq!(s.screen.game.map.units_at(Hex::new(50, 53)), [cav]);
    assert_eq!(s.screen.game.map.units().get(cav).unwrap().movement, 14);
    assert!(s.screen.game.selected().is_none());
    // Enter again selects it where it stands now, and once more deselects.
    press(&mut s, SpecialKey::Enter);
    assert_eq!(s.screen.game.selected(), Some(cav));
    press(&mut s, SpecialKey::Enter);
    assert!(s.screen.game.selected().is_none());
}

#[test]
fn the_arrows_move_the_cursor() {
    let mut s = session();
    press(&mut s, SpecialKey::Left);
    assert_eq!(s.screen.game.map.cursor, Hex::new(50, 49));
    press(&mut s, SpecialKey::Up);
    assert_eq!(s.screen.game.map.cursor, Hex::new(49, 49));
    press(&mut s, SpecialKey::Right);
    press(&mut s, SpecialKey::Right);
    assert_eq!(s.screen.game.map.cursor, Hex::new(49, 51));
    press(&mut s, SpecialKey::Down);
    assert_eq!(s.screen.game.map.cursor, Hex::new(50, 51));
    // Small steps stay in view: the view has not moved.
    assert_eq!(s.screen.view.center(), START);
}

#[test]
fn the_view_follows_the_cursor_when_it_leaves() {
    let mut s = session();
    // 19 columns count as shown: nine steps left stay in view, the tenth leaves.
    for _ in 0..9 {
        press(&mut s, SpecialKey::Left);
    }
    assert_eq!((s.screen.game.map.cursor, s.screen.view.center()), (Hex::new(50, 41), START));
    press(&mut s, SpecialKey::Left);
    assert_eq!((s.screen.game.map.cursor, s.screen.view.center()), (Hex::new(50, 40), Hex::new(50, 40)));
    // 11 rows count as shown: five steps down stay, the sixth leaves. Only the
    // row of the center changes.
    for _ in 0..5 {
        press(&mut s, SpecialKey::Down);
    }
    assert_eq!(s.screen.view.center(), Hex::new(50, 40));
    press(&mut s, SpecialKey::Down);
    assert_eq!((s.screen.game.map.cursor, s.screen.view.center()), (Hex::new(56, 40), Hex::new(56, 40)));
}

#[test]
fn the_cursor_stops_at_the_map_edge_and_the_view_does_not_jump() {
    let mut s = session();
    for _ in 0..120 {
        press(&mut s, SpecialKey::Left);
        press(&mut s, SpecialKey::Up);
    }
    assert_eq!(s.screen.game.map.cursor, Hex::new(0, 0));
    // The view came along in steps and now rests near the corner. More presses
    // against the edge change nothing (the C++ recentered on every one).
    let center = s.screen.view.center();
    assert!(center.row <= 5 && center.col <= 9, "{center}");
    for _ in 0..5 {
        press(&mut s, SpecialKey::Left);
        press(&mut s, SpecialKey::Up);
    }
    assert_eq!((s.screen.game.map.cursor, s.screen.view.center()), (Hex::new(0, 0), center));
    for _ in 0..250 {
        press(&mut s, SpecialKey::Right);
        press(&mut s, SpecialKey::Down);
    }
    assert_eq!(s.screen.game.map.cursor, Hex::new(99, 99));
    assert!(s.screen.view.center().row >= 94 && s.screen.view.center().col >= 90, "{}", s.screen.view.center());
}

#[test]
fn one_and_two_zoom_and_center_on_the_cursor() {
    let mut s = session();
    let hex = Hex::new(47, 55);
    let p = pixel_of(&s, hex);
    hover(&mut s, p);
    assert_eq!(s.screen.view.center(), START);

    assert!(type_char(&mut s, '1'));
    assert_eq!((s.screen.view.zoom_level(), s.screen.view.center()), (2, hex));
    for _ in 0..5 {
        type_char(&mut s, '1');
    }
    assert_eq!(s.screen.view.zoom_level(), 4, "the widest level");
    // At the last level the key does nothing at all: with the cursor somewhere
    // else, the view does not jump to it.
    let other = Hex::new(40, 62);
    let p = pixel_of(&s, other);
    hover(&mut s, p);
    assert!(type_char(&mut s, '1'));
    assert_eq!((s.screen.view.zoom_level(), s.screen.view.center()), (4, hex));
    let p = pixel_of(&s, hex);
    hover(&mut s, p);
    for _ in 0..10 {
        type_char(&mut s, '2');
    }
    assert_eq!((s.screen.view.zoom_level(), s.screen.view.center()), (0, hex), "the closest level");
    // Zooming changes neither the cursor nor the selection.
    assert_eq!(s.screen.game.map.cursor, hex);
    assert!(s.screen.game.selected().is_none());
}

#[test]
fn picking_follows_the_zoom_and_the_center() {
    let mut s = session();
    type_char(&mut s, '2');
    press(&mut s, SpecialKey::Left);
    type_char(&mut s, '1');
    type_char(&mut s, '1');
    assert_eq!((s.screen.view.zoom_level(), s.screen.view.center()), (2, Hex::new(50, 49)));
    for hex in [Hex::new(44, 40), Hex::new(57, 58), Hex::new(50, 49)] {
        let p = pixel_of(&s, hex);
        hover(&mut s, p);
        assert_eq!(s.screen.game.map.cursor, hex);
    }
}

#[test]
fn a_press_dragged_off_the_map_leaves_the_cursor_on_it() {
    use turbo_ferret::Modifiers;
    let mut s = session();
    let on_map = pixel_of(&s, Hex::new(48, 52));
    s.screen.ui.pointer_move(on_map, Modifiers::NONE);
    s.screen.ui.pointer_down(on_map, 1, Modifiers::NONE);
    s.pump();
    assert_eq!(s.screen.game.map.cursor, Hex::new(48, 52));
    // The map widget holds the pointer while the button is down, wherever it
    // goes: below the map, and off to the right over the minimap.
    for p in [Point::new(400, 700), Point::new(950, 100), Point::new(-20, 300)] {
        s.screen.ui.pointer_move(p, Modifiers::NONE);
        s.pump();
        assert_eq!(s.screen.game.map.cursor, Hex::new(48, 52), "after a drag to {p:?}");
    }
    // Released out there, it is no click: nothing was selected or moved.
    s.screen.ui.pointer_up(Point::new(400, 700), Modifiers::NONE);
    s.pump();
    assert!(s.screen.game.selected().is_none());
    assert_eq!(s.screen.view.center(), START);
}

#[test]
fn the_overlay_and_the_minimap_follow_changes_to_the_map() {
    use hexgame::state::TileType;
    let mut s = session();
    assert!(s.screen.take_minimap_dirty());
    press(&mut s, SpecialKey::Enter);
    let next_door = Hex::new(50, 51);
    assert!(s.screen.game.can_move_to(next_door));
    s.frame(16);
    assert!(!s.screen.take_minimap_dirty(), "selecting and moving units is not a terrain change");

    // Something changes the terrain (an editor, say): by the next frame the
    // range is right and the minimap is asked to redraw, with nobody told.
    s.screen.game.map.tile_mut(next_door).unwrap().kind = TileType::Water;
    s.frame(32);
    assert!(!s.screen.game.can_move_to(next_door));
    assert!(s.screen.take_minimap_dirty());
    s.frame(48);
    assert!(!s.screen.take_minimap_dirty());

    // The selected unit dies: it is no longer selected.
    let cav = s.screen.game.selected().unwrap();
    s.screen.game.map.kill_unit(cav);
    s.frame(64);
    assert!(s.screen.game.selected().is_none() && s.screen.game.range().is_none());
    assert!(!s.screen.take_minimap_dirty());
}
