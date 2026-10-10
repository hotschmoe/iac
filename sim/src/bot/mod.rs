// Built-in bot players.
//
// A bot is a `ScriptedPlayer`: it reads the `GameState` a client gets and
// sends the orders a client sends. Two independent choices define one:
//
//   Personality  what it builds and what its fleets are for
//                (turtle, miner, raider, explorer, mixed)
//   Attention    when it is at the controls
//                check-in  a few short look-ins a day: queue, regroup, hand
//                          every fleet to a standing order, leave
//                hybrid    the same, and one look-in a day is a piloting
//                          session flown by hand
//                hands-on  always there: fleets are flown by hand, no
//                          standing orders
//
// The home planner (`home.rs`) is the same for every attention pattern; only
// how often it runs and how far ahead it queues differ. Fleet handling
// (`field.rs`) either assigns doctrines or pilots. A bot reads its own row
// of the leaderboard, and the wings that fly for points stay home once
// combat and exploration have filled the score cap.
//
// Memory is small and rebuildable: a bot seated again on its account (after
// a host restart) starts with a look-in and relearns the rest from the
// state. Randomness (look-in times) is hashed from the world seed, the
// player id and the day.

mod field;
mod home;

use iac_shared::protocol::{Command, GameState, ServerMessage};

use crate::score::EXTRA_CAP;
use crate::script::{Order, ScriptedPlayer};

pub const DAY: u64 = 86_400;
/// A look-in: long enough to queue, call fleets home, merge and send them out.
const SESSION_TICKS: u64 = 300;
/// The hybrid player's daily piloting session.
const PILOT_TICKS: u64 = 3_600;
/// Look-ins are spread over this much of the day; the rest is sleep.
const WAKING_TICKS: u64 = 57_600;
pub const DEFAULT_SESSIONS_PER_DAY: u64 = 4;
pub const MAX_SESSIONS_PER_DAY: u64 = 24;
/// How often a present player revisits the homeworld queues.
const PLAN_PERIOD: u64 = 20;
/// Points are chased until they exceed the cap by this factor: the cap
/// rises with the empire, so a little is banked ahead.
const CAP_HEADROOM: f32 = 1.2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Personality {
    Turtle,
    Miner,
    Raider,
    Explorer,
    Mixed,
}

impl Personality {
    pub const ALL: [Personality; 5] =
        [Personality::Turtle, Personality::Miner, Personality::Raider, Personality::Explorer, Personality::Mixed];

