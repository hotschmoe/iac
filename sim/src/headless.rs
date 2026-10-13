// A world with scripted players and no network: build it, step it as fast as
// the CPU allows, read the results. Used by `sim-run` and the bot tests.
//
// Besides the end state it keeps what a balance report needs: each player's
// score split at every sample point, and a running tally of what the field
// paid them, counted from the engine's own events.

use std::collections::BTreeMap;

use iac_shared::constants::{
    Density, ResourceKind, Resources, TerrainType, FUEL_DEUT_PER_UNIT, OUTPOST_MAX_LEVEL, OUTPOST_MAX_RING, OUTPOST_MIN_RING,
    OUTPOST_REPAIR_SHARE, OUTPOST_SPACING, RAID_ROLL_INTERVAL,
};
use iac_shared::hex::{hex_ring, Hex};
use iac_shared::pace::Pace;
use iac_shared::protocol::{BattleOutcome, EventKind, GameEvent, PursuitPhase, QueueType, WaitReason};
use iac_shared::scaling::{self, BuildingType, ResearchType};
use serde::Serialize;

use crate::engine::{FleetStatus, GameEngine, Outpost, OutpostJob, OutpostTask, Player, RaidState};
use crate::host::{self, Phase, TickError, Watch};
use crate::score::{self, ExtraRule, Meter, Score};
use crate::script::{ScriptedPlayer, Seat, Want};
use crate::{queue, views};

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct ScoreBreakdown {
    pub econ: f32,
    pub fleet: f32,
    pub defence: f32,
    /// The finished levels of the player's outpost.
    pub outpost: f32,
    pub core: f32,
    /// Raw combat points, before the cap.
    pub combat: f32,
    /// Raw exploration points, before the cap.
    pub explore: f32,
    /// What combat and exploration add to the score once capped.
    pub extra: f32,
    pub total: f32,
}

impl ScoreBreakdown {
    fn under(s: Score, rule: ExtraRule) -> ScoreBreakdown {
        ScoreBreakdown {
            econ: s.econ,
            fleet: s.fleet,
            defence: s.defence,
            outpost: s.outpost,
            core: s.core(),
            combat: s.combat,
            explore: s.explore,
            extra: s.total_under(rule) - s.core(),
            total: s.total_under(rule),
        }
    }
}

/// What the map and the raids paid a player so far. Resources are given as
/// their score weight: the core points they are worth once spent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct FieldTally {
    pub ore: f32,
    pub salvage: f32,
    pub derelict_loot: f32,
    /// Wreckage of repelled raids scooped at the homeworld.
    pub raid_salvage: f32,
    pub kills: u32,
    pub sites_boarded: u32,
    pub sectors_charted: u32,
    pub ships_lost: u32,
    /// Fleets destroyed in a fight, and the ships that went with them.
    pub fleets_lost: u32,
    pub ships_lost_with_fleets: u32,
    /// Fights that ended because nobody could hurt anybody.
    pub standoffs: u32,
    /// Fights the last fleet jumped out of under fire, and fights it left
    /// by the emergency jump.
    pub fights_fled: u32,
    pub fights_recalled: u32,
    /// Times a group set out after one of the player's fleets, and times
    /// one gave it up.
    pub pursuits: u32,
    pub pursuits_abandoned: u32,
    /// Rounds a fleet of the player's stood in without firing.
    pub rounds_fleeing: u32,
    /// Rounds fought, and the most ships one fleet took into a round.
    pub rounds: u32,
    pub largest_fleet: u32,
    /// Deuterium paid at the homeworld to fill fleet tanks.
    pub refuel_deuterium: f64,
    /// Raids on the homeworld.
    pub raids_repelled: u32,
    pub raids_lost: u32,
    /// Raids on the outpost.
    pub outpost_raids_repelled: u32,
    pub outpost_raids_lost: u32,
    /// The part of `outpost_raids_lost` that found it breached already,
    /// with no platform to meet them.
    pub outpost_raids_on_breach: u32,
    /// What lost raids took from the outpost's store and the holds there.
    pub outpost_raided: f32,
    /// What the outpost's extractor put into its store: its rate, as far as
    /// the store had room, counted each tick the outpost stood in service.
    /// The tick a raid breaches it or a repair ends may be off by one.
    pub outpost_yield: f32,
    /// What the extractor would have made had the outpost never been
    /// breached and its store never full: its rate at the level it stood
    /// at, every tick since level 1 was finished.
    pub outpost_potential: f32,
    /// What a full store turned away while the outpost was in service.
    pub outpost_spilled: f32,
    /// What the outpost paid from its store for repairs.
    pub outpost_repairs: f32,
    /// What `load` took from the outpost's store.
    pub outpost_hauled: f32,
    /// Ticks the player had an outpost with a finished level.
    pub outpost_ticks: u64,
    /// Ticks the outpost stood breached.
    pub outpost_ticks_breached: u64,
    /// The part of `outpost_ticks_breached` in which no repair ran because
    /// the store could not pay for one.
    pub outpost_ticks_waiting: u64,
    /// The part of it in which no repair ran because a level was building.
    pub outpost_ticks_behind_build: u64,
    /// Hours spent away from home, summed over fleets.
    pub fleet_hours: f32,
    /// The part of `fleet_hours` flown while one more combat or exploration
    /// point would not have counted; an hour in which it would have counted
    /// for half is half an hour here.
    pub unpaid_hours: f32,
}

