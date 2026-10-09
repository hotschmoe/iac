// Queue projection: for every waiting order, why it has not started and
// roughly when it will.
//
// The queues are first in, first out. An order starts when a slot is free,
// what it needs is built and the stockpile covers its whole cost; an order
// that is short of resources holds back the ones behind it, so the line
// never reorders. An order that waits on a prerequisite does not hold the
// line, because nothing else can make it start sooner.
//
// The estimate walks the line once with the current production. It ignores
// what the other two queues will spend and any building that raises
// production meanwhile, so it is a guide, not a promise.

use iac_shared::constants::Resources;
use iac_shared::pace::Pace;
use iac_shared::protocol::WaitReason;
use iac_shared::scaling::{self, BuildingLevels, BuildingType, ResearchLevels, ResearchType, ShipyardItem};

use crate::engine::Player;

/// One waiting order's outlook.
#[derive(Debug, Clone, PartialEq)]
pub struct Wait {
    pub reason: WaitReason,
    /// Ticks until it starts; None when that cannot be estimated.
    pub start_in: Option<u64>,
    /// What the stockpile lacks right now; None when it covers the cost.
    pub short: Option<Resources>,
}

#[derive(Debug, Clone, Default)]
pub struct QueueWaits {
    pub buildings: Vec<Wait>,
    pub research: Vec<Wait>,
    pub ships: Vec<Wait>,
}

#[derive(Clone, Copy)]
enum Done {
    Building(BuildingType, u8),
    Research(ResearchType, u8),
}

struct Item {
    cost: Resources,
    duration: u64,
    ready: Box<dyn Fn(&BuildingLevels, &ResearchLevels) -> bool>,
    finishes: Option<Done>,
}

pub fn project(player: &Player, tick: u64, pace: &Pace) -> QueueWaits {
    let rate = player.production_per_tick(pace);
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
            duration: scaling::building_time(b, target, player.buildings.fabricator, pace),
            ready: Box::new(move |bl, _| bl.get(b) + 1 == target && scaling::building_prerequisites_met(b, bl)),
            finishes: Some(Done::Building(b, target)),
        }
    }).collect();
    let buildings = walk(player, &items, slots, &mut finishes, rate);

    let mut slots = vec![player.research_queue.as_ref().map_or(0, |q| q.end_tick.saturating_sub(tick))];
    if let Some(q) = &player.research_queue {
        finishes.push((q.end_tick.saturating_sub(tick), Done::Research(q.tech, q.target_level)));
    }
    slots.truncate(1);
    let items: Vec<Item> = player.research_pending.iter().map(|q| {
        let (t, target) = (q.tech, q.target_level);
        Item {
            cost: scaling::research_cost(t, target),
            duration: scaling::research_time(t, target, player.buildings.research_lab, pace),
            ready: Box::new(move |bl, rl| {
                bl.research_lab > 0 && rl.get(t) + 1 == target && scaling::research_prerequisites_met(t, bl, rl)
            }),
            finishes: Some(Done::Research(t, target)),
        }
    }).collect();
    let research = walk(player, &items, slots, &mut finishes, rate);

    let running = player.ship_queue.as_ref().map_or(0, |q| {
        let unit = q.item.unit_ticks(player.buildings.shipyard, pace);
        q.end_tick.saturating_sub(tick) + u64::from(q.count - q.built).saturating_sub(1) * unit
    });
    let items: Vec<Item> = player.ship_pending.iter().map(|q| {
        let item = q.item;
        Item {
            cost: item.unit_cost().scale(f32::from(q.count)),
            duration: item.unit_ticks(player.buildings.shipyard, pace) * u64::from(q.count),
            ready: Box::new(move |bl, rl| {
                bl.shipyard > 0
                    && match item {
                        ShipyardItem::Ship(class) => scaling::ship_class_unlocked(class, rl),
                        ShipyardItem::Defence(kind) => scaling::defence_prerequisites_met(kind, bl, rl),
                    }
            }),
            finishes: None,
        }
    }).collect();
    let ships = walk(player, &items, vec![running], &mut finishes, rate);

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
    let mut spent = Resources::default();
    let mut gate: Option<u64> = Some(0);
    for item in items {
        let ready_now = (item.ready)(&player.buildings, &player.research);
        let short_now = player.resources.shortfall(item.cost);
        let short = (short_now.total() > 0.0).then_some(short_now);
        let free_slot_now = slots.iter().any(|&s| s == 0);
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
            let afford = afford_in(player.resources.sub(spent), item.cost, rate)?;
            let slot = slots.iter().copied().min().unwrap_or(0);
            Some(ready.max(slot).max(afford).max(gate?))
        });
        out.push(Wait { reason, start_in: start, short });

        match start {
            Some(at) => {
                if let Some(slot) = slots.iter_mut().min() {
                    *slot = at + item.duration;
                }
                if let Some(done) = item.finishes {
                    finishes.push((at + item.duration, done));
                }
                spent = spent.add(item.cost);
                if ready_now {
                    gate = Some(at);
                }
            }
            None => {
                if ready_now {
                    gate = None;
                }
            }
        }
    }
    out
}

/// Ticks until `stock` covers `cost` at `rate` per tick, None if never.
fn afford_in(stock: Resources, cost: Resources, rate: Resources) -> Option<u64> {
    let mut worst = 0.0f32;
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
