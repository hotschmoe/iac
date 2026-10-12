// One single-player world: a hosted world (`iac_sim::host`), its one
// commander, and the clock that says how many ticks the wall time has earned.
//
// The tick and the gate are the host core's, the ones the server runs, so
// above the transport a client sees what a server would send. Two things
// differ, both because there is one player and no network:
//   - the account has no password: the world's door is `Commander`, which
//     logs that name in and hands out no token;
//   - the world is behind whenever the page was not running. Tick `t` is due
//     at `anchor_ms + (t - anchor_tick)` seconds of wall time, wherever the
//     host says the wall clock stands. A few ticks behind are played one
//     frame each. More is an absence: the missed ticks are played without
//     frames, bots and standing orders acting on every one as if the page
//     had stayed open, up to `CATCH_UP_LIMIT`; then the session gets a full
//     state and alerts that say what happened.

use iac_shared::constants::TICK_RATE_HZ;
use iac_shared::pace::Pace;
use iac_shared::protocol::{ClientMessage, EventKind, GameEvent, ServerMessage};
use iac_sim::bot::Style;
use iac_sim::engine::AuthError;
use iac_sim::host::{Commander, Heard, SessionId};
use iac_sim::store::{self, Due, Unread};
use iac_sim::{commands, GameEngine, Host};
use serde::{Deserialize, Serialize};

use crate::away::{Absence, Standing, Tally};

pub const TICK_MS: u64 = 1000 / TICK_RATE_HZ;
/// Ticks the world may be behind and still be played one frame each.
const LIVE_BURST: u64 = 5;
/// The most an absence is caught up: a week. The rest never happens.
pub const CATCH_UP_LIMIT: u64 = 7 * 86_400 * TICK_RATE_HZ;
/// A shorter catch-up is a stalled page, not an absence: nothing is said.
const REPORT_MIN_TICKS: u64 = 300 * TICK_RATE_HZ;
/// A world nobody gives orders to is saved this often (`store::Due` says
/// when a save is owed; these are this host's three times). What a lost
/// save costs is only time: the ticks since the last one are played again
/// on reload and come out the same, since nothing but orders enters the
/// world. Kept under `REPORT_MIN_TICKS` so a reload is never taken for an
/// absence.
const SAVE_PERIOD_MS: u64 = 240_000;
/// While catching up a lost save costs real work, so it is saved this often.
const CATCH_UP_SAVE_MS: u64 = 20_000;
/// An order is the one thing a reload cannot recompute. Once one is given
/// the next save does not wait for its period, only this long after the
/// last save.
const ORDER_SAVE_MS: u64 = 2_000;
/// The page: the one session a single-player world has.
const SESSION: SessionId = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Clock {
    anchor_ms: u64,
    anchor_tick: u64,
}

impl Clock {
    /// The last tick that is due at `now_ms`.
    fn due(&self, now_ms: u64) -> u64 {
        self.anchor_tick + now_ms.saturating_sub(self.anchor_ms) / TICK_MS
    }

    /// When `tick` was due.
    fn at(&self, tick: u64) -> u64 {
        self.anchor_ms + tick.saturating_sub(self.anchor_tick) * TICK_MS
    }
}

/// An absence being played.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CatchUp {
    from_tick: u64,
    over_limit: u64,
    tally: Tally,
}

