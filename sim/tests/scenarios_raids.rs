// Raids as players live them. The cadence: how often forecasts arrive at a
// pace, how far ahead they warn and how they are sized. And the raid
// itself, which is a battle (docs/design/ogame.md, section 7.6): what it
// flies, the rounds read one by one at the homeworld, its three endings
// and what follows each, the defences rebuilt, a fleet that leaves before
// it or flees from it, and what arrives or is finished while it is fought.

#[allow(dead_code)]
mod common;

use common::*;
use iac_shared::constants::{
    Resources, ShipClass, DEFENCE_REBUILD_PCT, DEFENCE_REBUILD_SPREAD, RAID_DEFENSE_SALVAGE_FRACTION, RAID_LOSS_CAP_CRYSTAL,
    RAID_LOSS_CAP_DEUT, RAID_LOSS_CAP_METAL, RAID_MIN_PLAYER_AGE, RAID_POWER_ROLL_MAX, RAID_POWER_ROLL_MIN, RAID_ROUNDS,
};
use iac_shared::protocol::{
    BattleOutcome, BattleRoundEvent, Command, CombatEndedEvent, ErrorCode, EventKind, PolicyPreset, RaidLabel,
    RaidResolvedEvent, UnitCount,
};
use iac_shared::scaling::{self, DefenceKind, ShipyardItem};
use iac_sim::GameEngine;

use DefenceKind::{
    GaussCannon, HeavyLaser, IonCannon, LargeShieldDome, LightLaser, PlasmaTurret, RocketLauncher, SmallShieldDome,
};
use ShipClass::{Battleship, Bomber, Cruiser, LightFighter, SmallCargo, SolarSatellite};

/// An empire whose one fleet stands at home as `ships` (gone when empty),
/// with `defences` and so many Solar Satellites, in a season.
fn garrison(ships: &[(ShipClass, u32)], defences: &[(DefenceKind, u32)], satellites: u32) -> Empire {
    let mut e = Empire::join(world("season"), "Warden");
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    muster(&mut e.engine, fid, ships, home);
    fortify(&mut e.engine, pid, defences, satellites);
    e
}

/// A raid as it was told: every round of it, how its fight ended and how
/// it was settled.
struct Told {
    rounds: Vec<BattleRoundEvent>,
    ended: Option<CombatEndedEvent>,
    resolved: RaidResolvedEvent,
    alerts: Vec<String>,
}

/// Play until the raid on its way is settled.
fn sit_out(e: &mut Empire) -> Told {
    let mark = e.mark();
    e.run_until(60, |e| e.log[mark..].iter().any(|ev| matches!(ev.kind, EventKind::RaidResolved(_))));
    told_since(e, mark)
}

fn told_since(e: &Empire, mark: usize) -> Told {
    Told {
        rounds: e.since(mark, |k| match k {
            EventKind::BattleRound(r) => Some(r.clone()),
            _ => None,
        }),
        ended: e
            .since(mark, |k| match k {
                EventKind::CombatEnded(c) => Some(c.clone()),
                _ => None,
            })
            .pop(),
        resolved: e
            .since(mark, |k| match k {
                EventKind::RaidResolved(r) => Some(r.clone()),
                _ => None,
            })
            .pop()
            .expect("the raid is settled"),
        alerts: e.since(mark, |k| match k {
            EventKind::Alert(a) => Some(a.message.clone()),
            _ => None,
        }),
    }
}

fn unit(item: impl Into<Unit>, count: u32) -> UnitCount {
    UnitCount { unit: item.into().0, count }
}

struct Unit(ShipyardItem);

impl From<ShipClass> for Unit {
    fn from(class: ShipClass) -> Unit {
        Unit(ShipyardItem::Ship(class))
    }
}

impl From<DefenceKind> for Unit {
    fn from(kind: DefenceKind) -> Unit {
        Unit(ShipyardItem::Defence(kind))
    }
}

fn lost_of(told: &Told, item: impl Into<Unit>) -> u32 {
    let wanted = item.into().0;
    told.resolved.lost.iter().find(|u| u.unit == wanted).map_or(0, |u| u.count)
}

fn rebuilt_of(told: &Told, kind: DefenceKind) -> u32 {
    told.resolved.rebuilt.iter().find(|u| u.unit == ShipyardItem::Defence(kind)).map_or(0, |u| u.count)
}

fn standing(e: &Empire, kind: DefenceKind) -> u32 {
    e.state().homeworld.defences.iter().find(|d| d.kind == kind).map_or(0, |d| d.count)
}

fn shots(round: &BattleRoundEvent, from: impl Into<Unit>, at: impl Into<Unit>) -> u32 {
    let (from, at) = (from.into().0, at.into().0);
    let both = round.fleets.volleys.iter().chain(&round.hostiles.volleys);
    both.filter(|v| v.from == from && v.at == at).map(|v| v.shots).sum()
}

