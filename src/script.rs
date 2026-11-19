//! Scripted input, for screenshots after some play (`--script`) and for tests.
//! A script is a list of steps separated by commas:
//!
//! - `left`, `right`, `up`, `down`, `enter`, `esc`, `space`: press and release
//!   that key;
//! - `1`, `2`: type that character (zoom out, zoom in);
//! - `key:X`: type the character X; with modifiers, `key:ctrl+s`,
//!   `key:shift+cmd+z` (`ctrl`, `shift`, `alt`, `cmd`);
//! - `click:ROW:COL`, `hover:ROW:COL`: click on, or point at, the middle of that
//!   hex of the main map (it has to be in view);
//! - `drag:ROW:COL:ROW:COL`: press the button on the first hex, move to the
//!   second and let go (both have to be in view);
//! - `minimap:ROW:COL`: click on that hex of the minimap (the main map goes there);
//! - `press:ID`: click the middle of the widget with that id (a button);
//! - `point:ID`: point at the middle of that widget (what is shown while the
//!   pointer is on it shows);
//! - `scroll:DX:DY`: two fingers on a trackpad (or the wheel) with the pointer
//!   in the middle of the main map: the map moves DX pixels right and DY down
//!   (`scroll:-200:0` shows what is to the right);
//! - `miniscroll:DX:DY`: the same on the minimap: the view goes the same way,
//!   by that many pixels of the minimap (`miniscroll:-10:0` goes far right);
//! - `zoomin:ROW:COL`, `zoomout:ROW:COL`: Ctrl and the wheel with the pointer on
//!   that hex of the main map (it has to be in view; it stays under the pointer);
//! - `pull:DX:DY`: drag the main map by its middle with the right button;
//! - `cursor:ROW:COL`: put the cursor on that hex and center the view on it, as
//!   far as the map's edges let it (not something a player can do in one step;
//!   it is for getting to a far corner);
//! - any step may end in `*N` to run it N times: `left*12`.
//!
//! The keys and the pointer go through the UI exactly as the window sends them,
//! and the scene applies what they did after every step.
//!
//! Not a port: the C++ had nothing like it.

use turbo_ferret::{Modifiers, Point, SpecialKey, TbId};

use crate::screen::{MapScreen, Scene};
use crate::state::Hex;

/// Run a script on a scene. On an error the steps before it have happened.
pub fn run<S: Scene>(scene: &mut S, script: &str) -> Result<(), String> {
    for step in script.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let (action, times) = match step.rsplit_once('*') {
            Some((action, n)) if !action.is_empty() && !action.ends_with(':') => {
                (action.trim(), n.trim().parse::<u32>().map_err(|_| format!("{step}: \"{n}\" is not a count"))?)
            }
            _ => (step, 1),
        };
        for _ in 0..times {
            run_step(scene, action).map_err(|e| format!("{step}: {e}"))?;
        }
    }
    Ok(())
}

enum Step {
    Key(SpecialKey),
    Char(char, Modifiers),
    Click(Hex),
    Hover(Hex),
    Drag(Hex, Hex),
    MiniMap(Hex),
    Press(String),
    Point(String),
    Cursor(Hex),
    Scroll(i32, i32),
    MiniScroll(i32, i32),
    Zoom(Hex, bool),
    Pull(i32, i32),
}

/// `X`, or modifiers and X joined by `+`: `ctrl+s`.
fn parse_chord(chord: &str) -> Result<(char, Modifiers), String> {
    let (names, key) = match chord.rsplit_once('+') {
        Some((names, key)) if !key.is_empty() => (names, key),
        // The plus key with modifiers: `ctrl++`.
        Some((names, _)) if names.ends_with('+') => (names, "+"),
        _ => ("", chord), // no modifiers (and "+" is the plus key)
    };
    let mut mods = Modifiers::NONE;
    for name in names.split('+').filter(|n| !n.is_empty()) {
        mods |= parse_modifier(name)?;
    }
    let mut chars = key.chars();
    match (chars.next(), chars.next()) {
        (Some(ch), None) => Ok((ch, mods)),
        _ => Err(format!("\"{key}\" is not one character")),
    }
}

