// The economy's numbers as data, for docs/design/economy/sim.py.
//
// Rust is the only place these numbers are written. This test renders them
// into docs/design/economy/numbers.json and fails when the file in the
// repository is not what it renders. Under "game" is what the simulation
// reads instead of declaring: the constants under their Rust names, the cost
// curves, what each thing requires, the ships and defences with their fight
// figures and rapid fire, and the curves it only looks up at whole rings,
// ratings and levels (the mines, energy and storage among them). Under
// "tables" is what the Rust formulas give over the ranges the simulation
// computes for itself, which tools/check_sim_constants.py holds its Python
// formulas to.
//
//   cargo test -p iac-sim --test economy_numbers                     # verify
//   UPDATE_FIXTURES=1 cargo test -p iac-sim --test economy_numbers   # regenerate

use std::fs;
use std::path::Path;

use iac_shared::constants::*;
use iac_shared::pace::{CAP_EXPONENT_PAST_KNEE, CAP_KNEE, FIND_ANCHOR, PRESETS, Pace, PaceClass};
use iac_shared::scaling::*;
use iac_sim::score::DEFENCE_SHARE;

const FILE: &str = "docs/design/economy/numbers.json";
const RINGS: u16 = 60;
const PACES: [f64; 3] = [1.0, 10.0, 600.0];
const FIND_PACES: [f64; 4] = [1.0, 10.0, 600.0, 8000.0];
const CAP_LEVELS: [u8; 3] = [0, 5, 10];
const FIND_SPREAD_MAX: f64 = 0.03;
const DENSITIES: [Density; 4] = [Density::Sparse, Density::Moderate, Density::Rich, Density::Pristine];

// A number is kept as the text serde_json writes for its own type, so an f32
// is the shortest decimal that reads back as it: 0.08, the figure in the
// source, where a `serde_json::Value` would hold 0.07999999821186066.
enum Json {
    Atom(String),
    List(Vec<Json>),
    Map(Vec<(String, Json)>),
}

macro_rules! atoms {
    ($($t:ty),*) => {$(
        impl From<$t> for Json {
            fn from(x: $t) -> Json {
                Json::Atom(serde_json::to_string(&x).unwrap())
            }
        }
    )*};
}
atoms!(f32, f64, u8, u16, u32, i32, u64, usize, &str, String);

impl<T: Into<Json>> From<Vec<T>> for Json {
    fn from(items: Vec<T>) -> Json {
        Json::List(items.into_iter().map(Into::into).collect())
    }
}

impl<T: Into<Json>, const N: usize> From<[T; N]> for Json {
    fn from(items: [T; N]) -> Json {
        Json::List(items.into_iter().map(Into::into).collect())
    }
}

impl From<Resources> for Json {
    fn from(r: Resources) -> Json {
        [r.metal, r.crystal, r.deuterium].into()
    }
}

macro_rules! list {
    ($($value:expr),* $(,)?) => { Json::List(vec![$(Json::from($value)),*]) };
}

macro_rules! map {
    ($($key:literal: $value:expr),* $(,)?) => { Json::Map(vec![$(($key.to_string(), Json::from($value))),*]) };
}

/// Constants under the names they have in Rust.
macro_rules! named {
    ($($name:ident),* $(,)?) => { Json::Map(vec![$((stringify!($name).to_string(), Json::from($name))),*]) };
}

impl Json {
    fn keyed<K: std::fmt::Debug>(entries: impl IntoIterator<Item = (K, Json)>) -> Json {
        Json::Map(entries.into_iter().map(|(key, value)| (format!("{key:?}"), value)).collect())
    }

    /// Containers down to `open` levels below this one get a line for each
    /// entry; deeper ones are written on one line.
    fn write(&self, out: &mut String, open: usize, indent: usize) {
        let (entries, close): (Vec<(String, &Json)>, char) = match self {
            Json::Atom(text) => return out.push_str(text),
            Json::List(items) => {
                out.push('[');
                (items.iter().map(|item| (String::new(), item)).collect(), ']')
            }
            Json::Map(entries) => {
                out.push('{');
                (entries.iter().map(|(key, value)| (format!("{}: ", serde_json::to_string(key).unwrap()), value)).collect(), '}')
            }
        };
        let pad = if open > 0 { format!("\n{}", "  ".repeat(indent + 1)) } else { String::new() };
        for (i, (key, value)) in entries.iter().enumerate() {
            if i > 0 {
                out.push_str(if open > 0 { "," } else { ", " });
            }
            out.push_str(&pad);
            out.push_str(key);
            value.write(out, open.saturating_sub(1), indent + 1);
        }
        if open > 0 && !entries.is_empty() {
            out.push('\n');
            out.push_str(&"  ".repeat(indent));
        }
        out.push(close);
    }
}

