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
/// A kill pays this share of the NPC group's build-cost weight.
pub const KILL_SHARE: f32 = 0.25;
/// A kill pays nothing when the engaged power is at least this many times
/// the NPC group's: trivial targets are not a score farm.
pub const OVERWHELMING_RATIO: f32 = 8.0;
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

/// Points for destroying an NPC group worth `bounty` in build cost while
/// `engaged_power` of the player's ships fought its `npc_power`.
pub fn kill_points(bounty: Resources, engaged_power: f32, npc_power: f32) -> f32 {
    if engaged_power >= OVERWHELMING_RATIO * npc_power {
        return 0.0;
    }
    KILL_SHARE * scaling::resource_weight(&bounty)
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
        assert_eq!(s.total(), 150.0, "over the cap only 25 percent of core is added");
    }

    #[test]
    fn overwhelming_force_earns_nothing() {
        let bounty = Resources { metal: 1000.0, crystal: 400.0, deuterium: 200.0 };
        let paid = kill_points(bounty, 70.0, 10.0);
        assert!((paid - 0.25 * (1000.0 + 600.0 + 400.0) / 1000.0).abs() < 1e-5);
        assert_eq!(kill_points(bounty, 80.0, 10.0), 0.0, "8x the NPC's power");
        assert_eq!(kill_points(bounty, 800.0, 10.0), 0.0);
    }
}
