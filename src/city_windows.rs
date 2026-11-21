//! The game's windows about cities: a city in full, where its holder sets
//! what it builds; and a list of cities (all of them, or the player's) with a
//! minimap that shows where the chosen one is, from which a city is opened in
//! full or gone to on the map. They are built from code (`gui::build`); the
//! session hands them the clicks on their buttons and does what they ask
//! (`CityOrder`).
//!
//! Not a port: the C++ had no windows.

use turbo_ferret::widgets::{ImageWidget, ScrollContainer, ScrollMode};
use turbo_ferret::{ImageHandle, LayoutParams, Rect, TbId, Ui, Visibility, WidgetId};

use crate::game::Sight;
use crate::gui::build::{self, WindowParts};
use crate::info;
use crate::render::minimap_image;
use crate::state::scenario::side_name;
use crate::state::{CityId, Map, Side};

/// A city's window, its buttons for what it builds (`build_0` ... and
/// nothing), and its close button.
pub const CITY_WINDOW_ID: &str = "city_window";
pub const BUILD_NOTHING_ID: &str = "build_none";
pub const CITY_CLOSE_ID: &str = "city_close";
/// Where its new units go: a hex to pick on the map, or nowhere (they stay).
pub const CITY_RALLY_ID: &str = "city_rally";
pub const CITY_NO_RALLY_ID: &str = "city_no_rally";
pub const CITY_WALLS_ID: &str = "city_walls";
/// The list of cities, its rows (`city_row_0` ...) and its buttons.
pub const LIST_WINDOW_ID: &str = "cities_window";
pub const LIST_DETAILS_ID: &str = "cities_details";
pub const LIST_GOTO_ID: &str = "cities_goto";
pub const LIST_CLOSE_ID: &str = "cities_close";

/// The button that sets a city to build the `i`th of what it can.
pub fn build_id(i: usize) -> String {
    format!("build_{i}")
}

/// The `i`th row of the list of cities.
pub fn row_id(i: usize) -> String {
    format!("city_row_{i}")
}

/// The lines of the details of what a city can build (`info::production_details`).
pub const DETAIL_LINES: usize = 6;
/// The size of a unit's icon on its button, in dp.
const OPTION_ICON_DP: i32 = 28;

/// The resource a button for what a city can build is made from: the unit's
/// icon and a line of text beside it.
fn option_resource(id: &str) -> String {
    format!(
        "TBButton: id: {id}, is-focusable: 0\n\
         \tTBLayout: axis: x, spacing: 6, position: center\n\
         \t\tTBImageWidget: id: {id}_icon\n\
         \t\tTBTextField: id: {id}_text, text-align: left\n"
    )
}

/// The list's size and its minimap's, and the space between things, in dp.
const LIST_DP: (i32, i32) = (200, 240);
const MINIMAP_DP: (i32, i32) = (220, 170);
const SPACING_DP: i32 = 8;
/// Room for the list's scroll bar, in dp.
const SCROLL_BAR_DP: i32 = 18;

/// What a click in a city's window asks of the game.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum CityOrder {
    /// Set what the city builds: an index into what it can build, or nothing.
    SetProduction(CityId, Option<usize>),
    /// Open the city's own window.
    Details(CityId),
    /// Show the city on the map, and make its hex the selected one.
    GoTo(CityId),
    /// Pick on the map where the city sends its new units.
    PickRally(CityId),
    /// The city's new units stay in it.
    NoRally(CityId),
    /// Raise the city's walls a level (`Order::Walls`).
    RaiseWalls(CityId),
}

/// A line of text, with what it says now (so that it is set only on a change).
struct Text {
    widget: WidgetId,
    text: String,
}

impl Text {
    fn new(widget: WidgetId) -> Text {
        Text { widget, text: String::new() }
    }

    /// Say `text`. True if that was a change.
    fn set(&mut self, ui: &mut Ui, text: &str) -> bool {
        if self.text == text {
            return false;
        }
        ui.set_text(self.widget, text);
        self.text = text.to_string();
        true
    }
}

