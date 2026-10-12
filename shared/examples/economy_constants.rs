// Prints the economy numbers the Rust game uses as JSON, for
// tools/check_sim_constants.py to compare with docs/design/economy/sim.py.
//
//   cargo run -q -p iac-shared --example economy_constants

use serde_json::{json, Map, Value};

use iac_shared::constants::{
    defense_grid_scout_units, Density, ShipClass, FUEL_DEUT_PER_UNIT, FUEL_RATE_PER_MASS, OUTPOST_COST,
    OUTPOST_MAX_LEVEL, OUTPOST_MAX_RING, OUTPOST_MIN_RING, OUTPOST_STORE_BASE,
};
use iac_shared::maths;
use iac_shared::pace::{Pace, PRESETS};
use iac_shared::scaling::*;

const LEVELS: u8 = 20;

fn triple(r: iac_shared::Resources) -> Value {
    json!([r.metal, r.crystal, r.deuterium])
}

fn main() {
    let p1 = Pace::PERSISTENT;
    let mut out = Map::new();

    let mut buildings = Map::new();
    for b in (0..BuildingType::COUNT).filter_map(BuildingType::from_usize) {
        let prereq = building_prerequisites(b).map(|p| json!([format!("{:?}", p.building), p.level]));
        buildings.insert(format!("{b:?}"), json!({
            "cost": (1..=LEVELS).map(|l| triple(building_cost(b, l))).collect::<Vec<_>>(),
            "ticks": (1..=LEVELS).map(|l| building_time(b, l, 0, &p1)).collect::<Vec<_>>(),
            "ticks_fab4": (1..=LEVELS).map(|l| building_time(b, l, 4, &p1)).collect::<Vec<_>>(),
            "prereq": prereq,
            "per_hour": (1..=LEVELS).map(|l| production_per_tick(b, l, &p1) * 3600.0).collect::<Vec<_>>(),
        }));
    }
    out.insert("buildings".into(), Value::Object(buildings));

    let mut research = Map::new();
    for t in ResearchType::ALL {
        let prereqs: Vec<Value> = research_prerequisites(t).into_iter().flatten().map(|p| match p {
            ResearchPrereqKind::Building(b) => json!(["b", format!("{:?}", b.building), b.level]),
            ResearchPrereqKind::Research { tech, level } => json!(["r", format!("{tech:?}"), level]),
        }).collect();
        let max = research_max_level(t);
        research.insert(format!("{t:?}"), json!({
            "max": max,
            "cost": (1..=max).map(|l| triple(research_cost(t, l))).collect::<Vec<_>>(),
            "ticks": (1..=max).map(|l| research_time(t, l, 0, &p1)).collect::<Vec<_>>(),
            "ticks_lab5": (1..=max).map(|l| research_time(t, l, 5, &p1)).collect::<Vec<_>>(),
            "prereqs": prereqs,
        }));
    }
    out.insert("research".into(), Value::Object(research));

    let mut ships = Map::new();
    for c in ShipClass::ALL {
        let stats = c.base_stats();
        ships.insert(format!("{c:?}"), json!({
            "stats": [stats.hull, stats.shield, stats.weapon, stats.cargo, stats.fuel],
            "cost": triple(c.build_cost()),
            "ticks_sy0": ship_build_time(c, 0, &p1),
            "ticks_sy5": ship_build_time(c, 5, &p1),
        }));
    }
    out.insert("ships".into(), Value::Object(ships));

    let mut defences = Map::new();
    for d in DefenceKind::ALL {
        defences.insert(format!("{d:?}"), json!({
            "cost": triple(d.build_cost()),
            "power": d.power(),
            "ticks_sy0": defence_build_time(d, 0, &p1),
            "prereqs": d.prerequisites().iter().map(|(p, n)| match p {
                PrereqRef::Building(b) => json!(["b", format!("{b:?}"), n]),
                PrereqRef::Research(t) => json!(["r", format!("{t:?}"), n]),
            }).collect::<Vec<_>>(),
        }));
    }
    out.insert("defences".into(), Value::Object(defences));

    let mut storage = Map::new();
    for pace in [1.0, 10.0, 600.0] {
        let pace_v = Pace::new(pace).unwrap();
        storage.insert(pace.to_string(), json!(
            (0..=14).map(|v| triple(storage_cap(v, &pace_v))).collect::<Vec<_>>()
        ));
    }
    out.insert("storage_caps".into(), Value::Object(storage));

    out.insert("npc".into(), json!((1..=60u16).map(|d| {
        let (class, count, mult) = npc_composition(d);
        json!({
            "dist": d, "power": npc_power(d), "class": format!("{class:?}"), "count": count, "mult": mult,
            "group_power": ship_power(class) * mult * f32::from(count),
            "presence": npc_presence_pct(d) / 100.0, "passive": npc_passive_share(d),
            "respawn_h": npc_respawn_hours(d), "threat": threat_rating(npc_power(d)),
        })
    }).collect::<Vec<_>>()));
    let tile_totals: Vec<f32> = [Density::Sparse, Density::Moderate, Density::Rich, Density::Pristine].iter().map(|d| d.reserve_units()).collect();
    out.insert("ore".into(), json!({
        "tile_totals": tile_totals,
        "rings": (1..=60u16).map(|d| json!({
            "dist": d, "ring_mult": ring_mult(d), "regen_h": ore_regen_hours(d), "odds": ore_density_odds(d),
        })).collect::<Vec<_>>(),
        "harvest_yield_per_level": harvest_yield(1) - 1.0,
    }));
    out.insert("loot".into(), json!({
        "split": LOOT_SPLIT,
        "rings": (1..=60u16).map(|d| {
            let rating = threat_rating(npc_power(d));
            json!({
                "dist": d, "wreck": wreck_value(npc_power(d)), "derelict": derelict_value(d), "tier": derelict_tier(d),
                "ambush": derelict_ambush(rating), "recovery": recovery_chance(rating),
                "data_core": data_core_chance(rating), "relic": relic_chance(rating),
            })
        }).collect::<Vec<_>>(),
    }));
    out.insert("presets".into(), json!(PRESETS.iter().map(|p| (p.name.to_string(), json!(p.pace))).collect::<Map<_, _>>()));
    out.insert("raid_power_base".into(), json!(
        [1.0f32, 10.0, 100.0, 500.0, 1000.0, 2000.0].iter().map(|s| (s.to_string(), json!(raid_power_base(*s)))).collect::<Map<_, _>>()
    ));
    out.insert("grid_scout_units".into(), json!((0..=8).map(defense_grid_scout_units).collect::<Vec<_>>()));
    out.insert("resource_weight_unit".into(), json!(resource_weight(&iac_shared::Resources { metal: 1000.0, crystal: 1000.0, deuterium: 1000.0 })));
    out.insert("starting_resources".into(), triple(iac_shared::constants::STARTING_RESOURCES));
    let exchange_spans: Vec<Value> = [1.0, 10.0, 600.0]
        .iter()
        .map(|&p| {
            let pace = Pace::new(p).unwrap();
            let mines = iac_shared::Resources {
                metal: production_per_tick(BuildingType::MetalMine, 10, &pace),
                crystal: production_per_tick(BuildingType::CrystalMine, 9, &pace),
                deuterium: production_per_tick(BuildingType::DeuteriumSynthesizer, 8, &pace),
            };
            json!({"pace": p, "window": exchange_window(&pace), "span": exchange_span(&mines, &pace)})
        })
        .collect();
    let exchange_returns: Vec<Value> = [(0.0, 632.0, 4185.0), (1000.0, 5000.0, 4185.0), (9000.0, 300.0, 4185.0), (0.0, 80.0, 0.0)]
        .iter()
        .map(|&(pressure, value, span)| {
            json!({"pressure": pressure, "value": value, "span": span, "back": exchange_return(pressure, value, span)})
        })
        .collect();
    out.insert("exchange".into(), json!({"spans": exchange_spans, "returns": exchange_returns}));
    let paces = [1.0, 10.0, 600.0].map(|p| Pace::new(p).unwrap());
    let outpost_levels: Vec<Value> = (1..=OUTPOST_MAX_LEVEL)
        .map(|level| {
            json!({
                "level": level,
                "cost": triple(outpost_cost(level)),
                "ticks": paces.iter().map(|pace| outpost_build_ticks(level, pace)).collect::<Vec<_>>(),
                "store_cap": paces.iter().map(|pace| triple(outpost_store_cap(level, pace))).collect::<Vec<_>>(),
                "platform": outpost_platform_power(level),
                "repair_price": outpost_repair_price(level),
                "repair_ticks": paces.iter().map(|pace| outpost_repair_ticks(level, pace)).collect::<Vec<_>>(),
                "points": outpost_points(level),
            })
        })
        .collect();
    let densities = [Density::Sparse, Density::Moderate, Density::Rich, Density::Pristine];
    let outpost_yields: Vec<Value> = (OUTPOST_MIN_RING..=OUTPOST_MAX_RING)
        .flat_map(|ring| (1..=OUTPOST_MAX_LEVEL).map(move |level| (ring, level)))
        .map(|(ring, level)| {
            let per_hour: Vec<f32> = densities
                .iter()
                .map(|d| outpost_yield_per_tick([*d, Density::None, Density::None], ring, level, &p1).metal * 3600.0)
                .collect();
            json!({"ring": ring, "level": level, "per_hour": per_hour})
        })
        .collect();
    out.insert("outpost".into(), json!({
        "paces": [1.0, 10.0, 600.0],
        "cost": triple(OUTPOST_COST),
        "store_base": triple(OUTPOST_STORE_BASE),
        "store_cap_unbuilt": triple(outpost_store_cap(0, &p1)),
        "platform_unbuilt": outpost_platform_power(0),
        "levels": outpost_levels,
        "yields": outpost_yields,
        "season_yield_per_hour": outpost_yield_per_tick([Density::Rich; 3], 12, 2, &paces[1]).metal * 3600.0,
        "grade": site_grade([Density::Pristine, Density::Rich, Density::Sparse]),
        "raid_power": (OUTPOST_MIN_RING..=OUTPOST_MAX_RING).map(outpost_raid_power).collect::<Vec<_>>(),
    }));
    out.insert("misc".into(), json!({
        "fuel_deut_per_unit": FUEL_DEUT_PER_UNIT,
        "fuel_rate_per_mass": FUEL_RATE_PER_MASS,
        "fuel_depot_per_level": fuel_depot_modifier(1) - 1.0,
        "cargo_exponent": maths::ln(Pace::new(10000.0).unwrap().cargo_mult()) / maths::ln(10000.0f32),
        "vault_growth": STORAGE_GROWTH,
        "build_rate": BUILD_RATE,
        "research_rate": RESEARCH_RATE,
        "ship_rate": SHIP_RATE,
        "fabricator_speed": FABRICATOR_SPEED,
        "lab_speed": LAB_SPEED,
        "shipyard_speed": SHIPYARD_SPEED,
        "production_growth": PRODUCTION_GROWTH,
        "queue_waiting": QUEUE_WAITING,
        "finds_exponent": maths::ln(Pace::new(10000.0).unwrap().finds_mult()) / maths::ln(10000.0f32),
    }));
    println!("{}", serde_json::to_string(&Value::Object(out)).unwrap());
}
