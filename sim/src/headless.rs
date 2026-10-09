// A world with scripted players and no network: build it, step it as fast as
// the CPU allows, read the results. Used by `sim-run`, the determinism tests
// and (next milestone) the bot balance reports.

use std::collections::BTreeMap;

use iac_shared::pace::Pace;
use iac_shared::scaling::{BuildingType, ResearchType};
use serde::Serialize;

use crate::engine::GameEngine;
use crate::score;
use crate::script::ScriptedPlayer;
use crate::views;

struct Seat {
    player_id: u64,
    player: Box<dyn ScriptedPlayer>,
    accepted: u64,
    rejected: u64,
}

pub struct Headless {
    pub engine: GameEngine,
    seats: Vec<Seat>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScoreBreakdown {
    pub econ: f32,
    pub fleet: f32,
    pub defence: f32,
    pub combat: f32,
    pub explore: f32,
    pub core: f32,
    pub total: f32,
}

/// One player's end state, for the JSON report.
#[derive(Debug, Clone, Serialize)]
pub struct PlayerSummary {
    pub name: String,
    pub player_id: u64,
    pub score: ScoreBreakdown,
    pub buildings: BTreeMap<String, u8>,
    pub research: BTreeMap<String, u8>,
    pub ships: usize,
    pub commands_accepted: u64,
    pub commands_rejected: u64,
}

impl Headless {
    pub fn new(world_seed: u64, pace: Pace) -> Headless {
        Headless { engine: GameEngine::new(world_seed, pace), seats: Vec::new() }
    }

    /// Register the player's account and seat them; returns the player id.
    /// Tokens are derived from the id: a headless world has nothing to protect.
    pub fn add_player(&mut self, player: Box<dyn ScriptedPlayer>) -> Result<u64, String> {
        let name = player.name();
        let granted = self
            .engine
            .authenticate(&name, None, true, || format!("{:064x}", self.seats.len()))
            .map_err(|e| format!("cannot add player '{name}': {e:?}"))?;
        self.seats.push(Seat { player_id: granted.player_id, player, accepted: 0, rejected: 0 });
        Ok(granted.player_id)
    }

    /// One tick: every seat that wants a turn acts, then the world advances.
    pub fn step(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let tick = self.engine.current_tick() + 1;
        let mut commands = Vec::new();
        for seat in &mut self.seats {
            if !seat.player.wants_turn(tick) {
                continue;
            }
            let Some(state) = views::full_state(&self.engine, seat.player_id) else { continue };
            commands.clear();
            seat.player.act(&state, &mut commands);
            for cmd in commands.drain(..) {
                if self.engine.execute(seat.player_id, cmd).error.is_none() {
                    seat.accepted += 1;
                } else {
                    seat.rejected += 1;
                }
            }
        }
        self.engine.tick()?;
        self.engine.drain_events();
        self.engine.persist_dirty_state();
        Ok(())
    }

    pub fn run(&mut self, ticks: u64) -> Result<(), Box<dyn std::error::Error>> {
        for _ in 0..ticks {
            self.step()?;
        }
        Ok(())
    }

    pub fn summaries(&self) -> Vec<PlayerSummary> {
        self.seats
            .iter()
            .filter_map(|seat| {
                let player = self.engine.players.get(&seat.player_id)?;
                let s = score::score_of(player, self.engine.fleets.values());
                let buildings = (0..BuildingType::COUNT)
                    .filter_map(BuildingType::from_usize)
                    .map(|b| (b.label().to_string(), player.buildings.get(b)))
                    .collect();
                let research = (0..ResearchType::COUNT)
                    .filter_map(ResearchType::from_usize)
                    .map(|r| (r.label().to_string(), player.research.get(r)))
                    .collect();
                let ships = self
                    .engine
                    .fleets
                    .values()
                    .filter(|f| f.owner_id == player.id)
                    .map(|f| f.ship_count)
                    .sum();
                Some(PlayerSummary {
                    name: player.name.clone(),
                    player_id: player.id,
                    score: ScoreBreakdown {
                        econ: s.econ,
                        fleet: s.fleet,
                        defence: s.defence,
                        combat: s.combat,
                        explore: s.explore,
                        core: s.core(),
                        total: s.total(),
                    },
                    buildings,
                    research,
                    ships,
                    commands_accepted: seat.accepted,
                    commands_rejected: seat.rejected,
                })
            })
            .collect()
    }
}
