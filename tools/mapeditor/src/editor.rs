//! The map editor without its window: a map screen (the main map, the minimap)
//! with the editor's sidebar, and what the player's input does to the map. Like
//! the game's session it needs no graphics context, so tests drive it.
//!
//! The widgets hold nothing: the sidebar's buttons and the keys arrive as
//! commands (`hexlords::gui`), and everything the editor knows is here.

use std::path::{Path, PathBuf};

use hexlords::game::{Game, describe_terrain};
use hexlords::gui::{MapCommand, MapKey, RootCommand};
use hexlords::help::{HelpSection, command_key};
use hexlords::screen::{MapScreen, Scene};
use hexlords::state::{Hex, Map, TileType, scenario};
use turbo_ferret::{LayoutParams, Modifiers, Point, Size, SpecialKey, TbId, WidgetId, tbid};

use crate::brush::{MAX_RADIUS, footprint, region};
use crate::history::History;

/// The layout, relative to the asset directory.
pub const LAYOUT: &str = "layouts/editor.tb.txt";

/// The sidebar's width, the status line's height and the space between the
/// widgets (`spacing: 5` in the layout), in dp.
const SIDEBAR_DP: i32 = 270;
const STATUS_DP: i32 = 24;
const SPACING_DP: i32 = 5;
/// The height the sidebar's buttons and fields need below the minimap, in dp.
const CONTROLS_DP: i32 = 500;
/// The minimap is never lower than this, in dp.
const MIN_MINIMAP_DP: i32 = 60;
/// A drag paints every hex it crosses: between two pointer positions the brush
/// is put down this many pixels apart at most, less than any hex is wide.
const STROKE_STEP_PX: i32 = 4;

/// Laid over the hexes the brush would paint: light enough to see what has
/// been painted under it.
const BRUSH_TINT: [f32; 4] = [1.0, 1.0, 1.0, 0.25];

/// What a press on the map does.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Tool {
    /// Give the hexes under the brush the chosen terrain; a drag paints a line.
    Paint,
    /// Give the chosen terrain to the hex and to every hex of its terrain that
    /// is connected to it.
    Fill,
    /// Lay a road on the hex (over water: a bridge); a drag lays it along the
    /// way. The terrain stays.
    Road,
    /// Take the hex's road away.
    Unroad,
}

impl Tool {
    /// The tool's button in the sidebar.
    fn button(self) -> TbId {
        match self {
            Tool::Paint => tbid!("tool_paint"),
            Tool::Fill => tbid!("tool_fill"),
            Tool::Road => tbid!("tool_road"),
            Tool::Unroad => tbid!("tool_unroad"),
        }
    }

    const ALL: [Tool; 4] = [Tool::Paint, Tool::Fill, Tool::Road, Tool::Unroad];

    /// True for the tools a drag goes on with.
    fn strokes(self) -> bool {
        self != Tool::Fill
    }
}

/// Something that throws unsaved changes away, and so has to be asked twice.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum Discard {
    Quit,
    Open,
    New,
}

/// The sidebar's widgets the editor reads or writes.
struct Widgets {
    sidebar: WidgetId,
    status: WidgetId,
    file: WidgetId,
    rows: WidgetId,
    cols: WidgetId,
    undo: WidgetId,
    redo: WidgetId,
}

/// The editor's keys and clicks, for the help window (F1).
fn help_section() -> HelpSection {
    let cmd = command_key();
    let (save, undo, redo) = (format!("{cmd}+S"), format!("{cmd}+Z"), format!("{cmd}+Shift+Z  or  {cmd}+Y"));
    HelpSection::new(
        "Editing",
        &[
            ("A terrain's letter", "Choose it, and paint it at the cursor (the buttons say which)"),
            ("Click or drag on the map", "Use the tool: paint, fill, lay a road or take one up"),
            ("Enter  or  Space", "Use the tool at the cursor"),
            ("[  /  ]", "A smaller  /  bigger brush"),
            (&save, "Save"),
            (&undo, "Undo"),
            (&redo, "Redo"),
        ],
    )
}

