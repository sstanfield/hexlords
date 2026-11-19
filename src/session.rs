//! The game without its window: a map screen (the UI, the map and the views)
//! with the game's layout and what the player's input does to the game. It
//! needs no graphics context, so tests drive it the way the window does.
//!
//! Under the map is the panel: the line about the hex under the cursor, then
//! the *selected* hex (the one last clicked), a slot for each unit it can hold
//! (the unit's icon and its strength and moves; a click selects the unit, and
//! the pointer on it opens a box with all about it), and a line about the
//! selected unit. Beside the map, under the minimap, is the selected hex's
//! city. The map takes all the height the panel leaves. `info` has the words;
//! this module puts them into the widgets.
//!
//! C++: assets/layouts/main.lua (the layout), src/gui/mapwidget.cpp
//! MapWidget::OnEvent (what input does to the map)

use std::collections::HashMap;
use std::path::Path;

use turbo_ferret::widgets::ImageWidget;
use turbo_ferret::{ImageHandle, LayoutParams, Modifiers, Point, Rect, Size, TbId, Visibility, WidgetId};

use crate::game::Game;
use crate::gui::{MapCommand, MapKey, RootCommand};
use crate::info;
use crate::render::{AssetDirs, unit_icon};
use crate::screen::{MapScreen, Scene};
use crate::state::{DEFAULT_UNIT_IMAGE, Hex, MAX_STACK, Side, UnitId};

/// The layout, relative to the asset directory.
pub const LAYOUT: &str = "layouts/main.tb.txt";

/// The main map takes this share of the window's width; the minimap gets the
/// rest of it, and this share of the height. C++: assets/layouts/main.lua
const MAP_WIDTH_SHARE: f32 = 0.8;
const MINIMAP_HEIGHT_SHARE: f32 = 0.3;
/// The space between widgets in the layout, in dp (`spacing: 5` in the layout).
const SPACING_DP: i32 = 5;
/// The height of the line about the cursor, and of the panels' other lines, in dp.
const STATUS_DP: i32 = 24;
const LINE_DP: i32 = 20;
/// A unit's slot in the stack: its width (less if nine do not fit under the
/// map), the space between two, and the size of the icon in it, in dp.
const SLOT_DP: i32 = 84;
const SLOT_SPACING_DP: i32 = 4;
const ICON_DP: i32 = 40;
/// How many slots the stack has: one for each unit a hex can hold, and one
/// more (for a leader, one day).
pub const STACK_SLOTS: usize = MAX_STACK + 1;
/// The button that ends the turn, and the line about the cursor.
pub const END_TURN_ID: &str = "end_turn";
pub const STATUS_ID: &str = "status";
/// The selected hex's heading, and the line about the selected unit.
pub const HEX_TITLE_ID: &str = "hex_title";
pub const UNIT_LINE_ID: &str = "unit_line";
/// The panel under the map, and the row of slots in it.
const PANEL_ID: &str = "hex_panel";
const STACK_ID: &str = "stack";
/// The box about the unit under the pointer, and its lines (`tip_0` ...).
pub const UNIT_TIP_ID: &str = "unit_tip";
pub const TIP_LINES: usize = 5;
/// The lines about the selected city: `city_0` ...
pub const CITY_LINES: usize = 9;

/// The id of the button of the `i`th unit of the selected hex.
pub fn unit_button_id(i: usize) -> String {
    format!("unit_{i}")
}

/// A widget whose text the session writes, with what it shows now (so that it
/// is written only when it changes).
struct Line {
    widget: WidgetId,
    text: String,
}

impl Line {
    fn find(screen: &MapScreen, id: &str) -> Result<Line, String> {
        Ok(Line { widget: find(screen, id)?, text: String::new() })
    }

    fn set(&mut self, screen: &mut MapScreen, text: &str) {
        if self.text != text {
            screen.ui.set_text(self.widget, text);
            self.text = text.to_string();
        }
    }
}

fn find(screen: &MapScreen, id: &str) -> Result<WidgetId, String> {
    screen.ui.find(screen.ui.root(), TbId::new(id)).ok_or(format!("{LAYOUT}: no widget with id {id}"))
}

