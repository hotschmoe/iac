// Fleet handling over real distances: splitting and merging, fuel and the
// way home, the emergency reserve, and what standing orders refuse.

#[allow(dead_code)]
mod common;

use common::*;
use iac_shared::hex::Hex;
use iac_shared::constants::{Resources, ShipClass, FUEL_DEUT_PER_UNIT, STRANDED_RECOVERY_TICKS};
use iac_shared::protocol::{
    AlertEvent, AlertLevel, Command, ErrorCode, EventKind, PolicyParams, PolicyPreset, ServerMessage, ThreatBasis,
    MAX_ODDS_SECTORS,
};
use iac_shared::scaling::{ratio_label, RatioLabel};
use iac_shared::world::NpcBehaviorType;
use iac_sim::engine::{fleet_force, template_npc_id, Fleet, FleetStatus};
use iac_sim::odds::{self, Force};
use iac_sim::persist::ExploreCredit;

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

fn tuned_prospector(engage_ratio_x10: u8) -> (Empire, u64) {
    let mut e = Empire::join(world("test"), "Dialled");
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    let frontier: Vec<Hex> = (-4i16..=4)
        .flat_map(|q| (-4i16..=4).map(move |r| Hex { q: home.q + q, r: home.r + r }))
        .filter(|h| (3..=4).contains(&Hex::distance(h, &home)))
        .collect();
    let waiting: Vec<Force> = frontier.iter().map(|&h| e.engine.known_opposition(pid, h).1).collect();
    let best_odds = |fleet: &Fleet| {
        let ours = fleet_force(fleet);
        waiting.iter().map(|theirs| odds::outlook(&ours, theirs).ratio()).fold(0.0f32, f32::max)
    };
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
        let fleet = s.fleets.get_mut(&fid).unwrap();
        let (mut lo, mut hi) = (0.0f32, 1e6f32);
        for _ in 0..60 {
            let mid = (lo + hi) / 2.0;
            for ship in &mut fleet.ships[..fleet.ship_count] {
                ship.weapon_power = mid;
            }
            if best_odds(fleet) < 1.6 { lo = mid } else { hi = mid }
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
fn docked_with_half_a_tank(budget: u8, deuterium: impl Fn(f32) -> f32) -> (Empire, f32, f32) {
    let mut e = Empire::join(world("season"), "Thrifty");
    let (pid, fid) = (e.pid, e.fid);
    let pace = e.engine.world.pace;
    let tank = e.engine.fleets[&fid].fuel_max;
    edit(&mut e.engine, |s| {
        let player = s.players.get_mut(&pid).unwrap();
        player.buildings.deuterium_synthesizer = 2;
        let cap = iac_shared::scaling::storage_cap(player.buildings.storage_vault, &pace).deuterium;
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
    assert!(deuterium(&e) >= start + (wait - 11) as f32 * income, "the stockpile kept all it produced");
    let waiting = policy_reasons(&e, mark, "hold");
    assert!(waiting[0].starts_with("refuelling on a fuel budget of 20% of deuterium income"), "{waiting:?}");
    assert!(policy_reasons(&e, mark, "move").is_empty());

    let (before, tick) = (deuterium(&e), e.engine.current_tick());
    e.run_until(20, |e| fuel_of(e, fid) > half);
    assert_eq!(fuel_of(&e, fid), e.engine.fleets[&fid].fuel_max, "the whole fill at once");
    let earned = (e.engine.current_tick() - tick) as f32 * income;
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

    assert_eq!(e.engine.fleets[&fid].ship_count, 2, "it came home whole");
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
    let at = lone_scout_sector(&e.engine, NpcBehaviorType::Passive);
    deploy(&mut e.engine, fid, at, None);
    edit(&mut e.engine, |s| {
        let fleet = s.fleets.get_mut(&fid).unwrap();
        for ship in &mut fleet.ships[..fleet.ship_count] {
            ship.weapon_power = 0.0;
            ship.shield = 0.0;
        }
        fleet.ships[0].hull = 0.01;
        fleet.ships[1].hull = 1e6;
    });
    let whole = e.fleet();
    assert_eq!(whole.ships.len(), 2);
    assert_eq!(whole.fuel, whole.fuel_max, "it set out with a full tank");
    let hops_home = whole.home_fuel / whole.jump_fuel;

    e.order(Command::Attack { fleet_id: fid, target_fleet_id: template_npc_id(at) }).expect("attack");
    e.run_until(200, |e| e.engine.fleets[&fid].ship_count == 1);
    edit(&mut e.engine, |s| s.fleets.get_mut(&fid).unwrap().ships[0].weapon_power = 5000.0);
    e.run_until(20, |e| e.engine.active_combats.is_empty());

    let mauled = e.fleet();
    assert_eq!(mauled.ships.len(), 1);
    assert!(
        (mauled.fuel_max - whole.fuel_max / 2.0).abs() < 0.01,
        "one scout of two is left but the tank reads {} of the {} two carried",
        mauled.fuel_max,
        whole.fuel_max,
    );
    assert!(mauled.fuel <= mauled.fuel_max, "{} fuel in a {} tank", mauled.fuel, mauled.fuel_max);
    assert!((mauled.jump_fuel - whole.jump_fuel / 2.0).abs() < 1e-3, "a jump costs what the ships present weigh");
    assert!((mauled.home_fuel / mauled.jump_fuel - hops_home).abs() < 1e-3, "the way home is as many jumps as before");
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

fn wreckage(e: &Empire, at: Hex) -> Resources {
    e.engine.sector_overrides.get(&at.to_key()).and_then(|o| o.salvage).unwrap_or_default()
}

fn same(a: Resources, b: Resources) -> bool {
    (a.metal - b.metal).abs() < 0.05 && (a.crystal - b.crystal).abs() < 0.05 && (a.deuterium - b.deuterium).abs() < 0.05
}

/// The starting fleet at a lone passive scout, unarmed, its hold nine tenths
/// full, and each ship with the hull given.
fn laden_and_outgunned(e: &mut Empire, hulls: [f32; 2]) -> (Hex, f32, Resources) {
    let fid = e.fid;
    let at = lone_scout_sector(&e.engine, NpcBehaviorType::Passive);
    deploy(&mut e.engine, fid, at, None);
    let hold = e.fleet().cargo_capacity;
    let cargo = Resources { metal: hold * 0.5, crystal: hold * 0.25, deuterium: hold * 0.15 };
    edit(&mut e.engine, |s| {
        let fleet = s.fleets.get_mut(&fid).unwrap();
        for (ship, hull) in fleet.ships.iter_mut().zip(hulls) {
            ship.weapon_power = 0.0;
            ship.shield = 0.0;
            ship.hull = hull;
        }
        fleet.cargo = cargo;
    });
    (at, hold, cargo)
}

#[test]
fn a_fleet_that_loses_a_ship_in_a_fight_leaves_what_it_can_no_longer_carry_as_wreckage() {
    let mut e = Empire::join(world("test"), "Spilled");
    let fid = e.fid;
    let (at, hold, cargo) = laden_and_outgunned(&mut e, [0.01, 1e6]);

    let mark = e.mark();
    e.order(Command::Attack { fleet_id: fid, target_fleet_id: template_npc_id(at) }).expect("attack");
    e.run_until(200, |e| e.engine.fleets[&fid].ship_count == 1);

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
    edit(&mut e.engine, |s| s.fleets.get_mut(&fid).unwrap().ships[0].weapon_power = 5000.0);
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
    let (at, _, cargo) = laden_and_outgunned(&mut e, [0.01, 0.01]);

    let mark = e.mark();
    e.order(Command::Attack { fleet_id: fid, target_fleet_id: template_npc_id(at) }).expect("attack");
    e.run_until(200, |e| e.engine.fleets.get(&fid).is_none_or(|f| f.ship_count == 0));
    e.run(2);
    assert!(!e.engine.fleets.contains_key(&fid));

    assert!(same(wreckage(&e, at), cargo), "the whole cargo lies in the sector: {:?}", wreckage(&e, at));
    let told: Vec<String> = alerts_since(&e, mark).into_iter().map(|a| a.message).collect();
    assert!(told.iter().any(|m| m.contains("FLEET LOST")), "{told:?}");
    assert!(told.iter().any(|m| m.contains("drifts as wreckage") && m.contains(&at.to_string())), "{told:?}");

    deploy(&mut e.engine, rival_fleet, at, None);
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
    muster(&mut e.engine, fid, &[(ShipClass::Scout, 20)], far);
    let hold = e.fleet().cargo_capacity;
    let cargo = Resources { metal: hold * 0.5, crystal: hold * 0.25, deuterium: hold * 0.25 };
    e.set_stockpile(Resources::default());
    edit(&mut e.engine, |s| {
        let fleet = s.fleets.get_mut(&fid).unwrap();
        for ship in &mut fleet.ships[1..20] {
            ship.hull = 0.001;
        }
        fleet.ships[0].hull = 1e6;
        fleet.ships[0].hull_max = 1e6;
        fleet.fuel = 1e12;
        fleet.fuel_max = 1e12;
        fleet.cargo = cargo;
    });

    let mark = e.mark();
    e.order(Command::Recall { fleet_id: fid }).expect("recall");
    let broke = e
        .since(mark, |k| match k {
            EventKind::ShipDestroyed(d) => Some(d.sector),
            _ => None,
        })
        .len();
    let back = e.fleet();
    assert!(broke > 0 && back.ships.len() == 20 - broke, "{broke} ships broke up, {} came home", back.ships.len());
    assert_eq!(back.location, home);

    let left_behind = wreckage(&e, far);
    assert!(same(left_behind, cargo.scale(broke as f32 / 20.0)), "{broke} holds of 20 stayed behind: {left_behind:?}");
    assert!(
        same(e.stockpile().add(back.cargo).add(left_behind), cargo),
        "what came home is the cargo less the wreckage: {:?} banked, {:?} aboard",
        e.stockpile(),
        back.cargo,
    );
    assert_eq!(wreckage(&e, home), Resources::default());
}