/// What this host keeps of a world beside its snapshot: the `host` part of
/// the save document (`iac_sim::store`).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Bookmark {
    /// The player's account.
    commander: u64,
    clock: Clock,
    saved_at_ms: u64,
    /// The absence being caught up when the save was written.
    catch_up: Option<CatchUp>,
    /// What the player has yet to be told about the last absence.
    notices: Vec<GameEvent>,
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
    host: Host<Commander>,
    commander: u64,
    clock: Clock,
    catch_up: Option<CatchUp>,
    notices: Vec<GameEvent>,
    /// When the next save is owed; hurried by what should not wait for the
    /// periodic one.
    save: Due,
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
        let mut host = Host::new(engine, Commander(commander));
        match host.enlist(bots, 0, &mut mint) {
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
            host,
            commander,
            clock: Clock { anchor_ms: now_ms, anchor_tick: 0 },
            catch_up: None,
            notices: Vec::new(),
            save: Due::never(),
        })
    }

    /// The world a save document holds, or why this build refuses it.
    pub fn load(document: &[u8]) -> Result<World, String> {
        let (bookmark, snapshot): (Bookmark, _) = store::decode(document).map_err(|unread| match unread {
            Unread::Stale(_) => format!("{}; start a new game", unread.sentence("save")),
            Unread::Damaged(_) => unread.sentence("save"),
        })?;
        if !snapshot.players.contains_key(&bookmark.commander) {
            return Err("this save cannot be read: its world does not hold its commander".to_string());
        }
        Ok(World {
            host: Host::new(GameEngine::restore(snapshot), Commander(bookmark.commander)),
            commander: bookmark.commander,
            clock: bookmark.clock,
            catch_up: bookmark.catch_up,
            notices: bookmark.notices,
            save: Due::since(bookmark.saved_at_ms),
        })
    }

    pub fn save(&mut self, now_ms: u64) -> Vec<u8> {
        self.save.stored(now_ms);
        let bookmark = Bookmark {
            commander: self.commander,
            clock: self.clock,
            saved_at_ms: now_ms,
            catch_up: self.catch_up,
            notices: self.notices.clone(),
        };
        store::encode(&bookmark, &self.host.engine().snapshot())
    }

    pub fn status(&self, now_ms: u64) -> Status {
        let engine = self.host.engine();
        let tick = engine.current_tick();
        let behind = self.clock.due(now_ms).saturating_sub(tick);
        let period = if self.catch_up.is_some() { CATCH_UP_SAVE_MS } else { SAVE_PERIOD_MS };
        Status {
            commander: engine.players[&self.commander].name.clone(),
            pace: engine.pace().value(),
            preset: engine.pace().preset().map(|p| p.name),
            seed: format!("{:016X}", engine.world_gen.world_seed),
            tick,
            bots: self.host.roster().seats().len(),
            behind,
            catch_up: self.catch_up.map(|c| Progress { played: tick - c.from_tick, remaining: behind }),
            notices: self.notices.iter().filter_map(alert_text).collect(),
            save_due: self.save.is_due(now_ms, period, ORDER_SAVE_MS),
        }
    }

    /// A client connected: it has yet to log in.
    pub fn open(&mut self) {
        self.host.close(SESSION);
    }

    pub fn close(&mut self) {
        self.host.close(SESSION);
    }

    /// One frame from the client. Its answers go to `out`.
    pub fn receive(&mut self, frame: &[u8], out: &mut Vec<ServerMessage>) -> Result<(), String> {
        let msg: ClientMessage = serde_json::from_slice(frame).map_err(|e| format!("unreadable frame: {e}"))?;
        let order = match &msg {
            ClientMessage::Command(cmd) => !commands::is_query(cmd),
            ClientMessage::PolicyUpdate(_) => true,
            ClientMessage::Auth(_) | ClientMessage::RequestFullState => false,
        };
        match self.host.receive(SESSION, msg, out) {
            Heard::Answered(_) if order => self.save.hurry(),
            Heard::LoggedIn(_) if self.catch_up.is_none() => self.tell(out),
            Heard::Answered(_) | Heard::LoggedIn(_) | Heard::Outside => {}
        }
        Ok(())
    }

    /// Hand the session what it has not been told about the last absence.
    fn tell(&mut self, out: &mut Vec<ServerMessage>) {
        if !self.notices.is_empty() {
            self.save.hurry();
        }
        out.extend(self.notices.drain(..).map(ServerMessage::Event));
    }

    /// Play the ticks that are due at `now_ms`, at most `max_ticks` of them.
    /// Frames for a logged-in session go to `out`.
    pub fn advance(&mut self, now_ms: u64, max_ticks: u64, out: &mut Vec<ServerMessage>) -> Result<(), String> {
        let tick = self.host.engine().current_tick();
        if now_ms + LIVE_BURST * TICK_MS < self.clock.at(tick) {
            // The wall clock was set back: the world carries on from here
            // instead of standing still until the clock has caught up.
            self.clock = Clock { anchor_ms: now_ms, anchor_tick: tick };
        }
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
                self.host.step(&mut (), |_, frame| out.push(frame)).map_err(|e| self.failed(e))?;
            }
            return Ok(());
        }
        for _ in 0..run {
            let events = self.host.step_unseen(&mut ()).map_err(|e| self.failed(e))?;
            let engine = self.host.engine();
            if let (Some(catch_up), Some(commander)) = (&mut self.catch_up, engine.players.get(&self.commander)) {
                for event in &events {
                    catch_up.tally.count(event, commander, engine);
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
        let tick = self.host.engine().current_tick();
        let cut_short = self.clock.due(now_ms).saturating_sub(tick);
        self.clock = Clock { anchor_ms: now_ms, anchor_tick: tick };
        self.arrive(cut_short, out);
    }

    /// The catch-up is over: word it, and bring a logged-in session up to date.
    fn arrive(&mut self, cut_short: u64, out: &mut Vec<ServerMessage>) {
        let Some(catch_up) = self.catch_up.take() else { return };
        let tick = self.host.engine().current_tick();
        let played = tick - catch_up.from_tick;
        if played >= REPORT_MIN_TICKS || catch_up.over_limit > 0 || cut_short > 0 {
            let absence = Absence {
                played,
                over_limit: catch_up.over_limit,
                cut_short,
                tally: catch_up.tally,
                outpost: self.host.engine().outposts.get(&self.commander).map(Standing::of),
            };
            self.notices.extend(absence.notices(self.commander, tick));
        }
        self.save.hurry();
        if self.host.player_of(SESSION).is_some() {
            self.host.receive(SESSION, ClientMessage::RequestFullState, out);
            self.tell(out);
        }
    }

    fn failed(&self, error: impl std::fmt::Display) -> String {
        format!("tick {} failed: {error}", self.host.engine().current_tick())
    }
}

fn alert_text(event: &GameEvent) -> Option<String> {
    match &event.kind {
        EventKind::Alert(alert) => Some(alert.message.clone()),
        _ => None,
    }
}
