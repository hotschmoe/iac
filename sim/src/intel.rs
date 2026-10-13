// Sector intel: what each player can see this tick and what they remember.
//
// A sector is "live" for a player while a fleet of theirs is in it, their
// homeworld's sensors cover it, or an active scan has not expired. Live sectors
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
// So is its ore: a stripped tile refills every tick, and writing that into
// every watcher's memory every tick was most of the upkeep. What the watchers
// of such a sector hold in common is kept once (`KnownSectors::shown`); their
// memories are rewritten only when something other than the ore moves, and
// take the ore from there when the sector lapses or a row is asked for.

use std::collections::BTreeSet;

use iac_shared::hex::Hex;
use iac_shared::keymap::{KeyMap, KeySet};
use iac_shared::protocol::{
    FleetBrief, GroupErrand, NpcBehavior, NpcFleetInfo, OutpostBrief, SectorResources, SectorState, ShipCount,
    SiteBrief,
};
use iac_shared::scaling::{self, Arms};
use iac_shared::world::{NpcBehaviorType, NpcTemplate};

use crate::engine::{site_risk_label, template_npc_id, Fleet, GameEngine, NpcFleet, SectorOverride, Ships};
use crate::snapshot::KnownRow;

#[derive(Debug, Clone)]
struct Remembered {
    /// Meaningful only while the sector is not live; see `KnownSectors::settled`.
    last_seen: u64,
    /// As last observed, with `last_seen` zeroed. `live` is always false and
    /// `player_fleets` empty: other empires' positions are not remembered.
    /// While the sector is live its `ore_reserve` may lag; see `KnownSectors::shown`.
    state: SectorState,
}

/// The sectors one player watches, split by how they change.
#[derive(Debug, Default)]
struct Coverage {
    sensor: Option<(Hex, u8)>,
    sensor_set: KeySet<u32>,
    fleets: Vec<u32>,
    scans: Vec<u32>,
    outpost: Option<u32>,
    extras: KeySet<u32>,
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
    by_player: KeyMap<u64, KeyMap<u32, Remembered>>,
    coverage: KeyMap<u64, Coverage>,
    /// Sector key to the players (ascending) that have it live.
    watchers: KeyMap<u32, Vec<u64>>,
    /// Watched sectors with moving parts, as their watchers were last shown
    /// them. Each watcher's memory equals this in everything but the ore,
    /// which is current here and only here.
    shown: KeyMap<u32, SectorState>,
    /// Remembered, not live sectors with a salvage pile: (despawn tick, key).
    pins: KeyMap<u64, BTreeSet<(u64, u32)>>,
    /// Keys that stopped being live at the last refresh.
    lapsed: KeyMap<u64, Vec<u32>>,
    /// Remembered sectors whose salvage pin expired at the last refresh.
    expired: KeyMap<u64, Vec<u32>>,
    /// Sectors whose chart mark may have moved since the refresh before.
    marked: KeyMap<u64, Vec<u32>>,
    /// Sectors an NPC group stood in or was homed to at the last refresh.
    npc_sectors: Vec<u32>,
    /// Sectors an outpost stood in at the last refresh.
    outpost_sectors: Vec<u32>,
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
        self.marked.clear();
        for &(pid, key) in &engine.chart_news {
            self.marked.entry(pid).or_default().push(key);
        }

        let mut fleet_cells: KeyMap<u64, Vec<u32>> = KeyMap::default();
        for fleet in engine.fleets.values().filter(|f| !f.ships.is_empty()) {
            fleet_cells.entry(fleet.owner_id).or_default().push(fleet.location.to_key());
        }
        let mut entered: Vec<(u64, u32)> = Vec::new();
        for &pid in engine.players.keys() {
            let mut cells = fleet_cells.remove(&pid).unwrap_or_default();
            cells.sort_unstable();
            cells.dedup();
            self.update_coverage(engine, pid, cells, &mut entered);
        }

        let mut wanted: KeySet<u32> = entered.iter().map(|&(_, key)| key).collect();
        let dynamic = self.dynamic_watched(engine);
        wanted.extend(dynamic.iter().copied());
        let observed = observe(engine, &wanted);

        for &(pid, key) in &entered {
            Self::record(&mut self.by_player, pid, key, &observed[&key], tick);
        }
        for key in dynamic {
            let Some(pids) = self.watchers.get(&key) else { continue };
            let seen = &observed[&key];
            if let Some(shown) = self.shown.get_mut(&key) {
                let ore = std::mem::replace(&mut shown.ore_reserve, seen.ore_reserve);
                if shown == seen {
                    if ore == seen.ore_reserve {
                        shown.ore_reserve = ore;
                    }
                    continue;
                }
            }
            for &pid in pids {
                Self::record(&mut self.by_player, pid, key, seen, tick);
            }
            self.shown.insert(key, seen.clone());
        }

