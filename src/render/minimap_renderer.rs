//! The minimap: the whole map as flat-colored hexes on black, its cities as
//! squares in their holders' colors, drawn once into a texture and blitted
//! from it every frame, with a marker on top that shows the part of the map
//! the main view is looking at.
//!
//! The C++ drew the minimap into the window and read it back with
//! `glReadPixels` to cache it. Here it is drawn into a render target, which has
//! to happen before the frame's own render pass begins.
//!
//! C++: src/render/minimaprender.cpp MiniMapRenderer (renderMap, CTX::drawHex,
//! CTX::saveMiniMap, CTX::miniMapLocMarker, getTileColor)

use miniquad::{PassAction, RenderPass, RenderingBackend, TextureId};

use super::hex_mesh::{HEX_INDICES, HEX_VERTICES, HexVariant};
use super::map_renderer::{BATTLE_BORDER, DIGIT_BARS, RING_COLOR, flag_number, side_color};
use super::mesh::{BufferPool, MAX_VERTICES, Mesh};
use super::shaders::{BlitVertex, FlatVertex, Pipelines};
use super::textures::texture_params;
use crate::game::Sight;
use crate::minimap_view::MiniMapView;
use crate::state::{Hex, Map, TileType};

/// Behind the hexes. C++: MiniMapRenderer::renderMap ("Map backing color")
pub const BACKING: (f32, f32, f32, f32) = (0.0, 0.0, 0.0, 1.0);
/// The marker over the main view's part of the map. C++: miniMapLocMarker
pub const MARKER: [f32; 4] = [0.0, 0.0, 0.0, 0.3];

/// A terrain's color on the minimap. Open country has the color its tile has
/// from far away (the mean of `assets/tiles/<name>_tile.png`; a test holds
/// them together). The places are a picture on grass, so their tiles are all
/// but grass from far away: they get the color of what stands on them, to be
/// found on the minimap. C++: getTileColor
pub fn terrain_color(kind: TileType) -> [f32; 3] {
    match kind {
        TileType::Grass => [0.37, 0.56, 0.22],
        TileType::Forest => [0.15, 0.29, 0.11],
        TileType::Swamp => [0.30, 0.33, 0.22],
        TileType::Desert => [0.81, 0.69, 0.45],
        TileType::Hill => [0.51, 0.53, 0.29],
        TileType::Mountain => [0.48, 0.45, 0.41],
        TileType::Water => [0.19, 0.38, 0.64],
        // The dome's gold, the bare ground's brown, nobody's roofs' grey (a
        // city's mark in its holder's color goes over it: `city_marks`).
        TileType::Temple => [0.95, 0.80, 0.30],
        TileType::Ruin => [0.42, 0.28, 0.18],
        TileType::City => [0.55, 0.55, 0.55],
    }
}

/// The seven vertices of a hex on the minimap, in the minimap's normalized device
/// coordinates. C++: MiniMapRenderer::CTX::drawHex
pub fn minimap_hex(map: &Map, view: &MiniMapView, hex: Hex) -> [FlatVertex; HEX_VERTICES] {
    let [r, g, b] = map.tile(hex).map_or([0.0; 3], |t| terrain_color(t.kind));
    let positions = HexVariant::choose(hex.row, hex.col, map.rows(), map.cols()).positions();
    let [cx, cy] = view.hex_center(hex);
    std::array::from_fn(|i| FlatVertex {
        pos: view.to_ndc([positions[i][0] + cx, positions[i][1] + cy]),
        color: [r, g, b, 1.0],
    })
}

