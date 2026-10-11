// Outposts: one station per empire on a sector of rings 9 to 20, built from
// what a convoy carries to it. The founding expedition, every rule that
// refuses one, what the extractor makes at three paces, the dock (store,
// tanks, charts, the fleet cap), the levels paid from the store, the score,
// and giving one up.

#[allow(dead_code)]
mod common;

use common::*;
use iac_shared::constants::{
    Density, ResourceKind, Resources, ShipClass, TerrainType, FUEL_DEUT_PER_UNIT, MAX_FLEETS_PER_PLAYER,
    SALVAGE_DESPAWN_TICKS,
};
use iac_shared::hex::{hex_ring, Hex};
use iac_shared::protocol::{
    AlertEvent, AlertLevel, ChartMark, Command, ErrorCode, EventKind, OutpostState, OutpostSurvey, OutpostWork,
    PolicyPreset, ServerMessage, ThreatBasis,
};
use iac_shared::scaling::{self, RatioLabel};
use iac_shared::world::{NpcBehaviorType, SectorTemplate};
use iac_sim::engine::{template_npc_id, FleetStatus};
use iac_sim::GameEngine;

use ResourceKind::{Crystal, Deuterium, Metal};

const COST: Resources = Resources { metal: 1000.0, crystal: 500.0, deuterium: 200.0 };

fn res(metal: f32, crystal: f32, deuterium: f32) -> Resources {
    Resources { metal, crystal, deuterium }
}

fn grade_of(t: &SectorTemplate) -> f32 {
    scaling::site_grade([t.metal_density, t.crystal_density, t.deut_density])
}

/// No lair, no derelict, no Anomaly: nothing but the ring can refuse an outpost here.
fn open_ground(t: &SectorTemplate) -> bool {
    t.npc_template.is_none() && !t.has_derelict && t.terrain != TerrainType::Anomaly
}

/// Open ground in `ring` whose ore `wanted` accepts.
fn site(engine: &GameEngine, ring: u16, wanted: impl Fn(&SectorTemplate) -> bool) -> Hex {
    sector_in_ring(engine, ring, |_, t| open_ground(t) && wanted(t))
}

fn any_ore(t: &SectorTemplate) -> bool {
    grade_of(t) >= 1.0
}

/// The first sector of rings 9 to 20 that `wanted` accepts.
fn in_outer_ring(engine: &GameEngine, wanted: impl Fn(Hex, &SectorTemplate) -> bool) -> Hex {
    (9..=20u16)
        .flat_map(|ring| hex_ring(Hex::ORIGIN, ring))
        .find(|at| wanted(*at, &engine.world_gen.generate_sector(*at)))
        .expect("such a sector in the Outer Ring")
}

fn stow(e: &mut Empire, fid: u64, cargo: Resources) {
    edit(&mut e.engine, |s| {
        let fleet = s.fleets.get_mut(&fid).unwrap();
        fleet.cargo = cargo;
        fleet.state = FleetStatus::Idle;
        fleet.move_target = None;
    });
}

/// Haulers enough to hold `load` at this world's pace.
fn haulers_for(engine: &GameEngine, load: f32) -> usize {
    (load / (200.0 * engine.pace().cargo_mult())).ceil() as usize
}

/// Stand the empire's fleet on `at` as Haulers carrying `cargo`.
fn haul(e: &mut Empire, at: Hex, cargo: Resources) {
    let fid = e.fid;
    let haulers = haulers_for(&e.engine, cargo.total().max(1.0));
    muster(&mut e.engine, fid, &[(ShipClass::Hauler, haulers)], at);
    stow(e, fid, cargo);
}

/// Ships with the tanks they are built with, full.
fn crew(e: &mut Empire, fid: u64, classes: &[(ShipClass, usize)], at: Hex) {
    muster(&mut e.engine, fid, classes, at);
    edit(&mut e.engine, |s| {
        let fleet = s.fleets.get_mut(&fid).unwrap();
        let tank: f32 = fleet.ships[..fleet.ship_count].iter().map(|s| s.ship_class.base_stats().fuel as f32).sum();
        fleet.fuel_max = tank;
        fleet.fuel = tank;
    });
}

fn found(e: &mut Empire, at: Hex) {
    haul(e, at, COST);
    e.order(Command::FoundOutpost { fleet_id: e.fid }).expect("the site qualifies");
}

fn survey(e: &mut Empire, sector: Option<Hex>) -> OutpostSurvey {
    let outcome = e.engine.execute(e.pid, Command::OutpostSurvey { fleet_id: e.fid, sector });
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    let Some(ServerMessage::OutpostSurvey(survey)) = outcome.reply else { panic!("no survey") };
    survey
}

fn outpost(e: &Empire) -> OutpostState {
    e.state().outpost.expect("the empire has an outpost")
}

fn build_ticks(e: &Empire, level: u8) -> u64 {
    scaling::outpost_build_ticks(level, &e.engine.pace())
}

/// An empire whose outpost on `at` has just finished level 1, the founding
/// Haulers still standing there with empty holds.
fn settle(pace: &str, ring: u16, wanted: impl Fn(&SectorTemplate) -> bool) -> (Empire, Hex) {
    let mut e = Empire::join(world(pace), "Settler");
    let at = site(&e.engine, ring, wanted);
    found(&mut e, at);
    e.run(build_ticks(&e, 1));
    assert_eq!(outpost(&e).level, 1);
    (e, at)
}

fn deliver(e: &mut Empire, at: Hex, cargo: Resources) {
    haul(e, at, cargo);
    e.order(Command::Unload { fleet_id: e.fid }).expect("unload at the outpost");
}

fn alerts_since(e: &Empire, mark: usize) -> Vec<AlertEvent> {
    e.since(mark, |k| match k {
        EventKind::Alert(a) => Some(a.clone()),
        _ => None,
    })
}

