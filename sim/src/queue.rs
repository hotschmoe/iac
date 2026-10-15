// Queue projection: for every waiting order, why it has not started and
// roughly when it will.
//
// Buildings and research: a free slot starts the earliest waiting order
// that can start now, meaning what it needs is built and the stockpile
// covers its cost. An order that cannot start keeps its place and does not
// hold the others back, unless it carries `reserve`: then, while it is ready
// and short of resources, nothing behind it starts.
//
// Shipyard: a batch is paid for when it is ordered, so it waits only for the
// yard or for something queued ahead that it needs, and the yard takes the
// earliest batch whose prerequisites are built.
//
// The estimate walks each line once with the current production, handing
// earlier orders the stockpile first. It ignores what the other queues will
// spend and any building that raises production or speed meanwhile, so it is
// a guide, not a promise.

use iac_shared::constants::Resources;
use iac_shared::pace::Pace;
use iac_shared::protocol::{BatchWait, WaitReason};
use iac_shared::scaling::{self, BuildingLevels, BuildingType, ResearchLevels, ResearchType};

use crate::engine::Player;

/// One waiting building or research order's outlook.
#[derive(Debug, Clone, PartialEq)]
pub struct Wait {
    pub reason: WaitReason,
    /// Ticks until it starts; None when that cannot be estimated.
    pub start_in: Option<u64>,
    /// What the stockpile lacks right now; None when it covers the cost.
    pub short: Option<Resources>,
}

