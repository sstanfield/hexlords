//! The new game screen: the maps to choose from, how to play (`Mode`), the
//! order of the turns, and who plays each side of the chosen map (a person or
//! the computer); the chosen map is shown beside them, to look over, and the
//! game starts with the button (`Shell`, in `shell.rs`, then hands the window
//! to a `Session`). Or a random map (`mapgen`, the plan's G8): its preset,
//! size, sides, cities, temples, cities at sea and in the air, and seed, made
//! with a button and shown before the game starts. And the games that were
//! left before they were over (`save`): each can be played on, or deleted.
//!
//! Not a port: the C++ started its demo at once.

use std::path::{Path, PathBuf};

use turbo_ferret::widgets::{ScrollContainer, ScrollMode};
use turbo_ferret::{LayoutParams, Size, TbId, WidgetId};

use crate::files;
use crate::game::Game;
use crate::gui::RootCommand;
use crate::gui::build;
use crate::help::HelpSection;
use crate::mapgen::config::{Config, DENSITIES, PRESETS, SIZES, Size as MapSize, TEMPLE_CHOICES};
use crate::mapgen::reach::{MoveClass, movement_class};
use crate::platform;
use crate::save::{self, Summary};
use crate::screen::{MapScreen, Scene};
use crate::setup::{Controller, Fog, Mode, Setup, TurnOrder};
use crate::state::combat::{Exact, MAX_CHANCE, System, UNIT_CAP};
use crate::state::scenario::side_name;
use crate::state::{Hex, Map, Side, TileType};

/// The layout, relative to the asset directory.
pub const LAYOUT: &str = "layouts/new_game.tb.txt";
/// The button that starts the game.
pub const START_ID: &str = "start_game";
/// The buttons of the settings.
pub const MODE_TURNS_ID: &str = "mode_turns";
pub const MODE_ROUNDS_ID: &str = "mode_rounds";
pub const ORDER_FIXED_ID: &str = "order_fixed";
pub const ORDER_DRAWN_ID: &str = "order_drawn";
pub const BATTLES_LINES_ID: &str = "battles_lines";
pub const BATTLES_STRENGTH_ID: &str = "battles_strength";
pub const BATTLES_DICE_ID: &str = "battles_dice";
/// On the dice, what a roll of exactly a side's number is.
pub const EXACT_HITS_ID: &str = "exact_hits";
pub const EXACT_MISSES_ID: &str = "exact_misses";
pub const EXACT_DEFENDERS_ID: &str = "exact_defenders";
pub const CHANCE_LESS_ID: &str = "chance_less";
pub const CHANCE_MORE_ID: &str = "chance_more";
pub const WOUNDS_ID: &str = "wounds";
pub const BLOWS_BACK_ID: &str = "blows_back";
pub const CAP_ID: &str = "unit_cap";
pub const FOG_OFF_ID: &str = "fog_off";
pub const FOG_ON_ID: &str = "fog_on";
pub const FOG_TOTAL_ID: &str = "fog_total";
pub const GOLD_LESS_ID: &str = "gold_less";
pub const GOLD_MORE_ID: &str = "gold_more";
/// The random map: the entry in the list, and its settings' buttons.
pub const RANDOM_ID: &str = "map_random";
pub const PRESET_LESS_ID: &str = "random_preset_less";
pub const PRESET_MORE_ID: &str = "random_preset_more";
pub const SIZE_LESS_ID: &str = "random_size_less";
pub const SIZE_MORE_ID: &str = "random_size_more";
pub const SIDES_LESS_ID: &str = "random_sides_less";
pub const SIDES_MORE_ID: &str = "random_sides_more";
pub const CITIES_LESS_ID: &str = "random_cities_less";
pub const CITIES_MORE_ID: &str = "random_cities_more";
pub const TEMPLES_LESS_ID: &str = "random_temples_less";
pub const TEMPLES_MORE_ID: &str = "random_temples_more";
pub const SEA_ID: &str = "random_sea";
pub const AIR_ID: &str = "random_air";
pub const SEED_ID: &str = "random_seed";
pub const MAKE_ID: &str = "random_make";
/// The line that says how big the map shown is, and how many cities it has.
pub const MAP_FACTS_ID: &str = "map_facts";
/// The units template random maps get, relative to the asset directory.
pub const UNITS_TEMPLATE: &str = "mapgen/units.txt";
/// How much a press of Gold's - or + changes it, and the most it can be.
const GOLD_STEP: u32 = 10;
const MAX_GOLD: u32 = 1000;
/// The agent the computer plays with, unless told otherwise.
pub const COMPUTER: &str = "steady";
/// The computer's players a seat can have, in the order a click on its
/// Computer button goes through them (`agents`, `planner`).
pub const COMPUTERS: [&str; 4] = ["steady", "bold", "careful", "simple"];

/// The buttons of the `i`th game in progress: play on, delete.
pub fn continue_id(i: usize) -> String {
    format!("saved_continue_{i}")
}

pub fn delete_id(i: usize) -> String {
    format!("saved_delete_{i}")
}

