//! What a side's cities will build over its next pay days, and what its
//! gold will do: the production window's forecast (`Game::forecast`).
//!
//! It is the rules of a side's pay day (`Game::pay_day`) played forward on
//! copies of its cities, with the same pieces (`City::work`, `map::fits`,
//! `City::built_one`): taxes in, upkeep out, then each city in turn, its
//! unit paid for if the gold is there and placed if there is room. It
//! changes nothing. It takes the world to stay as it is: the side holds its
//! cities, nobody fights, no new orders are given and no walls raised, the
//! units a city sends somewhere are gone from it by its next pay day, and
//! nothing arrives in a city from elsewhere. It stops at the first pay day
//! whose upkeep the gold cannot pay: who leaves then is the rules' to say,
//! and what comes after depends on it.
//!
//! Not a port: the C++ had no gold.

use super::Game;
use crate::state::map::fits;
use crate::state::{CityId, Sends, Side};

/// A unit a city finishes on a pay day, paid for and placed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Built {
    pub city: CityId,
    /// Its kind's name, for people ("Light Infantry").
    pub kind: String,
    pub price: u32,
    /// What it costs a turn, from the pay day after.
    pub upkeep: u32,
    /// Where its city sends it.
    pub sends: Sends,
}

/// Why a city's unit, done, is not out on a pay day.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Wait {
    /// The gold left after the upkeep and the cities before it does not
    /// pay its price.
    Gold,
    /// Its city is full (a hex holds eight units, and a commander besides).
    Room,
}

/// One pay day of the side's: the start of its turn in turns, the end of a
/// round in rounds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PayDay {
    /// The round it falls in.
    pub round: u32,
    pub gold_before: u32,
    pub taxes: u32,
    /// What its units cost that day (those built on earlier pay days too).
    pub upkeep: u32,
    pub gold_after: u32,
    /// What its cities finish, in the order the rules go through them.
    pub built: Vec<Built>,
    /// The cities whose unit is done and waits.
    pub waiting: Vec<(CityId, Wait)>,
    /// The gold does not pay the upkeep: units leave (and the forecast
    /// stops here).
    pub short: bool,
}

/// What a side's next pay days look like (`Game::forecast`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Forecast {
    pub side: Side,
    pub days: Vec<PayDay>,
    /// The round of the first pay day whose upkeep the gold will not pay.
    pub short_at: Option<u32>,
}

impl Forecast {
    /// The upkeep the side pays on its last pay day looked at, and what it
    /// will pay after it with what those days built.
    pub fn upkeep_after(&self) -> u32 {
        self.days.last().map_or(0, |d| d.upkeep + d.built.iter().map(|b| b.upkeep).sum::<u32>())
    }
}

impl Game {
    /// The round of `side`'s next pay day: in rounds, when this round has
    /// played out; in turns, this round if its turn has yet to begin in it
    /// (none in the first), else the next.
    fn next_pay_day(&self, side: Side) -> u32 {
        if self.rounds() {
            return self.round + 1;
        }
        match self.order.iter().position(|s| *s == side) {
            Some(i) if i > self.current && self.round > 1 => self.round,
            _ => self.round + 1,
        }
    }

    /// `side`'s next `pay_days` pay days, as the rules would have them if
    /// nothing else changed (the module's doc says what it takes to stay as
    /// it is). Read only: the game is not touched.
    pub fn forecast(&self, side: Side, pay_days: usize) -> Forecast {
        let map = &self.map;
        let units = map.units();
        let mut cities: Vec<_> =
            map.cities().iter().filter(|c| c.side == side && side != Side::Grey).cloned().collect();
        cities.sort_by_key(|c| c.id.0);
        // Who stays in each city (others, commanders): those with nowhere
        // to go and no duty elsewhere.
        let mut room: Vec<(usize, usize)> = cities
            .iter()
            .map(|c| {
                let staying = map
                    .units_at(c.pos)
                    .iter()
                    .filter_map(|id| units.get(*id))
                    .filter(|u| u.route.is_empty() && !u.on_duty());
                staying.fold((0, 0), |(o, k), u| if map.is_commander(u.id()) { (o, k + 1) } else { (o + 1, k) })
            })
            .collect();
        let (mut gold, taxes, mut upkeep) = (self.gold(side), self.income(side), self.upkeep(side));
        let mut forecast = Forecast { side, days: Vec::new(), short_at: None };
        let first = self.next_pay_day(side);
        for round in (first..).take(pay_days) {
            let gold_before = gold;
            gold = gold.saturating_add(taxes);
            if upkeep > gold {
                let day = PayDay {
                    round,
                    gold_before,
                    taxes,
                    upkeep,
                    gold_after: 0,
                    built: Vec::new(),
                    waiting: Vec::new(),
                    short: true,
                };
                forecast.days.push(day);
                forecast.short_at = Some(round);
                break;
            }
            gold -= upkeep;
            let (mut built, mut waiting, mut added) = (Vec::new(), Vec::new(), 0);
            for (city, here) in cities.iter_mut().zip(room.iter_mut()) {
                let Some(prod) = city.work() else { continue };
                if gold < prod.cost {
                    waiting.push((city.id, Wait::Gold));
                    continue;
                }
                let Some(kind) = units.kind_named(&prod.basename).and_then(|k| units.kind(k)) else { continue };
                let commander = kind.commander > 0;
                if !fits(here.0, here.1, commander) {
                    waiting.push((city.id, Wait::Room));
                    continue;
                }
                gold -= prod.cost;
                city.progress = 0;
                let sends = city.sends.clone();
                let leaves = match &sends {
                    Sends::Route(route) => !route.is_empty(),
                    Sends::Point { n, .. } => self.rally_point(side, *n).is_some(),
                    Sends::Stay | Sends::Defend => false,
                };
                if !leaves {
                    if commander {
                        here.1 += 1;
                    } else {
                        here.0 += 1;
                    }
                }
                added += prod.upkeep;
                built.push(Built {
                    city: city.id,
                    kind: kind.name.clone(),
                    price: prod.cost,
                    upkeep: prod.upkeep,
                    sends,
                });
                city.built_one();
            }
            forecast.days.push(PayDay {
                round,
                gold_before,
                taxes,
                upkeep,
                gold_after: gold,
                built,
                waiting,
                short: false,
            });
            upkeep += added;
        }
        forecast
    }
}
