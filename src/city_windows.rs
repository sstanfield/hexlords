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
use crate::orders::RALLY_POINTS;
use crate::render::minimap_image;
use crate::state::scenario::side_name;
use crate::state::{CityId, MAX_QUEUE, Map, QueueEnd, Sends, Side};

/// A city's window, its buttons for what it builds (`build_0` ... and
/// nothing), and its close button.
pub const CITY_WINDOW_ID: &str = "city_window";
pub const BUILD_NOTHING_ID: &str = "build_none";
pub const CITY_CLOSE_ID: &str = "city_close";
/// Where its new units go: a hex to pick on the map, nowhere (they stay),
/// nowhere (they defend it), one of the rally points (`point_id`); and
/// whether they seek a blessing on the way to the point.
pub const CITY_RALLY_ID: &str = "city_rally";
pub const CITY_NO_RALLY_ID: &str = "city_no_rally";
pub const CITY_DEFEND_ID: &str = "city_defend";
pub const CITY_BLESS_ID: &str = "city_bless";

/// The button that sends a city's new units to rally point `n`.
pub fn point_id(n: usize) -> String {
    format!("city_point_{n}")
}

/// The queue (the window's alone): the button beside the `i`th kind it can
/// build that adds one to the queue; the `i`th item's buttons (choose it,
/// one less, one more, take it out); what it does after the last (stop,
/// repeat from the chosen item, how many times).
pub fn queue_add_id(i: usize) -> String {
    format!("queue_add_{i}")
}
pub fn queue_item_id(i: usize) -> String {
    format!("queue_item_{i}")
}
pub fn queue_less_id(i: usize) -> String {
    format!("queue_less_{i}")
}
pub fn queue_more_id(i: usize) -> String {
    format!("queue_more_{i}")
}
pub fn queue_remove_id(i: usize) -> String {
    format!("queue_remove_{i}")
}
pub const QUEUE_STOP_ID: &str = "queue_stop";
pub const QUEUE_REPEAT_ID: &str = "queue_repeat";
pub const QUEUE_FEWER_ID: &str = "queue_fewer";
pub const QUEUE_MORE_TIMES_ID: &str = "queue_more_times";
/// The width of an item of the queue, in dp.
const QUEUE_ITEM_DP: i32 = 330;
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

/// The production panel's unit buttons: a box each, as a stack's under the
/// map (the owner's word, 2026-11-24: "The unit buttons should be like the
/// stack buttons at the bottom"): the unit's icon over its price and turns.
fn box_resource(id: &str) -> String {
    format!(
        "TBButton: id: {id}, skin: UnitSlot, is-focusable: 0\n\
         \tTBLayout: axis: y, spacing: 2\n\
         \t\tTBImageWidget: id: {id}_icon\n\
         \t\tTBTextField: id: {id}_text, text-align: center\n"
    )
}

