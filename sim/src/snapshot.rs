// A complete, canonical copy of the world: what `GameEngine::snapshot` writes
// and `GameEngine::restore` reads. Every map is ordered, so equal worlds
// serialize to identical bytes and `hash` can stand in for "same state".

use std::collections::{BTreeMap, BTreeSet};

use iac_shared::constants::{ECONOMY_VERSION, WORLDGEN_VERSION};
use iac_shared::pace::Pace;
use iac_shared::protocol::GameEvent;
use serde::{Deserialize, Serialize};
use xxhash_rust::xxh3::xxh3_64;

use crate::engine::{Combat, Fleet, FleetPolicy, HarvestReport, NpcFleet, Player, RaidState, SectorOverride};
use crate::persist::{ExploreCredit, KnownRow, WorldMeta};

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
    pub pending_events: Vec<GameEvent>,
    pub explored: BTreeSet<(u64, u32)>,
    pub credited: BTreeSet<(u64, u32, ExploreCredit)>,
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
            pending_events: Vec::new(),
            explored: BTreeSet::new(),
            credited: BTreeSet::new(),
            known: Vec::new(),
        }
    }

    /// A 64-bit digest of the whole state.
    pub fn hash(&self) -> u64 {
        xxh3_64(&serde_json::to_vec(self).expect("a snapshot always serializes"))
    }
}
