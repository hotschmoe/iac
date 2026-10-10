// Scripted players: the seam where built-in bots plug in.
//
// A scripted player sees exactly what a connected client sees (`GameState`,
// the protocol's full-state view) and acts only by emitting what a client
// sends: protocol `Command`s and `PolicyUpdate`s. A `Seat` runs them through
// `GameEngine::execute` and `handle_policy_update` like any other player's.
// There is no other way to touch the world, so a bot cannot cheat by
// construction.
//
// A host (the headless runner, the server, a WebAssembly page) keeps one
// `Seat` per bot and, before every `GameEngine::tick`, calls `take_turn` on
// each in a fixed order. Per seat:
//   1. `wants_turn(tick)`  -- cheap; false skips building the view at all
//   2. `act(&state, &mut orders)` -- observe, push orders
//   3. the seat executes the orders in order; `hear` gets any reply
// Refused orders are counted, not fatal: a bot learns by looking at the next
// state. A bot's memory lives in the implementor; it must be deterministic
// (no clocks, no OS randomness; randomness is seeded from what `seated`
// hands over and the tick) so a run replays from the world seed.

use iac_shared::protocol::{Command, GameState, PolicyUpdate, ServerMessage};

use crate::engine::{AuthError, GameEngine};
use crate::views;

/// One thing a client can send for a logged-in player.
#[derive(Debug, Clone)]
pub enum Order {
    Command(Command),
    Policy(PolicyUpdate),
}

pub trait ScriptedPlayer {
    /// Account name; must satisfy the usual name rules.
    fn name(&self) -> String;

    /// Called once when the player takes its seat.
    fn seated(&mut self, world_seed: u64, player_id: u64) {
        let _ = (world_seed, player_id);
    }

    /// Whether `act` should run at `tick` (the tick about to be simulated).
    fn wants_turn(&self, tick: u64) -> bool {
        let _ = tick;
        true
    }

    /// Observe the state as of the end of the previous tick and queue orders.
    fn act(&mut self, state: &GameState, orders: &mut Vec<Order>);

    /// The server's answer to one of this turn's orders that has one
    /// (`leaderboard`, `preview_move`).
    fn hear(&mut self, reply: &ServerMessage) {
        let _ = reply;
    }
}

/// A scripted player bound to its account in one world.
pub struct Seat {
    player_id: u64,
    player: Box<dyn ScriptedPlayer>,
    orders: Vec<Order>,
    turns: u64,
    accepted: u64,
    refused: u64,
}

impl Seat {
    /// Register the player's account (or log in to it with `token`) and seat it.
    pub fn join(
        engine: &mut GameEngine,
        player: Box<dyn ScriptedPlayer>,
        token: Option<&str>,
        new_token: impl FnOnce() -> String,
    ) -> Result<Seat, AuthError> {
        let granted = engine.authenticate(&player.name(), token, true, new_token)?;
        Ok(Seat::resume(engine, granted.player_id, player))
    }

    /// Seat a player on an account the host already knows is its own.
    pub fn resume(engine: &GameEngine, player_id: u64, mut player: Box<dyn ScriptedPlayer>) -> Seat {
        player.seated(engine.world_gen.world_seed, player_id);
        Seat { player_id, player, orders: Vec::new(), turns: 0, accepted: 0, refused: 0 }
    }

    pub fn player_id(&self) -> u64 {
        self.player_id
    }

    pub fn turns(&self) -> u64 {
        self.turns
    }

    pub fn accepted(&self) -> u64 {
        self.accepted
    }

    pub fn refused(&self) -> u64 {
        self.refused
    }

    /// Let the player act on the state before the next tick is simulated.
    pub fn take_turn(&mut self, engine: &mut GameEngine) {
        if !self.player.wants_turn(engine.current_tick() + 1) {
            return;
        }
        let Some(state) = views::full_state(engine, self.player_id) else { return };
        self.turns += 1;
        self.orders.clear();
        self.player.act(&state, &mut self.orders);
        for order in self.orders.drain(..) {
            let ok = match order {
                Order::Command(cmd) => {
                    let outcome = engine.execute(self.player_id, cmd);
                    if let Some(reply) = &outcome.reply {
                        self.player.hear(reply);
                    }
                    outcome.error.is_none()
                }
                Order::Policy(update) => engine
                    .handle_policy_update(self.player_id, update.fleet_id, update.preset, update.params)
                    .is_ok(),
            };
            if ok {
                self.accepted += 1;
            } else {
                self.refused += 1;
            }
        }
    }
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

    fn act(&mut self, _state: &GameState, _orders: &mut Vec<Order>) {}
}
