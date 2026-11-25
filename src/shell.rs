//! The game's window, from start to end: the new game screen, then the game
//! it sets up, and the new game screen again when a game is over and a new
//! one is asked for. The window runs one scene (`app::App`); this is a scene
//! that is now the one, now the other. A game that is left before it is
//! over is kept in a file (`save`), if the shell was told where
//! (`keep_games_in`): the new game screen lists those, to be played on.
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
    /// Where the games in progress are kept, if anywhere (and the
    /// settings, `settings`).
    saves: Option<PathBuf>,
    /// The interface's size the screen is built at, in percent of what the
    /// system says (`app::ui_dpi`), and the system's DPI (the screen's is
    /// the skin's nearest, which does not say it back).
    ui_scale: u32,
    system_dpi: Option<i32>,
}

impl Shell {
    /// Begin with the new game screen, `chosen` (a map's path) chosen.
    pub fn new_game(assets: &Path, viewport: Size, dpi: i32, chosen: Option<&Path>) -> Result<Shell, String> {
        let screen = NewGame::new(assets, viewport, dpi, chosen)?;
        let now = Now::NewGame(Box::new(screen));
        let (options, ui_scale, system_dpi) = (Options::default(), 100, None);
        Ok(Shell { assets: assets.to_path_buf(), now, options, saves: None, ui_scale, system_dpi })
    }

    /// Begin with a game.
    pub fn playing(assets: &Path, session: Session) -> Shell {
        let options = session.options();
        let now = Now::Playing(Box::new(session));
        Shell { assets: assets.to_path_buf(), now, options, saves: None, ui_scale: 100, system_dpi: None }
    }

    /// The interface's size the screen was built at (`--ui-scale`, the
    /// environment's, the settings'), and the DPI that made (`app::ui_dpi`
    /// of the system's): for the Options window to show, and to change from.
    pub fn set_ui_scale(&mut self, percent: u32, dpi: i32) {
        self.ui_scale = percent;
        self.system_dpi = Some((dpi * 100 + percent as i32 / 2) / percent.max(1) as i32);
        self.options.ui_scale = percent;
        if let Now::Playing(session) = &mut self.now {
            let options = Options { ui_scale: percent, ..session.options() };
            session.set_options(options);
        }
    }

    /// The screen built again at the size the Options window chose, around
    /// the same game (its windows close); the size kept in the settings.
    fn rebuild(&mut self, viewport: Size, dpi: i32) {
        let Now::Playing(session) = &mut self.now else { return };
        let parts = session.take_parts();
        let options = parts.options;
        // The system's DPI (or, not told, from the one it was built at).
        let system = self.system_dpi.unwrap_or((dpi * 100 + self.ui_scale as i32 / 2) / self.ui_scale.max(1) as i32);
        let dpi = crate::app::ui_dpi(system, Some(options.ui_scale));
        match Session::with_options(&self.assets, viewport, dpi, parts.game, options) {
            Ok(mut fresh) => {
                fresh.restore(parts.journal, parts.seen);
                if let Some(path) = parts.kept {
                    fresh.keep_in(path);
                }
                self.now = Now::Playing(Box::new(fresh));
                (self.ui_scale, self.options) = (options.ui_scale, options);
                if let Some(dir) = &self.saves {
                    if let Err(e) = crate::settings::set_ui_scale(dir, options.ui_scale) {
                        crate::platform::complain(&format!("hexlords: {e}"));
                    }
                }
            }
            Err(e) => crate::platform::complain(&format!("hexlords: the screen was not built again: {e}")),
        }
    }

    /// Keep the games that are not over in files in `dir` (`save`), from
    /// now on: the one being played, if one is, in a new file there; and
    /// the new game screen lists those there are.
    pub fn keep_games_in(&mut self, dir: PathBuf) {
        match &mut self.now {
            Now::NewGame(screen) => screen.set_saves(Some(dir.clone())),
            Now::Playing(session) => keep(session, &dir),
        }
        self.saves = Some(dir);
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
        if matches!(&self.now, Now::Playing(session) if session.wants_rebuild()) {
            self.rebuild(viewport, dpi);
            return;
        }
        match &mut self.now {
            Now::NewGame(screen) if screen.wants_to_start() => {
                let game = screen.take_game();
                match Session::with_options(&self.assets, viewport, dpi, game, self.options) {
                    Ok(mut session) => {
                        if let Some(dir) = &self.saves {
                            keep(&mut session, dir);
                        }
                        self.now = Now::Playing(Box::new(session));
                    }
                    Err(e) => crate::platform::complain(&format!("hexlords: the game did not start: {e}")),
                }
            }
            // A game in progress, to be played on: as it was left.
            Now::NewGame(screen) => {
                let Some(path) = screen.take_resume() else { return };
                let session = crate::save::load_file(&path).and_then(|loaded| {
                    let mut session = Session::with_options(&self.assets, viewport, dpi, loaded.game, self.options)?;
                    session.restore(loaded.steps, loaded.seen);
                    session.keep_in(path);
                    Ok(session)
                });
                match session {
                    Ok(session) => self.now = Now::Playing(Box::new(session)),
                    Err(why) => screen.resume_failed(why),
                }
            }
            Now::Playing(session) if session.wants_new_game() => {
                // (Left before it is over: kept, to be played on.)
                session.save_now();
                self.options = session.options();
                match NewGame::new(&self.assets, viewport, dpi, None) {
                    Ok(mut screen) => {
                        screen.set_saves(self.saves.clone());
                        self.now = Now::NewGame(Box::new(screen));
                    }
                    Err(e) => crate::platform::complain(&format!("hexlords: {e}")),
                }
            }
            Now::Playing(_) => {}
        }
    }
}

/// Keep `session`'s game in a new file in `dir`, if it can be kept (it
/// knows the texts its map was read from, and has sides).
fn keep(session: &mut Session, dir: &Path) {
    // (A map with no sides is no game to play on: its file could not be
    // read.)
    if session.screen.game.setup().seats.is_empty() {
        return;
    }
    if let Some(source) = session.screen.game.source() {
        let path = crate::save::fresh(dir, &source.name);
        session.keep_in(path);
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

    fn minimap_marks(&self) -> crate::render::MiniMarks {
        self.scene().minimap_marks()
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
