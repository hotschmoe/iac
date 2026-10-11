// The hub and its exchange: what a hauler gets there, that no run of trades
// leaves a player with more than they brought, that an order in pieces
// returns what the whole order would, how the meter recovers and survives a
// restart, what is refused and why, and that the hub stays empty space.

#[allow(dead_code)]
mod common;

use std::sync::{Arc, Mutex};

use common::*;
use iac_shared::constants::{
    Density, ResourceKind, Resources, ShipClass, TerrainType, EXCHANGE_FEE_MAX, EXCHANGE_FEE_MIN, EXCHANGE_METER_SHARE,
};
use iac_shared::hex::{hex_ring, Hex};
use iac_shared::protocol::{
    Command, ErrorCode, EventKind, ExchangeMeter, ExchangeQuote, ExchangeTerms, ServerMessage,
};
use iac_shared::scaling;
use iac_shared::world::{HubService, NpcBehaviorType, WorldGen};
use iac_sim::engine::{FleetStatus, NpcFleet, Ship, MAX_NPC_SHIPS};
use iac_sim::{GameEngine, Persist, PersistBatch, Snapshot};

use ResourceKind::{Crystal, Deuterium, Metal};

const HUB: Hex = Hex::ORIGIN;

fn set_mines(e: &mut Empire, metal: u8, crystal: u8, deuterium: u8) {
    let pid = e.pid;
    edit(&mut e.engine, |s| {
        let b = &mut s.players.get_mut(&pid).unwrap().buildings;
        (b.metal_mine, b.crystal_mine, b.deuterium_synthesizer) = (metal, crystal, deuterium);
    });
}

fn load(e: &mut Empire, cargo: Resources) {
    let fid = e.fid;
    edit(&mut e.engine, |s| {
        let fleet = s.fleets.get_mut(&fid).unwrap();
        fleet.cargo = cargo;
        fleet.state = FleetStatus::Idle;
    });
}

/// An empire with mines at the given levels and `haulers` Haulers carrying `cargo` at the hub.
fn trader(pace: &str, mines: (u8, u8, u8), haulers: usize, cargo: Resources) -> Empire {
    let mut e = Empire::join(world(pace), "Trader");
    set_mines(&mut e, mines.0, mines.1, mines.2);
    let fid = e.fid;
    muster(&mut e.engine, fid, &[(ShipClass::Hauler, haulers)], HUB);
    load(&mut e, cargo);
    e
}

fn trade(e: &mut Empire, give: ResourceKind, want: ResourceKind, amount: Option<f32>) -> Result<ExchangeTerms, (ErrorCode, String)> {
    let mark = e.mark();
    let outcome = e.engine.execute(e.pid, Command::Exchange { fleet_id: e.fid, give, want, amount });
    e.absorb();
    let made = e.since(mark, |k| match k {
        EventKind::Exchanged(x) => Some(x.terms.clone()),
        _ => None,
    });
    match outcome.error {
        Some(refusal) => {
            assert!(made.is_empty(), "a refused trade reports nothing");
            Err(refusal)
        }
        None => Ok(made.into_iter().next().expect("a trade is answered with an Exchanged event")),
    }
}

fn quote(e: &mut Empire, give: ResourceKind, want: ResourceKind, amount: Option<f32>) -> ExchangeQuote {
    let outcome = e.engine.execute(e.pid, Command::ExchangeQuote { fleet_id: e.fid, give, want, amount });
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    let Some(ServerMessage::ExchangeQuote(q)) = outcome.reply else { panic!("no quote") };
    q
}

fn meter(e: &Empire) -> ExchangeMeter {
    e.state().player.exchange
}

fn weight(r: Resources) -> f64 {
    ResourceKind::ALL.into_iter().map(|k| f64::from(r.get(k)) * f64::from(k.weight())).sum()
}

/// Weight of everything the player owns: the stockpile and every hold.
fn owned(e: &Empire) -> f64 {
    let holds: f64 = e.engine.fleets.values().filter(|f| f.owner_id == e.pid).map(|f| weight(f.cargo)).sum();
    weight(e.engine.players[&e.pid].resources) + holds
}

