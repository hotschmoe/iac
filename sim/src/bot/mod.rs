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
// A day of look-ins is a day of the clock up to the season's pace and
// shorter beyond it (`look_in_day`), so a check-in bot in a blitz world
// looks in a few times an hour, as a player who checks in on one would.
//
// The home planner (`home.rs`) is the same for every attention pattern; only
// how often it runs and how far ahead it queues differ. Fleet handling
// (`field.rs`) either assigns doctrines or pilots. A bot reads its own row
// of the leaderboard, and the wings that fly for points stay home once
// combat and exploration have filled the score cap.
//
// Everything a bot carries from one turn to the next is its `Memory`. With
// its `Style` that is a `BotRecord`, which the engine keeps per bot account
// as part of the world (`roster.rs`), so a world resumed from a snapshot,
// a server's file or a save plays on as if it had never stopped.
// Randomness (look-in times) is hashed from the world seed, the player id
// and the day.

mod field;
mod home;
mod roster;

use std::collections::BTreeMap;

use iac_shared::pace::{preset_named, Pace};
use iac_shared::protocol::{Command, GameState, ServerMessage};
use serde::{Deserialize, Serialize};

use crate::score::ExtraRule;
use crate::script::{Order, Outlook, ScriptedPlayer};

pub use roster::Roster;

pub const DAY: u64 = 86_400;
/// A look-in: long enough to queue, call fleets home, merge and send them
/// out, and never more than a quarter of the time between two.
const SESSION_TICKS: u64 = 300;
/// The hybrid player's daily piloting session, at most half of that time.
const PILOT_TICKS: u64 = 3_600;
/// Look-ins are spread over this much of a day; the rest is sleep.
const WAKING_TICKS: u64 = 57_600;
pub const DEFAULT_SESSIONS_PER_DAY: u64 = 4;
pub const MAX_SESSIONS_PER_DAY: u64 = 24;
/// How often a present player revisits the homeworld queues.
const PLAN_PERIOD: u64 = 20;
/// Points are chased until they exceed the cap by this factor: the cap
/// rises with the empire, so a little is banked ahead.
const CAP_HEADROOM: f32 = 1.2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
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

/// What kind of player a bot is. Fixed for the life of its account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Style {
    pub personality: Personality,
    pub attention: Attention,
    /// Look-ins per day of the check-in and hybrid patterns.
    pub sessions_per_day: u64,
    /// Fly for combat and exploration points whatever the score cap says.
    /// Measures what the map offers; a bot that heeds the cap stops short.
    pub ignore_cap: bool,
}

impl Style {
    pub fn new(personality: Personality, attention: Attention) -> Style {
        Style { personality, attention, sessions_per_day: DEFAULT_SESSIONS_PER_DAY, ignore_cap: false }
    }

    /// "explorer/check-in".
    pub fn label(&self) -> String {
        format!("{}/{}", self.personality.label(), self.attention.label())
    }

    /// The seats one spec asks for: `<personality>:<attention>[:<count>]`,
    /// where `all` stands for every personality or every attention.
    pub fn parse_seats(spec: &str) -> Result<Vec<Style>, String> {
        let parts: Vec<&str> = spec.split(':').collect();
        if !(2..=3).contains(&parts.len()) {
            return Err(format!("{spec}: expected <personality>:<attention>[:<count>]"));
        }
        let personalities = match parts[0] {
            "all" => Personality::ALL.to_vec(),
            name => vec![Personality::parse(name).ok_or_else(|| format!("{spec}: unknown personality '{name}'"))?],
        };
        let attentions = match parts[1] {
            "all" => Attention::ALL.to_vec(),
            name => vec![Attention::parse(name).ok_or_else(|| format!("{spec}: unknown attention '{name}'"))?],
        };
        let count = match parts.get(2) {
            None => 1,
            Some(text) => text.parse::<usize>().map_err(|_| format!("{spec}: '{text}' is not a count"))?,
        };
        let mut seats = Vec::new();
        for &attention in &attentions {
            for &personality in &personalities {
                seats.extend(std::iter::repeat_n(Style::new(personality, attention), count));
            }
        }
        Ok(seats)
    }
}

