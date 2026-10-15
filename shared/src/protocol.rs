// Wire protocol types shared between server and client.
// All messages are JSON-serialized over WebSocket.

use serde::{Deserialize, Serialize};

use crate::constants::{Density, Drive, ResourceKind, ShipClass, TerrainType};
use crate::scaling::{
    self, BuildingLevels, BuildingType, DefenceKind, PrereqRef, Prereqs, RatioLabel, Refit, ResearchLevels,
    ResearchType, ShipyardItem,
};
use crate::hex::Hex;
use crate::pace::Pace;
use crate::world::{HubService, HUB};
use crate::Resources;

// ── Client → Server Messages ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    #[serde(rename = "auth")]
    Auth(AuthRequest),
    #[serde(rename = "command")]
    Command(Command),
    #[serde(rename = "policy_update")]
    PolicyUpdate(PolicyUpdate),
    #[serde(rename = "request_full_state")]
    RequestFullState,
}

/// First message on a connection. The server decides what it means from the
/// name alone, so there is no separate register/login action:
///
/// - unknown name: valid names register a new account and the reply carries
///   the new secret in `AuthResult.token` (this is the only time it is sent);
///   a world with no free home seat replies `WorldFull` instead;
/// - known name: `token` must match the one issued earlier, otherwise the
///   reply is `TokenRequired` / `InvalidToken`;
/// - an account from before tokens existed has none yet and is claimed by
///   its next login, which is issued one the same way.
///
/// Names are matched case-insensitively ("admiral" logs in to "Admiral"), so
/// no two accounts can differ only by case. An exact-case match wins, which
/// keeps any case-variant accounts registered before this rule reachable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthRequest {
    pub player_name: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub token: Option<String>,
    /// True when an AI agent, not a person, flies this account. It is only
    /// read when the account is created and only labels the leaderboard:
    /// agents and humans are one population with one scoring formula.
    #[serde(default)]
    pub agent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action")]
pub enum Command {
    /// Jump to the connected sector `target`. The fuel is paid at once and
    /// the jump charges for the fleet's `jump_ticks`, set by its slowest
    /// ship.
    ///
    /// From inside a fight it is flight (`FleetState.flight` quotes it): the
    /// fleet stops firing and is fired on every round until it is gone, one
    /// round fewer than its jump takes ticks and never fewer than
    /// `FLEE_MIN_ROUNDS`. `stop` calls the jump off, the fuel is forfeit and
    /// the fleet fights on. When the last fleet has left, a group that was
    /// beating it may follow (`FleetState.pursued_by`).
    #[serde(rename = "move")]
    Move { fleet_id: u64, target: Hex },
    #[serde(rename = "harvest")]
    Harvest { fleet_id: u64, resource: HarvestResource },
    #[serde(rename = "attack")]
    Attack { fleet_id: u64, target_fleet_id: u64 },
    #[serde(rename = "recall")]
    Recall { fleet_id: u64 },
    #[serde(rename = "collect_salvage")]
    CollectSalvage { fleet_id: u64 },
    /// Queue a building level. The order is accepted whenever it fits the
    /// queue, free slot or not, affordable or not, and gets a stable `id`
    /// (reported in the `Queue` event and the queue lists). A free slot
    /// starts the earliest waiting order that can start now (prerequisites
    /// built, stockpile covers it); an order that cannot start keeps its
    /// place and is re-checked every tick. `reserve: true` makes the order
    /// hold the line: while it is ready but short of resources, nothing
    /// behind it in the same queue starts, so an expensive order cannot be
    /// starved by cheap ones. Refused only when it can never start
    /// (`PrerequisitesNotMet`, `MaxLevelReached`, `StorageTooSmall`) or the
    /// waiting line is full (`QueueFull`).
    #[serde(rename = "build")]
    Build {
        building_type: BuildingType,
        #[serde(default)]
        reserve: bool,
    },
    /// Queue a research level; same rule as `build`.
    #[serde(rename = "research")]
    Research {
        tech: ResearchType,
        #[serde(default)]
        reserve: bool,
    },
    /// Order `count` ships as one batch. Unlike a building or research
    /// order, a batch is accepted only if the stockpile covers every unit
    /// now, and it pays then; otherwise it is refused with
    /// `InsufficientResources` and a message naming the shortfall per
    /// resource. A paid batch gets a stable `id` and starts at once when the
    /// yard is free and what it needs is built; otherwise it waits for the
    /// yard or for something queued ahead of it (a Shipyard level, a tech).
    /// Also refused with `NoShipyard`, `ShipLocked`, `StorageTooSmall` (a
    /// batch larger than a stockpile cap) or `QueueFull`.
    #[serde(rename = "build_ship")]
    BuildShip {
        ship_class: ShipClass,
        #[serde(default = "default_ship_count")]
        count: u16,
    },
    /// Order `count` defences in the shipyard queue; same rule as
    /// `build_ship`. They stay at home and never burn fuel; rejected with
    /// `DefenceLocked` until the Shipyard and research it needs are in
    /// place, and with `MaxLevelReached` for a second dome of a kind.
    #[serde(rename = "build_defence")]
    BuildDefence {
        kind: DefenceKind,
        #[serde(default = "default_ship_count")]
        count: u16,
    },
    /// Cancel an item that has started, by its stable `id`. A building or
    /// research level gets back half of what it paid. A shipyard batch gets
    /// back half the price of every unit not yet delivered (the unit under
    /// way is not delivered; units already built are kept). A `Queue` event
    /// with `action: Cancelled` says what was refunded. `InvalidTarget` when
    /// `id` is not running in `queue_type`.
    #[serde(rename = "cancel_build")]
    CancelBuild { queue_type: QueueType, id: u64 },
    /// Remove an item that is still waiting in the queue, by its stable `id`.
    /// A waiting building or research order has paid nothing, so nothing is
    /// refunded; a waiting shipyard batch paid when it was ordered and gets
    /// half back (a `Queue` event with `action: Cancelled` says how much).
    /// `InvalidTarget` when `id` is not waiting in `queue_type`, even if it
    /// exists in another queue.
    #[serde(rename = "cancel_queued")]
    CancelQueued { queue_type: QueueType, id: u64 },
    /// Ask for the score table. The reply is a `leaderboard` message.
    #[serde(rename = "leaderboard")]
    Leaderboard {
        #[serde(default = "default_leaderboard_limit")]
        limit: u16,
    },
    /// Ask what one jump of `fleet_id` to the adjacent sector `target` would
    /// cost, without moving. The reply is a `preview_move` message.
    #[serde(rename = "preview_move")]
    PreviewMove { fleet_id: u64, target: Hex },
    /// Ask how `fleet_id` would fare in each of `sectors` (at most
    /// `MAX_ODDS_SECTORS`, anywhere on the map, its own sector included),
    /// without moving. The reply is an `odds` message.
    #[serde(rename = "odds")]
    Odds { fleet_id: u64, sectors: Vec<Hex> },
    #[serde(rename = "stop")]
    Stop {
        #[serde(default)]
        fleet_id: u64,
    },
    #[serde(rename = "scan")]
    Scan {
        #[serde(default)]
        fleet_id: u64,
    },
    #[serde(rename = "explore_site")]
    ExploreSite {
        #[serde(default)]
        fleet_id: u64,
    },
    /// Detach `ships`, so many of each class named, from `fleet_id` into a
    /// new fleet at the same sector. Whole ships of a class go first and
    /// damaged ones only when the whole run out; with `damaged_first` the
    /// most damaged go first, which is how the wounded are sent home. The
    /// new fleet starts with no standing orders, fights with the levels of
    /// the fleet it left (`FleetState.refit`) and takes a share of fuel and
    /// cargo proportional to the tank and hold it carries off. At least one
    /// ship must stay behind. The fleet must be idle. Rejected with
    /// `FleetLimitReached` at the fleet cap, `InvalidTarget` for a class
    /// named twice, a count of zero, more ships of a class than the fleet
    /// has, or an attempt to take every ship. At the homeworld this is also
    /// how docked ships are launched as their own fleet. The reply is an
    /// `Alert` (info) event naming the new fleet id.
    #[serde(rename = "split")]
    Split {
        fleet_id: u64,
        ships: Vec<ShipCount>,
        #[serde(default)]
        damaged_first: bool,
    },
    /// Fold `other_fleet_id` into `fleet_id`. Both must be yours, idle, and in
    /// the same sector (`NotInSector` otherwise). Ships, cargo and fuel are
    /// pooled (fuel up to the combined tank), the merged fleet keeps the
    /// standing orders of `fleet_id`, and `other_fleet_id` ceases to exist.
    /// Away from home it fights with the lower of the two fleets' Weapons,
    /// Shielding and Armour levels until it docks at home. Refused with
    /// `InvalidCommand` when the two hold more than `MAX_FLEET_SHIPS` ships
    /// between them. This works anywhere, so a fresh fleet can rescue a
    /// stranded one.
    #[serde(rename = "merge")]
    Merge { fleet_id: u64, other_fleet_id: u64 },
    /// Sell `amount` units of `give` from the hold of `fleet_id` to the hub's
    /// exchange and take `want` into the hold; without `amount`, all of
    /// `give` the fleet carries. The fleet must stand idle on a hub sector
    /// that offers the exchange (`WorldInfo.hub`). The answer is an
    /// `Exchanged` event; see `ExchangeTerms` for the rate. Refused, with
    /// nothing changed, when the fleet is elsewhere (`NotInSector`) or busy
    /// (`OnCooldown`), holds less than `amount` or none (`NoResources`),
    /// would get back less than one unit, names one resource twice or an
    /// `amount` that is not positive (`InvalidCommand`), or the trade would
    /// overfill the hold (`CargoFull`).
    #[serde(rename = "exchange")]
    Exchange {
        fleet_id: u64,
        give: ResourceKind,
        want: ResourceKind,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        amount: Option<f64>,
    },
    /// Ask what `exchange` with the same fields would give, without trading.
    /// The reply is an `exchange_quote` message. It answers for a fleet
    /// anywhere and for an `amount` the fleet does not carry: `refusal` then
    /// says why the trade itself would be refused right now.
    #[serde(rename = "exchange_quote")]
    ExchangeQuote {
        fleet_id: u64,
        give: ResourceKind,
        want: ResourceKind,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        amount: Option<f64>,
    },
    /// Move `amount` units of `resource` from the stockpile into the hold of
    /// `fleet_id`; without `amount`, all the stockpile holds and the hold
    /// has room for. One resource an order. The fleet must stand idle at
    /// its owner's homeworld and fly no standing orders. It then keeps
    /// everything in its hold aboard while it is docked
    /// (`FleetState.laden`); once it has been away, what it brings home
    /// unloads as cargo always does. `unload`, or giving the fleet standing
    /// orders, ends it sooner. The answer is an `Alert` (info) event saying what
    /// went aboard. Refused, with nothing moved, when the fleet is elsewhere
    /// (`NotInSector`) or busy (`OnCooldown`), the stockpile holds less than
    /// `amount` or none (`NoResources`), the hold lacks the room
    /// (`CargoFull`), `amount` is not positive or the fleet is under
    /// standing orders (`InvalidCommand`).
    ///
    /// At the player's outpost the same order takes from the outpost's
    /// store instead. Nothing there unloads by itself, so the fleet is not
    /// marked `laden`.
    #[serde(rename = "load")]
    Load {
        fleet_id: u64,
        resource: ResourceKind,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        amount: Option<f64>,
    },
    /// End what `load` began: the hold of `fleet_id`, docked at home, goes
    /// back to the stockpile as far as the caps allow, and the rest follows
    /// as room opens, as for any docked fleet. The answer is an `Alert`
    /// (info) event. Refused when the fleet is elsewhere (`NotInSector`),
    /// busy (`OnCooldown`) or was not loaded (`InvalidCommand`).
    ///
    /// At the player's outpost it puts the whole hold of any fleet into the
    /// outpost's store as far as the store has room, and is never refused
    /// for a hold that was not loaded by hand.
    #[serde(rename = "unload")]
    Unload { fleet_id: u64 },
    /// Turn deuterium in the hold of `fleet_id` into fuel, until the tank is
    /// full or the deuterium is gone. It is done at once, anywhere, and at
    /// a loss: a unit of deuterium fills `REFINE_LOSS` less of the tank than
    /// it does at a dock. `FleetState.refine` says beforehand what the order
    /// would turn into what. Deuterium loaded at a dock counts as any other.
    /// The answer is an `Alert` (info) event with both figures. Refused,
    /// with nothing changed, while the fleet is in combat or charging a
    /// jump (`OnCooldown`), when its hold carries no deuterium
    /// (`NoResources`) or its tank is full (`InvalidCommand`).
    #[serde(rename = "refine")]
    Refine { fleet_id: u64 },
    /// Found the player's outpost in the sector `fleet_id` stands in, paid
    /// from its hold (`OutpostSurvey.cost`). The fleet must stand idle
    /// there and fly no standing orders; the sector must lie in rings 9 to
    /// 20, be no Anomaly, hold no derelict and no group, and lie at least
    /// 3 hexes from every other outpost; a player has one outpost. The
    /// answer is an `Alert` (info) event with the tick the first level is
    /// finished. Refused, with nothing changed and the sentence
    /// `outpost_survey` gives as `refusal`: `InvalidTarget` for a sector
    /// that does not qualify, `InvalidCommand` with an outpost already or
    /// under standing orders, `NoResources` naming what the hold lacks,
    /// `OnCooldown` for a fleet that is moving, fighting or busy.
    #[serde(rename = "found_outpost")]
    FoundOutpost { fleet_id: u64 },
    /// Ask what an outpost in `sector` would be worth, without founding:
    /// the sector `fleet_id` stands in when none is named, otherwise any
    /// sector on the player's chart (`InvalidTarget` for one never seen).
    /// The reply is an `outpost_survey` message.
    #[serde(rename = "outpost_survey")]
    OutpostSurvey {
        fleet_id: u64,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        sector: Option<Hex>,
    },
    /// Build the next level of the player's outpost, paid in full from the
    /// outpost's own store at that moment (`OutpostState.next_level`). The
    /// answer is an `Alert` (info) event. Refused with
    /// `InsufficientResources` and the exact shortfall when the store does
    /// not cover it, `MaxLevelReached` at the last level, `InvalidCommand`
    /// with no outpost, while it builds or repairs, or while it is breached.
    #[serde(rename = "upgrade_outpost")]
    UpgradeOutpost,
    /// Give up the player's outpost. Its store drifts as wreckage in the
    /// sector, nothing is refunded and another may be founded. The answer
    /// is an `Alert` (info) event. `InvalidCommand` with no outpost.
    #[serde(rename = "abandon_outpost")]
    AbandonOutpost,
    /// Found a relay in the sector `fleet_id` stands in, paid from its hold
    /// (`RelaySurvey.cost`). The fleet must stand idle there and fly no
    /// standing orders; the sector must lie in ring 9 or deeper, be no
    /// Anomaly, and hold no hub, lair, derelict, group, outpost or relay;
    /// the player may keep `RelaysState.limit` relays. The answer is an
    /// `Alert` (info) event and `RelayFounded`. Refused, with nothing
    /// changed and the sentence `relay_survey` gives as `refusal`:
    /// `InvalidTarget` for a sector that does not qualify,
    /// `InvalidCommand` at the limit or under standing orders,
    /// `NoResources` naming what the hold lacks, `OnCooldown` for a fleet
    /// that is moving, fighting or busy.
    ///
    /// A relay is a depot: `load` and `unload` of a fleet standing there
    /// move deuterium to and from its store (`RelayState.store`), and a
    /// fleet flown by hand that stands there has its tank filled from it.
    /// It makes nothing, mends nothing and no raid comes. It is gone, with
    /// its store, once no fleet of its owner has stood there for
    /// `RelayState.decay_in` ticks.
    #[serde(rename = "found_relay")]
    FoundRelay { fleet_id: u64 },
    /// Ask what a relay in `sector` would cost, without founding: the sector
    /// `fleet_id` stands in when none is named. The reply is a
    /// `relay_survey` message.
    #[serde(rename = "relay_survey")]
    RelaySurvey {
        fleet_id: u64,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        sector: Option<Hex>,
    },
}