impl FieldTally {
    /// Resources brought in by fleets away from home.
    pub fn haul(&self) -> f32 {
        self.ore + self.salvage + self.derelict_loot
    }
}

/// Ticks a player's production slots existed and ticks they stood empty,
/// counted since the player was seated.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct SlotTicks {
    ticks: u64,
    building: u64,
    building_idle: u64,
    research_idle: u64,
    shipyard_idle: u64,
}

/// Share of the time each kind of slot stood empty, 0 to 1. Building slots
/// are counted one by one; a lab or a yard not built yet counts as empty.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct SlotIdle {
    pub building: f32,
    pub research: f32,
    pub shipyard: f32,
}

impl From<SlotTicks> for SlotIdle {
    fn from(t: SlotTicks) -> SlotIdle {
        let share = |idle: u64, of: u64| if of == 0 { 0.0 } else { idle as f32 / of as f32 };
        SlotIdle {
            building: share(t.building_idle, t.building),
            research: share(t.research_idle, t.ticks),
            shipyard: share(t.shipyard_idle, t.ticks),
        }
    }
}

/// Ticks between two looks at why the slots stand empty. A prime, so the
/// looks drift across the 20-tick rhythm a present bot plans on.
const PROBE_EVERY: u64 = 61;

/// Why a slot stood empty. The engine says why an order waits; where none
/// waits, the player's own planner says what it would do with the place
/// (`ScriptedPlayer::outlook`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Why<T> {
    /// An order waits for resources. A shipyard batch cannot wait unpaid, so
    /// for the yard: the planner wants a unit the stockpile does not cover.
    pub short: T,
    /// An order waits for a prerequisite, or the planner's next choice needs
    /// something that is not built.
    pub blocked: T,
    /// The lab or the yard itself is not built.
    pub unbuilt: T,
    /// Nothing ordered, and the stockpile covers what the planner wants.
    pub unspent: T,
    /// Nothing ordered, and the stockpile does not cover what the planner wants.
    pub poor: T,
    /// Nothing ordered: the planner's own rule says not yet, though the
    /// stockpile covers a level.
    pub declined: T,
    /// Nothing ordered: the planner's rule says not yet, and the stockpile
    /// would not cover a level either.
    pub not_yet: T,
    /// Nothing left to order.
    pub done: T,
}

/// The fields of `Why`, in the order `Why::each` gives them.
pub const WHY_LABELS: [&str; 8] = ["short", "blocked", "unbuilt", "unspent", "poor", "declined", "not yet", "done"];

impl<T> Why<T> {
    pub fn each(&mut self) -> [&mut T; 8] {
        [
            &mut self.short,
            &mut self.blocked,
            &mut self.unbuilt,
            &mut self.unspent,
            &mut self.poor,
            &mut self.declined,
            &mut self.not_yet,
            &mut self.done,
        ]
    }
}

impl Why<u64> {
    /// Each reason's share of the looks that found the slot empty.
    pub fn shares(mut self) -> Why<f32> {
        let total: u64 = self.each().into_iter().map(|n| *n).sum();
        let mut shares = Why::default();
        if total > 0 {
            for (share, n) in shares.each().into_iter().zip(self.each()) {
                *share = *n as f32 / total as f32;
            }
        }
        shares
    }

    /// The place of a waiting building or research order. One that is ready
    /// with a place free is waiting for resources: its own, or those of an
    /// order ahead that reserves.
    fn waiting(&mut self, reason: WaitReason) -> &mut u64 {
        match reason {
            WaitReason::Prerequisite => &mut self.blocked,
            WaitReason::Resources | WaitReason::Slot | WaitReason::Order => &mut self.short,
        }
    }

    fn wanted(&mut self, want: Want) -> &mut u64 {
        match want {
            Want::Order { covered: true } => &mut self.unspent,
            Want::Order { covered: false } => &mut self.poor,
            Want::Prerequisite => &mut self.blocked,
            Want::NotYet { covered: true } => &mut self.declined,
            Want::NotYet { covered: false } => &mut self.not_yet,
            Want::Nothing => &mut self.done,
        }
    }
}

/// `Why` for each kind of slot. Building slots are counted one by one: a
/// waiting order accounts for one empty slot, the planner for the rest.
/// Counted in looks, one every `PROBE_EVERY` ticks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct IdleWhy<T> {
    pub building: Why<T>,
    pub research: Why<T>,
    pub shipyard: Why<T>,
}

impl IdleWhy<u64> {
    pub fn shares(self) -> IdleWhy<f32> {
        IdleWhy { building: self.building.shares(), research: self.research.shares(), shipyard: self.shipyard.shares() }
    }
}