fn dp(ui: &Ui, dp: i32) -> i32 {
    ui.dimension_converter().dp_to_px(dp)
}

fn set_pressed(ui: &mut Ui, button: WidgetId, pressed: bool) {
    let value = i32::from(pressed);
    if ui.value(button).as_int() != value {
        ui.set_value(button, value);
    }
}

/// A side's name with a capital: "Red".
fn title_case(side: Side) -> String {
    let name = side_name(side);
    let mut chars = name.chars();
    chars.next().map_or(String::new(), |first| first.to_uppercase().chain(chars).collect())
}

/// A city in full: its name, holder, walls and place, its garrison, what it
/// can build (a button each, and one for nothing: the one it builds is
/// pressed), how far it has got, and where its new units go. Only the city's
/// holder, if that is the player, can press them. Built into a window
/// ([`CityWindow`]) or, smaller, into the sidebar ([`CityPanel`]); the ids
/// of its buttons start with its prefix, so that both can be there at once.
pub struct CityView {
    prefix: &'static str,
    compact: bool,
    city: CityId,
    summary: Text,
    income: Text,
    garrison: Text,
    progress: Text,
    note: Text,
    /// All about the option under the pointer (or the one it builds); none
    /// in the sidebar (the session's box over the pointer says it there).
    details: Vec<Text>,
    /// Where its new units go (in the sidebar, on two lines).
    rally: Text,
    rally_to: Option<Text>,
    rally_buttons: [WidgetId; 2],
    /// What its holder alone knows (what it builds, where its units go):
    /// shown or not, as much as the one looking knows (`info::Known`).
    private: Vec<WidgetId>,
    known: info::Known,
    /// The buttons for what it builds, and what each sets.
    builds: Vec<(WidgetId, Option<usize>)>,
}

