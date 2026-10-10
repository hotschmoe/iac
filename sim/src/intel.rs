// Sector intel: what each player can see this tick and what they remember.
//
// A sector is "live" for a player while a fleet of theirs is in it, their
// sensor array covers it, or an active scan has not expired. Live sectors
// stream to the client every tick. Every live sector is also written into the
// player's chart (`KnownSectors`), so when it stops being live the client keeps
// what was last observed, marked stale with the tick it was seen.
//
// The chart is maintained from changes, not recomputed: each player's coverage
// is re-derived only when its inputs (fleet positions, sensor level, scans)
// differ from the last tick, and a sector's content is re-read only when
// something dynamic could have touched it (an override, an NPC group, or the
// place an NPC group left) and some player is watching it. A live sector's
// `last_seen` is implicit (the last refresh) and is written down when it lapses.

use std::collections::{BTreeSet, HashMap, HashSet};

use iac_shared::constants::ShipClass;
use iac_shared::hex::Hex;
use iac_shared::protocol::{
    FleetBrief, NpcBehavior, NpcFleetInfo, NpcShipInfo, SectorResources, SectorState, ShipClassCounts,
    SiteBrief, ThreatInfo,
};
use iac_shared::scaling;
use iac_shared::world::{NpcBehaviorType, NpcTemplate};

use crate::engine::{site_risk_label, template_npc_id, Fleet, GameEngine, NpcFleet, SectorOverride};
use crate::persist::KnownRow;

/// Live sectors are rewritten to disk this often even if nothing about them
/// changed, so a restart reports an honest `last_seen`.
const LIVE_TOUCH_TICKS: u64 = 60;

#[derive(Debug, Clone)]
struct Remembered {
    /// Meaningful only while the sector is not live; see `KnownSectors::seen`.
    last_seen: u64,
    /// As last observed, with `last_seen` zeroed. `live` is always false and
    /// `player_fleets` empty: other empires' positions are not remembered.
    state: SectorState,
}

/// The sectors one player watches, split by how they change.
#[derive(Debug, Default)]
struct Coverage {
    sensor: Option<(Hex, u8)>,
    sensor_set: HashSet<u32>,
    fleets: Vec<u32>,
    scans: Vec<u32>,
    extras: HashSet<u32>,
}

impl Coverage {
    fn contains(&self, key: u32) -> bool {
        self.extras.contains(&key) || self.sensor_set.contains(&key)
    }

    fn keys(&self) -> impl Iterator<Item = u32> + '_ {
        self.extras.iter().chain(self.sensor_set.iter()).copied()
    }
}

/// Every player's explored-sector memory.
#[derive(Debug, Default)]
pub struct KnownSectors {
    by_player: HashMap<u64, HashMap<u32, Remembered>>,
    coverage: HashMap<u64, Coverage>,
    /// Sector key to the players (ascending) that have it live.
    watchers: HashMap<u32, Vec<u64>>,
    /// Remembered, not live sectors with a salvage pile: (despawn tick, key).
    pins: HashMap<u64, BTreeSet<(u64, u32)>>,
    /// Keys that stopped being live at the last refresh.
    lapsed: HashMap<u64, Vec<u32>>,
    /// Remembered sectors whose salvage pin expired at the last refresh.
    expired: HashMap<u64, Vec<u32>>,
    dirty: HashSet<(u64, u32)>,
    /// Sectors an NPC group stood in or was homed to at the last refresh.
    npc_sectors: Vec<u32>,
    refreshed: u64,
}

impl KnownSectors {
    pub fn load(rows: Vec<KnownRow>) -> KnownSectors {
        let mut known = KnownSectors::default();
        for row in rows {
            let key = row.sector.to_key();
            let mut state = row.state;
            state.last_seen = 0;
            if let Some(despawn) = state.salvage_despawn_tick {
                known.pins.entry(row.player_id).or_default().insert((despawn, key));
            }
            known.by_player.entry(row.player_id).or_default().insert(key, Remembered { last_seen: row.last_seen, state });
        }
        known
    }

