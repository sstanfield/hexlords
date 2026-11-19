//! Milestone 3: the layout, the game's widgets and what they record. No window
//! and no GPU: a `Session` on the real assets.

mod common;

use common::{assets, click, hover, pixel_of, press, session, session_at, widget};
use hexgame::game::Game;
use hexgame::gui::{MapCommand, MapKey, MapWidget, MiniMapWidget, RootCommand, RootWindow};
use hexgame::session::Session;
use hexgame::state::{Hex, Map, TileType};
use turbo_ferret::{Modifiers, Point, Rect, Size, SpecialKey, tbid};

fn map_commands(s: &mut Session) -> Vec<MapCommand> {
    let map = s.screen.map_widget();
    s.screen.ui.with_widget::<MapWidget, _>(map, |w, _| w.take_commands()).expect("the map widget")
}

/// The widget's rectangle in window pixels.
fn window_rect(s: &Session, name: &str) -> Rect {
    let id = widget(s, name);
    Rect::from_origin_size(s.screen.ui.to_root(id, Point::ZERO), s.screen.ui.rect(id).size())
}

#[test]
fn the_layout_at_1024_by_768() {
    let s = session();
    assert_eq!(window_rect(&s, "RootWindow"), Rect::new(0, 0, 1024, 768));
    assert_eq!(window_rect(&s, "main_layout"), Rect::new(0, 0, 1024, 768));
    // The map: 80% of the width, 70% of the height, in the top left corner.
    assert_eq!(window_rect(&s, "main_map"), Rect::new(0, 0, 819, 537));
    assert_eq!(s.screen.map_rect(), Rect::new(0, 0, 819, 537));
    // The sidebar is right of it, after the spacing, and the minimap tops it.
    let sidebar = window_rect(&s, "main_sidebar");
    assert_eq!((sidebar.x, sidebar.y, sidebar.w), (824, 0, 200));
    assert_eq!(window_rect(&s, "mini_map"), Rect::new(824, 0, 200, 231));
    assert_eq!(s.screen.minimap_rect(), Rect::new(824, 0, 200, 231));
    // The button is under the minimap, at its own size.
    let button = window_rect(&s, "end_turn");
    assert!(button.y >= 231 + 5 && button.h < 60 && button.w < 100, "{button:?}");
    assert!(sidebar.x <= button.x && button.right() <= sidebar.right(), "{button:?}");
    assert_eq!(s.screen.ui.text(widget(&s, "end_turn")), Some("End turn"));
    widget(&s, "main_layout1");
    widget(&s, "main_layout2");
    // The status line is under the map, as wide as it.
    let status = window_rect(&s, "status");
    assert_eq!((status.x, status.w), (0, 819), "{status:?}");
    assert!(status.y >= 537 + 5 && status.h >= 20, "{status:?}");
    // Nothing reaches outside the window.
    for name in ["main_layout1", "main_sidebar", "mini_map", "end_turn", "main_map", "status"] {
        let r = window_rect(&s, name);
        assert!(r.x >= 0 && r.y >= 0 && r.right() <= 1024 && r.bottom() <= 768, "{name}: {r:?}");
    }
}

#[test]
fn the_widgets_are_the_games_own() {
    let mut s = session();
    assert!(s.screen.ui.get::<MapWidget>(s.screen.map_widget()).is_some());
    assert!(s.screen.ui.get::<MiniMapWidget>(s.screen.minimap_widget()).is_some());
    assert!(s.screen.ui.get::<RootWindow>(s.screen.root_window()).is_some());
    assert_eq!(s.screen.map_widget(), widget(&s, "main_map"));
    assert_eq!(s.screen.minimap_widget(), widget(&s, "mini_map"));
    assert_eq!(s.screen.root_window(), widget(&s, "RootWindow"));
    // The map has the keyboard from the start, and nothing else can take it.
    assert_eq!(s.screen.ui.focused(), Some(s.screen.map_widget()));
    for name in ["mini_map", "end_turn", "RootWindow", "main_layout"] {
        let id = widget(&s, name);
        assert!(!s.screen.ui.set_focus(id), "{name} takes the focus");
    }
}

#[test]
fn the_views_have_the_widgets_sizes() {
    let s = session();
    assert_eq!(s.screen.view.size(), (819, 537));
    assert_eq!(s.screen.mini_view.size(), (200, 231));
}