fn said(e: &Empire, mark: usize) -> Vec<String> {
    alerts_since(e, mark).into_iter().map(|a| a.message).collect()
}

fn completed(e: &Empire, mark: usize) -> Vec<u8> {
    e.since(mark, |k| match k {
        EventKind::OutpostCompleted(done) => Some(done.level),
        _ => None,
    })
}

fn near(a: Resources, b: Resources, tolerance: f32) -> bool {
    ResourceKind::ALL.into_iter().all(|k| (a.get(k) - b.get(k)).abs() <= tolerance)
}

fn no_mines(e: &mut Empire) {
    let pid = e.pid;
    edit(&mut e.engine, |s| {
        let b = &mut s.players.get_mut(&pid).unwrap().buildings;
        (b.metal_mine, b.crystal_mine, b.deuterium_synthesizer) = (0, 0, 0);
    });
}

#[test]
fn a_convoy_carries_an_outpost_to_a_lair_and_founds_it_on_the_sector_it_won() {
    let mut e = Empire::join(world("season"), "Founder");
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    let target = hex_ring(Hex::ORIGIN, 10)
        .find(|at| {
            let t = e.engine.world_gen.generate_sector(*at);
            t.npc_template.as_ref().is_some_and(|npc| npc.behavior != NpcBehaviorType::Passive)
                && !t.has_derelict
                && t.terrain != TerrainType::Anomaly
                && lane_path(&e.engine, home, *at, 9).is_some()
        })
        .expect("a hostile lair in ring 10 within nine hops of home");

    muster(&mut e.engine, fid, &[(ShipClass::Corvette, 4), (ShipClass::Hauler, 3)], home);
    e.set_stockpile(COST);
    for kind in ResourceKind::ALL {
        e.order(Command::Load { fleet_id: fid, resource: kind, amount: Some(COST.get(kind)) }).expect("load at home");
    }
    assert_eq!((e.fleet().cargo, e.stockpile()), (COST, Resources::default()), "the cost is aboard and nowhere else");

    let (code, why) = e.refusal(Command::OutpostSurvey { fleet_id: fid, sector: Some(target) });
    assert_eq!(code, ErrorCode::InvalidTarget);
    assert_eq!(why, format!("{target} is not on your chart; a survey reads a sector you have seen"));

    let mark = e.mark();
    e.travel(target);
    e.run_until(2000, |e| e.engine.fleets[&fid].state == FleetStatus::Idle);
    let won = e.since(mark, |k| match k {
        EventKind::CombatEnded(fight) => Some((fight.sector, fight.player_victory)),
        _ => None,
    });
    assert_eq!(won, vec![(target, true)], "the convoy took the sector from its pack");
    let convoy = e.fleet();
    assert_eq!(convoy.cargo, COST, "the convoy lost no hold on the way: {} ships", convoy.ships.len());

    let quote = survey(&mut e, None);
    assert_eq!((quote.sector, quote.ring, quote.refusal.clone()), (target, 10, None));
    assert_eq!((quote.cost, quote.build_ticks, quote.platform_power), (COST, 5400, 40.0));
    let pack = scaling::npc_group_power(10);
    assert!((quote.raid_power - 2.0 * pack * 0.975).abs() < 0.01, "{}", quote.raid_power);
    assert_eq!(quote.hops_home, Some(lane_path(&e.engine, target, home, 12).unwrap().len() as u32));

    let mark = e.mark();
    let founded_at = e.engine.current_tick();
    e.order(Command::FoundOutpost { fleet_id: fid }).expect("the sector is won and the hold pays");
    assert_eq!(e.fleet().cargo, Resources::default(), "the hold paid exactly the cost");
    let told = said(&e, mark);
    assert_eq!(told.len(), 1);
    assert!(
        told[0].starts_with(&format!(
            "fleet {fid} founded your outpost at {target} for 1000 metal, 500 crystal and 200 deuterium: \
             level 1 is finished at tick {}, in 5400 ticks",
            founded_at + 5400,
        )),
        "{told:?}",
    );
    let post = outpost(&e);
    assert_eq!((post.location, post.level, post.max_level, post.in_service), (target, 0, 3, false));
    let work = post.work.expect("level 1 is being built");
    assert_eq!((work.kind, work.level, work.start_tick, work.end_tick), (OutpostWork::Build, 1, founded_at, founded_at + 5400));
    assert_eq!((post.production, post.platform_power), (Resources::default(), 0.0));
    assert_eq!(e.fleet().outpost_fuel, None, "an outpost under construction fills no tank");

    e.run(quote.build_ticks - 1);
    assert_eq!((outpost(&e).level, completed(&e, mark)), (0, vec![]));
    e.run(1);
    let post = outpost(&e);
    assert_eq!((post.level, post.in_service, post.work, completed(&e, mark)), (1, true, None, vec![1]));
    assert_eq!((post.platform_power, post.production), (40.0, quote.production));
    assert_eq!(e.fleet().outpost_fuel, Some(0.0));

    let respawn = e.engine.pace().econ_ticks(f64::from(scaling::npc_respawn_hours(10)) * 3600.0);
    assert!(quote.build_ticks > respawn, "the lair's return is overdue: it took {respawn} ticks");
    assert!(e.engine.pending_template_npc(target).is_none(), "no group lives where an outpost stands");
    let seen = chart(&e.engine, pid)[&target.to_key()].clone();
    let brief = seen.outpost.clone().expect("the sector shows its outpost");
    assert_eq!((brief.owner.as_str(), brief.level, brief.breached, brief.mine), ("Founder", 1, false, true));
    assert_eq!((seen.hostiles.clone(), seen.threat.rating, seen.threat.est_power, seen.threat.basis), (None, 1, 0.0, ThreatBasis::Observed));
    let outcome = e.engine.execute(pid, Command::Odds { fleet_id: fid, sectors: vec![target] });
    let Some(ServerMessage::Odds(odds)) = outcome.reply else { panic!("no odds") };
    assert_eq!((odds.sectors[0].ratio, odds.sectors[0].label), (10.0, RatioLabel::Safe));

    let step = *e.engine.world_gen.connected_neighbors(target).slice().first().expect("a lane out");
    sweep(&mut e.engine, &[step]);
    e.travel(step);
    let aboard = e.engine.fleets[&fid].charts.len();
    assert!(aboard > 0, "the way out was charted and nothing has been delivered");
    let mark = e.mark();
    e.travel(target);
    let fights = e.since(mark, |k| matches!(k, EventKind::CombatStarted(_)).then_some(()));
    assert!(fights.is_empty(), "coming back to the outpost starts no fight");
    let paid = e.since(mark, |k| match k {
        EventKind::ChartDelivered(c) => Some((c.fleet_id, c.sectors)),
        _ => None,
    });
    assert_eq!(paid, vec![(fid, aboard as u32)], "the charts aboard are paid on arrival at the outpost");
    assert_eq!(chart(&e.engine, pid)[&step.to_key()].chart, Some(ChartMark::Delivered));
    assert!(e.engine.fleets[&fid].charts.is_empty());
}