fn default_ship_count() -> u16 {
    1
}

fn default_leaderboard_limit() -> u16 {
    20
}

/// Which resource a harvest order mines. `Auto` takes everything the sector
/// has, metal then crystal then deuterium, until the hold is full; a specific
/// resource mines only that one and is rejected with
/// `ErrorCode::ResourceNotPresent` if the sector has none of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HarvestResource {
    Metal,
    Crystal,
    Deuterium,
    Auto,
}

/// Standing orders for a fleet: assign a doctrine, the server flies it.
/// `preset: "manual"` clears the policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyUpdate {
    pub fleet_id: u64,
    pub preset: PolicyPreset,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub params: Option<PolicyParams>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyPreset {
    Manual,
    Prospect,
    MineAndReturn,
    SalvageAndSites,
    PatrolHome,
}

impl PolicyPreset {
    pub const ALL: [PolicyPreset; 5] = [
        PolicyPreset::Manual,
        PolicyPreset::Prospect,
        PolicyPreset::MineAndReturn,
        PolicyPreset::SalvageAndSites,
        PolicyPreset::PatrolHome,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PolicyPreset::Manual => "manual",
            PolicyPreset::Prospect => "prospect",
            PolicyPreset::MineAndReturn => "mine+return",
            PolicyPreset::SalvageAndSites => "salvage+sites",
            PolicyPreset::PatrolHome => "patrol home",
        }
    }
}

/// Doctrine thresholds. Every field is optional on the wire; an omitted field
/// (or an omitted `params` object) takes the default in constants.rs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PolicyParams {
    /// Spare fuel, as a percent of the tank, kept on top of the exact fuel
    /// cost of the route home. The autopilot heads home when fuel falls below
    /// route cost plus this reserve, and never takes a hop outward that would
    /// leave it unable to return. Clamped to 0..=90. Default 10.
    pub min_fuel_pct: u8,
    /// Head home when cargo fills past this percent. Clamped to 10..=100.
    /// Default 85.
    pub cargo_return_pct: u8,
    /// Maximum distance in hexes from the HOMEWORLD that the autopilot will
    /// claim, prospect, salvage or hunt at; never measured from the last
    /// claim. Clamped to 1..=30. Default 4.
    pub max_range: u8,
    /// Engage hostiles only if the fleet's odds against them (`SectorOdds`
    /// `ratio`) are at least this / 10. It is also exactly the least ratio,
    /// against what the player knows of a sector, at which any doctrine will
    /// enter it, hostiles there or not: the setting is honoured as given,
    /// including a deliberate value below EVEN (1.1). A refused sector is
    /// reported as a `PolicyAction` hold with the reason; a standing hold is
    /// repeated at most once a minute and again only if its cause changes.
    /// Clamped to 1..=100. Default 15 (1.5x, FAVOURABLE).
    pub engage_ratio_x10: u8,
    /// Leave a fight once the fleet's odds in it fall below this / 10. Every
    /// `POLICY_EVAL_INTERVAL` ticks of a fight the odds are read again from
    /// the two sides as they then stand, and below the ratio the fleet jumps
    /// out toward home under fire, as a `move` out of a fight does, and
    /// reports it as a `PolicyAction` `flee` with the odds. Whatever doctrine
    /// it flies, and however it came into the fight. Clamped to 1..=100 and
    /// never above `engage_ratio_x10`. Default 9 (0.9x, the edge of RISKY).
    pub disengage_ratio_x10: u8,
    /// The share of the homeworld's deuterium income, in percent, that this
    /// fleet may spend on fuel while it flies under these orders. The fleet
    /// earns an allowance at that share of the current deuterium production
    /// and can save up the price of one full tank; it starts empty, so an
    /// empire with no deuterium income yet keeps its stock for its queues.
    /// At the dock its tank is filled in one go once the allowance and the stockpile cover what is missing, and
    /// until then the fleet waits there and says so (a `PolicyAction` hold
    /// with the time left); it never leaves on standing orders with a
    /// part-filled tank. While the deuterium store is full the fill is free
    /// of the allowance: that income would overflow anyway. 100 lifts the
    /// bound (the tank takes whatever the stockpile holds, ahead of every
    /// queue, as a fleet flown by hand does). Clamped to 0..=100. Default 20.
    pub fuel_budget_pct: u8,
    /// Refine before turning home for fuel. When the fleet's fuel no longer
    /// covers the way home and the reserve, or the next jump outward would
    /// leave too little for them, it turns the deuterium in its hold into
    /// fuel first, as a `refine` order does and at the same loss, reports it
    /// as a `PolicyAction` `refine`, and is weighed again; it turns home
    /// only if that was not enough. What it refines is not charged to
    /// `fuel_budget_pct`. Default false.
    pub refine: bool,
}

impl Default for PolicyParams {
    fn default() -> Self {
        PolicyParams {
            min_fuel_pct: crate::constants::POLICY_DEFAULT_MIN_FUEL_PCT,
            cargo_return_pct: crate::constants::POLICY_DEFAULT_CARGO_RETURN_PCT,
            max_range: crate::constants::POLICY_DEFAULT_MAX_RANGE,
            engage_ratio_x10: crate::constants::POLICY_DEFAULT_ENGAGE_RATIO_X10,
            disengage_ratio_x10: crate::constants::POLICY_DEFAULT_DISENGAGE_RATIO_X10,
            fuel_budget_pct: crate::constants::POLICY_DEFAULT_FUEL_BUDGET_PCT,
            refine: false,
        }
    }
}

/// The standing orders a fleet flies under, as `FleetState` carries them:
/// the doctrine and every setting in force, after the server's clamping.
/// The doctrine is never `manual`; a fleet flown by hand has no orders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StandingOrders {
    pub preset: PolicyPreset,
    pub params: PolicyParams,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueueType {
    Building,
    Ship,
    Research,
}

// ── Server → Client Messages ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    #[serde(rename = "auth_result")]
    AuthResult(AuthResult),
    #[serde(rename = "tick_update")]
    TickUpdate(TickUpdate),
    #[serde(rename = "full_state")]
    FullState(GameState),
    #[serde(rename = "event")]
    Event(GameEvent),
    #[serde(rename = "error")]
    Error(ErrorMessage),
    #[serde(rename = "preview_move")]
    PreviewMove(MovePreview),
    #[serde(rename = "odds")]
    Odds(FleetOdds),
    #[serde(rename = "leaderboard")]
    Leaderboard(LeaderboardReply),
    #[serde(rename = "exchange_quote")]
    ExchangeQuote(ExchangeQuote),
    #[serde(rename = "outpost_survey")]
    OutpostSurvey(OutpostSurvey),
    #[serde(rename = "relay_survey")]
    RelaySurvey(RelaySurvey),
}

/// Answer to `relay_survey`: what a relay in `sector` would cost and hold,
/// and why it cannot be founded now. `refusal` depends on the asking fleet
/// and on the moment; the rest follows from the sector's ring and the
/// player's research.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelaySurvey {
    pub fleet_id: u64,
    pub sector: Hex,
    /// Hexes from the centre of the map.
    pub ring: u16,
    /// What founding takes from the hold.
    pub cost: Resources,
    /// Deuterium the store holds.
    pub store_cap: f64,
    /// Ticks the relay stands with no fleet of its owner in its sector
    /// before it is gone.
    pub decay_ticks: u64,
    /// Relays the player has now.
    pub count: u32,
    /// Relays the player may keep, and the technology that raises it.
    pub limit: u32,
    pub technology: ResearchType,
    /// Why `found_relay` would be refused right now; absent when the fleet
    /// could found here this tick.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub refusal: Option<String>,
}

/// A relay of the player's: a depot of deuterium, owner-only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelayState {
    pub sector: Hex,
    /// Hexes from the centre of the map.
    pub ring: u16,
    /// Deuterium in the store.
    pub store: f64,
    pub store_cap: f64,
    /// Ticks until the relay is gone unless a fleet of the owner stands in
    /// its sector: a visit sets it back to the full span, and it stays there
    /// while a fleet stands in the sector.
    pub decay_in: u64,
    pub founded_tick: u64,
}

/// The player's relays and how many they may keep. `limit` is the base
/// plus a step for each `levels_per_step` levels of `technology`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelaysState {
    pub limit: u32,
    pub technology: ResearchType,
    /// The player's level of `technology`.
    pub level: u8,
    pub relays: Vec<RelayState>,
}

impl Default for RelaysState {
    fn default() -> RelaysState {
        RelaysState { limit: 0, technology: ResearchType::HyperspaceTechnology, level: 0, relays: Vec::new() }
    }
}

/// Answer to `outpost_survey`: what an outpost in `sector` would cost, make
/// and face, worked out by the server. `ring`, `cost`, `build_ticks`,
/// `raid_power` and `platform_power` follow from where the sector lies and
/// are told of any sector. `site` is what only eyes on the sector tell.
/// `refusal` depends on the asking fleet and on the moment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutpostSurvey {
    pub fleet_id: u64,
    pub sector: Hex,
    /// Hexes from the centre of the map.
    pub ring: u16,
    /// The site itself. Absent for a sector the player has never seen:
    /// `refusal` then says so, and says nothing of what is there.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub site: Option<SurveySite>,
    /// What founding takes from the hold: level 1.
    pub cost: Resources,
    /// Ticks level 1 takes to build at this world's pace.
    pub build_ticks: u64,
    /// The least and the most a raid on an outpost in this ring can be.
    pub raid_power: PowerRange,
    /// What the platform defends with at level 1.
    pub platform_power: f32,
    /// The defences the platform is at level 1.
    #[serde(default)]
    pub platform: Vec<UnitCount>,
    /// The raid of this ring at its typical power against that platform
    /// alone: what it flies and how the fight is expected to end.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub raid: Option<RaidForecast>,
    /// Why `found_outpost` would be refused right now; absent when the
    /// fleet could found here this tick.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub refusal: Option<String>,
}

/// A raid on the homeworld or on the outpost, announced or expected, and how
/// it would go against what stands there now.
///
/// A raid is a battle (`BattleRound`, one a tick): the raiders against every
/// ship of the owner standing there and, at home, every defence and Solar
/// Satellite, or at the outpost its platform. It is fought for at most
/// `RAID_ROUNDS` rounds. It is repelled when no raider is left or when the
/// rounds are over and a defender still stands; it is lost only when no
/// defender is left.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RaidForecast {
    /// The tick an announced raid arrives. Absent while none is on its way
    /// and while one is being fought: with neither, `power` and `ships` are
    /// those of a raid of typical size were one announced now.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub arrival_tick: Option<u64>,
    /// The raid has arrived and is being fought: the rounds still to fight
    /// at the most, as `BattleState.raid` of the fight says them. `power`
    /// and `ships` are then what it flew in with, and `odds` how the fight
    /// would end from where it stands. Absent otherwise.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub rounds_left: Option<u32>,
    /// The raid's power on the scale `FleetState.power` uses. It is fixed
    /// when the raid is announced.
    pub power: f32,
    /// What it flies, by class in catalogue order. Fixed when announced.
    pub ships: Vec<ShipCount>,
    /// How it would end against what stands in its way this tick.
    pub odds: RaidOdds,
}

/// How a raid would end against what stands in its way now: the server
/// plays the fight itself, the round a raid is fought by, a few times over
/// with fixed dice for `RAID_ROUNDS` rounds, as `SectorOdds` are worked out.
/// `destroyed`, `outlasted` and `lost` are the shares of the fights played
/// that ended each way and add up to 1.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RaidOdds {
    pub label: RaidLabel,
    /// No raider was left: the raid is repelled and leaves wreckage.
    pub destroyed: f32,
    /// The rounds were over and a defender still stood: the raid is
    /// repelled and leaves nothing.
    pub outlasted: f32,
    /// No defender was left: the raid takes its share of the stockpile.
    pub lost: f32,
    /// The share of their worth the defenders are expected to keep, before
    /// any defence or Solar Satellite is rebuilt, and the share of theirs
    /// the raiders keep.
    pub kept: f32,
    pub raiders_kept: f32,
}

/// What `RaidOdds` come to, from the worst for the defender to the best.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RaidLabel {
    /// Lost in every fight played, or nothing stands in its way.
    Lost,
    /// Lost in some of the fights played.
    AtRisk,
    /// Lost in none, and not destroyed in every one: the defence lasts the
    /// rounds out.
    Held,
    /// The raiders were destroyed in every fight played.
    Repelled,
}

impl RaidLabel {
    pub fn label(self) -> &'static str {
        match self {
            RaidLabel::Lost => "LOST",
            RaidLabel::AtRisk => "AT RISK",
            RaidLabel::Held => "HELD",
            RaidLabel::Repelled => "REPELLED",
        }
    }

    /// The sentence a client prints beside the label.
    pub fn outcome(self) -> &'static str {
        match self {
            RaidLabel::Lost => "nothing here stands through the fight: the raid takes its share",
            RaidLabel::AtRisk => "the fight may leave no defender standing",
            RaidLabel::Held => "a defender stands when the raiders give up; what falls before that is lost",
            RaidLabel::Repelled => "the raiders are destroyed and leave their wreckage; what of yours falls before that is lost",
        }
    }
}

/// What a survey reads of a sector the player has seen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurveySite {
    /// The ore the sector was made with, which is what the extractor reads:
    /// harvesting the tiles does not lower it.
    pub densities: SectorResources,
    /// 0 to 12: Sparse 0.5, Moderate 1, Rich 2, Pristine 4, summed over the
    /// three resources.
    pub grade: f32,
    /// What the extractor would add to the store each tick at level 1.
    pub production: Resources,
    /// Lane hops from the sector to the homeworld; absent beyond `LANE_SEARCH_HOPS`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub hops_home: Option<u32>,
}

/// The bounds of a roll of combat power, on the scale `FleetState.power`
/// and threat ratings share: no roll is below `least` or above `most`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PowerRange {
    pub least: f32,
    pub most: f32,
}

