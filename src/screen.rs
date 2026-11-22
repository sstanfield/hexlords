//! What every window of this project is made of: a Turbo Ferret `Ui` with a
//! main map and a minimap in it, the map they show, and the two views. The game
//! (`session::Session`) and the tools (the map editor) each put their own layout
//! and their own rules on top of a [`MapScreen`], and the window (`app::App`)
//! runs anything that is a [`Scene`]. None of it needs a graphics context, so
//! tests drive it the way the window does.
//!
//! Moving the map about is done here for every window: the keys and the
//! minimap through the widgets, and what the widgets never see (the wheel, a
//! trackpad's two fingers, a drag with the right button) through
//! [`MapScreen::scroll`], [`MapScreen::zoom_at`] and [`MapScreen::grab`],
//! which the window calls. So is the help window (F1, or `?`): every key, the
//! ones all windows share and the scene's own ([`MapScreen::set_help`]).
//!
//! C++: src/Application.cpp Application (the Turbo Badger setup and the frame),
//! src/gui/mapwidget.cpp MapWidget::OnEvent (the cursor and zoom keys)

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use turbo_ferret::font::DEFAULT_FONT;
use turbo_ferret::{
    Config, FontDesc, Gravity, LayoutParams, LoadError, Point, Rect, ResourceLoader, Services, Size, SpecialKey, TbId,
    Ui, WidgetId, tbid,
};

use crate::files;
use crate::game::Game;
use crate::gui::build;
use crate::gui::{self, MapCommand, MapKey, MapWidget, MiniMapCommand, MiniMapWidget, RootCommand, RootWindow};
use crate::help::{self, HelpSection};
use crate::minimap_view::MiniMapView;
use crate::render::RANGE_OVERLAY;
use crate::state::{Direction, Hex};
use crate::view::MapView;

/// The ids a layout file must give its widgets to be a map screen: the layout
/// that fills the window, the main map (a `MapWidget`) and the minimap (a
/// `MiniMapWidget`).
pub const MAIN_LAYOUT_ID: &str = "main_layout";
pub const MAP_ID: &str = "main_map";
pub const MINIMAP_ID: &str = "mini_map";
/// The help window, and its button that closes it.
pub const HELP_ID: &str = "help_window";
pub const HELP_CLOSE_ID: &str = "help_close";
/// The help window's title when it has the keys.
pub const KEYS_TITLE: &str = "Help: keys and mouse";
/// What every window's menus may hold, as the keys for them do: the help's
/// keys, fullscreen, quit, zoom (`MapScreen::pump_shared` does them).
pub const HELP_KEYS_ID: &str = "help_keys";
pub const FULLSCREEN_ID: &str = "fullscreen";
pub const QUIT_ID: &str = "quit";
pub const ZOOM_IN_ID: &str = "zoom_in";
pub const ZOOM_OUT_ID: &str = "zoom_out";
/// The space between the help's columns, and between its sections, in dp.
const HELP_COLUMN_DP: i32 = 16;
const HELP_SECTION_DP: i32 = 8;
/// What a window takes besides its content's room: its title bar, its
/// padding and a button row under the content, in dp; and a scroll bar's
/// width.
const HELP_CHROME_DP: i32 = 120;
const HELP_SCROLL_BAR_DP: i32 = 18;
/// How far, in dp, the pointer goes with the button held on the main map
/// before it is a drag that moves the map rather than a click (in a scene
/// that wants that, [`MapScreen::set_drag_pans`]).
pub const DRAG_DP: i32 = 6;

/// Turbo Ferret's files (the skin, the layouts, the pictures), read from the
/// asset directory through [`crate::files`]: from the disk, or in a browser
/// from inside the program.
pub struct AssetLoader {
    root: PathBuf,
}

impl AssetLoader {
    pub fn new(root: impl Into<PathBuf>) -> AssetLoader {
        AssetLoader { root: root.into() }
    }
}

