//! The rally points' window (R, or the Units menu's Rally points): the
//! player's ten points, where each is and how many are on their way to it;
//! a point is placed (or moved) by a click on the map, looked at, taken
//! away, and set to have those who get there defend it. A city's window
//! sends its new units to one; Go to onto one's flag sends a stack there.
//!
//! Not a port: the C++ had no windows.

use turbo_ferret::{LayoutParams, Rect, TbId, Ui, Visibility, WidgetId};

use crate::game::Game;
use crate::gui::build::{self, WindowParts};
use crate::info;
use crate::orders::RALLY_POINTS;
use crate::state::Side;

pub const RALLY_WINDOW_ID: &str = "rally_window";
pub const RALLY_CLOSE_ID: &str = "rally_close";

/// The buttons of the `n`th point's row: look at it, place it (or move
/// it), take it away; and its box, defend there.
pub fn look_id(n: usize) -> String {
    format!("rally_look_{n}")
}
pub fn place_id(n: usize) -> String {
    format!("rally_place_{n}")
}
pub fn remove_id(n: usize) -> String {
    format!("rally_remove_{n}")
}
pub fn defend_id(n: usize) -> String {
    format!("rally_defend_{n}")
}

/// The space between things, and the width of a row's words, in dp.
const SPACING_DP: i32 = 8;
const WORDS_DP: i32 = 240;

/// What a click in the window asks for.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum RallyClick {
    /// Look at point `n` on the map.
    Look(u8),
    /// Pick on the map where point `n` goes.
    Place(u8),
    /// Take point `n` away.
    Remove(u8),
    /// Those who get to point `n` defend it, or no longer (the box's state).
    Defend(u8, bool),
}

/// A point's row: its words, its buttons, its box (and the box's label).
struct Row {
    words: WidgetId,
    look: WidgetId,
    place: WidgetId,
    remove: WidgetId,
    defend: WidgetId,
    defend_label: WidgetId,
    text: String,
    /// Whether it is set, and defends, as the row shows it (`None` before
    /// it shows anything).
    set: Option<Option<bool>>,
}

pub struct RallyWindow {
    parts: WindowParts,
    rows: Vec<Row>,
}

impl RallyWindow {
    /// Open the window of `side`'s rally points on `parent` (the root
    /// window), placed in `area` at `align` (see `build::place`).
    pub fn open(
        ui: &mut Ui,
        parent: WidgetId,
        game: &Game,
        side: Side,
        (area, align): (Rect, (f32, f32)),
    ) -> RallyWindow {
        let dp = |ui: &Ui, n: i32| ui.dimension_converter().dp_to_px(n);
        let parts = build::window(ui, parent, RALLY_WINDOW_ID, "Rally points");
        build::text(ui, parts.content, "Where your cities send their new units, and Go to your stacks, to gather.");
        build::text(ui, parts.content, "Once one is full, those who come after stand beside it.");
        build::gap(ui, parts.content, dp(ui, SPACING_DP));
        let list = build::column(ui, parts.content, dp(ui, 2));
        let words_w = dp(ui, WORDS_DP);
        let mut rows = Vec::new();
        for n in 0..RALLY_POINTS {
            let row = build::row(ui, list, dp(ui, SPACING_DP) / 2);
            let words = build::text(ui, row, "");
            ui.set_layout_params(
                words,
                Some(LayoutParams { min_w: Some(words_w), max_w: Some(words_w), ..LayoutParams::default() }),
            );
            let look = build::button(ui, row, &look_id(n), "Look");
            let place = build::button(ui, row, &place_id(n), "Place...");
            let remove = build::button(ui, row, &remove_id(n), "Remove");
            let id = defend_id(n);
            let resource = format!("TBClickLabel: text: \"Defend there\"\n\tTBCheckBox: id: {id}, is-focusable: 0\n");
            let _ = ui.inflate(row, &resource);
            let defend = ui.find(row, TbId::new(&id)).unwrap_or(row);
            let defend_label = ui.parent(defend).unwrap_or(defend);
            rows.push(Row { words, look, place, remove, defend, defend_label, text: String::new(), set: None });
        }
        build::gap(ui, parts.content, dp(ui, SPACING_DP));
        build::button(ui, parts.content, RALLY_CLOSE_ID, "Close");
        let mut window = RallyWindow { parts, rows };
        window.refresh(ui, game, side);
        build::place(ui, parts, area, align);
        window
    }

    /// Its window (the session closes the one on top first).
    pub fn window(&self) -> WidgetId {
        self.parts.window
    }

    /// True while it is open (its close button removes it).
    pub fn is_open(&self, ui: &Ui) -> bool {
        build::is_open(ui, self.parts.window)
    }

    pub fn close(self, ui: &mut Ui) {
        build::close(ui, self.parts.window);
    }

    /// What the `n`th row says.
    pub fn line(&self, n: usize) -> &str {
        self.rows.get(n).map_or("", |r| r.text.as_str())
    }

    /// Bring it up to date with `side`'s points: where each is, how many
    /// are on their way there, whether they defend there. A point not set
    /// has Place only.
    pub fn refresh(&mut self, ui: &mut Ui, game: &Game, side: Side) {
        let points = game.rally_points(side);
        let mut changed = false;
        for (n, row) in self.rows.iter_mut().enumerate() {
            let point = points[n];
            let coming = game.map.units().of_side(side).filter(|u| u.bound == Some(n as u8)).count();
            let text = match point {
                None => format!("{n}: not set"),
                Some(p) => {
                    let there = game.map.units_at(p.at).len();
                    let mut words = format!("{n}: {}", info::place(&game.map, p.at));
                    if there > 0 {
                        words.push_str(&format!(", {there} there"));
                    }
                    if coming > 0 {
                        words.push_str(&format!(", {coming} coming"));
                    }
                    words
                }
            };
            if text != row.text {
                ui.set_text(row.words, &text);
                row.text = text;
            }
            let set = point.map(|p| p.defend);
            if Some(set) != row.set {
                let shown = if set.is_some() { Visibility::Visible } else { Visibility::Gone };
                for widget in [row.look, row.remove, row.defend_label] {
                    ui.set_visibility(widget, shown);
                }
                ui.set_text(row.place, if set.is_some() { "Move..." } else { "Place..." });
                row.set = Some(set);
                changed = true;
            }
            let value = i32::from(set == Some(true));
            if ui.value(row.defend).as_int() != value {
                ui.set_value(row.defend, value);
            }
        }
        if changed {
            build::refit(ui, self.parts);
        }
    }

    /// What a click on one of its buttons or boxes asks for (the box has
    /// turned itself on or off already). `None` if it was none of them.
    pub fn click(&self, ui: &Ui, id: TbId) -> Option<RallyClick> {
        let n = |f: fn(usize) -> String| (0..RALLY_POINTS).find(|n| id == TbId::new(&f(*n))).map(|n| n as u8);
        if let Some(n) = n(look_id) {
            return Some(RallyClick::Look(n));
        }
        if let Some(n) = n(place_id) {
            return Some(RallyClick::Place(n));
        }
        if let Some(n) = n(remove_id) {
            return Some(RallyClick::Remove(n));
        }
        let n = n(defend_id)?;
        let on = self.rows.get(usize::from(n)).is_some_and(|r| ui.value(r.defend).as_int() != 0);
        Some(RallyClick::Defend(n, on))
    }
}
