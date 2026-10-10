// One single-player world: the engine, its bots, the one commander, and the
// clock that says how many ticks the wall time has earned.
//
// It hosts the engine as the server does, in the same order per tick:
//   bots take turns -> `GameEngine::tick` -> the session's `tick_update`
//   -> bots observe
// and a logged-in session's messages are answered by `GameEngine::answer`,
// so above the transport a client sees what a server would send. Two things
// differ, both because there is one player and no network:
//   - the account has no password: the world knows its commander, logs that
//     name in and hands out no token;
//   - the world is behind whenever the page was not running. Tick `t` is due
//     at `anchor_ms + (t - anchor_tick)` seconds of wall time, wherever the
//     host says the wall clock stands. A few ticks behind are played one
//     frame each. More is an absence: the missed ticks are played without
//     frames, bots and standing orders acting on every one as if the page
//     had stayed open, up to `CATCH_UP_LIMIT`; then the session gets a full
//     state and alerts that say what happened.

use iac_shared::constants::TICK_RATE_HZ;
use iac_shared::pace::Pace;
use iac_shared::protocol::{AuthRequest, AuthResult, ClientMessage, ErrorCode, EventKind, GameEvent, ServerMessage};
use iac_sim::bot::{Roster, Style};
use iac_sim::engine::AuthError;
use iac_sim::{commands, views, GameEngine};
use serde::{Deserialize, Serialize};

use crate::away::{Absence, Tally};
use crate::save::{self, Bookmark};

pub const TICK_MS: u64 = 1000 / TICK_RATE_HZ;
/// Ticks the world may be behind and still be played one frame each.
const LIVE_BURST: u64 = 5;
/// The most an absence is caught up: a week. The rest never happens.
pub const CATCH_UP_LIMIT: u64 = 7 * 86_400 * TICK_RATE_HZ;
/// A shorter catch-up is a stalled page, not an absence: nothing is said.
const REPORT_MIN_TICKS: u64 = 300 * TICK_RATE_HZ;
/// A world nobody gives orders to is saved this often. What a lost save
/// costs is only time: the ticks since the last one are played again on
/// reload and come out the same, since nothing but orders enters the world.
/// Kept under `REPORT_MIN_TICKS` so a reload is never taken for an absence.
const SAVE_PERIOD_MS: u64 = 240_000;
/// While catching up a lost save costs real work, so it is saved this often.
const CATCH_UP_SAVE_MS: u64 = 20_000;
/// An order is the one thing a reload cannot recompute: it is saved no
/// later than this after it was given.
const ORDER_SAVE_MS: u64 = 2_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Clock {
    anchor_ms: u64,
    anchor_tick: u64,
}

impl Clock {
    fn due(&self, now_ms: u64) -> u64 {
        self.anchor_tick + now_ms.saturating_sub(self.anchor_ms) / TICK_MS
    }
}

/// An absence being played.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CatchUp {
    from_tick: u64,
    over_limit: u64,
    tally: Tally,
}

#[derive(Debug, Serialize)]
pub struct Progress {
    pub played: u64,
    pub remaining: u64,
}

/// What a host shows of a world without logging in to it.
#[derive(Debug, Serialize)]
pub struct Status {
    pub commander: String,
    pub pace: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset: Option<&'static str>,
    pub seed: String,
    pub tick: u64,
    pub bots: usize,
    /// Ticks the wall clock is ahead of the world.
    pub behind: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catch_up: Option<Progress>,
    /// What the last absence did, until the commander has been told.
    pub notices: Vec<String>,
    pub save_due: bool,
}

pub struct World {
    engine: GameEngine,
    roster: Roster,
    commander: u64,
    clock: Clock,
    logged_in: bool,
    catch_up: Option<CatchUp>,
    notices: Vec<GameEvent>,
    saved_at_ms: u64,
    /// Something happened that should not wait for the next periodic save.
    save_soon: bool,
}

