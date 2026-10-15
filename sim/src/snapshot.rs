// A complete, canonical copy of the world: what `GameEngine::snapshot` writes
// and `GameEngine::restore` reads. Every map is ordered, so equal worlds
// serialize to identical bytes and `hash` can stand in for "same state".
//
// It is also everything a host stores (`store`): whatever must survive a
// restart is a field here, and is nowhere else.

use std::collections::{BTreeMap, BTreeSet};

use iac_shared::constants::{ECONOMY_VERSION, WORLDGEN_VERSION};
use iac_shared::hex::Hex;
use iac_shared::pace::Pace;
use iac_shared::protocol::{GameEvent, SectorState};
use serde::{Deserialize, Serialize};
use xxhash_rust::xxh3::xxh3_64;

use crate::bot::BotRecord;
use crate::engine::{Combat, Fleet, FleetPolicy, HarvestReport, NpcFleet, Outpost, Player, RaidState, Relay, SectorOverride};

/// Settings fixed when a world is created.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WorldMeta {
    pub pace: Pace,
    pub economy_version: u32,
    pub worldgen_version: u32,
}

/// One sector of one player's chart.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownRow {
    pub player_id: u64,
    pub sector: Hex,
    pub last_seen: u64,
    pub state: SectorState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub world_seed: u64,
    pub meta: WorldMeta,
    pub tick: u64,
    pub next_id: u64,
    pub players: BTreeMap<u64, Player>,
    pub fleets: BTreeMap<u64, Fleet>,
    pub npc_fleets: BTreeMap<u64, NpcFleet>,
    pub combats: BTreeMap<u64, Combat>,
    pub sector_overrides: BTreeMap<u32, SectorOverride>,
    pub policies: BTreeMap<u64, FleetPolicy>,
    pub scan_reveals: BTreeMap<u64, BTreeMap<u32, u64>>,
    pub raid_states: BTreeMap<u64, RaidState>,
    pub storage_marks: BTreeMap<u64, [u8; 3]>,
    pub home_salvage_warned: BTreeMap<u64, u64>,
    pub harvest_reports: BTreeMap<u64, HarvestReport>,
    /// Built-in bots by account: what each is and remembers.
    pub bots: BTreeMap<u64, BotRecord>,
    /// Outposts by owner.
    pub outposts: BTreeMap<u64, Outpost>,
    /// Relays by owner, in the order they were founded.
    pub relays: BTreeMap<u64, Vec<Relay>>,
    pub pending_events: Vec<GameEvent>,
    pub explored: BTreeSet<(u64, u32)>,
    /// (player, sector key): charts delivered and paid for.
    pub charted: BTreeSet<(u64, u32)>,
    pub known: Vec<KnownRow>,
}

impl Snapshot {
    /// An empty world at tick 0.
    pub fn new(world_seed: u64, pace: Pace) -> Snapshot {
        Snapshot {
            world_seed,
            meta: WorldMeta { pace, economy_version: ECONOMY_VERSION, worldgen_version: WORLDGEN_VERSION },
            tick: 0,
            next_id: 1,
            players: BTreeMap::new(),
            fleets: BTreeMap::new(),
            npc_fleets: BTreeMap::new(),
            combats: BTreeMap::new(),
            sector_overrides: BTreeMap::new(),
            policies: BTreeMap::new(),
            scan_reveals: BTreeMap::new(),
            raid_states: BTreeMap::new(),
            storage_marks: BTreeMap::new(),
            home_salvage_warned: BTreeMap::new(),
            harvest_reports: BTreeMap::new(),
            bots: BTreeMap::new(),
            outposts: BTreeMap::new(),
            relays: BTreeMap::new(),
            pending_events: Vec::new(),
            explored: BTreeSet::new(),
            charted: BTreeSet::new(),
            known: Vec::new(),
        }
    }

    /// A 64-bit digest of the whole state.
    pub fn hash(&self) -> u64 {
        xxh3_64(&serde_json::to_vec(self).expect("a snapshot always serializes"))
    }
}
