use std::collections::HashMap;

use iac_shared::constants::{Density, ShipClass};
use iac_shared::hex::Hex;
use iac_shared::pace::Pace;
use iac_shared::protocol::{
    Command, ErrorCode, EventKind, GameEvent, GameState, HarvestResource, QueueEvent, QueueType, SectorState,
    ServerMessage, ShipCount,
};
use iac_shared::scaling::{ship_figures, Arms, BuildingType, Refit, ResearchType};
use iac_shared::world::NpcBehaviorType;
use iac_sim::engine::template_npc_id;
use iac_sim::engine::{FleetStatus, NpcFleet, Ships};
use iac_sim::host::Cast;
use iac_sim::store::{self, Store};
use iac_sim::{views, GameEngine, Snapshot};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

pub const SEED: u64 = 42;

pub fn world(pace: &str) -> GameEngine {
    GameEngine::new(SEED, Pace::parse(pace).expect("known pace"))
}

pub fn join(engine: &mut GameEngine, name: &str) -> (u64, u64) {
    let pid = engine.register_player(name.to_string()).expect("register");
    let fid = engine
        .fleets
        .values()
        .find(|f| f.owner_id == pid)
        .map(|f| f.id)
        .expect("starting fleet");
    (pid, fid)
}

pub fn edit(engine: &mut GameEngine, change: impl FnOnce(&mut Snapshot)) {
    let mut snapshot = engine.snapshot();
    change(&mut snapshot);
    *engine = GameEngine::restore(snapshot);
}

/// A place that keeps a world's document in memory.
#[derive(Default)]
pub struct Memory(Option<Vec<u8>>);

impl Store for Memory {
    fn read(&mut self) -> Result<Option<Vec<u8>>, String> {
        Ok(self.0.clone())
    }

    fn replace(&mut self, document: &[u8]) -> Result<(), String> {
        self.0 = Some(document.to_vec());
        Ok(())
    }
}

/// What a host that stopped now would find when it started again: the
/// world's document, put in `place` and read back from it.
pub fn stored_in(place: &mut impl Store, engine: &GameEngine) -> Snapshot {
    place.replace(&store::encode(&(), &engine.snapshot())).expect("the place takes the document");
    let document = place.read().expect("the place is read").expect("the place holds a world");
    let ((), world) = store::decode(&document).unwrap_or_else(|unread| panic!("{}", unread.sentence("document")));
    world
}

pub fn stored(engine: &GameEngine) -> Snapshot {
    stored_in(&mut Memory::default(), engine)
}

pub fn run(engine: &mut GameEngine, ticks: u64) -> Vec<GameEvent> {
    let mut events = engine.drain_events();
    for _ in 0..ticks {
        engine.tick().expect("tick");
        events.extend(engine.drain_events());
    }
    events
}

pub fn run_until(engine: &mut GameEngine, limit: u64, mut done: impl FnMut(&GameEngine) -> bool) -> Vec<GameEvent> {
    let mut events = engine.drain_events();
    for _ in 0..limit {
        if done(engine) {
            return events;
        }
        engine.tick().expect("tick");
        events.extend(engine.drain_events());
    }
    panic!("condition not met within {limit} ticks");
}

pub fn chart(engine: &GameEngine, pid: u64) -> HashMap<u32, SectorState> {
    engine.known.chart(engine, pid).into_iter().map(|s| (s.location.to_key(), s)).collect()
}

pub fn find_sector(engine: &GameEngine, min_dist: u16, pred: impl Fn(Density, Density, Density) -> bool) -> Hex {
    for q in -30i16..30 {
        for r in -30i16..30 {
            let coord = Hex { q, r };
            let t = engine.world_gen.generate_sector(coord);
            if coord.dist_from_origin() >= min_dist && pred(t.metal_density, t.crystal_density, t.deut_density) {
                return coord;
            }
        }
    }
    panic!("no matching sector in a 60x60 window");
}

