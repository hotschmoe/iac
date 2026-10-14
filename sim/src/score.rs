// Score: one ranking for humans and agents (docs/design/economy/spec.md section 9).
//
//   score = buildings + research + fleet + outpost + DEFENCE_SHARE * defences
//
// What an empire has built and keeps, recomputed from state whenever it is
// asked for: a fleet that is destroyed takes its ships out of the score.
// Fighting and exploring score nothing themselves; they pay in resources,
// which score once they are built into something at home. Everything is
// priced with `scaling::resource_weight`, so crystal and deuterium count for
// more than metal.

use iac_shared::constants::ShipClass;
use iac_shared::protocol::{LeaderboardEntry, LeaderboardReply};
use iac_shared::scaling::{self, DefenceKind};

use crate::engine::{GameEngine, Player};

/// The share of their cost that defence structures score. Provisional, and
/// the owner's: "0 points on defense BUT raids give good resources", so a
/// defence pays for itself in the wreckage of the raids it beats.
pub const DEFENCE_SHARE: f32 = 0.0;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Score {
    pub buildings: f32,
    pub research: f32,
    /// Every ship of the empire's, docked or away, its Solar Satellites
    /// among them.
    pub fleet: f32,
    /// The finished levels of the outpost, while it stands.
    pub outpost: f32,
    /// What the standing defences cost, whether or not they score.
    pub defences: f32,
}

impl Score {
    /// The part of the score the defences make.
    pub fn defence(&self) -> f32 {
        DEFENCE_SHARE * self.defences
    }

    pub fn total(&self) -> f32 {
        self.total_with(DEFENCE_SHARE)
    }

    /// The score were defences to count at `defence_share` of their cost:
    /// `sim-run --defence-share` measures another share beside the game's.
    pub fn total_with(&self, defence_share: f32) -> f32 {
        self.buildings + self.research + self.fleet + self.outpost + defence_share * self.defences
    }
}

/// `player`'s score: their levels, every ship of theirs docked or away,
/// their Solar Satellites as ships, the finished levels of their outpost and
/// their defences at `DEFENCE_SHARE`.
pub fn score_of(engine: &GameEngine, player: &Player) -> Score {
    let fleet: f32 = engine
        .fleets
        .values()
        .filter(|f| f.owner_id == player.id)
        .flat_map(|f| f.ships.counts())
        .map(|(class, count)| count as f32 * scaling::resource_weight(&class.build_cost()))
        .sum();
    let satellites = player.satellites as f32 * scaling::resource_weight(&ShipClass::SolarSatellite.build_cost());
    let defences: f32 = DefenceKind::ALL
        .iter()
        .map(|&k| player.defences.count[k as usize] as f32 * scaling::resource_weight(&k.build_cost()))
        .sum();
    Score {
        buildings: scaling::building_points(&player.buildings),
        research: scaling::research_points(&player.research),
        fleet: fleet + satellites,
        outpost: scaling::outpost_points(engine.outposts.get(&player.id).map_or(0, |o| o.level)),
        defences,
    }
}

/// The table, best first, for `viewer`. Built-in bots are ranked apart from
/// everyone else (`LeaderboardReply::bots`); each list holds its first
/// `limit` rows, and the viewer's own row is returned in `you` when it made
/// neither.
pub fn leaderboard(engine: &GameEngine, limit: usize, viewer: u64) -> LeaderboardReply {
    let mut rows: Vec<(&Player, Score)> = engine
        .players
        .values()
        .map(|p| (p, score_of(engine, p)))
        .collect();
    rows.sort_by(|a, b| {
        b.1.total().total_cmp(&a.1.total()).then_with(|| a.0.id.cmp(&b.0.id))
    });
    let entry = |rank: usize, p: &Player, s: &Score| LeaderboardEntry {
        rank: rank as u32 + 1,
        name: p.name.clone(),
        agent: p.agent,
        score: s.total(),
        buildings: s.buildings,
        research: s.research,
        fleet: s.fleet,
        outpost: s.outpost,
        defence: s.defence(),
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