/// The cities' marks: for each city a dark square and over it one a little
/// smaller in its holder's color (nobody's grey), bigger than a hex of a big
/// map so that every city can be found. Quads of four vertices. With fog of
/// war, as `sight` knows them: in the fog, in the colors of who held them
/// when they were seen last; on land never seen, none.
pub fn city_marks(map: &Map, view: &MiniMapView, sight: &Sight) -> Vec<FlatVertex> {
    let edge = (view.city_mark_reach() / 4.0).max(1.0);
    let mut quads = Vec::with_capacity(map.cities().len() * 8);
    for city in map.cities().iter().filter(|c| sight.knows(c.pos)) {
        let [r, g, b] = side_color(sight.holder(city));
        quads.extend(quad_corners(view.city_mark(city.pos, edge)).map(|pos| FlatVertex { pos, color: RING_COLOR }));
        quads.extend(quad_corners(view.city_mark(city.pos, 0.0)).map(|pos| FlatVertex { pos, color: [r, g, b, 1.0] }));
    }
    quads
}

/// Where battles were fought, on the minimap: for each a cross, dark and
/// over it one a little thinner in the color the main map rings a battle's
/// hex with (`BATTLE_BORDER`), bigger than a city's mark so that it shows
/// beside one, and bigger still the brighter it is (0 to 1: the battle
/// being shown now, as its hex flashes). Quads of four vertices, four for
/// each battle. (A cross, where a city is a square: a shape of its own,
/// not a color alone.)
pub fn battle_marks(view: &MiniMapView, battles: &[(Hex, f32)]) -> Vec<FlatVertex> {
    let (w, h) = view.size();
    let (w, h) = (w.max(1) as f32, h.max(1) as f32);
    let [r, g, b] = BATTLE_BORDER;
    let mut quads = Vec::with_capacity(battles.len() * 16);
    for (hex, bright) in battles {
        let [x, y] = view.to_ndc(view.hex_center(*hex));
        let reach = view.city_mark_reach() * (1.5 + 0.7 * bright.clamp(0.0, 1.0));
        let thick = (reach * 0.28).max(1.0);
        for (more, color) in [(thick * 0.6 + 0.5, RING_COLOR), (0.0, [r, g, b, 1.0])] {
            // The two bars of the cross, each from corner to corner: its
            // ends, and half its width across.
            for way in [1.0, -1.0] {
                let (along, across) = (reach + more, (thick + more) * std::f32::consts::FRAC_1_SQRT_2);
                let corners = [
                    [-along - across, -along * way + across * way],
                    [-along + across, -along * way - across * way],
                    [along + across, along * way - across * way],
                    [along - across, along * way + across * way],
                ];
                quads.extend(corners.map(|[dx, dy]| FlatVertex { pos: [x + dx * 2.0 / w, y + dy * 2.0 / h], color }));
            }
        }
    }
    quads
}

/// The rally points' flags on the minimap (the owner's word, 2026-11-24:
/// "marking rally points on the minimap"): for each, as on the main map, a
/// pole and a cloth in the side's `color` from its hex up and to the right,
/// dark around them, and on the cloth the point's number (0 to 9) in seven
/// bars, as big as the minimap lets them be (they grow with the widget, as
/// the cities' marks do). Quads of four vertices.
pub fn flag_marks(view: &MiniMapView, flags: &[(Hex, u8)], color: [f32; 3]) -> Vec<FlatVertex> {
    let (w, h) = view.size();
    let (w, h) = (w.max(1) as f32, h.max(1) as f32);
    // A unit of the flag, in pixels: 9 of them tall, 6.5 wide.
    let u = (view.city_mark_reach() / 2.5).max(1.0);
    let [r, g, b] = color;
    let mut quads = Vec::with_capacity(flags.len() * 44);
    for (hex, n) in flags {
        let [x, y] = view.to_ndc(view.hex_center(*hex));
        // A rectangle in units from the hex's middle (x right, y up).
        let mut rect = |x0: f32, x1: f32, y0: f32, y1: f32, color: [f32; 4]| {
            let corners = quad_corners([x0, y0, x1, y1]);
            quads.extend(
                corners.map(|[dx, dy]| FlatVertex { pos: [x + dx * u * 2.0 / w, y + dy * u * 2.0 / h], color }),
            );
        };
        let (pole, cloth) = ((-0.5, 0.5, 0.0, 9.0), (0.5, 6.5, 4.0, 9.0));
        let grow = 0.6;
        for (x0, x1, y0, y1) in [pole, cloth] {
            rect(x0 - grow, x1 + grow, y0 - grow, y1 + grow, RING_COLOR);
        }
        rect(pole.0, pole.1, pole.2, pole.3, [0.85, 0.80, 0.68, 1.0]);
        rect(cloth.0, cloth.1, cloth.2, cloth.3, [r, g, b, 1.0]);
        // The number: seven bars, a unit thick at the least.
        let (x0, x1, y0, y1) = (2.2, 4.8, 4.8, 8.2);
        let t = (0.7f32).max(1.0 / u);
        let ym = (y0 + y1) / 2.0;
        let bars = [
            (x0, x1, y1 - t, y1),
            (x1 - t, x1, ym, y1),
            (x1 - t, x1, y0, ym),
            (x0, x1, y0, y0 + t),
            (x0, x0 + t, y0, ym),
            (x0, x0 + t, ym, y1),
            (x0, x1, ym - t / 2.0, ym + t / 2.0),
        ];
        let number = flag_number(color);
        for (bar, lit) in bars.iter().zip(DIGIT_BARS[usize::from(n % 10)]) {
            if lit {
                rect(bar.0, bar.1, bar.2, bar.3, number);
            }
        }
    }
    quads
}