pub fn lone_fighter_sector(engine: &GameEngine, behavior: NpcBehaviorType) -> Hex {
    for q in -20i16..20 {
        for r in -20i16..20 {
            let coord = Hex { q, r };
            if let Some(t) = engine.world_gen.generate_sector(coord).npc_template
                && t.behavior == behavior
                && t.count == 1
                && t.ship_class == ShipClass::LightFighter
                && engine.derelict_site_at(coord).is_none()
            {
                return coord;
            }
        }
    }
    panic!("no such NPC sector in a 40x40 window");
}

/// Levels no empire reaches: a fleet stamped with them fires 26 times its
/// class's attack behind 26 times its hull and shield, and keeps them until
/// it docks at home.
pub const MIGHTY: Refit = Refit { weapons: 250, shielding: 250, armour: 250 };

pub fn deploy(engine: &mut GameEngine, fid: u64, at: Hex, refit: Option<Refit>) {
    edit(engine, |s| {
        let fleet = s.fleets.get_mut(&fid).expect("fleet");
        fleet.location = at;
        if let Some(refit) = refit {
            fleet.refit = refit;
        }
    });
}

/// Research `pid` to `MIGHTY` and stamp it on `fid`: might the home dock
/// leaves in place.
pub fn empower(engine: &mut GameEngine, pid: u64, fid: u64) {
    edit(engine, |s| {
        let research = &mut s.players.get_mut(&pid).expect("player").research;
        research.weapons_technology = MIGHTY.weapons;
        research.shielding_technology = MIGHTY.shielding;
        research.armour_technology = MIGHTY.armour;
        s.fleets.get_mut(&fid).expect("fleet").refit = MIGHTY;
    });
}

pub fn muster(engine: &mut GameEngine, fid: u64, classes: &[(ShipClass, u32)], at: Hex) {
    edit(engine, |s| {
        let fleet = s.fleets.get_mut(&fid).expect("fleet");
        fleet.ships = Ships::of(classes);
        fleet.fuel_max = 1e6;
        fleet.fuel = 1e6;
        fleet.location = at;
    });
}

/// `whole` ships of `class` and one more for each entry of `damaged`,
/// holding that much of the class's own hull.
pub fn squadron(class: ShipClass, whole: u32, damaged: &[u32]) -> Ships {
    let full = ship_figures(class, Arms::BASE).hull;
    let mut ships = Ships::of(&[(class, whole)]);
    for &hull in damaged {
        ships.add_holding(class, hull, full);
    }
    ships
}

/// So many ships of each class, as a `split` names them.
pub fn counts(ships: &[(ShipClass, u32)]) -> Vec<ShipCount> {
    ships.iter().map(|&(ship_class, count)| ShipCount { ship_class, count }).collect()
}

/// A hostile group of `ships` standing in `at`, every figure of its
/// classes at `grade`, under the id `id`: what `attack` is aimed at.
pub fn horde(engine: &mut GameEngine, id: u64, at: Hex, ships: &[(ShipClass, u32)], grade: f32) {
    edit(engine, |s| {
        let ships = Ships::of(ships);
        let arms = Arms::of_grade(grade);
        s.npc_fleets.insert(
            id,
            NpcFleet {
                id,
                location: at,
                power: ships.power(arms),
                muster: ships.clone(),
                ships,
                arms,
                behavior: NpcBehaviorType::Passive,
                home_sector: at,
                in_combat: false,
                bounty: iac_shared::constants::Resources::default(),
                label: "horde".to_string(),
                rested: 0,
                strayed: 0,
                errand: None,
            },
        );
    });
}

/// Hull the ships of `fid` hold between them; 0 for a fleet that is gone.
pub fn hull_held(engine: &GameEngine, fid: u64) -> u64 {
    engine.fleets.get(&fid).map_or(0, |fleet| fleet.ships.hull(fleet.arms()).0)
}

pub fn lair(engine: &GameEngine, ring: u16, wanted: impl Fn(NpcBehaviorType) -> bool) -> Hex {
    iac_shared::hex::hex_ring(Hex::ORIGIN, ring)
        .find(|at| {
            engine.world_gen.generate_sector(*at).npc_template.is_some_and(|t| wanted(t.behavior))
                && engine.derelict_site_at(*at).is_none()
        })
        .expect("such a lair in the ring")
}

