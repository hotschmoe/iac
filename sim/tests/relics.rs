// Fading relics: a boarding in the deep puts an item aboard a fleet, the
// owner's dock claims it, and the bonus it gives falls in a straight line to
// nothing. A relic is carried risk (lost with its fleet), the same kind
// refreshes and never stacks, a family of kinds is capped together, and
// nothing about one moves the score. A world stopped and started through its
// document is the world played straight through.

#[allow(dead_code)]
mod common;

use common::*;
use iac_shared::constants::{
    RelicKind, ShipClass, RELIC_FADE_TICKS, RELIC_PCT_DEEP_MAX, RELIC_PCT_FLOOR_SHARE, RELIC_TOTAL_CAP,
};
use iac_shared::hex::Hex;
use iac_shared::protocol::{Command, EventKind};
use iac_shared::scaling::{self, ClaimedRelic, RelicBonus, RelicItem, ResearchType};
use iac_sim::engine::{FleetStatus, Ships};
use iac_sim::GameEngine;

const FRACTION: f64 = 1e-9;

fn fade_span(e: &Empire) -> u64 {
    e.engine.pace().econ_ticks(RELIC_FADE_TICKS as f64)
}

fn away_from_home(e: &Empire) -> Hex {
    let home = e.engine.players[&e.pid].homeworld;
    e.engine.world_gen.connected_neighbors(home).slice()[0]
}

fn bring_home(e: &mut Empire, fid: u64, relic: RelicItem) {
    let away = away_from_home(e);
    edit(&mut e.engine, |s| {
        let fleet = s.fleets.get_mut(&fid).unwrap();
        fleet.ships = Ships::of(&[(ShipClass::LightFighter, 40)]);
        fleet.fuel_max = 1e6;
        fleet.fuel = 1e6;
        fleet.state = FleetStatus::Idle;
        fleet.location = away;
        fleet.relic = Some(relic);
    });
    e.order(Command::Recall { fleet_id: fid }).expect("the fleet comes home");
}

fn claim(e: &mut Empire, kind: RelicKind, pct: f64) {
    bring_home(e, e.fid, RelicItem { kind, pct });
}

fn held(e: &Empire) -> Vec<ClaimedRelic> {
    e.engine.players[&e.pid].relics.clone()
}

fn bonus(e: &Empire) -> RelicBonus {
    e.engine.relics_of(e.pid)
}

fn mined(e: &Empire, relics: &RelicBonus) -> iac_shared::constants::Resources {
    e.engine.players[&e.pid].mined_per_tick(relics, &e.engine.pace())
}

fn empire() -> Empire {
    Empire::join(world("season"), "Keeper")
}

#[test]
fn a_boarding_in_the_deep_puts_a_relic_aboard_and_the_dock_claims_it() {
    let mut e = Empire::join(world("test"), "Boarder");
    let fid = e.fid;
    let sites: Vec<Hex> = (19..=34u16)
        .flat_map(|ring| iac_shared::hex::hex_ring(Hex::ORIGIN, ring))
        .filter(|&h| e.engine.derelict_site_at(h).is_some() && e.engine.world_gen.generate_sector(h).npc_template.is_none())
        .take(12)
        .collect();
    assert!(!sites.is_empty(), "derelicts with no guard in the deep");
    let home = e.engine.players[&e.pid].homeworld;
    let mut found = None;
    'search: for round in 0..40u64 {
        for &site in &sites {
            edit(&mut e.engine, |s| {
                s.sector_overrides.entry(site.to_key()).or_default().site_looted_tick = None;
                let fleet = s.fleets.get_mut(&fid).unwrap();
                fleet.ships = Ships::of(&[(ShipClass::Battleship, 60)]);
                fleet.fuel_max = 1e6;
                fleet.fuel = 1e6;
                fleet.state = FleetStatus::Idle;
                fleet.action_cooldown = 0;
                fleet.location = site;
                fleet.relic = None;
            });
            let mark = e.mark();
            e.order(Command::ExploreSite { fleet_id: fid }).expect("a boarding party goes in");
            e.run(u64::from(iac_shared::constants::EXPLORE_DURATION_TICKS) + 2);
            e.run_until(60, |e| e.engine.active_combats.is_empty());
            let relics = e.since(mark, |k| match k {
                EventKind::RelicFound(f) => Some(f.clone()),
                _ => None,
            });
            if let Some(f) = relics.first() {
                found = Some((site, f.clone(), round));
                break 'search;
            }
        }
    }
    let (site, event, _) = found.expect("a relic within forty rounds of every site");
    assert!(event.carried);
    let ceiling = scaling::relic_ceiling(site.dist_from_origin());
    assert!(
        (ceiling * RELIC_PCT_FLOOR_SHARE..=ceiling).contains(&event.pct),
        "{} outside [{}, {ceiling}] at ring {}",
        event.pct,
        ceiling * RELIC_PCT_FLOOR_SHARE,
        site.dist_from_origin(),
    );
    assert!(event.pct <= RELIC_PCT_DEEP_MAX);
    let aboard = e.fleet().relic.expect("the fleet carries it");
    assert_eq!((aboard.kind, aboard.pct), (event.relic_kind, event.pct));
    assert!(held(&e).is_empty(), "carried is not claimed: nothing is held until a dock");
    assert_eq!(bonus(&e), RelicBonus::NONE);

    let mark = e.mark();
    edit(&mut e.engine, |s| {
        let fleet = s.fleets.get_mut(&fid).unwrap();
        fleet.state = FleetStatus::Idle;
        fleet.fuel_max = 1e12;
        fleet.fuel = 1e12;
    });
    e.order(Command::Recall { fleet_id: fid }).expect("home");
    assert_eq!(e.engine.fleets[&fid].location, home);
    let claimed = e.since(mark, |k| match k {
        EventKind::RelicClaimed(c) => Some(c.clone()),
        _ => None,
    });
    assert_eq!(claimed.len(), 1);
    assert!(claimed[0].applied && claimed[0].relic_kind == event.relic_kind);
    assert!(e.fleet().relic.is_none());
    let list = e.state().player.relics;
    assert_eq!(list.len(), 1);
    assert_eq!((list[0].kind, list[0].pct, list[0].pct_now), (event.relic_kind, event.pct, event.pct));
    assert_eq!(list[0].expires_at, e.engine.current_tick() + fade_span(&e));
}