/// How many boxes a row holds, a box's width and its icon's, in dp.
pub const BOXES_A_ROW: usize = 6;
const BOX_DP: i32 = 70;
const BOX_ICON_DP: i32 = 34;

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
    /// The city's new units stay in it and defend it.
    DefendNew(CityId),
    /// The city's new units go to the side's rally point `n`.
    ToPoint(CityId, u8),
    /// The city's new units, sent to a rally point, seek a blessing on the
    /// way, or no longer.
    ToggleBless(CityId),
    /// An item of the queue was chosen in the window (the window keeps it):
    /// what it builds and where its units go are set from then on (the
    /// `Item` orders).
    Chose(CityId),
    /// Add one of the `what`th kind it can build to the end of its queue.
    QueueAdd(CityId, usize),
    /// The production panel's unit buttons: the queue is the one way to
    /// build (the owner's word, 2026-11-24: "Pressing a unit button puts it
    /// in the queue. If it was empty the defaults are to repeat and keep
    /// here simulating the old no queue behaviour"). One of the `what`th
    /// kind at the end of the queue; a city with none starts one that
    /// repeats for ever, what it builds now (if anything) its first item.
    Enqueue(CityId, usize),
    /// The `i`th item: one more of it, or one less (`up`), or out.
    QueueCount(CityId, usize, bool),
    QueueRemove(CityId, usize),
    /// After the last item: stop; repeat from the `i`th; once more or once
    /// less (`up`; fewer than once is for ever).
    QueueStop(CityId),
    QueueRepeat(CityId, usize),
    QueueTimes(CityId, bool),
    /// The `i`th item builds the `what`th kind instead.
    ItemKind(CityId, usize, usize),
    /// Where the `i`th item's units go: they stay, they defend the city, to
    /// rally point `n`, a blessing on the way or not, a hex to pick.
    ItemStay(CityId, usize),
    ItemDefend(CityId, usize),
    ItemPoint(CityId, usize, u8),
    ItemBless(CityId, usize),
    ItemPick(CityId, usize),
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
    /// The production panel's form: the queue first, then the units it
    /// can build as boxes (`Enqueue`), then all about the one in focus.
    stacked: bool,
    /// The box the keyboard is on (the panel's), and the one shown so.
    focus: Option<usize>,
    focus_shown: Option<usize>,
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
    /// Send to..., Keep, Defend.
    rally_buttons: [WidgetId; 3],
    /// The rally points' buttons (shown for those that are set), their row,
    /// which are set; the box for a blessing on the way, and its row.
    point_buttons: Vec<WidgetId>,
    points_row: WidgetId,
    points: [bool; RALLY_POINTS],
    bless: WidgetId,
    bless_row: WidgetId,
    /// The queue (the window's alone): its heading and what it says when
    /// empty; a row for each item (its row, choose it, less, more, out);
    /// what it does after the last (its row, Stop, Repeat from here; the
    /// line that says it, once less, once more); the item chosen.
    queue_hint: Option<Text>,
    queue_rows: Vec<(WidgetId, Text, [WidgetId; 3], Text)>,
    queue_end: Option<(WidgetId, [WidgetId; 2])>,
    queue_times: Option<(WidgetId, Text)>,
    chosen: Option<usize>,
    /// The item its buttons are about now (the one chosen, or the one it
    /// builds), while it has a queue: the window's alone.
    about: Option<usize>,
    /// The buttons that add one to the queue.
    adds: Vec<WidgetId>,
    /// What its holder alone knows (what it builds, where its units go):
    /// shown or not, as much as the one looking knows (`info::Known`).
    private: Vec<WidgetId>,
    known: info::Known,
    /// The buttons for what it builds, and what each sets.
    builds: Vec<(WidgetId, Option<usize>)>,
}

