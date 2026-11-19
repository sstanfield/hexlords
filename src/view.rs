//! The main map's view: zoom, scale factors, which hexes are visible, which hex
//! is under a point, and when the view follows the cursor. Pure math: no UI and
//! no graphics.
//!
//! # Coordinates
//!
//! A hex model is 2 units wide and 2 high: its center is at (0, 0), its points at
//! (±1, 0) and its flat top and bottom run from x = -0.5 to 0.5 at y = ±1.
//! Column `c` is 1.5 units right of column 0, row `r` is 2 units *below* row 0
//! (y grows upward, map rows grow downward), and odd columns are drawn 1 unit
//! higher. Model units become the widget's normalized device coordinates (-1..1
//! on both axes, y up) by multiplying x with the width factor and y with the
//! height factor.
//!
//! C++: src/render/maprender.cpp MapRenderer (updateMapDimensions, renderMap,
//! getMapDisplayCenter, setMapDisplayCenter, zoomInMap, zoomOutMap, setCenterMap,
//! moveUp, moveDown)

use crate::state::{Direction, Hex};

/// The map scale at each zoom level, most zoomed in first.
pub const ZOOM_LEVELS: [f32; 5] = [0.2, 0.1, 0.075, 0.05, 0.03];
/// The zoom level a view starts at.
pub const START_ZOOM: usize = 1;
/// Scales y so that hexes look regular (about √3/2). C++: HEX_HEIGHT_ADJUST
#[allow(clippy::excessive_precision)] // the C++ constant, digit for digit
pub const HEX_HEIGHT_ADJUST: f32 = 0.863_309_35;

/// The width and height factors for a widget of `w` x `h` pixels at `scale`: the
/// longer side is scaled down so that hexes keep their shape.
/// C++: MapRenderer::updateMapDimensions
pub fn factors(w: i32, h: i32, scale: f32) -> (f32, f32) {
    let (wf, hf) = if w <= 0 || h <= 0 {
        (scale, scale) // no size yet
    } else if h < w {
        (h as f32 / w as f32 * scale, scale)
    } else {
        (scale, w as f32 / h as f32 * scale)
    };
    (wf, hf * HEX_HEIGHT_ADJUST)
}

/// The (row, col) of the hex that contains a point of the endless grid, in grid
/// coordinates: model units in which hex (row, col) has its center at
/// `(1.5 * col, -2 * row)`, one unit higher in odd columns.
///
/// The column nearest to the point is right unless the point is in one of the
/// corners that a neighbouring column's hexes reach into, so the columns either
/// side are tried too.
pub fn hex_at(x: f32, y: f32) -> (i32, i32) {
    let nearest = (x / 1.5).round() as i32;
    let candidate = |col: i32| {
        let raise = if col.rem_euclid(2) == 1 { 1.0 } else { 0.0 };
        let row = ((raise - y) / 2.0).round() as i32;
        let (dx, dy) = ((x - 1.5 * col as f32).abs(), (y - (raise - 2.0 * row as f32)).abs());
        // Inside the hex: between its flat edges, and inside its slanted ones.
        (row, col, dy <= 1.0 && dx <= 1.0 - dy / 2.0)
    };
    let first = candidate(nearest);
    [first, candidate(nearest - 1), candidate(nearest + 1)]
        .into_iter()
        .find(|&(_, _, inside)| inside)
        .map_or((first.0, first.1), |(row, col, _)| (row, col))
}

/// True if a point in grid coordinates (see [`hex_at`]) is inside a `rows` x
/// `cols` map, whose edges are straight: the outer halves of the edge hexes are
/// cut off.
pub fn on_grid(x: f32, y: f32, rows: u16, cols: u16) -> bool {
    if rows == 0 || cols == 0 {
        return false;
    }
    let (right, bottom) = (1.5 * f32::from(cols - 1) + 0.5, -2.0 * f32::from(rows - 1));
    (-0.5..=right).contains(&x) && (bottom..=1.0).contains(&y)
}

/// A hex in view, with the center of its model in model units relative to the
/// middle of the widget.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct VisibleHex {
    pub hex: Hex,
    pub center: [f32; 2],
}

#[derive(Clone, Debug, PartialEq)]
pub struct MapView {
    rows: u16,
    cols: u16,
    zoom_level: usize,
    /// The hex nearest the middle of the widget.
    center: Hex,
    /// Where the middle of the widget is from the center hex's place in the
    /// grid, (1.5 * col, -2 * row), in model units (an odd column's hex is drawn
    /// 1 unit above its place). Zero after `center_on`; a pan leaves a part of a
    /// hex here, at most 0.75 across and 1 up or down.
    offset: [f32; 2],
    size: (i32, i32),
    wf: f32,
    hf: f32,
}

impl MapView {
    /// A view of a `rows` x `cols` map centered on `center`, with no size yet.
    pub fn new(rows: u16, cols: u16, center: Hex) -> MapView {
        let (wf, hf) = factors(0, 0, ZOOM_LEVELS[START_ZOOM]);
        MapView { rows, cols, zoom_level: START_ZOOM, center, offset: [0.0, 0.0], size: (0, 0), wf, hf }
    }

    /// The widget's size in pixels changed. C++: MapRenderer::resizeMap
    pub fn resize(&mut self, w: i32, h: i32) {
        self.size = (w, h);
        self.update_factors();
    }

