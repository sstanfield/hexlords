//! Cities and what they can produce. The demo has no cities; the next phases do.
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
        self.cities.push(City { id, side: Side::Grey, name: name.to_string(), units: Vec::new(), pos, walls });
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