#[test]
fn the_layout_in_framebuffer_pixels_on_a_hidpi_screen() {
    // A 1024 x 768 window on a 2x screen: everything is in the 2048 x 1536 pixels.
    let s = session_at(2048, 1536, 192);
    assert_eq!(s.screen.map_rect(), Rect::new(0, 0, 1638, 1075));
    // The spacing is 5 dp, which is 10 pixels here.
    assert_eq!(s.screen.minimap_rect(), Rect::new(1648, 0, 400, 461));
    let button = window_rect(&s, "end_turn");
    assert!(button.right() <= 2048 && button.h > 40, "{button:?}");
}

#[test]
fn the_layout_follows_the_window() {
    let mut s = session();
    assert!(s.screen.take_minimap_dirty());
    assert!(!s.screen.take_minimap_dirty());
    s.resize(Size::new(1280, 800));
    s.frame(16);
    assert_eq!(s.screen.map_rect(), Rect::new(0, 0, 1024, 560));
    assert_eq!(s.screen.minimap_rect(), Rect::new(1029, 0, 251, 240));
    assert_eq!(s.screen.view.size(), (1024, 560));
    assert_eq!(s.screen.mini_view.size(), (251, 240));
    // A new size means a new minimap picture.
    assert!(s.screen.take_minimap_dirty());
    // A frame without a change does not.
    s.frame(32);
    assert!(!s.screen.take_minimap_dirty());
}

#[test]
fn odd_window_sizes_do_not_break_the_layout() {
    for (w, h) in [(1, 1), (0, 0), (40, 3000), (5000, 60), (2560, 1080)] {
        let mut s = session_at(w, h, 96);
        s.frame(16);
        let (map, mini) = (s.screen.map_rect(), s.screen.minimap_rect());
        // Nothing has a negative size, and the map is its share of the window.
        assert!(map.w >= 0 && map.h >= 0 && mini.w >= 0 && mini.h >= 0, "{w}x{h}: {map:?} {mini:?}");
        assert_eq!((map.w, map.h), ((w as f32 * 0.8) as i32, (h as f32 * 0.7) as i32), "{w}x{h}");
        assert_eq!(s.screen.view.size(), (map.w, map.h));
        assert_eq!(s.screen.mini_view.size(), (mini.w, mini.h));
        // The views give finite numbers at any size, the center hex is drawn,
        // and input does not panic.
        let (wf, hf) = s.screen.view.factors();
        let (mwf, mhf) = s.screen.mini_view.factors();
        assert!([wf, hf, mwf, mhf].iter().all(|f| f.is_finite() && *f > 0.0), "{w}x{h}");
        assert!(s.screen.view.visible_hexes().any(|v| v.hex == s.screen.view.center()), "{w}x{h}");
        assert!(s.screen.mini_view.marker(&s.screen.view).iter().all(|c| (-1.0..=1.0).contains(c)), "{w}x{h}");
        click(&mut s, Point::new(0, 0));
        click(&mut s, Point::new(w - 1, 0));
        press(&mut s, SpecialKey::Left);
        s.frame(32);
    }
}

#[test]
fn the_map_records_pointer_moves_presses_and_clicks() {
    let mut s = session();
    let size = Size::new(819, 537);
    s.screen.ui.pointer_move(Point::new(100, 50), Modifiers::NONE);
    assert_eq!(map_commands(&mut s), [MapCommand::Hover(Point::new(100, 50), size)]);

    assert!(s.screen.ui.pointer_down(Point::new(100, 50), 1, Modifiers::NONE));
    assert_eq!(map_commands(&mut s), [MapCommand::Press(Point::new(100, 50), size)], "a press alone is not a click");
    s.screen.ui.pointer_move(Point::new(102, 51), Modifiers::NONE);
    assert!(s.screen.ui.pointer_up(Point::new(102, 51), Modifiers::NONE));
    assert_eq!(
        map_commands(&mut s),
        [
            MapCommand::Hover(Point::new(102, 51), size),
            MapCommand::Release,
            MapCommand::Click(Point::new(102, 51), size)
        ]
    );
    // A press on the map released outside it is no click.
    s.screen.ui.pointer_down(Point::new(100, 50), 1, Modifiers::NONE);
    s.screen.ui.pointer_up(Point::new(900, 700), Modifiers::NONE);
    assert_eq!(map_commands(&mut s), [MapCommand::Press(Point::new(100, 50), size), MapCommand::Release]);
    // Moves elsewhere are not the map's.
    s.screen.ui.pointer_move(Point::new(900, 100), Modifiers::NONE);
    s.screen.ui.pointer_move(Point::new(500, 700), Modifiers::NONE);
    assert_eq!(map_commands(&mut s), []);
}