#[test]
fn founding_is_refused_with_the_rule_that_forbids_it() {
    let mut e = Empire::join(world("season"), "Surveyor");
    let (pid, fid) = (e.pid, e.fid);
    let refused = |e: &mut Empire, at: Hex, cargo: Resources| -> (ErrorCode, String) {
        haul(e, at, cargo);
        let before = e.engine.snapshot().hash();
        let (code, why) = e.refusal(Command::FoundOutpost { fleet_id: fid });
        assert_eq!(e.engine.snapshot().hash(), before, "a refused founding at {at} changes nothing: {why}");
        assert_eq!(survey(e, None).refusal, Some(why.clone()), "the survey gives the same sentence");
        (code, why)
    };

    let inside = sector_in_ring(&e.engine, 8, |_, t| open_ground(t));
    assert_eq!(
        refused(&mut e, inside, COST),
        (ErrorCode::InvalidTarget, format!("fleet {fid} is at ring 8; an outpost stands in rings 9 to 20")),
    );
    let beyond = sector_in_ring(&e.engine, 21, |_, t| open_ground(t));
    assert_eq!(
        refused(&mut e, beyond, COST),
        (ErrorCode::InvalidTarget, format!("fleet {fid} is at ring 21; an outpost stands in rings 9 to 20")),
    );

    let anomaly = in_outer_ring(&e.engine, |_, t| t.terrain == TerrainType::Anomaly && !t.has_derelict);
    assert_eq!(
        refused(&mut e, anomaly, COST),
        (ErrorCode::InvalidTarget, format!("{anomaly} is an Anomaly; no outpost stands in one")),
    );
    let wreck = in_outer_ring(&e.engine, |_, t| t.has_derelict && t.terrain != TerrainType::Anomaly);
    assert_eq!(
        refused(&mut e, wreck, COST),
        (ErrorCode::InvalidTarget, format!("a derelict drifts at {wreck}; no outpost stands beside one, stripped or not")),
    );
    let lair = in_outer_ring(&e.engine, |_, t| t.npc_template.is_some() && !t.has_derelict && t.terrain != TerrainType::Anomaly);
    let pack = e.engine.world_gen.generate_sector(lair).npc_template.unwrap().label();
    assert_eq!(
        refused(&mut e, lair, COST),
        (ErrorCode::InvalidTarget, format!("a {pack} holds {lair}; an outpost is founded on a sector its founder has won")),
    );

    let ground = site(&e.engine, 11, |_| true);
    assert_eq!(
        refused(&mut e, ground, res(1000.0, 499.0, 200.0)),
        (
            ErrorCode::NoResources,
            format!(
                "the hold of fleet {fid} lacks 1 crystal of the 1000 metal, 500 crystal and 200 deuterium an outpost costs; \
                 it is built from what the founding fleet carries, loaded at home"
            ),
        ),
    );
    assert_eq!(refused(&mut e, ground, res(0.0, 500.0, 0.0)).0, ErrorCode::NoResources);

    let den = in_outer_ring(&e.engine, |at, t| {
        at.dist_from_origin() >= 16
            && t.npc_template.as_ref().is_some_and(|npc| npc.behavior != NpcBehaviorType::Passive && npc.count >= 4)
            && !t.has_derelict
            && t.terrain != TerrainType::Anomaly
    });
    haul(&mut e, den, COST);
    e.order(Command::Attack { fleet_id: fid, target_fleet_id: template_npc_id(den) }).expect("pick the fight");
    assert_eq!(
        e.refusal(Command::FoundOutpost { fleet_id: fid }),
        (
            ErrorCode::OnCooldown,
            format!("fleet {fid} is in combat; an outpost is founded on a sector its founder has won"),
        ),
    );
    edit(&mut e.engine, |s| {
        s.combats.clear();
        s.npc_fleets.clear();
    });

    haul(&mut e, ground, COST);
    let next = e.engine.world_gen.connected_neighbors(ground).slice()[0];
    e.order(Command::Move { fleet_id: fid, target: next }).expect("a jump");
    let (code, why) = e.refusal(Command::FoundOutpost { fleet_id: fid });
    assert_eq!((code, why), (ErrorCode::OnCooldown, format!("fleet {fid} is in transit; it can found an outpost once it stands idle")));
    e.order(Command::Stop { fleet_id: fid }).expect("call the jump off");

    e.engine.handle_policy_update(pid, fid, PolicyPreset::Prospect, None).expect("standing orders");
    let (code, why) = e.refusal(Command::FoundOutpost { fleet_id: fid });
    assert_eq!(
        (code, why),
        (
            ErrorCode::InvalidCommand,
            format!("fleet {fid} is under standing orders (prospect); set it to manual before founding an outpost"),
        ),
    );
    e.engine.handle_policy_update(pid, fid, PolicyPreset::Manual, None).expect("manual again");

    let (rival, rival_fleet) = join(&mut e.engine, "Rival");
    let theirs = site(&e.engine, 14, |_| true);
    muster(&mut e.engine, rival_fleet, &[(ShipClass::Hauler, 3)], theirs);
    edit(&mut e.engine, |s| s.fleets.get_mut(&rival_fleet).unwrap().cargo = COST);
    let outcome = e.engine.execute(rival, Command::FoundOutpost { fleet_id: rival_fleet });
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    e.absorb();

    let ok_at = |e: &Empire, hexes: u16| {
        in_outer_ring(&e.engine, |at, t| open_ground(t) && Hex::distance(&at, &theirs) == hexes)
    };
    let crowded = ok_at(&e, 2);
    assert_eq!(
        refused(&mut e, crowded, COST),
        (
            ErrorCode::InvalidTarget,
            format!("{crowded} is 2 hexes from the outpost of Rival at {theirs}; outposts stand at least 3 hexes apart"),
        ),
    );
    assert_eq!(refused(&mut e, theirs, COST).0, ErrorCode::InvalidTarget, "nor on the sector itself");

    let far_enough = ok_at(&e, 3);
    haul(&mut e, far_enough, COST);
    assert_eq!(survey(&mut e, None).refusal, None);
    e.order(Command::FoundOutpost { fleet_id: fid }).expect("three hexes is room enough");

    let elsewhere = site(&e.engine, 18, |_| true);
    assert_eq!(
        refused(&mut e, elsewhere, COST),
        (
            ErrorCode::InvalidCommand,
            format!("you already have an outpost at {far_enough}; an empire has one, and abandon_outpost gives it up"),
        ),
    );
}

