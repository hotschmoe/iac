// One player's whole arc through the public command surface at test pace:
// the world as a client sees it (`views`), commands as a client sends them
// (`GameEngine::execute`), time passing only through `tick`.

#[allow(dead_code)]
mod common;

use common::*;
use iac_shared::constants::{Resources, ShipClass, RAID_ROUNDS};
use iac_shared::hex::Hex;
use iac_shared::protocol::{
    BatchWait, BattleOutcome, Command, ErrorCode, EventKind, QueueAction, QueueEvent, QueueType, RaidLabel, ServerMessage,
    StockpileCause, WaitReason,
};
use iac_shared::scaling::{self, BuildingType, ResearchType};
use iac_shared::world::NpcBehaviorType;
use iac_sim::engine::{template_npc_id, AuthError};

fn recruit(name: &str) -> Empire {
    let mut engine = world("test");
    let issued = "t".repeat(64);
    let auth = engine.authenticate(name, None, false, || issued.clone()).expect("registration");
    assert!(auth.registered && auth.new_token.as_deref() == Some(issued.as_str()));
    let again = engine.authenticate(&name.to_uppercase(), auth.new_token.as_deref(), false, || unreachable!());
    assert!(matches!(again, Ok(a) if a.player_id == auth.player_id && !a.registered), "names are case-insensitive accounts");
    assert!(matches!(engine.authenticate(name, None, false, || issued.clone()), Err(AuthError::TokenRequired)));
    engine.authenticate("Idler", None, false, || issued.clone()).expect("a second empire");
    let pid = auth.player_id;
    let fid = engine.fleets.values().find(|f| f.owner_id == pid).expect("starting fleet").id;
    Empire { engine, pid, fid, log: Vec::new() }
}

fn chart_the_neighbourhood(e: &mut Empire) {
    let home = e.engine.players[&e.pid].homeworld;
    let quiet = |h: Hex| e.engine.world_gen.generate_sector(h).npc_template.is_none();
    let rivals: Vec<Hex> = e.engine.players.values().map(|p| p.homeworld).collect();
    let out = e
        .engine
        .world_gen
        .connected_neighbors(home)
        .slice()
        .iter()
        .copied()
        .find(|&h| quiet(h) && !rivals.contains(&h))
        .expect("a quiet neighbour");

    let delivered = |e: &Empire, mark: usize| {
        e.since(mark, |k| match k {
            EventKind::ChartDelivered(d) => Some(d.sectors),
            _ => None,
        })
    };
    let paid_for_charts = |e: &Empire, mark: usize| {
        e.since(mark, |k| match k {
            EventKind::Stockpile(s) if s.cause == StockpileCause::Charts => Some(s.amount),
            _ => None,
        })
    };
    let before = e.score_row();
    let mark = e.mark();
    e.travel(out);
    assert!(delivered(e, mark).is_empty(), "nothing is paid in the field");
    let stock = e.engine.players[&e.pid].resources;
    e.travel(home);
    assert_eq!(delivered(e, mark), [1], "one sector charted");
    let rating = scaling::threat_rating(scaling::npc_power(out.dist_from_origin()));
    let paid = paid_for_charts(e, mark);
    assert_eq!(paid, [scaling::split_loot(scaling::chart_value(rating))], "the chart pays its value in resources");
    assert!(e.engine.players[&e.pid].resources.metal >= stock.metal + paid[0].metal, "into the stockpile");
    assert_eq!(e.score_row().score, before.score, "and scores nothing until it is built into something");

    let mark = e.mark();
    e.travel(out);
    e.travel(home);
    assert!(delivered(e, mark).is_empty() && paid_for_charts(e, mark).is_empty(), "the same sector never pays twice");
}

const HALF: f64 = scaling::CANCEL_REFUND_FRACTION;

fn item_started(events: &[QueueEvent], item: &str) -> Option<usize> {
    events.iter().position(|q| q.action == QueueAction::Started && q.item.starts_with(item))
}

