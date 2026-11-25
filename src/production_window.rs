//! The production panel (E, the Cities menu's Production..., O on one's own
//! city): a side's production in one place, over the map (the owner's word,
//! 2026-11-23: "a central production hub ... a birds eye view of current
//! production as well as make changes"; 2026-11-24: "Merge the production
//! and city dialogs ... The goal is to have a single consice area to view
//! and manage all of a side production with a minimum of hassle").
//!
//! It covers the map's rectangle; the stack's boxes under the map, the
//! sidebar and the minimap stay (the minimap marks the cities idle or
//! waiting and the chosen one: `minimap_marks`). Its header says the side's
//! cities at a glance, and warns when the gold will not pay the upkeep
//! (`Game::forecast`). On the left the cities, a row each (those at no work
//! first), with a row of filters over them, and what the next rounds bring;
//! on the right the city chosen in full (the city window's view, built into
//! `city_host` by the session: what it builds, its queue, where its units
//! go, its walls), or, when asked for, the side's stacks, dearest first, to
//! cut the upkeep. Along the bottom: Your stacks, and Stop all production
//! (after asking).
//!
//! It gives no order itself: what a click asks for goes back to the session
//! ([`ProductionClick`]), as the other windows' clicks do; the city's own
//! clicks are the city window's (`CityOrder`).
//!
//! Not a port: the C++ had no windows.

use std::collections::HashMap;

use turbo_ferret::widgets::{ImageWidget, ScrollContainer, ScrollMode};
use turbo_ferret::{ImageHandle, LayoutParams, Rect, TbId, Ui, Visibility, WidgetId};

use crate::game::{CityState, Game, PayDay, Wait};
use crate::gui::build::{self, WindowParts};
use crate::info;
use crate::orders::{RALLY_POINTS, RallyPoint};
use crate::render::{BATTLE_BORDER, CHOSEN_COLOR, MiniMarks};
use crate::state::scenario::side_name;
use crate::state::{CityId, Hex, Sends, Side, UnitId};

pub const PRODUCTION_WINDOW_ID: &str = "production_window";
pub const PRODUCTION_CLOSE_ID: &str = "production_close";
pub const PROD_STACKS_ID: &str = "prod_stacks";
pub const PROD_SELECT_ID: &str = "prod_select";
pub const PROD_DISBAND_ID: &str = "prod_disband";
pub const PROD_STOP_ALL_ID: &str = "prod_stop_all";
pub const PROD_STOP_ALL_YES_ID: &str = "prod_stop_all_yes";
pub const PROD_STOP_ALL_NO_ID: &str = "prod_stop_all_no";
/// The filters over the cities: all of them, those at no work, those
/// waiting, those building.
pub const PROD_ALL_ID: &str = "prod_filter_all";
pub const PROD_IDLE_ID: &str = "prod_filter_idle";
pub const PROD_WAITING_ID: &str = "prod_filter_waiting";
pub const PROD_BUILDING_ID: &str = "prod_filter_building";

/// How many rounds it lists; how far it looks for the gold; how near the
/// round the gold runs short must be for the header to warn (the owner's
/// word, 2026-11-24: "The upkeep not getting paid is agressive, maybe only
/// display it if it is within 5 turns or so").
pub const SHOWN_ROUNDS: usize = 5;
pub const LOOKED_ROUNDS: usize = 20;
pub const WARN_ROUNDS: u32 = 5;

/// The size of an icon of what comes, in dp.
const COMING_ICON_DP: i32 = 22;

/// The left column's width, the spacing, a scroll bar's room, in dp.
const LEFT_DP: i32 = 300;
const SPACING_DP: i32 = 8;
const SCROLL_BAR_DP: i32 = 16;

/// The id of the `i`th city's row (of those the filter shows).
pub fn city_row_id(i: usize) -> String {
    format!("prod_city_{i}")
}

/// The id of the icon of the `i`th unit the `d`th round shown finishes.
pub fn coming_id(d: usize, i: usize) -> String {
    format!("prod_coming_{d}_{i}")
}

/// The id of the `i`th stack's row.
pub fn stack_row_id(i: usize) -> String {
    format!("prod_stack_{i}")
}

/// Which part of the panel the keyboard is in: Tab goes from one to the
/// next (the owner's word, 2026-11-24: "the user should be able to use the
/// keyboard to move between units and update the discription area").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Area {
    /// Up and Down choose a city; Enter goes to its units.
    Cities,
    /// Up and Down choose an item of the queue; - and + its count, Delete
    /// takes it out.
    Queue,
    /// The arrows go from unit to unit; Enter adds the one there to the
    /// queue.
    Units,
}

