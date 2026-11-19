//! The minimap as a picture for the UI, drawn on the CPU: for a minimap in a
//! window (the list of cities), which the host cannot draw under the UI as it
//! draws the sidebar's (`docs`, "Things that bite"). The same colors as the
//! minimap: each hex its terrain's, each city a square in its holder's, and
//! one city ringed so that it is found at once.
//!
//! Not a port: the C++ had no windows.

use turbo_ferret::{Image, Size};

use super::map_renderer::{RING_COLOR, SELECTED_RING, side_color};
use super::minimap_renderer::{BACKING, terrain_color};
use crate::minimap_view::MiniMapView;
use crate::state::{Hex, Map};

/// The map in `w` x `h` pixels, with `ringed` (a city's hex, say) ringed in
/// gold: straight-alpha RGBA, top row first, opaque.
pub fn minimap_image(map: &Map, w: i32, h: i32, ringed: Option<Hex>) -> Image {
    let (w, h) = (w.max(0), h.max(0));
    let mut view = MiniMapView::new(map.rows(), map.cols());
    view.resize(w, h);
    let (half_w, half_h) = (w as f32 / 2.0, h as f32 / 2.0);
    let [left, bottom, right, top] = view.map_rect();
    let byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    let (br, bg, bb, _) = BACKING;
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for py in 0..h {
        for px in 0..w {
            let (x, y) = ((px as f32 + 0.5 - half_w) / half_w, -(py as f32 + 0.5 - half_h) / half_h);
            let [r, g, b] = if x < left || x > right || y < bottom || y > top {
                [br, bg, bb]
            } else {
                map.tile(view.pick(x, y)).map_or([br, bg, bb], |t| terrain_color(t.kind))
            };
            rgba.extend([byte(r), byte(g), byte(b), 255]);
        }
    }
    let mut image = Image { size: Size::new(w, h), rgba };
    // The cities: a dark square, and in it the holder's color.
    let reach = view.city_mark_reach();
    let edge = (reach / 4.0).max(1.0);
    for city in map.cities().iter() {
        let [cx, cy] = pixel_of(&view, city.pos);
        let [r, g, b] = side_color(city.side);
        fill_square(&mut image, cx, cy, reach + edge, [RING_COLOR[0], RING_COLOR[1], RING_COLOR[2]]);
        fill_square(&mut image, cx, cy, reach, [r, g, b]);
    }
    // The ringed one: a gold ring well outside its square, edged in dark.
    if let Some(hex) = ringed {
        let [cx, cy] = pixel_of(&view, hex);
        let (radius, width) = (reach * 2.2 + 3.0, (reach / 2.0).max(2.0));
        let gold = [SELECTED_RING[0], SELECTED_RING[1], SELECTED_RING[2]];
        let dark = [RING_COLOR[0], RING_COLOR[1], RING_COLOR[2]];
        ring(&mut image, cx, cy, radius, width + 2.0, dark);
        ring(&mut image, cx, cy, radius, width, gold);
    }
    image
}

/// The pixel (x right, y down, fractional) of a hex's middle.
fn pixel_of(view: &MiniMapView, hex: Hex) -> [f32; 2] {
    let (w, h) = view.size();
    let [x, y] = view.to_ndc(view.hex_center(hex));
    [(x + 1.0) / 2.0 * w as f32, (1.0 - y) / 2.0 * h as f32]
}