    /// Record what each player sees this tick. Called once per tick, after
    /// the simulation has run.
    pub fn refresh(&mut self, engine: &GameEngine) {
        let tick = engine.current_tick;
        self.lapsed.clear();
        self.expired.clear();

        let mut fleet_cells: HashMap<u64, Vec<u32>> = HashMap::new();
        for fleet in engine.fleets.values().filter(|f| f.ship_count > 0) {
            fleet_cells.entry(fleet.owner_id).or_default().push(fleet.location.to_key());
        }
        let mut entered: Vec<(u64, u32)> = Vec::new();
        for &pid in engine.players.keys() {
            let mut cells = fleet_cells.remove(&pid).unwrap_or_default();
            cells.sort_unstable();
            cells.dedup();
            self.update_coverage(engine, pid, cells, &mut entered);
        }

        let mut wanted: HashSet<u32> = entered.iter().map(|&(_, key)| key).collect();
        let dynamic = self.dynamic_watched(engine);
        wanted.extend(dynamic.iter().copied());
        let observed = observe(engine, &wanted);

        for &(pid, key) in &entered {
            self.record(pid, key, &observed[&key], tick);
        }
        for key in dynamic {
            let Some(pids) = self.watchers.get(&key).cloned() else { continue };
            for pid in pids {
                self.record(pid, key, &observed[&key], tick);
            }
        }

        self.expire_pins(tick);
        if tick.is_multiple_of(LIVE_TOUCH_TICKS) {
            self.touch_live();
        }
        self.refreshed = tick;
    }

    fn update_coverage(&mut self, engine: &GameEngine, pid: u64, fleets: Vec<u32>, entered: &mut Vec<(u64, u32)>) {
        let sensor = engine.players.get(&pid).and_then(|p| {
            let range = scaling::sensor_range(p.buildings.sensor_array);
            (range > 0).then_some((p.homeworld, range))
        });
        let mut scans: Vec<u32> = engine.scan_revealed_coords(pid).into_iter().map(Hex::to_key).collect();
        scans.sort_unstable();

        let old = self.coverage.remove(&pid).unwrap_or_default();
        if old.sensor == sensor && old.fleets == fleets && old.scans == scans {
            self.coverage.insert(pid, old);
            return;
        }

        let sensor_changed = old.sensor != sensor;
        let mut now = Coverage { sensor, fleets, scans, ..Coverage::default() };
        now.extras = now.fleets.iter().chain(&now.scans).copied().collect();
        if sensor_changed {
            if let Some((origin, range)) = sensor {
                now.sensor_set = engine.get_sensor_revealed_coords(origin, range).iter().map(|h| h.to_key()).collect();
            }
        } else {
            now.sensor_set = old.sensor_set.clone();
        }

        let mut touched: Vec<u32> = if sensor_changed {
            old.keys().chain(now.keys()).collect()
        } else {
            old.extras.symmetric_difference(&now.extras).copied().collect()
        };
        touched.sort_unstable();
        touched.dedup();
        for key in touched {
            match (old.contains(key), now.contains(key)) {
                (false, true) => {
                    self.unpin(pid, key);
                    let watchers = self.watchers.entry(key).or_default();
                    if let Err(at) = watchers.binary_search(&pid) {
                        watchers.insert(at, pid);
                    }
                    entered.push((pid, key));
                }
                (true, false) => self.leave(pid, key),
                _ => {}
            }
        }
        self.coverage.insert(pid, now);
    }

    fn unpin(&mut self, pid: u64, key: u32) {
        let despawn = self.by_player.get(&pid).and_then(|m| m.get(&key)).and_then(|r| r.state.salvage_despawn_tick);
        if let (Some(despawn), Some(pins)) = (despawn, self.pins.get_mut(&pid)) {
            pins.remove(&(despawn, key));
        }
    }

    fn leave(&mut self, pid: u64, key: u32) {
        if let Some(watchers) = self.watchers.get_mut(&key) {
            watchers.retain(|&p| p != pid);
            if watchers.is_empty() {
                self.watchers.remove(&key);
            }
        }
        if let Some(r) = self.by_player.get_mut(&pid).and_then(|m| m.get_mut(&key)) {
            r.last_seen = self.refreshed;
            if let Some(despawn) = r.state.salvage_despawn_tick {
                self.pins.entry(pid).or_default().insert((despawn, key));
            }
        }
        self.lapsed.entry(pid).or_default().push(key);
        self.dirty.insert((pid, key));
    }

    /// Watched sectors whose content may differ from what the players hold:
    /// those with an override, an NPC group, or that had one at the last refresh.
    fn dynamic_watched(&mut self, engine: &GameEngine) -> Vec<u32> {
        let mut npc_sectors: Vec<u32> = engine
            .npc_fleets
            .values()
            .flat_map(|n| [n.location.to_key(), n.home_sector.to_key()])
            .collect();
        npc_sectors.sort_unstable();
        npc_sectors.dedup();

        let mut seen: HashSet<u32> = HashSet::new();
        let dynamic = engine
            .sector_overrides
            .keys()
            .chain(&npc_sectors)
            .chain(&self.npc_sectors)
            .copied()
            .filter(|key| self.watchers.contains_key(key) && seen.insert(*key))
            .collect();
        self.npc_sectors = npc_sectors;
        dynamic
    }