/// One trade at the hub's exchange, quoted or made: `amount` of `give` for
/// `receive` of `want`.
///
/// The exchange trades resource for resource and always keeps a fee, so a
/// trade never raises what the player owns. Par values are the score
/// weights: metal 1, crystal 1.5, deuterium 2 a unit. Each player has a
/// meter that counts the weight they sold (`ExchangeMeter`). The fee on a
/// unit of weight is 25 percent of par while the meter is empty and rises in
/// a straight line to 75 percent when it reads `span`, and stays there
/// beyond. A trade pays the fee of every unit it moves the meter through,
/// so one large order and the same order in pieces return the same;
/// `receive` is rounded down to whole units. The meter falls by one `span`
/// every `window_ticks`, whether or not the player is connected.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExchangeTerms {
    pub fleet_id: u64,
    pub give: ResourceKind,
    pub want: ResourceKind,
    /// Units of `give` handed over.
    pub amount: f64,
    /// Units of `want` that come back: whole units, rounded down.
    pub receive: f64,
    /// Units of `want` the same `amount` is worth at par, with no fee.
    pub par: f64,
    /// The share of par the exchange keeps on this trade, `1 - receive / par`.
    pub fee: f64,
    /// The player's meter before the trade.
    pub meter: ExchangeMeter,
    /// And after it.
    pub meter_after: ExchangeMeter,
}

/// A player's standing at the exchange.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ExchangeMeter {
    /// Weight sold and not yet forgotten.
    pub pressure: f64,
    /// The pressure at which the fee stops rising: 15 percent of what the
    /// player's own mines make in one window, in weight units. It grows
    /// with the mines; 0 for an empire with no mine output, which pays the
    /// top fee.
    pub span: f64,
    /// The fee on the next unit of weight sold, 0.25 to 0.75.
    pub fee: f64,
    /// The meter forgets one `span` every this many ticks.
    pub window_ticks: u64,
    /// Ticks until the meter is empty again; 0 when it is, absent when it
    /// does not fall (`span` is 0).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub clear_in: Option<u64>,
}

/// Answer to `exchange_quote`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExchangeQuote {
    #[serde(flatten)]
    pub terms: ExchangeTerms,
    /// Why `exchange` with the same fields would be refused right now;
    /// absent when it would go through.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub refusal: Option<String>,
}

/// The score table: one ranking for every player, human or agent. A score
/// is what an empire has built and keeps:
/// `score = buildings + research + fleet + outpost + defence`, each part
/// the cost of what stands, weighted 1 : 1.5 : 2 for
/// metal : crystal : deuterium, per thousand. `fleet` is the ships the
/// empire has now, its Solar Satellites among them, so a fleet destroyed
/// leaves the score; `defence` is the defence structures at the share of
/// their cost the score gives them (`iac_sim::score::DEFENCE_SHARE`, none
/// today). Kills, charts and derelicts score nothing: they pay in resources.
///
/// Built-in bots are part of the world but not of the ranking: they are
/// never in `entries` and take no rank there. They are listed in `bots`,
/// ranked among themselves. A bot's account name always starts with
/// `BOT_NAME_PREFIX`, which no other account can register, so every place
/// that names a player (fleets in a sector, combat events) tags a bot by
/// its name alone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LeaderboardReply {
    pub tick: u64,
    pub entries: Vec<LeaderboardEntry>,
    /// The asking player's own row, when it falls outside `entries`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub you: Option<LeaderboardEntry>,
    /// The world's built-in bots, best first; absent when it has none.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub bots: Vec<LeaderboardEntry>,
}

/// Start of every built-in bot's account name, reserved for them
/// (compared without regard to case).
pub const BOT_NAME_PREFIX: &str = "BOT-";

/// The fewest characters an account name may have (`iac_sim::auth` holds
/// the other rules of a name).
pub const NAME_MIN_LEN: usize = 3;
/// The most characters an account name may have.
pub const NAME_MAX_LEN: usize = 24;

/// Whether `name` is a built-in bot's.
pub fn is_bot_name(name: &str) -> bool {
    name.get(..BOT_NAME_PREFIX.len()).is_some_and(|start| start.eq_ignore_ascii_case(BOT_NAME_PREFIX))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LeaderboardEntry {
    pub rank: u32,
    pub name: String,
    pub agent: bool,
    pub score: f32,
    pub buildings: f32,
    pub research: f32,
    pub fleet: f32,
    /// The finished levels of the empire's outpost, while it stands.
    pub outpost: f32,
    pub defence: f32,
}

/// Answer to `preview_move`: what a jump costs and what waits at the far end.
///
/// `threat` is what the player knows of the target sector (observed if it is
/// live, the remembered rating if charted, else the distance estimate).
/// `ratio` and `label` are the odds of the fleet named by `fleet_id` (not the
/// strongest one) against it, as in `SectorOdds`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MovePreview {
    pub fleet_id: u64,
    pub target: Hex,
    /// Fuel the jump burns.
    pub fuel_cost: f64,
    /// Fuel left afterwards (0 when the fleet cannot afford the jump).
    pub fuel_after: f64,
    /// The fleet holds enough fuel for the jump.
    pub can_jump: bool,
    /// Hops from `target` back to the homeworld by the shortest real route,
    /// charted or not: what the fleet can actually fly.
    pub hops_home: u32,
    /// Hops of the shortest route over sectors the player has entered; None
    /// when no such route exists. Never shorter than `hops_home`.
    #[serde(default)]
    pub charted_hops_home: Option<u32>,
    /// The `hops_home` route crosses a sector the player has never entered.
    #[serde(default)]
    pub route_unexplored: bool,
    /// Fuel the `hops_home` route costs.
    pub fuel_to_return: f64,
    /// After the jump the fleet still holds enough fuel to get home.
    pub can_return: bool,
    pub threat: ThreatInfo,
    /// Combat power of the whole fleet, on the scale of `threat.est_power`.
    /// The two do not divide into `ratio`: that weighs the ship classes.
    pub fleet_power: f32,
    pub ratio: f32,
    pub label: RatioLabel,
    /// The fleet is in a fight, so this jump is flight: what it costs, and
    /// whether a group would follow it to `target`. Absent outside a fight.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub flight: Option<FlightQuote>,
}

/// Most sectors one `odds` request may name.
pub const MAX_ODDS_SECTORS: usize = 256;

/// Answer to `odds`, in the order asked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FleetOdds {
    pub fleet_id: u64,
    pub sectors: Vec<SectorOdds>,
}

/// How a fleet would fare against what the player knows of a sector.
///
/// The server plays the fight itself, the round a fight is fought by, a few
/// times over with fixed dice and the two sides as they stand, damage and
/// refit included, both scaled down together when they are large. `ratio`
/// puts the share of its worth each side kept on the scale of a power ratio
/// between equal ship classes (1.0 is an even match, 0.1 to 10), and `label`
/// is its band (`RatioLabel`): safe from 2.5, favourable from 1.5, even
/// from 1.1, risky from 0.9, below that deadly. A sector with nothing in
/// sight is rated against the group its ring spawns. Standing orders use
/// the same number (`engage_ratio_x10`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SectorOdds {
    pub sector: Hex,
    pub threat: ThreatInfo,
    pub ratio: f32,
    pub label: RatioLabel,
}

/// How dangerous a sector is: T1 to T9 from the power of what is there.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ThreatInfo {
    /// 1 to 9, or 0 where there is nothing to fight and nothing will come:
    /// a sector an outpost stands in.
    pub rating: u8,
    pub est_power: f32,
    pub basis: ThreatBasis,
}

/// Where a `ThreatInfo` comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreatBasis {
    /// A hostile group was seen; the power is the group's own (Aggressive and
    /// Swarm groups count 25 percent more). Or an outpost was seen, which
    /// keeps every group out: rating 0, power 0.
    Observed,
    /// The sector's seed holds a group that is away or cleared; the power is
    /// the ring's.
    Template,
    /// Distance only: the ring's group power.
    Estimate,
}

/// Reply to `AuthRequest`. On success `token` is set only when the server just
/// issued one (new account or claimed legacy account): the client must store
/// it. On failure `code` says why (`InvalidName`, `TokenRequired`,
/// `InvalidToken`, or `WorldFull` when a new name finds every home seat
/// taken) and `message` is readable text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub player_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub code: Option<ErrorCode>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub message: Option<String>,
}

/// Per-tick update. `player`, `homeworld_update`, `sector_updates` and
/// `events` are optional and omitted when there is nothing to say.
/// `sector_updates` holds every live sector (current this tick) plus, once,
/// each sector that stopped being live this tick (`live: false`, so the
/// client knows to dim it). Stale sectors are not repeated after that. `fleets`
/// is not optional: it is the complete, authoritative list of the player's
/// fleets, sent whole every tick (like `GameState.fleets`), so a client
/// replaces its list wholesale. A fleet missing from the list no longer
/// exists, and an empty list means every fleet is lost.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickUpdate {
    pub tick: u64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub player: Option<PlayerState>,
    pub fleets: Vec<FleetState>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub sector_updates: Option<Vec<SectorState>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub homeworld_update: Option<HomeworldState>,
    /// The player's outpost, whole, every tick they have one. Absent means
    /// they have none: a client drops the one it held.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub outpost: Option<OutpostState>,
    /// The player's relays and their limit, whole, every tick.
    #[serde(default)]
    pub relays: RelaysState,
    /// Every fight under way that the player has a fleet in or is present
    /// at, whole, every tick, as in `GameState.battles` but without
    /// `last_round`: the round fought this tick is the `BattleRound` event
    /// of the same update. Left out when there is none: a client replaces
    /// its list with this one.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub battles: Vec<BattleState>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub events: Option<Vec<GameEvent>>,
}

/// Full snapshot, sent on login and on `request_full_state`.
///
/// `known_sectors` is the player's whole chart: every sector the player has
/// ever had eyes on (fleet present, the homeworld's sensors, active scan), not only the
/// ones with fresh intel. Entries with `live: true` are current; the rest
/// carry what was last observed and the tick it was observed (`last_seen`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameState {
    pub tick: u64,
    pub player: PlayerState,
    pub fleets: Vec<FleetState>,
    pub homeworld: HomeworldState,
    /// The player's outpost; absent when they have none.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub outpost: Option<OutpostState>,
    #[serde(default)]
    pub relays: RelaysState,
    pub known_sectors: Vec<SectorState>,
    /// The fights under way that the player has a fleet in or is present
    /// at, each with its last round: what a client that arrives in the
    /// middle of one has missed. The rounds before that are not kept.
    /// A raid being fought at the player's homeworld or outpost is one of
    /// them, with `raid` set. Left out when there is none.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub battles: Vec<BattleState>,
    pub world: WorldInfo,
}

/// A fight under way, as `GameState.battles` and `TickUpdate.battles` carry it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BattleState {
    pub sector: Hex,
    /// Rounds fought so far.
    pub rounds: u32,
    /// The last of them, in a full state; absent before the first is
    /// fought, and in a tick update, whose events carry the round.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub last_round: Option<BattleRoundEvent>,
    /// What each fleet and group has lost so far, as `CombatEnded` will say.
    pub losses: Vec<BattleLosses>,
    /// The fight is a raid on a homeworld or an outpost; absent for a fight
    /// in open space.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub raid: Option<RaidBattle>,
}

/// A raid that has arrived and is being fought.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RaidBattle {
    /// The empire raided.
    pub owner: String,
    /// It is the receiver's own homeworld or outpost.
    #[serde(default)]
    pub mine: bool,
    /// The raid is on the owner's outpost, not the homeworld.
    pub outpost: bool,
    /// The power it was announced with.
    pub power: f32,
    /// What it flew in with, by class.
    pub ships: Vec<ShipCount>,
    /// Rounds still to fight at the most: the raiders leave after the last.
    pub rounds_left: u32,
    /// How the fight would end from where it stands, over the rounds left.
    pub odds: RaidOdds,
}

/// A player's outpost: a station on one sector of rings 9 to 20, built from
/// what fleets carry to it. One level (1 to `max_level`) sets its store, its
/// platform and how many site rates its extractor makes.
///
/// It is a dock. `load` and `unload` there move cargo between a hold and
/// `store`, never the stockpile, and nothing moves by itself. A fleet
/// standing there without standing orders is refuelled from the store's
/// deuterium while the outpost is `in_service`; any fleet of the owner
/// arriving at a finished outpost hands over its charts; a fleet standing
/// at a finished outpost does not count against the deployed fleets.
/// Nothing in the store scores or can be spent at home.
///
/// Pirates of its ring raid it on the homeworld's raid clock
/// (`RaidIncoming` and `RaidResolved` carry `outpost`), and the raid is a
/// battle as a raid on the homeworld is (`RaidForecast`): the raiders
/// against the `platform` and every ship of the owner standing there. A
/// lost raid, one that left no defender, takes a quarter of what lies above
/// `protected` in the store and breaches the outpost. No raid destroys it,
/// the platform stands whole again after every fight it is not breached
/// by, and a fleet standing there is in the fight.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutpostState {
    pub location: Hex,
    /// 0 while the first level is being built.
    pub level: u8,
    pub max_level: u8,
    /// Level 1 or more and not breached: the extractor runs, tanks are
    /// filled and the platform defends.
    pub in_service: bool,
    /// A lost raid broke it: no extractor, no refuelling, no platform until
    /// the repair is done. `store`, `load`, `unload` and chart banking work.
    pub breached: bool,
    /// The one job under way: a level being built, or the repair.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub work: Option<OutpostWorkState>,
    pub store: Resources,
    /// The most the store holds of each resource. The extractor stops
    /// there, and `unload` leaves aboard what does not fit.
    pub store_cap: Resources,
    /// What a raid cannot take: what the repair of the level it stands at,
    /// or of the level it is building, would take from the store as it
    /// stands. A repair is priced in metal; where the store lacks metal,
    /// crystal pays for it and then deuterium, each unit at its par value
    /// (1.5 and 2).
    pub protected: Resources,
    /// What the extractor adds to the store each tick now; zero while not
    /// `in_service`.
    pub production: Resources,
    /// The resources the extractor makes and the store holds all it takes
    /// of: that yield is lost until a fleet loads some.
    #[serde(default)]
    pub yield_lost: Vec<ResourceKind>,
    /// The site's grade, 0 to 12; see `SurveySite.grade`.
    pub grade: f32,
    /// What the platform adds up to on the power scale; 0 while breached or
    /// unfinished.
    pub platform_power: f32,
    /// The defences the platform is; empty while breached or unfinished.
    #[serde(default)]
    pub platform: Vec<UnitCount>,
    /// What the owner's ships standing in the sector add up to.
    pub ships_power: f32,
    /// `platform_power` and `ships_power` together. It is a sum on the
    /// power scale and decides nothing: `raid.odds` is how a raid would go.
    pub defence_power: f32,
    /// The least and the most a raid on this outpost can be.
    pub raid_power: PowerRange,
    /// Tick an announced raid arrives; absent when none is on its way.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub raid_arrival_tick: Option<u64>,
    /// The raid being fought, else the one announced, else one of typical
    /// size for the ring, against the platform and the ships standing there
    /// now. Absent before level 1 is finished: nothing raids a building
    /// site.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub raid: Option<RaidForecast>,
    /// What `upgrade_outpost` would build; absent at `max_level`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub next_level: Option<OutpostLevel>,
    /// Why `upgrade_outpost` would be refused right now, in the sentence
    /// the refusal would carry; absent when it would be accepted this tick.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub upgrade_refusal: Option<String>,
    /// What the repair will take from the store, paid as `protected` says;
    /// what the whole store does not cover is counted as metal. Present
    /// while breached with no repair under way.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub repair_cost: Option<Resources>,
    /// What the store lacks of `repair_cost`, in metal: the repair waits
    /// for a fleet to unload it, or crystal or deuterium to the same par
    /// value. Absent when no repair is waiting.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub repair_waiting_for: Option<Resources>,
    /// Lane hops to the homeworld; absent beyond `LANE_SEARCH_HOPS`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub hops_home: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutpostWork {
    Build,
    Repair,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutpostWorkState {
    pub kind: OutpostWork,
    /// The level being built, or the level being repaired.
    pub level: u8,
    pub start_tick: u64,
    pub end_tick: u64,
}