impl ResourceLoader for AssetLoader {
    fn load(&mut self, path: &str) -> Result<Cow<'static, [u8]>, LoadError> {
        match files::read(&self.root.join(path)) {
            Ok(bytes) => Ok(Cow::Owned(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(LoadError::NotFound(path.to_string())),
            Err(e) => Err(LoadError::Io(format!("{path}: {e}"))),
        }
    }
}

pub struct MapScreen {
    pub ui: Ui,
    /// The map, with what is selected on it. (A tool that only edits the map
    /// never selects anything.)
    pub game: Game,
    /// What the main map shows.
    pub view: MapView,
    /// What the minimap shows.
    pub mini_view: MiniMapView,
    root_window: WidgetId,
    map: WidgetId,
    minimap: WidgetId,
    quit: bool,
    fullscreen_toggles: u32,
    /// The minimap's picture is out of date (its size or the terrain changed).
    minimap_dirty: bool,
    /// The map's terrain revision the minimap's picture was last asked for.
    terrain_seen: u64,
    /// Where the pointer holds the main map (see [`MapScreen::grab`]).
    grab: Option<Point>,
    /// A drag with the (left) button on the main map moves it (see
    /// [`MapScreen::set_drag_pans`]); the press, while the button is down;
    /// and whether the last press was such a drag, whose click is no click.
    drag_pans: bool,
    press: Option<Press>,
    panned: bool,
    /// The scene's own sections of the help, between the map's and the window's.
    help_sections: Vec<HelpSection>,
    /// The help window, while it is open, and its title.
    help: Option<WidgetId>,
    /// The band along the top the help window keeps clear of (a scene's menu
    /// bar: the help's own menu stays at hand), in pixels.
    clear_top: i32,
    help_title: String,
}

/// The button held down on the main map: where it went down, where the
/// pointer was last, and whether it has moved far enough to move the map.
#[derive(Copy, Clone, Debug)]
struct Press {
    at: Point,
    last: Point,
    panning: bool,
}

/// One of the screen's two maps.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum Which {
    Map,
    MiniMap,
}

impl MapScreen {
    /// Set up the UI for a window of `viewport` pixels at `dpi`, with the skin
    /// and the layout file `layout` from the asset directory, showing `game`.
    ///
    /// The map widgets have no size of their own: the caller gives them one with
    /// [`MapScreen::set_map_sizes`] and then calls [`MapScreen::settle`].
    /// C++: Application::Application
    pub fn new(assets: &Path, viewport: Size, dpi: i32, layout: &str, game: Game) -> Result<MapScreen, String> {
        let in_assets = |what: &str, e: &dyn std::fmt::Display| format!("{what}: {e} (assets: {})", assets.display());
        let mut ui = Ui::new(Config::for_dpi(dpi), Services::with_loader(AssetLoader::new(assets)));
        // Turbo Badger's default skin, with the game's few changes on top.
        ui.load_skin("skin/skin.tb.txt", Some("skin/hexlords.tb.txt")).map_err(|e| in_assets("skin", &e))?;
        // The built-in font at 14 dp. (The C++ used DroidSans at this size.)
        let size = ui.dimension_converter().dp_to_px(14).clamp(1, 255) as u16;
        ui.set_default_font(FontDesc::new(DEFAULT_FONT, size));
        gui::register(&mut ui);
        ui.set_viewport(viewport);

        // The root window covers the viewport and follows its size.
        let root = ui.root();
        let root_window = ui.add(root, RootWindow::default());
        ui.set_id(root_window, tbid!("RootWindow"));
        ui.set_gravity(root_window, Gravity::ALL);
        ui.set_rect(root_window, Rect::from_size(viewport));

        // The layout fills the root window (layouts follow their parent's size).
        ui.inflate_file(root_window, layout).map_err(|e| in_assets(layout, &e))?;
        let find = |ui: &Ui, name: &str| {
            ui.find(root_window, turbo_ferret::TbId::new(name)).ok_or(format!("{layout}: no widget with id {name}"))
        };
        let main_layout = find(&ui, MAIN_LAYOUT_ID)?;
        let map = find(&ui, MAP_ID)?;
        let minimap = find(&ui, MINIMAP_ID)?;
        if ui.get::<MapWidget>(map).is_none() {
            return Err(format!("{layout}: {MAP_ID} is not a MapWidget"));
        }
        if ui.get::<MiniMapWidget>(minimap).is_none() {
            return Err(format!("{layout}: {MINIMAP_ID} is not a MiniMapWidget"));
        }
        ui.set_rect(main_layout, ui.padding_rect(root_window));

        // The map has the keyboard. Keys it does not use bubble up to the root
        // window.
        if !ui.set_focus(map) {
            return Err(format!("{layout}: {MAP_ID} cannot take the keyboard focus"));
        }

        let (rows, cols) = (game.map.rows(), game.map.cols());
        let mut view = MapView::new(rows, cols, game.map.cursor);
        view.set_dpi(ui.dimension_converter().dp_to_px(96));
        let mini_view = MiniMapView::new(rows, cols);
        let terrain_seen = game.map.terrain_revision();
        Ok(MapScreen {
            ui,
            game,
            view,
            mini_view,
            root_window,
            map,
            minimap,
            quit: false,
            fullscreen_toggles: 0,
            minimap_dirty: true,
            terrain_seen,
            grab: None,
            drag_pans: false,
            press: None,
            panned: false,
            help_sections: Vec::new(),
            help: None,
            clear_top: 0,
            help_title: String::new(),
        })
    }