#[test]
fn a_raid_destroyed_at_home_is_fought_round_by_round_and_leaves_its_wreckage() {
    let mut e = garrison(&[(Cruiser, 20)], &[(LightLaser, 40)], 6);
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    e.run(5);
    send_raid(&mut e.engine, pid, &[(LightFighter, 30)], 1.0, 3);
    let arrival = e.engine.current_tick() + 3;

    let forecast = e.state().homeworld.raid;
    assert_eq!((forecast.arrival_tick, &forecast.ships), (Some(arrival), &counts(&[(LightFighter, 30)])));
    assert_eq!(forecast.power, 30.0 * scaling::ship_power(LightFighter));
    assert_eq!(
        (forecast.odds.label, forecast.odds.destroyed, forecast.odds.lost),
        (RaidLabel::Repelled, 1.0, 0.0),
        "twenty Cruisers and forty Light Lasers destroy thirty Light Fighters every time the server plays it",
    );
    assert!(forecast.odds.kept > 0.9 && forecast.odds.raiders_kept == 0.0, "{:?}", forecast.odds);

    let mark = e.mark();
    e.run(3);
    assert_eq!(e.engine.current_tick(), arrival);
    assert!(told_rounds(&e, mark).is_empty(), "the raid stands at home on the tick it arrives and fires on the next");
    let state = e.state();
    let battle = &state.battles[0];
    let raid = battle.raid.as_ref().expect("the fight at home is the raid");
    assert_eq!((battle.sector, battle.rounds, raid.rounds_left), (home, 0, RAID_ROUNDS));
    assert!(raid.mine && !raid.outpost && raid.ships == forecast.ships && raid.power == forecast.power);
    assert_eq!(raid.odds.label, RaidLabel::Repelled);
    let fought = &state.homeworld.raid;
    assert_eq!(
        (fought.arrival_tick, fought.rounds_left),
        (None, Some(RAID_ROUNDS)),
        "the forecast is the raid being fought until it ends",
    );
    assert!(fought.ships == forecast.ships && fought.power == forecast.power && fought.odds == raid.odds);
    let fleet = e.fleet();
    assert_eq!(fleet.state, iac_shared::protocol::FleetStatus::InCombat);
    let quote = fleet.flight.expect("a fleet may leave a raid's fight as it leaves any");
    assert!(quote.chase.is_none() && quote.refusal.is_none(), "raiders follow nobody: {quote:?}");
    assert_eq!(
        e.refusal(Command::Load { fleet_id: fid, resource: iac_shared::constants::ResourceKind::Metal, amount: None }),
        (ErrorCode::OnCooldown, format!("fleet {fid} is busy; it can load once it is idle")),
        "a hold is not loaded in a fight",
    );
    assert!(
        e.log[mark..].iter().any(|ev| matches!(&ev.kind, EventKind::Alert(a) if a.message.starts_with(
            "the raid of 30 Light Fighter has arrived at your homeworld"
        ) && a.message.contains("it meets 20 ship(s), 6 Solar Satellite(s) and 40 defence(s)"))),
        "the owner is told what arrived and what it meets",
    );

    e.run_until(20, |e| e.log[mark..].iter().any(|ev| matches!(ev.kind, EventKind::RaidResolved(_))));
    let told = told_since(&e, mark);
    let fought = told.rounds.len() as u32;
    assert!((1..=RAID_ROUNDS).contains(&fought), "{fought} rounds");
    for (i, round) in told.rounds.iter().enumerate() {
        assert!(round.raid && round.mine && round.sector == home && round.round == i as u32 + 1);
        let fleets = &round.fleets.groups;
        assert!(fleets.iter().any(|g| g.fleet_id == fid && !g.battery && g.mine), "the docked fleet is in the round");
        let battery = fleets.iter().find(|g| g.battery).expect("the defences stand in the round as a group of their own");
        assert_eq!((battery.fleet_id, battery.label.as_str(), battery.mine), (0, "home defences", true));
        let kinds: Vec<ShipyardItem> = battery.units.iter().map(|u| u.unit).collect();
        assert_eq!(kinds, [ShipyardItem::Ship(SolarSatellite), ShipyardItem::Defence(LightLaser)]);
        let raiders = &round.hostiles.groups;
        assert_eq!(raiders.len(), 1);
        assert_eq!((raiders[0].label.as_str(), &raiders[0].owner), ("raid of 30 Light Fighter", &None));
        assert!(shots(round, LightLaser, LightFighter) > 0 && shots(round, Cruiser, LightFighter) > 20, "defences and ships fire");
    }
    assert_eq!(e.log[mark..].iter().find(|ev| matches!(ev.kind, EventKind::BattleRound(_))).unwrap().tick, arrival + 1);

    let ended = told.ended.as_ref().expect("a fought raid ends as a fight ends");
    assert!(ended.raid && ended.mine && ended.player_victory);
    assert_eq!((ended.outcome, ended.rounds), (BattleOutcome::HostilesDestroyed, fought));
    let outcome = &told.resolved;
    assert!(outcome.defended && !outcome.breached && outcome.outpost.is_none());
    assert_eq!((outcome.outcome, outcome.rounds), (Some(BattleOutcome::HostilesDestroyed), fought));
    assert_eq!((&outcome.raiders, &outcome.raiders_lost), (&forecast.ships, &forecast.ships));
    assert_eq!((outcome.raid_power, outcome.resources_lost), (forecast.power, Resources::default()));
    let pile = LightFighter.build_cost().scale(30.0 * RAID_DEFENSE_SALVAGE_FRACTION);
    assert_eq!(outcome.salvage_dropped, Some(pile), "three tenths of what thirty Light Fighters cost");
    assert_eq!(ended.salvage, Some(pile));

    let lasers_lost = lost_of(&told, LightLaser);
    let rebuilt = rebuilt_of(&told, LightLaser);
    assert!(rebuilt <= lasers_lost && outcome.structures_lost == lasers_lost && outcome.structures_restored == rebuilt);
    assert_eq!(standing(&e, LightLaser), 40 - lasers_lost + rebuilt, "what fell is gone, what was rebuilt stands at once");
    assert_eq!(e.state().homeworld.satellites, 6 - outcome.satellites_lost, "a Solar Satellite lost is lost");
    assert_eq!(outcome.satellites_lost, lost_of(&told, SolarSatellite));
    assert_eq!(e.fleet().ship_count, 20 - outcome.ships_lost);
    assert_eq!(outcome.ships_lost, lost_of(&told, Cruiser));

    assert!(e.state().battles.is_empty() && e.fleet().flight.is_none());
    let next = e.state().homeworld.raid;
    assert_eq!((next.arrival_tick, next.rounds_left), (None, None), "with the raid over the forecast is of the next one");
    e.run(2);
    assert!(e.fleet().repair.is_none(), "the home dock mends what the fight left of the fleet");
    let scooped = e.since(mark, |k| match k {
        EventKind::SalvageCollected(c) if c.sector == home => Some(c.resources),
        _ => None,
    });
    assert_eq!(scooped, [pile], "the docked fleet scoops the wreckage into storage");
}