        self.expire_pins(tick);
        self.refreshed = tick;
    }

    fn update_coverage(&mut self, engine: &GameEngine, pid: u64, fleets: Vec<u32>, entered: &mut Vec<(u64, u32)>) {
        let sensor = engine.players.get(&pid).and_then(|p| {
            let range = scaling::sensor_range(p.research.espionage_technology);
            (range > 0).then_some((p.homeworld, range))
        });
        let mut scans: Vec<u32> = engine.scan_revealed_coords(pid).into_iter().map(Hex::to_key).collect();
        scans.sort_unstable();
        let outpost = engine.outposts.get(&pid).map(|o| o.sector.to_key());

        let old = self.coverage.remove(&pid).unwrap_or_default();
        if old.sensor == sensor && old.fleets == fleets && old.scans == scans && old.outpost == outpost {
            self.coverage.insert(pid, old);
            return;
        }

        let sensor_changed = old.sensor != sensor;
        let mut now = Coverage { sensor, fleets, scans, outpost, ..Coverage::default() };
        now.extras = now.fleets.iter().chain(&now.scans).chain(&now.outpost).copied().collect();
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
        if let Some(r) = self.by_player.get_mut(&pid).and_then(|m| m.get_mut(&key)) {
            r.last_seen = self.refreshed;
            if let Some(shown) = self.shown.get(&key) {
                r.state.ore_reserve = shown.ore_reserve;
            }
            if let Some(despawn) = r.state.salvage_despawn_tick {
                self.pins.entry(pid).or_default().insert((despawn, key));
            }
        }
        if let Some(watchers) = self.watchers.get_mut(&key) {
            watchers.retain(|&p| p != pid);
            if watchers.is_empty() {
                self.watchers.remove(&key);
                self.shown.remove(&key);
            }
        }
        self.lapsed.entry(pid).or_default().push(key);
    }

    /// Watched sectors whose content may differ from what the players hold:
    /// those with an override, an NPC group or an outpost, or that had a
    /// group or an outpost at the last refresh.
    fn dynamic_watched(&mut self, engine: &GameEngine) -> Vec<u32> {
        let mut npc_sectors: Vec<u32> = engine
            .npc_fleets
            .values()
            .flat_map(|n| [n.location.to_key(), n.home_sector.to_key()])
            .collect();
        npc_sectors.sort_unstable();
        npc_sectors.dedup();
        let outpost_sectors: Vec<u32> = engine.outposts.values().map(|o| o.sector.to_key()).collect();

        let mut seen: KeySet<u32> = KeySet::default();
        let dynamic = engine
            .sector_overrides
            .keys()
            .chain(&npc_sectors)
            .chain(&self.npc_sectors)
            .chain(&outpost_sectors)
            .chain(&self.outpost_sectors)
            .copied()
            .filter(|key| self.watchers.contains_key(key) && seen.insert(*key))
            .collect();
        self.npc_sectors = npc_sectors;
        self.outpost_sectors = outpost_sectors;
        dynamic
    }

    /// Write what the player sees of a live sector into their memory.
    fn record(
        by_player: &mut KeyMap<u64, KeyMap<u32, Remembered>>,
        pid: u64,
        key: u32,
        observed: &SectorState,
        tick: u64,
    ) {
        let memory = by_player.entry(pid).or_default();
        match memory.get_mut(&key) {
            Some(r) => {
                let ore = std::mem::replace(&mut r.state.ore_reserve, observed.ore_reserve);
                if r.state != *observed {
                    r.state = observed.clone();
                } else if ore == observed.ore_reserve {
                    r.state.ore_reserve = ore;
                    return;
                } else {
                    // Equal is not identical for a float (0.0 and -0.0): the
                    // memory takes the observed bits, as a copy would.
                    r.state.salvage = observed.salvage;
                    r.state.threat = observed.threat;
                }
                r.last_seen = tick;
            }
            None => {
                memory.insert(key, Remembered { last_seen: tick, state: observed.clone() });
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
                expired.push(key);
            }
            if !expired.is_empty() {
                expired.sort_unstable();
                self.expired.insert(pid, expired);
            }
        }
    }

    /// A charted sector as last seen. While the sector is live for the
    /// player its ore may lag behind (`shown`); everything else is current.
    pub fn remembered(&self, player_id: u64, coord: Hex) -> Option<&SectorState> {
        self.by_player.get(&player_id)?.get(&coord.to_key()).map(|r| &r.state)
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
        let state = self.settled(player_id, key, r);
        Some(KnownRow { player_id, sector: Hex::from_key(key), last_seen: state.last_seen, state })
    }

    /// A remembered sector with what is implicit while it is live written
    /// in: it was seen at the last refresh, and its ore is the ore shown
    /// then. A sector that is not live carries both itself.
    fn settled(&self, player_id: u64, key: u32, r: &Remembered) -> SectorState {
        let mut state = r.state.clone();
        if self.coverage.get(&player_id).is_some_and(|c| c.contains(key)) {
            state.last_seen = self.refreshed;
            if let Some(shown) = self.shown.get(&key) {
                state.ore_reserve = shown.ore_reserve;
            }
        } else {
            state.last_seen = r.last_seen;
        }
        state
    }

    fn stale_view(&self, player_id: u64, key: u32, r: &Remembered) -> SectorState {
        let mut s = self.settled(player_id, key, r);
        s.live = false;
        s.pins_stale = s.salvage.is_some() || s.site.is_some();
        s
    }

    /// The whole chart: live sectors as they are now, everything else as
    /// last observed.
    pub fn chart(&self, engine: &GameEngine, player_id: u64) -> Vec<SectorState> {
        let live = live_coords(engine, player_id);
        let live_keys: KeySet<u32> = live.iter().map(|h| h.to_key()).collect();
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
        mark_charts(engine, player_id, &mut out);
        out
    }

    /// Per-tick delta: every live sector, plus, once, each remembered sector
    /// that changed at the last refresh: it stopped being live (flagged
    /// stale), its salvage pin expired, or its chart mark moved.
    pub fn tick_updates(&self, engine: &GameEngine, player_id: u64) -> Vec<SectorState> {
        let live = live_coords(engine, player_id);
        let mut out = live_states(engine, &live, player_id);
        if let Some(memory) = self.by_player.get(&player_id) {
            let cover = self.coverage.get(&player_id);
            let mut changed: Vec<u32> = [&self.lapsed, &self.expired, &self.marked]
                .into_iter()
                .filter_map(|keys| keys.get(&player_id))
                .flatten()
                .copied()
                .collect();
            changed.sort_unstable();
            changed.dedup();
            out.extend(
                changed
                    .iter()
                    .filter(|&&key| !cover.is_some_and(|c| c.contains(key)))
                    .filter_map(|&key| memory.get(&key).map(|r| self.stale_view(player_id, key, r))),
            );
        }
        mark_charts(engine, player_id, &mut out);
        out
    }
}