/// A rectangle `[left, bottom, right, top]` as four corners and two triangles.
const QUAD_INDICES: [u16; 6] = [0, 1, 2, 0, 2, 3];

fn quad_corners(rect: [f32; 4]) -> [[f32; 2]; 4] {
    let [left, bottom, right, top] = rect;
    [[left, bottom], [right, bottom], [right, top], [left, top]]
}

/// The marker's quad. C++: MiniMapRenderer::CTX::miniMapLocMarker
pub fn marker_quad(rect: [f32; 4]) -> [FlatVertex; 4] {
    quad_corners(rect).map(|pos| FlatVertex { pos, color: MARKER })
}

/// The picture of the map: a texture and the pass that draws into it.
struct Target {
    pass: RenderPass,
    texture: TextureId,
    size: (i32, i32),
}

/// Draws the minimap.
#[derive(Default)]
pub struct MiniMapRenderer {
    target: Option<Target>,
    /// Scratch for the hexes while the picture is drawn.
    hexes: Mesh<FlatVertex>,
    /// One pair of buffers, refilled for each batch of hexes.
    hex_buffers: BufferPool,
    /// The blit quad and the marker.
    quad_buffers: BufferPool,
    /// How many times the picture was drawn (it should be rare).
    redraws: u64,
}

impl MiniMapRenderer {
    pub fn new() -> MiniMapRenderer {
        MiniMapRenderer::default()
    }