#[test]
fn a_survey_reads_any_charted_sector_and_says_what_stands_in_the_way() {
    let mut e = Empire::join(world("season"), "Scout");
    let fid = e.fid;
    e.run(1);
    let rich = site(&e.engine, 12, |t| grade_of(t) >= 4.0);
    let template = e.engine.world_gen.generate_sector(rich);
    let next = e.engine.world_gen.connected_neighbors(rich).slice()[0];
    haul(&mut e, next, Resources::default());
    sweep(&mut e.engine, &[rich, next]);
    e.travel(rich);
    e.travel(next);

    let far = survey(&mut e, Some(rich));
    assert_eq!((far.sector, far.ring, far.fleet_id), (rich, 12, fid));
    assert_eq!(
        (far.densities.metal, far.densities.crystal, far.densities.deuterium),
        (template.metal_density, template.crystal_density, template.deut_density),
    );
    assert_eq!(far.grade, grade_of(&template));
    let ring_factor = 1.1f32.powi(9);
    let per_tick = |d: Density| 2.0 / 3600.0 * d.harvest_multiplier() * ring_factor * 10.0;
    let quoted = res(per_tick(template.metal_density), per_tick(template.crystal_density), per_tick(template.deut_density));
    assert!(near(far.production, quoted, 1e-6), "{:?} against {quoted:?}", far.production);
    assert_eq!(
        far.refusal,
        Some(format!("fleet {fid} is at {next}, 1 hex from {rich}; an outpost is founded by a fleet standing in its sector")),
        "the site qualifies: only the fleet's place stands in the way",
    );

    let home = e.engine.players[&e.pid].homeworld;
    let from_home = survey(&mut e, Some(home));
    assert_eq!(from_home.hops_home, Some(0));
    assert_eq!(
        from_home.refusal,
        Some(format!("{home} is at ring {}; an outpost stands in rings 9 to 20", home.dist_from_origin())),
    );

    let (code, why) = e.refusal(Command::OutpostSurvey { fleet_id: 999, sector: None });
    assert_eq!((code, why.as_str()), (ErrorCode::FleetNotFound, "fleet 999 not found (or it has no ships left)"));
}

#[test]
fn the_extractor_makes_what_the_survey_said_at_every_pace() {
    for (pace, p, ticks, cap) in [
        ("persistent", 1.0f32, 54_000u64, res(3000.0, 1500.0, 750.0)),
        ("season", 10.0, 5400, res(9486.833, 4743.4165, 2371.7083)),
        ("blitz", 600.0, 90, res(73_484.69, 36_742.346, 18_371.173)),
    ] {
        let mut e = Empire::join(world(pace), "Miner");
        let at = site(&e.engine, 12, |t| t.metal_density != Density::None && grade_of(t) >= 3.0);
        let template = e.engine.world_gen.generate_sector(at);
        haul(&mut e, at, COST);
        let quote = survey(&mut e, None);
        let per_tick = |d: Density| 2.0 / 3600.0 * d.harvest_multiplier() * 1.1f32.powi(9) * p;
        let rate = res(per_tick(template.metal_density), per_tick(template.crystal_density), per_tick(template.deut_density));
        assert!(near(quote.production, rate, rate.total() * 1e-5), "{pace}: {:?} against {rate:?}", quote.production);
        assert_eq!(quote.build_ticks, ticks, "{pace}");

        e.order(Command::FoundOutpost { fleet_id: e.fid }).expect("found");
        e.run(ticks - 1);
        let waiting = outpost(&e);
        assert_eq!((waiting.level, waiting.store, waiting.production), (0, Resources::default(), Resources::default()), "{pace}");
        assert!(near(waiting.store_cap, cap, 0.05), "{pace}: {:?}", waiting.store_cap);

        e.run(1);
        let open = outpost(&e);
        assert_eq!((open.level, open.production, open.store), (1, quote.production, quote.production), "{pace}: it works from the tick it opens");
        e.run(999);
        let after = outpost(&e).store;
        assert!(near(after, rate.scale(1000.0), rate.total() * 1000.0 * 1e-3), "{pace}: {after:?} after 1000 ticks at {rate:?}");
        assert!(near(open.store_cap, cap, 0.05) && near(open.protected, res(250.0, 125.0, 50.0), 0.0), "{pace}");

        let pid = e.pid;
        let almost = res(open.store_cap.metal - rate.metal * 1.5, 0.0, 0.0);
        edit(&mut e.engine, |s| s.outposts.get_mut(&pid).unwrap().store = almost);
        let mark = e.mark();
        e.run(50);
        assert_eq!(outpost(&e).store.metal, open.store_cap.metal, "{pace}: the extractor stops at the cap");
        let warned: Vec<AlertEvent> = alerts_since(&e, mark);
        assert_eq!(warned.len(), 1, "{pace}: one warning a fill: {warned:?}");
        assert!(matches!(warned[0].level, AlertLevel::Warning) && warned[0].sector == Some(at) && warned[0].fleet_id.is_none());
        assert_eq!(
            warned[0].message,
            format!(
                "your outpost at {at} holds all the metal its store takes ({:.0}): the extractor makes no more of it \
                 until a fleet loads some",
                open.store_cap.metal,
            ),
        );

        edit(&mut e.engine, |s| {
            let post = s.outposts.get_mut(&pid).unwrap();
            post.breached = true;
            post.store = Resources::default();
        });
        e.run(200);
        let broken = outpost(&e);
        assert_eq!((broken.store, broken.production, broken.in_service), (Resources::default(), Resources::default(), false), "{pace}");
    }
}