    /// The scene's own sections of the help (its keys and clicks), shown
    /// between the map's and the window's. Takes effect the next time the help
    /// opens.
    pub fn set_help(&mut self, sections: Vec<HelpSection>) {
        self.help_sections = sections;
    }

    /// Everything the help window says, section by section.
    pub fn help(&self) -> Vec<HelpSection> {
        help::sections(&self.help_sections)
    }

    /// True while the help window is open.
    pub fn help_open(&self) -> bool {
        self.help.is_some_and(|id| self.ui.node(id).is_some())
    }

    /// Open the help window, or close it if it is open.
    pub fn toggle_help(&mut self) {
        if self.help_open() {
            self.close_help();
        } else {
            self.open_help();
        }
    }

    /// Close the help window, if it is open.
    pub fn close_help(&mut self) {
        if let Some(id) = self.help.take() {
            if self.ui.node(id).is_some() {
                self.ui.close_window(id);
            }
        }
    }

    /// Show a topic of the help (a scene's Help menu): `sections` of words and
    /// what they say, in the help window, which stands for whatever it showed.
    pub fn show_topic(&mut self, title: &str, sections: &[HelpSection]) {
        self.close_help();
        self.open_help_window(title, sections);
    }

    /// The title of what the help window shows, while it is open.
    pub fn help_title(&self) -> Option<&str> {
        self.help.filter(|id| self.ui.node(*id).is_some()).map(|_| self.help_title.as_str())
    }

    /// Keep the help window clear of a band `px` tall along the top (the
    /// scene's menu bar).
    pub fn set_clear_top(&mut self, px: i32) {
        self.clear_top = px.max(0);
    }

    /// The help window with every key.
    fn open_help(&mut self) {
        let sections = help::sections(&self.help_sections);
        self.open_help_window(KEYS_TITLE, &sections);
    }