/// An order that took its price from the stockpile.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Paid {
    pub tick: u64,
    pub queue: QueueType,
    pub item: String,
    /// Score weight of what it paid.
    pub weight: f32,
}

/// What the mines made and what a full store turned away, in score weight.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Mined {
    made: f64,
    spilled: f64,
}

/// An outpost's running sums, in score weight. A tick's yield is far below
/// what an f32 total resolves, so they are kept wide.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Extracted {
    made: f64,
    potential: f64,
    spilled: f64,
    /// `Outpost::hauled` as it was last seen, in score weight.
    hauled_seen: f32,
    /// A raid arriving this tick finds the outpost breached already.
    open: bool,
}

/// An outpost stood up for the report without a founding (`sim-run --outpost`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OutpostOrder {
    pub ring: u16,
    /// The site grade asked for; the nearest the ring has free is taken.
    pub grade: f32,
    pub level: u8,
    /// The next level is under construction from the first tick, already paid.
    pub building: bool,
    /// How many repairs the store is stocked for.
    pub repairs: u32,
}

/// A player's outpost as the run left it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OutpostSummary {
    pub sector: Hex,
    pub ring: u16,
    /// The ore the sector was generated with: metal, crystal, deuterium.
    pub ore: [Density; 3],
    pub grade: f32,
    pub level: u8,
    pub breached: bool,
    /// Breached, no repair running, and the store cannot pay for one.
    pub waiting: bool,
    pub store: Resources,
    /// Units an hour its extractor makes at this level when in service.
    pub rate_per_hour: f64,
    /// Units an hour the homeworld's mines make.
    pub home_per_hour: f64,
    /// Game hours of its yield, at pace 1 and uninterrupted, that pay back
    /// level 1 here.
    pub payback_hours: f32,
    /// The Metal Mine level its owner's homeworld would build next, and the
    /// same payback for it.
    pub next_mine_level: u8,
    pub mine_payback_hours: f32,
}

/// What a daily allowance of `share` of core (`score::Meter`) would have made
/// of the player's combat and exploration points.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct MeterReading {
    pub share: f32,
    /// What the points added to the score: `core + bonus` is the total.
    pub bonus: f32,
    /// As `FieldTally::unpaid_hours`, by the meter instead of the run's rule.
    pub unpaid_hours: f32,
}

/// A meter kept beside a player's run.
#[derive(Debug, Clone, Copy)]
struct Metered {
    meter: Meter,
    unpaid_hours: f32,
}