impl World {
    /// A new world whose first account is the commander's, then `bots`.
    /// `mint` supplies account tokens: nothing presents them again, they
    /// only lock the accounts.
    pub fn create(
        seed: u64,
        pace: Pace,
        name: &str,
        bots: &[Style],
        now_ms: u64,
        mut mint: impl FnMut() -> String,
    ) -> Result<World, String> {
        let mut engine = GameEngine::new(seed, pace);
        let commander = engine
            .authenticate(name, None, false, &mut mint)
            .map_err(|refusal| commands::auth_failure(name, refusal).1)?
            .player_id;
        let mut roster = Roster::default();
        match roster.enlist(&mut engine, bots, 0, &mut mint) {
            (_, None) => {}
            (added, Some(AuthError::WorldFull)) => {
                return Err(format!(
                    "{} bots asked for, but the world has home seats for {added} beside yours",
                    bots.len()
                ));
            }
            (_, Some(refusal)) => return Err(format!("could not seat the bots: {refusal:?}")),
        }
        Ok(World {
            engine,
            roster,
            commander,
            clock: Clock { anchor_ms: now_ms, anchor_tick: 0 },
            logged_in: false,
            catch_up: None,
            notices: Vec::new(),
            saved_at_ms: 0,
            save_soon: true,
        })
    }

    /// The world a save document holds, or why this build refuses it.
    pub fn load(document: &[u8]) -> Result<World, String> {
        let (bookmark, snapshot) = save::decode(document)?;
        let engine = GameEngine::restore(snapshot);
        let roster = Roster::resume(&engine);
        Ok(World {
            engine,
            roster,
            commander: bookmark.commander,
            clock: bookmark.clock,
            logged_in: false,
            catch_up: bookmark.catch_up,
            notices: bookmark.notices,
            saved_at_ms: bookmark.saved_at_ms,
            save_soon: false,
        })
    }

    pub fn save(&mut self, now_ms: u64) -> Vec<u8> {
        self.saved_at_ms = now_ms;
        self.save_soon = false;
        let bookmark = Bookmark {
            commander: self.commander,
            clock: self.clock,
            saved_at_ms: now_ms,
            catch_up: self.catch_up,
            notices: self.notices.clone(),
        };
        save::encode(bookmark, &self.engine.snapshot())
    }

    pub fn status(&self, now_ms: u64) -> Status {
        let tick = self.engine.current_tick();
        let behind = self.clock.due(now_ms).saturating_sub(tick);
        let since_save = now_ms.saturating_sub(self.saved_at_ms);
        let period = if self.catch_up.is_some() { CATCH_UP_SAVE_MS } else { SAVE_PERIOD_MS };
        Status {
            commander: self.engine.players[&self.commander].name.clone(),
            pace: self.engine.pace().value(),
            preset: self.engine.pace().preset().map(|p| p.name),
            seed: format!("{:016X}", self.engine.world_gen.world_seed),
            tick,
            bots: self.roster.seats().len(),
            behind,
            catch_up: self.catch_up.map(|c| Progress { played: tick - c.from_tick, remaining: behind }),
            notices: self.notices.iter().filter_map(alert_text).collect(),
            save_due: since_save >= period || (self.save_soon && since_save >= ORDER_SAVE_MS),
        }
    }

    /// A client connected: it has yet to log in.
    pub fn open(&mut self) {
        self.logged_in = false;
    }

    pub fn close(&mut self) {
        self.logged_in = false;
    }

    /// One frame from the client. Its answers go to `out`.
    pub fn receive(&mut self, frame: &[u8], out: &mut Vec<ServerMessage>) -> Result<(), String> {
        let msg: ClientMessage = serde_json::from_slice(frame).map_err(|e| format!("unreadable frame: {e}"))?;
        match (self.logged_in, msg) {
            (true, msg) => {
                self.save_soon |= match &msg {
                    ClientMessage::Command(cmd) => !commands::is_query(cmd),
                    ClientMessage::PolicyUpdate(_) => true,
                    ClientMessage::Auth(_) | ClientMessage::RequestFullState => false,
                };
                out.extend(self.engine.answer(self.commander, msg));
            }
            (false, ClientMessage::Auth(auth)) => self.log_in(&auth, out),
            (false, msg) => out.extend(commands::not_logged_in(&msg)),
        }
        Ok(())
    }

