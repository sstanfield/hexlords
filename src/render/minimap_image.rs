//! The minimap as a picture for the UI, drawn on the CPU: for a minimap in a
//! window (the list of cities), which the host cannot draw under the UI as it
//! draws the sidebar's (`docs`, "Things that bite"). The same colors as the
//! minimap: each hex its terrain's, each city a square in its holder's, and
//! one city ringed so that it is found at once. The production window's
//! has more on it ([`Marks`]): the rally points, the cities whose work
//! waits, and where the chosen city sends its units.
//!
//! Not a port: the C++ had no windows.

use turbo_ferret::{Image, Size};

use super::map_renderer::{BATTLE_BORDER, OWN_WAY_BORDER, RING_COLOR, SELECTED_RING, side_color};
use super::minimap_renderer::{BACKING, terrain_color};
use crate::game::Sight;
use crate::minimap_view::MiniMapView;
use crate::state::{Hex, Map};

/// What is marked on the map's picture besides the terrain and the cities.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Marks {
    /// A hex ringed in gold (the city chosen).
    pub ringed: Option<Hex>,
    /// Flags (the side's rally points): white, on a pole, edged in dark.
    pub flags: Vec<Hex>,
    /// Hexes with a small orange ring (cities whose work waits, or that
    /// build nothing).
    pub warned: Vec<Hex>,
    /// A line, light on dark, from a hex to another (where the chosen
    /// city's units go).
    pub line: Option<(Hex, Hex)>,
}

/// The map in `w` x `h` pixels, with `ringed` (a city's hex, say) ringed in
/// gold: straight-alpha RGBA, top row first, opaque. As `sight` knows it, as
/// the minimap shows it (`city_marks`).
pub fn minimap_image(map: &Map, w: i32, h: i32, ringed: Option<Hex>, sight: &Sight) -> Image {
    minimap_marked(map, w, h, &Marks { ringed, ..Marks::default() }, sight)
}

/// [`minimap_image`] with all of `marks` on it: the line under the cities,
/// the rings and the flags over them.
pub fn minimap_marked(map: &Map, w: i32, h: i32, marks: &Marks, sight: &Sight) -> Image {
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
                let hex = view.pick(x, y);
                map.tile(hex).filter(|_| sight.knows(hex)).map_or([br, bg, bb], |t| terrain_color(t.kind))
            };
            rgba.extend([byte(r), byte(g), byte(b), 255]);
        }
    }
    let mut image = Image { size: Size::new(w, h), rgba };
    let reach = view.city_mark_reach();
    let edge = (reach / 4.0).max(1.0);
    let dark = [RING_COLOR[0], RING_COLOR[1], RING_COLOR[2]];
    // Where the chosen city's units go: a light line on a dark one.
    if let Some((from, to)) = marks.line {
        let (a, b) = (pixel_of(&view, from), pixel_of(&view, to));
        let width = edge.max(2.0);
        line(&mut image, a, b, width + 2.0, dark);
        line(&mut image, a, b, width, OWN_WAY_BORDER);
    }
    // The cities: a dark square, and in it the holder's color.
    for city in map.cities().iter().filter(|c| sight.knows(c.pos)) {
        let [cx, cy] = pixel_of(&view, city.pos);
        let [r, g, b] = side_color(sight.holder(city));
        fill_square(&mut image, cx, cy, reach + edge, [RING_COLOR[0], RING_COLOR[1], RING_COLOR[2]]);
        fill_square(&mut image, cx, cy, reach, [r, g, b]);
    }
    // Those warned of: a thin orange ring close round the square.
    for hex in &marks.warned {
        let [cx, cy] = pixel_of(&view, *hex);
        let (radius, width) = (reach * 1.6 + 1.5, (reach / 3.0).max(1.5));
        ring(&mut image, cx, cy, radius, width + 1.5, dark);
        ring(&mut image, cx, cy, radius, width, BATTLE_BORDER);
    }
    // The ringed one: a gold ring well outside its square, edged in dark.
    if let Some(hex) = marks.ringed {
        let [cx, cy] = pixel_of(&view, hex);
        let (radius, width) = (reach * 2.2 + 3.0, (reach / 2.0).max(2.0));
        let gold = [SELECTED_RING[0], SELECTED_RING[1], SELECTED_RING[2]];
        ring(&mut image, cx, cy, radius, width + 2.0, dark);
        ring(&mut image, cx, cy, radius, width, gold);
    }
    // The flags: a pole up from the hex's middle, and a white cloth on it.
    for hex in &marks.flags {
        let [cx, cy] = pixel_of(&view, *hex);
        let size = (reach * 1.2).max(2.0);
        let (top, pole) = (cy - size * 2.4, (size / 3.0).max(1.0));
        line(&mut image, [cx, cy], [cx, top], pole + 1.0, dark);
        let cloth = [cx + size, top + size * 0.7];
        fill_square(&mut image, cloth[0], cloth[1], size * 0.75 + 1.0, dark);
        fill_square(&mut image, cloth[0], cloth[1], size * 0.75, OWN_WAY_BORDER);
    }
    image
}