fn close(a: f32, b: f64, tolerance: f64) -> bool {
    (f64::from(a) - b).abs() <= tolerance
}

#[test]
fn a_hauler_flies_its_metal_to_the_hub_and_gets_the_quoted_deuterium() {
    let mut e = Empire::join(world("season"), "Hauler");
    set_mines(&mut e, 10, 9, 8);
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    let first = lane_path(&e.engine, home, HUB, 12).expect("a lane to the hub")[0];
    muster(&mut e.engine, fid, &[(ShipClass::Hauler, 1)], first);
    deploy(&mut e.engine, fid, first, Some(5000.0));
    let hold = e.fleet().cargo_capacity;
    assert!((hold - 632.4555).abs() < 0.01, "one Hauler holds 200 x sqrt(10) at season pace: {hold}");
    load(&mut e, Resources { metal: hold, ..Resources::default() });

    let afar = quote(&mut e, Metal, Deuterium, None);
    assert!(afar.refusal.as_deref().is_some_and(|why| why.contains("from the exchange at the hub")), "{:?}", afar.refusal);
    assert_eq!(trade(&mut e, Metal, Deuterium, None).unwrap_err().0, ErrorCode::NotInSector);

    e.travel(HUB);
    e.run_until(200, |e| e.engine.fleets[&fid].state == FleetStatus::Idle);
    let world = e.state().world;
    assert!(world.hub.iter().any(|site| site.location == HUB && site.services == [HubService::Exchange]));

    let pace = e.engine.pace();
    let production = e.engine.players[&pid].production_per_tick(&pace);
    let window = scaling::exchange_window(&pace);
    let span = EXCHANGE_METER_SHARE * weight(production) * window as f64;
    assert!((15_364..=15_365).contains(&window), "the window is 86400 s / 10^0.75: {window}");
    assert!((span - 4185.0).abs() < 5.0, "the span is 15 percent of one window of mines 10/9/8: {span}");

    let empty = meter(&e);
    assert_eq!((empty.pressure, empty.fee, empty.clear_in, empty.window_ticks), (0.0, 0.25, Some(0), window));
    assert!(close(empty.span, span, 0.01));

    let asked = quote(&mut e, Metal, Deuterium, None);
    assert_eq!(asked.refusal, None);
    assert_eq!((asked.terms.amount, asked.terms.receive), (hold, 225.0), "632 metal gives 225 deuterium");
    assert!(close(asked.terms.par, f64::from(hold) / 2.0, 0.01));
    assert!((0.28..0.30).contains(&asked.terms.fee), "about 29 percent: {}", asked.terms.fee);
    assert_eq!(e.fleet().cargo.metal, hold, "a quote changes nothing");
    assert_eq!(meter(&e), empty);

    let score = e.score_row();
    let before = owned(&e);
    let made = trade(&mut e, Metal, Deuterium, None).expect("the trade goes through");
    assert_eq!(made, asked.terms, "the trade is the quote");
    let cargo = e.fleet().cargo;
    assert_eq!((cargo.metal, cargo.crystal, cargo.deuterium), (0.0, 0.0, 225.0));
    assert!(owned(&e) < before - 0.25 * f64::from(hold));
    assert_eq!(e.score_row(), score, "a trade moves no score");

    let after = meter(&e);
    assert_eq!(after, made.meter_after);
    assert!(close(after.pressure, f64::from(hold), 0.01), "the meter rose by the weight sold");
    assert!(close(after.fee, 0.25 + 0.5 * f64::from(hold) / span, 1e-4));
    let clear = (f64::from(hold) / span * window as f64).ceil() as u64;
    assert_eq!(after.clear_in, Some(clear));
}

