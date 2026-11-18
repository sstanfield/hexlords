//! Milestone 3: the layout, the game's widgets and what they record. No window
//! and no GPU: a `Session` on the real assets.

mod common;

use common::{assets, click, hover, press, session, session_at, widget};
use hexgame::gui::{MapCommand, MapKey, MapWidget, MiniMapWidget, RootWindow};
use hexgame::session::Session;
use turbo_ferret::{Modifiers, Point, Rect, Size, SpecialKey};

fn map_commands(s: &mut Session) -> Vec<MapCommand> {
    let map = s.map_widget();
    s.ui.with_widget::<MapWidget, _>(map, |w, _| w.take_commands()).expect("the map widget")
}

/// The widget's rectangle in window pixels.
fn window_rect(s: &Session, name: &str) -> Rect {
    let id = widget(s, name);
    Rect::from_origin_size(s.ui.to_root(id, Point::ZERO), s.ui.rect(id).size())
}

#[test]
fn the_layout_at_1024_by_768() {
    let s = session();
    assert_eq!(window_rect(&s, "RootWindow"), Rect::new(0, 0, 1024, 768));
    assert_eq!(window_rect(&s, "main_layout"), Rect::new(0, 0, 1024, 768));
    // The map: 80% of the width, 70% of the height, in the top left corner.
    assert_eq!(window_rect(&s, "main_map"), Rect::new(0, 0, 819, 537));
    assert_eq!(s.map_rect(), Rect::new(0, 0, 819, 537));
    // The sidebar is right of it, after the spacing, and the minimap tops it.
    let sidebar = window_rect(&s, "main_sidebar");
    assert_eq!((sidebar.x, sidebar.y, sidebar.w), (824, 0, 200));
    assert_eq!(window_rect(&s, "mini_map"), Rect::new(824, 0, 200, 231));
    assert_eq!(s.minimap_rect(), Rect::new(824, 0, 200, 231));
    // The button is under the minimap, at its own size.
    let button = window_rect(&s, "button");
    assert!(button.y >= 231 + 5 && button.h < 60 && button.w < 100, "{button:?}");
    assert!(sidebar.x <= button.x && button.right() <= sidebar.right(), "{button:?}");
    assert_eq!(s.ui.text(widget(&s, "button")), Some("SLS"));
    widget(&s, "main_layout1");
    widget(&s, "main_layout2");
    // Nothing reaches outside the window.
    for name in ["main_layout1", "main_sidebar", "mini_map", "button", "main_map"] {
        let r = window_rect(&s, name);
        assert!(r.x >= 0 && r.y >= 0 && r.right() <= 1024 && r.bottom() <= 768, "{name}: {r:?}");
    }
}

#[test]
fn the_widgets_are_the_games_own() {
    let mut s = session();
    assert!(s.ui.get::<MapWidget>(s.map_widget()).is_some());
    assert!(s.ui.get::<MiniMapWidget>(s.minimap_widget()).is_some());
    assert!(s.ui.get::<RootWindow>(s.root_window()).is_some());
    assert_eq!(s.map_widget(), widget(&s, "main_map"));
    assert_eq!(s.minimap_widget(), widget(&s, "mini_map"));
    assert_eq!(s.root_window(), widget(&s, "RootWindow"));
    // The map has the keyboard from the start, and nothing else can take it.
    assert_eq!(s.ui.focused(), Some(s.map_widget()));
    for name in ["mini_map", "button", "RootWindow", "main_layout"] {
        let id = widget(&s, name);
        assert!(!s.ui.set_focus(id), "{name} takes the focus");
    }
}

#[test]
fn the_views_have_the_widgets_sizes() {
    let s = session();
    assert_eq!(s.view.size(), (819, 537));
    assert_eq!(s.mini_view.size(), (200, 231));
}

#[test]
fn the_layout_in_framebuffer_pixels_on_a_hidpi_screen() {
    // A 1024 x 768 window on a 2x screen: everything is in the 2048 x 1536 pixels.
    let s = session_at(2048, 1536, 192);
    assert_eq!(s.map_rect(), Rect::new(0, 0, 1638, 1075));
    // The spacing is 5 dp, which is 10 pixels here.
    assert_eq!(s.minimap_rect(), Rect::new(1648, 0, 400, 461));
    let button = window_rect(&s, "button");
    assert!(button.right() <= 2048 && button.h > 40, "{button:?}");
}

