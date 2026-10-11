// Outposts: one station per empire on a sector of rings 9 to 20, built from
// what a convoy carries to it. The founding expedition, every rule that
// refuses one, what the extractor makes at three paces, the dock (store,
// tanks, charts, the fleet cap), the levels paid from the store, the score,
// giving one up, the raids of its ring won and lost with the breach and the
// repair, a month of them on an outpost nobody defends, a week of them on
// outposts defended as their rings ask, what the balance report counts, and
// the whole of it replayed and resumed from a snapshot.

#[allow(dead_code)]
mod common;

use common::*;
use iac_shared::constants::{
    Density, ResourceKind, Resources, ShipClass, TerrainType, FUEL_DEUT_PER_UNIT, MAX_FLEETS_PER_PLAYER,
    SALVAGE_DESPAWN_TICKS,
};
use iac_shared::hex::{hex_ring, Hex};
use iac_shared::pace::Pace;
use iac_shared::protocol::{
    AlertEvent, AlertLevel, ChartMark, Command, ErrorCode, EventKind, OutpostState, OutpostSurvey, OutpostWork,
    PolicyPreset, RaidIncomingEvent, RaidResolvedEvent, ServerMessage, ThreatBasis,
};
use iac_shared::scaling::{self, RatioLabel};
use iac_shared::world::{NpcBehaviorType, SectorTemplate};
use iac_sim::engine::{template_npc_id, FleetStatus};
use iac_sim::headless::{Headless, OutpostOrder};
use iac_sim::script::IdlePlayer;
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

    let pack = scaling::npc_group_power(10);
    let unseen = survey(&mut e, Some(target));
    assert_eq!(
        unseen.refusal,
        Some(format!("{target} is not on your chart; a survey reads the site of a sector you have seen")),
    );
    assert_eq!(unseen.site, None, "nothing of a sector the empire has never seen is told");
    assert_eq!((unseen.ring, unseen.cost, unseen.build_ticks, unseen.platform_power), (10, COST, 5400, 40.0));
    assert!((unseen.raid_power.least - 1.6 * pack).abs() < 0.01 && (unseen.raid_power.most - 2.3 * pack).abs() < 0.01);
    let beyond = Hex::new(target.q * 3, target.r * 3);
    assert_eq!(
        survey(&mut e, Some(beyond)).refusal,
        Some(format!("{beyond} is at ring 30; an outpost stands in rings 9 to 20")),
        "the ring is no secret, so an unseen sector outside the rings is refused for its ring",
    );

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
    assert_eq!(quote.raid_power, unseen.raid_power);
    let found = quote.site.clone().expect("the fleet stands in the sector");
    assert_eq!(found.hops_home, Some(lane_path(&e.engine, target, home, 12).unwrap().len() as u32));

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
    assert!(e.fleet().at_outpost, "and the fleet is told to stand at it all the same");

    e.run(quote.build_ticks - 1);
    assert_eq!((outpost(&e).level, completed(&e, mark)), (0, vec![]));
    e.run(1);
    let post = outpost(&e);
    assert_eq!((post.level, post.in_service, post.work, completed(&e, mark)), (1, true, None, vec![1]));
    assert_eq!((post.platform_power, post.production), (40.0, found.production));
    assert_eq!(e.fleet().outpost_fuel, Some(0.0));

    let respawn = e.engine.pace().econ_ticks(f64::from(scaling::npc_respawn_hours(10)) * 3600.0);
    assert!(quote.build_ticks > respawn, "the lair's return is overdue: it took {respawn} ticks");
    assert!(e.engine.pending_template_npc(target).is_none(), "no group lives where an outpost stands");
    let seen = chart(&e.engine, pid)[&target.to_key()].clone();
    let brief = seen.outpost.clone().expect("the sector shows its outpost");
    assert_eq!((brief.owner.as_str(), brief.level, brief.breached, brief.mine), ("Founder", 1, false, true));
    assert_eq!((seen.hostiles.clone(), seen.threat.rating, seen.threat.est_power, seen.threat.basis), (None, 0, 0.0, ThreatBasis::Observed));
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
    let read = far.site.clone().expect("the sector is on the chart");
    assert_eq!(
        (read.densities.metal, read.densities.crystal, read.densities.deuterium),
        (template.metal_density, template.crystal_density, template.deut_density),
    );
    assert_eq!(read.grade, grade_of(&template));
    let ring_factor = 1.1f32.powi(9);
    let per_tick = |d: Density| 2.0 / 3600.0 * d.harvest_multiplier() * ring_factor * 10.0;
    let quoted = res(per_tick(template.metal_density), per_tick(template.crystal_density), per_tick(template.deut_density));
    assert!(near(read.production, quoted, 1e-6), "{:?} against {quoted:?}", read.production);
    assert_eq!(
        far.refusal,
        Some(format!("fleet {fid} is at {next}, 1 hex from {rich}; an outpost is founded by a fleet standing in its sector")),
        "the site qualifies: only the fleet's place stands in the way",
    );

    let home = e.engine.players[&e.pid].homeworld;
    let from_home = survey(&mut e, Some(home));
    assert_eq!(from_home.site.and_then(|s| s.hops_home), Some(0));
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
        let quoted = quote.site.as_ref().expect("the fleet stands in the sector").production;
        let per_tick = |d: Density| 2.0 / 3600.0 * d.harvest_multiplier() * 1.1f32.powi(9) * p;
        let rate = res(per_tick(template.metal_density), per_tick(template.crystal_density), per_tick(template.deut_density));
        assert!(near(quoted, rate, rate.total() * 1e-5), "{pace}: {quoted:?} against {rate:?}");
        assert_eq!(quote.build_ticks, ticks, "{pace}");

        e.order(Command::FoundOutpost { fleet_id: e.fid }).expect("found");
        e.run(ticks - 1);
        let waiting = outpost(&e);
        assert_eq!((waiting.level, waiting.store, waiting.production), (0, Resources::default(), Resources::default()), "{pace}");
        assert!(near(waiting.store_cap, cap, 0.05), "{pace}: {:?}", waiting.store_cap);

        e.run(1);
        let open = outpost(&e);
        assert_eq!((open.level, open.production, open.store), (1, quoted, quoted), "{pace}: it works from the tick it opens");
        e.run(999);
        let after = outpost(&e).store;
        assert!(near(after, rate.scale(1000.0), rate.total() * 1000.0 * 1e-3), "{pace}: {after:?} after 1000 ticks at {rate:?}");
        assert!(near(open.store_cap, cap, 0.05) && near(open.protected, res(250.0, 125.0, 50.0), 0.0), "{pace}");

        let pid = e.pid;
        let almost = res(open.store_cap.metal - rate.metal * 1.5, 0.0, 0.0);
        edit(&mut e.engine, |s| s.outposts.get_mut(&pid).unwrap().store = almost);
        let mark = e.mark();
        assert_eq!(outpost(&e).yield_lost, vec![], "{pace}: the store has room yet");
        e.run(50);
        assert_eq!(outpost(&e).store.metal, open.store_cap.metal, "{pace}: the extractor stops at the cap");
        assert_eq!(outpost(&e).yield_lost, vec![ResourceKind::Metal], "{pace}: the state names the yield that is lost");
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
    let foretold = outpost(&e).upgrade_refusal.expect("the state says beforehand why the order would be refused");
    let (code, why) = e.refusal(Command::UpgradeOutpost);
    assert_eq!(code, ErrorCode::InsufficientResources);
    assert_eq!(
        why,
        format!(
            "outpost level 2 costs 2500 metal, 1250 crystal and 500 deuterium; the store at {at} is short by {} metal \
             and 400 deuterium. A level is paid in full from the outpost's own store when it is ordered",
            (2500.0 - held.metal).ceil(),
        ),
    );
    assert_eq!(foretold, why, "and in the sentence the refusal carries");
    assert_eq!(e.engine.snapshot().hash(), before, "a refused upgrade changes nothing");

    deliver(&mut e, at, res(600.0, 0.0, 400.0));
    let held = outpost(&e).store;
    assert_eq!(outpost(&e).upgrade_refusal, None, "the store covers it, so nothing stands in the way");
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
        (code, why.clone()),
        (
            ErrorCode::InvalidCommand,
            format!("your outpost at {at} is building level 2 until tick {}; it does one job at a time", ordered + 13_500),
        ),
    );
    assert_eq!(building.upgrade_refusal, Some(why));

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
    assert_eq!(three.upgrade_refusal, Some(format!("your outpost at {at} is at level 3, the highest")));
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