#[test]
fn no_run_of_trades_leaves_a_player_with_more_than_they_brought() {
    let mut e = trader("season", (6, 5, 4), 8, Resources { metal: 1800.0, crystal: 1200.0, deuterium: 900.0 });
    let fid = e.fid;
    let score = e.score_row();
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut draw = |n: u32| {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        ((seed >> 33) as u32) % n
    };
    let (mut made, mut refused) = (0, 0);

    for step in 0..400 {
        let cargo = e.engine.fleets[&fid].cargo;
        let stocked: Vec<ResourceKind> = ResourceKind::ALL.into_iter().filter(|k| cargo.get(*k) >= 8.0).collect();
        if stocked.is_empty() {
            break;
        }
        let give = stocked[draw(stocked.len() as u32) as usize];
        let want = ResourceKind::ALL[(give as usize + 1 + draw(2) as usize) % 3];
        let amount = match draw(16) {
            0 => None,
            share => Some(cargo.get(give) * share as f32 / 64.0),
        };
        let (owned_before, hold_before, meter_before) = (owned(&e), weight(cargo), meter(&e));
        match trade(&mut e, give, want, amount) {
            Ok(terms) => {
                made += 1;
                let gave = f64::from(terms.amount) * f64::from(give.weight());
                let got = f64::from(terms.receive) * f64::from(want.weight());
                assert!(got <= (1.0 - EXCHANGE_FEE_MIN) * gave + 1e-6, "step {step}: {got} back for {gave}");
                assert!(got >= (1.0 - EXCHANGE_FEE_MAX) * gave - f64::from(want.weight()), "step {step}: {got} back for {gave}");
                assert!(terms.fee >= EXCHANGE_FEE_MIN as f32 - 1e-6);
                let lost = gave - got;
                assert!((owned_before - owned(&e) - lost).abs() < 0.01, "step {step}: only the hold changed");
                assert!((hold_before - weight(e.engine.fleets[&fid].cargo) - lost).abs() < 0.01);
                assert!(close(meter(&e).pressure, f64::from(meter_before.pressure) + gave, 0.05), "step {step}");
            }
            Err((code, why)) => {
                refused += 1;
                assert!(
                    matches!(code, ErrorCode::NoResources | ErrorCode::InvalidCommand | ErrorCode::CargoFull),
                    "step {step}: {code:?} {why}"
                );
                assert_eq!(e.engine.fleets[&fid].cargo, cargo, "step {step}: a refused trade takes nothing");
                assert_eq!(meter(&e), meter_before);
            }
        }
        if step % 7 == 0 {
            let waited = u64::from(draw(4000));
            let hold = e.engine.fleets[&fid].cargo;
            e.run(waited);
            assert_eq!(e.engine.fleets[&fid].cargo, hold, "a hold away from home keeps what it carries");
        }
    }
    assert!(made >= 60 && refused >= 1, "{made} trades made, {refused} refused");
    assert_eq!(e.score_row(), score, "trades move no score");

    let mut round = trader("season", (6, 5, 4), 8, Resources { metal: 3000.0, ..Resources::default() });
    for (give, want) in [(Metal, Deuterium), (Deuterium, Crystal), (Crystal, Metal)] {
        trade(&mut round, give, want, None).expect("one leg of the loop");
    }
    let back = round.engine.fleets[&round.fid].cargo;
    assert_eq!((back.crystal, back.deuterium), (0.0, 0.0));
    assert!(back.metal <= 3000.0 * 0.75 * 0.75 * 0.75, "three legs keep at most 42 percent: {}", back.metal);
}

#[test]
fn an_order_in_pieces_returns_what_the_whole_order_would() {
    let cases = [
        ("from an empty meter", (10, 9, 8), 0.0, vec![100.0; 6]),
        ("across the span", (2, 1, 1), 0.0, vec![37.5, 400.0, 262.5, 800.0]),
        ("from a part-filled meter", (6, 5, 4), 500.0, vec![5.0, 7.0, 9.0, 579.0, 150.0, 250.0]),
        ("above the span", (1, 1, 1), 4000.0, vec![333.0, 333.0, 334.0]),
    ];
    for (name, mines, sold_before, pieces) in cases {
        let total: f32 = pieces.iter().sum();
        let mut whole = trader("season", mines, 12, Resources { metal: 6000.0, ..Resources::default() });
        if sold_before > 0.0 {
            trade(&mut whole, Metal, Crystal, Some(sold_before)).expect("the earlier sale");
        }
        let mut split = Empire { engine: GameEngine::restore(whole.engine.snapshot()), pid: whole.pid, fid: whole.fid, log: Vec::new() };

        let asked = quote(&mut whole, Metal, Deuterium, Some(total));
        let one = trade(&mut whole, Metal, Deuterium, Some(total)).expect("the whole order");
        assert_eq!(one, asked.terms, "{name}");

        let mut received = 0.0;
        for &piece in &pieces {
            received += trade(&mut split, Metal, Deuterium, Some(piece)).expect("one piece").receive;
        }
        assert!(received <= one.receive, "{name}: pieces gave {received}, the whole order {}", one.receive);
        assert!(one.receive - received < pieces.len() as f32, "{name}: each piece loses less than one unit to rounding");
        let (a, b) = (whole.engine.players[&whole.pid].exchange_pressure, split.engine.players[&split.pid].exchange_pressure);
        assert!((a - b).abs() < 1e-6, "{name}: the meter reads {a} and {b}");
        assert_eq!(whole.engine.fleets[&whole.fid].cargo.metal, split.engine.fleets[&split.fid].cargo.metal, "{name}");
    }
}

