//! The window: a miniquad `EventHandler` that feeds input to the session and
//! draws the frame.
//!
//! C++: src/main.cpp (the GLFW window, callbacks and main loop)

use std::path::PathBuf;
use std::time::Instant;

use miniquad::{Backend, EventHandler, KeyCode, KeyMods, MouseButton, PassAction, TouchPhase, window};
use turbo_ferret::Size;
use turbo_ferret_miniquad::{InputAdapter, MiniquadClipboard, MiniquadRenderer, window_dpi};

use crate::paths::asset_root;
use crate::session::Session;

/// The window's size at start, in logical pixels. C++: main.cpp
pub const WINDOW_SIZE: (i32, i32) = (1024, 768);
pub const WINDOW_TITLE: &str = "HEX Game!";
/// The color behind everything. C++: main.cpp glClearColor
pub const CLEAR_COLOR: (f32, f32, f32, f32) = (0.5, 0.5, 0.5, 1.0);

/// The command line.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// `--assets <dir>`: where the assets are (see `paths::asset_root`).
    pub assets: Option<PathBuf>,
    /// `--frames <n>`: quit after drawing this many frames.
    pub frames: Option<u64>,
}

pub struct App {
    session: Session,
    renderer: MiniquadRenderer,
    input: InputAdapter,
    start: Instant,
    frames: u64,
    fullscreen: bool,
    options: Options,
}

impl App {
    /// Call inside `miniquad::start`'s closure (it needs the window).
    pub fn new(options: Options) -> Result<App, String> {
        let renderer = MiniquadRenderer::new(window::new_rendering_backend()).map_err(|e| format!("shader: {e}"))?;
        if renderer.context().info().backend == Backend::Metal {
            return Err("the Metal backend is not supported: start miniquad with OpenGL (its default)".to_string());
        }
        let (w, h) = window::screen_size();
        let assets = asset_root(options.assets.as_deref());
        let mut session = Session::new(&assets, Size::new(w as i32, h as i32), window_dpi())?;
        // Copy and paste with the rest of the system.
        session.ui.services_mut().clipboard = Box::new(MiniquadClipboard::new());
        Ok(App {
            session,
            renderer,
            input: InputAdapter::new(),
            start: Instant::now(),
            frames: 0,
            fullscreen: false,
            options,
        })
    }

    fn now(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }

    /// The UI's clock is data: keep it current before input (click counting reads it).
    fn tick(&mut self) {
        let t = self.now();
        self.session.ui.set_time(t);
    }

    /// After input: apply what the widgets recorded.
    fn after_input(&mut self) {
        self.session.pump();
        if self.session.quit_requested() {
            window::order_quit();
        }
        if self.session.take_fullscreen_toggles() % 2 == 1 {
            self.fullscreen = !self.fullscreen;
            window::set_fullscreen(self.fullscreen);
        }
    }
}

impl EventHandler for App {
    fn update(&mut self) {
        let t = self.now();
        self.session.frame(t);
    }

    fn draw(&mut self) {
        let (r, g, b, a) = CLEAR_COLOR;
        self.renderer.context_mut().begin_default_pass(PassAction::clear_color(r, g, b, a));
        self.session.ui.paint(&mut self.renderer);
        let ctx = self.renderer.context_mut();
        ctx.end_render_pass();
        ctx.commit_frame();

        self.frames += 1;
        if self.options.frames == Some(self.frames) {
            let vp = self.session.ui.viewport();
            println!("{} frames drawn at {}x{}, {} dpi; quitting", self.frames, vp.w, vp.h, window_dpi());
            window::order_quit();
        }
    }

    fn resize_event(&mut self, width: f32, height: f32) {
        self.session.resize(Size::new(width.round().max(0.0) as i32, height.round().max(0.0) as i32));
    }

    fn mouse_motion_event(&mut self, x: f32, y: f32) {
        self.tick();
        self.input.mouse_motion_event(&mut self.session.ui, x, y);
        self.after_input();
    }

    fn mouse_wheel_event(&mut self, x: f32, y: f32) {
        self.tick();
        self.input.mouse_wheel_event(&mut self.session.ui, x, y);
        self.after_input();
    }

    fn mouse_button_down_event(&mut self, button: MouseButton, x: f32, y: f32) {
        self.tick();
        self.input.mouse_button_down_event(&mut self.session.ui, button, x, y);
        self.after_input();
    }

    fn mouse_button_up_event(&mut self, button: MouseButton, x: f32, y: f32) {
        self.tick();
        self.input.mouse_button_up_event(&mut self.session.ui, button, x, y);
        self.after_input();
    }

    fn char_event(&mut self, character: char, keymods: KeyMods, repeat: bool) {
        self.tick();
        self.input.char_event(&mut self.session.ui, character, keymods, repeat);
        self.after_input();
    }

    fn key_down_event(&mut self, keycode: KeyCode, keymods: KeyMods, repeat: bool) {
        self.tick();
        self.input.key_down_event(&mut self.session.ui, keycode, keymods, repeat);
        self.after_input();
    }

    fn key_up_event(&mut self, keycode: KeyCode, keymods: KeyMods) {
        self.tick();
        self.input.key_up_event(&mut self.session.ui, keycode, keymods);
        self.after_input();
    }

    fn touch_event(&mut self, phase: TouchPhase, id: u64, x: f32, y: f32) {
        self.tick();
        self.input.touch_event(&mut self.session.ui, phase, id, x, y);
        self.after_input();
    }

    fn window_minimized_event(&mut self) {
        self.input.focus_lost(&mut self.session.ui);
    }
}