fn pace(value: f64) -> Pace {
    Pace::new(value).unwrap()
}

fn requires(prereqs: Prereqs) -> Vec<Json> {
    prereqs
        .iter()
        .map(|(p, level)| match p {
            PrereqRef::Building(b) => list!["building", format!("{b:?}"), *level],
            PrereqRef::Research(t) => list!["research", format!("{t:?}"), *level],
        })
        .collect()
}

fn item_name(item: ShipyardItem) -> String {
    match item {
        ShipyardItem::Ship(c) => format!("{c:?}"),
        ShipyardItem::Defence(d) => format!("{d:?}"),
    }
}

fn items() -> impl Iterator<Item = ShipyardItem> {
    ShipClass::ALL.into_iter().map(ShipyardItem::Ship).chain(DefenceKind::ALL.into_iter().map(ShipyardItem::Defence))
}

/// What `value` gives at every level from 0 to `max`.
fn at_levels<T: Into<Json>>(max: u8, value: impl Fn(u8) -> T) -> Json {
    Json::List((0..=max).map(|level| value(level).into()).collect())
}

fn researched(tech: ResearchType, level: u8) -> ResearchLevels {
    let mut research = ResearchLevels::default();
    research.set(tech, level);
    research
}

/// An empire with nothing but what is named.
fn built(levels: &[(BuildingType, u8)]) -> BuildingLevels {
    let mut buildings = BuildingLevels::default();
    for b in BuildingType::ALL {
        buildings.set(b, 0);
    }
    for &(b, level) in levels {
        buildings.set(b, level);
    }
    buildings
}

const MINES: [BuildingType; 3] = [BuildingType::MetalMine, BuildingType::CrystalMine, BuildingType::DeuteriumSynthesizer];