impl CityView {
    /// Build the city's view into `content`, a column. `icons` are the
    /// pictures of what it can build, in its order; `compact` is the
    /// sidebar's form. `None` if there is no such city.
    #[allow(clippy::too_many_arguments)]
    fn build(
        ui: &mut Ui,
        content: WidgetId,
        map: &Map,
        id: CityId,
        known: info::Known,
        icons: &[ImageHandle],
        (prefix, compact): (&'static str, bool),
    ) -> Option<CityView> {
        let city = map.cities().get(id)?;
        let pid = |name: &str| format!("{prefix}{name}");
        build::heading(ui, content, &city.name);
        let summary = Text::new(build::text(ui, content, ""));
        let income = Text::new(build::text(ui, content, ""));
        let garrison = Text::new(build::text(ui, content, ""));
        let space = dp(ui, SPACING_DP);
        build::gap(ui, content, space);
        let heading = build::heading(ui, content, if compact { "Builds" } else { "What it builds" });
        let options = build::column(ui, content, dp(ui, 2));
        let group = TbId::new(&pid("production"));
        let mut builds = vec![(build::button(ui, options, &pid(BUILD_NOTHING_ID), "Nothing"), None)];
        let icon_size = dp(ui, OPTION_ICON_DP);
        for (i, prod) in city.units.iter().enumerate() {
            // The unit's icon, its name, strength and moves.
            let id = pid(&build_id(i));
            if ui.inflate(options, &option_resource(&id)).is_err() {
                continue;
            }
            let found = |ui: &Ui, name: &str| ui.find(options, TbId::new(name));
            let (Some(button), Some(icon), Some(text)) =
                (found(ui, &id), found(ui, &format!("{id}_icon")), found(ui, &format!("{id}_text")))
            else {
                continue;
            };
            ui.set_layout_params(icon, Some(LayoutParams::fixed(icon_size, icon_size)));
            if let Some(handle) = icons.get(i) {
                let handle = handle.clone();
                ui.with_widget::<ImageWidget, _>(icon, |w, cx| w.set_image(cx, handle));
            }
            ui.set_text(text, &info::production_button(map, prod));
            builds.push((button, Some(i)));
        }
        // As wide as the widest, one under the other, and one of them pressed.
        let width = builds.iter().map(|(b, _)| ui.preferred_size_free(*b).pref.w).max().unwrap_or(0);
        for (button, _) in &builds {
            build::set_width(ui, *button, width);
            ui.set_group_id(*button, group);
        }
        // All about one of them: the one under the pointer.
        let mut details = Vec::new();
        if !compact {
            build::gap(ui, content, dp(ui, 4));
            details = (0..DETAIL_LINES).map(|_| Text::new(build::text(ui, content, ""))).collect();
            // As wide as the widest they will say, so that the window does
            // not change its size as the pointer goes from one option to
            // another.
            if let Some(first) = details.first() {
                let mut widest = 0;
                let hint = ["Point at one: all about it shows here.".to_string()];
                for line in city.units.iter().flat_map(|p| info::production_details(map, p, city.trains)).chain(hint) {
                    ui.set_text(first.widget, &line);
                    widest = widest.max(ui.preferred_size_free(first.widget).pref.w);
                }
                ui.set_text(first.widget, "");
                for line in &details {
                    let lp = LayoutParams { min_w: Some(widest), ..LayoutParams::default() };
                    ui.set_layout_params(line.widget, Some(lp));
                }
            }
        }
        build::gap(ui, content, dp(ui, 4));
        let progress = Text::new(build::text(ui, content, ""));
        // Where its new units go.
        let rally_gap = build::gap(ui, content, space);
        let rally = Text::new(build::text(ui, content, ""));
        let rally_to = compact.then(|| Text::new(build::text(ui, content, "")));
        let rally_row = build::row(ui, content, space);
        let (send, keep) =
            if compact { ("Send to...", "Keep here") } else { ("Send new units to...", "Keep them here") };
        let send = build::button(ui, rally_row, &pid(CITY_RALLY_ID), send);
        let keep = build::button(ui, rally_row, &pid(CITY_NO_RALLY_ID), keep);
        let note = Text::new(build::text(ui, content, ""));
        let private = [heading, options, rally_gap, rally.widget, rally_row]
            .into_iter()
            .chain(rally_to.iter().map(|t| t.widget))
            .chain(details.iter().map(|d: &Text| d.widget))
            .collect();
        Some(CityView {
            prefix,
            compact,
            city: id,
            summary,
            income,
            garrison,
            progress,
            note,
            details,
            rally,
            rally_to,
            rally_buttons: [send, keep],
            private,
            known,
            builds,
        })
    }

    /// Which of what it can build the pointer is on, if any.
    fn hovered(&self, ui: &Ui) -> Option<usize> {
        let over = ui.hovered()?;
        self.builds.iter().find(|(button, _)| ui.is_ancestor_of(*button, over)).and_then(|(_, what)| *what)
    }

    /// The button of what it can build that the pointer is on, and which it
    /// is.
    fn hovered_button(&self, ui: &Ui) -> Option<(WidgetId, usize)> {
        let over = ui.hovered()?;
        self.builds
            .iter()
            .find(|(button, _)| ui.is_ancestor_of(*button, over))
            .and_then(|(b, what)| Some((*b, (*what)?)))
    }

    /// Write what it says; true if a line changed.
    fn fill(&mut self, ui: &mut Ui, map: &Map, player: Side) -> bool {
        let Some(city) = map.cities().get(self.city) else { return false };
        let mine = city.side == player && city.side != Side::Grey;
        let known = self.known;
        let note = match (known, mine, city.side) {
            (info::Known::Fog(None), _, _) => "Not seen: who holds it, who is in it.".to_string(),
            (info::Known::Fog(Some((side, round))), _, _) => format!(
                "Not seen since round {round}, when it was {}: who is in it.",
                if side == Side::Grey { "nobody's".to_string() } else { format!("{}'s", side_name(side)) }
            ),
            (info::Known::Outside, _, Side::Grey) => "Nobody's: what it builds is not known.".to_string(),
            (info::Known::Outside, _, side) => format!("Only {} knows what it builds.", side_name(side)),
            (_, true, _) => String::new(),
            (_, false, Side::Grey) => "Nobody's: nobody sets what it builds.".to_string(),
            (_, false, side) => format!("Only {} sets what it builds.", side_name(side)),
        };
        let all = known == info::Known::All;
        let summary = match known {
            info::Known::Fog(_) => {
                format!("Not seen, walls {}, at row {}, column {}", city.walls, city.pos.row, city.pos.col)
            }
            _ if self.compact => format!("Walls {}, taxes {}", city.walls, city.income),
            _ => info::city_summary(city),
        };
        let units = map.units_at(city.pos).len();
        let garrison = match known {
            info::Known::All if !self.compact => info::garrison_line(map, city),
            info::Known::All | info::Known::Outside => match units {
                0 => "No garrison".to_string(),
                1 => "Garrison: 1 unit".to_string(),
                n => format!("Garrison: {n} units"),
            },
            info::Known::Fog(_) => String::new(),
        };
        let progress = match (all, self.compact) {
            (false, _) => String::new(),
            (true, true) => info::progress_short(map, city),
            (true, false) => info::progress_line(map, city),
        };
        let mut changed = self.summary.set(ui, &summary);
        changed |= self.income.set(ui, if self.compact { String::new() } else { info::income_line(city) }.as_str());
        changed |= self.garrison.set(ui, &garrison);
        changed |= self.progress.set(ui, &progress);
        // A line with nothing to say takes no room.
        for line in [&self.income, &self.garrison, &self.progress] {
            let visibility = if line.text.is_empty() { Visibility::Gone } else { Visibility::Visible };
            if ui.node(line.widget).is_some_and(|n| n.visibility() != visibility) {
                ui.set_visibility(line.widget, visibility);
                changed = true;
            }
        }
        let visibility = if all { Visibility::Visible } else { Visibility::Gone };
        for widget in &self.private {
            if ui.node(*widget).is_some_and(|n| n.visibility() != visibility) {
                ui.set_visibility(*widget, visibility);
                changed = true;
            }
        }
        // All about what is under the pointer; else about what it builds.
        let shown = self.hovered(ui).or(city.building);
        let details = match shown.and_then(|i| city.units.get(i)) {
            Some(prod) => info::production_details(map, prod, city.trains),
            None if city.units.is_empty() => Vec::new(),
            None => vec!["Point at one: all about it shows here.".to_string()],
        };
        for (i, line) in self.details.iter_mut().enumerate() {
            changed |= line.set(ui, details.get(i).map_or("", String::as_str));
        }
        let (rally, rally_to) = match (city.rally, self.compact) {
            (Some(to), false) => (format!("New units go to {}.", info::place(map, to)), String::new()),
            (Some(to), true) => ("New units go to".to_string(), info::place(map, to)),
            (None, false) => ("New units stay in the city.".to_string(), String::new()),
            (None, true) => ("New units stay here".to_string(), String::new()),
        };
        changed |= self.rally.set(ui, &rally);
        if let Some(line) = &mut self.rally_to {
            changed |= line.set(ui, &rally_to);
            let visibility = if all && !rally_to.is_empty() { Visibility::Visible } else { Visibility::Gone };
            if ui.node(line.widget).is_some_and(|n| n.visibility() != visibility) {
                ui.set_visibility(line.widget, visibility);
                changed = true;
            }
        }
        for button in self.rally_buttons {
            if ui.node(button).is_some_and(|n| n.is_disabled() == mine) {
                ui.set_disabled(button, !mine);
            }
        }
        changed |= self.note.set(ui, &note);
        let visibility = if note.is_empty() { Visibility::Gone } else { Visibility::Visible };
        if ui.node(self.note.widget).is_some_and(|n| n.visibility() != visibility) {
            ui.set_visibility(self.note.widget, visibility);
            changed = true;
        }
        for (button, what) in &self.builds {
            set_pressed(ui, *button, city.building == *what);
            if ui.node(*button).is_some_and(|n| n.is_disabled() == mine) {
                ui.set_disabled(*button, !mine);
            }
        }
        changed
    }

    /// What a click on one of its buttons asks for.
    fn click(&self, id: TbId) -> Option<CityOrder> {
        let is = |name: &str| id == TbId::new(&format!("{}{name}", self.prefix));
        if is(CITY_RALLY_ID) {
            return Some(CityOrder::PickRally(self.city));
        }
        if is(CITY_NO_RALLY_ID) {
            return Some(CityOrder::NoRally(self.city));
        }
        if is(BUILD_NOTHING_ID) {
            return Some(CityOrder::SetProduction(self.city, None));
        }
        let what = (0..self.builds.len()).find(|i| is(&build_id(*i)))?;
        Some(CityOrder::SetProduction(self.city, Some(what)))
    }
}

/// A city's window: its view, a button that raises its walls (its
/// holder's, if that is the player), and one that closes it.
pub struct CityWindow {
    parts: WindowParts,
    view: CityView,
    walls: WidgetId,
    /// What the walls' button says, and whether it is pressable.
    walls_state: (String, bool, bool),
}

impl CityWindow {
    /// Its window (the session closes the one on top first).
    pub fn window(&self) -> WidgetId {
        self.parts.window
    }

