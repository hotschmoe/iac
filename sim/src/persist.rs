// The persistence boundary. The engine never touches storage: it hands
// whole-row deltas (`PersistBatch`) to whatever `Persist` the host plugged in,
// and the host loads a world back through `Snapshot` (see `snapshot.rs`).

use std::collections::BTreeMap;

use iac_shared::hex::Hex;
use iac_shared::pace::Pace;
use iac_shared::protocol::SectorState;
use serde::{Deserialize, Serialize};

use crate::engine::{Combat, Fleet, FleetPolicy, HarvestReport, NpcFleet, Player, RaidState, SectorOverride};

/// Settings fixed when a world is created.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WorldMeta {
    pub pace: Pace,
    pub economy_version: u32,
    pub worldgen_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExploredEdge {
    pub player_id: u64,
    pub from: Hex,
    pub to: Hex,
    pub tick: u64,
}

/// What an explore credit was paid for: it is paid once per player and sector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ExploreCredit {
    /// First successful boarding of the derelict here.
    Boarded = 0,
    /// The sector's chart was delivered to a dock.
    Charted = 1,
}

/// One sector of one player's chart.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownRow {
    pub player_id: u64,
    pub sector: Hex,
    pub last_seen: u64,
    pub state: SectorState,
}

/// The counters of a fleet whose row is otherwise unchanged.
#[derive(Debug, Clone, Copy)]
pub struct FleetClocks {
    pub fleet_id: u64,
    pub move_cooldown: u16,
    pub action_cooldown: u16,
    pub idle_ticks: u16,
}

/// The `Snapshot` maps a host stores entry by entry, without a schema of
/// their own. The id is the map's key: an NPC group, a combat, a player or
/// a fleet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RuntimeKind {
    NpcFleet,
    Combat,
    Raid,
    Scans,
    StorageMarks,
    HomeSalvageWarned,
    HarvestReport,
}

impl RuntimeKind {
    pub fn name(self) -> &'static str {
        match self {
            RuntimeKind::NpcFleet => "npc_fleet",
            RuntimeKind::Combat => "combat",
            RuntimeKind::Raid => "raid",
            RuntimeKind::Scans => "scans",
            RuntimeKind::StorageMarks => "storage_marks",
            RuntimeKind::HomeSalvageWarned => "home_salvage_warned",
            RuntimeKind::HarvestReport => "harvest_report",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuntimeEntry {
    NpcFleet(Box<NpcFleet>),
    Combat(Combat),
    Raid(RaidState),
    Scans(BTreeMap<u32, u64>),
    StorageMarks([u8; 3]),
    HomeSalvageWarned(u64),
    HarvestReport(HarvestReport),
}

/// One entry as it is now; `None` once it is gone.
#[derive(Debug, Clone)]
pub struct RuntimeRow {
    pub kind: RuntimeKind,
    pub id: u64,
    pub entry: Option<RuntimeEntry>,
}

/// Everything that changed since the previous batch. Rows are whole-row
/// upserts, so replaying batches in order is idempotent.
#[derive(Debug, Default)]
pub struct PersistBatch {
    pub tick: u64,
    pub next_id: u64,
    pub players: Vec<Player>,
    pub fleets: Vec<Fleet>,
    pub fleet_clocks: Vec<FleetClocks>,
    pub deleted_fleets: Vec<u64>,
    pub policies: Vec<(u64, FleetPolicy)>,
    pub deleted_policies: Vec<u64>,
    pub sectors: Vec<(Hex, SectorOverride)>,
    pub runtime: Vec<RuntimeRow>,
    pub explored_edges: Vec<ExploredEdge>,
    pub known_sectors: Vec<KnownRow>,
    pub explore_credits: Vec<(u64, Hex, ExploreCredit)>,
}

/// Where the engine sends its deltas. `submit` must not block the tick;
/// implementations queue and write elsewhere.
pub trait Persist: Send {
    fn submit(&self, batch: PersistBatch);

    /// Block until every submitted batch is durable (shutdown, tests).
    fn flush(&self) -> Result<(), String>;
}
