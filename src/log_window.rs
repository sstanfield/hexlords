//! The log: what the game's history (`Game::history`) says to the side at the
//! screen, in a window that scrolls. Its battles, as the attacker or the
//! defender, with who won and who died on each side; the cities it took and
//! lost; the units that left it unpaid; the sides that went out. The latest
//! round first, each round's events in the order they happened; what is new
//! since the side last ended a turn in a color of its own. A line on top says
//! whose turn it is while the others play. The words are `info::log`'s.
//!
//! Not a port: the C++ had no windows.

use turbo_ferret::widgets::{ScrollContainer, ScrollMode, TextField};
use turbo_ferret::{LayoutParams, Rect, TbId, Ui, WidgetId};

use crate::game::Game;
use crate::gui::build::{self, WindowParts};
use crate::info;
use crate::state::Side;
use crate::state::scenario::side_name;

/// The log's window and its close button.
pub const LOG_WINDOW_ID: &str = "log_window";
pub const LOG_CLOSE_ID: &str = "log_close";
/// The skin of what is new.
pub const NEW_SKIN: &str = "LogNew";

/// The Watch button of the battle of the history's record `record`.
pub fn watch_id(record: usize) -> String {
    format!("watch_{record}")
}

/// The scrolled list's size at most, and the space between things, in dp.
const LIST_DP: (i32, i32) = (560, 340);
const SPACING_DP: i32 = 8;
/// How far a line after an event's first stands in, in spaces.
const INDENT: &str = "    ";

/// The log, while it is open: whose it is, from where in the history it is
/// new, and how long the history was when it was written (a longer one is
/// news, and the list is written again).
pub struct LogWindow {
    parts: WindowParts,
    status: WidgetId,
    status_text: String,
    list: WidgetId,
    side: Side,
    new_from: usize,
    length: usize,
    /// The Watch buttons of the battles, and the record each plays.
    watch: Vec<(TbId, usize)>,
}

impl LogWindow {
    /// Its window (the session closes the one on top first).
    pub fn window(&self) -> WidgetId {
        self.parts.window
    }

    /// Open `side`'s log of `game` on `parent` (the root window), what came
    /// after the history was `new_from` long marked new, placed in `area` at
    /// `align` (see `build::place`).
    pub fn open(
        ui: &mut Ui,
        parent: WidgetId,
        game: &Game,
        (side, new_from): (Side, usize),
        (area, align): (Rect, (f32, f32)),
    ) -> LogWindow {
        let mut name = side_name(side).chars();
        let title = name.next().map_or(String::new(), |c| c.to_uppercase().chain(name).collect::<String>());
        let parts = build::window(ui, parent, LOG_WINDOW_ID, &format!("{title}'s log"));
        let dp = |ui: &Ui, n: i32| ui.dimension_converter().dp_to_px(n);
        let status = build::heading(ui, parts.content, "");
        build::text(ui, parts.content, "Your battles, the cities you took and lost, who is out: the latest first.");
        let legend = build::text(ui, parts.content, "What happened since you ended your turn is in this color.");
        ui.set_skin(legend, TbId::new(NEW_SKIN));
        build::gap(ui, parts.content, dp(ui, SPACING_DP));

        // The list, scrolled when it is long; no taller than the area leaves.
        let room = (area.h - dp(ui, 180)).max(dp(ui, 80));
        let (list_w, list_h) = (dp(ui, LIST_DP.0), dp(ui, LIST_DP.1).min(room));
        let scroll = ui.add(parts.content, ScrollContainer::new(ScrollMode::YAuto).with_adapt_content_size(true));
        ui.set_layout_params(scroll, Some(LayoutParams::fixed(list_w, list_h)));
        let list = build::column(ui, scroll, 0);

        build::gap(ui, parts.content, dp(ui, SPACING_DP));
        build::button(ui, parts.content, LOG_CLOSE_ID, "Close");
        let mut log =
            LogWindow { parts, status, status_text: String::new(), list, side, new_from, length: 0, watch: Vec::new() };
        log.fill(ui, game);
        build::place(ui, parts, area, align);
        log
    }

