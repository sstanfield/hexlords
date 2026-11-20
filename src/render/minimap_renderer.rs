//! The minimap: the whole map as flat-colored hexes on black, its cities as
//! squares in their holders' colors, drawn once into a texture and blitted
//! from it every frame, with a marker on top that shows the part of the map
//! the main view is looking at.
//!
//! The C++ drew the minimap into the window and read it back with
//! `glReadPixels` to cache it. Here it is drawn into a render target, which has
//! to happen before the frame's own render pass begins (plan, section 3.3).
//!
//! C++: src/render/minimaprender.cpp MiniMapRenderer (renderMap, CTX::drawHex,
//! CTX::saveMiniMap, CTX::miniMapLocMarker, getTileColor)

use miniquad::{PassAction, RenderPass, RenderingBackend, TextureId};

use super::hex_mesh::{HEX_INDICES, HEX_VERTICES, HexVariant};
use super::map_renderer::{RING_COLOR, side_color};
use super::mesh::{BufferPool, MAX_VERTICES, Mesh};
use super::shaders::{BlitVertex, FlatVertex, Pipelines};
use super::textures::texture_params;
use crate::minimap_view::MiniMapView;
use crate::state::{Hex, Map, Side, TileType};

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
/// map so that every city can be found. Quads of four vertices.
pub fn city_marks(map: &Map, view: &MiniMapView, seen: &dyn Fn(Hex) -> bool) -> Vec<FlatVertex> {
    let edge = (view.city_mark_reach() / 4.0).max(1.0);
    let mut quads = Vec::with_capacity(map.cities().len() * 8);
    for city in map.cities().iter() {
        // In the fog, nobody's: who holds it is not seen.
        let [r, g, b] = side_color(if seen(city.pos) { city.side } else { Side::Grey });
        quads.extend(quad_corners(view.city_mark(city.pos, edge)).map(|pos| FlatVertex { pos, color: RING_COLOR }));
        quads.extend(quad_corners(view.city_mark(city.pos, 0.0)).map(|pos| FlatVertex { pos, color: [r, g, b, 1.0] }));
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
        (map, seen): (&Map, &dyn Fn(Hex) -> bool),
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
        for row in 0..map.rows() {
            for col in 0..map.cols() {
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
        for quad in city_marks(map, view, seen).chunks(4) {
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
    pub fn draw(&mut self, ctx: &mut dyn RenderingBackend, pipelines: &Pipelines, marker: [f32; 4]) {
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

    #[test]
    fn a_city_is_a_square_in_its_holders_color_on_a_dark_one() {
        let mut map = Map::new(30, 40, TileType::Grass).unwrap();
        let red = map.add_city("Alder", Hex::new(5, 5), 1).unwrap();
        map.city_mut(red).unwrap().side = crate::state::Side::Red;
        map.add_city("Birch", Hex::new(20, 30), 0).unwrap();
        let mut view = MiniMapView::new(30, 40);
        view.resize(200, 231);
        let marks = city_marks(&map, &view, &|_| true);
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