pub fn sector_in_ring(engine: &GameEngine, ring: u16, wanted: impl Fn(Hex, &iac_shared::world::SectorTemplate) -> bool) -> Hex {
    iac_shared::hex::hex_ring(Hex::ORIGIN, ring)
        .find(|at| wanted(*at, &engine.world_gen.generate_sector(*at)))
        .unwrap_or_else(|| panic!("no such sector in ring {ring}"))
}

pub fn sweep(engine: &mut GameEngine, lanes: &[Hex]) {
    let tick = engine.current_tick();
    edit(engine, |s| {
        for at in lanes {
            s.sector_overrides.entry(at.to_key()).or_default().npc_cleared_tick = Some(tick);
        }
        s.npc_fleets.retain(|_, npc| !lanes.contains(&npc.location));
    });
}

pub struct Fights {
    pub fought: usize,
    pub won: usize,
    pub hull_kept: f32,
}

/// The same fight fought `fought` times over, each on other dice. A win is
/// a fight that destroyed the hostiles; `hull_kept` is the hull the fleet
/// held afterwards as a share of what it held before, lost fights counting 0.
pub fn fights(engine: &GameEngine, pid: u64, fid: u64, at: Hex, fought: usize) -> Fights {
    let before = hull_held(engine, fid) as f32;
    let (mut won, mut kept) = (0, 0.0);
    for round in 0..fought {
        let mut start = engine.snapshot();
        start.tick += 1 + 13 * round as u64;
        let mut world = GameEngine::restore(start);
        let outcome = world.execute(pid, Command::Attack { fleet_id: fid, target_fleet_id: template_npc_id(at) });
        assert!(outcome.error.is_none(), "{:?}", outcome.error);
        let events = run_until(&mut world, 5000, |e| e.active_combats.is_empty());
        let victory = events.iter().any(|e| matches!(&e.kind, EventKind::CombatEnded(c) if c.player_victory));
        won += usize::from(victory);
        kept += hull_held(&world, fid) as f32 / before;
    }
    Fights { fought, won, hull_kept: kept / fought as f32 }
}

pub fn fight(engine: &mut GameEngine, pid: u64, fid: u64, at: Hex) -> Vec<GameEvent> {
    deploy(engine, fid, at, Some(MIGHTY));
    let outcome = engine.execute(pid, Command::Attack { fleet_id: fid, target_fleet_id: template_npc_id(at) });
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    let events = run_until(engine, 10, |e| e.active_combats.is_empty());
    assert!(engine.active_combats.is_empty());
    events
}

pub fn pile_of(events: &[GameEvent]) -> (iac_shared::constants::Resources, u64) {
    events
        .iter()
        .find_map(|e| match &e.kind {
            EventKind::CombatEnded(c) => c.salvage.zip(c.salvage_despawn_tick),
            _ => None,
        })
        .expect("the fight left a pile")
}

pub fn lane_path(engine: &GameEngine, from: Hex, to: Hex, max_hops: usize) -> Option<Vec<Hex>> {
    let mut came_from = HashMap::from([(from.to_key(), from)]);
    let mut frontier = std::collections::VecDeque::from([(from, 0usize)]);
    while let Some((at, hops)) = frontier.pop_front() {
        if at == to {
            let mut path = vec![to];
            while *path.last().unwrap() != from {
                path.push(came_from[&path.last().unwrap().to_key()]);
            }
            path.reverse();
            path.remove(0);
            return Some(path);
        }
        if hops == max_hops {
            continue;
        }
        for &next in engine.world_gen.connected_neighbors(at).slice() {
            if let std::collections::hash_map::Entry::Vacant(slot) = came_from.entry(next.to_key()) {
                slot.insert(at);
                frontier.push_back((next, hops + 1));
            }
        }
    }
    None
}