#[test]
fn a_convoy_round_trip_makes_nothing_but_the_yield() {
    let mut e = Empire::join(world("season"), "Carrier");
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    no_mines(&mut e);
    let at = hex_ring(Hex::ORIGIN, 9)
        .filter(|at| {
            let t = e.engine.world_gen.generate_sector(*at);
            open_ground(&t) && t.metal_density != Density::None && t.deut_density == Density::None
        })
        .min_by_key(|at| lane_path(&e.engine, home, *at, 12).map_or(usize::MAX, |p| p.len()))
        .expect("an ore site in ring 9");
    let out = lane_path(&e.engine, home, at, 12).expect("a lane to the site");
    let back = lane_path(&e.engine, at, home, 12).expect("a lane home");
    assert!(out.len() + back.len() <= 12, "two Haulers fly 12 hops on a tank: {} out", out.len());

    found(&mut e, at);
    e.run(build_ticks(&e, 1));
    crew(&mut e, fid, &[(ShipClass::Hauler, 2)], at);
    stow(&mut e, fid, Resources::default());
    e.set_stockpile(res(0.0, 0.0, 400.0));
    e.run(600);

    let rate = outpost(&e).production;
    let owned = |e: &Empire| {
        let holds = e.engine.fleets.values().filter(|f| f.owner_id == pid).fold(Resources::default(), |sum, f| sum.add(f.cargo));
        e.stockpile().add(e.engine.outposts[&pid].store).add(holds)
    };
    let jump = e.fleet().jump_fuel;
    assert_eq!((jump, e.fleet().fuel, e.fleet().fuel_max), (16.0, 200.0, 200.0));
    let (before, started) = (owned(&e), e.engine.current_tick());

    e.order(Command::Load { fleet_id: fid, resource: Metal, amount: None }).expect("load the yield");
    let carried = e.fleet().cargo.metal;
    assert!(carried > 0.0 && !e.fleet().laden, "the hold is the fleet's own business at the outpost");
    sweep(&mut e.engine, &back);
    e.travel(home);
    e.run(2);
    assert!((e.stockpile().metal - carried).abs() < 0.01, "the home dock unloads the yield: {:?}", e.stockpile());
    assert_eq!(e.fleet().fuel, 200.0, "and fills the tank from the stockpile");

    e.order(Command::Load { fleet_id: fid, resource: Deuterium, amount: Some(100.0) }).expect("load fuel for the outpost");
    sweep(&mut e.engine, &out);
    e.travel(at);
    e.order(Command::Unload { fleet_id: fid }).expect("unload at the outpost");
    e.run(2);
    assert_eq!(e.fleet().fuel, 200.0, "the outpost fills the tank from what the convoy brought");

    let burned = jump * (out.len() + back.len()) as f32;
    let made = rate.scale((e.engine.current_tick() - started) as f32);
    let after = owned(&e);
    let spent = res(0.0, 0.0, burned * FUEL_DEUT_PER_UNIT);
    assert!(
        near(after.add(spent), before.add(made), 0.05),
        "{after:?} owned and {spent:?} burned, against {before:?} and {made:?} extracted",
    );
}