#[test]
fn an_output_relic_scales_the_mine_and_halves_at_half_the_fade() {
    let mut e = empire();
    let none = RelicBonus::NONE;
    let base = mined(&e, &none);
    claim(&mut e, RelicKind::MetalOutput, 0.2);
    let tick = e.engine.current_tick();
    let relics = bonus(&e);
    assert!((relics.metal - 0.2).abs() < FRACTION);
    let boosted = mined(&e, &relics);
    assert!((boosted.metal - base.metal * 1.2).abs() < 1e-9, "{} against {}", boosted.metal, base.metal);
    assert_eq!((boosted.crystal, boosted.deuterium), (base.crystal, base.deuterium));

    let span = fade_span(&e);
    let produced = e.engine.players[&e.pid].production_per_tick(&relics, &e.engine.pace());
    assert!((produced.metal - boosted.metal).abs() < 1e-9);

    e.run(span / 2);
    assert_eq!(e.engine.current_tick(), tick + span / 2);
    let halfway = bonus(&e);
    assert!((halfway.metal - 0.1).abs() < 1e-4, "half the fade leaves half: {}", halfway.metal);
    let state = &e.state().player.relics[0];
    assert!((state.pct_now - halfway.metal).abs() < FRACTION);

    let mark = e.mark();
    e.run(span - span / 2);
    assert!(held(&e).is_empty(), "a relic at zero is dropped");
    assert_eq!(bonus(&e), RelicBonus::NONE);
    assert!(
        e.since(mark, |k| matches!(k, EventKind::RelicFaded(f) if f.relic_kind == RelicKind::MetalOutput).then_some(())).len() == 1,
        "the fade is reported once"
    );
    assert!(e.state().player.relics.is_empty());
}

#[test]
fn the_stockpile_gains_what_the_bonus_says() {
    let mut boosted = empire();
    let mut plain = empire();
    claim(&mut boosted, RelicKind::MetalOutput, 0.25);
    let (a0, b0) = (boosted.stockpile().metal, plain.stockpile().metal);
    boosted.run(60);
    plain.run(60);
    let (gain_boosted, gain_plain) = (boosted.stockpile().metal - a0, plain.stockpile().metal - b0);
    assert!(gain_boosted > gain_plain * 1.2, "{gain_boosted} against {gain_plain}");
    assert!(gain_boosted < gain_plain * 1.26, "{gain_boosted} against {gain_plain}");
}