fn told_rounds(e: &Empire, mark: usize) -> Vec<BattleRoundEvent> {
    e.since(mark, |k| match k {
        EventKind::BattleRound(r) => Some(r.clone()),
        _ => None,
    })
}

#[test]
fn a_defence_that_could_never_beat_the_raid_outlasts_it_for_six_rounds() {
    let mut e = garrison(&[], &[(LargeShieldDome, 1), (IonCannon, 30)], 0);
    let pid = e.pid;
    e.set_stockpile(Resources { metal: 50_000.0, crystal: 40_000.0, deuterium: 30_000.0 });
    e.run(3);
    send_raid(&mut e.engine, pid, &[(Cruiser, 24)], 1.0, 2);
    let odds = e.state().homeworld.raid.odds;
    assert_eq!(
        (odds.label, odds.destroyed, odds.outlasted, odds.lost),
        (RaidLabel::Held, 0.0, 1.0, 0.0),
        "the server never saw it destroy the raid, nor fall to it",
    );
    assert!(odds.kept < 0.6 && odds.raiders_kept > 0.5, "holding costs most of the cannons: {odds:?}");

    let stock = e.stockpile();
    let told = sit_out(&mut e);
    assert_eq!(told.rounds.len() as u32, RAID_ROUNDS);
    assert!(told.rounds.iter().all(|round| round.fleets.hull_damage + round.hostiles.hull_damage > 0), "hull fell in every round");
    let dome = ShipyardItem::Defence(LargeShieldDome);
    let at_dome: Vec<_> = told.rounds.iter().flat_map(|r| &r.hostiles.volleys).filter(|v| v.at == dome).collect();
    assert!(!at_dome.is_empty() && at_dome.iter().all(|v| v.absorbed > 0 && v.hull_damage == 0 && v.destroyed == 0));
    assert_eq!(at_dome.iter().map(|v| v.bounced).sum::<u32>(), 0, "a Cruiser's shot is four hundredths of that shield: it soaks them");

    let ended = told.ended.as_ref().unwrap();
    assert!(ended.raid && !ended.player_victory && ended.salvage.is_none());
    assert_eq!((ended.outcome, ended.rounds), (BattleOutcome::Outlasted, RAID_ROUNDS));
    let outcome = &told.resolved;
    assert!(outcome.defended, "a defender stood when the raiders gave up");
    assert_eq!((outcome.outcome, outcome.rounds), (Some(BattleOutcome::Outlasted), RAID_ROUNDS));
    assert_eq!((outcome.salvage_dropped, outcome.resources_lost), (None, Resources::default()), "no wreckage, and nothing taken");
    let raiders_lost: u32 = outcome.raiders_lost.iter().map(|s| s.count).sum();
    assert!(raiders_lost < 24, "{raiders_lost} of 24 Cruisers fell");
    let fell = lost_of(&told, IonCannon);
    let back = rebuilt_of(&told, IonCannon);
    assert!(fell >= 15 && lost_of(&told, LargeShieldDome) == 0, "{fell} cannons fell and the dome stood");
    assert!((fell * 6 / 10..=fell * 8 / 10).contains(&back), "{back} of {fell} cannons rebuilt");
    assert_eq!((standing(&e, IonCannon), standing(&e, LargeShieldDome)), (30 - fell + back, 1));
    let made = e.engine.players[&pid].production_per_tick(&e.engine.pace()).scale(f64::from(RAID_ROUNDS + 2));
    assert!((e.stockpile().metal - stock.metal - made.metal).abs() < 1e-6, "the stockpile only grew");
    assert_eq!(e.engine.snapshot().raid_states[&pid].suppress_until, 0, "a raid repelled buys no quiet");
}