    /// Bring the picture up to date: make the render target the widget's size,
    /// and draw the map into it if the target is new or `dirty` says the map
    /// changed. Call this outside any render pass, before the frame's own.
    /// C++: MiniMapRenderer::resizeMap, renderMap (the dirty branch), saveMiniMap
    pub fn update(
        &mut self,
        ctx: &mut dyn RenderingBackend,
        pipelines: &Pipelines,
        (map, sight): (&Map, &Sight),
        view: &MiniMapView,
        dirty: bool,
    ) {
        let size = view.size();
        if size.0 <= 0 || size.1 <= 0 {
            self.drop_target(ctx);
            return;
        }
        let fresh = self.target.as_ref().is_none_or(|t| t.size != size);
        if fresh {
            self.drop_target(ctx);
            let texture = ctx.new_render_texture(texture_params(size.0 as u32, size.1 as u32));
            let pass = ctx.new_render_pass(texture, None);
            self.target = Some(Target { pass, texture, size });
        }
        if !(fresh || dirty) {
            return;
        }
        let Some(target) = &self.target else { return };
        let (r, g, b, a) = BACKING;
        ctx.begin_pass(Some(target.pass), PassAction::clear_color(r, g, b, a));
        ctx.apply_pipeline(&pipelines.flat);
        // A big map has more vertices than 16-bit indices reach: draw it in
        // batches, reusing one pair of buffers. (That is fine on OpenGL, where a
        // draw call has run by the time the buffer is written again.)
        let hexes_per_batch = MAX_VERTICES / HEX_VERTICES;
        let mut mesh = std::mem::take(&mut self.hexes);
        let flush = |mesh: &mut Mesh<FlatVertex>, buffers: &mut BufferPool, ctx: &mut dyn RenderingBackend| {
            if mesh.indices.is_empty() {
                return;
            }
            buffers.rewind();
            let (vb, ib) = buffers.upload(ctx, mesh);
            ctx.apply_bindings_from_slice(&[vb], ib, &[]);
            ctx.draw(0, mesh.indices.len() as i32, 1);
            mesh.vertices.clear();
            mesh.indices.clear();
        };
        // (Land never seen is left black.)
        for row in 0..map.rows() {
            for col in 0..map.cols() {
                if !sight.knows(Hex::new(row, col)) {
                    continue;
                }
                if mesh.vertices.len() / HEX_VERTICES >= hexes_per_batch {
                    flush(&mut mesh, &mut self.hex_buffers, ctx);
                }
                let base = mesh.vertices.len() as u16;
                mesh.vertices.extend_from_slice(&minimap_hex(map, view, Hex::new(row, col)));
                mesh.indices.extend(HEX_INDICES.iter().map(|i| base + i));
            }
        }
        flush(&mut mesh, &mut self.hex_buffers, ctx);
        // The cities over the hexes.
        for quad in city_marks(map, view, sight).chunks(4) {
            if mesh.vertices.len() + 4 > MAX_VERTICES {
                flush(&mut mesh, &mut self.hex_buffers, ctx);
            }
            let base = mesh.vertices.len() as u16;
            mesh.vertices.extend_from_slice(quad);
            mesh.indices.extend(QUAD_INDICES.iter().map(|i| base + i));
        }
        flush(&mut mesh, &mut self.hex_buffers, ctx);
        self.hexes = mesh;
        ctx.end_render_pass();
        self.redraws += 1;
    }

    /// Draw the minimap into the current viewport, which the caller has set to the
    /// minimap widget's rectangle: the cached picture, then the marker (`[left,
    /// bottom, right, top]` in normalized device coordinates).
    /// C++: MiniMapRenderer::renderMap (the cached branch), miniMapLocMarker
    /// Over both, `marks`: quads of four vertices (`battle_marks`).
    pub fn draw(
        &mut self,
        ctx: &mut dyn RenderingBackend,
        pipelines: &Pipelines,
        marker: [f32; 4],
        marks: &[FlatVertex],
    ) {
        let Some(target) = &self.target else { return };
        self.quad_buffers.rewind();

        // The picture fills the viewport. The texture's v = 0 is its bottom row,
        // the row drawn at y = -1.
        let corners = quad_corners([-1.0, -1.0, 1.0, 1.0]);
        let uvs = quad_corners([0.0, 0.0, 1.0, 1.0]);
        let blit = Mesh {
            vertices: (0..4).map(|i| BlitVertex { pos: corners[i], uv: uvs[i] }).collect(),
            indices: QUAD_INDICES.to_vec(),
        };
        let (vb, ib) = self.quad_buffers.upload(ctx, &blit);
        ctx.apply_pipeline(&pipelines.blit);
        ctx.apply_bindings_from_slice(&[vb], ib, &[target.texture]);
        ctx.draw(0, 6, 1);

        let quad = Mesh { vertices: marker_quad(marker).to_vec(), indices: QUAD_INDICES.to_vec() };
        let (vb, ib) = self.quad_buffers.upload(ctx, &quad);
        ctx.apply_pipeline(&pipelines.flat);
        ctx.apply_bindings_from_slice(&[vb], ib, &[]);
        ctx.draw(0, 6, 1);

        for batch in marks.chunks(MAX_VERTICES / 4 * 4) {
            let quads = batch.len() / 4;
            let indices = (0..quads as u16).flat_map(|q| QUAD_INDICES.map(|i| q * 4 + i)).collect();
            let mesh = Mesh { vertices: batch[..quads * 4].to_vec(), indices };
            let (vb, ib) = self.quad_buffers.upload(ctx, &mesh);
            ctx.apply_bindings_from_slice(&[vb], ib, &[]);
            ctx.draw(0, quads as i32 * 6, 1);
        }
    }

