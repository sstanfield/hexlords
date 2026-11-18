//! Everything about a running game except the window and the GPU: the UI, the
//! game, the views, and what the host does with the widgets' commands. It needs
//! no graphics context, so tests drive it the way the window does.
//!
//! C++: src/Application.cpp Application (the Turbo Badger setup and the frame),
//! assets/layouts/main.lua (the layout), src/gui/mapwidget.cpp MapWidget::OnEvent
//! (what input does to the map)

use std::path::Path;

use turbo_ferret::font::DEFAULT_FONT;
use turbo_ferret::{
    Config, FontDesc, FsLoader, Gravity, LayoutParams, Point, Rect, Services, Size, Ui, WidgetId, tbid,
};

use crate::game::Game;
use crate::gui::{self, MapCommand, MapKey, MapWidget, MiniMapCommand, MiniMapWidget, RootCommand, RootWindow};
use crate::minimap_view::MiniMapView;
use crate::state::{Direction, Hex};
use crate::view::MapView;

/// The layout, relative to the asset directory.
pub const LAYOUT: &str = "layouts/main.tb.txt";

/// The main map takes this share of the window's width and height; the minimap
/// gets the rest of each. C++: assets/layouts/main.lua
const MAP_SHARE: (f32, f32) = (0.8, 0.7);
/// The space between widgets in the layout, in dp (`spacing: 5` in the layout).
const SPACING_DP: i32 = 5;

pub struct Session {
    pub ui: Ui,
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
}

impl Session {
    /// Set up the UI for a window of `viewport` pixels at `dpi` with the skin and
    /// the layout from the asset directory, and start the demo game.
    /// C++: Application::Application
    pub fn new(assets: &Path, viewport: Size, dpi: i32) -> Result<Session, String> {
        let in_assets = |what: &str, e: &dyn std::fmt::Display| format!("{what}: {e} (assets: {})", assets.display());
        let mut ui = Ui::new(Config::for_dpi(dpi), Services::with_loader(FsLoader::new(assets)));
        ui.load_skin("skin/skin.tb.txt", None).map_err(|e| in_assets("skin", &e))?;
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
        ui.inflate_file(root_window, LAYOUT).map_err(|e| in_assets(LAYOUT, &e))?;
        let find = |ui: &Ui, name: &str| {
            ui.find(root_window, turbo_ferret::TbId::new(name)).ok_or(format!("{LAYOUT}: no widget with id {name}"))
        };
        let main_layout = find(&ui, "main_layout")?;
        let map = find(&ui, "main_map")?;
        let minimap = find(&ui, "mini_map")?;
        if ui.get::<MapWidget>(map).is_none() {
            return Err(format!("{LAYOUT}: main_map is not a MapWidget"));
        }
        if ui.get::<MiniMapWidget>(minimap).is_none() {
            return Err(format!("{LAYOUT}: mini_map is not a MiniMapWidget"));
        }
        ui.set_rect(main_layout, ui.padding_rect(root_window));

        // The map has the keyboard, and keeps it: nothing else in the layout takes
        // the focus, so the map's keys work whatever was clicked last. Keys the
        // map does not use bubble up to the root window.
        if !ui.set_focus(map) {
            return Err(format!("{LAYOUT}: main_map cannot take the keyboard focus"));
        }

        let game = Game::demo();
        let (rows, cols) = (game.map.rows(), game.map.cols());
        let view = MapView::new(rows, cols, game.map.cursor);
        let mini_view = MiniMapView::new(rows, cols);
        let mut session = Session {
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
            terrain_seen: 0,
        };
        session.terrain_seen = session.game.map.terrain_revision();
        session.apply_layout_sizes();
        session.ui.update();
        session.sync_views();
        Ok(session)
    }