/// One level of an outpost, as an upgrade to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutpostLevel {
    pub level: u8,
    pub cost: Resources,
    pub ticks: u64,
    pub store_cap: Resources,
    pub platform_power: f32,
    /// The defences the platform is at this level.
    #[serde(default)]
    pub platform: Vec<UnitCount>,
    /// What the extractor makes each tick at this level, on this site.
    pub production: Resources,
}

/// An outpost standing in a sector, as anyone with eyes on the sector sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutpostBrief {
    /// The empire it belongs to.
    pub owner: String,
    /// 0 while its first level is being built.
    pub level: u8,
    pub breached: bool,
    /// It is the receiver's own.
    #[serde(default)]
    pub mine: bool,
}

/// The world's fixed settings, sent with every `full_state`. `pace` scales
/// economy timers (see `docs/design/economy/spec.md` section 1) and never
/// changes for the life of a world.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldInfo {
    pub pace: f64,
    /// Name of the preset the pace equals, if it equals one.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub preset: Option<String>,
    pub tick_hz: u32,
    pub economy_version: u32,
    pub worldgen_version: u32,
    /// Simulation estimate for a competent player; present for presets.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub estimate: Option<WorldEstimate>,
    /// The neutral post at the centre of the map: every hub sector and what
    /// it offers. A hub sector has no ore, no hostile group and no derelict.
    #[serde(default)]
    pub hub: Vec<HubSite>,
    /// Every rapid-fire figure of the catalogue: which ship class fires
    /// again at what, ships and defences alike. It never changes.
    #[serde(default)]
    pub rapid_fire: Vec<RapidFire>,
}

/// After a shot at a unit of `against`, a ship of `ship_class` fires again
/// with probability `(shots - 1) / shots`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RapidFire {
    #[serde(rename = "class")]
    pub ship_class: ShipClass,
    /// A ship class or a defence: the wire carries just its name.
    pub against: ShipyardItem,
    pub shots: u16,
}

/// One sector of the hub.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HubSite {
    pub location: Hex,
    pub services: Vec<HubService>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldEstimate {
    pub first_cruiser_s: u64,
    pub endgame_s: u64,
}

impl WorldInfo {
    pub fn new(pace: Pace, economy_version: u32, worldgen_version: u32) -> Self {
        let preset = pace.preset();
        WorldInfo {
            pace: pace.value(),
            preset: preset.map(|p| p.name.to_string()),
            tick_hz: crate::constants::TICK_RATE_HZ as u32,
            economy_version,
            worldgen_version,
            estimate: preset.map(|p| WorldEstimate {
                first_cruiser_s: p.first_cruiser_s,
                endgame_s: p.endgame_s,
            }),
            hub: HUB
                .iter()
                .map(|(location, services)| HubSite { location: *location, services: services.to_vec() })
                .collect(),
            rapid_fire: ShipClass::ALL
                .into_iter()
                .flat_map(|ship_class| {
                    scaling::rapid_fire(ship_class).map(move |(against, shots)| RapidFire { ship_class, against, shots })
                })
                .collect(),
        }
    }
}

// ── State Types ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerState {
    pub id: u64,
    pub name: String,
    pub resources: Resources,
    pub homeworld: Hex,
    /// Every ship of every fleet the player has, docked or away.
    pub ship_count: u32,
    /// Where the player's meter at the hub's exchange stands this tick.
    #[serde(default)]
    pub exchange: ExchangeMeter,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetState {
    pub id: u64,
    pub location: Hex,
    pub state: FleetStatus,
    /// Its ships by class, in catalogue order; a class it has none of is
    /// left out. A fleet is a count for each class and the hull of its
    /// damaged ships: no ship has an id.
    pub ships: Vec<ShipGroupState>,
    /// Every ship of every class.
    pub ship_count: u32,
    /// Hull the whole fleet holds, and at full. Hull lost stays lost until a
    /// dock mends it (`repair`).
    pub hull: u64,
    pub hull_max: u64,
    /// What the fleet fires in one round, and the shield it raises at the
    /// start of each, with `refit` counted.
    pub attack: u64,
    pub shield: u64,
    /// The Weapons, Shielding and Armour Technology the fleet fights with:
    /// the levels it last docked at home with. Research that finishes while
    /// it is away reaches it when it next docks there, not at the outpost.
    pub refit: Refit,
    /// The owner's levels are above `refit`: docking at home would raise it.
    #[serde(default)]
    pub refit_due: bool,
    /// What mending its damaged ships would take; absent when none is damaged.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub repair: Option<RepairQuote>,
    /// The fleet is in a fight and firing: what a `move` out of it would
    /// cost right now, and why it would be refused if it would. Absent
    /// outside a fight and once the fleet is leaving one.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub flight: Option<FlightQuote>,
    /// The fleet is charging a jump while hostiles fire on it: it does not
    /// answer, and stays a target until it is gone. Its `state` is `Moving`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub fleeing: Option<Fleeing>,
    /// A hostile group is following the fleet and jumping to where it stands.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub pursued_by: Option<Pursuit>,
    pub cargo: Resources,
    /// Hold size: the most `cargo` can total, as the server enforces it.
    pub cargo_capacity: f64,
    pub fuel: f64,
    pub fuel_max: f64,
    /// Fuel one jump costs this fleet right now: the fuel figure of each of
    /// its ships, less Energy Technology's saving. A fleet with less than this and away
    /// from home is stranded and only regains fuel from the emergency
    /// reserve, one jump per five minutes.
    #[serde(default)]
    pub jump_fuel: f64,
    /// Fuel the way home costs: the fewest hops over the real lanes, charted
    /// or not, times `jump_fuel`. That is the way a fleet flies one jump at
    /// a time, and what standing orders budget for. 0 at the homeworld.
    /// Compare with `fuel`. An emergency recall does not fly this way: it
    /// jumps straight home and burns twice `jump_fuel` per hex of straight
    /// distance.
    #[serde(default)]
    pub home_fuel: f64,
    /// Fewest hops home over the real lanes, charted or not. `home_fuel` is
    /// the fuel for these. 0 at the homeworld. Counted from where the fleet
    /// is bound: its destination while a jump is in progress, otherwise
    /// where it stands. An emergency recall does not fly these hops.
    pub hops_home: u32,
    /// Ticks one jump takes this fleet from the order to the arrival, set
    /// by its slowest ship and that ship's drive. A route of `n` jumps flown
    /// without a pause takes `n * jump_ticks`.
    pub jump_ticks: u16,
    /// Hops a `scan` by this fleet reveals: `SCAN_PROBE_RANGE` with an
    /// Espionage Probe aboard, `SCAN_BASE_RANGE` without.
    pub scan_range: u8,
    /// Ticks before the fleet takes its next order. While it boards a
    /// derelict (`Exploring`) it is what the boarding has left, out of
    /// `EXPLORE_DURATION_TICKS`.
    #[serde(default)]
    pub cooldown_remaining: u16,
    /// Combat power of the whole fleet on the scale raids and threat ratings
    /// share: each ship counts its attack and a tenth of its shield and of
    /// the hull it holds, with `refit` counted.
    #[serde(default)]
    pub power: f32,
    /// How many more hops outward the fleet can fly and still get home:
    /// `(fuel - home_fuel) / (2 * jump_fuel)`, rounded down, 0 when it
    /// cannot. At the homeworld it is the round-trip reach.
    #[serde(default)]
    pub range_hops: u32,
    /// At the homeworld with cargo the stockpile has no room for; it stays
    /// aboard and unloads as space frees up.
    #[serde(default)]
    pub cargo_blocked: bool,
    /// Loaded by `load` and still at home: the dock leaves this hold alone.
    /// It ends with `unload`, with standing orders, or with the fleet's
    /// next jump, so what the fleet brings home unloads as any cargo does.
    #[serde(default)]
    pub laden: bool,
    /// The fleet is in the sector of its owner's outpost, whatever state
    /// the outpost is in: `load` and `unload` use the outpost's store
    /// here, and a raid on it finds the fleet in its fight.
    #[serde(default)]
    pub at_outpost: bool,
    /// The fleet is in the sector of one of its owner's relays: `load` and
    /// `unload` use the relay's deuterium store here, and a fleet flown by
    /// hand has its tank filled from it.
    #[serde(default)]
    pub at_relay: bool,
    /// Fuel the way to the owner's outpost costs: the fewest lane hops
    /// there times `jump_fuel`, 0 at the outpost. Present while that
    /// outpost is in service, which is when it fills a tank.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub outpost_fuel: Option<f64>,
    /// What a `refine` order would do right now. Absent when it would be
    /// refused.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub refine: Option<Refining>,
    /// Standing orders currently flying this fleet, if any: the doctrine
    /// with its settings, so the two cannot be told apart or disagree.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub policy: Option<StandingOrders>,
}

/// What leaving a fight costs a fleet, as things stand this tick. The price
/// of flight is the rounds it takes without firing back, the damage, which
/// stays until a dock mends it, the fuel, nothing earned from a group that
/// was not destroyed, and pursuit if the fleet is the slower.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlightQuote {
    /// Fuel the jump out burns: `jump_fuel` times `FLEE_FUEL_MULTIPLIER`.
    pub fuel: f64,
    /// Ticks from the order until the fleet is gone.
    pub ticks: u16,
    /// Rounds of fire it takes in that time without answering.
    pub rounds: u16,
    /// The odds of the fight as it stands now, both sides with the losses
    /// and the damage they have taken: what `SectorOdds` would say of it,
    /// and what a standing order's `disengage_ratio_x10` is held against.
    pub ratio: f32,
    pub label: RatioLabel,
    /// Why a `move` out would be refused now, in the engine's words; absent
    /// when it would be taken.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub refusal: Option<String>,
    /// The group that would follow the fleet if it left now and the others
    /// with it; absent when none would. In a `MovePreview` it is for the
    /// jump to `target`: no group follows into home ground, the hub or a
    /// sector an outpost stands in.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub chase: Option<ChaseForecast>,
}

/// How a group would follow a fleet that left it now.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChaseForecast {
    pub group_id: u64,
    /// What the group is ("3x cruiser pack").
    pub label: String,
    /// Jumps it would make after the fleet before it turns back.
    pub hexes: u8,
    /// Ticks one jump takes it.
    pub jump_ticks: u16,
    /// Rounds it would fire on the fleet at each of those hexes, if the
    /// fleet orders its next jump the moment it lands: the difference of the
    /// two jump times, 0 for a fleet as fast as the group. A fleet that
    /// stops is caught and the fight is joined again.
    pub rounds_per_hex: u16,
}

/// A jump out of a fight under way.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Fleeing {
    pub to: Hex,
    /// Ticks until the fleet is gone.
    pub ticks: u16,
    /// Rounds of fire it still takes.
    pub rounds: u16,
}

/// A group on a fleet's trail, as the fleet's owner is told of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pursuit {
    pub group_id: u64,
    /// What the group is ("3x cruiser pack").
    pub label: String,
    /// The sector it is jumping from.
    pub from: Hex,
    /// Ticks until it lands where the fleet stands.
    pub arrives_in: u16,
    /// It lands before the fleet can be gone: before the jump the fleet is
    /// charging lands or, for a fleet that is not charging one, before a
    /// jump ordered now would. It then fires on the fleet.
    pub catches: bool,
    /// Jumps it will still make after this one before it turns back.
    pub hexes_left: u8,
}

/// One `refine` order: the deuterium that leaves the hold and the fuel the
/// tank gains for it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Refining {
    pub deuterium: f64,
    pub fuel: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FleetStatus {
    Idle,
    Moving,
    Harvesting,
    InCombat,
    Returning,
    Docked,
    Exploring,
}

/// The ships of one class in a fleet or a hostile group. Shields are not a
/// state: every unit's is full at the start of every round.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ShipGroupState {
    #[serde(rename = "class")]
    pub ship_class: ShipClass,
    pub count: u32,
    /// How many are not whole.
    pub damaged: u32,
    /// How many of those stand at or under `EXPLODE_BELOW_PCT` hundredths of
    /// their hull: each may explode the next time a shot breaks its shield.
    pub at_risk: u32,
    /// Hull they hold between them, and at full.
    pub hull: u64,
    pub hull_max: u64,
    /// One ship's attack, shield and full hull in a round, with what its
    /// fleet or group fights with counted.
    pub unit_attack: u32,
    pub unit_shield: u32,
    pub unit_hull: u32,
}

/// What mending a fleet's damaged ships takes at each dock.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepairQuote {
    /// Damaged ships, and the hull they lack between them.
    pub ships: u32,
    pub hull: u64,
    /// The home dock: it mends a fleet the tick it docks, and every tick it
    /// stands there.
    pub home: DockRepair,
    /// The owner's outpost; absent when they have none.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub outpost: Option<DockRepair>,
}

/// One dock's answer to a fleet's damage, as things stand this tick.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DockRepair {
    /// The whole repair priced in metal: for each ship the dock's share of
    /// its cost at par, times the share of its hull that is gone. 0 at a
    /// dock that mends for nothing.
    pub price: f64,
    /// What the dock's stock would pay now: metal first, crystal and
    /// deuterium at their weights where the metal runs out.
    pub payment: Resources,
    /// Ships that payment makes whole, the most damaged first, and the hull
    /// it restores in all. A ship the stock does not cover is left as it
    /// is, and so is every ship after it.
    pub ships: u32,
    pub hull: u64,
    /// Why this dock would mend nothing, in the engine's words; absent when
    /// it would mend.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub refusal: Option<String>,
}