#[test]
fn the_same_kind_refreshes_and_never_stacks() {
    let mut e = empire();
    claim(&mut e, RelicKind::CrystalOutput, 0.1);
    e.run(100);
    claim(&mut e, RelicKind::CrystalOutput, 0.15);
    let list = held(&e);
    assert_eq!(list.len(), 1);
    assert_eq!((list[0].pct, list[0].claimed_tick), (0.15, e.engine.current_tick()));
    assert!((bonus(&e).crystal - 0.15).abs() < FRACTION, "refreshed to the new, not 0.25");

    let mark = e.mark();
    e.run(10);
    claim(&mut e, RelicKind::CrystalOutput, 0.05);
    let list = held(&e);
    assert_eq!((list.len(), list[0].pct), (1, 0.15), "a weaker find leaves the held one alone");
    let applied: Vec<bool> = e.since(mark, |k| match k {
        EventKind::RelicClaimed(c) => Some(c.applied),
        _ => None,
    });
    assert_eq!(applied, vec![false]);
    assert!(e.fleet().relic.is_none(), "the item is spent");
}

#[test]
fn a_family_of_kinds_shares_the_cap() {
    let mut e = empire();
    for kind in [RelicKind::MetalOutput, RelicKind::CrystalOutput, RelicKind::DeuteriumOutput] {
        claim(&mut e, kind, 0.25);
    }
    let b = bonus(&e);
    let sum = b.metal + b.crystal + b.deuterium;
    assert!((sum - RELIC_TOTAL_CAP).abs() < FRACTION, "{sum}");
    assert!((b.metal - b.crystal).abs() < FRACTION && (b.crystal - b.deuterium).abs() < FRACTION);

    claim(&mut e, RelicKind::FuelBurn, 0.25);
    assert!((bonus(&e).fuel - 0.25).abs() < FRACTION, "another family is not held to this one's cap");
}

#[test]
fn a_fleet_that_dies_carrying_a_relic_loses_it() {
    let mut e = empire();
    let (pid, fid) = (e.pid, e.fid);
    let lair = lair(&e.engine, 15, |_| true);
    edit(&mut e.engine, |s| {
        let fleet = s.fleets.get_mut(&fid).unwrap();
        fleet.ships = Ships::of(&[(ShipClass::LightFighter, 1)]);
        fleet.fuel_max = 1e6;
        fleet.fuel = 1e6;
        fleet.relic = Some(RelicItem { kind: RelicKind::BuildingTime, pct: 0.2 });
    });
    deploy(&mut e.engine, fid, lair, None);
    let target = iac_sim::engine::template_npc_id(lair);
    e.order(Command::Attack { fleet_id: fid, target_fleet_id: target }).expect("it attacks");
    e.run_until(30, |e| e.engine.fleets.get(&fid).is_none_or(|f| f.ships.is_empty()));
    e.run(2);
    assert!(e.engine.fleets.values().filter(|f| f.owner_id == pid).all(|f| f.relic.is_none()));
    assert!(held(&e).is_empty(), "a relic lost is never claimed");
    assert_eq!(bonus(&e), RelicBonus::NONE);
}

#[test]
fn a_merge_keeps_the_stronger_and_a_split_leaves_the_relic_with_the_original() {
    let mut e = empire();
    let (pid, fid) = (e.pid, e.fid);
    edit(&mut e.engine, |s| {
        s.fleets.get_mut(&fid).unwrap().ships = Ships::of(&[(ShipClass::LightFighter, 10), (ShipClass::SmallCargo, 4)]);
    });
    e.order(Command::Split { fleet_id: fid, ships: counts(&[(ShipClass::SmallCargo, 4)]), damaged_first: false })
        .expect("a split at home");
    let other = e.engine.fleets.values().find(|f| f.owner_id == pid && f.id != fid).unwrap().id;
    edit(&mut e.engine, |s| {
        s.fleets.get_mut(&fid).unwrap().relic = Some(RelicItem { kind: RelicKind::FuelBurn, pct: 0.1 });
        s.fleets.get_mut(&other).unwrap().relic = Some(RelicItem { kind: RelicKind::ResearchTime, pct: 0.2 });
    });
    e.order(Command::Merge { fleet_id: fid, other_fleet_id: other }).expect("a merge at home");
    let kept = e.engine.fleets[&fid].relic.expect("one is kept");
    assert_eq!((kept.kind, kept.pct), (RelicKind::ResearchTime, 0.2));

    e.order(Command::Split { fleet_id: fid, ships: counts(&[(ShipClass::SmallCargo, 4)]), damaged_first: false })
        .expect("a split at home");
    let new = e.engine.fleets.values().find(|f| f.owner_id == pid && f.id != fid).unwrap();
    assert!(new.relic.is_none());
    assert!(e.engine.fleets[&fid].relic.is_some());
}

