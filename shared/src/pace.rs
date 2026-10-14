// World pace: one multiplier (P) that compresses or stretches the economy.
// Costs never change; economy timers divide by P and production multiplies by P.
// Every duration constant declares how it reacts to pace (see `PaceClass`).

use serde::{Deserialize, Serialize};

use crate::maths;

pub const PACE_MIN: f64 = 0.05;
/// Pace above which the storage cap grows with P^0.25 instead of P^0.75.
pub const CAP_KNEE: f64 = 10.0;
/// The storage cap's exponent above the knee; below it the cap is attention class.
pub const CAP_EXPONENT_PAST_KNEE: f64 = 0.25;
pub const PACE_MAX: f64 = 20_000.0;

/// How a timer or quantity reacts to the world pace. The exponent is the
/// power of P applied to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaceClass {
    /// Economy timers: divided by P (production, build, research, ship times,
    /// regeneration).
    Economy,
    /// Attention timers and caps: P^0.75 (storage caps, raid cadence).
    Attention,
    /// Finds: P^0.5 (derelict respawn, a pirate group's return and mending,
    /// raid warning, and the size of holds and stores).
    Finds,
    /// Real time: never scaled (cooldowns, combat, fuel, boarding).
    RealTime,
}

impl PaceClass {
    pub fn exponent(self) -> f64 {
        match self {
            PaceClass::Economy => 1.0,
            PaceClass::Attention => 0.75,
            PaceClass::Finds => 0.5,
            PaceClass::RealTime => 0.0,
        }
    }
}

/// Every duration constant in `constants.rs`, with the class that governs it.
/// A test fails when a duration constant is missing here.
pub const TIMER_CLASSES: &[(&str, PaceClass)] = &[
    ("TICK_DURATION_NS", PaceClass::RealTime),
    ("MOVE_BASE_COOLDOWN", PaceClass::RealTime),
    ("HARVEST_COOLDOWN", PaceClass::RealTime),
    ("SCAN_COOLDOWN", PaceClass::RealTime),
    ("REPAIR_TICKS", PaceClass::RealTime),
    ("STRANDED_RECOVERY_TICKS", PaceClass::RealTime),
    ("FLEE_MIN_ROUNDS", PaceClass::RealTime),
    ("NPC_HEAL_SHARE", PaceClass::Finds),
    ("SCAN_REVEAL_TICKS", PaceClass::RealTime),
    ("RAID_ROLL_INTERVAL", PaceClass::Attention),
    ("RAID_ROUNDS", PaceClass::RealTime),
    ("RAID_MIN_INTERVAL", PaceClass::Attention),
    ("RAID_MIN_PLAYER_AGE", PaceClass::Attention),
    ("RAID_SUPPRESS_AFTER_LOSS", PaceClass::Attention),
    ("SALVAGE_DESPAWN_TICKS", PaceClass::RealTime),
    ("HOME_SALVAGE_WARN_TICKS", PaceClass::RealTime),
    ("HARVEST_REPORT_TICKS", PaceClass::RealTime),
    ("EXPLORE_DURATION_TICKS", PaceClass::RealTime),
    ("DERELICT_RESPAWN_TICKS", PaceClass::Finds),
    ("POLICY_EVAL_INTERVAL", PaceClass::RealTime),
    ("EXCHANGE_WINDOW_TICKS", PaceClass::Attention),
];

pub fn timer_class(name: &str) -> Option<PaceClass> {
    TIMER_CLASSES.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
}

/// Named multipliers; any positive number in range is also accepted.
pub const PRESETS: &[Preset] = &[
    Preset { name: "persistent", pace: 1.0, first_cruiser_s: 1_779_300, endgame_s: 4_806_300 },
    Preset { name: "fortnight", pace: 5.0, first_cruiser_s: 414_780, endgame_s: 1_062_480 },
    Preset { name: "season", pace: 10.0, first_cruiser_s: 204_420, endgame_s: 544_920 },
    Preset { name: "sprint", pace: 70.0, first_cruiser_s: 25_685, endgame_s: 72_630 },
    Preset { name: "dev", pace: 100.0, first_cruiser_s: 18_150, endgame_s: 46_340 },
    Preset { name: "blitz", pace: 600.0, first_cruiser_s: 3_236, endgame_s: 8_355 },
    Preset { name: "test", pace: 8000.0, first_cruiser_s: 252, endgame_s: 642 },
];

