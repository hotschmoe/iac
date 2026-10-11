// Score: one ranking for humans and agents (docs/design/economy/spec.md section 9).
//
//   core  = econ + fleet + defence
//   score = core + min(combat + explore, 0.25 * core)
//
// econ, fleet and defence are recomputed from state whenever they are
// asked for; combat and explore accumulate in the player row. Everything is
// priced with `scaling::resource_weight`, so crystal and deuterium count
// for more than metal.

use iac_shared::constants::Resources;
use iac_shared::protocol::{LeaderboardEntry, LeaderboardReply};
use iac_shared::scaling::{self, DefenceKind};

use crate::engine::{Fleet, GameEngine, Player};

/// Combat and exploration together may add at most this share of `core`.
pub const EXTRA_CAP: f32 = 0.25;
/// A kill pays this share of the weight of the pile it leaves, so points
/// track the resources the same kill yields.
pub const KILL_SHARE: f32 = 1.0;
/// A kill earns full points up to this ratio of engaged power to the NPC
/// group's power; beyond it pay falls as `FULL_PAY_RATIO / ratio`, down to
/// `MIN_PAY`. The old rule paid nothing from 8x.
pub const FULL_PAY_RATIO: f32 = 4.0;
pub const MIN_PAY: f32 = 0.05;
/// A first boarding pays this share of the weight of the loot it rolled
/// (finds factor included), plus `DISCOVERY_POINTS * T^1.5`.
pub const BOARD_VALUE_SHARE: f32 = 0.5;
pub const DISCOVERY_POINTS: f32 = 1.5;
/// A sector's chart, once delivered to a dock, pays
/// `CHART_BASE_POINTS + CHART_THREAT_POINTS * T^1.5`.
pub const CHART_BASE_POINTS: f32 = 0.15;
pub const CHART_THREAT_POINTS: f32 = 0.10;
/// Defence structures count at half value, so farming defenceless-looking
/// players with a wall of cheap turrets does not pay.
pub const DEFENCE_SHARE: f32 = 0.5;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Score {
    pub econ: f32,
    pub fleet: f32,
    pub defence: f32,
    pub combat: f32,
    pub explore: f32,
}

impl Score {
    pub fn core(&self) -> f32 {
        self.econ + self.fleet + self.defence
    }

    pub fn total(&self) -> f32 {
        self.total_under(ExtraRule::GAME)
    }

    pub fn total_under(&self, rule: ExtraRule) -> f32 {
        let core = self.core();
        core + rule.counted(self.combat + self.explore, core)
    }
}

/// How combat and exploration points count toward a score. Every world plays
/// under `ExtraRule::GAME`. The others are alternatives for `sim-run --cap`
/// to measure: nothing a client sees is scored with them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExtraRule {
    /// In full up to this share of core, not at all beyond it.
    Cap(f32),
    /// In full up to this share of core (the knee). Beyond it the counted
    /// points grow with the square root of the raw ones: four times the
    /// points count double.
    Soft(f32),
    /// In full.
    Uncapped,
}

impl ExtraRule {
    pub const GAME: ExtraRule = ExtraRule::Cap(EXTRA_CAP);

    /// What `raw` combat and exploration points add to a score of `core`.
    pub fn counted(self, raw: f32, core: f32) -> f32 {
        match self {
            ExtraRule::Cap(share) => raw.min(share * core),
            ExtraRule::Soft(knee) if raw > knee * core => (knee * core * raw).sqrt(),
            ExtraRule::Soft(_) | ExtraRule::Uncapped => raw,
        }
    }

    /// What one more point would add to a score standing at `raw` and `core`:
    /// 1 while points count in full, 0 once they do not count at all.
    pub fn next_point(self, raw: f32, core: f32) -> f32 {
        match self {
            ExtraRule::Cap(share) if raw >= share * core => 0.0,
            ExtraRule::Soft(knee) if raw > knee * core => 0.5 * (knee * core / raw).sqrt(),
            ExtraRule::Cap(_) | ExtraRule::Soft(_) | ExtraRule::Uncapped => 1.0,
        }
    }
}

/// Ticks in which a `Meter` drains by one allowance.
const METER_WINDOW: f64 = 86_400.0;

/// A daily allowance for combat and exploration points: another alternative
/// no world plays under, which `sim-run --meter` measures beside the run's
/// own rule. The meter fills with every point earned and drains by one
/// allowance (`share` of core) a day. A point counts in full while the
/// meter stands under one allowance and by the square root beyond it: a
/// burst of four allowances counts as two. Unlike a cap this needs a memory:
/// what a point counted is banked when it is earned.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Meter {
    pub share: f32,
    level: f64,
    banked: f64,
}

impl Meter {
    pub fn new(share: f32) -> Meter {
        Meter { share, level: 0.0, banked: 0.0 }
    }

