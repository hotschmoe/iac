// Fleet handling over real distances: splitting and merging, fuel and the
// way home, the emergency reserve, and what standing orders refuse.

#[allow(dead_code)]
mod common;

use common::*;
use iac_shared::hex::Hex;
use iac_shared::constants::{Resources, STRANDED_RECOVERY_TICKS};
use iac_shared::protocol::{
    AlertEvent, AlertLevel, Command, ErrorCode, EventKind, PolicyParams, PolicyPreset, ServerMessage,
};
use iac_sim::engine::{fleet_power, FleetStatus};

fn alerts_since(e: &Empire, mark: usize) -> Vec<AlertEvent> {
    e.since(mark, |k| match k {
        EventKind::Alert(a) => Some(a.clone()),
        _ => None,
    })
}

fn fuel_of(e: &Empire, fid: u64) -> f32 {
    e.engine.fleets[&fid].fuel
}

fn hop_fuel(e: &Empire) -> f32 {
    e.fleet().jump_fuel
}

fn with_rival(name: &str) -> (Empire, u64, u64) {
    let mut engine = world("test");
    let (rival, rival_fleet) = join(&mut engine, "Rival");
    let (pid, fid) = join(&mut engine, name);
    (Empire { engine, pid, fid, log: Vec::new() }, rival, rival_fleet)
}

fn outpost(name: &str, hops: usize) -> (Empire, Vec<Hex>) {
    let mut e = Empire::join(world("test"), name);
    let home = e.engine.players[&e.pid].homeworld;
    let chain = outward_chain(&e.engine, home, hops);
    e.travel(chain[hops - 1]);
    (e, chain)
}

#[test]
fn splitting_and_merging_conserve_ships_fuel_and_cargo() {
    let (mut e, rival, rival_fleet) = with_rival("Fleeter");
    let (pid, fid) = (e.pid, e.fid);
    edit(&mut e.engine, |s| s.fleets.get_mut(&fid).unwrap().cargo = Resources { metal: 30.0, ..Resources::default() });
    let ships: Vec<u64> = e.fleet().ships.iter().map(|s| s.id).collect();
    let fuel_before = fuel_of(&e, fid);

    assert_eq!(e.order(Command::Split { fleet_id: fid, ship_ids: vec![] }), Err(ErrorCode::InvalidTarget));
    assert_eq!(e.order(Command::Split { fleet_id: fid, ship_ids: ships.clone() }), Err(ErrorCode::InvalidTarget), "one ship must stay");
    assert_eq!(e.order(Command::Split { fleet_id: fid, ship_ids: vec![999_999] }), Err(ErrorCode::InvalidTarget));
    let outcome = e.engine.execute(rival, Command::Split { fleet_id: fid, ship_ids: vec![ships[0]] });
    assert_eq!(outcome.error.map(|(code, _)| code), Some(ErrorCode::FleetNotFound));
    assert_eq!(e.order(Command::Merge { fleet_id: fid, other_fleet_id: fid }), Err(ErrorCode::InvalidTarget));
    assert_eq!(e.order(Command::Merge { fleet_id: fid, other_fleet_id: rival_fleet }), Err(ErrorCode::FleetNotFound));

    e.order(Command::Split { fleet_id: fid, ship_ids: vec![ships[1]] }).expect("split one ship off");
    let own: Vec<_> = e.state().fleets;
    assert_eq!(own.len(), 2);
    let child = own.iter().find(|f| f.id != fid).unwrap().id;
    let (a, b) = (e.engine.fleets[&fid].clone(), e.engine.fleets[&child].clone());
    assert_eq!((a.ship_count, b.ship_count), (1, 1));
    assert!((a.fuel + b.fuel - fuel_before).abs() < 0.01, "fuel is conserved");
    assert!((a.cargo.metal + b.cargo.metal - 30.0).abs() < 0.01, "cargo is conserved");
    assert!(a.fuel <= a.fuel_max && b.fuel <= b.fuel_max);

    e.run(10);
    assert_eq!(e.state().fleets.len(), 2, "docking does not merge fleets");

    let home = e.engine.players[&pid].homeworld;
    let out = outward_chain(&e.engine, home, 1)[0];
    e.travel_fleet(child, out);
    assert_eq!(e.order(Command::Merge { fleet_id: fid, other_fleet_id: child }), Err(ErrorCode::NotInSector));
    e.travel_fleet(child, home);
    e.run_until(200, |e| {
        [fid, child].iter().all(|id| matches!(e.engine.fleets[id].state, FleetStatus::Idle | FleetStatus::Docked))
    });
    let carried = e.engine.fleets[&fid].cargo.metal + e.engine.fleets[&child].cargo.metal;
    let fuel = fuel_of(&e, fid) + fuel_of(&e, child);
    e.order(Command::Merge { fleet_id: fid, other_fleet_id: child }).expect("merge at home");
    assert_eq!(e.state().fleets.len(), 1);
    let merged = e.fleet();
    assert_eq!(merged.ships.len(), 2);
    assert!((merged.fuel - fuel).abs() < 0.01 && (merged.cargo.metal - carried).abs() < 0.01);
}

