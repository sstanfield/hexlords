//! Triangle meshes built on the CPU each frame, and the GPU buffers they are
//! uploaded to.
//!
//! The C++ kept one small static buffer per hex model and issued a draw call (or
//! three) for every hex. Here a whole layer of the map is one mesh and one draw
//! call (plan, section 3.4).

use miniquad::{BufferId, BufferSource, BufferType, BufferUsage, RenderingBackend};

/// The most vertices one mesh can hold: indices are 16 bits wide.
pub const MAX_VERTICES: usize = 1 << 16;

/// Triangles for one draw call.
#[derive(Clone, Debug)]
pub struct Mesh<V> {
    pub vertices: Vec<V>,
    /// Three per triangle, into `vertices`.
    pub indices: Vec<u16>,
}

impl<V> Default for Mesh<V> {
    fn default() -> Self {
        Mesh { vertices: Vec::new(), indices: Vec::new() }
    }
}

/// The meshes of one layer: usually one, more when a layer has more vertices than
/// 16-bit indices can address. Cleared and refilled every frame; it keeps its
/// allocations.
#[derive(Clone, Debug)]
pub struct MeshList<V> {
    meshes: Vec<Mesh<V>>,
    /// The meshes in use since the last `clear` (the rest are spare allocations).
    used: usize,
}

impl<V> Default for MeshList<V> {
    fn default() -> Self {
        MeshList { meshes: Vec::new(), used: 0 }
    }
}

impl<V: Copy> MeshList<V> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        for m in &mut self.meshes[..self.used] {
            m.vertices.clear();
            m.indices.clear();
        }
        self.used = 0;
    }

    /// Add a shape: its vertices, and indices that count from its first vertex.
    /// A shape is never split between two meshes.
    pub fn push(&mut self, vertices: &[V], indices: &[u16]) {
        debug_assert!(vertices.len() <= MAX_VERTICES);
        debug_assert!(indices.iter().all(|&i| usize::from(i) < vertices.len()));
        let fits = self.used > 0 && self.meshes[self.used - 1].vertices.len() + vertices.len() <= MAX_VERTICES;
        if !fits {
            if self.used == self.meshes.len() {
                self.meshes.push(Mesh::default());
            }
            self.used += 1;
        }
        let mesh = &mut self.meshes[self.used - 1];
        let base = mesh.vertices.len() as u16;
        mesh.vertices.extend_from_slice(vertices);
        mesh.indices.extend(indices.iter().map(|i| base + i));
    }

    /// The meshes to draw, in the order they were filled.
    pub fn meshes(&self) -> &[Mesh<V>] {
        &self.meshes[..self.used]
    }

    pub fn is_empty(&self) -> bool {
        self.used == 0
    }

    pub fn vertex_count(&self) -> usize {
        self.meshes().iter().map(|m| m.vertices.len()).sum()
    }

    pub fn index_count(&self) -> usize {
        self.meshes().iter().map(|m| m.indices.len()).sum()
    }
}

/// GPU buffers for the frame's meshes: a vertex and an index buffer per mesh,
/// handed out in order and reused the next frame. A buffer that is too small is
/// replaced by a bigger one.
#[derive(Default)]
pub struct BufferPool {
    /// Buffer and its size in bytes.
    vertex: Vec<(BufferId, usize)>,
    /// Buffer and the number of indices it holds.
    index: Vec<(BufferId, usize)>,
    next: usize,
}

impl BufferPool {
    pub fn new() -> BufferPool {
        BufferPool::default()
    }

    /// Start handing out from the first buffers again (a new frame).
    pub fn rewind(&mut self) {
        self.next = 0;
    }

    /// Upload a mesh into the next pair of buffers: (vertex buffer, index buffer).
    pub fn upload<V>(&mut self, ctx: &mut dyn RenderingBackend, mesh: &Mesh<V>) -> (BufferId, BufferId) {
        let slot = self.next;
        self.next += 1;
        let bytes = std::mem::size_of_val(mesh.vertices.as_slice());
        let vb = Self::fit(ctx, &mut self.vertex, slot, bytes, BufferType::VertexBuffer);
        let ib = Self::fit(ctx, &mut self.index, slot, mesh.indices.len(), BufferType::IndexBuffer);
        ctx.buffer_update(vb, BufferSource::slice(&mesh.vertices));
        ctx.buffer_update(ib, BufferSource::slice(&mesh.indices));
        (vb, ib)
    }

