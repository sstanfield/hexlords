//! The window: a miniquad `EventHandler` that feeds input to a scene (the game,
//! or a tool) and draws the frame.
//!
//! C++: src/main.cpp (the GLFW window, callbacks and main loop)

use std::path::{Path, PathBuf};
use std::time::Instant;

use miniquad::conf::Conf;
use miniquad::{
    Backend, EventHandler, KeyCode, KeyMods, MouseButton, PassAction, RenderPass, RenderingBackend, TouchPhase, window,
};
use turbo_ferret::{Modifiers, Rect, Size};
use turbo_ferret_miniquad::{InputAdapter, MiniquadClipboard, MiniquadRenderer, window_dpi};

use crate::paths::asset_root;
use crate::platform;
use crate::render::textures::texture_params;
use crate::render::{AssetDirs, MapRenderer, Marks, MiniMapRenderer, Pipelines, gl_rect, png_out};
use crate::screen::Scene;

/// The game window's size at start, in logical pixels. C++: main.cpp
pub const WINDOW_SIZE: (i32, i32) = (1024, 768);
pub const WINDOW_TITLE: &str = "HEX Game!";
/// The color behind everything. C++: main.cpp glClearColor
pub const CLEAR_COLOR: (f32, f32, f32, f32) = (0.5, 0.5, 0.5, 1.0);

/// The part of the command line every window of this project understands.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Options {
    /// `--assets <dir>`: where the assets are (see `paths::asset_root`).
    pub assets: Option<PathBuf>,
    /// `--frames <n>`: quit after drawing this many frames (at least 1).
    pub frames: Option<u64>,
    /// `--screenshot <file>`: draw a frame into this PNG file and quit. It is the
    /// last frame of `--frames`, or the second without it.
    pub screenshot: Option<PathBuf>,
}

/// The usage text of the options [`Options::take`] reads.
pub const OPTIONS_USAGE: &str = "[--assets <dir>] [--frames <n>] [--screenshot <file.png>]";

impl Options {
    /// Read `arg` if it is one of these options, taking its value from `rest`.
    /// `Ok(false)`: it is something else, and nothing was taken.
    pub fn take(&mut self, arg: &str, rest: &mut dyn Iterator<Item = String>) -> Result<bool, String> {
        let mut value = |what: &str| rest.next().ok_or(format!("{arg} needs {what}"));
        match arg {
            "--assets" => self.assets = Some(PathBuf::from(value("a directory")?)),
            "--frames" => {
                let n = value("a number")?;
                let frames: u64 = n.parse().map_err(|_| format!("--frames: \"{n}\" is not a number"))?;
                if frames == 0 {
                    return Err("--frames: at least 1".to_string());
                }
                self.frames = Some(frames);
            }
            "--screenshot" => self.screenshot = Some(PathBuf::from(value("a file")?)),
            _ => return Ok(false),
        }
        Ok(true)
    }
}

/// What is different from one window of this project to the next.
#[derive(Copy, Clone, Debug)]
pub struct WindowSpec {
    /// The program's name, in front of its error messages.
    pub program: &'static str,
    pub title: &'static str,
    /// The window's size at start, in logical pixels.
    pub size: (i32, i32),
}

/// Open the window and run the scene `build` makes until the player quits.
/// `build` gets the asset directory, the window's size in pixels and its DPI.
/// An error from it ends the program with status 1.
pub fn run<S, B>(spec: WindowSpec, options: Options, build: B)
where
    S: Scene + 'static,
    B: FnOnce(&Path, Size, i32) -> Result<S, String> + 'static,
{
    let conf = Conf {
        window_title: spec.title.to_string(),
        window_width: spec.size.0,
        window_height: spec.size.1,
        high_dpi: true,
        ..Default::default()
    };
    miniquad::start(conf, move || match App::new(spec.program, options, build) {
        Ok(app) => Box::new(app),
        Err(e) => {
            eprintln!("{}: {e}", spec.program);
            std::process::exit(1);
        }
    });
}

/// What miniquad's wheel numbers mean on this system, and what the window
/// makes of them: the map moves with a trackpad's two fingers or the wheel, and
/// zooms when Ctrl or Cmd is held.
///
/// miniquad passes the system's own units. macOS: points for a trackpad (the
/// fingers' way on the screen, with the system's momentum after them), 10 for
/// a line of a wheel. Windows: 120 for a notch. X11: 1 for a notch. A positive
/// y is a turn away from the player, which moves the content down; a positive
/// x moves it right, except on Windows, where it is a tilt to the right.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Wheel {
    /// How many points (logical pixels) the map moves for one unit.
    points_per_unit: f32,
    /// 1 if a positive x moves the content right, -1 if left.
    x_sign: f32,
    /// Shift turns the wheel sideways. (macOS does that itself.)
    shift_turns: bool,
    /// How many points make one zoom level.
    zoom_points: f32,
    /// The points turned toward the next zoom level; positive is closer.
    zoom: f32,
}