#[test]
fn the_outpost_fills_tanks_from_its_store_and_says_once_when_it_cannot() {
    let (mut e, at) = settle("season", 10, |t| t.deut_density == Density::None && grade_of(t) >= 1.0);
    let (pid, fid) = (e.pid, e.fid);
    let next = e.engine.world_gen.connected_neighbors(at).slice()[0];
    sweep(&mut e.engine, &[next]);
    crew(&mut e, fid, &[(ShipClass::Hauler, 1), (ShipClass::Corvette, 1)], at);
    stow(&mut e, fid, res(40.0, 0.0, 0.0));

    let mark = e.mark();
    e.travel(next);
    sweep(&mut e.engine, &[next]);
    e.travel(at);
    let fleet = e.fleet();
    assert_eq!((fleet.fuel, fleet.fuel_max, fleet.jump_fuel), (124.0, 150.0, 13.0), "two jumps burned, none refilled");
    let dry: Vec<AlertEvent> = alerts_since(&e, mark).into_iter().filter(|a| a.message.contains("no deuterium left")).collect();
    assert_eq!(dry.len(), 1, "{dry:?}");
    assert!(matches!(dry[0].level, AlertLevel::Warning) && dry[0].fleet_id == Some(fid));
    assert_eq!(
        dry[0].message,
        format!(
            "your outpost at {at} has no deuterium left for the tank of fleet {fid}; unload some into its store: \
             fleet {fid} stands at 124 of 150 fuel"
        ),
    );
    e.run(300);
    assert_eq!(alerts_since(&e, mark).iter().filter(|a| a.message.contains("no deuterium left")).count(), 1, "once a stay");
    assert_eq!(e.fleet().fuel, 124.0, "nothing fills a tank from an empty store, and a fleet at a dock is not stranded");

    edit(&mut e.engine, |s| s.outposts.get_mut(&pid).unwrap().store.deuterium = 1.0);
    e.run(1);
    assert_eq!((e.fleet().fuel, outpost(&e).store.deuterium), (136.5, 0.0), "one deuterium buys 12.5 fuel");
    assert_eq!(alerts_since(&e, mark).iter().filter(|a| a.message.contains("no deuterium left")).count(), 1);

    edit(&mut e.engine, |s| s.outposts.get_mut(&pid).unwrap().store.deuterium = 50.0);
    e.run(1);
    let filled = e.fleet();
    assert_eq!(filled.fuel, 150.0);
    assert!((outpost(&e).store.deuterium - (50.0 - 13.5 * FUEL_DEUT_PER_UNIT)).abs() < 1e-4, "the home price: 0.08 a unit");
    assert_eq!(filled.cargo, res(40.0, 0.0, 0.0), "nothing at an outpost unloads by itself");

    let mark = e.mark();
    e.travel(next);
    edit(&mut e.engine, |s| s.outposts.get_mut(&pid).unwrap().store.deuterium = 0.0);
    sweep(&mut e.engine, &[next]);
    e.engine.handle_policy_update(pid, fid, PolicyPreset::PatrolHome, None).expect("standing orders");
    edit(&mut e.engine, |s| {
        s.outposts.get_mut(&pid).unwrap().store.deuterium = 50.0;
        let fleet = s.fleets.get_mut(&fid).unwrap();
        fleet.location = at;
        fleet.state = FleetStatus::Idle;
        fleet.move_target = None;
    });
    let before = e.fleet();
    e.run(1);
    let under_orders = e.fleet();
    assert!(under_orders.fuel <= before.fuel, "a fleet under standing orders is not filled at the outpost");
    assert_eq!((outpost(&e).store.deuterium, under_orders.cargo), (50.0, before.cargo), "nor does it touch the store");
    assert!(alerts_since(&e, mark).iter().all(|a| !a.message.contains("no deuterium left")));
}

#[test]
fn a_level_is_paid_from_the_store_and_the_outpost_works_on_meanwhile() {
    let (mut e, at) = settle("season", 12, |t| t.metal_density != Density::None && grade_of(t) >= 2.0);
    let fid = e.fid;
    let one = outpost(&e);
    let next = one.next_level.clone().expect("level 2 is on offer");
    assert_eq!((next.level, next.cost, next.ticks, next.platform_power), (2, res(2500.0, 1250.0, 500.0), 13_500, 100.0));
    assert!(near(next.store_cap, one.store_cap.scale(2.5), 0.05) && near(next.production, one.production.scale(2.0), 1e-6));

    deliver(&mut e, at, res(2000.0, 1250.0, 100.0));
    let held = outpost(&e).store;
    let before = e.engine.snapshot().hash();
    let (code, why) = e.refusal(Command::UpgradeOutpost);
    assert_eq!(code, ErrorCode::InsufficientResources);
    assert_eq!(
        why,
        format!(
            "outpost level 2 costs 2500 metal, 1250 crystal and 500 deuterium; the store at {at} is short by {} metal \
             and 400 deuterium. Nothing was started: a level is paid in full from the outpost's own store when it is ordered",
            (2500.0 - held.metal).ceil(),
        ),
    );
    assert_eq!(e.engine.snapshot().hash(), before, "a refused upgrade changes nothing");

    deliver(&mut e, at, res(600.0, 0.0, 400.0));
    let held = outpost(&e).store;
    let (mark, ordered) = (e.mark(), e.engine.current_tick());
    e.order(Command::UpgradeOutpost).expect("the store covers level 2");
    let building = outpost(&e);
    assert!(near(building.store, held.sub(next.cost), 1e-3), "{:?}", building.store);
    let work = building.work.expect("level 2 is being built");
    assert_eq!((work.kind, work.level, work.start_tick, work.end_tick), (OutpostWork::Build, 2, ordered, ordered + 13_500));
    assert_eq!((building.level, building.in_service, building.platform_power), (1, true, 40.0));
    assert_eq!((building.production, building.store_cap), (one.production, one.store_cap), "it works at level 1 meanwhile");
    assert_eq!(building.next_level.map(|n| n.level), Some(3), "what could be ordered after this one");
    let told = said(&e, mark);
    assert!(told[0].starts_with(&format!("your outpost at {at} began level 2, paid 2500 metal, 1250 crystal and 500 deuterium from its store")), "{told:?}");

    let (code, why) = e.refusal(Command::UpgradeOutpost);
    assert_eq!(
        (code, why),
        (
            ErrorCode::InvalidCommand,
            format!("your outpost at {at} is building level 2 until tick {}; it does one job at a time", ordered + 13_500),
        ),
    );

    let stored = outpost(&e).store.metal;
    e.run(13_499);
    assert_eq!(outpost(&e).level, 1);
    assert!((outpost(&e).store.metal - stored - one.production.metal * 13_499.0).abs() < 0.5, "the extractor ran all the while");
    e.run(1);
    let two = outpost(&e);
    assert_eq!((two.level, two.work, two.platform_power, completed(&e, mark)), (2, None, 100.0, vec![2]));
    assert_eq!((two.store_cap, two.production), (next.store_cap, next.production));
    assert_eq!(two.protected, res(625.0, 312.5, 125.0), "the floor is the repair cost of the level it stands at");

    let last = two.next_level.expect("level 3 is on offer");
    assert_eq!((last.level, last.cost, last.ticks, last.platform_power), (3, res(6250.0, 3125.0, 1250.0), 33_750, 250.0));
    deliver(&mut e, at, last.cost);
    e.order(Command::UpgradeOutpost).expect("the store covers level 3");
    e.run(33_750);
    let three = outpost(&e);
    assert_eq!((three.level, three.platform_power, three.next_level.clone()), (3, 250.0, None));
    assert!(near(three.production, one.production.scale(3.0), 1e-6) && near(three.store_cap, one.store_cap.scale(6.25), 0.5));
    assert_eq!(
        e.refusal(Command::UpgradeOutpost),
        (ErrorCode::MaxLevelReached, format!("your outpost at {at} is at level 3, the highest")),
    );
    assert_eq!(e.engine.fleets[&fid].location, at);
}