#[test]
fn the_map_records_its_keys_and_consumes_them() {
    let mut s = session();
    let down = |s: &mut Session, key| s.screen.ui.key(None, key, Modifiers::NONE, true);
    for (special, want) in [
        (SpecialKey::Left, MapKey::Left),
        (SpecialKey::Right, MapKey::Right),
        (SpecialKey::Up, MapKey::Up),
        (SpecialKey::Down, MapKey::Down),
        (SpecialKey::Enter, MapKey::Enter),
    ] {
        assert!(down(&mut s, special), "{special:?} down is consumed");
        assert_eq!(map_commands(&mut s), [MapCommand::Key(want)]);
        assert!(s.screen.ui.key(None, special, Modifiers::NONE, false), "{special:?} up is consumed");
        assert_eq!(map_commands(&mut s), [], "a release records nothing");
    }
    // Esc is consumed, down and up, and does nothing.
    assert!(down(&mut s, SpecialKey::Esc));
    assert!(s.screen.ui.key(None, SpecialKey::Esc, Modifiers::NONE, false));
    assert_eq!(map_commands(&mut s), []);

    // 1 and 2 zoom.
    assert!(s.screen.ui.key(Some('1'), SpecialKey::None, Modifiers::NONE, true));
    assert!(s.screen.ui.key(Some('2'), SpecialKey::None, Modifiers::NONE, true));
    assert_eq!(map_commands(&mut s), [MapCommand::Key(MapKey::ZoomOut), MapCommand::Key(MapKey::ZoomIn)]);
    assert!(!s.screen.ui.key(Some('1'), SpecialKey::None, Modifiers::NONE, false), "their releases pass through");

    // Other keys are not the map's.
    assert!(!s.screen.ui.key(Some('x'), SpecialKey::None, Modifiers::NONE, true));
    assert!(!s.screen.ui.key(Some('3'), SpecialKey::None, Modifiers::NONE, true));
    assert!(!down(&mut s, SpecialKey::F5));
    assert!(!down(&mut s, SpecialKey::PageUp));
    assert_eq!(map_commands(&mut s), []);
}

#[test]
fn the_wheel_does_nothing() {
    let mut s = session();
    hover(&mut s, Point::new(400, 300));
    let before = (s.screen.view.clone(), s.screen.game.map.cursor);
    // (`wheel` says whether a widget got the event, not whether one used it.)
    s.screen.ui.wheel(Point::new(400, 300), Point::new(0, 1), Modifiers::NONE);
    s.screen.ui.wheel(Point::new(400, 300), Point::new(0, -3), Modifiers::NONE);
    assert_eq!(map_commands(&mut s), []);
    s.pump();
    assert_eq!((s.screen.view.clone(), s.screen.game.map.cursor), before);
}

#[test]
fn the_keyboard_stays_with_the_map() {
    let mut s = session();
    let button = window_rect(&s, "end_turn");
    // The minimap, the button, the empty sidebar and the strip under the map.
    for p in [Point::new(900, 100), Point::new(button.x + 5, button.y + 5), Point::new(900, 600), Point::new(300, 700)]
    {
        click(&mut s, p);
        assert_eq!(s.screen.ui.focused(), Some(s.screen.map_widget()), "after a click at {p:?}");
        let before = s.screen.game.map.cursor;
        assert!(press(&mut s, SpecialKey::Left), "the arrows still work after a click at {p:?}");
        assert_eq!(s.screen.game.map.cursor.col + 1, before.col);
    }
}

#[test]
fn the_button_does_nothing() {
    let mut s = session();
    let b = window_rect(&s, "end_turn");
    let before = (s.screen.view.clone(), s.screen.game.map.cursor);
    click(&mut s, Point::new(b.x + b.w / 2, b.y + b.h / 2));
    assert_eq!((s.screen.view.clone(), s.screen.game.map.cursor), before);
    assert!(!s.screen.quit_requested() && s.screen.game.selected().is_none());
}