impl Metered {
    fn reading(&self) -> MeterReading {
        MeterReading { share: self.meter.share, bonus: self.meter.banked(), unpaid_hours: self.unpaid_hours }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Sample {
    pub tick: u64,
    pub score: ScoreBreakdown,
    pub field: FieldTally,
    pub idle: SlotIdle,
    pub idle_why: IdleWhy<u64>,
    /// Score weight of the unspent stockpile.
    pub stock: f32,
    /// Score weight of what the mines made so far, and of the part a full
    /// store turned away.
    pub mined: f32,
    pub spilled: f32,
    pub ships: usize,
    /// One reading per `Headless::meter` asked for.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub meters: Vec<MeterReading>,
}

/// One player's run, for the report.
#[derive(Debug, Clone, Serialize)]
pub struct PlayerSummary {
    pub name: String,
    pub style: String,
    pub player_id: u64,
    pub score: ScoreBreakdown,
    pub field: FieldTally,
    pub idle: SlotIdle,
    pub idle_why: IdleWhy<u64>,
    pub mined: f32,
    pub spilled: f32,
    pub samples: Vec<Sample>,
    pub buildings: BTreeMap<String, u8>,
    pub research: BTreeMap<String, u8>,
    pub ships: usize,
    pub turns: u64,
    pub orders_accepted: u64,
    pub orders_refused: u64,
    /// Every order that paid, in the order they did.
    pub orders_paid: Vec<Paid>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub meters: Vec<MeterReading>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outpost: Option<OutpostSummary>,
}

/// What the run keeps of one player. `Headless::seats` holds the player
/// itself, at the same place.
struct Entry {
    player_id: u64,
    style: String,
    tally: FieldTally,
    slots: SlotTicks,
    why: IdleWhy<u64>,
    mined: Mined,
    extracted: Extracted,
    paid: Vec<Paid>,
    meters: Vec<Metered>,
    /// Combat plus exploration points and core as the meters last saw them.
    points_seen: f32,
    core_seen: f32,
    samples: Vec<Sample>,
}

fn entry_of(entries: &mut [Entry], player_id: Option<u64>) -> Option<&mut Entry> {
    let player_id = player_id?;
    entries.iter_mut().find(|e| e.player_id == player_id)
}

fn tally_of(entries: &mut [Entry], player_id: Option<u64>) -> Option<&mut FieldTally> {
    entry_of(entries, player_id).map(|e| &mut e.tally)
}

type Seats = Vec<Seat<Box<dyn ScriptedPlayer>>>;

pub struct Headless {
    pub engine: GameEngine,
    seats: Seats,
    entries: Vec<Entry>,
    /// The rule the report scores by. The engine keeps the game's own.
    rule: ExtraRule,
    /// Shares of core to keep a `score::Meter` at for every player seated.
    meters: Vec<f32>,
}

impl Headless {
    pub fn new(world_seed: u64, pace: Pace) -> Headless {
        Headless {
            engine: GameEngine::new(world_seed, pace),
            seats: Vec::new(),
            entries: Vec::new(),
            rule: ExtraRule::GAME,
            meters: Vec::new(),
        }
    }

    /// Report scores under `rule` instead of the game's. Bots have to be
    /// seated under the same rule (`Bot::under`) to play to it.
    pub fn score_under(&mut self, rule: ExtraRule) {
        self.rule = rule;
    }

    /// Also report what a daily allowance of `share` of core would count,
    /// for every player seated after this call.
    pub fn meter(&mut self, share: f32) {
        self.meters.push(share);
    }

    /// Register the player's account and seat them; returns the player id.
    /// `style` groups players in the report. Tokens are derived from the
    /// seat number: a headless world has nothing to protect.
    pub fn add_player(&mut self, player: Box<dyn ScriptedPlayer>, style: impl Into<String>) -> Result<u64, String> {
        let name = player.name();
        let number = self.entries.len();
        let seat = Seat::join(&mut self.engine, player, None, || format!("{number:064x}"))
            .map_err(|e| format!("cannot add player '{name}': {e:?}"))?;
        let player_id = seat.player_id();
        self.seats.push(seat);
        self.entries.push(Entry {
            player_id,
            style: style.into(),
            tally: FieldTally::default(),
            slots: SlotTicks::default(),
            why: IdleWhy::default(),
            mined: Mined::default(),
            extracted: Extracted::default(),
            paid: Vec::new(),
            meters: self.meters.iter().map(|&share| Metered { meter: Meter::new(share), unpaid_hours: 0.0 }).collect(),
            points_seen: 0.0,
            core_seen: 0.0,
            samples: Vec::new(),
        });
        Ok(player_id)
    }

    pub fn players(&self) -> usize {
        self.entries.len()
    }

    /// Stand the outpost `order` asks for, one for each of `players` in
    /// turn, by one edit of the world's snapshot: nothing is founded,
    /// carried or paid. Each takes the sector of the ring whose generated
    /// ore is nearest the grade asked, among those the founding rules leave
    /// open (no Anomaly, no derelict, the spacing) and nobody stands in. A
    /// lair there is displaced, as a founding displaces it. The raid clock
    /// starts as if level 1 had just been finished, and the store holds the
    /// repair it is stocked for. Returns the sectors, in the order of `players`.
    pub fn install_outposts(&mut self, players: &[u64], order: OutpostOrder) -> Result<Vec<Hex>, String> {
        let engine = &self.engine;
        let top = if order.building { OUTPOST_MAX_LEVEL - 1 } else { OUTPOST_MAX_LEVEL };
        if !(1..=top).contains(&order.level) {
            return Err(format!("an outpost stands at level 1 to {top} here, not {}", order.level));
        }
        if !(OUTPOST_MIN_RING..=OUTPOST_MAX_RING).contains(&order.ring) {
            return Err(format!(
                "an outpost stands in rings {OUTPOST_MIN_RING} to {OUTPOST_MAX_RING}, not ring {}",
                order.ring
            ));
        }
        let (tick, pace) = (engine.current_tick(), engine.pace());
        let mut taken: Vec<Hex> = engine.outposts.values().map(|o| o.sector).collect();
        let mut stood = Vec::new();
        for &player_id in players {
            if !engine.players.contains_key(&player_id) || engine.outposts.contains_key(&player_id) {
                return Err(format!("player {player_id} is not seated, or has an outpost already"));
            }
            let off = |at: &Hex| (scaling::site_grade(engine.site_densities(*at)) - order.grade).abs();
            let open = |at: &Hex| {
                let ground = engine.world_gen.generate_sector(*at);
                ground.terrain != TerrainType::Anomaly
                    && !ground.has_derelict
                    && taken.iter().all(|other| Hex::distance(at, other) >= OUTPOST_SPACING)
                    && engine.fleets.values().all(|f| f.location != *at)
                    && engine.npc_fleets.values().all(|n| n.location != *at)
            };
            let sector = hex_ring(Hex::ORIGIN, order.ring)
                .filter(open)
                .min_by(|a, b| off(a).total_cmp(&off(b)))
                .ok_or_else(|| format!("ring {} has no free site left for player {player_id}", order.ring))?;
            taken.push(sector);

            let job = order.building.then(|| OutpostJob {
                task: OutpostTask::Build { level: order.level + 1 },
                start_tick: tick,
                end_tick: tick + scaling::outpost_build_ticks(order.level + 1, &pace),
            });
            let mut outpost = Outpost {
                sector,
                level: order.level,
                store: Resources::default(),
                job,
                breached: false,
                raid: RaidState {
                    first_seen_tick: tick,
                    next_roll_tick: tick + pace.attention_ticks(RAID_ROLL_INTERVAL as f64),
                    last_raid_tick: 0,
                    suppress_until: 0,
                    missed_rolls: 0,
                    incoming: None,
                },
                full_marks: [false; 3],
                repair_warned: false,
                dry_tanks: Vec::new(),
                founded_tick: tick,
                hauled: Resources::default(),
            };
            outpost.store = scaling::outpost_cost(outpost.repair_level())
                .scale(OUTPOST_REPAIR_SHARE * f64::from(order.repairs))
                .min(scaling::outpost_store_cap(order.level, &pace));
            stood.push((player_id, outpost));
        }

        let mut snapshot = self.engine.snapshot();
        snapshot.outposts.extend(stood);
        self.engine = GameEngine::restore(snapshot);
        Ok(taken.split_off(taken.len() - players.len()))
    }

    /// One tick of the host core, with the seats as its cast and the
    /// report's counting watching it.
    pub fn step(&mut self) -> Result<(), TickError> {
        let mut counting = Counting {
            entries: &mut self.entries,
            rule: self.rule,
            metered: !self.meters.is_empty(),
            tanks: Vec::new(),
            stores: Vec::new(),
        };
        host::step(&mut self.engine, &mut self.seats, &mut counting, |_, _| {})?;
        Ok(())
    }

    pub fn run(&mut self, ticks: u64) -> Result<(), TickError> {
        for _ in 0..ticks {
            self.step()?;
        }
        Ok(())
    }

    /// Run `ticks`, taking a sample every `every` ticks and at the end.
    pub fn run_sampled(&mut self, ticks: u64, every: u64) -> Result<(), TickError> {
        let end = self.engine.current_tick() + ticks;
        while self.engine.current_tick() < end {
            let leg = every.max(1).min(end - self.engine.current_tick());
            self.run(leg)?;
            self.sample();
        }
        Ok(())
    }

    /// Record every player's score and tally as of now.
    pub fn sample(&mut self) {
        let tick = self.engine.current_tick();
        for i in 0..self.entries.len() {
            let Some(sample) = self.sample_of(i, tick) else { continue };
            self.entries[i].samples.push(sample);
        }
    }

    fn sample_of(&self, index: usize, tick: u64) -> Option<Sample> {
        let entry = &self.entries[index];
        let player = self.engine.players.get(&entry.player_id)?;
        Some(Sample {
            tick,
            score: ScoreBreakdown::under(score::score_of(&self.engine, player), self.rule),
            field: entry.tally,
            idle: entry.slots.into(),
            idle_why: entry.why,
            stock: scaling::resource_weight(&player.resources),
            mined: entry.mined.made as f32,
            spilled: entry.mined.spilled as f32,
            ships: self.ships_of(player.id),
            meters: entry.meters.iter().map(Metered::reading).collect(),
        })
    }

    fn ships_of(&self, player_id: u64) -> usize {
        self.engine.fleets.values().filter(|f| f.owner_id == player_id).map(|f| f.ships.total() as usize).sum()
    }

    /// The player's outpost as it stands, with the two paybacks the design
    /// compares: level 1 on this site, and the Metal Mine level its owner
    /// would buy next. Both in game hours at pace 1, which no pace changes.
    fn outpost_of(&self, player: &Player) -> Option<OutpostSummary> {
        let engine = &self.engine;
        let outpost = engine.outposts.get(&player.id)?;
        let (ring, ore, pace) = (outpost.sector.dist_from_origin(), engine.site_densities(outpost.sector), engine.pace());
        let hourly = |per_tick: Resources| per_tick.total() * 3600.0;
        let slow = Pace::PERSISTENT;
        let mine = player.buildings.metal_mine;
        let mine_gain = scaling::mine_output_per_hour(BuildingType::MetalMine, mine + 1)
            - scaling::mine_output_per_hour(BuildingType::MetalMine, mine);
        let unpaid = scaling::outpost_repair_payment(outpost.level, outpost.store).1 > 0.0;
        Some(OutpostSummary {
            sector: outpost.sector,
            ring,
            ore,
            grade: scaling::site_grade(ore),
            level: outpost.level,
            breached: outpost.breached,
            waiting: outpost.breached && outpost.job.is_none() && unpaid,
            store: outpost.store,
            rate_per_hour: hourly(scaling::outpost_yield_per_tick(ore, ring, outpost.level, &pace)),
            home_per_hour: hourly(player.production_per_tick(&pace)),
            payback_hours: (scaling::outpost_cost(1).total() / hourly(scaling::outpost_yield_per_tick(ore, ring, 1, &slow)))
                as f32,
            next_mine_level: mine + 1,
            mine_payback_hours: (scaling::building_cost(BuildingType::MetalMine, mine + 1).total() / mine_gain) as f32,
        })
    }

    pub fn summaries(&self) -> Vec<PlayerSummary> {
        self.entries
            .iter()
            .zip(&self.seats)
            .filter_map(|(entry, seat)| {
                let player = self.engine.players.get(&entry.player_id)?;
                let buildings = BuildingType::ALL
                    .into_iter()
                    .map(|b| (b.label().to_string(), player.buildings.get(b)))
                    .collect();
                let research = ResearchType::ALL
                    .into_iter()
                    .map(|r| (r.label().to_string(), player.research.get(r)))
                    .collect();
                Some(PlayerSummary {
                    name: player.name.clone(),
                    style: entry.style.clone(),
                    player_id: player.id,
                    score: ScoreBreakdown::under(score::score_of(&self.engine, player), self.rule),
                    field: entry.tally,
                    idle: entry.slots.into(),
                    idle_why: entry.why,
                    mined: entry.mined.made as f32,
                    spilled: entry.mined.spilled as f32,
                    samples: entry.samples.clone(),
                    buildings,
                    research,
                    ships: self.ships_of(player.id),
                    turns: seat.turns(),
                    orders_accepted: seat.accepted(),
                    orders_refused: seat.refused(),
                    orders_paid: entry.paid.clone(),
                    meters: entry.meters.iter().map(Metered::reading).collect(),
                    outpost: self.outpost_of(player),
                })
            })
            .collect()
    }
}

/// The report's counting of one tick: it looks at the world once the seats
/// have had their turns, and again when the tick is simulated.
struct Counting<'a> {
    entries: &'a mut [Entry],
    rule: ExtraRule,
    /// Whether any meter is kept.
    metered: bool,
    /// Every fleet before the tick: its fuel, and whether it was moving.
    tanks: Vec<(u64, f64, bool)>,
    /// Every outpost before the tick: its store, and whether a raid arriving
    /// in the tick finds it breached.
    stores: Vec<(u64, Resources, bool)>,
}

impl Watch<Seats> for Counting<'_> {
    fn at(&mut self, phase: Phase, engine: &GameEngine, seats: &Seats, events: &[GameEvent]) {
        match phase {
            Phase::Turns => {
                self.tanks = engine.fleets.values().map(|f| (f.id, f.fuel, f.state == FleetStatus::Moving)).collect();
                let mending = engine.current_tick() + 1;
                self.stores = engine
                    .outposts
                    .iter()
                    .map(|(owner, o)| {
                        let mended = o.job.is_some_and(|job| job.task == OutpostTask::Repair && job.end_tick <= mending);
                        (*owner, o.store, o.breached && !mended)
                    })
                    .collect();
            }
            Phase::Tick => {
                self.count_refuelling(engine);
                self.count_outposts(engine);
                self.count_slots(engine);
                self.run_meters(engine);
                if engine.current_tick().is_multiple_of(PROBE_EVERY) {
                    self.probe(engine, seats);
                }
                self.count(engine, events);
            }
            Phase::Frames | Phase::Observed => {}
        }
    }
}

