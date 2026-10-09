// The engine is a pure function of (world seed, pace, command script).
// These tests play a busy scripted world twice and compare whole-state
// hashes, then stop one run midway, round-trip it through JSON and compare
// again.

use iac_shared::constants::{ShipClass, DEFAULT_WORLD_SEED};
use iac_shared::pace::Pace;
use iac_shared::protocol::{Command, EventKind, HarvestResource, QueueType};
use iac_shared::scaling::{BuildingType, ResearchType};
use iac_sim::engine::template_npc_id;
use iac_sim::{GameEngine, Snapshot};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

const TICKS: u64 = 10_000;
const PLAYERS: usize = 4;

#[derive(Default)]
struct Seen {
    combats: u32,
    raids: u32,
    commands_ok: u32,
}

/// A deterministic function of the tick and the visible world: no state of its own.
fn script(engine: &mut GameEngine, pids: &[u64], seen: &mut Seen) {
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
        if let Some(cmd) = cmd
            && engine.execute(pid, cmd).error.is_none()
        {
            seen.commands_ok += 1;
        }
        // Orders that cannot be refused for lack of a target keep the queues busy.
        if tick.is_multiple_of(40) {
            let _ = engine.execute(pid, Command::CancelBuild { queue_type: QueueType::Building, id: 0 });
        }
    }
}

fn new_world(seed: u64) -> (GameEngine, Vec<u64>) {
    let mut engine = GameEngine::new(seed, Pace::parse("blitz").unwrap());
    let pids = (0..PLAYERS)
        .map(|i| engine.authenticate(&format!("Pilot{i}"), None, i % 2 == 0, || "x".repeat(64)).unwrap().player_id)
        .collect();
    (engine, pids)
}

fn advance(engine: &mut GameEngine, pids: &[u64], seen: &mut Seen, ticks: u64) {
    for _ in 0..ticks {
        script(engine, pids, seen);
        engine.tick().unwrap();
        for event in engine.drain_events() {
            match event.kind {
                EventKind::CombatStarted(_) => seen.combats += 1,
                EventKind::RaidResolved(_) => seen.raids += 1,
                _ => {}
            }
        }
        engine.persist_dirty_state();
    }
}

fn hash(engine: &GameEngine) -> u64 {
    engine.snapshot().hash()
}

#[test]
fn same_seed_and_script_give_identical_state() {
    let (mut a, pids_a) = new_world(DEFAULT_WORLD_SEED);
    let (mut b, pids_b) = new_world(DEFAULT_WORLD_SEED);
    assert_eq!(pids_a, pids_b);
    let (mut seen_a, mut seen_b) = (Seen::default(), Seen::default());

    for _ in 0..TICKS / 1000 {
        advance(&mut a, &pids_a, &mut seen_a, 1000);
        advance(&mut b, &pids_b, &mut seen_b, 1000);
        assert_eq!(hash(&a), hash(&b), "states diverged by tick {}", a.current_tick());
    }
    assert_eq!(a.current_tick(), TICKS);
    assert!(seen_a.commands_ok > 500, "the script must actually play: {} commands accepted", seen_a.commands_ok);
    assert!(seen_a.combats > 0 && seen_a.raids > 0, "combat {} raids {}: the world must be busy", seen_a.combats, seen_a.raids);

    let (mut other, other_pids) = new_world(DEFAULT_WORLD_SEED ^ 1);
    advance(&mut other, &other_pids, &mut Seen::default(), 1000);
    let (mut again, again_pids) = new_world(DEFAULT_WORLD_SEED);
    advance(&mut again, &again_pids, &mut Seen::default(), 1000);
    assert_ne!(hash(&other), hash(&again), "a different seed must give a different world");
}

#[test]
fn snapshot_and_restore_midway_continues_identically() {
    let (mut straight, pids) = new_world(DEFAULT_WORLD_SEED);
    let mut seen = Seen::default();
    let (mut paused, _) = new_world(DEFAULT_WORLD_SEED);
    let mut seen_paused = Seen::default();

    advance(&mut paused, &pids, &mut seen_paused, TICKS / 2);
    let json = serde_json::to_string(&paused.snapshot()).unwrap();
    let before = hash(&paused);
    let snapshot: Snapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(snapshot.hash(), before, "a snapshot survives its JSON round trip");
    let mut resumed = GameEngine::restore(snapshot);
    assert_eq!(hash(&resumed), before, "restore rebuilds the same state");

    advance(&mut straight, &pids, &mut seen, TICKS / 2);
    for _ in 0..TICKS / 2 / 1000 {
        advance(&mut straight, &pids, &mut seen, 1000);
        advance(&mut resumed, &pids, &mut seen_paused, 1000);
        assert_eq!(hash(&straight), hash(&resumed), "resumed run diverged by tick {}", straight.current_tick());
    }
    assert_eq!(resumed.current_tick(), TICKS);
}