    /// A window in the middle of the screen with `sections`: a column of what
    /// to press (or a word) and one of what it does, under a heading for each
    /// section, and a button that closes it. It takes nothing from the map:
    /// the keyboard stays there, and a click beside the window goes to
    /// whatever is under it.
    fn open_help_window(&mut self, title: &str, sections: &[HelpSection]) {
        let dp = |ui: &Ui, dp: i32| ui.dimension_converter().dp_to_px(dp);
        let ui = &mut self.ui;
        let parts = build::window(ui, self.root_window, HELP_ID, title);
        // The sections scroll when they do not fit under the menu bar; the
        // Close button stays under them.
        let scroll = ui.add(
            parts.content,
            turbo_ferret::widgets::ScrollContainer::new(turbo_ferret::widgets::ScrollMode::YAuto)
                .with_adapt_content_size(true),
        );
        let content = build::column(ui, scroll, 0);
        let mut keys = Vec::new();
        for (i, section) in sections.iter().enumerate() {
            if i > 0 {
                let px = dp(ui, HELP_SECTION_DP);
                build::gap(ui, content, px);
            }
            build::heading(ui, content, &section.title);
            for (press, what) in &section.rows {
                let spacing = dp(ui, HELP_COLUMN_DP);
                let row = build::row(ui, content, spacing);
                keys.push(build::text(ui, row, press));
                build::text(ui, row, what);
            }
        }
        let px = dp(ui, HELP_SECTION_DP);
        build::gap(ui, parts.content, px);
        build::button(ui, parts.content, HELP_CLOSE_ID, "Close");
        // The keys' column is as wide as its widest.
        let width = keys.iter().map(|key| ui.preferred_size_free(*key).pref.w).max().unwrap_or(0);
        for key in keys {
            build::set_width(ui, key, width);
        }
        // In the middle of what is under the menu bar, if the scene has one;
        // as tall as the sections, or as fits there.
        let size = ui.rect(self.root_window).size();
        let under = Rect::new(0, self.clear_top, size.w, (size.h - self.clear_top).max(0));
        let pref = ui.preferred_size_free(content).pref;
        let room = (under.h - dp(ui, HELP_CHROME_DP)).max(dp(ui, 120));
        let bar = if pref.h > room { dp(ui, HELP_SCROLL_BAR_DP) } else { 0 };
        ui.set_layout_params(scroll, Some(turbo_ferret::LayoutParams::fixed(pref.w + bar, pref.h.min(room))));
        build::place(ui, parts, under, (0.5, 0.5));
        self.help = Some(parts.window);
        self.help_title = title.to_string();
    }

    /// Show another map: the views start over, on its cursor.
    pub fn set_game(&mut self, game: Game) {
        let (rows, cols) = (game.map.rows(), game.map.cols());
        self.view = MapView::new(rows, cols, game.map.cursor);
        self.view.set_dpi(self.ui.dimension_converter().dp_to_px(96));
        self.mini_view = MiniMapView::new(rows, cols);
        self.terrain_seen = game.map.terrain_revision();
        self.game = game;
        self.minimap_dirty = true;
        self.grab = None;
        self.sync_views();
    }

    /// Give the main map and the minimap their sizes in the layout, in pixels.
    pub fn set_map_sizes(&mut self, map: Size, minimap: Size) {
        self.ui.set_layout_params(self.map, Some(LayoutParams::fixed(map.w.max(0), map.h.max(0))));
        self.ui.set_layout_params(self.minimap, Some(LayoutParams::fixed(minimap.w.max(0), minimap.h.max(0))));
    }

    /// Lay the UI out now and give the views their widgets' sizes, without
    /// waiting for the next frame.
    pub fn settle(&mut self) {
        self.ui.update();
        self.sync_views();
    }

    /// One frame of host work before drawing: the time, the UI's own update, the
    /// views' sizes, and whatever depends on a map that may have changed.
    /// C++: Application::processFrame
    pub fn frame(&mut self, now_ms: u64) {
        self.ui.set_time(now_ms);
        self.ui.update();
        self.sync_views();
        self.sync_game();
    }

    /// The map may have changed since the last look: bring the selected unit's
    /// range up to date, and ask for a new minimap picture if the terrain did.
    pub fn sync_game(&mut self) {
        self.game.refresh();
        let terrain = self.game.map.terrain_revision();
        if terrain != self.terrain_seen {
            self.terrain_seen = terrain;
            self.minimap_dirty = true;
        }
    }

    /// Give the views the sizes the layout gave their widgets.
    fn sync_views(&mut self) {
        let map = self.map_rect().size();
        if self.view.size() != (map.w, map.h) {
            self.view.resize(map.w, map.h);
        }
        let mini = self.minimap_rect().size();
        if self.mini_view.size() != (mini.w, mini.h) {
            self.mini_view.resize(mini.w, mini.h);
            self.minimap_dirty = true;
        }
    }

    /// A widget's area inside its padding, in window pixels (top left origin).
    pub fn area(&self, id: WidgetId) -> Rect {
        self.ui.padding_rect(id).offset_by(self.ui.to_root(id, Point::ZERO))
    }

    /// Where the main map is drawn, in window pixels (top left origin).
    pub fn map_rect(&self) -> Rect {
        self.area(self.map)
    }

