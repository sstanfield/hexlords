//! The production window (E, or the Cities menu's Production...): a side's
//! production in one place, so that its cities need not be visited one by
//! one (the owner's word, 2026-11-23: "a central production hub ... a
//! birds eye view of current production as well as make changes"). A map
//! of where its cities and rally points are; its gold, and whether it will
//! go on paying the upkeep (`Game::forecast`); every city, what it builds
//! and where the units go, those at no work first; what the next rounds
//! bring; and, when asked for, the side's stacks, dearest first, to cut
//! the upkeep.
//!
//! A city row chooses the city: open its window (what it builds, its
//! queue), stop it, keep its new units in it or have them defend it, send
//! them to a rally point or to another of the side's cities (To a city...,
//! then that city's row). Stop all production stops every city, after
//! asking. A stack row chooses the stack: Select puts it under the map (to
//! disband some of it with X), Disband... all of it. It gives no order
//! itself: what a click asks for goes back to the session
//! ([`ProductionClick`]), as the other windows' clicks do.
//!
//! Not a port: the C++ had no windows.

use turbo_ferret::widgets::{ImageWidget, ScrollContainer, ScrollMode};
use turbo_ferret::{ImageHandle, LayoutParams, Rect, TbId, Ui, Visibility, WidgetId};

use crate::city_windows::CityOrder;
use crate::game::{CityState, Game, Sight};
use crate::gui::build::{self, WindowParts};
use crate::info;
use crate::orders::{RALLY_POINTS, RallyPoint};
use crate::render::minimap_image::{Marks, minimap_marked};
use crate::state::scenario::side_name;
use crate::state::{CityId, Hex, Sends, Side, UnitId};

pub const PRODUCTION_WINDOW_ID: &str = "production_window";
pub const PRODUCTION_CLOSE_ID: &str = "production_close";
pub const PROD_STACKS_ID: &str = "prod_stacks";
pub const PROD_OPEN_ID: &str = "prod_open";
pub const PROD_STOP_ID: &str = "prod_stop";
pub const PROD_STAY_ID: &str = "prod_stay";
pub const PROD_DEFEND_ID: &str = "prod_defend";
pub const PROD_TO_CITY_ID: &str = "prod_to_city";
pub const PROD_SELECT_ID: &str = "prod_select";
pub const PROD_DISBAND_ID: &str = "prod_disband";
pub const PROD_STOP_ALL_ID: &str = "prod_stop_all";
pub const PROD_STOP_ALL_YES_ID: &str = "prod_stop_all_yes";
pub const PROD_STOP_ALL_NO_ID: &str = "prod_stop_all_no";

/// How many rounds it lists; how far it looks for the gold.
pub const SHOWN_ROUNDS: usize = 5;
pub const LOOKED_ROUNDS: usize = 20;

/// The map's picture, the list, the left column, in dp.
const MAP_DP: (i32, i32) = (240, 160);
const LIST_DP: (i32, i32) = (460, 330);
const SPACING_DP: i32 = 8;
const SCROLL_BAR_DP: i32 = 16;

/// The id of the `i`th city's row.
pub fn city_row_id(i: usize) -> String {
    format!("prod_city_{i}")
}

/// The id of the `i`th stack's row.
pub fn stack_row_id(i: usize) -> String {
    format!("prod_stack_{i}")
}

