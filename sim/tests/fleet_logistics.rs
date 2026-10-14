// Fleet handling over real distances: splitting and merging, fuel and the
// way home, the emergency reserve, and what standing orders refuse.

#[allow(dead_code)]
mod common;

use common::*;
use iac_shared::hex::Hex;
use iac_shared::constants::{
    Density, ResourceKind, Resources, ShipClass, EXPLORE_DURATION_TICKS, FUEL_DEUT_PER_UNIT, RECALL_FUEL_MULTIPLIER,
    REFINE_LOSS, STRANDED_RECOVERY_TICKS,
};
use iac_shared::pace::Pace;
use iac_shared::protocol::{
    AlertEvent, AlertLevel, ClientMessage, Command, ErrorCode, EventKind, HarvestResource, PolicyParams, PolicyPreset,
    PolicyUpdate, ServerMessage, ThreatBasis, MAX_ODDS_SECTORS,
};
use iac_shared::scaling::{ratio_label, ship_tank, RatioLabel, ResearchLevels};
use iac_shared::world::{self, NpcBehaviorType};
use iac_sim::engine::{template_npc_id, Errand, FleetStatus, SectorOverride, Ships};
use iac_sim::snapshot::ExploreCredit;
use iac_sim::GameEngine;

fn alerts_since(e: &Empire, mark: usize) -> Vec<AlertEvent> {
    e.since(mark, |k| match k {
        EventKind::Alert(a) => Some(a.clone()),
        _ => None,
    })
}

fn fuel_of(e: &Empire, fid: u64) -> f64 {
    e.engine.fleets[&fid].fuel
}