#[test]
fn a_round_that_hurts_nobody_ends_a_raid_before_its_six() {
    let mut e = garrison(&[], &[(SmallShieldDome, 1), (RocketLauncher, 10)], 0);
    e.run(3);
    send_raid(&mut e.engine, e.pid, &[(Cruiser, 4)], 1.0, 2);
    assert_eq!(e.state().homeworld.raid.odds.label, RaidLabel::Held, "the forecast says the dome holds and destroys nothing");
    let told = sit_out(&mut e);
    let fought = told.rounds.len() as u32;
    assert!((2..RAID_ROUNDS).contains(&fought), "{fought} rounds");
    let last = told.rounds.last().unwrap();
    assert_eq!((last.fleets.hull_damage, last.hostiles.hull_damage), (0, 0), "the last round took no hull from anybody");
    assert!(told.rounds[..told.rounds.len() - 1].iter().all(|r| r.fleets.hull_damage + r.hostiles.hull_damage > 0));
    assert_eq!(last.fleets.groups[0].units.iter().map(|u| (u.unit, u.count)).collect::<Vec<_>>(), [
        (ShipyardItem::Defence(SmallShieldDome), 1)
    ]);
    assert_eq!((lost_of(&told, RocketLauncher), lost_of(&told, SmallShieldDome)), (10, 0));
    assert!(told.resolved.defended && told.resolved.raiders_lost.is_empty());
    assert_eq!(told.resolved.outcome, Some(BattleOutcome::Outlasted), "four Cruisers cannot break a Small Shield Dome's shield in a round");
}