/// One sector as the player knows it.
///
/// `live: true` means the server is watching the sector this tick (a fleet
/// is there, the homeworld's sensors cover it, or a scan has not expired) and every
/// field is current. `live: false` is chart memory: `resources`, `hostiles`,
/// `salvage` and `site` are what was last observed at tick `last_seen` and
/// may have changed since (a wreck despawns, a hostile moves, a derelict is
/// looted). `terrain` and `connections` never change. `player_fleets` is
/// only ever sent for live sectors; other empires' positions are not
/// remembered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectorState {
    pub location: Hex,
    pub terrain: TerrainType,
    pub resources: SectorResources,
    pub connections: Vec<Hex>,
    /// Hostile fleets. `id` is the fleet id to pass to `attack`: it is stable
    /// whether or not the fleet has materialised yet, and it is the same id
    /// that later appears as `enemy_fleet_id` in `CombatStarted`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub hostiles: Option<Vec<NpcFleetInfo>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub player_fleets: Option<Vec<FleetBrief>>,
    /// Collectible wreckage pile (what `collect_salvage` would take).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub salvage: Option<Resources>,
    /// Tick the pile despawns if nobody collects it; seconds left are
    /// `salvage_despawn_tick - tick`. A remembered pile whose tick has passed
    /// is removed from the chart.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub salvage_despawn_tick: Option<u64>,
    /// A boardable derelict hulk drifting in this sector.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub site: Option<SiteBrief>,
    /// The `salvage` or `site` pin is chart memory from `last_seen` that
    /// nothing has confirmed since; it may be gone. Absent (false) on live
    /// sectors and on sectors with no pin. A pin the player saw removed (a
    /// boarding, a collected or despawned pile, a scan that found nothing) is
    /// dropped instead.
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub pins_stale: bool,
    /// What the ore tiles hold, in raw units; absent when the sector has none.
    /// When not `live` it is as of `last_seen`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ore_reserve: Option<OreReserve>,
    /// Where the player's own chart of this sector stands; absent when no
    /// fleet of theirs holds or has delivered one. Always current, also on
    /// a sector that is not `live`: a sector whose mark changes is sent
    /// once more in `sector_updates`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub chart: Option<ChartMark>,
    /// The outpost standing here, whoever owns it. While it stands no group
    /// lives in the sector or wanders in. When not `live` it is the outpost
    /// last seen, which may have grown, been breached or been abandoned.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub outpost: Option<OutpostBrief>,
    /// How dangerous the sector is. When not `live` it is the rating last
    /// seen, as old as `last_seen`.
    pub threat: ThreatInfo,
    /// Tick this sector was last observed; the current tick when `live`.
    pub last_seen: u64,
    /// See the struct docs.
    pub live: bool,
}

/// A sector's chart is made by entering it and pays once, in resources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChartMark {
    /// A fleet that entered the sector still carries the chart: it is paid
    /// when that fleet docks at the homeworld or arrives at its owner's
    /// finished outpost, and lost if the fleet is.
    Aboard,
    /// Delivered and paid; entering again earns nothing.
    Delivered,
}

/// What a fleet on-site can tell about a derelict before boarding.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SiteBrief {
    pub tier: u8,
    /// Rough boarding risk read: "quiet" / "uneasy" / "hot".
    pub risk: SiteRisk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SiteRisk {
    Quiet,
    Uneasy,
    Hot,
}

impl SiteRisk {
    pub fn label(self) -> &'static str {
        match self {
            SiteRisk::Quiet => "quiet",
            SiteRisk::Uneasy => "uneasy",
            SiteRisk::Hot => "hot",
        }
    }
}

/// The ore of a sector tile by tile. A tile is a small nugget: harvesting
/// takes `units` down toward 0, and it refills on a timer while no fleet sits on it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct OreReserve {
    pub metal: TileReserve,
    pub crystal: TileReserve,
    pub deuterium: TileReserve,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TileReserve {
    /// Raw units left to harvest.
    pub units: f64,
    /// Units the tile holds when full (0 for a sector without this ore).
    pub max_units: f64,
    /// Seconds until the tile is full again, if it is not.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub refills_in_s: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SectorResources {
    pub metal: Density,
    pub crystal: Density,
    pub deuterium: Density,
}

/// A hostile group. `ships` is by class as a fleet's is. A group keeps the
/// losses and the damage of its fights: it is listed as it stands, and
/// mends only at rest in its own sector.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NpcFleetInfo {
    pub id: u64,
    pub ships: Vec<ShipGroupState>,
    /// Every ship of every class.
    pub ship_count: u32,
    pub behavior: NpcBehavior,
    /// Ships it has when whole: `muster - ship_count` are lost.
    pub muster: u32,
    /// The tick it will be whole again, ships and hull, if nothing disturbs
    /// it; absent for a group that is whole. It mends in steps and only at
    /// rest in its own sector: a fight or a pursuit stops the clock and the
    /// tick moves. On a sector that is not `live` it is what was true at
    /// `last_seen`; the odds count such a group whole once the tick is past.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub whole_at: Option<u64>,
    /// Ticks one jump takes it.
    pub jump_ticks: u16,
    /// The most jumps it makes after a fleet that leaves a fight with it:
    /// 0 for a group that never follows.
    pub chase_hexes: u8,
    /// It is following a fleet or on its way back to its own sector.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub errand: Option<GroupErrand>,
}

/// What takes a group out of its own sector. Its sector is empty meanwhile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupErrand {
    /// The sector it lairs in and goes back to.
    pub home: Hex,
    /// The fleet it follows and the empire that owns it; absent when it has
    /// turned back.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub quarry: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub quarry_owner: Option<String>,
    /// The sector it is jumping to and the tick it lands there; absent while
    /// it fights.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub to: Option<Hex>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub arrives_at: Option<u64>,
    /// Jumps it will still make after a fleet before it turns back.
    pub hexes_left: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NpcBehavior {
    Passive,
    Patrol,
    Aggressive,
    Swarm,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FleetBrief {
    pub id: u64,
    pub owner_name: String,
    pub ship_count: u32,
    /// Its ships by class, in catalogue order; a class it has none of is left out.
    #[serde(default)]
    pub ship_classes: Vec<ShipCount>,
}

/// So many ships of one class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShipCount {
    #[serde(rename = "class")]
    pub ship_class: ShipClass,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomeworldState {
    pub location: Hex,
    /// What the stockpile gains each tick at current levels and the energy
    /// there is (before the storage cap discards the excess; see
    /// `storage`): the mines at `energy.fraction`, the natural production,
    /// less the deuterium the Fusion Reactor burns, so deuterium may be
    /// negative.
    pub production: Resources,
    pub energy: EnergyState,
    /// Solar Satellites at home. They are built in the shipyard queue as
    /// the ship class `SolarSatellite` and never join a fleet. Those a raid
    /// destroys are counted out as they fall and the ones rebuilt are back
    /// the tick the raid ends, as the defences are.
    pub satellites: u32,
    pub storage: StorageState,
    pub buildings: Vec<BuildingState>,
    pub research: Vec<ResearchState>,
    /// Buildings under construction, at most `build_slots` of them.
    pub build_queue: Vec<BuildQueueItem>,
    /// Waiting behind them; nothing is paid until an item starts.
    pub build_pending: Vec<QueuedBuild>,
    /// How many buildings may be under construction at once.
    pub build_slots: u8,
    /// What opens one more building slot; absent when every slot is open.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub build_slot_unlock: Option<Requirement>,
    /// Orders that may wait behind the running ones in each queue. An order
    /// that fits is always accepted, started or not; only a full waiting
    /// line (or an order that can never start) is refused with `QueueFull`.
    pub queue_waiting_max: u8,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub shipyard_queue: Option<ShipyardQueueItem>,
    pub shipyard_pending: Vec<QueuedShip>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub research_active: Option<ResearchItem>,
    pub research_pending: Vec<QueuedResearch>,
    /// The ships of every fleet standing at home, summed by class, in
    /// catalogue order, and how many they are in all.
    pub docked_ships: Vec<ShipGroupState>,
    pub docked_ship_count: u32,
    /// Defences standing at home. Those a raid destroys are counted out as
    /// they fall and the ones rebuilt are back the tick the raid ends.
    pub defences: Vec<DefenceState>,
    /// Docked ships, Solar Satellites and all defences together, on the
    /// power scale. It is a sum and decides nothing: a raid is a battle,
    /// and `raid.odds` is how it would go.
    pub home_defence_power: f32,
    /// The share of `home_defence_power` the ships standing at home add.
    pub docked_power: f32,
    /// The Solar Satellites' share of `home_defence_power`.
    pub satellite_power: f32,
    /// The standing defences' share of `home_defence_power`.
    pub structure_power: f32,
    /// Typical power of the next raid, from the empire's building and
    /// research points; the actual raid rolls 0.80 to 1.15 times this.
    pub next_raid_estimate_power: f32,
    /// A raid can come to this empire: its Shipyard stands at
    /// `RAID_MIN_SHIPYARD` or it has a defence. While false, `raid` is only
    /// the raid it would get if it did, and none is on the clock.
    pub raids_possible: bool,
    /// The raid being fought (`rounds_left`), else the one announced
    /// (`arrival_tick`), else one of typical size were it announced now:
    /// what it flies, and how it would end against the ships, satellites
    /// and defences standing at home this tick.
    pub raid: RaidForecast,
    /// What can be queued right now and what it costs; see `HomeworldCatalog`.
    pub catalog: HomeworldCatalog,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DefenceState {
    pub kind: DefenceKind,
    pub count: u32,
    /// Power of one with the owner's Weapons, Shielding and Armour Technology.
    pub power: f32,
}

/// A homeworld's energy. The mines need it and the Solar Plant, the Fusion
/// Reactor and the Solar Satellites make it; it is not stored.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EnergyState {
    pub made: u32,
    pub needed: u32,
    /// The fraction of their output the mines run at: `made / needed`, at
    /// most 1, and 1 when nothing is needed.
    pub fraction: f64,
    /// Energy each Solar Satellite makes.
    pub per_satellite: u32,
    /// Deuterium the Fusion Reactor burns each tick while it runs.
    pub fusion_burn: f64,
    /// The Fusion Reactor stands and makes nothing: the stockpile has no
    /// deuterium for it to burn.
    pub fusion_starved: bool,
}

/// Stockpile limits, each from the level of its own storage building and
/// the world pace. Mines stop at `cap`; docking cargo unloads only up to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StorageState {
    pub cap: Resources,
    /// What a raid cannot skim.
    pub protected: Resources,
    /// Seconds until each stockpile is full at the current production;
    /// 0 when it is, null when it is not filling.
    pub full_in_s: ResourceEta,
    /// Resources whose stockpile is at the cap right now.
    pub capped: Vec<ResourceKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ResourceEta {
    pub metal: Option<u64>,
    pub crystal: Option<u64>,
    pub deuterium: Option<u64>,
}

/// Everything a client needs to offer homeworld actions without knowing any
/// balance formulas: costs, build times, prerequisites and what each level
/// does, computed by the server from `scaling.rs` for the player's current
/// levels. Resent with every `HomeworldState`, so it tracks level changes
/// the tick they land. What never changes (a class's base figures, its
/// label, the rapid-fire table) is not here: the figures are columns of the
/// enums and the table is `WorldInfo.rapid_fire`. Whether the player can
/// pay is left to the client (compare `cost` with the stockpile in
/// `PlayerState.resources`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HomeworldCatalog {
    /// One entry per `BuildingType`, in declaration order.
    pub buildings: Vec<BuildingOption>,
    /// One entry per `ResearchType`, in declaration order.
    pub research: Vec<ResearchOption>,
    /// One entry per `ShipClass`, in declaration order.
    pub ships: Vec<ShipOption>,
    /// One entry per `DefenceKind`, in declaration order.
    pub defences: Vec<DefenceOption>,
}

/// One prerequisite: a building or tech that must reach level `need`, with
/// the player's current level, or (`energy`) an amount of energy the
/// homeworld must be making, with what it makes. The server's
/// "prerequisites not met" errors are written from the same values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Requirement {
    /// Building or tech label ("Shipyard", "Impulse Drive"), or "Energy".
    pub name: String,
    pub need: u32,
    pub have: u32,
    pub met: bool,
    /// `need` and `have` are energy made, not levels.
    pub energy: bool,
}

impl Requirement {
    fn level(name: &str, need: u8, have: u8) -> Self {
        Requirement { name: name.to_string(), need: need.into(), have: have.into(), met: have >= need, energy: false }
    }

    fn energy(need: u32, have: u32) -> Self {
        Requirement { name: "Energy".to_string(), need, have, met: have >= need, energy: true }
    }

    /// Compact form for cards: "Shipyard >= 4".
    pub fn label(&self) -> String {
        format!("{} >= {}", self.name, self.need)
    }

    fn phrase(&self) -> String {
        if self.energy {
            format!("{} energy made (you make {})", self.need, self.have)
        } else {
            format!("{} level {} (you have {})", self.name, self.need, self.have)
        }
    }
}

/// "Cruiser needs Shipyard level 5 (you have 2) and Ion Technology level 2
/// (you have 0)"; None when every requirement is met.
pub fn missing_requirements_message(subject: &str, requires: &[Requirement]) -> Option<String> {
    let parts: Vec<String> = requires.iter().filter(|r| !r.met).map(Requirement::phrase).collect();
    let (last, rest) = parts.split_last()?;
    let list = if rest.is_empty() { last.clone() } else { format!("{} and {}", rest.join(", "), last) };
    Some(format!("{subject} needs {list}"))
}

fn requirements(prereqs: Prereqs, buildings: &BuildingLevels, research: &ResearchLevels) -> Vec<Requirement> {
    prereqs
        .iter()
        .map(|(p, need)| match p {
            PrereqRef::Building(b) => Requirement::level(b.label(), *need, buildings.get(*b)),
            PrereqRef::Research(t) => Requirement::level(t.label(), *need, research.get(*t)),
        })
        .collect()
}

/// Everything standing between the player and the next level of `b`.
pub fn building_requirements(buildings: &BuildingLevels, research: &ResearchLevels, b: BuildingType) -> Vec<Requirement> {
    requirements(scaling::building_prerequisites(b), buildings, research)
}

/// Prerequisites for researching level `level` of `t`, including the
/// Research Lab the server requires before any research and, for Graviton
/// Technology, the energy the homeworld must be making (`energy_made`).
pub fn research_requirements(
    buildings: &BuildingLevels,
    research: &ResearchLevels,
    t: ResearchType,
    level: u8,
    energy_made: u32,
) -> Vec<Requirement> {
    let prereqs = scaling::research_prerequisites(t);
    let names_lab = prereqs.iter().any(|(p, _)| *p == PrereqRef::Building(BuildingType::ResearchLab));
    let mut requires = Vec::new();
    if !names_lab {
        requires.push(Requirement::level(BuildingType::ResearchLab.label(), 1, buildings.research_lab));
    }
    requires.extend(requirements(prereqs, buildings, research));
    let energy = scaling::research_energy(t, level);
    if energy > 0 {
        requires.push(Requirement::energy(energy, energy_made));
    }
    requires
}

/// Prerequisites for building `c`: its Shipyard level and its research.
pub fn ship_requirements(buildings: &BuildingLevels, research: &ResearchLevels, c: ShipClass) -> Vec<Requirement> {
    requirements(scaling::ship_prerequisites(c), buildings, research)
}

/// Prerequisites for a defence: its Shipyard level and its research.
pub fn defence_requirements(buildings: &BuildingLevels, research: &ResearchLevels, kind: DefenceKind) -> Vec<Requirement> {
    requirements(kind.prerequisites(), buildings, research)
}