fn forecasts(e: &Empire, mark: usize) -> Vec<(u64, RaidIncomingEvent)> {
    e.log[mark..]
        .iter()
        .filter_map(|event| match &event.kind {
            EventKind::RaidIncoming(raid) => Some((event.tick, raid.clone())),
            _ => None,
        })
        .collect()
}

fn outcomes(e: &Empire, mark: usize) -> Vec<(u64, RaidResolvedEvent)> {
    e.log[mark..]
        .iter()
        .filter_map(|event| match &event.kind {
            EventKind::RaidResolved(raid) => Some((event.tick, raid.clone())),
            _ => None,
        })
        .collect()
}

/// Play until the raid that is on its way has been resolved; the forecast and the outcome.
fn next_raid(e: &mut Empire, limit: u64) -> ((u64, RaidIncomingEvent), (u64, RaidResolvedEvent)) {
    let mark = e.mark();
    e.run_until(limit, |e| !outcomes(e, mark).is_empty());
    let warned = forecasts(e, mark).pop().expect("a raid is announced before it arrives");
    (warned, outcomes(e, mark).pop().unwrap())
}

fn everything(e: &Empire) -> Resources {
    let holds = e.engine.fleets.values().filter(|f| f.owner_id == e.pid).fold(Resources::default(), |sum, f| sum.add(f.cargo));
    e.engine.outposts[&e.pid].store.add(holds)
}