/// How far a notch of a wheel moves the map, in points.
const NOTCH_POINTS: f32 = 60.0;

impl Wheel {
    pub fn new(points_per_unit: f32, x_sign: f32, shift_turns: bool, zoom_points: f32) -> Wheel {
        Wheel { points_per_unit, x_sign, shift_turns, zoom_points, zoom: 0.0 }
    }

    pub fn for_platform() -> Wheel {
        if cfg!(target_os = "macos") {
            // The map follows the fingers exactly; a zoom level is a short stroke.
            Wheel::new(1.0, 1.0, false, 40.0)
        } else if cfg!(target_os = "windows") {
            Wheel::new(NOTCH_POINTS / 120.0, -1.0, true, NOTCH_POINTS)
        } else {
            Wheel::new(NOTCH_POINTS, 1.0, true, NOTCH_POINTS)
        }
    }

    /// How far the content moves for a wheel event, in pixels: x to the right,
    /// y downward. `scale` is pixels per point.
    pub fn pan(&self, x: f32, y: f32, shift: bool, scale: f32) -> (f32, f32) {
        let finite = |v: f32| if v.is_finite() { v } else { 0.0 };
        let k = self.points_per_unit * scale;
        let (dx, dy) = (finite(x) * self.x_sign * k, finite(y) * k);
        if self.shift_turns && shift && dx == 0.0 { (dy, 0.0) } else { (dx, dy) }
    }

    /// How many zoom levels a wheel event makes: positive is closer (a turn
    /// away from the player), and mostly zero, until enough has been turned.
    pub fn zoom(&mut self, y: f32) -> i32 {
        let points = if y.is_finite() { y * self.points_per_unit } else { 0.0 };
        // A turn the other way starts from nothing.
        if points * self.zoom < 0.0 {
            self.zoom = 0.0;
        }
        self.zoom += points;
        let levels = (self.zoom / self.zoom_points).trunc();
        self.zoom -= levels * self.zoom_points;
        levels as i32
    }
}

/// The earliest frame a screenshot is taken of: the second, when the layout has
/// settled.
const SCREENSHOT_FRAME: u64 = 2;

pub struct App<S: Scene> {
    /// The game's asset directory.
    assets: PathBuf,
    scene: S,
    /// Turbo Ferret's renderer. It owns the miniquad context, which the game's
    /// own renderers borrow from it.
    renderer: MiniquadRenderer,
    pipelines: Pipelines,
    map_renderer: MapRenderer,
    minimap_renderer: MiniMapRenderer,
    input: InputAdapter,
    wheel: Wheel,
    /// The button that holds the map (the right or the middle one drags it).
    holding: Option<MouseButton>,
    start: Instant,
    frames: u64,
    fullscreen: bool,
    program: &'static str,
    options: Options,
}

impl<S: Scene> App<S> {
    /// Call inside `miniquad::start`'s closure (it needs the window).
    pub fn new(
        program: &'static str,
        options: Options,
        build: impl FnOnce(&Path, Size, i32) -> Result<S, String>,
    ) -> Result<App<S>, String> {
        let mut renderer =
            MiniquadRenderer::new(window::new_rendering_backend()).map_err(|e| format!("shader: {e}"))?;
        if renderer.context().info().backend == Backend::Metal {
            return Err("the Metal backend is not supported: start miniquad with OpenGL (its default)".to_string());
        }
        let (w, h) = window::screen_size();
        let assets = asset_root(options.assets.as_deref());
        let mut scene = build(&assets, Size::new(w as i32, h as i32), window_dpi())?;
        // Copy and paste with the rest of the system.
        let ui = &mut scene.screen_mut().ui;
        ui.services_mut().clipboard = Box::new(MiniquadClipboard::new());

        // The game's own pipelines and textures live on the renderer's context.
        let ctx = renderer.context_mut();
        let pipelines = Pipelines::new(ctx).map_err(|e| format!("shader: {e}"))?;
        let dirs = AssetDirs::new(&assets, scene.screen().game.assets.as_deref());
        let map_renderer = MapRenderer::new(ctx, dirs)?;
        Ok(App {
            assets,
            scene,
            renderer,
            pipelines,
            map_renderer,
            minimap_renderer: MiniMapRenderer::new(),
            input: InputAdapter::new(),
            wheel: Wheel::for_platform(),
            holding: None,
            start: Instant::now(),
            frames: 0,
            fullscreen: false,
            program,
            options,
        })
    }

    fn now(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }

    /// The UI's clock is data: keep it current before input (click counting reads it).
    fn tick(&mut self) {
        let t = self.now();
        self.scene.screen_mut().ui.set_time(t);
    }