#[test]
fn a_relic_moves_no_score() {
    let mut boosted = empire();
    let mut plain = empire();
    claim(&mut boosted, RelicKind::BuildingTime, 0.2);
    claim(&mut boosted, RelicKind::MetalOutput, 0.2);
    edit(&mut plain.engine, |s| {
        let fleet = s.fleets.get_mut(&plain.fid).unwrap();
        fleet.ships = Ships::of(&[(ShipClass::LightFighter, 40)]);
    });
    let (a, b) = (boosted.score_row(), plain.score_row());
    assert_eq!((a.score, a.buildings, a.research, a.fleet, a.outpost, a.defence), (b.score, b.buildings, b.research, b.fleet, b.outpost, b.defence));
    boosted.run(300);
    plain.run(300);
    assert!(boosted.stockpile().metal > plain.stockpile().metal, "the relic is working");
    let (a, b) = (boosted.score_row(), plain.score_row());
    assert_eq!(a.score, b.score);
    assert_eq!((a.buildings, a.research, a.fleet), (b.buildings, b.research, b.fleet));
}

#[test]
fn a_fuel_relic_cuts_the_burn_of_every_jump() {
    let mut e = empire();
    let fid = e.fid;
    edit(&mut e.engine, |s| s.fleets.get_mut(&fid).unwrap().ships = Ships::of(&[(ShipClass::LightFighter, 40)]));
    let before = e.engine.hop_fuel_cost(&e.engine.fleets[&e.fid]);
    claim(&mut e, RelicKind::FuelBurn, 0.2);
    let after = e.engine.hop_fuel_cost(&e.engine.fleets[&e.fid]);
    let ships = &e.engine.fleets[&e.fid].ships;
    assert!(ships.total() > 0);
    let reference = {
        let research = &e.engine.players[&e.pid].research;
        ships.counts().fold(0.0, |burn, (class, count)| burn + f64::from(count) * scaling::ship_burn(class, research))
            * scaling::fuel_rate_modifier(research.energy_technology, &RelicBonus::NONE)
    };
    assert!((after - reference * 0.8).abs() < 1e-9, "{after} against {}", reference * 0.8);
    assert!(before > after);
    assert_eq!(e.fleet().jump_fuel, after);
}

#[test]
fn a_research_relic_shortens_what_is_started_after_it() {
    let mut plain = empire();
    let mut boosted = empire();
    for e in [&mut plain, &mut boosted] {
        e.set_stockpile(iac_shared::constants::Resources { metal: 1e6, crystal: 1e6, deuterium: 1e6 });
        edit(&mut e.engine, |s| s.players.get_mut(&e.pid).unwrap().buildings.research_lab = 3);
    }
    claim(&mut boosted, RelicKind::ResearchTime, 0.2);
    for e in [&mut plain, &mut boosted] {
        let mark = e.mark();
        e.order(Command::Research { tech: ResearchType::EnergyTechnology, reserve: false }).expect("research starts");
        e.run(1);
        assert!(!e.queue_events(mark).is_empty());
    }
    let span = |e: &Empire| {
        let q = e.engine.players[&e.pid].research_queue.as_ref().expect("running");
        q.end_tick - q.start_tick
    };
    let (long, short) = (span(&plain), span(&boosted));
    let buildings = &boosted.engine.players[&boosted.pid].buildings;
    let relics = RelicBonus { research: 0.2, ..RelicBonus::NONE };
    let expected = scaling::research_time(ResearchType::EnergyTechnology, 1, buildings, &relics, &boosted.engine.pace());
    assert_eq!(short, expected);
    assert!(short < long && short.abs_diff(long * 4 / 5) <= 1, "{short} of {long}");
}

#[test]
fn a_world_stopped_and_started_is_the_world_played_straight_through() {
    let build = || {
        let mut e = empire();
        e.set_stockpile(iac_shared::constants::Resources { metal: 1e6, crystal: 1e6, deuterium: 1e6 });
        claim(&mut e, RelicKind::MetalOutput, 0.2);
        claim(&mut e, RelicKind::ResearchTime, 0.1);
        edit(&mut e.engine, |s| s.fleets.get_mut(&e.fid).unwrap().relic = Some(RelicItem { kind: RelicKind::FuelBurn, pct: 0.15 }));
        e
    };
    let mut straight = build();
    let mut stopped = build();
    let span = fade_span(&straight);
    let stops = [1, span / 4, span / 2, span - 1, span, span + 1];
    let mut at = 0;
    for stop in stops {
        straight.run(stop - at);
        stopped.run(stop - at);
        at = stop;
        stopped.engine = GameEngine::restore(stored(&stopped.engine));
        let (a, b) = (straight.engine.snapshot(), stopped.engine.snapshot());
        assert_eq!(serde_json::to_string(&a).unwrap(), serde_json::to_string(&b).unwrap(), "stopped at {stop} after the claim");
    }
    assert!(held(&straight).is_empty());
}