impl CityView {
    /// Build the city's view into `content`, a column. `icons` are the
    /// pictures of what it can build, in its order; `stacked`: one column
    /// (in the production panel), not two side by side; `compact` is the
    /// sidebar's form. `None` if there is no such city.
    #[allow(clippy::too_many_arguments)]
    fn build(
        ui: &mut Ui,
        content: WidgetId,
        map: &Map,
        id: CityId,
        known: info::Known,
        icons: &[ImageHandle],
        (prefix, compact, stacked): (&'static str, bool, bool),
    ) -> Option<CityView> {
        let city = map.cities().get(id)?;
        let pid = |name: &str| format!("{prefix}{name}");
        build::heading(ui, content, &city.name);
        let summary = Text::new(build::text(ui, content, ""));
        let income = Text::new(build::text(ui, content, ""));
        let garrison = Text::new(build::text(ui, content, ""));
        let space = dp(ui, SPACING_DP);
        build::gap(ui, content, space);
        // In the window two columns: what it can build, and all about the
        // one under the pointer; its queue, and where its units go.
        let (left, right) = if compact {
            (content, content)
        } else if stacked {
            // (Made in this order, so shown so: the queue and where its
            // units go over the units to add.)
            let upper = build::column(ui, content, 0);
            let lower = build::column(ui, content, 0);
            (lower, upper)
        } else {
            let columns = build::top_row(ui, content, space * 2);
            (build::column(ui, columns, 0), build::column(ui, columns, 0))
        };
        let content = left;
        let heading = build::heading(
            ui,
            content,
            if compact {
                "Builds"
            } else if stacked {
                "Build (adds to the queue)"
            } else {
                "What it builds"
            },
        );
        let options = build::column(ui, content, dp(ui, 2));
        let group = TbId::new(&pid("production"));
        // (In the panel, Nothing is the queue's: over it, by the queue.)
        let nothing_parent = if stacked { right } else { options };
        let nothing_label = if stacked { "Build nothing" } else { "Nothing" };
        let mut builds = vec![(build::button(ui, nothing_parent, &pid(BUILD_NOTHING_ID), nothing_label), None)];
        let mut box_row = None;
        let icon_size = dp(ui, OPTION_ICON_DP);
        let mut adds = Vec::new();
        for (i, prod) in city.units.iter().enumerate() {
            let id = pid(&build_id(i));
            if stacked {
                // A box, as a stack's under the map: the unit's icon, and its
                // price and turns under it ("9g 2t"); rows of `BOXES_A_ROW`.
                if i % BOXES_A_ROW == 0 {
                    box_row = Some(build::row(ui, options, dp(ui, 4)));
                }
                let Some(row) = box_row else { continue };
                if ui.inflate(row, &box_resource(&id)).is_err() {
                    continue;
                }
                let found = |ui: &Ui, name: &str| ui.find(row, TbId::new(name));
                let (Some(button), Some(icon), Some(text)) =
                    (found(ui, &id), found(ui, &format!("{id}_icon")), found(ui, &format!("{id}_text")))
                else {
                    continue;
                };
                let size = dp(ui, BOX_ICON_DP);
                ui.set_layout_params(icon, Some(LayoutParams::fixed(size, size)));
                if let Some(handle) = icons.get(i) {
                    let handle = handle.clone();
                    ui.with_widget::<ImageWidget, _>(icon, |w, cx| w.set_image(cx, handle));
                }
                ui.set_text(text, &format!("{}g {}t", prod.cost, prod.time.max(1)));
                build::set_width(ui, button, dp(ui, BOX_DP));
                builds.push((button, Some(i)));
                continue;
            }
            // The unit's icon, its name, strength and moves; in the window, a
            // button beside it that adds one to the queue.
            let line = if compact { options } else { build::row(ui, options, dp(ui, 2)) };
            if ui.inflate(line, &option_resource(&id)).is_err() {
                continue;
            }
            if !compact {
                adds.push(build::button(ui, line, &queue_add_id(i), "+"));
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
        // (In the panel the boxes are as wide as each other already.)
        let width = builds.iter().map(|(b, _)| ui.preferred_size_free(*b).pref.w).max().unwrap_or(0);
        for (button, _) in &builds {
            if !stacked {
                build::set_width(ui, *button, width);
            }
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
        let content = right;
        // The queue.
        let (mut queue_hint, mut queue_rows, mut queue_end, mut queue_times) = (None, Vec::new(), None, None);
        let mut queue_widgets = Vec::new();
        if !compact {
            queue_widgets.push(build::heading(ui, content, "Queue"));
            let hint = build::text(ui, content, "");
            queue_hint = Some(Text::new(hint));
            let rows = build::column(ui, content, dp(ui, 2));
            queue_widgets.push(rows);
            for i in 0..MAX_QUEUE {
                // Over the item it goes back to, a line that says so (the
                // owner: "very easy to repeat from the wrong point").
                let mark = build::heading(ui, rows, "");
                ui.set_visibility(mark, Visibility::Gone);
                let row = build::row(ui, rows, dp(ui, 2));
                let item = build::button(ui, row, &queue_item_id(i), "");
                build::text_from_left(ui, item);
                build::set_width(ui, item, dp(ui, QUEUE_ITEM_DP));
                let less = build::button(ui, row, &queue_less_id(i), "-");
                let more = build::button(ui, row, &queue_more_id(i), "+");
                let out = build::button(ui, row, &queue_remove_id(i), "\u{d7}");
                ui.set_visibility(row, Visibility::Gone);
                queue_rows.push((row, Text::new(item), [less, more, out], Text::new(mark)));
            }
            let end = build::row(ui, content, dp(ui, 2));
            build::text(ui, end, "Then:");
            let stop = build::button(ui, end, QUEUE_STOP_ID, "Stop");
            let repeat = build::button(ui, end, QUEUE_REPEAT_ID, "Repeat from item 1");
            ui.set_visibility(end, Visibility::Gone);
            queue_end = Some((end, [stop, repeat]));
            let times = build::row(ui, content, dp(ui, 2));
            let words = Text::new(build::text(ui, times, ""));
            build::button(ui, times, QUEUE_FEWER_ID, "-");
            build::button(ui, times, QUEUE_MORE_TIMES_ID, "+");
            ui.set_visibility(times, Visibility::Gone);
            queue_times = Some((times, words));
            queue_widgets.push(hint);
        }
        build::gap(ui, content, dp(ui, 4));
        let progress = Text::new(build::text(ui, content, ""));
        // Where its new units go.
        let rally_gap = build::gap(ui, content, space);
        let rally = Text::new(build::text(ui, content, ""));
        let rally_to = compact.then(|| Text::new(build::text(ui, content, "")));
        // (In the sidebar the three have to fit its width, clear of its
        // scroll bar: shorter words, closer together. Defend once ran off
        // its edge.)
        let rally_row = build::row(ui, content, if compact { dp(ui, 2) } else { space / 2 });
        let (send, keep, defend) =
            if compact { ("Send...", "Keep", "Defend") } else { ("Send to...", "Keep here", "Defend here") };
        let send = build::button(ui, rally_row, &pid(CITY_RALLY_ID), send);
        let keep = build::button(ui, rally_row, &pid(CITY_NO_RALLY_ID), keep);
        let defend = build::button(ui, rally_row, &pid(CITY_DEFEND_ID), defend);
        // The rally points that are set, a button each. (In the sidebar
        // the ten do not fit in a line beside their words, and ran off its
        // edge: under the words, in two lines of five.)
        let points_row = if compact { build::column(ui, content, 0) } else { build::row(ui, content, dp(ui, 2)) };
        build::text(ui, points_row, if compact { "To rally point" } else { "To a rally point:" });
        let point_lines = if compact {
            vec![build::row(ui, points_row, dp(ui, 2)), build::row(ui, points_row, dp(ui, 2))]
        } else {
            vec![points_row]
        };
        let per_line = RALLY_POINTS.div_ceil(point_lines.len());
        let point_buttons: Vec<WidgetId> = (0..RALLY_POINTS)
            .map(|n| {
                let button = build::button(ui, point_lines[n / per_line], &pid(&point_id(n)), &n.to_string());
                ui.set_visibility(button, Visibility::Gone);
                button
            })
            .collect();
        // A blessing on the way there (in the window, beside the points: a
        // city with nine kinds to build still fits 768 high).
        let bless_row = build::row(ui, if compact { content } else { points_row }, 0);
        let bless_id = pid(CITY_BLESS_ID);
        let words = if compact { "A blessing on the way" } else { "  A temple's blessing on the way" };
        let resource = format!("TBClickLabel: text: \"{words}\"\n\tTBCheckBox: id: {bless_id}, is-focusable: 0\n");
        let _ = ui.inflate(bless_row, &resource);
        let bless = ui.find(bless_row, TbId::new(&bless_id)).unwrap_or(bless_row);
        let note = Text::new(build::text(ui, content, ""));
        let private = [heading, options, rally_gap, rally.widget, rally_row]
            .into_iter()
            .chain(rally_to.iter().map(|t| t.widget))
            .chain(details.iter().map(|d: &Text| d.widget))
            .chain(queue_widgets)
            .collect();
        Some(CityView {
            prefix,
            compact,
            stacked,
            focus: None,
            focus_shown: None,
            city: id,
            summary,
            income,
            garrison,
            progress,
            note,
            details,
            rally,
            rally_to,
            rally_buttons: [send, keep, defend],
            point_buttons,
            points_row,
            points: [false; RALLY_POINTS],
            bless,
            bless_row,
            queue_hint,
            queue_rows,
            queue_end,
            queue_times,
            chosen: None,
            about: None,
            adds,
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
            (info::Known::Fog(None, _), _, _) => "Not seen: who holds it, who is in it.".to_string(),
            (info::Known::Fog(Some((side, round)), _), _, _) => format!(
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
            info::Known::Fog(_, seen) => {
                let walls = seen.unwrap_or(city.walls);
                format!("Not seen, walls {walls}, at row {}, column {}", city.pos.row, city.pos.col)
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
            info::Known::Fog(..) => String::new(),
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
        // The box the keyboard is on looks as a stack's box of units on
        // their way does (blue).
        if self.stacked && self.focus_shown != self.focus {
            for (button, what) in &self.builds {
                if what.is_some() {
                    let skin = if *what == self.focus { "UnitComing" } else { "UnitSlot" };
                    ui.set_skin(*button, TbId::new(skin));
                }
            }
            self.focus_shown = self.focus;
        }
        // All about what is under the pointer, or the keyboard's; else about
        // what it builds.
        let shown = self.hovered(ui).or(self.focus).or(city.building);
        let details = match shown.and_then(|i| city.units.get(i)) {
            Some(prod) => info::production_details(map, prod, city.trains),
            None if city.units.is_empty() => Vec::new(),
            None => vec!["Point at one: all about it shows here.".to_string()],
        };
        for (i, line) in self.details.iter_mut().enumerate() {
            changed |= line.set(ui, details.get(i).map_or("", String::as_str));
        }
        // The queue (the window's): an item a row, the one it builds marked,
        // the one chosen pressed; what it does after the last.
        // (In the sidebar, always the item it builds now: what it builds
        // and where they go are set for that item, and the queue stays.)
        let queue = &city.queue;
        let about = match (queue.is_empty(), self.compact) {
            (true, _) => None,
            (false, true) => Some(queue.at),
            (false, false) => Some(self.chosen.filter(|i| *i < queue.items.len()).unwrap_or(queue.at)),
        };
        self.about = about;
        if let Some(hint) = &mut self.queue_hint {
            // (Once there is a queue, its items say enough.)
            let words = match (queue.is_empty(), self.stacked, city.production().is_some()) {
                (false, _, _) => "",
                (true, false, _) => "None: + beside a kind starts one.",
                (true, true, false) => "None: a unit below starts one, built over and over.",
                (true, true, true) => "None: it builds this over and over; a unit below makes a queue of it.",
            };
            changed |= hint.set(ui, words);
            let visibility = if all && queue.is_empty() { Visibility::Visible } else { Visibility::Gone };
            if ui.node(hint.widget).is_some_and(|n| n.visibility() != visibility) {
                ui.set_visibility(hint.widget, visibility);
                changed = true;
            }
        }
        for (i, (row, text, buttons, mark)) in self.queue_rows.iter_mut().enumerate() {
            let item = queue.items.get(i);
            let visibility = if all && item.is_some() { Visibility::Visible } else { Visibility::Gone };
            if ui.node(*row).is_some_and(|n| n.visibility() != visibility) {
                ui.set_visibility(*row, visibility);
                changed = true;
            }
            // The line over the item the queue goes back to.
            let repeats = match queue.end {
                QueueEnd::Repeat { from, times } if from == i && item.is_some() => Some(times),
                _ => None,
            };
            let words = match repeats {
                Some(None) => "Repeats from here, for ever:".to_string(),
                Some(Some(t)) => match t.saturating_sub(queue.rounds) {
                    0 => "Repeated from here; this is the last time:".to_string(),
                    1 => "Repeats from here, once more:".to_string(),
                    left => format!("Repeats from here, {left} times more:"),
                },
                None => String::new(),
            };
            changed |= mark.set(ui, &words);
            let visibility = if all && repeats.is_some() { Visibility::Visible } else { Visibility::Gone };
            if ui.node(mark.widget).is_some_and(|n| n.visibility() != visibility) {
                ui.set_visibility(mark.widget, visibility);
                changed = true;
            }
            let Some(item) = item else { continue };
            let now = if i == queue.at { format!("  (now {}/{})", queue.made + 1, item.count) } else { String::new() };
            changed |= text.set(ui, &format!("{}. {}{now}", i + 1, info::queue_item_line(map, city, i)));
            set_pressed(ui, text.widget, about == Some(i));
            for button in std::iter::once(&text.widget).chain(buttons.iter()) {
                if ui.node(*button).is_some_and(|n| n.is_disabled() == mine) {
                    ui.set_disabled(*button, !mine);
                }
            }
        }
        if let Some((row, [stop, repeat])) = self.queue_end {
            let visibility = if all && !queue.is_empty() { Visibility::Visible } else { Visibility::Gone };
            if ui.node(row).is_some_and(|n| n.visibility() != visibility) {
                ui.set_visibility(row, visibility);
                changed = true;
            }
            set_pressed(ui, stop, queue.end == QueueEnd::Stop);
            set_pressed(ui, repeat, queue.end != QueueEnd::Stop);
            // It says which item it would repeat from: the one chosen.
            let words = format!("Repeat from item {}", about.map_or(1, |i| i + 1));
            if ui.text(repeat) != Some(words.as_str()) {
                ui.set_text(repeat, &words);
                changed = true;
            }
        }
        if let Some((row, words)) = &mut self.queue_times {
            let repeats = matches!(queue.end, QueueEnd::Repeat { .. });
            let visibility = if all && !queue.is_empty() { Visibility::Visible } else { Visibility::Gone };
            if ui.node(*row).is_some_and(|n| n.visibility() != visibility) {
                ui.set_visibility(*row, visibility);
                changed = true;
            }
            let line = if queue.is_empty() { String::new() } else { info::queue_end_line(city) };
            changed |= words.set(ui, &line);
            for button in ui.children(*row).to_vec().into_iter().skip(1) {
                if ui.node(button).is_some_and(|n| n.is_disabled() == (mine && repeats)) {
                    ui.set_disabled(button, !(mine && repeats));
                }
            }
        }
        for button in &self.adds {
            if ui.node(*button).is_some_and(|n| n.is_disabled() == mine) {
                ui.set_disabled(*button, !mine);
            }
        }
        // Where the units go: the chosen item's, or the city's.
        let sends = about.and_then(|i| queue.items.get(i)).map_or(&city.sends, |item| &item.sends);
        // (In the sidebar, where they end, and how many stops before.)
        let (rally, rally_to) = match (sends, self.compact) {
            (Sends::Route(route), true) => {
                let stops = match route.len().saturating_sub(1) {
                    0 => String::new(),
                    1 => ", after a stop".to_string(),
                    n => format!(", after {n} stops"),
                };
                let end = route.last().map_or(String::new(), |to| info::place(map, *to));
                ("New units go to".to_string(), format!("{end}{stops}"))
            }
            (Sends::Point { n, bless }, true) => {
                let by = if *bless { ", by a temple" } else { "" };
                ("New units go to".to_string(), format!("rally point {n}{by}"))
            }
            (Sends::Stay, true) => ("New units stay here".to_string(), String::new()),
            (Sends::Defend, true) => ("New units defend it".to_string(), String::new()),
            (sends, false) => match about {
                Some(i) => (format!("Item {}'s units {}.", i + 1, info::sends_words(map, sends)), String::new()),
                None => (format!("New units {}.", info::sends_words(map, sends)), String::new()),
            },
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
        for button in self.rally_buttons.iter().chain(&self.point_buttons).chain([&self.bless]) {
            if ui.node(*button).is_some_and(|n| n.is_disabled() == mine) {
                ui.set_disabled(*button, !mine);
            }
        }
        // Keep and Defend are pressed while they are what it does; so is
        // the rally point its units go to.
        set_pressed(ui, self.rally_buttons[1], *sends == Sends::Stay);
        set_pressed(ui, self.rally_buttons[2], *sends == Sends::Defend);
        let to_point = match *sends {
            Sends::Point { n, bless } => Some((usize::from(n), bless)),
            _ => None,
        };
        let show = |ui: &mut Ui, widget: WidgetId, shown: bool| {
            let visibility = if shown { Visibility::Visible } else { Visibility::Gone };
            let change = ui.node(widget).is_some_and(|n| n.visibility() != visibility);
            if change {
                ui.set_visibility(widget, visibility);
            }
            change
        };
        for (n, button) in self.point_buttons.iter().enumerate() {
            changed |= show(ui, *button, self.points[n]);
            set_pressed(ui, *button, to_point.is_some_and(|(p, _)| p == n));
        }
        changed |= show(ui, self.points_row, all && self.points.iter().any(|p| *p));
        changed |= show(ui, self.bless_row, all && to_point.is_some());
        set_pressed(ui, self.bless, to_point.is_some_and(|(_, bless)| bless));
        changed |= self.note.set(ui, &note);
        let visibility = if note.is_empty() { Visibility::Gone } else { Visibility::Visible };
        if ui.node(self.note.widget).is_some_and(|n| n.visibility() != visibility) {
            ui.set_visibility(self.note.widget, visibility);
            changed = true;
        }
        // The kind the chosen item builds is pressed; or what it builds.
        let kind = about.and_then(|i| queue.items.get(i)).map_or(city.building, |item| Some(item.what));
        for (button, what) in &self.builds {
            set_pressed(ui, *button, kind == *what);
            if ui.node(*button).is_some_and(|n| n.is_disabled() == mine) {
                ui.set_disabled(*button, !mine);
            }
        }
        changed
    }

    /// What a click on one of its buttons asks for. While it has a queue,
    /// the window's buttons for what it builds and where its units go are
    /// about the item chosen.
    fn click(&mut self, id: TbId) -> Option<CityOrder> {
        let is = |name: &str| id == TbId::new(&format!("{}{name}", self.prefix));
        let (city, about) = (self.city, self.about);
        if is(CITY_RALLY_ID) {
            return Some(about.map_or(CityOrder::PickRally(city), |i| CityOrder::ItemPick(city, i)));
        }
        if is(CITY_NO_RALLY_ID) {
            return Some(about.map_or(CityOrder::NoRally(city), |i| CityOrder::ItemStay(city, i)));
        }
        if is(CITY_DEFEND_ID) {
            return Some(about.map_or(CityOrder::DefendNew(city), |i| CityOrder::ItemDefend(city, i)));
        }
        if is(CITY_BLESS_ID) {
            return Some(about.map_or(CityOrder::ToggleBless(city), |i| CityOrder::ItemBless(city, i)));
        }
        if let Some(n) = (0..RALLY_POINTS).find(|n| is(&point_id(*n))) {
            let n = n as u8;
            return Some(about.map_or(CityOrder::ToPoint(city, n), |i| CityOrder::ItemPoint(city, i, n)));
        }
        if is(BUILD_NOTHING_ID) {
            return Some(CityOrder::SetProduction(city, None));
        }
        if let Some(what) = (0..self.builds.len()).find(|i| is(&build_id(*i))) {
            if self.stacked {
                return Some(CityOrder::Enqueue(city, what));
            }
            return Some(
                about.map_or(CityOrder::SetProduction(city, Some(what)), |i| CityOrder::ItemKind(city, i, what)),
            );
        }
        if self.compact {
            return None;
        }
        let row = |f: fn(usize) -> String| (0..MAX_QUEUE).find(|i| id == TbId::new(&f(*i)));
        if let Some(what) = (0..self.adds.len()).find(|i| id == TbId::new(&queue_add_id(*i))) {
            return Some(CityOrder::QueueAdd(city, what));
        }
        if let Some(i) = row(queue_item_id) {
            self.chosen = Some(i);
            return Some(CityOrder::Chose(city));
        }
        if let Some(i) = row(queue_less_id) {
            return Some(CityOrder::QueueCount(city, i, false));
        }
        if let Some(i) = row(queue_more_id) {
            return Some(CityOrder::QueueCount(city, i, true));
        }
        if let Some(i) = row(queue_remove_id) {
            return Some(CityOrder::QueueRemove(city, i));
        }
        if id == TbId::new(QUEUE_STOP_ID) {
            return Some(CityOrder::QueueStop(city));
        }
        if id == TbId::new(QUEUE_REPEAT_ID) {
            return Some(CityOrder::QueueRepeat(city, about.unwrap_or(0)));
        }
        if id == TbId::new(QUEUE_FEWER_ID) || id == TbId::new(QUEUE_MORE_TIMES_ID) {
            return Some(CityOrder::QueueTimes(city, id == TbId::new(QUEUE_MORE_TIMES_ID)));
        }
        None
    }
}

/// A city's window: its view, a button that raises its walls (its
/// holder's, if that is the player), and one that closes it. Or the same
/// built into the production panel (`embed`: the owner's word, 2026-11-24,
/// "Merge the production and city dialogs"), with no window of its own and
/// no Close: the panel lays it out.
pub struct CityWindow {
    parts: WindowParts,
    view: CityView,
    walls: WidgetId,
    /// What the walls' button says, and whether it is pressable.
    walls_state: (String, bool, bool),
    embedded: bool,
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
        let Some(view) = CityView::build(ui, parts.content, map, id, known, icons, ("", false, false)) else {
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
        let mut window = CityWindow { parts, view, walls, walls_state: (String::new(), false, false), embedded: false };
        window.view.fill(ui, map, player);
        build::place(ui, parts, area, align);
        Some(window)
    }

    /// The city's view built into `host` (a column of the production
    /// panel), with its walls' button over it and no Close. `None` if there
    /// is no such city.
    pub fn embed(
        ui: &mut Ui,
        host: WidgetId,
        map: &Map,
        id: CityId,
        (player, known): (Side, info::Known),
        icons: &[ImageHandle],
    ) -> Option<Self> {
        let column = build::column(ui, host, 0);
        // (The city's view has the window's id wherever it is.)
        ui.set_id(column, TbId::new(CITY_WINDOW_ID));
        let parts = WindowParts { window: column, content: column };
        let Some(view) = CityView::build(ui, column, map, id, known, icons, ("", false, true)) else {
            build::close(ui, column);
            return None;
        };
        let space = dp(ui, SPACING_DP);
        build::gap(ui, column, space);
        let walls = build::button(ui, column, CITY_WALLS_ID, "");
        ui.set_visibility(walls, Visibility::Gone);
        let mut window = CityWindow { parts, view, walls, walls_state: (String::new(), false, false), embedded: true };
        window.view.fill(ui, map, player);
        Some(window)
    }

    /// True if it is built into the production panel.
    pub fn embedded(&self) -> bool {
        self.embedded
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
        if !self.embedded {
            build::refit(ui, self.parts);
        }
    }

    /// Choose the `i`th item of the queue (what it builds and where its
    /// units go are set for it).
    pub fn choose_item(&mut self, i: usize) {
        self.view.chosen = Some(i);
    }

    /// The unit box the keyboard is on (the production panel's), if any.
    pub fn focus_unit(&mut self, what: Option<usize>) {
        self.view.focus = what.filter(|w| *w + 1 < self.view.builds.len());
    }

    pub fn focused_unit(&self) -> Option<usize> {
        self.view.focus
    }

    /// Scroll its box so that the unit in focus and all about it show
    /// (the panel's right column scrolls).
    pub fn show_focus(&self, ui: &mut Ui) {
        if let Some(last) = self.view.details.iter().rev().find(|d| !d.text.is_empty()) {
            ui.scroll_into_view_recursive(last.widget);
        }
    }

    /// How many kinds it can build (its boxes).
    pub fn kinds(&self) -> usize {
        self.view.builds.len().saturating_sub(1)
    }

    /// The item of the queue its buttons are about, if it has a queue.
    pub fn item(&self) -> Option<usize> {
        self.view.about
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

    /// Which of the player's rally points are set (a button each, to send
    /// the city's new units there); the next `refresh` shows them.
    pub fn set_points(&mut self, points: [bool; RALLY_POINTS]) {
        self.view.points = points;
    }

    /// Bring it up to date with the city. False if the city is gone.
    pub fn refresh(&mut self, ui: &mut Ui, map: &Map, (player, known): (Side, info::Known)) -> bool {
        if map.cities().get(self.view.city).is_none() {
            return false;
        }
        self.view.known = known;
        if self.view.fill(ui, map, player) && !self.embedded {
            build::refit(ui, self.parts);
        }
        true
    }

    /// What a click on one of its buttons asks for (its close button is the
    /// caller's to handle).
    pub fn click(&mut self, id: TbId) -> Option<CityOrder> {
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
        let Some(view) = CityView::build(ui, root, map, id, known, icons, (PANEL_PREFIX, true, false)) else {
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

    /// Which of the player's rally points are set (a button each, to send
    /// the city's new units there); the next `refresh` shows them.
    pub fn set_points(&mut self, points: [bool; RALLY_POINTS]) {
        self.view.points = points;
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
    pub fn click(&mut self, id: TbId) -> Option<CityOrder> {
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
