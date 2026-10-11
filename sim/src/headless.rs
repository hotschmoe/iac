// A world with scripted players and no network: build it, step it as fast as
// the CPU allows, read the results. Used by `sim-run` and the bot tests.
//
// Besides the end state it keeps what a balance report needs: each player's
// score split at every sample point, and a running tally of what the field
// paid them, counted from the engine's own events.

use std::collections::BTreeMap;

use iac_shared::constants::FUEL_DEUT_PER_UNIT;
use iac_shared::pace::Pace;
use iac_shared::protocol::{EventKind, GameEvent};
use iac_shared::scaling::{self, BuildingType, ResearchType};
use serde::Serialize;

use crate::engine::{FleetStatus, GameEngine};
use crate::score::{self, Score};
use crate::script::{ScriptedPlayer, Seat};

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct ScoreBreakdown {
    pub econ: f32,
    pub fleet: f32,
    pub defence: f32,
    pub core: f32,
    /// Raw combat points, before the cap.
    pub combat: f32,
    /// Raw exploration points, before the cap.
    pub explore: f32,
    /// What combat and exploration add to the score once capped.
    pub extra: f32,
    pub total: f32,
}

impl From<Score> for ScoreBreakdown {
    fn from(s: Score) -> ScoreBreakdown {
        ScoreBreakdown {
            econ: s.econ,
            fleet: s.fleet,
            defence: s.defence,
            core: s.core(),
            combat: s.combat,
            explore: s.explore,
            extra: s.total() - s.core(),
            total: s.total(),
        }
    }
}

/// What the map and the raids paid a player so far. Resources are given as
/// their score weight: the core points they are worth once spent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct FieldTally {
    pub ore: f32,
    pub salvage: f32,
    pub derelict_loot: f32,
    /// Wreckage of repelled raids scooped at the homeworld.
    pub raid_salvage: f32,
    pub kills: u32,
    pub sites_boarded: u32,
    pub sectors_charted: u32,
    pub ships_lost: u32,
    /// Deuterium paid at the homeworld to fill fleet tanks.
    pub refuel_deuterium: f32,
    pub raids_repelled: u32,
    pub raids_lost: u32,
}

impl FieldTally {
    /// Resources brought in by fleets away from home.
    pub fn haul(&self) -> f32 {
        self.ore + self.salvage + self.derelict_loot
    }
}

/// Ticks a player's production slots existed and ticks they stood empty,
/// counted since the player was seated.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct SlotTicks {
    ticks: u64,
    building: u64,
    building_idle: u64,
    research_idle: u64,
    shipyard_idle: u64,
}

/// Share of the time each kind of slot stood empty, 0 to 1. Building slots
/// are counted one by one; a lab or a yard not built yet counts as empty.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct SlotIdle {
    pub building: f32,
    pub research: f32,
    pub shipyard: f32,
}

