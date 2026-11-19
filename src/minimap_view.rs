//! The minimap's view: the whole map fitted into the widget, and the marker that
//! shows which part of it the main map is looking at. Pure math.
//!
//! The coordinates are those of `view`: hex models 2 units wide and high, columns
//! 1.5 units apart, rows 2 units apart and growing downward, odd columns 1 unit
//! higher. Here they are measured from the middle of the map.
//!
//! C++: src/render/minimaprender.cpp MiniMapRenderer (resizeMap, renderMap,
//! miniMapLocMarker)

use crate::state::Hex;
use crate::view::{HEX_HEIGHT_ADJUST, MapView, factors, hex_at};

#[derive(Clone, Debug, PartialEq)]
pub struct MiniMapView {
    rows: u16,
    cols: u16,
    size: (i32, i32),
    wf: f32,
    hf: f32,
}

impl MiniMapView {
    /// A view of a `rows` x `cols` map, with no size yet.
    pub fn new(rows: u16, cols: u16) -> MiniMapView {
        let mut v = MiniMapView { rows, cols, size: (0, 0), wf: 0.0, hf: 0.0 };
        v.resize(0, 0);
        v
    }

    /// The map's extent in model units: (width, height). The map's edges are
    /// straight (edge hexes are clipped), so the first column loses its left
    /// point and the last its right one, and likewise half a hex at the top and
    /// the bottom.
    fn extent(&self) -> (f32, f32) {
        (1.5 * f32::from(self.cols - 1) + 1.0, 2.0 * f32::from(self.rows - 1) + 1.0)
    }

    /// How high the map is on screen for its width: the height of a rectangle
    /// that fits it exactly, divided by that rectangle's width. A minimap of
    /// this shape has no empty margins.
    pub fn shape(&self) -> f32 {
        let (w, h) = self.extent();
        h * HEX_HEIGHT_ADJUST / w
    }

    /// The middle of the map, in model units measured from the center of hex (0, 0).
    fn middle(&self) -> [f32; 2] {
        // x runs from -0.5 to 1.5 * (cols - 1) + 0.5; y from -2 * (rows - 1) to 1.
        [0.75 * f32::from(self.cols - 1), 0.5 - f32::from(self.rows - 1)]
    }

    /// The widget's size in pixels changed: fit the whole map into it, keeping the
    /// hexes' shape.
    ///
    /// C++: MiniMapRenderer::resizeMap. It scaled by the smaller of
    /// `1 / (rows * hf)` and `1 / ((cols / 2) * 1.5 * wf)` around hex (rows / 2,
    /// cols / 2), which is within a hex of this but cut half a row off the top
    /// (deviation D13).
    pub fn resize(&mut self, w: i32, h: i32) {
        self.size = (w, h);
        let (wf, hf) = factors(w, h, 1.0);
        let (map_w, map_h) = self.extent();
        let fit = (2.0 / (map_w * wf)).min(2.0 / (map_h * hf));
        (self.wf, self.hf) = (wf * fit, hf * fit);
    }

    pub fn size(&self) -> (i32, i32) {
        self.size
    }

    /// The (width, height) factors: model units times these are the widget's
    /// normalized device coordinates.
    pub fn factors(&self) -> (f32, f32) {
        (self.wf, self.hf)
    }

    /// The center of a hex's model, in model units relative to the middle of the
    /// widget.
    pub fn hex_center(&self, hex: Hex) -> [f32; 2] {
        let [mx, my] = self.middle();
        let odd = if hex.col % 2 == 1 { 1.0 } else { 0.0 };
        [1.5 * f32::from(hex.col) - mx, odd - 2.0 * f32::from(hex.row) - my]
    }

    /// Model units to the widget's normalized device coordinates.
    pub fn to_ndc(&self, model: [f32; 2]) -> [f32; 2] {
        [model[0] * self.wf, model[1] * self.hf]
    }

    /// The hex under a point given in the widget's normalized device coordinates
    /// (x right, y up). A point beside the map picks the nearest hex on its edge,
    /// so that dragging past the minimap's border keeps working.
    pub fn pick(&self, rel_x: f32, rel_y: f32) -> Hex {
        let [mx, my] = self.middle();
        let (row, col) = hex_at(rel_x / self.wf + mx, rel_y / self.hf + my);
        Hex::new(row.clamp(0, i32::from(self.rows) - 1) as u16, col.clamp(0, i32::from(self.cols) - 1) as u16)
    }