#[test]
fn a_stranded_fleet_regains_one_jump_slowly_and_a_refused_recall_burns_nothing() {
    let (mut e, chain) = outpost("Adrift", 2);
    let fid = e.fid;
    let hop = hop_fuel(&e);

    edit(&mut e.engine, |s| s.fleets.get_mut(&fid).unwrap().fuel = hop * 3.9);
    assert_eq!(e.order(Command::Recall { fleet_id: fid }), Err(ErrorCode::InsufficientFuel));
    assert_eq!((e.engine.fleets[&fid].location, fuel_of(&e, fid)), (chain[1], hop * 3.9), "a refused recall burns nothing");

    edit(&mut e.engine, |s| s.fleets.get_mut(&fid).unwrap().fuel = 0.0);
    assert_eq!(e.order(Command::Move { fleet_id: fid, target: chain[0] }), Err(ErrorCode::InsufficientFuel));

    let mark = e.mark();
    e.run(STRANDED_RECOVERY_TICKS - 1);
    assert!(fuel_of(&e, fid) < hop, "still short one tick before the end");
    assert_eq!(e.order(Command::Move { fleet_id: fid, target: chain[0] }), Err(ErrorCode::InsufficientFuel));
    let progress = alerts_since(&e, mark);
    assert!(progress.len() >= 3, "quarter-way progress is announced: {progress:?}");

    let mark = e.mark();
    e.run(1);
    assert!(alerts_since(&e, mark).iter().any(|a| a.message.contains("reserve ready")));
    e.order(Command::Move { fleet_id: fid, target: chain[0] }).expect("one jump toward home");
    assert!(fuel_of(&e, fid) < hop * 0.01, "spent: the next hop needs the whole wait again");
}

#[test]
fn moves_that_cannot_get_home_warn_by_how_bad_it_is_and_safe_moves_stay_quiet() {
    let case = |at: usize, fuel_hops: f32, to: usize| {
        let (mut e, chain) = outpost("Reckless", 3);
        let (home, fid) = (e.engine.players[&e.pid].homeworld, e.fid);
        let from = if at == 0 { home } else { chain[at - 1] };
        e.travel(from);
        let hop = hop_fuel(&e);
        edit(&mut e.engine, |s| s.fleets.get_mut(&fid).unwrap().fuel = hop * fuel_hops);
        let mark = e.mark();
        e.run_until(100, |e| e.engine.fleets[&fid].action_cooldown == 0);
        e.order(Command::Move { fleet_id: fid, target: chain[to] }).expect("risky moves are allowed");
        alerts_since(&e, mark)
    };

    let stranded = case(2, 1.5, 2);
    assert_eq!(stranded.len(), 1, "{stranded:?}");
    assert!(matches!(stranded[0].level, AlertLevel::Critical) && stranded[0].message.contains("stranded"));

    let short = case(2, 3.0, 2);
    assert_eq!(short.len(), 1, "{short:?}");
    assert!(matches!(short[0].level, AlertLevel::Warning));
    assert!(short[0].message.contains("the way home is 3 hops"), "{}", short[0].message);

    assert!(case(1, 10.0, 1).is_empty(), "a safe move says nothing");
}

#[test]
fn previews_and_warnings_tell_charted_routes_from_direct_ones() {
    let (mut e, chain) = outpost("Pathfinder", 2);
    let (pid, fid) = (e.pid, e.fid);
    e.travel(chain[0]);
    let preview = |e: &mut Empire| {
        let Some(ServerMessage::PreviewMove(p)) = e.engine.execute(pid, Command::PreviewMove { fleet_id: fid, target: chain[1] }).reply else {
            panic!("no preview");
        };
        p
    };
    let charted = preview(&mut e);
    assert_eq!((charted.hops_home, charted.charted_hops_home, charted.route_unexplored), (2, Some(2), false));

    edit(&mut e.engine, |s| {
        s.explored.remove(&(pid, chain[0].to_key()));
    });
    let blind = preview(&mut e);
    assert_eq!((blind.hops_home, blind.charted_hops_home, blind.route_unexplored), (2, None, true));

    let hop = hop_fuel(&e);
    edit(&mut e.engine, |s| s.fleets.get_mut(&fid).unwrap().fuel = hop * 1.5);
    let mark = e.mark();
    e.order(Command::Move { fleet_id: fid, target: chain[1] }).expect("a risky move is allowed");
    let warning = alerts_since(&e, mark).remove(0);
    assert!(warning.message.contains("no charted route home; the direct route is 2 hops"), "{}", warning.message);
    assert!(warning.message.contains("across unexplored space"), "{}", warning.message);
}