    /// Where the minimap is drawn, in window pixels (top left origin).
    pub fn minimap_rect(&self) -> Rect {
        self.area(self.minimap)
    }

    /// The window pixel in the middle of a hex of the main map, if that is inside
    /// the map's rectangle. Of a hex cut in half at the map's top or bottom edge
    /// (row 0's odd columns, the last row's even ones), whose middle is on the
    /// edge, the middle of the half that is drawn.
    pub fn hex_pixel(&self, hex: Hex) -> Option<Point> {
        let [x, mut y] = self.view.hex_center(hex);
        let odd = hex.col % 2 == 1;
        if hex.row == 0 && odd {
            y -= 0.5;
        } else if hex.row + 1 == self.game.map.rows() && !odd {
            y += 0.5;
        }
        let [x, y] = self.view.to_ndc([x, y]);
        let r = self.map_rect();
        let p = Point::new(r.x + ((x + 1.0) / 2.0 * r.w as f32) as i32, r.y + ((1.0 - y) / 2.0 * r.h as f32) as i32);
        (x.abs() < 1.0 && y.abs() < 1.0 && r.contains(p)).then_some(p)
    }

    /// The window pixel of a hex on the minimap: the one its middle is on, or
    /// the minimap's last pixel that way where the middle is on the map's very
    /// edge (the last row's even columns are cut through their middle).
    pub fn minimap_pixel(&self, hex: Hex) -> Point {
        let [x, y] = self.mini_view.to_ndc(self.mini_view.hex_center(hex));
        let r = self.minimap_rect();
        let within = |from: i32, share: f32, len: i32| from + ((share * len as f32) as i32).clamp(0, (len - 1).max(0));
        Point::new(within(r.x, (x + 1.0) / 2.0, r.w), within(r.y, (1.0 - y) / 2.0, r.h))
    }

    /// What the map widget recorded since the last call, oldest first.
    ///
    /// In a scene where a drag moves the map ([`MapScreen::set_drag_pans`]),
    /// the button held down on the main map and moved more than [`DRAG_DP`]
    /// takes the map along with the pointer: those moves come as
    /// `MapCommand::Pan` (which [`MapScreen::navigate`] carries out, in their
    /// turn), and the click its release would make is not handed on.
    pub fn take_map_commands(&mut self) -> Vec<MapCommand> {
        let commands = self.ui.with_widget::<MapWidget, _>(self.map, |w, _| w.take_commands()).unwrap_or_default();
        if !self.drag_pans {
            return commands;
        }
        let far = self.ui.dimension_converter().dp_to_px(DRAG_DP);
        let mut out = Vec::with_capacity(commands.len());
        for command in commands {
            match command {
                MapCommand::Press(p, _) => {
                    self.press = Some(Press { at: p, last: p, panning: false });
                    self.panned = false;
                }
                MapCommand::Hover(p, _) => {
                    if let Some(press) = self.press.as_mut() {
                        if press.panning || (p.x - press.at.x).abs().max((p.y - press.at.y).abs()) > far {
                            press.panning = true;
                            out.push(MapCommand::Pan(p.x - press.last.x, p.y - press.last.y));
                            press.last = p;
                            continue;
                        }
                    }
                }
                MapCommand::Release => self.panned = self.press.take().is_some_and(|p| p.panning),
                MapCommand::Click(..) if std::mem::take(&mut self.panned) => continue,
                _ => {}
            }
            out.push(command);
        }
        out
    }

    /// Whether a drag with the button on the main map moves the map (the
    /// game, the new game screen) or is the scene's (the editor paints with
    /// it). Off at first.
    pub fn set_drag_pans(&mut self, on: bool) {
        self.drag_pans = on;
    }

    /// The hex of the main map under a point a map command carries.
    pub fn pick(&self, p: Point, size: Size) -> Option<Hex> {
        self.view.pick_pixel(p.x, p.y, size.w, size.h)
    }