/// Which of the side's cities the list shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    All,
    Idle,
    Waiting,
    Building,
}

impl Filter {
    /// Its button, and the word for its cities ("2 idle").
    const ALL: [(Filter, &'static str, &'static str); 4] = [
        (Filter::All, PROD_ALL_ID, "All"),
        (Filter::Idle, PROD_IDLE_ID, "idle"),
        (Filter::Waiting, PROD_WAITING_ID, "waiting"),
        (Filter::Building, PROD_BUILDING_ID, "building"),
    ];

    /// True if a city that stands so is one it shows.
    pub fn shows(self, state: CityState) -> bool {
        match self {
            Filter::All => true,
            Filter::Idle => state == CityState::Idle,
            Filter::Waiting => state == CityState::Waiting,
            Filter::Building => state == CityState::Building,
        }
    }
}

/// Some of a side's units on one hex, as one row of the stacks: those
/// that defend there, or the others (a hex with both is two rows).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StackRow {
    pub at: Hex,
    pub units: Vec<UnitId>,
    pub defending: bool,
    /// What they cost a turn, together.
    pub upkeep: u32,
}

/// `side`'s stacks, the dearest first (then by place): on each hex, those
/// that defend and the others, apart.
pub fn stack_rows(game: &Game, side: Side) -> Vec<StackRow> {
    let map = &game.map;
    let mut rows = Vec::new();
    for at in game.stacks_of(side) {
        let here: Vec<_> =
            map.units_at(at).iter().filter_map(|id| map.units().get(*id)).filter(|u| u.side() == side).collect();
        for defending in [true, false] {
            let units: Vec<_> = here.iter().filter(|u| u.defending == defending).collect();
            if !units.is_empty() {
                let upkeep = units.iter().map(|u| u.upkeep).sum();
                rows.push(StackRow { at, units: units.iter().map(|u| u.id()).collect(), defending, upkeep });
            }
        }
    }
    rows.sort_by_key(|r| (std::cmp::Reverse(r.upkeep), r.at.row, r.at.col, !r.defending));
    rows
}

/// A stack's row in words: where, how many, what they cost, what they do.
/// "Alder: 4 units, upkeep 9, defending"; "row 12, column 6: 2 units,
/// upkeep 4, on the way to rally point 1 (row 20, column 3)".
pub fn stack_line(game: &Game, row: &StackRow) -> String {
    let map = &game.map;
    let units: Vec<_> = row.units.iter().filter_map(|id| map.units().get(*id)).collect();
    let count = if units.len() == 1 { "1 unit".to_string() } else { format!("{} units", units.len()) };
    let doing = if row.defending {
        "defending".to_string()
    } else if let Some(words) = units.iter().find_map(|u| info::duty_words(u)) {
        words.replace(": the computer leads it", "")
    } else if let Some(unit) = units.iter().find(|u| !u.route.is_empty()) {
        format!("on the way to {}", info::route_words_to(map, &unit.route, unit.bound))
    } else {
        "waiting".to_string()
    };
    format!("{}: {count}, upkeep {}, {doing}", info::place(map, row.at), row.upkeep)
}

/// What a click in the panel asks of the session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProductionClick {
    /// Every city of the side builds nothing (asked first).
    StopAll,
    /// The stack's units, selected (under the map).
    Select { at: Hex, units: Vec<UnitId> },
    /// The stack goes home for good (the disband window asks first).
    Disband(Vec<UnitId>),
}

/// What is chosen in the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Chosen {
    City(CityId),
    /// A stack, by its hex and whether it is the defenders.
    Stack(Hex, bool),
}

/// What the panel shows was worked out from: when none of it changed,
/// nothing need be worked out again.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Key {
    revision: u64,
    side: Side,
    points: [Option<RallyPoint>; RALLY_POINTS],
    gold: u32,
    turn: u32,
    player: Side,
    acting: bool,
    terrain: u64,
}

