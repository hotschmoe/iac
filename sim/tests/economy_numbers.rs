// The economy's numbers as data, for docs/design/economy/sim.py.
//
// Rust is the only place these numbers are written. This test renders them
// into docs/design/economy/numbers.json and fails when the file in the
// repository is not what it renders. Under "game" is what the simulation
// reads instead of declaring: the constants under their Rust names, the cost
// curves, the ships, and the curves it only looks up at whole rings, ratings
// and levels. Under "tables" is what the Rust formulas give over the ranges
// the simulation computes for itself, which tools/check_sim_constants.py
// holds its Python formulas to.
//
//   cargo test -p iac-sim --test economy_numbers                     # verify
//   UPDATE_FIXTURES=1 cargo test -p iac-sim --test economy_numbers   # regenerate

use std::fs;
use std::path::Path;

use iac_shared::constants::*;
use iac_shared::pace::{CAP_EXPONENT_PAST_KNEE, CAP_KNEE, PRESETS, Pace, PaceClass};
use iac_shared::scaling::*;
use iac_sim::score::{
    BOARD_VALUE_SHARE, CHART_BASE_POINTS, CHART_THREAT_POINTS, DEFENCE_SHARE, DISCOVERY_POINTS, EXTRA_CAP,
    FULL_PAY_RATIO, KILL_SHARE, MIN_PAY, THREAT_POINTS_EXPONENT, board_points, chart_points, kill_pay_factor,
};

const FILE: &str = "docs/design/economy/numbers.json";
const RINGS: u16 = 60;
const PACES: [f64; 3] = [1.0, 10.0, 600.0];
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
atoms!(f32, f64, u8, u16, u64, usize, &str, String);

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

fn buildings() -> impl Iterator<Item = BuildingType> {
    (0..BuildingType::COUNT).filter_map(BuildingType::from_usize)
}

fn pace(value: f64) -> Pace {
    Pace::new(value).unwrap()
}

fn requirement(kind: &str, name: impl std::fmt::Debug, level: u8) -> Json {
    list![kind, format!("{name:?}"), level]
}

/// What `value` gives at every level from 0 to `max`.
fn at_levels<T: Into<Json>>(max: u8, value: impl Fn(u8) -> T) -> Json {
    Json::List((0..=max).map(|level| value(level).into()).collect())
}

