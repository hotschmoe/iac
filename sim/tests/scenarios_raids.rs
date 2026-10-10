// Raid cadence as players live it: how often forecasts arrive at a pace,
// how far ahead they warn and how they are sized.

#[allow(dead_code)]
mod common;

use common::*;
use iac_shared::constants::{RAID_MIN_PLAYER_AGE, RAID_POWER_ROLL_MAX, RAID_POWER_ROLL_MIN};
use iac_shared::protocol::EventKind;
use iac_shared::scaling;
use iac_sim::GameEngine;

struct Forecast {
    forecast_tick: u64,
    arrival_tick: u64,
    est_power: f32,
}

fn raided_world(pace: &str, players: usize) -> (GameEngine, Vec<u64>) {
    let mut engine = world(pace);
    let pids: Vec<u64> = (0..players).map(|i| join(&mut engine, &format!("Raided{i}")).0).collect();
    edit(&mut engine, |s| {
        for pid in &pids {
            s.players.get_mut(pid).unwrap().buildings.shipyard = 2;
        }
    });
    (engine, pids)
}

fn forecasts(engine: &mut GameEngine, pids: &[u64], ticks: u64) -> Vec<Vec<Forecast>> {
    let mut seen: Vec<Vec<Forecast>> = pids.iter().map(|_| Vec::new()).collect();
    for e in run(engine, ticks) {
        if let EventKind::RaidIncoming(r) = e.kind
            && let Some(i) = pids.iter().position(|p| *p == r.player_id)
        {
            seen[i].push(Forecast { forecast_tick: e.tick, arrival_tick: r.arrival_tick, est_power: r.est_power });
        }
    }
    seen
}

#[test]
fn blitz_players_see_sized_raids_within_the_hour_and_never_before_they_are_old_enough() {
    let (mut engine, pids) = raided_world("blitz", 12);
    let base = {
        let p = &engine.players[&pids[0]];
        scaling::raid_power_base(scaling::econ_points(&p.buildings, &p.research))
    };
    let warning = engine.pace().raid_warning_ticks();
    let min_age = engine.pace().attention_ticks(RAID_MIN_PLAYER_AGE as f64);

    let seen = forecasts(&mut engine, &pids, 3600);
    let counts: Vec<usize> = seen.iter().map(Vec::len).collect();
    let quiet = counts.iter().filter(|c| **c == 0).count();
    let mean = counts.iter().sum::<usize>() as f32 / counts.len() as f32;
    assert!(quiet <= 1, "{quiet} of 12 blitz hours had no raid: {counts:?}");
    assert!((1.2..=3.0).contains(&mean), "mean {mean} raids per blitz hour: {counts:?}");

    for f in seen.iter().flatten() {
        assert!(f.forecast_tick >= min_age, "forecast at {} before the empire was {min_age} ticks old", f.forecast_tick);
        assert_eq!(f.arrival_tick - f.forecast_tick, warning);
        assert!(
            (base * RAID_POWER_ROLL_MIN..=base * RAID_POWER_ROLL_MAX).contains(&f.est_power),
            "{} vs base {base}",
            f.est_power
        );
    }
}

#[test]
fn season_players_see_about_three_raids_a_day() {
    let (mut engine, pids) = raided_world("season", 6);
    let seen = forecasts(&mut engine, &pids, 24 * 3600);
    let mean = seen.iter().map(Vec::len).sum::<usize>() as f32 / pids.len() as f32;
    assert!((1.8..=4.5).contains(&mean), "mean {mean} raids per 24 h of season");
}
