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
/// pressed), and how far it has got. Only the city's holder, if that is the
/// player, can press them.
pub struct CityWindow {
    parts: WindowParts,
    city: CityId,
    summary: Text,
    garrison: Text,
    progress: Text,
    note: Text,
    /// The buttons for what it builds, and what each sets.
    builds: Vec<(WidgetId, Option<usize>)>,
}

impl CityWindow {
    /// Open a city's window on `parent` (the root window), placed in `area`
    /// at `align` (see `build::place`). `None` if there is no such city.
    pub fn open(
        ui: &mut Ui,
        parent: WidgetId,
        map: &Map,
        id: CityId,
        player: Side,
        (area, align): (Rect, (f32, f32)),
    ) -> Option<Self> {
        let city = map.cities().get(id)?;
        let parts = build::window(ui, parent, CITY_WINDOW_ID, &city.name);
        let content = parts.content;
        build::heading(ui, content, &city.name);
        let summary = Text::new(build::text(ui, content, ""));
        let garrison = Text::new(build::text(ui, content, ""));
        let space = dp(ui, SPACING_DP);
        build::gap(ui, content, space);
        build::heading(ui, content, "What it builds");
        let options = build::column(ui, content, dp(ui, 2));
        let group = TbId::new("production");
        let mut builds = vec![(build::button(ui, options, BUILD_NOTHING_ID, "Nothing"), None)];
        for (i, prod) in city.units.iter().enumerate() {
            builds.push((build::button(ui, options, &build_id(i), &info::production_option(map, prod)), Some(i)));
        }
        // As wide as the widest, one under the other, and one of them pressed.
        let width = builds.iter().map(|(b, _)| ui.preferred_size_free(*b).pref.w).max().unwrap_or(0);
        for (button, _) in &builds {
            build::set_width(ui, *button, width);
            ui.set_group_id(*button, group);
        }
        let progress = Text::new(build::text(ui, content, ""));
        let note = Text::new(build::text(ui, content, ""));
        build::gap(ui, content, space);
        build::button(ui, content, CITY_CLOSE_ID, "Close");
        let mut window = CityWindow { parts, city: id, summary, garrison, progress, note, builds };
        window.fill(ui, map, player);
        build::place(ui, parts, area, align);
        Some(window)
    }

    /// The city it is about.
    pub fn city(&self) -> CityId {
        self.city
    }

    /// True while it is open (its title bar's button closes it too).
    pub fn is_open(&self, ui: &Ui) -> bool {
        build::is_open(ui, self.parts.window)
    }

    pub fn close(&self, ui: &mut Ui) {
        build::close(ui, self.parts.window);
    }

    /// Bring it up to date with the city. False if the city is gone.
    pub fn refresh(&mut self, ui: &mut Ui, map: &Map, player: Side) -> bool {
        if map.cities().get(self.city).is_none() {
            return false;
        }
        if self.fill(ui, map, player) {
            build::refit(ui, self.parts);
        }
        true
    }

    /// Write what it says; true if a line changed.
    fn fill(&mut self, ui: &mut Ui, map: &Map, player: Side) -> bool {
        let Some(city) = map.cities().get(self.city) else { return false };
        let mine = city.side == player && city.side != Side::Grey;
        let note = match (mine, city.side) {
            (true, _) => String::new(),
            (false, Side::Grey) => "Nobody's: nobody sets what it builds.".to_string(),
            (false, side) => format!("Only {} sets what it builds.", side_name(side)),
        };
        let mut changed = self.summary.set(ui, &info::city_summary(city));
        changed |= self.garrison.set(ui, &info::garrison_line(map, city));
        changed |= self.progress.set(ui, &info::progress_line(map, city));
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

    /// What a click on one of its buttons asks for (its close button is the
    /// caller's to handle).
    pub fn click(&self, id: TbId) -> Option<CityOrder> {
        let what = if id == TbId::new(BUILD_NOTHING_ID) {
            None
        } else {
            Some((0..self.builds.len()).find(|i| id == TbId::new(&build_id(*i)))?)
        };
        Some(CityOrder::SetProduction(self.city, what))
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
    /// What the picture shows: the chosen city, and the terrain as it was.
    shows: Option<(Option<CityId>, u64)>,
    /// The picture, held so that it stays loaded; and how many have been made.
    image: ImageHandle,
    made: u64,
    details: WidgetId,
    goto: WidgetId,
}

impl CityList {
    /// Open the list of every city, or of `player`'s (`own`), on `parent`
    /// (the root window), placed in `area` at `align` (see `build::place`).
    pub fn open(
        ui: &mut Ui,
        parent: WidgetId,
        map: &Map,
        own: bool,
        player: Side,
        (area, align): (Rect, (f32, f32)),
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
        let cities = map.cities().iter().filter(|city| !own || city.side == player);
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
        list.refresh(ui, map);
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
    pub fn choose(&mut self, ui: &mut Ui, map: &Map, id: CityId) {
        if self.rows.iter().any(|(_, city)| *city == id) {
            self.chosen = Some(id);
            self.refresh(ui, map);
        }
    }

    /// Bring it up to date with the map: what the rows say, which is marked,
    /// the picture.
    pub fn refresh(&mut self, ui: &mut Ui, map: &Map) {
        for (text, id) in &mut self.rows {
            if let Some(city) = map.cities().get(*id) {
                text.set(ui, &info::city_row(map, city, self.own));
            }
            set_pressed(ui, text.widget, self.chosen == Some(*id));
        }
        let chosen = self.chosen.and_then(|id| map.cities().get(id));
        let line = chosen.map_or("Choose a city to see where it is.".to_string(), |city| {
            format!("{}: {}", city.name, info::holder(city.side).to_lowercase())
        });
        self.chosen_line.set(ui, &line);
        for button in [self.details, self.goto] {
            if ui.node(button).is_some_and(|n| n.is_disabled() == chosen.is_some()) {
                ui.set_disabled(button, chosen.is_none());
            }
        }
        // A new picture when the chosen city or the terrain changed.
        let wanted = (self.chosen, map.terrain_revision());
        if self.shows != Some(wanted) {
            let size =
                ui.layout_params(self.picture).map_or((0, 0), |lp| (lp.pref_w.unwrap_or(0), lp.pref_h.unwrap_or(0)));
            let image = minimap_image(map, size.0, size.1, chosen.map(|city| city.pos));
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
    pub fn click(&mut self, ui: &mut Ui, map: &Map, id: TbId) -> Option<CityOrder> {
        if let Some(i) = (0..self.rows.len()).find(|i| id == TbId::new(&row_id(*i))) {
            let city = self.rows[i].1;
            self.choose(ui, map, city);
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