    /// What a map command does in every window: the hex under the pointer
    /// becomes the cursor, the arrows move it, `1` and `2` zoom. What a press, a
    /// click or Enter means is the caller's business. C++: MapWidget::OnEvent
    pub fn navigate(&mut self, command: MapCommand) {
        match command {
            // The view stays where it is.
            MapCommand::Hover(p, size) | MapCommand::Press(p, size) | MapCommand::Click(p, size, _) => {
                if let Some(hex) = self.pick(p, size) {
                    self.game.set_cursor(hex);
                }
            }
            MapCommand::Key(key) => match key {
                MapKey::Left => self.step(Direction::Left),
                MapKey::Right => self.step(Direction::Right),
                MapKey::Up => self.step(Direction::Up),
                MapKey::Down => self.step(Direction::Down),
                // A zoom centers on the cursor. At the last level nothing zooms,
                // and the view stays where it is.
                MapKey::ZoomOut => {
                    if self.view.zoom_out() {
                        self.view.center_on(self.game.map.cursor);
                    }
                }
                MapKey::ZoomIn => {
                    if self.view.zoom_in() {
                        self.view.center_on(self.game.map.cursor);
                    }
                }
                MapKey::Enter | MapKey::ShiftEnter => {}
                MapKey::Esc => self.close_help(),
            },
            MapCommand::Pan(dx, dy) => {
                self.view.pan_pixels(dx as f32, dy as f32);
            }
            MapCommand::Release => {}
        }
    }

    /// An arrow key: the cursor takes a step, and the view follows if that took
    /// the cursor out of it. If the view was moved away from the cursor (the
    /// minimap, a scroll), it comes back to it.
    fn step(&mut self, dir: Direction) {
        self.game.move_cursor(dir);
        let cursor = self.game.map.cursor;
        if self.view.shows(cursor) {
            self.view.keep_cursor_visible(cursor, dir);
        } else {
            self.view.center_on(cursor);
        }
    }

    /// Which map a window pixel is on, if a pointer there reaches it (nothing
    /// of the UI lies over it).
    fn map_at(&self, at: Point) -> Option<Which> {
        let top = self.ui.widget_at(self.ui.root(), at)?;
        if top == self.map {
            Some(Which::Map)
        } else if top == self.minimap {
            Some(Which::MiniMap)
        } else {
            None
        }
    }

    /// A point of the window in a rectangle's normalized device coordinates (x
    /// right, y up, -1..1 inside it).
    fn ndc_in(r: Rect, at: Point) -> (f32, f32) {
        let (half_w, half_h) = (r.w.max(1) as f32 / 2.0, r.h.max(1) as f32 / 2.0);
        (((at.x - r.x) as f32 + 0.5 - half_w) / half_w, -((at.y - r.y) as f32 + 0.5 - half_h) / half_h)
    }

    /// The cursor goes to the hex of the main map under a window pixel, if
    /// there is one.
    fn cursor_to_pointer(&mut self, at: Point) {
        let r = self.map_rect();
        if let Some(hex) = self.view.pick_pixel(at.x - r.x, at.y - r.y, r.w, r.h) {
            self.game.set_cursor(hex);
        }
    }

    /// The wheel turned, or two fingers moved on a trackpad, by `(dx, dy)`
    /// pixels (x right, y down) with the pointer at the window pixel `at`.
    /// The view goes the same way on both maps, against the fingers: on the
    /// main map the map moves with them by that many pixels, and on the
    /// minimap, whose picture stays, the view's marker goes that far the other
    /// way (so a short stroke there crosses the map). False, and nothing
    /// happens, if the pointer is on neither.
    pub fn scroll(&mut self, at: Point, dx: f32, dy: f32) -> bool {
        match self.map_at(at) {
            Some(Which::Map) => {
                self.view.pan_pixels(dx, dy);
                // Another hex is under the pointer now.
                self.cursor_to_pointer(at);
            }
            Some(Which::MiniMap) => {
                let (w, h) = self.mini_view.size();
                let (wf, hf) = self.mini_view.factors();
                if w > 0 && h > 0 {
                    self.view.pan(-dx * 2.0 / w as f32 / wf, dy * 2.0 / h as f32 / hf);
                    // As with a press on the minimap: the keys carry on from here.
                    self.game.set_cursor(self.view.center());
                }
            }
            None => return false,
        }
        true
    }

