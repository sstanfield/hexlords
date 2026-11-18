//! The window: a miniquad `EventHandler` that feeds input to the session and
//! draws the frame.
//!
//! C++: src/main.cpp (the GLFW window, callbacks and main loop)

use std::path::PathBuf;
use std::time::Instant;

use miniquad::{
    Backend, EventHandler, KeyCode, KeyMods, MouseButton, PassAction, RenderPass, RenderingBackend, TouchPhase, window,
};
use turbo_ferret::{Rect, Size};
use turbo_ferret_miniquad::{InputAdapter, MiniquadClipboard, MiniquadRenderer, window_dpi};

use crate::paths::asset_root;
use crate::platform;
use crate::render::textures::texture_params;
use crate::render::{MapRenderer, MiniMapRenderer, Pipelines, gl_rect, png_out};
use crate::script;
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
    /// `--frames <n>`: quit after drawing this many frames (at least 1).
    pub frames: Option<u64>,
    /// `--screenshot <file>`: draw a frame into this PNG file and quit. It is the
    /// last frame of `--frames`, or the second without it.
    pub screenshot: Option<PathBuf>,
    /// `--script <steps>`: input to play before the first frame (see `script`).
    pub script: Option<String>,
}

/// The earliest frame a screenshot is taken of: the second, when the layout has
/// settled.
const SCREENSHOT_FRAME: u64 = 2;

pub struct App {
    session: Session,
    /// Turbo Ferret's renderer. It owns the miniquad context, which the game's
    /// own renderers borrow from it.
    renderer: MiniquadRenderer,
    pipelines: Pipelines,
    map_renderer: MapRenderer,
    minimap_renderer: MiniMapRenderer,
    input: InputAdapter,
    start: Instant,
    frames: u64,
    fullscreen: bool,
    options: Options,
}