#[test]
fn a_platform_that_outguns_its_ring_repels_the_raid_and_loses_nothing() {
    let (mut e, at) = settle("blitz", 10, |t| t.metal_density != Density::None);
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    deliver(&mut e, at, res(2500.0, 1250.0, 500.0));
    e.order(Command::UpgradeOutpost).expect("level 2");
    muster(&mut e.engine, fid, &[(ShipClass::Scout, 1)], home);
    e.run(build_ticks(&e, 2));
    let post = outpost(&e);
    assert_eq!((post.level, post.platform_power, post.defence_power), (2, 100.0, 100.0), "nobody is home but the platform");
    let estimate = e.state().homeworld.next_raid_estimate_power;

    let opened = e.engine.outposts[&pid].raid.first_seen_tick;
    let min_age = e.engine.pace().attention_ticks(86_400.0);
    let before = post.store;
    let ((told, forecast), (resolved, outcome)) = next_raid(&mut e, 20_000);

    assert!(told >= opened + min_age, "no raid before the outpost has stood a day of raid clock: {told} against {opened} + {min_age}");
    assert_eq!((forecast.player_id, forecast.outpost), (pid, Some(at)));
    assert_eq!(forecast.arrival_tick, told + e.engine.pace().raid_warning_ticks(), "the homeworld's warning");
    let base = 2.0 * scaling::npc_group_power(10);
    assert!((base * 0.80..=base * 1.15).contains(&forecast.est_power), "{} against a base of {base}", forecast.est_power);
    assert_eq!(forecast.threat, if forecast.est_power < 50.0 { "light" } else { "moderate" });

    assert_eq!(resolved, forecast.arrival_tick);
    assert_eq!((outcome.outpost, outcome.defended, outcome.breached), (Some(at), true, false));
    assert_eq!((outcome.raid_power, outcome.defense_power), (forecast.est_power, 100.0));
    assert_eq!((outcome.resources_lost, outcome.ships_lost, outcome.structures_lost), (Resources::default(), 0, 0));
    assert_eq!((outcome.salvage_dropped, outcome.salvage_despawn_tick), (None, None), "a repelled outpost raid leaves no salvage");

    let after = outpost(&e);
    assert!(after.store.metal > before.metal && after.store.crystal >= before.crystal, "the store only grew: {:?}", after.store);
    assert!(after.in_service && !after.breached && after.raid_arrival_tick.is_none());
    assert_eq!(e.state().homeworld.next_raid_estimate_power, estimate);
    assert!(chart(&e.engine, pid)[&at.to_key()].salvage.is_none());
}

