// A world is stored whole, as one document (`iac_sim::store`), and a host
// that stops comes back as its last document says. This plays one busy
// world twice: straight through, and stopped and started again through its
// document, at fixed intervals and at the moments most likely to lose
// something (a fight between two of its rounds; an announced raid, a live
// scan and a jump at once; a boarding; an outpost being built, or warned of
// a raid). The two must be the same world at every stop and at the end.
// A second world does the same to a fight of large fleets, stopped between
// every two rounds, a third to a flight and the pursuit that follows it,
// stopped after every tick, and a fourth to a raid fought at a homeworld,
// stopped from its warning to the tick after it is settled.

#[allow(dead_code)]
mod common;

use std::collections::{HashMap, VecDeque};

use common::{stored_in, Memory};
use iac_shared::constants::{ResourceKind, ShipClass, TerrainType, DEFAULT_WORLD_SEED};
use iac_shared::hex::Hex;
use iac_shared::pace::Pace;
use iac_shared::protocol::{Command, EventKind, HarvestResource, PolicyPreset, QueueType, ShipCount};
use iac_shared::scaling::{BuildingType, DefenceKind, ResearchType};
use iac_shared::world::NpcBehaviorType;
use iac_sim::engine::{template_npc_id, FleetStatus};
use iac_sim::host::{self, Cast};
use iac_sim::{GameEngine, Snapshot};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

const TICKS: u64 = 2_000;
const PLAYERS: usize = 4;
const RESTART_EVERY: u64 = 250;
const LOOK_EVERY: u64 = 25;

/// A fifth empire that plays no random orders: it carries an outpost to the
/// Outer Ring, stocks it over further trips and raises it a level.
struct Settler {
    pid: u64,
    fleet: u64,
    home: Hex,
    site: Hex,
    /// Lane hops from home to the site, the site last, by sectors where nothing attacks on sight.
    way: Vec<Hex>,
}

impl Settler {
    fn join(engine: &mut GameEngine) -> Settler {
        let pid = engine.authenticate("Settler", None, false, || "f".repeat(64)).unwrap().player_id;
        let fleet = engine.fleets.values().find(|f| f.owner_id == pid).unwrap().id;
        let home = engine.players[&pid].homeworld;
        let calm = |at: Hex| {
            engine.world_gen.generate_sector(at).npc_template.is_none_or(|npc| npc.behavior == NpcBehaviorType::Passive)
        };
        let mut came_from = HashMap::from([(home.to_key(), home)]);
        let mut frontier = VecDeque::from([home]);
        let mut site = None;
        while let Some(at) = frontier.pop_front() {
            let t = engine.world_gen.generate_sector(at);
            let open = t.npc_template.is_none() && !t.has_derelict && t.terrain != TerrainType::Anomaly;
            if at.dist_from_origin() >= 9 && open {
                site = Some(at);
                break;
            }
            for &next in engine.world_gen.connected_neighbors(at).slice() {
                if calm(next) && !came_from.contains_key(&next.to_key()) {
                    came_from.insert(next.to_key(), at);
                    frontier.push_back(next);
                }
            }
        }
        let site = site.expect("an outpost site reachable by calm lanes");
        let mut way = vec![site];
        while came_from[&way[way.len() - 1].to_key()] != home {
            way.push(came_from[&way[way.len() - 1].to_key()]);
        }
        way.reverse();
        Settler { pid, fleet, home, site, way }
    }