/// What the simulation reads in place of constants of its own.
fn game() -> Json {
    let constants = named![
        PRODUCTION_PER_HOUR_METAL, PRODUCTION_PER_HOUR_CRYSTAL, PRODUCTION_PER_HOUR_DEUTERIUM,
        NATURAL_PER_HOUR_METAL, NATURAL_PER_HOUR_CRYSTAL,
        ENERGY_PER_MINE, ENERGY_PER_SYNTHESIZER, ENERGY_PER_SOLAR_PLANT, ENERGY_PER_FUSION_REACTOR, FUSION_BURN_PER_HOUR,
        GRAVITON_ENERGY, GRAVITON_GROWTH, HOME_TEMPERATURE_MAX,
        BUILD_RATE, RESEARCH_RATE, SHIP_RATE,
        MAX_BUILDING_LEVEL, MAX_RESEARCH_LEVEL, QUEUE_WAITING,
        STARTING_RESOURCES, HOMEWORLD_MIN_DIST, HOMEWORLD_MAX_DIST,
        STORAGE_BASE, STORAGE_STEP, STORAGE_GROWTH, STORAGE_PROTECT_PER_LEVEL, STORAGE_PROTECT_MAX,
        CAP_KNEE, CAP_EXPONENT_PAST_KNEE,
        MAP_PAYS, JUMP_SPEED_REF, TANK_PER_DEUT_TANK_LEVEL, FUEL_SAVING_PER_LEVEL, FUEL_SAVING_MAX,
        HARVEST_YIELD_PER_LEVEL, HARVEST_YIELD_LEVELS, BUILD_SLOT_LEVELS, RECALL_SAVING_PER_LEVEL, SENSOR_HOPS_PER_LEVEL,
        SMALL_CARGO_IMPULSE_LEVEL, SMALL_CARGO_IMPULSE_SPEED, SMALL_CARGO_IMPULSE_FUEL,
        BOMBER_HYPERSPACE_LEVEL, BOMBER_HYPERSPACE_SPEED,
        FUEL_DEUT_PER_UNIT,
        NPC_POWER_BASE, NPC_POWER_GROWTH, NPC_MAX_SHIPS, NPC_STAT_MULT_MAX, NPC_MIN_PACK,
        MAX_THREAT, RAPID_FIRE_UNARMED, RAPID_FIRE_UNARMED_DEATHSTAR,
        WRECK_VALUE_K, WRECK_VALUE_EXPONENT, LOOT_SPLIT,
        REPAIR_COST_SHARE_HOME, REPAIR_COST_SHARE_OUTPOST,
        CHASE_HEXES, NPC_HEAL_STEPS,
        ORE_RING_GROWTH, ORE_RING_FIRST, ORE_RING_LAST, ORE_REGEN_BASE_HOURS, ORE_REGEN_HOURS_PER_RING,
        DERELICT_SPAWN_PCT, DERELICT_RESPAWN_TICKS, DERELICT_VALUE_BASE, DERELICT_VALUE_GROWTH, DERELICT_VALUE_RING,
        AMBUSH_GUARD_SHARE,
        RAID_MIN_PLAYER_AGE, RAID_MIN_SHIPYARD, RAID_POWER_FLOOR, RAID_POWER_PER_POINT, RAID_POWER_EXPONENT,
        RAID_POWER_ROLL_MIN, RAID_POWER_ROLL_MAX, RAID_ROUNDS, DEFENCE_REBUILD_PCT, DEFENCE_REBUILD_SPREAD,
        DEFENCE_REBUILD_EACH_BELOW, RAID_LOSS_CAP_METAL, RAID_LOSS_CAP_CRYSTAL, RAID_LOSS_CAP_DEUT,
        RAID_DEFENSE_SALVAGE_FRACTION,
        DEFENCE_SHARE, CHART_VALUE_BASE, CHART_VALUE_THREAT, CHART_VALUE_EXPONENT,
        RELIC_LOOT_MULT, FARM_DECAY, FARM_HALF_LIFE_RESPAWNS,
        EXCHANGE_FEE_MIN, EXCHANGE_FEE_MAX, EXCHANGE_METER_SHARE, EXCHANGE_WINDOW_TICKS,
        OUTPOST_MIN_RING, OUTPOST_MAX_RING, OUTPOST_SPACING, OUTPOST_MAX_LEVEL,
        OUTPOST_COST, OUTPOST_GROWTH, OUTPOST_BUILD_RATE, OUTPOST_STORE_BASE, OUTPOST_YIELD_PER_HOUR,
        OUTPOST_RAID_FACTOR, OUTPOST_RAID_LOSS, OUTPOST_REPAIR_SHARE,
        RELAY_MIN_RING, RELAY_COST_BASE, RELAY_COST_GROWTH, RELAY_BASE_MAX, RELAY_LEVELS_PER_STEP, RELAY_STORE,
        DEPOT_FILL_JUMPS_BASE, DEPOT_FILL_JUMPS_PER_LEVEL, RELAY_DECAY_TICKS,
        HUB_COUNT, HUB_FIRST_RING, HUB_BAND_RINGS, HUB_PERIOD_TICKS, HUB_WARN_TICKS, HUB_ATTEMPTS,
        RELIC_FADE_TICKS, RELIC_TOTAL_CAP, RELIC_PCT_OUTER_MAX, RELIC_PCT_DEEP_MAX, RELIC_DEEP_RING, RELIC_PCT_FLOOR_SHARE,
    ];

    let start = BuildingLevels::default();
    let building_rows = BuildingType::ALL.into_iter().map(|b| {
        let (cost, growth) = building_cost_curve(b);
        let row = map! {
            "label": b.label(), "cost": cost, "growth": growth, "requires": requires(building_prerequisites(b)),
            "starts_at": start.get(b),
        };
        (b, row)
    });

    let research_rows = ResearchType::ALL.into_iter().map(|t| {
        let (cost, growth) = research_cost_curve(t);
        let row = map! {
            "label": t.label(), "cost": cost, "growth": growth, "max_level": research_max_level(t),
            "requires": requires(research_prerequisites(t)), "energy": research_energy(t, 1),
        };
        (t, row)
    });

    let ship_rows = ShipClass::ALL.into_iter().map(|c| {
        let s = c.base_stats();
        let rapid = Json::Map(rapid_fire(c).map(|(target, shots)| (item_name(target), shots.into())).collect());
        let row = map! {
            "label": c.label(), "cost": c.build_cost(), "hull": s.hull, "shield": s.shield, "weapon": s.weapon,
            "power": ship_power(c), "speed": s.speed, "cargo": s.cargo, "fuel": s.fuel,
            "drive": c.drive().map_or(String::new(), |d| format!("{d:?}")),
            "tank_jumps": c.tank_jumps(), "harvest_power": c.harvest_power(), "flies": u8::from(c.flies()),
            "requires": requires(ship_prerequisites(c)), "rapid_fire": rapid,
        };
        (c, row)
    });

    let defence_rows = DefenceKind::ALL.into_iter().map(|d| {
        let s = d.base_stats();
        let row = map! {
            "label": d.label(), "cost": d.build_cost(), "hull": s.hull, "shield": s.shield, "weapon": s.weapon,
            "power": d.power(), "max_count": d.max_count().unwrap_or(0), "requires": requires(d.prerequisites()),
        };
        (d, row)
    });

    let starting_ships: Vec<Json> = STARTING_SHIPS.iter().map(|&(class, count)| list![format!("{class:?}"), count]).collect();

    let platform = |level: u8| -> Json {
        outpost_platform(level).iter().map(|&(kind, count)| list![format!("{kind:?}"), count]).collect::<Vec<Json>>().into()
    };

    let density_rows = DENSITIES
        .into_iter()
        .map(|d| (d, map! {"reserve_units": d.reserve_units(), "harvest_multiplier": d.harvest_multiplier()}));

    let rings: Vec<Json> = (1..=RINGS)
        .map(|ring| {
            map! {
                "ring": ring, "npc_class": format!("{:?}", npc_class(ring)),
                "npc_base_multiplier": npc_base_multiplier(ring), "npc_presence_pct": npc_presence_pct(ring),
                "npc_passive_share": npc_passive_share(ring), "npc_respawn_hours": npc_respawn_hours(ring),
                "ore_density_odds": ore_density_odds(ring),
            }
        })
        .collect();

    let ratings: Vec<Json> = (1..=MAX_THREAT)
        .map(|rating| {
            map! {
                "rating": rating, "threat_band": threat_band(rating),
                "derelict_ambush": derelict_ambush(rating), "relic_chance": relic_chance(rating),
                "chart_value": chart_value(rating, &pace(FIND_ANCHOR)),
            }
        })
        .collect();

    let arms = |tech: ResearchType, level: u8| Refit::of(&researched(tech, level)).arms();
    let share = |per_mille: u16| f64::from(per_mille) / 1000.0;
    let (b_top, r_top) = (MAX_BUILDING_LEVEL, MAX_RESEARCH_LEVEL);
    let p1 = Pace::PERSISTENT;
    let by_level = map! {
        "mine_output_per_hour": Json::keyed(MINES.map(|b| (b, at_levels(b_top, |l| mine_output_per_hour(b, l))))),
        "energy_needed": Json::keyed(MINES.map(|b| (b, at_levels(b_top, |l| energy_needed_by(b, l))))),
        "solar_plant_energy": at_levels(b_top, solar_plant_energy),
        "fusion_energy_by_energy_technology": at_levels(r_top, |tech| at_levels(b_top, |l| fusion_energy(l, tech))),
        "fusion_burn_per_hour": at_levels(b_top, fusion_burn_per_hour),
        "storage_cap": at_levels(b_top, |l| storage_cap_at(l, &p1)),
        "storage_protected": at_levels(b_top, |l| storage_protected_at(l, &p1)),
        "tank_modifier": at_levels(b_top, tank_modifier),
        "fuel_rate_modifier": at_levels(r_top, |level| fuel_rate_modifier(level, &RelicBonus::NONE)),
        "harvest_yield": at_levels(r_top, harvest_yield),
        "recall_damage_reduction": at_levels(r_top, recall_damage_reduction),
        "sensor_range": at_levels(r_top, sensor_range),
        "hull_modifier": at_levels(r_top, |l| share(arms(ResearchType::ArmourTechnology, l).hull)),
        "shield_modifier": at_levels(r_top, |l| share(arms(ResearchType::ShieldingTechnology, l).shield)),
        "weapon_modifier": at_levels(r_top, |l| share(arms(ResearchType::WeaponsTechnology, l).attack)),
        "building_slots": at_levels(r_top, |l| building_slots(&researched(ResearchType::ComputerTechnology, l))),
        "outpost_platform": at_levels(OUTPOST_MAX_LEVEL, platform),
    };

    let classes = [PaceClass::Economy, PaceClass::Attention, PaceClass::Finds, PaceClass::RealTime];
    map! {
        "constants": constants,
        "satellite_energy": satellite_energy(),
        "synthesizer_factor": synthesizer_factor(),
        "starting_ships": starting_ships,
        "resource_weight": Json::Map(ResourceKind::ALL.into_iter().map(|k| (k.label().to_string(), k.weight().into())).collect()),
        "storage_building": Json::Map(ResourceKind::ALL.into_iter().map(|k| (k.label().to_string(), format!("{:?}", storage_building(k)).into())).collect()),
        "zone_radius": map! {"InnerRing": Zone::INNER_RING_RADIUS, "OuterRing": Zone::OUTER_RING_RADIUS},
        "pace_exponent": Json::keyed(classes.into_iter().map(|c| (c, c.exponent().into()))),
        "presets": Json::Map(PRESETS.iter().map(|p| (p.name.to_string(), p.pace.into())).collect()),
        "buildings": Json::keyed(building_rows),
        "research": Json::keyed(research_rows),
        "ships": Json::keyed(ship_rows),
        "defences": Json::keyed(defence_rows),
        "densities": Json::keyed(density_rows),
        "by_level": by_level,
        "ratings": ratings,
        "rings": rings,
    }
}