pub struct ProductionWindow {
    parts: WindowParts,
    side: Side,
    summary: WidgetId,
    warning: WidgetId,
    filters: Vec<(Filter, WidgetId)>,
    city_scroll: WidgetId,
    city_list: WidgetId,
    coming_heading: WidgetId,
    coming_scroll: WidgetId,
    coming_list: WidgetId,
    /// The right column: the city (the session builds its view into
    /// `city_host`), or the stacks.
    right_scroll: WidgetId,
    city_host: WidgetId,
    stack_box: WidgetId,
    stack_list: WidgetId,
    stack_actions: WidgetId,
    stack_buttons: Vec<WidgetId>,
    bottom: WidgetId,
    stacks_toggle: WidgetId,
    stop_all: WidgetId,
    confirm: WidgetId,
    confirm_text: WidgetId,
    /// The rectangle it covers (the map's).
    area: Rect,
    row_w: i32,
    /// The side's cities (the filter's), in the order of their rows, and
    /// what the rows say; how many stand each way, for the filters.
    cities: Vec<CityId>,
    city_lines: Vec<String>,
    counts: [usize; 4],
    coming: Vec<(bool, String)>,
    /// The next rounds as the forecast has them (the strip of icons), the
    /// icons of what the side's cities build (`set_icons`), and the words
    /// of each icon shown (the session's tip box says them on hover).
    days: Vec<PayDay>,
    icons: HashMap<(CityId, String), ImageHandle>,
    coming_tips: Vec<(WidgetId, String)>,
    warning_line: Option<String>,
    stacks: Vec<StackRow>,
    stack_lines: Vec<String>,
    stacks_shown: bool,
    filter: Filter,
    focus: Area,
    chosen: Option<Chosen>,
    asking: bool,
    acting: bool,
    key: Option<Key>,
}

fn dp(ui: &Ui, n: i32) -> i32 {
    ui.dimension_converter().dp_to_px(n)
}

fn set_pressed(ui: &mut Ui, button: WidgetId, pressed: bool) {
    let value = i32::from(pressed);
    if ui.value(button).as_int() != value {
        ui.set_value(button, value);
    }
}

fn show(ui: &mut Ui, widget: WidgetId, shown: bool) {
    let visibility = if shown { Visibility::Visible } else { Visibility::Gone };
    if ui.node(widget).is_some_and(|n| n.visibility() != visibility) {
        ui.set_visibility(widget, visibility);
    }
}

fn enable(ui: &mut Ui, button: WidgetId, enabled: bool) {
    if ui.node(button).is_some_and(|n| n.is_disabled() == enabled) {
        ui.set_disabled(button, !enabled);
    }
}

impl ProductionWindow {
    /// The panel (the session closes the one on top first).
    pub fn window(&self) -> WidgetId {
        self.parts.window
    }

    /// Open `side`'s production on `parent` (the root window), over `area`
    /// (the map's rectangle); `acting`: the person may give orders now.
    pub fn open(
        ui: &mut Ui,
        parent: WidgetId,
        game: &Game,
        (side, acting): (Side, bool),
        area: Rect,
    ) -> ProductionWindow {
        let parts = build::panel(ui, parent, PRODUCTION_WINDOW_ID);
        let space = dp(ui, SPACING_DP);
        let content = parts.content;

        // The header: whose, the cities at a glance, Close; a warning
        // under it when the gold will not pay the upkeep.
        let header = build::row(ui, content, space * 2);
        let mut name = side_name(side).chars();
        let title = name.next().map_or(String::new(), |c| c.to_uppercase().chain(name).collect::<String>());
        build::heading(ui, header, &format!("{title}'s production"));
        let summary = build::text(ui, header, "");
        build::button(ui, header, PRODUCTION_CLOSE_ID, "Close (E)");
        let warning = build::text(ui, content, "");
        ui.set_skin(warning, TbId::new("Warning"));
        ui.set_visibility(warning, Visibility::Gone);
        build::gap(ui, content, space / 2);

        let columns = build::top_row(ui, content, space * 2);
        // The left: the filters, the cities, what comes.
        let left = build::column(ui, columns, space / 2);
        let filter_row = build::row(ui, left, dp(ui, 2));
        let filters = Filter::ALL
            .iter()
            .map(|(filter, id, _)| {
                let button = build::button(ui, filter_row, id, "");
                ui.set_group_id(button, TbId::new("prod_filter"));
                (*filter, button)
            })
            .collect();
        let city_scroll = ui.add(left, ScrollContainer::new(ScrollMode::YAuto).with_adapt_content_size(true));
        let city_list = build::column(ui, city_scroll, 0);
        let coming_heading = build::heading(ui, left, &format!("Coming up, the next {SHOWN_ROUNDS} rounds"));
        let coming_scroll = ui.add(left, ScrollContainer::new(ScrollMode::YAuto).with_adapt_content_size(true));
        let coming_list = build::column(ui, coming_scroll, 0);

        // The right: the city, or the stacks.
        let right_scroll = ui.add(columns, ScrollContainer::new(ScrollMode::YAuto).with_adapt_content_size(true));
        let right = build::column(ui, right_scroll, space / 2);
        let city_host = build::column(ui, right, 0);
        let stack_box = build::column(ui, right, space / 2);
        build::heading(ui, stack_box, "Your stacks, the dearest first");
        build::text(ui, stack_box, "Select one to send some of it home (X), or disband it all.");
        let stack_list = build::column(ui, stack_box, 0);
        let stack_actions = build::row(ui, stack_box, space / 2);
        let stack_buttons = vec![
            build::button(ui, stack_actions, PROD_SELECT_ID, "Select (under the map)"),
            build::button(ui, stack_actions, PROD_DISBAND_ID, "Disband..."),
        ];
        ui.set_visibility(stack_box, Visibility::Gone);

        // The bottom: the stacks, and Stop all (asking in place).
        build::gap(ui, content, space / 2);
        let bottom = build::row(ui, content, space * 2);
        let stacks_toggle = build::button(ui, bottom, PROD_STACKS_ID, "");
        let stop_all = build::button(ui, bottom, PROD_STOP_ALL_ID, "Stop all production...");
        let confirm = build::row(ui, bottom, space);
        let confirm_text = build::text(ui, confirm, "");
        build::primary_button(ui, confirm, PROD_STOP_ALL_YES_ID, "Stop them all");
        build::button(ui, confirm, PROD_STOP_ALL_NO_ID, "Keep building");
        ui.set_visibility(confirm, Visibility::Gone);

        let mut window = ProductionWindow {
            parts,
            side,
            summary,
            warning,
            filters,
            city_scroll,
            city_list,
            coming_heading,
            coming_scroll,
            coming_list,
            right_scroll,
            city_host,
            stack_box,
            stack_list,
            stack_actions,
            stack_buttons,
            bottom,
            stacks_toggle,
            stop_all,
            confirm,
            confirm_text,
            area,
            row_w: dp(ui, LEFT_DP - SCROLL_BAR_DP),
            cities: Vec::new(),
            city_lines: Vec::new(),
            counts: [0; 4],
            coming: Vec::new(),
            days: Vec::new(),
            icons: HashMap::new(),
            coming_tips: Vec::new(),
            warning_line: None,
            stacks: Vec::new(),
            stack_lines: Vec::new(),
            stacks_shown: false,
            filter: Filter::All,
            focus: Area::Cities,
            chosen: None,
            asking: false,
            acting,
            key: None,
        };
        window.refresh(ui, game, (side, acting));
        window
    }