fn put(image: &mut Image, x: i32, y: i32, color: [f32; 3]) {
    let (w, h) = (image.size.w, image.size.h);
    if x < 0 || y < 0 || x >= w || y >= h {
        return;
    }
    let i = ((y * w + x) * 4) as usize;
    for (k, c) in color.into_iter().enumerate() {
        image.rgba[i + k] = (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    }
}

/// The pixels whose middles are within `reach` of a point, each way.
fn fill_square(image: &mut Image, cx: f32, cy: f32, reach: f32, color: [f32; 3]) {
    let (x0, x1) = ((cx - reach).round() as i32, (cx + reach).round() as i32);
    let (y0, y1) = ((cy - reach).round() as i32, (cy + reach).round() as i32);
    for y in y0..y1 {
        for x in x0..x1 {
            put(image, x, y, color);
        }
    }
}

/// The pixels whose middles are within `width / 2` of a circle.
fn ring(image: &mut Image, cx: f32, cy: f32, radius: f32, width: f32, color: [f32; 3]) {
    let outer = (radius + width).ceil() as i32;
    for dy in -outer..=outer {
        for dx in -outer..=outer {
            let (x, y) = (cx.floor() as i32 + dx, cy.floor() as i32 + dy);
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            if (d - radius).abs() <= width / 2.0 {
                put(image, x, y, color);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Side, TileType};

    fn pixel(image: &Image, x: i32, y: i32) -> [u8; 3] {
        let i = ((y * image.size.w + x) * 4) as usize;
        [image.rgba[i], image.rgba[i + 1], image.rgba[i + 2]]
    }

    fn byte(c: [f32; 3]) -> [u8; 3] {
        c.map(|v| (v * 255.0).round() as u8)
    }

    /// A wide grass map with a lake, a red city and nobody's.
    fn map() -> Map {
        let mut map = Map::new(20, 40, TileType::Grass).unwrap();
        for col in 25..35 {
            map.tile_mut(Hex::new(10, col)).unwrap().kind = TileType::Water;
        }
        let red = map.add_city("Alder", Hex::new(5, 5), 1).unwrap();
        map.city_mut(red).unwrap().side = Side::Red;
        map.add_city("Birch", Hex::new(15, 15), 0).unwrap();
        map
    }

    #[test]
    fn the_map_in_its_colors_on_black() {
        let image = minimap_image(&map(), 240, 200, None);
        assert_eq!((image.size, image.rgba.len()), (Size::new(240, 200), 240 * 200 * 4));
        assert!(image.rgba.chunks(4).all(|p| p[3] == 255));
        // The map is wider than the picture's shape: black above and below.
        assert_eq!(pixel(&image, 120, 1), [0, 0, 0]);
        assert_eq!(pixel(&image, 120, 198), [0, 0, 0]);
        // Grass, and the lake where it is.
        let view = {
            let mut v = MiniMapView::new(20, 40);
            v.resize(240, 200);
            v
        };
        let [x, y] = pixel_of(&view, Hex::new(10, 30));
        assert_eq!(pixel(&image, x as i32, y as i32), byte(terrain_color(TileType::Water)));
        let [x, y] = pixel_of(&view, Hex::new(2, 30));
        assert_eq!(pixel(&image, x as i32, y as i32), byte(terrain_color(TileType::Grass)));
        // The cities in their holders' colors, in a dark edge.
        let [x, y] = pixel_of(&view, Hex::new(5, 5));
        assert_eq!(pixel(&image, x as i32, y as i32), byte(side_color(Side::Red)));
        let edge = (x + view.city_mark_reach() + 0.5) as i32;
        assert_eq!(pixel(&image, edge, y as i32), byte([RING_COLOR[0], RING_COLOR[1], RING_COLOR[2]]));
        let [x, y] = pixel_of(&view, Hex::new(15, 15));
        assert_eq!(pixel(&image, x as i32, y as i32), byte(side_color(Side::Grey)));
    }

    #[test]
    fn the_ringed_city_has_a_gold_ring_round_it() {
        let plain = minimap_image(&map(), 240, 200, None);
        let ringed = minimap_image(&map(), 240, 200, Some(Hex::new(15, 15)));
        let gold = byte([SELECTED_RING[0], SELECTED_RING[1], SELECTED_RING[2]]);
        let golden = |image: &Image| image.rgba.chunks(4).filter(|p| p[..3] == gold).count();
        assert_eq!(golden(&plain), 0);
        assert!(golden(&ringed) > 30, "{}", golden(&ringed));
        // Only around that city: the other's corner is as it was.
        let mut view = MiniMapView::new(20, 40);
        view.resize(240, 200);
        let [x, y] = pixel_of(&view, Hex::new(5, 5));
        assert_eq!(pixel(&ringed, x as i32, y as i32), pixel(&plain, x as i32, y as i32));
        // Any size, even none.
        assert!(minimap_image(&map(), 0, 0, Some(Hex::new(15, 15))).rgba.is_empty());
        assert_eq!(minimap_image(&map(), 3, 2, Some(Hex::new(0, 0))).rgba.len(), 24);
    }
}