#[test]
fn an_outpost_scores_its_finished_levels_and_nothing_it_holds() {
    let mut e = Empire::join(world("season"), "Counter");
    let fid = e.fid;
    let at = site(&e.engine, 12, any_ore);
    muster(&mut e.engine, fid, &[(ShipClass::Hauler, 16)], at);
    stow(&mut e, fid, COST);
    let before = e.score_row();

    e.order(Command::FoundOutpost { fleet_id: fid }).expect("found");
    assert_eq!(e.score_row().core, before.core, "an outpost under construction scores nothing");
    e.run(build_ticks(&e, 1));
    let one = e.score_row();
    assert!((one.core - before.core - 2.15).abs() < 1e-3, "level 1 is 1000 + 1.5 x 500 + 2 x 200 a thousand: {}", one.core - before.core);

    stow(&mut e, fid, res(5000.0, 3000.0, 1000.0));
    e.order(Command::Unload { fleet_id: fid }).expect("unload at the outpost");
    e.run(500);
    e.order(Command::Load { fleet_id: e.fid, resource: Crystal, amount: Some(100.0) }).expect("load from the store");
    assert_eq!(e.score_row().core, one.core, "the store, the yield, load and unload score nothing");

    e.order(Command::UpgradeOutpost).expect("level 2");
    assert_eq!(e.score_row().core, one.core, "a level scores when it is finished, not when it is paid");
    e.run(build_ticks(&e, 2));
    let two = e.score_row();
    assert!((two.core - one.core - 5.375).abs() < 1e-3, "level 2 adds 5.375: {}", two.core - one.core);
    assert_eq!((two.combat, two.explore), (before.combat, before.explore));

    let pid = e.pid;
    let estimate = |e: &Empire| e.state().homeworld.next_raid_estimate_power;
    let raid = estimate(&e);
    e.order(Command::AbandonOutpost).expect("abandon");
    assert_eq!(e.score_row().core, before.core, "its score goes with it");
    assert_eq!(estimate(&e), raid, "an outpost never sized the raids on the homeworld");
    assert!(!e.engine.outposts.contains_key(&pid));
}

#[test]
fn a_fleet_at_the_outpost_is_docked_and_needs_a_slot_to_leave() {
    let (mut e, at) = settle("season", 9, |_| true);
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    muster(&mut e.engine, fid, &[(ShipClass::Scout, 5)], home);
    stow(&mut e, fid, Resources::default());
    let ships: Vec<u64> = e.fleet().ships.iter().map(|s| s.id).collect();
    let mut wings = vec![fid];
    for ship in &ships[1..] {
        e.order(Command::Split { fleet_id: fid, ship_ids: vec![*ship] }).expect("split at home");
        let newest = e.engine.fleets.values().filter(|f| f.owner_id == pid).map(|f| f.id).max().unwrap();
        wings.push(newest);
    }
    assert_eq!(wings.len(), 5);

    let garrison = wings[0];
    let way = lane_path(&e.engine, home, at, 12).expect("a lane to the outpost");
    sweep(&mut e.engine, &way);
    e.travel_fleet(garrison, at);
    let lanes: Vec<Hex> = e.engine.world_gen.connected_neighbors(home).slice().to_vec();
    sweep(&mut e.engine, &lanes);
    for wing in &wings[1..=MAX_FLEETS_PER_PLAYER] {
        e.order(Command::Move { fleet_id: *wing, target: lanes[0] }).expect("three fleets may be away, with a fourth at the outpost");
    }
    e.run_until(60, |e| wings[1..=MAX_FLEETS_PER_PLAYER].iter().all(|w| e.engine.fleets[w].location == lanes[0]));

    let out = e.engine.world_gen.connected_neighbors(at).slice()[0];
    let (code, why) = e.refusal(Command::Move { fleet_id: garrison, target: out });
    assert_eq!((code, why.as_str()), (ErrorCode::FleetLimitReached, "fleet limit reached: at most 3 fleets can be deployed"));
    assert_eq!(e.refusal(Command::Move { fleet_id: wings[4], target: lanes[0] }).0, ErrorCode::FleetLimitReached, "nor the one at home");

    e.order(Command::Move { fleet_id: wings[1], target: home }).expect("one comes home");
    e.run_until(60, |e| e.engine.fleets[&wings[1]].location == home);
    sweep(&mut e.engine, &[out]);
    e.order(Command::Move { fleet_id: garrison, target: out }).expect("a slot is free");
}

