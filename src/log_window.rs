//! The log: what the game's history (`Game::history`) says to the side at the
//! screen, in a window that scrolls. Its battles, as the attacker or the
//! defender, with who won and who died on each side; the cities it took and
//! lost; the sides that went out. The latest round first, each round's events
//! in the order they happened. The words are `info::log`'s.
//!
//! Not a port: the C++ had no windows.

use turbo_ferret::widgets::{ScrollContainer, ScrollMode, TextField};
use turbo_ferret::{LayoutParams, Rect, Ui, WidgetId};

use crate::game::Game;
use crate::gui::build::{self, WindowParts};
use crate::info;
use crate::state::Side;
use crate::state::scenario::side_name;

/// The log's window and its close button.
pub const LOG_WINDOW_ID: &str = "log_window";
pub const LOG_CLOSE_ID: &str = "log_close";

/// The scrolled list's size at most, and the space between things, in dp.
const LIST_DP: (i32, i32) = (560, 340);
const SPACING_DP: i32 = 8;
/// How far a line after an event's first stands in, in spaces.
const INDENT: &str = "    ";

/// The log, while it is open: whose it is, and how long the history was
/// when it was written (a longer one is news, and the log is written again).
pub struct LogWindow {
    parts: WindowParts,
    list: WidgetId,
    side: Side,
    length: usize,
}

impl LogWindow {
    /// Open `side`'s log of `game` on `parent` (the root window), placed in
    /// `area` at `align` (see `build::place`).
    pub fn open(
        ui: &mut Ui,
        parent: WidgetId,
        game: &Game,
        side: Side,
        (area, align): (Rect, (f32, f32)),
    ) -> LogWindow {
        let mut name = side_name(side).chars();
        let title = name.next().map_or(String::new(), |c| c.to_uppercase().chain(name).collect::<String>());
        let parts = build::window(ui, parent, LOG_WINDOW_ID, &format!("{title}'s log"));
        let dp = |ui: &Ui, n: i32| ui.dimension_converter().dp_to_px(n);
        build::text(ui, parts.content, "Your battles, the cities you took and lost, who is out: the latest first.");
        build::gap(ui, parts.content, dp(ui, SPACING_DP));

        // The list, scrolled when it is long; no taller than the area leaves.
        let room = (area.h - dp(ui, 140)).max(dp(ui, 80));
        let (list_w, list_h) = (dp(ui, LIST_DP.0), dp(ui, LIST_DP.1).min(room));
        let scroll = ui.add(parts.content, ScrollContainer::new(ScrollMode::YAuto).with_adapt_content_size(true));
        ui.set_layout_params(scroll, Some(LayoutParams::fixed(list_w, list_h)));
        let list = build::column(ui, scroll, 0);
        let log = info::log(game, side);
        if log.is_empty() {
            build::text(ui, list, "Nothing yet.");
        }
        for (round, entries) in &log {
            build::heading(ui, list, &format!("Round {round}"));
            for lines in entries {
                for (i, line) in lines.iter().enumerate() {
                    let text = if i == 0 { line.clone() } else { format!("{INDENT}{line}") };
                    build::text(ui, list, &text);
                }
            }
        }

        build::gap(ui, parts.content, dp(ui, SPACING_DP));
        build::button(ui, parts.content, LOG_CLOSE_ID, "Close");
        build::place(ui, parts, area, align);
        LogWindow { parts, list, side, length: game.history().len() }
    }

    /// True while the window is open.
    pub fn is_open(&self, ui: &Ui) -> bool {
        build::is_open(ui, self.parts.window)
    }

    pub fn close(self, ui: &mut Ui) {
        build::close(ui, self.parts.window);
    }

    /// True if what it says is out of date: the history has grown (or
    /// shrunk, an undo), or it is another side's turn.
    pub fn stale(&self, game: &Game, side: Side) -> bool {
        self.side != side || self.length != game.history().len()
    }

    /// What it says, a line at a time, the round headings among them.
    pub fn lines(&self, ui: &Ui) -> Vec<String> {
        ui.children(self.list).iter().filter_map(|id| ui.get::<TextField>(*id)).map(|t| t.text.clone()).collect()
    }
}