/// Chart marks, and whether an outpost is the viewer's own, belong to the
/// viewer and not to the sector, so they are laid over a view when it is
/// built and never remembered.
fn mark_charts(engine: &GameEngine, player_id: u64, sectors: &mut [SectorState]) {
    let aboard = engine.charts_aboard(player_id);
    let name = engine.players.get(&player_id).map(|p| p.name.as_str());
    for sector in sectors {
        sector.chart = engine.chart_mark(player_id, sector.location.to_key(), &aboard);
        if let Some(outpost) = &mut sector.outpost {
            outpost.mine = name == Some(outpost.owner.as_str());
        }
    }
}

/// Sectors the server is watching for this player right now: fleet
/// locations, their outpost, the homeworld's sensors, and unexpired
/// active scans.
pub fn live_coords(engine: &GameEngine, player_id: u64) -> Vec<Hex> {
    let mut seen: KeySet<u32> = KeySet::default();
    let mut out: Vec<Hex> = Vec::new();
    let mut add = |c: Hex| {
        if seen.insert(c.to_key()) {
            out.push(c);
        }
    };

    let mut fleet_locations: Vec<Hex> = engine
        .fleets
        .values()
        .filter(|f| f.owner_id == player_id && !f.ships.is_empty())
        .map(|f| f.location)
        .collect();
    fleet_locations.sort_by_key(|h| h.to_key());
    fleet_locations.into_iter().for_each(&mut add);
    engine.outposts.get(&player_id).map(|o| o.sector).into_iter().for_each(&mut add);

    if let Some(p) = engine.players.get(&player_id) {
        let range = scaling::sensor_range(p.research.espionage_technology);
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
    let count = u32::from(tmpl.count);
    NpcFleetInfo {
        id: template_npc_id(coord),
        ships: Ships::of(&[(tmpl.ship_class, count)]).states(Arms::of_grade(tmpl.stat_multiplier)),
        ship_count: count,
        behavior: behavior(tmpl.behavior),
        muster: count,
        whole_at: None,
        jump_ticks: scaling::npc_jump_ticks(tmpl.ship_class, coord.dist_from_origin()),
        chase_hexes: tmpl.behavior.chase_hexes(),
        errand: None,
    }
}

/// A group that stands somewhere, as a sector view lists it.
fn live_hostile(engine: &GameEngine, npc: &NpcFleet) -> NpcFleetInfo {
    let away = npc.location != npc.home_sector || npc.errand.is_some();
    let quarry = npc.errand.and_then(|errand| errand.quarry);
    NpcFleetInfo {
        id: npc.id,
        ships: npc.ships.states(npc.arms),
        ship_count: npc.ships.total(),
        behavior: behavior(npc.behavior),
        muster: npc.muster.total(),
        whole_at: engine.group_whole_at(npc),
        jump_ticks: npc.jump_ticks(),
        chase_hexes: npc.behavior.chase_hexes(),
        errand: away.then(|| GroupErrand {
            home: npc.home_sector,
            quarry,
            quarry_owner: quarry
                .and_then(|id| engine.fleets.get(&id))
                .and_then(|fleet| engine.players.get(&fleet.owner_id))
                .map(|owner| owner.name.clone()),
            to: npc.errand.map(|errand| errand.to),
            arrives_at: npc.errand.map(|errand| errand.arrives),
            hexes_left: npc.leash(),
        }),
    }
}

/// Where the NPC groups and (optionally) fleets stand, for the sectors asked about.
struct Presence<'a> {
    npcs: KeyMap<u32, Vec<&'a NpcFleet>>,
    fleets: KeyMap<u32, Vec<&'a Fleet>>,
}