    /// Write what the player sees of a live sector into their memory.
    fn record(&mut self, pid: u64, key: u32, observed: &SectorState, tick: u64) {
        let memory = self.by_player.entry(pid).or_default();
        match memory.get_mut(&key) {
            Some(r) if r.state == *observed => {}
            Some(r) => {
                // Refilling ore moves every tick; it is written with the periodic touch.
                let mut probe = observed.clone();
                probe.ore_reserve = r.state.ore_reserve;
                if probe != r.state {
                    self.dirty.insert((pid, key));
                }
                r.state = observed.clone();
                r.last_seen = tick;
            }
            None => {
                memory.insert(key, Remembered { last_seen: tick, state: observed.clone() });
                self.dirty.insert((pid, key));
            }
        }
    }

    /// A remembered pile past its despawn tick is known to be gone.
    fn expire_pins(&mut self, tick: u64) {
        for (&pid, pins) in &mut self.pins {
            let mut expired = Vec::new();
            while let Some(&(despawn, key)) = pins.first() {
                if despawn > tick {
                    break;
                }
                pins.pop_first();
                if let Some(r) = self.by_player.get_mut(&pid).and_then(|m| m.get_mut(&key)) {
                    r.state.salvage = None;
                    r.state.salvage_despawn_tick = None;
                }
                self.dirty.insert((pid, key));
                expired.push(key);
            }
            if !expired.is_empty() {
                expired.sort_unstable();
                self.expired.insert(pid, expired);
            }
        }
    }

    /// The threat of a charted sector as last seen.
    pub fn threat(&self, player_id: u64, coord: Hex) -> Option<ThreatInfo> {
        self.by_player.get(&player_id)?.get(&coord.to_key()).map(|r| r.state.threat)
    }

    /// Queue every live sector for the next persist (clean shutdown).
    pub fn touch_live(&mut self) {
        for (&pid, cover) in &self.coverage {
            self.dirty.extend(cover.keys().map(|key| (pid, key)));
        }
    }

    /// The remembered rows changed since the last `clear_dirty`.
    pub fn dirty_rows(&self) -> Vec<KnownRow> {
        let mut dirty: Vec<(u64, u32)> = self.dirty.iter().copied().collect();
        dirty.sort_unstable();
        dirty.into_iter().filter_map(|(pid, key)| self.row(pid, key)).collect()
    }

    pub fn clear_dirty(&mut self) {
        self.dirty.clear();
    }

    /// Every remembered sector, ordered by player and sector.
    pub fn rows(&self) -> Vec<KnownRow> {
        let mut keys: Vec<(u64, u32)> = self.by_player
            .iter()
            .flat_map(|(&pid, memory)| memory.keys().map(move |&key| (pid, key)))
            .collect();
        keys.sort_unstable();
        keys.into_iter().filter_map(|(pid, key)| self.row(pid, key)).collect()
    }

    fn row(&self, player_id: u64, key: u32) -> Option<KnownRow> {
        let r = self.by_player.get(&player_id)?.get(&key)?;
        let last_seen = self.seen(player_id, key, r);
        let mut state = r.state.clone();
        state.last_seen = last_seen;
        Some(KnownRow { player_id, sector: Hex::from_key(key), last_seen, state })
    }

    /// A live sector was seen at the last refresh; the rest carry their own stamp.
    fn seen(&self, player_id: u64, key: u32, r: &Remembered) -> u64 {
        if self.coverage.get(&player_id).is_some_and(|c| c.contains(key)) {
            self.refreshed
        } else {
            r.last_seen
        }
    }

    fn stale_view(&self, player_id: u64, key: u32, r: &Remembered) -> SectorState {
        let mut s = r.state.clone();
        s.last_seen = self.seen(player_id, key, r);
        s.live = false;
        s.pins_stale = s.salvage.is_some() || s.site.is_some();
        s
    }

    /// The whole chart: live sectors as they are now, everything else as
    /// last observed.
    pub fn chart(&self, engine: &GameEngine, player_id: u64) -> Vec<SectorState> {
        let live = live_coords(engine, player_id);
        let live_keys: HashSet<u32> = live.iter().map(|h| h.to_key()).collect();
        let mut out = live_states(engine, &live, player_id);
        if let Some(memory) = self.by_player.get(&player_id) {
            out.extend(
                memory
                    .iter()
                    .filter(|(key, _)| !live_keys.contains(key))
                    .map(|(&key, r)| self.stale_view(player_id, key, r)),
            );
        }
        out.sort_by_key(|s| s.location.to_key());
        out
    }

