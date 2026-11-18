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
use crate::gui::{self, MapCommand, MapKey, MapWidget, RootCommand, RootWindow};
use crate::minimap_view::MiniMapView;
use crate::state::Direction;
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
        ui.set_focusable(root_window, true);

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
        ui.set_rect(main_layout, ui.padding_rect(root_window));

        // The C++ left the focus on the root, so the arrows did nothing until the
        // map was clicked. Start on the map instead (deviation D11).
        if !ui.set_focus(map) {
            ui.set_focus(root_window);
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
        };
        session.apply_layout_sizes();
        session.ui.update();
        session.sync_views();
        Ok(session)
    }

    /// Size the map and the minimap from the window's size: the map is 80% of the
    /// width and 70% of the height, the minimap what is left of each.
    ///
    /// C++: assets/layouts/main.lua, which did this once at startup; here the
    /// sizes follow the window (deviation D14).
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

    /// One frame of host work before drawing: the time, the UI's own update, and
    /// the views' sizes. C++: Application::processFrame
    pub fn frame(&mut self, now_ms: u64) {
        self.ui.set_time(now_ms);
        self.ui.update();
        self.sync_views();
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

    /// Apply what the widgets recorded since the last call. The host calls this
    /// after every input event it forwards to the UI.
    pub fn pump(&mut self) {
        let commands = self.ui.with_widget::<MapWidget, _>(self.map, |w, _| w.take_commands()).unwrap_or_default();
        for command in commands {
            self.apply(command);
        }
        let commands =
            self.ui.with_widget::<RootWindow, _>(self.root_window, |w, _| w.take_commands()).unwrap_or_default();
        for command in commands {
            match command {
                RootCommand::Quit => self.quit = true,
                RootCommand::ToggleFullscreen => self.fullscreen_toggles += 1,
            }
        }
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
                MapKey::ZoomOut => {
                    self.view.zoom_out();
                    self.view.center_on(self.game.map.cursor);
                }
                MapKey::ZoomIn => {
                    self.view.zoom_in();
                    self.view.center_on(self.game.map.cursor);
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

    /// The terrain changed: the minimap has to be drawn again.
    pub fn mark_minimap_dirty(&mut self) {
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
