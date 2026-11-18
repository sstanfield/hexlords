//! The minimap: the whole map as flat-colored hexes on black, drawn once into a
//! texture and blitted from it every frame, with a marker on top that shows the
//! part of the map the main view is looking at.
//!
//! The C++ drew the minimap into the window and read it back with
//! `glReadPixels` to cache it. Here it is drawn into a render target, which has
//! to happen before the frame's own render pass begins (plan, section 3.3).
//!
//! C++: src/render/minimaprender.cpp MiniMapRenderer (renderMap, CTX::drawHex,
//! CTX::saveMiniMap, CTX::miniMapLocMarker, getTileColor)

use miniquad::{PassAction, RenderPass, RenderingBackend, TextureId};

use super::hex_mesh::{HEX_INDICES, HEX_VERTICES, HexVariant};
use super::mesh::{BufferPool, MAX_VERTICES, Mesh};
use super::shaders::{BlitVertex, FlatVertex, Pipelines};
use super::textures::texture_params;
use crate::minimap_view::MiniMapView;
use crate::state::{Hex, Map, TileType};

/// Behind the hexes. C++: MiniMapRenderer::renderMap ("Map backing color")
pub const BACKING: (f32, f32, f32, f32) = (0.0, 0.0, 0.0, 1.0);
/// The marker over the main view's part of the map. C++: miniMapLocMarker
pub const MARKER: [f32; 4] = [0.0, 0.0, 0.0, 0.3];

/// A terrain's color on the minimap. Deserts and cities, which the C++ left
/// black ("XXX Fixme"), have colors of their own. C++: getTileColor
pub fn terrain_color(kind: TileType) -> [f32; 3] {
    match kind {
        TileType::Grass => [0.0, 0.76, 0.0],
        TileType::Forest => [0.22, 0.416, 0.122],
        TileType::Swamp => [0.0, 0.76, 0.522],
        TileType::Desert => [0.87, 0.78, 0.5],
        TileType::Hill => [0.863, 0.667, 0.208],
        TileType::Mountain => [0.475, 0.467, 0.443],
        TileType::Water => [0.09, 0.886, 0.902],
        TileType::Temple => [0.5, 0.5, 0.5],
        TileType::Ruin => [0.6, 0.6, 0.6],
        TileType::City => [0.75, 0.1, 0.1],
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
        map: &Map,
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
    use crate::game::Game;

    fn mini() -> MiniMapView {
        let mut v = MiniMapView::new(100, 100);
        v.resize(200, 231);
        v
    }

    #[test]
    fn terrain_colors() {
        assert_eq!(terrain_color(TileType::Grass), [0.0, 0.76, 0.0]);
        assert_eq!(terrain_color(TileType::Forest), [0.22, 0.416, 0.122]);
        assert_eq!(terrain_color(TileType::Swamp), [0.0, 0.76, 0.522]);
        assert_eq!(terrain_color(TileType::Hill), [0.863, 0.667, 0.208]);
        assert_eq!(terrain_color(TileType::Mountain), [0.475, 0.467, 0.443]);
        assert_eq!(terrain_color(TileType::Water), [0.09, 0.886, 0.902]);
        assert_eq!(terrain_color(TileType::Temple), [0.5, 0.5, 0.5]);
        assert_eq!(terrain_color(TileType::Ruin), [0.6, 0.6, 0.6]);
        // Every terrain has a color of its own (the C++ left desert and city
        // black), so that a map can be read off the minimap.
        for (i, a) in TileType::ALL.into_iter().enumerate() {
            for b in &TileType::ALL[i + 1..] {
                assert_ne!(terrain_color(a), terrain_color(*b), "{a:?} and {b:?}");
            }
        }
    }

    #[test]
    fn a_minimap_hex_has_its_terrains_color_and_its_place() {
        let game = Game::demo();
        let view = mini();
        let grass = minimap_hex(&game.map, &view, Hex::new(10, 11));
        assert!(grass.iter().all(|v| v.color == [0.0, 0.76, 0.0, 1.0]));
        let mountain = minimap_hex(&game.map, &view, Hex::new(10, 10));
        assert!(mountain.iter().all(|v| v.color == [0.475, 0.467, 0.443, 1.0]));
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
        let game = Game::demo();
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
}