    /// The buffer in `slot`, holding at least `size` (bytes for vertices, indices
    /// for indices).
    fn fit(
        ctx: &mut dyn RenderingBackend,
        buffers: &mut Vec<(BufferId, usize)>,
        slot: usize,
        size: usize,
        kind: BufferType,
    ) -> BufferId {
        if let Some(&(id, capacity)) = buffers.get(slot) {
            if capacity >= size {
                return id;
            }
            ctx.delete_buffer(id);
        }
        let capacity = size.next_power_of_two().max(1024);
        let source = match kind {
            BufferType::VertexBuffer => BufferSource::empty::<u8>(capacity),
            BufferType::IndexBuffer => BufferSource::empty::<u16>(capacity),
        };
        let id = ctx.new_buffer(kind, BufferUsage::Stream, source);
        if slot < buffers.len() {
            buffers[slot] = (id, capacity);
        } else {
            buffers.push((id, capacity));
        }
        id
    }

    pub fn delete_all(&mut self, ctx: &mut dyn RenderingBackend) {
        for (id, _) in self.vertex.drain(..).chain(self.index.drain(..)) {
            ctx.delete_buffer(id);
        }
        self.next = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRI: [u16; 3] = [0, 1, 2];

    #[test]
    fn shapes_share_a_mesh_and_their_indices_are_rebased() {
        let mut list: MeshList<u8> = MeshList::new();
        assert!(list.is_empty());
        list.push(&[10, 11, 12], &TRI);
        list.push(&[20, 21, 22, 23], &[0, 1, 2, 0, 2, 3]);
        assert_eq!(list.meshes().len(), 1);
        let mesh = &list.meshes()[0];
        assert_eq!(mesh.vertices, [10, 11, 12, 20, 21, 22, 23]);
        assert_eq!(mesh.indices, [0, 1, 2, 3, 4, 5, 3, 5, 6]);
        assert_eq!((list.vertex_count(), list.index_count()), (7, 9));
    }

    #[test]
    fn a_full_mesh_starts_another() {
        let mut list: MeshList<u8> = MeshList::new();
        let shape = [0u8; 7];
        let hexes = MAX_VERTICES / 7 + 5; // five more than one mesh holds
        for _ in 0..hexes {
            list.push(&shape, &TRI);
        }
        let meshes = list.meshes();
        assert_eq!(meshes.len(), 2);
        assert_eq!(meshes[0].vertices.len(), MAX_VERTICES / 7 * 7);
        assert!(meshes[0].vertices.len() <= MAX_VERTICES);
        assert_eq!(meshes[1].vertices.len(), 5 * 7);
        // The last index of the first mesh is in range, and the second mesh
        // counts from zero again.
        assert_eq!(usize::from(*meshes[0].indices.last().unwrap()), meshes[0].vertices.len() - 7 + 2);
        assert_eq!(meshes[1].indices[..3], TRI);
        assert_eq!(list.vertex_count(), hexes * 7);
    }

    #[test]
    fn exactly_full_is_one_mesh() {
        let mut list: MeshList<u8> = MeshList::new();
        let shape = [0u8; 4];
        for _ in 0..MAX_VERTICES / 4 {
            list.push(&shape, &TRI);
        }
        assert_eq!(list.meshes().len(), 1);
        assert_eq!(list.meshes()[0].vertices.len(), MAX_VERTICES);
        assert_eq!(*list.meshes()[0].indices.iter().max().unwrap(), u16::MAX - 1);
        list.push(&shape, &TRI);
        assert_eq!(list.meshes().len(), 2);
    }

    #[test]
    fn clear_empties_and_keeps_the_allocations() {
        let mut list: MeshList<u8> = MeshList::new();
        list.push(&[1, 2, 3], &TRI);
        list.clear();
        assert!(list.is_empty() && list.meshes().is_empty());
        assert_eq!(list.vertex_count(), 0);
        list.push(&[4, 5, 6], &TRI);
        assert_eq!(list.meshes()[0].vertices, [4, 5, 6]);
        assert_eq!(list.meshes()[0].indices, TRI);
    }
}
