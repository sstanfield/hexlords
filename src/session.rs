//! The game without its window: a map screen (the UI, the map and the views)
//! with the game's layout and what the player's input does to the game. It
//! needs no graphics context, so tests drive it the way the window does.
//!
//! C++: assets/layouts/main.lua (the layout), src/gui/mapwidget.cpp
//! MapWidget::OnEvent (what input does to the map)

use std::path::Path;

use turbo_ferret::Size;

use crate::game::Game;
use crate::gui::{MapCommand, MapKey};
use crate::screen::{MapScreen, Scene};
use crate::state::Hex;

/// The layout, relative to the asset directory.
pub const LAYOUT: &str = "layouts/main.tb.txt";

/// The main map takes this share of the window's width and height; the minimap
/// gets the rest of each. C++: assets/layouts/main.lua
const MAP_SHARE: (f32, f32) = (0.8, 0.7);
/// The space between widgets in the layout, in dp (`spacing: 5` in the layout).
const SPACING_DP: i32 = 5;

pub struct Session {
    pub screen: MapScreen,
}

impl Session {
    /// Set up the UI for a window of `viewport` pixels at `dpi` with the skin and
    /// the layout from the asset directory, and start the demo game.
    pub fn new(assets: &Path, viewport: Size, dpi: i32) -> Result<Session, String> {
        Session::with_game(assets, viewport, dpi, Game::demo())
    }

    /// The same, playing `game`.
    pub fn with_game(assets: &Path, viewport: Size, dpi: i32, game: Game) -> Result<Session, String> {
        let screen = MapScreen::new(assets, viewport, dpi, LAYOUT, game)?;
        let mut session = Session { screen };
        session.apply_layout_sizes();
        session.screen.settle();
        Ok(session)
    }

    /// Size the map and the minimap from the window's size: the map is 80% of the
    /// width and 70% of the height, the minimap what is left of each. Called again
    /// whenever the window's size changes. C++: assets/layouts/main.lua
    fn apply_layout_sizes(&mut self) {
        let Size { w, h } = self.screen.ui.viewport();
        let spacing = self.screen.ui.dimension_converter().dp_to_px(SPACING_DP);
        let map_w = (w as f32 * MAP_SHARE.0).floor() as i32;
        let map_h = (h as f32 * MAP_SHARE.1).floor() as i32;
        self.screen.set_map_sizes(Size::new(map_w, map_h), Size::new(w - map_w - spacing, h - map_h));
    }

    /// The window was resized.
    pub fn resize(&mut self, viewport: Size) {
        self.screen.ui.set_viewport(viewport);
        self.apply_layout_sizes();
    }

    /// One frame of host work before drawing (see `MapScreen::frame`).
    pub fn frame(&mut self, now_ms: u64) {
        self.screen.frame(now_ms);
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
                        self.screen.game.select_hex(hex);
                    }
                }
                MapCommand::Key(MapKey::Enter) => {
                    self.screen.game.select_hex(self.screen.game.map.cursor);
                }
                _ => {}
            }
        }
        // Nothing in the game's layout has a meaning yet beyond the map.
        self.screen.pump_shared();
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

    /// Where the selected unit can go.
    fn marked(&self, hex: Hex) -> bool {
        self.screen.game.can_move_to(hex)
    }
}
