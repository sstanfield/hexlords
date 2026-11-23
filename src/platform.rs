//! What the window needs from the operating system that miniquad does not do,
//! and what differs in a browser (the WebAssembly build): the clock, and where
//! a complaint goes.
//!
//! This is the one module that may hold `unsafe`: a call into C or
//! Objective-C, behind a safe function, as small as the call, with a `SAFETY`
//! comment that says why it is sound.

/// Bring the game's window in front of the other applications' windows. Call it
/// from one of miniquad's callbacks (they run on the main thread, once the
/// window exists).
///
/// On macOS a program started from a terminal opens its window *behind* the
/// terminal. miniquad 0.4.11 asks for activation in a delegate callback
/// (`applicationDidUpdate:`) with `NSRunningApplication activateWithOptions:`,
/// and on current macOS that request is not honoured; asking `NSApplication`
/// itself is. (Tried on macOS 15.8: miniquad alone and `[NSApp activate]` leave
/// the terminal in front, `[NSApp activateIgnoringOtherApps:YES]` brings the
/// window forward, and a helper process started by the game is refused.) So
/// this sends that one message.
///
/// It is the only `unsafe` in the crate, and it adds no dependency: the
/// Objective-C plumbing is what miniquad re-exports. If miniquad fixes its
/// activation, delete this. On other systems the window manager puts new
/// windows in front, and this does nothing.
pub fn bring_to_front() {
    #[cfg(target_os = "macos")]
    macos::activate();
}

#[cfg(target_os = "macos")]
mod macos {
    use miniquad::native::apple::frameworks::{ObjcId, YES, class, msg_send, sel, sel_impl};

    #[allow(unsafe_code)]
    pub fn activate() {
        // SAFETY: `NSApplication sharedApplication` returns the application object
        // (it exists: miniquad created it before any callback runs), and
        // `activateIgnoringOtherApps:` takes one BOOL and returns nothing. Both
        // are sent on the main thread, where miniquad runs its callbacks, as
        // AppKit requires.
        unsafe {
            let app: ObjcId = msg_send![class!(NSApplication), sharedApplication];
            let () = msg_send![app, activateIgnoringOtherApps: YES];
        }
    }
}

/// True in a browser (the WebAssembly build, `cargo run -p web`).
pub const WEB: bool = cfg!(target_arch = "wasm32");

/// The time since it was started, for the UI's clock. (`std::time::Instant`
/// does not exist in a browser: there the time is the page's.)
#[derive(Copy, Clone, Debug)]
pub struct Clock {
    #[cfg(not(target_arch = "wasm32"))]
    start: std::time::Instant,
    #[cfg(target_arch = "wasm32")]
    start: f64,
}

#[cfg(not(target_arch = "wasm32"))]
impl Clock {
    pub fn start() -> Clock {
        Clock { start: std::time::Instant::now() }
    }

    /// The milliseconds since [`Clock::start`].
    pub fn millis(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }
}

#[cfg(target_arch = "wasm32")]
impl Clock {
    pub fn start() -> Clock {
        Clock { start: miniquad::date::now() }
    }

    /// The milliseconds since [`Clock::start`].
    pub fn millis(&self) -> u64 {
        ((miniquad::date::now() - self.start).max(0.0) * 1000.0) as u64
    }
}

/// The time of day in nanoseconds since 1970, for a seed that is another
/// each time (the rules never look at the clock). In a browser it is known to
/// the millisecond.
#[cfg(not(target_arch = "wasm32"))]
pub fn clock_nanos() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos())
}

#[cfg(target_arch = "wasm32")]
pub fn clock_nanos() -> u128 {
    (miniquad::date::now().max(0.0) * 1e9) as u128
}

/// Where the person's saved games are kept (`save`), if this system has a
/// place for such things: `Hexlords/saves` under the system's directory
/// for a program's data (macOS: `~/Library/Application Support`; Windows:
/// `%APPDATA%`; else `$XDG_DATA_HOME`, or `~/.local/share`). `None` in a
/// browser, which has no files, and where the system does not say.
pub fn saves_dir() -> Option<std::path::PathBuf> {
    if WEB {
        return None;
    }
    let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(std::path::PathBuf::from);
    saves_under(std::env::consts::OS, var("HOME"), var("APPDATA"), var("XDG_DATA_HOME"))
}

/// `saves_dir` for a system called `os`, from what its environment says.
fn saves_under(
    os: &str,
    home: Option<std::path::PathBuf>,
    appdata: Option<std::path::PathBuf>,
    xdg: Option<std::path::PathBuf>,
) -> Option<std::path::PathBuf> {
    let base = match os {
        "macos" => home?.join("Library").join("Application Support"),
        "windows" => appdata?,
        _ => xdg.or_else(|| home.map(|home| home.join(".local").join("share")))?,
    };
    Some(base.join(if os == "macos" || os == "windows" { "Hexlords" } else { "hexlords" }).join("saves"))
}

/// Say that something went wrong: on the terminal, or in a browser on its
/// console (where what a program prints goes nowhere).
pub fn complain(message: &str) {
    miniquad::error!("{}", message);
}

/// In a browser, have a panic say what it was on the console before the page
/// stops (without this it is only "unreachable executed"). Elsewhere nothing:
/// a panic already says so on the terminal.
pub fn report_panics() {
    if WEB {
        std::panic::set_hook(Box::new(|info| miniquad::error!("{}", info)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_starts_at_nothing_and_goes_on() {
        let clock = Clock::start();
        let first = clock.millis();
        assert!(first < 1000, "{first}");
        assert!(clock.millis() >= first);
    }

    #[test]
    fn saved_games_go_where_the_system_keeps_a_programs_data() {
        use std::path::PathBuf;
        let (home, appdata, xdg) = (PathBuf::from("/home/me"), PathBuf::from("C:/Roaming"), PathBuf::from("/data"));
        let at = |os: &str, home: Option<&PathBuf>, appdata: Option<&PathBuf>, xdg: Option<&PathBuf>| {
            saves_under(os, home.cloned(), appdata.cloned(), xdg.cloned())
        };
        let mac = at("macos", Some(&home), None, Some(&xdg));
        assert_eq!(mac, Some(PathBuf::from("/home/me/Library/Application Support/Hexlords/saves")));
        assert_eq!(at("linux", Some(&home), None, None), Some(PathBuf::from("/home/me/.local/share/hexlords/saves")));
        assert_eq!(at("linux", Some(&home), None, Some(&xdg)), Some(PathBuf::from("/data/hexlords/saves")));
        assert_eq!(at("windows", None, Some(&appdata), None), Some(appdata.join("Hexlords").join("saves")));
        // A system that does not say where: nowhere.
        assert_eq!((at("macos", None, None, None), at("linux", None, None, None)), (None, None));
        assert_eq!(saves_dir().is_some(), !WEB);
    }

    #[test]
    fn the_time_of_day_is_a_seed() {
        // After 2020, in nanoseconds: a number of 19 digits.
        assert!(clock_nanos() > 1_577_836_800_000_000_000);
    }
}
