// The host core: everything between a world and those who play it that is
// the same wherever the world runs. The server, the single-player page and
// the headless runner all go through here, so there is one tick, one gate
// and one way to seat a world.
//
// One tick, in the only order there is (`step`):
//   the cast takes its turns -> `GameEngine::tick` -> its events are drained
//   -> every logged-in session's `tick_update` -> the cast observes
//   -> what changed goes to storage
// A host that needs to look between two of those gives a `Watch`.
//
// One gate (`Host::receive`): a logged-in session's message is answered by
// `GameEngine::answer`; `auth` is put to the `Door`, and a session it admits
// is sent the result and then its full state; anything else from outside is
// `commands::not_logged_in`.
//
// What differs between hosts is the `Door` and nothing else here: `Tokens`
// for a world behind a network, `Commander` for a world with one player and
// no password. Transport, clocks and storage are the host's own.

use std::collections::BTreeMap;

use iac_shared::constants::{ECONOMY_VERSION, WORLDGEN_VERSION};
use iac_shared::protocol::{AuthRequest, AuthResult, ClientMessage, ErrorCode, GameEvent, ServerMessage};

use crate::bot::{Roster, Style};
use crate::engine::{AuthError, GameEngine};
use crate::script::{ScriptedPlayer, Seat};
use crate::{commands, views};

pub type SessionId = u64;
pub type TickError = Box<dyn std::error::Error>;

/// Those who play a world from inside its host.
pub trait Cast {
    /// Everyone who wants one takes a turn on the world as the last tick left it.
    fn take_turns(&mut self, engine: &mut GameEngine);

    /// Everyone following the world sees the tick just simulated.
    fn observe(&mut self, engine: &GameEngine);
}

impl<P: ScriptedPlayer> Cast for Vec<Seat<P>> {
    fn take_turns(&mut self, engine: &mut GameEngine) {
        for seat in self {
            seat.take_turn(engine);
        }
    }

    fn observe(&mut self, engine: &GameEngine) {
        for seat in self {
            seat.observe(engine);
        }
    }
}

/// The points of a tick a host may look at, each named for what has just
/// been done, in the order they come.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Turns,
    Tick,
    Frames,
    Observed,
    Persisted,
}

/// A host's look at each phase of a tick. `events` are the tick's own, empty
/// until it has been simulated.
pub trait Watch<C: ?Sized> {
    fn at(&mut self, phase: Phase, engine: &GameEngine, cast: &C, events: &[GameEvent]);
}

impl<C: ?Sized> Watch<C> for () {
    fn at(&mut self, _phase: Phase, _engine: &GameEngine, _cast: &C, _events: &[GameEvent]) {}
}

/// One tick of a hosted world; returns its events. `frames` is where the
/// sessions are sent what the tick owes them.
pub fn step<C: Cast>(
    engine: &mut GameEngine,
    cast: &mut C,
    watch: &mut impl Watch<C>,
    mut frames: impl FnMut(&GameEngine, &[GameEvent]),
) -> Result<Vec<GameEvent>, TickError> {
    cast.take_turns(engine);
    watch.at(Phase::Turns, engine, cast, &[]);
    engine.tick()?;
    let events = engine.drain_events();
    watch.at(Phase::Tick, engine, cast, &events);
    frames(engine, &events);
    watch.at(Phase::Frames, engine, cast, &events);
    cast.observe(engine);
    watch.at(Phase::Observed, engine, cast, &events);
    engine.persist_dirty_state();
    watch.at(Phase::Persisted, engine, cast, &events);
    Ok(events)
}

/// An account a `Door` let in.
#[derive(Debug)]
pub struct Admitted {
    pub player_id: u64,
    /// A token issued just now, shown to the client this once.
    pub token: Option<String>,
    pub message: Option<String>,
}

/// Who may log in to a world.
pub trait Door {
    fn admit(&mut self, engine: &mut GameEngine, auth: &AuthRequest) -> Result<Admitted, (ErrorCode, String)>;
}

/// Accounts with a password: a new name registers, a known one presents its
/// token. `0` mints the tokens, from entropy the host has.
pub struct Tokens<M>(pub M);

impl<M: FnMut() -> String> Door for Tokens<M> {
    fn admit(&mut self, engine: &mut GameEngine, auth: &AuthRequest) -> Result<Admitted, (ErrorCode, String)> {
        let granted = engine
            .authenticate(&auth.player_name, auth.token.as_deref(), auth.agent, &mut self.0)
            .map_err(|refusal| commands::auth_failure(&auth.player_name, refusal))?;
        let message = match (&granted.new_token, granted.registered) {
            (Some(_), true) => Some("account created; keep this token, it is your password".to_string()),
            (Some(_), false) => Some("account claimed; keep this token, it is your password".to_string()),
            (None, _) => None,
        };
        Ok(Admitted { player_id: granted.player_id, token: granted.new_token, message })
    }
}

/// A world with one player: its commander's name logs in, nothing else
/// does, and no token is asked for or handed out.
pub struct Commander(pub u64);