fn set_visibility(screen: &mut MapScreen, widget: WidgetId, visibility: Visibility) {
    if screen.ui.node(widget).is_some_and(|n| n.visibility() != visibility) {
        screen.ui.set_visibility(widget, visibility);
    }
}

/// The picture a unit is drawn with in a slot: its kind's image and its side.
type IconKey = (String, Side);

/// A unit's place in the stack: a button with the unit's icon and its numbers.
struct Slot {
    button: WidgetId,
    icon: WidgetId,
    numbers: Line,
    /// The icon it shows now.
    shows: Option<IconKey>,
}

impl Slot {
    /// The resource the slot is made from (`Session` makes them, so that their
    /// number is a constant, not a list in the layout).
    fn resource(i: usize) -> String {
        let id = unit_button_id(i);
        format!(
            "TBButton: id: {id}, skin: UnitSlot, group-id: stack, is-focusable: 0\n\
             \tTBLayout: axis: y, spacing: 2\n\
             \t\tTBImageWidget: id: {id}_icon\n\
             \t\tTBTextField: id: {id}_text, text-align: center\n"
        )
    }
}

pub struct Session {
    pub screen: MapScreen,
    /// The hex the panels are about: the one last chosen on the map.
    shown: Hex,
    /// Where the units' pictures come from (the map's own assets first).
    assets: AssetDirs,
    /// The units' icons drawn so far, for the UI.
    icons: HashMap<IconKey, ImageHandle>,
    panel: WidgetId,
    stack_row: WidgetId,
    status: Line,
    hex_title: Line,
    slots: Vec<Slot>,
    unit_line: Line,
    tip: WidgetId,
    tip_lines: Vec<Line>,
    city: Vec<Line>,
}

impl Session {
    /// Set up the UI for a window of `viewport` pixels at `dpi` with the skin and
    /// the layout from the asset directory, and start the demo game.
    pub fn new(assets: &Path, viewport: Size, dpi: i32) -> Result<Session, String> {
        Session::with_game(assets, viewport, dpi, Game::demo())
    }

    /// The same, playing `game`.
    pub fn with_game(assets: &Path, viewport: Size, dpi: i32, game: Game) -> Result<Session, String> {
        let dirs = AssetDirs::new(assets, game.assets.as_deref());
        let mut screen = MapScreen::new(assets, viewport, dpi, LAYOUT, game)?;
        let stack_row = find(&screen, STACK_ID)?;
        let mut slots = Vec::with_capacity(STACK_SLOTS);
        for i in 0..STACK_SLOTS {
            screen.ui.inflate(stack_row, &Slot::resource(i)).map_err(|e| format!("a slot of the stack: {e}"))?;
            let id = unit_button_id(i);
            let (button, icon) = (find(&screen, &id)?, find(&screen, &format!("{id}_icon"))?);
            slots.push(Slot { button, icon, numbers: Line::find(&screen, &format!("{id}_text"))?, shows: None });
        }
        let lines = |screen: &MapScreen, name: &str, n: usize| -> Result<Vec<Line>, String> {
            (0..n).map(|i| Line::find(screen, &format!("{name}_{i}"))).collect()
        };
        let tip = find(&screen, UNIT_TIP_ID)?;
        // The box is over the map, and the pointer goes through it.
        screen.ui.set_ignore_input(tip, true);
        let mut session = Session {
            shown: screen.game.map.cursor,
            assets: dirs,
            icons: HashMap::new(),
            panel: find(&screen, PANEL_ID)?,
            stack_row,
            status: Line::find(&screen, STATUS_ID)?,
            hex_title: Line::find(&screen, HEX_TITLE_ID)?,
            slots,
            unit_line: Line::find(&screen, UNIT_LINE_ID)?,
            tip,
            tip_lines: lines(&screen, "tip", TIP_LINES)?,
            city: lines(&screen, "city", CITY_LINES)?,
            screen,
        };
        session.apply_layout_sizes();
        session.refresh();
        session.screen.settle();
        Ok(session)
    }