fn build_the_foundations(e: &mut Empire) {
    e.set_stockpile(Resources { metal: 5000.0, crystal: 5000.0, deuterium: 5000.0 });
    let mark = e.mark();
    let line = [BuildingType::MetalMine, BuildingType::RoboticsFactory, BuildingType::RoboticsFactory, BuildingType::Shipyard];
    for building in line {
        e.order(Command::Build { building_type: building, reserve: false }).expect("the line has room");
    }
    assert_eq!(
        e.order(Command::Build { building_type: BuildingType::DeuteriumSynthesizer, reserve: false }),
        Err(ErrorCode::QueueFull)
    );

    let events = e.queue_events(mark);
    let waiting: Vec<_> = events.iter().filter(|q| q.action == QueueAction::Waiting).collect();
    assert_eq!(events[0].action, QueueAction::Started);
    assert_eq!(waiting.iter().map(|q| q.waiting_on).collect::<Vec<_>>(), [Some(WaitReason::Slot), Some(WaitReason::Prerequisite), Some(WaitReason::Prerequisite)]);
    let yard = waiting[2].id;

    assert_eq!(e.order(Command::CancelBuild { queue_type: QueueType::Building, id: yard }), Err(ErrorCode::InvalidTarget), "waiting is not running");
    assert_eq!(e.order(Command::CancelQueued { queue_type: QueueType::Ship, id: yard }), Err(ErrorCode::InvalidTarget), "wrong queue");
    let cancel_mark = e.mark();
    e.order(Command::CancelQueued { queue_type: QueueType::Building, id: yard }).expect("cancel the waiting yard by id");
    let cancelled = e.queue_events(cancel_mark);
    assert_eq!((cancelled[0].id, cancelled[0].action, cancelled[0].refunded), (yard, QueueAction::Cancelled, Resources::default()));
    assert_eq!(e.order(Command::CancelQueued { queue_type: QueueType::Building, id: yard }), Err(ErrorCode::InvalidTarget), "already gone");

    e.order(Command::Build { building_type: BuildingType::DeuteriumSynthesizer, reserve: false }).expect("room again");
    e.run_until(100, |e| {
        let home = e.state().homeworld;
        home.build_queue.is_empty() && home.build_pending.is_empty()
    });
    let done: Vec<_> = e.since(mark, |k| match k {
        EventKind::BuildingCompleted(b) => Some((b.building_type, b.new_level)),
        _ => None,
    });
    assert_eq!(
        done,
        [
            (BuildingType::MetalMine, 2),
            (BuildingType::RoboticsFactory, 1),
            (BuildingType::RoboticsFactory, 2),
            (BuildingType::DeuteriumSynthesizer, 1),
        ],
        "the order the line ran in"
    );
}

#[test]
fn an_empire_grows_fights_charts_and_survives_a_raid() {
    let mut e = recruit("Arc");
    let start = e.state();
    assert_eq!(start.fleets.len(), 1);
    assert_eq!((start.fleets[0].ship_count, start.fleets[0].ships.len()), (2, 1), "two ships of one class");
    chart_the_neighbourhood(&mut e);
    build_the_foundations(&mut e);
    mind_the_queue(&mut e);
    research_a_chain(&mut e);
    let (lair, power) = hostile_lair(&e);
    let need = ((3.0 * power - e.fleet().power) / scaling::ship_power(ShipClass::LightFighter)).ceil().max(3.0) as u16;
    build_a_battle_group(&mut e, need);
    hunt_pirates(&mut e, lair);
    withstand_a_raid(&mut e);
    hold_the_line(&mut e, 4000);
    settle_the_score(&mut e);
}

#[test]
fn a_short_order_without_reserve_is_passed_over_by_one_that_can_be_paid() {
    let mut e = recruit("Pass");
    let pid = e.pid;
    let cost = |e: &Empire, b: BuildingType| {
        e.state().homeworld.catalog.buildings.iter().find(|o| o.building_type == b).and_then(|o| o.next).expect("upgrade available").cost
    };
    let cheap = cost(&e, BuildingType::MetalMine);
    assert!(cost(&e, BuildingType::DeuteriumSynthesizer).metal > cheap.metal);
    edit(&mut e.engine, |s| {
        let player = s.players.get_mut(&pid).unwrap();
        player.resources = cheap;
        player.research.computer_technology = iac_shared::constants::BUILD_SLOT_LEVELS[0];
    });
    assert_eq!(e.state().homeworld.build_slots, 2);
    let mark = e.mark();
    e.order(Command::Build { building_type: BuildingType::DeuteriumSynthesizer, reserve: false }).expect("short head");
    e.order(Command::Build { building_type: BuildingType::MetalMine, reserve: false }).expect("payable order behind it");
    let pending = e.state().homeworld.build_pending;
    assert_eq!(pending.iter().map(|q| q.waiting_on).collect::<Vec<_>>(), [WaitReason::Resources]);
    e.run_until(200, |e| e.state().homeworld.build_pending.is_empty());
    let started = e.queue_events(mark);
    let metal = item_started(&started, "Metal Mine").expect("the payable order started");
    let deuterium = item_started(&started, "Deut").expect("the short head started once paid");
    assert!(metal < deuterium, "the payable order started past the short head");
}