/// What the Rust formulas give where the simulation computes for itself.
fn tables() -> Json {
    use BuildingType as B;
    use ResearchType as R;
    let p1 = Pace::PERSISTENT;
    let none = built(&[]);
    let robotics_4 = built(&[(B::RoboticsFactory, 4)]);
    let nanite_1 = built(&[(B::RoboticsFactory, 10), (B::NaniteFactory, 1)]);
    let lab_5 = built(&[(B::ResearchLab, 5)]);
    let shipyard_5 = built(&[(B::Shipyard, 5)]);
    let shipyard_5_nanite_1 = built(&[(B::Shipyard, 5), (B::NaniteFactory, 1)]);

    let building_levels: Vec<Json> = BuildingType::ALL
        .into_iter()
        .flat_map(|b| (1..=MAX_BUILDING_LEVEL).map(move |level| (b, level)))
        .map(|(b, level)| {
            map! {
                "building": format!("{b:?}"), "level": level, "cost": building_cost(b, level),
                "ticks": building_time(b, level, &none, &RelicBonus::NONE, &p1),
                "ticks_robotics_4": building_time(b, level, &robotics_4, &RelicBonus::NONE, &p1),
                "ticks_robotics_10_nanite_1": building_time(b, level, &nanite_1, &RelicBonus::NONE, &p1),
            }
        })
        .collect();

    let research_levels: Vec<Json> = ResearchType::ALL
        .into_iter()
        .flat_map(|t| (1..=research_max_level(t)).map(move |level| (t, level)))
        .map(|(t, level)| {
            map! {
                "tech": format!("{t:?}"), "level": level, "cost": research_cost(t, level),
                "ticks": research_time(t, level, &none, &RelicBonus::NONE, &p1), "ticks_lab_5": research_time(t, level, &lab_5, &RelicBonus::NONE, &p1),
                "energy": research_energy(t, level),
            }
        })
        .collect();

    let shipyard: Vec<Json> = items()
        .map(|item| {
            map! {
                "item": item_name(item), "ticks": item.unit_ticks(&none, &p1),
                "ticks_shipyard_5": item.unit_ticks(&shipyard_5, &p1),
                "ticks_shipyard_5_nanite_1": item.unit_ticks(&shipyard_5_nanite_1, &p1),
            }
        })
        .collect();

    let storage_caps: Vec<Json> = PACES
        .into_iter()
        .flat_map(|p| (0..=MAX_BUILDING_LEVEL).map(move |level| (p, level)))
        .map(|(p, level)| {
            map! {
                "pace": p, "level": level, "cap": storage_cap_at(level, &pace(p)),
                "protected": storage_protected_at(level, &pace(p)),
            }
        })
        .collect();

    // Homeworlds as (metal, crystal, deuterium, solar, fusion, Energy Technology, satellites).
    let homes: [(u8, u8, u8, u8, u8, u8, u32); 9] = [
        (1, 1, 0, 1, 0, 0, 0),
        (0, 0, 0, 0, 0, 0, 0),
        (5, 4, 3, 5, 0, 0, 0),
        (10, 9, 8, 14, 0, 0, 0),
        (10, 9, 8, 10, 0, 0, 0),
        (10, 9, 8, 10, 0, 0, 12),
        (15, 14, 12, 16, 6, 8, 0),
        (20, 20, 20, 20, 12, 12, 40),
        (20, 20, 20, 20, 0, 0, 0),
    ];
    let production: Vec<Json> = homes
        .into_iter()
        .flat_map(|home| PACES.into_iter().map(move |p| (home, p)))
        .map(|((metal, crystal, deuterium, solar, fusion, tech, satellites), p)| {
            let buildings = built(&[
                (B::MetalMine, metal),
                (B::CrystalMine, crystal),
                (B::DeuteriumSynthesizer, deuterium),
                (B::SolarPlant, solar),
                (B::FusionReactor, fusion),
            ]);
            let pace = pace(p);
            let power = home_power(&buildings, &researched(R::EnergyTechnology, tech), satellites, true, &pace);
            let mined = production_per_tick(&buildings, power.fraction(), &RelicBonus::NONE, &pace);
            map! {
                "pace": p, "mines": [metal, crystal, deuterium], "solar_plant": solar, "fusion_reactor": fusion,
                "energy_technology": tech, "satellites": satellites,
                "energy_made": power.made, "energy_needed": power.needed, "fraction": power.fraction(),
                "mined_per_hour": mined.scale(3600.0), "fusion_burn_per_hour": power.fusion_burn * 3600.0,
            }
        })
        .collect();

    // Ships as (class, Combustion, Impulse, Hyperspace Drive, Energy Technology, Deuterium Tank).
    let flights: [(ShipClass, u8, u8, u8, u8, u8); 22] = [
        (ShipClass::EspionageProbe, 0, 0, 0, 0, 0),
        (ShipClass::LightFighter, 0, 0, 0, 0, 0),
        (ShipClass::LightFighter, 9, 0, 0, 5, 3),
        (ShipClass::HeavyFighter, 0, 0, 0, 0, 0),
        (ShipClass::HeavyFighter, 0, 2, 0, 0, 0),
        (ShipClass::Cruiser, 0, 0, 0, 0, 0),
        (ShipClass::Cruiser, 0, 4, 0, 4, 0),
        (ShipClass::Cruiser, 0, 12, 0, 12, 8),
        (ShipClass::Battleship, 0, 0, 0, 0, 0),
        (ShipClass::Battleship, 0, 0, 4, 0, 0),
        (ShipClass::Battleship, 0, 0, 14, 0, 0),
        (ShipClass::Battlecruiser, 0, 0, 5, 0, 0),
        (ShipClass::LargeCargo, 6, 0, 0, 0, 0),
        (ShipClass::LargeCargo, 7, 0, 0, 0, 0),
        (ShipClass::SmallCargo, 0, 0, 0, 0, 0),
        (ShipClass::SmallCargo, 2, 0, 0, 0, 0),
        (ShipClass::SmallCargo, 2, 5, 0, 0, 0),
        (ShipClass::Destroyer, 0, 0, 6, 0, 0),
        (ShipClass::Bomber, 0, 6, 0, 0, 0),
        (ShipClass::Bomber, 0, 6, 8, 0, 0),
        (ShipClass::Recycler, 6, 0, 0, 0, 0),
        (ShipClass::Deathstar, 0, 0, 7, 0, 0),
    ];
    let flight: Vec<Json> = flights
        .into_iter()
        .map(|(class, combustion, impulse, hyperspace, energy, tank)| {
            let research = ResearchLevels {
                combustion_drive: combustion,
                impulse_drive: impulse,
                hyperspace_drive: hyperspace,
                energy_technology: energy,
                ..ResearchLevels::default()
            };
            let speed = ship_speed(class, &research);
            map! {
                "class": format!("{class:?}"), "combustion_drive": combustion, "impulse_drive": impulse,
                "hyperspace_drive": hyperspace, "energy_technology": energy, "deuterium_tank": tank,
                "speed": speed, "jump_ticks": jump_ticks(speed),
                "jump_fuel": ship_burn(class, &research) * fuel_rate_modifier(energy, &RelicBonus::NONE),
                "tank": ship_tank(class, &research) * tank_modifier(tank),
            }
        })
        .collect();

    let raid_power: Vec<Json> = [1.0f32, 10.0, 100.0, 500.0, 1000.0, 2000.0, 20_000.0]
        .into_iter()
        .map(|econ| map! {"econ": econ, "power": raid_power_base(econ)})
        .collect();

    let rings: Vec<Json> = (1..=RINGS)
        .map(|ring| {
            let (class, count, multiplier) = npc_composition(ring);
            map! {
                "ring": ring, "npc_power": npc_power(ring), "threat_rating": threat_rating(npc_power(ring)),
                "count": count, "stat_multiplier": multiplier,
                "group_power": ship_power(class) * multiplier * f32::from(count),
                "npc_jump_ticks": npc_jump_ticks(class, ring),
                "outpost_raid_power": outpost_raid_power(ring),
                "ring_mult": ring_mult(ring), "ore_regen_hours": ore_regen_hours(ring),
                "wreck_value": wreck_value(npc_power(ring)),
                "derelict_value": derelict_value(ring), "derelict_tier": derelict_tier(ring),
            }
        })
        .collect();

    let group_healing: Vec<Json> = (1..=RINGS)
        .flat_map(|ring| PACES.into_iter().map(move |p| (ring, p)))
        .map(|(ring, p)| {
            map! {"ring": ring, "pace": p, "step_ticks": npc_heal_step_ticks(ring, &pace(p)), "respawn_ticks": npc_respawn_ticks(ring, &pace(p))}
        })
        .collect();

    let exchange_spans: Vec<Json> = PACES
        .into_iter()
        .map(|p| {
            let pace = pace(p);
            let levels = built(&[(B::MetalMine, 10), (B::CrystalMine, 9), (B::DeuteriumSynthesizer, 8)]);
            let mines = production_per_tick(&levels, 1.0, &RelicBonus::NONE, &pace);
            map! {"pace": p, "mines": [10u8, 9, 8], "window": exchange_window(&pace), "span": exchange_span(&mines, &pace)}
        })
        .collect();

    let exchange_returns: Vec<Json> = [(0.0, 632.0, 11_900.0), (1000.0, 15_000.0, 11_900.0), (25_000.0, 300.0, 11_900.0), (0.0, 80.0, 0.0)]
        .into_iter()
        .map(|(pressure, value, span): (f64, f64, f64)| {
            map! {"pressure": pressure, "value": value, "span": span, "back": exchange_return(pressure, value, span)}
        })
        .collect();

    let find_worth = find_worth();
    let find_kept = find_kept();

    let outpost_levels: Vec<Json> = (0..=OUTPOST_MAX_LEVEL)
        .flat_map(|level| PACES.into_iter().map(move |p| (level, p)))
        .map(|(level, p)| {
            let pace = pace(p);
            map! {
                "level": level, "pace": p, "cost": outpost_cost(level),
                "ticks": outpost_build_ticks(level, &pace), "repair_ticks": outpost_repair_ticks(level, &pace),
                "repair_price": outpost_repair_price(level), "store_cap": outpost_store_cap(level, &pace),
                "platform": outpost_platform_power(level), "points": outpost_points(level),
                "platform_units": outpost_platform(level).iter().map(|&(kind, count)| list![format!("{kind:?}"), count]).collect::<Vec<Json>>(),
            }
        })
        .collect();

    let outpost_yield = |ring: u16, level: u8, p: f64| {
        let per_hour = DENSITIES
            .map(|d| outpost_yield_per_tick([d, Density::None, Density::None], ring, level, &pace(p)).metal * 3600.0);
        map! {"ring": ring, "level": level, "pace": p, "per_hour": per_hour}
    };
    let outpost_yields: Vec<Json> = (OUTPOST_MIN_RING..=OUTPOST_MAX_RING)
        .flat_map(|ring| (1..=OUTPOST_MAX_LEVEL).map(move |level| (ring, level, 1.0)))
        .chain([(12, 2, 10.0)])
        .map(|(ring, level, p)| outpost_yield(ring, level, p))
        .collect();

    let relays: Vec<Json> = (RELAY_MIN_RING..=RINGS)
        .flat_map(|ring| PACES.into_iter().map(move |p| (ring, p)))
        .map(|(ring, p)| {
            map! {
                "ring": ring, "pace": p, "cost": relay_cost(ring),
                "decay_ticks": pace(p).attention_ticks(RELAY_DECAY_TICKS as f64),
            }
        })
        .collect();

    let hubs: Vec<Json> = PACES
        .into_iter()
        .map(|p| map! {
            "pace": p, "period_ticks": hub_period_ticks(&pace(p)), "warn_ticks": hub_warn_ticks(&pace(p)),
        })
        .collect();

    let relics: Vec<Json> = PACES
        .into_iter()
        .map(|p| map! { "pace": p, "fade_ticks": pace(p).econ_ticks(RELIC_FADE_TICKS as f64) })
        .collect();
    let relic_rings: Vec<Json> = (RELAY_MIN_RING..=RELIC_DEEP_RING + 4)
        .map(|ring| map! { "ring": ring, "ceiling": relic_ceiling(ring) })
        .collect();

    map! {
        "building_levels": building_levels,
        "research_levels": research_levels,
        "shipyard": shipyard,
        "storage_caps": storage_caps,
        "production": production,
        "flight": flight,
        "raid_power": raid_power,
        "rings": rings,
        "group_healing": group_healing,
        "exchange_spans": exchange_spans,
        "exchange_returns": exchange_returns,
        "find_worth": find_worth,
        "find_kept": find_kept,
        "outpost_levels": outpost_levels,
        "outpost_yields": outpost_yields,
        "relays": relays,
        "hubs": hubs,
        "relics": relics,
        "relic_rings": relic_rings,
    }
}

