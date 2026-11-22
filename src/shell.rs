//! The game's window, from start to end: the new game screen, then the game
//! it sets up, and the new game screen again when a game is over and a new
//! one is asked for. The window runs one scene (`app::App`); this is a scene
//! that is now the one, now the other.
//!
//! Not a port: the C++ started its demo at once.

use std::path::{Path, PathBuf};

use turbo_ferret::Size;

use crate::new_game::NewGame;
use crate::options_window::Options;
use crate::screen::{MapScreen, Scene};
use crate::session::Session;
use crate::state::{Hex, UnitId};

enum Now {
    NewGame(Box<NewGame>),
    Playing(Box<Session>),
}

pub struct Shell {
    assets: PathBuf,
    now: Now,
    /// The person's options, from one game to the next.
    options: Options,
}

impl Shell {
    /// Begin with the new game screen, `chosen` (a map's path) chosen.
    pub fn new_game(assets: &Path, viewport: Size, dpi: i32, chosen: Option<&Path>) -> Result<Shell, String> {
        let screen = NewGame::new(assets, viewport, dpi, chosen)?;
        Ok(Shell { assets: assets.to_path_buf(), now: Now::NewGame(Box::new(screen)), options: Options::default() })
    }

    /// Begin with a game.
    pub fn playing(assets: &Path, session: Session) -> Shell {
        let options = session.options();
        Shell { assets: assets.to_path_buf(), now: Now::Playing(Box::new(session)), options }
    }

    /// The new game screen, while it shows.
    pub fn new_game_screen(&mut self) -> Option<&mut NewGame> {
        match &mut self.now {
            Now::NewGame(screen) => Some(screen),
            Now::Playing(_) => None,
        }
    }

    /// The game, while it is played.
    pub fn session(&mut self) -> Option<&mut Session> {
        match &mut self.now {
            Now::Playing(session) => Some(session),
            Now::NewGame(_) => None,
        }
    }

    fn scene(&self) -> &dyn Scene {
        match &self.now {
            Now::NewGame(screen) => screen.as_ref(),
            Now::Playing(session) => session.as_ref(),
        }
    }

    fn scene_mut(&mut self) -> &mut dyn Scene {
        match &mut self.now {
            Now::NewGame(screen) => screen.as_mut(),
            Now::Playing(session) => session.as_mut(),
        }
    }

    /// From the one to the other, if that was asked for.
    fn switch(&mut self) {
        let ui = &self.scene().screen().ui;
        let (viewport, dpi) = (ui.viewport(), ui.dimension_converter().dp_to_px(96));
        match &mut self.now {
            Now::NewGame(screen) if screen.wants_to_start() => {
                let game = screen.take_game();
                match Session::with_options(&self.assets, viewport, dpi, game, self.options) {
                    Ok(session) => self.now = Now::Playing(Box::new(session)),
                    Err(e) => crate::platform::complain(&format!("hexlords: the game did not start: {e}")),
                }
            }
            Now::Playing(session) if session.wants_new_game() => {
                self.options = session.options();
                match NewGame::new(&self.assets, viewport, dpi, None) {
                    Ok(screen) => self.now = Now::NewGame(Box::new(screen)),
                    Err(e) => crate::platform::complain(&format!("hexlords: {e}")),
                }
            }
            _ => {}
        }
    }
}

impl Scene for Shell {
    fn screen(&self) -> &MapScreen {
        self.scene().screen()
    }

    fn screen_mut(&mut self) -> &mut MapScreen {
        self.scene_mut().screen_mut()
    }

    fn resize(&mut self, viewport: Size) {
        self.scene_mut().resize(viewport);
    }

    fn pump(&mut self) {
        self.scene_mut().pump();
        self.switch();
    }

    fn frame(&mut self, now_ms: u64) {
        self.scene_mut().frame(now_ms);
    }

    fn marked(&self, hex: Hex) -> bool {
        self.scene().marked(hex)
    }

    fn front_unit(&self) -> Option<UnitId> {
        self.scene().front_unit()
    }

    fn mark_color(&self) -> [f32; 4] {
        self.scene().mark_color()
    }

    fn outlined(&self) -> &[Hex] {
        self.scene().outlined()
    }

    fn rings(&self) -> Vec<(Hex, [f32; 3])> {
        self.scene().rings()
    }

    fn flash(&self) -> Option<(Hex, [f32; 4])> {
        self.scene().flash()
    }

    fn battles(&self) -> Vec<(Hex, f32)> {
        self.scene().battles()
    }

    fn flags(&self) -> &[(Hex, u8)] {
        self.scene().flags()
    }

    fn moving(&self) -> Vec<crate::render::Moving> {
        self.scene().moving()
    }

    fn confirm_quit(&mut self) -> bool {
        self.scene_mut().confirm_quit()
    }

    fn next_turn(&mut self) -> bool {
        self.scene_mut().next_turn()
    }

    fn viewer(&self) -> Option<crate::state::Side> {
        self.scene().viewer()
    }

    fn sight(&self) -> &crate::game::Sight {
        self.scene().sight()
    }
}
