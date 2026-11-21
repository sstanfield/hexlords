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

/// Where a city sends the units it builds (`City::sends`, `Order::Rally`).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum Sends {
    /// They stay in it, and wait for orders.
    #[default]
    Stay,
    /// They stay in it and defend it (`Order::Defend`): nobody need see to
    /// them (a garrison).
    Defend,
    /// They go along a route, a turn at a time (`Order::GoTo`): the stops in
    /// order, the last where they end. Never empty.
    Route(Vec<Hex>),
    /// They go to one of the side's rally points (`Game::rally_points`), by
    /// way of a temple near the way first if `bless`.
    Point { n: u8, bless: bool },
}

/// The most items a city's queue holds.
pub const MAX_QUEUE: usize = 8;

/// An item of a city's queue (`City::queue`, `Order::Queue`): what it
/// builds (an index into `City::units`), how many (one at least), and where
/// they go.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueueItem {
    pub what: usize,
    pub count: u32,
    pub sends: Sends,
}

/// What a city's queue does once its last item is built.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum QueueEnd {
    /// It stops: the city builds nothing, and waits for orders.
    #[default]
    Stop,
    /// It goes back to the item `from` (0 is the first) and on from there,
    /// `times` times (`None`: for ever), and then stops.
    Repeat { from: usize, times: Option<u32> },
}

/// A city's production queue: what it builds, in turn. Empty: none (the
/// city builds what `City::building` says, one after another, for ever).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Queue {
    pub items: Vec<QueueItem>,
    pub end: QueueEnd,
    /// The item being built, and how many of it are out already.
    pub at: usize,
    pub made: u32,
    /// How many times it has gone back (`QueueEnd::Repeat`).
    pub rounds: u32,
}

impl Queue {
    /// True if there is no queue.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The item being built, if there is a queue.
    pub fn current(&self) -> Option<&QueueItem> {
        self.items.get(self.at)
    }
}

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
    /// Where the units it builds go (`Order::Rally`): they stay in it (at
    /// first), or defend it, or go somewhere. With a queue, the item's.
    pub sends: Sends,
    /// Its production queue (`Order::Queue`): while it has one, `building`
    /// and `sends` are its item's.
    pub queue: Queue,
    /// Its holder said it is to build nothing (it is no task that needs
    /// orders while it does not).
    pub resting: bool,
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

    /// A unit of what it builds is out: its queue, if it has one, goes on
    /// (the next of the item, the next item, back to where it repeats from,
    /// or, at its end, nothing, and the city waits for orders).
    pub fn built_one(&mut self) {
        let Some(count) = self.queue.current().map(|item| item.count) else { return };
        self.queue.made += 1;
        if self.queue.made < count {
            return;
        }
        self.queue.made = 0;
        self.queue.at += 1;
        if self.queue.at >= self.queue.items.len() {
            match self.queue.end {
                QueueEnd::Repeat { from, times } if times.is_none_or(|t| self.queue.rounds < t) => {
                    self.queue.rounds += 1;
                    self.queue.at = from.min(self.queue.items.len() - 1);
                }
                _ => {
                    self.queue = Queue::default();
                    (self.building, self.progress, self.resting) = (None, 0, false);
                    return;
                }
            }
        }
        self.follow_queue();
    }

    /// Its work and where its units go are its queue's item's.
    pub fn follow_queue(&mut self) {
        let Some(item) = self.queue.current().cloned() else { return };
        if self.building != Some(item.what) {
            (self.building, self.progress) = (Some(item.what), 0);
        }
        self.sends = item.sends;
        self.resting = false;
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
            sends: Sends::Stay,
            queue: Queue::default(),
            resting: false,
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
