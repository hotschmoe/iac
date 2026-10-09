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
use iac_shared::protocol::LeaderboardEntry;
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
        let core = self.core();
        core + (self.combat + self.explore).min(EXTRA_CAP * core)
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

/// The table, best first. `viewer`'s own row is returned separately when it
/// does not make the first `limit`.
pub fn leaderboard(engine: &GameEngine, limit: usize, viewer: u64) -> (Vec<LeaderboardEntry>, Option<LeaderboardEntry>) {
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
    let entries: Vec<LeaderboardEntry> = rows.iter().enumerate().take(limit).map(|(i, (p, s))| entry(i, p, s)).collect();
    let you = rows
        .iter()
        .enumerate()
        .skip(limit)
        .find(|(_, (p, _))| p.id == viewer)
        .map(|(i, (p, s))| entry(i, p, s));
    (entries, you)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_extra_share_is_capped_at_a_quarter_of_core() {
        let mut s = Score { econ: 100.0, fleet: 20.0, defence: 0.0, combat: 10.0, explore: 5.0 };
        assert_eq!(s.core(), 120.0);
        assert_eq!(s.total(), 135.0, "under the cap everything counts");
        s.combat = 80.0;
        assert!((s.total() - 1.25 * 120.0).abs() < 1e-4, "over the cap only 25 percent of core is added");
    }

    #[test]
    fn exploration_points_follow_the_formulas() {
        let loot = Resources { metal: 1000.0, crystal: 400.0, deuterium: 200.0 };
        let weight = (1000.0 + 600.0 + 400.0) / 1000.0;
        assert!((board_points(loot, 1) - (0.5 * weight + 1.5)).abs() < 1e-5);
        assert!((board_points(loot, 9) - (0.5 * weight + 1.5 * 27.0)).abs() < 1e-4);
        assert!((chart_points(1) - 0.25).abs() < 1e-6);
        assert!((chart_points(4) - 0.95).abs() < 1e-5);
        assert!(chart_points(9) > chart_points(2));
        assert!(board_points(Resources::default(), 1) > 1.0, "a derelict that paid nothing is still a discovery");
    }

    #[test]
    fn kills_fall_off_smoothly_with_the_power_ratio() {
        let pile = Resources { metal: 1000.0, crystal: 400.0, deuterium: 200.0 };
        let full = kill_points(pile, 40.0, 10.0);
        assert!((full - 2.0).abs() < 1e-5, "{full}");
        assert_eq!(kill_points(pile, 10.0, 10.0), full, "an even fight pays in full");
        assert_eq!(kill_points(pile, 40.0, 10.0), full, "so does 4x");
        let eight = kill_points(pile, 80.0, 10.0);
        assert!((eight - full / 2.0).abs() < 1e-5, "8x pays half, not nothing: {eight}");
        let mut last = full;
        for ratio in [5.0, 8.0, 12.0, 20.0, 40.0] {
            let paid = kill_points(pile, ratio * 10.0, 10.0);
            assert!(paid < last && paid > 0.0, "ratio {ratio}: {paid} after {last}");
            last = paid;
        }
        assert!((kill_points(pile, 10_000.0, 10.0) - full * MIN_PAY).abs() < 1e-5, "a dominant fleet still earns the floor");
    }
}