fn hop_fuel(e: &Empire) -> f64 {
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
    assert_eq!(e.fleet().ship_count, 2);
    let fuel_before = fuel_of(&e, fid);
    let split = |ships: &[(ShipClass, u32)]| Command::Split { fleet_id: fid, ships: counts(ships), damaged_first: false };

    assert_eq!(e.refusal(split(&[])).1, "split needs at least one class and a count of its ships to take");
    assert_eq!(
        e.refusal(split(&[(ShipClass::LightFighter, 2)])),
        (ErrorCode::InvalidTarget, format!("the split would take all 2 ships of fleet {fid}; at least one ship must stay behind")),
    );
    assert_eq!(e.refusal(split(&[(ShipClass::LightFighter, 3)])).1, format!("fleet {fid} has 2 Light Fighter, not the 3 the split names"));
    assert_eq!(e.refusal(split(&[(ShipClass::Cruiser, 1)])).1, format!("fleet {fid} has 0 Cruiser, not the 1 the split names"));
    assert_eq!(e.refusal(split(&[(ShipClass::LightFighter, 0)])).1, "split names Light Fighter with a count of 0");
    assert_eq!(
        e.refusal(split(&[(ShipClass::LightFighter, 1), (ShipClass::LightFighter, 1)])).1,
        "split names Light Fighter twice; give each class one count",
    );
    let outcome = e.engine.execute(rival, split(&[(ShipClass::LightFighter, 1)]));
    assert_eq!(outcome.error.map(|(code, _)| code), Some(ErrorCode::FleetNotFound));
    assert_eq!(e.order(Command::Merge { fleet_id: fid, other_fleet_id: fid }), Err(ErrorCode::InvalidTarget));
    assert_eq!(e.order(Command::Merge { fleet_id: fid, other_fleet_id: rival_fleet }), Err(ErrorCode::FleetNotFound));

    e.order(split(&[(ShipClass::LightFighter, 1)])).expect("split one ship off");
    let own: Vec<_> = e.state().fleets;
    assert_eq!(own.len(), 2);
    let child = own.iter().find(|f| f.id != fid).unwrap().id;
    let (a, b) = (e.engine.fleets[&fid].clone(), e.engine.fleets[&child].clone());
    assert_eq!((a.ships.total(), b.ships.total()), (1, 1));
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
    assert_eq!((merged.ship_count, merged.ships.len(), merged.ships[0].count), (2, 1, 2));
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
    let case = |at: usize, fuel_hops: f64, to: usize| {
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

#[test]
fn home_fuel_prices_the_lanes_a_fleet_flies_and_a_recall_does_not() {
    let (mut e, chain) = outpost("Accountant", 2);
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    let at = chain[1];
    assert_eq!(e.engine.fleets[&fid].location, at);
    edit(&mut e.engine, |s| s.explored.retain(|key| key.0 != pid));
    let route = e.engine.home_route(pid, at);
    assert!(route.charted.is_none() && route.unexplored, "the chart joins this fleet to home by no lane: {route:?}");
    assert!(route.direct >= 2, "the lane home is more than one hop: {}", route.direct);
    let hop = e.fleet().jump_fuel;
    let need = hop * f64::from(route.direct);
    assert_eq!(e.fleet().hops_home, route.direct, "hops_home counts the lanes home_fuel is priced from");
    assert_eq!(e.fleet().home_fuel, need, "home_fuel is those lanes, chart or none");
    let straight = f64::from(Hex::distance(&at, &home));
    let bent = (-12..12)
        .flat_map(|q| (-12..12).map(move |r| Hex::new(q, r)))
        .find(|hex| *hex != home && e.engine.home_route(pid, *hex).direct != Hex::distance(hex, &home) as u32)
        .expect("a sector whose lane home is not the straight line");
    edit(&mut e.engine, |s| {
        let fleet = s.fleets.get_mut(&fid).unwrap();
        fleet.location = bent;
        fleet.state = FleetStatus::Idle;
        fleet.move_target = None;
    });
    let bent_route = e.engine.home_route(pid, bent);
    assert_eq!(e.fleet().hops_home, bent_route.direct);
    assert_ne!(e.fleet().hops_home, Hex::distance(&bent, &home) as u32, "not the straight line, at {bent}");
    assert_eq!(e.fleet().home_fuel, hop * f64::from(bent_route.direct));
    edit(&mut e.engine, |s| {
        let fleet = s.fleets.get_mut(&fid).unwrap();
        fleet.location = home;
        fleet.state = FleetStatus::Docked;
        fleet.move_target = None;
    });
    assert_eq!((e.fleet().hops_home, e.fleet().home_fuel), (0, 0.0), "at the homeworld both are zero");
    edit(&mut e.engine, |s| {
        let fleet = s.fleets.get_mut(&fid).unwrap();
        fleet.location = at;
        fleet.state = FleetStatus::Idle;
        fleet.move_target = None;
        fleet.fuel = need;
    });
    assert_eq!(
        e.engine.recall_fuel_cost(&e.engine.fleets[&fid]),
        hop * straight * RECALL_FUEL_MULTIPLIER,
        "a recall burns twice the jump fuel per hex of straight distance, and then jumps",
    );

    e.engine
        .handle_policy_update(pid, fid, PolicyPreset::MineAndReturn, Some(PolicyParams {
            min_fuel_pct: 0,
            cargo_return_pct: 100,
            max_range: 8,
            engage_ratio_x10: 15,
            disengage_ratio_x10: 9,
            fuel_budget_pct: 100,
            refine: false,
        }))
        .expect("orders");
    let mark = e.mark();
    e.run(1);
    let covered = reports(&e, mark);
    assert!(
        covered.iter().all(|(_, why)| !why.contains("route home")),
        "fuel for the direct way is enough, though the chart has no way home: {covered:?}",
    );

    edit(&mut e.engine, |s| {
        let fleet = s.fleets.get_mut(&fid).unwrap();
        fleet.location = at;
        fleet.fuel = need - hop * 0.5;
        fleet.state = FleetStatus::Idle;
        fleet.move_target = None;
        fleet.move_cooldown = 0;
        fleet.action_cooldown = 0;
        s.policies.get_mut(&fid).unwrap().next_eval_tick = s.tick + 1;
    });
    let mark = e.mark();
    e.run(1);
    let moves = policy_reasons(&e, mark, "move");
    assert!(!moves.is_empty(), "short of the direct way, it starts home: {:?}", reports(&e, mark));
    assert!(
        moves[0].contains(&format!("fuel {:.0} is below the route home {:.0}", need - hop * 0.5, need)),
        "{moves:?}",
    );
    let step = e.engine.fleets[&fid].move_target.expect("the jump is ordered");
    let step_hops = e.engine.home_route(pid, step).direct;
    assert!(step_hops < route.direct, "the jump is along the lanes toward home, to {step}");
    assert_eq!(e.fleet().hops_home, step_hops, "while the jump is in progress the count is from where it is bound");
    assert_eq!(e.fleet().home_fuel, hop * f64::from(step_hops));
}

#[test]
fn odds_cover_any_sector_and_say_what_the_preview_of_a_jump_says() {
    let (mut e, rival, rival_fleet) = with_rival("Oddsmaker");
    let (pid, fid) = (e.pid, e.fid);
    let here = e.engine.fleets[&fid].location;
    let next = e.engine.world_gen.connected_neighbors(here).slice()[0];
    let far = Hex { q: 14, r: -3 };
    let ask = |e: &mut Empire, who: u64, fleet_id: u64, sectors: Vec<Hex>| e.engine.execute(who, Command::Odds { fleet_id, sectors });

    let Some(ServerMessage::Odds(reply)) = ask(&mut e, pid, fid, vec![next, here, far]).reply else { panic!("no odds") };
    assert_eq!(reply.fleet_id, fid);
    assert_eq!(reply.sectors.iter().map(|s| s.sector).collect::<Vec<_>>(), [next, here, far]);
    let Some(ServerMessage::PreviewMove(preview)) = e.engine.execute(pid, Command::PreviewMove { fleet_id: fid, target: next }).reply else {
        panic!("no preview");
    };
    assert_eq!((reply.sectors[0].threat, reply.sectors[0].ratio, reply.sectors[0].label), (preview.threat, preview.ratio, preview.label));
    assert_eq!((reply.sectors[2].threat.basis, reply.sectors[2].label), (ThreatBasis::Estimate, RatioLabel::Deadly));
    for odds in &reply.sectors {
        assert_eq!(odds.label, ratio_label(odds.ratio));
        assert_eq!(odds.ratio, e.engine.fleet_odds(&e.engine.fleets[&fid], odds.sector).ratio);
    }

    let crowd = vec![far; MAX_ODDS_SECTORS + 1];
    assert_eq!(ask(&mut e, pid, fid, crowd).error.map(|(code, _)| code), Some(ErrorCode::InvalidCommand));
    assert!(ask(&mut e, rival, fid, vec![far]).error.is_some(), "another empire's fleet");
    assert!(ask(&mut e, pid, rival_fleet, vec![far]).error.is_some());
}

/// A lone Light Fighter at home under `prospect`, with the two rings around
/// its homeworld charted, so that its first outing is into unscouted space.
fn charted_prospector(engage_ratio_x10: u8) -> (Empire, u64) {
    let mut e = Empire::join(world("test"), "Dialled");
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    edit(&mut e.engine, |s| {
        for dr in -2i16..=2 {
            for dq in -2i16..=2 {
                let h = Hex { q: home.q + dq, r: home.r + dr };
                if Hex::distance(&h, &home) <= 2 {
                    s.explored.insert((pid, h.to_key()));
                    s.credited.insert((pid, h.to_key(), ExploreCredit::Charted));
                }
            }
        }
        s.fleets.get_mut(&fid).unwrap().ships = Ships::of(&[(ShipClass::LightFighter, 1)]);
    });
    let params = PolicyParams { max_range: 4, engage_ratio_x10, ..PolicyParams::default() };
    e.engine.handle_policy_update(pid, fid, PolicyPreset::Prospect, Some(params)).unwrap();
    (e, fid)
}

#[test]
fn a_doctrine_enters_unscouted_sectors_only_at_the_engage_ratio_the_player_set() {
    let outings = |engage_ratio_x10: u8| {
        let (mut e, _) = charted_prospector(engage_ratio_x10);
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
    // What a lone Light Fighter rates past the charted ring is the world's
    // to say. The ratio is a bar: the highest setting it still sets out at
    // is found by halving, and one tenth above it the fleet stays, and says
    // of every sector it names that it rates below the bar.
    assert_eq!(outings(100).0, 0, "a lone Light Fighter rates 10x nowhere past the charted ring");
    assert!(outings(1).0 > 0, "and 0.1x anywhere");
    let (mut goes, mut stays) = (1u8, 100u8);
    while stays - goes > 1 {
        let mid = goes + (stays - goes) / 2;
        if outings(mid).0 > 0 { goes = mid } else { stays = mid }
    }
    assert!((3..98).contains(&goes), "a bar with room on both sides: {goes} tenths");
    let need = f32::from(stays) / 10.0;
    let (moves, holds) = outings(stays);
    assert_eq!(moves, 0);
    assert!(!holds.is_empty(), "the refusal is reported");
    for hold in &holds {
        let rated: f32 = hold
            .split_once("your fleet rates ")
            .and_then(|(_, rating)| rating.split_once('x'))
            .and_then(|(ratio, _)| ratio.parse().ok())
            .unwrap_or_else(|| panic!("the ratio the fleet rates at: {hold}"));
        assert!(rated < need + 0.005, "{rated}x was refused at a bar of {need:.1}x: {hold}");
        assert!(hold.contains(&format!("the doctrine needs {need:.1}x")), "{hold}");
    }
    let (moves, holds) = outings(goes);
    assert!(moves > 0, "one tenth lower it sets out: {holds:?}");
}

#[test]
fn an_autopilot_will_not_take_a_hop_that_strands_it_and_turns_for_home() {
    let (mut e, _) = outpost("Careful", 1);
    let (pid, fid) = (e.pid, e.fid);
    let hop = hop_fuel(&e);
    let params = PolicyParams { min_fuel_pct: 0, max_range: 4, ..PolicyParams::default() };
    edit(&mut e.engine, |s| {
        let fleet = s.fleets.get_mut(&fid).unwrap();
        fleet.fuel = hop * 2.5;
        fleet.refit = MIGHTY;
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

fn policy_reasons(e: &Empire, mark: usize, action: &str) -> Vec<String> {
    e.since(mark, |k| match k {
        EventKind::PolicyAction(p) if p.action == action => Some(p.reason.clone()),
        _ => None,
    })
}

/// A prospecting fleet docked at home with half a tank, under a fuel budget
/// of `budget` percent, the stockpile holding `deuterium` (given the cap).
/// Returns the empire, the deuterium the fill costs and the deuterium income
/// per tick.
fn docked_with_half_a_tank(budget: u8, deuterium: impl Fn(f64) -> f64) -> (Empire, f64, f64) {
    let mut e = Empire::join(world("season"), "Thrifty");
    let (pid, fid) = (e.pid, e.fid);
    let pace = e.engine.world.pace;
    let tank = e.engine.fleets[&fid].fuel_max;
    edit(&mut e.engine, |s| {
        let player = s.players.get_mut(&pid).unwrap();
        player.buildings.deuterium_synthesizer = 2;
        let cap = iac_shared::scaling::storage_cap(&player.buildings, &pace).deuterium;
        player.resources.deuterium = deuterium(cap);
        s.fleets.get_mut(&fid).unwrap().fuel = tank / 2.0;
    });
    let income = e.engine.players[&pid].production_per_tick(&pace).deuterium;
    assert!(income > 0.0);
    let params = PolicyParams { fuel_budget_pct: budget, ..PolicyParams::default() };
    e.engine.handle_policy_update(pid, fid, PolicyPreset::Prospect, Some(params)).unwrap();
    (e, tank / 2.0 * FUEL_DEUT_PER_UNIT, income)
}

#[test]
fn a_young_empire_with_no_deuterium_income_keeps_its_stock_from_a_fleet_under_orders() {
    let mut e = Empire::join(world("season"), "Newcomer");
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    let stock = e.engine.players[&pid].resources.deuterium;
    assert_eq!(e.engine.players[&pid].production_per_tick(&e.engine.world.pace).deuterium, 0.0);
    let tank = e.engine.fleets[&fid].fuel_max;
    edit(&mut e.engine, |s| s.fleets.get_mut(&fid).unwrap().fuel = tank / 2.0);
    e.engine.handle_policy_update(pid, fid, PolicyPreset::Prospect, None).unwrap();
    let mark = e.mark();
    e.run(600);

    assert_eq!(e.engine.players[&pid].resources.deuterium, stock, "the starting deuterium is for the queues");
    assert_eq!((fuel_of(&e, fid), e.engine.fleets[&fid].location), (tank / 2.0, home));
    let waiting = policy_reasons(&e, mark, "hold");
    assert!(waiting[0].contains("earns nothing"), "{waiting:?}");

    e.engine.handle_policy_update(pid, fid, PolicyPreset::Manual, None).unwrap();
    e.run(2);
    assert_eq!(fuel_of(&e, fid), tank, "flown by hand it fills as before");
    assert!(e.engine.players[&pid].resources.deuterium < stock);
}

#[test]
fn a_fleet_on_a_fuel_budget_waits_for_its_share_of_the_deuterium_income_and_then_fills_in_one_go() {
    let (mut e, cost, income) = docked_with_half_a_tank(20, |cap| cap / 2.0);
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    let deuterium = |e: &Empire| e.engine.players[&e.pid].resources.deuterium;
    let (start, half) = (deuterium(&e), fuel_of(&e, fid));
    let wait = (cost / (0.20 * income)).ceil() as u64;
    assert!(wait > 120, "the budget takes minutes to earn the fill: {wait} ticks");

    let mark = e.mark();
    e.run(wait - 10);
    assert_eq!(fuel_of(&e, fid), half, "no part fills");
    assert_eq!(e.engine.fleets[&fid].location, home, "and it does not leave on half a tank");
    assert!(deuterium(&e) >= start + (wait - 11) as f64 * income, "the stockpile kept all it produced");
    let waiting = policy_reasons(&e, mark, "hold");
    assert!(waiting[0].starts_with("refuelling on a fuel budget of 20% of deuterium income"), "{waiting:?}");
    assert!(policy_reasons(&e, mark, "move").is_empty());

    let (before, tick) = (deuterium(&e), e.engine.current_tick());
    e.run_until(20, |e| fuel_of(e, fid) > half);
    assert_eq!(fuel_of(&e, fid), e.engine.fleets[&fid].fuel_max, "the whole fill at once");
    let earned = (e.engine.current_tick() - tick) as f64 * income;
    let paid = before + earned - deuterium(&e);
    assert!((paid - cost).abs() < 0.02 * cost, "paid {paid:.2} for a fill of {cost:.2}");
    e.run_until(60, |e| e.engine.fleets[&fid].location != home);
}

#[test]
fn a_full_deuterium_store_or_an_unbounded_budget_fills_the_tank_at_once() {
    let (mut overflowing, ..) = docked_with_half_a_tank(20, |cap| cap);
    let (mut unbounded, ..) = docked_with_half_a_tank(100, |cap| cap / 2.0);
    for e in [&mut overflowing, &mut unbounded] {
        let fid = e.fid;
        e.run(2);
        assert_eq!(fuel_of(e, fid), e.engine.fleets[&fid].fuel_max);
        assert_eq!(e.engine.policies[&fid].fuel_credit, 0.0, "nothing was drawn on the allowance");
    }
}

/// A quiet sector in `ring` (from the hub) with a lane home, and the odds of
/// the empire's fleet in every sector of that lane but home.
fn outpost_in_ring(e: &Empire, ring: u16) -> (Hex, Vec<f32>) {
    let home = e.engine.players[&e.pid].homeworld;
    iac_shared::hex::hex_ring(Hex::ORIGIN, ring)
        .filter(|at| e.engine.world_gen.generate_sector(*at).npc_template.is_none())
        .filter_map(|at| {
            let mut lane = lane_path(&e.engine, at, home, 12)?;
            lane.pop();
            let odds = e.engine.odds_for(e.pid, e.fid, &lane).unwrap();
            Some((at, odds.iter().map(|o| o.ratio).collect::<Vec<f32>>()))
        })
        .min_by_key(|(at, _)| Hex::distance(at, &home))
        .expect("a quiet sector in that ring")
}

#[test]
fn a_fleet_heading_home_takes_the_mildest_lane_and_says_so() {
    let mut e = Empire::join(world("persistent"), "Homesick");
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    let (at, lane) = outpost_in_ring(&e, 8);
    assert!(lane.iter().any(|r| *r < 1.5), "no lane home meets the default doctrine: {lane:?}");
    deploy(&mut e.engine, fid, at, None);
    e.engine.handle_policy_update(pid, fid, PolicyPreset::Prospect, None).unwrap();
    let mark = e.mark();
    e.run_until(600, |e| e.engine.fleets.get(&fid).is_none_or(|f| f.location == home));

    assert_eq!(e.engine.fleets[&fid].ships.total(), 2, "it came home whole");
    let jumps = policy_reasons(&e, mark, "move");
    assert!(jumps[0].contains("no lane home meets the doctrine's 1.5x, taking the mildest"), "{jumps:?}");
    assert!(jumps.iter().all(|why| !why.contains("DEADLY")), "{jumps:?}");
}

#[test]
fn a_fleet_cut_off_by_deadly_odds_holds_and_alerts_its_owner_instead_of_jumping_into_them() {
    let mut e = Empire::join(world("persistent"), "Marooned");
    let (pid, fid) = (e.pid, e.fid);
    let (at, lane) = outpost_in_ring(&e, 11);
    assert!(lane[0] < 0.9, "the first sector of the lane home is deadly to two scouts: {lane:?}");
    deploy(&mut e.engine, fid, at, None);
    e.engine.handle_policy_update(pid, fid, PolicyPreset::Prospect, None).unwrap();
    let mark = e.mark();
    e.run(45);

    assert_eq!(e.engine.fleets[&fid].location, at);
    assert!(policy_reasons(&e, mark, "move").is_empty());
    let held = policy_reasons(&e, mark, "hold");
    assert!(held.iter().any(|why| why.starts_with("cut off from home")), "{held:?}");
    let warned = alerts_since(&e, mark);
    assert!(
        warned.iter().any(|a| matches!(a.level, AlertLevel::Warning) && a.message.contains("cut off from home")),
        "{warned:?}",
    );
}

#[test]
fn a_fleet_that_loses_a_ship_in_a_fight_shows_the_tank_and_range_of_the_ships_it_has_left() {
    let mut e = Empire::join(world("test"), "Mauled");
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    let at = lone_fighter_sector(&e.engine, NpcBehaviorType::Passive);
    deploy(&mut e.engine, fid, at, None);
    // Two Small Cargos, whose guns scratch a fighter and no more, one of
    // them a hit from breaking up.
    edit(&mut e.engine, |s| {
        let fleet = s.fleets.get_mut(&fid).unwrap();
        fleet.ships = squadron(ShipClass::SmallCargo, 1, &[1]);
        fleet.fuel_max = 2.0 * ship_tank(ShipClass::SmallCargo, &ResearchLevels::default());
        fleet.fuel = fleet.fuel_max;
    });
    let whole = e.fleet();
    assert_eq!((whole.ship_count, whole.ships[0].damaged, whole.ships[0].at_risk), (2, 1, 1));
    assert_eq!(whole.fuel, whole.fuel_max, "it set out with a full tank");
    let hops_home = whole.home_fuel / whole.jump_fuel;
    assert!((f64::from(whole.hops_home) - hops_home).abs() < 1e-3, "hops_home is that count");

    e.order(Command::Attack { fleet_id: fid, target_fleet_id: template_npc_id(at) }).expect("attack");
    e.run_until(200, |e| e.engine.fleets[&fid].ships.total() == 1);
    edit(&mut e.engine, |s| s.fleets.get_mut(&fid).unwrap().refit.weapons = MIGHTY.weapons);
    e.run_until(20, |e| e.engine.active_combats.is_empty());

    let mauled = e.fleet();
    assert_eq!(mauled.ship_count, 1);
    assert!(
        (mauled.fuel_max - whole.fuel_max / 2.0).abs() < 0.01,
        "one cargo ship of two is left but the tank reads {} of the {} two carried",
        mauled.fuel_max,
        whole.fuel_max,
    );
    assert!(mauled.fuel <= mauled.fuel_max, "{} fuel in a {} tank", mauled.fuel, mauled.fuel_max);
    assert!((mauled.jump_fuel - whole.jump_fuel / 2.0).abs() < 1e-3, "a jump costs what the ships present weigh");
    assert!((mauled.home_fuel / mauled.jump_fuel - hops_home).abs() < 1e-3, "the way home is as many jumps as before");
    assert_eq!(mauled.hops_home, whole.hops_home, "losing a ship does not change how many hops home are");
    let reach = ((mauled.fuel - mauled.home_fuel) / (2.0 * mauled.jump_fuel)).floor().max(0.0) as u32;
    assert_eq!(mauled.range_hops, reach);

    let doorstep = outward_chain(&e.engine, home, 1)[0];
    deploy(&mut e.engine, fid, doorstep, None);
    let near = e.fleet();
    let full_tank_reach = ((near.fuel_max - near.home_fuel) / (2.0 * near.jump_fuel)).floor() as u32;
    assert_eq!(near.range_hops, full_tank_reach, "the range shown is what the surviving ship's own tank reaches");
    e.travel(home);
    e.run(3);
    let docked = e.fleet();
    assert_eq!(docked.fuel_max, mauled.fuel_max, "docking finds nothing to correct");
    assert!(docked.range_hops >= near.range_hops, "and the range shown in the field was not more than a full tank gives");
}

#[test]
fn recall_at_home_is_refused_and_the_jump_the_fleet_paid_for_goes_on() {
    let mut e = Empire::join(world("persistent"), "Homebody");
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    let out = outward_chain(&e.engine, home, 1)[0];
    let docked = e.engine.snapshot().hash();

    let (code, why) = e.refusal(Command::Recall { fleet_id: fid });
    assert_eq!(code, ErrorCode::InvalidCommand);
    assert!(why.contains(&format!("fleet {fid} is already at its homeworld")), "{why}");
    assert_eq!(e.engine.snapshot().hash(), docked, "a refused recall changes nothing");

    edit(&mut e.engine, |s| s.fleets.get_mut(&fid).unwrap().state = FleetStatus::Harvesting);
    let mining = e.engine.snapshot().hash();
    let (code, why) = e.refusal(Command::Recall { fleet_id: fid });
    assert_eq!(code, ErrorCode::InvalidCommand);
    assert!(why.contains("at work there") && why.contains("stop"), "{why}");
    assert_eq!(e.engine.snapshot().hash(), mining, "a fleet harvesting at home is not halted by it");
    e.order(Command::Stop { fleet_id: fid }).expect("stop is the order that halts it");

    let tank = fuel_of(&e, fid);
    e.order(Command::Move { fleet_id: fid, target: out }).expect("a jump out");
    let paid = tank - fuel_of(&e, fid);
    assert!(paid > 0.0, "the jump is paid for when it is ordered");
    let charging = e.engine.snapshot().hash();

    let (code, why) = e.refusal(Command::Recall { fleet_id: fid });
    assert_eq!(code, ErrorCode::InvalidCommand);
    assert!(why.contains(&format!("charging a jump to {out}")) && why.contains("stop"), "{why}");
    assert_eq!(e.engine.snapshot().hash(), charging, "the jump, its fuel and its countdown are as they were");

    e.run_until(200, |e| e.engine.fleets[&fid].location == out);
    assert_eq!(e.engine.fleets[&fid].state, FleetStatus::Idle, "the fuel bought the jump it was spent on");

    e.run_until(200, |e| e.engine.fleets[&fid].action_cooldown == 0);
    e.order(Command::Recall { fleet_id: fid }).expect("one hop out there is somewhere to come home from");
    assert_eq!(e.engine.fleets[&fid].location, home);
}

#[test]
fn standing_orders_for_a_fleet_that_is_not_the_players_are_refused_by_name() {
    let (mut e, _, rival_fleet) = with_rival("Meddler");
    let orders = ClientMessage::PolicyUpdate(PolicyUpdate { fleet_id: rival_fleet, preset: PolicyPreset::Prospect, params: None });
    let answer = e.engine.answer(e.pid, orders);
    let [ServerMessage::Error(refusal)] = answer.as_slice() else { panic!("one refusal: {answer:?}") };
    assert_eq!(refusal.code, ErrorCode::FleetNotFound);
    assert!(refusal.message.starts_with(&format!("fleet {rival_fleet} not found")), "{}", refusal.message);
    assert!(run(&mut e.engine, 5).iter().all(|ev| !matches!(ev.kind, EventKind::PolicyAction(_))), "no orders were set");
}

fn wreckage(e: &Empire, at: Hex) -> Resources {
    e.engine.sector_overrides.get(&at.to_key()).and_then(|o| o.salvage).unwrap_or_default()
}

fn same(a: Resources, b: Resources) -> bool {
    (a.metal - b.metal).abs() < 0.05 && (a.crystal - b.crystal).abs() < 0.05 && (a.deuterium - b.deuterium).abs() < 0.05
}

/// Two Small Cargos at a lone passive fighter they can only scratch, their
/// hold nine tenths full: `whole` of them whole, the rest a hit from
/// breaking up.
fn laden_and_outgunned(e: &mut Empire, whole: u32) -> (Hex, f64, Resources) {
    let fid = e.fid;
    let at = lone_fighter_sector(&e.engine, NpcBehaviorType::Passive);
    deploy(&mut e.engine, fid, at, None);
    let brittle = vec![1; 2 - whole as usize];
    edit(&mut e.engine, |s| s.fleets.get_mut(&fid).unwrap().ships = squadron(ShipClass::SmallCargo, whole, &brittle));
    let hold = e.fleet().cargo_capacity;
    let cargo = Resources { metal: hold * 0.5, crystal: hold * 0.25, deuterium: hold * 0.15 };
    edit(&mut e.engine, |s| s.fleets.get_mut(&fid).unwrap().cargo = cargo);
    (at, hold, cargo)
}

#[test]
fn a_fleet_that_loses_a_ship_in_a_fight_leaves_what_it_can_no_longer_carry_as_wreckage() {
    let mut e = Empire::join(world("test"), "Spilled");
    let fid = e.fid;
    let (at, hold, cargo) = laden_and_outgunned(&mut e, 1);

    let mark = e.mark();
    e.order(Command::Attack { fleet_id: fid, target_fleet_id: template_npc_id(at) }).expect("attack");
    e.run_until(200, |e| e.engine.fleets[&fid].ships.total() == 1);

    let mauled = e.fleet();
    assert!((mauled.cargo_capacity - hold / 2.0).abs() < 0.01, "one hold of two is left");
    assert!(
        mauled.cargo.total() <= mauled.cargo_capacity,
        "{} aboard a hold of {}",
        mauled.cargo.total(),
        mauled.cargo_capacity,
    );
    assert!(same(mauled.cargo, cargo.scale(hold / 2.0 / cargo.total())), "each resource gave up its share: {:?}", mauled.cargo);
    let spilled = wreckage(&e, at);
    assert!(spilled.total() > hold * 0.39, "{spilled:?}");
    assert!(same(mauled.cargo.add(spilled), cargo), "nothing was made or lost: {:?} aboard, {spilled:?} adrift", mauled.cargo);
    let told = alerts_since(&e, mark);
    assert!(
        told.iter().any(|a| {
            matches!(a.level, AlertLevel::Warning) && a.sector == Some(at) && a.message.contains("drifts as wreckage")
        }),
        "{told:?}",
    );

    let mark = e.mark();
    edit(&mut e.engine, |s| s.fleets.get_mut(&fid).unwrap().refit.weapons = MIGHTY.weapons);
    e.run_until(20, |e| e.engine.active_combats.is_empty());
    let (pile, _) = pile_of(&e.log[mark..]);
    assert_eq!(pile, wreckage(&e, at), "the victory reports the pile that lies there");
    assert!(pile.metal >= spilled.metal && pile.crystal >= spilled.crystal && pile.deuterium >= spilled.deuterium);

    let _ = e.order(Command::CollectSalvage { fleet_id: fid });
    let scooped = e.fleet();
    assert!(scooped.cargo.total() <= scooped.cargo_capacity + 0.01, "a full hold scoops nothing back");
    assert!(same(scooped.cargo.add(wreckage(&e, at)), mauled.cargo.add(pile)));
}

#[test]
fn a_destroyed_fleet_leaves_its_cargo_where_it_fell_for_whoever_collects() {
    let (mut e, rival, rival_fleet) = with_rival("Wrecked");
    let fid = e.fid;
    let (at, _, cargo) = laden_and_outgunned(&mut e, 0);

    let mark = e.mark();
    e.order(Command::Attack { fleet_id: fid, target_fleet_id: template_npc_id(at) }).expect("attack");
    e.run_until(200, |e| e.engine.fleets.get(&fid).is_none_or(|f| f.ships.is_empty()));
    e.run(2);
    assert!(!e.engine.fleets.contains_key(&fid));

    assert!(same(wreckage(&e, at), cargo), "the whole cargo lies in the sector: {:?}", wreckage(&e, at));
    let told: Vec<String> = alerts_since(&e, mark).into_iter().map(|a| a.message).collect();
    assert!(told.iter().any(|m| m.contains("FLEET LOST")), "{told:?}");
    assert!(told.iter().any(|m| m.contains("drifts as wreckage") && m.contains(&at.to_string())), "{told:?}");

    muster(&mut e.engine, rival_fleet, &[(ShipClass::SmallCargo, 2)], at);
    let collect = e.engine.execute(rival, Command::CollectSalvage { fleet_id: rival_fleet });
    assert!(collect.error.is_none(), "{:?}", collect.error);
    assert!(same(e.engine.fleets[&rival_fleet].cargo, cargo), "the finder has it: {:?}", e.engine.fleets[&rival_fleet].cargo);
    assert!(wreckage(&e, at).total() <= 1.0);
}

#[test]
fn a_recall_that_breaks_ships_leaves_their_share_of_the_cargo_where_it_jumped_from() {
    let mut e = Empire::join(world("persistent"), "Breaker");
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    let far = Hex { q: home.q + 25, r: home.r };
    muster(&mut e.engine, fid, &[(ShipClass::LightFighter, 20)], far);
    let hold = e.fleet().cargo_capacity;
    let cargo = Resources { metal: hold * 0.5, crystal: hold * 0.25, deuterium: hold * 0.25 };
    e.set_stockpile(Resources::default());
    edit(&mut e.engine, |s| {
        let fleet = s.fleets.get_mut(&fid).unwrap();
        fleet.ships = squadron(ShipClass::LightFighter, 1, &[1; 19]);
        fleet.fuel = 1e12;
        fleet.fuel_max = 1e12;
        fleet.cargo = cargo;
    });

    let mark = e.mark();
    e.order(Command::Recall { fleet_id: fid }).expect("recall");
    let lost = e.since(mark, |k| match k {
        EventKind::ShipsLost(lost) if lost.mine && lost.sector == far => Some(lost.ships.clone()),
        _ => None,
    });
    let [lost] = lost.as_slice() else { panic!("one report of the ships that broke up: {lost:?}") };
    assert!(matches!(lost.as_slice(), [one] if one.ship_class == ShipClass::LightFighter), "{lost:?}");
    let broke = lost[0].count as usize;
    let back = e.fleet();
    assert!(broke > 0 && back.ship_count as usize == 20 - broke, "{broke} ships broke up, {} came home", back.ship_count);
    assert_eq!(back.location, home);
    assert!(back.repair.is_none() && back.hull == back.hull_max, "the home dock mended the ships the jump harmed");

    let left_behind = wreckage(&e, far);
    assert!(same(left_behind, cargo.scale(broke as f64 / 20.0)), "{broke} holds of 20 stayed behind: {left_behind:?}");
    assert!(
        same(e.stockpile().add(back.cargo).add(left_behind), cargo),
        "what came home is the cargo less the wreckage: {:?} banked, {:?} aboard",
        e.stockpile(),
        back.cargo,
    );
    assert_eq!(wreckage(&e, home), Resources::default());

    let told = alerts_since(&e, mark);
    let cost = told.iter().find(|a| a.message.starts_with(&format!("fleet {fid} made the emergency jump home from {far} for "))).expect("the cost of the jump");
    assert!(matches!(cost.level, AlertLevel::Warning) && cost.sector == Some(far), "{cost:?}");
    assert!(cost.message.contains(&format!(": {broke} of its 20 ships broke up in the jump")), "{}", cost.message);
}

#[test]
fn a_jump_called_off_tells_its_owner_what_fuel_is_forfeit_and_a_halted_harvest_is_quiet() {
    let mut e = Empire::join(world("persistent"), "Hesitant");
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    let out = outward_chain(&e.engine, home, 1)[0];

    let tank = fuel_of(&e, fid);
    e.order(Command::Move { fleet_id: fid, target: out }).expect("a jump out");
    assert_eq!(tank - fuel_of(&e, fid), 40.0, "two Light Fighters burn 20 fuel each a jump, paid when it is ordered");
    let mark = e.mark();
    e.order(Command::Stop { fleet_id: fid }).expect("stop calls a charging jump off");
    let told = alerts_since(&e, mark);
    assert_eq!(told.len(), 1, "{told:?}");
    assert_eq!(told[0].message, format!("fleet {fid} called off its jump to {out}: the 40 fuel it burned for the jump is forfeit"));
    assert!(matches!(told[0].level, AlertLevel::Warning) && told[0].fleet_id == Some(fid) && told[0].sector == Some(home), "{:?}", told[0]);
    let halted = &e.engine.fleets[&fid];
    assert_eq!((halted.state, halted.location, halted.fuel), (FleetStatus::Idle, home, tank - 40.0), "the fuel is gone, as the event says");

    edit(&mut e.engine, |s| s.fleets.get_mut(&fid).unwrap().state = FleetStatus::Harvesting);
    let mark = e.mark();
    e.order(Command::Stop { fleet_id: fid }).expect("stop halts a harvest");
    assert!(alerts_since(&e, mark).is_empty(), "halting a harvest costs nothing and says nothing");
    let (code, why) = e.refusal(Command::Stop { fleet_id: fid });
    assert_eq!((code, why), (ErrorCode::InvalidCommand, format!("fleet {fid} has nothing to stop")));
}

#[test]
fn a_boarding_pays_on_the_tick_it_announces() {
    let mut e = Empire::join(world("test"), "Boarder");
    let fid = e.fid;
    let site = (-30i16..30)
        .flat_map(|q| (-30i16..30).map(move |r| Hex { q, r }))
        .find(|&h| e.engine.derelict_site_at(h).is_some() && e.engine.world_gen.generate_sector(h).npc_template.is_none())
        .expect("a derelict with no hostile group over it");
    deploy(&mut e.engine, fid, site, None);

    let mark = e.mark();
    e.order(Command::ExploreSite { fleet_id: fid }).expect("a boarding party goes in");
    let (announced_at, end_tick) = e.log[mark..]
        .iter()
        .find_map(|ev| match &ev.kind {
            EventKind::SiteExplorationStarted(s) => Some((ev.tick, s.end_tick)),
            _ => None,
        })
        .expect("the boarding announces when it pays");
    assert_eq!(end_tick, announced_at + u64::from(EXPLORE_DURATION_TICKS));
    assert_eq!(e.engine.current_tick(), announced_at);

    let mut saw_exploring = false;
    e.run_until(u64::from(EXPLORE_DURATION_TICKS) + 5, |e| {
        let fleet = &e.engine.fleets[&fid];
        if fleet.state == FleetStatus::Exploring {
            saw_exploring = true;
            assert_eq!(
                u64::from(e.fleet().cooldown_remaining),
                end_tick - e.engine.current_tick(),
                "the ring counts down to the announced tick, at tick {}",
                e.engine.current_tick(),
            );
            return false;
        }
        true
    });
    assert!(saw_exploring, "the boarding was visible before it paid");
    let paid: Vec<u64> = e.log[mark..]
        .iter()
        .filter(|ev| matches!(ev.kind, EventKind::SiteExplored(_) | EventKind::SiteAmbush(_)))
        .map(|ev| ev.tick)
        .collect();
    assert_eq!(paid, vec![end_tick], "loot or the ambush comes on the tick the boarding announced");
    let fleet = &e.engine.fleets[&fid];
    assert_ne!(fleet.state, FleetStatus::Exploring);
    assert_eq!(fleet.action_cooldown, 0, "the timer does not outlive the boarding");
}

#[test]
fn a_fight_that_catches_a_boarding_tells_its_owner_and_frees_the_fleet() {
    let mut e = Empire::join(world("test"), "Interrupted");
    let fid = e.fid;
    let site = (-30i16..30)
        .flat_map(|q| (-30i16..30).map(move |r| Hex { q, r }))
        .find(|&h| e.engine.derelict_site_at(h).is_some() && e.engine.world_gen.generate_sector(h).npc_template.is_none())
        .expect("a derelict with no hostile group over it");
    deploy(&mut e.engine, fid, site, Some(MIGHTY));

    let from = e.engine.world_gen.connected_neighbors(site).slice().iter().copied()
        .find(|c| !world::is_home_ground(*c) && !world::is_hub(*c))
        .expect("a lane into the derelict's sector");
    let lands = e.engine.current_tick() + 5;
    let npc_id = 900_000;
    horde(&mut e.engine, npc_id, from, &[(ShipClass::LightFighter, 1)], 1.0);
    edit(&mut e.engine, |s| {
        let patrol = s.npc_fleets.get_mut(&npc_id).unwrap();
        patrol.behavior = NpcBehaviorType::Patrol;
        patrol.home_sector = site;
        patrol.strayed = 1;
        patrol.errand = Some(Errand { quarry: None, to: site, arrives: lands });
    });

    let mark = e.mark();
    e.order(Command::ExploreSite { fleet_id: fid }).expect("a boarding party goes in");
    let quiet = e.engine.derelict_site_at(site).expect("the site stands").1;
    e.run_until(u64::from(EXPLORE_DURATION_TICKS) + 5, |e| e.engine.current_tick() >= lands);
    let joined = e.since(mark, |k| match k {
        EventKind::CombatStarted(c) if c.enemy_fleet_id == npc_id => Some((c.player_fleet_id, c.sector)),
        _ => None,
    });
    assert_eq!(joined, vec![(fid, site)], "the patrol came back to its sector in the middle of the boarding and fired on the fleet");

    e.run_until(30, |e| e.engine.fleets[&fid].state != FleetStatus::InCombat);
    let told = alerts_since(&e, mark);
    assert_eq!(told.len(), 1, "{told:?}");
    let fleet = &e.engine.fleets[&fid];
    assert_ne!(fleet.state, FleetStatus::Exploring, "the boarding does not resume");
    assert_eq!(fleet.action_cooldown, 0, "the boarding timer does not outlive the fight");
    assert_eq!(e.engine.derelict_site_at(site).expect("the site was not looted").1, quiet, "a fight is not a backing-out: the site is not put on alert");
    assert_eq!(
        told[0].message,
        format!("fleet {fid} was caught in a fight at {site} while boarding the derelict: the boarding is abandoned and nothing was recovered"),
    );
    assert!(matches!(told[0].level, AlertLevel::Warning) && told[0].fleet_id == Some(fid) && told[0].sector == Some(site), "{:?}", told[0]);
}

#[test]
fn backing_out_of_a_boarding_tells_its_owner_the_site_is_on_alert() {
    let mut e = Empire::join(world("test"), "Coldfeet");
    let fid = e.fid;
    let site = (-30i16..30)
        .flat_map(|q| (-30i16..30).map(move |r| Hex { q, r }))
        .find(|&h| e.engine.derelict_site_at(h).is_some() && e.engine.world_gen.generate_sector(h).npc_template.is_none())
        .expect("a derelict with no hostile group over it");
    deploy(&mut e.engine, fid, site, None);
    let risk = |e: &Empire| e.engine.derelict_site_at(site).expect("the site stands").1;
    let quiet = risk(&e);

    e.order(Command::ExploreSite { fleet_id: fid }).expect("a boarding party goes in");
    let mark = e.mark();
    e.order(Command::Stop { fleet_id: fid }).expect("stop pulls it out");
    let told = alerts_since(&e, mark);
    assert_eq!(told.len(), 1, "{told:?}");
    assert_eq!(
        told[0].message,
        format!("fleet {fid} backed out of the derelict at {site}: nothing was recovered and the site is on alert, so the next boarding there is likelier to be ambushed"),
    );
    assert!(matches!(told[0].level, AlertLevel::Warning), "{:?}", told[0]);
    assert_eq!((e.engine.fleets[&fid].state, risk(&e)), (FleetStatus::Idle, quiet + 1), "which is what the engine did");
}

#[test]
fn a_recall_tells_its_owner_what_the_jump_cost_in_fuel_in_hull_and_in_a_jump_abandoned() {
    let mut e = Empire::join(world("persistent"), "Limper");
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    let far = Hex { q: home.q + 25, r: home.r };
    muster(&mut e.engine, fid, &[(ShipClass::LightFighter, 20)], far);
    let fuel = e.engine.recall_fuel_cost(&e.engine.fleets[&fid]);
    let burn = f64::from(ShipClass::LightFighter.base_stats().fuel);
    assert_eq!(fuel, 20.0 * burn * 25.0 * 2.0, "twenty Light Fighters at OGame's fuel figure each, 25 hexes, doubled");
    assert_eq!(fuel, 20_000.0);

    let mark = e.mark();
    e.order(Command::Recall { fleet_id: fid }).expect("recall");
    let back = e.fleet();
    assert_eq!((back.location, back.ship_count), (home, 20), "a whole hull loses at most four fifths of itself: none broke up");
    let told = alerts_since(&e, mark);
    assert_eq!(told.len(), 2, "one event for the jump and one for the dock: {told:?}");
    let jump = format!("fleet {fid} made the emergency jump home from {far} for 20000 fuel: ");
    let harm = told[0].message.strip_prefix(&jump).unwrap_or_else(|| panic!("{}", told[0].message));
    let figures: Vec<u64> = harm.split(' ').filter_map(|word| word.parse().ok()).collect();
    let [hurt, 20, hull] = figures[..] else { panic!("the ships harmed, of how many, and the hull they lost: {harm}") };
    assert_eq!(harm, format!("{hurt} of its 20 ships took {hull} hull damage"));
    assert!(hurt > 2 && hurt < 20, "half the ships are damaged at this range, give or take: {hurt}");
    assert!((hurt * 80..=hurt * 320).contains(&hull), "each lost 20 to 80 percent of a Light Fighter's 400 hull: {hull}");
    assert!(matches!(told[0].level, AlertLevel::Warning) && told[0].sector == Some(far), "{:?}", told[0]);
    assert_eq!(
        told[1].message,
        format!("fleet {fid} was mended at the home dock: {hurt} ship(s) made whole, {hull} hull restored, for nothing"),
    );
    assert!(back.hull == back.hull_max && back.repair.is_none(), "home mends at once and for nothing");

    let mut e = Empire::join(world("persistent"), "Drilled");
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    let chain = outward_chain(&e.engine, home, 2);
    edit(&mut e.engine, |s| s.players.get_mut(&pid).unwrap().research.hyperspace_technology = 10);
    e.travel(chain[0]);
    e.run_until(200, |e| e.engine.fleets[&fid].action_cooldown == 0);
    e.order(Command::Move { fleet_id: fid, target: chain[1] }).expect("a jump further out");
    let mark = e.mark();
    e.order(Command::Recall { fleet_id: fid }).expect("recall from one hop out, a jump charging");
    let told = alerts_since(&e, mark);
    assert_eq!(told.len(), 1, "{told:?}");
    assert_eq!(
        told[0].message,
        format!(
            "fleet {fid} made the emergency jump home from {} for 80 fuel, abandoning the jump to {} it had burned 40 fuel for: no ship was damaged",
            chain[0], chain[1],
        ),
    );
    assert!(matches!(told[0].level, AlertLevel::Info), "a jump that harmed nothing is news, not a warning: {:?}", told[0]);
    assert_eq!(e.fleet().location, home);
}

#[test]
fn a_harvest_that_ends_by_itself_says_why_once_and_a_fleet_under_orders_leaves_it_to_its_doctrine() {
    let seam = |e: &Empire| {
        let home = e.engine.players[&e.pid].homeworld;
        let quiet = |h: Hex| e.engine.world_gen.generate_sector(h).npc_template.is_none();
        (-3i16..=3)
            .flat_map(|q| (-3i16..=3).map(move |r| Hex { q: home.q + q, r: home.r + r }))
            .filter(|&h| h != home && quiet(h) && e.engine.world_gen.generate_sector(h).metal_density != Density::None)
            .find(|&h| lane_path(&e.engine, h, home, 3).is_some_and(|way| way.into_iter().all(quiet)))
            .expect("a metal seam near home with a quiet way back")
    };
    let stopped = |e: &Empire, mark: usize| -> Vec<String> {
        alerts_since(e, mark).into_iter().map(|a| a.message).filter(|m| m.contains("stopped mining")).collect()
    };

    let mut e = Empire::join(world("test"), "Miner");
    let fid = e.fid;
    let at = seam(&e);
    muster(&mut e.engine, fid, &[(ShipClass::LightFighter, 2)], at);
    let hold = e.fleet().cargo_capacity;
    edit(&mut e.engine, |s| s.fleets.get_mut(&fid).unwrap().cargo.crystal = hold - 1.0);
    let mark = e.mark();
    e.order(Command::Harvest { fleet_id: fid, resource: HarvestResource::Metal }).expect("harvest");
    e.run_until(60, |e| e.engine.fleets[&fid].state != FleetStatus::Harvesting);
    e.run(30);
    assert_eq!(
        stopped(&e, mark),
        [format!("fleet {fid} stopped mining at {at}: its hold is full; it carries {hold:.0} of {hold:.0}")],
        "said when the harvest ends, and not again while the fleet stands there",
    );
    let told = alerts_since(&e, mark);
    assert!(matches!(told[0].level, AlertLevel::Warning) && told[0].fleet_id == Some(fid) && told[0].sector == Some(at), "{:?}", told[0]);

    let mut e = Empire::join(world("test"), "Stripper");
    let fid = e.fid;
    muster(&mut e.engine, fid, &[(ShipClass::Recycler, 40)], at);
    let hold = e.fleet().cargo_capacity;
    let mark = e.mark();
    e.order(Command::Harvest { fleet_id: fid, resource: HarvestResource::Metal }).expect("harvest");
    e.run_until(4000, |e| e.engine.fleets[&fid].state != FleetStatus::Harvesting);
    e.run(30);
    let aboard = e.fleet().cargo.total();
    assert!(aboard > 0.0 && aboard < hold, "forty Recyclers strip the metal with room to spare: {aboard} of {hold}");
    assert_eq!(
        stopped(&e, mark),
        [format!("fleet {fid} stopped mining at {at}: no metal is left to mine there; it carries {aboard:.0} of {hold:.0}")],
    );

    let mut e = Empire::join(world("test"), "Ordered");
    let (pid, fid) = (e.pid, e.fid);
    muster(&mut e.engine, fid, &[(ShipClass::LightFighter, 2)], at);
    let hold = e.fleet().cargo_capacity;
    edit(&mut e.engine, |s| s.fleets.get_mut(&fid).unwrap().cargo.crystal = hold - 1.0);
    e.order(Command::Harvest { fleet_id: fid, resource: HarvestResource::Metal }).expect("harvest");
    e.engine.handle_policy_update(pid, fid, PolicyPreset::MineAndReturn, None).expect("orders for a fleet at work");
    let mark = e.mark();
    e.run_until(60, |e| e.engine.fleets[&fid].state != FleetStatus::Harvesting);
    e.run_until(60, |e| !policy_reasons(e, mark, "move").is_empty());
    let said = policy_reasons(&e, mark, "move");
    assert!(said[0].starts_with("returning: cargo 100%"), "the doctrine says what the full hold means for it: {said:?}");
    assert_eq!(stopped(&e, mark), Vec::<String>::new(), "so the fleet is not reported twice");
}

fn reports(e: &Empire, mark: usize) -> Vec<(String, String)> {
    e.since(mark, |k| match k {
        EventKind::PolicyAction(p) if p.action != "engage" => Some((p.action.clone(), p.reason.clone())),
        _ => None,
    })
}

fn mended(e: &Empire, mark: usize) -> Vec<String> {
    alerts_since(e, mark).into_iter().map(|a| a.message).filter(|m| m.contains("was mended at")).collect()
}

#[test]
fn a_ship_from_the_yard_or_a_fleet_merged_in_leaves_the_docked_fleet_fit_for_its_orders() {
    let mut e = Empire::join(world("test"), "Yardmaster");
    let (pid, fid) = (e.pid, e.fid);
    edit(&mut e.engine, |s| {
        let player = s.players.get_mut(&pid).unwrap();
        player.buildings.shipyard = 1;
        player.research.combustion_drive = 1;
        player.research.weapons_technology = 3;
    });
    e.set_stockpile(ShipClass::LightFighter.build_cost());
    e.order(Command::BuildShip { ship_class: ShipClass::LightFighter, count: 1 }).expect("a fighter ordered");
    e.run_until(400, |e| e.fleet().ship_count == 3);
    let docked = e.fleet();
    assert_eq!((docked.refit.weapons, docked.refit_due), (3, false), "research reaches a docked fleet at once, and the yard's ship with it");
    assert_eq!((docked.ships[0].unit_attack, docked.attack), (65, 195), "a Light Fighter's 50 with three levels of Weapons Technology");
    assert_eq!((docked.hull, docked.repair.is_none()), (docked.hull_max, true), "the ship the yard delivers is whole");

    e.order(Command::Split { fleet_id: fid, ships: counts(&[(ShipClass::LightFighter, 1)]), damaged_first: false }).expect("split one off");
    let other = e.state().fleets.iter().find(|f| f.id != fid).expect("the new fleet").id;
    e.order(Command::Merge { fleet_id: fid, other_fleet_id: other }).expect("and merge it back");
    assert_eq!((e.fleet().ship_count, e.fleet().refit.weapons), (3, 3), "a merge at home leaves the fleet at its owner's levels");

    let mark = e.mark();
    e.engine.handle_policy_update(pid, fid, PolicyPreset::Prospect, None).expect("orders");
    e.run_until(30, |e| !reports(e, mark).is_empty());
    let first = reports(&e, mark).remove(0);
    assert!(!first.1.contains("mended"), "its first report under orders is not a wait for the dock: {first:?}");
}

#[test]
fn a_patrol_that_fought_the_convoy_over_its_own_dock_is_mended_there_and_never_waits_for_it() {
    let mut e = (1u64..400)
        .find_map(|seed| {
            let e = Empire::join(GameEngine::new(seed, Pace::parse("blitz").expect("known pace")), "Warden");
            let home = e.engine.players[&e.pid].homeworld;
            e.engine.pending_template_npc(home).is_some().then_some(e)
        })
        .expect("a world whose first home has a convoy standing over it");
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    muster(&mut e.engine, fid, &[(ShipClass::LightFighter, 3)], home);
    assert_eq!(e.fleet().hull, e.fleet().hull_max);

    let mark = e.mark();
    let params = PolicyParams { max_range: 1, engage_ratio_x10: 15, ..PolicyParams::default() };
    e.engine.handle_policy_update(pid, fid, PolicyPreset::PatrolHome, Some(params)).expect("patrol orders");
    e.run_until(10, |e| !reports(e, mark).is_empty());
    assert_eq!(reports(&e, mark), [("attack".to_string(), "engaging hostiles".to_string())], "its first act under orders is the fight");

    let ended = |e: &Empire| {
        e.since(mark, |k| match k {
            EventKind::CombatEnded(c) => Some((c.sector, c.player_victory)),
            _ => None,
        })
    };
    e.run_until(300, |e| !ended(e).is_empty());
    assert_eq!(ended(&e), [(home, true)], "three fighters beat the convoy, in the home sector");
    let hull_lost: u64 = e
        .since(mark, |k| match k {
            EventKind::BattleRound(round) => Some(round.hostiles.hull_damage),
            _ => None,
        })
        .into_iter()
        .sum();
    assert!(hull_lost > 0, "both sides fire at once, so the convoy shot back");

    e.run(2);
    let told = mended(&e, mark);
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(
        told[0].starts_with(&format!("fleet {fid} was mended at the home dock: "))
            && told[0].ends_with(&format!(" {hull_lost} hull restored, for nothing")),
        "the fight was over the dock, which mends at once and for nothing: {}",
        told[0],
    );
    assert_eq!((e.fleet().hull, e.fleet().repair.is_none()), (e.fleet().hull_max, true));
    assert!(reports(&e, mark).iter().all(|(_, why)| !why.contains("mended")), "so the patrol never waited for it: {:?}", reports(&e, mark));
}

/// A patrol of `ships` at home under `params`, its fuel unbounded, every
/// lair within a hex of its beat cleared: only the wreckage put out for it
/// decides where it flies.
fn quiet_patrol(name: &str, ships: &[(ShipClass, u32)], params: PolicyParams) -> (Empire, Hex) {
    let mut e = Empire::join(world("persistent"), name);
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    let lairs: Vec<u32> = iac_shared::hex::hex_spiral(home, u16::from(params.max_range) + 1)
        .filter(|h| e.engine.world_gen.generate_sector(*h).npc_template.is_some())
        .map(|h| h.to_key())
        .collect();
    muster(&mut e.engine, fid, ships, home);
    edit(&mut e.engine, |s| {
        let player = s.players.get_mut(&pid).unwrap();
        (player.buildings.metal_storage, player.buildings.deuterium_tank) = (8, 8);
        player.resources = Resources { metal: 20_000.0, crystal: 20_000.0, deuterium: 50_000.0 };
        for key in lairs {
            s.sector_overrides.entry(key).or_insert_with(SectorOverride::default).npc_cleared_tick = Some(s.tick);
        }
    });
    let params = PolicyParams { fuel_budget_pct: 100, ..params };
    e.engine.handle_policy_update(pid, fid, PolicyPreset::PatrolHome, Some(params)).expect("patrol orders");
    (e, home)
}

fn drop_wreckage(e: &mut Empire, at: Hex, pile: Resources) {
    edit(&mut e.engine, |s| {
        let ov = s.sector_overrides.entry(at.to_key()).or_insert_with(SectorOverride::default);
        ov.salvage = Some(pile);
        ov.salvage_despawn_tick = Some(s.tick + 1_000_000);
    });
}

const BEAT: PolicyParams = PolicyParams {
    min_fuel_pct: 10,
    cargo_return_pct: 85,
    max_range: 3,
    engage_ratio_x10: 15,
    disengage_ratio_x10: 9,
    fuel_budget_pct: 100,
    refine: false,
};

#[test]
fn a_patrol_flies_out_for_wreckage_it_did_not_make_and_brings_it_home() {
    let (mut e, home) = quiet_patrol("Gleaner", &[(ShipClass::LightFighter, 10)], BEAT);
    let fid = e.fid;
    let at = outward_chain(&e.engine, home, 2)[1];
    drop_wreckage(&mut e, at, Resources { metal: 300.0, crystal: 100.0, deuterium: 0.0 });

    let mark = e.mark();
    e.run_until(1200, |e| wreckage(e, at).total() == 0.0 && e.engine.fleets[&fid].location == home);
    let acts = reports(&e, mark);
    assert_eq!(
        acts.iter().filter(|(action, _)| action != "move").cloned().collect::<Vec<_>>(),
        [
            ("salvage".to_string(), format!("scooping wreckage at {at} that it did not make")),
            ("unload".to_string(), "unloaded 300 metal and 100 crystal into the stockpile".to_string()),
        ],
        "{acts:?}",
    );
    let moves = policy_reasons(&e, mark, "move");
    assert_eq!(moves[0], format!("collecting the wreckage at {at}"), "{moves:?}");
    assert!(
        moves[moves.len() - 1].starts_with("returning: no contact within 3 hops of home that the fleet rates 1.5x against, and no wreckage"),
        "with the pile aboard and nothing left in range it goes home: {moves:?}",
    );
    assert_eq!(e.engine.fleets[&fid].cargo.total(), 0.0);
}

#[test]
fn a_patrol_with_a_full_hold_takes_it_home_unloads_is_mended_and_goes_back_for_the_rest() {
    let (mut e, home) = quiet_patrol("Hauler", &[(ShipClass::LightFighter, 10)], BEAT);
    let fid = e.fid;
    let at = outward_chain(&e.engine, home, 1)[0];
    edit(&mut e.engine, |s| {
        let fleet = s.fleets.get_mut(&fid).unwrap();
        fleet.ships = squadron(ShipClass::LightFighter, 9, &[350]);
        fleet.location = at;
    });
    let hold = e.fleet().cargo_capacity;
    drop_wreckage(&mut e, at, Resources { metal: 1.6 * hold, crystal: 0.0, deuterium: 0.0 });

    let mark = e.mark();
    e.run_until(1200, |e| wreckage(e, at).total() == 0.0);
    assert_eq!(
        reports(&e, mark),
        [
            ("salvage".to_string(), format!("scooping wreckage at {at} that it did not make")),
            ("move".to_string(), "returning to unload the salvage: cargo 100%".to_string()),
            ("unload".to_string(), format!("unloaded {} metal into the stockpile", hold.ceil())),
            ("move".to_string(), format!("collecting the wreckage at {at}")),
            ("salvage".to_string(), format!("scooping wreckage at {at} that it did not make")),
        ],
    );
    let told = mended(&e, mark);
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(told[0].starts_with(&format!("fleet {fid} was mended at the home dock: 1 ship(s) made whole")), "{}", told[0]);
    assert_eq!(e.fleet().hull, e.fleet().hull_max);
}

#[test]
fn a_patrol_whose_next_pile_would_not_fit_the_room_left_unloads_first() {
    let (mut e, home) = quiet_patrol("Tidy", &[(ShipClass::LightFighter, 10)], PolicyParams { cargo_return_pct: 100, ..BEAT });
    let fid = e.fid;
    let chain = outward_chain(&e.engine, home, 2);
    let hold = e.fleet().cargo_capacity;
    edit(&mut e.engine, |s| {
        let fleet = s.fleets.get_mut(&fid).unwrap();
        fleet.location = chain[0];
        fleet.cargo.metal = hold - 1.0;
    });
    drop_wreckage(&mut e, chain[1], Resources { metal: hold, crystal: 0.0, deuterium: 0.0 });

    let mark = e.mark();
    e.run_until(1200, |e| wreckage(e, chain[1]).total() == 0.0);
    let acts = reports(&e, mark);
    assert_eq!(
        acts[..3],
        [
            (
                "move".to_string(),
                format!("returning to unload the salvage: the hold has room for 1 of the {hold:.0} units of wreckage at {}", chain[1]),
            ),
            ("unload".to_string(), format!("unloaded {} metal into the stockpile", (hold - 1.0).ceil())),
            ("move".to_string(), format!("collecting the wreckage at {}", chain[1])),
        ],
        "{acts:?}",
    );
}

#[test]
fn a_patrol_passes_over_wreckage_not_worth_the_fuel_to_fetch_it() {
    let (mut e, home) = quiet_patrol("Frugal", &[(ShipClass::LightFighter, 10)], BEAT);
    let fid = e.fid;
    let at = outward_chain(&e.engine, home, 3)[2];
    let burned = 6.0 * e.engine.hop_fuel_cost(&e.engine.fleets[&fid]) * FUEL_DEUT_PER_UNIT;
    assert!(burned < e.fleet().cargo_capacity, "the pile fits the hold: {burned:.0}");

    drop_wreckage(&mut e, at, Resources { metal: burned, crystal: 0.0, deuterium: 0.0 });
    let mark = e.mark();
    e.run(300);
    assert!(policy_reasons(&e, mark, "move").is_empty(), "{burned:.0} metal for {burned:.0} deuterium of fuel: {:?}", reports(&e, mark));
    let held = policy_reasons(&e, mark, "hold");
    assert!(
        !held.is_empty()
            && held.iter().all(|why| why == "no contact within 3 hops of home that the fleet rates 1.5x against, and no wreckage that repays the fuel"),
        "{held:?}",
    );

    let hold = e.fleet().cargo_capacity;
    drop_wreckage(&mut e, at, Resources { metal: hold, crystal: 0.0, deuterium: 0.0 });
    let mark = e.mark();
    e.run_until(300, |e| wreckage(e, at).total() == 0.0);
    assert_eq!(policy_reasons(&e, mark, "move")[0], format!("collecting the wreckage at {at}"));
}

#[test]
fn a_patrol_with_a_small_cargo_in_it_brings_home_more_of_the_same_wreckage() {
    let haul = |ships: &[(ShipClass, u32)]| {
        let (mut e, home) = quiet_patrol("Convoy", ships, BEAT);
        let at = outward_chain(&e.engine, home, 2)[1];
        drop_wreckage(&mut e, at, Resources { metal: 200_000.0, crystal: 0.0, deuterium: 0.0 });
        let mark = e.mark();
        e.run(600);
        let trips = policy_reasons(&e, mark, "unload").len();
        (200_000.0 - wreckage(&e, at).metal - e.fleet().cargo.metal, trips, e.fleet().cargo_capacity)
    };
    let (warships, trips, hold) = haul(&[(ShipClass::LightFighter, 10)]);
    let (with_cargo, cargo_trips, cargo_hold) = haul(&[(ShipClass::LightFighter, 10), (ShipClass::SmallCargo, 1)]);
    assert!(
        with_cargo > 3.0 * warships,
        "{warships:.0} metal home in {trips} trips with a hold of {hold:.0}, {with_cargo:.0} in {cargo_trips} with a Small Cargo and a hold of {cargo_hold:.0}",
    );
}

#[test]
fn a_jump_takes_the_ticks_the_fleet_reports() {
    use ShipClass as S;
    type Classes = &'static [(ShipClass, u32)];
    let fighters: Classes = &[(S::LightFighter, 2)];
    let with_a_heavy: Classes = &[(S::LightFighter, 1), (S::HeavyFighter, 1)];
    // The drives as (Combustion, Impulse, Hyperspace). A fleet jumps at the
    // speed of its slowest ship: max(1, ceil(sqrt(50,000 / speed))) ticks.
    let cases: [(Classes, (u8, u8, u8), u16); 13] = [
        (fighters, (0, 0, 0), 2),
        (fighters, (9, 0, 0), 2),
        (with_a_heavy, (0, 0, 0), 3),
        (with_a_heavy, (0, 2, 0), 2),
        (&[(S::EspionageProbe, 2)], (0, 0, 0), 1),
        (&[(S::SmallCargo, 1)], (0, 0, 0), 4),
        (&[(S::SmallCargo, 1)], (2, 0, 0), 3),
        (&[(S::SmallCargo, 1)], (2, 5, 0), 2),
        (&[(S::Recycler, 1), (S::Cruiser, 3)], (6, 4, 0), 4),
        (&[(S::Cruiser, 3)], (0, 4, 0), 2),
        (&[(S::Battleship, 1)], (0, 0, 4), 2),
        (&[(S::Bomber, 1)], (0, 6, 8), 2),
        (&[(S::Deathstar, 1)], (0, 0, 7), 13),
    ];
    for (classes, drives, ticks) in cases {
        let mut e = Empire::join(world("test"), "Jumper");
        let (pid, fid) = (e.pid, e.fid);
        let home = e.engine.players[&pid].homeworld;
        let chain = outward_chain(&e.engine, home, 3);
        sweep(&mut e.engine, &chain);
        muster(&mut e.engine, fid, classes, home);
        edit(&mut e.engine, |s| {
            let research = &mut s.players.get_mut(&pid).unwrap().research;
            (research.combustion_drive, research.impulse_drive, research.hyperspace_drive) = drives;
        });
        assert_eq!(e.fleet().jump_ticks, ticks, "{classes:?} with drives {drives:?}");

        for &hop in &chain {
            assert_eq!(e.order(Command::Move { fleet_id: fid, target: hop }), Ok(()), "it takes the next order the tick it lands");
            let mut flown = 0;
            while e.fleet().location != hop {
                e.run(1);
                flown += 1;
                assert!(flown <= ticks, "{classes:?} with drives {drives:?}: not at {hop} after {flown} ticks");
            }
            assert_eq!(flown, ticks, "{classes:?} with drives {drives:?}");
        }
    }
}

fn adrift(e: &mut Empire, at: Hex, fuel: f64, deuterium: f64) {
    let fid = e.fid;
    edit(&mut e.engine, |s| {
        let fleet = s.fleets.get_mut(&fid).unwrap();
        fleet.location = at;
        fleet.fuel = fuel;
        fleet.cargo = Resources { metal: 7.0, deuterium, ..Resources::default() };
    });
}

fn refined(e: &Empire, mark: usize) -> String {
    let told = alerts_since(e, mark);
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(matches!(told[0].level, AlertLevel::Info) && told[0].fleet_id == Some(e.fid), "{:?}", told[0]);
    told[0].message.clone()
}

#[test]
fn a_fleet_too_dry_to_fly_home_refines_the_deuterium_in_its_hold_at_a_loss_and_flies_back() {
    let (mut e, chain) = outpost("Refiner", 2);
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    adrift(&mut e, chain[1], 20.0, 16.0);

    let before = e.fleet();
    assert_eq!(
        (before.jump_fuel, before.home_fuel, before.fuel_max),
        (40.0, 80.0, 400.0),
        "two Light Fighters: 20 fuel each a jump, and a tank of ten jumps"
    );
    assert_eq!(e.order(Command::Move { fleet_id: fid, target: chain[0] }), Err(ErrorCode::InsufficientFuel));
    let offer = before.refine.expect("the state says what refining would do");
    assert_eq!(offer.deuterium, 16.0, "all the deuterium aboard: the tank has room for what it makes");
    assert!((offer.fuel - 170.0).abs() < 1e-3, "{offer:?}");
    let at_the_dock = offer.deuterium / FUEL_DEUT_PER_UNIT;
    assert!((offer.fuel / at_the_dock - (1.0 - REFINE_LOSS)).abs() < 1e-5, "the dock's 50 less the loss");

    let mark = e.mark();
    e.order(Command::Refine { fleet_id: fid }).expect("refine");
    assert_eq!(
        refined(&e, mark),
        format!("fleet {fid} refined 16 deuterium into 170 fuel: the tank stands at 190 of 400, and the hold has no deuterium left"),
    );
    let after = e.fleet();
    assert_eq!(after.fuel, 20.0 + offer.fuel, "what the state offered is what the order did");
    assert_eq!((after.cargo.deuterium, after.cargo.metal), (0.0, 7.0), "only the deuterium is touched");
    assert_eq!(after.refine, None);
    assert_eq!(
        e.refusal(Command::Refine { fleet_id: fid }),
        (ErrorCode::NoResources, format!("the hold of fleet {fid} carries no deuterium to refine")),
    );
    assert_eq!(fuel_of(&e, fid), after.fuel, "a refused order changes nothing");

    assert!(after.fuel >= after.home_fuel && after.range_hops >= 1, "{after:?}");
    e.travel(home);
    assert_eq!(e.fleet().location, home);
}

#[test]
fn refining_stops_when_the_tank_is_full_and_keeps_the_rest_of_the_deuterium() {
    let (mut e, chain) = outpost("Topped", 2);
    let fid = e.fid;
    adrift(&mut e, chain[1], 400.0 - 8.5, 10.0);

    let offer = e.fleet().refine.expect("the state says what refining would do");
    assert!((offer.deuterium - 0.8).abs() < 1e-4 && offer.fuel == 8.5, "only what the tank has room for: {offer:?}");
    let mark = e.mark();
    e.order(Command::Refine { fleet_id: fid }).expect("refine");
    assert_eq!(
        refined(&e, mark),
        format!("fleet {fid} refined 0.8 deuterium into 8.5 fuel: the tank stands at 400 of 400, and 9.2 deuterium is left in the hold"),
    );
    let after = e.fleet();
    assert_eq!(after.fuel, after.fuel_max, "the tank is full to the unit");
    assert!((after.cargo.deuterium - 9.2).abs() < 1e-4, "{:?}", after.cargo);
    assert_eq!(after.refine, None);
    assert_eq!(
        e.refusal(Command::Refine { fleet_id: fid }),
        (ErrorCode::InvalidCommand, format!("the tank of fleet {fid} is full (400 of 400 fuel); refining would add nothing")),
    );
    assert_eq!(e.fleet().cargo, after.cargo, "a refused order changes nothing");
}

#[test]
fn deuterium_loaded_at_the_dock_is_refined_like_any_other_once_the_tank_has_room() {
    let mut e = Empire::join(world("test"), "Tanker");
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    let out = outward_chain(&e.engine, home, 2)[1];
    e.set_stockpile(Resources { deuterium: 50.0, ..Resources::default() });

    e.order(Command::Load { fleet_id: fid, resource: ResourceKind::Deuterium, amount: Some(5.0) }).expect("load");
    assert_eq!(
        e.refusal(Command::Refine { fleet_id: fid }),
        (ErrorCode::InvalidCommand, format!("the tank of fleet {fid} is full (400 of 400 fuel); refining would add nothing")),
        "the dock keeps the tank full, so there is nothing to refine for at home",
    );

    e.travel(out);
    assert_eq!((fuel_of(&e, fid), e.fleet().cargo.deuterium), (360.0, 5.0));
    e.order(Command::Refine { fleet_id: fid }).expect("refine");
    let after = e.fleet();
    assert_eq!(after.fuel, 400.0, "the jump the dock did not pay for is made good from the hold");
    assert!((after.cargo.deuterium - (5.0 - 40.0 / 10.625)).abs() < 1e-4, "{:?}", after.cargo);
}

/// A prospector `hops` out with `fuel` in its tank and `deuterium` in its
/// hold, given orders with no reserve; the mark is taken before the orders.
fn dry_prospector(hops: usize, fuel: f64, deuterium: f64, refine: bool) -> (Empire, usize) {
    let (mut e, chain) = outpost("Thirsty", hops);
    let (pid, fid) = (e.pid, e.fid);
    adrift(&mut e, chain[hops - 1], fuel, deuterium);
    edit(&mut e.engine, |s| s.fleets.get_mut(&fid).unwrap().refit = MIGHTY);
    let mark = e.mark();
    let params = PolicyParams { min_fuel_pct: 0, max_range: 4, refine, ..PolicyParams::default() };
    e.engine.handle_policy_update(pid, fid, PolicyPreset::Prospect, Some(params)).unwrap();
    e.absorb();
    (e, mark)
}

#[test]
fn a_doctrine_told_to_refine_does_so_before_it_would_turn_home_for_fuel_and_turns_home_only_if_that_was_not_enough() {
    let (mut e, mark) = dry_prospector(1, 100.0, 16.0, false);
    let fid = e.fid;
    assert!(!policy_reasons(&e, mark, "engage")[0].contains("refining"), "the switch is off unless set");
    e.run_until(60, |e| !policy_reasons(e, mark, "move").is_empty());
    let moves = policy_reasons(&e, mark, "move");
    assert!(moves[0].starts_with("returning: a jump to"), "without the switch it turns home: {moves:?}");
    assert!(policy_reasons(&e, mark, "refine").is_empty());
    assert_eq!(e.engine.fleets[&fid].cargo.deuterium, 16.0, "with its deuterium for the stockpile");

    let (mut e, mark) = dry_prospector(1, 100.0, 16.0, true);
    assert!(
        policy_reasons(&e, mark, "engage")[0].ends_with("fuel budget 20% of deuterium income, refining from the hold when short of fuel)"),
        "{:?}", policy_reasons(&e, mark, "engage"),
    );
    e.run_until(60, |e| !policy_reasons(e, mark, "move").is_empty());
    let refined = policy_reasons(&e, mark, "refine");
    assert_eq!(refined.len(), 1, "{refined:?}");
    assert!(
        refined[0].starts_with("refined 16 deuterium from the hold into 170 fuel: a jump to ")
            && refined[0].ends_with(" would leave 60 fuel against 80 needed to return home"),
        "{refined:?}",
    );
    assert_eq!(policy_reasons(&e, mark, "move")[0], "pushing frontier", "and then flies on");
    assert_eq!(e.engine.fleets[&fid].cargo.deuterium, 0.0);

    let (mut e, mark) = dry_prospector(2, 20.0, 16.0, true);
    e.run_until(60, |e| !policy_reasons(e, mark, "move").is_empty());
    assert_eq!(
        policy_reasons(&e, mark, "refine"),
        ["refined 16 deuterium from the hold into 170 fuel: fuel 20 is below the route home 80 plus reserve 0"],
    );
    assert!(!policy_reasons(&e, mark, "move")[0].starts_with("returning"), "{:?}", policy_reasons(&e, mark, "move"));

    let (mut e, mark) = dry_prospector(2, 20.0, 2.0, true);
    e.run_until(60, |e| !policy_reasons(e, mark, "move").is_empty());
    assert_eq!(
        policy_reasons(&e, mark, "refine"),
        ["refined 2 deuterium from the hold into 21.3 fuel: fuel 20 is below the route home 80 plus reserve 0"],
    );
    assert_eq!(
        policy_reasons(&e, mark, "move")[0],
        "returning: fuel 41 is below the route home 80 plus reserve 0",
        "what it refined was not enough, so it turns home",
    );
}

#[test]
fn refining_is_refused_in_a_fight_and_while_a_jump_charges_and_goes_on_beside_a_harvest_or_a_boarding() {
    let (mut e, rival, rival_fleet) = with_rival("Chemist");
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    let chain = outward_chain(&e.engine, home, 2);
    let idle = |e: &Empire| {
        let fleet = &e.engine.fleets[&fid];
        fleet.state == FleetStatus::Idle && fleet.action_cooldown == 0
    };

    let outcome = e.engine.execute(pid, Command::Refine { fleet_id: rival_fleet });
    assert_eq!(
        outcome.error,
        Some((ErrorCode::FleetNotFound, format!("fleet {rival_fleet} not found (or it has no ships left)"))),
    );
    assert!(e.engine.execute(rival, Command::Refine { fleet_id: fid }).error.is_some());

    adrift(&mut e, chain[1], 60.0, 1.0);
    e.order(Command::Move { fleet_id: fid, target: chain[0] }).expect("a jump toward home");
    assert_eq!(e.fleet().refine, None, "nothing is offered while the jump charges");
    assert_eq!(
        e.refusal(Command::Refine { fleet_id: fid }),
        (ErrorCode::OnCooldown, format!("fleet {fid} is charging a jump to {}; it can refine once it has arrived", chain[0])),
    );
    e.run_until(50, idle);
    e.order(Command::Refine { fleet_id: fid }).expect("arrived, it refines");

    let lair = lone_fighter_sector(&e.engine, NpcBehaviorType::Passive);
    deploy(&mut e.engine, fid, lair, Some(MIGHTY));
    adrift(&mut e, lair, 60.0, 1.0);
    e.order(Command::Attack { fleet_id: fid, target_fleet_id: template_npc_id(lair) }).expect("attack");
    assert_eq!(e.engine.fleets[&fid].state, FleetStatus::InCombat);
    assert_eq!(e.fleet().refine, None, "nothing is offered in a fight");
    assert_eq!(
        e.refusal(Command::Refine { fleet_id: fid }),
        (ErrorCode::OnCooldown, format!("fleet {fid} is in combat; nothing is refined in a fight")),
    );
    assert_eq!((fuel_of(&e, fid), e.engine.fleets[&fid].cargo.deuterium), (60.0, 1.0));
    e.run_until(20, idle);
    e.order(Command::Refine { fleet_id: fid }).expect("the fight over, it refines");

    let seam = find_sector(&e.engine, 3, |metal, _, _| metal != Density::None);
    sweep(&mut e.engine, &[seam]);
    adrift(&mut e, seam, 60.0, 1.0);
    e.order(Command::Harvest { fleet_id: fid, resource: HarvestResource::Auto }).expect("harvest");
    e.order(Command::Refine { fleet_id: fid }).expect("a harvesting fleet refines");
    assert_eq!(e.engine.fleets[&fid].state, FleetStatus::Harvesting, "and the harvest goes on");
    assert!(fuel_of(&e, fid) > 70.0);

    let site = (-30i16..30)
        .flat_map(|q| (-30i16..30).map(move |r| Hex { q, r }))
        .find(|&h| e.engine.derelict_site_at(h).is_some() && e.engine.world_gen.generate_sector(h).npc_template.is_none())
        .expect("a derelict with no hostile group over it");
    e.order(Command::Stop { fleet_id: fid }).expect("stop the harvest");
    e.run_until(10, idle);
    adrift(&mut e, site, 60.0, 1.0);
    e.order(Command::ExploreSite { fleet_id: fid }).expect("a boarding party goes in");
    let timer = e.engine.fleets[&fid].action_cooldown;
    e.order(Command::Refine { fleet_id: fid }).expect("a boarding fleet refines");
    let fleet = &e.engine.fleets[&fid];
    assert_eq!((fleet.state, fleet.action_cooldown), (FleetStatus::Exploring, timer), "and the boarding goes on");
}

fn newest_fleet(e: &Empire) -> u64 {
    e.engine.fleets.values().filter(|f| f.owner_id == e.pid).map(|f| f.id).max().expect("a fleet")
}

#[test]
fn split_takes_the_whole_ships_first_or_the_wounded_first_and_a_merge_brings_every_hull_back() {
    use ShipClass::{Cruiser, LightFighter};
    let (mut e, _) = outpost("Surgeon", 1);
    let fid = e.fid;
    edit(&mut e.engine, |s| {
        let mut ships = squadron(LightFighter, 7, &[100, 200, 300]);
        ships.absorb(squadron(Cruiser, 2, &[]));
        s.fleets.get_mut(&fid).unwrap().ships = ships;
    });
    let told = e.fleet();
    let fighters = told.ships.iter().find(|g| g.ship_class == LightFighter).unwrap();
    assert_eq!((told.ship_count, told.ships.len(), told.hull, told.hull_max), (12, 2, 3400 + 5400, 4000 + 5400));
    assert_eq!(
        (fighters.count, fighters.damaged, fighters.at_risk, fighters.hull, fighters.hull_max),
        (10, 3, 2, 3400, 4000),
        "three are damaged, and the two at or under 70 percent of their 400 hull may explode when next hit",
    );

    let split = |fleet_id: u64, ships: &[(ShipClass, u32)], damaged_first: bool| Command::Split {
        fleet_id,
        ships: counts(ships),
        damaged_first,
    };
    e.order(split(fid, &[(LightFighter, 8), (Cruiser, 1)], false)).expect("eight fighters and a cruiser");
    let sound = newest_fleet(&e);
    let mut taken = squadron(LightFighter, 7, &[300]);
    taken.absorb(squadron(Cruiser, 1, &[]));
    assert_eq!(e.engine.fleets[&sound].ships, taken, "the whole go first, and then the least damaged");
    let mut kept = squadron(LightFighter, 0, &[100, 200]);
    kept.absorb(squadron(Cruiser, 1, &[]));
    assert_eq!(e.engine.fleets[&fid].ships, kept);

    e.order(split(sound, &[(LightFighter, 3)], true)).expect("the wounded and two more");
    let limping = newest_fleet(&e);
    assert_eq!(e.engine.fleets[&limping].ships, squadron(LightFighter, 2, &[300]), "the damaged go first when asked");
    assert!(!e.engine.fleets[&sound].ships.has_damage());
    let quote = |id: u64| e.state().fleets.into_iter().find(|f| f.id == id).unwrap().repair;
    assert_eq!(quote(sound), None, "a fleet with no damage is quoted no repair");
    assert_eq!(quote(limping).map(|q| (q.ships, q.hull, q.home.ships, q.home.price)), Some((1, 100, 1, 0.0)));

    e.order(Command::Merge { fleet_id: fid, other_fleet_id: sound }).expect("merge the sound wing back");
    e.order(Command::Merge { fleet_id: fid, other_fleet_id: limping }).expect("and the wounded");
    let mut whole = squadron(LightFighter, 7, &[100, 200, 300]);
    whole.absorb(squadron(Cruiser, 2, &[]));
    assert_eq!(e.engine.fleets[&fid].ships, whole, "every ship is back with the hull it had");
    assert_eq!(e.state().fleets.len(), 1);
}

#[test]
fn research_reaches_a_fleet_when_it_docks_at_home_and_a_merged_fleet_fights_at_the_lower_levels_until_then() {
    use ShipClass::LightFighter;
    let mut e = Empire::join(world("test"), "Refitter");
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    let out = outward_chain(&e.engine, home, 1)[0];
    muster(&mut e.engine, fid, &[(LightFighter, 4)], home);
    let split = |fleet_id: u64, count: u32| Command::Split { fleet_id, ships: counts(&[(LightFighter, count)]), damaged_first: false };
    e.order(split(fid, 2)).expect("two stay at the dock");
    let stay = newest_fleet(&e);
    e.travel_fleet(fid, out);

    edit(&mut e.engine, |s| {
        let research = &mut s.players.get_mut(&pid).unwrap().research;
        (research.weapons_technology, research.armour_technology) = (2, 1);
    });
    e.run(2);
    let state_of = |e: &Empire, id: u64| e.state().fleets.into_iter().find(|f| f.id == id).unwrap();
    let levels = |e: &Empire, id: u64| {
        let f = state_of(e, id);
        (f.refit.weapons, f.refit.shielding, f.refit.armour, f.refit_due)
    };
    let figures = |e: &Empire, id: u64| {
        let f = state_of(e, id);
        (f.ships[0].unit_attack, f.ships[0].unit_shield, f.ships[0].unit_hull, f.attack, f.hull_max)
    };
    assert_eq!(levels(&e, fid), (0, 0, 0, true), "the fleet in the field fights with what it left the dock with");
    assert_eq!(figures(&e, fid), (50, 10, 400, 100, 800));
    assert_eq!(levels(&e, stay), (2, 0, 1, false), "research that finishes over a docked fleet reaches it at once");
    assert_eq!(figures(&e, stay), (60, 10, 440, 120, 880), "a tenth a level, as OGame has it");

    e.order(split(fid, 1)).expect("split in the field");
    let half = newest_fleet(&e);
    assert_eq!((levels(&e, half), levels(&e, fid)), ((0, 0, 0, true), (0, 0, 0, true)), "both halves keep the fleet's levels");

    e.travel_fleet(stay, out);
    assert_eq!(levels(&e, stay), (2, 0, 1, false), "the levels fly with the fleet");
    edit(&mut e.engine, |s| {
        let mut ships = Ships::of(&[(LightFighter, 1)]);
        ships.add_holding(LightFighter, 220, 440);
        s.fleets.get_mut(&stay).unwrap().ships = ships;
    });
    let mark = e.mark();
    e.order(Command::Merge { fleet_id: fid, other_fleet_id: stay }).expect("merge in the field");
    assert_eq!(levels(&e, fid), (0, 0, 0, true), "the merged fleet takes the lower of each level");
    assert_eq!(e.engine.fleets[&fid].ships, squadron(LightFighter, 2, &[200]), "and a damaged ship keeps the half of its hull it had");
    let told = alerts_since(&e, mark);
    assert!(
        told.iter().any(|a| a.message.contains(
            "fighting at the lower of the two fleets' levels (Weapons 0, Shielding 0, Armour 0) until the home dock refits them"
        )),
        "{told:?}",
    );

    let mark = e.mark();
    e.travel_fleet(fid, home);
    e.run(1);
    let told: Vec<String> = alerts_since(&e, mark).into_iter().map(|a| a.message).collect();
    assert_eq!(
        told,
        [
            format!("fleet {fid} was refitted at the home dock: Weapons 2, Shielding 0, Armour 1 (it flew with 0, 0, 0)"),
            format!("fleet {fid} was mended at the home dock: 1 ship(s) made whole, 220 hull restored, for nothing"),
        ],
    );
    assert_eq!(levels(&e, fid), (2, 0, 1, false));
    assert_eq!(figures(&e, fid), (60, 10, 440, 180, 1320));
    assert_eq!(state_of(&e, fid).hull, 1320);
    assert_eq!(levels(&e, half), (0, 0, 0, true), "the half still out waits for its own docking");
}

#[test]
fn damage_lasts_from_fight_to_fight_and_only_a_dock_mends_it() {
    use ShipClass::LightFighter;
    let (mut e, chain) = outpost("Veteran", 2);
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    let at = chain[1];
    muster(&mut e.engine, fid, &[(LightFighter, 12)], at);
    let fight = |e: &mut Empire, id: u64| {
        horde(&mut e.engine, id, at, &[(LightFighter, 6)], 0.8);
        e.order(Command::Attack { fleet_id: fid, target_fleet_id: id }).expect("attack");
        e.run_until(200, |e| e.engine.active_combats.is_empty());
        e.fleet()
    };

    let first = fight(&mut e, 900_001);
    assert!(first.ship_count >= 6, "twelve fighters beat six weaker ones: {} left", first.ship_count);
    let group = first.ships[0];
    assert!(group.damaged > 0 && first.hull < first.hull_max, "and come out of it damaged: {group:?}");
    let quote = first.repair.clone().expect("a damaged fleet is told what mending it takes");
    assert_eq!((quote.ships, quote.hull), (group.damaged, first.hull_max - first.hull));
    assert_eq!((quote.home.price, quote.home.ships, quote.home.hull, quote.outpost), (0.0, quote.ships, quote.hull, None));

    e.run(300);
    let later = e.fleet();
    assert_eq!((later.hull, later.ships[0].damaged), (first.hull, group.damaged), "nothing mends a hull in the field, and no shield is kept");

    let second = fight(&mut e, 900_002);
    assert!(second.hull < first.hull, "the second fight starts where the first left the fleet: {} then {}", first.hull, second.hull);

    let mark = e.mark();
    e.travel(home);
    e.run(1);
    let home_again = e.fleet();
    assert_eq!((home_again.hull, home_again.repair), (home_again.hull_max, None), "the home dock makes every ship whole");
    assert_eq!(home_again.ship_count, second.ship_count, "but brings none back");
    let told: Vec<String> = alerts_since(&e, mark).into_iter().map(|a| a.message).filter(|m| m.contains("mended")).collect();
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(told[0].ends_with(&format!("{} hull restored, for nothing", second.hull_max - second.hull)), "{}", told[0]);
}