    /// Open a city's window on `parent` (the root window), placed in `area`
    /// at `align` (see `build::place`). `icons` are the pictures of what it
    /// can build, in its order. `None` if there is no such city.
    pub fn open(
        ui: &mut Ui,
        parent: WidgetId,
        map: &Map,
        id: CityId,
        (player, known): (Side, info::Known),
        icons: &[ImageHandle],
        (area, align): (Rect, (f32, f32)),
    ) -> Option<Self> {
        let city = map.cities().get(id)?;
        let parts = build::window(ui, parent, CITY_WINDOW_ID, &city.name);
        let Some(view) = CityView::build(ui, parts.content, map, id, known, icons, ("", false)) else {
            build::close(ui, parts.window);
            return None;
        };
        let space = dp(ui, SPACING_DP);
        build::gap(ui, parts.content, space);
        // Close, and beside it (its holder's) the walls: a row, no taller
        // than the button (a city with nine kinds to build fits 768 high).
        let row = build::row(ui, parts.content, space * 2);
        build::button(ui, row, CITY_CLOSE_ID, "Close");
        let walls = build::button(ui, row, CITY_WALLS_ID, "");
        ui.set_visibility(walls, Visibility::Gone);
        let mut window = CityWindow { parts, view, walls, walls_state: (String::new(), false, false) };
        window.view.fill(ui, map, player);
        build::place(ui, parts, area, align);
        Some(window)
    }