    /// What the points earned so far have added to the score.
    pub fn banked(&self) -> f32 {
        self.banked as f32
    }

    fn allowance(&self, core: f32) -> f64 {
        f64::from(self.share) * f64::from(core)
    }

    /// One tick passes for an empire of `core`.
    pub fn drain(&mut self, core: f32) {
        self.level = (self.level - self.allowance(core) / METER_WINDOW).max(0.0);
    }

    /// An empire of `core` earns `points`.
    pub fn earn(&mut self, points: f32, core: f32) {
        let allowance = self.allowance(core);
        let counted_up_to = |level: f64| if level <= allowance { level } else { (allowance * level).sqrt() };
        let after = self.level + f64::from(points);
        self.banked += counted_up_to(after) - counted_up_to(self.level);
        self.level = after;
    }

    /// What one more point would add now: 1 under the allowance, less beyond it.
    pub fn next_point(&self, core: f32) -> f32 {
        let allowance = self.allowance(core);
        if self.level <= allowance { 1.0 } else { (0.5 * (allowance / self.level).sqrt()) as f32 }
    }
}

/// `fleets` are all of the player's fleets, docked or away.
pub fn score_of<'a>(player: &Player, fleets: impl Iterator<Item = &'a Fleet>) -> Score {
    let fleet: f32 = fleets
        .filter(|f| f.owner_id == player.id)
        .flat_map(|f| f.ships[0..f.ship_count].iter())
        .filter(|s| s.hull > 0.0)
        .map(|s| scaling::resource_weight(&s.ship_class.build_cost()))
        .sum();
    let defence: f32 = DefenceKind::ALL
        .iter()
        .map(|&k| player.defences.count[k as usize] as f32 * scaling::resource_weight(&k.build_cost()))
        .sum();
    Score {
        econ: scaling::econ_points(&player.buildings, &player.research),
        fleet,
        defence: DEFENCE_SHARE * defence,
        combat: player.combat_points,
        explore: player.explore_points,
    }
}

/// Share of a kill's points paid when `engaged_power` of the player's ships
/// fought a group of `npc_power`.
pub fn kill_pay_factor(engaged_power: f32, npc_power: f32) -> f32 {
    if npc_power <= 0.0 {
        return MIN_PAY;
    }
    (FULL_PAY_RATIO * npc_power / engaged_power.max(1e-3)).clamp(MIN_PAY, 1.0)
}

/// Points for a kill that left `pile` behind.
pub fn kill_points(pile: Resources, engaged_power: f32, npc_power: f32) -> f32 {
    KILL_SHARE * scaling::resource_weight(&pile) * kill_pay_factor(engaged_power, npc_power)
}

/// Points for the first boarding of a derelict that rolled `loot` (finds
/// factor included) at threat `rating`.
pub fn board_points(loot: Resources, rating: u8) -> f32 {
    BOARD_VALUE_SHARE * scaling::resource_weight(&loot) + DISCOVERY_POINTS * f32::from(rating).powf(1.5)
}

/// Points for delivering the chart of one new sector at threat `rating`.
pub fn chart_points(rating: u8) -> f32 {
    CHART_BASE_POINTS + CHART_THREAT_POINTS * f32::from(rating).powf(1.5)
}

/// The table, best first, for `viewer`. Built-in bots are ranked apart from
/// everyone else (`LeaderboardReply::bots`); each list holds its first
/// `limit` rows, and the viewer's own row is returned in `you` when it made
/// neither.
pub fn leaderboard(engine: &GameEngine, limit: usize, viewer: u64) -> LeaderboardReply {
    let mut rows: Vec<(&Player, Score)> = engine
        .players
        .values()
        .map(|p| (p, score_of(p, engine.fleets.values())))
        .collect();
    rows.sort_by(|a, b| {
        b.1.total().total_cmp(&a.1.total()).then_with(|| a.0.id.cmp(&b.0.id))
    });
    let entry = |rank: usize, p: &Player, s: &Score| LeaderboardEntry {
        rank: rank as u32 + 1,
        name: p.name.clone(),
        agent: p.agent,
        score: s.total(),
        core: s.core(),
        combat: s.combat,
        explore: s.explore,
    };
    let (bots, ranked): (Vec<_>, Vec<_>) = rows.into_iter().partition(|(p, _)| engine.is_bot(p.id));
    let list = if engine.is_bot(viewer) { &bots } else { &ranked };
    let you = list
        .iter()
        .enumerate()
        .skip(limit)
        .find(|(_, (p, _))| p.id == viewer)
        .map(|(i, (p, s))| entry(i, p, s));
    let first = |list: &[(&Player, Score)]| -> Vec<LeaderboardEntry> {
        list.iter().enumerate().take(limit).map(|(i, (p, s))| entry(i, p, s)).collect()
    };
    LeaderboardReply { tick: engine.current_tick(), entries: first(&ranked), you, bots: first(&bots) }
}
