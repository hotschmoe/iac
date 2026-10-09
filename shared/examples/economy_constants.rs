// Prints the economy numbers the Rust game uses as JSON, for
// tools/check_sim_constants.py to compare with docs/design/economy/sim.py.
//
//   cargo run -q -p iac-shared --example economy_constants

use serde_json::{json, Map, Value};

use iac_shared::constants::{defense_grid_scout_units, Density, ShipClass, FUEL_DEUT_PER_UNIT, FUEL_RATE_PER_MASS};
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
    out.insert("presets".into(), json!(PRESETS.iter().map(|p| (p.name.to_string(), json!(p.pace))).collect::<Map<_, _>>()));
    out.insert("raid_power_base".into(), json!(
        [1.0f32, 10.0, 100.0, 500.0, 1000.0, 2000.0].iter().map(|s| (s.to_string(), json!(raid_power_base(*s)))).collect::<Map<_, _>>()
    ));
    out.insert("grid_scout_units".into(), json!((0..=8).map(defense_grid_scout_units).collect::<Vec<_>>()));
    out.insert("resource_weight_unit".into(), json!(resource_weight(&iac_shared::Resources { metal: 1000.0, crystal: 1000.0, deuterium: 1000.0 })));
    out.insert("starting_resources".into(), triple(iac_shared::constants::STARTING_RESOURCES));
    out.insert("misc".into(), json!({
        "fuel_deut_per_unit": FUEL_DEUT_PER_UNIT,
        "fuel_rate_per_mass": FUEL_RATE_PER_MASS,
        "fuel_depot_per_level": fuel_depot_modifier(1) - 1.0,
        "cap_pace_exponent": Pace::new(100.0).unwrap().cap_mult().ln() / 100.0f32.ln(),
        "vault_growth": STORAGE_GROWTH,
        "build_rate": BUILD_RATE,
        "research_rate": RESEARCH_RATE,
        "ship_rate": SHIP_RATE,
        "fabricator_speed": FABRICATOR_SPEED,
        "lab_speed": LAB_SPEED,
        "shipyard_speed": SHIPYARD_SPEED,
        "production_growth": PRODUCTION_GROWTH,
        "queue_depth": QUEUE_DEPTH,
    }));
    println!("{}", serde_json::to_string(&Value::Object(out)).unwrap());
}