    /// Say what raising the walls costs, if the player holds the city:
    /// `price` is the game's answer (`Game::walls_price`), the price or why
    /// not. At the cap, it says so; else it gives the price and can be
    /// pressed only if the game would raise them.
    pub fn set_walls(&mut self, ui: &mut Ui, map: &Map, player: Side, price: Result<u32, String>) {
        let Some(city) = map.cities().get(self.view.city) else { return };
        let shown = city.side == player && city.side != Side::Grey && self.view.known == info::Known::All;
        let text = if city.walls >= crate::state::WALLS_CAP {
            "The walls are as high as walls go".to_string()
        } else if price.as_ref().is_err_and(|why| why.contains("this turn")) {
            "The walls were raised this turn".to_string()
        } else {
            format!("Raise the walls to {}: {} gold", city.walls + 1, crate::state::wall_price(city.walls))
        };
        let state = (text, shown, price.is_ok());
        if state == self.walls_state {
            return;
        }
        ui.set_text(self.walls, &state.0);
        ui.set_visibility(self.walls, if state.1 { Visibility::Visible } else { Visibility::Gone });
        ui.set_disabled(self.walls, !state.2);
        self.walls_state = state;
        build::refit(ui, self.parts);
    }

    /// What the walls' button says, if it shows, and whether it can be pressed.
    pub fn walls(&self) -> Option<(&str, bool)> {
        self.walls_state.1.then_some((self.walls_state.0.as_str(), self.walls_state.2))
    }