    fn update_factors(&mut self) {
        (self.wf, self.hf) = factors(self.size.0, self.size.1, ZOOM_LEVELS[self.zoom_level]);
    }

    pub fn size(&self) -> (i32, i32) {
        self.size
    }

    pub fn zoom_level(&self) -> usize {
        self.zoom_level
    }

    pub fn center(&self) -> Hex {
        self.center
    }

    /// The (width, height) factors: model units times these are the widget's
    /// normalized device coordinates.
    pub fn factors(&self) -> (f32, f32) {
        (self.wf, self.hf)
    }

    /// Hexes get bigger. False at the closest level. C++: MapRenderer::zoomInMap
    pub fn zoom_in(&mut self) -> bool {
        if self.zoom_level == 0 {
            return false;
        }
        self.zoom_level -= 1;
        self.update_factors();
        true
    }

    /// Hexes get smaller. False at the widest level. C++: MapRenderer::zoomOutMap
    pub fn zoom_out(&mut self) -> bool {
        if self.zoom_level + 1 >= ZOOM_LEVELS.len() {
            return false;
        }
        self.zoom_level += 1;
        self.update_factors();
        true
    }

    /// Put a hex in the middle of the widget. C++: MapRenderer::setMapDisplayCenter
    pub fn center_on(&mut self, hex: Hex) {
        self.center = hex;
        self.offset = [0.0, 0.0];
    }

    /// The middle of the widget in grid coordinates (see [`hex_at`]).
    pub fn middle(&self) -> [f32; 2] {
        [1.5 * f32::from(self.center.col) + self.offset[0], self.offset[1] - 2.0 * f32::from(self.center.row)]
    }

    /// Put a point of the grid in the middle of the widget, or the point of the
    /// map nearest to it: the middle never leaves the map. (A point that is not
    /// a number is no place to go, and the view stays.)
    pub fn set_middle(&mut self, x: f32, y: f32) {
        if !x.is_finite() || !y.is_finite() {
            return;
        }
        let (last_col, last_row) = (self.cols.saturating_sub(1), self.rows.saturating_sub(1));
        let x = x.clamp(0.0, 1.5 * f32::from(last_col));
        let y = y.clamp(-2.0 * f32::from(last_row), 0.0);
        let col = ((x / 1.5).round() as u16).min(last_col);
        let row = ((-y / 2.0).round() as u16).min(last_row);
        self.center = Hex::new(row, col);
        self.offset = [x - 1.5 * f32::from(col), y + 2.0 * f32::from(row)];
    }

    /// Move the view over the map by model units: x to the right, y upward.
    /// False if it is where it was (it stops at the map's edges).
    pub fn pan(&mut self, dx: f32, dy: f32) -> bool {
        let before = (self.center, self.offset);
        let [x, y] = self.middle();
        self.set_middle(x + dx, y + dy);
        before != (self.center, self.offset)
    }

    /// Move the map under the widget by pixels, x to the right and y *downward*:
    /// what was under a pointer stays under it when the pointer moves that far.
    /// (The view goes the other way.) False if nothing moved.
    pub fn pan_pixels(&mut self, dx: f32, dy: f32) -> bool {
        let (w, h) = self.size;
        if w <= 0 || h <= 0 {
            return false;
        }
        // A pixel is 2 / w of the widget's normalized width.
        self.pan(-dx * 2.0 / w as f32 / self.wf, dy * 2.0 / h as f32 / self.hf)
    }

    /// The point of the grid under a point given in the widget's normalized
    /// device coordinates (x right, y up, both -1..1).
    pub fn grid_at(&self, rel_x: f32, rel_y: f32) -> [f32; 2] {
        let [x, y] = self.middle();
        [rel_x / self.wf + x, rel_y / self.hf + y]
    }

    /// Zoom one level in (`closer`) or out, keeping what is under a point of the
    /// widget (normalized device coordinates) under it, as far as the map's
    /// edges allow. False at the last level.
    pub fn zoom_about(&mut self, rel_x: f32, rel_y: f32, closer: bool) -> bool {
        let [x, y] = self.grid_at(rel_x, rel_y);
        if !(if closer { self.zoom_in() } else { self.zoom_out() }) {
            return false;
        }
        self.set_middle(x - rel_x / self.wf, y - rel_y / self.hf);
        true
    }

    /// True if a hex's center is inside the widget.
    pub fn shows(&self, hex: Hex) -> bool {
        let [x, y] = self.hex_center(hex);
        let (half_w, half_h) = self.half_extent();
        x.abs() <= half_w && y.abs() <= half_h
    }

    /// The center of a hex's model, in model units relative to the middle of the
    /// widget.
    pub fn hex_center(&self, hex: Hex) -> [f32; 2] {
        let col = i32::from(hex.col) - i32::from(self.center.col);
        let row = i32::from(self.center.row) - i32::from(hex.row);
        let raise = if hex.col % 2 == 1 { 1.0 } else { 0.0 };
        [1.5 * col as f32 - self.offset[0], 2.0 * row as f32 + raise - self.offset[1]]
    }

    /// Model units to the widget's normalized device coordinates.
    pub fn to_ndc(&self, model: [f32; 2]) -> [f32; 2] {
        [model[0] * self.wf, model[1] * self.hf]
    }