struct Capture(Arc<Mutex<Vec<PersistBatch>>>);

impl Persist for Capture {
    fn submit(&self, batch: PersistBatch) {
        self.0.lock().unwrap().push(batch);
    }

    fn flush(&self) -> Result<(), String> {
        Ok(())
    }
}

#[test]
fn the_meter_falls_one_span_per_window_and_survives_a_restart() {
    let mut e = trader("season", (4, 4, 4), 12, Resources { metal: 6000.0, ..Resources::default() });
    let pid = e.pid;
    let batches = Arc::new(Mutex::new(Vec::new()));
    e.engine.attach_persist(Box::new(Capture(batches.clone())));
    e.engine.persist_dirty_state();
    batches.lock().unwrap().clear();

    let sold = trade(&mut e, Metal, Deuterium, Some(3000.0)).expect("a sale past the span");
    let (span, window) = (f64::from(sold.meter.span), sold.meter.window_ticks);
    assert!(3000.0 > 2.0 * span, "the sale fills the meter more than twice over: span {span}");
    assert_eq!(sold.meter_after.fee, 0.75, "the fee is flat beyond the span");
    let sold_at = e.engine.current_tick();

    e.engine.persist_dirty_state();
    let stored: Vec<(f64, u64)> = batches
        .lock()
        .unwrap()
        .iter()
        .flat_map(|batch| &batch.players)
        .filter(|p| p.id == pid)
        .map(|p| (p.exchange_pressure, p.exchange_tick))
        .collect();
    assert_eq!(stored, [(3000.0, sold_at)], "the trade writes the meter with the player's row");

    e.run(window);
    assert!(close(meter(&e).pressure, 3000.0 - span, 0.01), "one window forgets one span: {:?}", meter(&e));
    assert_eq!(meter(&e).fee, 0.75);

    let saved: Snapshot = serde_json::from_slice(&serde_json::to_vec(&e.engine.snapshot()).unwrap()).unwrap();
    assert_eq!(saved.hash(), e.engine.snapshot().hash());
    let mut restored = Empire { engine: GameEngine::restore(saved), pid, fid: e.fid, log: Vec::new() };
    assert_eq!(meter(&restored), meter(&e), "the meter comes back as it was");

    let left = meter(&e).clear_in.expect("a meter with mines behind it empties");
    assert!(left.abs_diff(((3000.0 - span) / span * window as f64).ceil() as u64) <= 1, "{left} ticks to empty");
    e.run(left - 1);
    restored.run(left - 1);
    assert_eq!(meter(&restored), meter(&e));
    assert!(meter(&e).pressure > 0.0 && meter(&e).fee > 0.25 && meter(&e).clear_in == Some(1));
    e.run(1);
    restored.run(1);
    assert_eq!((meter(&e).pressure, meter(&e).fee, meter(&e).clear_in), (0.0, 0.25, Some(0)));
    assert_eq!(restored.engine.snapshot().hash(), e.engine.snapshot().hash());

    let again = quote(&mut e, Metal, Deuterium, Some(100.0)).terms;
    let fresh = quote(&mut restored, Metal, Deuterium, Some(100.0)).terms;
    assert_eq!(again, fresh);
    assert_eq!(again.receive, (scaling::exchange_return(0.0, 100.0, span) / 2.0).floor() as f32);

    let mut idle = trader("season", (0, 0, 0), 2, Resources { metal: 1000.0, ..Resources::default() });
    let flat = trade(&mut idle, Metal, Crystal, Some(600.0)).expect("an empire with no mines still trades");
    assert_eq!((flat.receive, flat.meter.span, flat.meter.fee), (100.0, 0.0, 0.75), "at the top fee");
    assert_eq!(flat.meter_after.clear_in, None);
    idle.run(5000);
    assert_eq!(meter(&idle).pressure, 600.0, "with no mine output the meter does not fall");
}