/// What a level of a building or a research does, as a figure a client
/// prints beside its label. Rates are a tick at the world's pace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectKind {
    /// What the mine makes with all the energy it needs; the natural
    /// production is not counted.
    MetalPerTick,
    CrystalPerTick,
    DeuteriumPerTick,
    EnergyUsed,
    EnergyMade,
    /// Deuterium the Fusion Reactor burns while it runs.
    FusionBurnPerTick,
    /// What building, ship and defence, or research time is divided by,
    /// with every other building as it stands.
    BuildingSpeed,
    ShipyardSpeed,
    ResearchSpeed,
    /// The cap of the building's own resource, and the share of it a raid
    /// cannot take.
    StorageCap,
    StorageProtected,
    /// What every fleet's tank is multiplied by.
    TankFactor,
    /// What attack, shield or hull is multiplied by: for defences and
    /// Solar Satellites at once, for a fleet from when it next docks at
    /// home (`FleetState.refit`).
    Attack,
    Shield,
    Hull,
    /// What the speed of the ships on this drive is multiplied by.
    Speed,
    /// What a jump's burn is multiplied by.
    JumpFuel,
    /// Taken off the chance that an emergency jump harms a ship, as a share.
    RecallRisk,
    BuildSlots,
    /// Hops around the homeworld that are seen.
    SensorHops,
    /// What a unit of ore yields.
    OreYield,
}

impl EffectKind {
    pub fn label(self) -> &'static str {
        match self {
            EffectKind::MetalPerTick => "metal",
            EffectKind::CrystalPerTick => "crystal",
            EffectKind::DeuteriumPerTick => "deuterium",
            EffectKind::EnergyUsed => "energy used",
            EffectKind::EnergyMade => "energy made",
            EffectKind::FusionBurnPerTick => "deuterium burned",
            EffectKind::BuildingSpeed => "building speed",
            EffectKind::ShipyardSpeed => "shipyard speed",
            EffectKind::ResearchSpeed => "research speed",
            EffectKind::StorageCap => "storage",
            EffectKind::StorageProtected => "safe from raids",
            EffectKind::TankFactor => "fleet tanks",
            EffectKind::Attack => "attack",
            EffectKind::Shield => "shield",
            EffectKind::Hull => "hull",
            EffectKind::Speed => "speed",
            EffectKind::JumpFuel => "fuel a jump",
            EffectKind::RecallRisk => "emergency jump risk",
            EffectKind::BuildSlots => "building slots",
            EffectKind::SensorHops => "sensor range",
            EffectKind::OreYield => "ore yield",
        }
    }
}

/// One figure of a building or research: at the level it stands at, and at
/// the next (absent at the highest level).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Effect {
    pub kind: EffectKind,
    pub now: f64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub next: Option<f64>,
}

fn effects(kinds: &[EffectKind], level: u8, max_level: u8, at: impl Fn(EffectKind, u8) -> f64) -> Vec<Effect> {
    kinds
        .iter()
        .map(|&kind| Effect { kind, now: at(kind, level), next: (level < max_level).then(|| at(kind, level + 1)) })
        .collect()
}

fn building_effects(b: BuildingType, buildings: &BuildingLevels, research: &ResearchLevels, pace: &Pace) -> Vec<Effect> {
    use EffectKind as E;
    let kinds: &[EffectKind] = match b {
        BuildingType::MetalMine => &[E::MetalPerTick, E::EnergyUsed],
        BuildingType::CrystalMine => &[E::CrystalPerTick, E::EnergyUsed],
        BuildingType::DeuteriumSynthesizer => &[E::DeuteriumPerTick, E::EnergyUsed],
        BuildingType::SolarPlant => &[E::EnergyMade],
        BuildingType::FusionReactor => &[E::EnergyMade, E::FusionBurnPerTick],
        BuildingType::RoboticsFactory => &[E::BuildingSpeed],
        BuildingType::NaniteFactory => &[E::BuildingSpeed, E::ShipyardSpeed],
        BuildingType::Shipyard => &[E::ShipyardSpeed],
        BuildingType::MetalStorage | BuildingType::CrystalStorage => &[E::StorageCap, E::StorageProtected],
        BuildingType::DeuteriumTank => &[E::StorageCap, E::StorageProtected, E::TankFactor],
        BuildingType::ResearchLab => &[E::ResearchSpeed],
    };
    effects(kinds, buildings.get(b), scaling::MAX_BUILDING_LEVEL, |kind, level| {
        let mut with = buildings.clone();
        with.set(b, level);
        match kind {
            E::MetalPerTick | E::CrystalPerTick | E::DeuteriumPerTick => {
                pace.rate(scaling::mine_output_per_hour(b, level) / 3600.0)
            }
            E::EnergyUsed => f64::from(scaling::energy_needed_by(b, level)),
            E::EnergyMade if b == BuildingType::FusionReactor => {
                f64::from(scaling::fusion_energy(level, research.energy_technology))
            }
            E::EnergyMade => f64::from(scaling::solar_plant_energy(level)),
            E::FusionBurnPerTick => pace.rate(f64::from(scaling::fusion_burn_per_hour(level)) / 3600.0),
            E::BuildingSpeed => scaling::building_speed(&with),
            E::ShipyardSpeed => scaling::shipyard_speed(&with),
            E::ResearchSpeed => scaling::research_speed(&with),
            E::StorageCap => scaling::storage_cap_at(level, pace),
            E::StorageProtected => scaling::storage_protected_at(level, pace),
            E::TankFactor => scaling::tank_modifier(level),
            _ => 0.0,
        }
    })
}

fn research_effects(t: ResearchType, buildings: &BuildingLevels, research: &ResearchLevels) -> Vec<Effect> {
    use EffectKind as E;
    let drive = match t {
        ResearchType::CombustionDrive => Some(Drive::Combustion),
        ResearchType::ImpulseDrive => Some(Drive::Impulse),
        ResearchType::HyperspaceDrive => Some(Drive::Hyperspace),
        _ => None,
    };
    let kinds: &[EffectKind] = match t {
        ResearchType::EspionageTechnology => &[E::SensorHops],
        ResearchType::ComputerTechnology => &[E::BuildSlots],
        ResearchType::WeaponsTechnology => &[E::Attack],
        ResearchType::ShieldingTechnology => &[E::Shield],
        ResearchType::ArmourTechnology => &[E::Hull],
        ResearchType::EnergyTechnology if buildings.fusion_reactor > 0 => &[E::JumpFuel, E::EnergyMade],
        ResearchType::EnergyTechnology => &[E::JumpFuel],
        ResearchType::HyperspaceTechnology => &[E::RecallRisk],
        ResearchType::CombustionDrive | ResearchType::ImpulseDrive | ResearchType::HyperspaceDrive => &[E::Speed],
        ResearchType::LaserTechnology => &[E::OreYield],
        ResearchType::IonTechnology | ResearchType::PlasmaTechnology | ResearchType::GravitonTechnology => &[],
    };
    effects(kinds, research.get(t), scaling::research_max_level(t), |kind, level| {
        let mut with = research.clone();
        with.set(t, level);
        match kind {
            E::SensorHops => f64::from(scaling::sensor_range(level)),
            E::BuildSlots => f64::from(scaling::building_slots(&with)),
            E::Attack | E::Shield | E::Hull => 1.0 + 0.1 * f64::from(level),
            E::JumpFuel => scaling::fuel_rate_modifier(level),
            E::EnergyMade => f64::from(scaling::fusion_energy(buildings.fusion_reactor, level)),
            E::RecallRisk => f64::from(scaling::recall_damage_reduction(level)),
            E::Speed => 1.0 + drive.map_or(0.0, Drive::speed_per_level) * f64::from(level),
            E::OreYield => scaling::harvest_yield(level),
            _ => 0.0,
        }
    })
}

/// Cost and duration of the next level of a building or tech.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UpgradeStep {
    pub level: u8,
    pub cost: Resources,
    pub ticks: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildingOption {
    pub building_type: BuildingType,
    pub level: u8,
    pub max_level: u8,
    /// The upgrade to `level + 1`; absent at `max_level`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub next: Option<UpgradeStep>,
    /// All prerequisites with their status; the entry is locked while any is unmet.
    pub requires: Vec<Requirement>,
    /// What the building does at its level and at the next.
    pub effects: Vec<Effect>,
    /// The homeworld's energy once `next` has finished, everything else as
    /// it stands now; absent at `max_level`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub energy_after: Option<EnergyAfter>,
}

/// The homeworld's energy as it would stand after one upgrade.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EnergyAfter {
    pub made: u32,
    pub needed: u32,
    /// `made / needed`, at most 1, as `EnergyState.fraction` is.
    pub fraction: f64,
    /// The Solar Plant stands at `MAX_BUILDING_LEVEL`: no further level of
    /// it can close a shortfall, only satellites and the Fusion Reactor.
    pub plant_maxed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResearchOption {
    pub tech: ResearchType,
    pub level: u8,
    pub max_level: u8,
    /// The upgrade to `level + 1`; absent at `max_level`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub next: Option<UpgradeStep>,
    /// Includes the Research Lab the server requires before any research.
    pub requires: Vec<Requirement>,
    /// What the research does at its level and at the next; empty for one
    /// that is only a requirement of other things.
    pub effects: Vec<Effect>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShipOption {
    #[serde(rename = "ship_class")]
    pub ship_class: ShipClass,
    /// Cost of one ship; a batch of `count` costs `count` times this.
    pub unit_cost: Resources,
    /// Ticks to build one ship at the current Shipyard and Nanite Factory levels.
    pub ticks_per_ship: u64,
    pub requires: Vec<Requirement>,
    /// What a ship in a fleet docked at home fights with: the class's
    /// figures with the owner's Armour, Shielding and Weapons Technology,
    /// cut off to whole numbers as a round counts them. A fleet away from
    /// home fights with `FleetState.refit`.
    pub hull: f32,
    pub shield: f32,
    pub weapon: f32,
    /// `weapon + (hull + shield) / 10` of those.
    pub power: f32,
    /// One ship's hold at the world's pace.
    pub cargo: f64,
    /// False for the Solar Satellite: it stays at the homeworld, counted in
    /// `HomeworldState.satellites`, and the flight figures below are 0.
    pub flies: bool,
    /// The drive it flies on with the owner's research; the Small Cargo and
    /// the Bomber change theirs.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub drive: Option<Drive>,
    pub speed: f64,
    /// Ticks one jump takes a fleet whose slowest ship this is.
    pub jump_ticks: u16,
    /// Fuel one jump burns, with Energy Technology's saving.
    pub jump_fuel: f64,
    /// The tank one ship adds to a fleet, with the Deuterium Tank counted.
    pub tank: f64,
    /// Raw units of ore a tick it extracts at density multiplier 1.
    pub harvest: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DefenceOption {
    pub kind: DefenceKind,
    /// Standing now.
    pub count: u32,
    /// The most that may stand, those on order included: 1 for a dome.
    /// Absent when there is no limit.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub max: Option<u32>,
    /// How many more may be ordered now: `max` less those standing and on
    /// order. Absent when there is no limit.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub room: Option<u32>,
    pub unit_cost: Resources,
    pub ticks_per_unit: u64,
    /// What one fights with: the kind's figures with the owner's Armour,
    /// Shielding and Weapons Technology as they stand now.
    pub hull: f32,
    pub shield: f32,
    pub weapon: f32,
    /// `weapon + (hull + shield) / 10` of those.
    pub power: f32,
    pub requires: Vec<Requirement>,
}

impl HomeworldCatalog {
    /// `defences` is what stands of each kind; `committed` adds to it those
    /// on order, which is what a limit counts.
    pub fn new(
        buildings: &BuildingLevels,
        research: &ResearchLevels,
        defences: &[u32; DefenceKind::COUNT],
        committed: &[u32; DefenceKind::COUNT],
        satellites: u32,
        fusion_fed: bool,
        pace: &Pace,
    ) -> Self {
        let energy_made = scaling::home_power(buildings, research, satellites, fusion_fed, pace).made;
        let building_options = BuildingType::ALL
            .into_iter()
            .map(|b| {
                let level = buildings.get(b);
                BuildingOption {
                    building_type: b,
                    level,
                    max_level: scaling::MAX_BUILDING_LEVEL,
                    next: (level < scaling::MAX_BUILDING_LEVEL).then(|| UpgradeStep {
                        level: level + 1,
                        cost: scaling::building_cost(b, level + 1),
                        ticks: scaling::building_time(b, level + 1, buildings, pace),
                    }),
                    requires: building_requirements(buildings, research, b),
                    effects: building_effects(b, buildings, research, pace),
                    energy_after: (level < scaling::MAX_BUILDING_LEVEL).then(|| {
                        let mut with = buildings.clone();
                        with.set(b, level + 1);
                        let power = scaling::home_power(&with, research, satellites, fusion_fed, pace);
                        EnergyAfter {
                            made: power.made,
                            needed: power.needed,
                            fraction: power.fraction(),
                            plant_maxed: buildings.solar_plant >= scaling::MAX_BUILDING_LEVEL,
                        }
                    }),
                }
            })
            .collect();

        let research_options = ResearchType::ALL
            .into_iter()
            .map(|t| {
                let level = research.get(t);
                let max_level = scaling::research_max_level(t);
                ResearchOption {
                    tech: t,
                    level,
                    max_level,
                    next: (level < max_level).then(|| UpgradeStep {
                        level: level + 1,
                        cost: scaling::research_cost(t, level + 1),
                        ticks: scaling::research_time(t, level + 1, buildings, pace),
                    }),
                    requires: research_requirements(buildings, research, t, level.saturating_add(1), energy_made),
                    effects: research_effects(t, buildings, research),
                }
            })
            .collect();

        let ship_options = ShipClass::ALL
            .into_iter()
            .map(|c| {
                let built = scaling::ship_figures(c, scaling::Refit::of(research).arms());
                let speed = scaling::ship_speed(c, research);
                ShipOption {
                    ship_class: c,
                    unit_cost: c.build_cost(),
                    ticks_per_ship: scaling::ship_build_time(c, buildings, pace),
                    requires: ship_requirements(buildings, research, c),
                    hull: built.hull as f32,
                    shield: built.shield as f32,
                    weapon: built.attack as f32,
                    power: built.power(),
                    cargo: f64::from(c.base_stats().cargo) * pace.cargo_mult(),
                    flies: c.flies(),
                    drive: scaling::ship_drive(c, research).map(|(drive, _, _)| drive),
                    speed,
                    jump_ticks: if c.flies() { scaling::jump_ticks(speed) } else { 0 },
                    jump_fuel: scaling::ship_burn(c, research) * scaling::fuel_rate_modifier(research.energy_technology),
                    tank: scaling::ship_tank(c, research) * scaling::tank_modifier(buildings.deuterium_tank),
                    harvest: c.harvest_power(),
                }
            })
            .collect();

        let defence_options = DefenceKind::ALL
            .into_iter()
            .map(|kind| {
                let built = scaling::unit_figures(ShipyardItem::Defence(kind), scaling::Refit::of(research).arms());
                DefenceOption {
                    kind,
                    count: defences[kind as usize],
                    max: kind.max_count(),
                    room: kind.max_count().map(|most| most.saturating_sub(committed[kind as usize])),
                    unit_cost: kind.build_cost(),
                    ticks_per_unit: scaling::defence_build_time(kind, buildings, pace),
                    hull: built.hull as f32,
                    shield: built.shield as f32,
                    weapon: built.attack as f32,
                    power: scaling::structure_power(kind, research),
                    requires: defence_requirements(buildings, research, kind),
                }
            })
            .collect();

        HomeworldCatalog {
            buildings: building_options,
            research: research_options,
            ships: ship_options,
            defences: defence_options,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildingState {
    pub building_type: BuildingType,
    pub level: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchState {
    pub tech: ResearchType,
    pub level: u8,
}

/// A building under construction. `id` is the stable queue id: the one the
/// order got when it was queued, kept when it started.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildQueueItem {
    pub id: u64,
    pub building_type: BuildingType,
    pub target_level: u8,
    pub start_tick: u64,
    pub end_tick: u64,
}

/// Why a queued order has not started yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WaitReason {
    /// Every slot is busy.
    Slot,
    /// The stockpile does not cover the cost (see `waiting_for`).
    Resources,
    /// A level, building or tech it needs is still being built.
    Prerequisite,
    /// It could start, but an earlier order with `reserve` set is ready and
    /// short of resources, and holds the line.
    Order,
}

/// Why a shipyard batch has not started yet. A batch is paid for when it is
/// ordered, so it never waits on resources or behind a reserving order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BatchWait {
    /// The yard is busy with a batch ahead of it.
    Slot,
    /// A Shipyard level, building or tech it needs is still being built.
    Prerequisite,
}

impl From<BatchWait> for WaitReason {
    fn from(wait: BatchWait) -> WaitReason {
        match wait {
            BatchWait::Slot => WaitReason::Slot,
            BatchWait::Prerequisite => WaitReason::Prerequisite,
        }
    }
}

/// A building waiting to start. A free slot starts the earliest waiting
/// order that can start now; an order that cannot keeps its place unless it
/// has `reserve` set, in which case it holds the line while short. `id` is
/// stable for the life of the order (cancel by it). `cost` and `ticks` are
/// what it would cost and take if it started now; `waiting_for` is the exact
/// resources still missing (absent when nothing is short); `waiting_on`
/// names the binding reason; `start_in` is the estimated ticks until it
/// starts at the current production, ignoring what the other queues will
/// spend (absent when it depends on something that cannot be estimated).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueuedBuild {
    pub id: u64,
    pub building_type: BuildingType,
    pub target_level: u8,
    #[serde(default)]
    pub reserve: bool,
    pub cost: Resources,
    pub ticks: u64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub waiting_for: Option<Resources>,
    pub waiting_on: WaitReason,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub start_in: Option<u64>,
}

/// A research level waiting to start; see `QueuedBuild`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueuedResearch {
    pub id: u64,
    pub tech: ResearchType,
    pub target_level: u8,
    #[serde(default)]
    pub reserve: bool,
    pub cost: Resources,
    pub ticks: u64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub waiting_for: Option<Resources>,
    pub waiting_on: WaitReason,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub start_in: Option<u64>,
}

/// A shipyard batch that is paid for and waiting its turn. `id` is stable
/// for the life of the order (cancel by it; half of `cost` comes back).
/// `cost` is what the whole batch paid when it was ordered, `unit_cost` one
/// unit, `ticks` the time for one unit; `start_in` is the estimated ticks
/// until its first unit starts (absent when it depends on something that
/// cannot be estimated). A batch that has started builds every unit without
/// stopping.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueuedShip {
    pub id: u64,
    /// A ship class or a defence structure: the wire carries just its name.
    pub item: ShipyardItem,
    pub count: u16,
    pub cost: Resources,
    pub unit_cost: Resources,
    pub ticks: u64,
    pub waiting_on: BatchWait,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub start_in: Option<u64>,
}