#[test]
fn the_layout_follows_the_window() {
    let mut s = session();
    assert!(s.take_minimap_dirty());
    assert!(!s.take_minimap_dirty());
    s.resize(Size::new(1280, 800));
    s.frame(16);
    assert_eq!(s.map_rect(), Rect::new(0, 0, 1024, 560));
    assert_eq!(s.minimap_rect(), Rect::new(1029, 0, 251, 240));
    assert_eq!(s.view.size(), (1024, 560));
    assert_eq!(s.mini_view.size(), (251, 240));
    // A new size means a new minimap picture.
    assert!(s.take_minimap_dirty());
    // A frame without a change does not.
    s.frame(32);
    assert!(!s.take_minimap_dirty());
}

#[test]
fn a_tiny_window_does_not_break_the_layout() {
    for (w, h) in [(1, 1), (0, 0), (40, 3000)] {
        let mut s = session_at(w, h, 96);
        s.frame(16);
        let (map, mini) = (s.map_rect(), s.minimap_rect());
        assert!(map.w >= 0 && map.h >= 0 && mini.w >= 0 && mini.h >= 0, "{w}x{h}: {map:?} {mini:?}");
        // The views cope with any size.
        assert!(s.view.visible_hexes().count() > 0);
        let (wf, hf) = s.mini_view.factors();
        assert!(wf.is_finite() && hf.is_finite());
    }
}

#[test]
fn the_map_records_pointer_moves_and_clicks() {
    let mut s = session();
    let size = Size::new(819, 537);
    s.ui.pointer_move(Point::new(100, 50), Modifiers::NONE);
    assert_eq!(map_commands(&mut s), [MapCommand::Hover(Point::new(100, 50), size)]);

    assert!(s.ui.pointer_down(Point::new(100, 50), 1, Modifiers::NONE));
    assert_eq!(map_commands(&mut s), [], "a press alone is not a click");
    s.ui.pointer_move(Point::new(102, 51), Modifiers::NONE);
    assert!(s.ui.pointer_up(Point::new(102, 51), Modifiers::NONE));
    assert_eq!(
        map_commands(&mut s),
        [MapCommand::Hover(Point::new(102, 51), size), MapCommand::Click(Point::new(102, 51), size)]
    );
    // A press on the map released outside it is no click.
    s.ui.pointer_down(Point::new(100, 50), 1, Modifiers::NONE);
    s.ui.pointer_up(Point::new(900, 700), Modifiers::NONE);
    assert_eq!(map_commands(&mut s), []);
    // Moves elsewhere are not the map's.
    s.ui.pointer_move(Point::new(900, 100), Modifiers::NONE);
    s.ui.pointer_move(Point::new(500, 700), Modifiers::NONE);
    assert_eq!(map_commands(&mut s), []);
}

#[test]
fn the_map_records_its_keys_and_consumes_them() {
    let mut s = session();
    let down = |s: &mut Session, key| s.ui.key(None, key, Modifiers::NONE, true);
    for (special, want) in [
        (SpecialKey::Left, MapKey::Left),
        (SpecialKey::Right, MapKey::Right),
        (SpecialKey::Up, MapKey::Up),
        (SpecialKey::Down, MapKey::Down),
        (SpecialKey::Enter, MapKey::Enter),
    ] {
        assert!(down(&mut s, special), "{special:?} down is consumed");
        assert_eq!(map_commands(&mut s), [MapCommand::Key(want)]);
        assert!(s.ui.key(None, special, Modifiers::NONE, false), "{special:?} up is consumed");
        assert_eq!(map_commands(&mut s), [], "a release records nothing");
    }
    // Esc is consumed, down and up, and does nothing.
    assert!(down(&mut s, SpecialKey::Esc));
    assert!(s.ui.key(None, SpecialKey::Esc, Modifiers::NONE, false));
    assert_eq!(map_commands(&mut s), []);

    // 1 and 2 zoom.
    assert!(s.ui.key(Some('1'), SpecialKey::None, Modifiers::NONE, true));
    assert!(s.ui.key(Some('2'), SpecialKey::None, Modifiers::NONE, true));
    assert_eq!(map_commands(&mut s), [MapCommand::Key(MapKey::ZoomOut), MapCommand::Key(MapKey::ZoomIn)]);
    assert!(!s.ui.key(Some('1'), SpecialKey::None, Modifiers::NONE, false), "their releases pass through");

    // Other keys are not the map's.
    assert!(!s.ui.key(Some('x'), SpecialKey::None, Modifiers::NONE, true));
    assert!(!s.ui.key(Some('3'), SpecialKey::None, Modifiers::NONE, true));
    assert!(!down(&mut s, SpecialKey::F5));
    assert!(!down(&mut s, SpecialKey::PageUp));
    assert_eq!(map_commands(&mut s), []);
}