pub struct Editor {
    pub screen: MapScreen,
    widgets: Widgets,
    brush: TileType,
    /// The brush's size: 0 is one hex, 1 is seven, 2 is nineteen.
    radius: u8,
    tool: Tool,
    history: History,
    /// A drag is painting: the pointer's last position in the map area.
    stroke: Option<Point>,
    /// The file the map was read from or last written to.
    path: Option<PathBuf>,
    /// The map differs from the file.
    modified: bool,
    /// The thing that was refused once because of unsaved changes.
    armed: Option<Discard>,
    /// What the last action has to say, for the status line.
    message: String,
    /// The hexes the brush would paint at the cursor (shown when it is larger
    /// than the cursor's own hex).
    preview: Vec<Hex>,
    /// What the status line shows now.
    status: String,
}

fn terrain_id(kind: TileType) -> TbId {
    TbId::new(&format!("terrain_{}", kind.to_char()))
}

const BRUSH_IDS: [TbId; MAX_RADIUS as usize + 1] = [tbid!("brush_1"), tbid!("brush_7"), tbid!("brush_19")];

impl Editor {
    /// Set up the UI for a window of `viewport` pixels at `dpi` with the skin and
    /// the layout from the asset directory, editing `map`, which came from (or
    /// will be saved to) `path`.
    pub fn new(assets: &Path, viewport: Size, dpi: i32, map: Map, path: Option<PathBuf>) -> Result<Editor, String> {
        let mut screen = MapScreen::new(assets, viewport, dpi, LAYOUT, Game::new(map))?;
        screen.set_help(vec![help_section()]);
        let find = |name: &str| {
            screen.ui.find(screen.ui.root(), TbId::new(name)).ok_or(format!("{LAYOUT}: no widget with id {name}"))
        };
        let widgets = Widgets {
            sidebar: find("sidebar")?,
            status: find("status")?,
            file: find("file_name")?,
            rows: find("new_rows")?,
            cols: find("new_cols")?,
            undo: find("undo")?,
            redo: find("redo")?,
        };
        let mut editor = Editor {
            screen,
            widgets,
            brush: TileType::Grass,
            radius: 0,
            tool: Tool::Paint,
            history: History::new(),
            stroke: None,
            path,
            modified: false,
            armed: None,
            message: String::new(),
            preview: Vec::new(),
            status: String::new(),
        };
        if let Some(path) = editor.path.as_ref().filter(|p| !p.exists()) {
            editor.message = format!("{} is not there yet: save to make it", path.display());
        }
        editor.show_map_in_fields();
        editor.apply_layout_sizes();
        editor.screen.settle();
        editor.refresh();
        Ok(editor)
    }

    /// The map being edited.
    pub fn map(&self) -> &Map {
        &self.screen.game.map
    }

    pub fn brush(&self) -> TileType {
        self.brush
    }

    pub fn radius(&self) -> u8 {
        self.radius
    }

