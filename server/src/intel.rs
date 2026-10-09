// Sector intel: what each player can see this tick and what they remember.
//
// A sector is "live" for a player while a fleet of theirs is in it, their
// sensor array covers it, or an active scan has not expired. Live sectors
// stream to the client every tick. Every live sector is also written into the
// player's chart (`KnownSectors`), so when it stops being live the client keeps
// what was last observed, marked stale with the tick it was seen.

use std::collections::{HashMap, HashSet};

use iac_shared::constants::ShipClass;
use iac_shared::hex::Hex;
use iac_shared::protocol::{
    FleetBrief, NpcBehavior, NpcFleetInfo, NpcShipInfo, SectorResources, SectorState, ShipClassCounts,
    SiteBrief, ThreatInfo,
};
use iac_shared::scaling;
use iac_shared::world::{NpcBehaviorType, NpcTemplate};

use crate::database::KnownRow;
use crate::engine::{site_risk_label, template_npc_id, GameEngine, SectorOverride};

/// Live sectors are rewritten to disk this often even if nothing about them
/// changed, so a restart reports an honest `last_seen`.
const LIVE_TOUCH_TICKS: u64 = 60;

#[derive(Debug, Clone)]
struct Remembered {
    last_seen: u64,
    /// As last observed. `live` is always false and `player_fleets` empty:
    /// other empires' positions are not remembered.
    state: SectorState,
}

/// Every player's explored-sector memory.
#[derive(Debug, Default)]
pub struct KnownSectors {
    by_player: HashMap<u64, HashMap<u32, Remembered>>,
    /// Keys that were live at the last refresh.
    live: HashMap<u64, HashSet<u32>>,
    /// Keys that stopped being live at the last refresh.
    lapsed: HashMap<u64, Vec<u32>>,
    dirty: HashSet<(u64, u32)>,
}

impl KnownSectors {
    pub fn load(rows: Vec<KnownRow>) -> KnownSectors {
        let mut known = KnownSectors::default();
        for row in rows {
            known.by_player.entry(row.player_id).or_default().insert(
                row.sector.to_key(),
                Remembered { last_seen: row.last_seen, state: row.state },
            );
        }
        known
    }

    /// Record what each player sees this tick. Called once per tick, after
    /// the simulation has run.
    pub fn refresh(&mut self, engine: &GameEngine) {
        let tick = engine.current_tick;
        for &pid in engine.players.keys() {
            let now_live: HashSet<u32> = live_coords(engine, pid).into_iter().map(Hex::to_key).collect();
            let memory = self.by_player.entry(pid).or_default();

            for &key in &now_live {
                let mut observed = build_sector_state(engine, Hex::from_key(key), None);
                observed.live = false;
                let touch = tick.is_multiple_of(LIVE_TOUCH_TICKS);
                match memory.get_mut(&key) {
                    Some(r) => {
                        observed.last_seen = r.state.last_seen;
                        // Refilling ore moves every tick; it is written with the periodic touch.
                        let fresh_ore = std::mem::replace(&mut observed.ore_reserve, r.state.ore_reserve);
                        let changed = r.state != observed;
                        observed.ore_reserve = fresh_ore;
                        observed.last_seen = tick;
                        *r = Remembered { last_seen: tick, state: observed };
                        if changed || touch {
                            self.dirty.insert((pid, key));
                        }
                    }
                    None => {
                        observed.last_seen = tick;
                        memory.insert(key, Remembered { last_seen: tick, state: observed });
                        self.dirty.insert((pid, key));
                    }
                }
            }

            let before = self.live.remove(&pid).unwrap_or_default();
            let lapsed: Vec<u32> = before.difference(&now_live).copied().collect();
            for &key in &lapsed {
                self.dirty.insert((pid, key));
            }
            self.lapsed.insert(pid, lapsed);
            self.live.insert(pid, now_live);
        }
    }