    /// Cover `area` (the map's rectangle, after a resize).
    pub fn place(&mut self, ui: &mut Ui, area: Rect) {
        if self.area != area {
            self.area = area;
            self.layout(ui);
        }
    }

    /// Choose `city` (when it opens, or O on it), if it is the side's:
    /// with the filter that shows it.
    pub fn choose_city(&mut self, game: &Game, city: CityId) {
        let Some(c) = game.map.cities().get(city).filter(|c| c.side == self.side) else { return };
        if !self.filter.shows(CityState::of(c)) {
            self.filter = Filter::All;
        }
        (self.chosen, self.stacks_shown) = (Some(Chosen::City(city)), false);
        self.key = None;
    }

    /// The next city of the list (`down`) or the one before (Up, Down), or
    /// the next stack while they show. True if the choice moved.
    pub fn step(&mut self, down: bool) -> bool {
        let at = |n: usize, i: Option<usize>| match (i, down) {
            (None, _) => 0,
            (Some(i), true) => (i + 1).min(n - 1),
            (Some(i), false) => i.saturating_sub(1),
        };
        let before = self.chosen;
        if self.stacks_shown {
            if self.stacks.is_empty() {
                return false;
            }
            let i = self.stacks.iter().position(|r| Some(Chosen::Stack(r.at, r.defending)) == self.chosen);
            let row = &self.stacks[at(self.stacks.len(), i)];
            self.chosen = Some(Chosen::Stack(row.at, row.defending));
        } else {
            if self.cities.is_empty() {
                return false;
            }
            let i = self.cities.iter().position(|id| Some(Chosen::City(*id)) == self.chosen);
            self.chosen = Some(Chosen::City(self.cities[at(self.cities.len(), i)]));
        }
        self.key = None;
        before != self.chosen
    }

    /// The part the keyboard is in.
    pub fn focus(&self) -> Area {
        self.focus
    }

    /// The next part (Tab), or the one before (Shift+Tab); the stacks,
    /// while shown, have only their list.
    pub fn next_focus(&mut self, forward: bool) {
        const ORDER: [Area; 3] = [Area::Cities, Area::Queue, Area::Units];
        if self.stacks_shown {
            self.focus = Area::Cities;
            return;
        }
        let i = ORDER.iter().position(|a| *a == self.focus).unwrap_or(0);
        self.focus = ORDER[if forward { (i + 1) % 3 } else { (i + 2) % 3 }];
    }

    pub fn set_focus(&mut self, area: Area) {
        self.focus = area;
    }

    /// Show only the cities `filter` lets through.
    pub fn set_filter(&mut self, filter: Filter) {
        self.filter = filter;
        self.stacks_shown = false;
        self.key = None;
    }

