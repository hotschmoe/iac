// World pace: one multiplier (P) that compresses or stretches the economy.
// Costs never change; economy timers divide by P and production multiplies by P.
// Every duration constant declares how it reacts to pace (see `PaceClass`).

use serde::{Deserialize, Serialize};

pub const PACE_MIN: f64 = 0.05;
pub const PACE_MAX: f64 = 20_000.0;

/// How a timer or quantity reacts to the world pace. The exponent is the
/// power of P applied to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaceClass {
    /// Economy timers: divided by P (production, build, research, ship times,
    /// regeneration, respawns).
    Economy,
    /// Attention timers and caps: P^0.75 (storage caps, raid cadence).
    Attention,
    /// Finds: P^0.5 (loot value, derelict respawn, raid warning).
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
    ("SHIELD_REGEN_IDLE_TICKS", PaceClass::RealTime),
    ("STRANDED_RECOVERY_TICKS", PaceClass::RealTime),
    ("NPC_RESPAWN_INNER", PaceClass::Economy),
    ("NPC_RESPAWN_OUTER", PaceClass::Economy),
    ("NPC_RESPAWN_WANDERING", PaceClass::Economy),
    ("NPC_PATROL_INTERVAL", PaceClass::RealTime),
    ("SCAN_REVEAL_TICKS", PaceClass::RealTime),
    ("RAID_ROLL_INTERVAL", PaceClass::Attention),
    ("RAID_WARNING_TICKS", PaceClass::Finds),
    ("RAID_MIN_INTERVAL", PaceClass::Attention),
    ("RAID_MIN_PLAYER_AGE", PaceClass::Attention),
    ("RAID_SUPPRESS_AFTER_LOSS", PaceClass::Attention),
    ("SALVAGE_DESPAWN_TICKS", PaceClass::RealTime),
    ("HARVEST_REPORT_TICKS", PaceClass::RealTime),
    ("EXPLORE_DURATION_TICKS", PaceClass::RealTime),
    ("POLICY_EVAL_INTERVAL", PaceClass::RealTime),
];

pub fn timer_class(name: &str) -> Option<PaceClass> {
    TIMER_CLASSES.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
}

/// Named multipliers; any positive number in range is also accepted.
pub const PRESETS: &[Preset] = &[
    Preset { name: "persistent", pace: 1.0, first_cruiser_s: 1_365_000, endgame_s: 6_385_000 },
    Preset { name: "fortnight", pace: 5.0, first_cruiser_s: 276_000, endgame_s: 1_279_000 },
    Preset { name: "season", pace: 10.0, first_cruiser_s: 165_600, endgame_s: 622_000 },
    Preset { name: "sprint", pace: 70.0, first_cruiser_s: 18_000, endgame_s: 86_400 },
    Preset { name: "dev", pace: 100.0, first_cruiser_s: 13_700, endgame_s: 46_800 },
    Preset { name: "blitz", pace: 600.0, first_cruiser_s: 2_520, endgame_s: 10_800 },
    Preset { name: "test", pace: 8000.0, first_cruiser_s: 240, endgame_s: 720 },
];