impl<'a> Presence<'a> {
    fn of(engine: &'a GameEngine, keys: &KeySet<u32>, with_fleets: bool) -> Presence<'a> {
        let mut npcs: KeyMap<u32, Vec<&NpcFleet>> = KeyMap::default();
        for npc in engine.npc_fleets.values() {
            let key = npc.location.to_key();
            if keys.contains(&key) {
                npcs.entry(key).or_default().push(npc);
            }
        }
        let mut fleets: KeyMap<u32, Vec<&Fleet>> = KeyMap::default();
        if with_fleets {
            for fleet in engine.fleets.values().filter(|f| !f.ships.is_empty()) {
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
fn observe(engine: &GameEngine, keys: &KeySet<u32>) -> KeyMap<u32, SectorState> {
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
    let keys: KeySet<u32> = coords.iter().map(|c| c.to_key()).collect();
    let presence = Presence::of(engine, &keys, true);
    coords.iter().map(|&c| presence.state(engine, c, Some(viewer))).collect()
}

/// The sector as it is this tick. Other empires' fleets are listed unless
/// `viewer` is None (a memory snapshot), and `viewer`'s own fleets never are.
pub fn build_sector_state(engine: &GameEngine, coord: Hex, viewer: Option<u64>) -> SectorState {
    let keys = KeySet::from_iter([coord.to_key()]);
    Presence::of(engine, &keys, viewer.is_some()).state(engine, coord, viewer)
}

fn sector_state(engine: &GameEngine, coord: Hex, viewer: Option<u64>, npcs: &[&NpcFleet], fleets: &[&Fleet]) -> SectorState {
    let template = engine.world_gen.generate_sector(coord);
    let neighbors = engine.world_gen.connected_neighbors(coord);
    let ov = engine.sector_overrides.get(&coord.to_key());
    let (metal_d, crystal_d, deut_d) = SectorOverride::effective_densities(ov, &template);
    let pending = engine.pending_template_in(coord, &template, ov);

    let mut hostile_list: Vec<NpcFleetInfo> =
        npcs.iter().filter(|npc| !npc.ships.is_empty()).map(|npc| live_hostile(engine, npc)).collect();
    hostile_list.sort_by_key(|h| h.id);

    // A template hostile that has not materialised yet carries the id it
    // will have once it does (see `template_npc_id`).
    if hostile_list.is_empty()
        && let Some(tmpl) = &pending
    {
        hostile_list.push(template_hostile(coord, tmpl));
    }

    let mut player_fleet_list: Vec<FleetBrief> = Vec::new();
    if let Some(viewer) = viewer {
        for fleet in fleets.iter().filter(|f| f.owner_id != viewer) {
            let counts = fleet.ships.counts().map(|(ship_class, count)| ShipCount { ship_class, count }).collect();
            player_fleet_list.push(FleetBrief {
                id: fleet.id,
                owner_name: engine
                    .players
                    .get(&fleet.owner_id)
                    .map(|o| o.name.clone())
                    .unwrap_or_else(|| "Unknown".to_string()),
                ship_count: fleet.ships.total(),
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
        site: GameEngine::derelict_site_in(coord, &template, ov)
            .map(|(tier, bumps)| SiteBrief { tier, risk: site_risk_label(coord, bumps) }),
        ore_reserve: engine.ore_reserve_in(coord, &template, ov),
        chart: None,
        outpost: engine.outpost_at(coord).map(|(owner, outpost)| OutpostBrief {
            owner: engine.players.get(&owner).map(|o| o.name.clone()).unwrap_or_else(|| "Unknown".to_string()),
            level: outpost.level,
            breached: outpost.breached,
            mine: false,
        }),
        threat: engine.threat_among(coord, &template, pending.as_ref(), npcs.iter().copied()),
        last_seen: engine.current_tick,
        live: true,
        pins_stale: false,
    }
}