    /// The city it is about.
    pub fn city(&self) -> CityId {
        self.view.city
    }

    /// Which of what it can build the pointer is on, if any.
    pub fn hovered(&self, ui: &Ui) -> Option<usize> {
        self.view.hovered(ui)
    }

    /// What the details under the options say.
    pub fn details(&self) -> Vec<&str> {
        self.view.details.iter().map(|t| t.text.as_str()).filter(|t| !t.is_empty()).collect()
    }

    /// True while it is open (its title bar's button closes it too).
    pub fn is_open(&self, ui: &Ui) -> bool {
        build::is_open(ui, self.parts.window)
    }

    pub fn close(&self, ui: &mut Ui) {
        build::close(ui, self.parts.window);
    }

    /// Bring it up to date with the city. False if the city is gone.
    pub fn refresh(&mut self, ui: &mut Ui, map: &Map, (player, known): (Side, info::Known)) -> bool {
        if map.cities().get(self.view.city).is_none() {
            return false;
        }
        self.view.known = known;
        if self.view.fill(ui, map, player) {
            build::refit(ui, self.parts);
        }
        true
    }

    /// What a click on one of its buttons asks for (its close button is the
    /// caller's to handle).
    pub fn click(&self, id: TbId) -> Option<CityOrder> {
        if id == TbId::new(CITY_WALLS_ID) {
            return Some(CityOrder::RaiseWalls(self.view.city));
        }
        self.view.click(id)
    }
}

/// The ids of the sidebar's city: its buttons' start with this, and the one
/// that opens its window.
pub const PANEL_PREFIX: &str = "panel_";
pub const PANEL_OPEN_ID: &str = "panel_city_open";

/// One of the player's cities in the sidebar: its view, smaller, and a
/// button that opens its window (where there is more room, and all about
/// what it can build).
pub struct CityPanel {
    root: WidgetId,
    view: CityView,
}

impl CityPanel {
    /// Build the city's panel into `parent` (a column of the sidebar).
    pub fn open(
        ui: &mut Ui,
        parent: WidgetId,
        map: &Map,
        id: CityId,
        (player, known): (Side, info::Known),
        icons: &[ImageHandle],
    ) -> Option<CityPanel> {
        let root = build::column(ui, parent, 0);
        let Some(view) = CityView::build(ui, root, map, id, known, icons, (PANEL_PREFIX, true)) else {
            ui.remove(root);
            return None;
        };
        build::gap(ui, root, dp(ui, SPACING_DP));
        build::button(ui, root, PANEL_OPEN_ID, "Open the city's window");
        let mut panel = CityPanel { root, view };
        panel.view.fill(ui, map, player);
        Some(panel)
    }

    /// The city it is about.
    pub fn city(&self) -> CityId {
        self.view.city
    }

    /// Take it out of the sidebar.
    pub fn close(self, ui: &mut Ui) {
        ui.remove(self.root);
    }

    /// Bring it up to date with the city. False if the city is gone.
    pub fn refresh(&mut self, ui: &mut Ui, map: &Map, (player, known): (Side, info::Known)) -> bool {
        if map.cities().get(self.view.city).is_none() {
            return false;
        }
        self.view.known = known;
        self.view.fill(ui, map, player);
        true
    }

    /// The button of what the city can build that the pointer is on, and
    /// which it is (the session says all about it in its box).
    pub fn hovered(&self, ui: &Ui) -> Option<(WidgetId, usize)> {
        self.view.hovered_button(ui)
    }