pub struct Empire {
    pub engine: GameEngine,
    pub pid: u64,
    pub fid: u64,
    pub log: Vec<GameEvent>,
}

impl Empire {
    pub fn join(engine: GameEngine, name: &str) -> Empire {
        let mut engine = engine;
        let (pid, fid) = join(&mut engine, name);
        Empire { engine, pid, fid, log: Vec::new() }
    }

    pub fn absorb(&mut self) {
        let drained = self.engine.drain_events();
        let player = &self.engine.players[&self.pid];
        let seen: Vec<GameEvent> = drained.iter().filter_map(|e| views::event_for(e, player, &self.engine)).collect();
        self.log.extend(seen);
    }

    pub fn state(&self) -> GameState {
        views::full_state(&self.engine, self.pid).expect("player exists")
    }

    pub fn order(&mut self, cmd: Command) -> Result<(), ErrorCode> {
        let outcome = self.engine.execute(self.pid, cmd);
        self.absorb();
        match outcome.error {
            None => Ok(()),
            Some((code, _)) => Err(code),
        }
    }

    pub fn refusal(&mut self, cmd: Command) -> (ErrorCode, String) {
        let outcome = self.engine.execute(self.pid, cmd);
        self.absorb();
        outcome.error.expect("the command is refused")
    }

    pub fn stockpile(&self) -> iac_shared::constants::Resources {
        self.engine.players[&self.pid].resources
    }

    pub fn set_stockpile(&mut self, resources: iac_shared::constants::Resources) {
        let pid = self.pid;
        edit(&mut self.engine, |s| s.players.get_mut(&pid).unwrap().resources = resources);
    }

    pub fn run(&mut self, ticks: u64) {
        for _ in 0..ticks {
            self.engine.tick().expect("tick");
            self.absorb();
        }
    }

    pub fn run_until(&mut self, limit: u64, mut done: impl FnMut(&Empire) -> bool) {
        for _ in 0..limit {
            if done(self) {
                return;
            }
            self.run(1);
        }
        panic!("condition not met within {limit} ticks at tick {}", self.engine.current_tick());
    }

    pub fn mark(&self) -> usize {
        self.log.len()
    }

    pub fn since<T>(&self, mark: usize, pick: impl Fn(&EventKind) -> Option<T>) -> Vec<T> {
        self.log[mark..].iter().filter_map(|e| pick(&e.kind)).collect()
    }

    pub fn queue_events(&self, mark: usize) -> Vec<QueueEvent> {
        self.since(mark, |k| match k {
            EventKind::Queue(q) => Some(q.clone()),
            _ => None,
        })
    }

    pub fn fleet(&self) -> iac_shared::protocol::FleetState {
        self.state().fleets.into_iter().find(|f| f.id == self.fid).expect("main fleet")
    }

    pub fn travel(&mut self, to: Hex) {
        self.travel_fleet(self.fid, to);
    }

    pub fn travel_fleet(&mut self, fid: u64, to: Hex) {
        let from = self.engine.fleets[&fid].location;
        for hop in lane_path(&self.engine, from, to, 12).expect("a lane") {
            self.run_until(200, |e| {
                let f = &e.engine.fleets[&fid];
                matches!(f.state, FleetStatus::Docked) || (f.state == FleetStatus::Idle && f.action_cooldown == 0)
            });
            self.order(Command::Move { fleet_id: fid, target: hop }).expect("move along a lane");
            self.run_until(200, |e| e.engine.fleets[&fid].location == hop && e.engine.fleets[&fid].state != FleetStatus::Moving);
        }
    }

    pub fn score_row(&mut self) -> iac_shared::protocol::LeaderboardEntry {
        let outcome = self.engine.execute(self.pid, Command::Leaderboard { limit: 10 });
        let Some(ServerMessage::Leaderboard(reply)) = outcome.reply else { panic!("no leaderboard reply") };
        reply.entries.into_iter().find(|r| r.name == self.engine.players[&self.pid].name).expect("own row")
    }
}

