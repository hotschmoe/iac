// The chart a player is sent must not depend on how the engine maintains it.
// A scripted world is played for TICKS ticks; every tick each player's
// `tick_update` sector updates (and, every FULL_STATE_EVERY ticks, the
// `full_state` chart) are hashed together and compared with the recording in
// `data/chart_golden.txt`, one "tick hash" line per tick.
//
// The first recording came from the original recompute-everything chart, and
// the upkeep that replaced it matched it tick for tick. World generation 3
// changed the world under the script, so the file was recorded again from
// that upkeep: it pins what the chart sends from here on and no longer
// compares two implementations. Record again only when the world or the
// protocol is meant to change, never to make a change to the chart pass:
//
//   UPDATE_CHART_GOLDEN=1 cargo test -p iac-sim --test chart_equivalence
//
// It was recorded once more when sectors gained their chart mark
// (`SectorState::chart`), after the engine with marks had reproduced the
// previous recording with the marks left out of the comparison.
//
// And again when a fleet that loses ships began to shed the cargo it can no
// longer carry: the recording held to tick 1010, and at tick 1011 a fleet
// destroyed at [1,6] with 34.9 units aboard now leaves them there as
// wreckage, which its owner's chart shows.
//
// And when `recall` began to be refused at the homeworld: that recording
// held to tick 111, where the script recalls a fleet harvesting at its own
// homeworld [-5,1]. The recall used to halt the harvest; refused, the fleet
// mines on and the sector's ore reads lower from tick 112. The script does
// the same at ticks 427 and 1615.
//
// And when amounts became 64-bit: every line changed, because an ore figure
// is written as itself (42.9) and no longer as the nearest `f32`
// (42.900001525878906). The two engines were played side by side with what
// this test hashes written out. They sent the same events on the same
// ticks, and the charts differed in three ways: ore figures in their last
// digits, `refills_in_s` by one second, and six times in the 4000 ticks a
// refilling tile reached its next level one tick apart.
//
// And when the catalogue became OGame's: the world under the script is
// another one. Its lairs hold Light Fighters, Cruisers and Battlecruisers,
// its tiles three times the ore, and the pilots fly two Light Fighters.
// Nothing was compared across that change; the file pins what this
// catalogue's chart sends.
//
// And when a fleet became a count for each class and a fight OGame's round:
// a hostile group is listed with the hull its ships hold and what one of
// them fights with, a group's ships take no ids from the world's counter,
// and every fight runs another course. Nothing was compared across that
// change either.

#[allow(dead_code)]
mod common;

use common::Pilots;
use iac_shared::constants::DEFAULT_WORLD_SEED;
use iac_shared::pace::Pace;
use iac_shared::protocol::GameEvent;
use iac_sim::{host, views, GameEngine};
use xxhash_rust::xxh3::xxh3_64;

const TICKS: u64 = 4_000;
const PLAYERS: usize = 4;
const FULL_STATE_EVERY: u64 = 50;

/// The scripted pilots are the cast of a hosted world, and the chart is read
/// where a host sends it: between the tick and the cast's look at it.
fn run(mut each_tick: impl FnMut(u64, &[String])) {
    let mut engine = GameEngine::new(DEFAULT_WORLD_SEED, Pace::parse("blitz").unwrap());
    let mut pilots = Pilots::join(&mut engine, PLAYERS);
    let pids = pilots.pids.clone();
    for _ in 0..TICKS {
        let frames = |engine: &GameEngine, events: &[GameEvent]| {
            let tick = engine.current_tick();
            let mut out = Vec::new();
            for &pid in &pids {
                let mut updates = views::tick_update(engine, pid, events).sector_updates.unwrap_or_default();
                updates.sort_by_key(|s| s.location.to_key());
                out.push(serde_json::to_string(&updates).unwrap());
                if tick.is_multiple_of(FULL_STATE_EVERY) {
                    out.push(serde_json::to_string(&views::full_state(engine, pid).unwrap().known_sectors).unwrap());
                }
            }
            each_tick(tick, &out);
        };
        host::step(&mut engine, &mut pilots, &mut (), frames).unwrap();
    }
}

fn line(tick: u64, out: &[String]) -> String {
    format!("{tick} {:08x}", xxh3_64(out.join("\n").as_bytes()) as u32)
}

#[test]
fn chart_matches_the_recording() {
    if std::env::var("UPDATE_CHART_GOLDEN").is_ok_and(|v| !v.is_empty() && v != "0") {
        let mut recording = String::new();
        run(|tick, out| recording.push_str(&(line(tick, out) + "\n")));
        std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/chart_golden.txt"), recording).unwrap();
        return;
    }
    let golden: Vec<&str> = include_str!("data/chart_golden.txt").lines().collect();
    assert_eq!(golden.len() as u64, TICKS);
    run(|tick, out| {
        assert_eq!(line(tick, out), golden[tick as usize - 1], "chart diverged from the recording at tick {tick}");
    });
}
