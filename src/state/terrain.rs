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

    /// Its place in [`TileType::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }

    /// The terrain's name, in lower case. Its tile image is
    /// `tiles/<name>_tile.png`.
    pub fn name(self) -> &'static str {
        match self {
            TileType::Grass => "grass",
            TileType::Forest => "forest",
            TileType::Swamp => "swamp",
            TileType::Desert => "desert",
            TileType::Hill => "hill",
            TileType::Mountain => "mountain",
            TileType::Water => "water",
            TileType::Temple => "temple",
            TileType::Ruin => "ruin",
            TileType::City => "city",
        }
    }

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
/// `road` and `bridge` are what a step along a road costs, on land and over
/// water (see `movement::step_cost`). `shore` has no tiles yet.
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
    /// The words for what a unit pays to enter, in the order files and panels
    /// list them: the terrains, then a step along a road over land and over
    /// water. (`shore` has no tiles and no word yet.)
    pub const WORDS: [&'static str; 10] =
        ["grass", "forest", "swamp", "desert", "hill", "mountain", "water", "city", "road", "bridge"];

    /// The price one of [`MoveCost::WORDS`] names, to set it.
    pub fn price_mut(&mut self, word: &str) -> Option<&mut u16> {
        Some(match word {
            "grass" => &mut self.grass,
            "forest" => &mut self.forest,
            "swamp" => &mut self.swamp,
            "desert" => &mut self.desert,
            "hill" => &mut self.hill,
            "mountain" => &mut self.mountain,
            "water" => &mut self.water,
            "city" => &mut self.city,
            "road" => &mut self.road,
            "bridge" => &mut self.bridge,
            _ => return None,
        })
    }

    /// Every word with its price (0: cannot be entered), in the order of
    /// [`MoveCost::WORDS`].
    pub fn prices(&self) -> [(&'static str, u16); 10] {
        let mut copy = *self;
        MoveCost::WORDS.map(|word| (word, copy.price_mut(word).map_or(0, |price| *price)))
    }

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

    #[test]
    fn every_price_has_a_word() {
        let mut cost = MoveCost::default();
        for (i, word) in MoveCost::WORDS.into_iter().enumerate() {
            *cost.price_mut(word).unwrap() = i as u16 + 1;
        }
        assert!(cost.price_mut("lava").is_none() && cost.price_mut("shore").is_none());
        // Each word set its own field, and all of them but the shore's.
        let expected = MoveCost {
            grass: 1,
            forest: 2,
            swamp: 3,
            desert: 4,
            hill: 5,
            mountain: 6,
            water: 7,
            shore: 0,
            city: 8,
            road: 9,
            bridge: 10,
        };
        assert_eq!(cost, expected);
        let prices = cost.prices();
        assert_eq!(prices.map(|(word, _)| word), MoveCost::WORDS);
        assert_eq!(prices.map(|(_, price)| price), [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    }
}
