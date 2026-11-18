//! The hex model and its clipped variants for the map's edges. Pure data.
//!
//! A hex has seven vertices: its center, then the six corners clockwise from the
//! top left. It is 2 units wide (points at x = ±1) and 2 high (flat top and
//! bottom at y = ±1, from x = -0.5 to 0.5), with y up.
//!
//! The map's edges are straight, so hexes there are cut: column 0 loses its left
//! point and the last column its right one; in row 0 the odd columns (which sit
//! half a hex higher) lose their top half, and in the last row the even columns
//! lose their bottom half. A cut moves vertices; a half that is cut off collapses
//! into the center. The tables are the C++'s, digit for digit.
//!
//! C++: src/render/maprender.cpp MapRenderer::MapRenderer (the `hex_*_buffer_data`
//! tables) and MapRenderer::CTX::drawHex (the choice of a variant)

/// The vertices of a hex model.
pub const HEX_VERTICES: usize = 7;

/// Six triangles around the center. C++: hexindices
pub const HEX_INDICES: [u16; 18] = [0, 2, 1, 0, 3, 2, 0, 4, 3, 0, 5, 4, 0, 6, 5, 0, 1, 6];

/// (center weight, rim weight) of each vertex: interpolated over a triangle, the
/// rim weight is 0 at the hex's center and 1 on its rim, which is how the
/// fragment shader finds the border. Every variant uses this one table.
/// C++: hex_barry_buffer_data
pub const HEX_BARY: [[f32; 2]; HEX_VERTICES] =
    [[1.0, 0.0], [0.0, 1.0], [0.0, 1.0], [0.0, 1.0], [0.0, 1.0], [0.0, 1.0], [0.0, 1.0]];

/// Which model a hex is drawn with.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum HexVariant {
    /// A whole hex.
    Full,
    /// Without its left point (column 0).
    Left,
    /// Without its right point (the last column).
    Right,
    /// Without its top half (odd columns in row 0).
    Top,
    /// Without its bottom half (even columns in the last row).
    Bottom,
    TopRight,
    BottomLeft,
    BottomRight,
}

//                                        center       left top      right top    right        right bottom  left bottom   left
const POS_FULL: [[f32; 2]; 7] =
    [[0.0, 0.0], [-0.5, 1.0], [0.5, 1.0], [1.0, 0.0], [0.5, -1.0], [-0.5, -1.0], [-1.0, 0.0]];
const POS_LEFT: [[f32; 2]; 7] =
    [[0.0, 0.0], [-0.5, 1.0], [0.5, 1.0], [1.0, 0.0], [0.5, -1.0], [-0.5, -1.0], [-0.5, 0.0]];
const POS_RIGHT: [[f32; 2]; 7] =
    [[0.0, 0.0], [-0.5, 1.0], [0.5, 1.0], [0.5, 0.0], [0.5, -1.0], [-0.5, -1.0], [-1.0, 0.0]];
const POS_TOP: [[f32; 2]; 7] = [[0.0, 0.0], [0.0, 0.0], [0.0, 0.0], [1.0, 0.0], [0.5, -1.0], [-0.5, -1.0], [-1.0, 0.0]];
const POS_BOTTOM: [[f32; 2]; 7] =
    [[0.0, 0.0], [-0.5, 1.0], [0.5, 1.0], [1.0, 0.0], [0.0, 0.0], [0.0, 0.0], [-1.0, 0.0]];
const POS_TOP_RIGHT: [[f32; 2]; 7] =
    [[0.0, 0.0], [0.0, 0.0], [0.0, 0.0], [0.5, 0.0], [0.5, -1.0], [-0.5, -1.0], [-1.0, 0.0]];
const POS_BOTTOM_LEFT: [[f32; 2]; 7] =
    [[0.0, 0.0], [-0.5, 1.0], [0.5, 1.0], [1.0, 0.0], [0.0, 0.0], [0.0, 0.0], [-0.5, 0.0]];
const POS_BOTTOM_RIGHT: [[f32; 2]; 7] =
    [[0.0, 0.0], [-0.5, 1.0], [0.5, 1.0], [0.5, 0.0], [0.0, 0.0], [0.0, 0.0], [-1.0, 0.0]];

const UV_FULL: [[f32; 2]; 7] = [[0.5, 0.5], [0.25, 1.0], [0.75, 1.0], [1.0, 0.5], [0.75, 0.0], [0.25, 0.0], [0.0, 0.5]];
const UV_LEFT: [[f32; 2]; 7] =
    [[0.5, 0.5], [0.25, 1.0], [0.75, 1.0], [1.0, 0.5], [0.75, 0.0], [0.25, 0.0], [0.25, 0.5]];
const UV_RIGHT: [[f32; 2]; 7] =
    [[0.5, 0.5], [0.25, 1.0], [0.75, 1.0], [0.75, 0.5], [0.75, 0.0], [0.25, 0.0], [0.0, 0.5]];