type FindValue = Box<dyn Fn(&Pace) -> f64>;

/// What a find is worth in minutes of the gross production of an empire with
/// its mines at 10, 9 and 8, at every pace: the same, whatever the pace.
fn find_worth() -> Json {
    let levels = built(&[(BuildingType::MetalMine, 10), (BuildingType::CrystalMine, 9), (BuildingType::DeuteriumSynthesizer, 8)]);
    let sum = |r: Resources| r.metal + r.crystal + r.deuterium;
    let finds: [(&str, FindValue); 6] = [
        ("derelict_ring_4", Box::new(|p| derelict_value(4) * p.find_mult())),
        ("derelict_ring_12", Box::new(|p| derelict_value(12) * p.find_mult())),
        ("derelict_ring_30", Box::new(|p| derelict_value(30) * p.find_mult())),
        ("chart_threat_3", Box::new(|p| chart_value(3, p))),
        ("chart_threat_7", Box::new(|p| chart_value(7, p))),
        ("wreck_ring_12", Box::new(|p| wreck_value(npc_power(12)) * p.find_mult())),
    ];
    let rows = finds.iter().map(|(name, value)| {
        let minutes: Vec<f64> = FIND_PACES
            .iter()
            .map(|&p| {
                let pace = pace(p);
                value(&pace) / (sum(production_per_tick(&levels, 1.0, &RelicBonus::NONE, &pace)) * 60.0)
            })
            .collect();
        let (lo, hi) = minutes.iter().fold((f64::MAX, f64::MIN), |(lo, hi), &m| (lo.min(m), hi.max(m)));
        assert!((hi - lo) / hi < FIND_SPREAD_MAX, "{name} is worth {minutes:?} minutes of production");
        (name.to_string(), Json::from(minutes))
    });
    map! {"paces": FIND_PACES, "minutes": Json::Map(rows.collect())}
}