#[test]
fn the_wheel_does_nothing() {
    let mut s = session();
    hover(&mut s, Point::new(400, 300));
    let before = (s.view.clone(), s.game.map.cursor);
    // (`wheel` says whether a widget got the event, not whether one used it.)
    s.ui.wheel(Point::new(400, 300), Point::new(0, 1), Modifiers::NONE);
    s.ui.wheel(Point::new(400, 300), Point::new(0, -3), Modifiers::NONE);
    assert_eq!(map_commands(&mut s), []);
    s.pump();
    assert_eq!((s.view.clone(), s.game.map.cursor), before);
}

#[test]
fn the_keyboard_stays_with_the_map() {
    let mut s = session();
    let button = window_rect(&s, "button");
    // The minimap, the button, the empty sidebar and the strip under the map.
    for p in [Point::new(900, 100), Point::new(button.x + 5, button.y + 5), Point::new(900, 600), Point::new(300, 700)]
    {
        click(&mut s, p);
        assert_eq!(s.ui.focused(), Some(s.map_widget()), "after a click at {p:?}");
        let before = s.game.map.cursor;
        assert!(press(&mut s, SpecialKey::Left), "the arrows still work after a click at {p:?}");
        assert_eq!(s.game.map.cursor.col + 1, before.col);
    }
}

#[test]
fn the_button_does_nothing() {
    let mut s = session();
    let b = window_rect(&s, "button");
    let before = (s.view.clone(), s.game.map.cursor);
    click(&mut s, Point::new(b.x + b.w / 2, b.y + b.h / 2));
    assert_eq!((s.view.clone(), s.game.map.cursor), before);
    assert!(!s.quit_requested() && s.game.selected().is_none());
}

#[test]
fn ctrl_q_quits_wherever_the_last_click_was() {
    for p in [Point::new(100, 100), Point::new(900, 100), Point::new(900, 600), Point::new(300, 700)] {
        let mut s = session();
        click(&mut s, p);
        // A plain q is nobody's key.
        assert!(!s.ui.key(Some('q'), SpecialKey::None, Modifiers::NONE, true), "{p:?}");
        s.pump();
        assert!(!s.quit_requested(), "{p:?}");
        // The window sends chords as the upper-case key.
        assert!(s.ui.key(Some('Q'), SpecialKey::None, Modifiers::CTRL, true), "{p:?}");
        assert!(!s.quit_requested(), "not before the session looks");
        s.pump();
        assert!(s.quit_requested(), "{p:?}");
    }
}

#[test]
fn cmd_q_quits_too() {
    let mut s = session();
    assert!(s.ui.key(Some('Q'), SpecialKey::None, Modifiers::SUPER, true));
    s.pump();
    assert!(s.quit_requested());
}

#[test]
fn f11_asks_for_fullscreen_and_alt_f_is_consumed() {
    let mut s = session();
    assert!(s.ui.key(None, SpecialKey::F11, Modifiers::NONE, true));
    assert!(!s.ui.key(None, SpecialKey::F11, Modifiers::NONE, false));
    s.pump();
    assert_eq!(s.take_fullscreen_toggles(), 1);
    assert_eq!(s.take_fullscreen_toggles(), 0);
    // Alt+F did nothing in the C++ either (its handler was a stub).
    assert!(s.ui.key(Some('f'), SpecialKey::None, Modifiers::ALT, true));
    assert!(!s.ui.key(Some('f'), SpecialKey::None, Modifiers::NONE, true));
    s.pump();
    assert_eq!(s.take_fullscreen_toggles(), 0);
    assert!(!s.quit_requested());
}

#[test]
fn missing_assets_are_an_error_that_names_the_directory() {
    let nowhere = assets().join("no-such-directory");
    let err = Session::new(&nowhere, Size::new(1024, 768), 96).err().expect("no skin there");
    assert!(err.contains("skin") && err.contains("no-such-directory"), "{err}");
}