#[test]
fn a_batch_larger_than_the_store_is_refused_though_refunds_would_pay_for_it() {
    let mut e = Empire::join(world("test"), "Hoard");
    let pid = e.pid;
    edit(&mut e.engine, |s| {
        let player = s.players.get_mut(&pid).unwrap();
        player.buildings.shipyard = 1;
        player.research.combustion_drive = 1;
    });
    let scouts = |count: u16| Command::BuildShip { ship_class: ShipClass::LightFighter, count };
    let cap = e.state().homeworld.storage.cap;
    let unit = ShipClass::LightFighter.build_cost();
    let fits = (cap.metal / unit.metal).floor() as u16;
    let whole = unit.scale(f64::from(fits));

    e.set_stockpile(cap);
    let mark = e.mark();
    e.order(scouts(fits)).expect("the largest batch the store holds");
    let running = e.queue_events(mark)[0].id;
    e.set_stockpile(cap);
    e.order(Command::CancelBuild { queue_type: QueueType::Ship, id: running }).expect("cancel it");
    let hoard = e.stockpile();
    assert_eq!(hoard, cap.add(whole.scale(HALF)), "the refund is paid past the cap");
    e.run(3);
    assert_eq!(e.stockpile(), hoard, "and stays there: the mines add nothing over the cap and take nothing away");

    let over = unit.scale(f64::from(fits + 1));
    assert!(over.metal > cap.metal && hoard.can_afford(over), "{over:?} against {cap:?} and {hoard:?}");
    let (code, message) = e.refusal(scouts(fits + 1));
    assert_eq!(code, ErrorCode::StorageTooSmall);
    assert_eq!(
        message,
        format!(
            "Light Fighter x{} costs {} metal, more than the {} metal your stockpile holds; needs Metal Storage level 1, or order {fits} or fewer at a time",
            fits + 1,
            over.metal.ceil(),
            cap.metal.floor()
        )
    );
    assert_eq!(e.stockpile(), hoard, "nothing was charged");
    assert!(yard_idle(&e));

    e.order(scouts(fits)).expect("the batch that fits is still taken from the same stockpile");
    assert_eq!(e.stockpile(), hoard.sub(whole));
}

fn mind_the_queue(e: &mut Empire) {
    let pid = e.pid;
    let next_cost = |e: &Empire, b: BuildingType| {
        e.state().homeworld.catalog.buildings.iter().find(|o| o.building_type == b).and_then(|o| o.next).expect("upgrade available").cost
    };
    let cheap = next_cost(e, BuildingType::MetalMine);
    assert!(next_cost(e, BuildingType::DeuteriumSynthesizer).metal > cheap.metal);
    edit(&mut e.engine, |s| {
        let player = s.players.get_mut(&pid).unwrap();
        player.resources = cheap;
        player.research.computer_technology = iac_shared::constants::BUILD_SLOT_LEVELS[0];
    });
    assert_eq!(e.state().homeworld.build_slots, 2);
    let mark = e.mark();
    e.order(Command::Build { building_type: BuildingType::DeuteriumSynthesizer, reserve: true }).expect("reserving order");
    e.order(Command::Build { building_type: BuildingType::MetalMine, reserve: false }).expect("cheap order behind it");
    let pending = e.state().homeworld.build_pending;
    assert_eq!(
        pending.iter().map(|q| q.waiting_on).collect::<Vec<_>>(),
        [WaitReason::Resources, WaitReason::Order],
        "the cheap order may not jump a reserving one"
    );
    e.run_until(200, |e| e.state().homeworld.build_pending.is_empty());
    let started = e.queue_events(mark);
    let deuterium = item_started(&started, "Deut").expect("reserving order started");
    let metal = item_started(&started, "Metal Mine").expect("cheap order started");
    assert!(deuterium < metal);

    e.run(30);
    let mark = e.mark();
    e.order(Command::Build { building_type: BuildingType::MetalStorage, reserve: false }).expect("storage");
    let running = e.queue_events(mark);
    let (id, paid) = (running[0].id, running[0].paid);
    e.order(Command::CancelBuild { queue_type: QueueType::Building, id }).expect("cancel the running storage by id");
    let cancelled = e.queue_events(mark);
    let refund = cancelled.iter().find(|q| q.action == QueueAction::Cancelled).expect("cancel confirmed");
    assert_eq!(refund.id, id);
    assert_eq!(refund.refunded, paid.scale(scaling::CANCEL_REFUND_FRACTION), "half of what it paid comes back");
}

