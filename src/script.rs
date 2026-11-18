//! Scripted input, for screenshots of the game after some play (`--script`) and
//! for tests. A script is a list of steps separated by commas:
//!
//! - `left`, `right`, `up`, `down`, `enter`, `esc`: press and release that key;
//! - `1`, `2`: type that character (zoom out, zoom in);
//! - `click:ROW:COL`, `hover:ROW:COL`: click on, or point at, the middle of that
//!   hex of the main map (it has to be in view);
//! - `minimap:ROW:COL`: click on that hex of the minimap (the main map goes there);
//! - `cursor:ROW:COL`: put the cursor on that hex and center the view on it (not
//!   something a player can do in one step; it is for getting to a far corner);
//! - any step may end in `*N` to run it N times: `left*12`.
//!
//! The keys and the pointer go through the UI exactly as the window sends them.
//!
//! Not a port: the C++ had nothing like it.

use turbo_ferret::{Modifiers, SpecialKey};

use crate::session::Session;
use crate::state::Hex;

/// Run a script on a session. On an error the steps before it have happened.
pub fn run(session: &mut Session, script: &str) -> Result<(), String> {
    for step in script.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let (action, times) = match step.split_once('*') {
            Some((action, n)) => {
                (action.trim(), n.trim().parse::<u32>().map_err(|_| format!("{step}: \"{n}\" is not a count"))?)
            }
            None => (step, 1),
        };
        for _ in 0..times {
            run_step(session, action).map_err(|e| format!("{step}: {e}"))?;
        }
    }
    Ok(())
}

/// One step, parsed.
enum Step {
    Key(SpecialKey),
    Char(char),
    Click(Hex),
    Hover(Hex),
    MiniMap(Hex),
    Cursor(Hex),
}

fn parse_step(action: &str) -> Result<Step, String> {
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
        "1" => Step::Char('1'),
        "2" => Step::Char('2'),
        "click" => Step::Click(hex()?),
        "hover" => Step::Hover(hex()?),
        "minimap" => Step::MiniMap(hex()?),
        "cursor" => Step::Cursor(hex()?),
        _ => return Err("unknown step".to_string()),
    };
    if parts.next().is_some() {
        return Err("too many parts".to_string());
    }
    Ok(step)
}

fn run_step(session: &mut Session, action: &str) -> Result<(), String> {
    // Parse first: a step that is wrong does nothing.
    let step = parse_step(action)?;
    let pixel = |session: &Session, hex: Hex| -> Result<turbo_ferret::Point, String> {
        if !session.game.map.contains(hex) {
            return Err(format!("{hex} is not on the map"));
        }
        session.hex_pixel(hex).ok_or(format!("{hex} is not in view"))
    };
    match step {
        Step::Key(key) => {
            session.ui.key(None, key, Modifiers::NONE, true);
            session.ui.key(None, key, Modifiers::NONE, false);
        }
        Step::Char(ch) => {
            session.ui.key(Some(ch), SpecialKey::None, Modifiers::NONE, true);
            session.ui.key(Some(ch), SpecialKey::None, Modifiers::NONE, false);
        }
        Step::Hover(hex) => {
            let p = pixel(session, hex)?;
            session.ui.pointer_move(p, Modifiers::NONE);
        }
        Step::Click(hex) => {
            let p = pixel(session, hex)?;
            session.ui.pointer_move(p, Modifiers::NONE);
            session.ui.pointer_down(p, 1, Modifiers::NONE);
            session.ui.pointer_up(p, Modifiers::NONE);
        }
        Step::MiniMap(hex) => {
            if !session.game.map.contains(hex) {
                return Err(format!("{hex} is not on the map"));
            }
            let p = session.minimap_pixel(hex);
            session.ui.pointer_move(p, Modifiers::NONE);
            session.ui.pointer_down(p, 1, Modifiers::NONE);
            session.ui.pointer_up(p, Modifiers::NONE);
        }
        Step::Cursor(hex) => {
            if !session.game.set_cursor(hex) {
                return Err(format!("{hex} is not on the map"));
            }
            session.view.center_on(hex);
        }
    }
    session.pump();
    Ok(())
}
