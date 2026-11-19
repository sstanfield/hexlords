//! Which picture a hex's road is drawn with, and how it is turned.
//!
//! A road leaves a hex through the sides whose neighbours join roads
//! (`Map::road_links`): a mask of six bits, 64 shapes. Turned by sixths of a
//! turn they are 14, and `tools/assetgen` draws those, once as a road and
//! once as a bridge (`docs/tiles.md`). The picture of a shape is laid on the
//! hex turned: its texture coordinates are those of the hex's corners turned
//! back.
//!
//! Not a port: the C++ had no roads.

/// The shapes there are pictures of: of every mask, the smallest of its six
/// turns.
pub const ROAD_SHAPES: [u8; 14] = [0, 1, 3, 5, 7, 9, 11, 13, 15, 21, 23, 27, 31, 63];

/// The pictures: every shape as a road, then every shape as a bridge.
pub const ROAD_PICTURES: usize = 2 * ROAD_SHAPES.len();

/// A mask turned clockwise by `turns` sixths of a turn.
pub fn turned(mask: u8, turns: u8) -> u8 {
    let (mask, turns) = (mask & 0x3f, turns % 6);
    ((mask << turns) | (mask >> (6 - turns))) & 0x3f
}

/// The shape a mask is drawn with (its place in [`ROAD_SHAPES`]) and how far
/// that shape is turned clockwise to give the mask.
pub fn shape_of(mask: u8) -> (usize, u8) {
    let (shape, back) = (0..6).map(|k| (turned(mask, k), k)).min().unwrap_or((0, 0));
    (ROAD_SHAPES.iter().position(|s| *s == shape).unwrap_or(0), (6 - back) % 6)
}

/// The picture of a road: `shape` is a place in [`ROAD_SHAPES`]. Over water
/// it is a bridge.
pub fn picture(shape: usize, over_water: bool) -> usize {
    shape + if over_water { ROAD_SHAPES.len() } else { 0 }
}

/// A picture's file, relative to an asset directory.
pub fn road_image(picture: usize) -> String {
    let shape = ROAD_SHAPES[picture % ROAD_SHAPES.len()];
    format!("roads/{}_{shape:02}.png", if picture < ROAD_SHAPES.len() { "road" } else { "bridge" })
}

/// The texture coordinates of a point of the hex model (x right, y up) on a
/// picture that is turned clockwise by `turns` sixths of a turn: the point is
/// turned back, as the hex turns onto itself, and read as on any tile.
pub fn turned_uv(point: [f32; 2], turns: u8) -> [f32; 2] {
    // The model's hex is a regular one stretched upward by 2 / √3.
    let h = 3f32.sqrt() / 2.0;
    let (sin, cos) = (f32::from(turns % 6) * std::f32::consts::FRAC_PI_3).sin_cos();
    let [x, y] = point;
    let (tx, ty) = (x * cos - y * h * sin, (x * sin + y * h * cos) / h);
    [(tx + 1.0) / 2.0, (1.0 - ty) / 2.0]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::HexVariant;
    use crate::state::HexSide;

    #[test]
    fn every_mask_has_a_shape_and_a_turn() {
        let mut seen = [false; ROAD_SHAPES.len()];
        for mask in 0..64u8 {
            let (shape, turns) = shape_of(mask);
            assert_eq!(turned(ROAD_SHAPES[shape], turns), mask, "mask {mask}");
            seen[shape] = true;
        }
        assert!(seen.iter().all(|s| *s));
        // A road in from the north alone is shape 1 as it is; from the south,
        // turned half way round.
        assert_eq!(shape_of(1 << HexSide::North.index()), (1, 0));
        assert_eq!(shape_of(1 << HexSide::South.index()), (1, 3));
        assert_eq!(shape_of(0), (0, 0));
        assert_eq!(shape_of(63), (13, 0));
    }

    #[test]
    fn the_pictures_files() {
        assert_eq!(ROAD_PICTURES, 28);
        assert_eq!(road_image(picture(0, false)), "roads/road_00.png");
        assert_eq!(road_image(picture(5, false)), "roads/road_09.png");
        assert_eq!(road_image(picture(5, true)), "roads/bridge_09.png");
        assert_eq!(road_image(27), "roads/bridge_63.png");
        // Every one is in the repository's assets.
        let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
        for picture in 0..ROAD_PICTURES {
            assert!(assets.join(road_image(picture)).is_file(), "{}", road_image(picture));
        }
    }

    #[test]
    fn a_turned_picture_has_its_sides_where_the_roads_leave() {
        let close = |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).abs() < 1e-5 && (a[1] - b[1]).abs() < 1e-5;
        // Not turned: a tile's own coordinates.
        for (pos, uv) in HexVariant::Full.positions().iter().zip(HexVariant::Full.uvs()) {
            assert!(close(turned_uv(*pos, 0), *uv), "{pos:?}");
        }
        // The middles of the sides, clockwise from the top. A picture whose
        // road leaves through the top (the middle of its upper edge), turned
        // by k, is read there at the middle of side k.
        let middles = [[0.0, 1.0], [0.75, 0.5], [0.75, -0.5], [0.0, -1.0], [-0.75, -0.5], [-0.75, 0.5]];
        for (k, middle) in middles.into_iter().enumerate() {
            assert!(close(turned_uv(middle, k as u8), [0.5, 0.0]), "side {k}: {:?}", turned_uv(middle, k as u8));
        }
        // The hex's corners stay corners, and its middle the middle.
        let corners = HexVariant::Full.uvs();
        for turns in 0..6 {
            assert!(close(turned_uv([0.0, 0.0], turns), [0.5, 0.5]));
            for pos in &HexVariant::Full.positions()[1..] {
                let uv = turned_uv(*pos, turns);
                assert!(corners[1..].iter().any(|c| close(uv, *c)), "{pos:?} turned {turns}: {uv:?}");
            }
        }
    }
}