#[test]
fn a_lost_raid_takes_everything_that_stayed_and_the_defences_come_back_at_ogames_rate() {
    let stage = |tick: u64| {
        let mut e = garrison(&[(LightFighter, 10)], &[(RocketLauncher, 100), (LightLaser, 5)], 20);
        let pid = e.pid;
        edit(&mut e.engine, |s| {
            let b = &mut s.players.get_mut(&pid).unwrap().buildings;
            (b.metal_storage, b.crystal_storage, b.deuterium_tank) = (2, 2, 2);
        });
        e.run(tick);
        e.set_stockpile(Resources { metal: 400_000.0, crystal: 300_000.0, deuterium: 250_000.0 });
        send_raid(&mut e.engine, pid, &[(Cruiser, 200)], 1.0, 2);
        e
    };

    let mut e = stage(4);
    let (pid, fid) = (e.pid, e.fid);
    assert_eq!(e.state().homeworld.raid.odds.label, RaidLabel::Lost);
    let mark = e.mark();
    e.run(2);
    let before = e.stockpile();
    let pace = e.engine.pace();
    let protected = scaling::storage_protected(&e.engine.players[&pid].buildings, &pace);
    assert!(protected.metal > 0.0 && protected.metal < before.metal);
    let told = sit_out(&mut e);
    let told = Told { alerts: told_since(&e, mark).alerts, ..told };

    let outcome = &told.resolved;
    assert!(!outcome.defended && told.rounds.len() as u32 == outcome.rounds && outcome.rounds <= 2);
    assert_eq!(outcome.outcome, Some(BattleOutcome::FleetsDestroyed));
    let ended = told.ended.as_ref().unwrap();
    assert!(ended.raid && !ended.player_victory && ended.salvage.is_none());
    assert_eq!(
        outcome.lost,
        [unit(LightFighter, 10), unit(SolarSatellite, 20), unit(RocketLauncher, 100), unit(LightLaser, 5)],
        "every ship that stayed, every satellite and every defence",
    );
    assert_eq!((outcome.ships_lost, outcome.satellites_lost, outcome.structures_lost), (10, 20, 105));
    assert!(outcome.raiders_lost.iter().map(|s| s.count).sum::<u32>() < 10);
    assert!(!e.engine.fleets.contains_key(&fid) || e.engine.fleets[&fid].ships.is_empty());
    assert!(told.alerts.iter().any(|m| m.contains(&format!("FLEET LOST: fleet {fid} destroyed")) && m.contains("by the raid of 200 Cruiser")));

    let launchers = rebuilt_of(&told, RocketLauncher);
    let (least, most) = (100 * (DEFENCE_REBUILD_PCT - DEFENCE_REBUILD_SPREAD) / 100, 100 * (DEFENCE_REBUILD_PCT + DEFENCE_REBUILD_SPREAD) / 100);
    assert!((least..=most).contains(&launchers), "{launchers} of 100 launchers rebuilt");
    assert_eq!((standing(&e, RocketLauncher), standing(&e, LightLaser)), (launchers, rebuilt_of(&told, LightLaser)));
    assert_eq!(e.state().homeworld.satellites, 0, "no satellite and no ship is rebuilt");
    assert_eq!(outcome.structures_restored, launchers + rebuilt_of(&told, LightLaser));

    let made = e.engine.players[&pid].production_per_tick(&pace).scale(f64::from(outcome.rounds));
    let exposed = before.add(made).sub(protected);
    let taken = Resources {
        metal: exposed.metal * RAID_LOSS_CAP_METAL,
        crystal: exposed.crystal * RAID_LOSS_CAP_CRYSTAL,
        deuterium: exposed.deuterium * RAID_LOSS_CAP_DEUT,
    };
    let near = |a: Resources, b: Resources| (a.metal - b.metal).abs() < 60.0 && (a.crystal - b.crystal).abs() < 60.0 && (a.deuterium - b.deuterium).abs() < 60.0;
    assert!(near(outcome.resources_lost, taken), "8, 8 and 5 percent above the protected share: {:?} against {taken:?}", outcome.resources_lost);
    assert!(near(outcome.protected_kept, protected));
    let clock = &e.engine.snapshot().raid_states[&pid];
    let tick = e.engine.current_tick();
    assert_eq!(clock.suppress_until, tick + pace.attention_ticks(172_800.0), "and no raid comes for two days of raid clock");

    let (mut launchers, mut lasers) = (0u32, 0u32);
    for tick in 5..17 {
        let mut e = stage(tick);
        let told = sit_out(&mut e);
        assert!(!told.resolved.defended);
        launchers += rebuilt_of(&told, RocketLauncher);
        lasers += rebuilt_of(&told, LightLaser);
    }
    assert!((64 * 12..=76 * 12).contains(&launchers), "{launchers} of 1200 launchers rebuilt over twelve raids: 70 in a hundred");
    assert!((30..=54).contains(&lasers), "{lasers} of 60 lasers rebuilt, each on its own 70 in a hundred");
}

#[test]
fn rapid_fire_against_defences_shows_in_the_rounds() {
    let first_round = |defence: DefenceKind, count: u32, raider: ShipClass| {
        let mut e = garrison(&[], &[(defence, count)], 0);
        e.run(3);
        send_raid(&mut e.engine, e.pid, &[(raider, 10)], 1.0, 2);
        sit_out(&mut e).rounds.remove(0)
    };

    let launchers = first_round(RocketLauncher, 200, Cruiser);
    let fired = shots(&launchers, Cruiser, RocketLauncher);
    assert!(fired >= 50, "ten Cruisers fire again nine times in ten at a Rocket Launcher: {fired} shots");
    assert_eq!(shots(&launchers, RocketLauncher, Cruiser), 200, "a defence fires once");

    let lasers = first_round(LightLaser, 200, Cruiser);
    assert_eq!(shots(&lasers, Cruiser, LightLaser), 10, "and once each at a Light Laser, which a Cruiser has no rapid fire against");

    let bombed = first_round(LightLaser, 200, Bomber);
    let fired = shots(&bombed, Bomber, LightLaser);
    assert!(fired >= 100, "ten Bombers fire again nineteen times in twenty at a Light Laser: {fired} shots");

    let heavy = first_round(HeavyLaser, 200, Bomber);
    let fired = shots(&heavy, Bomber, HeavyLaser);
    assert!((30..180).contains(&fired), "and nine times in ten at a Heavy Laser: {fired} shots");

    for (kind, count) in [(GaussCannon, 50), (PlasmaTurret, 20)] {
        let spared = first_round(kind, count, Bomber);
        assert_eq!(shots(&spared, Bomber, kind), 10, "a Bomber has no rapid fire against the {}", kind.label());
    }

    let satellites = {
        let mut e = garrison(&[], &[(IonCannon, 20)], 400);
        e.run(3);
        send_raid(&mut e.engine, e.pid, &[(LightFighter, 20)], 1.0, 2);
        sit_out(&mut e).rounds.remove(0)
    };
    let fired = shots(&satellites, LightFighter, SolarSatellite);
    assert!(fired >= 50, "every armed ship fires again four times in five at a Solar Satellite: {fired} shots from twenty");
}