impl Door for Commander {
    fn admit(&mut self, engine: &mut GameEngine, auth: &AuthRequest) -> Result<Admitted, (ErrorCode, String)> {
        let name = &engine.players[&self.0].name;
        if !auth.player_name.eq_ignore_ascii_case(name) {
            let message = format!("this world belongs to {name}: a single-player world has one commander");
            return Err((ErrorCode::InvalidName, message));
        }
        Ok(Admitted { player_id: self.0, token: None, message: None })
    }
}

/// What became of a session's message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Heard {
    /// The session logged in to this account with it.
    LoggedIn(u64),
    /// It came from a session logged in to this account.
    Answered(u64),
    /// It came from outside and the session stays there.
    Outside,
}

/// A world, its bots and the sessions logged in to it.
pub struct Host<D> {
    engine: GameEngine,
    roster: Roster,
    door: D,
    sessions: BTreeMap<SessionId, u64>,
}

impl<D: Door> Host<D> {
    /// Host `engine` with the bots it holds, nobody logged in.
    pub fn new(engine: GameEngine, door: D) -> Host<D> {
        let roster = Roster::resume(&engine);
        Host { engine, roster, door, sessions: BTreeMap::new() }
    }

    pub fn engine(&self) -> &GameEngine {
        &self.engine
    }

    pub fn engine_mut(&mut self) -> &mut GameEngine {
        &mut self.engine
    }

    pub fn roster(&self) -> &Roster {
        &self.roster
    }

    /// See `Roster::enlist`.
    pub fn enlist(
        &mut self,
        wanted: &[Style],
        keep_free: usize,
        new_token: impl FnMut() -> String,
    ) -> (usize, Option<AuthError>) {
        self.roster.enlist(&mut self.engine, wanted, keep_free, new_token)
    }

    /// The account `session` is logged in to.
    pub fn player_of(&self, session: SessionId) -> Option<u64> {
        self.sessions.get(&session).copied()
    }

    /// `session` is gone, or has to log in again.
    pub fn close(&mut self, session: SessionId) {
        self.sessions.remove(&session);
    }

    /// One message from `session`. What it is sent back goes to `out`, in order.
    pub fn receive(&mut self, session: SessionId, msg: ClientMessage, out: &mut Vec<ServerMessage>) -> Heard {
        match (self.player_of(session), msg) {
            (Some(player_id), msg) => {
                out.extend(self.engine.answer(player_id, msg));
                Heard::Answered(player_id)
            }
            (None, ClientMessage::Auth(auth)) => self.log_in(session, &auth, out),
            (None, msg) => {
                out.extend(commands::not_logged_in(&msg));
                Heard::Outside
            }
        }
    }

    fn log_in(&mut self, session: SessionId, auth: &AuthRequest, out: &mut Vec<ServerMessage>) -> Heard {
        let admitted = match self.door.admit(&mut self.engine, auth) {
            Ok(admitted) => admitted,
            Err((code, message)) => {
                out.push(ServerMessage::AuthResult(AuthResult {
                    success: false,
                    player_id: None,
                    token: None,
                    code: Some(code),
                    message: Some(message),
                }));
                return Heard::Outside;
            }
        };
        let player_id = admitted.player_id;
        self.sessions.insert(session, player_id);
        out.push(ServerMessage::AuthResult(AuthResult {
            success: true,
            player_id: Some(player_id),
            token: admitted.token,
            code: None,
            message: admitted.message,
        }));
        out.extend(self.engine.answer(player_id, ClientMessage::RequestFullState));
        Heard::LoggedIn(player_id)
    }

    /// One tick; every logged-in session's `tick_update` goes to `send`.
    pub fn step(
        &mut self,
        watch: &mut impl Watch<Roster>,
        mut send: impl FnMut(SessionId, ServerMessage),
    ) -> Result<Vec<GameEvent>, TickError> {
        let sessions = &self.sessions;
        step(&mut self.engine, &mut self.roster, watch, |engine, events| {
            for (&session, &player_id) in sessions {
                send(session, ServerMessage::TickUpdate(views::tick_update(engine, player_id, events)));
            }
        })
    }

    /// One tick no session is shown: a world catching up on time it missed.
    pub fn step_unseen(&mut self, watch: &mut impl Watch<Roster>) -> Result<Vec<GameEvent>, TickError> {
        step(&mut self.engine, &mut self.roster, watch, |_, _| {})
    }
}

/// Why this build does not play a stored world.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stale {
    Economy { found: u32 },
    Worldgen { found: u32 },
}

impl Stale {
    /// Whether a world stored under these versions is one this build plays.
    pub fn check(economy_version: u32, worldgen_version: u32) -> Result<(), Stale> {
        if economy_version != ECONOMY_VERSION {
            return Err(Stale::Economy { found: economy_version });
        }
        if worldgen_version != WORLDGEN_VERSION {
            return Err(Stale::Worldgen { found: worldgen_version });
        }
        Ok(())
    }

    /// The refusal, for a world kept as a `stored` ("save", "database"). The
    /// host adds what its player or operator can do about it.
    pub fn sentence(&self, stored: &str) -> String {
        match self {
            Stale::Economy { found } => {
                format!("this {stored} is economy version {found}, this build runs version {ECONOMY_VERSION}")
            }
            Stale::Worldgen { found } => format!(
                "this world was generated with worldgen version {found}, this build runs version {WORLDGEN_VERSION}"
            ),
        }
    }
}
