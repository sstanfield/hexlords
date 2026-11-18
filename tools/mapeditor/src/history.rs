//! What the editor did to the terrain, so that it can be undone. A step is
//! everything one gesture changed: a drag of the brush, a fill, a key.

use hexgame::state::{Hex, Map, TileType};

/// How many steps are remembered. Older ones are forgotten.
pub const MAX_STEPS: usize = 200;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct Change {
    hex: Hex,
    before: TileType,
    after: TileType,
}

#[derive(Default)]
pub struct History {
    /// The steps that can be undone, oldest first.
    done: Vec<Vec<Change>>,
    /// The steps that were undone and can be done again, oldest undo first.
    undone: Vec<Vec<Change>>,
    /// The step in the making.
    open: Vec<Change>,
}

impl History {
    pub fn new() -> History {
        History::default()
    }

    /// Change a tile's terrain as part of the step in the making. False if the
    /// tile has that terrain already, or the hex is off the map.
    pub fn set(&mut self, map: &mut Map, hex: Hex, kind: TileType) -> bool {
        let Some(before) = map.tile(hex).map(|t| t.kind) else { return false };
        if before == kind {
            return false;
        }
        if let Some(tile) = map.tile_mut(hex) {
            tile.kind = kind;
        }
        self.open.push(Change { hex, before, after: kind });
        true
    }

    /// The gesture is over: what it changed becomes one step. Returns how many
    /// tiles that is; a gesture that changed nothing leaves no step.
    pub fn commit(&mut self) -> usize {
        let step = std::mem::take(&mut self.open);
        let tiles = step.len();
        if tiles > 0 {
            self.done.push(step);
            if self.done.len() > MAX_STEPS {
                self.done.remove(0);
            }
            // A new step leaves nothing to do again.
            self.undone.clear();
        }
        tiles
    }

    /// Take back the last step (the one in the making first, if there is one).
    /// Returns how many tiles changed, or `None` if there is nothing to undo.
    pub fn undo(&mut self, map: &mut Map) -> Option<usize> {
        self.commit();
        let step = self.done.pop()?;
        // Backwards: a gesture may have changed a tile more than once.
        for change in step.iter().rev() {
            if let Some(tile) = map.tile_mut(change.hex) {
                tile.kind = change.before;
            }
        }
        let tiles = step.len();
        self.undone.push(step);
        Some(tiles)
    }

    /// Do the step that was last undone again.
    pub fn redo(&mut self, map: &mut Map) -> Option<usize> {
        if !self.open.is_empty() {
            return None; // a step in the making is a new step
        }
        let step = self.undone.pop()?;
        for change in &step {
            if let Some(tile) = map.tile_mut(change.hex) {
                tile.kind = change.after;
            }
        }
        let tiles = step.len();
        self.done.push(step);
        Some(tiles)
    }

    /// Forget everything (another map was opened).
    pub fn clear(&mut self) {
        *self = History::default();
    }

    pub fn can_undo(&self) -> bool {
        !self.done.is_empty() || !self.open.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.undone.is_empty() && self.open.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> Map {
        Map::new(20, 20, TileType::Grass).unwrap()
    }

    fn kind(map: &Map, row: u16, col: u16) -> TileType {
        map.tile(Hex::new(row, col)).unwrap().kind
    }

    #[test]
    fn a_step_is_undone_and_redone_as_a_whole() {
        let (mut m, mut h) = (map(), History::new());
        assert!(h.set(&mut m, Hex::new(1, 1), TileType::Water));
        assert!(h.set(&mut m, Hex::new(1, 2), TileType::Water));
        assert!(!h.set(&mut m, Hex::new(1, 2), TileType::Water), "no change, nothing recorded");
        assert!(!h.set(&mut m, Hex::new(20, 0), TileType::Water), "off the map");
        assert_eq!(h.commit(), 2);
        assert!(h.set(&mut m, Hex::new(5, 5), TileType::Hill));
        assert_eq!(h.commit(), 1);

        assert_eq!(h.undo(&mut m), Some(1));
        assert_eq!(kind(&m, 5, 5), TileType::Grass);
        assert_eq!(kind(&m, 1, 1), TileType::Water);
        assert_eq!(h.undo(&mut m), Some(2));
        assert_eq!((kind(&m, 1, 1), kind(&m, 1, 2)), (TileType::Grass, TileType::Grass));
        assert_eq!(h.undo(&mut m), None);
        assert!(!h.can_undo() && h.can_redo());

        assert_eq!(h.redo(&mut m), Some(2));
        assert_eq!(h.redo(&mut m), Some(1));
        assert_eq!(h.redo(&mut m), None);
        assert_eq!((kind(&m, 1, 1), kind(&m, 5, 5)), (TileType::Water, TileType::Hill));
    }

    #[test]
    fn a_tile_changed_twice_in_a_step_goes_back_to_what_it_was() {
        let (mut m, mut h) = (map(), History::new());
        h.set(&mut m, Hex::new(3, 3), TileType::Forest);
        h.set(&mut m, Hex::new(3, 3), TileType::Swamp);
        h.commit();
        h.undo(&mut m);
        assert_eq!(kind(&m, 3, 3), TileType::Grass);
        h.redo(&mut m);
        assert_eq!(kind(&m, 3, 3), TileType::Swamp);
    }

    #[test]
    fn a_new_step_forgets_what_was_undone() {
        let (mut m, mut h) = (map(), History::new());
        h.set(&mut m, Hex::new(1, 1), TileType::Water);
        h.commit();
        h.undo(&mut m);
        h.set(&mut m, Hex::new(2, 2), TileType::Hill);
        assert_eq!(h.redo(&mut m), None, "not while a step is in the making");
        h.commit();
        assert_eq!(h.redo(&mut m), None);
        assert_eq!(kind(&m, 1, 1), TileType::Grass);
    }

    #[test]
    fn undo_takes_the_step_in_the_making_too() {
        let (mut m, mut h) = (map(), History::new());
        h.set(&mut m, Hex::new(1, 1), TileType::Water);
        assert!(h.can_undo());
        assert_eq!(h.undo(&mut m), Some(1));
        assert_eq!(kind(&m, 1, 1), TileType::Grass);
    }

    #[test]
    fn an_empty_gesture_leaves_no_step_and_old_steps_are_forgotten() {
        let (mut m, mut h) = (map(), History::new());
        assert_eq!(h.commit(), 0);
        assert!(!h.can_undo());
        for i in 0..(MAX_STEPS + 5) {
            let kind = if i % 2 == 0 { TileType::Water } else { TileType::Grass };
            h.set(&mut m, Hex::new(0, 0), kind);
            h.commit();
        }
        let mut undone = 0;
        while h.undo(&mut m).is_some() {
            undone += 1;
        }
        assert_eq!(undone, MAX_STEPS);
        h.clear();
        assert!(!h.can_undo() && !h.can_redo());
    }
}