/// A named pace with the simulation's milestone estimates for a competent
/// player: the estimates table of `docs/design/economy/sim.py --tables`,
/// whose endgame is the kit that file names.
#[derive(Debug, Clone, Copy)]
pub struct Preset {
    pub name: &'static str,
    pub pace: f64,
    pub first_cruiser_s: u64,
    pub endgame_s: u64,
}

pub fn preset_named(name: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.name.eq_ignore_ascii_case(name))
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct Pace(f64);

impl TryFrom<f64> for Pace {
    type Error = String;

    fn try_from(value: f64) -> Result<Pace, String> {
        Pace::new(value)
    }
}

impl From<Pace> for f64 {
    fn from(pace: Pace) -> f64 {
        pace.0
    }
}

impl Pace {
    pub const PERSISTENT: Pace = Pace(1.0);

    pub fn new(value: f64) -> Result<Pace, String> {
        if value.is_finite() && (PACE_MIN..=PACE_MAX).contains(&value) {
            Ok(Pace(value))
        } else {
            Err(format!("pace {value} is outside {PACE_MIN}..={PACE_MAX}"))
        }
    }

    /// A preset name ("blitz") or a number ("25", "0.5").
    pub fn parse(text: &str) -> Result<Pace, String> {
        let text = text.trim();
        if let Some(p) = preset_named(text) {
            return Pace::new(p.pace);
        }
        text.parse::<f64>().map_err(|_| {
            let names: Vec<&str> = PRESETS.iter().map(|p| p.name).collect();
            format!("'{text}' is neither a number nor a preset ({})", names.join(", "))
        }).and_then(Pace::new)
    }

    pub fn value(&self) -> f64 {
        self.0
    }

    /// The preset this pace equals exactly, if any.
    pub fn preset(&self) -> Option<&'static Preset> {
        PRESETS.iter().find(|p| p.pace == self.0)
    }

    /// Whole ticks for an economy duration given in pace-1 ticks (seconds).
    pub fn econ_ticks(&self, base: f64) -> u64 {
        ((base / self.0).ceil() as u64).max(1)
    }

    /// Per-tick production or regeneration rate at this pace.
    pub fn rate(&self, per_tick: f64) -> f64 {
        per_tick * self.0
    }

    pub fn attention_ticks(&self, base: f64) -> u64 {
        ((base / maths::powf(self.0, PaceClass::Attention.exponent())).ceil() as u64).max(1)
    }

    /// Storage cap multiplier: P^0.75 up to the season pace, then P^0.25.
    /// Production grows with P while a human's look-in window shrinks, so
    /// beyond a season the cap must grow much slower than P^0.75 to stay
    /// within a few windows of production; at a blitz P^0.75 (121x) held four
    /// hours of mine output and storage never mattered. OGame does not scale
    /// storage with its universe speed; this multiplier is this game's.
    pub fn cap_mult(&self) -> f64 {
        let (p, attention) = (self.0, PaceClass::Attention.exponent());
        if p <= CAP_KNEE {
            maths::powf(p, attention)
        } else {
            maths::powf(CAP_KNEE, attention) * maths::powf(p / CAP_KNEE, CAP_EXPONENT_PAST_KNEE)
        }
    }

    /// What a find is worth, in units, against pace 1: loot, wreckage,
    /// derelict and chart value follow production, so a find is worth the same
    /// minutes of income at every pace.
    pub fn find_mult(&self) -> f64 {
        self.0
    }

    /// Ship holds and outpost stores scale like finds' timers (P^0.5), not
    /// like their value: a find can be worth more than a hold carries, and
    /// what a hold cannot take drifts loose as salvage.
    pub fn cargo_mult(&self) -> f64 {
        self.0.sqrt()
    }

    pub fn finds_ticks(&self, base: f64) -> u64 {
        ((base / self.0.sqrt()).ceil() as u64).max(1)
    }

    pub fn raid_warning_ticks(&self) -> u64 {
        (600.0 / self.0.sqrt()).clamp(30.0, 600.0) as u64
    }
}

impl Default for Pace {
    fn default() -> Self {
        Pace::PERSISTENT
    }
}