    /// Size the map and the minimap from the window's size: the map is 80% of the
    /// width and 70% of the height, the minimap what is left of each. Called again
    /// whenever the window's size changes. C++: assets/layouts/main.lua
    fn apply_layout_sizes(&mut self) {
        let Size { w, h } = self.ui.viewport();
        let spacing = self.ui.dimension_converter().dp_to_px(SPACING_DP);
        let map_w = (w as f32 * MAP_SHARE.0).floor() as i32;
        let map_h = (h as f32 * MAP_SHARE.1).floor() as i32;
        self.ui.set_layout_params(self.map, Some(LayoutParams::fixed(map_w, map_h)));
        let mini = LayoutParams::fixed((w - map_w - spacing).max(0), (h - map_h).max(0));
        self.ui.set_layout_params(self.minimap, Some(mini));
    }

    /// The window was resized.
    pub fn resize(&mut self, viewport: Size) {
        self.ui.set_viewport(viewport);
        self.apply_layout_sizes();
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
    fn sync_game(&mut self) {
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
    fn area(&self, id: WidgetId) -> Rect {
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
    /// the map's rectangle.
    pub fn hex_pixel(&self, hex: Hex) -> Option<Point> {
        let [x, y] = self.view.to_ndc(self.view.hex_center(hex));
        let r = self.map_rect();
        let p = Point::new(r.x + ((x + 1.0) / 2.0 * r.w as f32) as i32, r.y + ((1.0 - y) / 2.0 * r.h as f32) as i32);
        (x.abs() < 1.0 && y.abs() < 1.0 && r.contains(p)).then_some(p)
    }

    /// The window pixel of a hex on the minimap.
    pub fn minimap_pixel(&self, hex: Hex) -> Point {
        let [x, y] = self.mini_view.to_ndc(self.mini_view.hex_center(hex));
        let r = self.minimap_rect();
        Point::new(r.x + ((x + 1.0) / 2.0 * r.w as f32) as i32, r.y + ((1.0 - y) / 2.0 * r.h as f32) as i32)
    }

    /// Apply what the widgets recorded since the last call. The host calls this
    /// after every input event it forwards to the UI.
    pub fn pump(&mut self) {
        let commands = self.ui.with_widget::<MapWidget, _>(self.map, |w, _| w.take_commands()).unwrap_or_default();
        for command in commands {
            self.apply(command);
        }
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
        for command in commands {
            match command {
                RootCommand::Quit => self.quit = true,
                RootCommand::ToggleFullscreen => self.fullscreen_toggles += 1,
            }
        }
        self.sync_game();
    }

    /// C++: MapWidget::OnEvent
    fn apply(&mut self, command: MapCommand) {
        match command {
            // The hex under the pointer becomes the cursor. The view stays.
            MapCommand::Hover(p, size) => {
                if let Some(hex) = self.view.pick_pixel(p.x, p.y, size.w, size.h) {
                    self.game.set_cursor(hex);
                }
            }
            MapCommand::Click(p, size) => {
                if let Some(hex) = self.view.pick_pixel(p.x, p.y, size.w, size.h) {
                    self.game.set_cursor(hex);
                    self.game.select_hex(hex);
                }
            }
            MapCommand::Key(key) => match key {
                MapKey::Left => self.step(Direction::Left),
                MapKey::Right => self.step(Direction::Right),
                MapKey::Up => self.step(Direction::Up),
                MapKey::Down => self.step(Direction::Down),
                MapKey::Enter => {
                    self.game.select_hex(self.game.map.cursor);
                }
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
            },
        }
    }

    /// An arrow key: the cursor takes a step, and the view follows if that took
    /// the cursor out of it.
    fn step(&mut self, dir: Direction) {
        self.game.move_cursor(dir);
        self.view.keep_cursor_visible(self.game.map.cursor, dir);
    }

    /// True once the player asked to quit.
    pub fn quit_requested(&self) -> bool {
        self.quit
    }

    /// How many times the player asked to switch fullscreen on or off since the
    /// last call.
    pub fn take_fullscreen_toggles(&mut self) -> u32 {
        std::mem::take(&mut self.fullscreen_toggles)
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
