// Scripted players: the seam where built-in bots plug in.
//
// A scripted player sees exactly what a connected client sees (`GameState`,
// the protocol's full-state view) and acts only by emitting protocol
// `Command`s, which the host runs through `GameEngine::execute` like any
// other player's. There is no other way to touch the world, so a bot cannot
// cheat by construction.
//
// Turn order per tick, for every seat in the order they were added:
//   1. `wants_turn(tick)`  -- cheap; false skips building the view at all
//   2. `act(&state, &mut commands)` -- observe, push commands
//   3. the host executes the commands in order, then ticks the engine once
// Rejected commands are counted, not fatal: a bot learns by looking at the
// next state. Bot state lives in the implementor; it must be deterministic
// (no clocks, no OS randomness) so a run replays from the world seed.

use iac_shared::constants::Resources;
use iac_shared::protocol::{Command, GameState};

pub trait ScriptedPlayer {
    /// Account name; must satisfy the usual name rules.
    fn name(&self) -> String;

    /// Whether `act` should run at `tick` (the tick about to be simulated).
    fn wants_turn(&self, tick: u64) -> bool {
        let _ = tick;
        true
    }

    /// Observe the state as of the end of the previous tick and queue commands.
    fn act(&mut self, state: &GameState, commands: &mut Vec<Command>);
}

/// Does nothing: the control that measures what the starting position alone earns.
pub struct IdlePlayer {
    pub label: String,
}

impl ScriptedPlayer for IdlePlayer {
    fn name(&self) -> String {
        self.label.clone()
    }

    fn wants_turn(&self, _tick: u64) -> bool {
        false
    }

    fn act(&mut self, _state: &GameState, _commands: &mut Vec<Command>) {}
}

/// The check-in player in its plainest form: every `interval` ticks, if
/// nothing is being built or waiting, queue the cheapest building upgrade
/// that is unlocked and affordable. Never touches a fleet.
pub struct CheckInBuilder {
    pub label: String,
    pub interval: u64,
}

impl CheckInBuilder {
    pub const DEFAULT_INTERVAL: u64 = 3600;

    pub fn new(label: impl Into<String>) -> CheckInBuilder {
        CheckInBuilder { label: label.into(), interval: Self::DEFAULT_INTERVAL }
    }
}

fn total(cost: &Resources) -> f32 {
    cost.metal + cost.crystal + cost.deuterium
}

impl ScriptedPlayer for CheckInBuilder {
    fn name(&self) -> String {
        self.label.clone()
    }

    fn wants_turn(&self, tick: u64) -> bool {
        tick.is_multiple_of(self.interval)
    }

    fn act(&mut self, state: &GameState, commands: &mut Vec<Command>) {
        let home = &state.homeworld;
        if !home.build_queue.is_empty() || !home.build_pending.is_empty() {
            return;
        }
        let stock = state.player.resources;
        let cheapest = home
            .catalog
            .buildings
            .iter()
            .filter(|b| b.requires.iter().all(|r| r.met))
            .filter_map(|b| b.next.map(|step| (b.building_type, step.cost)))
            .filter(|(_, cost)| cost.metal <= stock.metal && cost.crystal <= stock.crystal && cost.deuterium <= stock.deuterium)
            .min_by(|a, b| total(&a.1).total_cmp(&total(&b.1)));
        if let Some((building_type, _)) = cheapest {
            commands.push(Command::Build { building_type, reserve: false });
        }
    }
}