    pub fn label(self) -> &'static str {
        match self {
            Personality::Turtle => "turtle",
            Personality::Miner => "miner",
            Personality::Raider => "raider",
            Personality::Explorer => "explorer",
            Personality::Mixed => "mixed",
        }
    }

    pub fn parse(text: &str) -> Option<Personality> {
        Personality::ALL.into_iter().find(|p| p.label() == text)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Attention {
    CheckIn,
    Hybrid,
    HandsOn,
}

impl Attention {
    pub const ALL: [Attention; 3] = [Attention::CheckIn, Attention::Hybrid, Attention::HandsOn];

    pub fn label(self) -> &'static str {
        match self {
            Attention::CheckIn => "check-in",
            Attention::Hybrid => "hybrid",
            Attention::HandsOn => "hands-on",
        }
    }

    pub fn parse(text: &str) -> Option<Attention> {
        Attention::ALL.into_iter().find(|a| a.label() == text)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Presence {
    Away,
    /// Queues and standing orders; `closing` is the last turn of the look-in.
    Managing { closing: bool },
    /// Queues and fleets flown by hand.
    Piloting { closing: bool },
}

pub struct Bot {
    name: String,
    personality: Personality,
    attention: Attention,
    sessions_per_day: u64,
    world_seed: u64,
    player_id: u64,
    /// End of the look-in a bot holds when it first sees its empire.
    intro_until: Option<u64>,
    wake: u64,
    next_plan: u64,
    /// The bot's own row of the leaderboard, as last asked.
    standing: Option<Standing>,
    ignore_cap: bool,
    log: field::Logbook,
}

#[derive(Debug, Clone, Copy)]
struct Standing {
    core: f32,
    /// Combat plus exploration points, before the cap.
    extra: f32,
}

impl Bot {
    pub fn new(name: impl Into<String>, personality: Personality, attention: Attention) -> Bot {
        Bot {
            name: name.into(),
            personality,
            attention,
            sessions_per_day: DEFAULT_SESSIONS_PER_DAY,
            world_seed: 0,
            player_id: 0,
            intro_until: None,
            wake: 0,
            next_plan: 0,
            standing: None,
            ignore_cap: false,
            log: field::Logbook::default(),
        }
    }

    /// Look-ins per day of the check-in and hybrid patterns.
    pub fn sessions_per_day(mut self, sessions: u64) -> Bot {
        self.sessions_per_day = sessions.clamp(1, MAX_SESSIONS_PER_DAY);
        self
    }

    /// Fly for combat and exploration points whatever the score cap says.
    /// Measures what the map offers; a bot that heeds the cap stops short.
    pub fn ignore_cap(mut self, ignore: bool) -> Bot {
        self.ignore_cap = ignore;
        self
    }

    /// Whether more combat or exploration points would still raise the score.
    fn points_wanted(&self) -> bool {
        self.ignore_cap || self.standing.is_none_or(|s| s.extra < CAP_HEADROOM * EXTRA_CAP * s.core)
    }

    fn hash(&self, a: u64, b: u64) -> u64 {
        mix(mix(mix(self.world_seed, self.player_id), a), b)
    }

    /// The player's own clock: every bot keeps a different time of day.
    fn local(&self, tick: u64) -> u64 {
        tick + self.hash(0, 0) % DAY
    }

    /// Start, length and kind of look-in `k` on local day `day`, in local ticks.
    fn session(&self, day: u64, k: u64) -> (u64, u64, bool) {
        let gap = WAKING_TICKS / self.sessions_per_day;
        let start = day * DAY + k * gap + self.hash(day + 1, k) % (gap / 4);
        let piloting = self.attention == Attention::Hybrid && k == 0;
        let length = if piloting { PILOT_TICKS.min(gap / 2) } else { SESSION_TICKS };
        (start, length, piloting)
    }

    fn presence(&self, tick: u64) -> Presence {
        if self.attention == Attention::HandsOn {
            return Presence::Piloting { closing: false };
        }
        match self.intro_until {
            None => return Presence::Managing { closing: false },
            Some(until) if tick < until => return Presence::Managing { closing: tick + 1 == until },
            Some(_) => {}
        }
        let local = self.local(tick);
        for k in 0..self.sessions_per_day {
            let (start, length, piloting) = self.session(local / DAY, k);
            if (start..start + length).contains(&local) {
                let closing = local + 1 == start + length;
                return if piloting { Presence::Piloting { closing } } else { Presence::Managing { closing } };
            }
        }
        Presence::Away
    }

    /// Ticks from `tick` until the next look-in begins.
    fn absence(&self, tick: u64) -> u64 {
        let local = self.local(tick);
        let day = local / DAY;
        (day..=day + 1)
            .flat_map(|d| (0..self.sessions_per_day).map(move |k| (d, k)))
            .map(|(d, k)| self.session(d, k).0)
            .find(|&start| start > local)
            .map_or(DAY, |start| start - local)
    }
}

fn mix(seed: u64, value: u64) -> u64 {
    let mut z = seed ^ value.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl ScriptedPlayer for Bot {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn seated(&mut self, world_seed: u64, player_id: u64) {
        self.world_seed = world_seed;
        self.player_id = player_id;
    }

    fn wants_turn(&self, tick: u64) -> bool {
        match self.presence(tick) {
            Presence::Away => false,
            Presence::Managing { .. } => true,
            Presence::Piloting { closing } => closing || tick >= self.wake,
        }
    }

    fn act(&mut self, state: &GameState, orders: &mut Vec<Order>) {
        let turn = state.tick + 1;
        if self.intro_until.is_none() {
            self.intro_until = Some(turn + SESSION_TICKS);
        }
        let presence = self.presence(turn);
        let closing = matches!(presence, Presence::Managing { closing: true } | Presence::Piloting { closing: true });
        if presence == Presence::Away {
            return;
        }

        let away_for = (self.attention != Attention::HandsOn).then(|| self.absence(turn));
        if closing || turn >= self.next_plan {
            home::plan(state, self.personality, away_for, orders);
            orders.push(Order::Command(Command::Leaderboard { limit: 1 }));
            self.next_plan = turn + PLAN_PERIOD;
        }

        self.log.observe(state);
        let brief = field::Brief {
            personality: self.personality,
            attention: self.attention,
            away_for,
            points_wanted: self.points_wanted(),
        };
        let mut admiral = field::Admiral::new(state, brief, &mut self.log, orders);
        let idle_for = match presence {
            Presence::Managing { closing } => {
                admiral.manage(closing);
                1
            }
            Presence::Piloting { closing } => admiral.pilot(closing),
            Presence::Away => 1,
        };
        self.wake = turn + idle_for.clamp(1, PLAN_PERIOD);
    }

    fn hear(&mut self, reply: &ServerMessage) {
        if let ServerMessage::Leaderboard(board) = reply
            && let Some(row) = board.entries.iter().chain(&board.you).find(|row| row.name == self.name)
        {
            self.standing = Some(Standing { core: row.core, extra: row.combat + row.explore });
        }
    }
}