/// The share of a find the stockpile takes when it arrives into an empty
/// store, at each pace and at the storage level of all three buildings: a
/// derelict at its mean roll and at its best, and a chart.
fn find_kept() -> Json {
    let kept = |value: f64, level: u8, pace: &Pace| {
        let cap = storage_cap_at(level, pace);
        let split = split_loot(value);
        let taken = split.metal.min(cap) + split.crystal.min(cap) + split.deuterium.min(cap);
        taken / value
    };
    let rows = FIND_PACES.iter().flat_map(|&p| {
        let pace = pace(p);
        let finds: [(&str, f64); 6] = [
            ("derelict_ring_12", derelict_value(12) * pace.find_mult()),
            ("derelict_ring_12_best_roll", derelict_value(12) * f64::from(DERELICT_ROLL_MAX) * pace.find_mult()),
            ("derelict_ring_30", derelict_value(30) * pace.find_mult()),
            ("derelict_ring_30_best_roll", derelict_value(30) * f64::from(DERELICT_ROLL_MAX) * pace.find_mult()),
            ("chart_threat_3", chart_value(3, &pace)),
            ("chart_threat_7", chart_value(7, &pace)),
        ];
        finds.into_iter().flat_map(move |(name, value)| {
            CAP_LEVELS.into_iter().map(move |level| {
                map! {"pace": p, "find": name, "storage_level": level, "units": value, "cap": storage_cap_at(level, &pace), "kept": kept(value, level, &pace)}
            })
        })
    });
    rows.collect::<Vec<Json>>().into()
}

