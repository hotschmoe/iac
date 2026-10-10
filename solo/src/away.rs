// What happened to the commander while nobody was watching: counted from
// the engine's own events as the missed ticks are played, then worded once
// as the alerts the player reads on return.

use iac_shared::constants::{Resources, TICK_RATE_HZ};
use iac_shared::protocol::{AlertEvent, AlertLevel, EventKind, GameEvent};
use iac_sim::engine::{GameEngine, Player};
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
    pub docked_ships_lost: u32,
    pub fleets_lost: u32,
    pub ships_lost: u32,
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
                | EventKind::ShipDestroyed(_)
        );
        if !reported {
            return;
        }
        let Some(seen) = views::event_for(event, commander, engine) else { return };
        match seen.kind {
            EventKind::BuildingCompleted(_) => self.buildings += 1,
            EventKind::ResearchCompleted(_) => self.research += 1,
            EventKind::ShipBuilt(e) => self.ships += u32::from(e.count),
            EventKind::DefenceBuilt(e) => self.defences += u32::from(e.count),
            EventKind::RaidResolved(e) if e.defended => self.raids_repelled += 1,
            EventKind::RaidResolved(e) => {
                self.raids_lost += 1;
                self.raided = self.raided.add(e.resources_lost);
                self.docked_ships_lost += e.ships_lost;
            }
            EventKind::FleetDestroyed(e) if e.mine => self.fleets_lost += 1,
            EventKind::ShipDestroyed(e) if e.mine => self.ships_lost += 1,
            _ => {}
        }
    }

    fn gains(&self) -> Option<String> {
        let finished = list(&[
            count(self.buildings, "building", "buildings"),
            count(self.research, "research level", "research levels"),
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
            let docked = count(self.docked_ships_lost, "docked ship", "docked ships")
                .map(|ships| format!(" and {ships}"))
                .unwrap_or_default();
            format!(
                "{raids} broke through and took {} Fe, {} Cr, {} De{docked}",
                self.raided.metal.round(),
                self.raided.crystal.round(),
                self.raided.deuterium.round(),
            )
        });
        let lost = list(&[count(self.fleets_lost, "fleet", "fleets"), count(self.ships_lost, "ship", "ships")])
            .map(|lost| format!("{lost} lost in the field"));
        match (broke, lost) {
            (None, None) => None,
            (Some(one), None) | (None, Some(one)) => Some(one),
            (Some(broke), Some(lost)) => Some(format!("{broke}; {lost}")),
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
