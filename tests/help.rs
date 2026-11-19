//! The help window: F1 or `?` opens it and closes it again, and so do Esc,
//! its button and its title bar's close button. It says every key, the ones
//! all windows share and the scene's own. No window and no GPU: a `Session`
//! on the real assets.

mod common;

use common::{click, hover, pixel_of, press, session, session_at, type_char, widget};
use hexgame::gui::RootCommand;
use hexgame::help::{command_key, fullscreen_keys};
use hexgame::screen::{HELP_CLOSE_ID, HELP_ID};
use hexgame::session::Session;
use hexgame::state::Hex;
use turbo_ferret::widgets::Window;
use turbo_ferret::{Modifiers, Point, Rect, SpecialKey, TbId};

fn help_window(s: &Session) -> Option<Rect> {
    let id = s.screen.ui.find(s.screen.ui.root(), TbId::new(HELP_ID))?;
    Some(Rect::from_origin_size(s.screen.ui.to_root(id, Point::ZERO), s.screen.ui.rect(id).size()))
}

fn f1(s: &mut Session) -> bool {
    press(s, SpecialKey::F1)
}

#[test]
fn f1_opens_the_help_and_closes_it_again() {
    let mut s = session();
    assert!(!s.screen.help_open() && help_window(&s).is_none());
    assert!(f1(&mut s), "F1 is the window's key");
    assert!(s.screen.help_open());
    // In the middle of the window, inside it.
    let r = help_window(&s).expect("the help window");
    assert!(r.x > 0 && r.y > 0 && r.right() < 1024 && r.bottom() < 768, "{r:?}");
    assert!((r.x + r.w / 2 - 512).abs() <= 1 && (r.y + r.h / 2 - 384).abs() <= 1, "{r:?}");
    // The keyboard stays with the map: the arrows still move the cursor.
    assert_eq!(s.screen.ui.focused(), Some(s.screen.map_widget()));
    let cursor = s.screen.game.map.cursor;
    press(&mut s, SpecialKey::Left);
    assert_ne!(s.screen.game.map.cursor, cursor);
    f1(&mut s);
    assert!(!s.screen.help_open() && help_window(&s).is_none());
    // `?` does the same, for keyboards whose F1 is the system's.
    assert!(type_char(&mut s, '?'));
    assert!(s.screen.help_open());
    type_char(&mut s, '?');
    assert!(!s.screen.help_open());
}

#[test]
fn esc_and_the_buttons_close_it() {
    let mut s = session();
    f1(&mut s);
    press(&mut s, SpecialKey::Esc);
    assert!(!s.screen.help_open(), "Esc");
    // Esc with the help closed is the scene's, as before.
    press(&mut s, SpecialKey::Esc);
    assert!(!s.screen.help_open());

    f1(&mut s);
    let close = s.screen.area(widget(&s, HELP_CLOSE_ID));
    click(&mut s, Point::new(close.x + close.w / 2, close.y + close.h / 2));
    assert!(!s.screen.help_open(), "its Close button");
    assert_eq!(s.screen.ui.focused(), Some(s.screen.map_widget()));

    f1(&mut s);
    let win = widget(&s, HELP_ID);
    let x = s.screen.ui.get::<Window>(win).and_then(|w| w.close_button()).expect("a close button");
    let x = s.screen.area(x);
    click(&mut s, Point::new(x.x + x.w / 2, x.y + x.h / 2));
    s.frame(16);
    assert!(!s.screen.help_open(), "the title bar's close button");
    // And F1 opens it again after that.
    f1(&mut s);
    assert!(s.screen.help_open());
}

#[test]
fn a_click_beside_it_goes_to_the_map() {
    let mut s = session();
    f1(&mut s);
    let hex = Hex::new(48, 44);
    let p = pixel_of(&s, hex);
    assert!(!help_window(&s).unwrap().contains(p), "the hex is beside the window");
    hover(&mut s, p);
    assert_eq!(s.screen.game.map.cursor, hex);
    // Its buttons are no one else's: the scene hears nothing of them.
    let close = s.screen.area(widget(&s, HELP_CLOSE_ID));
    let c = Point::new(close.x + close.w / 2, close.y + close.h / 2);
    s.screen.ui.pointer_move(c, Modifiers::NONE);
    s.screen.ui.pointer_down(c, 1, Modifiers::NONE);
    s.screen.ui.pointer_up(c, Modifiers::NONE);
    assert_eq!(s.screen.pump_shared(), Vec::<RootCommand>::new());
}

#[test]
fn it_says_every_key_of_the_game() {
    let s = session();
    let help = s.screen.help();
    let titles: Vec<&str> = help.iter().map(|section| section.title.as_str()).collect();
    assert_eq!(titles, ["The map", "The game", "The window"]);
    let keys: Vec<&str> = help.iter().flat_map(|section| section.rows.iter().map(|(keys, _)| keys.as_str())).collect();
    for expected in ["Arrow keys", "1  /  2", "F1  or  ?", fullscreen_keys(), &format!("{}+Q", command_key())] {
        assert!(keys.contains(&expected), "{expected} is not in {keys:?}");
    }
    assert!(keys.iter().any(|k| k.contains(&format!("{}+T", command_key()))), "{keys:?}");
}

#[test]
fn the_window_shows_every_line_and_fits_a_small_screen() {
    let mut s = session();
    f1(&mut s);
    let texts: Vec<String> = {
        let root = widget(&s, HELP_ID);
        let mut out = Vec::new();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if let Some(text) = s.screen.ui.text(id) {
                out.push(text.to_string());
            }
            stack.extend(s.screen.ui.children(id).iter().copied());
        }
        out
    };
    for section in s.screen.help() {
        assert!(texts.contains(&section.title), "{}", section.title);
        for (keys, what) in &section.rows {
            assert!(texts.contains(keys) && texts.contains(what), "{keys}: {what}");
        }
    }
    // The rows are inside the window, under its title.
    let win = help_window(&s).unwrap();
    let close = s.screen.area(widget(&s, HELP_CLOSE_ID));
    assert!(win.contains(Point::new(close.x, close.y)) && close.bottom() <= win.bottom(), "{close:?} {win:?}");
    // A window too small for it: the help stays inside it.
    let mut small = session_at(500, 300, 96);
    f1(&mut small);
    let r = help_window(&small).unwrap();
    assert!(r.x >= 0 && r.y >= 0 && r.right() <= 500 && r.bottom() <= 300, "{r:?}");
}