fn tuned_prospector(engage_ratio_x10: u8) -> (Empire, u64) {
    let mut e = Empire::join(world("test"), "Dialled");
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    let frontier: Vec<Hex> = (-4i16..=4)
        .flat_map(|q| (-4i16..=4).map(move |r| Hex { q: home.q + q, r: home.r + r }))
        .filter(|h| (3..=4).contains(&Hex::distance(h, &home)))
        .collect();
    let weakest = frontier.iter().map(|&h| e.engine.known_threat(pid, h).est_power).fold(f32::MAX, f32::min);
    edit(&mut e.engine, |s| {
        for dr in -2i16..=2 {
            for dq in -2i16..=2 {
                let h = Hex { q: home.q + dq, r: home.r + dr };
                if Hex::distance(&h, &home) <= 2 {
                    s.explored.insert((pid, h.to_key()));
                }
            }
        }
        let fleet = s.fleets.get_mut(&fid).unwrap();
        let (mut lo, mut hi) = (0.0f32, 1e6f32);
        for _ in 0..60 {
            let mid = (lo + hi) / 2.0;
            for ship in &mut fleet.ships[..fleet.ship_count] {
                ship.weapon_power = mid;
            }
            if fleet_power(fleet) < 1.6 * weakest { lo = mid } else { hi = mid }
        }
    });
    let params = PolicyParams { max_range: 4, engage_ratio_x10, ..PolicyParams::default() };
    e.engine.handle_policy_update(pid, fid, PolicyPreset::Prospect, Some(params)).unwrap();
    (e, fid)
}

#[test]
fn a_doctrine_enters_unscouted_sectors_only_at_the_engage_ratio_the_player_set() {
    let outings = |engage_ratio_x10: u8| {
        let (mut e, _) = tuned_prospector(engage_ratio_x10);
        e.run(300);
        let moves = e.since(0, |k| match k {
            EventKind::PolicyAction(p) if p.action == "move" => Some(p.reason.clone()),
            _ => None,
        });
        let holds = e.since(0, |k| match k {
            EventKind::PolicyAction(p) if p.action == "hold" => Some(p.reason.clone()),
            _ => None,
        });
        (moves.len(), holds)
    };
    let (moves, _) = outings(12);
    assert!(moves > 0, "1.6x clears the 1.2x bar");
    let (moves, holds) = outings(20);
    assert_eq!(moves, 0, "1.6x does not clear 2.0x");
    assert!(holds.iter().any(|h| h.contains("the doctrine needs 2.0x")), "{holds:?}");
}

#[test]
fn an_autopilot_will_not_take_a_hop_that_strands_it_and_turns_for_home() {
    let (mut e, _) = outpost("Careful", 1);
    let (pid, fid) = (e.pid, e.fid);
    let hop = hop_fuel(&e);
    let params = PolicyParams { min_fuel_pct: 0, max_range: 4, ..PolicyParams::default() };
    edit(&mut e.engine, |s| {
        s.fleets.get_mut(&fid).unwrap().fuel = hop * 2.5;
        for ship in &mut s.fleets.get_mut(&fid).unwrap().ships {
            ship.weapon_power = 5000.0;
        }
    });
    e.engine.handle_policy_update(pid, fid, PolicyPreset::Prospect, Some(params)).unwrap();
    let mark = e.mark();
    e.run(200);

    let reasons = e.since(mark, |k| match k {
        EventKind::PolicyAction(p) if p.action == "move" => Some(p.reason.clone()),
        _ => None,
    });
    assert!(reasons[0].starts_with("returning"), "turning for home counts as acting: {reasons:?}");
    let critical = alerts_since(&e, mark).into_iter().filter(|a| matches!(a.level, AlertLevel::Critical)).count();
    assert_eq!(critical, 0, "the autopilot never stranded itself");
}
