// What happened to the commander while nobody was watching: counted from
// the engine's own events as the missed ticks are played, then worded once
// as the alerts the player reads on return.

use iac_shared::constants::{Resources, TICK_RATE_HZ};
use iac_shared::hex::Hex;
use iac_shared::protocol::{AlertEvent, AlertLevel, BattleOutcome, EventKind, GameEvent};
use iac_shared::scaling::ShipyardItem;
use iac_sim::engine::{GameEngine, Outpost, OutpostTask, Player};
use iac_sim::views;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Tally {
    pub buildings: u32,
    pub research: u32,
    pub ships: u32,
    pub defences: u32,
    pub raids_repelled: u32,
    pub raids_lost: u32,
    pub raided: Resources,
    /// What the fights of the raids on the homeworld destroyed, repelled or
    /// lost: ships of fleets docked there, Solar Satellites and defences,
    /// and how many of the satellites and of the defences were rebuilt
    /// when a fight ended.
    pub docked_ships_lost: u32,
    pub satellites_lost: u32,
    pub satellites_rebuilt: u32,
    pub defences_lost: u32,
    pub defences_rebuilt: u32,
    /// Fleets and ships lost away from both docks.
    pub fleets_lost: u32,
    pub ships_lost: u32,
    /// Fights the commander's last fleet in them jumped out of under fire.
    pub fights_fled: u32,
    /// Levels the outpost finished.
    pub outpost_levels: u32,
    /// Raids on the outpost, counted apart from those on the homeworld.
    pub outpost_raids_repelled: u32,
    pub outpost_raids_lost: u32,
    pub outpost_raided: Resources,
    pub outpost_ships_lost: u32,
}

impl Tally {
    /// Count `event` if it is one of the commander's that the report names.
    pub fn count(&mut self, event: &GameEvent, commander: &Player, engine: &GameEngine) {
        let reported = matches!(
            event.kind,
            EventKind::BuildingCompleted(_)
                | EventKind::ResearchCompleted(_)
                | EventKind::ShipBuilt(_)
                | EventKind::DefenceBuilt(_)
                | EventKind::RaidResolved(_)
                | EventKind::FleetDestroyed(_)
                | EventKind::BattleRound(_)
                | EventKind::CombatEnded(_)
                | EventKind::ShipsLost(_)
                | EventKind::OutpostCompleted(_)
        );
        if !reported {
            return;
        }
        let Some(seen) = views::event_for(event, commander, engine) else { return };
        let at_a_dock = |sector: Hex| {
            sector == commander.homeworld || engine.outposts.get(&commander.id).is_some_and(|post| post.sector == sector)
        };
        match seen.kind {
            EventKind::BuildingCompleted(_) => self.buildings += 1,
            EventKind::ResearchCompleted(_) => self.research += 1,
            EventKind::ShipBuilt(e) => self.ships += u32::from(e.count),
            EventKind::DefenceBuilt(e) => self.defences += u32::from(e.count),
            EventKind::OutpostCompleted(_) => self.outpost_levels += 1,
            EventKind::RaidResolved(e) if e.outpost.is_some() => {
                self.outpost_ships_lost += e.ships_lost;
                if e.defended {
                    self.outpost_raids_repelled += 1;
                } else {
                    self.outpost_raids_lost += 1;
                    self.outpost_raided = self.outpost_raided.add(e.resources_lost);
                }
            }
            EventKind::RaidResolved(e) => {
                self.docked_ships_lost += e.ships_lost;
                self.satellites_lost += e.satellites_lost;
                self.satellites_rebuilt += e
                    .rebuilt
                    .iter()
                    .filter(|units| matches!(units.unit, ShipyardItem::Ship(_)))
                    .map(|units| units.count)
                    .sum::<u32>();
                self.defences_lost += e.structures_lost;
                self.defences_rebuilt += e.structures_restored;
                if e.defended {
                    self.raids_repelled += 1;
                } else {
                    self.raids_lost += 1;
                    self.raided = self.raided.add(e.resources_lost);
                }
            }
            EventKind::FleetDestroyed(e) if e.mine && !at_a_dock(e.sector) => self.fleets_lost += 1,
            EventKind::BattleRound(e) if !e.raid => {
                let mine = e.fleets.groups.iter().filter(|group| group.mine);
                self.ships_lost += mine.flat_map(|group| &group.units).map(|units| units.lost).sum::<u32>();
            }
            EventKind::ShipsLost(e) if e.mine => self.ships_lost += e.ships.iter().map(|s| s.count).sum::<u32>(),
            EventKind::CombatEnded(e) if e.mine && !e.raid && e.outcome == BattleOutcome::Fled => self.fights_fled += 1,
            _ => {}
        }
    }

