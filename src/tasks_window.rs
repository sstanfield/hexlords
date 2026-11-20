//! The list of tasks (T, or the Tasks button): what the player has left to do
//! this turn, the most pressing first (`Game::tasks`): the stacks that have
//! had no orders and the cities that build nothing; then the stacks that
//! could go further and the cities whose work waits; then the rest. A row is
//! a button: a stack's selects it, a city's opens it. It follows the game as
//! it goes, so the player can work down it.

use turbo_ferret::widgets::{ScrollContainer, ScrollMode};
use turbo_ferret::{LayoutParams, Rect, TbId, Ui, WidgetId};

use crate::game::{Game, Task};
use crate::gui::build::{self, WindowParts};
use crate::info;
use crate::state::Side;
use crate::state::scenario::side_name;

pub const TASKS_WINDOW_ID: &str = "tasks_window";
pub const TASKS_CLOSE_ID: &str = "tasks_close";
/// The list's size, in dp; no taller than the map leaves.
const LIST_DP: (i32, i32) = (560, 340);
const SPACING_DP: i32 = 8;
const SCROLL_BAR_DP: i32 = 16;

/// The headings of the tiers ([`Task::tier`]).
pub const TIERS: [&str; 3] = ["Needs orders", "Could do more", "The rest"];

/// The id of the `i`th row.
pub fn row_id(i: usize) -> String {
    format!("task_{i}")
}

pub struct TasksWindow {
    parts: WindowParts,
    summary: WidgetId,
    /// The list, in a box that scrolls once it is `list_size` tall.
    scroll: WidgetId,
    list: WidgetId,
    list_size: (i32, i32),
    row_w: i32,
    side: Side,
    /// The tasks its rows show, and what they say.
    tasks: Vec<Task>,
    lines: Vec<String>,
}

impl TasksWindow {
    /// Its window (the session closes the one on top first).
    pub fn window(&self) -> WidgetId {
        self.parts.window
    }

    /// Open the list of `side`'s tasks on `parent` (the root window), placed
    /// in `area` at `align` (see `build::place`).
    pub fn open(
        ui: &mut Ui,
        parent: WidgetId,
        game: &Game,
        side: Side,
        (area, align): (Rect, (f32, f32)),
    ) -> TasksWindow {
        let dp = |ui: &Ui, n: i32| ui.dimension_converter().dp_to_px(n);
        let mut name = side_name(side).chars();
        let title = name.next().map_or(String::new(), |c| c.to_uppercase().chain(name).collect::<String>());
        let parts = build::window(ui, parent, TASKS_WINDOW_ID, &format!("{title}'s tasks"));
        let summary = build::heading(ui, parts.content, "");
        build::text(ui, parts.content, "Click one to see to it: a stack is selected, a city opens.");
        build::gap(ui, parts.content, dp(ui, SPACING_DP));
        let room = (area.h - dp(ui, 170)).max(dp(ui, 80));
        let (list_w, list_h) = (dp(ui, LIST_DP.0), dp(ui, LIST_DP.1).min(room));
        let scroll = ui.add(parts.content, ScrollContainer::new(ScrollMode::YAuto).with_adapt_content_size(true));
        ui.set_layout_params(scroll, Some(LayoutParams::fixed(list_w, list_h)));
        let list = build::column(ui, scroll, 0);
        build::gap(ui, parts.content, dp(ui, SPACING_DP));
        build::button(ui, parts.content, TASKS_CLOSE_ID, "Close");
        let row_w = list_w - dp(ui, SCROLL_BAR_DP);
        let mut window = TasksWindow {
            parts,
            summary,
            scroll,
            list,
            list_size: (list_w, list_h),
            row_w,
            side,
            tasks: Vec::new(),
            lines: vec!["(new)".to_string()],
        };
        window.refresh(ui, game, side);
        build::place(ui, parts, area, align);
        window
    }

    /// Bring it up to date with the game (and with whose tasks they are).
    /// True if its rows changed (the caller lays the screen out again).
    pub fn refresh(&mut self, ui: &mut Ui, game: &Game, side: Side) -> bool {
        let tasks = game.tasks(side);
        let lines: Vec<String> = tasks.iter().map(|t| info::task_line(game, t)).collect();
        if self.side == side && tasks == self.tasks && lines == self.lines {
            return false;
        }
        (self.side, self.tasks, self.lines) = (side, tasks, lines);
        self.fill(ui);
        // As tall as its rows, up to its size; then it scrolls.
        let (w, most) = self.list_size;
        let h = ui.preferred_size_free(self.list).pref.h.clamp(1, most);
        ui.set_layout_params(self.scroll, Some(LayoutParams::fixed(w, h)));
        build::refit(ui, self.parts);
        true
    }

    /// Write the rows: a heading for each tier there is, and its tasks.
    fn fill(&mut self, ui: &mut Ui) {
        for child in ui.children(self.list).to_vec() {
            ui.remove(child);
        }
        let count = |tier: u8| self.tasks.iter().filter(|t| t.tier() == tier).count();
        let summary = match (count(1), count(2)) {
            (0, 0) => "All seen to".to_string(),
            (first, second) => format!("{}: {first}, {}: {second}", TIERS[0], TIERS[1].to_lowercase()),
        };
        ui.set_text(self.summary, &summary);
        if self.tasks.is_empty() {
            build::text(ui, self.list, "No stacks and no cities.");
            return;
        }
        let mut tier = 0;
        for (i, (task, line)) in self.tasks.iter().zip(&self.lines).enumerate() {
            if task.tier() != tier {
                tier = task.tier();
                if i > 0 {
                    build::gap(ui, self.list, ui.dimension_converter().dp_to_px(SPACING_DP));
                }
                build::heading(ui, self.list, TIERS[usize::from(tier) - 1]);
            }
            let button = build::button(ui, self.list, &row_id(i), line);
            ui.set_skin(button, TbId::new("ListRow"));
            build::set_width(ui, button, self.row_w);
            build::text_from_left(ui, button);
        }
    }

    /// The task a click on one of its rows is about, if it was one.
    pub fn click(&self, id: TbId) -> Option<&Task> {
        (0..self.tasks.len()).find(|i| id == TbId::new(&row_id(*i))).and_then(|i| self.tasks.get(i))
    }

    /// The tasks it shows, in order.
    pub fn tasks(&self) -> &[Task] {
        &self.tasks
    }

    /// What its rows say.
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn is_open(&self, ui: &Ui) -> bool {
        build::is_open(ui, self.parts.window)
    }

    pub fn close(self, ui: &mut Ui) {
        build::close(ui, self.parts.window);
    }
}