    /// The settler's order for this tick, from what the engine holds.
    fn play(&self, engine: &mut GameEngine) {
        let (pid, fleet_id) = (self.pid, self.fleet);
        let stock = &mut engine.players.get_mut(&pid).unwrap().resources;
        (stock.metal, stock.crystal, stock.deuterium) = (stock.metal.max(27_000.0), stock.crystal.max(15_000.0), stock.deuterium.max(9000.0));
        let Some(fleet) = engine.fleets.get(&fleet_id) else { return };
        if !matches!(fleet.state, FleetStatus::Idle | FleetStatus::Docked) || fleet.action_cooldown > 0 {
            return;
        }
        let (at, laden) = (fleet.location, fleet.cargo.total() > 0.0);
        let hop_toward = |goal: Hex| {
            let place = self.way.iter().position(|hop| *hop == at);
            if goal == self.site {
                self.way[place.map_or(0, |i| i + 1)]
            } else {
                place.and_then(|i| i.checked_sub(1)).map_or(self.home, |i| self.way[i])
            }
        };
        if at == self.home && !laden {
            for (resource, amount) in [(ResourceKind::Metal, 4500.0), (ResourceKind::Crystal, 2400.0), (ResourceKind::Deuterium, 1200.0)] {
                let _ = engine.execute(pid, Command::Load { fleet_id, resource, amount: Some(amount) });
            }
        } else if at == self.site {
            let _ = engine.execute(pid, Command::FoundOutpost { fleet_id });
            let _ = engine.execute(pid, Command::Unload { fleet_id });
            let _ = engine.execute(pid, Command::UpgradeOutpost);
            if engine.outposts.get(&pid).is_some_and(|o| o.level >= 1) {
                let _ = engine.execute(pid, Command::Move { fleet_id, target: hop_toward(self.home) });
            }
        } else {
            let goal = if laden { self.site } else { self.home };
            let _ = engine.execute(pid, Command::Move { fleet_id, target: hop_toward(goal) });
        }
    }
}

/// Those who play the world: the settler, then four pilots whose orders are
/// a function of the tick and of what the engine holds, so two engines in
/// the same state are played the same.
struct Crew {
    settler: Settler,
    pilots: Vec<u64>,
}

impl Crew {
    /// The pilots begin with a yard that builds Light Fighters and draws
    /// raids, so that neither waits on the luck of their random orders.
    fn join(engine: &mut GameEngine) -> Crew {
        let pilots: Vec<u64> = (0..PLAYERS)
            .map(|i| engine.authenticate(&format!("Pilot{i}"), None, i % 2 == 0, || "f".repeat(64)).unwrap().player_id)
            .collect();
        for pid in &pilots {
            let player = engine.players.get_mut(pid).unwrap();
            (player.buildings.robotics_factory, player.buildings.shipyard) = (2, 2);
            player.research.combustion_drive = 1;
        }
        Crew { settler: Settler::join(engine), pilots }
    }
}