#[test]
fn a_lost_raid_takes_a_quarter_above_the_floor_a_ship_and_breaches_the_outpost_until_it_repairs_itself() {
    let (mut e, at) = settle("blitz", 16, |t| t.metal_density != Density::None && t.deut_density == Density::None);
    let (pid, fid) = (e.pid, e.fid);
    deliver(&mut e, at, res(2000.0, 600.0, 300.0));
    muster(&mut e.engine, fid, &[(ShipClass::Corvette, 2)], at);
    stow(&mut e, fid, res(300.0, 0.0, 0.0));
    let hold = e.fleet().cargo_capacity;
    assert!((hold - 489.9).abs() < 0.1, "two Corvettes hold 20 x sqrt(600): {hold}");
    let post = outpost(&e);
    assert_eq!((post.platform_power, post.defence_power), (40.0, 40.0 + 2.0 * 22.0));
    let floor = res(250.0, 125.0, 50.0);
    assert_eq!(post.protected, floor);

    let mark = e.mark();
    e.run_until(20_000, |e| !forecasts(e, mark).is_empty());
    let (told, forecast) = forecasts(&e, mark).pop().unwrap();
    assert_eq!((forecast.outpost, forecast.threat.as_str()), (Some(at), "heavy"));
    assert_eq!(outpost(&e).raid_arrival_tick, Some(forecast.arrival_tick));
    e.run(forecast.arrival_tick - told - 1);
    assert!(outcomes(&e, mark).is_empty());
    let before = everything(&e);
    let rate = outpost(&e).production;

    e.run(1);
    let (resolved, outcome) = outcomes(&e, mark).pop().expect("the raid arrives on its tick");
    assert_eq!(resolved, forecast.arrival_tick);
    assert_eq!((outcome.outpost, outcome.defended, outcome.breached, outcome.ships_lost), (Some(at), false, true, 1));
    assert_eq!(outcome.raid_power, forecast.est_power);
    let after = everything(&e);
    let pile = before.add(rate);
    let taken = res((pile.metal - 250.0) * 0.25, (pile.crystal - 125.0) * 0.25, (pile.deuterium - 50.0) * 0.25);
    assert!(near(outcome.resources_lost, taken, 0.01), "a quarter above the floor: {:?} against {taken:?}", outcome.resources_lost);
    assert!(near(after, pile.sub(taken), 0.01), "and no more: {after:?} left of {pile:?}");
    assert_eq!(outcome.protected_kept, floor);
    let fleet = e.fleet();
    assert_eq!(fleet.ships.len(), 1, "a quarter of the ships there, at least one");
    assert!(fleet.cargo.total() <= fleet.cargo_capacity + 0.01 && fleet.cargo_capacity < hold, "what the lost ship carried went to the store");

    let broken = outpost(&e);
    assert!(broken.breached && !broken.in_service);
    assert_eq!((broken.production, broken.platform_power, broken.defence_power), (Resources::default(), 0.0, 22.0));
    assert_eq!((broken.repair_cost, broken.work, broken.raid_arrival_tick), (Some(floor), None, None));
    assert_eq!(broken.repair_waiting_for, None, "the store covers the repair, so it waits for nothing");
    assert!(e.fleet().at_outpost && e.fleet().outpost_fuel.is_none(), "a fleet stands at a breached outpost that fills no tank");
    let brief = chart(&e.engine, pid)[&at.to_key()].outpost.clone().unwrap();
    assert!(brief.breached && brief.mine);
    assert_eq!(e.refusal(Command::UpgradeOutpost).0, ErrorCode::InvalidCommand, "a breached outpost repairs before it grows");

    let mark = e.mark();
    e.run(1);
    let repairing = outpost(&e);
    let work = repairing.work.expect("the repair starts by itself");
    let ticks = scaling::outpost_repair_ticks(1, &e.engine.pace());
    assert_eq!((work.kind, work.level, work.start_tick, work.end_tick), (OutpostWork::Repair, 1, resolved + 1, resolved + 1 + ticks));
    assert_eq!(ticks, 23, "a quarter of level 1's 90 blitz ticks");
    assert!(near(repairing.store, broken.store.sub(floor), 1e-3) && repairing.repair_cost.is_none());
    assert_eq!(
        said(&e, mark),
        vec![format!(
            "your outpost at {at} began its repair, paid 250 metal, 125 crystal and 50 deuterium from its store: \
             it is whole at tick {}, in 23 ticks",
            work.end_tick,
        )],
    );
    e.run(ticks - 1);
    assert!(outpost(&e).breached && outpost(&e).store == repairing.store, "the extractor stands still until the repair is done");
    e.run(1);
    let whole = outpost(&e);
    assert!(!whole.breached && whole.in_service && whole.work.is_none());
    assert_eq!((whole.platform_power, whole.production), (40.0, rate));
    assert!(whole.store.metal > repairing.store.metal, "it extracts again from the tick it is whole");
    assert_eq!(said(&e, mark).last().unwrap(), &format!("your outpost at {at} is repaired: its extractor, its tanks and its platform work again"));

    let grace = e.engine.pace().attention_ticks(172_800.0);
    let mark = e.mark();
    e.run_until(40_000, |e| !forecasts(e, mark).is_empty());
    let (again, _) = forecasts(&e, mark).pop().unwrap();
    assert!(again >= resolved + grace, "no raid is announced inside the grace: {again} against {resolved} + {grace}");
}