    /// Size everything from the window's size: the map is 80% of the width and
    /// all the height the panel under it leaves; the minimap is the rest of
    /// the width and 30% of the height; the lines get the widths that leaves
    /// them. Called again whenever the window's size changes.
    /// C++: assets/layouts/main.lua
    fn apply_layout_sizes(&mut self) {
        let Size { w, h } = self.screen.ui.viewport();
        let dp = |screen: &MapScreen, dp: i32| screen.ui.dimension_converter().dp_to_px(dp);
        let spacing = dp(&self.screen, SPACING_DP);
        let map_w = (w as f32 * MAP_WIDTH_SHARE).floor() as i32;
        let side_w = (w - map_w - spacing).max(0);

        let (status_h, line_h) = (dp(&self.screen, STATUS_DP), dp(&self.screen, LINE_DP));
        let gap = dp(&self.screen, SLOT_SPACING_DP);
        let fit = (map_w - gap * (STACK_SLOTS as i32 - 1)) / STACK_SLOTS as i32;
        let (slot_w, icon) = (dp(&self.screen, SLOT_DP).min(fit).max(0), dp(&self.screen, ICON_DP));
        let ui = &mut self.screen.ui;
        ui.set_layout_params(self.status.widget, Some(LayoutParams::fixed(map_w, status_h)));
        ui.set_layout_params(self.hex_title.widget, Some(LayoutParams::fixed(map_w, line_h)));
        ui.set_layout_params(self.unit_line.widget, Some(LayoutParams::fixed(map_w, line_h)));
        for slot in &self.slots {
            let mut lp = LayoutParams::default();
            lp.set_width(slot_w);
            ui.set_layout_params(slot.button, Some(lp));
            ui.set_layout_params(slot.icon, Some(LayoutParams::fixed(icon, icon)));
        }
        // The slots do not push the sidebar off a narrow window.
        ui.set_layout_params(self.stack_row, Some(LayoutParams { max_w: Some(map_w), ..LayoutParams::default() }));
        for line in &self.city {
            ui.set_layout_params(line.widget, Some(LayoutParams::fixed(side_w, line_h)));
        }
        // The panel is as tall as its lines and its slots (an empty slot keeps
        // its room), and the map has the rest.
        let panel_h = ui.preferred_size_free(self.panel).pref.h;
        let map_h = (h - spacing - panel_h).max(0);
        let minimap_h = h - (h as f32 * (1.0 - MINIMAP_HEIGHT_SHARE)).floor() as i32;
        self.screen.set_map_sizes(Size::new(map_w, map_h), Size::new(side_w, minimap_h.min(h)));
    }

    /// The line about the hex under the cursor.
    pub fn status(&self) -> &str {
        &self.status.text
    }

    /// The hex the panels are about: the one last chosen on the map (at the
    /// start, the one the cursor starts on).
    pub fn shown(&self) -> Hex {
        self.shown
    }

    /// The selected hex's heading.
    pub fn hex_title(&self) -> &str {
        &self.hex_title.text
    }

    /// The units of the selected hex, in the stack's order: the order of their
    /// slots, the first the one the map shows.
    fn stack_units(&self) -> Vec<UnitId> {
        let map = &self.screen.game.map;
        map.units_at(self.shown).iter().copied().filter(|id| map.units().get(*id).is_some()).collect()
    }

    /// The numbers under the icons of the selected hex's units, the first unit
    /// first.
    pub fn stack(&self) -> Vec<&str> {
        let units = self.stack_units().len();
        self.slots.iter().take(units).map(|slot| slot.numbers.text.as_str()).collect()
    }

    /// The picture the `i`th slot shows: the image of the unit's kind and its
    /// side (`None` for an empty slot).
    pub fn slot_icon(&self, i: usize) -> Option<(&str, Side)> {
        self.slots.get(i)?.shows.as_ref().map(|(image, side)| (image.as_str(), *side))
    }

    /// The line about the selected unit.
    pub fn unit_line(&self) -> &str {
        &self.unit_line.text
    }

    /// The lines of the box the pointer opens over a unit's slot (none while
    /// it is closed).
    pub fn tip(&self) -> Vec<&str> {
        let open = self.screen.ui.node(self.tip).is_some_and(|n| n.visibility() == Visibility::Visible);
        let lines = self.tip_lines.iter().map(|line| line.text.as_str()).filter(|text| !text.is_empty());
        lines.filter(|_| open).collect()
    }

    /// The lines about the selected hex's city (none if it has no city).
    pub fn city(&self) -> Vec<&str> {
        self.city.iter().map(|line| line.text.as_str()).filter(|text| !text.is_empty()).collect()
    }

