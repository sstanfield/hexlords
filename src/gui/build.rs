//! Windows built from code rather than from a layout file: for what has as
//! many rows as the game has (the keys, the cities, what a city can build),
//! and text that could not go into a resource as it is. A window here takes
//! nothing from the map: it has no keyboard focus to take (its buttons are
//! not focusable), and a click beside it goes to what is under it. Its
//! buttons' clicks reach the root window, as every button's do.
//!
//! Not a port: the C++ had no windows.

use turbo_ferret::layout::stack::{LayoutDistributionPosition, LayoutPosition};
use turbo_ferret::widgets::{Button, ImageWidget, Layout, ResizeFit, TextAlign, TextField, Window, WindowSettings};

/// A button's text from the left, as in a list, instead of in its middle.
pub fn text_from_left(ui: &mut Ui, button: WidgetId) {
    let Some(layout) = ui.get::<Button>(button).and_then(|b| b.layout()) else { return };
    ui.with_widget::<Layout, _>(layout, |l, cx| {
        l.configure(cx, |l| l.distribution_position = LayoutDistributionPosition::LeftTop)
    });
    for child in ui.children(layout).to_vec() {
        ui.with_widget::<TextField, _>(child, |field, cx| {
            field.align = TextAlign::Left;
            cx.invalidate();
        });
    }
}
use turbo_ferret::{Axis, Gravity, ImageHandle, LayoutParams, Rect, TbId, Ui, WidgetId};

/// The skin of a heading: a color of its own (`assets/skin/hexlords.tb.txt`).
pub const HEADING_SKIN: &str = "Heading";
/// The skin of a cover: opaque.
pub const COVER_SKIN: &str = "Cover";

/// A window and the column its content goes in.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct WindowParts {
    pub window: WidgetId,
    pub content: WidgetId,
}

/// A window with a title bar and a close button, without the fade in and
/// out, on `parent` (the root window), with the id `id`. Fill its `content`,
/// then [`place`] it.
pub fn window(ui: &mut Ui, parent: WidgetId, id: &str, title: &str) -> WindowParts {
    let mut window = Window::new(title).with_settings(WindowSettings::TITLEBAR | WindowSettings::CLOSE_BUTTON);
    window.animate = false;
    let window = ui.add(parent, window);
    ui.set_id(window, TbId::new(id));
    let content = column(ui, window, 0);
    ui.set_gravity(content, Gravity::ALL);
    WindowParts { window, content }
}

/// A layout of widgets one under the other, from the top left, `spacing`
/// pixels apart.
pub fn column(ui: &mut Ui, parent: WidgetId, spacing: i32) -> WidgetId {
    stack(ui, parent, Axis::Y, spacing)
}

/// A layout of widgets side by side, from the left, `spacing` pixels apart,
/// their middles in a line (a word beside a button).
pub fn row(ui: &mut Ui, parent: WidgetId, spacing: i32) -> WidgetId {
    stack(ui, parent, Axis::X, spacing)
}

fn stack(ui: &mut Ui, parent: WidgetId, axis: Axis, spacing: i32) -> WidgetId {
    let mut layout = Layout::new(axis);
    layout.position = if axis == Axis::X { LayoutPosition::Center } else { LayoutPosition::LeftTop };
    layout.distribution_position = LayoutDistributionPosition::LeftTop;
    layout.spacing = Some(spacing);
    ui.add(parent, layout)
}

/// A layout of widgets side by side, from the left, `spacing` pixels apart,
/// their tops in a line (columns side by side).
pub fn top_row(ui: &mut Ui, parent: WidgetId, spacing: i32) -> WidgetId {
    let mut layout = Layout::new(Axis::X);
    layout.position = LayoutPosition::LeftTop;
    layout.distribution_position = LayoutDistributionPosition::LeftTop;
    layout.spacing = Some(spacing);
    ui.add(parent, layout)
}

/// A line of text, from the left.
pub fn text(ui: &mut Ui, parent: WidgetId, text: &str) -> WidgetId {
    let mut field = TextField::new(text);
    field.align = TextAlign::Left;
    ui.add(parent, field)
}

/// A line of text in the headings' color.
pub fn heading(ui: &mut Ui, parent: WidgetId, title: &str) -> WidgetId {
    let field = text(ui, parent, title);
    ui.set_skin(field, TbId::new(HEADING_SKIN));
    field
}

/// A button with the id `id`, which never takes the keyboard.
pub fn button(ui: &mut Ui, parent: WidgetId, id: &str, label: &str) -> WidgetId {
    let button = ui.add(parent, Button::new(label));
    ui.set_id(button, TbId::new(id));
    ui.set_focusable(button, false);
    button
}

/// A button in the skin's `Primary` look (bronze): the one a window means
/// its player to press.
pub fn primary_button(ui: &mut Ui, parent: WidgetId, id: &str, label: &str) -> WidgetId {
    let button = self::button(ui, parent, id, label);
    ui.set_skin(button, TbId::new(PRIMARY_SKIN));
    button
}

/// The skin element of a primary button (`assets/skin/hexlords.tb.txt`).
pub const PRIMARY_SKIN: &str = "Primary";

/// A line of text with a picture before it (a unit's icon), `size` pixels
/// square. Returns the text's widget.
pub fn icon_line(ui: &mut Ui, parent: WidgetId, icon: Option<ImageHandle>, size: i32, text: &str) -> WidgetId {
    let line = self::row(ui, parent, size / 4);
    if let Some(icon) = icon {
        let picture = ui.add(line, ImageWidget::new());
        ui.set_layout_params(picture, Some(LayoutParams::fixed(size, size)));
        ui.with_widget::<ImageWidget, _>(picture, |w, cx| w.set_image(cx, icon));
    }
    self::text(ui, line, text)
}