#[test]
fn a_breached_outpost_whose_store_cannot_pay_waits_for_a_delivery_and_says_so_once() {
    let (mut e, at) = settle("blitz", 18, |t| t.metal_density != Density::None);
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    let mark = e.mark();
    e.run_until(20_000, |e| !forecasts(e, mark).is_empty());
    for kind in ResourceKind::ALL {
        let _ = e.order(Command::Load { fleet_id: fid, resource: kind, amount: None });
    }
    assert!(e.fleet().cargo.metal > 250.0, "the owner carries the store off before the raid arrives");
    muster(&mut e.engine, fid, &[(ShipClass::Scout, 1)], home);
    stow(&mut e, fid, Resources::default());
    e.run_until(100, |e| !outcomes(e, mark).is_empty());
    let (resolved, outcome) = outcomes(&e, mark).pop().unwrap();
    assert_eq!((outcome.defended, outcome.breached, outcome.ships_lost), (false, true, 0), "nobody was there to lose");
    assert_eq!(outcome.resources_lost, Resources::default(), "nothing lies above a floor the store had not reached");

    let mark = e.mark();
    e.run(400);
    let waiting = outpost(&e);
    assert!(waiting.breached && waiting.work.is_none() && waiting.repair_cost == Some(res(250.0, 125.0, 50.0)));
    assert_eq!(
        waiting.repair_waiting_for,
        Some(waiting.store.shortfall(res(250.0, 125.0, 50.0))),
        "the state says what the repair waits for",
    );
    let warned = alerts_since(&e, mark);
    assert_eq!(warned.len(), 1, "{warned:?}");
    assert!(matches!(warned[0].level, AlertLevel::Warning));
    assert!(
        warned[0].message.starts_with(&format!("your outpost at {at} is breached and cannot repair itself: its store lacks"))
            && warned[0].message.contains("125 crystal")
            && warned[0].message.ends_with("until a fleet unloads the rest"),
        "{}",
        warned[0].message,
    );
    assert_eq!(e.engine.outposts[&pid].store, waiting.store, "a breached outpost extracts nothing, so it cannot save up the repair");
    assert_eq!(e.engine.current_tick(), resolved + 400);

    deliver(&mut e, at, res(250.0, 125.0, 50.0));
    let stocked = outpost(&e).store;
    e.run(1);
    let repairing = outpost(&e);
    assert_eq!(repairing.work.map(|w| w.kind), Some(OutpostWork::Repair), "the repair starts on the delivery");
    assert!(near(repairing.store, stocked.sub(res(250.0, 125.0, 50.0)), 1e-3));
    e.run(scaling::outpost_repair_ticks(1, &e.engine.pace()));
    assert!(outpost(&e).in_service);
}

#[test]
fn thirty_days_of_raids_never_destroy_an_undefended_outpost() {
    let (mut e, at) = settle("season", 18, |t| t.metal_density != Density::None && grade_of(t) >= 2.0);
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    deliver(&mut e, at, res(2000.0, 3000.0, 1500.0));
    muster(&mut e.engine, fid, &[(ShipClass::Scout, 1)], home);
    stow(&mut e, fid, Resources::default());
    let estimate = e.state().homeworld.next_raid_estimate_power;
    let core = e.score_row().core;

    let mark = e.mark();
    let (mut breached, mut lowest) = (0u64, f32::MAX);
    for _ in 0..30 * 24 {
        e.run(360);
        let post = &e.engine.outposts[&pid];
        breached += u64::from(post.breached);
        lowest = lowest.min(post.store.metal).min(post.store.crystal).min(post.store.deuterium);
        assert_eq!((post.sector, post.level), (at, 1), "it still stands at tick {}", e.engine.current_tick());
    }
    let raids = outcomes(&e, mark);
    let lost = raids.iter().filter(|(_, raid)| !raid.defended).count();
    assert!(raids.len() >= 5 && lost == raids.len(), "{lost} of {} raids were lost: nothing defends it", raids.len());
    assert!(raids.iter().all(|(_, raid)| raid.outpost == Some(at) && raid.breached && raid.ships_lost == 0));
    assert_eq!(forecasts(&e, mark).len(), raids.len(), "every raid was announced");
    let repairs = said(&e, mark).iter().filter(|line| line.contains("is repaired")).count();
    assert!(repairs + 1 >= raids.len(), "it repaired itself after each: {repairs} repairs for {} raids", raids.len());
    assert!((1..30 * 24 / 4).contains(&breached), "breached at {breached} of 720 looks: down a while after each raid, up most of the time");
    assert!(lowest >= 0.0);
    let grace = e.engine.pace().attention_ticks(172_800.0);
    for pair in raids.windows(2) {
        assert!(pair[1].0 - pair[0].0 >= grace, "two lost raids {} ticks apart, inside the grace of {grace}", pair[1].0 - pair[0].0);
    }

    assert_eq!(e.state().homeworld.next_raid_estimate_power, estimate, "the outpost never sized a raid on the homeworld");
    assert_eq!(e.score_row().core, core, "and its level still counts");
}