    /// How far the widget reaches from its middle, in model units: (half its
    /// width, half its height).
    pub fn half_extent(&self) -> (f32, f32) {
        (1.0 / self.wf, 1.0 / self.hf)
    }

    /// The hexes to draw: every hex of the map whose model touches the widget,
    /// column by column.
    ///
    /// A hex reaches 1 unit from its center each way, so the columns go out to
    /// where `1.5 * col - 1` passes the widget's half width, and the rows to where
    /// `2 * row - 1` passes its half height, with one more row for the unit an odd
    /// column is raised by, and as much more as the middle is off the center
    /// hex's place. C++: MapRenderer::renderMap, which padded the columns by a
    /// quarter plus six: too few once the widget is wider than about 2:1.
    pub fn visible_hexes(&self) -> impl Iterator<Item = VisibleHex> + '_ {
        let (half_w, half_h) = self.half_extent();
        let half_x = ((half_w + 1.0 + self.offset[0].abs()) / 1.5).ceil() as i32;
        let half_y = ((half_h + 2.0 + self.offset[1].abs()) / 2.0).ceil() as i32;
        let (center_row, center_col) = (i32::from(self.center.row), i32::from(self.center.col));
        (-half_x..=half_x).flat_map(move |col| {
            let map_col = center_col + col;
            (-half_y..=half_y).filter_map(move |row| {
                let hex = Hex::on_map(center_row - row, map_col, self.rows, self.cols)?;
                Some(VisibleHex { hex, center: self.hex_center(hex) })
            })
        })
    }

    /// The hex under a point given in the widget's normalized device coordinates
    /// (x right, y up, both -1..1), if the point is on the map.
    pub fn pick(&self, rel_x: f32, rel_y: f32) -> Option<Hex> {
        let [x, y] = self.grid_at(rel_x, rel_y);
        if !on_grid(x, y, self.rows, self.cols) {
            return None;
        }
        let (row, col) = hex_at(x, y);
        Hex::on_map(row, col, self.rows, self.cols)
    }

    /// The hex under a pixel of a `w` x `h` widget (x right, y down, from its top
    /// left corner). C++: MapWidget::OnEvent (the pointer cases)
    pub fn pick_pixel(&self, x: i32, y: i32, w: i32, h: i32) -> Option<Hex> {
        // Outside the widget is not on the map, even where the map goes on: the
        // pointer can be there while the widget holds it (a press dragged out).
        if !(0..w).contains(&x) || !(0..h).contains(&y) {
            return None;
        }
        let (half_w, half_h) = (w as f32 / 2.0, h as f32 / 2.0);
        // The middle of the pixel.
        self.pick((x as f32 + 0.5 - half_w) / half_w, -(y as f32 + 0.5 - half_h) / half_h)
    }

    /// The number of rows the view counts as shown. C++: getMapDisplayCenter
    pub fn rows_displayed(&self) -> i32 {
        (1.0 / self.hf) as i32
    }

    /// The number of columns the view counts as shown. C++: getMapDisplayCenter
    pub fn cols_displayed(&self) -> i32 {
        (1.0 / self.wf * 1.25) as i32
    }

    /// The cursor took a step in direction `dir`: if that took it out of the part
    /// of the map the view counts as shown, center the view on the cursor along
    /// that axis. Returns true if the view moved.
    ///
    /// C++: MapWidget::OnEvent (left, right), MapRenderer::moveUp, moveDown. Those
    /// compared unsigned numbers, so near the left and top edges the bound wrapped
    /// around and every key press recentered (deviation D6).
    pub fn keep_cursor_visible(&mut self, cursor: Hex, dir: Direction) -> bool {
        let (row, col) = (i32::from(cursor.row), i32::from(cursor.col));
        let (center_row, center_col) = (i32::from(self.center.row), i32::from(self.center.col));
        let (half_rows, half_cols) = (self.rows_displayed() / 2, self.cols_displayed() / 2);
        let out = match dir {
            Direction::Left => col < center_col - half_cols,
            Direction::Right => col > center_col + half_cols,
            Direction::Up => row < center_row - half_rows,
            Direction::Down => row > center_row + half_rows,
        };
        if out {
            match dir {
                Direction::Left | Direction::Right => (self.center.col, self.offset[0]) = (cursor.col, 0.0),
                Direction::Up | Direction::Down => (self.center.row, self.offset[1]) = (cursor.row, 0.0),
            }
        }
        out
    }

    /// Where a hex's border starts, as the rim weight of a fragment (0 at the
    /// hex's center, 1 at its rim): the cursor's hex has a wider border, and
    /// borders widen a little as the view zooms out. C++: MapRenderer::CTX::drawHex
    pub fn border_threshold(&self, is_cursor: bool) -> f32 {
        (if is_cursor { 0.85 } else { 0.95 }) - self.zoom_level as f32 * 0.01
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The demo's main map widget at 1024 x 768.
    const W: i32 = 819;
    const H: i32 = 537;

    fn view() -> MapView {
        let mut v = MapView::new(100, 100, Hex::new(50, 50));
        v.resize(W, H);
        v
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn factors_scale_the_longer_side_down() {
        // Wider than high: the height factor is the scale (times the hex adjust).
        let (wf, hf) = factors(800, 400, 0.1);
        assert!(close(wf, 0.05) && close(hf, 0.1 * HEX_HEIGHT_ADJUST), "{wf} {hf}");
        // Higher than wide: the width factor is the scale.
        let (wf, hf) = factors(400, 800, 0.1);
        assert!(close(wf, 0.1) && close(hf, 0.05 * HEX_HEIGHT_ADJUST), "{wf} {hf}");
        // Square.
        let (wf, hf) = factors(500, 500, 0.2);
        assert!(close(wf, 0.2) && close(hf, 0.2 * HEX_HEIGHT_ADJUST));
        // No size yet: finite numbers, not a division by zero.
        let (wf, hf) = factors(0, 0, 0.1);
        assert!(close(wf, 0.1) && close(hf, 0.1 * HEX_HEIGHT_ADJUST));
        let (wf, hf) = factors(-5, 100, 0.1);
        assert!(wf.is_finite() && hf.is_finite());
    }

    #[test]
    fn hexes_are_as_wide_as_they_are_high_on_screen() {
        // A hex is 2 model units wide and 2 high; in pixels its height must be
        // HEX_HEIGHT_ADJUST times its width whatever the widget's shape.
        for (w, h) in [(W, H), (300, 900), (640, 640)] {
            let mut v = view();
            v.resize(w, h);
            let (wf, hf) = v.factors();
            let (px_w, px_h) = (2.0 * wf * w as f32 / 2.0, 2.0 * hf * h as f32 / 2.0);
            assert!((px_h / px_w - HEX_HEIGHT_ADJUST).abs() < 1e-4, "{w}x{h}: {px_w} x {px_h}");
        }
    }

    #[test]
    fn a_new_view_starts_at_zoom_level_one() {
        let v = view();
        assert_eq!(v.zoom_level(), 1);
        assert_eq!(v.center(), Hex::new(50, 50));
        assert_eq!(v.size(), (W, H));
        let (wf, hf) = v.factors();
        assert!(close(wf, H as f32 / W as f32 * 0.1) && close(hf, 0.1 * HEX_HEIGHT_ADJUST));
    }

    #[test]
    fn zoom_clamps_at_both_ends() {
        let mut v = view();
        assert!(v.zoom_in());
        assert_eq!(v.zoom_level(), 0);
        assert!(!v.zoom_in());
        assert_eq!(v.zoom_level(), 0);
        assert!(close(v.factors().1, 0.2 * HEX_HEIGHT_ADJUST));
        for (level, scale) in ZOOM_LEVELS.iter().enumerate().skip(1) {
            assert!(v.zoom_out());
            assert_eq!(v.zoom_level(), level);
            assert!(close(v.factors().1, scale * HEX_HEIGHT_ADJUST));
        }
        assert!(!v.zoom_out());
        assert_eq!(v.zoom_level(), ZOOM_LEVELS.len() - 1);
        // Zooming keeps the center and the size.
        assert_eq!((v.center(), v.size()), (Hex::new(50, 50), (W, H)));
    }

    #[test]
    fn hex_centers() {
        let v = view();
        assert_eq!(v.hex_center(Hex::new(50, 50)), [0.0, 0.0]);
        // Rows grow downward.
        assert_eq!(v.hex_center(Hex::new(49, 50)), [0.0, 2.0]);
        assert_eq!(v.hex_center(Hex::new(51, 50)), [0.0, -2.0]);
        // Odd columns are one unit higher.
        assert_eq!(v.hex_center(Hex::new(50, 51)), [1.5, 1.0]);
        assert_eq!(v.hex_center(Hex::new(50, 49)), [-1.5, 1.0]);
        assert_eq!(v.hex_center(Hex::new(50, 52)), [3.0, 0.0]);
        // Around an odd center column the center hex itself is one unit up.
        let mut v = view();
        v.center_on(Hex::new(50, 51));
        assert_eq!(v.hex_center(Hex::new(50, 51)), [0.0, 1.0]);
        assert_eq!(v.hex_center(Hex::new(50, 50)), [-1.5, 0.0]);
    }

    #[test]
    fn visible_hexes_cover_the_widget_and_stay_on_the_map() {
        // The demo's widget, a tall one, a square, and wide ones (an ultrawide
        // screen's map is 2048 x 756): the C++ padding lost columns on those.
        for (w, h) in [(W, H), (300, 900), (640, 640), (2048, 756), (2048, 600), (3000, 200), (40, 2000)] {
            for center in [Hex::new(50, 50), Hex::new(50, 51), Hex::new(30, 30), Hex::new(2, 97)] {
                let mut v = view();
                v.resize(w, h);
                v.center_on(center);
                v.zoom_in();
                for level in 0..ZOOM_LEVELS.len() {
                    let visible: Vec<VisibleHex> = v.visible_hexes().collect();
                    // No hex twice.
                    let mut hexes: Vec<Hex> = visible.iter().map(|vh| vh.hex).collect();
                    hexes.sort();
                    hexes.dedup();
                    assert_eq!(hexes.len(), visible.len(), "{w}x{h}, level {level}");
                    // Every hex whose model touches the widget is in the list, and
                    // the list is not much longer than that.
                    let (hw, hh) = v.half_extent();
                    let mut touching = 0;
                    for row in 0..100 {
                        for col in 0..100 {
                            let hex = Hex::new(row, col);
                            let [x, y] = v.hex_center(hex);
                            if x.abs() - 1.0 < hw && y.abs() - 1.0 < hh {
                                touching += 1;
                                assert!(
                                    hexes.binary_search(&hex).is_ok(),
                                    "{w}x{h}, center {center}, level {level}: {hex} is missing"
                                );
                            }
                        }
                    }
                    assert!(touching > 0 && visible.len() <= touching + touching / 2 + 80, "{w}x{h}: {touching}");
                    for vh in &visible {
                        assert_eq!(vh.center, v.hex_center(vh.hex));
                    }
                    v.zoom_out();
                }
            }
        }
    }

    #[test]
    fn visible_hexes_at_the_map_edge_skip_what_is_off_the_map() {
        let mut v = view();
        v.center_on(Hex::new(0, 0));
        let visible: Vec<Hex> = v.visible_hexes().map(|vh| vh.hex).collect();
        assert!(visible.contains(&Hex::new(0, 0)));
        assert!(!visible.is_empty() && visible.len() < view().visible_hexes().count());
        v.center_on(Hex::new(99, 99));
        assert!(v.visible_hexes().any(|vh| vh.hex == Hex::new(99, 99)));
        assert!(v.visible_hexes().all(|vh| vh.hex.row < 100 && vh.hex.col < 100));
    }

    #[test]
    fn the_start_view_draws_about_three_hundred_hexes() {
        // Half the widget is 15.25 units wide: (15.25 + 1) / 1.5 rounds up to 11
        // columns each side. It is 11.58 units high: (11.58 + 2) / 2 rounds up to
        // 7 rows each side.
        assert_eq!(view().visible_hexes().count(), 23 * 15);
    }

    #[test]
    fn picking_a_hex_center_gives_that_hex_at_every_zoom_level() {
        for center in [Hex::new(50, 50), Hex::new(50, 51), Hex::new(3, 2), Hex::new(97, 99)] {
            let mut v = view();
            v.center_on(center);
            v.zoom_in();
            for level in 0..ZOOM_LEVELS.len() {
                assert_eq!(v.zoom_level(), level);
                let (hw, hh) = v.half_extent();
                let mut picked = 0;
                for vh in v.visible_hexes() {
                    let [x, y] = vh.center;
                    if x.abs() > hw || y.abs() > hh {
                        continue; // its center is outside the widget
                    }
                    let [nx, ny] = v.to_ndc(vh.center);
                    assert_eq!(v.pick(nx, ny), Some(vh.hex), "center {center}, level {level}, hex {}", vh.hex);
                    picked += 1;
                }
                assert!(picked > 10, "center {center}, level {level}: {picked}");
                v.zoom_out();
            }
        }
    }

    /// Points inside the whole hex model, as offsets from its center: the middle,
    /// near each corner, and near the middle of each edge.
    fn points_inside_a_hex() -> Vec<[f32; 2]> {
        let corners = [[-0.5, 1.0], [0.5, 1.0], [1.0, 0.0], [0.5, -1.0], [-0.5, -1.0], [-1.0, 0.0]];
        let mut points = vec![[0.0, 0.0]];
        for i in 0..6 {
            let (a, b): ([f32; 2], [f32; 2]) = (corners[i], corners[(i + 1) % 6]);
            points.push([a[0] * 0.95, a[1] * 0.95]);
            points.push([(a[0] + b[0]) / 2.0 * 0.97, (a[1] + b[1]) / 2.0 * 0.97]);
        }
        points
    }

    #[test]
    fn every_point_of_a_hex_picks_that_hex() {
        // Inner hexes only: the ones on the map's edge are cut (see below).
        for center in [Hex::new(50, 50), Hex::new(50, 51), Hex::new(7, 6), Hex::new(93, 90)] {
            let mut v = view();
            v.center_on(center);
            v.zoom_in();
            for level in 0..ZOOM_LEVELS.len() {
                let (hw, hh) = v.half_extent();
                let mut picked = 0;
                for vh in v.visible_hexes() {
                    let inner = (1..99).contains(&vh.hex.row) && (1..99).contains(&vh.hex.col);
                    for [dx, dy] in points_inside_a_hex() {
                        let (x, y) = (vh.center[0] + dx, vh.center[1] + dy);
                        if !inner || x.abs() > hw || y.abs() > hh {
                            continue; // outside the widget
                        }
                        let [nx, ny] = v.to_ndc([x, y]);
                        assert_eq!(
                            v.pick(nx, ny),
                            Some(vh.hex),
                            "center {center}, level {level}, hex {} at offset ({dx}, {dy})",
                            vh.hex
                        );
                        picked += 1;
                    }
                }
                assert!(picked > 100, "center {center}, level {level}: {picked}");
                v.zoom_out();
            }
        }
    }

    #[test]
    fn hex_at_follows_the_slanted_edges() {
        // Hex (0, 0) is centered at the origin; (0, 1) is right of it and one unit
        // up, (1, 1) right of it and one unit down. Their shared corner with (0, 0)
        // is its right point, at (1, 0).
        assert_eq!(hex_at(0.0, 0.0), (0, 0));
        assert_eq!(hex_at(0.9, 0.0), (0, 0), "just inside the right point");
        // At x = 0.9 the hex is only 0.2 high: above that is (0, 1), below (1, 1).
        // (The rectangle around the hex, which the C++ used, says (0, 1) for both.)
        assert_eq!(hex_at(0.9, 0.3), (0, 1));
        assert_eq!(hex_at(0.9, -0.3), (1, 1));
        // Left of the origin the columns are odd again, at negative indices.
        assert_eq!(hex_at(-0.9, 0.3), (0, -1));
        assert_eq!(hex_at(-0.9, -0.3), (1, -1));
        assert_eq!(hex_at(-1.5, 1.0), (0, -1));
        // Straight up and down.
        assert_eq!(hex_at(0.0, 1.5), (-1, 0));
        assert_eq!(hex_at(0.0, -1.5), (1, 0));
        assert_eq!(hex_at(3.0, -4.0), (2, 2));
        assert_eq!(hex_at(4.5, -3.0), (2, 3));
    }

    #[test]
    fn every_point_is_in_exactly_one_hex() {
        // Walk a fine grid over a few hexes: the hex `hex_at` names contains the
        // point, and (away from the shared edges) no neighbour does.
        let inside = |x: f32, y: f32, row: i32, col: i32, slack: f32| {
            let raise = if col.rem_euclid(2) == 1 { 1.0 } else { 0.0 };
            let (dx, dy) = ((x - 1.5 * col as f32).abs(), (y - (raise - 2.0 * row as f32)).abs());
            dy <= 1.0 + slack && dx <= 1.0 - dy / 2.0 + slack
        };
        for i in -200..200 {
            for j in -200..200 {
                let (x, y) = (i as f32 * 0.037 + 0.011, j as f32 * 0.041 + 0.007);
                let (row, col) = hex_at(x, y);
                assert!(inside(x, y, row, col, 1e-4), "({x}, {y}) is not in ({row}, {col})");
                let others = (row - 2..=row + 2)
                    .flat_map(|r| (col - 2..=col + 2).map(move |c| (r, c)))
                    .filter(|&(r, c)| (r, c) != (row, col) && inside(x, y, r, c, -1e-3))
                    .count();
                assert_eq!(others, 0, "({x}, {y}) is in ({row}, {col}) and {others} more");
            }
        }
    }

    #[test]
    fn the_cut_off_parts_of_edge_hexes_pick_nothing() {
        let mut v = view();
        v.center_on(Hex::new(0, 0));
        let pick = |v: &MapView, x: f32, y: f32| {
            let [nx, ny] = v.to_ndc([x, y]);
            v.pick(nx, ny)
        };
        // Column 0 loses its left point: left of x = -0.5 is off the map.
        assert_eq!(pick(&v, -0.4, 0.0), Some(Hex::new(0, 0)));
        assert_eq!(pick(&v, -0.6, 0.0), None);
        // Row 0's odd columns lose their top half: above y = 1 is off the map.
        assert_eq!(pick(&v, 1.5, 0.9), Some(Hex::new(0, 1)));
        assert_eq!(pick(&v, 1.5, 1.1), None);
        assert_eq!(pick(&v, 0.0, 0.9), Some(Hex::new(0, 0)));
        assert_eq!(pick(&v, 0.0, 1.1), None);

        v.center_on(Hex::new(99, 99));
        // The view's middle is one unit below hex (99, 99)'s center (an odd column).
        // The last column loses its right point, and the map ends at the middle of
        // the last row's even columns, which is the bottom of its odd ones.
        assert_eq!(pick(&v, 0.4, 0.5), Some(Hex::new(99, 99)));
        assert_eq!(pick(&v, 0.6, 0.5), None);
        assert_eq!(pick(&v, -1.5, 0.1), Some(Hex::new(99, 98)));
        assert_eq!(pick(&v, -1.5, -0.1), None);
        assert_eq!(pick(&v, 0.0, -0.1), None);
    }

    #[test]
    fn pick_is_none_off_the_map() {
        let mut v = view();
        v.center_on(Hex::new(0, 0));
        assert_eq!(v.pick(0.0, 0.0), Some(Hex::new(0, 0)));
        assert_eq!(v.pick(-0.5, 0.0), None, "left of column 0");
        assert_eq!(v.pick(0.0, 0.5), None, "above row 0");
        v.center_on(Hex::new(99, 99));
        assert_eq!(v.pick(0.5, 0.0), None, "right of the last column");
        assert_eq!(v.pick(0.0, -0.5), None, "below the last row");
    }

    #[test]
    fn pick_pixel_maps_the_widget_to_the_view() {
        let v = view();
        // The middle of the widget is the center hex.
        assert_eq!(v.pick_pixel(W / 2, H / 2, W, H), Some(Hex::new(50, 50)));
        // One hex is 2 * hf * H / 2 = 46.4 pixels high: 50 pixels up is the row above.
        assert_eq!(v.pick_pixel(W / 2, H / 2 - 50, W, H), Some(Hex::new(49, 50)));
        assert_eq!(v.pick_pixel(W / 2, H / 2 + 50, W, H), Some(Hex::new(51, 50)));
        // Columns are 1.5 * wf * W / 2 = 40.3 pixels apart.
        assert_eq!(v.pick_pixel(W / 2 + 80, H / 2, W, H), Some(Hex::new(50, 52)));
        assert_eq!(v.pick_pixel(W / 2 - 80, H / 2, W, H), Some(Hex::new(50, 48)));
        // A widget without a size picks nothing, and neither does a pixel outside
        // the widget (the map goes on there, but it is not what is shown).
        assert_eq!(v.pick_pixel(0, 0, 0, 0), None);
        assert_eq!(v.pick_pixel(0, 0, W, H), Some(Hex::new(44, 40)));
        for (x, y) in [(-1, 10), (10, -1), (W, 10), (10, H), (W / 2, H + 160)] {
            assert_eq!(v.pick_pixel(x, y, W, H), None, "({x}, {y})");
        }
    }

    #[test]
    fn rows_and_columns_displayed() {
        let v = view();
        // 1 / hf = 11.58 and 1.25 / wf = 19.06.
        assert_eq!(v.rows_displayed(), 11);
        assert_eq!(v.cols_displayed(), 19);
    }

    #[test]
    fn the_view_recenters_only_when_the_cursor_leaves_it() {
        let mut v = view();
        // Half of 19 columns is 9: columns 41..=59 are in view.
        for col in (41..50).rev() {
            assert!(!v.keep_cursor_visible(Hex::new(50, col), Direction::Left), "column {col}");
        }
        assert_eq!(v.center(), Hex::new(50, 50));
        assert!(v.keep_cursor_visible(Hex::new(50, 40), Direction::Left));
        assert_eq!(v.center(), Hex::new(50, 40));
        // Only the column changed, and now 31..=49 are in view.
        assert!(!v.keep_cursor_visible(Hex::new(50, 49), Direction::Right));
        assert!(v.keep_cursor_visible(Hex::new(50, 50), Direction::Right));
        assert_eq!(v.center(), Hex::new(50, 50));

        // Half of 11 rows is 5: rows 45..=55 are in view.
        assert!(!v.keep_cursor_visible(Hex::new(45, 50), Direction::Up));
        assert!(v.keep_cursor_visible(Hex::new(44, 50), Direction::Up));
        assert_eq!(v.center(), Hex::new(44, 50));
        assert!(!v.keep_cursor_visible(Hex::new(49, 50), Direction::Down));
        assert!(v.keep_cursor_visible(Hex::new(50, 50), Direction::Down));
        assert_eq!(v.center(), Hex::new(50, 50));
    }

    #[test]
    fn a_step_only_checks_the_side_it_moves_toward() {
        // The pointer can put the cursor near the edge of the widget, outside the
        // counted columns. Stepping back toward the middle must not move the view.
        let mut v = view();
        assert!(!v.keep_cursor_visible(Hex::new(50, 61), Direction::Left));
        assert!(!v.keep_cursor_visible(Hex::new(57, 50), Direction::Up));
        assert_eq!(v.center(), Hex::new(50, 50));
    }

    #[test]
    fn no_wrap_around_near_the_left_and_top_edges() {
        // The C++ computed `centerCol - colsDisplayed / 2` in unsigned numbers: near
        // the edge it wrapped to a huge value and every step recentered.
        let mut v = view();
        v.center_on(Hex::new(3, 4));
        assert!(!v.keep_cursor_visible(Hex::new(3, 3), Direction::Left));
        assert!(!v.keep_cursor_visible(Hex::new(3, 0), Direction::Left));
        assert!(!v.keep_cursor_visible(Hex::new(2, 4), Direction::Up));
        assert!(!v.keep_cursor_visible(Hex::new(0, 4), Direction::Up));
        assert_eq!(v.center(), Hex::new(3, 4));
    }

    /// The widget's normalized device coordinates of a pixel's middle.
    fn ndc(x: i32, y: i32) -> (f32, f32) {
        let (half_w, half_h) = (W as f32 / 2.0, H as f32 / 2.0);
        ((x as f32 + 0.5 - half_w) / half_w, -(y as f32 + 0.5 - half_h) / half_h)
    }

    #[test]
    fn a_pan_moves_the_view_by_parts_of_a_hex() {
        let mut v = view();
        assert_eq!(v.middle(), [75.0, -100.0]);
        // A third of a hex to the right and a little up: the same center hex,
        // drawn that much to the left and lower.
        assert!(v.pan(0.5, 0.25));
        assert_eq!(v.center(), Hex::new(50, 50));
        assert_eq!(v.middle(), [75.5, -99.75]);
        assert_eq!(v.hex_center(Hex::new(50, 50)), [-0.5, -0.25]);
        // Further, and the next column's hex is the nearest one.
        assert!(v.pan(0.5, 0.0));
        assert_eq!(v.center(), Hex::new(50, 51));
        assert!(close(v.middle()[0], 76.0));
        // The hexes are where they were on the map, whichever is the center.
        assert!(close(v.hex_center(Hex::new(50, 50))[0], -1.0));
        assert!(!v.pan(0.0, 0.0), "no move");
        // Centering on a hex forgets the part of a hex.
        v.center_on(Hex::new(50, 51));
        assert_eq!(v.middle(), [76.5, -100.0]);
    }

    #[test]
    fn a_pan_stops_at_the_maps_edges() {
        let mut v = view();
        assert!(v.pan(-1000.0, 1000.0));
        assert_eq!((v.center(), v.middle()), (Hex::new(0, 0), [0.0, 0.0]));
        assert!(!v.pan(-1.0, 1.0), "already in the corner");
        assert!(v.pan(1000.0, -1000.0));
        assert_eq!((v.center(), v.middle()), (Hex::new(99, 99), [148.5, -198.0]));
        // Not a number: not a move.
        assert!(!v.pan(f32::NAN, 0.0) && !v.pan(0.0, f32::INFINITY));
        assert_eq!(v.middle(), [148.5, -198.0]);
    }

    #[test]
    fn panning_by_pixels_keeps_the_map_under_the_pointer() {
        for level in 0..ZOOM_LEVELS.len() {
            let mut v = view();
            v.zoom_in();
            (0..level).for_each(|_| {
                v.zoom_out();
            });
            // What is under the pixel (300, 200) ...
            let (x, y) = ndc(300, 200);
            let under = v.grid_at(x, y);
            // ... is under (337, 171) after the fingers went 37 right and 29 up.
            assert!(v.pan_pixels(37.0, -29.0));
            let (x, y) = ndc(337, 171);
            let after = v.grid_at(x, y);
            assert!((under[0] - after[0]).abs() < 1e-3 && (under[1] - after[1]).abs() < 1e-3, "level {level}");
        }
        // A view with no size cannot say how far a pixel is.
        let mut v = MapView::new(100, 100, Hex::new(50, 50));
        assert!(!v.pan_pixels(10.0, 10.0));
    }

    #[test]
    fn picking_and_the_visible_hexes_hold_after_a_pan() {
        for (dx, dy) in [(0.3, 0.0), (0.74, 0.99), (-0.6, -0.8), (7.1, -3.3), (-200.0, 0.4)] {
            let mut v = view();
            v.zoom_in();
            for level in 0..ZOOM_LEVELS.len() {
                v.center_on(Hex::new(50, 50));
                v.pan(dx, dy);
                let (hw, hh) = v.half_extent();
                let visible: Vec<VisibleHex> = v.visible_hexes().collect();
                let mut picked = 0;
                for row in 0..100 {
                    for col in 0..100 {
                        let hex = Hex::new(row, col);
                        let [x, y] = v.hex_center(hex);
                        if x.abs() - 1.0 < hw && y.abs() - 1.0 < hh {
                            assert!(
                                visible.iter().any(|vh| vh.hex == hex && vh.center == [x, y]),
                                "pan ({dx}, {dy}), level {level}: {hex} is not drawn"
                            );
                        }
                        if x.abs() < hw && y.abs() < hh {
                            let [nx, ny] = v.to_ndc([x, y]);
                            assert_eq!(v.pick(nx, ny), Some(hex), "pan ({dx}, {dy}), level {level}");
                            assert!(v.shows(hex));
                            picked += 1;
                        } else {
                            assert!(!v.shows(hex));
                        }
                    }
                }
                assert!(picked > 10, "pan ({dx}, {dy}), level {level}: {picked}");
                v.zoom_out();
            }
        }
    }

    #[test]
    fn a_zoom_about_a_point_keeps_what_is_under_it() {
        let mut v = view();
        let (x, y) = ndc(650, 120);
        let under = v.grid_at(x, y);
        assert!(v.zoom_about(x, y, true));
        assert_eq!(v.zoom_level(), 0);
        let after = v.grid_at(x, y);
        assert!((under[0] - after[0]).abs() < 1e-3 && (under[1] - after[1]).abs() < 1e-3, "{under:?} {after:?}");
        assert!(!v.zoom_about(x, y, true), "the closest level");
        assert_eq!(v.grid_at(x, y), after, "and nothing moved");
        while v.zoom_about(x, y, false) {}
        assert_eq!(v.zoom_level(), ZOOM_LEVELS.len() - 1);
        let wide = v.grid_at(x, y);
        assert!((under[0] - wide[0]).abs() < 1e-2 && (under[1] - wide[1]).abs() < 1e-2, "{under:?} {wide:?}");
        // About the middle, a zoom is the plain one.
        let mut plain = view();
        plain.zoom_out();
        let mut about = view();
        about.zoom_about(0.0, 0.0, false);
        assert_eq!(plain, about);
        // In a corner of the map the middle cannot follow: it stays on the map.
        let mut v = view();
        v.center_on(Hex::new(0, 0));
        assert!(v.zoom_about(0.9, -0.9, false));
        assert_eq!(v.middle(), [0.0, 0.0]);
    }

    #[test]
    fn following_the_cursor_forgets_the_pan_on_that_axis() {
        let mut v = view();
        v.pan(0.4, 0.6);
        assert!(v.keep_cursor_visible(Hex::new(50, 70), Direction::Right));
        assert_eq!(v.center(), Hex::new(50, 70));
        assert_eq!(v.middle(), [105.0, -99.4]);
        assert!(v.keep_cursor_visible(Hex::new(70, 70), Direction::Down));
        assert_eq!(v.middle(), [105.0, -140.0]);
    }

    #[test]
    fn border_thresholds() {
        let mut v = view();
        assert!(close(v.border_threshold(true), 0.84));
        assert!(close(v.border_threshold(false), 0.94));
        v.zoom_in();
        assert!(close(v.border_threshold(true), 0.85));
        assert!(close(v.border_threshold(false), 0.95));
        while v.zoom_out() {}
        assert!(close(v.border_threshold(true), 0.81));
        assert!(close(v.border_threshold(false), 0.91));
    }
}
