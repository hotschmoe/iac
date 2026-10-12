// The engine is a pure function of (world seed, pace, command script).
// These tests play a busy scripted world twice and compare whole-state
// hashes, then stop one run midway, round-trip it through JSON and compare
// again.

#[allow(dead_code)]
mod common;

use common::Pilots;
use iac_shared::constants::DEFAULT_WORLD_SEED;
use iac_shared::pace::Pace;
use iac_shared::protocol::EventKind;
use iac_sim::{host, GameEngine, Snapshot};

const TICKS: u64 = 10_000;
const PLAYERS: usize = 4;

/// A world, the scripted pilots who play it, and what has happened in it.
struct Run {
    engine: GameEngine,
    pilots: Pilots,
    combats: u32,
    raids: u32,
}

impl Run {
    fn new(seed: u64) -> Run {
        let mut engine = GameEngine::new(seed, Pace::parse("blitz").unwrap());
        let pilots = Pilots::join(&mut engine, PLAYERS);
        Run { engine, pilots, combats: 0, raids: 0 }
    }

    fn advance(&mut self, ticks: u64) {
        for _ in 0..ticks {
            for event in host::step(&mut self.engine, &mut self.pilots, &mut (), |_, _| {}).unwrap() {
                match event.kind {
                    EventKind::CombatStarted(_) => self.combats += 1,
                    EventKind::RaidResolved(_) => self.raids += 1,
                    _ => {}
                }
            }
        }
    }

    fn hash(&self) -> u64 {
        self.engine.snapshot().hash()
    }
}

#[test]
fn same_seed_and_script_give_identical_state() {
    let (mut a, mut b) = (Run::new(DEFAULT_WORLD_SEED), Run::new(DEFAULT_WORLD_SEED));
    assert_eq!(a.pilots.pids, b.pilots.pids);

    for _ in 0..TICKS / 1000 {
        a.advance(1000);
        b.advance(1000);
        assert_eq!(a.hash(), b.hash(), "states diverged by tick {}", a.engine.current_tick());
    }
    assert_eq!(a.engine.current_tick(), TICKS);
    assert!(a.pilots.accepted > 500, "the script must actually play: {} commands accepted", a.pilots.accepted);
    assert!(a.combats > 0 && a.raids > 0, "combat {} raids {}: the world must be busy", a.combats, a.raids);

    let (mut other, mut again) = (Run::new(DEFAULT_WORLD_SEED ^ 1), Run::new(DEFAULT_WORLD_SEED));
    other.advance(1000);
    again.advance(1000);
    assert_ne!(other.hash(), again.hash(), "a different seed must give a different world");
}

#[test]
fn snapshot_and_restore_midway_continues_identically() {
    let (mut straight, mut paused) = (Run::new(DEFAULT_WORLD_SEED), Run::new(DEFAULT_WORLD_SEED));

    paused.advance(TICKS / 2);
    let json = serde_json::to_string(&paused.engine.snapshot()).unwrap();
    let before = paused.hash();
    let snapshot: Snapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(snapshot.hash(), before, "a snapshot survives its JSON round trip");
    let mut resumed = Run { engine: GameEngine::restore(snapshot), ..paused };
    assert_eq!(resumed.hash(), before, "restore rebuilds the same state");

    straight.advance(TICKS / 2);
    for _ in 0..TICKS / 2 / 1000 {
        straight.advance(1000);
        resumed.advance(1000);
        assert_eq!(straight.hash(), resumed.hash(), "resumed run diverged by tick {}", straight.engine.current_tick());
    }
    assert_eq!(resumed.engine.current_tick(), TICKS);
}