/// What the simulation reads in place of constants of its own.
fn game() -> Json {
    let constants = named![
        PRODUCTION_PER_HOUR_METAL, PRODUCTION_PER_HOUR_CRYSTAL, PRODUCTION_PER_HOUR_DEUTERIUM, PRODUCTION_GROWTH,
        BUILD_RATE, RESEARCH_RATE, SHIP_RATE, FABRICATOR_SPEED, LAB_SPEED, SHIPYARD_SPEED,
        MAX_BUILDING_LEVEL, QUEUE_WAITING,
        STARTING_RESOURCES, STARTING_SCOUTS, HOMEWORLD_MIN_DIST, HOMEWORLD_MAX_DIST,
        STORAGE_BASE, STORAGE_GROWTH, CAP_KNEE, CAP_EXPONENT_PAST_KNEE,
        FUEL_RATE_PER_MASS, FUEL_DEUT_PER_UNIT,
        NPC_POWER_BASE, NPC_POWER_GROWTH, NPC_MAX_SHIPS, NPC_STAT_MULT_MAX, MAX_THREAT,
        WRECK_VALUE_K, WRECK_VALUE_EXPONENT, LOOT_SPLIT,
        ORE_RING_GROWTH, ORE_RING_FIRST, ORE_RING_LAST, ORE_REGEN_BASE_HOURS, ORE_REGEN_HOURS_PER_RING,
        DERELICT_SPAWN_PCT, DERELICT_RESPAWN_TICKS, DERELICT_VALUE_BASE, DERELICT_VALUE_GROWTH, DERELICT_VALUE_RING,
        AMBUSH_GUARD_SHARE,
        RAID_MIN_PLAYER_AGE, RAID_POWER_FLOOR, RAID_POWER_PER_POINT, RAID_POWER_EXPONENT, RAID_POWER_ROLL_MAX,
        DEFENCE_GRID_BONUS,
        EXTRA_CAP, KILL_SHARE, FULL_PAY_RATIO, MIN_PAY, BOARD_VALUE_SHARE, DISCOVERY_POINTS,
        CHART_BASE_POINTS, CHART_THREAT_POINTS, THREAT_POINTS_EXPONENT, DEFENCE_SHARE,
        RELIC_POINTS, FARM_DECAY, FARM_HALF_LIFE_RESPAWNS,
        EXCHANGE_FEE_MIN, EXCHANGE_FEE_MAX, EXCHANGE_METER_SHARE, EXCHANGE_WINDOW_TICKS,
        OUTPOST_MIN_RING, OUTPOST_MAX_RING, OUTPOST_SPACING, OUTPOST_MAX_LEVEL,
        OUTPOST_COST, OUTPOST_GROWTH, OUTPOST_BUILD_RATE, OUTPOST_STORE_BASE, OUTPOST_YIELD_PER_HOUR,
        OUTPOST_PLATFORM_POWER, OUTPOST_RAID_FACTOR, OUTPOST_RAID_LOSS, OUTPOST_REPAIR_SHARE,
    ];

    let start = BuildingLevels::default();
    let building_rows = buildings().map(|b| {
        let (cost, growth) = building_cost_curve(b);
        let requires: Vec<Json> =
            building_prerequisites(b).into_iter().map(|p| requirement("building", p.building, p.level)).collect();
        (b, map! {"cost": cost, "growth": growth, "requires": requires, "starts_at": start.get(b)})
    });

    let research_rows = ResearchType::ALL.into_iter().map(|t| {
        let (cost, growth) = research_cost_curve(t);
        let requires: Vec<Json> = research_prerequisites(t)
            .into_iter()
            .flatten()
            .map(|p| match p {
                ResearchPrereqKind::Building(b) => requirement("building", b.building, b.level),
                ResearchPrereqKind::Research { tech, level } => requirement("research", tech, level),
            })
            .collect();
        (t, map! {"cost": cost, "growth": growth, "max_level": research_max_level(t), "requires": requires})
    });

    let ship_rows = ShipClass::ALL.into_iter().map(|c| {
        let s = c.base_stats();
        let stats = map! {
            "cost": c.build_cost(), "hull": s.hull, "shield": s.shield, "weapon": s.weapon,
            "speed": s.speed, "cargo": s.cargo, "fuel": s.fuel,
        };
        (c, stats)
    });

    let defence_rows = DefenceKind::ALL.into_iter().map(|d| {
        let requires: Vec<Json> = d
            .prerequisites()
            .iter()
            .map(|(p, level)| match p {
                PrereqRef::Building(b) => requirement("building", b, *level),
                PrereqRef::Research(t) => requirement("research", t, *level),
            })
            .collect();
        (d, map! {"cost": d.build_cost(), "power": d.power(), "requires": requires})
    });

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
            }
        })
        .collect();

    let researched = |tech: ResearchType, level: u8| {
        let mut research = ResearchLevels::default();
        research.set(tech, level);
        research
    };
    let unit = ShipStats { hull: 1.0, shield: 1.0, weapon: 1.0, speed: 0, cargo: 0, fuel: 0 };
    let stats = |tech: ResearchType, level: u8| apply_research_to_stats(unit, &researched(tech, level));
    let top = research_max_level;
    let by_level = map! {
        "defense_grid_scout_units": at_levels(MAX_BUILDING_LEVEL, defense_grid_scout_units),
        "fuel_depot_modifier": at_levels(MAX_BUILDING_LEVEL, fuel_depot_modifier),
        "fuel_rate_modifier": at_levels(top(ResearchType::FuelEfficiency), fuel_rate_modifier),
        "fuel_capacity_modifier": at_levels(top(ResearchType::ExtendedFuelTanks), fuel_capacity_modifier),
        "harvest_yield": at_levels(top(ResearchType::HarvestingEfficiency), harvest_yield),
        "hull_modifier": at_levels(top(ResearchType::ReinforcedHulls), |l| stats(ResearchType::ReinforcedHulls, l).hull),
        "shield_modifier": at_levels(top(ResearchType::AdvancedShields), |l| stats(ResearchType::AdvancedShields, l).shield),
        "weapon_modifier": at_levels(top(ResearchType::WeaponsResearch), |l| stats(ResearchType::WeaponsResearch, l).weapon),
        "building_slots": at_levels(top(ResearchType::ModularFabrication), |l| building_slots(&researched(ResearchType::ModularFabrication, l))),
    };

    let classes = [PaceClass::Economy, PaceClass::Attention, PaceClass::Finds, PaceClass::RealTime];
    map! {
        "constants": constants,
        "resource_weight": Json::Map(ResourceKind::ALL.into_iter().map(|k| (k.label().to_string(), k.weight().into())).collect()),
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
    let p1 = Pace::PERSISTENT;

    let building_levels: Vec<Json> = buildings()
        .flat_map(|b| (1..=MAX_BUILDING_LEVEL).map(move |level| (b, level)))
        .map(|(b, level)| {
            map! {
                "building": format!("{b:?}"), "level": level, "cost": building_cost(b, level),
                "ticks": building_time(b, level, 0, &p1), "ticks_fabricator_4": building_time(b, level, 4, &p1),
                "per_hour": production_per_tick(b, level, &p1) * 3600.0,
            }
        })
        .collect();

    let research_levels: Vec<Json> = ResearchType::ALL
        .into_iter()
        .flat_map(|t| (1..=research_max_level(t)).map(move |level| (t, level)))
        .map(|(t, level)| {
            map! {
                "tech": format!("{t:?}"), "level": level, "cost": research_cost(t, level),
                "ticks": research_time(t, level, 0, &p1), "ticks_lab_5": research_time(t, level, 5, &p1),
            }
        })
        .collect();

    let items = ShipClass::ALL.into_iter().map(ShipyardItem::Ship).chain(DefenceKind::ALL.into_iter().map(ShipyardItem::Defence));
    let shipyard: Vec<Json> = items
        .map(|item| {
            let name = match item {
                ShipyardItem::Ship(c) => format!("{c:?}"),
                ShipyardItem::Defence(d) => format!("{d:?}"),
            };
            map! {"item": name, "ticks": item.unit_ticks(0, &p1), "ticks_shipyard_5": item.unit_ticks(5, &p1)}
        })
        .collect();

    let storage_caps: Vec<Json> = PACES
        .into_iter()
        .flat_map(|p| (0..=14u8).map(move |vault| (p, vault)))
        .map(|(p, vault)| map! {"pace": p, "vault": vault, "cap": storage_cap(vault, &pace(p))})
        .collect();

    let raid_power: Vec<Json> = [1.0f32, 10.0, 100.0, 500.0, 1000.0, 2000.0]
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
                "outpost_raid_power": outpost_raid_power(ring),
                "ring_mult": ring_mult(ring), "ore_regen_hours": ore_regen_hours(ring),
                "wreck_value": wreck_value(npc_power(ring)),
                "derelict_value": derelict_value(ring), "derelict_tier": derelict_tier(ring),
            }
        })
        .collect();

    let ratings: Vec<Json> = (1..=MAX_THREAT)
        .map(|rating| {
            let discovery = board_points(Resources::default(), rating);
            map! {"rating": rating, "chart_points": chart_points(rating), "discovery_points": discovery}
        })
        .collect();

    let kill_pay: Vec<Json> = [0.5f32, 1.0, 4.0, 8.0, 40.0, 200.0]
        .into_iter()
        .map(|ratio| map! {"ratio": ratio, "pay": kill_pay_factor(ratio, 1.0)})
        .collect();

    let exchange_spans: Vec<Json> = PACES
        .into_iter()
        .map(|p| {
            let pace = pace(p);
            let mines = Resources {
                metal: production_per_tick(BuildingType::MetalMine, 10, &pace),
                crystal: production_per_tick(BuildingType::CrystalMine, 9, &pace),
                deuterium: production_per_tick(BuildingType::DeuteriumSynthesizer, 8, &pace),
            };
            map! {"pace": p, "mines": [10u8, 9, 8], "window": exchange_window(&pace), "span": exchange_span(&mines, &pace)}
        })
        .collect();

    let exchange_returns: Vec<Json> = [(0.0, 632.0, 4185.0), (1000.0, 5000.0, 4185.0), (9000.0, 300.0, 4185.0), (0.0, 80.0, 0.0)]
        .into_iter()
        .map(|(pressure, value, span): (f64, f64, f64)| {
            map! {"pressure": pressure, "value": value, "span": span, "back": exchange_return(pressure, value, span)}
        })
        .collect();

    let outpost_levels: Vec<Json> = (0..=OUTPOST_MAX_LEVEL)
        .flat_map(|level| PACES.into_iter().map(move |p| (level, p)))
        .map(|(level, p)| {
            let pace = pace(p);
            map! {
                "level": level, "pace": p, "cost": outpost_cost(level),
                "ticks": outpost_build_ticks(level, &pace), "repair_ticks": outpost_repair_ticks(level, &pace),
                "repair_price": outpost_repair_price(level), "store_cap": outpost_store_cap(level, &pace),
                "platform": outpost_platform_power(level), "points": outpost_points(level),
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

    map! {
        "building_levels": building_levels,
        "research_levels": research_levels,
        "shipyard": shipyard,
        "storage_caps": storage_caps,
        "raid_power": raid_power,
        "rings": rings,
        "ratings": ratings,
        "kill_pay": kill_pay,
        "exchange_spans": exchange_spans,
        "exchange_returns": exchange_returns,
        "outpost_levels": outpost_levels,
        "outpost_yields": outpost_yields,
    }
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