#[test]
fn a_trade_that_cannot_be_made_is_refused_with_the_reason() {
    let mut e = trader("season", (6, 5, 4), 1, Resources { metal: 400.0, deuterium: 232.0, ..Resources::default() });
    let (pid, fid) = (e.pid, e.fid);
    let cargo = e.engine.fleets[&fid].cargo;
    let untouched = |e: &Empire| e.engine.fleets[&fid].cargo == cargo && meter(e).pressure == 0.0;

    let (code, why) = trade(&mut e, Crystal, Metal, None).unwrap_err();
    assert_eq!(code, ErrorCode::NoResources);
    assert!(why.contains("holds no crystal to trade") && why.contains("400 metal"), "{why}");

    let (code, why) = trade(&mut e, Metal, Crystal, Some(400.5)).unwrap_err();
    assert_eq!(code, ErrorCode::NoResources);
    assert!(why.contains("holds 400.0 metal; it cannot give 400.5"), "{why}");

    let (code, why) = trade(&mut e, Metal, Metal, Some(10.0)).unwrap_err();
    assert_eq!(code, ErrorCode::InvalidCommand);
    assert!(why.contains("give and want are both metal"), "{why}");

    for bad in [0.0, -5.0, f32::NAN, f32::INFINITY] {
        let (code, why) = trade(&mut e, Metal, Crystal, Some(bad)).unwrap_err();
        assert_eq!(code, ErrorCode::InvalidCommand);
        assert!(why.contains("must be a positive number"), "{why}");
    }

    let (code, why) = trade(&mut e, Metal, Deuterium, Some(2.0)).unwrap_err();
    assert_eq!(code, ErrorCode::InvalidCommand);
    assert!(why.contains("too little to trade") && why.contains("less than one deuterium"), "{why}");

    let (code, why) = trade(&mut e, Deuterium, Metal, None).unwrap_err();
    assert_eq!(code, ErrorCode::CargoFull);
    assert!(why.contains("no room for it") && why.contains("carries 632 of 632"), "{why}");
    let full = quote(&mut e, Deuterium, Metal, None);
    assert_eq!(full.refusal.as_deref(), Some(why.as_str()), "the quote says what the trade would be told");
    assert!(full.terms.receive > full.terms.amount, "deuterium buys more units of metal than it gives");

    let outcome = e.engine.execute(pid + 1000, Command::Exchange { fleet_id: fid, give: Metal, want: Crystal, amount: None });
    assert_eq!(outcome.error.map(|(code, _)| code), Some(ErrorCode::FleetNotFound), "another player's fleet");
    let outcome = e.engine.execute(pid, Command::ExchangeQuote { fleet_id: fid + 1000, give: Metal, want: Crystal, amount: None });
    assert!(outcome.reply.is_none());
    assert!(outcome.error.is_some_and(|(code, why)| code == ErrorCode::FleetNotFound && why.contains("not found")));
    assert!(untouched(&e));

    let out = hex_ring(HUB, 1)
        .find(|at| e.engine.world_gen.generate_sector(*at).npc_template.is_none())
        .expect("a quiet sector beside the hub");
    e.order(Command::Move { fleet_id: fid, target: out }).expect("leave the hub");
    let (code, why) = trade(&mut e, Metal, Crystal, None).unwrap_err();
    assert_eq!(code, ErrorCode::OnCooldown);
    assert!(why.contains("in transit"), "{why}");
    e.run_until(200, |e| e.engine.fleets[&fid].state == FleetStatus::Idle && e.engine.fleets[&fid].location == out);

    let (code, why) = trade(&mut e, Metal, Crystal, None).unwrap_err();
    assert_eq!(code, ErrorCode::NotInSector);
    assert!(why.contains(&format!("is at {out}, 1 hex from the exchange at the hub {HUB}")), "{why}");
    let afar = quote(&mut e, Metal, Crystal, Some(900.0));
    assert_eq!(afar.refusal.as_deref(), Some(why.as_str()));
    assert!(afar.terms.receive > 0.0 && afar.terms.amount == 900.0, "a quote from afar still names the rate");
    assert!(untouched(&e));
}