fn research_a_chain(e: &mut Empire) {
    e.set_stockpile(Resources { metal: 20_000.0, crystal: 20_000.0, deuterium: 20_000.0 });
    let mark = e.mark();
    for building in [BuildingType::ResearchLab, BuildingType::ResearchLab, BuildingType::Shipyard] {
        e.order(Command::Build { building_type: building, reserve: false }).expect("a level queued");
    }
    let chain = [
        ResearchType::ArmourTechnology,
        ResearchType::ArmourTechnology,
        ResearchType::EnergyTechnology,
        ResearchType::CombustionDrive,
    ];
    for tech in chain {
        e.order(Command::Research { tech, reserve: false }).expect("research queues behind the lab that is itself queued");
    }
    let waiting = e.state().homeworld.research_pending;
    assert_eq!(
        (waiting[0].tech, waiting[0].target_level, waiting[0].waiting_on),
        (ResearchType::ArmourTechnology, 1, WaitReason::Prerequisite)
    );
    assert_eq!((waiting[1].tech, waiting[1].target_level), (ResearchType::ArmourTechnology, 2), "a second order builds on the first");
    assert_eq!(
        (waiting[3].tech, waiting[3].waiting_on),
        (ResearchType::CombustionDrive, WaitReason::Prerequisite),
        "an order may wait on a research that is itself queued"
    );

    e.run_until(300, |e| {
        let home = e.state().homeworld;
        home.research_active.is_none() && home.research_pending.is_empty() && home.build_queue.is_empty() && home.build_pending.is_empty()
    });
    let finished: Vec<String> = e
        .log[mark..]
        .iter()
        .filter_map(|ev| match &ev.kind {
            EventKind::BuildingCompleted(b) if b.building_type == BuildingType::ResearchLab => Some(format!("lab{}", b.new_level)),
            EventKind::ResearchCompleted(r) => Some(format!("{:?}{}", r.tech, r.new_level)),
            _ => None,
        })
        .collect();
    let at = |name: &str| finished.iter().position(|f| f == name).unwrap_or_else(|| panic!("{name} missing from {finished:?}"));
    assert!(at("lab2") < at("ArmourTechnology1") && at("ArmourTechnology1") < at("ArmourTechnology2"), "{finished:?}");
    assert!(at("EnergyTechnology1") < at("CombustionDrive1"), "{finished:?}");
}

fn hostile_lair(e: &Empire) -> (Hex, f32) {
    let home = e.engine.players[&e.pid].homeworld;
    let homes: Vec<Hex> = e.engine.players.values().map(|p| p.homeworld).collect();
    let mut best: Option<(usize, Hex, f32)> = None;
    for q in -9i16..=9 {
        for r in -9i16..=9 {
            let at = Hex { q: home.q + q, r: home.r + r };
            let Some(t) = e.engine.world_gen.generate_sector(at).npc_template else { continue };
            if t.behavior != NpcBehaviorType::Patrol || t.power() < 100.0 || homes.contains(&at) || e.engine.derelict_site_at(at).is_some() {
                continue;
            }
            let Some(path) = lane_path(&e.engine, home, at, 8) else { continue };
            if best.is_none_or(|(hops, ..)| path.len() < hops) {
                best = Some((path.len(), at, t.power()));
            }
        }
    }
    let (_, at, power) = best.expect("a pirate lair within eight hops");
    (at, power)
}

fn yard_idle(e: &Empire) -> bool {
    let home = e.state().homeworld;
    home.shipyard_queue.is_none() && home.shipyard_pending.is_empty()
}

fn fighters(count: u16) -> Command {
    Command::BuildShip { ship_class: ShipClass::LightFighter, count }
}