    /// Bring it up to date with the game, for `side` (whose it is; `acting`:
    /// whether the person may give orders now). True if it was laid out
    /// anew.
    pub fn refresh(&mut self, ui: &mut Ui, game: &Game, (side, acting): (Side, bool)) -> bool {
        let key = Key {
            revision: game.map.revision(),
            side,
            points: game.rally_points(side),
            gold: game.gold(side),
            turn: game.turn(),
            player: game.player(),
            acting,
            terrain: game.map.terrain_revision(),
        };
        if self.key.as_ref() == Some(&key) {
            return false;
        }
        if self.side != side {
            (self.chosen, self.asking) = (None, false);
        }
        (self.side, self.acting) = (side, acting);
        if !acting {
            self.asking = false;
        }
        let map = &game.map;
        let forecast = game.forecast(side, LOOKED_ROUNDS);
        // The cities: those at no work first, then those waiting, then the
        // rest; by place within each; those the filter lets through.
        let mut cities: Vec<_> = map.cities().iter().filter(|c| c.side == side && side != Side::Grey).collect();
        let rank = |state: CityState| match state {
            CityState::Idle => 0,
            CityState::Waiting => 1,
            CityState::Building => 2,
            CityState::Resting => 3,
        };
        cities.sort_by_key(|c| (rank(CityState::of(c)), c.pos.row, c.pos.col));
        self.counts =
            std::array::from_fn(|i| cities.iter().filter(|c| Filter::ALL[i].0.shows(CityState::of(c))).count());
        cities.retain(|c| self.filter.shows(CityState::of(c)));
        self.cities = cities.iter().map(|c| c.id).collect();
        self.city_lines = cities.iter().map(|c| info::production_row(map, c)).collect();
        self.coming = info::coming_lines(map, &forecast, SHOWN_ROUNDS);
        self.days = forecast.days.iter().take(SHOWN_ROUNDS).cloned().collect();
        self.warning_line = forecast
            .short_at
            .filter(|round| *round <= game.turn() + WARN_ROUNDS)
            .map(|round| format!("From round {round} the gold will not pay the upkeep: units will leave."));
        self.stacks = stack_rows(game, side);
        self.stack_lines = self.stacks.iter().map(|r| stack_line(game, r)).collect();
        // What was chosen may be gone (or filtered out: the first instead).
        self.chosen = match self.chosen {
            Some(Chosen::City(id)) if self.cities.contains(&id) => Some(Chosen::City(id)),
            Some(Chosen::Stack(at, d)) if self.stacks.iter().any(|r| r.at == at && r.defending == d) => {
                Some(Chosen::Stack(at, d))
            }
            _ if self.stacks_shown => self.stacks.first().map(|r| Chosen::Stack(r.at, r.defending)),
            _ => self.cities.first().map(|id| Chosen::City(*id)),
        };
        self.fill(ui, game);
        self.layout(ui);
        self.key = Some(key);
        true
    }