fn parse_modifier(name: &str) -> Result<Modifiers, String> {
    Ok(match name {
        "ctrl" => Modifiers::CTRL,
        "shift" => Modifiers::SHIFT,
        "alt" => Modifiers::ALT,
        "cmd" => Modifiers::SUPER,
        _ => return Err(format!("\"{name}\" is not a modifier (ctrl, shift, alt, cmd)")),
    })
}

fn parse_step(action: &str) -> Result<Step, String> {
    // The two steps whose argument is free text.
    if let Some(chord) = action.strip_prefix("key:") {
        let (ch, mods) = parse_chord(chord)?;
        return Ok(Step::Char(ch, mods));
    }
    if let Some(id) = action.strip_prefix("press:") {
        return if id.is_empty() { Err("needs a widget id".to_string()) } else { Ok(Step::Press(id.to_string())) };
    }
    if let Some(id) = action.strip_prefix("point:") {
        return if id.is_empty() { Err("needs a widget id".to_string()) } else { Ok(Step::Point(id.to_string())) };
    }
    let mut parts = action.split(':');
    let name = parts.next().unwrap_or("");
    let mut hex = || -> Result<Hex, String> {
        let mut number = |what: &str| -> Result<u16, String> {
            let s = parts.next().ok_or(format!("needs a {what}"))?;
            s.parse().map_err(|_| format!("\"{s}\" is not a {what}"))
        };
        Ok(Hex::new(number("row")?, number("column")?))
    };
    // Steps that take pixels, which may be negative.
    if matches!(name, "scroll" | "miniscroll" | "pull") {
        let mut pixels = || -> Result<i32, String> {
            let s = parts.next().ok_or("needs DX:DY, in pixels")?;
            s.parse().map_err(|_| format!("\"{s}\" is not a number of pixels"))
        };
        let (dx, dy) = (pixels()?, pixels()?);
        if parts.next().is_some() {
            return Err("too many parts".to_string());
        }
        return Ok(match name {
            "scroll" => Step::Scroll(dx, dy),
            "miniscroll" => Step::MiniScroll(dx, dy),
            _ => Step::Pull(dx, dy),
        });
    }
    let step = match name {
        "left" => Step::Key(SpecialKey::Left),
        "right" => Step::Key(SpecialKey::Right),
        "up" => Step::Key(SpecialKey::Up),
        "down" => Step::Key(SpecialKey::Down),
        "enter" => Step::Key(SpecialKey::Enter),
        "esc" => Step::Key(SpecialKey::Esc),
        "space" => Step::Char(' ', Modifiers::NONE),
        "1" => Step::Char('1', Modifiers::NONE),
        "2" => Step::Char('2', Modifiers::NONE),
        "click" => Step::Click(hex()?),
        "hover" => Step::Hover(hex()?),
        "drag" => Step::Drag(hex()?, hex()?),
        "minimap" => Step::MiniMap(hex()?),
        "cursor" => Step::Cursor(hex()?),
        "zoomin" => Step::Zoom(hex()?, true),
        "zoomout" => Step::Zoom(hex()?, false),
        _ => return Err("unknown step".to_string()),
    };
    if parts.next().is_some() {
        return Err("too many parts".to_string());
    }
    Ok(step)
}

/// The window pixel in the middle of a hex of the main map.
fn pixel(screen: &MapScreen, hex: Hex) -> Result<Point, String> {
    if !screen.game.map.contains(hex) {
        return Err(format!("{hex} is not on the map"));
    }
    screen.hex_pixel(hex).ok_or(format!("{hex} is not in view"))
}

/// The window pixel in the middle of a rectangle.
fn middle(r: turbo_ferret::Rect) -> Point {
    Point::new(r.x + r.w / 2, r.y + r.h / 2)
}

fn click(screen: &mut MapScreen, p: Point) {
    screen.ui.pointer_move(p, Modifiers::NONE);
    screen.ui.pointer_down(p, 1, Modifiers::NONE);
    screen.ui.pointer_up(p, Modifiers::NONE);
}