/// The batch in production. `built` units of `count` are delivered and one
/// more is under way until `end_tick`; the whole batch was paid for when it
/// was ordered.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipyardQueueItem {
    pub id: u64,
    /// A ship class or a defence structure: the wire carries just its name.
    pub item: ShipyardItem,
    pub count: u16,
    pub built: u16,
    pub start_tick: u64,
    pub end_tick: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchItem {
    pub id: u64,
    pub tech: ResearchType,
    pub target_level: u8,
    pub start_tick: u64,
    pub end_tick: u64,
}

// ── Game Events ───────────────────────────────────────────────────
//
// Scoping. A player only receives events that concern them: their own
// fleets, homeworld and queues, plus combat and loss detail for fights in
// any sector where they have a fleet or their homeworld. Other empires'
// fights elsewhere are not reported at all.
//
// Ownership. Combat and loss events name the empire that owns each fleet
// (`owner`, `None` for an NPC; empire names are public) and carry `mine`,
// computed per recipient: true when the event involves one of the
// receiver's own fleets. A player can therefore always tell their own
// losses from a fight they merely witnessed.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameEvent {
    pub tick: u64,
    #[serde(flatten)]
    pub kind: EventKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum EventKind {
    BattleRound(BattleRoundEvent),
    ShipsLost(ShipsLostEvent),
    FleetDestroyed(FleetDestroyedEvent),
    ResourceHarvested(ResourceHarvestedEvent),
    SectorEntered(SectorEnteredEvent),
    CombatStarted(CombatStartedEvent),
    CombatEnded(CombatEndedEvent),
    Pursuit(PursuitEvent),
    SalvageCollected(SalvageCollectedEvent),
    Stockpile(StockpileEvent),
    FleetArrived(FleetArrivedEvent),
    BuildingCompleted(BuildingCompletedEvent),
    ResearchCompleted(ResearchCompletedEvent),
    ShipBuilt(ShipBuiltEvent),
    DefenceBuilt(DefenceBuiltEvent),
    ScanCompleted(ScanCompletedEvent),
    RaidIncoming(RaidIncomingEvent),
    RaidResolved(RaidResolvedEvent),
    SiteExplorationStarted(SiteExplorationStartedEvent),
    SiteExplored(SiteExploredEvent),
    SiteAmbush(SiteAmbushEvent),
    PolicyAction(PolicyActionEvent),
    Alert(AlertEvent),
    SalvageDespawned(SalvageDespawnedEvent),
    StorageNearCap(StorageNearCapEvent),
    StorageFull(StorageFullEvent),
    ChartDelivered(ChartDeliveredEvent),
    Queue(QueueEvent),
    Exchanged(ExchangedEvent),
    OutpostCompleted(OutpostCompletedEvent),
    RelayFounded(RelayFoundedEvent),
    RelayDecayed(RelayDecayedEvent),
}

/// A relay of the player's was founded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayFoundedEvent {
    pub player_id: u64,
    pub sector: Hex,
    pub ring: u16,
}

/// A relay of the player's stood unvisited for its whole span and is gone,
/// with the deuterium that was in its store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayDecayedEvent {
    pub player_id: u64,
    pub sector: Hex,
    pub lost: f64,
}

/// A level of the player's outpost is finished. Level 1 opens it: the
/// extractor starts, tanks are filled and the raid clock begins to run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutpostCompletedEvent {
    pub player_id: u64,
    pub sector: Hex,
    pub level: u8,
}

/// A fleet traded at the hub's exchange: what it gave, what it got, the fee
/// and where the player's meter now stands.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangedEvent {
    pub sector: Hex,
    #[serde(flatten)]
    pub terms: ExchangeTerms,
}

/// What happened to a queue order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueueAction {
    /// The order began.
    Started,
    /// The order was accepted but cannot start yet.
    Waiting,
    /// The order was cancelled.
    Cancelled,
}

/// The answer to a build, research, ship or defence order, and to a cancel,
/// and the notice that a waiting order started on its own.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueEvent {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub player_id: Option<u64>,
    pub queue_type: QueueType,
    /// The stable queue id of the order; what `cancel_build` and
    /// `cancel_queued` take.
    pub id: u64,
    /// "Metal Mine Lv.5", "Light Fighter x3".
    pub item: String,
    pub action: QueueAction,
    /// What this event took from the stockpile. A building or research
    /// level pays when it starts (`Started`). A shipyard batch pays for
    /// every unit when it is ordered: on the event that answers the order,
    /// `Started` if it began at once or `Waiting` if it joined the line, and
    /// nothing on the later `Started` of a batch that waited.
    #[serde(default)]
    pub paid: Resources,
    /// `Cancelled`: what came back. Half of the payment for a started
    /// building or research level, nothing for a waiting one; for a
    /// shipyard batch, waiting or started, half the price of the units not
    /// yet delivered.
    #[serde(default)]
    pub refunded: Resources,
    /// `Waiting`: the exact shortfall of a building or research order,
    /// absent when nothing is short. Never present for a shipyard batch.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub waiting_for: Option<Resources>,
    /// `Waiting`: why it has not started.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub waiting_on: Option<WaitReason>,
    /// `Waiting`: estimated ticks until it starts, when it can be estimated.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub start_in: Option<u64>,
}

/// A stockpile passed 85 percent of its cap.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageNearCapEvent {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub player_id: Option<u64>,
    pub resource: ResourceKind,
    pub ratio: f64,
    /// Seconds until full at the current production; absent when not filling.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub full_in_s: Option<u64>,
}

/// A stockpile reached its cap; further production is discarded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageFullEvent {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub player_id: Option<u64>,
    pub resource: ResourceKind,
}

/// A boarding party is aboard the derelict; loot (or trouble) at end_tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteExplorationStartedEvent {
    pub fleet_id: u64,
    pub sector: Hex,
    pub tier: u8,
    pub end_tick: u64,
}

/// The boarding paid off.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteExploredEvent {
    pub fleet_id: u64,
    pub sector: Hex,
    pub tier: u8,
    /// Loot taken aboard, a relic's worth included (cargo-capped; overflow
    /// drops as sector salvage).
    pub resources: Resources,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub recovered_ship: Option<ShipClass>,
    /// Instant tech level granted by a recovered data core.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tech_cache: Option<ResearchType>,
    /// An ancient relic came aboard: the loot of the site is worth
    /// `RELIC_LOOT_MULT` times more for it.
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub relic: bool,
}

/// A fleet docked at the homeworld, or arrived at its owner's finished
/// outpost, and handed over the sectors it charted. What they paid into
/// the stockpile is a `StockpileEvent` of cause `Charts`; what a full store
/// has no room for is lost.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartDeliveredEvent {
    pub fleet_id: u64,
    /// Sectors newly credited to the empire.
    pub sectors: u32,
}

/// Whatever killed the crew is still aboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteAmbushEvent {
    pub fleet_id: u64,
    pub sector: Hex,
    pub npc_fleet_id: u64,
}

/// A standing order acted on the player's behalf — always says why.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyActionEvent {
    pub fleet_id: u64,
    pub preset: PolicyPreset,
    /// Short verb: "scan", "move", "harvest", "return", "attack", …
    pub action: String,
    pub reason: String,
}

/// One round of a fight, one a tick while it lasts. It is OGame's round:
/// every unit's shield is full at its start; every unit of both sides then
/// fires, at once, each at one unit of the other side picked at random,
/// and again while its rapid fire against what it hit holds. A shot weaker
/// than a hundredth of the target's full shield bounces; one that breaks
/// the shield takes the rest from the hull; a unit left with no shield and
/// at or under `EXPLODE_BELOW_PCT` hundredths of its hull may explode. The
/// report is by class, never by ship, so it is the same size for five ships
/// and for five thousand.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BattleRoundEvent {
    pub sector: Hex,
    /// 1 for the first round of the fight.
    pub round: u32,
    /// The players' fleets, with what they fired this round.
    pub fleets: BattleSide,
    /// The hostile groups, with what they fired. In a raid they are one
    /// group, the raiders.
    pub hostiles: BattleSide,
    /// A fleet of the receiver's is in the fight, or the fight is a raid on
    /// the receiver's homeworld or outpost.
    #[serde(default)]
    pub mine: bool,
    /// The round is a raid's: `fleets` holds the raided empire's fleets at
    /// the dock and, marked `battery`, its defences. `BattleState.raid` of
    /// the same sector says how many rounds are left.
    #[serde(default)]
    pub raid: bool,
}

/// One side of a round: who stands after it, and what the side fired.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BattleSide {
    /// Every fleet or group of the side that entered the round, by id, as
    /// it stands after it.
    pub groups: Vec<BattleGroup>,
    /// Units the side stood in the round with, in all its groups, and how
    /// many of them still stand after it.
    pub entered: u32,
    pub standing: u32,
    /// The side's fire, one line for each pair of firing and target unit
    /// that exchanged a shot, in catalogue order.
    pub volleys: Vec<Volley>,
    /// Shots the side fired, their attack summed, what the other side's
    /// shields took of it, what reached its hulls, and the units of the
    /// other side it destroyed.
    pub shots: u64,
    pub power: u64,
    pub absorbed: u64,
    pub hull_damage: u64,
    pub destroyed: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BattleGroup {
    /// The fleet or the group. A raid's raiders carry an id of their own;
    /// the defences of the place raided carry 0 and `battery`.
    pub fleet_id: u64,
    /// Empire owning the fleet; `None` for a hostile group.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub owner: Option<String>,
    /// What a hostile group is ("3x cruiser pack"); empty for a player's
    /// fleet. For a battery, what it is ("home defences", "outpost platform").
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub label: String,
    /// The fleet is the receiver's.
    #[serde(default)]
    pub mine: bool,
    /// The fleet is leaving: it was fired on this round and did not fire.
    #[serde(default)]
    pub fleeing: bool,
    /// Not a fleet: the defences and Solar Satellites of a homeworld under
    /// raid, or an outpost's platform. It stands on the fleets' side.
    #[serde(default)]
    pub battery: bool,
    /// By unit, in catalogue order. A unit wiped out this round is listed
    /// once more with a count of 0.
    pub units: Vec<BattleUnits>,
}

/// The units of one kind in a fleet or group after a round.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BattleUnits {
    /// A ship class or a defence: the wire carries just its name.
    pub unit: ShipyardItem,
    /// How many stood in the round when it began, and how many stand after
    /// it. The rest of the row is of those that stand.
    pub entered: u32,
    pub count: u32,
    pub damaged: u32,
    pub at_risk: u32,
    pub hull: u64,
    pub hull_max: u64,
    /// Destroyed this round.
    pub lost: u32,
}

/// What the units of one kind fired at the units of another in a round.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Volley {
    pub from: ShipyardItem,
    pub at: ShipyardItem,
    /// Every shot, rapid fire included. Of them `bounced` did nothing to a
    /// shield they were too weak for, and `wasted` went at a unit already
    /// destroyed in this round.
    pub shots: u32,
    pub bounced: u32,
    pub wasted: u32,
    /// Attack the targets' shields took, and what reached their hulls.
    pub absorbed: u64,
    pub hull_damage: u64,
    /// Targets destroyed.
    pub destroyed: u32,
}

/// How a fight ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BattleOutcome {
    /// No hostile unit is left and a fleet still stands.
    HostilesDestroyed,
    /// No ship of any fleet is left and a hostile still stands.
    FleetsDestroyed,
    /// The last units of both sides fell in the same round.
    BothDestroyed,
    /// The last fleet made the emergency jump home. The hostiles keep the
    /// losses and the damage they took, and mend in time.
    Withdrawn,
    /// `STANDOFF_ROUNDS` rounds took no hull from either side: nobody can
    /// hurt anybody. Both sides stay where they are, as they are, and
    /// nothing starts the fight again but an arrival or an `attack`.
    Standoff,
    /// The last fleet jumped out of the sector under fire. The hostiles keep
    /// the losses and the damage they took; a group that was beating the
    /// fleet may follow it (`Pursuit`).
    Fled,
    /// A raid's `RAID_ROUNDS` rounds are over, or a round took no hull from
    /// anybody, and a defender still stands: the raiders leave with nothing.
    Outlasted,
}