    fn gains(&self) -> Option<String> {
        let finished = list(&[
            count(self.buildings, "building", "buildings"),
            count(self.research, "research level", "research levels"),
            count(self.outpost_levels, "outpost level", "outpost levels"),
        ]);
        let built = list(&[count(self.ships, "ship", "ships"), count(self.defences, "defence", "defences")]);
        match (finished, built) {
            (None, None) => None,
            (Some(finished), None) => Some(format!("{finished} finished")),
            (None, Some(built)) => Some(format!("{built} built")),
            (Some(finished), Some(built)) => Some(format!("{finished} finished, {built} built")),
        }
    }

    fn trouble(&self) -> Option<String> {
        let broke = count(self.raids_lost, "raid", "raids").map(|raids| {
            format!(
                "{raids} left no defender standing and took {} Fe, {} Cr, {} De",
                self.raided.metal.round(),
                self.raided.crystal.round(),
                self.raided.deuterium.round(),
            )
        });
        let fought = list(&[
            count(self.docked_ships_lost, "docked ship", "docked ships"),
            count(self.satellites_lost, "Solar Satellite", "Solar Satellites"),
            count(self.defences_lost, "defence", "defences"),
        ])
        .map(|destroyed| {
            let rebuilt = list(&[
                count(self.satellites_rebuilt, "of the Solar Satellites", "of the Solar Satellites"),
                count(self.defences_rebuilt, "of the defences", "of the defences"),
            ])
            .map(|rebuilt| format!(", and {rebuilt} were rebuilt"))
            .unwrap_or_default();
            format!("the raids on your homeworld destroyed {destroyed}{rebuilt}")
        });
        let lost = list(&[count(self.fleets_lost, "fleet", "fleets"), count(self.ships_lost, "ship", "ships")])
            .map(|lost| format!("{lost} lost in the field"));
        let fled = count(self.fights_fled, "fight", "fights").map(|fights| format!("{fights} left under fire"));
        let parts: Vec<String> = [broke, fought, lost, fled].into_iter().flatten().collect();
        (!parts.is_empty()).then(|| parts.join("; "))
    }

    /// The ships the raids on the outpost destroyed there, as a clause.
    fn outpost_ships(&self) -> Option<String> {
        count(self.outpost_ships_lost, "ship there", "ships there")
    }

    /// What raids that broke the outpost took, and how it stands now.
    fn outpost_trouble(&self, post: &Standing) -> Option<String> {
        let raids = count(self.outpost_raids_lost, "raid", "raids")?;
        let ships = self.outpost_ships().map(|ships| format!(", and the raids destroyed {ships}")).unwrap_or_default();
        let now = match (post.breached, post.repaired_at) {
            (false, _) => "It has repaired itself and is whole".to_string(),
            (true, Some(tick)) => format!("It is breached and under repair until tick {tick}"),
            (true, None) => {
                "It is breached, and its store does not cover the repair: it makes nothing until a fleet unloads the rest"
                    .to_string()
            }
        };
        Some(format!(
            "{raids} broke your outpost at {} and took {} Fe, {} Cr, {} De{ships}. {now}",
            post.sector,
            self.outpost_raided.metal.round(),
            self.outpost_raided.crystal.round(),
            self.outpost_raided.deuterium.round(),
        ))
    }
}

/// How the commander's outpost stands when they return.
#[derive(Debug, Clone, Copy)]
pub struct Standing {
    pub sector: Hex,
    pub breached: bool,
    /// Tick the repair under way ends.
    pub repaired_at: Option<u64>,
}

