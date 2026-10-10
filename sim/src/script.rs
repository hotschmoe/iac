// Scripted players: the seam where built-in bots plug in.
//
// A scripted player sees exactly what a connected client sees and acts only
// by emitting what a client sends: protocol `Command`s and `PolicyUpdate`s.
// A `Seat` runs them through `GameEngine::execute` and
// `handle_policy_update` like any other player's. There is no other way to
// touch the world, so a bot cannot cheat by construction.
//
// A seat is an in-process client. It keeps the player's `GameState` the way
// a connected client does: the full state once, then every tick's
// `tick_update` folded in, so a player that acts tick after tick costs what
// a client's update costs and never a copy of the whole chart. A player that
// is not `watching` keeps no view; its next turn starts from a full state.
//
// A host (the headless runner, the server, a WebAssembly page) keeps one
// seat per bot and calls, in a fixed seat order:
//   before `GameEngine::tick`   `take_turn`  act on the state, if it wants to
//   after it                    `observe`    fold the tick in
// Per turn:
//   1. `wants_turn(tick)`  -- cheap; false leaves the player alone
//   2. `act(&state, &mut orders)` -- observe, push orders
//   3. the seat executes the orders in order; `hear` gets any reply
// Refused orders are counted, not fatal: a bot learns by looking at the next
// state. What a player carries between turns must be deterministic (no
// clocks, no OS randomness; randomness is seeded from what `seated` hands
// over and the tick) so a run replays from the world seed.

use iac_shared::protocol::{Command, GameState, PolicyUpdate, ServerMessage, TickUpdate};

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

    /// Whether the player will look at the world at `tick` or right after
    /// it. Its seat then follows the world by tick updates; otherwise the
    /// view is dropped and the next turn starts from a full state, which
    /// costs less than folding more than two or three updates nobody reads.
    fn watching(&self, tick: u64) -> bool {
        let _ = tick;
        true
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

impl<P: ScriptedPlayer + ?Sized> ScriptedPlayer for Box<P> {
    fn name(&self) -> String {
        (**self).name()
    }

    fn seated(&mut self, world_seed: u64, player_id: u64) {
        (**self).seated(world_seed, player_id);
    }

    fn watching(&self, tick: u64) -> bool {
        (**self).watching(tick)
    }

    fn wants_turn(&self, tick: u64) -> bool {
        (**self).wants_turn(tick)
    }

    fn act(&mut self, state: &GameState, orders: &mut Vec<Order>) {
        (**self).act(state, orders);
    }

    fn hear(&mut self, reply: &ServerMessage) {
        (**self).hear(reply);
    }
}

/// A scripted player bound to its account in one world.
pub struct Seat<P> {
    player_id: u64,
    player: P,
    /// The player's state as of the last tick, while it is being followed.
    view: Option<GameState>,
    orders: Vec<Order>,
    turns: u64,
    accepted: u64,
    refused: u64,
}

impl<P: ScriptedPlayer> Seat<P> {
    /// Register the player's account (or log in to it with `token`) and seat it.
    pub fn join(
        engine: &mut GameEngine,
        player: P,
        token: Option<&str>,
        new_token: impl FnOnce() -> String,
    ) -> Result<Seat<P>, AuthError> {
        let granted = engine.authenticate(&player.name(), token, true, new_token)?;
        Ok(Seat::resume(engine, granted.player_id, player))
    }

    /// Seat a player on an account the host already knows is its own.
    pub fn resume(engine: &GameEngine, player_id: u64, mut player: P) -> Seat<P> {
        player.seated(engine.world_gen.world_seed, player_id);
        Seat { player_id, player, view: None, orders: Vec::new(), turns: 0, accepted: 0, refused: 0 }
    }

    pub fn player_id(&self) -> u64 {
        self.player_id
    }

    pub fn player(&self) -> &P {
        &self.player
    }

    /// The state the player would act on now, if its seat is following the world.
    pub fn view(&self) -> Option<&GameState> {
        self.view.as_ref()
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
    /// True if it took a turn.
    pub fn take_turn(&mut self, engine: &mut GameEngine) -> bool {
        let now = engine.current_tick();
        if !self.player.wants_turn(now + 1) {
            return false;
        }
        if self.view.as_ref().is_none_or(|view| view.tick != now) {
            self.view = views::full_state(engine, self.player_id);
        }
        let Some(state) = &self.view else { return false };
        self.turns += 1;
        self.orders.clear();
        self.player.act(state, &mut self.orders);
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
        true
    }

    /// Fold the tick just simulated into the player's view. Call after every
    /// `GameEngine::tick`: a view that misses one is dropped and rebuilt.
    pub fn observe(&mut self, engine: &GameEngine) {
        let now = engine.current_tick();
        let following = self.player.watching(now + 1) && self.view.as_ref().is_some_and(|view| view.tick + 1 == now);
        match &mut self.view {
            Some(view) if following => apply(view, views::tick_update(engine, self.player_id, &[])),
            _ => self.view = None,
        }
    }
}

/// What a client does with a `tick_update`.
fn apply(state: &mut GameState, update: TickUpdate) {
    state.tick = update.tick;
    if let Some(player) = update.player {
        state.player = player;
    }
    state.fleets = update.fleets;
    if let Some(homeworld) = update.homeworld_update {
        state.homeworld = homeworld;
    }
    for sector in update.sector_updates.into_iter().flatten() {
        let key = sector.location.to_key();
        match state.known_sectors.binary_search_by_key(&key, |s| s.location.to_key()) {
            Ok(at) => state.known_sectors[at] = sector,
            Err(at) => state.known_sectors.insert(at, sector),
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

    fn watching(&self, _tick: u64) -> bool {
        false
    }

    fn wants_turn(&self, _tick: u64) -> bool {
        false
    }

    fn act(&mut self, _state: &GameState, _orders: &mut Vec<Order>) {}
}