    /// How many times the picture has been drawn since the start.
    pub fn redraws(&self) -> u64 {
        self.redraws
    }

    fn drop_target(&mut self, ctx: &mut dyn RenderingBackend) {
        if let Some(target) = self.target.take() {
            // This deletes the pass's texture too.
            ctx.delete_render_pass(target.pass);
        }
    }

    /// Free the GPU objects.
    pub fn delete(&mut self, ctx: &mut dyn RenderingBackend) {
        self.drop_target(ctx);
        self.hex_buffers.delete_all(ctx);
        self.quad_buffers.delete_all(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// True for the terrains that are a picture on grass (see `terrain_color`).
    fn is_place(kind: TileType) -> bool {
        matches!(kind, TileType::Temple | TileType::Ruin | TileType::City)
    }

    fn mini() -> MiniMapView {
        let mut v = MiniMapView::new(100, 100);
        v.resize(200, 231);
        v
    }

    #[test]
    fn terrain_colors() {
        // No two alike: every pair differs clearly in some channel.
        for (i, a) in TileType::ALL.into_iter().enumerate() {
            for b in &TileType::ALL[i + 1..] {
                let (ca, cb) = (terrain_color(a), terrain_color(*b));
                let apart = (0..3).map(|k| (ca[k] - cb[k]).abs()).fold(0.0, f32::max);
                assert!(apart > 0.1, "{a:?} and {b:?}: {ca:?} {cb:?}");
            }
        }
        // Water is the blue one, and the places stand out from the grass.
        let water = terrain_color(TileType::Water);
        assert!(water[2] > water[0] && water[2] > water[1]);
        assert!(TileType::ALL.into_iter().filter(|k| is_place(*k)).count() == 3);
    }

    #[test]
    fn open_country_has_its_tiles_color() {
        use crate::render::textures::load_image;
        use crate::render::tile_image;
        let mut loader = turbo_ferret::FsLoader::new(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets"));
        for kind in TileType::ALL.into_iter().filter(|k| !is_place(*k)) {
            let tile = load_image(&mut loader, &tile_image(kind)).unwrap();
            let pixels = (tile.rgba.len() / 4) as f32;
            let mean: [f32; 3] = std::array::from_fn(|k| {
                tile.rgba.chunks_exact(4).map(|p| f32::from(p[k])).sum::<f32>() / pixels / 255.0
            });
            let color = terrain_color(kind);
            let apart = (0..3).map(|k| (mean[k] - color[k]).abs()).fold(0.0, f32::max);
            assert!(apart < 0.03, "{kind:?}: the tile is {mean:?}, the minimap {color:?}");
        }
    }

    #[test]
    fn a_minimap_hex_has_its_terrains_color_and_its_place() {
        let game = crate::game::grassland();
        let view = mini();
        let grass = minimap_hex(&game.map, &view, Hex::new(10, 11));
        assert!(grass.iter().all(|v| v.color == [0.37, 0.56, 0.22, 1.0]));
        let mountain = minimap_hex(&game.map, &view, Hex::new(10, 10));
        assert!(mountain.iter().all(|v| v.color == [0.48, 0.45, 0.41, 1.0]));
        // Its first vertex is its center, where the view puts the hex.
        assert_eq!(grass[0].pos, view.to_ndc(view.hex_center(Hex::new(10, 11))));
        // A whole hex is 2 model units wide and high.
        let (wf, hf) = view.factors();
        let xs: Vec<f32> = grass.iter().map(|v| v.pos[0]).collect();
        let ys: Vec<f32> = grass.iter().map(|v| v.pos[1]).collect();
        let span = |v: &[f32]| v.iter().cloned().fold(f32::MIN, f32::max) - v.iter().cloned().fold(f32::MAX, f32::min);
        assert!((span(&xs) - 2.0 * wf).abs() < 1e-6 && (span(&ys) - 2.0 * hf).abs() < 1e-6);
    }

    #[test]
    fn every_minimap_hex_is_inside_the_widget() {
        let game = crate::game::grassland();
        let view = mini();
        for row in 0..100 {
            for col in 0..100 {
                for v in minimap_hex(&game.map, &view, Hex::new(row, col)) {
                    assert!(v.pos.iter().all(|c| c.abs() <= 1.0001), "({row}, {col}): {:?}", v.pos);
                }
            }
        }
        // The corner hexes reach the edges the fit is made for.
        let right = minimap_hex(&game.map, &view, Hex::new(50, 99)).iter().map(|v| v.pos[0]).fold(f32::MIN, f32::max);
        assert!((right - 1.0).abs() < 1e-4, "{right}");
    }

    #[test]
    fn the_marker_is_a_translucent_black_quad() {
        let quad = marker_quad([-0.5, -0.25, 0.5, 0.75]);
        assert_eq!(quad.map(|v| v.pos), [[-0.5, -0.25], [0.5, -0.25], [0.5, 0.75], [-0.5, 0.75]]);
        assert!(quad.iter().all(|v| v.color == [0.0, 0.0, 0.0, 0.3]));
        // Two triangles that share the diagonal.
        assert_eq!(QUAD_INDICES, [0, 1, 2, 0, 2, 3]);
    }

    /// The owner's word, 2026-11-24: "marking rally points on the
    /// minimap". A flag for each point: dark behind a pole and a cloth in
    /// the side's color, from the hex up and to the right, and its number
    /// in bars on the cloth.
    #[test]
    fn a_rally_point_is_a_numbered_flag_in_the_sides_color() {
        let mut view = MiniMapView::new(30, 40);
        view.resize(200, 231);
        let hex = Hex::new(12, 20);
        let color = [0.8, 0.2, 0.2];
        let one = flag_marks(&view, &[(hex, 1)], color);
        let eight = flag_marks(&view, &[(hex, 8)], color);
        // Two dark, the pole, the cloth; then the lit bars (2 of a 1, 7 of an 8).
        assert_eq!((one.len(), eight.len()), (4 * (4 + 2), 4 * (4 + 7)));
        let colors: Vec<[f32; 4]> = one.chunks(4).map(|q| q[0].color).collect();
        assert_eq!(colors[..2], [RING_COLOR, RING_COLOR]);
        assert_eq!(colors[3], [0.8, 0.2, 0.2, 1.0], "the cloth");
        assert_eq!(colors[4], flag_number(color), "the number, light on a dark cloth");
        // From the hex's middle, up and to the right, about three times a
        // city's mark tall.
        let [x, y] = view.to_ndc(view.hex_center(hex));
        let px = |v: &FlatVertex| [(v.pos[0] - x) * 100.0, (v.pos[1] - y) * 115.5];
        let tops = one.iter().map(|v| px(v)[1]).fold(f32::MIN, f32::max);
        let bottom = one[8..12].iter().map(|v| px(v)[1]).fold(f32::MAX, f32::min);
        assert!(bottom.abs() < 1e-3, "the pole stands on the hex: {bottom}");
        let reach = view.city_mark_reach();
        assert!(tops > reach * 3.0 && tops < reach * 5.0, "{tops} against {reach}");
        assert!(one[12..].iter().all(|v| px(v)[0] > 0.0), "the cloth and its number to the right");
        // None, nothing; a small minimap, still finite.
        assert!(flag_marks(&view, &[], color).is_empty());
        assert!(flag_marks(&MiniMapView::new(20, 20), &[(hex, 3)], color).iter().all(|v| v.pos[0].is_finite()));
    }

    /// The owner's word, 2026-11-22: "Marking the battle locations on the
    /// minimap is a good idea".
    #[test]
    fn a_battle_is_a_cross_in_the_battles_color_on_a_dark_one() {
        let mut view = MiniMapView::new(30, 40);
        view.resize(200, 231);
        let hex = Hex::new(12, 20);
        let marks = battle_marks(&view, &[(hex, 0.0)]);
        assert_eq!(marks.len(), 16, "two bars, each dark and then in color");
        let [r, g, b] = BATTLE_BORDER;
        let colors: Vec<[f32; 4]> = marks.chunks(4).map(|q| q[0].color).collect();
        assert_eq!(colors, [RING_COLOR, RING_COLOR, [r, g, b, 1.0], [r, g, b, 1.0]]);
        // Each bar goes through the hex's middle, from corner to corner,
        // as far across as up; the two cross.
        let [x, y] = view.to_ndc(view.hex_center(hex));
        let px = |v: &FlatVertex| [(v.pos[0] - x) * 100.0, (v.pos[1] - y) * 115.5];
        for bar in marks.chunks(4) {
            let mean: [f32; 2] = std::array::from_fn(|k| bar.iter().map(|v| px(v)[k]).sum::<f32>() / 4.0);
            assert!(mean[0].abs() < 1e-3 && mean[1].abs() < 1e-3, "{mean:?}");
        }
        let end = |bar: &[FlatVertex]| {
            let (a, b) = (px(&bar[2]), px(&bar[3]));
            [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]
        };
        let (up, down) = (end(&marks[8..12]), end(&marks[12..16]));
        assert!((up[0] - up[1]).abs() < 1e-3 && (down[0] + down[1]).abs() < 1e-3, "{up:?} {down:?}");
        // Bigger than a city's mark, and the dark one shows round the color.
        assert!(up[0] > view.city_mark_reach() * 1.4, "{up:?}");
        assert!(end(&marks[0..4])[0] > up[0]);
        // The battle being shown is bigger.
        let lit = battle_marks(&view, &[(hex, 1.0)]);
        assert!(end(&lit[8..12])[0] > up[0] * 1.3);
        // Every mark is of finite numbers, with no size too.
        assert!(battle_marks(&MiniMapView::new(20, 20), &[(hex, 0.5)]).iter().all(|v| v.pos[0].is_finite()));
    }

    #[test]
    fn a_city_is_a_square_in_its_holders_color_on_a_dark_one() {
        let mut map = Map::new(30, 40, TileType::Grass).unwrap();
        let red = map.add_city("Alder", Hex::new(5, 5), 1).unwrap();
        map.city_mut(red).unwrap().side = crate::state::Side::Red;
        map.add_city("Birch", Hex::new(20, 30), 0).unwrap();
        let mut view = MiniMapView::new(30, 40);
        view.resize(200, 231);
        let marks = city_marks(&map, &view, &Sight::all());
        assert_eq!(marks.len(), 16, "two quads for each of the two cities");
        let colors: Vec<[f32; 4]> = marks.chunks(4).map(|q| q[0].color).collect();
        let [r, g, b] = side_color(crate::state::Side::Red);
        let [nr, ng, nb] = side_color(crate::state::Side::Grey);
        assert_eq!(colors, [RING_COLOR, [r, g, b, 1.0], RING_COLOR, [nr, ng, nb, 1.0]]);
        // The color is inside the dark square, around the city's hex.
        let (outer, inner) = (&marks[0..4], &marks[4..8]);
        assert!(outer[0].pos[0] < inner[0].pos[0] && outer[2].pos[0] > inner[2].pos[0]);
        let [x, y] = view.to_ndc(view.hex_center(Hex::new(5, 5)));
        assert!(inner[0].pos[0] < x && x < inner[2].pos[0] && inner[0].pos[1] < y && y < inner[2].pos[1]);
    }
}