const UV_TOP: [[f32; 2]; 7] = [[0.5, 0.5], [0.5, 0.5], [0.5, 0.5], [1.0, 0.5], [0.75, 0.0], [0.25, 0.0], [0.0, 0.5]];
const UV_BOTTOM: [[f32; 2]; 7] =
    [[0.5, 0.5], [0.25, 1.0], [0.75, 1.0], [1.0, 0.5], [0.75, 0.5], [0.25, 0.5], [0.0, 0.5]];
const UV_TOP_RIGHT: [[f32; 2]; 7] =
    [[0.5, 0.5], [0.5, 0.5], [0.5, 0.5], [0.75, 0.5], [0.75, 0.0], [0.25, 0.0], [0.0, 0.5]];
const UV_BOTTOM_LEFT: [[f32; 2]; 7] =
    [[0.5, 0.5], [0.25, 1.0], [0.75, 1.0], [1.0, 0.5], [0.75, 0.5], [0.25, 0.5], [0.25, 0.5]];
const UV_BOTTOM_RIGHT: [[f32; 2]; 7] =
    [[0.5, 0.5], [0.25, 1.0], [0.75, 1.0], [0.75, 0.5], [0.75, 0.5], [0.25, 0.5], [0.0, 0.5]];

impl HexVariant {
    pub const ALL: [HexVariant; 8] = [
        HexVariant::Full,
        HexVariant::Left,
        HexVariant::Right,
        HexVariant::Top,
        HexVariant::Bottom,
        HexVariant::TopRight,
        HexVariant::BottomLeft,
        HexVariant::BottomRight,
    ];

    /// The model for the hex at (row, col) of a `rows` x `cols` map.
    /// C++: MapRenderer::CTX::drawHex (the same tests in the same order; a later
    /// one overrides an earlier one)
    pub fn choose(row: u16, col: u16, rows: u16, cols: u16) -> HexVariant {
        let (last_row, last_col) = (rows.saturating_sub(1), cols.saturating_sub(1));
        let odd = col % 2 == 1;
        let mut variant = HexVariant::Full;
        if col == 0 {
            variant = HexVariant::Left;
        }
        if col == last_col {
            variant = HexVariant::Right;
        }
        if row == 0 && odd {
            variant = if col == last_col { HexVariant::TopRight } else { HexVariant::Top };
        }
        if row == last_row && !odd {
            variant = if col == last_col { HexVariant::BottomRight } else { HexVariant::Bottom };
        }
        if row == last_row && col == 0 {
            variant = HexVariant::BottomLeft;
        }
        variant
    }