    /// The icon of a unit whose kind is drawn with `image`, of `side`, for the
    /// UI: drawn the first time it is asked for. A picture that cannot be read
    /// is the default unit's; if that cannot be read either, the disc alone.
    fn icon(&mut self, key: &IconKey) -> ImageHandle {
        if let Some(handle) = self.icons.get(key) {
            return handle.clone();
        }
        let (image, side) = key;
        let picture = self.assets.image(image).or_else(|_| self.assets.image(DEFAULT_UNIT_IMAGE)).ok();
        let size = self.screen.ui.dimension_converter().dp_to_px(ICON_DP);
        let icon = unit_icon(picture.as_ref(), *side, size);
        let name = format!("unit icon {image} {side:?} {size}");
        let handle = self.screen.ui.image_from_pixels(&name, icon.size, &icon.rgba);
        self.icons.insert(key.clone(), handle.clone());
        handle
    }

    /// Bring the panels up to date with the game.
    fn refresh(&mut self) {
        let game = &self.screen.game;
        let map = &game.map;
        let status = info::cursor_line(game);
        let title = info::hex_title(map, self.shown);
        // The units of the selected hex: each one's icon and numbers, and
        // which of them is the selected one.
        let ids = self.stack_units();
        let units: Vec<(IconKey, String)> = ids
            .iter()
            .filter_map(|id| map.units().get(*id))
            .map(|unit| {
                let image = map.units().kind_of(unit).map_or(DEFAULT_UNIT_IMAGE, |kind| kind.image.as_str());
                ((image.to_string(), unit.side()), info::unit_numbers(unit))
            })
            .collect();
        let chosen = game.selected().and_then(|id| ids.iter().position(|unit| *unit == id));
        // The selected unit in a line, wherever it stands.
        let line = match game.selected().and_then(|id| map.units().get(id)) {
            Some(unit) => info::unit_line(map, unit),
            None if ids.is_empty() => String::new(),
            None => "Click a unit to select it, then a hex to send it there.".to_string(),
        };
        // All about the unit whose slot the pointer is on.
        let hovered = self.screen.ui.hovered();
        let tipped = self.slots.iter().position(|slot| Some(slot.button) == hovered).filter(|i| *i < units.len());
        let mut tip =
            tipped.and_then(|i| map.units().get(ids[i])).map_or(Vec::new(), |unit| info::unit_details(map, unit));
        tip.truncate(TIP_LINES);
        let city = map.city_at(self.shown).map_or(Vec::new(), |city| info::city_lines(map, city, CITY_LINES));

        self.status.set(&mut self.screen, &status);
        self.hex_title.set(&mut self.screen, &title);
        self.unit_line.set(&mut self.screen, &line);
        for i in 0..self.slots.len() {
            let unit = units.get(i);
            let key = unit.map(|(key, _)| key.clone());
            if self.slots[i].shows != key {
                let handle = key.as_ref().map_or(ImageHandle::default(), |key| self.icon(key));
                let icon = self.slots[i].icon;
                self.screen.ui.with_widget::<ImageWidget, _>(icon, |w, cx| w.set_image(cx, handle));
                self.slots[i].shows = key;
            }
            let slot = &mut self.slots[i];
            slot.numbers.set(&mut self.screen, unit.map_or("", |(_, numbers)| numbers.as_str()));
            // An empty slot keeps its room.
            let visibility = if unit.is_some() { Visibility::Visible } else { Visibility::Invisible };
            set_visibility(&mut self.screen, slot.button, visibility);
            // The selected unit's slot is the one that is pressed in.
            let pressed = i32::from(chosen == Some(i));
            if self.screen.ui.value(slot.button).as_int() != pressed {
                self.screen.ui.set_value(slot.button, pressed);
            }
        }
        for (i, line) in self.tip_lines.iter_mut().enumerate() {
            line.set(&mut self.screen, tip.get(i).map_or("", String::as_str));
            // The box is as tall as what it says.
            let visibility = if i < tip.len() { Visibility::Visible } else { Visibility::Gone };
            set_visibility(&mut self.screen, line.widget, visibility);
        }
        match tipped.filter(|_| !tip.is_empty()) {
            Some(i) => self.open_tip(self.slots[i].button),
            None => set_visibility(&mut self.screen, self.tip, Visibility::Gone),
        }
        for (i, line) in self.city.iter_mut().enumerate() {
            line.set(&mut self.screen, city.get(i).map_or("", String::as_str));
        }
    }