impl App {
    /// Call inside `miniquad::start`'s closure (it needs the window).
    pub fn new(options: Options) -> Result<App, String> {
        let mut renderer =
            MiniquadRenderer::new(window::new_rendering_backend()).map_err(|e| format!("shader: {e}"))?;
        if renderer.context().info().backend == Backend::Metal {
            return Err("the Metal backend is not supported: start miniquad with OpenGL (its default)".to_string());
        }
        let (w, h) = window::screen_size();
        let assets = asset_root(options.assets.as_deref());
        let mut session = Session::new(&assets, Size::new(w as i32, h as i32), window_dpi())?;
        // Copy and paste with the rest of the system.
        session.ui.services_mut().clipboard = Box::new(MiniquadClipboard::new());
        if let Some(steps) = &options.script {
            script::run(&mut session, steps).map_err(|e| format!("--script: {e}"))?;
        }

        // The game's own pipelines and textures live on the renderer's context.
        let ctx = renderer.context_mut();
        let pipelines = Pipelines::new(ctx).map_err(|e| format!("shader: {e}"))?;
        let map_renderer = MapRenderer::new(ctx, &mut *session.ui.services_mut().loader)
            .map_err(|e| format!("{e} (assets: {})", assets.display()))?;
        Ok(App {
            session,
            renderer,
            pipelines,
            map_renderer,
            minimap_renderer: MiniMapRenderer::new(),
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

    /// Draw a frame into `target` (`None`: the window): the map and the minimap
    /// into their widgets' rectangles, then the UI on top. Turbo Ferret widgets
    /// cannot draw with the GPU themselves, so the map widgets paint nothing and
    /// the host does it here (plan, section 3.3).
    /// C++: Application::processFrame, GLWidget::OnPaint
    fn render(&mut self, target: Option<RenderPass>) {
        let framebuffer = self.session.ui.viewport();
        let dirty = self.session.take_minimap_dirty();
        let session = &self.session;
        let ctx = self.renderer.context_mut();

        // The minimap's picture is drawn into its own target, before the frame's
        // pass begins.
        self.minimap_renderer.update(ctx, &self.pipelines, &session.game.map, &session.mini_view, dirty);

        let (r, g, b, a) = CLEAR_COLOR;
        ctx.begin_pass(target, PassAction::clear_color(r, g, b, a));
        if set_viewport(ctx, session.map_rect(), framebuffer) {
            self.map_renderer.draw(ctx, &self.pipelines, &session.game, &session.view);
        }
        if set_viewport(ctx, session.minimap_rect(), framebuffer) {
            self.minimap_renderer.draw(ctx, &self.pipelines, session.mini_view.marker(&session.view));
        }
        // Back to the whole framebuffer for the UI, which draws on top.
        set_viewport(ctx, Rect::from_size(framebuffer), framebuffer);
        self.session.ui.paint(&mut self.renderer);
        self.renderer.context_mut().end_render_pass();
    }

    /// Draw a frame into a texture of the window's size and write it as a PNG.
    fn screenshot(&mut self) -> Result<(), String> {
        let Some(path) = self.options.screenshot.clone() else { return Ok(()) };
        let Size { w, h } = self.session.ui.viewport();
        if w <= 0 || h <= 0 {
            return Err("the window has no size".to_string());
        }
        let ctx = self.renderer.context_mut();
        let texture = ctx.new_render_texture(texture_params(w as u32, h as u32));
        let pass = ctx.new_render_pass(texture, None);
        self.render(Some(pass));

        let ctx = self.renderer.context_mut();
        let row = w as usize * 4;
        let mut pixels = vec![0u8; row * h as usize];
        ctx.texture_read_pixels(texture, &mut pixels);
        ctx.delete_render_pass(pass); // and its texture
        // The rows come bottom first.
        let top_down: Vec<u8> = pixels.chunks_exact(row).rev().flatten().copied().collect();
        let png = png_out::encode_rgb(w as u32, h as u32, &top_down).ok_or("could not encode the picture")?;
        std::fs::write(&path, png).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("wrote {} ({w}x{h})", path.display());
        Ok(())
    }
}

/// Point the viewport and the scissor at a rectangle of the window (top left
/// origin). False, and nothing is set, if none of it is inside the framebuffer.
fn set_viewport(ctx: &mut dyn RenderingBackend, rect: Rect, framebuffer: Size) -> bool {
    let visible = rect.clip(&Rect::from_size(framebuffer));
    if visible.is_empty() {
        return false;
    }
    let (x, y, w, h) = gl_rect(rect, framebuffer.h);
    ctx.apply_viewport(x, y, w, h);
    let (x, y, w, h) = gl_rect(visible, framebuffer.h);
    ctx.apply_scissor_rect(x, y, w, h);
    true
}

impl EventHandler for App {
    fn update(&mut self) {
        let t = self.now();
        self.session.frame(t);
    }

    fn draw(&mut self) {
        self.frames += 1;
        // A window someone is going to play in comes to the front; the scripted
        // runs (a screenshot, a count of frames) leave the desktop alone.
        if self.frames == 1 && self.options.screenshot.is_none() && self.options.frames.is_none() {
            platform::bring_to_front();
        }

        // With `--screenshot` the run ends with the picture: of the frame
        // `--frames` names, or of the second (when the layout has settled).
        let last = match (&self.options.screenshot, self.options.frames) {
            (Some(_), frames) => Some(frames.unwrap_or(SCREENSHOT_FRAME).max(SCREENSHOT_FRAME)),
            (None, frames) => frames,
        };
        let done = last.is_some_and(|last| self.frames >= last);
        if done && self.options.screenshot.is_some() {
            if let Err(e) = self.screenshot() {
                // A scripted run that did not make its picture must not look like
                // one that did.
                eprintln!("hexgame: --screenshot: {e}");
                std::process::exit(1);
            }
        } else {
            self.render(None);
        }
        self.renderer.context_mut().commit_frame();
        if done {
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
        // A held F11 must not flip fullscreen on every repeat of the key.
        if repeat && keycode == KeyCode::F11 {
            return;
        }
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
