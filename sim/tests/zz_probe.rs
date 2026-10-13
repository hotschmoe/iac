#[allow(dead_code)]
mod common;

use common::*;
use iac_shared::constants::ShipClass;
use iac_shared::hex::{hex_ring, Hex};
use iac_shared::protocol::Command;
use iac_shared::scaling;
use iac_shared::world::NpcBehaviorType::Passive;
use iac_sim::engine::template_npc_id;

#[test]
fn probe_rings() {
    for ring in 1..=24u16 {
        let (class, count, mult) = scaling::npc_composition(ring);
        println!(
            "ring {ring:2}: power {:8.1} group {count} x {} at {mult:.2} (group power {:.1})",
            scaling::npc_power(ring),
            class.label(),
            scaling::npc_group_power(ring)
        );
    }
}

#[test]
fn probe_starter() {
    for ring in 3..=12u16 {
        let mut engine = world("persistent");
        let (pid, fid) = join(&mut engine, "Rookie");
        let Some(at) = hex_ring(Hex::ORIGIN, ring)
            .find(|at| engine.world_gen.generate_sector(*at).npc_template.is_some_and(|t| t.behavior != Passive))
        else {
            continue;
        };
        deploy(&mut engine, fid, at, None);
        let odds = engine.fleet_odds(&engine.fleets[&fid], at);
        let f = fights(&engine, pid, fid, at, 40);
        println!("starter ring {ring}: {:.2} {} won {}/{} kept {:.2}", odds.ratio, odds.label.label(), f.won, f.fought, f.hull_kept);
        let _ = (Command::Stop { fleet_id: fid }, template_npc_id(at));
    }
}

#[test]
fn probe_labels() {
    use ShipClass::*;
    for class in [LightFighter, HeavyFighter, Cruiser, Battleship] {
        for ring in [5u16, 7, 9, 10, 11, 13, 15, 17, 20] {
            for count in [1usize, 2, 3, 4, 5, 6, 8, 10, 12, 16, 20, 24, 32] {
                let mut engine = world("persistent");
                let (pid, fid) = join(&mut engine, "Gauge");
                let at = lair(&engine, ring, |_| true);
                muster(&mut engine, fid, &[(class, count)], at);
                let seen = engine.fleet_odds(&engine.fleets[&fid], at);
                let f = fights(&engine, pid, fid, at, 30);
                println!(
                    "{count:2} {:13} ring {ring:2}: {:5.2} {:10} won {:2}/{} kept {:.3}",
                    class.label(),
                    seen.ratio,
                    seen.label.label(),
                    f.won,
                    f.fought,
                    f.hull_kept
                );
            }
        }
    }
}