    /// The hex under a pixel of a `w` x `h` widget (x right, y down, from its top
    /// left corner); see [`pick`](Self::pick). `None` if the widget has no size.
    pub fn pick_pixel(&self, x: i32, y: i32, w: i32, h: i32) -> Option<Hex> {
        if w <= 0 || h <= 0 {
            return None;
        }
        let (half_w, half_h) = (w as f32 / 2.0, h as f32 / 2.0);
        Some(self.pick((x as f32 + 0.5 - half_w) / half_w, -(y as f32 + 0.5 - half_h) / half_h))
    }

    /// The part of the map the main view shows, as a rectangle `[left, bottom,
    /// right, top]` in the minimap's normalized device coordinates, clamped to the
    /// widget.
    ///
    /// C++: MiniMapRenderer::miniMapLocMarker, which drew a quad as large as the
    /// whole minimap at a center that never changed (deviation D5).
    pub fn marker(&self, main: &MapView) -> [f32; 4] {
        // The middle of the main widget, measured from the middle of the map.
        let ([x, y], [mx, my]) = (main.middle(), self.middle());
        let (cx, cy) = (x - mx, y - my);
        let (half_w, half_h) = main.half_extent();
        let [left, bottom] = self.to_ndc([cx - half_w, cy - half_h]);
        let [right, top] = self.to_ndc([cx + half_w, cy + half_h]);
        [left, bottom, right, top].map(|v| v.clamp(-1.0, 1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The demo's minimap widget at 1024 x 768.
    const W: i32 = 200;
    const H: i32 = 231;

    fn mini(rows: u16, cols: u16, w: i32, h: i32) -> MiniMapView {
        let mut v = MiniMapView::new(rows, cols);
        v.resize(w, h);
        v
    }

    /// The map's bounding box in normalized device coordinates: [left, bottom,
    /// right, top], from the hexes' own centers and clipped models.
    fn bounds(v: &MiniMapView, rows: u16, cols: u16) -> [f32; 4] {
        let [left, _] = v.to_ndc([v.hex_center(Hex::new(0, 0))[0] - 0.5, 0.0]);
        let [right, _] = v.to_ndc([v.hex_center(Hex::new(0, cols - 1))[0] + 0.5, 0.0]);
        // Row 0: an even column's hex reaches 1 above its center.
        let [_, top] = v.to_ndc([0.0, v.hex_center(Hex::new(0, 0))[1] + 1.0]);
        // The last row: an even column's hex is cut off at its center.
        let [_, bottom] = v.to_ndc([0.0, v.hex_center(Hex::new(rows - 1, 0))[1]]);
        [left, bottom, right, top]
    }

    #[test]
    fn a_widget_of_the_maps_shape_is_filled_to_its_edges() {
        for (rows, cols) in [(100u16, 100u16), (20, 80), (300, 40)] {
            let mut v = MiniMapView::new(rows, cols);
            v.resize(400, (400.0 * v.shape()).round() as i32);
            // The map spans the widget (2 in normalized coordinates) both ways.
            let (wf, hf) = v.factors();
            let (map_w, map_h) = v.extent();
            assert!((map_w * wf - 2.0).abs() < 0.02, "{rows}x{cols}: {}", map_w * wf);
            assert!((map_h * hf - 2.0).abs() < 0.02, "{rows}x{cols}: {}", map_h * hf);
        }
    }

    #[test]
    fn the_whole_map_fits_and_is_centered() {
        for (rows, cols, w, h) in
            [(100, 100, W, H), (20, 20, W, H), (20, 600, 300, 100), (600, 20, 100, 300), (50, 80, 64, 64)]
        {
            let v = mini(rows, cols, w, h);
            let [left, bottom, right, top] = bounds(&v, rows, cols);
            assert!(left >= -1.0001 && right <= 1.0001 && bottom >= -1.0001 && top <= 1.0001, "{rows}x{cols}");
            assert!((left + right).abs() < 1e-4 && (bottom + top).abs() < 1e-4, "{rows}x{cols}: not centered");
            // It touches the widget's edge on one axis at least.
            let touches_x = (right - 1.0).abs() < 1e-4;
            let touches_y = (top - 1.0).abs() < 1e-4;
            assert!(touches_x || touches_y, "{rows}x{cols}: {right} {top}");
            // Hexes keep their shape: in pixels, the height factor is the width
            // factor times the adjust.
            let (wf, hf) = v.factors();
            let ratio = (hf * h as f32) / (wf * w as f32);
            assert!((ratio - crate::view::HEX_HEIGHT_ADJUST).abs() < 1e-4, "{rows}x{cols}: {ratio}");
        }
    }

    #[test]
    fn the_demo_minimap_fills_its_widget() {
        // 100 x 100 hexes are 149.5 x 199 model units, which on screen is almost
        // exactly the widget's shape: the width limits the fit, and the height is
        // within a percent of full.
        let v = mini(100, 100, W, H);
        let [left, bottom, right, top] = bounds(&v, 100, 100);
        assert!((right - 1.0).abs() < 1e-4 && (left + 1.0).abs() < 1e-4, "{left} {right}");
        assert!(top <= 1.0 && top > 0.98 && (bottom + top).abs() < 1e-4, "{bottom} {top}");
    }

    #[test]
    fn hex_centers_follow_the_grid() {
        let v = mini(100, 100, W, H);
        let at = |row, col| v.hex_center(Hex::new(row, col));
        let [x0, y0] = at(10, 10);
        assert_eq!(at(10, 12), [x0 + 3.0, y0]);
        // Rows grow downward, odd columns are one unit higher.
        assert_eq!(at(11, 10), [x0, y0 - 2.0]);
        assert_eq!(at(10, 11), [x0 + 1.5, y0 + 1.0]);
    }

    #[test]
    fn no_size_yet_gives_finite_factors() {
        let v = MiniMapView::new(100, 100);
        let (wf, hf) = v.factors();
        assert!(wf.is_finite() && hf.is_finite() && wf > 0.0 && hf > 0.0);
    }

    fn main_view(center: Hex) -> MapView {
        let mut m = MapView::new(100, 100, center);
        m.resize(819, 537);
        m
    }

    #[test]
    fn the_marker_is_the_main_views_part_of_the_map() {
        let v = mini(100, 100, W, H);
        let main = main_view(Hex::new(50, 50));
        let [left, bottom, right, top] = v.marker(&main);
        assert!(left < right && bottom < top);
        // The marker's corners are where the hexes at the main widget's corners
        // are on the minimap: go from the minimap back to the map.
        let (half_w, half_h) = main.half_extent();
        let (wf, hf) = v.factors();
        assert!(((right - left) / wf - 2.0 * half_w).abs() < 1e-3);
        assert!(((top - bottom) / hf - 2.0 * half_h).abs() < 1e-3);
        // Its middle is the center hex.
        let [cx, cy] = v.to_ndc(v.hex_center(Hex::new(50, 50)));
        assert!(((left + right) / 2.0 - cx).abs() < 1e-5 && ((bottom + top) / 2.0 - cy).abs() < 1e-5);
        // The main view shows about 20 of 100 columns and 12 of 100 rows.
        let share_x = (right - left) / (bounds(&v, 100, 100)[2] * 2.0);
        let share_y = (top - bottom) / 2.0;
        assert!((0.15..0.25).contains(&share_x), "{share_x}");
        assert!((0.08..0.16).contains(&share_y), "{share_y}");
    }

    #[test]
    fn the_marker_follows_the_main_view() {
        let v = mini(100, 100, W, H);
        let [l0, b0, r0, t0] = v.marker(&main_view(Hex::new(50, 50)));
        // Ten columns right and ten rows down.
        let [l1, b1, r1, t1] = v.marker(&main_view(Hex::new(60, 60)));
        let (wf, hf) = v.factors();
        assert!((l1 - l0 - 15.0 * wf).abs() < 1e-5 && (r1 - r0 - 15.0 * wf).abs() < 1e-5);
        assert!((b1 - b0 + 20.0 * hf).abs() < 1e-5 && (t1 - t0 + 20.0 * hf).abs() < 1e-5);
        // An odd center column does not move it up: the view's middle is the same.
        let [_, b2, _, t2] = v.marker(&main_view(Hex::new(50, 51)));
        assert!((b2 - b0).abs() < 1e-6 && (t2 - t0).abs() < 1e-6);
    }

    #[test]
    fn the_marker_moves_with_a_pan_of_part_of_a_hex() {
        let v = mini(100, 100, W, H);
        let mut main = main_view(Hex::new(50, 50));
        let [l0, b0, r0, t0] = v.marker(&main);
        main.pan(0.5, -0.25);
        let [l1, b1, r1, t1] = v.marker(&main);
        let (wf, hf) = v.factors();
        assert!((l1 - l0 - 0.5 * wf).abs() < 1e-6 && (r1 - r0 - 0.5 * wf).abs() < 1e-6);
        assert!((b1 - b0 + 0.25 * hf).abs() < 1e-6 && (t1 - t0 + 0.25 * hf).abs() < 1e-6);
    }

    #[test]
    fn the_marker_grows_as_the_main_view_zooms_out() {
        let v = mini(100, 100, W, H);
        let mut main = main_view(Hex::new(50, 50));
        let [l0, _, r0, _] = v.marker(&main);
        main.zoom_out();
        let [l1, _, r1, _] = v.marker(&main);
        assert!(r1 - l1 > r0 - l0);
    }

    #[test]
    fn the_marker_is_clamped_to_the_widget() {
        let v = mini(100, 100, W, H);
        let [left, bottom, right, top] = v.marker(&main_view(Hex::new(0, 0)));
        assert_eq!(top, 1.0);
        assert!(left >= -1.0 && right > left && bottom < top && bottom >= -1.0);
        let mut far = main_view(Hex::new(99, 99));
        while far.zoom_out() {}
        let m = v.marker(&far);
        assert!(m.iter().all(|c| (-1.0..=1.0).contains(c)), "{m:?}");
        assert_eq!(m[1], -1.0);
    }
    #[test]
    fn picking_a_hex_on_the_minimap() {
        let v = mini(100, 100, W, H);
        for hex in [Hex::new(0, 0), Hex::new(50, 50), Hex::new(50, 51), Hex::new(99, 99), Hex::new(13, 86)] {
            let [x, y] = v.to_ndc(v.hex_center(hex));
            assert_eq!(v.pick(x, y), hex);
        }
        // The middle of the widget is the middle of the map.
        let middle = v.pick(0.0, 0.0);
        assert!((49..=50).contains(&middle.row) && (49..=50).contains(&middle.col), "{middle}");
    }

    #[test]
    fn picking_beside_the_map_gives_the_nearest_edge_hex() {
        let v = mini(100, 100, W, H);
        assert_eq!(v.pick(-5.0, 5.0), Hex::new(0, 0));
        assert_eq!(v.pick(5.0, -5.0), Hex::new(99, 99));
        assert_eq!(v.pick(5.0, 5.0), Hex::new(0, 99));
        let left = v.pick(-3.0, 0.0);
        assert_eq!(left.col, 0);
        assert!((48..=51).contains(&left.row), "{left}");
        // A wide widget has bars left and right of the map: they pick its sides.
        let wide = mini(100, 100, 400, 100);
        assert_eq!(wide.pick(-0.9, 0.0).col, 0);
        assert_eq!(wide.pick(0.9, 0.0).col, 99);
    }

    #[test]
    fn picking_by_pixel() {
        let v = mini(100, 100, W, H);
        // The corners of the widget are the corners of the map.
        assert_eq!(v.pick_pixel(0, 0, W, H), Some(Hex::new(0, 0)));
        assert_eq!(v.pick_pixel(W - 1, H - 1, W, H), Some(Hex::new(99, 99)));
        // A hex is 2 pixels wide here: its own pixel picks it or a neighbour.
        let hex = Hex::new(30, 70);
        let [x, y] = v.to_ndc(v.hex_center(hex));
        let (px, py) = (((x + 1.0) / 2.0 * W as f32) as i32, ((1.0 - y) / 2.0 * H as f32) as i32);
        let got = v.pick_pixel(px, py, W, H).unwrap();
        assert!(got.row.abs_diff(hex.row) <= 1 && got.col.abs_diff(hex.col) <= 1, "{got}");
        // Pixels outside the widget (a drag that left it) still pick.
        assert_eq!(v.pick_pixel(-50, -50, W, H), Some(Hex::new(0, 0)));
        assert_eq!(v.pick_pixel(W + 50, H + 50, W, H), Some(Hex::new(99, 99)));
        assert_eq!(v.pick_pixel(10, 10, 0, 0), None);
    }
}