/// One waiting shipyard batch's outlook.
#[derive(Debug, Clone, PartialEq)]
pub struct BatchOutlook {
    pub reason: BatchWait,
    /// Ticks until its first unit starts; None when that cannot be estimated.
    pub start_in: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct QueueWaits {
    pub buildings: Vec<Wait>,
    pub research: Vec<Wait>,
    pub ships: Vec<BatchOutlook>,
}

#[derive(Clone, Copy)]
enum Done {
    Building(BuildingType, u8),
    Research(ResearchType, u8),
}

type Ready = Box<dyn Fn(&BuildingLevels, &ResearchLevels) -> bool>;

struct Item {
    cost: Resources,
    reserve: bool,
    duration: u64,
    ready: Ready,
    finishes: Done,
}

pub fn project(player: &Player, tick: u64, pace: &Pace) -> QueueWaits {
    let relics = player.relic_bonus(tick, pace);
    let rate = player.production_per_tick(&relics, pace);
    let mut finishes: Vec<(u64, Done)> = Vec::new();

    let mut slots: Vec<u64> = player.building_queue.iter().map(|q| q.end_tick.saturating_sub(tick)).collect();
    slots.resize(player.building_slots().max(slots.len()), 0);
    for q in &player.building_queue {
        finishes.push((q.end_tick.saturating_sub(tick), Done::Building(q.building_type, q.target_level)));
    }
    let items: Vec<Item> = player.building_pending.iter().map(|q| {
        let (b, target) = (q.building_type, q.target_level);
        Item {
            cost: scaling::building_cost(b, target),
            reserve: q.reserve,
            duration: scaling::building_time(b, target, &player.buildings, &relics, pace),
            ready: Box::new(move |bl, rl| bl.get(b) + 1 == target && scaling::building_prerequisites_met(b, bl, rl)),
            finishes: Done::Building(b, target),
        }
    }).collect();
    let buildings = walk(player, &items, slots, &mut finishes, rate);

    let energy = player.power(pace).made;
    let slots = vec![player.research_queue.as_ref().map_or(0, |q| q.end_tick.saturating_sub(tick))];
    if let Some(q) = &player.research_queue {
        finishes.push((q.end_tick.saturating_sub(tick), Done::Research(q.tech, q.target_level)));
    }
    let items: Vec<Item> = player.research_pending.iter().map(|q| {
        let (t, target) = (q.tech, q.target_level);
        Item {
            cost: scaling::research_cost(t, target),
            reserve: q.reserve,
            duration: scaling::research_time(t, target, &player.buildings, &relics, pace),
            ready: Box::new(move |bl, rl| {
                bl.research_lab > 0
                    && rl.get(t) + 1 == target
                    && scaling::research_prerequisites_met(t, bl, rl)
                    && energy >= scaling::research_energy(t, target)
            }),
            finishes: Done::Research(t, target),
        }
    }).collect();
    let research = walk(player, &items, slots, &mut finishes, rate);

    let ships = batches(player, tick, pace, &finishes);

    QueueWaits { buildings, research, ships }
}

/// The tick offset at which `ready` first holds once the known finishes have
/// landed, or None when it never does.
fn ready_at(player: &Player, finishes: &[(u64, Done)], ready: &dyn Fn(&BuildingLevels, &ResearchLevels) -> bool) -> Option<u64> {
    let mut buildings = player.buildings.clone();
    let mut research = player.research.clone();
    if ready(&buildings, &research) {
        return Some(0);
    }
    let mut order: Vec<&(u64, Done)> = finishes.iter().collect();
    order.sort_by_key(|(at, _)| *at);
    for (at, done) in order {
        match *done {
            Done::Building(b, level) => buildings.set(b, buildings.get(b).max(level)),
            Done::Research(t, level) => research.set(t, research.get(t).max(level)),
        }
        if ready(&buildings, &research) {
            return Some(*at);
        }
    }
    None
}

fn walk(
    player: &Player,
    items: &[Item],
    mut slots: Vec<u64>,
    finishes: &mut Vec<(u64, Done)>,
    rate: Resources,
) -> Vec<Wait> {
    let mut out = Vec::with_capacity(items.len());
    let mut claimed: Vec<(u64, Resources)> = Vec::new();
    let mut gate: Option<u64> = Some(0);
    for item in items {
        let ready_now = (item.ready)(&player.buildings, &player.research);
        let short_now = player.resources.shortfall(item.cost);
        let short = (short_now.total() > 0.0).then_some(short_now);
        let free_slot_now = slots.contains(&0);
        let reason = if !ready_now {
            WaitReason::Prerequisite
        } else if short.is_some() {
            WaitReason::Resources
        } else if !free_slot_now {
            WaitReason::Slot
        } else {
            WaitReason::Order
        };

        let start = ready_at(player, finishes, item.ready.as_ref()).and_then(|ready| {
            let slot = slots.iter().copied().min().unwrap_or(0);
            let earliest = ready.max(slot).max(gate?);
            let alone = afford_in(player.resources, item.cost, rate)?;
            let ahead = claimed.iter()
                .filter(|(at, _)| *at <= earliest.max(alone))
                .fold(Resources::default(), |sum, (_, cost)| sum.add(*cost));
            let afford = afford_in(player.resources.sub(ahead), item.cost, rate)?;
            Some(earliest.max(afford))
        });
        out.push(Wait { reason, start_in: start, short });

        match start {
            Some(at) => {
                if let Some(slot) = slots.iter_mut().min() {
                    *slot = at + item.duration;
                }
                finishes.push((at + item.duration, item.finishes));
                claimed.push((at, item.cost));
                if ready_now && item.reserve {
                    gate = Some(at);
                }
            }
            None => {
                if ready_now && item.reserve {
                    gate = None;
                }
            }
        }
    }
    out
}

/// The shipyard line. The yard is taken until the running batch has
/// delivered its last unit; each waiting batch then holds it for all of its
/// units.
fn batches(player: &Player, tick: u64, pace: &Pace, finishes: &[(u64, Done)]) -> Vec<BatchOutlook> {
    let yard = &player.buildings;
    let mut free_at = player.ship_queue.as_ref().map_or(0, |q| {
        let later_units = u64::from(q.undelivered().saturating_sub(1));
        q.end_tick.saturating_sub(tick) + later_units * q.item.unit_ticks(yard, pace)
    });
    let mut out = Vec::with_capacity(player.ship_pending.len());
    for q in &player.ship_pending {
        let item = q.item;
        let ready = move |bl: &BuildingLevels, rl: &ResearchLevels| {
            scaling::prerequisites_met(item.prerequisites(), bl, rl)
        };
        let reason = if ready(&player.buildings, &player.research) { BatchWait::Slot } else { BatchWait::Prerequisite };
        let start = ready_at(player, finishes, &ready).map(|at| at.max(free_at));
        if let Some(at) = start {
            free_at = at + u64::from(q.count) * item.unit_ticks(yard, pace);
        }
        out.push(BatchOutlook { reason, start_in: start });
    }
    out
}

/// Ticks until `stock` covers `cost` at `rate` per tick, None if never.
fn afford_in(stock: Resources, cost: Resources, rate: Resources) -> Option<u64> {
    let mut worst = 0.0f64;
    for (have, need, per_tick) in [
        (stock.metal, cost.metal, rate.metal),
        (stock.crystal, cost.crystal, rate.crystal),
        (stock.deuterium, cost.deuterium, rate.deuterium),
    ] {
        let short = need - have;
        if short <= 0.0 {
            continue;
        }
        if per_tick <= 0.0 {
            return None;
        }
        worst = worst.max(short / per_tick);
    }
    Some(worst.ceil() as u64)
}
