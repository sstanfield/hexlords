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
    /// The hex in the middle of the widget (an odd column's hex is 1 unit above
    /// the exact middle).
    center: Hex,
    size: (i32, i32),
    wf: f32,
    hf: f32,
}

impl MapView {
    /// A view of a `rows` x `cols` map centered on `center`, with no size yet.
    pub fn new(rows: u16, cols: u16, center: Hex) -> MapView {
        let (wf, hf) = factors(0, 0, ZOOM_LEVELS[START_ZOOM]);
        MapView { rows, cols, zoom_level: START_ZOOM, center, size: (0, 0), wf, hf }
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

    /// C++: MapRenderer::setMapDisplayCenter
    pub fn center_on(&mut self, hex: Hex) {
        self.center = hex;
    }

    /// The center of a hex's model, in model units relative to the middle of the
    /// widget.
    pub fn hex_center(&self, hex: Hex) -> [f32; 2] {
        let col = i32::from(hex.col) - i32::from(self.center.col);
        let row = i32::from(self.center.row) - i32::from(hex.row);
        [1.5 * col as f32, 2.0 * row as f32 + if hex.col % 2 == 1 { 1.0 } else { 0.0 }]
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

    /// The hexes to draw: every hex of the map that may show in the widget (with
    /// some to spare around the edges), column by column. C++: MapRenderer::renderMap
    pub fn visible_hexes(&self) -> impl Iterator<Item = VisibleHex> + '_ {
        let mut num_x = (1.0 / self.wf) as i32 + 1;
        num_x += num_x / 4 + 6; // columns interleave, so they need some padding
        let num_y = (1.0 / self.hf) as i32 + 2;
        let (half_x, half_y) = (num_x / 2, num_y / 2);
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
    /// (x right, y up, both -1..1), if it is on the map.
    ///
    /// The formula treats a hex as the rectangle around it, so near the slanted
    /// edges of a hex it can answer with the neighbour in the next column.
    /// C++: MapRenderer::setCenterMap (which, despite its name, picks a hex)
    pub fn pick(&self, rel_x: f32, rel_y: f32) -> Option<Hex> {
        let half = |v: f32| if v < 0.0 { -0.5 } else { 0.5 };
        let col = i32::from(self.center.col) + (rel_x / (1.5 * self.wf) + half(rel_x)) as i32;
        let odd = if col % 2 != 0 { 0.5 } else { 0.0 };
        let row = i32::from(self.center.row) - (rel_y / (2.0 * self.hf) + half(rel_y) - odd) as i32;
        Hex::on_map(row, col, self.rows, self.cols)
    }

    /// The hex under a pixel of a `w` x `h` widget (x right, y down, from its top
    /// left corner). C++: MapWidget::OnEvent (the pointer cases)
    pub fn pick_pixel(&self, x: i32, y: i32, w: i32, h: i32) -> Option<Hex> {
        if w <= 0 || h <= 0 {
            return None;
        }
        let (half_w, half_h) = (w as f32 / 2.0, h as f32 / 2.0);
        self.pick((x as f32 - half_w) / half_w, -(y as f32 - half_h) / half_h)
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
                Direction::Left | Direction::Right => self.center.col = cursor.col,
                Direction::Up | Direction::Down => self.center.row = cursor.row,
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
        for level in 0..ZOOM_LEVELS.len() {
            let mut v = view();
            while v.zoom_level() < level {
                v.zoom_out();
            }
            while v.zoom_level() > level {
                v.zoom_in();
            }
            let visible: Vec<VisibleHex> = v.visible_hexes().collect();
            // No hex twice.
            let mut hexes: Vec<Hex> = visible.iter().map(|vh| vh.hex).collect();
            hexes.sort();
            hexes.dedup();
            assert_eq!(hexes.len(), visible.len(), "level {level}");
            // Every hex whose model touches the widget is in the list.
            let (hw, hh) = v.half_extent();
            for row in 0..100 {
                for col in 0..100 {
                    let hex = Hex::new(row, col);
                    let [x, y] = v.hex_center(hex);
                    let touches = x.abs() - 1.0 < hw && y.abs() - 1.0 < hh;
                    if touches {
                        assert!(hexes.binary_search(&hex).is_ok(), "level {level}: {hex} is missing");
                    }
                }
            }
            for vh in &visible {
                assert_eq!(vh.center, v.hex_center(vh.hex));
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
        // 1/wf = 15.25, so 16 + 4 + 6 = 26 columns give -13..=13; 1/hf = 11.58, so
        // 13 rows give -6..=6.
        assert_eq!(view().visible_hexes().count(), 27 * 13);
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

    /// The C++ formula, copied: MapRenderer::setCenterMap. Returns (row, col),
    /// on the map or not.
    fn cpp_pick(center_row: i32, center_col: i32, wf: f32, hf: f32, x: f32, y: f32) -> (i32, i32) {
        let col = center_col + (x / (1.5 * wf) + (if x < 0.0 { -0.5 } else { 0.5 })) as i32;
        let row = center_row
            - ((y / (2.0 * hf) + (if y < 0.0 { -0.5 } else { 0.5 })) - (if col % 2 != 0 { 0.5 } else { 0.0 })) as i32;
        (row, col)
    }

    #[test]
    fn pick_matches_the_cpp_formula() {
        for center in [Hex::new(50, 50), Hex::new(50, 51), Hex::new(1, 1), Hex::new(99, 98)] {
            let mut v = view();
            v.center_on(center);
            v.zoom_in();
            for _ in 0..ZOOM_LEVELS.len() {
                let (wf, hf) = v.factors();
                for i in -40..=40 {
                    for j in -40..=40 {
                        let (x, y) = (i as f32 / 40.0, j as f32 / 40.0);
                        let (row, col) = cpp_pick(center.row.into(), center.col.into(), wf, hf, x, y);
                        assert_eq!(v.pick(x, y), Hex::on_map(row, col, 100, 100), "center {center} at ({x}, {y})");
                    }
                }
                v.zoom_out();
            }
        }
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
        // A widget without a size picks nothing.
        assert_eq!(v.pick_pixel(0, 0, 0, 0), None);
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