pub fn outward_chain(engine: &GameEngine, home: Hex, len: usize) -> Vec<Hex> {
    fn extend(engine: &GameEngine, chain: &mut Vec<Hex>, len: usize, home: Hex) -> bool {
        if chain.len() == len {
            return true;
        }
        let tip = *chain.last().unwrap();
        for &next in engine.world_gen.connected_neighbors(tip).slice() {
            let quiet = engine.world_gen.generate_sector(next).npc_template.is_none();
            if usize::from(Hex::distance(&next, &home)) != chain.len() || !quiet {
                continue;
            }
            chain.push(next);
            if extend(engine, chain, len, home) {
                return true;
            }
            chain.pop();
        }
        false
    }
    let mut chain = vec![home];
    assert!(extend(engine, &mut chain, len + 1, home), "no straight {len}-hop lane from home");
    chain.split_off(1)
}

/// Accounts flown by a script that is a function of the tick and the
/// visible world, with no state of its own: the cast of a busy world.
pub struct Pilots {
    pub pids: Vec<u64>,
    /// Orders the engine took.
    pub accepted: u32,
}

impl Pilots {
    pub fn join(engine: &mut GameEngine, count: usize) -> Pilots {
        let pids = (0..count)
            .map(|i| engine.authenticate(&format!("Pilot{i}"), None, i % 2 == 0, || "x".repeat(64)).unwrap().player_id)
            .collect();
        Pilots { pids, accepted: 0 }
    }
}

impl Cast for Pilots {
    fn take_turns(&mut self, engine: &mut GameEngine) {
        let tick = engine.current_tick() + 1;
        let mut rng = StdRng::seed_from_u64(tick.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        if tick.is_multiple_of(300) {
            for pid in &self.pids {
                if let Some(p) = engine.players.get_mut(pid) {
                    p.resources.metal = p.resources.metal.max(30_000.0);
                    p.resources.crystal = p.resources.crystal.max(20_000.0);
                    p.resources.deuterium = p.resources.deuterium.max(10_000.0);
                }
            }
        }
        if !tick.is_multiple_of(4) {
            return;
        }
        for &pid in &self.pids {
            let mut fleets: Vec<u64> = engine.fleets.values().filter(|f| f.owner_id == pid).map(|f| f.id).collect();
            fleets.sort_unstable();
            let fleet_id = if fleets.is_empty() { 0 } else { fleets[rng.random_range(0..fleets.len())] };
            let here = engine.fleets.get(&fleet_id).map(|f| f.location);
            let cmd = match rng.random_range(0..12) {
                0..=3 => here.map(|at| {
                    let around = engine.world_gen.connected_neighbors(at);
                    let n = around.slice();
                    Command::Move { fleet_id, target: n[rng.random_range(0..n.len())] }
                }),
                4 => Some(Command::Harvest { fleet_id, resource: HarvestResource::Auto }),
                5 => Some(Command::Scan { fleet_id }),
                6 => Some(Command::Build {
                    building_type: BuildingType::from_usize(rng.random_range(0..BuildingType::COUNT)).unwrap(),
                    reserve: rng.random_bool(0.3),
                }),
                7 => Some(Command::Research {
                    tech: ResearchType::from_usize(rng.random_range(0..ResearchType::COUNT)).unwrap(),
                    reserve: false,
                }),
                8 => Some(Command::BuildShip { ship_class: ShipClass::LightFighter, count: 2 }),
                9 => here.map(|at| Command::Attack { fleet_id, target_fleet_id: template_npc_id(at) }),
                10 => Some(Command::Recall { fleet_id }),
                _ => Some(Command::CollectSalvage { fleet_id }),
            };
            if let Some(cmd) = cmd
                && engine.execute(pid, cmd).error.is_none()
            {
                self.accepted += 1;
            }
            // Orders that cannot be refused for lack of a target keep the queues busy.
            if tick.is_multiple_of(40) {
                let _ = engine.execute(pid, Command::CancelBuild { queue_type: QueueType::Building, id: 0 });
            }
        }
    }

    fn observe(&mut self, _engine: &GameEngine) {}
}