impl From<SlotTicks> for SlotIdle {
    fn from(t: SlotTicks) -> SlotIdle {
        let share = |idle: u64, of: u64| if of == 0 { 0.0 } else { idle as f32 / of as f32 };
        SlotIdle {
            building: share(t.building_idle, t.building),
            research: share(t.research_idle, t.ticks),
            shipyard: share(t.shipyard_idle, t.ticks),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Sample {
    pub tick: u64,
    pub score: ScoreBreakdown,
    pub field: FieldTally,
    pub idle: SlotIdle,
    /// Score weight of the unspent stockpile.
    pub stock: f32,
    pub ships: usize,
}

/// One player's run, for the report.
#[derive(Debug, Clone, Serialize)]
pub struct PlayerSummary {
    pub name: String,
    pub style: String,
    pub player_id: u64,
    pub score: ScoreBreakdown,
    pub field: FieldTally,
    pub idle: SlotIdle,
    pub samples: Vec<Sample>,
    pub buildings: BTreeMap<String, u8>,
    pub research: BTreeMap<String, u8>,
    pub ships: usize,
    pub turns: u64,
    pub orders_accepted: u64,
    pub orders_refused: u64,
}

struct Entry {
    seat: Seat<Box<dyn ScriptedPlayer>>,
    style: String,
    tally: FieldTally,
    slots: SlotTicks,
    samples: Vec<Sample>,
}

fn tally_of(entries: &mut [Entry], player_id: Option<u64>) -> Option<&mut FieldTally> {
    let player_id = player_id?;
    entries.iter_mut().find(|e| e.seat.player_id() == player_id).map(|e| &mut e.tally)
}

pub struct Headless {
    pub engine: GameEngine,
    entries: Vec<Entry>,
}

impl Headless {
    pub fn new(world_seed: u64, pace: Pace) -> Headless {
        Headless { engine: GameEngine::new(world_seed, pace), entries: Vec::new() }
    }

    /// Register the player's account and seat them; returns the player id.
    /// `style` groups players in the report. Tokens are derived from the
    /// seat number: a headless world has nothing to protect.
    pub fn add_player(&mut self, player: Box<dyn ScriptedPlayer>, style: impl Into<String>) -> Result<u64, String> {
        let name = player.name();
        let number = self.entries.len();
        let seat = Seat::join(&mut self.engine, player, None, || format!("{number:064x}"))
            .map_err(|e| format!("cannot add player '{name}': {e:?}"))?;
        let player_id = seat.player_id();
        self.entries.push(Entry {
            seat,
            style: style.into(),
            tally: FieldTally::default(),
            slots: SlotTicks::default(),
            samples: Vec::new(),
        });
        Ok(player_id)
    }

    pub fn players(&self) -> usize {
        self.entries.len()
    }

    /// One tick: every seat takes its turn, the world advances, every seat
    /// sees what happened.
    pub fn step(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        for entry in &mut self.entries {
            entry.seat.take_turn(&mut self.engine);
        }
        let tanks: Vec<(u64, f32, bool)> =
            self.engine.fleets.values().map(|f| (f.id, f.fuel, f.state == FleetStatus::Moving)).collect();
        self.engine.tick()?;
        self.count_refuelling(&tanks);
        self.count_slots();
        for entry in &mut self.entries {
            entry.seat.observe(&self.engine);
        }
        let events = self.engine.drain_events();
        self.count(&events);
        self.engine.persist_dirty_state();
        Ok(())
    }

    pub fn run(&mut self, ticks: u64) -> Result<(), Box<dyn std::error::Error>> {
        for _ in 0..ticks {
            self.step()?;
        }
        Ok(())
    }

    /// Run `ticks`, taking a sample every `every` ticks and at the end.
    pub fn run_sampled(&mut self, ticks: u64, every: u64) -> Result<(), Box<dyn std::error::Error>> {
        let end = self.engine.current_tick() + ticks;
        while self.engine.current_tick() < end {
            let leg = every.max(1).min(end - self.engine.current_tick());
            self.run(leg)?;
            self.sample();
        }
        Ok(())
    }

    /// Record every player's score and tally as of now.
    pub fn sample(&mut self) {
        let tick = self.engine.current_tick();
        for i in 0..self.entries.len() {
            let Some(sample) = self.sample_of(i, tick) else { continue };
            self.entries[i].samples.push(sample);
        }
    }

    fn sample_of(&self, index: usize, tick: u64) -> Option<Sample> {
        let entry = &self.entries[index];
        let player = self.engine.players.get(&entry.seat.player_id())?;
        Some(Sample {
            tick,
            score: score::score_of(player, self.engine.fleets.values()).into(),
            field: entry.tally,
            idle: entry.slots.into(),
            stock: scaling::resource_weight(&player.resources),
            ships: self.ships_of(player.id),
        })
    }

    fn ships_of(&self, player_id: u64) -> usize {
        self.engine.fleets.values().filter(|f| f.owner_id == player_id).map(|f| f.ship_count).sum()
    }

    fn count_slots(&mut self) {
        for entry in &mut self.entries {
            let Some(player) = self.engine.players.get(&entry.seat.player_id()) else { continue };
            let slots = &mut entry.slots;
            let building = player.building_slots() as u64;
            slots.building += building;
            slots.building_idle += building.saturating_sub(player.building_queue.len() as u64);
            slots.ticks += 1;
            slots.research_idle += u64::from(player.research_queue.is_none());
            slots.shipyard_idle += u64::from(player.ship_queue.is_none());
        }
    }

    /// Fuel only rises at the homeworld, where it is bought; a fleet that
    /// jumped out during the tick is filled again before it leaves the sector.
    fn count_refuelling(&mut self, tanks: &[(u64, f32, bool)]) {
        let engine = &self.engine;
        for &(fleet_id, fuel, was_moving) in tanks {
            let Some(fleet) = engine.fleets.get(&fleet_id) else { continue };
            let at_home = engine.players.get(&fleet.owner_id).is_some_and(|p| p.homeworld == fleet.location);
            if !at_home {
                continue;
            }
            let jumped = !was_moving && fleet.state == FleetStatus::Moving;
            let burned = if jumped { engine.hop_fuel_cost(fleet) } else { 0.0 };
            let bought = fleet.fuel - fuel + burned;
            if bought > 0.0
                && let Some(t) = tally_of(&mut self.entries, Some(fleet.owner_id))
            {
                t.refuel_deuterium += bought * FUEL_DEUT_PER_UNIT;
            }
        }
    }

    fn count(&mut self, events: &[GameEvent]) {
        if events.is_empty() {
            return;
        }
        let engine = &self.engine;
        let entries = &mut self.entries;
        let flying = |fleet_id: u64| engine.fleets.get(&fleet_id).map(|f| f.owner_id);
        let named = |name: &str| engine.players.values().find(|p| p.name == name).map(|p| p.id);
        let home_of = |player_id: u64| engine.players.get(&player_id).map(|p| p.homeworld);

        for event in events {
            match &event.kind {
                EventKind::ResourceHarvested(e) => {
                    if let Some(t) = tally_of(entries, flying(e.fleet_id)) {
                        t.ore += scaling::resource_weight(&e.resources);
                    }
                }
                EventKind::SalvageCollected(e) => {
                    let owner = flying(e.fleet_id);
                    let at_home = owner.and_then(home_of) == Some(e.sector);
                    if let Some(t) = tally_of(entries, owner) {
                        let gain = scaling::resource_weight(&e.resources);
                        if at_home { t.raid_salvage += gain } else { t.salvage += gain }
                    }
                }
                EventKind::SiteExplored(e) => {
                    if let Some(t) = tally_of(entries, flying(e.fleet_id)) {
                        t.derelict_loot += scaling::resource_weight(&e.resources);
                        t.sites_boarded += 1;
                    }
                }
                EventKind::ChartDelivered(e) => {
                    if let Some(t) = tally_of(entries, flying(e.fleet_id)) {
                        t.sectors_charted += e.sectors;
                    }
                }
                EventKind::CombatEnded(e) if e.player_victory => {
                    for owner in &e.owners {
                        if let Some(t) = tally_of(entries, named(owner)) {
                            t.kills += 1;
                        }
                    }
                }
                EventKind::ShipDestroyed(e) if !e.is_npc => {
                    if let Some(t) = tally_of(entries, e.owner.as_deref().and_then(named)) {
                        t.ships_lost += 1;
                    }
                }
                EventKind::RaidResolved(e) => {
                    if let Some(t) = tally_of(entries, Some(e.player_id)) {
                        if e.defended { t.raids_repelled += 1 } else { t.raids_lost += 1 }
                        t.ships_lost += e.ships_lost;
                    }
                }
                _ => {}
            }
        }
    }

    pub fn summaries(&self) -> Vec<PlayerSummary> {
        self.entries
            .iter()
            .filter_map(|entry| {
                let player = self.engine.players.get(&entry.seat.player_id())?;
                let buildings = (0..BuildingType::COUNT)
                    .filter_map(BuildingType::from_usize)
                    .map(|b| (b.label().to_string(), player.buildings.get(b)))
                    .collect();
                let research = (0..ResearchType::COUNT)
                    .filter_map(ResearchType::from_usize)
                    .map(|r| (r.label().to_string(), player.research.get(r)))
                    .collect();
                Some(PlayerSummary {
                    name: player.name.clone(),
                    style: entry.style.clone(),
                    player_id: player.id,
                    score: score::score_of(player, self.engine.fleets.values()).into(),
                    field: entry.tally,
                    idle: entry.slots.into(),
                    samples: entry.samples.clone(),
                    buildings,
                    research,
                    ships: self.ships_of(player.id),
                    turns: entry.seat.turns(),
                    orders_accepted: entry.seat.accepted(),
                    orders_refused: entry.seat.refused(),
                })
            })
            .collect()
    }
}
