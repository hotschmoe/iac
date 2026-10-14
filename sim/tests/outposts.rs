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
    Density, ResourceKind, Resources, ShipClass, TerrainType, FUEL_DEUT_PER_UNIT, MAX_FLEETS_PER_PLAYER, RAID_ROUNDS,
    SALVAGE_DESPAWN_TICKS,
};
use iac_shared::hex::{hex_ring, Hex};
use iac_shared::maths;
use iac_shared::pace::Pace;
use iac_shared::protocol::{
    AlertEvent, AlertLevel, BattleOutcome, BattleRoundEvent, ChartMark, Command, ErrorCode, EventKind, OutpostState,
    OutpostSurvey, OutpostWork, PolicyPreset, RaidIncomingEvent, RaidLabel, RaidResolvedEvent, ServerMessage, ThreatBasis,
    UnitCount,
};
use iac_shared::scaling::{self, DefenceKind, RatioLabel, ShipyardItem};
use iac_shared::world::{NpcBehaviorType, SectorTemplate};
use iac_sim::engine::{template_npc_id, FleetStatus};
use iac_sim::headless::{Headless, OutpostOrder};
use iac_sim::script::IdlePlayer;
use iac_sim::GameEngine;

use ResourceKind::{Crystal, Deuterium, Metal};

const COST: Resources = Resources { metal: 3000.0, crystal: 1500.0, deuterium: 600.0 };
/// The platform at each level, as the defences it stands for add up: four
/// Rocket Launchers; eight and four Light Lasers; sixteen, eight and four
/// Heavy Lasers.
const PLATFORM: [f32; 3] = [408.0, 1306.0, 3972.0];
/// The repair of level 1: a quarter of its cost at par, in metal.
const REPAIR: f64 = 1612.5;