/// How big a map is and how many cities it has, in a line: `88 wide, 77
/// high: 34 cities, 4 of them a side's`.
pub fn map_facts(map: &Map) -> String {
    let cities = map.cities().iter().count();
    let held = map.cities().iter().filter(|c| c.side != Side::Grey).count();
    let cities = match cities {
        0 => "no cities".to_string(),
        1 => "1 city".to_string(),
        n => format!("{n} cities"),
    };
    let held = if held > 0 { format!(", {held} of them a side's") } else { String::new() };
    format!("{} wide, {} high: {cities}{held}", map.cols(), map.rows())
}

/// The button of the `i`th map.
pub fn map_id(i: usize) -> String {
    format!("map_{i}")
}

/// The button that has a person (`person`) or the computer play a side.
pub fn seat_id(side: Side, person: bool) -> String {
    format!("seat_{}_{}", side_name(side), if person { "person" } else { "computer" })
}

/// The map takes this share of the window's width; the minimap this share of
/// the height.
const MAP_WIDTH_SHARE: f32 = 0.62;
const MINIMAP_HEIGHT_SHARE: f32 = 0.3;
const SPACING_DP: i32 = 5;
/// The list of maps: how high it is at most, in dp.
const MAPS_DP: i32 = 140;

/// The maps there are to play: the game's (`assets/maps`: a `.map` file, or a
/// directory with a `terrain.map`), and the owner's private ones beside the
/// assets (`encumbered/`), by name.
pub fn find_maps(assets: &Path) -> Vec<PathBuf> {
    let mut maps = Vec::new();
    let encumbered = assets.parent().map(|root| root.join("encumbered"));
    for dir in [Some(assets.join("maps")), encumbered].into_iter().flatten() {
        let Ok(entries) = files::list(&dir) else { continue };
        let is_map = |p: &PathBuf| p.extension().is_some_and(|x| x == "map") || files::is_file(&p.join("terrain.map"));
        maps.extend(entries.into_iter().filter(is_map));
    }
    maps
}