    /// What a click on one of its buttons asks for.
    pub fn click(&self, id: TbId) -> Option<CityOrder> {
        if id == TbId::new(PANEL_OPEN_ID) {
            return Some(CityOrder::Details(self.view.city));
        }
        self.view.click(id)
    }
}

/// A list of cities, every one or the player's, a row each (its name and its
/// holder, or what one's own builds), and beside it the map with the chosen
/// city ringed, and buttons to open the chosen one or to go to it.
pub struct CityList {
    parts: WindowParts,
    /// Only the player's cities.
    own: bool,
    player: Side,
    rows: Vec<(Text, CityId)>,
    chosen: Option<CityId>,
    chosen_line: Text,
    picture: WidgetId,
    /// What the picture shows: the chosen city, the terrain as it was, and
    /// who held each city known, as known.
    shows: Option<(Option<CityId>, u64, Vec<Option<Side>>)>,
    /// The picture, held so that it stays loaded; and how many have been made.
    image: ImageHandle,
    made: u64,
    details: WidgetId,
    goto: WidgetId,
}

impl CityList {
    /// Its window (the session closes the one on top first).
    pub fn window(&self) -> WidgetId {
        self.parts.window
    }

    /// Open the list of every city, or of `player`'s (`own`), on `parent`
    /// (the root window), placed in `area` at `align` (see `build::place`).
    pub fn open(
        ui: &mut Ui,
        parent: WidgetId,
        map: &Map,
        own: bool,
        player: Side,
        (area, align): (Rect, (f32, f32)),
        sight: &Sight,
    ) -> CityList {
        let title = if own { format!("{}'s cities", title_case(player)) } else { "All cities".to_string() };
        let parts = build::window(ui, parent, LIST_WINDOW_ID, &title);
        let space = dp(ui, SPACING_DP);
        let sides = build::row(ui, parts.content, space);

        // The list, scrolled when it is long.
        let (list_w, list_h) = (dp(ui, LIST_DP.0), dp(ui, LIST_DP.1));
        let scroll = ui.add(sides, ScrollContainer::new(ScrollMode::YAuto).with_adapt_content_size(true));
        ui.set_layout_params(scroll, Some(LayoutParams::fixed(list_w, list_h)));
        let column = build::column(ui, scroll, 0);
        let row_w = list_w - dp(ui, SCROLL_BAR_DP);
        let group = TbId::new("city_rows");
        let mut rows = Vec::new();
        // (Not those on land never seen: total fog of war.)
        let cities = map.cities().iter().filter(|city| (!own || city.side == player) && sight.knows(city.pos));
        for (i, city) in cities.enumerate() {
            let button = build::button(ui, column, &row_id(i), "");
            ui.set_skin(button, TbId::new("ListRow"));
            ui.set_group_id(button, group);
            build::set_width(ui, button, row_w);
            build::text_from_left(ui, button);
            rows.push((Text::new(button), city.id));
        }
        if rows.is_empty() {
            let none = if own { "You hold no city." } else { "This map has no city." };
            build::text(ui, column, none);
        }

        // The map, with buttons for the chosen city under it.
        let right = build::column(ui, sides, space);
        let picture = ui.add(right, ImageWidget::new());
        ui.set_layout_params(picture, Some(LayoutParams::fixed(dp(ui, MINIMAP_DP.0), dp(ui, MINIMAP_DP.1))));
        let chosen_line = Text::new(build::text(ui, right, ""));
        let buttons = build::row(ui, right, space);
        let details = build::button(ui, buttons, LIST_DETAILS_ID, "Details");
        let goto = build::button(ui, buttons, LIST_GOTO_ID, "Go to");
        build::gap(ui, parts.content, space);
        build::button(ui, parts.content, LIST_CLOSE_ID, "Close");

        let mut list = CityList {
            parts,
            own,
            player,
            rows,
            chosen: None,
            chosen_line,
            picture,
            shows: None,
            image: ImageHandle::default(),
            made: 0,
            details,
            goto,
        };
        list.refresh(ui, map, sight);
        build::place(ui, parts, area, align);
        list
    }

    /// True for the list of the player's own cities.
    pub fn own(&self) -> bool {
        self.own
    }

    /// The city chosen in it, if any.
    pub fn chosen(&self) -> Option<CityId> {
        self.chosen
    }

