//! Cities and what they can produce.
//!
//! C++: src/state/city.h, src/state/city.cpp (UnitProd, City, CityManager)

use super::hex::Hex;
use super::units::Side;

/// A kind of unit a city can build.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnitProd {
    pub basename: String,
    /// Turns to build.
    pub time: u32,
    pub cost: u32,
    pub upkeep: u32,
    pub strength: u32,
    pub movement: u32,
}

/// Names a city in [`Cities`].
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct CityId(pub u32);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct City {
    pub id: CityId,
    pub side: Side,
    pub name: String,
    /// What the city can build.
    pub units: Vec<UnitProd>,
    pub pos: Hex,
    pub walls: u32,
    /// The gold it brings its holder at the start of each of its turns (its
    /// taxes).
    pub income: u32,
    /// The level the units it builds start at: 1, or more where it trains
    /// them (a veteran, 2; elite, 3).
    pub trains: u8,
    /// Where the units it builds go, a turn at a time (their `goto`), if its
    /// holder said (`Order::Rally`).
    pub rally: Option<Hex>,
    /// What it is building: an index into `units`, or nothing. Set it with
    /// `Map::set_production`.
    pub building: Option<usize>,
    /// The turns it has worked on what it is building, toward that's `time`.
    pub progress: u32,
}

impl City {
    /// What it is building, if anything.
    pub fn production(&self) -> Option<&UnitProd> {
        self.building.and_then(|i| self.units.get(i))
    }
}

/// What a city brings in a turn unless its map says otherwise.
pub const DEFAULT_INCOME: u32 = 10;

/// The owner's rules (2026-11-20): a city's holder raises its walls a level
/// at a time (`Order::Walls`), once a turn, up to `WALLS_CAP`; a level costs
/// `WALL_PRICE` for each level it raises them to (to 3, 75 gold: nearly two
/// turns of a capital's taxes). A city that is taken loses a level.
pub const WALLS_CAP: u32 = 6;
pub const WALL_PRICE: u32 = 25;

/// What it costs to raise walls of `walls` a level.
pub fn wall_price(walls: u32) -> u32 {
    WALL_PRICE.saturating_mul(walls.saturating_add(1))
}

/// Every city on the map.
#[derive(Clone, Debug, Default)]
pub struct Cities {
    cities: Vec<City>,
}

impl Cities {
    pub fn new() -> Cities {
        Cities::default()
    }

    /// Add a neutral city. C++: CityManager::newCity
    pub fn new_city(&mut self, name: &str, pos: Hex, walls: u32) -> CityId {
        let id = CityId(self.cities.len() as u32);
        let city = City {
            id,
            side: Side::Grey,
            name: name.to_string(),
            units: Vec::new(),
            pos,
            walls,
            income: DEFAULT_INCOME,
            trains: 1,
            rally: None,
            building: None,
            progress: 0,
        };
        self.cities.push(city);
        id
    }

    /// Add a kind of unit to what a city can build. Returns false if there is no
    /// such city. C++: CityManager::addCityProd
    pub fn add_city_prod(&mut self, id: CityId, prod: UnitProd) -> bool {
        match self.get_mut(id) {
            Some(city) => {
                city.units.push(prod);
                true
            }
            None => false,
        }
    }

    /// C++: CityManager::numCities
    pub fn len(&self) -> usize {
        self.cities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cities.is_empty()
    }

    /// C++: CityManager::getCity
    pub fn get(&self, id: CityId) -> Option<&City> {
        self.cities.get(id.0 as usize)
    }

    pub fn get_mut(&mut self, id: CityId) -> Option<&mut City> {
        self.cities.get_mut(id.0 as usize)
    }

    /// C++: CityManager::firstCity / nextCity
    pub fn iter(&self) -> impl Iterator<Item = &City> {
        self.cities.iter()
    }

    /// The city on a hex, if there is one. C++: Map::getCity
    pub fn at(&self, pos: Hex) -> Option<&City> {
        self.cities.iter().find(|c| c.pos == pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cities_start_neutral_and_are_found_by_position() {
        let mut cities = Cities::new();
        assert!(cities.is_empty());
        let a = cities.new_city("Alder", Hex::new(3, 4), 2);
        let b = cities.new_city("Birch", Hex::new(9, 1), 0);
        assert_eq!((a, b), (CityId(0), CityId(1)));
        assert_eq!(cities.len(), 2);
        let city = cities.get(a).unwrap();
        assert_eq!((city.name.as_str(), city.side, city.walls), ("Alder", Side::Grey, 2));
        assert_eq!(cities.at(Hex::new(9, 1)).map(|c| c.id), Some(b));
        assert!(cities.at(Hex::new(0, 0)).is_none());
        assert_eq!(cities.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["Alder", "Birch"]);
    }

    #[test]
    fn production_is_added_to_the_named_city() {
        let mut cities = Cities::new();
        let a = cities.new_city("Alder", Hex::new(3, 4), 2);
        let prod = UnitProd { basename: "Militia".to_string(), time: 1, cost: 4, upkeep: 2, strength: 1, movement: 8 };
        assert!(cities.add_city_prod(a, prod.clone()));
        assert!(!cities.add_city_prod(CityId(5), prod.clone()));
        assert_eq!(cities.get(a).unwrap().units, [prod]);
    }
}