/// What one fleet or group lost over a whole fight.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BattleLosses {
    pub fleet_id: u64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub owner: Option<String>,
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub label: String,
    #[serde(default)]
    pub mine: bool,
    /// As `BattleGroup.battery`: the defences of the place raided.
    #[serde(default)]
    pub battery: bool,
    /// Units destroyed, by kind, in catalogue order; empty when it lost none.
    pub lost: Vec<UnitCount>,
    /// Nothing of it is left.
    pub destroyed: bool,
}

/// So many units of one kind, ship or defence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitCount {
    pub unit: ShipyardItem,
    pub count: u32,
}

/// A group took up a fleet's trail, went on along it, or gave it up. A group
/// that catches its fleet says so with `CombatStarted`. It reaches the
/// fleet's owner and anyone present in `sector`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PursuitEvent {
    pub group_id: u64,
    /// What the group is ("3x cruiser pack").
    pub label: String,
    /// The fleet followed and the empire owning it.
    pub fleet_id: u64,
    pub owner: String,
    /// The fleet is the receiver's.
    #[serde(default)]
    pub mine: bool,
    /// Where the group stands.
    pub sector: Hex,
    pub phase: PursuitPhase,
    /// The sector it jumps to and the ticks until it lands there: where the
    /// fleet stands for a pursuit begun or continued, the way back to its
    /// own sector for one given up. Absent when it gave up at home.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub to: Option<Hex>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub arrives_in: Option<u16>,
    /// Jumps it will still make after this one before it turns back.
    pub hexes_left: u8,
    /// Why it gave up, in the engine's words; absent otherwise.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub why: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PursuitPhase {
    /// The fight broke off and the group set out after the fleet.
    Began,
    /// It landed where the fleet had been and jumped on after it.
    Continued,
    /// It turned back.
    Abandoned,
}

/// Ships a fleet lost outside a fight: those that broke up in an emergency
/// jump. A fight's losses are in `BattleRound` and `CombatEnded`, and a
/// raid's in `RaidResolved` as well.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShipsLostEvent {
    pub fleet_id: u64,
    pub sector: Hex,
    /// Empire that owned the ships.
    pub owner: String,
    /// The ships were the receiver's.
    #[serde(default)]
    pub mine: bool,
    pub ships: Vec<ShipCount>,
}

/// Every ship of a fleet is gone. For an NPC fleet killed by players,
/// `salvage` is the wreckage this kill added to `sector`. The fight's
/// `CombatEnded` adds the kills of the fight up in its `salvage`, and names
/// the whole pile lying there, wreckage left from before included, in
/// `salvage_pile` when that is more. Player fleets leave no wreckage, so
/// `salvage` is zero. A destroyed fleet of the receiver's also produces a
/// `Critical` `Alert`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetDestroyedEvent {
    pub fleet_id: u64,
    pub is_npc: bool,
    pub sector: Hex,
    /// Empire that owned the fleet; `None` for an NPC.
    pub owner: Option<String>,
    /// The fleet was the receiver's.
    #[serde(default)]
    pub mine: bool,
    pub salvage: Resources,
    /// What an NPC group was ("3x cruiser pack"); empty for a player fleet.
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub label: String,
}

/// What a harvesting fleet took aboard. Aggregated: one event per fleet
/// every `HARVEST_REPORT_TICKS` ticks (and when harvesting stops) rather
/// than one per resource per tick; `ticks` is how many ticks it covers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceHarvestedEvent {
    pub fleet_id: u64,
    pub resources: Resources,
    pub ticks: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectorEnteredEvent {
    pub fleet_id: u64,
    pub sector: Hex,
    pub first_visit: bool,
}

/// `enemy_fleet_id` is the same id the sector view listed under `hostiles`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombatStartedEvent {
    pub player_fleet_id: u64,
    /// Empire owning `player_fleet_id`.
    pub owner: String,
    pub enemy_fleet_id: u64,
    /// What the hostile group is ("3x cruiser pack").
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub enemy: String,
    pub sector: Hex,
    /// `player_fleet_id` is the receiver's.
    #[serde(default)]
    pub mine: bool,
}

/// A fight is over. `owners` lists the empires that fought. `salvage` is the
/// wreckage the hostiles' destruction left in the sector, the sum of the
/// fight's `FleetDestroyed` (absent when they were not destroyed). When
/// wreckage from before was still lying there, `salvage_pile` is the pile
/// the two make, which is what `collect_salvage` finds; absent when the
/// fight's wreckage is all there is. The pile despawns at
/// `salvage_despawn_tick`. When the fight was a raid, `raid` is set and the
/// `RaidResolved` of the same tick says what was rebuilt and what was taken.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombatEndedEvent {
    pub sector: Hex,
    /// `outcome` is `HostilesDestroyed`.
    pub player_victory: bool,
    pub outcome: BattleOutcome,
    /// The fight was a raid on a homeworld or an outpost.
    #[serde(default)]
    pub raid: bool,
    /// Rounds fought.
    pub rounds: u32,
    /// What every fleet and group that fought lost, fleets first.
    pub losses: Vec<BattleLosses>,
    pub owners: Vec<String>,
    #[serde(default)]
    pub mine: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub salvage: Option<Resources>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub salvage_pile: Option<Resources>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub salvage_despawn_tick: Option<u64>,
}

/// Resources went into the stockpile, or came out of it, other than by
/// production and by paying for an order, a repair or fuel: what a fleet's
/// hold put into it at the home dock, what `load` took from it into a hold,
/// wreckage lying on the homeworld that a docked fleet scooped into it, and
/// what the charts a fleet delivered paid.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StockpileEvent {
    pub player_id: u64,
    /// The fleet whose hold it came from or went to; for `Wreckage`, the
    /// docked fleet that scooped it; for `Charts`, the fleet that delivered
    /// them.
    pub fleet_id: u64,
    pub cause: StockpileCause,
    /// What went in (`Unloaded`, `Wreckage`, `Charts`) or came out
    /// (`Loaded`).
    pub amount: Resources,
}

/// Why the stockpile changed in a `StockpileEvent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StockpileCause {
    /// A fleet's hold was emptied into the stockpile at the home dock: a
    /// fleet docked with cargo, an `unload`, or the cargo of ships a fleet
    /// lost there.
    Unloaded,
    /// `load` put stock into a fleet's hold.
    Loaded,
    /// Wreckage on the homeworld, mostly a repelled raid's.
    Wreckage,
    /// Charts delivered to a dock: `scaling::chart_value` for each sector
    /// new to the empire, as far as the caps allow.
    Charts,
}

impl StockpileCause {
    /// What happened, as a client words it after the fleet's name and
    /// before the amount.
    pub fn phrase(self) -> &'static str {
        match self {
            StockpileCause::Unloaded => "unloaded into the stockpile",
            StockpileCause::Loaded => "loaded from the stockpile",
            StockpileCause::Wreckage => "scooped wreckage at home into the stockpile",
            StockpileCause::Charts => "delivered charts, paid into the stockpile",
        }
    }
}

/// A fleet scooped wreckage into its hold. `resources` is what went into the
/// hold; if the hold filled first, `remaining` is what is still on the
/// ground (it keeps its original despawn time). Absent when the pile is
/// gone. Wreckage on the player's own homeworld goes into the stockpile
/// instead, and is told by a `StockpileEvent`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SalvageCollectedEvent {
    pub fleet_id: u64,
    pub sector: Hex,
    pub resources: Resources,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub remaining: Option<Resources>,
}

/// A wreckage pile expired uncollected. Sent to players with a fleet or
/// their homeworld in `sector`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SalvageDespawnedEvent {
    pub sector: Hex,
    /// What was lost.
    pub resources: Resources,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetArrivedEvent {
    pub fleet_id: u64,
    pub sector: Hex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildingCompletedEvent {
    pub building_type: BuildingType,
    pub new_level: u8,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub player_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchCompletedEvent {
    pub tech: ResearchType,
    pub new_level: u8,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub player_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipBuiltEvent {
    #[serde(rename = "ship_class")]
    pub ship_class: ShipClass,
    pub count: u16,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub player_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefenceBuiltEvent {
    pub defence: DefenceKind,
    pub count: u16,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub player_id: Option<u64>,
}

/// Result of an active fleet scan: sectors revealed plus faint signal
/// contacts detected one hop beyond scan range.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanCompletedEvent {
    pub fleet_id: u64,
    /// The sector the scanning fleet is in. It is never one of the
    /// `signals`, which lie one hop beyond the scanned range.
    pub sector: Hex,
    pub sectors_revealed: u16,
    pub hostiles_detected: u16,
    /// The threat of every revealed sector.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub threats: Vec<SectorThreat>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub signals: Vec<SignalContact>,
}

/// A faint contact at the edge of scan range — a hint, not full intel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalContact {
    pub sector: Hex,
    pub signal: SignalKind,
    /// Coarse danger of the sector: 1 for T1 to T3, 2 for T4 to T6, 3 for T7 to T9.
    pub threat_band: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectorThreat {
    pub sector: Hex,
    pub threat: ThreatInfo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignalKind {
    RichOre,
    HostileMass,
    Derelict,
    Anomaly,
}

impl SignalKind {
    pub fn label(self) -> &'static str {
        match self {
            SignalKind::RichOre => "rich ore signature",
            SignalKind::HostileMass => "hostile mass reading",
            SignalKind::Derelict => "derelict transponder",
            SignalKind::Anomaly => "anomalous emissions",
        }
    }
}

/// Advance warning that a raid fleet is vectoring toward the homeworld, or
/// toward the player's outpost when `outpost` names its sector. A fleet may
/// leave before it arrives; one that stays is in the fight.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaidIncomingEvent {
    pub player_id: u64,
    pub arrival_tick: u64,
    /// `odds.label` in a word, read off the fight as it would go against
    /// what stands there now: "light" (the raiders are destroyed), "moderate"
    /// (the defence lasts the rounds out) or "heavy" (it may be lost).
    pub threat: String,
    /// The raid fleet's power, fixed at forecast time, on the scale
    /// `FleetState.power` uses.
    #[serde(default)]
    pub est_power: f32,
    /// What it flies, by class, fixed at forecast time.
    #[serde(default)]
    pub ships: Vec<ShipCount>,
    /// How it would end against what stands there now. The same figures
    /// follow in `HomeworldState.raid` and `OutpostState.raid` every tick
    /// until it arrives, as what stands there changes.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub odds: Option<RaidOdds>,
    /// The outpost the raid is bound for; absent for a raid on the homeworld.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub outpost: Option<Hex>,
}

/// How a raid on the homeworld ended, or on the player's outpost when
/// `outpost` names its sector. The fight itself was told round by round
/// (`BattleRound`) and ends with a `CombatEnded` in the same tick; a raid
/// that found nothing standing in its way was lost without a round, has no
/// `outcome` and no `CombatEnded`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaidResolvedEvent {
    pub player_id: u64,
    /// The outpost that was raided; absent for a raid on the homeworld.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub outpost: Option<Hex>,
    /// Repelled: no raider was left, or a defender stood when the rounds
    /// were over. False when no defender was left.
    pub defended: bool,
    /// How the fight ended: `hostiles_destroyed` and `both_destroyed` (no
    /// raider left), `outlasted`, `fleets_destroyed` (lost), or `fled` and
    /// `withdrawn` (lost: the last defender left). Absent when no round was
    /// fought.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub outcome: Option<BattleOutcome>,
    /// Rounds fought.
    #[serde(default)]
    pub rounds: u32,
    /// The raid broke the outpost: it makes nothing, fills no tank and has
    /// no platform until it has repaired itself from its store. Always
    /// false for a raid on the homeworld.
    #[serde(default)]
    pub breached: bool,
    /// What a lost raid took: its share of the stockpile above the
    /// protected floor, or of the outpost's store. What the ships destroyed
    /// in the fight carried was in the stockpile or the store by then, as
    /// far as it had room.
    pub resources_lost: Resources,
    /// The wreckage of a raid destroyed to the last ship at a homeworld;
    /// absent otherwise, and always for a raid on an outpost.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub salvage_dropped: Option<Resources>,
    /// Tick the pile despawns (absent when none dropped). A fleet docked at
    /// home scoops it into storage on its own; without one, an alert warns
    /// when it has `HOME_SALVAGE_WARN_TICKS` left.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub salvage_despawn_tick: Option<u64>,
    /// The power the raid was announced with, and what stood in its way
    /// when it arrived on the same scale. Neither decided anything.
    pub raid_power: f32,
    pub defense_power: f32,
    /// What the raid flew in with and what it lost, by class.
    #[serde(default)]
    pub raiders: Vec<ShipCount>,
    #[serde(default)]
    pub raiders_lost: Vec<ShipCount>,
    /// Everything the defender lost in the fight, by unit: ships, Solar
    /// Satellites and defences, the satellites and the defences before any
    /// is rebuilt.
    #[serde(default)]
    pub lost: Vec<UnitCount>,
    /// The Solar Satellites and the defences rebuilt at once and for
    /// nothing when the fight ended, by unit, the satellites first: about
    /// `DEFENCE_REBUILD_PCT` in a hundred of those that fell. Ships are not
    /// rebuilt, and nothing is at an outpost.
    #[serde(default)]
    pub rebuilt: Vec<UnitCount>,
    /// Solar Satellites destroyed in the fight, before any is rebuilt; how
    /// many came back is their row of `rebuilt`.
    #[serde(default)]
    pub satellites_lost: u32,
    /// Defences destroyed, and how many of them were rebuilt: the sums of
    /// `lost` and of `rebuilt` over the defences.
    #[serde(default)]
    pub structures_lost: u32,
    #[serde(default)]
    pub structures_restored: u32,
    /// What the storage buildings kept out of reach of a lost raid. At an
    /// outpost, what its protected floor kept.
    #[serde(default)]
    pub protected_kept: Resources,
    /// Ships of the owner's fleets destroyed in the fight.
    #[serde(default)]
    pub ships_lost: u32,
}

/// A message for the player. `player_id` names the recipient; `None` is a
/// server-wide notice. Losing a fleet raises a `Critical` alert.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertEvent {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub player_id: Option<u64>,
    pub level: AlertLevel,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub sector: Option<Hex>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub fleet_id: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AlertLevel {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorMessage {
    pub code: ErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum ErrorCode {
    InvalidCommand = 1000,
    InvalidTarget = 1001,
    NoConnection = 1002,
    InsufficientFuel = 1003,
    OnCooldown = 1004,
    FleetNotFound = 1005,
    NotInSector = 1006,
    NoResources = 1007,
    CargoFull = 1008,
    PrerequisitesNotMet = 1009,
    MaxLevelReached = 1010,
    QueueFull = 1011,
    ShipLocked = 1012,
    NoShipyard = 1013,
    NoResearchLab = 1014,
    FleetLimitReached = 1015,
    ResourceNotPresent = 1016,
    StorageTooSmall = 1017,
    DefenceLocked = 1018,
    InsufficientResources = 1019,
    AuthFailed = 2000,
    AlreadyAuthenticated = 2001,
    InvalidName = 2002,
    TokenRequired = 2003,
    InvalidToken = 2004,
    WorldFull = 2005,
    ServerError = 5000,
}