    /// Write its lines and rows, and set its buttons, as things are.
    fn fill(&mut self, ui: &mut Ui, game: &Game) {
        let [all, idle, waiting, building] = self.counts;
        let cities = if all == 1 { "1 city".to_string() } else { format!("{all} cities") };
        ui.set_text(self.summary, &format!("{cities}: {idle} idle, {waiting} waiting, {building} building"));
        ui.set_text(self.warning, self.warning_line.as_deref().unwrap_or(""));
        show(ui, self.warning, self.warning_line.is_some());
        for ((filter, button), (_, _, word)) in self.filters.iter().zip(Filter::ALL) {
            let n = self.counts[Filter::ALL.iter().position(|(f, _, _)| f == filter).unwrap_or(0)];
            let text = if *filter == Filter::All { format!("All {n}") } else { format!("{n} {word}") };
            ui.set_text(*button, &text);
            set_pressed(ui, *button, self.filter == *filter);
        }
        let row = |ui: &mut Ui, list: WidgetId, id: &str, text: &str, w: i32| {
            let button = build::button(ui, list, id, text);
            ui.set_skin(button, TbId::new("ListRow"));
            build::set_width(ui, button, w);
            build::text_from_left(ui, button);
            button
        };
        for child in ui.children(self.city_list).to_vec() {
            ui.remove(child);
        }
        if self.cities.is_empty() {
            let none = if self.counts[0] == 0 { "You hold no city." } else { "None." };
            build::text(ui, self.city_list, none);
        }
        for (i, (id, line)) in self.cities.iter().zip(&self.city_lines).enumerate() {
            let button = row(ui, self.city_list, &city_row_id(i), line, self.row_w);
            set_pressed(ui, button, self.chosen == Some(Chosen::City(*id)));
        }
        self.fill_coming(ui, game);
        // The right column: the city, or the stacks.
        show(ui, self.city_host, !self.stacks_shown);
        show(ui, self.stack_box, self.stacks_shown);
        for child in ui.children(self.stack_list).to_vec() {
            ui.remove(child);
        }
        if self.stacks_shown {
            let w = (self.area.w - dp(ui, LEFT_DP + 4 * SPACING_DP + 2 * SCROLL_BAR_DP)).max(dp(ui, 200));
            for (i, (stack, line)) in self.stacks.iter().zip(&self.stack_lines).enumerate() {
                let button = row(ui, self.stack_list, &stack_row_id(i), line, w);
                set_pressed(ui, button, self.chosen == Some(Chosen::Stack(stack.at, stack.defending)));
            }
        }
        show(ui, self.stack_actions, matches!(self.chosen, Some(Chosen::Stack(..))));
        enable(ui, self.stack_buttons[1], self.acting);
        let what = if self.stacks_shown { "back to the cities" } else { "to cut the upkeep" };
        ui.set_text(self.stacks_toggle, &format!("Your stacks ({}): {what}", self.stacks.len()));
        set_pressed(ui, self.stacks_toggle, self.stacks_shown);
        enable(ui, self.stop_all, self.acting && self.stoppable(game) > 0);
        let stopping = self.stoppable(game);
        let question = if stopping == 1 {
            "Stop your 1 city at work?".to_string()
        } else {
            format!("Stop all {stopping} at work?")
        };
        ui.set_text(self.confirm_text, &question);
        show(ui, self.confirm, self.asking);
        show(ui, self.stop_all, !self.asking);
    }

    /// What comes, as a strip a round (the owner's word, 2026-11-24: "I like
    /// the idea of a strip per round, in general our UI needs get more
    /// gamey with icons more and text less"): the round, an icon for each
    /// unit its cities finish, how many wait (for gold or room), and the
    /// gold left; a round the upkeep is not paid says so. The words of each
    /// icon show over it on hover (`hovered_coming`).
    fn fill_coming(&mut self, ui: &mut Ui, game: &Game) {
        for child in ui.children(self.coming_list).to_vec() {
            ui.remove(child);
        }
        self.coming_tips.clear();
        if self.days.is_empty() {
            build::text(ui, self.coming_list, "Nothing.");
        }
        let map = &game.map;
        let name = |id: CityId| map.cities().get(id).map_or("?".to_string(), |c| c.name.clone());
        let icon_px = dp(ui, COMING_ICON_DP);
        for (d, day) in self.days.iter().enumerate() {
            let row = build::row(ui, self.coming_list, dp(ui, 2));
            let round = build::text(ui, row, &format!("R{}", day.round));
            ui.set_skin(round, TbId::new("Heading"));
            build::set_width(ui, round, dp(ui, 30));
            if day.short {
                let text = build::text(ui, row, &format!("upkeep {}: units leave", day.upkeep));
                ui.set_skin(text, TbId::new("Warning"));
                break;
            }
            for (i, built) in day.built.iter().enumerate() {
                let icon = ui.add(row, ImageWidget::new());
                ui.set_id(icon, TbId::new(&coming_id(d, i)));
                ui.set_layout_params(icon, Some(LayoutParams::fixed(icon_px, icon_px)));
                if let Some(handle) = self.icons.get(&(built.city, built.kind.clone())).cloned() {
                    ui.with_widget::<ImageWidget, _>(icon, |w, cx| w.set_image(cx, handle));
                }
                let to = info::sends_short(map, &built.sends);
                let words = format!("{}: {}, {} gold; {to}", name(built.city), built.kind, built.price);
                self.coming_tips.push((icon, words));
            }
            if !day.waiting.is_empty() {
                let gold = day.waiting.iter().filter(|(_, w)| *w == Wait::Gold).count();
                let words = match (gold, day.waiting.len() - gold) {
                    (g, 0) => format!("+{g} wait for gold"),
                    (0, r) => format!("+{r} wait for room"),
                    (g, r) => format!("+{} wait (gold {g}, room {r})", g + r),
                };
                let text = build::text(ui, row, &words);
                ui.set_skin(text, TbId::new("Warning"));
            }
            if day.built.is_empty() && day.waiting.is_empty() {
                build::text(ui, row, "nothing");
            }
            build::text(ui, row, &format!("  {}g", day.gold_after));
        }
    }

    /// Give it the icons of what the side's cities build, by city and kind
    /// (the session draws them); shown at the next refresh if they changed.
    pub fn set_icons(&mut self, icons: HashMap<(CityId, String), ImageHandle>) {
        if icons != self.icons {
            self.icons = icons;
            self.key = None;
        }
    }