/// The pixels within `width / 2` of the segment from `a` to `b`.
fn line(image: &mut Image, a: [f32; 2], b: [f32; 2], width: f32, color: [f32; 3]) {
    let length = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
    let steps = (length * 2.0).ceil().max(1.0) as i32;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let (x, y) = (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t);
        fill_square(image, x, y, (width / 2.0).max(0.5), color);
    }
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
        let image = minimap_image(&map(), 240, 200, None, &Sight::all());
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
        let plain = minimap_image(&map(), 240, 200, None, &Sight::all());
        let ringed = minimap_image(&map(), 240, 200, Some(Hex::new(15, 15)), &Sight::all());
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
        assert!(minimap_image(&map(), 0, 0, Some(Hex::new(15, 15)), &Sight::all()).rgba.is_empty());
        assert_eq!(minimap_image(&map(), 3, 2, Some(Hex::new(0, 0)), &Sight::all()).rgba.len(), 24);
    }

    #[test]
    fn flags_rings_and_a_line_are_marked_where_they_are() {
        let plain = minimap_image(&map(), 240, 200, None, &Sight::all());
        let (flag, warned, from, to) = (Hex::new(14, 8), Hex::new(5, 5), Hex::new(5, 5), Hex::new(15, 35));
        let marks = Marks { ringed: None, flags: vec![flag], warned: vec![warned], line: Some((from, to)) };
        let image = minimap_marked(&map(), 240, 200, &marks, &Sight::all());
        let mut view = MiniMapView::new(20, 40);
        view.resize(240, 200);
        let count = |image: &Image, c: [f32; 3]| image.rgba.chunks(4).filter(|p| p[..3] == byte(c)).count();
        // White: the flag's cloth and the line, which the plain one has none of.
        assert_eq!(count(&plain, OWN_WAY_BORDER), 0);
        assert!(count(&image, OWN_WAY_BORDER) > 40, "{}", count(&image, OWN_WAY_BORDER));
        // The line's middle is on the way from one to the other.
        let ([ax, ay], [bx, by]) = (pixel_of(&view, from), pixel_of(&view, to));
        let middle = (((ax + bx) / 2.0) as i32, ((ay + by) / 2.0) as i32);
        assert_eq!(pixel(&image, middle.0, middle.1), byte(OWN_WAY_BORDER));
        // The flag flies up and to the right of its hex.
        let [fx, fy] = pixel_of(&view, flag);
        let reach = view.city_mark_reach();
        let size = (reach * 1.2).max(2.0);
        let cloth = ((fx + size) as i32, (fy - size * 2.4 + size * 0.7) as i32);
        assert_eq!(pixel(&image, cloth.0, cloth.1), byte(OWN_WAY_BORDER));
        // An orange ring round the city warned of, and its square as it was.
        assert!(count(&image, BATTLE_BORDER) > 10);
        let [wx, wy] = pixel_of(&view, warned);
        assert_eq!(pixel(&image, wx as i32, wy as i32), byte(side_color(Side::Red)));
        // Without marks, the same picture as before.
        assert_eq!(minimap_marked(&map(), 240, 200, &Marks::default(), &Sight::all()).rgba, plain.rgba);
    }
}
