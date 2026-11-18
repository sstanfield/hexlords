//! Terrain types and what it costs a unit to enter them.
//!
//! C++: src/state/map.h TileType, src/state/unit.h MoveCost,
//! src/state/mapmove.cpp tileEntryCost, src/state/map.cpp (the map file letters)

/// What a tile is made of.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum TileType {
    Grass,
    Forest,
    Swamp,
    Desert,
    Hill,
    Mountain,
    Water,
    Temple,
    Ruin,
    City,
}

impl TileType {
    /// Every terrain type, in declaration order.
    pub const ALL: [TileType; 10] = [
        TileType::Grass,
        TileType::Forest,
        TileType::Swamp,
        TileType::Desert,
        TileType::Hill,
        TileType::Mountain,
        TileType::Water,
        TileType::Temple,
        TileType::Ruin,
        TileType::City,
    ];

    /// The terrain for a map file letter. C++: Map::Map(std::string file)
    pub fn from_char(c: char) -> Option<TileType> {
        Some(match c {
            'g' => TileType::Grass,
            'h' => TileType::Hill,
            'f' => TileType::Forest,
            'm' => TileType::Mountain,
            'w' => TileType::Water,
            's' => TileType::Swamp,
            'd' => TileType::Desert,
            't' => TileType::Temple,
            'r' => TileType::Ruin,
            'c' => TileType::City,
            _ => return None,
        })
    }

    /// The map file letter of this terrain. C++: Map::write
    pub fn to_char(self) -> char {
        match self {
            TileType::Grass => 'g',
            TileType::Hill => 'h',
            TileType::Forest => 'f',
            TileType::Mountain => 'm',
            TileType::Water => 'w',
            TileType::Swamp => 's',
            TileType::Desert => 'd',
            TileType::Temple => 't',
            TileType::Ruin => 'r',
            TileType::City => 'c',
        }
    }
}

/// The movement points a kind of unit pays to enter each terrain. **0 means it
/// cannot enter.** Units share these tables (see `Units::acquire_move_cost`).
///
/// `shore`, `bridge` and `road` have no tiles yet; the fields are kept for the
/// phases that add them.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Hash)]
pub struct MoveCost {
    pub grass: u16,
    pub forest: u16,
    pub swamp: u16,
    pub desert: u16,
    pub hill: u16,
    pub mountain: u16,
    pub water: u16,
    pub shore: u16,
    pub bridge: u16,
    pub road: u16,
    pub city: u16,
}

impl MoveCost {
    /// The cost to enter a tile of terrain `t`, or `None` if the unit cannot.
    /// Temples and ruins cost what grass costs. C++: tileEntryCost
    pub fn entry(&self, t: TileType) -> Option<u16> {
        let cost = match t {
            TileType::Grass | TileType::Temple | TileType::Ruin => self.grass,
            TileType::Forest => self.forest,
            TileType::Swamp => self.swamp,
            TileType::Desert => self.desert,
            TileType::Hill => self.hill,
            TileType::Mountain => self.mountain,
            TileType::Water => self.water,
            TileType::City => self.city,
        };
        (cost > 0).then_some(cost)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn char_codes_round_trip() {
        for t in TileType::ALL {
            assert_eq!(TileType::from_char(t.to_char()), Some(t), "{t:?}");
        }
        let letters: String = TileType::ALL.iter().map(|t| t.to_char()).collect();
        assert_eq!(letters, "gfsdhmwtrc");
        assert_eq!(TileType::from_char('x'), None);
        assert_eq!(TileType::from_char('G'), None);
    }

    #[test]
    fn entry_costs() {
        let mc = MoveCost {
            grass: 2,
            forest: 4,
            swamp: 5,
            desert: 3,
            hill: 4,
            bridge: 1,
            road: 2,
            city: 1,
            ..MoveCost::default()
        };
        assert_eq!(mc.entry(TileType::Grass), Some(2));
        assert_eq!(mc.entry(TileType::Forest), Some(4));
        assert_eq!(mc.entry(TileType::Swamp), Some(5));
        assert_eq!(mc.entry(TileType::Desert), Some(3));
        assert_eq!(mc.entry(TileType::Hill), Some(4));
        assert_eq!(mc.entry(TileType::City), Some(1));
        // Temples and ruins cost what grass costs.
        assert_eq!(mc.entry(TileType::Temple), Some(2));
        assert_eq!(mc.entry(TileType::Ruin), Some(2));
        // 0 is impassable.
        assert_eq!(mc.entry(TileType::Mountain), None);
        assert_eq!(mc.entry(TileType::Water), None);
    }
}