    fn log_in(&mut self, auth: &AuthRequest, out: &mut Vec<ServerMessage>) {
        let name = &self.engine.players[&self.commander].name;
        if !auth.player_name.eq_ignore_ascii_case(name) {
            out.push(ServerMessage::AuthResult(AuthResult {
                success: false,
                player_id: None,
                token: None,
                code: Some(ErrorCode::InvalidName),
                message: Some(format!("this world belongs to {name}: a single-player world has one commander")),
            }));
            return;
        }
        self.logged_in = true;
        out.push(ServerMessage::AuthResult(AuthResult {
            success: true,
            player_id: Some(self.commander),
            token: None,
            code: None,
            message: None,
        }));
        out.extend(self.engine.answer(self.commander, ClientMessage::RequestFullState));
        if self.catch_up.is_none() {
            self.tell(out);
        }
    }

    /// Hand the session what it has not been told about the last absence.
    fn tell(&mut self, out: &mut Vec<ServerMessage>) {
        self.save_soon |= !self.notices.is_empty();
        out.extend(self.notices.drain(..).map(ServerMessage::Event));
    }

    /// Play the ticks that are due at `now_ms`, at most `max_ticks` of them.
    /// Frames for a logged-in session go to `out`.
    pub fn advance(&mut self, now_ms: u64, max_ticks: u64, out: &mut Vec<ServerMessage>) -> Result<(), String> {
        let tick = self.engine.current_tick();
        let mut owed = self.clock.due(now_ms).saturating_sub(tick);
        let over_limit = owed.saturating_sub(CATCH_UP_LIMIT);
        if over_limit > 0 {
            self.clock = Clock { anchor_ms: now_ms.saturating_sub(CATCH_UP_LIMIT * TICK_MS), anchor_tick: tick };
            owed = CATCH_UP_LIMIT;
        }
        if owed > LIVE_BURST {
            let catch_up =
                self.catch_up.get_or_insert(CatchUp { from_tick: tick, over_limit: 0, tally: Tally::default() });
            catch_up.over_limit += over_limit;
        }

        let run = owed.min(max_ticks);
        if self.catch_up.is_none() {
            for _ in 0..run {
                let events = self.step()?;
                if self.logged_in {
                    out.push(ServerMessage::TickUpdate(views::tick_update(&self.engine, self.commander, &events)));
                }
            }
            return Ok(());
        }
        for _ in 0..run {
            let events = self.step()?;
            if let (Some(catch_up), Some(commander)) = (&mut self.catch_up, self.engine.players.get(&self.commander)) {
                for event in &events {
                    catch_up.tally.count(event, commander, &self.engine);
                }
            }
        }
        if run == owed {
            self.arrive(0, out);
        }
        Ok(())
    }

    /// Stop catching up: the world is played from where it stands and the
    /// ticks still missing are dropped.
    pub fn take_command(&mut self, now_ms: u64, out: &mut Vec<ServerMessage>) {
        if self.catch_up.is_none() {
            return;
        }
        let tick = self.engine.current_tick();
        let cut_short = self.clock.due(now_ms).saturating_sub(tick);
        self.clock = Clock { anchor_ms: now_ms, anchor_tick: tick };
        self.arrive(cut_short, out);
    }

    /// The catch-up is over: word it, and bring a logged-in session up to date.
    fn arrive(&mut self, cut_short: u64, out: &mut Vec<ServerMessage>) {
        let Some(catch_up) = self.catch_up.take() else { return };
        let tick = self.engine.current_tick();
        let played = tick - catch_up.from_tick;
        if played >= REPORT_MIN_TICKS || catch_up.over_limit > 0 || cut_short > 0 {
            let absence = Absence { played, over_limit: catch_up.over_limit, cut_short, tally: catch_up.tally };
            self.notices.extend(absence.notices(self.commander, tick));
        }
        self.save_soon = true;
        if self.logged_in {
            out.extend(self.engine.answer(self.commander, ClientMessage::RequestFullState));
            self.tell(out);
        }
    }

    fn step(&mut self) -> Result<Vec<GameEvent>, String> {
        self.roster.take_turns(&mut self.engine);
        self.engine.tick().map_err(|e| format!("tick {} failed: {e}", self.engine.current_tick()))?;
        let events = self.engine.drain_events();
        self.roster.observe(&self.engine);
        self.engine.persist_dirty_state();
        Ok(events)
    }
}

fn alert_text(event: &GameEvent) -> Option<String> {
    match &event.kind {
        EventKind::Alert(alert) => Some(alert.message.clone()),
        _ => None,
    }
}
