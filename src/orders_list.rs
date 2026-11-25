//! The sidebar's list of what needs orders, and the side's army under it
//! (the owner's word, 2026-11-24: "Lets try putting the sides stack list
//! over there. Can select a stack to go to it. Color can indicate if it is
//! a garrison and if it has orders for the turn yet ... Needs orders stacks
//! and cities should probably be on top and cities that need orders should
//! go away but stacks can stay as a list of your army").
//!
//! A row each, from `Game::tasks`: first the stacks and cities that need
//! orders (bright), then the rest of the side's stacks (a garrison in a
//! colour of its own, the stacks done with for the turn dimmed). A city
//! leaves the list once it has orders; a stack stays. A row is a button:
//! the session selects the stack, or opens production on the city
//! (`Session::see_to`); T goes to the next that needs orders. It replaced
//! the Tasks window.
//!
//! Not a port: the C++ had no such list.

use turbo_ferret::widgets::ImageWidget;
use turbo_ferret::{ImageHandle, LayoutParams, TbId, Ui, WidgetId};

use crate::game::{CityState, Game, StackState, Task};
use crate::gui::build;
use crate::state::Side;

/// The skins of a row: needs orders, a garrison, done with for the turn;
/// else a list's row.
pub const NEEDS_SKIN: &str = "OrderNeeds";
pub const GARRISON_SKIN: &str = "OrderGarrison";
pub const DONE_SKIN: &str = "OrderDone";
const ROW_SKIN: &str = "ListRow";

/// A row's icon, in dp.
const ICON_DP: i32 = 18;

/// The id of the `i`th row.
pub fn row_id(i: usize) -> String {
    format!("order_row_{i}")
}

/// A row: what it is about, what it says, its skin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub task: Task,
    pub text: String,
    pub skin: &'static str,
}

/// True if a task needs orders (`Task::tier` 1: a stack nobody has told
/// anything this turn, one stopped short or that fought and waits; a city
/// that builds nothing and was not told so).
pub fn needs_orders(task: &Task) -> bool {
    task.tier() == 1
}

/// `side`'s rows: those that need orders first, then its other stacks, in
/// the tasks' order (cities that have orders are not in it).
pub fn rows(game: &Game, side: Side) -> Vec<Row> {
    if side == Side::Grey {
        return Vec::new();
    }
    let tasks = game.tasks(side);
    let (needs, rest): (Vec<Task>, Vec<Task>) = tasks.into_iter().partition(needs_orders);
    let rest = rest.into_iter().filter(|t| matches!(t, Task::Stack { .. }));
    needs
        .into_iter()
        .map(|t| (t, NEEDS_SKIN))
        .chain(rest.map(|t| {
            let skin = match t {
                Task::Stack { state: StackState::Defending, .. } => GARRISON_SKIN,
                Task::Stack { state: StackState::Done | StackState::Spent | StackState::MovesLeft, .. } => DONE_SKIN,
                _ => ROW_SKIN,
            };
            (t, skin)
        }))
        .map(|(task, skin)| Row { text: row_text(game, &task), task, skin })
        .collect()
}

/// A row's words, short (the sidebar is narrow; the colour says whether it
/// needs orders): "3\u{d7} Alder", "Riders 12,6: on the way", "Birch:
/// idle".
pub fn row_text(game: &Game, task: &Task) -> String {
    let map = &game.map;
    match task {
        Task::Stack { at, units, state } => {
            let side = map.units_at(*at).first().map_or(Side::Grey, |id| id.side);
            let about = game.task_units(side, *at);
            let first = about.first().or(map.units_at(*at).first()).and_then(|id| map.units().get(*id));
            let who = match (first, units) {
                (Some(unit), 1) => unit.name.clone(),
                _ => format!("{units}\u{d7}"),
            };
            let place = map.city_at(*at).map_or(format!("{},{}", at.row, at.col), |c| c.name.clone());
            let doing = match state {
                StackState::Waiting => "waits",
                StackState::Stalled => "stopped short",
                StackState::MovesLeft => "moved",
                StackState::Done => "done",
                StackState::Marching => "on the way",
                StackState::Defending => "defends",
                StackState::Spent => "spent",
                StackState::Patrolling => "patrols",
                StackState::Marauding => "marauds",
                StackState::Fought => "fought",
            };
            if needs_orders(task) && *state == StackState::Waiting {
                format!("{who} {place}")
            } else {
                format!("{who} {place}: {doing}")
            }
        }
        Task::City { city, state } => {
            let name = map.cities().get(*city).map_or(String::new(), |c| c.name.clone());
            let doing = match state {
                CityState::Idle => "idle",
                CityState::Resting => "stopped",
                CityState::Waiting => "waits",
                CityState::Building => "building",
            };
            format!("{name}: {doing}")
        }
    }
}