    pub fn tool(&self) -> Tool {
        self.tool
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// True if the map differs from its file.
    pub fn modified(&self) -> bool {
        self.modified
    }

    /// What the last action had to say.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The status line's text.
    pub fn status(&self) -> &str {
        &self.status
    }

    /// The map fills the window but for the sidebar on the right (the minimap at
    /// its top, as wide as the sidebar and as high as the map's shape asks, or
    /// lower in a low window) and the status line at the bottom. Called again when the window or the map
    /// changes size.
    fn apply_layout_sizes(&mut self) {
        let Size { w, h } = self.screen.ui.viewport();
        let dp = |v: i32| self.screen.ui.dimension_converter().dp_to_px(v);
        let (spacing, status_h) = (dp(SPACING_DP), dp(STATUS_DP));
        let sidebar_w = dp(SIDEBAR_DP).min(w * 2 / 5);
        // A spacing between the map and the sidebar, and one at the window's edge.
        let map = Size::new((w - sidebar_w - 2 * spacing).max(0), (h - status_h - spacing).max(0));
        // As high as the map's shape asks, if that leaves room for the rest of
        // the sidebar.
        let snug = (sidebar_w as f32 * self.screen.mini_view.shape()) as i32;
        let room = (map.h - dp(CONTROLS_DP)).max(dp(MIN_MINIMAP_DP));
        let minimap = Size::new(sidebar_w, snug.min(room));
        self.screen.set_map_sizes(map, minimap);
        self.screen.ui.set_layout_params(self.widgets.sidebar, Some(LayoutParams::fixed(sidebar_w, map.h)));
        self.screen.ui.set_layout_params(self.widgets.status, Some(LayoutParams::fixed(w, status_h)));
    }

    /// The window was resized.
    pub fn resize(&mut self, viewport: Size) {
        self.screen.ui.set_viewport(viewport);
        self.apply_layout_sizes();
    }

    /// Apply what the widgets recorded since the last call. The host calls this
    /// after every input event it forwards to the UI.
    pub fn pump(&mut self) {
        for command in self.screen.take_map_commands() {
            self.screen.navigate(command);
            match command {
                MapCommand::Press(p, size) => {
                    if let Some(hex) = self.screen.pick(p, size) {
                        self.use_tool(hex);
                    }
                    if self.tool.strokes() {
                        self.stroke = Some(p);
                    }
                }
                MapCommand::Hover(p, size) => {
                    if let Some(last) = self.stroke {
                        self.paint_line(last, p, size);
                        self.stroke = Some(p);
                    }
                }
                MapCommand::Release => {
                    self.stroke = None;
                    self.history.commit();
                }
                MapCommand::Key(MapKey::Enter) => self.use_tool_at_cursor(),
                _ => {}
            }
        }
        for command in self.screen.pump_shared() {
            match command {
                RootCommand::Click(id) => self.clicked(id),
                RootCommand::Key(ch, mods) => self.key(ch, mods),
                // Enter or Esc in a text field: back to the map.
                RootCommand::Special(SpecialKey::Enter | SpecialKey::Esc, _) => self.focus_map(),
                _ => {}
            }
        }
        // The buttons and the keys may have changed the map.
        self.screen.sync_game();
        self.refresh();
    }

    fn focus_map(&mut self) {
        let map = self.screen.map_widget();
        self.screen.ui.set_focus(map);
    }

    fn map_has_focus(&self) -> bool {
        self.screen.ui.focused() == Some(self.screen.map_widget())
    }

    /// A button of the sidebar.
    fn clicked(&mut self, id: TbId) {
        if let Some(kind) = TileType::ALL.into_iter().find(|k| terrain_id(*k) == id) {
            self.set_brush(kind);
        } else if let Some(radius) = BRUSH_IDS.iter().position(|b| *b == id) {
            self.set_radius(radius as u8);
        } else if let Some(tool) = Tool::ALL.into_iter().find(|tool| tool.button() == id) {
            self.set_tool(tool);
        } else if id == tbid!("undo") {
            self.undo();
        } else if id == tbid!("redo") {
            self.redo();
        } else if id == tbid!("save") {
            self.save();
        } else if id == tbid!("open") {
            self.open();
        } else if id == tbid!("new") {
            self.new_map();
        }
    }

    /// A key nothing in the UI used.
    fn key(&mut self, ch: char, mods: Modifiers) {
        let ch = ch.to_ascii_lowercase();
        if mods.intersects(Modifiers::CTRL | Modifiers::SUPER) {
            match ch {
                's' => self.save(),
                'z' if mods.contains(Modifiers::SHIFT) => self.redo(),
                'z' => self.undo(),
                'y' => self.redo(),
                _ => {}
            }
            return;
        }
        // The rest are the map's keys: not while a text field has the keyboard.
        if mods.contains(Modifiers::ALT) || !self.map_has_focus() {
            return;
        }
        match ch {
            ' ' => self.use_tool_at_cursor(),
            '[' => self.set_radius(self.radius.saturating_sub(1)),
            ']' => self.set_radius(self.radius + 1),
            _ => {
                // A terrain's letter chooses it and paints at the cursor.
                // C++: main.cpp, the commented-out editor keys
                if let Some(kind) = TileType::from_char(ch) {
                    self.set_brush(kind);
                    self.paint(self.map().cursor);
                    self.history.commit();
                }
            }
        }
    }

    /// Choose the terrain to paint with.
    pub fn set_brush(&mut self, kind: TileType) {
        self.brush = kind;
        self.press_button(terrain_id(kind));
    }

    /// Choose the brush's size (0, 1 or 2: one hex, seven, nineteen).
    pub fn set_radius(&mut self, radius: u8) {
        self.radius = radius.min(MAX_RADIUS);
        self.press_button(BRUSH_IDS[usize::from(self.radius)]);
    }

    pub fn set_tool(&mut self, tool: Tool) {
        self.tool = tool;
        self.press_button(tool.button());
    }

    /// Show a button of a group as the chosen one (the others pop out).
    fn press_button(&mut self, id: TbId) {
        if let Some(button) = self.screen.ui.find(self.screen.ui.root(), id) {
            self.screen.ui.set_value(button, 1);
        }
    }

    /// Paint at `hex` with the brush. Part of the step in the making.
    pub fn paint(&mut self, hex: Hex) {
        let map = &mut self.screen.game.map;
        let mut changed = false;
        for target in footprint(map, hex, self.radius) {
            changed |= self.history.set(map, target, self.brush);
        }
        self.changed(changed);
    }

    /// Fill from `hex` with the chosen terrain. Part of the step in the making.
    pub fn fill(&mut self, hex: Hex) {
        let map = &mut self.screen.game.map;
        let mut changed = false;
        for target in region(map, hex) {
            changed |= self.history.set(map, target, self.brush);
        }
        self.changed(changed);
    }

    /// Lay a road on `hex`, or take it away. Part of the step in the making.
    pub fn road(&mut self, hex: Hex, road: bool) {
        let changed = self.history.set_road(&mut self.screen.game.map, hex, road);
        self.changed(changed);
    }

    fn changed(&mut self, changed: bool) {
        if changed {
            self.modified = true;
            self.armed = None;
            self.message.clear();
        }
    }

    fn use_tool(&mut self, hex: Hex) {
        match self.tool {
            Tool::Paint => self.paint(hex),
            Tool::Fill => self.fill(hex),
            Tool::Road => self.road(hex, true),
            Tool::Unroad => self.road(hex, false),
        }
    }

    /// Enter or Space: the tool at the cursor, as one step.
    fn use_tool_at_cursor(&mut self) {
        self.use_tool(self.map().cursor);
        self.history.commit();
    }

    /// Paint along the pointer's way from `from` to `to` (points of the map
    /// area, which is `size`), so that a fast drag leaves no gaps.
    fn paint_line(&mut self, from: Point, to: Point, size: Size) {
        let (dx, dy) = (to.x - from.x, to.y - from.y);
        let steps = (dx.abs().max(dy.abs()) / STROKE_STEP_PX).clamp(1, 1024);
        let mut last = None;
        for i in 1..=steps {
            let p = Point::new(from.x + dx * i / steps, from.y + dy * i / steps);
            let hex = self.screen.pick(p, size);
            if hex != last {
                last = hex;
                if let Some(hex) = hex {
                    self.use_tool(hex);
                }
            }
        }
    }

    pub fn undo(&mut self) {
        self.stroke = None;
        match self.history.undo(&mut self.screen.game.map) {
            Some(tiles) => {
                self.modified = true;
                self.armed = None;
                self.message = format!("Undid {tiles} {}", plural(tiles, "hex", "hexes"));
            }
            None => self.message = "Nothing to undo".to_string(),
        }
    }

    pub fn redo(&mut self) {
        match self.history.redo(&mut self.screen.game.map) {
            Some(tiles) => {
                self.modified = true;
                self.armed = None;
                self.message = format!("Redid {tiles} {}", plural(tiles, "hex", "hexes"));
            }
            None => self.message = "Nothing to redo".to_string(),
        }
    }

    /// The file name in the sidebar's field.
    fn typed_path(&mut self) -> Option<PathBuf> {
        let name = self.screen.ui.text(self.widgets.file).unwrap_or("").trim().to_string();
        if name.is_empty() {
            self.message = "Type a file name first (in the File field)".to_string();
            return None;
        }
        Some(PathBuf::from(name))
    }

    /// Write the map to the file named in the sidebar. For a map directory
    /// that is its terrain file; its units and cities are text, edited as
    /// text, and what they have to say about the new terrain (a city that is
    /// no longer on a city tile) is said after the saving.
    pub fn save(&mut self) {
        let Some(path) = self.typed_path() else { return };
        self.history.commit();
        let file = scenario::terrain_path(&path);
        match self.map().save(&file) {
            Ok(()) => {
                self.message = format!("Saved {}", file.display());
                if path.is_dir() {
                    match scenario::load(&path) {
                        Ok(whole) => {
                            if let Some(warning) = whole.warnings.first() {
                                self.message.push_str(&format!(" ({warning})"));
                            }
                        }
                        Err(e) => self.message.push_str(&format!(", but: {e}")),
                    }
                }
                self.path = Some(path);
                self.modified = false;
                self.armed = None;
            }
            Err(e) => self.message = format!("Not saved: {e}"),
        }
    }

    /// Read the file named in the sidebar, in place of the map being edited.
    pub fn open(&mut self) {
        let Some(path) = self.typed_path() else { return };
        // Find out that the file is no good before asking to discard anything.
        // Of a map directory the editor has the terrain; the rest is text.
        let map = match Map::load(&scenario::terrain_path(&path)) {
            Ok(map) => map,
            Err(e) => {
                self.message = format!("Not opened: {e}");
                return;
            }
        };
        if !self.may_discard(Discard::Open) {
            return;
        }
        self.message = format!("Opened {}", path.display());
        self.replace_map(map, Some(path));
    }

    /// Start a map of the size in the sidebar's fields, all of the chosen terrain.
    pub fn new_map(&mut self) {
        let number = |id: WidgetId| self.screen.ui.text(id).unwrap_or("").trim().parse::<u16>().ok();
        let (Some(rows), Some(cols)) = (number(self.widgets.rows), number(self.widgets.cols)) else {
            self.message = "New map: the rows and columns have to be numbers".to_string();
            return;
        };
        let map = match Map::new(rows, cols, self.brush) {
            Ok(map) => map,
            Err(e) => {
                self.message = format!("New map: {e}");
                return;
            }
        };
        if !self.may_discard(Discard::New) {
            return;
        }
        self.message = format!("New map, {rows} x {cols}, all {}. It has no file yet.", self.brush.name());
        self.replace_map(map, None);
        self.screen.ui.set_text(self.widgets.file, "");
    }

    fn replace_map(&mut self, map: Map, path: Option<PathBuf>) {
        self.screen.set_game(Game::new(map));
        self.history.clear();
        self.stroke = None;
        self.path = path;
        self.modified = false;
        self.armed = None;
        self.show_map_in_fields();
        // The minimap takes the new map's shape.
        self.apply_layout_sizes();
    }

    /// The file's name and the map's size, in the sidebar's fields.
    fn show_map_in_fields(&mut self) {
        if let Some(path) = &self.path {
            self.screen.ui.set_text(self.widgets.file, &path.display().to_string());
        }
        let (rows, cols) = (self.map().rows(), self.map().cols());
        self.screen.ui.set_text(self.widgets.rows, &rows.to_string());
        self.screen.ui.set_text(self.widgets.cols, &cols.to_string());
    }

    /// True if nothing would be lost, or the player asked for this a second
    /// time in a row. Otherwise the status line says what is at stake.
    fn may_discard(&mut self, what: Discard) -> bool {
        if !self.modified || self.armed == Some(what) {
            self.armed = None;
            return true;
        }
        self.armed = Some(what);
        let again = match what {
            Discard::Quit => "Quit",
            Discard::Open => "Open",
            Discard::New => "New",
        };
        self.message = format!("The map has unsaved changes. {again} again to lose them, or save first.");
        false
    }

    /// Bring what is shown up to date with what the editor knows: the brush's
    /// preview, the undo buttons and the status line.
    fn refresh(&mut self) {
        let cursor = self.map().cursor;
        self.preview = if self.tool == Tool::Paint && self.radius > 0 {
            footprint(self.map(), cursor, self.radius)
        } else {
            Vec::new()
        };

        let ui = &mut self.screen.ui;
        for (button, enabled) in
            [(self.widgets.undo, self.history.can_undo()), (self.widgets.redo, self.history.can_redo())]
        {
            if ui.node(button).is_some_and(|n| n.is_disabled()) == enabled {
                ui.set_disabled(button, !enabled);
            }
        }

        let status = self.status_text();
        if status != self.status {
            self.screen.ui.set_text(self.widgets.status, &status);
            self.status = status;
        }
    }

    fn status_text(&self) -> String {
        let map = self.map();
        let cursor = map.cursor;
        let terrain = map.tile(cursor).map_or(String::new(), |t| describe_terrain(t.kind, t.road));
        let tool = match self.tool {
            Tool::Paint => format!("Paint {}, {}", self.brush.name(), [1, 7, 19][usize::from(self.radius)]),
            Tool::Fill => format!("Fill with {}", self.brush.name()),
            Tool::Road => "Lay road".to_string(),
            Tool::Unroad => "Remove road".to_string(),
        };
        let file = match &self.path {
            Some(path) => path.display().to_string(),
            None => "no file".to_string(),
        };
        let modified = if self.modified { " (changed)" } else { "" };
        let mut status = format!(
            "  {} x {}   |   row {}, column {}: {terrain}   |   {tool}   |   {file}{modified}",
            map.rows(),
            map.cols(),
            cursor.row,
            cursor.col
        );
        if !self.message.is_empty() {
            status.push_str("   |   ");
            status.push_str(&self.message);
        }
        status
    }
}

fn plural<'a>(n: usize, one: &'a str, many: &'a str) -> &'a str {
    if n == 1 { one } else { many }
}

impl Scene for Editor {
    fn screen(&self) -> &MapScreen {
        &self.screen
    }

    fn screen_mut(&mut self) -> &mut MapScreen {
        &mut self.screen
    }

    fn resize(&mut self, viewport: Size) {
        Editor::resize(self, viewport);
    }

    fn pump(&mut self) {
        Editor::pump(self);
    }

    fn frame(&mut self, now_ms: u64) {
        self.screen.frame(now_ms);
        self.refresh();
    }

    /// The hexes the brush would paint.
    fn marked(&self, hex: Hex) -> bool {
        self.preview.contains(&hex)
    }

    fn mark_color(&self) -> [f32; 4] {
        BRUSH_TINT
    }

    /// Quitting with unsaved changes has to be asked for twice.
    fn confirm_quit(&mut self) -> bool {
        let sure = self.may_discard(Discard::Quit);
        self.refresh();
        sure
    }
}