fn build_a_battle_group(e: &mut Empire, count: u16) {
    let unit = ShipClass::LightFighter.build_cost();
    let batch = unit.scale(f64::from(count));
    let pair = unit.scale(2.0);
    let mark = e.mark();

    e.set_stockpile(pair);
    let (code, message) = e.refusal(fighters(count));
    assert_eq!(code, ErrorCode::InsufficientResources);
    let short = batch.sub(pair);
    let wanted = format!(
        "Light Fighter x{count} costs {} metal and {} crystal; the stockpile is short by {} metal and {} crystal.",
        batch.metal, batch.crystal, short.metal, short.crystal
    );
    assert!(
        message.starts_with(&wanted)
            && message.ends_with("Nothing was ordered: a batch is paid in full when it is ordered. The stockpile covers 2."),
        "{message}"
    );
    assert_eq!(e.stockpile(), pair, "a batch the stockpile does not cover is not built in part");
    assert!(yard_idle(e) && e.queue_events(mark).is_empty());

    e.set_stockpile(batch);
    e.order(fighters(count)).expect("the stockpile covers the whole batch");
    let started = e.queue_events(mark);
    assert_eq!((started.len(), started[0].action, started[0].paid), (1, QueueAction::Started, batch), "a batch pays for every unit when it is ordered");
    assert_eq!(e.stockpile(), Resources::default());

    e.set_stockpile(pair.add(unit));
    e.order(fighters(2)).expect("a second batch behind the first");
    e.order(fighters(1)).expect("and a third");
    assert_eq!(e.stockpile(), Resources::default(), "a batch waiting for the yard has paid as well");
    let (second, third) = (e.queue_events(mark)[1].clone(), e.queue_events(mark)[2].clone());
    assert_eq!((second.action, second.waiting_on, second.waiting_for, second.paid), (QueueAction::Waiting, Some(WaitReason::Slot), None, pair));
    let line = e.state().homeworld.shipyard_pending;
    assert_eq!(line.iter().map(|q| (q.id, q.waiting_on, q.cost)).collect::<Vec<_>>(), [(second.id, BatchWait::Slot, pair), (third.id, BatchWait::Slot, unit)]);
    assert!(line[0].start_in.is_some_and(|ticks| ticks > 0) && line[1].start_in > line[0].start_in, "{line:?}");

    e.order(Command::CancelQueued { queue_type: QueueType::Ship, id: third.id }).expect("cancel the waiting batch");
    assert_eq!(e.queue_events(mark)[3].refunded, unit.scale(HALF), "a waiting batch refunds half of what it paid");
    assert_eq!(e.stockpile(), unit.scale(HALF));

    e.run_until(2000, |e| e.state().homeworld.shipyard_queue.is_some_and(|q| q.built >= 1));
    let running = e.state().homeworld.shipyard_queue.expect("the batch still holds the yard");
    assert!(running.built < running.count);
    let before = e.stockpile();
    let cancel_mark = e.mark();
    e.order(Command::CancelBuild { queue_type: QueueType::Ship, id: running.id }).expect("cancel the part-built batch");
    let cancelled = e.queue_events(cancel_mark);
    let refund = unit.scale(f64::from(running.count - running.built)).scale(HALF);
    assert_eq!(cancelled[0].refunded, refund, "half of every unit not delivered, the one under way included");
    assert!(cancelled[0].item.contains(&format!("{} already built and kept", running.built)), "{}", cancelled[0].item);
    assert_eq!(e.stockpile(), before.add(refund));
    assert_eq!(
        (cancelled[1].id, cancelled[1].action, cancelled[1].paid),
        (second.id, QueueAction::Started, Resources::default()),
        "the paid batch behind it takes the yard the moment it is free and pays nothing more"
    );

    let rest = count.saturating_sub(running.built + 2);
    if rest > 0 {
        e.set_stockpile(unit.scale(f64::from(rest)));
        e.order(fighters(rest)).expect("the rest of the group");
    }
    e.run_until(2000, yard_idle);
    let built: u16 = e.since(mark, |k| match k {
        EventKind::ShipBuilt(b) if b.ship_class == ShipClass::LightFighter => Some(b.count),
        _ => None,
    }).iter().sum();
    assert_eq!(built, running.built + 2 + rest);
    assert!(built >= count);
    assert_eq!(e.fleet().ship_count, 2 + u32::from(built), "new hulls reinforce the fleet at home");
}