impl Standing {
    pub fn of(outpost: &Outpost) -> Standing {
        Standing {
            sector: outpost.sector,
            breached: outpost.breached,
            repaired_at: outpost.job.filter(|job| job.task == OutpostTask::Repair).map(|job| job.end_tick),
        }
    }
}

/// One absence, start to end.
#[derive(Debug, Clone, Copy)]
pub struct Absence {
    /// Ticks the world played to catch up.
    pub played: u64,
    /// Ticks past the catch-up limit, which the world never plays.
    pub over_limit: u64,
    /// Ticks left unplayed because the player took command early.
    pub cut_short: u64,
    pub tally: Tally,
    /// The commander's outpost as it stands at the end, if they have one.
    pub outpost: Option<Standing>,
}

impl Absence {
    /// The alerts that tell the commander about it, stamped `tick`.
    pub fn notices(&self, commander: u64, tick: u64) -> Vec<GameEvent> {
        let away = span(self.played + self.over_limit + self.cut_short);
        let ran = format!("{} ({} ticks)", span(self.played), self.played);
        let mut clock = format!("Away {away}: the world ran on without you for {ran}");
        if self.over_limit > 0 {
            clock.push_str(", the most it catches up, and waited for you after that");
        }
        if self.cut_short > 0 {
            clock.push_str(&format!("; you took command early and the last {} were skipped", span(self.cut_short)));
        }
        clock.push('.');
        let gains = match self.tally.gains() {
            Some(gains) => format!("While away: {gains}."),
            None => "While away: no order finished.".to_string(),
        };
        let mut notices = vec![(AlertLevel::Info, clock), (AlertLevel::Info, gains)];
        if let Some(raids) = count(self.tally.raids_repelled, "raid was", "raids were") {
            notices.push((AlertLevel::Info, format!("While away: {raids} repelled.")));
        }
        if let Some(trouble) = self.tally.trouble() {
            notices.push((AlertLevel::Warning, format!("While away: {trouble}.")));
        }
        if let Some(post) = &self.outpost {
            if let Some(raids) = count(self.tally.outpost_raids_repelled, "raid", "raids") {
                let were = if self.tally.outpost_raids_repelled == 1 { "was" } else { "were" };
                let cost = self.tally.outpost_ships().filter(|_| self.tally.outpost_raids_lost == 0);
                let (level, cost) = match cost {
                    Some(ships) => (AlertLevel::Warning, format!(", at the cost of {ships}")),
                    None => (AlertLevel::Info, String::new()),
                };
                notices.push((level, format!("While away: {raids} on your outpost at {} {were} repelled{cost}.", post.sector)));
            }
            if let Some(trouble) = self.tally.outpost_trouble(post) {
                notices.push((AlertLevel::Warning, format!("While away: {trouble}.")));
            }
        }
        notices
            .into_iter()
            .map(|(level, message)| GameEvent {
                tick,
                kind: EventKind::Alert(AlertEvent { player_id: Some(commander), level, message, sector: None, fleet_id: None }),
            })
            .collect()
    }
}

fn count(n: u32, one: &str, many: &str) -> Option<String> {
    match n {
        0 => None,
        1 => Some(format!("1 {one}")),
        n => Some(format!("{n} {many}")),
    }
}

fn list(parts: &[Option<String>]) -> Option<String> {
    let parts: Vec<&str> = parts.iter().flatten().map(String::as_str).collect();
    match parts.as_slice() {
        [] => None,
        [one] => Some((*one).to_string()),
        [first @ .., last] => Some(format!("{} and {last}", first.join(", "))),
    }
}

/// "3d 4h", "8h 02m", "4m 10s".
pub fn span(ticks: u64) -> String {
    let total = ticks / TICK_RATE_HZ;
    let (days, hours, minutes, seconds) = (total / 86_400, total / 3_600 % 24, total / 60 % 60, total % 60);
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes:02}m")
    } else {
        format!("{minutes}m {seconds:02}s")
    }
}