#[test]
fn the_balance_report_counts_what_an_outpost_made_and_what_raids_took() {
    let weight = |r: Resources| scaling::resource_weight(&r);
    let mut held = Headless::new(SEED, Pace::parse("blitz").unwrap());
    let mut bare = Headless::new(SEED, Pace::parse("blitz").unwrap());
    let stocked = res(4000.0, 2000.0, 1000.0);
    let settle = |world: &mut Headless, ring: u16, upgrade: bool| -> u64 {
        let pid = world.add_player(Box::new(IdlePlayer { label: "Keeper".to_string() }), "idle").expect("a seat");
        let fid = world.engine.fleets.values().find(|f| f.owner_id == pid).unwrap().id;
        let home = world.engine.players[&pid].homeworld;
        let at = site(&world.engine, ring, |t| t.metal_density != Density::None);
        muster(&mut world.engine, fid, &[(ShipClass::Hauler, 2)], at);
        edit(&mut world.engine, |s| s.fleets.get_mut(&fid).unwrap().cargo = COST.add(stocked));
        for cmd in [Command::FoundOutpost { fleet_id: fid }, Command::Unload { fleet_id: fid }] {
            assert!(world.engine.execute(pid, cmd).error.is_none());
        }
        world.run(90).unwrap();
        if upgrade {
            assert!(world.engine.execute(pid, Command::UpgradeOutpost).error.is_none());
        }
        muster(&mut world.engine, fid, &[(ShipClass::Scout, 1)], home);
        world.run(scaling::outpost_build_ticks(2, &world.engine.pace())).unwrap();
        pid
    };
    let keeper = settle(&mut held, 10, true);
    let loser = settle(&mut bare, 18, false);

    let before = (held.summaries()[0].field, held.engine.outposts[&keeper].store);
    held.run(6000).unwrap();
    let report = held.summaries().remove(0);
    let post = &held.engine.outposts[&keeper];
    assert!((report.score.outpost - 7.525).abs() < 1e-3 && report.score.core >= report.score.econ + report.score.fleet + 7.5);
    assert!(report.field.outpost_raids_repelled >= 2, "{:?}", report.field);
    assert_eq!((report.field.outpost_raids_lost, report.field.outpost_ticks_breached, report.field.outpost_raided), (0, 0, 0.0));
    assert_eq!((report.field.raids_repelled, report.field.raids_lost), (0, 0), "the homeworld's raids are counted apart");
    let made = report.field.outpost_yield - before.0.outpost_yield;
    let grew = weight(post.store) - weight(before.1);
    assert!(made > 1.0 && (made - grew).abs() < 0.002 * made, "the yield counted is what the store gained: {made} against {grew}");
    let field = report.field;
    assert!(
        (field.outpost_yield + field.outpost_spilled - field.outpost_potential).abs() < 0.002 * field.outpost_potential,
        "never breached, it made its rate of every tick but what the full store turned away: {field:?}",
    );
    assert_eq!((field.outpost_repairs, field.outpost_ticks_waiting, field.outpost_hauled), (0.0, 0, 0.0));
    let site = report.outpost.expect("the report says where it stands");
    assert_eq!((site.sector, site.ring, site.level, site.breached, site.waiting), (post.sector, 10, 2, false, false));
    assert!((site.payback_hours - 1700.0 / (2.0 * site.grade * scaling::ring_mult(10))).abs() < 0.05, "{site:?}");
    assert_eq!(site.next_mine_level, 2, "an idle empire's next Metal Mine level");

    let (fid, at) = (held.engine.fleets.values().find(|f| f.owner_id == keeper).unwrap().id, post.sector);
    muster(&mut held.engine, fid, &[(ShipClass::Hauler, 2)], at);
    for (resource, amount) in [(Metal, Some(300.0)), (Crystal, None)] {
        assert!(held.engine.execute(keeper, Command::Load { fleet_id: fid, resource, amount }).error.is_none());
    }
    let aboard = held.engine.fleets[&fid].cargo;
    assert!(aboard.metal == 300.0 && aboard.crystal >= 750.0, "{aboard:?}");
    held.run(1).unwrap();
    let hauled = held.summaries()[0].field.outpost_hauled;
    assert!((hauled - weight(aboard)).abs() < 1e-3, "what load took from the store is counted as hauled: {hauled} against {aboard:?}");
    held.run(50).unwrap();
    assert_eq!(held.summaries()[0].field.outpost_hauled, hauled, "and counted once");

    bare.run(6000).unwrap();
    let report = bare.summaries().remove(0);
    let field = report.field;
    assert!(field.outpost_raids_lost >= 2 && field.outpost_raids_repelled == 0, "{field:?}");
    assert!(field.outpost_raided > 0.0 && field.outpost_ticks_breached >= u64::from(field.outpost_raids_lost) * 23, "{field:?}");
    assert!((report.score.outpost - 2.15).abs() < 1e-3);
    let post = &bare.engine.outposts[&loser];
    let repairs = weight(res(250.0, 125.0, 50.0)) * (field.outpost_raids_lost - u32::from(post.breached && post.job.is_none())) as f32;
    assert!((field.outpost_repairs - repairs).abs() < 1e-3, "every repair it paid is counted: {field:?}");
    let kept = weight(post.store) - weight(stocked);
    let owed = field.outpost_yield - field.outpost_raided - repairs;
    assert!((kept - owed).abs() < 0.02 * field.outpost_yield, "yield less raids less repairs is what the store kept: {kept} against {owed}");
    assert_eq!((field.outpost_raids_on_breach, field.outpost_ticks_waiting), (0, 0), "stocked, it was whole again before each raid");
    assert!(field.outpost_potential > field.outpost_yield + field.outpost_spilled, "a breached outpost makes less than its rate: {field:?}");
}