fn hunt_pirates(e: &mut Empire, lair: Hex) {
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;

    let mut piles = Vec::new();
    for round in 0..4 {
        let mark = e.mark();
        if round == 0 {
            e.travel(lair);
        } else {
            e.run_until(3000, |e| e.engine.pending_template_npc(lair).is_some());
        }
        let engaged = |e: &Empire| e.since(mark, |k| matches!(k, EventKind::CombatStarted(_)).then_some(())).len();
        if engaged(e) == 0 {
            e.order(Command::Attack { fleet_id: fid, target_fleet_id: template_npc_id(lair) }).expect("attack the listed hostile");
        }
        e.run_until(120, |e| e.engine.active_combats.is_empty() && engaged(e) > 0);
        let (pile, _) = pile_of(&e.log[mark..]);
        assert!(e.log[mark..].iter().any(|ev| matches!(&ev.kind, EventKind::CombatEnded(c) if c.player_victory && c.mine)));
        piles.push(pile.total());
        let row = e.score_row();
        assert_eq!(
            row.score,
            row.buildings + row.research + row.fleet + row.outpost + row.defence,
            "a kill adds nothing to the score: it pays its wreckage",
        );

        let fleet = e.fleet();
        let mut room = fleet.cargo_capacity - fleet.cargo.total();
        let mut take = |amount: f64| {
            let taken = amount.min(room);
            room -= taken;
            taken
        };
        let fits = Resources { metal: take(pile.metal), crystal: take(pile.crystal), deuterium: take(pile.deuterium) };
        e.order(Command::CollectSalvage { fleet_id: fid }).expect("scoop the wreckage");
        let collected = e.since(mark, |k| match k {
            EventKind::SalvageCollected(c) => Some(c.resources),
            _ => None,
        });
        assert_eq!(collected.last(), Some(&fits), "the hold takes what was advertised, as far as it has room");
    }
    assert!(piles.windows(2).all(|w| w[1] < w[0]), "each repeat kill leaves less: {piles:?}");

    let stock = e.state().player.resources;
    e.travel(home);
    e.run(2);
    assert_eq!(e.fleet().cargo, Resources::default(), "docking unloads the hold");
    let unloaded = e.state().player.resources;
    assert!(unloaded.metal > stock.metal && unloaded.crystal > stock.crystal);
}

fn withstand_a_raid(e: &mut Empire) {
    e.order(Command::Build { building_type: BuildingType::Shipyard, reserve: false }).expect("second shipyard level");
    e.run_until(100, |e| e.state().homeworld.build_queue.is_empty() && e.state().homeworld.build_pending.is_empty());
    let mark = e.mark();

    for _ in 0..5000 {
        if !reinforce(e) && yard_idle(e) {
            break;
        }
        e.run(1);
    }
    let expected = e.state().homeworld.raid;
    assert_eq!(expected.odds.label, RaidLabel::Repelled, "the fighters at home destroy the raid the server forecasts: {expected:?}");
    let fighters_only = |ships: &[iac_shared::protocol::ShipCount]| ships.len() == 1 && ships[0].ship_class == ShipClass::LightFighter;
    assert!(expected.arrival_tick.is_none() && fighters_only(&expected.ships), "an empire with no defence is raided by Light Fighters");

    e.run_until(5000, |e| e.log[mark..].iter().any(|ev| matches!(ev.kind, EventKind::RaidIncoming(_))));
    let forecast = e.since(mark, |k| match k {
        EventKind::RaidIncoming(r) => Some(r.clone()),
        _ => None,
    });
    let warned_at = e.engine.current_tick();
    assert_eq!(forecast[0].arrival_tick - warned_at, e.engine.pace().raid_warning_ticks());
    assert!(forecast[0].threat == "light" && fighters_only(&forecast[0].ships), "{:?}", forecast[0]);
    let announced = e.state().homeworld.raid;
    assert_eq!((announced.arrival_tick, announced.power), (Some(forecast[0].arrival_tick), forecast[0].est_power));

    e.run_until(100, |e| e.log[mark..].iter().any(|ev| matches!(ev.kind, EventKind::RaidResolved(_))));
    let resolved = e.since(mark, |k| match k {
        EventKind::RaidResolved(r) => Some(r.clone()),
        _ => None,
    });
    assert!(resolved[0].defended, "{:?}", resolved[0]);
    assert_eq!(resolved[0].outcome, Some(BattleOutcome::HostilesDestroyed), "it was fought, and no raider was left");
    assert!((1..=RAID_ROUNDS).contains(&resolved[0].rounds));
    assert_eq!((resolved[0].ships_lost, resolved[0].resources_lost), (0, Resources::default()));
    assert!((resolved[0].raid_power - forecast[0].est_power).abs() < 1e-3, "the forecast was the raid");
    let pile = resolved[0].salvage_dropped.expect("a raid destroyed leaves wreckage");

    e.run(2);
    let scooped = e.since(mark, |k| match k {
        EventKind::Stockpile(s) if s.cause == StockpileCause::Wreckage => Some(s.amount),
        _ => None,
    });
    assert_eq!(scooped, [pile], "the docked fleet scoops the wreckage into storage");
}