#[test]
fn a_fleet_that_leaves_during_the_warning_is_not_there_when_the_raid_arrives() {
    let mut e = garrison(&[(LightFighter, 10)], &[], 0);
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    e.set_stockpile(Resources { metal: 80_000.0, crystal: 60_000.0, deuterium: 40_000.0 });
    e.run(3);
    send_raid(&mut e.engine, pid, &[(Cruiser, 50)], 1.0, 6);
    assert_eq!(e.state().homeworld.raid.odds.label, RaidLabel::Lost);
    let beside = e.engine.world_gen.connected_neighbors(home).slice()[0];
    e.order(Command::Move { fleet_id: fid, target: beside }).expect("a fleet may leave before the raid arrives");
    assert_eq!(e.state().homeworld.raid.odds.kept, 0.0, "a fleet charging its jump out is not counted on");

    let before = e.stockpile();
    let told = sit_out(&mut e);
    assert!(told.rounds.is_empty() && told.ended.is_none(), "nothing stood in its way: no round is fought");
    let outcome = &told.resolved;
    assert!(!outcome.defended);
    assert_eq!((outcome.outcome, outcome.rounds, outcome.ships_lost), (None, 0, 0));
    assert!(outcome.lost.is_empty() && outcome.raiders_lost.is_empty() && outcome.raiders == counts(&[(Cruiser, 50)]));
    assert!(outcome.resources_lost.metal > 0.07 * before.metal, "it takes its share: {:?}", outcome.resources_lost);
    let fleet = e.fleet();
    assert_eq!((fleet.location, fleet.ship_count), (beside, 10), "the fleet is whole, one sector away");
    assert!(fleet.pursued_by.is_none() && e.state().battles.is_empty());
}

#[test]
fn a_fleet_flees_the_raid_under_fire_and_the_raiders_do_not_follow() {
    let mut e = garrison(&[(Cruiser, 30)], &[], 0);
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    e.set_stockpile(Resources { metal: 80_000.0, crystal: 60_000.0, deuterium: 40_000.0 });
    e.run(3);
    send_raid(&mut e.engine, pid, &[(Battleship, 40)], 1.0, 2);
    let mark = e.mark();
    e.run(3);
    assert_eq!(told_rounds(&e, mark).len(), 1, "one round fought");
    let quote = e.fleet().flight.expect("the fleet stands in the raid's fight");
    assert!(quote.ratio < 0.9 && quote.chase.is_none(), "forty Battleships are too many: {quote:?}");

    let beside = e.engine.world_gen.connected_neighbors(home).slice()[0];
    e.order(Command::Move { fleet_id: fid, target: beside }).expect("flight from a raid");
    let leaving = e.fleet().fleeing.expect("it is leaving");
    assert_eq!((leaving.to, leaving.rounds), (beside, quote.rounds));
    let ships_then = e.fleet().ship_count;

    let told = sit_out(&mut e);
    let told = Told { rounds: told_rounds(&e, mark), ..told };
    let parting = &told.rounds[1..];
    assert_eq!(parting.len(), usize::from(quote.rounds), "it is fired on for the rounds its jump charges");
    assert!(parting.iter().all(|r| r.fleets.groups[0].fleeing && r.fleets.shots == 0 && r.hostiles.shots > 0), "and does not answer");
    let outcome = &told.resolved;
    assert!(!outcome.defended, "with the last defender gone the raid is lost");
    assert_eq!(outcome.outcome, Some(BattleOutcome::Fled));
    assert_eq!(told.ended.as_ref().unwrap().outcome, BattleOutcome::Fled);
    assert!(outcome.resources_lost.metal > 0.0);
    let fleet = e.fleet();
    assert_eq!(fleet.location, beside);
    assert!(fleet.ship_count < ships_then && fleet.ship_count > 0, "{} of {ships_then} Cruisers got away", fleet.ship_count);
    assert_eq!(outcome.ships_lost, 30 - fleet.ship_count);
    assert!(fleet.pursued_by.is_none());
    assert!(e.since(mark, |k| matches!(k, EventKind::Pursuit(_)).then_some(())).is_empty(), "raiders follow nobody");
    assert!(e.state().battles.is_empty());
}