/// A named pace with the simulation's milestone estimates for a competent
/// player (`docs/design/economy/spec.md`, section 1.2).
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pace(f64);

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
    pub fn rate(&self, per_tick: f32) -> f32 {
        per_tick * self.0 as f32
    }

    pub fn attention_ticks(&self, base: f64) -> u64 {
        ((base / self.0.powf(0.75)).ceil() as u64).max(1)
    }

    pub fn cap_mult(&self) -> f32 {
        self.0.powf(0.75) as f32
    }

    pub fn finds_mult(&self) -> f32 {
        self.0.sqrt() as f32
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_numbers_and_presets() {
        assert_eq!(Pace::parse("blitz").unwrap().value(), 600.0);
        assert_eq!(Pace::parse("Persistent").unwrap().value(), 1.0);
        assert_eq!(Pace::parse("25").unwrap().value(), 25.0);
        assert_eq!(Pace::parse("0.5").unwrap().value(), 0.5);
        assert!(Pace::parse("fast").is_err());
        assert!(Pace::parse("0").is_err());
        assert!(Pace::parse("20001").is_err());
        assert!(Pace::parse("nan").is_err());
    }

    #[test]
    fn preset_is_recovered_from_value() {
        assert_eq!(Pace::parse("100").unwrap().preset().unwrap().name, "dev");
        assert!(Pace::parse("42").unwrap().preset().is_none());
    }

    #[test]
    fn pace_one_is_the_identity() {
        let p = Pace::PERSISTENT;
        assert_eq!(p.econ_ticks(300.0), 300);
        assert_eq!(p.attention_ticks(300.0), 300);
        assert_eq!(p.finds_ticks(120.0), 120);
        assert_eq!(p.rate(0.25), 0.25);
        assert_eq!(p.cap_mult(), 1.0);
        assert_eq!(p.finds_mult(), 1.0);
        assert_eq!(p.raid_warning_ticks(), 600);
    }

    #[test]
    fn helpers_scale_by_class_exponent() {
        let p = Pace::new(10.0).unwrap();
        assert_eq!(p.econ_ticks(100.0), 10);
        assert_eq!(p.econ_ticks(1.0), 1);
        assert_eq!(p.econ_ticks(101.0), 11);
        assert_eq!(p.rate(2.0), 20.0);
        assert!((p.cap_mult() - 5.6234).abs() < 1e-3);
        assert!((p.finds_mult() - 3.1623).abs() < 1e-3);
        assert_eq!(p.attention_ticks(21_600.0), 3842);
        assert_eq!(p.raid_warning_ticks(), 189);
        let blitz = Pace::new(600.0).unwrap();
        assert_eq!(blitz.raid_warning_ticks(), 30);
        assert!((blitz.cap_mult() - 121.23).abs() < 0.05);
    }

    #[test]
    fn class_exponents_match_the_spec() {
        assert_eq!(PaceClass::Economy.exponent(), 1.0);
        assert_eq!(PaceClass::Attention.exponent(), 0.75);
        assert_eq!(PaceClass::Finds.exponent(), 0.5);
        assert_eq!(PaceClass::RealTime.exponent(), 0.0);
    }

    /// Names of `pub const` items in constants.rs that look like durations.
    fn duration_constants() -> Vec<String> {
        const WORDS: [&str; 10] = [
            "TICKS", "INTERVAL", "COOLDOWN", "DELAY", "DURATION", "AGE", "SUPPRESS", "RESPAWN", "REVEAL", "RECOVERY",
        ];
        include_str!("constants.rs")
            .lines()
            .filter_map(|line| {
                let rest = line.trim().strip_prefix("pub const ")?;
                let (name, ty) = rest.split_once(':')?;
                let ty = ty.trim_start();
                let is_int = ["u8", "u16", "u32", "u64"].iter().any(|t| ty.starts_with(t));
                let is_duration = WORDS.iter().any(|w| name.split('_').any(|part| part == *w));
                (is_int && is_duration).then(|| name.trim().to_string())
            })
            .collect()
    }

    #[test]
    fn every_duration_constant_has_a_pace_class() {
        let found = duration_constants();
        assert!(found.len() >= 15, "constants.rs scan found only {found:?}");
        let missing: Vec<&String> = found.iter().filter(|n| timer_class(n).is_none()).collect();
        assert!(missing.is_empty(), "duration constants without a PaceClass in TIMER_CLASSES: {missing:?}");
    }

    #[test]
    fn the_class_table_has_no_stale_names() {
        let source = include_str!("constants.rs");
        for (name, _) in TIMER_CLASSES {
            assert!(source.contains(&format!("pub const {name}:")), "TIMER_CLASSES lists unknown constant {name}");
        }
    }
}