    /// Zoom one level in (`closer`) or out with the pointer at the window pixel
    /// `at`: on the main map what is under the pointer stays there, on the
    /// minimap the middle of the view does. False, and nothing happens, if the
    /// pointer is on neither.
    pub fn zoom_at(&mut self, at: Point, closer: bool) -> bool {
        match self.map_at(at) {
            Some(Which::Map) => {
                let (x, y) = Self::ndc_in(self.map_rect(), at);
                self.view.zoom_about(x, y, closer);
                self.cursor_to_pointer(at);
            }
            Some(Which::MiniMap) => {
                self.view.zoom_about(0.0, 0.0, closer);
            }
            None => return false,
        }
        true
    }

    /// True if a zoom with the pointer at this window pixel is the maps'
    /// business (see [`MapScreen::zoom_at`]).
    pub fn zoom_target(&self, at: Point) -> bool {
        self.map_at(at).is_some()
    }

    /// Take hold of the main map at a window pixel, to move it with
    /// [`MapScreen::drag`] (the window does this with the right and the middle
    /// button). False if the pixel is not on the main map.
    pub fn grab(&mut self, at: Point) -> bool {
        self.grab = (self.map_at(at) == Some(Which::Map)).then_some(at);
        self.grab.is_some()
    }

    /// The pointer moved to a window pixel: a map that is held goes with it.
    /// False if none is.
    pub fn drag(&mut self, to: Point) -> bool {
        let Some(from) = self.grab else { return false };
        self.view.pan_pixels((to.x - from.x) as f32, (to.y - from.y) as f32);
        self.grab = Some(to);
        true
    }

    /// Let go of the map. False if it was not held.
    pub fn let_go(&mut self) -> bool {
        self.grab.take().is_some()
    }

    /// Apply what the minimap and the root window recorded since the last call,
    /// and bring what depends on the map up to date. Every scene's `pump` calls
    /// this. It returns what the root window heard and has no meaning here (the
    /// clicks on buttons, the keys nothing used), oldest first, for the scene to
    /// give one.
    pub fn pump_shared(&mut self) -> Vec<RootCommand> {
        let commands =
            self.ui.with_widget::<MiniMapWidget, _>(self.minimap, |w, _| w.take_commands()).unwrap_or_default();
        for MiniMapCommand::Goto(p, size) in commands {
            // The main map looks at the hex that was pointed at, and the cursor
            // goes there too, so that the keys carry on from it.
            if let Some(hex) = self.mini_view.pick_pixel(p.x, p.y, size.w, size.h) {
                self.game.set_cursor(hex);
                self.view.center_on(hex);
            }
        }
        let commands =
            self.ui.with_widget::<RootWindow, _>(self.root_window, |w, _| w.take_commands()).unwrap_or_default();
        let mut rest = Vec::new();
        for command in commands {
            match command {
                RootCommand::Quit => self.quit = true,
                RootCommand::ToggleFullscreen => self.fullscreen_toggles += 1,
                RootCommand::Help => self.toggle_help(),
                // The help's button, and Esc while it is open, close it.
                RootCommand::Click(id) if id == TbId::new(HELP_CLOSE_ID) => self.close_help(),
                // The same from a menu; the scene hears of the click too (it
                // closes the menu).
                RootCommand::Click(id) if id == TbId::new(HELP_KEYS_ID) => {
                    // The keys: over a topic too (the keys again close them).
                    if self.help_title() == Some(KEYS_TITLE) {
                        self.close_help();
                    } else {
                        self.close_help();
                        self.open_help();
                    }
                    rest.push(command);
                }
                RootCommand::Click(id) if id == TbId::new(FULLSCREEN_ID) => {
                    self.fullscreen_toggles += 1;
                    rest.push(command);
                }
                RootCommand::Click(id) if id == TbId::new(QUIT_ID) => {
                    self.quit = true;
                    rest.push(command);
                }
                RootCommand::Click(id) if id == TbId::new(ZOOM_IN_ID) || id == TbId::new(ZOOM_OUT_ID) => {
                    let key = if id == TbId::new(ZOOM_IN_ID) { MapKey::ZoomIn } else { MapKey::ZoomOut };
                    self.navigate(MapCommand::Key(key));
                    rest.push(command);
                }
                RootCommand::Special(SpecialKey::Esc, _) if self.help_open() => self.close_help(),
                other => rest.push(other),
            }
        }
        self.sync_game();
        rest
    }