#[test]
fn another_empires_outpost_is_a_mark_on_the_chart_and_no_dock() {
    let (mut e, at) = settle("season", 10, any_ore);
    let (rival, rival_fleet) = join(&mut e.engine, "Rival");
    let next = e.engine.world_gen.connected_neighbors(at).slice()[0];
    muster(&mut e.engine, rival_fleet, &[(ShipClass::Hauler, 1)], next);
    edit(&mut e.engine, |s| s.fleets.get_mut(&rival_fleet).unwrap().cargo = res(50.0, 0.0, 0.0));
    sweep(&mut e.engine, &[next]);
    let outcome = e.engine.execute(rival, Command::Move { fleet_id: rival_fleet, target: at });
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    e.run_until(60, |e| e.engine.fleets[&rival_fleet].location == at && e.engine.fleets[&rival_fleet].state == FleetStatus::Idle);

    let seen = chart(&e.engine, rival)[&at.to_key()].clone();
    let brief = seen.outpost.expect("anyone with eyes on the sector sees the outpost");
    assert_eq!((brief.owner.as_str(), brief.level, brief.breached, brief.mine), ("Settler", 1, false, false));
    assert!(e.engine.active_combats.is_empty());
    let theirs = iac_sim::views::full_state(&e.engine, rival).unwrap();
    assert!(theirs.outpost.is_none() && theirs.fleets[0].outpost_fuel.is_none());

    let outcome = e.engine.execute(rival, Command::Unload { fleet_id: rival_fleet });
    let (code, why) = outcome.error.expect("it is not their dock");
    assert_eq!(code, ErrorCode::NotInSector);
    assert!(why.contains("at the homeworld only"), "{why}");
    let before = e.engine.fleets[&rival_fleet].fuel;
    e.run(5);
    assert_eq!(e.engine.fleets[&rival_fleet].fuel, before, "and it fills no stranger's tank");

    let outcome = e.engine.execute(rival, Command::Move { fleet_id: rival_fleet, target: next });
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    e.run_until(60, |e| e.engine.fleets[&rival_fleet].location == next);
    e.order(Command::AbandonOutpost).expect("abandon");
    e.run(2);
    let remembered = chart(&e.engine, rival)[&at.to_key()].clone();
    assert!(!remembered.live && remembered.outpost.is_some(), "a chart remembers the outpost it last saw");
}

#[test]
fn an_abandoned_outpost_leaves_its_store_as_wreckage_and_the_lair_comes_back() {
    let mut e = Empire::join(world("season"), "Quitter");
    let (pid, fid) = (e.pid, e.fid);
    let at = in_outer_ring(&e.engine, |_, t| {
        t.npc_template.as_ref().is_some_and(|npc| npc.behavior != NpcBehaviorType::Passive)
            && !t.has_derelict
            && t.terrain != TerrainType::Anomaly
            && (t.metal_density != Density::None, t.crystal_density, t.deut_density) == (true, Density::None, Density::None)
    });
    let ring = at.dist_from_origin();
    fight(&mut e.engine, pid, fid, at);
    found(&mut e, at);
    e.run(build_ticks(&e, 1) + 400);
    deliver(&mut e, at, res(0.0, 75.0, 20.0));
    let respawn = e.engine.pace().econ_ticks(f64::from(scaling::npc_respawn_hours(ring)) * 3600.0);
    assert!(e.engine.sector_overrides[&at.to_key()].npc_cleared_tick.is_none(), "the lair's own delay ran out long ago");
    assert!(e.engine.pending_template_npc(at).is_none() && chart(&e.engine, pid)[&at.to_key()].hostiles.is_none());

    let store = outpost(&e).store;
    assert!(store.metal > 0.0 && store.crystal == 75.0);
    let (mark, tick) = (e.mark(), e.engine.current_tick());
    e.order(Command::AbandonOutpost).expect("abandon");
    assert_eq!(
        said(&e, mark),
        vec![format!(
            "your outpost at {at} is abandoned: {} metal, 75 crystal and 20 deuterium from its store drifts there as \
             wreckage for 180 ticks. Nothing is refunded, and you may found another",
            store.metal.ceil(),
        )],
    );
    let state = e.state();
    assert!(state.outpost.is_none() && e.fleet().outpost_fuel.is_none());
    let ov = &e.engine.sector_overrides[&at.to_key()];
    assert_eq!((ov.salvage, ov.salvage_despawn_tick), (Some(store), Some(tick + u64::from(SALVAGE_DESPAWN_TICKS))));
    assert_eq!(ov.npc_cleared_tick, Some(tick));
    assert_eq!(e.refusal(Command::AbandonOutpost), (ErrorCode::InvalidCommand, "you have no outpost to abandon".to_string()));
    assert_eq!(e.refusal(Command::UpgradeOutpost), (ErrorCode::InvalidCommand, "you have no outpost to upgrade".to_string()));

    e.run(1);
    let seen = chart(&e.engine, pid)[&at.to_key()].clone();
    assert!(seen.outpost.is_none() && seen.salvage == Some(store), "{seen:?}");
    e.order(Command::CollectSalvage { fleet_id: fid }).expect("the wreckage is there for the taking");
    assert!(e.fleet().cargo.metal > 0.0);

    haul(&mut e, at, COST);
    assert_eq!(survey(&mut e, None).refusal, None, "the site is free again while the lair is away");
    e.run(respawn - 2);
    assert_eq!(e.engine.current_tick(), tick + respawn - 1);
    assert!(e.engine.pending_template_npc(at).is_none());
    e.run(1);
    assert!(e.engine.pending_template_npc(at).is_some(), "the displaced lair returns after its usual delay");
    assert_eq!(e.refusal(Command::FoundOutpost { fleet_id: fid }).0, ErrorCode::InvalidTarget, "and holds the sector");

    let fresh = site(&e.engine, 15, any_ore);
    found(&mut e, fresh);
    assert_eq!(outpost(&e).location, fresh, "an empire that gave one up may found another");
}