    /// Per-tick delta: every live sector, plus the sectors that stopped being
    /// live this tick (once, flagged stale).
    pub fn tick_updates(&self, engine: &GameEngine, player_id: u64) -> Vec<SectorState> {
        let live = live_coords(engine, player_id);
        let mut out = live_states(engine, &live, player_id);
        if let Some(memory) = self.by_player.get(&player_id) {
            let cover = self.coverage.get(&player_id);
            let mut lapsed = self.lapsed.get(&player_id).cloned().unwrap_or_default();
            lapsed.sort_unstable();
            let expired = self.expired.get(&player_id).map_or(&[][..], Vec::as_slice);
            out.extend(
                lapsed
                    .iter()
                    .chain(expired.iter().filter(|key| lapsed.binary_search(key).is_err()))
                    .filter(|&&key| !cover.is_some_and(|c| c.contains(key)))
                    .filter_map(|&key| memory.get(&key).map(|r| self.stale_view(player_id, key, r))),
            );
        }
        out
    }
}

/// Sectors the server is watching for this player right now: fleet
/// locations, the homeworld sensor array, and unexpired active scans.
pub fn live_coords(engine: &GameEngine, player_id: u64) -> Vec<Hex> {
    let mut seen: HashSet<u32> = HashSet::new();
    let mut out: Vec<Hex> = Vec::new();
    let mut add = |c: Hex| {
        if seen.insert(c.to_key()) {
            out.push(c);
        }
    };

    let mut fleet_locations: Vec<Hex> = engine
        .fleets
        .values()
        .filter(|f| f.owner_id == player_id && f.ship_count > 0)
        .map(|f| f.location)
        .collect();
    fleet_locations.sort_by_key(|h| h.to_key());
    fleet_locations.into_iter().for_each(&mut add);

    if let Some(p) = engine.players.get(&player_id) {
        let range = scaling::sensor_range(p.buildings.sensor_array);
        if range > 0 {
            engine.get_sensor_revealed_coords(p.homeworld, range).into_iter().for_each(&mut add);
        }
    }
    engine.scan_revealed_coords(player_id).into_iter().for_each(&mut add);
    out
}

fn behavior(b: NpcBehaviorType) -> NpcBehavior {
    match b {
        NpcBehaviorType::Passive => NpcBehavior::Passive,
        NpcBehaviorType::Patrol => NpcBehavior::Patrol,
        NpcBehaviorType::Aggressive => NpcBehavior::Aggressive,
        NpcBehaviorType::Swarm => NpcBehavior::Swarm,
    }
}

fn template_hostile(coord: Hex, tmpl: &NpcTemplate) -> NpcFleetInfo {
    NpcFleetInfo {
        id: template_npc_id(coord),
        ships: vec![NpcShipInfo { ship_class: tmpl.ship_class, count: tmpl.count as u16 }],
        behavior: behavior(tmpl.behavior),
    }
}

/// Where the NPC groups and (optionally) fleets stand, for the sectors asked about.
struct Presence<'a> {
    npcs: HashMap<u32, Vec<&'a NpcFleet>>,
    fleets: HashMap<u32, Vec<&'a Fleet>>,
}