fn render() -> String {
    let document = map! {
        "generated": "by sim/tests/economy_numbers.rs from shared/src and sim/src/score.rs. Do not edit: \
                      UPDATE_FIXTURES=1 cargo test -p iac-sim --test economy_numbers rewrites it.",
        "game": game(),
        "tables": tables(),
    };
    let mut out = String::new();
    document.write(&mut out, 3, 0);
    out.push('\n');
    out
}

#[test]
fn economy_numbers_match() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(FILE);
    let rendered = render();
    if std::env::var("UPDATE_FIXTURES").is_ok_and(|v| !v.is_empty() && v != "0") {
        fs::write(&path, &rendered).unwrap();
        eprintln!("wrote {}", path.display());
        return;
    }

    let stored = fs::read_to_string(&path).unwrap_or_default();
    let first_change = rendered
        .lines()
        .zip(stored.lines().chain(std::iter::repeat("")))
        .enumerate()
        .find(|(_, (now, then))| now != then)
        .map(|(i, (now, then))| format!("line {}\n  Rust says: {now}\n  the file:  {then}", i + 1));
    assert!(
        rendered == stored,
        "{FILE} is out of date: an economy number changed in Rust.\n\
         Run UPDATE_FIXTURES=1 cargo test -p iac-sim --test economy_numbers, then python3 tools/check_sim_constants.py, and commit the file.\n\
         First change: {}",
        first_change.unwrap_or_else(|| "the file is longer than what Rust renders".into())
    );
}
