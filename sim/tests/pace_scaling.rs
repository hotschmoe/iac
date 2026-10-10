// The same build order at two paces: waiting time shrinks in the ratio the
// pace predicts, because every build, research and ship timer is divided by it.

#[allow(dead_code)]
mod common;

use common::*;
use iac_shared::constants::{Resources, ShipClass};
use iac_shared::protocol::Command;
use iac_shared::scaling::{BuildingType, ResearchType};

const ORDERS: [Command; 6] = [
    Command::Build { building_type: BuildingType::MetalMine, reserve: false },
    Command::Build { building_type: BuildingType::CrystalMine, reserve: false },
    Command::Build { building_type: BuildingType::Shipyard, reserve: false },
    Command::Build { building_type: BuildingType::ResearchLab, reserve: false },
    Command::Research { tech: ResearchType::CorvetteTech, reserve: false },
    Command::BuildShip { ship_class: ShipClass::Corvette, count: 2, reserve: false },
];

fn ticks_to_finish_the_build_order(pace: &str) -> u64 {
    let mut e = Empire::join(world(pace), "Planner");
    let pid = e.pid;
    edit(&mut e.engine, |s| s.players.get_mut(&pid).unwrap().resources = Resources { metal: 9_000.0, crystal: 9_000.0, deuterium: 9_000.0 });
    let mut next = 0;
    for _ in 0..60_000 {
        if next < ORDERS.len() && e.order(ORDERS[next].clone()).is_ok() {
            next += 1;
        }
        e.run(1);
        let home = e.state().homeworld;
        let idle = home.build_queue.is_empty()
            && home.build_pending.is_empty()
            && home.research_active.is_none()
            && home.research_pending.is_empty()
            && home.shipyard_queue.is_none()
            && home.shipyard_pending.is_empty();
        if next == ORDERS.len() && idle {
            return e.engine.current_tick();
        }
    }
    panic!("build order did not finish at pace {pace}: {next} orders issued");
}

#[test]
fn the_same_build_order_finishes_in_the_ratio_the_pace_predicts() {
    let slow = ticks_to_finish_the_build_order("2");
    let fast = ticks_to_finish_the_build_order("20");
    let ratio = slow as f64 / fast as f64;
    assert!((9.0..=11.0).contains(&ratio), "{slow} ticks at pace 2 against {fast} at pace 20: ratio {ratio}");
}