    /// Show the box about a unit over its slot (under it if there is no room
    /// above, and wherever it fits in a window too small for either), inside
    /// the window.
    fn open_tip(&mut self, slot: WidgetId) {
        set_visibility(&mut self.screen, self.tip, Visibility::Visible);
        let ui = &mut self.screen.ui;
        let size = ui.preferred_size_free(self.tip).pref;
        let window = ui.viewport();
        let spacing = ui.dimension_converter().dp_to_px(SPACING_DP);
        let at = Rect::from_origin_size(ui.to_root(slot, Point::ZERO), ui.rect(slot).size());
        let x = at.x.min(window.w - size.w).max(0);
        let (above, below) = (at.y - spacing - size.h, at.bottom() + spacing);
        let y = if above >= 0 {
            above
        } else if below + size.h <= window.h {
            below
        } else {
            (window.h - size.h).max(0)
        };
        ui.set_rect(self.tip, Rect::new(x, y, size.w, size.h));
    }

    /// The window was resized.
    pub fn resize(&mut self, viewport: Size) {
        self.screen.ui.set_viewport(viewport);
        self.apply_layout_sizes();
    }

    /// One frame of host work before drawing (see `MapScreen::frame`).
    pub fn frame(&mut self, now_ms: u64) {
        self.screen.frame(now_ms);
        self.refresh();
    }

    /// The player chose a hex of the map (a click, or Enter at the cursor):
    /// the game selects or moves a unit, and the panels are about that hex
    /// from now on.
    fn choose(&mut self, hex: Hex) {
        self.screen.game.select_hex(hex);
        self.shown = hex;
    }

    /// Apply what the widgets recorded since the last call. The host calls this
    /// after every input event it forwards to the UI.
    pub fn pump(&mut self) {
        for command in self.screen.take_map_commands() {
            self.screen.navigate(command);
            // A click, or Enter at the cursor, chooses a hex.
            // C++: MapWidget::OnEvent
            match command {
                MapCommand::Click(p, size) => {
                    if let Some(hex) = self.screen.pick(p, size) {
                        self.choose(hex);
                    }
                }
                MapCommand::Key(MapKey::Enter) => self.choose(self.screen.game.map.cursor),
                _ => {}
            }
        }
        for command in self.screen.pump_shared() {
            match command {
                RootCommand::Click(id) if id == TbId::new(END_TURN_ID) => self.screen.game.end_turn(),
                // Ctrl+T (Cmd+T) ends the turn too. C++: main.cpp (the
                // commented-out Ctrl+T)
                RootCommand::Key(ch, mods)
                    if ch.eq_ignore_ascii_case(&'t') && mods.intersects(Modifiers::CTRL | Modifiers::SUPER) =>
                {
                    self.screen.game.end_turn();
                }
                // A unit's slot: that unit is the selected one.
                RootCommand::Click(id) => {
                    let unit = (0..STACK_SLOTS).find(|i| id == TbId::new(&unit_button_id(*i)));
                    let unit = unit.and_then(|i| self.stack_units().get(i).copied());
                    if let Some(unit) = unit {
                        self.screen.game.select_unit(unit);
                    }
                }
                _ => {}
            }
        }
        self.refresh();
    }
}

impl Scene for Session {
    fn screen(&self) -> &MapScreen {
        &self.screen
    }

    fn screen_mut(&mut self) -> &mut MapScreen {
        &mut self.screen
    }

    fn resize(&mut self, viewport: Size) {
        Session::resize(self, viewport);
    }

    fn pump(&mut self) {
        Session::pump(self);
    }

    fn frame(&mut self, now_ms: u64) {
        Session::frame(self, now_ms);
    }

    /// Where the selected unit can go.
    fn marked(&self, hex: Hex) -> bool {
        self.screen.game.can_move_to(hex)
    }

    /// The selected unit shows on its hex, over the others of its stack (in a
    /// city, its pip is ringed).
    fn front_unit(&self) -> Option<UnitId> {
        self.screen.game.selected()
    }
}