    /// The threat of a charted sector as last seen.
    pub fn threat(&self, player_id: u64, coord: Hex) -> Option<ThreatInfo> {
        self.by_player.get(&player_id)?.get(&coord.to_key()).map(|r| r.state.threat)
    }

    /// Queue every live sector for the next persist (clean shutdown).
    pub fn touch_live(&mut self) {
        for (&pid, keys) in &self.live {
            for &key in keys {
                self.dirty.insert((pid, key));
            }
        }
    }

    pub fn take_dirty(&mut self) -> Vec<KnownRow> {
        let dirty: Vec<(u64, u32)> = self.dirty.drain().collect();
        dirty
            .into_iter()
            .filter_map(|(pid, key)| {
                let r = self.by_player.get(&pid)?.get(&key)?;
                Some(KnownRow {
                    player_id: pid,
                    sector: Hex::from_key(key),
                    last_seen: r.last_seen,
                    state: r.state.clone(),
                })
            })
            .collect()
    }

    /// The whole chart: live sectors as they are now, everything else as
    /// last observed.
    pub fn chart(&self, engine: &GameEngine, player_id: u64) -> Vec<SectorState> {
        let live = live_coords(engine, player_id);
        let live_keys: HashSet<u32> = live.iter().map(|h| h.to_key()).collect();
        let mut out: Vec<SectorState> = live
            .into_iter()
            .map(|c| build_sector_state(engine, c, Some(player_id)))
            .collect();
        if let Some(memory) = self.by_player.get(&player_id) {
            out.extend(
                memory
                    .iter()
                    .filter(|(key, _)| !live_keys.contains(key))
                    .map(|(_, r)| stale_view(r)),
            );
        }
        out.sort_by_key(|s| s.location.to_key());
        out
    }

    /// Per-tick delta: every live sector, plus the sectors that stopped being
    /// live this tick (once, flagged stale).
    pub fn tick_updates(&self, engine: &GameEngine, player_id: u64) -> Vec<SectorState> {
        let mut out: Vec<SectorState> = live_coords(engine, player_id)
            .into_iter()
            .map(|c| build_sector_state(engine, c, Some(player_id)))
            .collect();
        if let (Some(lapsed), Some(memory)) = (self.lapsed.get(&player_id), self.by_player.get(&player_id)) {
            let live = self.live.get(&player_id);
            out.extend(
                lapsed
                    .iter()
                    .filter(|key| !live.is_some_and(|l| l.contains(key)))
                    .filter_map(|key| memory.get(key))
                    .map(stale_view),
            );
        }
        out
    }
}

fn stale_view(r: &Remembered) -> SectorState {
    let mut s = r.state.clone();
    s.last_seen = r.last_seen;
    s.live = false;
    s
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

/// The sector as it is this tick. Other empires' fleets are listed unless
/// `viewer` is None (a memory snapshot), and `viewer`'s own fleets never are.
pub fn build_sector_state(engine: &GameEngine, coord: Hex, viewer: Option<u64>) -> SectorState {
    let template = engine.world_gen.generate_sector(coord);
    let neighbors = engine.world_gen.connected_neighbors(coord);
    let ov = engine.sector_overrides.get(&coord.to_key());
    let (metal_d, crystal_d, deut_d) = SectorOverride::effective_densities(ov, &template);

    let mut hostile_list: Vec<NpcFleetInfo> = Vec::new();
    for npc in engine.npc_fleets.values() {
        if npc.location != coord {
            continue;
        }
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
        for fleet in engine.fleets.values() {
            if fleet.location != coord || fleet.ship_count == 0 || fleet.owner_id == viewer {
                continue;
            }
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
            .map(|(tier, bumps)| SiteBrief { tier, risk: site_risk_label(tier, bumps) }),
        ore_reserve: engine.ore_reserve_at(coord),
        threat: engine.sector_threat(coord),
        last_seen: engine.current_tick,
        live: true,
    }
}