fn settle_the_score(e: &mut Empire) {
    let state = e.state();
    let mut buildings = scaling::BuildingLevels::default();
    for b in &state.homeworld.buildings {
        buildings.set(b.building_type, b.level);
    }
    let mut research = scaling::ResearchLevels::default();
    for r in &state.homeworld.research {
        research.set(r.tech, r.level);
    }
    let fleet: f32 = state
        .fleets
        .iter()
        .flat_map(|f| &f.ships)
        .map(|s| s.count as f32 * scaling::resource_weight(&s.ship_class.build_cost()))
        .sum();
    let row = e.score_row();
    let built = scaling::building_points(&buildings) + scaling::research_points(&research);
    assert!(
        (row.buildings + row.research - built).abs() < 1e-3 * built,
        "buildings and research {} + {} vs what the empire holds {built}",
        row.buildings,
        row.research,
    );
    assert!((row.fleet - fleet).abs() < 1e-3 * fleet, "fleet {} vs the ships it has {fleet}", row.fleet);
    assert_eq!((row.outpost, row.defence), (0.0, 0.0));
    let expected = built + fleet;
    assert!((row.score - expected).abs() < 1e-3 * expected, "{} vs {expected}: what the empire has built and keeps", row.score);

    let outcome = e.engine.execute(e.pid, Command::Leaderboard { limit: 10 });
    let Some(ServerMessage::Leaderboard(table)) = outcome.reply else { panic!("no table") };
    let names: Vec<_> = table.entries.iter().map(|r| (r.rank, r.name.as_str())).collect();
    assert_eq!(names, [(1, "Arc"), (2, "Idler")]);
    assert!(table.you.is_none());
}

/// Order Light Fighters while the server's forecast of the next raid is
/// worse than a raid destroyed with nearly all that stands at home kept: as
/// many as the raid's power comes to, as far as the stockpile covers them,
/// once the yard is clear. True while more are wanted.
fn reinforce(e: &mut Empire) -> bool {
    let raid = e.state().homeworld.raid;
    if raid.odds.label == RaidLabel::Repelled && raid.odds.kept >= 0.95 {
        return false;
    }
    if yard_idle(e) {
        let lacking = (raid.power / scaling::ship_power(ShipClass::LightFighter)).ceil();
        let unit = ShipClass::LightFighter.build_cost();
        let stock = e.stockpile();
        let covered = (stock.metal / unit.metal).min(stock.crystal / unit.crystal).floor();
        let count = f64::from(lacking).min(covered) as u16;
        if count > 0 {
            e.order(fighters(count)).expect("reinforcements the stockpile covers");
        }
    }
    true
}

fn hold_the_line(e: &mut Empire, ticks: u64) {
    let mark = e.mark();
    for _ in 0..ticks {
        e.run(1);
        reinforce(e);
    }
    let raids = e.since(mark, |k| match k {
        EventKind::RaidResolved(r) => Some(r.clone()),
        _ => None,
    });
    assert!(raids.len() >= 2, "a long watch sees several raids: {}", raids.len());
    assert!(raids.iter().all(|r| r.defended && r.outcome == Some(BattleOutcome::HostilesDestroyed)), "{raids:?}");
    let (fell, felled): (u32, u32) = raids.iter().fold((0, 0), |(fell, felled), r| {
        (fell + r.ships_lost, felled + r.raiders_lost.iter().map(|s| s.count).sum::<u32>())
    });
    assert!(fell < felled, "the fighters destroyed more than they lost: {fell} for {felled}");
}