    /// After input: apply what the widgets recorded.
    fn after_input(&mut self) {
        self.scene.pump();
        if self.scene.screen().quit_requested() {
            if self.scene.confirm_quit() {
                window::order_quit();
            } else {
                self.scene.screen_mut().cancel_quit();
            }
        }
        if self.scene.screen_mut().take_fullscreen_toggles() % 2 == 1 {
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
        let framebuffer = self.scene.screen().ui.viewport();
        let dirty = self.scene.screen_mut().take_minimap_dirty();
        let scene = &self.scene;
        let screen = scene.screen();
        let ctx = self.renderer.context_mut();

        // Another map may have been opened, with pictures of its own.
        let dirs = AssetDirs::new(&self.assets, screen.game.assets.as_deref());
        if dirs != *self.map_renderer.dirs() {
            if let Err(e) = self.map_renderer.set_dirs(ctx, dirs) {
                eprintln!("{}: the map's assets: {e}", self.program);
            }
        }

        // The minimap's picture is drawn into its own target, before the frame's
        // pass begins.
        self.minimap_renderer.update(ctx, &self.pipelines, &screen.game.map, &screen.mini_view, dirty);

        let (r, g, b, a) = CLEAR_COLOR;
        ctx.begin_pass(target, PassAction::clear_color(r, g, b, a));
        if set_viewport(ctx, screen.map_rect(), framebuffer) {
            let marks = Marks { color: scene.mark_color(), marked: &|hex| scene.marked(hex) };
            self.map_renderer.draw(ctx, &self.pipelines, &screen.game.map, &screen.view, &marks);
        }
        if set_viewport(ctx, screen.minimap_rect(), framebuffer) {
            self.minimap_renderer.draw(ctx, &self.pipelines, screen.mini_view.marker(&screen.view));
        }
        // Back to the whole framebuffer for the UI, which draws on top.
        set_viewport(ctx, Rect::from_size(framebuffer), framebuffer);
        self.scene.screen_mut().ui.paint(&mut self.renderer);
        self.renderer.context_mut().end_render_pass();
    }

    /// Draw a frame into a texture of the window's size and write it as a PNG.
    fn screenshot(&mut self) -> Result<(), String> {
        let Some(path) = self.options.screenshot.clone() else { return Ok(()) };
        let Size { w, h } = self.scene.screen().ui.viewport();
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

impl<S: Scene> EventHandler for App<S> {
    fn update(&mut self) {
        let t = self.now();
        self.scene.frame(t);
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
                eprintln!("{}: --screenshot: {e}", self.program);
                std::process::exit(1);
            }
        } else {
            self.render(None);
        }
        self.renderer.context_mut().commit_frame();
        if done {
            let vp = self.scene.screen().ui.viewport();
            println!("{} frames drawn at {}x{}, {} dpi; quitting", self.frames, vp.w, vp.h, window_dpi());
            window::order_quit();
        }
    }

    fn resize_event(&mut self, width: f32, height: f32) {
        self.scene.resize(Size::new(width.round().max(0.0) as i32, height.round().max(0.0) as i32));
    }

    fn mouse_motion_event(&mut self, x: f32, y: f32) {
        self.tick();
        self.input.mouse_motion_event(&mut self.scene.screen_mut().ui, x, y);
        // A map that is held goes with the pointer.
        self.scene.screen_mut().drag(self.input.pointer());
        self.after_input();
    }

    /// Over a map the wheel (or two fingers on a trackpad) moves it, and zooms
    /// with Ctrl or Cmd held; anywhere else it is the UI's.
    fn mouse_wheel_event(&mut self, x: f32, y: f32) {
        self.tick();
        let (at, mods) = (self.input.pointer(), self.input.modifiers());
        let screen = self.scene.screen_mut();
        let used = if !mods.intersects(Modifiers::CTRL | Modifiers::SUPER) {
            let (dx, dy) = self.wheel.pan(x, y, mods.contains(Modifiers::SHIFT), window::dpi_scale());
            screen.scroll(at, dx, dy)
        } else if screen.zoom_target(at) {
            // Most turns are only a part of a level.
            let levels = self.wheel.zoom(y);
            for _ in 0..levels.abs() {
                screen.zoom_at(at, levels > 0);
            }
            true
        } else {
            false
        };
        if !used {
            self.input.mouse_wheel_event(&mut screen.ui, x, y);
        }
        self.after_input();
    }

    fn mouse_button_down_event(&mut self, button: MouseButton, x: f32, y: f32) {
        self.tick();
        self.input.mouse_button_down_event(&mut self.scene.screen_mut().ui, button, x, y);
        // The right and the middle button take hold of the map, to drag it.
        if button != MouseButton::Left && self.holding.is_none() && self.scene.screen_mut().grab(self.input.pointer()) {
            self.holding = Some(button);
        }
        self.after_input();
    }

    fn mouse_button_up_event(&mut self, button: MouseButton, x: f32, y: f32) {
        self.tick();
        if self.holding == Some(button) {
            // The end of a drag, not a click of that button.
            self.holding = None;
            self.scene.screen_mut().let_go();
        } else {
            self.input.mouse_button_up_event(&mut self.scene.screen_mut().ui, button, x, y);
        }
        self.after_input();
    }

    fn char_event(&mut self, character: char, keymods: KeyMods, repeat: bool) {
        self.tick();
        self.input.char_event(&mut self.scene.screen_mut().ui, character, keymods, repeat);
        self.after_input();
    }

    fn key_down_event(&mut self, keycode: KeyCode, keymods: KeyMods, repeat: bool) {
        // A held F11 must not flip fullscreen on every repeat of the key.
        if repeat && keycode == KeyCode::F11 {
            return;
        }
        self.tick();
        self.input.key_down_event(&mut self.scene.screen_mut().ui, keycode, keymods, repeat);
        self.after_input();
    }

    fn key_up_event(&mut self, keycode: KeyCode, keymods: KeyMods) {
        self.tick();
        self.input.key_up_event(&mut self.scene.screen_mut().ui, keycode, keymods);
        self.after_input();
    }

    fn touch_event(&mut self, phase: TouchPhase, id: u64, x: f32, y: f32) {
        self.tick();
        self.input.touch_event(&mut self.scene.screen_mut().ui, phase, id, x, y);
        self.after_input();
    }

    fn window_minimized_event(&mut self) {
        self.input.focus_lost(&mut self.scene.screen_mut().ui);
        self.holding = None;
        self.scene.screen_mut().let_go();
    }

    /// The window's close button.
    fn quit_requested_event(&mut self) {
        if !self.scene.confirm_quit() {
            window::cancel_quit();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_trackpad_moves_the_map_as_far_as_the_fingers() {
        // macOS: a unit is a point, and at two pixels a point the map goes twice
        // as many pixels. Up (positive y) moves the content down.
        let mac = Wheel::new(1.0, 1.0, false, 40.0);
        assert_eq!(mac.pan(3.0, -7.5, false, 2.0), (6.0, -15.0));
        // Shift is the system's business there.
        assert_eq!(mac.pan(0.0, 4.0, true, 1.0), (0.0, 4.0));
        assert_eq!(mac.pan(f32::NAN, f32::INFINITY, false, 1.0), (0.0, 0.0));
    }

    #[test]
    fn a_notch_of_a_wheel_is_sixty_points_everywhere_else() {
        let x11 = Wheel::new(NOTCH_POINTS, 1.0, true, NOTCH_POINTS);
        assert_eq!(x11.pan(0.0, -1.0, false, 1.0), (0.0, -60.0));
        // Shift turns the wheel sideways: down is to the right (the content goes left).
        assert_eq!(x11.pan(0.0, -1.0, true, 1.0), (-60.0, 0.0));
        assert_eq!(x11.pan(1.0, 0.0, true, 1.0), (60.0, 0.0));
        // Windows: 120 a notch, and a positive x is a tilt to the right.
        let windows = Wheel::new(NOTCH_POINTS / 120.0, -1.0, true, NOTCH_POINTS);
        assert_eq!(windows.pan(120.0, -120.0, false, 1.5), (-90.0, -90.0));
    }

    #[test]
    fn a_zoom_level_takes_a_stroke_and_a_turn_back_starts_over() {
        let mut mac = Wheel::new(1.0, 1.0, false, 40.0);
        assert_eq!([mac.zoom(15.0), mac.zoom(15.0), mac.zoom(15.0)], [0, 0, 1]);
        assert_eq!(mac.zoom(100.0), 2, "5 + 100 points: two levels and 25 over");
        // The 25 points toward a third level are forgotten when the turn reverses.
        assert_eq!(mac.zoom(-30.0), 0);
        assert_eq!(mac.zoom(-10.0), -1);
        assert_eq!(mac.zoom(f32::NAN), 0);
        // One level for every notch of a wheel.
        let mut x11 = Wheel::new(NOTCH_POINTS, 1.0, true, NOTCH_POINTS);
        assert_eq!([x11.zoom(1.0), x11.zoom(1.0), x11.zoom(-1.0)], [1, 1, -1]);
    }

    #[test]
    fn this_systems_wheel() {
        let wheel = Wheel::for_platform();
        // A turn away from the player moves the content down and zooms in.
        assert!(wheel.pan(0.0, 1.0, false, 1.0).1 > 0.0);
        let mut wheel = wheel;
        assert!(wheel.zoom(1000.0) > 0);
    }
}