#[test]
fn the_hub_is_empty_space_and_no_hostile_group_enters_it() {
    for seed in [1, 2, 3, 42, 1337, 0xDEAD_BEEF_CAFE_BABE] {
        let hub = WorldGen::init(seed).generate_sector(HUB);
        assert!(hub.npc_template.is_none() && !hub.has_derelict, "seed {seed}");
        assert_eq!(hub.terrain, TerrainType::Empty);
        assert_eq!([hub.metal_density, hub.crystal_density, hub.deut_density], [Density::None; 3]);
        assert_eq!(iac_shared::world::hub_services(HUB), [HubService::Exchange]);
        assert_eq!(WorldGen::init(seed).connected_neighbors(HUB).slice().len(), 6, "every lane to the hub is open");
    }

    let mut e = trader("season", (1, 1, 1), 1, Resources { metal: 100.0, ..Resources::default() });
    let (pid, fid) = (e.pid, e.fid);
    e.run(2);
    let here = &chart(&e.engine, pid)[&HUB.to_key()];
    assert!(here.live && here.hostiles.is_none() && here.site.is_none() && here.ore_reserve.is_none() && here.salvage.is_none());
    assert_eq!(e.order(Command::Harvest { fleet_id: fid, resource: iac_shared::protocol::HarvestResource::Auto }), Err(ErrorCode::NoResources));
    assert_eq!(e.order(Command::ExploreSite { fleet_id: fid }), Err(ErrorCode::InvalidTarget));
    assert_eq!(
        e.order(Command::Attack { fleet_id: fid, target_fleet_id: iac_sim::engine::template_npc_id(HUB) }),
        Err(ErrorCode::InvalidTarget),
        "nothing to fight at the hub",
    );

    let scout = ShipClass::Scout.base_stats();
    let posts: Vec<Hex> = hex_ring(HUB, 1).collect();
    let patrol = |id: u64, at: Hex| {
        let mut ships = [Ship::default(); MAX_NPC_SHIPS];
        ships[0] = Ship {
            id,
            ship_class: ShipClass::Scout,
            hull: scout.hull,
            hull_max: scout.hull,
            shield: scout.shield,
            shield_max: scout.shield,
            weapon_power: scout.weapon,
            speed: scout.speed,
        };
        NpcFleet {
            id,
            location: at,
            ships,
            ship_count: 1,
            behavior: NpcBehaviorType::Patrol,
            home_sector: at,
            patrol_timer: 0,
            in_combat: false,
            bounty: Resources::default(),
            power: scaling::ship_power(ShipClass::Scout),
            label: "scout patrol".to_string(),
        }
    };
    let mut moved = 0;
    for _ in 0..40 {
        edit(&mut e.engine, |s| {
            for (i, &at) in posts.iter().enumerate() {
                let id = 900_000 + i as u64;
                s.npc_fleets.insert(id, patrol(id, at));
            }
        });
        e.run(1);
        for (i, &at) in posts.iter().enumerate() {
            let now = e.engine.npc_fleets[&(900_000 + i as u64)].location;
            assert_ne!(now, HUB, "a patrol beside the hub never jumps into it");
            moved += usize::from(now != at);
        }
        assert!(e.engine.active_combats.is_empty(), "no fight at the hub");
    }
    assert!(moved >= 200, "the patrols did move: {moved} jumps");
    assert_eq!(e.engine.fleets[&fid].location, HUB);
    trade(&mut e, Metal, Crystal, None).expect("the exchange is open with patrols all around");
}