/// Empty room, `px` pixels high.
pub fn gap(ui: &mut Ui, parent: WidgetId, px: i32) -> WidgetId {
    let gap = ui.add(parent, Layout::new(Axis::X));
    ui.set_layout_params(gap, Some(LayoutParams::fixed(1, px.max(0))));
    gap
}

/// Make a widget exactly `w` pixels wide (as wide as its widest neighbour,
/// say), whatever it holds.
pub fn set_width(ui: &mut Ui, id: WidgetId, w: i32) {
    let mut lp = LayoutParams::default();
    lp.set_width(w.max(0));
    ui.set_layout_params(id, Some(lp));
}

/// Size a window to what it holds and put it in `area` (a rectangle of its
/// parent, the root window: the map's, say), `align` of the way across and
/// down the room it leaves there: (0, 0) in its top left corner, (0.5, 0.5)
/// in the middle, (1, 0) top right. All of it is inside the parent if it fits.
pub fn place(ui: &mut Ui, parts: WindowParts, area: Rect, align: (f32, f32)) {
    let Some(parent) = ui.parent(parts.window) else { return };
    let bounds = Rect::from_size(ui.rect(parent).size());
    let r = ui.fit_content_rect(parts.window, ResizeFit::Preferred);
    let x = area.x + ((area.w - r.w) as f32 * align.0) as i32;
    let y = area.y + ((area.h - r.h) as f32 * align.1) as i32;
    ui.set_rect(parts.window, Rect::new(x, y, r.w, r.h).move_in(&bounds).clip(&bounds));
    // The content goes under the title, inside the window's padding.
    let inside = ui.padding_rect(parts.window);
    ui.set_rect(parts.content, inside);
}

/// Size a window to what it holds again, where it is (its content grew or
/// shrank).
pub fn refit(ui: &mut Ui, parts: WindowParts) {
    let Some(parent) = ui.parent(parts.window) else { return };
    let bounds = Rect::from_size(ui.rect(parent).size());
    let old = ui.rect(parts.window);
    let r = ui.fit_content_rect(parts.window, ResizeFit::Preferred);
    ui.set_rect(parts.window, Rect::new(old.x, old.y, r.w, r.h).move_in(&bounds).clip(&bounds));
    let inside = ui.padding_rect(parts.window);
    ui.set_rect(parts.content, inside);
}

/// A panel on `parent` (the root window), with the id `id`: the windows'
/// look with no title bar, its content a column inside its padding. Put it
/// over a rectangle with [`place_panel`] (the production panel covers the
/// map's: the owner's word, 2026-11-24).
pub fn panel(ui: &mut Ui, parent: WidgetId, id: &str) -> WindowParts {
    let mut layout = Layout::new(Axis::Y);
    layout.position = LayoutPosition::LeftTop;
    layout.distribution_position = LayoutDistributionPosition::LeftTop;
    let panel = ui.add(parent, layout);
    ui.set_id(panel, TbId::new(id));
    ui.set_skin(panel, TbId::new("TBWindow"));
    let content = column(ui, panel, 0);
    WindowParts { window: panel, content }
}

/// Put a panel over `area` (a rectangle of its parent), its content inside
/// its padding.
pub fn place_panel(ui: &mut Ui, parts: WindowParts, area: Rect) {
    ui.set_rect(parts.window, area);
    let inside = ui.padding_rect(parts.window);
    ui.set_rect(parts.content, inside);
}

/// Put a panel over `area` and lay out what is in it again, now (its
/// parts' sizes changed). The root window places its children by hand, so
/// the layout pass does not reach them: a change of size arranges a widget
/// at once, and so it is given one.
pub fn arrange_panel(ui: &mut Ui, parts: WindowParts, area: Rect) {
    ui.set_rect(parts.window, Rect::new(area.x, area.y, area.w, area.h + 1));
    place_panel(ui, parts, area);
}

/// A screen over the whole of `parent` (the root window: the map and the
/// panels), opaque, with the id `id` and its content in its middle: what
/// hotseat play shows between two people's turns. Fill its `content`.
pub fn cover(ui: &mut Ui, parent: WidgetId, id: &str) -> WindowParts {
    let mut layout = Layout::new(Axis::Y);
    layout.position = LayoutPosition::Center;
    layout.distribution_position = LayoutDistributionPosition::Center;
    let cover = ui.add(parent, layout);
    ui.set_id(cover, TbId::new(id));
    ui.set_skin(cover, TbId::new(COVER_SKIN));
    let content = column(ui, cover, 0);
    let mut parts = WindowParts { window: cover, content };
    stretch_cover(ui, &mut parts);
    parts
}

/// Make a cover as big as its parent again (the window changed size).
pub fn stretch_cover(ui: &mut Ui, parts: &mut WindowParts) {
    if let Some(parent) = ui.parent(parts.window) {
        let all = Rect::from_size(ui.rect(parent).size());
        ui.set_rect(parts.window, all);
    }
}

/// True while a window is there (its close button removes it).
pub fn is_open(ui: &Ui, window: WidgetId) -> bool {
    ui.node(window).is_some()
}

/// Close a window (or a cover), if it is there.
pub fn close(ui: &mut Ui, window: WidgetId) {
    if !is_open(ui, window) {
        return;
    }
    if ui.get::<Window>(window).is_some() {
        ui.close_window(window);
    } else {
        ui.remove(window);
    }
}