/// The id of rally point `n`'s button.
pub fn point_id(n: usize) -> String {
    format!("prod_point_{n}")
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

/// What a click in the window asks of the session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProductionClick {
    /// What the city windows ask for: its window (`Details`), to build
    /// nothing (`SetProduction(_, None)`), where its units go.
    City(CityOrder),
    /// The city's new units go to another city of the side.
    SendTo { city: CityId, to: Hex },
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

/// What the window shows was worked out from: when none of it changed,
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
    picture: WidgetId,
    image: ImageHandle,
    made: u64,
    /// What the picture shows.
    shows: Option<(u64, Marks)>,
    economy: WidgetId,
    stop_all: WidgetId,
    confirm: WidgetId,
    confirm_text: WidgetId,
    scroll: WidgetId,
    list: WidgetId,
    list_size: (i32, i32),
    /// The stacks' button, and their list under it (in a box of its own
    /// that scrolls, shown when asked for).
    stacks_toggle: WidgetId,
    stack_scroll: WidgetId,
    stack_list: WidgetId,
    row_w: i32,
    prompt: WidgetId,
    city_actions: WidgetId,
    city_buttons: Vec<WidgetId>,
    to_city: WidgetId,
    points_row: WidgetId,
    point_buttons: Vec<WidgetId>,
    stack_actions: WidgetId,
    stack_buttons: Vec<WidgetId>,
    /// Its cities, in the order of their rows, and what the rows say.
    cities: Vec<CityId>,
    city_lines: Vec<String>,
    coming: Vec<(bool, String)>,
    economy_lines: Vec<String>,
    stacks: Vec<StackRow>,
    stack_lines: Vec<String>,
    stacks_shown: bool,
    chosen: Option<Chosen>,
    /// To a city... was pressed for this city: the next city row is where
    /// its units go.
    sending: Option<CityId>,
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
    /// Its window (the session closes the one on top first).
    pub fn window(&self) -> WidgetId {
        self.parts.window
    }

    /// Open `side`'s production on `parent` (the root window), in `area` at
    /// `align`; `acting`: the person may give orders now.
    pub fn open(
        ui: &mut Ui,
        parent: WidgetId,
        game: &Game,
        (side, sight, acting): (Side, &Sight, bool),
        (area, align): (Rect, (f32, f32)),
    ) -> ProductionWindow {
        let mut name = side_name(side).chars();
        let title = name.next().map_or(String::new(), |c| c.to_uppercase().chain(name).collect::<String>());
        let parts = build::window(ui, parent, PRODUCTION_WINDOW_ID, &format!("{title}'s production"));
        let space = dp(ui, SPACING_DP);
        let columns = build::top_row(ui, parts.content, space * 2);

        // The left: the map, the money, Stop all.
        let left = build::column(ui, columns, space / 2);
        let picture = ui.add(left, ImageWidget::new());
        ui.set_layout_params(picture, Some(LayoutParams::fixed(dp(ui, MAP_DP.0), dp(ui, MAP_DP.1))));
        build::text(ui, left, "Flags: rally points; orange: idle");
        build::gap(ui, left, space / 2);
        build::heading(ui, left, "Gold");
        let economy = build::column(ui, left, 0);
        build::gap(ui, left, space);
        let stop_all = build::button(ui, left, PROD_STOP_ALL_ID, "Stop all production...");
        let confirm = build::column(ui, left, space / 2);
        let confirm_text = build::text(ui, confirm, "");
        build::text(ui, confirm, "Work and queues are lost.");
        let yes_no = build::row(ui, confirm, space);
        build::primary_button(ui, yes_no, PROD_STOP_ALL_YES_ID, "Stop them all");
        build::button(ui, yes_no, PROD_STOP_ALL_NO_ID, "Keep building");
        ui.set_visibility(confirm, Visibility::Gone);

        // The right: the list, and what can be done with the one chosen.
        let right = build::column(ui, columns, space / 2);
        let room = (area.h - dp(ui, 230)).max(dp(ui, 100));
        let (list_w, list_h) = (dp(ui, LIST_DP.0), dp(ui, LIST_DP.1).min(room));
        let scroll = ui.add(right, ScrollContainer::new(ScrollMode::YAuto).with_adapt_content_size(true));
        ui.set_layout_params(scroll, Some(LayoutParams::fixed(list_w, list_h)));
        let list = build::column(ui, scroll, 0);
        let stacks_toggle = build::button(ui, right, PROD_STACKS_ID, "");
        let stack_scroll = ui.add(right, ScrollContainer::new(ScrollMode::YAuto).with_adapt_content_size(true));
        ui.set_layout_params(stack_scroll, Some(LayoutParams::fixed(list_w, list_h / 2)));
        let stack_list = build::column(ui, stack_scroll, 0);
        ui.set_visibility(stack_scroll, Visibility::Gone);
        let prompt = build::text(ui, right, "");
        ui.set_visibility(prompt, Visibility::Gone);
        let city_actions = build::column(ui, right, space / 2);
        let row = build::row(ui, city_actions, space / 2);
        let city_buttons = vec![
            build::button(ui, row, PROD_OPEN_ID, "Open its window"),
            build::button(ui, row, PROD_STOP_ID, "Stop"),
            build::button(ui, row, PROD_STAY_ID, "Keep"),
            build::button(ui, row, PROD_DEFEND_ID, "Defend"),
        ];
        let to_city = build::button(ui, row, PROD_TO_CITY_ID, "To a city...");
        let points_row = build::row(ui, city_actions, dp(ui, 2));
        build::text(ui, points_row, "To rally point:");
        let point_buttons: Vec<WidgetId> = (0..RALLY_POINTS)
            .map(|n| {
                let button = build::button(ui, points_row, &point_id(n), &n.to_string());
                ui.set_visibility(button, Visibility::Gone);
                button
            })
            .collect();
        ui.set_visibility(city_actions, Visibility::Gone);
        let stack_actions = build::row(ui, right, space / 2);
        let stack_buttons = vec![
            build::button(ui, stack_actions, PROD_SELECT_ID, "Select (under the map)"),
            build::button(ui, stack_actions, PROD_DISBAND_ID, "Disband..."),
        ];
        ui.set_visibility(stack_actions, Visibility::Gone);
        build::gap(ui, parts.content, space);
        build::button(ui, parts.content, PRODUCTION_CLOSE_ID, "Close");

        let mut window = ProductionWindow {
            parts,
            side,
            picture,
            image: ImageHandle::default(),
            made: 0,
            shows: None,
            economy,
            stop_all,
            confirm,
            confirm_text,
            scroll,
            list,
            list_size: (list_w, list_h),
            stacks_toggle,
            stack_scroll,
            stack_list,
            row_w: list_w - dp(ui, SCROLL_BAR_DP),
            prompt,
            city_actions,
            city_buttons,
            to_city,
            points_row,
            point_buttons,
            stack_actions,
            stack_buttons,
            cities: Vec::new(),
            city_lines: Vec::new(),
            coming: Vec::new(),
            economy_lines: Vec::new(),
            stacks: Vec::new(),
            stack_lines: Vec::new(),
            stacks_shown: false,
            chosen: None,
            sending: None,
            asking: false,
            acting,
            key: None,
        };
        window.refresh(ui, game, (side, sight, acting));
        build::place(ui, parts, area, align);
        window
    }

    /// Choose `city` (the city shown when it opens), if it is in the list.
    pub fn choose_city(&mut self, city: CityId) {
        if self.cities.contains(&city) {
            self.chosen = Some(Chosen::City(city));
            self.key = None;
        }
    }

    /// Bring it up to date with the game, for `side` (whose it is; `acting`:
    /// whether the person may give orders now). True if it was laid out
    /// anew (the caller lays the screen out again).
    pub fn refresh(&mut self, ui: &mut Ui, game: &Game, (side, sight, acting): (Side, &Sight, bool)) -> bool {
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
            (self.chosen, self.sending, self.asking) = (None, None, false);
        }
        (self.side, self.acting) = (side, acting);
        if !acting {
            (self.sending, self.asking) = (None, false);
        }
        let map = &game.map;
        let forecast = game.forecast(side, LOOKED_ROUNDS);
        // The cities: those at no work first, then those waiting, then the
        // rest; by place within each.
        let mut cities: Vec<_> = map.cities().iter().filter(|c| c.side == side && side != Side::Grey).collect();
        let rank = |state: CityState| match state {
            CityState::Idle => 0,
            CityState::Waiting => 1,
            CityState::Building => 2,
            CityState::Resting => 3,
        };
        cities.sort_by_key(|c| (rank(CityState::of(c)), c.pos.row, c.pos.col));
        self.cities = cities.iter().map(|c| c.id).collect();
        self.city_lines = cities.iter().map(|c| info::production_row(map, c)).collect();
        self.coming = info::coming_lines(map, &forecast, SHOWN_ROUNDS);
        self.economy_lines = info::economy_lines(game, side, &forecast, SHOWN_ROUNDS);
        self.stacks = stack_rows(game, side);
        self.stack_lines = self.stacks.iter().map(|r| stack_line(game, r)).collect();
        // What was chosen may be gone.
        self.chosen = match self.chosen {
            Some(Chosen::City(id)) if self.cities.contains(&id) => Some(Chosen::City(id)),
            Some(Chosen::Stack(at, d)) if self.stacks.iter().any(|r| r.at == at && r.defending == d) => {
                Some(Chosen::Stack(at, d))
            }
            _ => None,
        };
        if self.sending.is_some_and(|id| !self.cities.contains(&id)) {
            self.sending = None;
        }
        self.fill(ui, game);
        self.draw_map(ui, game, sight);
        // The lists as tall as their rows, up to their room; the stacks,
        // when shown, take up to half of it.
        let (w, most) = self.list_size;
        let stacks_h =
            if self.stacks_shown { ui.preferred_size_free(self.stack_list).pref.h.clamp(1, most / 2) } else { 0 };
        let h = ui.preferred_size_free(self.list).pref.h.clamp(1, most - stacks_h);
        ui.set_layout_params(self.scroll, Some(LayoutParams::fixed(w, h)));
        ui.set_layout_params(self.stack_scroll, Some(LayoutParams::fixed(w, stacks_h.max(1))));
        show(ui, self.stack_scroll, self.stacks_shown);
        build::refit(ui, self.parts);
        self.key = Some(key);
        true
    }

    /// Write its lines and rows, and set its buttons, as things are.
    fn fill(&mut self, ui: &mut Ui, game: &Game) {
        for child in ui.children(self.economy).to_vec() {
            ui.remove(child);
        }
        for line in &self.economy_lines {
            build::text(ui, self.economy, line);
        }
        for child in ui.children(self.list).to_vec() {
            ui.remove(child);
        }
        let space = dp(ui, SPACING_DP);
        let row = |ui: &mut Ui, list: WidgetId, id: &str, text: &str, w: i32| {
            let button = build::button(ui, list, id, text);
            ui.set_skin(button, TbId::new("ListRow"));
            build::set_width(ui, button, w);
            build::text_from_left(ui, button);
            button
        };
        // The cities.
        let states: Vec<CityState> =
            self.cities.iter().filter_map(|id| game.map.cities().get(*id)).map(CityState::of).collect();
        let at_work = states.iter().filter(|s| **s == CityState::Building).count();
        build::heading(ui, self.list, &format!("Cities ({}): {at_work} at work", self.cities.len()));
        if self.cities.is_empty() {
            build::text(ui, self.list, "You hold no city.");
        }
        for (i, (id, line)) in self.cities.iter().zip(&self.city_lines).enumerate() {
            let button = row(ui, self.list, &city_row_id(i), line, self.row_w);
            set_pressed(ui, button, self.chosen == Some(Chosen::City(*id)));
        }
        // What comes.
        build::gap(ui, self.list, space);
        build::heading(ui, self.list, &format!("Coming up, the next {SHOWN_ROUNDS} rounds"));
        if self.coming.is_empty() {
            build::text(ui, self.list, "Nothing.");
        }
        for (heading, line) in &self.coming {
            let text = build::text(ui, self.list, if *heading { line.clone() } else { format!("    {line}") }.as_str());
            if *heading {
                ui.set_skin(text, TbId::new("Heading"));
            }
        }
        // The stacks, when shown, in their own list.
        let what = if self.stacks_shown { "hide them" } else { "show them, to cut the upkeep" };
        ui.set_text(self.stacks_toggle, &format!("Your stacks ({}): {what}", self.stacks.len()));
        set_pressed(ui, self.stacks_toggle, self.stacks_shown);
        for child in ui.children(self.stack_list).to_vec() {
            ui.remove(child);
        }
        if self.stacks_shown {
            build::text(ui, self.stack_list, "The dearest first. Select one to send some of it home (X).");
            for (i, (stack, line)) in self.stacks.iter().zip(&self.stack_lines).enumerate() {
                let button = row(ui, self.stack_list, &stack_row_id(i), line, self.row_w);
                set_pressed(ui, button, self.chosen == Some(Chosen::Stack(stack.at, stack.defending)));
            }
        }
        // What can be done with what is chosen.
        let city = match self.chosen {
            Some(Chosen::City(id)) => game.map.cities().get(id),
            _ => None,
        };
        show(ui, self.city_actions, city.is_some());
        show(ui, self.stack_actions, matches!(self.chosen, Some(Chosen::Stack(..))));
        if let Some(city) = city {
            let points = game.rally_points(self.side);
            for (n, button) in self.point_buttons.iter().enumerate() {
                show(ui, *button, points[n].is_some());
                set_pressed(ui, *button, matches!(city.sends, Sends::Point { n: p, .. } if usize::from(p) == n));
                enable(ui, *button, self.acting);
            }
            show(ui, self.points_row, points.iter().any(Option::is_some));
            set_pressed(ui, self.city_buttons[1], city.resting);
            set_pressed(ui, self.city_buttons[2], city.sends == Sends::Stay);
            set_pressed(ui, self.city_buttons[3], city.sends == Sends::Defend);
            set_pressed(ui, self.to_city, self.sending == Some(city.id));
            for button in self.city_buttons[1..].iter().chain([&self.to_city]) {
                enable(ui, *button, self.acting);
            }
        }
        for button in &self.stack_buttons[1..] {
            enable(ui, *button, self.acting);
        }
        let prompt = match self.sending.and_then(|id| game.map.cities().get(id)) {
            Some(city) => {
                format!("Where do {}'s new units go? Click that city's row (or To a city... again).", city.name)
            }
            None => String::new(),
        };
        ui.set_text(self.prompt, &prompt);
        show(ui, self.prompt, !prompt.is_empty());
        enable(ui, self.stop_all, self.acting && !self.cities.is_empty());
        let stopping = self.stoppable(game);
        let question = if stopping == 1 {
            "Stop your 1 city at work?".to_string()
        } else {
            format!("Stop all {stopping} cities at work?")
        };
        ui.set_text(self.confirm_text, &question);
        show(ui, self.confirm, self.asking);
    }

    /// How many of the side's cities Stop all would stop.
    fn stoppable(&self, game: &Game) -> usize {
        self.cities
            .iter()
            .filter_map(|id| game.map.cities().get(*id))
            .filter(|c| c.production().is_some() || !c.queue.is_empty())
            .count()
    }

    /// The map: the side's cities as known, its rally points flagged, the
    /// cities at no work or waiting ringed in orange, the chosen city in
    /// gold, and a line to where it sends its units.
    fn draw_map(&mut self, ui: &mut Ui, game: &Game, sight: &Sight) {
        let map = &game.map;
        let flags: Vec<Hex> = game.rally_points(self.side).iter().flatten().map(|p| p.at).collect();
        let warned: Vec<Hex> = self
            .cities
            .iter()
            .filter_map(|id| map.cities().get(*id))
            .filter(|c| matches!(CityState::of(c), CityState::Idle | CityState::Waiting))
            .map(|c| c.pos)
            .collect();
        let chosen = match self.chosen {
            Some(Chosen::City(id)) => map.cities().get(id),
            _ => None,
        };
        let ringed = match self.chosen {
            Some(Chosen::City(_)) => chosen.map(|c| c.pos),
            Some(Chosen::Stack(at, _)) => Some(at),
            None => None,
        };
        let line = chosen.filter(|c| c.production().is_some()).and_then(|c| {
            let to = match &c.sends {
                Sends::Route(route) => route.last().copied(),
                Sends::Point { n, .. } => game.rally_point(self.side, *n).map(|p| p.at),
                Sends::Stay | Sends::Defend => None,
            };
            to.map(|to| (c.pos, to))
        });
        let marks = Marks { ringed, flags, warned, line };
        let wanted = (map.terrain_revision(), marks);
        if self.shows.as_ref() == Some(&wanted) {
            return;
        }
        let size = ui.layout_params(self.picture).map_or((0, 0), |lp| (lp.pref_w.unwrap_or(0), lp.pref_h.unwrap_or(0)));
        let image = minimap_marked(map, size.0, size.1, &wanted.1, sight);
        self.made += 1;
        let handle = ui.image_from_pixels(&format!("production's map {}", self.made), image.size, &image.rgba);
        ui.with_widget::<ImageWidget, _>(self.picture, |w, cx| w.set_image(cx, handle.clone()));
        self.image = handle;
        self.shows = Some(wanted);
    }

    /// True if `id` is one of its buttons (the session hands it the click).
    pub fn owns(&self, id: TbId) -> bool {
        let fixed = [
            PROD_STACKS_ID,
            PROD_OPEN_ID,
            PROD_STOP_ID,
            PROD_STAY_ID,
            PROD_DEFEND_ID,
            PROD_TO_CITY_ID,
            PROD_SELECT_ID,
            PROD_DISBAND_ID,
            PROD_STOP_ALL_ID,
            PROD_STOP_ALL_YES_ID,
            PROD_STOP_ALL_NO_ID,
        ];
        fixed.iter().any(|f| id == TbId::new(f))
            || (0..self.cities.len()).any(|i| id == TbId::new(&city_row_id(i)))
            || (0..self.stacks.len()).any(|i| id == TbId::new(&stack_row_id(i)))
            || (0..RALLY_POINTS).any(|n| id == TbId::new(&point_id(n)))
    }

    /// A click on one of its buttons: what is chosen, shown and asked it
    /// keeps itself (and is laid out anew); what it asks of the game it
    /// returns.
    pub fn click(&mut self, game: &Game, id: TbId) -> Option<ProductionClick> {
        let is = |name: &str| id == TbId::new(name);
        // Anything the window does itself is shown at the next refresh.
        self.key = None;
        if let Some(i) = (0..self.cities.len()).find(|i| id == TbId::new(&city_row_id(*i))) {
            let city = self.cities[i];
            if let Some(from) = self.sending {
                self.sending = None;
                if from != city {
                    let to = game.map.cities().get(city)?.pos;
                    self.chosen = Some(Chosen::City(from));
                    return Some(ProductionClick::SendTo { city: from, to });
                }
                return None;
            }
            self.chosen = Some(Chosen::City(city));
            return None;
        }
        if let Some(i) = (0..self.stacks.len()).find(|i| id == TbId::new(&stack_row_id(*i))) {
            let stack = &self.stacks[i];
            self.chosen = Some(Chosen::Stack(stack.at, stack.defending));
            self.sending = None;
            return None;
        }
        if is(PROD_STACKS_ID) {
            self.stacks_shown = !self.stacks_shown;
            if !self.stacks_shown && matches!(self.chosen, Some(Chosen::Stack(..))) {
                self.chosen = None;
            }
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
            return None;
        }
        let Some(Chosen::City(city)) = self.chosen else { return None };
        if is(PROD_TO_CITY_ID) {
            self.sending = if self.sending == Some(city) || !self.acting { None } else { Some(city) };
            return None;
        }
        self.sending = None;
        if let Some(n) = (0..RALLY_POINTS).find(|n| id == TbId::new(&point_id(*n))) {
            return Some(ProductionClick::City(CityOrder::ToPoint(city, n as u8)));
        }
        let order = if is(PROD_OPEN_ID) {
            CityOrder::Details(city)
        } else if is(PROD_STOP_ID) {
            CityOrder::SetProduction(city, None)
        } else if is(PROD_STAY_ID) {
            CityOrder::NoRally(city)
        } else if is(PROD_DEFEND_ID) {
            CityOrder::DefendNew(city)
        } else {
            return None;
        };
        Some(ProductionClick::City(order))
    }

    /// The side whose production it shows.
    pub fn side(&self) -> Side {
        self.side
    }

    /// Its cities, in the order of their rows, and what the rows say.
    pub fn cities(&self) -> &[CityId] {
        &self.cities
    }

    pub fn city_lines(&self) -> &[String] {
        &self.city_lines
    }

    /// What it says of the gold.
    pub fn economy_lines(&self) -> &[String] {
        &self.economy_lines
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

    /// The city chosen, if a city is.
    pub fn chosen_city(&self) -> Option<CityId> {
        match self.chosen {
            Some(Chosen::City(id)) => Some(id),
            _ => None,
        }
    }

    /// The city whose units To a city... is sending, while it waits for
    /// the city they go to.
    pub fn sending(&self) -> Option<CityId> {
        self.sending
    }

    /// True while Stop all asks.
    pub fn asking(&self) -> bool {
        self.asking
    }

    pub fn is_open(&self, ui: &Ui) -> bool {
        build::is_open(ui, self.parts.window)
    }

    pub fn close(self, ui: &mut Ui) {
        build::close(ui, self.parts.window);
    }
}