#[test]
fn clicks_on_buttons_and_unused_keys_reach_the_host() {
    let mut s = session();
    let b = window_rect(&s, "end_turn");
    s.screen.ui.pointer_move(Point::new(b.x + b.w / 2, b.y + b.h / 2), Modifiers::NONE);
    s.screen.ui.pointer_down(Point::new(b.x + b.w / 2, b.y + b.h / 2), 1, Modifiers::NONE);
    s.screen.ui.pointer_up(Point::new(b.x + b.w / 2, b.y + b.h / 2), Modifiers::NONE);
    // Keys the map does not use, and one it does (which never gets this far).
    s.screen.ui.key(Some('g'), SpecialKey::None, Modifiers::NONE, true);
    s.screen.ui.key(Some('s'), SpecialKey::None, Modifiers::CTRL, true);
    s.screen.ui.key(None, SpecialKey::Tab, Modifiers::SHIFT, true);
    s.screen.ui.key(None, SpecialKey::Left, Modifiers::NONE, true);
    s.screen.ui.key(Some('q'), SpecialKey::None, Modifiers::CTRL, true);
    // A click on the map is the map's, not a button's.
    let p = pixel_of(&s, Hex::new(50, 52));
    s.screen.ui.pointer_down(p, 1, Modifiers::NONE);
    s.screen.ui.pointer_up(p, Modifiers::NONE);
    assert_eq!(
        s.screen.pump_shared(),
        [
            RootCommand::Click(tbid!("end_turn")),
            RootCommand::Key('g', Modifiers::NONE),
            RootCommand::Key('s', Modifiers::CTRL),
            RootCommand::Special(SpecialKey::Tab, Modifiers::SHIFT),
        ]
    );
    assert!(s.screen.quit_requested(), "quitting is the screen's own business");
    assert_eq!(s.screen.pump_shared(), []);
}

#[test]
fn another_map_starts_the_views_over() {
    let mut s = session();
    s.frame(16);
    assert!(s.screen.take_minimap_dirty());
    let mut map = Map::new(30, 60, TileType::Water).unwrap();
    map.cursor = Hex::new(3, 55);
    s.screen.set_game(Game::new(map));
    assert!(s.screen.take_minimap_dirty(), "the minimap shows the new map");
    assert_eq!(s.screen.view.center(), Hex::new(3, 55));
    assert_eq!(s.screen.view.size(), (819, 537));
    assert_eq!(s.screen.mini_view.size(), (200, 231));
    // Both views know the new map's size: its far corner is on them.
    let p = s.screen.minimap_pixel(Hex::new(29, 59));
    click(&mut s, p);
    assert_eq!(s.screen.game.map.cursor, Hex::new(29, 59));
    assert!(s.screen.hex_pixel(Hex::new(29, 59)).is_some());
}

#[test]
fn ctrl_q_quits_wherever_the_last_click_was() {
    for p in [Point::new(100, 100), Point::new(900, 100), Point::new(900, 600), Point::new(300, 700)] {
        let mut s = session();
        click(&mut s, p);
        // A plain q is nobody's key.
        assert!(!s.screen.ui.key(Some('q'), SpecialKey::None, Modifiers::NONE, true), "{p:?}");
        s.pump();
        assert!(!s.screen.quit_requested(), "{p:?}");
        // The window sends chords as the upper-case key.
        assert!(s.screen.ui.key(Some('Q'), SpecialKey::None, Modifiers::CTRL, true), "{p:?}");
        assert!(!s.screen.quit_requested(), "not before the session looks");
        s.pump();
        assert!(s.screen.quit_requested(), "{p:?}");
    }
}

#[test]
fn cmd_q_quits_too() {
    let mut s = session();
    assert!(s.screen.ui.key(Some('Q'), SpecialKey::None, Modifiers::SUPER, true));
    s.pump();
    assert!(s.screen.quit_requested());
}

#[test]
fn f11_asks_for_fullscreen_and_alt_f_is_consumed() {
    let mut s = session();
    assert!(s.screen.ui.key(None, SpecialKey::F11, Modifiers::NONE, true));
    assert!(!s.screen.ui.key(None, SpecialKey::F11, Modifiers::NONE, false));
    s.pump();
    assert_eq!(s.screen.take_fullscreen_toggles(), 1);
    assert_eq!(s.screen.take_fullscreen_toggles(), 0);
    // Alt+F did nothing in the C++ either (its handler was a stub).
    assert!(s.screen.ui.key(Some('f'), SpecialKey::None, Modifiers::ALT, true));
    assert!(!s.screen.ui.key(Some('f'), SpecialKey::None, Modifiers::NONE, true));
    s.pump();
    assert_eq!(s.screen.take_fullscreen_toggles(), 0);
    assert!(!s.screen.quit_requested());
}

#[test]
fn missing_assets_are_an_error_that_names_the_directory() {
    let nowhere = assets().join("no-such-directory");
    let err = Session::new(&nowhere, Size::new(1024, 768), 96).err().expect("no skin there");
    assert!(err.contains("skin") && err.contains("no-such-directory"), "{err}");
}