/// An idle empire with an outpost stood for it as `sim-run --outpost` stands one, its fleet
/// mustered there as `garrison` when there is one.
fn stand(world: &mut Headless, ring: u16, level: u8, garrison: &[(ShipClass, usize)]) -> u64 {
    let label = format!("Keeper-{}", world.players());
    let pid = world.add_player(Box::new(IdlePlayer { label }), "idle").expect("a seat");
    let fid = world.engine.fleets.values().find(|f| f.owner_id == pid).unwrap().id;
    let order = OutpostOrder { ring, grade: 5.0, level, building: false, repairs: 1 };
    let at = world.install_outposts(&[pid], order).expect("a free site in the ring")[0];
    if !garrison.is_empty() {
        muster(&mut world.engine, fid, garrison, at);
    }
    pid
}

/// A ring, a level, and the ships standing there.
type Post = (u16, u8, &'static [(ShipClass, usize)]);

#[test]
fn a_season_week_of_raids_takes_nothing_from_an_outpost_defended_as_its_ring_asks() {
    use ShipClass::{Corvette, Cruiser, Frigate};
    let weight = |r: Resources| scaling::resource_weight(&r);
    let mut world = Headless::new(SEED, Pace::parse("season").unwrap());
    let posts: [Post; 6] = [
        (9, 1, &[(Corvette, 1)]),
        (12, 1, &[(Corvette, 3)]),
        (12, 2, &[]),
        (17, 3, &[]),
        (18, 3, &[(Frigate, 1)]),
        (20, 3, &[(Cruiser, 2)]),
    ];
    let keepers: Vec<u64> = posts.iter().map(|&(ring, level, garrison)| stand(&mut world, ring, level, garrison)).collect();
    let sites: Vec<Hex> = keepers.iter().map(|pid| world.engine.outposts[pid].sector).collect();
    for (i, a) in sites.iter().enumerate() {
        let t = world.engine.world_gen.generate_sector(*a);
        assert!(t.terrain != TerrainType::Anomaly && !t.has_derelict, "{a} is ground an outpost may stand on");
        assert_eq!(a.dist_from_origin(), posts[i].0);
        assert!(sites[..i].iter().all(|b| Hex::distance(a, b) >= 3), "{a} keeps its distance");
    }
    let stocked: Vec<f32> = keepers.iter().map(|pid| weight(world.engine.outposts[pid].store)).collect();

    world.run(7 * 86_400).unwrap();

    for ((report, (ring, level, _)), stocked) in world.summaries().into_iter().zip(posts).zip(stocked) {
        let field = report.field;
        let what = format!("level {level} at ring {ring}: {field:?}");
        assert!(field.outpost_raids_repelled >= 8, "raids came all week, {what}");
        assert_eq!((field.outpost_raids_lost, field.outpost_raided, field.outpost_ticks_breached, field.ships_lost), (0, 0.0, 0, 0), "{what}");
        assert_eq!(field.outpost_ticks, 7 * 86_400);
        let site = report.outpost.unwrap();
        assert!(site.level == level && !site.breached && site.grade > 0.0, "{site:?}");
        let kept = weight(site.store) - stocked;
        assert!((kept - field.outpost_yield).abs() < 0.005 * field.outpost_yield, "all it made is in its store, {what}: {kept}");
        assert!((field.outpost_yield + field.outpost_spilled - field.outpost_potential).abs() < 0.005 * field.outpost_potential, "{what}");
    }
}

/// A world with an outpost, a convoy shuttling out and back, levels and raids, played for
/// `ticks`. `stops` are ticks at which the world is written to JSON and read back.
fn story(ticks: u64, stops: &[u64]) -> (u64, Vec<&'static str>, Vec<u64>) {
    let mut e = Empire::join(world("blitz"), "Teller");
    let fid = e.fid;
    let at = in_outer_ring(&e.engine, |at, t| {
        at.dist_from_origin() == 15
            && open_ground(t)
            && t.metal_density != Density::None
            && e.engine.world_gen.connected_neighbors(at).slice().iter().any(|n| open_ground(&e.engine.world_gen.generate_sector(*n)))
    });
    let next = *e
        .engine
        .world_gen
        .connected_neighbors(at)
        .slice()
        .iter()
        .find(|n| open_ground(&e.engine.world_gen.generate_sector(**n)))
        .unwrap();
    muster(&mut e.engine, fid, &[(ShipClass::Hauler, 2), (ShipClass::Scout, 6)], at);
    stow(&mut e, fid, COST.add(res(2500.0, 1250.0, 500.0)).add(res(0.0, 0.0, 600.0)));
    e.order(Command::FoundOutpost { fleet_id: fid }).expect("found");
    e.order(Command::Unload { fleet_id: fid }).expect("the outpost under construction has a store");

    let (mut seen, mut marks) = (Vec::new(), Vec::new());
    let note = |what: &'static str, tick: u64, seen: &mut Vec<&'static str>, marks: &mut Vec<u64>| {
        if !seen.contains(&what) {
            seen.push(what);
            marks.push(tick);
        }
    };
    for _ in 0..ticks {
        let tick = e.engine.current_tick();
        if stops.contains(&tick) {
            let json = serde_json::to_string(&e.engine.snapshot()).unwrap();
            e.engine = GameEngine::restore(serde_json::from_str(&json).unwrap());
        }
        let post = e.engine.outposts[&e.pid].clone();
        if post.level == 0 {
            note("level 1 under construction", tick, &mut seen, &mut marks);
        }
        if post.level == 1 && post.job.is_some() {
            note("level 2 under construction", tick, &mut seen, &mut marks);
        }
        if post.raid.incoming.is_some() {
            note("a raid announced", tick, &mut seen, &mut marks);
        }
        if post.breached {
            note("breached", tick, &mut seen, &mut marks);
        }
        if tick >= 2500 && tick.is_multiple_of(50) {
            let _ = e.order(Command::UpgradeOutpost);
        }
        let here = e.engine.fleets.get(&fid).map(|f| f.location);
        match (tick % 300, here) {
            (10, Some(place)) if place == at => {
                let _ = e.order(Command::Load { fleet_id: fid, resource: Metal, amount: None });
                let _ = e.order(Command::Move { fleet_id: fid, target: next });
            }
            (40, Some(place)) if place == next => {
                let _ = e.order(Command::Move { fleet_id: fid, target: at });
            }
            (70, Some(place)) if place == at => {
                let _ = e.order(Command::Unload { fleet_id: fid });
            }
            _ => {}
        }
        e.run(1);
        e.engine.persist_dirty_state();
    }
    let raids = outcomes(&e, 0);
    let lost = raids.iter().filter(|(_, raid)| !raid.defended).count();
    assert!(lost >= 1 && raids.len() > lost, "the story has raids won and lost in it: {lost} lost of {}", raids.len());
    (e.engine.snapshot().hash(), seen, marks)
}

#[test]
fn a_world_with_an_outpost_replays_and_resumes_from_a_snapshot_to_the_same_state() {
    const TICKS: u64 = 6000;
    let (straight, seen, marks) = story(TICKS, &[]);
    for wanted in ["level 1 under construction", "level 2 under construction", "a raid announced", "breached"] {
        assert!(seen.contains(&wanted), "the story never came to: {wanted} (it came to {seen:?})");
    }
    assert_eq!(story(TICKS, &[]).0, straight, "the same seed and the same orders give the same world");

    let stops: Vec<u64> = marks.iter().map(|tick| tick + 3).collect();
    let (resumed, ..) = story(TICKS, &stops);
    assert_eq!(resumed, straight, "stopped and read back at ticks {stops:?} ({seen:?}), the world went on as if it had not been");
}