    /// The icon of what comes the pointer is on, and its words.
    pub fn hovered_coming(&self, ui: &Ui) -> Option<(WidgetId, String)> {
        let over = ui.hovered()?;
        self.coming_tips.iter().find(|(icon, _)| *icon == over || ui.is_ancestor_of(*icon, over)).cloned()
    }

    /// Size its parts to the rectangle it covers: the lists as tall as
    /// what the header and the bottom leave.
    fn layout(&mut self, ui: &mut Ui) {
        let space = dp(ui, SPACING_DP);
        build::place_panel(ui, self.parts, self.area);
        // (Its own padding's rectangle: the content's is its preferred size.)
        let inside = ui.padding_rect(self.parts.window);
        let header = ui.preferred_size_free(ui.children(self.parts.content)[0]).pref.h;
        let warning = if self.warning_line.is_some() { ui.preferred_size_free(self.warning).pref.h } else { 0 };
        let bottom = ui.preferred_size_free(self.bottom).pref.h;
        // (No more than there is room for: on a small screen the lists are
        // short and scroll rather than run past its edge; the owner saw its
        // top row go off the screen.)
        let body = (inside.h - header - warning - bottom - space * 3).max(dp(ui, 40));
        let left_w = dp(ui, LEFT_DP);
        let filters = ui.preferred_size_free(ui.parent(self.filters[0].1).unwrap_or(self.city_list)).pref.h;
        let heading = ui.preferred_size_free(self.coming_heading).pref.h;
        let lists = (body - filters - heading - space).max(dp(ui, 32));
        let coming_h = lists * 2 / 5;
        ui.set_layout_params(self.city_scroll, Some(LayoutParams::fixed(left_w, lists - coming_h)));
        ui.set_layout_params(self.coming_scroll, Some(LayoutParams::fixed(left_w, coming_h)));
        let right_w = (inside.w - left_w - space * 2).max(dp(ui, 200));
        ui.set_layout_params(self.right_scroll, Some(LayoutParams::fixed(right_w, body)));
        // (A scroll container is a layout boundary: what holds it hears of
        // its new size only if told.)
        for scroll in [self.city_scroll, self.coming_scroll, self.right_scroll] {
            if let Some(parent) = ui.parent(scroll) {
                ui.invalidate_layout(parent);
            }
        }
        build::arrange_panel(ui, self.parts, self.area);
    }

    /// The width the city's view has in the right column.
    pub fn city_width(&self, ui: &Ui) -> i32 {
        (self.area.w - dp(ui, LEFT_DP + 4 * SPACING_DP + SCROLL_BAR_DP)).max(dp(ui, 200))
    }

    /// Lay it out again (the city's view in it changed size).
    pub fn relayout(&mut self, ui: &mut Ui) {
        self.layout(ui);
    }

    /// How many of the side's cities Stop all would stop.
    fn stoppable(&self, game: &Game) -> usize {
        let side = self.side;
        game.map.cities().iter().filter(|c| c.side == side && (c.production().is_some() || !c.queue.is_empty())).count()
    }

    /// What the game's minimap marks while it is open (the owner's word,
    /// 2026-11-24: its own map went, "use the game minimap"): the side's
    /// cities at no work or waiting ringed in orange, the chosen city in
    /// gold, and a line from it to where it sends its units.
    pub fn minimap_marks(&self, game: &Game) -> MiniMarks {
        let map = &game.map;
        let side = self.side;
        let mut rings: Vec<(Hex, [f32; 3])> = map
            .cities()
            .iter()
            .filter(|c| c.side == side && matches!(CityState::of(c), CityState::Idle | CityState::Waiting))
            .map(|c| (c.pos, BATTLE_BORDER))
            .collect();
        let chosen = match self.chosen {
            Some(Chosen::City(id)) => map.cities().get(id),
            _ => None,
        };
        match self.chosen {
            Some(Chosen::City(_)) => {
                if let Some(c) = chosen {
                    rings.retain(|(at, _)| *at != c.pos);
                    rings.push((c.pos, CHOSEN_COLOR));
                }
            }
            Some(Chosen::Stack(at, _)) => rings.push((at, CHOSEN_COLOR)),
            None => {}
        }
        let line = chosen.filter(|c| c.production().is_some()).and_then(|c| {
            let to = match &c.sends {
                Sends::Route(route) => route.last().copied(),
                Sends::Point { n, .. } => game.rally_point(self.side, *n).map(|p| p.at),
                Sends::Stay | Sends::Defend => None,
            };
            to.map(|to| (c.pos, to))
        });
        MiniMarks { rings, line }
    }