fn run_step<S: Scene>(scene: &mut S, action: &str) -> Result<(), String> {
    // The whole step is read before any of it happens.
    let step = parse_step(action)?;
    let screen = scene.screen_mut();
    match step {
        Step::Key(key) => {
            screen.ui.key(None, key, Modifiers::NONE, true);
            screen.ui.key(None, key, Modifiers::NONE, false);
        }
        Step::Char(ch, mods) => {
            screen.ui.key(Some(ch), SpecialKey::None, mods, true);
            screen.ui.key(Some(ch), SpecialKey::None, mods, false);
        }
        Step::Hover(hex) => {
            let p = pixel(screen, hex)?;
            screen.ui.pointer_move(p, Modifiers::NONE);
        }
        Step::Click(hex) => {
            let p = pixel(screen, hex)?;
            click(screen, p);
        }
        Step::Drag(from, to) => {
            let (a, b) = (pixel(screen, from)?, pixel(screen, to)?);
            screen.ui.pointer_move(a, Modifiers::NONE);
            screen.ui.pointer_down(a, 1, Modifiers::NONE);
            screen.ui.pointer_move(b, Modifiers::NONE);
            screen.ui.pointer_up(b, Modifiers::NONE);
        }
        Step::MiniMap(hex) => {
            if !screen.game.map.contains(hex) {
                return Err(format!("{hex} is not on the map"));
            }
            let p = screen.minimap_pixel(hex);
            click(screen, p);
        }
        Step::Press(id) => {
            let widget = screen.ui.find(screen.ui.root(), TbId::new(&id)).ok_or(format!("no widget with id {id}"))?;
            let p = middle(screen.area(widget));
            click(screen, p);
        }
        Step::Point(id) => {
            let widget = screen.ui.find(screen.ui.root(), TbId::new(&id)).ok_or(format!("no widget with id {id}"))?;
            let p = middle(screen.area(widget));
            screen.ui.pointer_move(p, Modifiers::NONE);
        }
        Step::Scroll(dx, dy) => {
            let p = middle(screen.map_rect());
            screen.ui.pointer_move(p, Modifiers::NONE);
            if !screen.scroll(p, dx as f32, dy as f32) {
                return Err("something lies over the middle of the map".to_string());
            }
        }
        Step::MiniScroll(dx, dy) => {
            let p = middle(screen.minimap_rect());
            screen.ui.pointer_move(p, Modifiers::NONE);
            if !screen.scroll(p, dx as f32, dy as f32) {
                return Err("something lies over the middle of the minimap".to_string());
            }
        }
        Step::Zoom(hex, closer) => {
            let p = pixel(screen, hex)?;
            screen.ui.pointer_move(p, Modifiers::NONE);
            if !screen.zoom_at(p, closer) {
                return Err(format!("something lies over {hex}"));
            }
        }
        Step::Pull(dx, dy) => {
            let p = middle(screen.map_rect());
            screen.ui.pointer_move(p, Modifiers::NONE);
            if !screen.grab(p) {
                return Err("something lies over the middle of the map".to_string());
            }
            let to = Point::new(p.x + dx, p.y + dy);
            screen.ui.pointer_move(to, Modifiers::NONE);
            screen.drag(to);
            screen.let_go();
        }
        Step::Cursor(hex) => {
            if !screen.game.set_cursor(hex) {
                return Err(format!("{hex} is not on the map"));
            }
            screen.view.center_on(hex);
        }
    }
    scene.pump();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chord_is_a_key_with_its_modifiers() {
        assert_eq!(parse_chord("s"), Ok(('s', Modifiers::NONE)));
        assert_eq!(parse_chord("ctrl+s"), Ok(('s', Modifiers::CTRL)));
        assert_eq!(parse_chord("shift+cmd+z"), Ok(('z', Modifiers::SHIFT | Modifiers::SUPER)));
        // The plus key, alone and with modifiers.
        assert_eq!(parse_chord("+"), Ok(('+', Modifiers::NONE)));
        assert_eq!(parse_chord("ctrl++"), Ok(('+', Modifiers::CTRL)));
        assert_eq!(parse_chord("ctrl+shift++"), Ok(('+', Modifiers::CTRL | Modifiers::SHIFT)));
        assert!(parse_chord("hyper+s").is_err());
        assert!(parse_chord("ctrl+").is_err());
        assert!(parse_chord("ctrl+ab").is_err());
    }
}