#[test]
fn a_fleet_under_standing_orders_stands_through_a_raid_it_wins_and_leaves_one_it_would_die_in() {
    let under_orders = |ships: &[(ShipClass, u32)], defences: &[(DefenceKind, u32)], raid: &[(ShipClass, u32)]| {
        let mut e = garrison(ships, defences, 0);
        let (pid, fid) = (e.pid, e.fid);
        e.run(3);
        send_raid(&mut e.engine, pid, raid, 1.0, 1);
        e.run(1);
        e.engine.handle_policy_update(pid, fid, PolicyPreset::PatrolHome, None).expect("orders for a fleet in the raid's fight");
        let mark = e.mark();
        let told = sit_out(&mut e);
        let acts = e.since(mark, |k| match k {
            EventKind::PolicyAction(p) if p.fleet_id == fid => Some((p.action.clone(), p.reason.clone())),
            _ => None,
        });
        (e, told, acts)
    };

    let (e, told, acts) = under_orders(&[(Cruiser, 20)], &[(LightLaser, 40)], &[(LightFighter, 30)]);
    assert!(acts.iter().all(|(action, _)| action != "flee"), "{acts:?}");
    assert_eq!(told.resolved.outcome, Some(BattleOutcome::HostilesDestroyed));
    assert_eq!(e.fleet().location, e.engine.players[&e.pid].homeworld);

    let (e, told, acts) = under_orders(&[(Cruiser, 30)], &[], &[(Battleship, 40)]);
    let left: Vec<&String> = acts.iter().filter(|(action, _)| action == "flee").map(|(_, reason)| reason).collect();
    assert_eq!(left.len(), 1, "{acts:?}");
    assert!(left[0].starts_with("left the fight at") && left[0].contains("below the doctrine's 0.9x"), "{}", left[0]);
    assert_eq!(told.resolved.outcome, Some(BattleOutcome::Fled), "its leaving left the homeworld to the raid");
    let fleet = e.fleet();
    assert!(fleet.ship_count > 0 && fleet.location != e.engine.players[&e.pid].homeworld, "and the ships it kept are out of it");
}

#[test]
fn what_arrives_or_is_finished_during_a_raid_stands_in_it_from_the_next_round() {
    let mut e = garrison(&[(Cruiser, 6)], &[(LargeShieldDome, 1), (IonCannon, 30)], 0);
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    let beside = e.engine.world_gen.connected_neighbors(home).slice()[0];
    muster(&mut e.engine, fid, &[(Cruiser, 6)], beside);
    e.run(3);
    send_raid(&mut e.engine, pid, &[(Cruiser, 24)], 1.0, 2);
    let mark = e.mark();
    e.run(3);
    assert_eq!(told_rounds(&e, mark).len(), 1);
    assert!(told_rounds(&e, mark)[0].fleets.groups.iter().all(|g| g.battery), "the fleet is away: the defences fight alone");

    e.order(Command::Move { fleet_id: fid, target: home }).expect("the fleet comes home into the fight");
    edit(&mut e.engine, |s| {
        let player = s.players.get_mut(&pid).unwrap();
        player.defences.count[HeavyLaser as usize] += 5;
        player.satellites += 3;
        player.research.weapons_technology += 5;
    });
    let refit_before = e.fleet().refit;
    e.run_until(10, |e| e.engine.fleets[&fid].location == home);
    let landed = e.engine.current_tick();
    assert!(e.fleet().refit.weapons == refit_before.weapons + 5, "a fleet that arrives docks first: it is refitted, then it fights");
    assert_eq!(e.fleet().state, iac_shared::protocol::FleetStatus::InCombat, "in the round of the tick it lands");
    edit(&mut e.engine, |s| s.players.get_mut(&pid).unwrap().research.weapons_technology += 1);

    e.run_until(20, |e| e.log[mark..].iter().any(|ev| matches!(ev.kind, EventKind::RaidResolved(_))));
    let rounds: Vec<(u64, BattleRoundEvent)> = e.log[mark..]
        .iter()
        .filter_map(|ev| match &ev.kind {
            EventKind::BattleRound(r) => Some((ev.tick, r.clone())),
            _ => None,
        })
        .collect();
    let second = &rounds[1].1;
    let battery = second.fleets.groups.iter().find(|g| g.battery).unwrap();
    let count_of = |item: ShipyardItem| battery.units.iter().find(|u| u.unit == item).map_or(0, |u| u.count + u.lost);
    assert_eq!(
        (count_of(ShipyardItem::Defence(HeavyLaser)), count_of(ShipyardItem::Ship(SolarSatellite))),
        (5, 3),
        "what the yard finished after a round stands in the next",
    );
    for (tick, round) in &rounds {
        let with_fleet = round.fleets.groups.iter().any(|g| g.fleet_id == fid);
        assert_eq!(with_fleet, *tick >= landed, "tick {tick}: the fleet fights from the round of the tick it landed, {landed}");
    }
    assert!(rounds.iter().any(|(tick, _)| *tick > landed), "the fight outlasted the fleet's jump");
    assert!(told_since(&e, mark).resolved.defended);
    e.run(1);
    let fleet = e.fleet();
    assert_eq!(
        (fleet.refit.weapons, fleet.refit_due),
        (refit_before.weapons + 6, false),
        "research finished during the fight reaches the fleet when the fight is over",
    );
}