impl Counting<'_> {
    /// A tick of every meter: it drains, and takes in the points its player
    /// earned this tick. The core is read again when points come in and at
    /// every look; between two looks an allowance is the one of the last.
    fn run_meters(&mut self, engine: &GameEngine) {
        if !self.metered {
            return;
        }
        let look = engine.current_tick().is_multiple_of(PROBE_EVERY);
        for entry in self.entries.iter_mut() {
            let Some(player) = engine.players.get(&entry.player_id) else { continue };
            let points = player.combat_points + player.explore_points;
            let earned = points - entry.points_seen;
            if earned > 0.0 || look || entry.core_seen == 0.0 {
                entry.core_seen = score::score_of(engine, player).core();
            }
            for metered in &mut entry.meters {
                metered.meter.drain(entry.core_seen);
                if earned > 0.0 {
                    metered.meter.earn(earned, entry.core_seen);
                }
            }
            entry.points_seen = points;
        }
    }

    fn count_slots(&mut self, engine: &GameEngine) {
        let pace = engine.pace();
        for entry in self.entries.iter_mut() {
            let Some(player) = engine.players.get(&entry.player_id) else { continue };
            let slots = &mut entry.slots;
            let building = player.building_slots() as u64;
            slots.building += building;
            slots.building_idle += building.saturating_sub(player.building_queue.len() as u64);
            slots.ticks += 1;
            slots.research_idle += u64::from(player.research_queue.is_none());
            slots.shipyard_idle += u64::from(player.ship_queue.is_none());

            let made = player.mined_per_tick(&pace);
            let cap = scaling::storage_cap(&player.buildings, &pace);
            let mut spilled = Resources::default();
            for kind in ResourceKind::ALL {
                if player.resources.get(kind) >= cap.get(kind) {
                    *spilled.get_mut(kind) = made.get(kind);
                }
            }
            entry.mined.made += f64::from(scaling::resource_weight(&made));
            entry.mined.spilled += f64::from(scaling::resource_weight(&spilled));
        }
    }

    /// One look at every player: how many fleets are out and whether a
    /// point they earn would count; then every empty slot, which order it is
    /// waiting for and why, or what its owner's planner would do with it.
    fn probe(&mut self, engine: &GameEngine, seats: &Seats) {
        let rule = self.rule;
        let tick = engine.current_tick();
        let pace = engine.pace();
        for (entry, seat) in self.entries.iter_mut().zip(seats) {
            let id = entry.player_id;
            let Some(player) = engine.players.get(&id) else { continue };

            let out = engine.fleets.values().filter(|f| f.owner_id == id && f.location != player.homeworld).count();
            let score = score::score_of(engine, player);
            let hours = out as f32 * PROBE_EVERY as f32 / 3600.0;
            entry.tally.fleet_hours += hours;
            entry.tally.unpaid_hours += hours * (1.0 - rule.next_point(score.combat + score.explore, score.core()));
            for metered in &mut entry.meters {
                metered.unpaid_hours += hours * (1.0 - metered.meter.next_point(score.core()));
            }

            let Some(outlook) = views::full_state(engine, id).and_then(|state| seat.player().outlook(&state)) else {
                continue;
            };
            let waits = queue::project(player, tick, &pace);
            let why = &mut entry.why;

            let mut waiting = waits.buildings.iter();
            for _ in player.building_queue.len()..player.building_slots() {
                *match waiting.next() {
                    Some(wait) => why.building.waiting(wait.reason),
                    None => why.building.wanted(outlook.building),
                } += 1;
            }

            if player.research_queue.is_none() {
                let lab = &mut why.research;
                *if player.buildings.research_lab == 0 {
                    &mut lab.unbuilt
                } else if let Some(wait) = waits.research.first() {
                    lab.waiting(wait.reason)
                } else {
                    lab.wanted(outlook.research)
                } += 1;
            }

            if player.ship_queue.is_none() {
                let yard = &mut why.shipyard;
                *if player.buildings.shipyard == 0 {
                    &mut yard.unbuilt
                } else if !player.ship_pending.is_empty() {
                    &mut yard.blocked
                } else if outlook.shipyard == (Want::Order { covered: false }) {
                    &mut yard.short
                } else {
                    yard.wanted(outlook.shipyard)
                } += 1;
            }
        }
    }

    /// Fuel only rises at the homeworld, where it is bought; a fleet that
    /// jumped out during the tick is filled again before it leaves the sector.
    fn count_refuelling(&mut self, engine: &GameEngine) {
        for &(fleet_id, fuel, was_moving) in &self.tanks {
            let Some(fleet) = engine.fleets.get(&fleet_id) else { continue };
            let at_home = engine.players.get(&fleet.owner_id).is_some_and(|p| p.homeworld == fleet.location);
            if !at_home {
                continue;
            }
            let jumped = !was_moving && fleet.state == FleetStatus::Moving;
            let burned = if jumped { engine.hop_fuel_cost(fleet) } else { 0.0 };
            let bought = fleet.fuel - fuel + burned;
            if bought > 0.0
                && let Some(t) = tally_of(self.entries, Some(fleet.owner_id))
            {
                t.refuel_deuterium += bought * FUEL_DEUT_PER_UNIT;
            }
        }
    }

    /// One tick of every outpost: what `load` took since the last look, a
    /// repair that began, and then either breached, or extracting into the
    /// room its store had before the tick.
    fn count_outposts(&mut self, engine: &GameEngine) {
        let (tick, pace) = (engine.current_tick(), engine.pace());
        let weight = |r: Resources| f64::from(scaling::resource_weight(&r));
        for &(owner, before, open) in &self.stores {
            let Some(outpost) = engine.outposts.get(&owner) else { continue };
            let Some(entry) = entry_of(self.entries, Some(owner)) else { continue };
            let (t, sums) = (&mut entry.tally, &mut entry.extracted);
            sums.open = open;

            let hauled = scaling::resource_weight(&outpost.hauled);
            t.outpost_hauled += if hauled >= sums.hauled_seen { hauled - sums.hauled_seen } else { hauled };
            sums.hauled_seen = hauled;
            if outpost.level == 0 {
                continue;
            }

            t.outpost_ticks += 1;
            let ring = outpost.sector.dist_from_origin();
            let rate = scaling::outpost_yield_per_tick(engine.site_densities(outpost.sector), ring, outpost.level, &pace);
            sums.potential += weight(rate);
            if outpost.job.is_some_and(|job| job.task == OutpostTask::Repair && job.start_tick == tick) {
                let price = Resources { metal: scaling::outpost_repair_price(outpost.level), ..Resources::default() };
                t.outpost_repairs += scaling::resource_weight(&price);
            }
            if outpost.breached {
                t.outpost_ticks_breached += 1;
                let unpaid = outpost.job.is_none() && scaling::outpost_repair_payment(outpost.level, outpost.store).1 > 0.0;
                t.outpost_ticks_waiting += u64::from(unpaid);
                let building = outpost.job.is_some_and(|job| job.task != OutpostTask::Repair);
                t.outpost_ticks_behind_build += u64::from(building);
            } else {
                let made = before.add_capped(rate, scaling::outpost_store_cap(outpost.level, &pace)).sub(before);
                sums.made += weight(made);
                sums.spilled += weight(rate.sub(made));
            }
            t.outpost_yield = sums.made as f32;
            t.outpost_potential = sums.potential as f32;
            t.outpost_spilled = sums.spilled as f32;
        }
    }

    fn count(&mut self, engine: &GameEngine, events: &[GameEvent]) {
        if events.is_empty() {
            return;
        }
        let entries = &mut *self.entries;
        let flying = |fleet_id: u64| engine.fleets.get(&fleet_id).map(|f| f.owner_id);
        let named = |name: &str| engine.players.values().find(|p| p.name == name).map(|p| p.id);
        let home_of = |player_id: u64| engine.players.get(&player_id).map(|p| p.homeworld);

        for event in events {
            match &event.kind {
                EventKind::ResourceHarvested(e) => {
                    if let Some(t) = tally_of(entries, flying(e.fleet_id)) {
                        t.ore += scaling::resource_weight(&e.resources);
                    }
                }
                EventKind::SalvageCollected(e) => {
                    let owner = flying(e.fleet_id);
                    let at_home = owner.and_then(home_of) == Some(e.sector);
                    if let Some(t) = tally_of(entries, owner) {
                        let gain = scaling::resource_weight(&e.resources);
                        if at_home { t.raid_salvage += gain } else { t.salvage += gain }
                    }
                }
                EventKind::SiteExplored(e) => {
                    if let Some(t) = tally_of(entries, flying(e.fleet_id)) {
                        t.derelict_loot += scaling::resource_weight(&e.resources);
                        t.sites_boarded += 1;
                    }
                }
                EventKind::ChartDelivered(e) => {
                    if let Some(t) = tally_of(entries, flying(e.fleet_id)) {
                        t.sectors_charted += e.sectors;
                    }
                }
                EventKind::CombatEnded(e) => {
                    for owner in &e.owners {
                        if let Some(t) = tally_of(entries, named(owner)) {
                            t.kills += u32::from(e.player_victory);
                            t.standoffs += u32::from(e.outcome == BattleOutcome::Standoff);
                            t.fights_fled += u32::from(e.outcome == BattleOutcome::Fled);
                            t.fights_recalled += u32::from(e.outcome == BattleOutcome::Withdrawn);
                        }
                    }
                    for loss in e.losses.iter().filter(|loss| loss.destroyed) {
                        if let Some(t) = tally_of(entries, loss.owner.as_deref().and_then(named)) {
                            t.fleets_lost += 1;
                            t.ships_lost_with_fleets += loss.lost.iter().map(|u| u.count).sum::<u32>();
                        }
                    }
                }
                EventKind::BattleRound(e) => {
                    for group in &e.fleets.groups {
                        if let Some(t) = tally_of(entries, group.owner.as_deref().and_then(named)) {
                            let lost = group.units.iter().map(|u| u.lost).sum::<u32>();
                            let left = group.units.iter().map(|u| u.count).sum::<u32>();
                            t.ships_lost += lost;
                            t.rounds += 1;
                            t.rounds_fleeing += u32::from(group.fleeing);
                            t.largest_fleet = t.largest_fleet.max(left + lost);
                        }
                    }
                }
                EventKind::Pursuit(e) => {
                    if let Some(t) = tally_of(entries, named(&e.owner)) {
                        t.pursuits += u32::from(e.phase == PursuitPhase::Began);
                        t.pursuits_abandoned += u32::from(e.phase == PursuitPhase::Abandoned);
                    }
                }
                EventKind::ShipsLost(e) => {
                    if let Some(t) = tally_of(entries, named(&e.owner)) {
                        t.ships_lost += e.ships.iter().map(|s| s.count).sum::<u32>();
                    }
                }
                EventKind::RaidResolved(e) => {
                    if let Some(entry) = entry_of(entries, Some(e.player_id)) {
                        let t = &mut entry.tally;
                        match (e.outpost.is_some(), e.defended) {
                            (false, true) => t.raids_repelled += 1,
                            (false, false) => t.raids_lost += 1,
                            (true, true) => t.outpost_raids_repelled += 1,
                            (true, false) => {
                                t.outpost_raids_lost += 1;
                                t.outpost_raids_on_breach += u32::from(entry.extracted.open);
                                t.outpost_raided += scaling::resource_weight(&e.resources_lost);
                            }
                        }
                        t.ships_lost += e.ships_lost;
                    }
                }
                EventKind::Queue(e) if e.paid.total() > 0.0 => {
                    if let Some(entry) = entry_of(entries, e.player_id) {
                        entry.paid.push(Paid {
                            tick: event.tick,
                            queue: e.queue_type,
                            item: e.item.clone(),
                            weight: scaling::resource_weight(&e.paid),
                        });
                    }
                }
                _ => {}
            }
        }
    }
}
