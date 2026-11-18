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
//! - `cursor:ROW:COL`: put the cursor on that hex and center the view on it (not
//!   something a player can do in one step; it is for getting to a far corner);
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
    Cursor(Hex),
}

/// `X`, or modifiers and X joined by `+`: `ctrl+s`.
fn parse_chord(chord: &str) -> Result<(char, Modifiers), String> {
    let (names, key) = match chord.rsplit_once('+') {
        Some((names, key)) if !key.is_empty() => (names, key),
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
    let mut parts = action.split(':');
    let name = parts.next().unwrap_or("");
    let mut hex = || -> Result<Hex, String> {
        let mut number = |what: &str| -> Result<u16, String> {
            let s = parts.next().ok_or(format!("needs a {what}"))?;
            s.parse().map_err(|_| format!("\"{s}\" is not a {what}"))
        };
        Ok(Hex::new(number("row")?, number("column")?))
    };
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
            let r = screen.area(widget);
            let p = Point::new(r.x + r.w / 2, r.y + r.h / 2);
            click(screen, p);
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