fn res(metal: f64, crystal: f64, deuterium: f64) -> Resources {
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

/// Small Cargos enough to hold `load` at this world's pace.
fn haulers_for(engine: &GameEngine, load: f64) -> u32 {
    let hold = f64::from(ShipClass::SmallCargo.base_stats().cargo);
    (load / (hold * engine.pace().cargo_mult())).ceil() as u32
}

/// Stand the empire's fleet on `at` as Small Cargos carrying `cargo`.
fn haul(e: &mut Empire, at: Hex, cargo: Resources) {
    let fid = e.fid;
    let haulers = haulers_for(&e.engine, cargo.total().max(1.0));
    muster(&mut e.engine, fid, &[(ShipClass::SmallCargo, haulers)], at);
    stow(e, fid, cargo);
}

/// Ships with the tanks they are built with, full.
fn crew(e: &mut Empire, fid: u64, classes: &[(ShipClass, u32)], at: Hex) {
    muster(&mut e.engine, fid, classes, at);
    edit(&mut e.engine, |s| {
        let fleet = s.fleets.get_mut(&fid).unwrap();
        let none = scaling::ResearchLevels::default();
        let tank: f64 = fleet.ships.counts().map(|(class, count)| f64::from(count) * scaling::ship_tank(class, &none)).sum();
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
/// Small Cargos still standing there with empty holds.
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

fn near(a: Resources, b: Resources, tolerance: f64) -> bool {
    ResourceKind::ALL.into_iter().all(|k| (a.get(k) - b.get(k)).abs() <= tolerance)
}

/// A homeworld that makes its natural production and nothing else.
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

    muster(&mut e.engine, fid, &[(ShipClass::HeavyFighter, 8), (ShipClass::SmallCargo, 1)], home);
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
    assert_eq!((unseen.ring, unseen.cost, unseen.build_ticks, unseen.platform_power), (10, COST, 5400, PLATFORM[0]));
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
    assert_eq!((quote.cost, quote.build_ticks, quote.platform_power), (COST, 5400, PLATFORM[0]));
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
            "fleet {fid} founded your outpost at {target} for 3000 metal, 1500 crystal and 600 deuterium: \
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
    assert_eq!((post.platform_power, post.production), (PLATFORM[0], found.production));
    assert_eq!(e.fleet().outpost_fuel, Some(0.0));
    let founder = e.fleet();
    assert!(founder.fuel < founder.fuel_max && post.store.deuterium == 0.0, "the convoy's tank is not full and the new store has nothing for it");
    e.run(3);
    assert!(
        alerts_since(&e, mark).iter().all(|a| !matches!(a.level, AlertLevel::Warning)),
        "a new outpost does not greet its founder with a warning that its store is empty: {:?}",
        said(&e, mark),
    );

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
        refused(&mut e, ground, res(3000.0, 1499.0, 600.0)),
        (
            ErrorCode::NoResources,
            format!(
                "the hold of fleet {fid} lacks 1 crystal of the 3000 metal, 1500 crystal and 600 deuterium an outpost costs; \
                 it is built from what the founding fleet carries, loaded at home"
            ),
        ),
    );
    assert_eq!(refused(&mut e, ground, res(0.0, 1500.0, 0.0)).0, ErrorCode::NoResources);

    let den = in_outer_ring(&e.engine, |at, t| {
        at.dist_from_origin() >= 16
            && t.npc_template.as_ref().is_some_and(|npc| npc.behavior != NpcBehaviorType::Passive)
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
    let beside = e.engine.world_gen.connected_neighbors(den).slice()[0];
    let den_pack = e.engine.npc_fleets[&template_npc_id(den)].label.clone();
    edit(&mut e.engine, |s| {
        s.combats.clear();
        s.fleets.get_mut(&fid).unwrap().state = FleetStatus::Idle;
        let pack = s.npc_fleets.get_mut(&template_npc_id(den)).unwrap();
        (pack.in_combat, pack.location, pack.strayed) = (false, beside, 1);
    });
    assert_eq!(
        e.refusal(Command::FoundOutpost { fleet_id: fid }),
        (
            ErrorCode::InvalidTarget,
            format!(
                "the {den_pack} that holds {den} is away at {beside} and will be back; \
                 an outpost is founded on a sector its founder has won"
            ),
        ),
        "a lair whose group has gone after a fleet is empty and is not won",
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
    muster(&mut e.engine, rival_fleet, &[(ShipClass::SmallCargo, 3)], theirs);
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
    let ring_factor = maths::powi(1.1f64, 9);
    let per_tick = |d: Density| 6.0 / 3600.0 * f64::from(d.harvest_multiplier()) * ring_factor * 10.0;
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
        ("persistent", 1.0f64, 54_000u64, res(9000.0, 4500.0, 2250.0)),
        ("season", 10.0, 5400, res(28_460.499, 14_230.249_5, 7115.1247)),
        ("blitz", 600.0, 90, res(220_454.077, 110_227.038, 55_113.519)),
    ] {
        let mut e = Empire::join(world(pace), "Miner");
        let at = site(&e.engine, 12, |t| t.metal_density != Density::None && grade_of(t) >= 3.0);
        let template = e.engine.world_gen.generate_sector(at);
        haul(&mut e, at, COST);
        let quote = survey(&mut e, None);
        let quoted = quote.site.as_ref().expect("the fleet stands in the sector").production;
        let per_tick = |d: Density| 6.0 / 3600.0 * f64::from(d.harvest_multiplier()) * maths::powi(1.1f64, 9) * p;
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
        assert!(near(open.store_cap, cap, 0.05) && open.protected == open.store, "{pace}: a store that pays no repair yet is all floor");

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
    assert!(out.len() + back.len() <= 12, "two Small Cargos fly 12 hops on a tank: {} out", out.len());

    found(&mut e, at);
    e.run(build_ticks(&e, 1));
    crew(&mut e, fid, &[(ShipClass::SmallCargo, 2)], at);
    stow(&mut e, fid, Resources::default());
    e.set_stockpile(res(0.0, 0.0, 400.0));
    e.run(600);

    let rate = outpost(&e).production;
    let natural = e.state().homeworld.production;
    assert!(natural.metal > 0.0 && natural.deuterium == 0.0, "a home with no mines makes its natural production: {natural:?}");
    let owned = |e: &Empire| {
        let holds = e.engine.fleets.values().filter(|f| f.owner_id == pid).fold(Resources::default(), |sum, f| sum.add(f.cargo));
        e.stockpile().add(e.engine.outposts[&pid].store).add(holds)
    };
    let jump = e.fleet().jump_fuel;
    assert_eq!((jump, e.fleet().fuel, e.fleet().fuel_max), (20.0, 240.0, 240.0), "10 fuel a jump each, and a tank of twelve jumps");
    let (before, started, home_stock) = (owned(&e), e.engine.current_tick(), e.stockpile().metal);

    e.order(Command::Load { fleet_id: fid, resource: Metal, amount: None }).expect("load the yield");
    let carried = e.fleet().cargo.metal;
    assert!(carried > 0.0 && !e.fleet().laden, "the hold is the fleet's own business at the outpost");
    sweep(&mut e.engine, &back);
    e.travel(home);
    e.run(2);
    let at_home = home_stock + natural.metal * (e.engine.current_tick() - started) as f64;
    assert!((e.stockpile().metal - carried - at_home).abs() < 0.01, "the home dock unloads the yield: {:?}", e.stockpile());
    assert_eq!(e.fleet().fuel, 240.0, "and fills the tank from the stockpile");

    e.order(Command::Load { fleet_id: fid, resource: Deuterium, amount: Some(100.0) }).expect("load fuel for the outpost");
    sweep(&mut e.engine, &out);
    e.travel(at);
    e.order(Command::Unload { fleet_id: fid }).expect("unload at the outpost");
    e.run(2);
    assert_eq!(e.fleet().fuel, 240.0, "the outpost fills the tank from what the convoy brought");

    let burned = jump * (out.len() + back.len()) as f64;
    let made = rate.add(natural).scale((e.engine.current_tick() - started) as f64);
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
    crew(&mut e, fid, &[(ShipClass::SmallCargo, 1), (ShipClass::LightFighter, 1)], at);
    stow(&mut e, fid, res(40.0, 0.0, 0.0));

    let mark = e.mark();
    e.travel(next);
    sweep(&mut e.engine, &[next]);
    e.travel(at);
    let fleet = e.fleet();
    assert_eq!((fleet.fuel, fleet.fuel_max, fleet.jump_fuel), (260.0, 320.0, 30.0), "two jumps burned, none refilled");
    let dry: Vec<AlertEvent> = alerts_since(&e, mark).into_iter().filter(|a| a.message.contains("no deuterium left")).collect();
    assert_eq!(dry.len(), 1, "{dry:?}");
    assert!(matches!(dry[0].level, AlertLevel::Warning) && dry[0].fleet_id == Some(fid));
    assert_eq!(
        dry[0].message,
        format!(
            "your outpost at {at} has no deuterium left for the tank of fleet {fid}; unload some into its store: \
             fleet {fid} stands at 260 of 320 fuel"
        ),
    );
    e.run(300);
    assert_eq!(alerts_since(&e, mark).iter().filter(|a| a.message.contains("no deuterium left")).count(), 1, "once a stay");
    assert_eq!(e.fleet().fuel, 260.0, "nothing fills a tank from an empty store, and a fleet at a dock is not stranded");

    edit(&mut e.engine, |s| s.outposts.get_mut(&pid).unwrap().store.deuterium = 1.0);
    e.run(1);
    assert_eq!((e.fleet().fuel, outpost(&e).store.deuterium), (272.5, 0.0), "one deuterium buys 12.5 fuel");
    assert_eq!(alerts_since(&e, mark).iter().filter(|a| a.message.contains("no deuterium left")).count(), 1);

    edit(&mut e.engine, |s| s.outposts.get_mut(&pid).unwrap().store.deuterium = 50.0);
    e.run(1);
    let filled = e.fleet();
    assert_eq!(filled.fuel, 320.0);
    assert!((outpost(&e).store.deuterium - (50.0 - 47.5 * FUEL_DEUT_PER_UNIT)).abs() < 1e-4, "the home price: 0.08 a unit");
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
    assert_eq!((next.level, next.cost, next.ticks, next.platform_power), (2, res(7500.0, 3750.0, 1500.0), 13_500, PLATFORM[1]));
    assert!(near(next.store_cap, one.store_cap.scale(2.5), 0.05) && near(next.production, one.production.scale(2.0), 1e-6));

    deliver(&mut e, at, res(6000.0, 3750.0, 300.0));
    let held = outpost(&e).store;
    let before = e.engine.snapshot().hash();
    let foretold = outpost(&e).upgrade_refusal.expect("the state says beforehand why the order would be refused");
    let (code, why) = e.refusal(Command::UpgradeOutpost);
    assert_eq!(code, ErrorCode::InsufficientResources);
    assert_eq!(
        why,
        format!(
            "outpost level 2 costs 7500 metal, 3750 crystal and 1500 deuterium; the store at {at} is short by {} metal \
             and 1200 deuterium. A level is paid in full from the outpost's own store when it is ordered",
            (7500.0 - held.metal).ceil(),
        ),
    );
    assert_eq!(foretold, why, "and in the sentence the refusal carries");
    assert_eq!(e.engine.snapshot().hash(), before, "a refused upgrade changes nothing");

    deliver(&mut e, at, res(1800.0, 0.0, 1200.0));
    let held = outpost(&e).store;
    assert_eq!(outpost(&e).upgrade_refusal, None, "the store covers it, so nothing stands in the way");
    let (mark, ordered) = (e.mark(), e.engine.current_tick());
    e.order(Command::UpgradeOutpost).expect("the store covers level 2");
    let building = outpost(&e);
    assert!(near(building.store, held.sub(next.cost), 1e-3), "{:?}", building.store);
    let work = building.work.expect("level 2 is being built");
    assert_eq!((work.kind, work.level, work.start_tick, work.end_tick), (OutpostWork::Build, 2, ordered, ordered + 13_500));
    assert_eq!((building.level, building.in_service, building.platform_power), (1, true, PLATFORM[0]));
    assert_eq!((building.production, building.store_cap), (one.production, one.store_cap), "it works at level 1 meanwhile");
    assert_eq!(building.next_level.map(|n| n.level), Some(3), "what could be ordered after this one");
    let told = said(&e, mark);
    assert!(told[0].starts_with(&format!("your outpost at {at} began level 2, paid 7500 metal, 3750 crystal and 1500 deuterium from its store")), "{told:?}");

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
    assert_eq!((two.level, two.work, two.platform_power, completed(&e, mark)), (2, None, PLATFORM[1], vec![2]));
    assert_eq!((two.store_cap, two.production), (next.store_cap, next.production));
    assert!(scaling::par_value(&two.store) < scaling::outpost_repair_price(2), "{:?}", two.store);
    assert_eq!(two.protected, two.store, "the floor is the repair of the level it stands at: all of a store that holds less");

    let last = two.next_level.expect("level 3 is on offer");
    assert_eq!((last.level, last.cost, last.ticks, last.platform_power), (3, res(18_750.0, 9375.0, 3750.0), 33_750, PLATFORM[2]));
    deliver(&mut e, at, last.cost);
    e.order(Command::UpgradeOutpost).expect("the store covers level 3");
    e.run(33_750);
    let three = outpost(&e);
    assert_eq!((three.level, three.platform_power, three.next_level.clone()), (3, PLATFORM[2], None));
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
    muster(&mut e.engine, fid, &[(ShipClass::SmallCargo, 2)], at);
    stow(&mut e, fid, COST);
    let before = e.score_row();

    e.order(Command::FoundOutpost { fleet_id: fid }).expect("found");
    assert_eq!(e.score_row().core, before.core, "an outpost under construction scores nothing");
    e.run(build_ticks(&e, 1));
    let one = e.score_row();
    assert!((one.core - before.core - 6.45).abs() < 1e-3, "level 1 is 3000 + 1.5 x 1500 + 2 x 600 a thousand: {}", one.core - before.core);

    stow(&mut e, fid, res(15_000.0, 9000.0, 3000.0));
    e.order(Command::Unload { fleet_id: fid }).expect("unload at the outpost");
    e.run(500);
    e.order(Command::Load { fleet_id: e.fid, resource: Crystal, amount: Some(100.0) }).expect("load from the store");
    assert_eq!(e.score_row().core, one.core, "the store, the yield, load and unload score nothing");

    e.order(Command::UpgradeOutpost).expect("level 2");
    assert_eq!(e.score_row().core, one.core, "a level scores when it is finished, not when it is paid");
    e.run(build_ticks(&e, 2));
    let two = e.score_row();
    assert!((two.core - one.core - 16.125).abs() < 1e-3, "level 2 adds 16.125: {}", two.core - one.core);
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
fn the_outposts_dock_mends_ships_from_its_store_at_a_tenth_of_their_cost_and_does_not_refit_them() {
    use ShipClass::Cruiser;
    let (mut e, at) = settle("season", 9, any_ore);
    let (pid, fid) = (e.pid, e.fid);
    let stock = |e: &mut Empire, store: Resources| edit(&mut e.engine, |s| s.outposts.get_mut(&pid).unwrap().store = store);
    let wound = |e: &mut Empire, damaged: &[u32]| {
        edit(&mut e.engine, |s| s.fleets.get_mut(&fid).unwrap().ships = squadron(Cruiser, 1, damaged));
    };
    let mended = |e: &Empire, mark: usize| -> Vec<String> { said(e, mark).into_iter().filter(|m| m.contains("was mended")).collect() };

    // A Cruiser costs 20,000 metal, 7,000 crystal and 2,000 deuterium:
    // 34,500 at par, so a whole hull of 2,700 is 3,450 here. One Cruiser has
    // lost nine tenths of its hull and one a half.
    wound(&mut e, &[270, 1350]);
    stock(&mut e, Resources::default());
    let mark = e.mark();
    e.run(1);
    assert_eq!(mended(&e, mark), Vec::<String>::new(), "an empty store mends nothing");
    let quote = e.fleet().repair.expect("the fleet is damaged");
    let dock = quote.outpost.clone().expect("the empire has an outpost");
    assert_eq!((quote.ships, quote.hull, dock.price), (2, 2430 + 1350, 3105.0 + 1725.0));
    assert_eq!((dock.ships, dock.hull, dock.refusal), (0, 0, None), "and the state says so: {:?}", dock.payment);
    assert_eq!((quote.home.price, quote.home.ships, quote.home.hull), (0.0, 2, 3780), "the home dock would mend both for nothing");

    stock(&mut e, res(3200.0, 0.0, 0.0));
    let mark = e.mark();
    e.run(1);
    assert_eq!(
        mended(&e, mark),
        [format!(
            "fleet {fid} was mended at your outpost: 1 ship(s) made whole, 2430 hull restored, for 3105 metal; \
             1 more wait for the dock's stock to cover them"
        )],
        "the most damaged first, and no ship in part",
    );
    assert_eq!(e.engine.fleets[&fid].ships, squadron(Cruiser, 2, &[1350]));
    let left = outpost(&e).store;
    assert!((left.metal - 95.0).abs() < 1.0 && left.crystal < 1.0, "the store paid 3,105 metal: {left:?}");

    stock(&mut e, res(95.0, 2000.0, 0.0));
    let mark = e.mark();
    e.run(1);
    let told = mended(&e, mark);
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(told[0].contains("1 ship(s) made whole, 1350 hull restored, for ") && told[0].ends_with(" metal and 1087 crystal"), "{}", told[0]);
    let left = outpost(&e).store;
    assert!(
        left.metal < 1.0 && (left.crystal - (2000.0 - (1725.0 - 95.0) / 1.5)).abs() < 1.0,
        "metal first, then crystal at its weight of 1.5: {left:?}",
    );
    assert_eq!((e.fleet().hull, e.fleet().repair), (e.fleet().hull_max, None));

    edit(&mut e.engine, |s| s.players.get_mut(&pid).unwrap().research.weapons_technology = 3);
    e.run(2);
    let stood = e.fleet();
    assert_eq!((stood.refit.weapons, stood.refit_due, stood.ships[0].unit_attack), (0, true, 400), "the outpost has no yard: it does not refit");

    wound(&mut e, &[270]);
    stock(&mut e, res(50_000.0, 0.0, 0.0));
    e.engine.handle_policy_update(pid, fid, PolicyPreset::Prospect, None).expect("orders");
    let under_orders = e.fleet().repair.unwrap().outpost.unwrap();
    assert_eq!(
        (under_orders.ships, under_orders.refusal.as_deref()),
        (0, Some("a fleet under standing orders does not use the outpost")),
    );
    e.engine.handle_policy_update(pid, fid, PolicyPreset::Manual, None).expect("orders lifted");

    edit(&mut e.engine, |s| s.outposts.get_mut(&pid).unwrap().breached = true);
    let mark = e.mark();
    e.run(1);
    assert_eq!(mended(&e, mark), Vec::<String>::new(), "a breached outpost mends nothing");
    let breached = e.fleet().repair.unwrap().outpost.unwrap();
    assert_eq!(
        (breached.ships, breached.price, breached.refusal),
        (0, 3105.0, Some(format!("the outpost at {at} is breached and mends nothing until it is repaired"))),
    );
}

#[test]
fn a_fleet_at_the_outpost_is_docked_and_needs_a_slot_to_leave() {
    let (mut e, at) = settle("season", 9, |_| true);
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    muster(&mut e.engine, fid, &[(ShipClass::EspionageProbe, 5)], home);
    stow(&mut e, fid, Resources::default());
    let mut wings = vec![fid];
    for _ in 1..5 {
        let one = counts(&[(ShipClass::EspionageProbe, 1)]);
        e.order(Command::Split { fleet_id: fid, ships: one, damaged_first: false }).expect("split at home");
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
    muster(&mut e.engine, rival_fleet, &[(ShipClass::SmallCargo, 1)], next);
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

fn rounds_since(e: &Empire, mark: usize) -> Vec<BattleRoundEvent> {
    e.since(mark, |k| match k {
        EventKind::BattleRound(round) => Some(round.clone()),
        _ => None,
    })
}

fn platform_of(level: u8) -> Vec<UnitCount> {
    scaling::outpost_platform(level).iter().map(|&(kind, count)| UnitCount { unit: ShipyardItem::Defence(kind), count }).collect()
}

#[test]
fn a_platform_that_outguns_its_ring_destroys_the_raid_and_stands_whole_again() {
    let (mut e, at) = settle("blitz", 10, |t| t.metal_density != Density::None);
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    deliver(&mut e, at, res(7500.0, 3750.0, 1500.0));
    e.order(Command::UpgradeOutpost).expect("level 2");
    muster(&mut e.engine, fid, &[(ShipClass::LightFighter, 1)], home);
    e.run(build_ticks(&e, 2));
    let post = outpost(&e);
    assert_eq!((post.level, post.platform_power, post.defence_power), (2, PLATFORM[1], PLATFORM[1]), "nobody is home but the platform");
    assert_eq!(post.platform, platform_of(2), "eight Rocket Launchers and four Light Lasers");
    let expected = post.raid.clone().expect("a finished outpost has a raid to expect");
    assert!(expected.arrival_tick.is_none() && expected.ships == counts(&[(ShipClass::Cruiser, 1)]), "{expected:?}");
    assert_eq!(expected.odds.label, RaidLabel::Repelled, "the platform of level 2 destroys the raid of ring 10 every time it is played");
    let estimate = e.state().homeworld.next_raid_estimate_power;

    let opened = e.engine.outposts[&pid].raid.first_seen_tick;
    let min_age = e.engine.pace().attention_ticks(86_400.0);
    let before = post.store;
    let mark = e.mark();
    let ((told, forecast), (resolved, outcome)) = next_raid(&mut e, 20_000);

    assert!(told >= opened + min_age, "no raid before the outpost has stood a day of raid clock: {told} against {opened} + {min_age}");
    assert_eq!((forecast.player_id, forecast.outpost), (pid, Some(at)));
    assert_eq!(forecast.arrival_tick, told + e.engine.pace().raid_warning_ticks(), "the homeworld's warning");
    let base = 2.0 * scaling::npc_group_power(10);
    assert!((base * 0.80..=base * 1.15).contains(&forecast.est_power), "{} against a base of {base}", forecast.est_power);
    assert_eq!((forecast.threat.as_str(), &forecast.ships), ("light", &expected.ships), "one Cruiser of its ring, and the fight is the platform's");
    assert_eq!(forecast.odds.map(|odds| odds.label), Some(RaidLabel::Repelled));

    let rounds = rounds_since(&e, mark);
    assert!(!rounds.is_empty() && rounds.len() as u32 == outcome.rounds && outcome.rounds <= RAID_ROUNDS);
    assert_eq!(resolved, forecast.arrival_tick + u64::from(outcome.rounds), "it arrives on its tick and fights a round a tick after it");
    let first = &rounds[0];
    assert!(first.raid && first.mine && first.sector == at);
    let battery = &first.fleets.groups[0];
    assert!(first.fleets.groups.len() == 1 && battery.battery && battery.label == "outpost platform" && battery.fleet_id == 0);
    let stood: Vec<UnitCount> = battery.units.iter().map(|u| UnitCount { unit: u.unit, count: u.count + u.lost }).collect();
    assert_eq!(stood, platform_of(2), "the platform is the defences of its level, and they fire");
    assert!(first.fleets.shots == 12 && first.hostiles.groups[0].label == "raid of 1 Cruiser");

    assert_eq!((outcome.outpost, outcome.defended, outcome.breached), (Some(at), true, false));
    assert_eq!(outcome.outcome, Some(BattleOutcome::HostilesDestroyed));
    assert_eq!((outcome.raid_power, outcome.defense_power), (forecast.est_power, PLATFORM[1]));
    assert_eq!((outcome.resources_lost, outcome.ships_lost), (Resources::default(), 0));
    assert!(outcome.rebuilt.is_empty(), "nothing is rebuilt at an outpost: its platform is not its owner's to lose");
    assert_eq!((outcome.salvage_dropped, outcome.salvage_despawn_tick), (None, None), "a raid on an outpost leaves no wreckage");

    let after = outpost(&e);
    assert!(after.store.metal > before.metal && after.store.crystal >= before.crystal, "the store only grew: {:?}", after.store);
    assert!(after.in_service && !after.breached && after.raid_arrival_tick.is_none());
    assert_eq!((after.platform, after.platform_power), (platform_of(2), PLATFORM[1]), "whatever fell, the platform is whole again");
    assert_eq!(e.state().homeworld.next_raid_estimate_power, estimate);
    assert!(chart(&e.engine, pid)[&at.to_key()].salvage.is_none());

    let mark = e.mark();
    let (_, (_, again)) = next_raid(&mut e, 20_000);
    let first = &rounds_since(&e, mark)[0];
    let stood: Vec<UnitCount> = first.fleets.groups[0].units.iter().map(|u| UnitCount { unit: u.unit, count: u.count + u.lost }).collect();
    assert_eq!(stood, platform_of(2), "the next raid meets every defence of the level, whatever the last one destroyed");
    assert!(again.defended && again.defense_power == PLATFORM[1]);
}

#[test]
fn a_lost_raid_destroys_what_stood_there_takes_a_quarter_above_the_floor_and_breaches_the_outpost_until_it_repairs_itself() {
    let (mut e, at) = settle("blitz", 16, |t| t.metal_density != Density::None && t.deut_density == Density::None);
    let (pid, fid) = (e.pid, e.fid);
    deliver(&mut e, at, res(6000.0, 1800.0, 900.0));
    muster(&mut e.engine, fid, &[(ShipClass::LightFighter, 2)], at);
    stow(&mut e, fid, res(900.0, 0.0, 0.0));
    let hold = e.fleet().cargo_capacity;
    assert!((hold - 2449.49).abs() < 0.1, "two Light Fighters hold 100 x sqrt(600): {hold}");
    let post = outpost(&e);
    let fighter = scaling::ship_power(ShipClass::LightFighter);
    assert_eq!(fighter, 91.0, "50 attack and a tenth of 400 hull and 10 shield");
    assert_eq!((post.platform_power, post.defence_power), (PLATFORM[0], PLATFORM[0] + 2.0 * fighter));
    assert_eq!(post.platform, platform_of(1), "four Rocket Launchers");
    assert_eq!(
        post.raid.as_ref().map(|raid| (raid.ships.clone(), raid.odds.label)),
        Some((counts(&[(ShipClass::Battlecruiser, 1)]), RaidLabel::Lost)),
        "four Rocket Launchers and two Light Fighters do not stand six rounds of a Battlecruiser",
    );
    let floor = res(REPAIR, 0.0, 0.0);
    assert_eq!(post.protected, floor, "750 metal, 375 crystal and 150 deuterium at par, in metal while the store has it");

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
    assert!(outcomes(&e, mark).is_empty() && rounds_since(&e, mark).is_empty(), "it stands there on the tick it arrives");
    let battle = e.state().battles.into_iter().next().expect("the raid's fight is in the state");
    let raid = battle.raid.expect("and says it is a raid on the outpost");
    assert!(raid.outpost && raid.mine && battle.sector == at && raid.odds.label == RaidLabel::Lost);
    let fought = outpost(&e).raid.expect("the outpost's forecast is the raid being fought");
    assert_eq!((fought.arrival_tick, fought.rounds_left), (None, Some(raid.rounds_left)));
    assert!(fought.ships == raid.ships && fought.odds == raid.odds);
    assert_eq!(e.refusal(Command::AbandonOutpost), (ErrorCode::OnCooldown, "your outpost is under raid; it cannot be abandoned until the fight is over".to_string()));
    e.run_until(10, |e| !outcomes(e, mark).is_empty());
    let (resolved, outcome) = outcomes(&e, mark).pop().unwrap();
    let rounds = rounds_since(&e, mark);
    assert!((1..=RAID_ROUNDS).contains(&outcome.rounds) && rounds.len() as u32 == outcome.rounds);
    assert_eq!(resolved, forecast.arrival_tick + u64::from(outcome.rounds));
    let sides: Vec<bool> = rounds[0].fleets.groups.iter().map(|group| group.battery).collect();
    assert_eq!(sides, [false, true], "the fleet standing there and the platform fight side by side");

    assert_eq!((outcome.outpost, outcome.defended, outcome.breached), (Some(at), false, true));
    assert_eq!(outcome.outcome, Some(BattleOutcome::FleetsDestroyed));
    assert_eq!(outcome.ships_lost, 2, "every ship that stood there, not a quarter of them");
    assert_eq!(outcome.lost, [
        UnitCount { unit: ShipyardItem::Ship(ShipClass::LightFighter), count: 2 },
        UnitCount { unit: ShipyardItem::Defence(DefenceKind::RocketLauncher), count: 4 },
    ]);
    assert!(outcome.rebuilt.is_empty() && outcome.salvage_dropped.is_none());
    assert_eq!(outcome.raid_power, forecast.est_power);
    assert!(e.engine.fleets.get(&fid).is_none_or(|fleet| fleet.ships.is_empty()));
    let carried = said(&e, mark).into_iter().find(|line| line.contains("lost carried")).expect("the hold of the lost ships is told of");
    assert!(carried.ends_with("it went to the outpost's store"), "{carried}");

    let pile = before.add(rate.scale(f64::from(outcome.rounds)));
    let taken = res((pile.metal - REPAIR) * 0.25, pile.crystal * 0.25, pile.deuterium * 0.25);
    assert!(near(outcome.resources_lost, taken, 0.01), "a quarter above the floor: {:?} against {taken:?}", outcome.resources_lost);
    assert_eq!(outcome.protected_kept, floor);

    let repairing = outpost(&e);
    assert!(repairing.breached && !repairing.in_service);
    assert_eq!((repairing.production, repairing.platform_power, repairing.defence_power), (Resources::default(), 0.0, 0.0));
    assert!(repairing.platform.is_empty() && repairing.raid_arrival_tick.is_none());
    let brief = chart(&e.engine, pid)[&at.to_key()].outpost.clone().unwrap();
    assert!(brief.breached && brief.mine);
    assert_eq!(e.refusal(Command::UpgradeOutpost).0, ErrorCode::InvalidCommand, "a breached outpost repairs before it grows");
    let work = repairing.work.expect("the repair starts by itself, on the tick of the breach when the store covers it");
    let ticks = scaling::outpost_repair_ticks(1, &e.engine.pace());
    assert_eq!((work.kind, work.level, work.start_tick, work.end_tick), (OutpostWork::Repair, 1, resolved, resolved + ticks));
    assert_eq!(ticks, 23, "a quarter of level 1's 90 blitz ticks");
    assert!(near(repairing.store, pile.sub(taken).sub(floor), 0.01), "what the raid left, less the repair: {:?}", repairing.store);
    assert!(repairing.repair_cost.is_none() && repairing.repair_waiting_for.is_none());
    assert!(said(&e, mark).contains(&format!(
        "your outpost at {at} began its repair, paid 1613 metal from its store: it is whole at tick {}, in 23 ticks",
        work.end_tick,
    )));
    let mark = e.mark();
    e.run(ticks - 1);
    assert!(outpost(&e).breached && outpost(&e).store == repairing.store, "the extractor stands still until the repair is done");
    e.run(1);
    let whole = outpost(&e);
    assert!(!whole.breached && whole.in_service && whole.work.is_none());
    assert_eq!((whole.platform_power, whole.production, &whole.platform), (PLATFORM[0], rate, &platform_of(1)));
    assert!(whole.store.metal > repairing.store.metal, "it extracts again from the tick it is whole");
    assert_eq!(said(&e, mark).last().unwrap(), &format!("your outpost at {at} is repaired: its extractor, its tanks and its platform work again"));

    let grace = e.engine.pace().attention_ticks(172_800.0);
    let mark = e.mark();
    e.run_until(40_000, |e| !forecasts(e, mark).is_empty());
    let (again, _) = forecasts(&e, mark).pop().unwrap();
    assert!(again >= resolved + grace, "no raid is announced inside the grace: {again} against {resolved} + {grace}");
}

#[test]
fn ships_standing_at_the_outpost_fight_its_raid_beside_the_platform_and_win_it() {
    let (mut e, at) = settle("blitz", 12, |t| t.metal_density != Density::None);
    let (pid, fid) = (e.pid, e.fid);
    deliver(&mut e, at, res(6000.0, 3000.0, 1500.0));
    muster(&mut e.engine, fid, &[(ShipClass::Cruiser, 6)], at);
    stow(&mut e, fid, Resources::default());
    let post = outpost(&e);
    let expected = post.raid.clone().expect("a raid to expect");
    assert_eq!(expected.odds.label, RaidLabel::Repelled, "six Cruisers and the platform destroy the raid of ring 12: {expected:?}");
    assert!(post.ships_power > 0.0 && post.defence_power == post.platform_power + post.ships_power);

    send_outpost_raid(&mut e.engine, pid, &[(ShipClass::Cruiser, 2)], 1.0, 2);
    let announced = outpost(&e).raid.expect("the raid announced");
    assert_eq!((announced.arrival_tick, &announced.ships), (Some(e.engine.current_tick() + 2), &counts(&[(ShipClass::Cruiser, 2)])));
    let mark = e.mark();
    e.run_until(20, |e| !outcomes(e, mark).is_empty());
    let (_, outcome) = outcomes(&e, mark).pop().unwrap();
    let rounds = rounds_since(&e, mark);
    assert!(!rounds.is_empty() && rounds.iter().all(|round| round.raid && round.sector == at));
    let first = &rounds[0].fleets.groups;
    assert_eq!(first.iter().map(|group| (group.fleet_id, group.battery)).collect::<Vec<_>>(), [(fid, false), (0, true)]);
    assert!(rounds[0].fleets.volleys.iter().any(|v| v.from == ShipyardItem::Ship(ShipClass::Cruiser)));
    assert!(rounds[0].fleets.volleys.iter().any(|v| v.from == ShipyardItem::Defence(DefenceKind::RocketLauncher)));

    assert!(outcome.defended && !outcome.breached);
    assert_eq!(outcome.outcome, Some(BattleOutcome::HostilesDestroyed));
    assert_eq!(outcome.raiders_lost, counts(&[(ShipClass::Cruiser, 2)]));
    assert_eq!((outcome.salvage_dropped, outcome.resources_lost), (None, Resources::default()), "no wreckage at an outpost, and nothing taken");
    assert_eq!(e.fleet().ship_count, 6 - outcome.ships_lost);
    let after = outpost(&e);
    assert!(after.in_service && after.platform == platform_of(1), "the platform is whole again whatever it lost");
    assert!(e.state().battles.is_empty());
}

#[test]
fn a_fleet_recalled_from_a_breached_outpost_under_raid_leaves_it_to_the_raid() {
    let (mut e, at) = settle("blitz", 16, |t| t.metal_density != Density::None);
    let (pid, fid) = (e.pid, e.fid);
    deliver(&mut e, at, res(900.0, 0.0, 0.0));
    muster(&mut e.engine, fid, &[(ShipClass::SmallCargo, 6)], at);
    stow(&mut e, fid, Resources::default());
    edit(&mut e.engine, |s| s.outposts.get_mut(&pid).unwrap().breached = true);
    send_outpost_raid(&mut e.engine, pid, &[(ShipClass::Battlecruiser, 2)], 1.0, 1);
    let mark = e.mark();
    e.run(1);
    let battle = e.state().battles.into_iter().next().expect("the ships there are all that stands in the raid's way");
    assert!(battle.raid.is_some_and(|raid| raid.outpost && raid.odds.label == RaidLabel::Lost));
    assert!(rounds_since(&e, mark).is_empty());

    e.order(Command::Recall { fleet_id: fid }).expect("the emergency jump home is open from an outpost under raid");
    let (_, outcome) = outcomes(&e, mark).pop().expect("with the last defender gone the raid is settled at once");
    assert!(!outcome.defended && outcome.breached);
    assert_eq!((outcome.outcome, outcome.rounds, outcome.ships_lost), (Some(BattleOutcome::Withdrawn), 0, 0));
    assert_eq!(e.fleet().location, e.engine.players[&pid].homeworld);
    assert!(e.state().battles.is_empty());
}

#[test]
fn a_breached_outpost_whose_store_cannot_pay_waits_for_a_delivery_and_says_so_once() {
    let (mut e, at) = settle("blitz", 18, |t| t.metal_density != Density::None && t.deut_density == Density::None);
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    let mark = e.mark();
    e.run_until(20_000, |e| !forecasts(e, mark).is_empty());
    for kind in ResourceKind::ALL {
        let _ = e.order(Command::Load { fleet_id: fid, resource: kind, amount: None });
    }
    assert!(e.fleet().cargo.metal > 750.0, "the owner carries the store off before the raid arrives");
    muster(&mut e.engine, fid, &[(ShipClass::LightFighter, 1)], home);
    stow(&mut e, fid, Resources::default());
    e.run_until(100, |e| !outcomes(e, mark).is_empty());
    let (resolved, outcome) = outcomes(&e, mark).pop().unwrap();
    assert_eq!((outcome.defended, outcome.breached, outcome.ships_lost), (false, true, 0), "nobody was there to lose");
    assert_eq!(outcome.structures_lost, 4, "the platform fought alone and fell");
    assert_eq!(outcome.resources_lost, Resources::default(), "nothing lies above a floor the store had not reached");

    e.run(400);
    let waiting = outpost(&e);
    let held = scaling::par_value(&waiting.store);
    let short = REPAIR - held;
    assert!(waiting.breached && waiting.work.is_none() && short > 0.0, "the store holds {held} at par");
    assert_eq!(waiting.protected, waiting.store, "all of a store that cannot pay is floor");
    let lacks = waiting.repair_waiting_for.expect("the state says what the repair waits for");
    assert!((lacks.metal - short).abs() < 1e-3 && lacks.crystal == 0.0 && lacks.deuterium == 0.0, "the shortfall, in metal: {lacks:?}");
    assert_eq!(waiting.repair_cost, Some(waiting.store.add(lacks)), "the store and the metal it lacks");
    let warned: Vec<AlertEvent> = alerts_since(&e, mark).into_iter().filter(|alert| alert.message.contains("breached")).collect();
    assert_eq!(warned.len(), 1, "{warned:?}");
    assert!(matches!(warned[0].level, AlertLevel::Warning));
    assert_eq!(
        warned[0].message,
        format!(
            "your outpost at {at} is breached and cannot repair itself: the repair costs 1613 metal, or crystal at 1.5 and \
             deuterium at 2 a unit in its place, and its store is short by {} metal. It makes nothing, fills no tank and \
             has no platform until a fleet unloads the rest",
            lacks.metal.ceil(),
        ),
    );
    assert_eq!(e.engine.outposts[&pid].store, waiting.store, "a breached outpost extracts nothing, so it cannot save up the repair");
    assert_eq!(e.engine.current_tick(), resolved + 400);
    assert_eq!(
        e.refusal(Command::UpgradeOutpost).1,
        format!(
            "your outpost at {at} is breached; it repairs itself first, once its store covers the repair: 1613 metal, or \
             crystal at 1.5 and deuterium at 2 a unit in its place"
        ),
    );

    deliver(&mut e, at, res(0.0, (short / 1.5).ceil(), 40.0));
    let stocked = outpost(&e).store;
    e.run(1);
    let repairing = outpost(&e);
    assert_eq!(repairing.work.map(|w| w.kind), Some(OutpostWork::Repair), "the repair starts on the delivery, whatever it brought");
    let paid = stocked.sub(repairing.store);
    assert!((scaling::par_value(&paid) - REPAIR).abs() < 1e-2, "1612.5 at par: {paid:?}");
    assert!(paid.metal == stocked.metal && paid.crystal > 0.0 && paid.deuterium == 0.0, "metal first, then crystal, and no deuterium: {paid:?}");
    e.run(scaling::outpost_repair_ticks(1, &e.engine.pace()));
    assert!(outpost(&e).in_service);
}

#[test]
fn thirty_days_of_raids_never_destroy_an_undefended_outpost() {
    let (mut e, at) = settle("season", 18, |t| t.metal_density != Density::None && grade_of(t) >= 2.0);
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    deliver(&mut e, at, res(6000.0, 9000.0, 4500.0));
    muster(&mut e.engine, fid, &[(ShipClass::LightFighter, 1)], home);
    stow(&mut e, fid, Resources::default());
    let estimate = e.state().homeworld.next_raid_estimate_power;
    let core = e.score_row().core;

    let mark = e.mark();
    let (mut breached, mut lowest) = (0u64, f64::MAX);
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
fn a_site_that_lacks_an_ore_pays_every_repair_in_what_it_makes() {
    let (mut e, at) = settle("season", 18, |t| t.deut_density == Density::None && grade_of(t) >= 4.0);
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    muster(&mut e.engine, fid, &[(ShipClass::LightFighter, 1)], home);
    stow(&mut e, fid, Resources::default());
    assert_eq!(outpost(&e).production.deuterium, 0.0, "the site makes no deuterium, and nobody brings any");

    let mark = e.mark();
    let mut waited = 0u64;
    for _ in 0..30 * 24 {
        e.run(360);
        let post = outpost(&e);
        waited += u64::from(post.repair_waiting_for.is_some());
        assert_eq!(post.store.deuterium, 0.0);
    }
    let raids = outcomes(&e, mark);
    assert!(raids.len() >= 5 && raids.iter().all(|(_, raid)| raid.outpost == Some(at) && raid.breached), "{} raids, all lost", raids.len());
    let told = said(&e, mark);
    let repairs = told.iter().filter(|line| line.contains("is repaired")).count();
    assert!(repairs + 1 >= raids.len(), "it repaired itself after each: {repairs} repairs for {} raids", raids.len());
    assert!(told.iter().all(|line| !line.contains("cannot repair itself")) && waited == 0, "no repair ever waited: {told:?}");
    let paid: Vec<&String> = told.iter().filter(|line| line.contains("began its repair")).collect();
    assert!(paid.len() >= repairs && paid.iter().all(|line| !line.contains("deuterium")), "each was paid in metal and crystal: {paid:?}");
}

#[test]
fn the_balance_report_counts_what_an_outpost_made_and_what_raids_took() {
    let weight = |r: Resources| scaling::resource_weight(&r);
    let mut held = Headless::new(SEED, Pace::parse("blitz").unwrap());
    let mut bare = Headless::new(SEED, Pace::parse("blitz").unwrap());
    let stocked = res(12_000.0, 6000.0, 3000.0);
    let settle = |world: &mut Headless, ring: u16, upgrade: bool| -> u64 {
        let pid = world.add_player(Box::new(IdlePlayer { label: "Keeper".to_string() }), "idle").expect("a seat");
        let fid = world.engine.fleets.values().find(|f| f.owner_id == pid).unwrap().id;
        let home = world.engine.players[&pid].homeworld;
        let at = site(&world.engine, ring, |t| t.metal_density != Density::None);
        muster(&mut world.engine, fid, &[(ShipClass::SmallCargo, 2)], at);
        edit(&mut world.engine, |s| s.fleets.get_mut(&fid).unwrap().cargo = COST.add(stocked));
        for cmd in [Command::FoundOutpost { fleet_id: fid }, Command::Unload { fleet_id: fid }] {
            assert!(world.engine.execute(pid, cmd).error.is_none());
        }
        world.run(90).unwrap();
        if upgrade {
            assert!(world.engine.execute(pid, Command::UpgradeOutpost).error.is_none());
        }
        muster(&mut world.engine, fid, &[(ShipClass::LightFighter, 1)], home);
        world.run(scaling::outpost_build_ticks(2, &world.engine.pace())).unwrap();
        pid
    };
    let keeper = settle(&mut held, 10, true);
    let loser = settle(&mut bare, 18, false);

    let before = (held.summaries().remove(0).field, held.engine.outposts[&keeper].store);
    held.run(6000).unwrap();
    let report = held.summaries().remove(0);
    let post = &held.engine.outposts[&keeper];
    assert!((report.score.outpost - 22.575).abs() < 1e-3 && report.score.core >= report.score.econ + report.score.fleet + 22.5);
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
    let payback = COST.total() / (6.0 * f64::from(site.grade) * scaling::ring_mult(10));
    assert!((f64::from(site.payback_hours) - payback).abs() < 0.05, "{site:?}");
    assert_eq!(site.next_mine_level, 2, "an idle empire's next Metal Mine level");

    let (fid, at) = (held.engine.fleets.values().find(|f| f.owner_id == keeper).unwrap().id, post.sector);
    muster(&mut held.engine, fid, &[(ShipClass::SmallCargo, 2)], at);
    for (resource, amount) in [(Metal, Some(300.0)), (Crystal, None)] {
        assert!(held.engine.execute(keeper, Command::Load { fleet_id: fid, resource, amount }).error.is_none());
    }
    let aboard = held.engine.fleets[&fid].cargo;
    assert!(aboard.metal == 300.0 && aboard.crystal >= 2250.0, "{aboard:?}");
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
    assert!((report.score.outpost - 6.45).abs() < 1e-3);
    let post = &bare.engine.outposts[&loser];
    let repairs = weight(res(750.0, 375.0, 150.0)) * (field.outpost_raids_lost - u32::from(post.breached && post.job.is_none())) as f32;
    assert!((field.outpost_repairs - repairs).abs() < 1e-3, "every repair it paid is counted: {field:?}");
    let kept = weight(post.store) - weight(stocked);
    let owed = field.outpost_yield - field.outpost_raided - repairs;
    assert!((kept - owed).abs() < 0.02 * field.outpost_yield, "yield less raids less repairs is what the store kept: {kept} against {owed}");
    assert_eq!((field.outpost_raids_on_breach, field.outpost_ticks_waiting), (0, 0), "stocked, it was whole again before each raid");
    assert!(field.outpost_potential > field.outpost_yield + field.outpost_spilled, "a breached outpost makes less than its rate: {field:?}");
}

/// An idle empire with an outpost stood for it as `sim-run --outpost` stands one, its fleet
/// mustered there as `garrison` when there is one.
fn stand(world: &mut Headless, ring: u16, level: u8, garrison: &[(ShipClass, u32)]) -> u64 {
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
type Post = (u16, u8, &'static [(ShipClass, u32)]);

fn forecast_of(world: &Headless, pid: u64) -> iac_shared::protocol::RaidForecast {
    let state = iac_sim::views::full_state(&world.engine, pid).expect("the keeper's state");
    state.outpost.and_then(|post| post.raid).expect("a finished outpost has a raid to expect")
}

/// What a platform holds alone, by level and ring: the forecast of each of
/// these outposts against the typical raid of its ring, nobody standing
/// there. Level 1 holds nowhere, level 2 through the middle of the Outer
/// Ring, level 3 to its edge. It is the fight that says so; the platforms
/// are the study's starting values and none was tuned to this table.
#[test]
fn the_forecast_says_what_each_level_of_platform_holds_alone() {
    use RaidLabel::{AtRisk, Held, Lost, Repelled};
    let table: [(u16, [&[RaidLabel]; 3]); 6] = [
        (9, [&[AtRisk, Lost], &[Repelled], &[Repelled]]),
        (12, [&[Lost], &[Repelled], &[Repelled]]),
        (14, [&[Lost], &[Repelled], &[Repelled]]),
        (16, [&[Lost], &[Repelled, Held], &[Repelled]]),
        (18, [&[Lost], &[AtRisk, Lost], &[Repelled]]),
        (20, [&[Lost], &[AtRisk, Lost], &[Repelled, Held]]),
    ];
    for (ring, by_level) in table {
        let mut world = Headless::new(SEED, Pace::parse("season").unwrap());
        for (level, allowed) in (1..=3u8).zip(by_level) {
            let pid = stand(&mut world, ring, level, &[]);
            let raid = forecast_of(&world, pid);
            assert!(
                allowed.contains(&raid.odds.label),
                "level {level} at ring {ring} against {:?} of power {:.0}: {:?}",
                raid.ships, raid.power, raid.odds,
            );
            assert_eq!(raid.ships.len(), 1, "an outpost is raided by the class of its ring");
            assert_eq!(raid.ships[0].ship_class, scaling::npc_class(ring));
            assert_eq!(raid.odds.label <= AtRisk, raid.odds.lost > 0.0);
        }
    }
}

#[test]
fn a_season_week_of_raids_takes_nothing_from_an_outpost_whose_platform_holds_its_ring() {
    let weight = |r: Resources| scaling::resource_weight(&r);
    let mut world = Headless::new(SEED, Pace::parse("season").unwrap());
    let posts: [Post; 6] = [(9, 2, &[]), (12, 2, &[]), (14, 2, &[]), (16, 3, &[]), (18, 3, &[]), (20, 3, &[])];
    let keepers: Vec<u64> = posts.iter().map(|&(ring, level, garrison)| stand(&mut world, ring, level, garrison)).collect();
    for (pid, (ring, level, _)) in keepers.iter().zip(posts) {
        let odds = forecast_of(&world, *pid).odds;
        assert_eq!(odds.lost, 0.0, "level {level} at ring {ring} is never lost in the fights the server plays: {odds:?}");
    }
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
        assert!(field.raid_rounds >= field.outpost_raids_repelled, "and each was fought, {what}");
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
/// `ticks`. `stops` are ticks at which the world is stored as its document and read back.
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
    muster(&mut e.engine, fid, &[(ShipClass::SmallCargo, 2), (ShipClass::LightFighter, 8)], at);
    stow(&mut e, fid, COST.add(res(7500.0, 3750.0, 1500.0)).add(res(0.0, 0.0, 600.0)));
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
            e.engine = GameEngine::restore(stored(&e.engine));
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
        if e.engine.active_combats.values().any(|fight| fight.siege.is_some() && fight.round >= 1) {
            note("a raid between two of its rounds", tick, &mut seen, &mut marks);
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
            (250, Some(place)) if place == next => {
                let _ = e.order(Command::Move { fleet_id: fid, target: at });
            }
            (280, Some(place)) if place == at => {
                let _ = e.order(Command::Unload { fleet_id: fid });
            }
            _ => {}
        }
        e.run(1);
    }
    let raids = outcomes(&e, 0);
    let lost = raids.iter().filter(|(_, raid)| !raid.defended).count();
    assert!(lost >= 1 && raids.len() > lost, "the story has raids won and lost in it: {lost} lost of {}", raids.len());
    assert!(raids.iter().any(|(_, raid)| raid.rounds >= 2), "and fought over more rounds than one");
    (e.engine.snapshot().hash(), seen, marks)
}

#[test]
fn a_world_with_an_outpost_replays_and_resumes_from_a_snapshot_to_the_same_state() {
    const TICKS: u64 = 6000;
    let (straight, seen, marks) = story(TICKS, &[]);
    let moments =
        ["level 1 under construction", "level 2 under construction", "a raid announced", "a raid between two of its rounds", "breached"];
    for wanted in moments {
        assert!(seen.contains(&wanted), "the story never came to: {wanted} (it came to {seen:?})");
    }
    assert_eq!(story(TICKS, &[]).0, straight, "the same seed and the same orders give the same world");

    let stops: Vec<u64> = seen
        .iter()
        .zip(&marks)
        .map(|(what, tick)| if *what == "a raid between two of its rounds" { *tick } else { tick + 3 })
        .collect();
    let (resumed, ..) = story(TICKS, &stops);
    assert_eq!(resumed, straight, "stopped and read back at ticks {stops:?} ({seen:?}), the world went on as if it had not been");
}
