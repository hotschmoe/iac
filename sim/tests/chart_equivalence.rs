// The chart a player is sent must not depend on how the engine maintains it.
// A scripted world is played for TICKS ticks; every tick each player's
// `tick_update` sector updates (and, every FULL_STATE_EVERY ticks, the
// `full_state` chart) are hashed together and compared with the hashes the
// original recompute-everything chart produced for the same world. The golden
// file is `data/chart_golden.txt`, one "tick hash" line per tick.

use iac_shared::constants::{ShipClass, DEFAULT_WORLD_SEED};
use iac_shared::pace::Pace;
use iac_shared::protocol::{Command, HarvestResource, QueueType};
use iac_shared::scaling::{BuildingType, ResearchType};
use iac_sim::engine::template_npc_id;
use iac_sim::{views, GameEngine};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use xxhash_rust::xxh3::xxh3_64;

const TICKS: u64 = 4_000;
const PLAYERS: usize = 4;
const FULL_STATE_EVERY: u64 = 50;

fn script(engine: &mut GameEngine, pids: &[u64]) {
    let tick = engine.current_tick() + 1;
    let mut rng = StdRng::seed_from_u64(tick.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    if tick.is_multiple_of(300) {
        for pid in pids {
            if let Some(p) = engine.players.get_mut(pid) {
                p.resources.metal = p.resources.metal.max(30_000.0);
                p.resources.crystal = p.resources.crystal.max(20_000.0);
                p.resources.deuterium = p.resources.deuterium.max(10_000.0);
            }
        }
    }
    if !tick.is_multiple_of(4) {
        return;
    }
    for &pid in pids {
        let mut fleets: Vec<u64> = engine.fleets.values().filter(|f| f.owner_id == pid).map(|f| f.id).collect();
        fleets.sort_unstable();
        let fleet_id = if fleets.is_empty() { 0 } else { fleets[rng.random_range(0..fleets.len())] };
        let here = engine.fleets.get(&fleet_id).map(|f| f.location);
        let cmd = match rng.random_range(0..12) {
            0..=3 => here.map(|at| {
                let around = engine.world_gen.connected_neighbors(at);
                let n = around.slice();
                Command::Move { fleet_id, target: n[rng.random_range(0..n.len())] }
            }),
            4 => Some(Command::Harvest { fleet_id, resource: HarvestResource::Auto }),
            5 => Some(Command::Scan { fleet_id }),
            6 => Some(Command::Build {
                building_type: BuildingType::from_usize(rng.random_range(0..BuildingType::COUNT)).unwrap(),
                reserve: rng.random_bool(0.3),
            }),
            7 => Some(Command::Research {
                tech: ResearchType::from_usize(rng.random_range(0..ResearchType::COUNT)).unwrap(),
                reserve: false,
            }),
            8 => Some(Command::BuildShip { ship_class: ShipClass::Scout, count: 2, reserve: false }),
            9 => here.map(|at| Command::Attack { fleet_id, target_fleet_id: template_npc_id(at) }),
            10 => Some(Command::Recall { fleet_id }),
            _ => Some(Command::CollectSalvage { fleet_id }),
        };
        if let Some(cmd) = cmd {
            let _ = engine.execute(pid, cmd);
        }
        if tick.is_multiple_of(40) {
            let _ = engine.execute(pid, Command::CancelBuild { queue_type: QueueType::Building, id: 0 });
        }
    }
}

fn run(mut each_tick: impl FnMut(u64, &[String])) {
    let mut engine = GameEngine::new(DEFAULT_WORLD_SEED, Pace::parse("blitz").unwrap());
    let pids: Vec<u64> = (0..PLAYERS)
        .map(|i| engine.authenticate(&format!("Pilot{i}"), None, i % 2 == 0, || "x".repeat(64)).unwrap().player_id)
        .collect();
    for _ in 0..TICKS {
        script(&mut engine, &pids);
        engine.tick().unwrap();
        let events = engine.drain_events();
        let tick = engine.current_tick();
        let mut out = Vec::new();
        for &pid in &pids {
            let mut updates = views::tick_update(&engine, pid, &events).sector_updates.unwrap_or_default();
            updates.sort_by_key(|s| s.location.to_key());
            out.push(serde_json::to_string(&updates).unwrap());
            if tick.is_multiple_of(FULL_STATE_EVERY) {
                out.push(serde_json::to_string(&views::full_state(&engine, pid).unwrap().known_sectors).unwrap());
            }
        }
        each_tick(tick, &out);
        engine.persist_dirty_state();
    }
}

#[test]
fn chart_matches_the_recorded_original() {
    let golden: Vec<&str> = include_str!("data/chart_golden.txt").lines().collect();
    assert_eq!(golden.len() as u64, TICKS);
    run(|tick, out| {
        let got = format!("{tick} {:08x}", xxh3_64(out.join("\n").as_bytes()) as u32);
        assert_eq!(got, golden[tick as usize - 1], "chart diverged from the original at tick {tick}");
    });
}