/// What a bot carries from one turn to the next.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Memory {
    /// End of the look-in a bot holds when it first sees its empire.
    intro_until: Option<u64>,
    wake: u64,
    next_plan: u64,
    /// Deuterium of the unit the shipyard wanted at the last plan and could
    /// not pay for; the pilot does not fly it away.
    yard_deuterium: f64,
    /// The bot's own row of the leaderboard, as last asked.
    standing: Option<Standing>,
    /// The lanes each fleet is following, by fleet id.
    routes: BTreeMap<u64, field::Route>,
    /// The lairs its fleets attacked, by sector key: a lair farmed pays less.
    heat: BTreeMap<u32, field::Heat>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct Standing {
    core: f32,
    /// Combat plus exploration points, before the cap.
    extra: f32,
}

/// A bot account as the world stores it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotRecord {
    pub style: Style,
    memory: Memory,
}

impl BotRecord {
    pub fn new(style: Style) -> BotRecord {
        BotRecord { style, memory: Memory::default() }
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

/// The day a check-in or hybrid player's look-ins are spread over in a world
/// at `pace`. Up to the season's pace it is a day of the clock. Beyond it the
/// store fills faster than a day's look-ins could keep up with, and the day
/// shortens with the time a full store holds of production (the storage
/// cap's growth over the pace's): a player who checks in looks in as often,
/// against the world, as a season's player does.
pub fn look_in_day(pace: Pace) -> u64 {
    let season = preset_named("season").and_then(|p| Pace::new(p.pace).ok()).unwrap_or(pace);
    let window = |pace: Pace| pace.cap_mult() / pace.value();
    let share = (window(pace) / window(season)).min(1.0);
    ((DAY as f64 * share).round() as u64).clamp(1, DAY)
}

pub struct Bot {
    name: String,
    style: Style,
    world_seed: u64,
    player_id: u64,
    /// `look_in_day` of the world it plays in.
    day: u64,
    memory: Memory,
    /// Asked and answered within a turn, so never part of the record.
    odds: field::Odds,
    /// The score rule of the world it plays in: the world's, not the bot's,
    /// so not part of the record either.
    rule: ExtraRule,
}

impl Bot {
    pub fn new(name: impl Into<String>, style: Style) -> Bot {
        Bot::from_record(name, BotRecord::new(style))
    }

    /// A bot that carries on from what the world stored of it.
    pub fn from_record(name: impl Into<String>, record: BotRecord) -> Bot {
        let style = Style { sessions_per_day: record.style.sessions_per_day.clamp(1, MAX_SESSIONS_PER_DAY), ..record.style };
        Bot {
            name: name.into(),
            style,
            world_seed: 0,
            player_id: 0,
            day: DAY,
            memory: record.memory,
            odds: field::Odds::new(),
            rule: ExtraRule::GAME,
        }
    }

    /// The same bot in a world scored by `rule`, for a run that measures one.
    pub fn under(self, rule: ExtraRule) -> Bot {
        Bot { rule, ..self }
    }

    pub fn record(&self) -> BotRecord {
        BotRecord { style: self.style, memory: self.memory.clone() }
    }

    pub fn style(&self) -> Style {
        self.style
    }

    /// Whether more combat or exploration points would still raise the
    /// score. Under a rule with no hard cap every point does.
    fn points_wanted(&self) -> bool {
        self.style.ignore_cap
            || self.memory.standing.is_none_or(|s| match self.rule {
                ExtraRule::Cap(share) => s.extra < CAP_HEADROOM * share * s.core,
                ExtraRule::Soft(_) | ExtraRule::Uncapped => true,
            })
    }

    /// What the admiral is told for the turn at `turn`.
    fn brief(&self, turn: u64) -> field::Brief {
        let away_for = (self.style.attention != Attention::HandsOn).then(|| self.absence(turn));
        field::Brief {
            personality: self.style.personality,
            away_for,
            points_wanted: self.points_wanted(),
            yard_deuterium: self.memory.yard_deuterium,
        }
    }

    fn hash(&self, a: u64, b: u64) -> u64 {
        mix(mix(mix(self.world_seed, self.player_id), a), b)
    }

    /// The player's own clock: every bot keeps a different time of day.
    fn local(&self, tick: u64) -> u64 {
        tick + self.hash(0, 0) % self.day
    }

    /// Ticks between the starts of two look-ins.
    fn gap(&self) -> u64 {
        (self.day * WAKING_TICKS / DAY / self.style.sessions_per_day).max(4)
    }

    fn session_ticks(&self) -> u64 {
        SESSION_TICKS.min(self.gap() / 4)
    }

    /// Start, length and kind of look-in `k` on local day `day`, in local ticks.
    fn session(&self, day: u64, k: u64) -> (u64, u64, bool) {
        let gap = self.gap();
        let start = day * self.day + k * gap + self.hash(day + 1, k) % (gap / 4);
        let piloting = self.style.attention == Attention::Hybrid && k == 0;
        let length = if piloting { PILOT_TICKS.min(gap / 2) } else { self.session_ticks() };
        (start, length, piloting)
    }

    fn presence(&self, tick: u64) -> Presence {
        if self.style.attention == Attention::HandsOn {
            return Presence::Piloting { closing: false };
        }
        match self.memory.intro_until {
            None => return Presence::Managing { closing: false },
            Some(until) if tick < until => return Presence::Managing { closing: tick + 1 == until },
            Some(_) => {}
        }
        let local = self.local(tick);
        for k in 0..self.style.sessions_per_day {
            let (start, length, piloting) = self.session(local / self.day, k);
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
        let day = local / self.day;
        (day..=day + 1)
            .flat_map(|d| (0..self.style.sessions_per_day).map(move |k| (d, k)))
            .map(|(d, k)| self.session(d, k).0)
            .find(|&start| start > local)
            .map_or(self.day, |start| start - local)
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

    fn seated(&mut self, world_seed: u64, pace: Pace, player_id: u64) {
        self.world_seed = world_seed;
        self.player_id = player_id;
        self.day = look_in_day(pace);
    }

    fn watching(&self, tick: u64) -> bool {
        self.wants_turn(tick) || self.wants_turn(tick + 1)
    }

    fn wants_turn(&self, tick: u64) -> bool {
        match self.presence(tick) {
            Presence::Away => false,
            Presence::Managing { .. } => true,
            Presence::Piloting { closing } => closing || tick >= self.memory.wake,
        }
    }

    fn ask(&mut self, state: &GameState, questions: &mut Vec<Command>) {
        let turn = state.tick + 1;
        self.odds.clear();
        if self.presence(turn) != Presence::Away {
            field::questions(state, self.brief(turn), questions);
        }
    }

    fn act(&mut self, state: &GameState, orders: &mut Vec<Order>) {
        let turn = state.tick + 1;
        if self.memory.intro_until.is_none() {
            self.memory.intro_until = Some(turn + self.session_ticks());
        }
        let presence = self.presence(turn);
        let closing = matches!(presence, Presence::Managing { closing: true } | Presence::Piloting { closing: true });
        if presence == Presence::Away {
            return;
        }

        let away_for = self.brief(turn).away_for;
        if closing || turn >= self.memory.next_plan {
            let before = orders.len();
            let yard_saving = home::plan(state, self.style.personality, away_for, orders);
            let placed = orders.len() > before;
            orders.push(Order::Command(Command::Leaderboard { limit: 1 }));
            // A player who stays queues one order at a time: once one is
            // placed it looks again at once, in case the next can follow.
            let again = if placed && away_for.is_none() { 1 } else { PLAN_PERIOD };
            self.memory.next_plan = turn + again;
            self.memory.yard_deuterium = yard_saving.deuterium;
        }

        self.memory.routes.retain(|id, _| state.fleets.iter().any(|f| f.id == *id));
        let brief = self.brief(turn);
        let memory = &mut self.memory;
        let mut admiral = field::Admiral::new(state, brief, &mut memory.routes, &mut memory.heat, &self.odds, orders);
        let idle_for = match presence {
            Presence::Managing { closing } => {
                admiral.manage(closing);
                1
            }
            Presence::Piloting { closing } => admiral.pilot(closing),
            Presence::Away => 1,
        };
        self.memory.wake = (turn + idle_for.clamp(1, PLAN_PERIOD)).min(self.memory.next_plan);
    }

    fn hear(&mut self, reply: &ServerMessage) {
        match reply {
            ServerMessage::Leaderboard(board) => {
                let rows = board.entries.iter().chain(&board.bots).chain(&board.you);
                if let Some(row) = rows.into_iter().find(|row| row.name == self.name) {
                    self.memory.standing = Some(Standing { core: row.core, extra: row.combat + row.explore });
                }
            }
            ServerMessage::Odds(answer) => {
                let rated = answer.sectors.iter().map(|s| ((answer.fleet_id, s.sector.to_key()), s.ratio));
                self.odds.extend(rated);
            }
            _ => {}
        }
    }

    fn outlook(&self, state: &GameState) -> Option<Outlook> {
        home::outlook(state, self.style.personality, self.brief(state.tick + 1).away_for)
    }
}