impl Cast for Crew {
    fn take_turns(&mut self, engine: &mut GameEngine) {
        self.settler.play(engine);
        let tick = engine.current_tick() + 1;
        let mut rng = StdRng::seed_from_u64(tick.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        if tick.is_multiple_of(300) {
            for pid in &self.pilots {
                let p = engine.players.get_mut(pid).unwrap();
                p.resources.metal = p.resources.metal.max(30_000.0);
                p.resources.crystal = p.resources.crystal.max(20_000.0);
                p.resources.deuterium = p.resources.deuterium.max(10_000.0);
            }
        }
        if !tick.is_multiple_of(4) {
            return;
        }
        for &pid in &self.pilots {
            let fleets: Vec<u64> = engine.fleets.values().filter(|f| f.owner_id == pid).map(|f| f.id).collect();
            let pick = |rng: &mut StdRng| if fleets.is_empty() { 0 } else { fleets[rng.random_range(0..fleets.len())] };
            let fleet_id = pick(&mut rng);
            let here = engine.fleets.get(&fleet_id).map(|f| f.location);
            if tick.is_multiple_of(200) {
                let preset = PolicyPreset::ALL[rng.random_range(0..PolicyPreset::ALL.len())];
                let _ = engine.handle_policy_update(pid, fleet_id, preset, None);
                continue;
            }
            let cmd = match rng.random_range(0..19) {
                0..=3 => here.map(|at| {
                    let around = engine.world_gen.connected_neighbors(at);
                    let n = around.slice();
                    Command::Move { fleet_id, target: n[rng.random_range(0..n.len())] }
                }),
                4 => Some(Command::Harvest { fleet_id, resource: HarvestResource::Auto }),
                5 | 6 => Some(Command::Scan { fleet_id }),
                7 => Some(Command::Build {
                    building_type: BuildingType::from_usize(rng.random_range(0..BuildingType::COUNT)).unwrap(),
                    reserve: rng.random_bool(0.3),
                }),
                8 => Some(Command::Research {
                    tech: ResearchType::from_usize(rng.random_range(0..ResearchType::COUNT)).unwrap(),
                    reserve: false,
                }),
                9 => Some(Command::BuildShip { ship_class: ShipClass::LightFighter, count: 2 }),
                10 => Some(Command::BuildDefence { kind: DefenceKind::RocketLauncher, count: 2 }),
                11 => here.map(|at| Command::Attack { fleet_id, target_fleet_id: template_npc_id(at) }),
                12 => Some(Command::Recall { fleet_id }),
                13 => Some(Command::ExploreSite { fleet_id }),
                14 => engine.fleets.get(&fleet_id).and_then(|f| f.ships.groups().first()).map(|group| Command::Split {
                    fleet_id,
                    ships: vec![ShipCount { ship_class: group.ship_class, count: 1 }],
                    damaged_first: tick.is_multiple_of(8),
                }),
                15 => Some(Command::Merge { fleet_id, other_fleet_id: pick(&mut rng) }),
                16 => Some(Command::Load { fleet_id, resource: ResourceKind::ALL[rng.random_range(0..3)], amount: None }),
                17 => Some(Command::Unload { fleet_id }),
                _ => Some(Command::CollectSalvage { fleet_id }),
            };
            if let Some(cmd) = cmd {
                let _ = engine.execute(pid, cmd);
            }
            if tick.is_multiple_of(40) {
                let _ = engine.execute(pid, Command::CancelBuild { queue_type: QueueType::Building, id: 0 });
            }
        }
    }

    fn observe(&mut self, _engine: &GameEngine) {}
}

fn tick(engine: &mut GameEngine, crew: &mut Crew) -> Vec<EventKind> {
    host::step(engine, crew, &mut (), |_, _| {}).unwrap().into_iter().map(|event| event.kind).collect()
}

/// Where two JSON trees first differ. Leaves are compared as written, so
/// 0.0 and -0.0 differ here as they do in the hash.
fn first_difference(a: &serde_json::Value, b: &serde_json::Value, at: &str) -> Option<String> {
    use serde_json::Value::{Array, Object};
    match (a, b) {
        (Array(x), Array(y)) if x.len() == y.len() => {
            x.iter().zip(y).enumerate().find_map(|(i, (x, y))| first_difference(x, y, &format!("{at}[{i}]")))
        }
        (Object(x), Object(y)) if x.keys().eq(y.keys()) => {
            x.iter().find_map(|(k, v)| first_difference(v, &y[k], &format!("{at}.{k}")))
        }
        _ => {
            let (engine, document) = (a.to_string(), b.to_string());
            (engine != document).then(|| format!("{at}: {engine} in the engine, {document} in its document"))
        }
    }
}

fn assert_same(engine: &Snapshot, stored: &Snapshot, when: &str) {
    let (a, b) = (serde_json::to_value(engine).unwrap(), serde_json::to_value(stored).unwrap());
    if let Some(difference) = first_difference(&a, &b, "world") {
        panic!("{when}: {difference}");
    }
    assert_eq!(engine.hash(), stored.hash(), "{when}");
}

/// What the restarts of a run interrupted.
#[derive(Debug, Default)]
struct Caught {
    fights: u32,
    announced_raids: u32,
    scans: u32,
    jumps: u32,
    boardings: u32,
    all_at_once: u32,
    outpost_builds: u32,
    outpost_warnings: u32,
}

#[test]
fn a_world_restarted_through_its_document_continues_as_if_it_had_never_stopped() {
    let pace = Pace::parse("test").unwrap();
    let mut straight = GameEngine::new(DEFAULT_WORLD_SEED, pace);
    let mut stopped = GameEngine::new(DEFAULT_WORLD_SEED, pace);
    let mut crew = Crew::join(&mut straight);
    let mut stopped_crew = Crew::join(&mut stopped);
    assert_eq!((&crew.pilots, crew.settler.pid), (&stopped_crew.pilots, stopped_crew.settler.pid));
    let (pilots, settler, site) = (crew.pilots.clone(), crew.settler.pid, crew.settler.site);
    let mut place = Memory::default();

    let mut caught = Caught::default();
    let mut raids_announced = 0u32;
    let mut last_restart = 0u64;
    let (mut raids, mut doctrine_moves, mut laden_holds) = (0u32, 0u32, 0u32);
    let (mut outpost_raids, mut stored_outposts) = (0u32, 0u32);
    for _ in 0..TICKS {
        tick(&mut straight, &mut crew);
        for event in tick(&mut stopped, &mut stopped_crew) {
            match event {
                EventKind::RaidIncoming(r) if r.outpost.is_some() => {}
                EventKind::RaidResolved(r) if r.outpost.is_some() => outpost_raids += 1,
                EventKind::RaidIncoming(_) => raids_announced += 1,
                EventKind::RaidResolved(_) => {
                    raids_announced -= 1;
                    raids += 1;
                }
                EventKind::PolicyAction(p) if p.action == "move" => doctrine_moves += 1,
                _ => {}
            }
        }
        let now = stopped.current_tick();

        if now.is_multiple_of(LOOK_EVERY) {
            let world = stored_in(&mut place, &stopped);
            laden_holds += u32::from(world.fleets.values().any(|f| f.laden && f.cargo.total() > 0.0));
            stored_outposts += u32::from(world.outposts.get(&settler).is_some_and(|o| o.store.total() > 0.0));
            assert_same(&stopped.snapshot(), &world, &format!("stored after tick {now}"));
        }

        let fight = !stopped.active_combats.is_empty();
        let raid = raids_announced > 0;
        let scan = pilots.iter().any(|&pid| !stopped.scan_revealed_coords(pid).is_empty());
        let jump = stopped.fleets.values().any(|f| f.state == FleetStatus::Moving);
        let boarding = stopped.fleets.values().any(|f| f.state == FleetStatus::Exploring);
        let outpost = stopped.outposts.get(&settler);
        let outpost_build = outpost.is_some_and(|o| o.job.is_some());
        let outpost_warning = outpost.is_some_and(|o| o.raid.incoming.is_some());
        let rare = (fight && caught.fights < 6)
            || (raid && scan && jump && caught.all_at_once < 3)
            || (boarding && caught.boardings < 3)
            || (outpost_build && caught.outpost_builds < 3)
            || (outpost_warning && caught.outpost_warnings < 3);
        if now.is_multiple_of(RESTART_EVERY) || (rare && now > last_restart + 5) {
            last_restart = now;
            caught.fights += u32::from(fight);
            caught.announced_raids += u32::from(raid);
            caught.scans += u32::from(scan);
            caught.jumps += u32::from(jump);
            caught.boardings += u32::from(boarding);
            caught.all_at_once += u32::from(raid && scan && jump);
            caught.outpost_builds += u32::from(outpost_build);
            caught.outpost_warnings += u32::from(outpost_warning);

            let before = stopped.snapshot();
            let world = stored_in(&mut place, &stopped);
            assert_same(&before, &world, &format!("stopped after tick {now}"));
            assert_eq!(world.hash(), straight.snapshot().hash(), "the restarted world left the other by tick {now}");
            stopped = GameEngine::restore(world);
        }
    }

    assert_eq!(stopped.snapshot().hash(), straight.snapshot().hash(), "restarts changed the course of the world");
    assert!(
        caught.fights > 0 && caught.all_at_once > 0 && caught.boardings > 0,
        "no restart interrupted a fight, or none an announced raid, a scan and a jump together, or none a boarding \
         ({raids} raids resolved): {caught:?}",
    );
    assert!(raids > 0 && doctrine_moves > 0, "{raids} raids resolved, {doctrine_moves} moves by standing orders");
    assert!(laden_holds > 0, "no document held a hold loaded at the dock");
    let outpost = stopped.outposts.get(&settler).expect("the settler founded an outpost");
    assert!(
        outpost.sector == site && outpost.level >= 2,
        "the outpost was to be stocked over several trips and raised a level: it stands at level {}",
        outpost.level,
    );
    assert!(
        caught.outpost_builds > 0 && caught.outpost_warnings > 0 && outpost_raids > 0,
        "no restart fell inside an outpost's build, or none inside the warning of a raid on it ({outpost_raids} resolved): {caught:?}",
    );
    assert!(stored_outposts > 0, "no document held a stocked outpost");
}

/// Two thousand ships against two thousand: the host stops after every
/// round and starts again from its document. What a fight needs between
/// two ticks is the fleets' counts and the hull of their damaged ships, and
/// the dice are drawn from the world's seed, the tick, the sector and the
/// round, so the next round picks the same targets.
#[test]
fn a_fight_of_large_fleets_stopped_between_every_two_rounds_is_fought_as_if_it_had_never_stopped() {
    use ShipClass::{Cruiser, HeavyFighter, LightFighter};
    const HORDE: u64 = 900_001;
    let muster = || {
        let mut engine = GameEngine::new(DEFAULT_WORLD_SEED, Pace::parse("persistent").unwrap());
        let (pid, fid) = common::join(&mut engine, "Admiral");
        let home = engine.players[&pid].homeworld;
        let at = common::outward_chain(&engine, home, 2)[1];
        common::muster(&mut engine, fid, &[(LightFighter, 1500), (HeavyFighter, 300), (Cruiser, 200)], at);
        common::horde(&mut engine, HORDE, at, &[(LightFighter, 2000), (Cruiser, 150)], 1.0);
        let attack = engine.execute(pid, Command::Attack { fleet_id: fid, target_fleet_id: HORDE });
        assert!(attack.error.is_none(), "{:?}", attack.error);
        (engine, fid)
    };
    let (mut straight, fid) = muster();
    let (mut stopped, _) = muster();
    let mut place = Memory::default();

    let (mut rounds, mut damaged_most, mut largest) = (0u32, 0u32, 0usize);
    while !stopped.active_combats.is_empty() {
        straight.tick().unwrap();
        stopped.tick().unwrap();
        let (told, told_stopped) = (straight.drain_events(), stopped.drain_events());
        assert_eq!(
            serde_json::to_value(&told).unwrap(),
            serde_json::to_value(&told_stopped).unwrap(),
            "round {}: the restarted world reported another round",
            rounds + 1,
        );
        rounds += told.iter().filter(|e| matches!(e.kind, EventKind::BattleRound(_))).count() as u32;
        assert!(rounds < 200, "the fight does not end");

        largest = largest.max(iac_sim::store::encode(&(), &stopped.snapshot()).len());
        let world = stored_in(&mut place, &stopped);
        assert_same(&stopped.snapshot(), &world, &format!("stored after round {rounds}"));
        assert_eq!(world.hash(), straight.snapshot().hash(), "the restarted world left the other by round {rounds}");
        let wounded = world.fleets.get(&fid).map_or(0, |f| f.ships.damaged()) + world.npc_fleets.get(&HORDE).map_or(0, |n| n.ships.damaged());
        damaged_most = damaged_most.max(wounded);
        if let Some(fight) = world.combats.values().next() {
            assert_eq!(fight.round, rounds, "the document holds the fight's count of rounds");
            assert!(fight.last_round.is_some(), "and its last round, for a client that arrives now");
        }
        stopped = GameEngine::restore(world);
    }

    assert_eq!(stopped.snapshot().hash(), straight.snapshot().hash(), "restarts changed the course of the fight");
    assert!(rounds >= 4, "a fight of thousands lasts: {rounds} rounds");
    assert!(damaged_most > 200, "the documents held the hull of hundreds of damaged ships: {damaged_most}");
    assert!(largest < 64 * 1024, "and a world with four thousand ships in a fight is {largest} bytes");
}

/// A convoy slower than the pack it leaves runs four hexes down a lane and
/// is caught at two of them, and the host stops after every tick: in the
/// middle of the jump out, with the pack on its trail, with the pack firing
/// on a fleet that is charging, with the pack on its way back, and while it
/// mends. Who is leaving is the fleet's own state, and the trail, the
/// leash and the mending are the group's, so the document holds them all.
#[test]
fn a_flight_and_a_pursuit_stopped_after_every_tick_are_flown_as_if_the_host_had_never_stopped() {
    use ShipClass::{LightFighter, SmallCargo};
    const PACK: u64 = 900_002;
    let stage = || {
        let mut engine = GameEngine::new(DEFAULT_WORLD_SEED, Pace::parse("season").unwrap());
        let (pid, fid) = common::join(&mut engine, "Convoy");
        let home = engine.players[&pid].homeworld;
        let lane = common::open_lane(&engine, home, 5);
        common::muster(&mut engine, fid, &[(SmallCargo, 300)], lane[0]);
        common::horde(&mut engine, PACK, lane[0], &[(LightFighter, 160)], 1.0);
        common::edit(&mut engine, |s| s.npc_fleets.get_mut(&PACK).unwrap().behavior = NpcBehaviorType::Aggressive);
        let attack = engine.execute(pid, Command::Attack { fleet_id: fid, target_fleet_id: PACK });
        assert!(attack.error.is_none(), "{:?}", attack.error);
        (engine, pid, fid, lane)
    };
    let run_on = |engine: &mut GameEngine, pid: u64, fid: u64, lane: &[Hex]| {
        let fleet = &engine.fleets[&fid];
        let next = lane.iter().position(|at| *at == fleet.location).and_then(|here| lane.get(here + 1)).copied();
        if let (Some(target), true) = (next, fleet.state != FleetStatus::Moving) {
            let order = engine.execute(pid, Command::Move { fleet_id: fid, target });
            assert!(order.error.is_none(), "{:?}", order.error);
        }
    };
    let (mut straight, pid, fid, lane) = stage();
    let (mut stopped, ..) = stage();
    let mut place = Memory::default();

    let (mut fleeing, mut hunted, mut caught, mut returning, mut mending) = (0u32, 0u32, 0u32, 0u32, 0u32);
    let mut whole_at = None;
    for tick in 1..=400u64 {
        if tick > 1 {
            run_on(&mut straight, pid, fid, &lane);
            run_on(&mut stopped, pid, fid, &lane);
        }
        straight.tick().unwrap();
        stopped.tick().unwrap();
        assert_eq!(
            serde_json::to_value(straight.drain_events()).unwrap(),
            serde_json::to_value(stopped.drain_events()).unwrap(),
            "tick {tick}: the restarted world told another story",
        );

        let world = stored_in(&mut place, &stopped);
        assert_same(&stopped.snapshot(), &world, &format!("stored after tick {tick}"));
        assert_eq!(world.hash(), straight.snapshot().hash(), "the restarted world left the other by tick {tick}");
        let fleet = &world.fleets[&fid];
        let leaving = fleet.state == FleetStatus::Moving && world.combats.values().any(|fight| fight.player_fleet_ids.contains(&fid));
        let pack = world.npc_fleets.get(&PACK);
        let errand = pack.and_then(|pack| pack.errand);
        fleeing += u32::from(leaving && pack.is_some_and(|pack| pack.strayed == 0));
        caught += u32::from(leaving && pack.is_some_and(|pack| pack.strayed > 0));
        hunted += u32::from(errand.is_some_and(|errand| errand.quarry == Some(fid)));
        returning += u32::from(errand.is_some_and(|errand| errand.quarry.is_none()));
        let resting = pack.is_some_and(|pack| !pack.in_combat && pack.errand.is_none() && pack.location == lane[0] && !pack.whole());
        mending += u32::from(resting && pack.is_some_and(|pack| pack.rested > 0));
        stopped = GameEngine::restore(world);
        if resting {
            whole_at = whole_at.or(stopped.npc_fleets.get(&PACK).and_then(|pack| stopped.group_whole_at(pack)));
        }
    }

    assert_eq!(stopped.snapshot().hash(), straight.snapshot().hash(), "restarts changed the course of the flight");
    assert_eq!(stopped.fleets[&fid].location, lane[4], "the convoy ran the lane");
    assert!(
        fleeing >= 3 && hunted >= 4 && caught >= 4 && returning >= 4 && mending >= 10,
        "the host was to stop in the jump out ({fleeing}), with the pack on the trail ({hunted}), with it firing on a \
         charging fleet ({caught}), on its way back ({returning}) and while it mended ({mending})",
    );
    let whole_at = whole_at.expect("the pack came back wounded");
    assert!(whole_at <= 400, "in a season a pack a convoy scratched is whole within the run: tick {whole_at}");
    assert!(!stopped.npc_fleets.contains_key(&PACK), "and whole again it has stood down");
}

/// A raid on a homeworld with a docked fleet, Solar Satellites, a dome and
/// two kinds of defence, and the host stops after every tick: during the
/// warning, on the tick the raid arrives, between every two rounds and
/// after the fight is settled. The raiders are no group of the map: what
/// they are, the hull of their damaged ships and of the damaged defences
/// are the fight's own, so the document holds them with the fight.
#[test]
fn a_raid_stopped_after_every_round_is_fought_as_if_the_host_had_never_stopped() {
    use DefenceKind::{IonCannon, LargeShieldDome, RocketLauncher};
    use ShipClass::{Cruiser, LightFighter};
    let stage = || {
        let mut engine = GameEngine::new(DEFAULT_WORLD_SEED, Pace::parse("season").unwrap());
        let (pid, fid) = common::join(&mut engine, "Warden");
        let home = engine.players[&pid].homeworld;
        common::muster(&mut engine, fid, &[(Cruiser, 12), (LightFighter, 40)], home);
        common::fortify(&mut engine, pid, &[(LargeShieldDome, 1), (IonCannon, 30), (RocketLauncher, 60)], 25);
        common::send_raid(&mut engine, pid, &[(Cruiser, 40), (LightFighter, 120)], 1.0, 3);
        (engine, pid, fid)
    };
    let (mut straight, pid, fid) = stage();
    let (mut stopped, ..) = stage();
    let mut place = Memory::default();

    let (mut warned, mut between_rounds, mut damaged_most) = (0u32, 0u32, 0usize);
    let mut settled = None;
    for tick in 1..=14u64 {
        straight.tick().unwrap();
        stopped.tick().unwrap();
        let (told, told_stopped) = (straight.drain_events(), stopped.drain_events());
        assert_eq!(
            serde_json::to_value(&told).unwrap(),
            serde_json::to_value(&told_stopped).unwrap(),
            "tick {tick}: the restarted world told another story",
        );
        for event in told {
            if let EventKind::RaidResolved(raid) = event.kind {
                settled = Some(raid);
            }
        }

        let world = stored_in(&mut place, &stopped);
        assert_same(&stopped.snapshot(), &world, &format!("stored after tick {tick}"));
        assert_eq!(world.hash(), straight.snapshot().hash(), "the restarted world left the other by tick {tick}");
        warned += u32::from(world.raid_states[&pid].incoming.is_some());
        if let Some(fight) = world.combats.values().next() {
            let siege = fight.siege.as_ref().expect("the fight at home is the raid's");
            assert_eq!((siege.owner, fight.npc_fleet_ids.len()), (pid, 0), "its raiders are the fight's own");
            assert!(fight.player_fleet_ids.contains(&fid), "and the docked fleet is in it");
            between_rounds += 1;
            let hurt = siege.raiders.damaged() as usize
                + siege.battery.iter().map(|squad| squad.damaged.len()).sum::<usize>()
                + world.fleets[&fid].ships.damaged() as usize;
            damaged_most = damaged_most.max(hurt);
        }
        stopped = GameEngine::restore(world);
    }

    assert_eq!(stopped.snapshot().hash(), straight.snapshot().hash(), "restarts changed the course of the raid");
    let settled = settled.expect("the raid was settled within the run");
    assert!(settled.rounds >= 3, "the fight was to last: {} rounds", settled.rounds);
    assert!(
        warned >= 2 && between_rounds >= settled.rounds && damaged_most >= 20,
        "the host was to stop during the warning ({warned}), on the tick of the arrival and between the rounds \
         ({between_rounds} for {} rounds), with damage carried from round to round ({damaged_most} hulls)",
        settled.rounds,
    );
    assert!(stopped.active_combats.is_empty() && stopped.snapshot().raid_states[&pid].incoming.is_none());
}