    /// Write the list from the history.
    fn fill(&mut self, ui: &mut Ui, game: &Game) {
        for child in ui.children(self.list).to_vec() {
            ui.remove(child);
        }
        self.watch.clear();
        let log = info::log(game, self.side, self.new_from);
        if log.is_empty() {
            build::text(ui, self.list, "Nothing yet.");
        }
        let space = ui.dimension_converter().dp_to_px(SPACING_DP);
        for (round, entries) in &log {
            build::heading(ui, self.list, &format!("Round {round}"));
            for entry in entries {
                for (i, line) in entry.lines.iter().enumerate() {
                    let text = if i == 0 { line.clone() } else { format!("{INDENT}{line}") };
                    // A battle that can be watched has its button beside its
                    // first line.
                    let watchable = i == 0 && entry.battle && game.fight(entry.record).is_some();
                    let parent = if watchable { build::row(ui, self.list, space) } else { self.list };
                    let field = build::text(ui, parent, &text);
                    if entry.new {
                        ui.set_skin(field, TbId::new(NEW_SKIN));
                    }
                    if watchable {
                        let id = watch_id(entry.record);
                        build::button(ui, parent, &id, "Watch");
                        self.watch.push((TbId::new(&id), entry.record));
                    }
                }
            }
        }
        self.length = game.history().len();
    }

    /// Bring it up to date: `side`'s log, new from `new_from`, as the
    /// history is now. True if it was written again (its new lines and
    /// buttons are to be laid out).
    pub fn refresh(&mut self, ui: &mut Ui, game: &Game, side: Side, new_from: usize) -> bool {
        if self.side == side && self.new_from == new_from && self.length == game.history().len() {
            return false;
        }
        (self.side, self.new_from) = (side, new_from);
        self.fill(ui, game);
        build::refit(ui, self.parts);
        true
    }

    /// The line on top: whose turn it is, while the others play.
    pub fn set_status(&mut self, ui: &mut Ui, text: &str) {
        if self.status_text != text {
            ui.set_text(self.status, text);
            self.status_text = text.to_string();
        }
    }

    /// True while the window is open.
    pub fn is_open(&self, ui: &Ui) -> bool {
        build::is_open(ui, self.parts.window)
    }

    pub fn close(self, ui: &mut Ui) {
        build::close(ui, self.parts.window);
    }

    /// The record a click on one of its Watch buttons plays, if it was one.
    pub fn watch(&self, id: TbId) -> Option<usize> {
        self.watch.iter().find(|(button, _)| *button == id).map(|(_, record)| *record)
    }

    /// The Watch buttons' records, in the order they stand.
    pub fn watchable(&self) -> Vec<usize> {
        self.watch.iter().map(|(_, record)| *record).collect()
    }

    /// Its lines' fields, in order: the list's, and those in its rows (a
    /// battle's first line, beside its button).
    fn fields(&self, ui: &Ui) -> Vec<WidgetId> {
        let mut fields = Vec::new();
        for child in ui.children(self.list) {
            if ui.get::<TextField>(*child).is_some() {
                fields.push(*child);
            } else {
                fields.extend(ui.children(*child).iter().copied().filter(|id| ui.get::<TextField>(*id).is_some()));
            }
        }
        fields
    }

    /// What it says, a line at a time, the round headings among them.
    pub fn lines(&self, ui: &Ui) -> Vec<String> {
        self.fields(ui).iter().filter_map(|id| ui.get::<TextField>(*id)).map(|t| t.text.clone()).collect()
    }

    /// The lines of it that are new.
    pub fn new_lines(&self, ui: &Ui) -> Vec<String> {
        self.fields(ui)
            .iter()
            .filter(|id| ui.node(**id).is_some_and(|n| n.skin() == TbId::new(NEW_SKIN)))
            .filter_map(|id| ui.get::<TextField>(*id))
            .map(|t| t.text.clone())
            .collect()
    }

    /// The line on top.
    pub fn status(&self) -> &str {
        &self.status_text
    }
}
