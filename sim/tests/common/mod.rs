use std::collections::HashMap;

use iac_shared::constants::{Density, ShipClass};
use iac_shared::hex::Hex;
use iac_shared::pace::Pace;
use iac_shared::protocol::{Command, ErrorCode, EventKind, GameEvent, GameState, QueueEvent, SectorState, ServerMessage};
use iac_shared::world::NpcBehaviorType;
use iac_sim::engine::template_npc_id;
use iac_sim::engine::FleetStatus;
use iac_sim::{views, GameEngine, Snapshot};

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

pub fn lone_scout_sector(engine: &GameEngine, behavior: NpcBehaviorType) -> Hex {
    for q in -20i16..20 {
        for r in -20i16..20 {
            let coord = Hex { q, r };
            if let Some(t) = engine.world_gen.generate_sector(coord).npc_template
                && t.behavior == behavior
                && t.count == 1
                && t.ship_class == ShipClass::Scout
                && engine.derelict_site_at(coord).is_none()
            {
                return coord;
            }
        }
    }
    panic!("no such NPC sector in a 40x40 window");
}

pub fn deploy(engine: &mut GameEngine, fid: u64, at: Hex, weapon_power: Option<f32>) {
    edit(engine, |s| {
        let fleet = s.fleets.get_mut(&fid).expect("fleet");
        fleet.location = at;
        if let Some(power) = weapon_power {
            for ship in &mut fleet.ships[..fleet.ship_count] {
                ship.weapon_power = power;
                ship.hull = 1e6;
                ship.hull_max = 1e6;
            }
        }
    });
}

pub fn fight(engine: &mut GameEngine, pid: u64, fid: u64, at: Hex) -> Vec<GameEvent> {
    deploy(engine, fid, at, Some(5000.0));
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
