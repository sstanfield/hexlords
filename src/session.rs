//! Everything about a running game except the window and the GPU: the UI and
//! what the host does with its commands. It needs no graphics context, so tests
//! drive it the way the window does.
//!
//! C++: src/Application.cpp Application (the Turbo Badger setup and the frame)

use std::path::Path;

use turbo_ferret::font::DEFAULT_FONT;
use turbo_ferret::{Config, FontDesc, FsLoader, Gravity, Rect, Services, Size, Ui, WidgetId, tbid};

use crate::gui::{RootCommand, RootWindow};

pub struct Session {
    pub ui: Ui,
    root_window: WidgetId,
    quit: bool,
    fullscreen_toggles: u32,
}

impl Session {
    /// Set up the UI for a window of `viewport` pixels at `dpi`, with the skin from
    /// the asset directory. C++: Application::Application
    pub fn new(assets: &Path, viewport: Size, dpi: i32) -> Result<Session, String> {
        let mut ui = Ui::new(Config::for_dpi(dpi), Services::with_loader(FsLoader::new(assets)));
        ui.load_skin("skin/skin.tb.txt", None).map_err(|e| format!("skin: {e} (assets: {})", assets.display()))?;
        // The built-in font at 14 dp. (The C++ used DroidSans at this size.)
        let size = ui.dimension_converter().dp_to_px(14).clamp(1, 255) as u16;
        ui.set_default_font(FontDesc::new(DEFAULT_FONT, size));
        ui.set_viewport(viewport);

        // The root window covers the viewport and follows its size.
        let root = ui.root();
        let root_window = ui.add(root, RootWindow::default());
        ui.set_id(root_window, tbid!("RootWindow"));
        ui.set_gravity(root_window, Gravity::ALL);
        ui.set_rect(root_window, Rect::from_size(viewport));
        ui.set_focusable(root_window, true);
        ui.set_focus(root_window);

        Ok(Session { ui, root_window, quit: false, fullscreen_toggles: 0 })
    }

    /// The window was resized.
    pub fn resize(&mut self, viewport: Size) {
        self.ui.set_viewport(viewport);
    }

    /// One frame of host work before drawing: the time, then the UI's own update.
    /// C++: Application::processFrame
    pub fn frame(&mut self, now_ms: u64) {
        self.ui.set_time(now_ms);
        self.ui.update();
    }

    /// Apply what the widgets recorded since the last call. The host calls this
    /// after every input event it forwards to the UI.
    pub fn pump(&mut self) {
        let commands =
            self.ui.with_widget::<RootWindow, _>(self.root_window, |w, _| w.take_commands()).unwrap_or_default();
        for command in commands {
            match command {
                RootCommand::Quit => self.quit = true,
                RootCommand::ToggleFullscreen => self.fullscreen_toggles += 1,
            }
        }
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

    pub fn root_window(&self) -> WidgetId {
        self.root_window
    }
}