/// A map's name for the list: its file's or directory's name.
pub fn map_name(path: &Path) -> String {
    path.file_stem().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

pub struct NewGame {
    pub screen: MapScreen,
    maps: Vec<PathBuf>,
    chosen: Option<usize>,
    setup: Setup,
    /// Where the settings go, and what is in it now.
    settings: WidgetId,
    settings_scroll: WidgetId,
    start_button: WidgetId,
    sidebar: WidgetId,
    /// What went wrong with the chosen map, if anything.
    problem: Option<String>,
    start: bool,
    /// The random map, while it is the one chosen.
    random: Option<Random>,
    /// The units template's text, if the assets have it (no random maps
    /// without it), and whether it has a ship and a flyer.
    template: Option<(String, bool, bool)>,
    /// Where the games in progress are kept, if anywhere (`save`), and
    /// those there are: each one's file, and what it says of itself (or
    /// why it cannot be read).
    saves: Option<PathBuf>,
    saved: Vec<(PathBuf, Result<Summary, String>)>,
    /// The one whose Delete was pressed once: again, and it is gone.
    deleting: Option<usize>,
    /// The one to play on, once asked for; and why the last one asked for
    /// could not be.
    resume: Option<PathBuf>,
    resume_problem: Option<String>,
}

/// A random map's settings (a generator's config), and whether one was made
/// with them (and what it says of itself).
#[derive(Clone, Debug)]
pub struct Random {
    pub config: Config,
    pub preset: usize,
    pub made: bool,
    /// What the hard checks failed (`mapgen::validate`), if the map was made
    /// all the same: it is less fair than asked.
    pub failures: Vec<String>,
}

/// A seed from the clock (each map its own; the rules never look at it).
fn clock_seed() -> u64 {
    (platform::clock_nanos() % 1_000_000_007) as u64
}

/// The size a config asks for, as an index into `SIZES` (medium if none).
fn size_index(cfg: &Config) -> usize {
    match cfg.size {
        Some(MapSize::Named(name)) => SIZES.iter().position(|(n, _)| *n == name).unwrap_or(2),
        _ => 2,
    }
}

/// The density, as an index into `DENSITIES` (normal if none).
fn density_index(cfg: &Config) -> usize {
    cfg.density.and_then(|d| DENSITIES.iter().position(|(_, x)| (*x - d).abs() < 1e-9)).unwrap_or(1)
}

/// The temples for each side, as an index into `TEMPLE_CHOICES`.
fn temples_index(cfg: &Config) -> usize {
    TEMPLE_CHOICES.iter().position(|x| (*x - cfg.temples_per_side).abs() < 1e-9).unwrap_or(2)
}

impl NewGame {
    /// The screen for a window of `viewport` pixels at `dpi`, with `chosen`
    /// (or the game's continent, or the first map) chosen.
    pub fn new(assets: &Path, viewport: Size, dpi: i32, chosen: Option<&Path>) -> Result<NewGame, String> {
        let blank = Game::new(Map::new(20, 20, TileType::Water).map_err(|e| format!("{e:?}"))?);
        let mut screen = MapScreen::new(assets, viewport, dpi, LAYOUT, blank)?;
        // The map is there to be looked over: a drag moves it.
        screen.set_drag_pans(true);
        screen.set_help(vec![HelpSection::new(
            "A new game",
            &[
                ("A map's name", "Choose it: it shows beside the settings"),
                ("A random map", "Its preset, size, sides, cities, temples, cities at sea and in the air, seed: then Make the map"),
                ("Drag on the map", "Move it"),
                ("Turns, Rounds", "How to play: one side after the other; or all sides give their orders, then the round plays out at once"),
                ("Order of turns", "The same each round, or drawn anew (in rounds: who steps first as a round plays out)"),
                ("Battles", "Lines of duels, strength against strength, or dice; how much chance, 0 to 10"),
                ("Battles, on the dice", "What a roll of exactly a side's number does, in place of the chance"),
                ("Wounds last, Losers hit back", "Wounds stay until a side's turn ends; the loser of an exchange may hit back"),
                ("Cap 9", "No unit fights stronger than 9, its bonuses counted"),
                ("Gold", "What each side's treasury holds at the start"),
                ("Fog of war", "Each side sees only what is near its units and cities"),
                ("Fog: Total", "And the land a side has never seen is black, shut to its units"),
                ("Person, Computer", "Who plays each side: several people take turns at the screen (Computer again: its next style)"),
                ("Start the game", "Play"),
                ("Games in progress", "Games left before they were over: Play on; or Delete, pressed twice"),
            ],
        )]);
        let find = |screen: &MapScreen, id: &str| {
            screen.ui.find(screen.ui.root(), TbId::new(id)).ok_or(format!("{LAYOUT}: no widget with id {id}"))
        };
        let (settings, sidebar) = (find(&screen, "settings")?, find(&screen, "main_sidebar")?);
        let (settings_scroll, start_button) = (find(&screen, "settings_scroll")?, find(&screen, START_ID)?);
        let maps = find_maps(assets);
        let pick = chosen
            .and_then(|c| maps.iter().position(|m| m == c))
            .or_else(|| maps.iter().position(|m| map_name(m) == "continent"))
            .or((!maps.is_empty()).then_some(0));
        let mut setup = Setup::solo(&screen.game.map, None, COMPUTER);
        // Each game its own chance: the seed is drawn here, once (the rules
        // themselves never look at the clock).
        setup.seed = (platform::clock_nanos() as u64).max(1);
        let template = files::read_to_string(&assets.join(UNITS_TEMPLATE)).ok().and_then(|text| {
            let kinds = crate::state::scenario::parse_units(&text).ok()?;
            let has = |class: MoveClass| kinds.iter().any(|k| movement_class(k) == class);
            Some((text, has(MoveClass::Naval), has(MoveClass::Flyer)))
        });
        let mut new_game = NewGame {
            screen,
            maps,
            chosen: None,
            setup,
            settings,
            settings_scroll,
            start_button,
            sidebar,
            problem: None,
            start: false,
            random: None,
            template,
            saves: None,
            saved: Vec::new(),
            deleting: None,
            resume: None,
            resume_problem: None,
        };
        new_game.apply_layout_sizes();
        match pick {
            Some(i) => new_game.choose(i),
            None => new_game.rebuild(),
        }
        new_game.screen.settle();
        Ok(new_game)
    }

    /// The games in progress are kept in `dir` (or nowhere): those there
    /// are listed, to be played on or deleted.
    pub fn set_saves(&mut self, dir: Option<PathBuf>) {
        self.saves = dir;
        self.list_saved();
        self.rebuild();
    }

    fn list_saved(&mut self) {
        self.saved = self.saves.as_deref().map(save::list).unwrap_or_default();
        self.deleting = None;
    }

    /// The games in progress: each one's file, and what it says of itself.
    pub fn saved(&self) -> &[(PathBuf, Result<Summary, String>)] {
        &self.saved
    }

    /// The game in progress that was asked for, to be played on: once.
    pub fn take_resume(&mut self) -> Option<PathBuf> {
        self.resume.take()
    }

    /// The game asked for could not be played on: the screen says why.
    pub fn resume_failed(&mut self, why: String) {
        self.resume_problem = Some(why);
        self.rebuild();
    }

    /// Why the last game asked for could not be played on, if it could not.
    pub fn resume_problem(&self) -> Option<&str> {
        self.resume_problem.as_deref()
    }

    /// Delete the `i`th game in progress: asked once it says so, asked
    /// again it is gone.
    pub fn delete_saved(&mut self, i: usize) {
        let Some((path, _)) = self.saved.get(i) else { return };
        if self.deleting != Some(i) {
            self.deleting = Some(i);
        } else {
            match save::delete(path) {
                Ok(()) => self.resume_problem = None,
                Err(why) => self.resume_problem = Some(why),
            }
            self.list_saved();
        }
        self.rebuild();
    }

    /// The maps it offers.
    pub fn maps(&self) -> &[PathBuf] {
        &self.maps
    }

    /// The map chosen, if any (a map file: not a random map).
    pub fn chosen(&self) -> Option<&Path> {
        self.chosen.map(|i| self.maps[i].as_path())
    }

    /// The random map's settings, while it is the one chosen.
    pub fn random(&self) -> Option<&Random> {
        self.random.as_ref()
    }

    /// To change the random map's settings (its seed, say): it is to be
    /// made again.
    pub fn random_config_mut(&mut self) -> Option<&mut Config> {
        if self.random.is_some() {
            self.unmake();
        }
        self.random.as_mut().map(|r| &mut r.config)
    }

    /// Choose a random map: its settings show (the classic preset at first,
    /// a seed from the clock); nothing is made until Make the map.
    pub fn choose_random(&mut self) {
        self.chosen = None;
        self.problem = None;
        if self.random.is_none() {
            let config = Config { seed: clock_seed(), size: Some(MapSize::Named("medium")), ..Config::default() };
            self.random = Some(Random { config, preset: 0, made: false, failures: Vec::new() });
        }
        if !self.random.as_ref().is_some_and(|r| r.made) {
            self.screen.set_game(Game::new(Map::new(20, 20, TileType::Water).expect("20 x 20 is a valid map")));
            self.setup.seats.clear();
        }
        self.rebuild();
    }

    /// Make the random map with its settings, and show it: its sides get
    /// their seats, as a chosen map's do.
    pub fn make_random(&mut self) {
        let (Some(random), Some((units, _, _))) = (self.random.as_mut(), self.template.as_ref()) else { return };
        match crate::mapgen::generate(&random.config, units, UNITS_TEMPLATE) {
            Ok(made) => match made.map() {
                Ok(map) => {
                    random.made = true;
                    random.failures = made.failures.clone();
                    // (The texts the map is, for a saved game to keep.)
                    let source = crate::state::MapSource {
                        name: "random".to_string(),
                        terrain: made.terrain_text(),
                        units: Some(units.clone()),
                        cities: Some(made.cities_text()),
                    };
                    let game = Game::from_scenario(crate::state::Scenario { map, assets: None, warnings: Vec::new() })
                        .with_source(source);
                    self.seat(game);
                    self.problem = None;
                }
                Err(e) => self.problem = Some(e),
            },
            Err(e) => self.problem = Some(e),
        }
        self.rebuild();
    }

    /// Show `game`'s map, its sides seated (the first side's a person, the
    /// others the computer's), the rest of the setup as it was.
    fn seat(&mut self, game: Game) {
        let person = game.setup().seats.iter().find(|s| s.controller == Controller::Person).map(|s| s.side);
        let mut setup = Setup::solo(&game.map, person, COMPUTER);
        (setup.mode, setup.order, setup.seed) = (self.setup.mode, self.setup.order, self.setup.seed);
        setup.combat = self.setup.combat;
        setup.gold = self.setup.gold;
        setup.fog = self.setup.fog;
        self.setup = setup;
        self.screen.set_game(game);
    }

    /// A random map's setting changed: it is to be made again.
    fn unmake(&mut self) {
        if let Some(random) = self.random.as_mut() {
            random.made = false;
        }
        self.screen.set_game(Game::new(Map::new(20, 20, TileType::Water).expect("20 x 20 is a valid map")));
        self.setup.seats.clear();
    }

    /// How the game will be set up.
    pub fn setup(&self) -> &Setup {
        &self.setup
    }

    /// To change how the game will be set up (the seed, say).
    pub fn setup_mut(&mut self) -> &mut Setup {
        &mut self.setup
    }

    /// What is wrong with the chosen map, if anything.
    pub fn problem(&self) -> Option<&str> {
        self.problem.as_deref()
    }

    /// Choose the `i`th map: it shows, its sides get their seats (the first
    /// side's a person, the others the computer's), the rest of the setup stays.
    pub fn choose(&mut self, i: usize) {
        let Some(path) = self.maps.get(i).cloned() else { return };
        self.chosen = Some(i);
        self.random = None;
        match Game::load(&path) {
            Ok((game, _)) => {
                self.seat(game);
                self.problem = None;
            }
            Err(e) => self.problem = Some(e),
        }
        self.rebuild();
    }

    /// True if the chosen map can be played: it was read (or made), and it
    /// has sides.
    pub fn can_start(&self) -> bool {
        let map = self.chosen.is_some() || self.random.as_ref().is_some_and(|r| r.made);
        self.problem.is_none() && map && !self.setup.seats.is_empty()
    }

    /// True once the game was asked to start, with a map that can be played.
    pub fn wants_to_start(&self) -> bool {
        self.start && self.can_start()
    }

    /// The game to play: the chosen map, set up as chosen. Leaves the screen
    /// with a blank map.
    pub fn take_game(&mut self) -> Game {
        let blank = Game::new(Map::new(20, 20, TileType::Water).expect("20 x 20 is a valid map"));
        std::mem::replace(&mut self.screen.game, blank).with_setup(self.setup.clone())
    }

    /// Size the map and the minimap from the window's size.
    fn apply_layout_sizes(&mut self) {
        let Size { w, h } = self.screen.ui.viewport();
        let spacing = self.screen.ui.dimension_converter().dp_to_px(SPACING_DP);
        let map_w = (w as f32 * MAP_WIDTH_SHARE).floor() as i32;
        let side_w = (w - map_w - spacing).max(0);
        let minimap_h = (h as f32 * MINIMAP_HEIGHT_SHARE) as i32;
        self.screen.set_map_sizes(Size::new(map_w, h.max(0)), Size::new(side_w, minimap_h));
        let narrow = LayoutParams { min_w: Some(0), max_w: Some(side_w), ..LayoutParams::default() };
        self.screen.ui.set_layout_params(self.sidebar, Some(narrow));
        // The settings get what the minimap and the start button leave, and
        // scroll in it.
        let button_h = self.screen.ui.preferred_size_free(self.start_button).pref.h;
        let settings_h = (h - minimap_h - button_h - 3 * spacing).max(0);
        self.screen.ui.set_layout_params(self.settings_scroll, Some(LayoutParams::fixed(side_w, settings_h)));
    }

    /// Build the settings again: the maps, the mode, the order, the seats.
    fn rebuild(&mut self) {
        let ui = &mut self.screen.ui;
        for child in ui.children(self.settings).to_vec() {
            ui.remove(child);
        }
        let dp = |ui: &turbo_ferret::Ui, n: i32| ui.dimension_converter().dp_to_px(n);
        let (c, space) = (self.settings, dp(ui, SPACING_DP));
        let press = |ui: &mut turbo_ferret::Ui, button: WidgetId, group: &str, on: bool| {
            ui.set_group_id(button, TbId::new(group));
            ui.set_value(button, i32::from(on));
        };

        if !self.saved.is_empty() || self.resume_problem.is_some() {
            build::heading(ui, c, "Games in progress");
            for (i, (path, summary)) in self.saved.iter().enumerate() {
                let (line, players) = match summary {
                    Ok(summary) => (summary.line(), summary.players()),
                    Err(why) => (format!("{}: it cannot be read", map_name(path)), why.clone()),
                };
                build::text(ui, c, &line);
                let row = build::row(ui, c, space);
                let play = build::button(ui, row, &continue_id(i), "Play on");
                ui.set_disabled(play, summary.is_err());
                let delete = if self.deleting == Some(i) { "Delete it for good" } else { "Delete" };
                build::button(ui, row, &delete_id(i), delete);
                build::text(ui, row, &players);
            }
            if let Some(problem) = &self.resume_problem {
                build::text(ui, c, problem);
            }
            build::gap(ui, c, space);
        }

        build::heading(ui, c, "Map");
        let scroll = ui.add(c, ScrollContainer::new(ScrollMode::YAuto).with_adapt_content_size(true));
        let side_w = ui.layout_params(self.sidebar).and_then(|lp| lp.max_w).unwrap_or(200);
        let rows = (self.maps.len() as i32 + 1) * dp(ui, 22);
        ui.set_layout_params(scroll, Some(LayoutParams::fixed(side_w, rows.min(dp(ui, MAPS_DP)))));
        let list = build::column(ui, scroll, 0);
        let random = build::button(ui, list, RANDOM_ID, "A random map");
        ui.set_skin(random, TbId::new("ListRow"));
        build::set_width(ui, random, side_w - dp(ui, 18));
        build::text_from_left(ui, random);
        press(ui, random, "maps", self.random.is_some());
        ui.set_disabled(random, self.template.is_none());
        for (i, path) in self.maps.iter().enumerate() {
            let button = build::button(ui, list, &map_id(i), &map_name(path));
            ui.set_skin(button, TbId::new("ListRow"));
            build::set_width(ui, button, side_w - dp(ui, 18));
            build::text_from_left(ui, button);
            press(ui, button, "maps", self.chosen == Some(i));
        }
        if self.maps.is_empty() {
            build::text(ui, list, "No maps in assets/maps.");
        }
        if let Some(problem) = &self.problem {
            build::text(ui, c, problem);
        }
        // How big the map shown is, and how many cities it has (a random
        // map: once it is made).
        let shown = self.chosen.is_some() || self.random.as_ref().is_some_and(|r| r.made);
        let facts = (shown && self.problem.is_none()).then(|| map_facts(&self.screen.game.map));
        if let (Some(facts), None) = (&facts, &self.random) {
            let line = build::text(ui, c, facts);
            ui.set_id(line, TbId::new(MAP_FACTS_ID));
        }
        if let (Some(random), Some((_, ships, flyers))) = (&self.random, &self.template) {
            let cfg = &random.config;
            let stepper = |ui: &mut turbo_ferret::Ui, (less, more): (&str, &str), words: &str| {
                let row = build::row(ui, c, space);
                build::button(ui, row, less, "-");
                build::text(ui, row, words);
                build::button(ui, row, more, "+");
            };
            build::gap(ui, c, space);
            build::heading(ui, c, "A random map");
            stepper(ui, (PRESET_LESS_ID, PRESET_MORE_ID), &format!("Preset: {}", PRESETS[random.preset].0));
            stepper(ui, (SIZE_LESS_ID, SIZE_MORE_ID), &format!("Size: {}", SIZES[size_index(cfg)].0));
            stepper(ui, (SIDES_LESS_ID, SIDES_MORE_ID), &format!("{} sides", cfg.sides));
            let cities = ["Few cities", "Cities as usual", "Many cities"][density_index(cfg)];
            stepper(ui, (CITIES_LESS_ID, CITIES_MORE_ID), cities);
            let temples = match TEMPLE_CHOICES[temples_index(cfg)] {
                0.0 => "No temples".to_string(),
                0.5 => "A temple for two sides".to_string(),
                1.0 => "A temple for each side".to_string(),
                n => format!("{n} temples for each side"),
            };
            stepper(ui, (TEMPLES_LESS_ID, TEMPLES_MORE_ID), &temples);
            let row = build::row(ui, c, space);
            let on = |b: bool| if b { "on" } else { "off" };
            let sea = build::button(ui, row, SEA_ID, &format!("Cities at sea: {}", on(cfg.sea_cities)));
            ui.set_disabled(sea, !ships);
            let air = build::button(ui, row, AIR_ID, &format!("In the air: {}", on(cfg.fly_cities)));
            ui.set_disabled(air, !flyers);
            let row = build::row(ui, c, space);
            build::text(ui, row, &format!("Seed {}", cfg.seed));
            build::button(ui, row, SEED_ID, "New seed");
            build::primary_button(ui, row, MAKE_ID, if random.made { "Make it again" } else { "Make the map" });
            if let Some(facts) = &facts {
                let line = build::text(ui, c, facts);
                ui.set_id(line, TbId::new(MAP_FACTS_ID));
            }
            if !random.made {
                build::text(ui, c, "Make the map to see it and play it.");
            } else if !random.failures.is_empty() {
                build::text(ui, c, "Less fair than asked: no try made it so.");
            }
        }

        build::gap(ui, c, space);
        build::heading(ui, c, "How to play");
        let row = build::row(ui, c, space);
        let turns = build::button(ui, row, MODE_TURNS_ID, "Turns");
        press(ui, turns, "mode", self.setup.mode == Mode::Turns);
        let rounds = build::button(ui, row, MODE_ROUNDS_ID, "Rounds");
        press(ui, rounds, "mode", self.setup.mode == Mode::Rounds);

        build::gap(ui, c, space);
        build::heading(ui, c, "Order of turns");
        let row = build::row(ui, c, space);
        let fixed = build::button(ui, row, ORDER_FIXED_ID, "The same");
        press(ui, fixed, "order", self.setup.order == TurnOrder::Fixed);
        let drawn = build::button(ui, row, ORDER_DRAWN_ID, "Drawn each round");
        press(ui, drawn, "order", self.setup.order == TurnOrder::Drawn);

        build::gap(ui, c, space);
        build::heading(ui, c, "Battles");
        let row = build::row(ui, c, space);
        let lines = build::button(ui, row, BATTLES_LINES_ID, "Lines");
        press(ui, lines, "battles", self.setup.combat.system == System::Lines);
        let strength = build::button(ui, row, BATTLES_STRENGTH_ID, "Strength");
        press(ui, strength, "battles", self.setup.combat.system == System::Strength);
        let dice = build::button(ui, row, BATTLES_DICE_ID, "Dice");
        press(ui, dice, "battles", self.setup.combat.system == System::Dice);
        let row = build::row(ui, c, space);
        if self.setup.combat.system == System::Dice {
            // The dice are the chance: what a roll of exactly a number does.
            build::text(ui, row, "Exact roll");
            for (id, word, exact) in [
                (EXACT_HITS_ID, "Wins", Exact::Hits),
                (EXACT_MISSES_ID, "Misses", Exact::Misses),
                (EXACT_DEFENDERS_ID, "Wins defending", Exact::Defenders),
            ] {
                let button = build::button(ui, row, id, word);
                press(ui, button, "exact", self.setup.combat.exact == exact);
            }
        } else {
            build::button(ui, row, CHANCE_LESS_ID, "-");
            build::text(ui, row, &format!("Chance {} of {MAX_CHANCE}", self.setup.combat.chance));
            build::button(ui, row, CHANCE_MORE_ID, "+");
        }
        let row = build::row(ui, c, space);
        let wounds = build::button(ui, row, WOUNDS_ID, "Wounds last");
        press(ui, wounds, "wounds", self.setup.combat.wounds);
        let back = build::button(ui, row, BLOWS_BACK_ID, "Losers hit back");
        press(ui, back, "blows_back", self.setup.combat.blows_back);
        let cap = build::button(ui, row, CAP_ID, &format!("Cap {UNIT_CAP}"));
        press(ui, cap, "unit_cap", self.setup.combat.cap.is_some());

        // The gold and the fog of war, each a row that says what it is.
        build::gap(ui, c, space);
        let row = build::row(ui, c, space);
        build::button(ui, row, GOLD_LESS_ID, "-");
        build::text(ui, row, &format!("{} gold for each side at the start", self.setup.gold));
        build::button(ui, row, GOLD_MORE_ID, "+");
        let row = build::row(ui, c, space);
        build::text(ui, row, "Fog of war");
        for (id, word, fog) in
            [(FOG_OFF_ID, "Off", Fog::Off), (FOG_ON_ID, "On", Fog::On), (FOG_TOTAL_ID, "Total", Fog::Total)]
        {
            let button = build::button(ui, row, id, word);
            press(ui, button, "fog", self.setup.fog == fog);
        }

        build::gap(ui, c, space);
        build::heading(ui, c, "Who plays");
        if self.setup.seats.is_empty() {
            build::text(ui, c, "This map has no sides: nothing to play.");
        }
        let fixed_order = self.setup.order == TurnOrder::Fixed;
        let mut names = Vec::new();
        for (i, seat) in self.setup.seats.iter().enumerate() {
            let row = build::row(ui, c, space);
            let name = side_name(seat.side);
            let label = if fixed_order { format!("{}. {name}", i + 1) } else { name.to_string() };
            names.push(build::text(ui, row, &label));
            let person = build::button(ui, row, &seat_id(seat.side, true), "Person");
            let label = match &seat.controller {
                Controller::Computer(agent) => format!("Computer: {agent}"),
                Controller::Person => format!("Computer: {COMPUTER}"),
            };
            let computer = build::button(ui, row, &seat_id(seat.side, false), &label);
            let group = format!("seat_{name}");
            press(ui, person, &group, seat.controller == Controller::Person);
            press(ui, computer, &group, seat.controller != Controller::Person);
        }
        let width = names.iter().map(|n| ui.preferred_size_free(*n).pref.w).max().unwrap_or(0);
        for name in names {
            build::set_width(ui, name, width);
        }
        let playable = self.can_start();
        if let Some(start) = self.screen.ui.find(self.screen.ui.root(), TbId::new(START_ID)) {
            self.screen.ui.set_disabled(start, !playable);
        }
        // Laid out at once, so that the new buttons are where they show.
        self.screen.settle();
    }

    /// The window was resized.
    pub fn resize(&mut self, viewport: Size) {
        self.screen.ui.set_viewport(viewport);
        self.apply_layout_sizes();
        self.rebuild();
    }

    /// Apply what the widgets recorded.
    pub fn pump(&mut self) {
        for command in self.screen.take_map_commands() {
            self.screen.navigate(command);
        }
        for command in self.screen.pump_shared() {
            let RootCommand::Click(id) = command else { continue };
            if id == TbId::new(START_ID) {
                self.start = true;
            } else if let Some(i) = (0..self.saved.len()).find(|i| id == TbId::new(&continue_id(*i))) {
                self.resume = self.saved.get(i).map(|(path, _)| path.clone());
            } else if let Some(i) = (0..self.saved.len()).find(|i| id == TbId::new(&delete_id(*i))) {
                self.delete_saved(i);
            } else if id == TbId::new(RANDOM_ID) {
                self.choose_random();
            } else if id == TbId::new(MAKE_ID) {
                self.make_random();
            } else if let Some(random) = self.random.as_mut().filter(|_| random_setting(id)) {
                let cfg = &mut random.config;
                let step = |i: usize, n: usize, more: bool| if more { (i + 1).min(n - 1) } else { i.saturating_sub(1) };
                let more = |a: &str| id == TbId::new(a);
                if more(PRESET_LESS_ID) || more(PRESET_MORE_ID) {
                    random.preset = step(random.preset, PRESETS.len(), more(PRESET_MORE_ID));
                    // The preset's terrain, with the screen's other settings.
                    if let Ok(mut fresh) = Config::preset(PRESETS[random.preset].0) {
                        (fresh.seed, fresh.size, fresh.sides, fresh.density) =
                            (cfg.seed, cfg.size, cfg.sides, cfg.density);
                        (fresh.temples_per_side, fresh.temples) = (cfg.temples_per_side, None);
                        // (The islands preset asks for cities at sea.)
                        fresh.sea_cities = fresh.sea_cities || cfg.sea_cities;
                        fresh.fly_cities = cfg.fly_cities;
                        *cfg = fresh;
                        // (A colour for each side, as many as the screen's.)
                        let _ = cfg.set("sides", &cfg.sides.to_string());
                    }
                } else if more(SIZE_LESS_ID) || more(SIZE_MORE_ID) {
                    let i = step(size_index(cfg), SIZES.len(), more(SIZE_MORE_ID));
                    cfg.size = Some(MapSize::Named(SIZES[i].0));
                } else if more(SIDES_LESS_ID) || more(SIDES_MORE_ID) {
                    // Through `Config::set`, which gives each side a colour
                    // (the preset's are four: a fifth side's map crashed).
                    let n =
                        if more(SIDES_MORE_ID) { (cfg.sides + 1).min(8) } else { cfg.sides.saturating_sub(1).max(2) };
                    let _ = cfg.set("sides", &n.to_string());
                } else if more(CITIES_LESS_ID) || more(CITIES_MORE_ID) {
                    let i = step(density_index(cfg), DENSITIES.len(), more(CITIES_MORE_ID));
                    cfg.density = Some(DENSITIES[i].1);
                } else if more(TEMPLES_LESS_ID) || more(TEMPLES_MORE_ID) {
                    let i = step(temples_index(cfg), TEMPLE_CHOICES.len(), more(TEMPLES_MORE_ID));
                    (cfg.temples_per_side, cfg.temples) = (TEMPLE_CHOICES[i], None);
                } else if more(SEA_ID) {
                    cfg.sea_cities = !cfg.sea_cities;
                } else if more(AIR_ID) {
                    cfg.fly_cities = !cfg.fly_cities;
                } else if more(SEED_ID) {
                    cfg.seed = clock_seed();
                }
                self.unmake();
                self.rebuild();
            } else if id == TbId::new(MODE_TURNS_ID) {
                self.setup.mode = Mode::Turns;
                self.rebuild();
            } else if id == TbId::new(MODE_ROUNDS_ID) {
                self.setup.mode = Mode::Rounds;
                self.rebuild();
            } else if let Some(system) = [
                (BATTLES_LINES_ID, System::Lines),
                (BATTLES_STRENGTH_ID, System::Strength),
                (BATTLES_DICE_ID, System::Dice),
            ]
            .into_iter()
            .find_map(|(button, system)| (id == TbId::new(button)).then_some(system))
            {
                self.setup.combat.system = system;
                self.rebuild();
            } else if let Some(exact) =
                [(EXACT_HITS_ID, Exact::Hits), (EXACT_MISSES_ID, Exact::Misses), (EXACT_DEFENDERS_ID, Exact::Defenders)]
                    .into_iter()
                    .find_map(|(button, exact)| (id == TbId::new(button)).then_some(exact))
            {
                self.setup.combat.exact = exact;
                self.rebuild();
            } else if id == TbId::new(WOUNDS_ID) {
                self.setup.combat.wounds = !self.setup.combat.wounds;
                self.rebuild();
            } else if id == TbId::new(BLOWS_BACK_ID) {
                self.setup.combat.blows_back = !self.setup.combat.blows_back;
                self.rebuild();
            } else if id == TbId::new(CAP_ID) {
                let cap = &mut self.setup.combat.cap;
                *cap = if cap.is_some() { None } else { Some(UNIT_CAP) };
                self.rebuild();
            } else if id == TbId::new(CHANCE_LESS_ID) || id == TbId::new(CHANCE_MORE_ID) {
                let chance = &mut self.setup.combat.chance;
                *chance = if id == TbId::new(CHANCE_LESS_ID) {
                    chance.saturating_sub(1)
                } else {
                    (*chance + 1).min(MAX_CHANCE)
                };
                self.rebuild();
            } else if id == TbId::new(FOG_OFF_ID) {
                self.setup.fog = Fog::Off;
            } else if id == TbId::new(FOG_ON_ID) {
                self.setup.fog = Fog::On;
            } else if id == TbId::new(FOG_TOTAL_ID) {
                self.setup.fog = Fog::Total;
            } else if id == TbId::new(GOLD_LESS_ID) || id == TbId::new(GOLD_MORE_ID) {
                let gold = &mut self.setup.gold;
                *gold = if id == TbId::new(GOLD_LESS_ID) {
                    gold.saturating_sub(GOLD_STEP)
                } else {
                    (*gold + GOLD_STEP).min(MAX_GOLD)
                };
                self.rebuild();
            } else if id == TbId::new(ORDER_FIXED_ID) || id == TbId::new(ORDER_DRAWN_ID) {
                self.setup.order = if id == TbId::new(ORDER_FIXED_ID) { TurnOrder::Fixed } else { TurnOrder::Drawn };
                self.rebuild();
            } else if let Some(i) = (0..self.maps.len()).find(|i| id == TbId::new(&map_id(*i))) {
                self.choose(i);
            } else {
                let sides: Vec<Side> = self.setup.seats.iter().map(|s| s.side).collect();
                for side in sides {
                    let Some(seat) = self.setup.seats.iter_mut().find(|s| s.side == side) else { continue };
                    seat.controller = if id == TbId::new(&seat_id(side, true)) {
                        Controller::Person
                    } else if id == TbId::new(&seat_id(side, false)) {
                        // A computer already: its next style.
                        match &seat.controller {
                            Controller::Computer(agent) => {
                                let at = COMPUTERS.iter().position(|a| a == agent).map_or(0, |i| i + 1);
                                Controller::Computer(COMPUTERS[at % COMPUTERS.len()].to_string())
                            }
                            Controller::Person => Controller::Computer(COMPUTER.to_string()),
                        }
                    } else {
                        continue;
                    };
                    // The button says which.
                    self.rebuild();
                    break;
                }
            }
        }
        self.screen.sync_game();
    }
}

/// True if `id` is one of the random map's settings (not Make the map).
fn random_setting(id: TbId) -> bool {
    [
        PRESET_LESS_ID,
        PRESET_MORE_ID,
        SIZE_LESS_ID,
        SIZE_MORE_ID,
        SIDES_LESS_ID,
        SIDES_MORE_ID,
        CITIES_LESS_ID,
        CITIES_MORE_ID,
        TEMPLES_LESS_ID,
        TEMPLES_MORE_ID,
        SEA_ID,
        AIR_ID,
        SEED_ID,
    ]
    .iter()
    .any(|a| id == TbId::new(a))
}

impl Scene for NewGame {
    fn screen(&self) -> &MapScreen {
        &self.screen
    }

    fn screen_mut(&mut self) -> &mut MapScreen {
        &mut self.screen
    }

    fn resize(&mut self, viewport: Size) {
        NewGame::resize(self, viewport);
    }

    fn pump(&mut self) {
        NewGame::pump(self);
    }

    fn marked(&self, _hex: Hex) -> bool {
        false
    }
}