    /// The cities in it, in the order of its rows.
    pub fn cities(&self) -> Vec<CityId> {
        self.rows.iter().map(|(_, id)| *id).collect()
    }

    /// What its rows say.
    pub fn rows(&self) -> Vec<&str> {
        self.rows.iter().map(|(text, _)| text.text.as_str()).collect()
    }

    pub fn is_open(&self, ui: &Ui) -> bool {
        build::is_open(ui, self.parts.window)
    }

    pub fn close(&self, ui: &mut Ui) {
        build::close(ui, self.parts.window);
    }

    /// Choose a city of the list: its row is marked, and it is ringed on the map.
    pub fn choose(&mut self, ui: &mut Ui, map: &Map, id: CityId, sight: &Sight) {
        if self.rows.iter().any(|(_, city)| *city == id) {
            self.chosen = Some(id);
            self.refresh(ui, map, sight);
        }
    }

    /// Bring it up to date with the map: what the rows say, which is marked,
    /// the picture.
    pub fn refresh(&mut self, ui: &mut Ui, map: &Map, sight: &Sight) {
        for (text, id) in &mut self.rows {
            if let Some(city) = map.cities().get(*id) {
                let row = if sight.sees(city.pos) {
                    info::city_row(map, city, self.own)
                } else {
                    format!("{} (not seen{})", city.name, info::last_held(sight, city))
                };
                text.set(ui, &row);
            }
            set_pressed(ui, text.widget, self.chosen == Some(*id));
        }
        let chosen = self.chosen.and_then(|id| map.cities().get(id));
        let line = chosen.map_or("Choose a city to see where it is.".to_string(), |city| {
            let holder = if sight.sees(city.pos) {
                info::holder(city.side).to_lowercase()
            } else {
                format!("not seen{}", info::last_held(sight, city))
            };
            format!("{}: {holder}", city.name)
        });
        self.chosen_line.set(ui, &line);
        for button in [self.details, self.goto] {
            if ui.node(button).is_some_and(|n| n.is_disabled() == chosen.is_some()) {
                ui.set_disabled(button, chosen.is_none());
            }
        }
        // A new picture when the chosen city, the terrain, or what is seen of
        // the cities changed.
        let seen_cities: Vec<Option<Side>> =
            map.cities().iter().map(|c| sight.knows(c.pos).then(|| sight.holder(c))).collect();
        let wanted = (self.chosen, map.terrain_revision(), seen_cities);
        if self.shows.as_ref() != Some(&wanted) {
            let size =
                ui.layout_params(self.picture).map_or((0, 0), |lp| (lp.pref_w.unwrap_or(0), lp.pref_h.unwrap_or(0)));
            let image = minimap_image(map, size.0, size.1, chosen.map(|city| city.pos), sight);
            self.made += 1;
            let handle = ui.image_from_pixels(&format!("cities' minimap {}", self.made), image.size, &image.rgba);
            ui.with_widget::<ImageWidget, _>(self.picture, |w, cx| w.set_image(cx, handle.clone()));
            self.image = handle;
            self.shows = Some(wanted);
        }
    }

    /// What a click on one of its buttons asks of the game: a row chooses its
    /// city (which the list does itself), Details and Go to are about the
    /// chosen one. (Its close button is the caller's to handle.)
    pub fn click(&mut self, ui: &mut Ui, map: &Map, id: TbId, sight: &Sight) -> Option<CityOrder> {
        if let Some(i) = (0..self.rows.len()).find(|i| id == TbId::new(&row_id(*i))) {
            let city = self.rows[i].1;
            self.choose(ui, map, city, sight);
            return None;
        }
        let chosen = self.chosen?;
        if id == TbId::new(LIST_DETAILS_ID) {
            Some(CityOrder::Details(chosen))
        } else if id == TbId::new(LIST_GOTO_ID) {
            Some(CityOrder::GoTo(chosen))
        } else {
            None
        }
    }

    /// The side whose cities it lists (if it lists only one side's).
    pub fn player(&self) -> Side {
        self.player
    }
}