#[test]
fn an_empire_with_nothing_at_home_loses_every_raid_and_a_share_of_its_stock_each_time() {
    let mut e = Empire::join(world("season"), "Absent");
    let (pid, fid) = (e.pid, e.fid);
    let home = e.engine.players[&pid].homeworld;
    let far = outward_chain(&e.engine, home, 3)[2];
    muster(&mut e.engine, fid, &[(SmallCargo, 2)], far);
    sweep(&mut e.engine, &[far]);
    edit(&mut e.engine, |s| s.players.get_mut(&pid).unwrap().buildings.shipyard = 2);
    let stock = Resources { metal: 100_000.0, crystal: 60_000.0, deuterium: 30_000.0 };

    let (mut raids, mut taken) = (Vec::new(), Resources::default());
    for _ in 0..7 * 24 {
        e.set_stockpile(stock);
        let mark = e.mark();
        e.run(3600);
        for event in &e.log[mark..] {
            if let EventKind::RaidResolved(r) = &event.kind {
                raids.push((event.tick, r.clone()));
                taken = taken.add(r.resources_lost);
            }
        }
    }
    assert!((8..=16).contains(&raids.len()), "{} raids in a season week", raids.len());
    assert!(raids.iter().all(|(_, r)| !r.defended && r.outcome.is_none() && r.rounds == 0 && r.lost.is_empty()));
    let grace = e.engine.pace().attention_ticks(172_800.0);
    assert!(raids.windows(2).all(|pair| pair[1].0 - pair[0].0 >= grace), "each lost raid buys two days of raid clock");
    let per_raid = taken.metal / raids.len() as f64;
    assert!((0.075 * stock.metal..0.09 * stock.metal).contains(&per_raid), "8 percent of the metal each time: {per_raid}");
    assert!(
        e.log.iter().all(|ev| !matches!(ev.kind, EventKind::BattleRound(_) | EventKind::CombatEnded(_))),
        "and no fight is told of where none was fought",
    );
    eprintln!(
        "an empire with nothing at home, a season week: {} raids, {:.0} metal, {:.0} crystal, {:.0} deuterium taken of a stockpile held at {:.0}/{:.0}/{:.0}",
        raids.len(), taken.metal, taken.crystal, taken.deuterium, stock.metal, stock.crystal, stock.deuterium,
    );
}

struct Forecast {
    forecast_tick: u64,
    arrival_tick: u64,
    est_power: f32,
}

fn raided_world(pace: &str, players: usize) -> (GameEngine, Vec<u64>) {
    let mut engine = world(pace);
    let pids: Vec<u64> = (0..players).map(|i| join(&mut engine, &format!("Raided{i}")).0).collect();
    edit(&mut engine, |s| {
        for pid in &pids {
            s.players.get_mut(pid).unwrap().buildings.shipyard = 2;
        }
    });
    (engine, pids)
}

fn forecasts(engine: &mut GameEngine, pids: &[u64], ticks: u64) -> Vec<Vec<Forecast>> {
    let mut seen: Vec<Vec<Forecast>> = pids.iter().map(|_| Vec::new()).collect();
    for e in run(engine, ticks) {
        if let EventKind::RaidIncoming(r) = e.kind
            && let Some(i) = pids.iter().position(|p| *p == r.player_id)
        {
            seen[i].push(Forecast { forecast_tick: e.tick, arrival_tick: r.arrival_tick, est_power: r.est_power });
        }
    }
    seen
}

#[test]
fn blitz_players_see_sized_raids_within_the_hour_and_never_before_they_are_old_enough() {
    let (mut engine, pids) = raided_world("blitz", 12);
    let base = {
        let p = &engine.players[&pids[0]];
        scaling::raid_power_base(scaling::econ_points(&p.buildings, &p.research))
    };
    let warning = engine.pace().raid_warning_ticks();
    let min_age = engine.pace().attention_ticks(RAID_MIN_PLAYER_AGE as f64);

    let seen = forecasts(&mut engine, &pids, 3600);
    let counts: Vec<usize> = seen.iter().map(Vec::len).collect();
    let quiet = counts.iter().filter(|c| **c == 0).count();
    let mean = counts.iter().sum::<usize>() as f32 / counts.len() as f32;
    assert!(quiet <= 1, "{quiet} of 12 blitz hours had no raid: {counts:?}");
    // Two Light Fighters at home repel every raid on an empire this small,
    // so no raid is followed by the quiet a lost one buys.
    assert!((2.0..=4.5).contains(&mean), "mean {mean} raids per blitz hour: {counts:?}");

    for f in seen.iter().flatten() {
        assert!(f.forecast_tick >= min_age, "forecast at {} before the empire was {min_age} ticks old", f.forecast_tick);
        assert_eq!(f.arrival_tick - f.forecast_tick, warning);
        assert!(
            (base * RAID_POWER_ROLL_MIN..=base * RAID_POWER_ROLL_MAX).contains(&f.est_power),
            "{} vs base {base}",
            f.est_power
        );
    }
}

#[test]
fn season_players_see_about_three_raids_a_day() {
    let (mut engine, pids) = raided_world("season", 6);
    let seen = forecasts(&mut engine, &pids, 24 * 3600);
    let mean = seen.iter().map(Vec::len).sum::<usize>() as f32 / pids.len() as f32;
    assert!((1.8..=4.5).contains(&mean), "mean {mean} raids per 24 h of season");
}