    /// The seven vertex positions, in model units around the hex's center.
    pub fn positions(self) -> &'static [[f32; 2]; HEX_VERTICES] {
        match self {
            HexVariant::Full => &POS_FULL,
            HexVariant::Left => &POS_LEFT,
            HexVariant::Right => &POS_RIGHT,
            HexVariant::Top => &POS_TOP,
            HexVariant::Bottom => &POS_BOTTOM,
            HexVariant::TopRight => &POS_TOP_RIGHT,
            HexVariant::BottomLeft => &POS_BOTTOM_LEFT,
            HexVariant::BottomRight => &POS_BOTTOM_RIGHT,
        }
    }

    /// The seven texture coordinates. (v = 1 is at the top of the hex, and the
    /// tile images are uploaded top row first, so a tile is drawn upside down
    /// relative to its file, as in the C++.)
    pub fn uvs(self) -> &'static [[f32; 2]; HEX_VERTICES] {
        match self {
            HexVariant::Full => &UV_FULL,
            HexVariant::Left => &UV_LEFT,
            HexVariant::Right => &UV_RIGHT,
            HexVariant::Top => &UV_TOP,
            HexVariant::Bottom => &UV_BOTTOM,
            HexVariant::TopRight => &UV_TOP_RIGHT,
            HexVariant::BottomLeft => &UV_BOTTOM_LEFT,
            HexVariant::BottomRight => &UV_BOTTOM_RIGHT,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_indices_make_six_triangles_around_the_center() {
        assert_eq!(HEX_INDICES.len(), 18);
        for tri in HEX_INDICES.chunks(3) {
            assert_eq!(tri[0], 0, "every triangle starts at the center");
            assert!(tri[1] >= 1 && tri[1] <= 6 && tri[2] >= 1 && tri[2] <= 6 && tri[1] != tri[2]);
        }
        // Each corner is used by two triangles.
        for corner in 1..=6u16 {
            assert_eq!(HEX_INDICES.iter().filter(|&&i| i == corner).count(), 2, "corner {corner}");
        }
    }

    #[test]
    fn the_rim_weight_is_zero_at_the_center_and_one_at_the_corners() {
        assert_eq!(HEX_BARY[0], [1.0, 0.0]);
        assert!(HEX_BARY[1..].iter().all(|b| *b == [0.0, 1.0]));
    }

    #[test]
    fn inner_hexes_are_whole() {
        for (row, col) in [(1, 1), (5, 4), (98, 98), (0, 2), (99, 97), (50, 1)] {
            assert_eq!(HexVariant::choose(row, col, 100, 100), HexVariant::Full, "({row}, {col})");
        }
    }

    #[test]
    fn edge_hexes_are_clipped() {
        let v = |row, col| HexVariant::choose(row, col, 100, 100);
        // Left and right columns.
        assert_eq!(v(5, 0), HexVariant::Left);
        assert_eq!(v(5, 99), HexVariant::Right);
        // Row 0: odd columns stick out above and lose their top; even ones do not.
        assert_eq!(v(0, 1), HexVariant::Top);
        assert_eq!(v(0, 97), HexVariant::Top);
        assert_eq!(v(0, 2), HexVariant::Full);
        // The last row: even columns stick out below and lose their bottom.
        assert_eq!(v(99, 2), HexVariant::Bottom);
        assert_eq!(v(99, 98), HexVariant::Bottom);
        assert_eq!(v(99, 1), HexVariant::Full);
        // Corners. Column 0 is even: whole on top, cut below. Column 99 is odd.
        assert_eq!(v(0, 0), HexVariant::Left);
        assert_eq!(v(99, 0), HexVariant::BottomLeft);
        assert_eq!(v(0, 99), HexVariant::TopRight);
        assert_eq!(v(99, 99), HexVariant::Right);
    }

    #[test]
    fn an_even_last_column_is_cut_at_the_bottom_right() {
        let v = |row, col| HexVariant::choose(row, col, 20, 21);
        assert_eq!(v(19, 20), HexVariant::BottomRight);
        assert_eq!(v(0, 20), HexVariant::Right);
        assert_eq!(v(0, 19), HexVariant::Top);
    }

    /// The edges a variant is cut along: (left, right, top, bottom).
    fn cuts(variant: HexVariant) -> (bool, bool, bool, bool) {
        use HexVariant as V;
        (
            matches!(variant, V::Left | V::BottomLeft),
            matches!(variant, V::Right | V::TopRight | V::BottomRight),
            matches!(variant, V::Top | V::TopRight),
            matches!(variant, V::Bottom | V::BottomLeft | V::BottomRight),
        )
    }

    #[test]
    fn clipped_models_stay_inside_their_cut() {
        for variant in HexVariant::ALL {
            let (left, right, top, bottom) = cuts(variant);
            let pos = variant.positions();
            assert_eq!(pos[0], [0.0, 0.0], "{variant:?}: the center");
            let min_x = pos.iter().map(|p| p[0]).fold(f32::MAX, f32::min);
            let max_x = pos.iter().map(|p| p[0]).fold(f32::MIN, f32::max);
            let min_y = pos.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
            let max_y = pos.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
            assert_eq!(min_x, if left { -0.5 } else { -1.0 }, "{variant:?}: left");
            assert_eq!(max_x, if right { 0.5 } else { 1.0 }, "{variant:?}: right");
            assert_eq!(max_y, if top { 0.0 } else { 1.0 }, "{variant:?}: top");
            assert_eq!(min_y, if bottom { 0.0 } else { -1.0 }, "{variant:?}: bottom");
        }
    }

    #[test]
    fn texture_coordinates_follow_the_positions() {
        // Where a vertex is not collapsed into the center, its texture coordinate
        // is its position mapped from the model's box to the unit square.
        for variant in HexVariant::ALL {
            let (pos, uv) = (variant.positions(), variant.uvs());
            assert_eq!(uv[0], [0.5, 0.5]);
            for i in 1..HEX_VERTICES {
                assert!(uv[i].iter().all(|c| (0.0..=1.0).contains(c)), "{variant:?} vertex {i}");
                if pos[i] != [0.0, 0.0] {
                    let want = [(pos[i][0] + 1.0) / 2.0, (pos[i][1] + 1.0) / 2.0];
                    assert_eq!(uv[i], want, "{variant:?} vertex {i}");
                }
            }
        }
    }

    #[test]
    fn the_full_hex_is_the_cpp_table() {
        assert_eq!(
            HexVariant::Full.positions(),
            &[[0.0, 0.0], [-0.5, 1.0], [0.5, 1.0], [1.0, 0.0], [0.5, -1.0], [-0.5, -1.0], [-1.0, 0.0]]
        );
        assert_eq!(
            HexVariant::Full.uvs(),
            &[[0.5, 0.5], [0.25, 1.0], [0.75, 1.0], [1.0, 0.5], [0.75, 0.0], [0.25, 0.0], [0.0, 0.5]]
        );
    }
}