/// The list in the sidebar: its heading and its column of rows.
pub struct OrdersList {
    heading: WidgetId,
    list: WidgetId,
    rows: Vec<Row>,
    width: i32,
}

impl OrdersList {
    pub fn new(heading: WidgetId, list: WidgetId) -> OrdersList {
        OrdersList { heading, list, rows: Vec::new(), width: 0 }
    }

    /// Show `rows`, each with its icon (its stack's front unit's; none for
    /// a city), `width` wide. Nothing is made again if nothing changed;
    /// true if it was.
    pub fn show(&mut self, ui: &mut Ui, rows: Vec<Row>, icons: &[Option<ImageHandle>], width: i32) -> bool {
        if rows == self.rows && width == self.width {
            return false;
        }
        for child in ui.children(self.list).to_vec() {
            ui.remove(child);
        }
        let needs = rows.iter().filter(|r| needs_orders(&r.task)).count();
        let heading = match needs {
            0 if rows.is_empty() => "No stacks".to_string(),
            0 => "All have orders".to_string(),
            1 => "1 needs orders".to_string(),
            n => format!("{n} need orders"),
        };
        ui.set_text(self.heading, &heading);
        let icon_px = ui.dimension_converter().dp_to_px(ICON_DP);
        for (i, row) in rows.iter().enumerate() {
            let id = row_id(i);
            let resource = format!(
                "TBButton: id: {id}, skin: {}, is-focusable: 0\n\
                 \tTBLayout: axis: x, spacing: 4, position: center\n\
                 \t\tTBImageWidget: id: {id}_icon\n\
                 \t\tTBTextField: id: {id}_text, text-align: left\n",
                row.skin
            );
            if ui.inflate(self.list, &resource).is_err() {
                continue;
            }
            let found = |ui: &Ui, name: &str| ui.find(self.list, TbId::new(name));
            let (Some(button), Some(icon), Some(text)) =
                (found(ui, &id), found(ui, &format!("{id}_icon")), found(ui, &format!("{id}_text")))
            else {
                continue;
            };
            ui.set_text(text, &row.text);
            build::set_width(ui, button, width);
            build::text_from_left(ui, button);
            match icons.get(i).cloned().flatten() {
                Some(handle) => {
                    ui.set_layout_params(icon, Some(LayoutParams::fixed(icon_px, icon_px)));
                    ui.with_widget::<ImageWidget, _>(icon, |w, cx| w.set_image(cx, handle));
                }
                None => ui.set_layout_params(icon, Some(LayoutParams::fixed(0, icon_px))),
            }
        }
        (self.rows, self.width) = (rows, width);
        true
    }

    /// The task of a row clicked, if `id` is one.
    pub fn click(&self, id: TbId) -> Option<Task> {
        (0..self.rows.len()).find(|i| id == TbId::new(&row_id(*i))).map(|i| self.rows[i].task)
    }

    /// The rows shown.
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// The task after `from` (or before, `back`) of those that need orders,
    /// going round; the first if `from` is none of them.
    pub fn next_needing(&self, from: Option<&Task>, back: bool) -> Option<Task> {
        let needing: Vec<Task> = self.rows.iter().map(|r| r.task).filter(needs_orders).collect();
        if needing.is_empty() {
            return None;
        }
        let at = from.and_then(|f| needing.iter().position(|t| t == f));
        let n = needing.len();
        let i = match (at, back) {
            (None, false) => 0,
            (None, true) => n - 1,
            (Some(i), false) => (i + 1) % n,
            (Some(i), true) => (i + n - 1) % n,
        };
        Some(needing[i])
    }
}