    /// True if `id` is one of its buttons (the session hands it the click).
    pub fn owns(&self, id: TbId) -> bool {
        let fixed = [
            PROD_STACKS_ID,
            PROD_SELECT_ID,
            PROD_DISBAND_ID,
            PROD_STOP_ALL_ID,
            PROD_STOP_ALL_YES_ID,
            PROD_STOP_ALL_NO_ID,
            PROD_ALL_ID,
            PROD_IDLE_ID,
            PROD_WAITING_ID,
            PROD_BUILDING_ID,
        ];
        fixed.iter().any(|f| id == TbId::new(f))
            || (0..self.cities.len()).any(|i| id == TbId::new(&city_row_id(i)))
            || (0..self.stacks.len()).any(|i| id == TbId::new(&stack_row_id(i)))
    }

    /// A click on one of its buttons: what is chosen, shown and asked it
    /// keeps itself (and is laid out anew); what it asks of the game it
    /// returns.
    pub fn click(&mut self, id: TbId) -> Option<ProductionClick> {
        let is = |name: &str| id == TbId::new(name);
        // Anything the panel does itself is shown at the next refresh.
        self.key = None;
        if let Some(i) = (0..self.cities.len()).find(|i| id == TbId::new(&city_row_id(*i))) {
            self.chosen = Some(Chosen::City(self.cities[i]));
            return None;
        }
        if let Some(i) = (0..self.stacks.len()).find(|i| id == TbId::new(&stack_row_id(*i))) {
            let stack = &self.stacks[i];
            self.chosen = Some(Chosen::Stack(stack.at, stack.defending));
            return None;
        }
        if let Some((filter, _, _)) = Filter::ALL.iter().find(|(_, button, _)| is(button)) {
            self.set_filter(*filter);
            return None;
        }
        if is(PROD_STACKS_ID) {
            self.stacks_shown = !self.stacks_shown;
            self.chosen = None;
            return None;
        }
        if is(PROD_STOP_ALL_ID) {
            self.asking = self.acting && !self.asking;
            return None;
        }
        if is(PROD_STOP_ALL_NO_ID) {
            self.asking = false;
            return None;
        }
        if is(PROD_STOP_ALL_YES_ID) {
            let asked = std::mem::replace(&mut self.asking, false);
            return asked.then_some(ProductionClick::StopAll);
        }
        if let Some(Chosen::Stack(at, defending)) = self.chosen {
            let units = self.stacks.iter().find(|r| r.at == at && r.defending == defending)?.units.clone();
            if is(PROD_SELECT_ID) {
                return Some(ProductionClick::Select { at, units });
            }
            if is(PROD_DISBAND_ID) {
                return Some(ProductionClick::Disband(units));
            }
        }
        None
    }

    /// The side whose production it shows.
    pub fn side(&self) -> Side {
        self.side
    }

    /// Its cities (the filter's), in the order of their rows, and what the
    /// rows say.
    pub fn cities(&self) -> &[CityId] {
        &self.cities
    }

    pub fn city_lines(&self) -> &[String] {
        &self.city_lines
    }

    /// The filter, and how many cities each lets through.
    pub fn filter(&self) -> Filter {
        self.filter
    }

    /// The warning under the header, if the gold will not pay the upkeep.
    pub fn warning(&self) -> Option<&str> {
        self.warning_line.as_deref()
    }

    /// What it says comes in the next rounds (true: a round's heading).
    pub fn coming(&self) -> &[(bool, String)] {
        &self.coming
    }

    /// The side's stacks as its rows have them (shown or not), and what
    /// the rows say.
    pub fn stacks(&self) -> &[StackRow] {
        &self.stacks
    }

    pub fn stack_lines(&self) -> &[String] {
        &self.stack_lines
    }

    pub fn stacks_shown(&self) -> bool {
        self.stacks_shown
    }

    /// The city chosen, if a city is (its view is in the right column).
    pub fn chosen_city(&self) -> Option<CityId> {
        match self.chosen {
            Some(Chosen::City(id)) if !self.stacks_shown => Some(id),
            _ => None,
        }
    }

    /// Where the session builds the chosen city's view.
    pub fn city_host(&self) -> WidgetId {
        self.city_host
    }

    /// True while Stop all asks.
    pub fn asking(&self) -> bool {
        self.asking
    }

    /// The rectangle it covers.
    pub fn area(&self) -> Rect {
        self.area
    }

    pub fn is_open(&self, ui: &Ui) -> bool {
        build::is_open(ui, self.parts.window)
    }

    pub fn close(self, ui: &mut Ui) {
        build::close(ui, self.parts.window);
    }
}