impl<'a> Presence<'a> {
    fn of(engine: &'a GameEngine, keys: &HashSet<u32>, with_fleets: bool) -> Presence<'a> {
        let mut npcs: HashMap<u32, Vec<&NpcFleet>> = HashMap::new();
        for npc in engine.npc_fleets.values() {
            let key = npc.location.to_key();
            if keys.contains(&key) {
                npcs.entry(key).or_default().push(npc);
            }
        }
        let mut fleets: HashMap<u32, Vec<&Fleet>> = HashMap::new();
        if with_fleets {
            for fleet in engine.fleets.values().filter(|f| f.ship_count > 0) {
                let key = fleet.location.to_key();
                if keys.contains(&key) {
                    fleets.entry(key).or_default().push(fleet);
                }
            }
        }
        Presence { npcs, fleets }
    }

    fn state(&self, engine: &GameEngine, coord: Hex, viewer: Option<u64>) -> SectorState {
        let key = coord.to_key();
        let npcs = self.npcs.get(&key).map_or(&[][..], Vec::as_slice);
        let fleets = self.fleets.get(&key).map_or(&[][..], Vec::as_slice);
        sector_state(engine, coord, viewer, npcs, fleets)
    }
}

/// What a memory snapshot of each sector holds this tick: other empires'
/// fleets left out and the stamp zeroed.
fn observe(engine: &GameEngine, keys: &HashSet<u32>) -> HashMap<u32, SectorState> {
    let presence = Presence::of(engine, keys, false);
    keys.iter()
        .map(|&key| {
            let mut state = presence.state(engine, Hex::from_key(key), None);
            state.live = false;
            state.last_seen = 0;
            (key, state)
        })
        .collect()
}

/// The sectors as they are this tick for `viewer`, in `coords` order.
fn live_states(engine: &GameEngine, coords: &[Hex], viewer: u64) -> Vec<SectorState> {
    let keys: HashSet<u32> = coords.iter().map(|c| c.to_key()).collect();
    let presence = Presence::of(engine, &keys, true);
    coords.iter().map(|&c| presence.state(engine, c, Some(viewer))).collect()
}

/// The sector as it is this tick. Other empires' fleets are listed unless
/// `viewer` is None (a memory snapshot), and `viewer`'s own fleets never are.
pub fn build_sector_state(engine: &GameEngine, coord: Hex, viewer: Option<u64>) -> SectorState {
    let keys = HashSet::from([coord.to_key()]);
    Presence::of(engine, &keys, viewer.is_some()).state(engine, coord, viewer)
}

fn sector_state(engine: &GameEngine, coord: Hex, viewer: Option<u64>, npcs: &[&NpcFleet], fleets: &[&Fleet]) -> SectorState {
    let template = engine.world_gen.generate_sector(coord);
    let neighbors = engine.world_gen.connected_neighbors(coord);
    let ov = engine.sector_overrides.get(&coord.to_key());
    let (metal_d, crystal_d, deut_d) = SectorOverride::effective_densities(ov, &template);

    let mut hostile_list: Vec<NpcFleetInfo> = Vec::new();
    for npc in npcs {
        let mut counts = [0u16; ShipClass::ALL.len()];
        for ship in &npc.ships[0..npc.ship_count as usize] {
            counts[ship.ship_class as usize] += 1;
        }
        let ships = ShipClass::ALL
            .iter()
            .filter(|&&c| counts[c as usize] > 0)
            .map(|&c| NpcShipInfo { ship_class: c, count: counts[c as usize] })
            .collect();
        hostile_list.push(NpcFleetInfo { id: npc.id, ships, behavior: behavior(npc.behavior) });
    }
    hostile_list.sort_by_key(|h| h.id);

    // A template hostile that has not materialised yet carries the id it
    // will have once it does (see `template_npc_id`).
    if hostile_list.is_empty()
        && let Some(tmpl) = engine.pending_template_npc(coord)
    {
        hostile_list.push(template_hostile(coord, &tmpl));
    }

    let mut player_fleet_list: Vec<FleetBrief> = Vec::new();
    if let Some(viewer) = viewer {
        for fleet in fleets.iter().filter(|f| f.owner_id != viewer) {
            let mut counts = ShipClassCounts::default();
            for ship in &fleet.ships[0..fleet.ship_count] {
                match ship.ship_class {
                    ShipClass::Scout => counts.scout += 1,
                    ShipClass::Corvette => counts.corvette += 1,
                    ShipClass::Frigate => counts.frigate += 1,
                    ShipClass::Cruiser => counts.cruiser += 1,
                    ShipClass::Hauler => counts.hauler += 1,
                }
            }
            player_fleet_list.push(FleetBrief {
                id: fleet.id,
                owner_name: engine
                    .players
                    .get(&fleet.owner_id)
                    .map(|o| o.name.clone())
                    .unwrap_or_else(|| "Unknown".to_string()),
                ship_count: fleet.ship_count as u16,
                ship_classes: counts,
            });
        }
        player_fleet_list.sort_by_key(|f| f.id);
    }

    let salvage = ov.and_then(|o| o.salvage);
    SectorState {
        location: coord,
        terrain: template.terrain,
        resources: SectorResources { metal: metal_d, crystal: crystal_d, deuterium: deut_d },
        connections: neighbors.slice().to_vec(),
        hostiles: (!hostile_list.is_empty()).then_some(hostile_list),
        player_fleets: (!player_fleet_list.is_empty()).then_some(player_fleet_list),
        salvage,
        salvage_despawn_tick: salvage.and(ov.and_then(|o| o.salvage_despawn_tick)),
        site: engine
            .derelict_site_at(coord)
            .map(|(tier, bumps)| SiteBrief { tier, risk: site_risk_label(coord, bumps) }),
        ore_reserve: engine.ore_reserve_at(coord),
        threat: engine.threat_among(coord, npcs.iter().copied()),
        last_seen: engine.current_tick,
        live: true,
        pins_stale: false,
    }
}