    /// True once the player asked to quit (Ctrl+Q).
    pub fn quit_requested(&self) -> bool {
        self.quit
    }

    /// Forget that the player asked to quit (the scene said no).
    pub fn cancel_quit(&mut self) {
        self.quit = false;
    }

    /// How many times the player asked to switch fullscreen on or off since the
    /// last call.
    pub fn take_fullscreen_toggles(&mut self) -> u32 {
        std::mem::take(&mut self.fullscreen_toggles)
    }

    /// Ask for the minimap's picture to be drawn again (what is seen of it
    /// changed).
    pub fn redraw_minimap(&mut self) {
        self.minimap_dirty = true;
    }

    /// True if the minimap's picture is out of date; the caller redraws it.
    pub fn take_minimap_dirty(&mut self) -> bool {
        std::mem::take(&mut self.minimap_dirty)
    }

    pub fn root_window(&self) -> WidgetId {
        self.root_window
    }

    /// The main map's widget.
    pub fn map_widget(&self) -> WidgetId {
        self.map
    }

    /// The minimap's widget.
    pub fn minimap_widget(&self) -> WidgetId {
        self.minimap
    }
}

/// Something the window can run: a map screen and the rules on top of it.
pub trait Scene {
    fn screen(&self) -> &MapScreen;

    fn screen_mut(&mut self) -> &mut MapScreen;

    /// The window's size changed.
    fn resize(&mut self, viewport: Size);

    /// Apply what the widgets recorded since the last call. The host calls this
    /// after every input event it forwards to the UI.
    fn pump(&mut self);

    /// One frame of host work before drawing.
    fn frame(&mut self, now_ms: u64) {
        self.screen_mut().frame(now_ms);
    }

    /// Whether the main map lays a tint over this hex.
    fn marked(&self, hex: Hex) -> bool;

    /// The unit that shows on its hex whoever else stands there: the selected
    /// one, in the game.
    fn front_unit(&self) -> Option<crate::state::UnitId> {
        None
    }

    /// The hexes the main map outlines: in the game, the stops of a route.
    fn outlined(&self) -> &[Hex] {
        &[]
    }

    /// The hexes the main map rings in a color of their own (over the
    /// cursor's border and `outlined`): in the game, the selected stack's
    /// hex, the way it is to take, where a battle is.
    fn rings(&self) -> Vec<(Hex, [f32; 3])> {
        Vec::new()
    }

    /// A hex with a tint of its own over it: in the game, where a battle
    /// is being fought.
    fn flash(&self) -> Option<(Hex, [f32; 4])> {
        None
    }

    /// The flags on the main map, each with its number: in the game, the
    /// rally points of the person at the screen.
    fn flags(&self) -> &[(Hex, u8)] {
        &[]
    }

    /// The stacks shown going from hex to hex: in the game, the moves being
    /// shown.
    fn moving(&self) -> Vec<crate::render::Moving> {
        Vec::new()
    }

    /// The tint of the marked hexes: red, green, blue and how much of it.
    fn mark_color(&self) -> [f32; 4] {
        RANGE_OVERLAY
    }

    /// The player wants to quit (Ctrl+Q, or the window's close button): false
    /// keeps the window open.
    fn confirm_quit(&mut self) -> bool {
        true
    }

    /// Whose units show what they are on the map (the others' are banners):
    /// the person at the screen. `None`: everyone's (a tool).
    fn viewer(&self) -> Option<crate::state::Side> {
        None
    }

    /// What who is at the screen knows of the map (fog of war): all of it,
    /// in a tool.
    fn sight(&self) -> &crate::game::Sight {
        &crate::game::SEE_ALL
    }

    /// A round in one step, for scripts (`turn`): the person's turn ends, the
    /// others play theirs, the person's next starts. False where there are
    /// no turns (a tool), or it is nobody's turn to end.
    fn next_turn(&mut self) -> bool {
        false
    }
}
