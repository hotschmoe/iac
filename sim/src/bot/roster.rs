// The bots of one world: their seats, and their records in the engine.
//
// The engine keeps a `BotRecord` per bot account (style and memory) as part
// of the world, so it is in every snapshot and goes to storage with the
// rest. The roster is what a host holds beside the engine: it rebuilds the
// bots from those records, gives each its turn, and writes a record back
// whenever its bot's memory moved.
//
//   let mut roster = Roster::resume(&engine);          // bots the world has
//   roster.enlist(&mut engine, &wanted, keep_free, new_token);   // bots it should have
//   loop {
//       roster.take_turns(&mut engine);
//       engine.tick()?;
//       roster.observe(&engine);
//   }

use iac_shared::protocol::BOT_NAME_PREFIX;

use super::{Bot, BotRecord, Style};
use crate::engine::{AuthError, GameEngine};
use crate::script::Seat;

/// A name is tried under the next number this many times before giving up.
const NAME_TRIES: usize = 64;

#[derive(Default)]
pub struct Roster {
    seats: Vec<Seat<Bot>>,
}

impl Roster {
    /// Seats for every bot account the world holds, in account order.
    pub fn resume(engine: &GameEngine) -> Roster {
        let seats = engine
            .bots()
            .iter()
            .filter_map(|(&player_id, record)| {
                let name = engine.players.get(&player_id)?.name.clone();
                Some(Seat::resume(engine, player_id, Bot::from_record(name, record.clone())))
            })
            .collect();
        Roster { seats }
    }

    pub fn seats(&self) -> &[Seat<Bot>] {
        &self.seats
    }

    /// Bring the world's bots up to `wanted`: for each personality and
    /// attention, as many new accounts as the roster has fewer of than
    /// asked for, in the order asked. Bots already there stay, wanted or
    /// not. A bot is added only while more than `keep_free` home seats are
    /// free, so that many stay open for players. Returns how many were
    /// added, and the refusal that stopped it short if one did (`WorldFull`
    /// when the seats ran out).
    pub fn enlist(
        &mut self,
        engine: &mut GameEngine,
        wanted: &[Style],
        keep_free: usize,
        mut new_token: impl FnMut() -> String,
    ) -> (usize, Option<AuthError>) {
        let same = |a: Style, b: Style| a.personality == b.personality && a.attention == b.attention;
        let mut have: Vec<Style> = self.seats.iter().map(|seat| seat.player().style()).collect();
        let mut added = 0;
        for &style in wanted {
            if let Some(at) = have.iter().position(|&h| same(h, style)) {
                have.swap_remove(at);
                continue;
            }
            if engine.free_home_seats() <= keep_free {
                return (added, Some(AuthError::WorldFull));
            }
            match self.add(engine, style, new_token()) {
                Ok(()) => added += 1,
                Err(refusal) => return (added, Some(refusal)),
            }
        }
        (added, None)
    }

    fn add(&mut self, engine: &mut GameEngine, style: Style, token: String) -> Result<(), AuthError> {
        let first = engine.bots().len() + 1;
        let mut number = first;
        loop {
            let name = format!("{BOT_NAME_PREFIX}{}-{number}", style.personality.label());
            match engine.enlist_bot(&name, BotRecord::new(style), token.clone()) {
                Ok(player_id) => {
                    self.seats.push(Seat::resume(engine, player_id, Bot::new(name, style)));
                    return Ok(());
                }
                Err(AuthError::InvalidName(_)) if number + 1 < first + NAME_TRIES => number += 1,
                Err(refusal) => return Err(refusal),
            }
        }
    }

    /// Every bot that wants one takes its turn. Call before `GameEngine::tick`.
    pub fn take_turns(&mut self, engine: &mut GameEngine) {
        for seat in &mut self.seats {
            if seat.take_turn(engine) {
                engine.set_bot(seat.player_id(), seat.player().record());
            }
        }
    }

    /// Every bot that is following the world sees the tick just simulated.
    /// Call after `GameEngine::tick`.
    pub fn observe(&mut self, engine: &GameEngine) {
        for seat in &mut self.seats {
            seat.observe(engine);
        }
    }
}
