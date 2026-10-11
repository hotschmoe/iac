// Wire protocol types shared between server and client.
// All messages are JSON-serialized over WebSocket.

use serde::{Deserialize, Serialize};

use crate::constants::{Density, ResourceKind, ShipClass, TerrainType};
use crate::scaling::{
    self, BuildingLevels, BuildingType, DefenceKind, PrereqRef, RatioLabel, ResearchLevels, ResearchPrereqKind,
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
    /// Order `count` defence structures in the shipyard queue; same rule as
    /// `build_ship`. They stay at home and never burn fuel; rejected with
    /// `DefenceLocked` until the Defense Grid and research it needs are in place.
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
    /// Detach the listed ships from `fleet_id` into a new fleet at the same
    /// sector. The new fleet starts with no standing orders and takes a share
    /// of fuel and cargo proportional to the tank and hold it carries off.
    /// At least one ship must stay behind. Both fleets must be idle. Rejected
    /// with `FleetLimitReached` at the fleet cap, `InvalidTarget` for ship ids
    /// not in the fleet or an attempt to take every ship. At the homeworld
    /// this is also how docked ships are launched as their own fleet. The
    /// reply is an `Alert` (info) event naming the new fleet id.
    #[serde(rename = "split")]
    Split { fleet_id: u64, ship_ids: Vec<u64> },
    /// Fold `other_fleet_id` into `fleet_id`. Both must be yours, idle, and in
    /// the same sector (`NotInSector` otherwise). Ships, cargo and fuel are
    /// pooled (fuel up to the combined tank), the merged fleet keeps the
    /// standing orders of `fleet_id`, and `other_fleet_id` ceases to exist.
    /// This works anywhere, so a fresh fleet can rescue a stranded one.
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
        amount: Option<f32>,
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
        amount: Option<f32>,
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
        amount: Option<f32>,
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
}

impl Default for PolicyParams {
    fn default() -> Self {
        PolicyParams {
            min_fuel_pct: crate::constants::POLICY_DEFAULT_MIN_FUEL_PCT,
            cargo_return_pct: crate::constants::POLICY_DEFAULT_CARGO_RETURN_PCT,
            max_range: crate::constants::POLICY_DEFAULT_MAX_RANGE,
            engage_ratio_x10: crate::constants::POLICY_DEFAULT_ENGAGE_RATIO_X10,
            fuel_budget_pct: crate::constants::POLICY_DEFAULT_FUEL_BUDGET_PCT,
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
    /// Why `found_outpost` would be refused right now; absent when the
    /// fleet could found here this tick.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub refusal: Option<String>,
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
    /// Lane hops from the sector to the homeworld; absent beyond 64.
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
    pub amount: f32,
    /// Units of `want` that come back: whole units, rounded down.
    pub receive: f32,
    /// Units of `want` the same `amount` is worth at par, with no fee.
    pub par: f32,
    /// The share of par the exchange keeps on this trade, `1 - receive / par`.
    pub fee: f32,
    /// The player's meter before the trade.
    pub meter: ExchangeMeter,
    /// And after it.
    pub meter_after: ExchangeMeter,
}

/// A player's standing at the exchange.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ExchangeMeter {
    /// Weight sold and not yet forgotten.
    pub pressure: f32,
    /// The pressure at which the fee stops rising: 15 percent of what the
    /// player's own mines make in one window, in weight units. It grows
    /// with the mines; 0 for an empire with no mine output, which pays the
    /// top fee.
    pub span: f32,
    /// The fee on the next unit of weight sold, 0.25 to 0.75.
    pub fee: f32,
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

/// The score table: one ranking for every player, human or agent.
/// `score = core + min(combat + explore, 0.25 * core)`, where `core` counts
/// completed building and research levels, living ships and half of the
/// defence structures, each at its cost weighted 1 : 1.5 : 2 for
/// metal : crystal : deuterium, per thousand.
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
    pub core: f32,
    /// Raw combat points, before the 25 percent cap.
    pub combat: f32,
    /// Raw exploration points, before the 25 percent cap.
    pub explore: f32,
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
    pub fuel_cost: f32,
    /// Fuel left afterwards (0 when the fleet cannot afford the jump).
    pub fuel_after: f32,
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
    pub fuel_to_return: f32,
    /// After the jump the fleet still holds enough fuel to get home.
    pub can_return: bool,
    pub threat: ThreatInfo,
    /// Combat power of the whole fleet, on the scale of `threat.est_power`.
    /// The two do not divide into `ratio`: that weighs the ship classes.
    pub fleet_power: f32,
    pub ratio: f32,
    pub label: RatioLabel,
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
/// The server plays the fight on averages, with the ships on each side:
/// rapid fire between classes, each shot against hull and shield, the fleet
/// firing first. `ratio` puts the expected result on the scale of a power
/// ratio between equal ship classes (1.0 is an even match, 0.1 to 10), and
/// `label` is its band: at least 2.5 is safe (99 percent of the hull kept or
/// more), 1.5 favourable (86 to 99), 1.1 even (a win that keeps 53 to 86),
/// 0.9 risky (anything from a loss to a win with half the hull), below that
/// deadly. A sector with nothing in sight is rated against the group its
/// ring spawns. Standing orders use the same number (`engage_ratio_x10`).
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
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub events: Option<Vec<GameEvent>>,
}

/// Full snapshot, sent on login and on `request_full_state`.
///
/// `known_sectors` is the player's whole chart: every sector the player has
/// ever had eyes on (fleet present, sensor array, active scan), not only the
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
    pub known_sectors: Vec<SectorState>,
    pub world: WorldInfo,
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
/// (`RaidIncoming` and `RaidResolved` carry `outpost`). A lost raid takes a
/// quarter of what lies above `protected` in the store and in the holds of
/// fleets standing there, destroys a quarter of those ships, and breaches
/// the outpost. No raid destroys it, and a fleet standing there is not safe.
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
    /// What a raid cannot take: the repair cost of the level it stands at,
    /// or of the level it is building, so the materials of its next repair
    /// are never raided.
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
    /// What the platform defends with; 0 while breached or unfinished.
    pub platform_power: f32,
    /// The platform and the owner's ships standing in the sector: what the
    /// next raid meets.
    pub defence_power: f32,
    /// The least and the most a raid on this outpost can be. The outcome
    /// is `defence_power`, taken a tenth higher or lower by chance, against
    /// the raid's power.
    pub raid_power: PowerRange,
    /// Tick an announced raid arrives; absent when none is on its way.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub raid_arrival_tick: Option<u64>,
    /// What `upgrade_outpost` would build; absent at `max_level`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub next_level: Option<OutpostLevel>,
    /// Why `upgrade_outpost` would be refused right now, in the sentence
    /// the refusal would carry; absent when it would be accepted this tick.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub upgrade_refusal: Option<String>,
    /// What the repair will take from the store once the store covers it;
    /// present while breached with no repair under way.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub repair_cost: Option<Resources>,
    /// What the store lacks of `repair_cost`: the repair waits for a fleet
    /// to unload it. Absent when no repair is waiting.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub repair_waiting_for: Option<Resources>,
    /// Lane hops to the homeworld; absent beyond 64.
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
    /// Where the player's meter at the hub's exchange stands this tick.
    #[serde(default)]
    pub exchange: ExchangeMeter,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetState {
    pub id: u64,
    pub location: Hex,
    pub state: FleetStatus,
    pub ships: Vec<ShipState>,
    pub cargo: Resources,
    /// Hold size: the most `cargo` can total, as the server enforces it.
    pub cargo_capacity: f32,
    pub fuel: f32,
    pub fuel_max: f32,
    /// Fuel one jump costs this fleet right now (it scales with the fleet's
    /// hull mass and fuel research). A fleet with less than this and away
    /// from home is stranded and only regains fuel from the emergency
    /// reserve, one jump per five minutes.
    #[serde(default)]
    pub jump_fuel: f32,
    /// Fuel the way home costs: the fewest hops over lanes the player has
    /// charted, times `jump_fuel`; 0 at the homeworld. Compare with `fuel`.
    #[serde(default)]
    pub home_fuel: f32,
    #[serde(default)]
    pub cooldown_remaining: u16,
    /// Combat power of the whole fleet on the scale raids and threat ratings
    /// share: each ship counts `weapon + (hull + shield) / 10`.
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
    /// here, and a raid on it finds the fleet.
    #[serde(default)]
    pub at_outpost: bool,
    /// Fuel the way to the owner's outpost costs: the fewest lane hops
    /// there times `jump_fuel`, 0 at the outpost. Present while that
    /// outpost is in service, which is when it fills a tank.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub outpost_fuel: Option<f32>,
    /// Standing orders currently flying this fleet, if any: the doctrine
    /// with its settings, so the two cannot be told apart or disagree.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub policy: Option<StandingOrders>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipState {
    pub id: u64,
    #[serde(rename = "class")]
    pub ship_class: ShipClass,
    pub hull: f32,
    pub hull_max: f32,
    pub shield: f32,
    pub shield_max: f32,
    pub weapon_power: f32,
}

/// One sector as the player knows it.
///
/// `live: true` means the server is watching the sector this tick (a fleet
/// is there, the sensor array covers it, or a scan has not expired) and every
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

/// A sector's chart is made by entering it and pays explore points once.
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
    pub units: f32,
    /// Units the tile holds when full (0 for a sector without this ore).
    pub max_units: f32,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NpcFleetInfo {
    pub id: u64,
    pub ships: Vec<NpcShipInfo>,
    pub behavior: NpcBehavior,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NpcShipInfo {
    #[serde(rename = "class")]
    pub ship_class: ShipClass,
    pub count: u16,
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
    pub ship_count: u16,
    #[serde(default)]
    pub ship_classes: ShipClassCounts,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ShipClassCounts {
    pub scout: u16,
    pub corvette: u16,
    pub frigate: u16,
    pub cruiser: u16,
    pub hauler: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomeworldState {
    pub location: Hex,
    /// What the mines make each tick at current levels (before the storage
    /// cap discards the excess; see `storage`).
    pub production: Resources,
    pub storage: StorageState,
    pub buildings: Vec<BuildingState>,
    pub research: Vec<ResearchState>,
    /// Buildings under construction, at most `build_slots` of them.
    pub build_queue: Vec<BuildQueueItem>,
    /// Waiting behind them; nothing is paid until an item starts.
    pub build_pending: Vec<QueuedBuild>,
    /// How many buildings may be under construction at once.
    pub build_slots: u8,
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
    pub docked_ships: Vec<ShipState>,
    /// Structures standing at home (those being rebuilt after a raid are
    /// counted in `restoring`, not `count`).
    pub defences: Vec<DefenceState>,
    /// Docked ships, the Defense Grid and all structures together: what the
    /// next raid meets.
    pub home_defence_power: f32,
    /// Typical power of the next raid, from the empire's building and
    /// research points; the actual raid rolls 0.80 to 1.15 times this.
    pub next_raid_estimate_power: f32,
    /// What can be queued right now and what it costs; see `HomeworldCatalog`.
    pub catalog: HomeworldCatalog,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DefenceState {
    pub kind: DefenceKind,
    pub count: u32,
    /// Structures a repelled raid destroyed that come back for free.
    pub restoring: u32,
    /// Power of one structure at the current Defense Grid level.
    pub power: f32,
}

/// Stockpile limits, from the Storage Vault level and the world pace.
/// Mines stop at `cap`; docking cargo unloads only up to it.
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
/// balance formulas: costs, build times and prerequisites, computed by the
/// server from `scaling.rs` for the player's current levels. Resent with
/// every `HomeworldState`, so it tracks level changes the tick they land.
/// Whether the player can pay is left to the client (compare `cost` with the
/// stockpile in `PlayerState.resources`).
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

/// One prerequisite: a building or tech that must reach `need`, with the
/// player's current level. The server's "prerequisites not met" errors are
/// written from the same values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Requirement {
    /// Building or tech label ("Shipyard", "Frigate Tech").
    pub name: String,
    pub need: u8,
    pub have: u8,
    pub met: bool,
}

impl Requirement {
    fn new(name: &str, need: u8, have: u8) -> Self {
        Requirement { name: name.to_string(), need, have, met: have >= need }
    }

    /// Compact form for cards: "Shipyard >= 4".
    pub fn label(&self) -> String {
        format!("{} >= {}", self.name, self.need)
    }
}

/// "Frigate needs Shipyard level 4 (you have 2) and Frigate Tech level 1
/// (you have 0)"; None when every requirement is met.
pub fn missing_requirements_message(subject: &str, requires: &[Requirement]) -> Option<String> {
    let parts: Vec<String> = requires
        .iter()
        .filter(|r| !r.met)
        .map(|r| format!("{} level {} (you have {})", r.name, r.need, r.have))
        .collect();
    let (last, rest) = parts.split_last()?;
    let list = if rest.is_empty() { last.clone() } else { format!("{} and {}", rest.join(", "), last) };
    Some(format!("{subject} needs {list}"))
}

fn building_req(levels: &BuildingLevels, b: BuildingType, need: u8) -> Requirement {
    Requirement::new(b.label(), need, levels.get(b))
}

fn tech_req(levels: &ResearchLevels, t: ResearchType, need: u8) -> Requirement {
    Requirement::new(t.label(), need, levels.get(t))
}

/// Everything standing between the player and the next level of `b`.
pub fn building_requirements(levels: &BuildingLevels, b: BuildingType) -> Vec<Requirement> {
    scaling::building_prerequisites(b)
        .map(|p| building_req(levels, p.building, p.level))
        .into_iter()
        .collect()
}

/// Prerequisites for researching `t`, including the Research Lab the server
/// requires before any research.
pub fn research_requirements(buildings: &BuildingLevels, research: &ResearchLevels, t: ResearchType) -> Vec<Requirement> {
    let prereqs: Vec<_> = scaling::research_prerequisites(t).into_iter().flatten().collect();
    // Every tech needs a lab; skip the generic entry when the tech names a lab level itself.
    let names_lab = prereqs.iter().any(|p| {
        matches!(p, ResearchPrereqKind::Building(b) if b.building == BuildingType::ResearchLab)
    });
    let mut requires = Vec::new();
    if !names_lab {
        requires.push(building_req(buildings, BuildingType::ResearchLab, 1));
    }
    for p in prereqs {
        requires.push(match p {
            ResearchPrereqKind::Building(b) => building_req(buildings, b.building, b.level),
            ResearchPrereqKind::Research { tech, level } => tech_req(research, tech, level),
        });
    }
    requires
}

/// Prerequisites for building `c`: a Shipyard plus the class's unlock tech.
pub fn ship_requirements(buildings: &BuildingLevels, research: &ResearchLevels, c: ShipClass) -> Vec<Requirement> {
    let mut requires = vec![building_req(buildings, BuildingType::Shipyard, 1)];
    requires.extend(scaling::ship_class_tech(c).map(|t| tech_req(research, t, 1)));
    requires
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShipOption {
    #[serde(rename = "ship_class")]
    pub ship_class: ShipClass,
    /// Cost of one ship; a batch of `count` costs `count` times this.
    pub unit_cost: Resources,
    /// Ticks to build one ship at the current shipyard level.
    pub ticks_per_ship: u64,
    /// Includes the Shipyard the server requires before any ship.
    pub requires: Vec<Requirement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DefenceOption {
    pub kind: DefenceKind,
    /// Standing now.
    pub count: u32,
    pub unit_cost: Resources,
    pub ticks_per_unit: u64,
    /// Power of one structure at the current Defense Grid level.
    pub power: f32,
    /// Includes the Shipyard the server requires before anything is built.
    pub requires: Vec<Requirement>,
}

/// Prerequisites for a defence structure: a Shipyard plus its own list.
pub fn defence_requirements(buildings: &BuildingLevels, research: &ResearchLevels, kind: DefenceKind) -> Vec<Requirement> {
    let mut requires = vec![building_req(buildings, BuildingType::Shipyard, 1)];
    for (p, need) in kind.prerequisites() {
        requires.push(match p {
            PrereqRef::Building(b) => building_req(buildings, *b, *need),
            PrereqRef::Research(t) => tech_req(research, *t, *need),
        });
    }
    requires
}

impl HomeworldCatalog {
    pub fn new(
        buildings: &BuildingLevels,
        research: &ResearchLevels,
        defences: &[u32; DefenceKind::COUNT],
        pace: &Pace,
    ) -> Self {
        let building_options = (0..BuildingType::COUNT)
            .filter_map(BuildingType::from_usize)
            .map(|b| {
                let level = buildings.get(b);
                BuildingOption {
                    building_type: b,
                    level,
                    max_level: scaling::MAX_BUILDING_LEVEL,
                    next: (level < scaling::MAX_BUILDING_LEVEL).then(|| UpgradeStep {
                        level: level + 1,
                        cost: scaling::building_cost(b, level + 1),
                        ticks: scaling::building_time(b, level + 1, buildings.fabricator, pace),
                    }),
                    requires: building_requirements(buildings, b),
                }
            })
            .collect();

        let research_options = ResearchType::ALL
            .iter()
            .map(|&t| {
                let level = research.get(t);
                let max_level = scaling::research_max_level(t);
                ResearchOption {
                    tech: t,
                    level,
                    max_level,
                    next: (level < max_level).then(|| UpgradeStep {
                        level: level + 1,
                        cost: scaling::research_cost(t, level + 1),
                        ticks: scaling::research_time(t, level + 1, buildings.research_lab, pace),
                    }),
                    requires: research_requirements(buildings, research, t),
                }
            })
            .collect();

        let ship_options = ShipClass::ALL
            .iter()
            .map(|&c| ShipOption {
                ship_class: c,
                unit_cost: c.build_cost(),
                ticks_per_ship: scaling::ship_build_time(c, buildings.get(BuildingType::Shipyard), pace),
                requires: ship_requirements(buildings, research, c),
            })
            .collect();

        let grid = buildings.get(BuildingType::DefenseGrid);
        let defence_options = DefenceKind::ALL
            .iter()
            .map(|&kind| DefenceOption {
                kind,
                count: defences[kind as usize],
                unit_cost: kind.build_cost(),
                ticks_per_unit: scaling::defence_build_time(kind, buildings.get(BuildingType::Shipyard), pace),
                power: scaling::structure_power(kind, grid),
                requires: defence_requirements(buildings, research, kind),
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
    CombatRound(CombatRoundEvent),
    ShipDestroyed(ShipDestroyedEvent),
    FleetDestroyed(FleetDestroyedEvent),
    ResourceHarvested(ResourceHarvestedEvent),
    SectorEntered(SectorEnteredEvent),
    CombatStarted(CombatStartedEvent),
    CombatEnded(CombatEndedEvent),
    SalvageCollected(SalvageCollectedEvent),
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
    /// "Metal Mine Lv.5", "Corvette x3".
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
    pub ratio: f32,
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
    /// Loot taken aboard (cargo-capped; overflow drops as sector salvage).
    pub resources: Resources,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub recovered_ship: Option<ShipClass>,
    /// Instant tech level granted by a recovered data core.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tech_cache: Option<ResearchType>,
    /// An ancient relic came aboard (worth 25 explore points; no other use yet).
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub relic: bool,
    /// Explore points this boarding earned (0 when the site was boarded before).
    #[serde(skip_serializing_if = "is_zero", default)]
    pub points: f32,
}

fn is_zero(v: &f32) -> bool {
    *v == 0.0
}

/// A fleet docked at the homeworld, or arrived at its owner's finished
/// outpost, and handed over the sectors it charted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartDeliveredEvent {
    pub fleet_id: u64,
    /// Sectors newly credited to the empire.
    pub sectors: u32,
    /// Explore points they earned.
    pub points: f32,
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

/// One shot in a fight.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombatRoundEvent {
    pub sector: Hex,
    pub attacker_fleet_id: u64,
    /// Empire owning the attacking fleet; `None` for an NPC.
    pub attacker_owner: Option<String>,
    pub attacker_ship_id: u64,
    pub target_fleet_id: u64,
    /// Empire owning the target fleet; `None` for an NPC.
    pub target_owner: Option<String>,
    pub target_ship_id: u64,
    pub damage: f32,
    pub shield_absorbed: f32,
    pub hull_damage: f32,
    pub rapid_fire: bool,
    /// One of the two fleets is the receiver's.
    #[serde(default)]
    pub mine: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipDestroyedEvent {
    pub ship_id: u64,
    #[serde(rename = "ship_class")]
    pub ship_class: ShipClass,
    pub owner_fleet_id: u64,
    pub is_npc: bool,
    pub sector: Hex,
    /// Empire that owned the ship; `None` for an NPC.
    pub owner: Option<String>,
    /// The ship was the receiver's.
    #[serde(default)]
    pub mine: bool,
}

/// Every ship of a fleet is gone. For an NPC fleet killed by players,
/// `salvage` is the wreckage it left in `sector`: exactly what
/// `collect_salvage` can take (when several NPC fleets die in one fight the
/// sector pile is the sum of their events). Player fleets leave no wreckage,
/// so `salvage` is zero. A destroyed fleet of the receiver's also produces a
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
    /// What an NPC group was ("3x corvette pack"); empty for a player fleet.
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
    /// What the hostile group is ("3x corvette pack").
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub enemy: String,
    pub sector: Hex,
    /// `player_fleet_id` is the receiver's.
    #[serde(default)]
    pub mine: bool,
}

/// `owners` lists the empires that fought. `salvage` is the pile the victory
/// left in the sector (absent on defeat); it despawns at `salvage_despawn_tick`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombatEndedEvent {
    pub sector: Hex,
    pub player_victory: bool,
    pub owners: Vec<String>,
    #[serde(default)]
    pub mine: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub salvage: Option<Resources>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub salvage_despawn_tick: Option<u64>,
}

/// A fleet scooped wreckage. `resources` is what went into the hold; if the
/// hold filled first, `remaining` is what is still on the ground (it keeps
/// its original despawn time). Absent when the pile is gone.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SalvageCollectedEvent {
    /// The collecting fleet. For the automatic scoop of a pile on the
    /// player's own homeworld, the lowest-id fleet docked there.
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
/// toward the player's outpost when `outpost` names its sector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaidIncomingEvent {
    pub player_id: u64,
    pub arrival_tick: u64,
    /// Rough size read against the defence standing now: "light" (under
    /// half of it), "moderate" (under all of it) or "heavy".
    pub threat: String,
    /// The raid fleet's power, fixed at forecast time, on the same scale as
    /// `home_defence_power` and `OutpostState.defence_power`.
    #[serde(default)]
    pub est_power: f32,
    /// The outpost the raid is bound for; absent for a raid on the homeworld.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub outpost: Option<Hex>,
}

/// Outcome of a raid on the homeworld, or on the player's outpost when
/// `outpost` names its sector. An outpost has no structures and a repelled
/// raid on one leaves no salvage, so those fields are zero and absent then.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaidResolvedEvent {
    pub player_id: u64,
    /// The outpost that was raided; absent for a raid on the homeworld.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub outpost: Option<Hex>,
    pub defended: bool,
    /// The raid broke the outpost: it makes nothing, fills no tank and has
    /// no platform until it has repaired itself from its store. Always
    /// false for a raid on the homeworld.
    #[serde(default)]
    pub breached: bool,
    /// What a lost raid took: its share of the stockpile and of the cargo
    /// aboard fleets at home, counted as one pile above the protected
    /// floor. At an outpost, the same of its store and the fleets there.
    pub resources_lost: Resources,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub salvage_dropped: Option<Resources>,
    /// Tick the pile on the homeworld despawns (absent when none dropped).
    /// A fleet docked at home scoops it into storage on its own; without
    /// one, an alert warns when it has `HOME_SALVAGE_WARN_TICKS` left.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub salvage_despawn_tick: Option<u64>,
    pub raid_power: f32,
    pub defense_power: f32,
    /// Structures the raid destroyed.
    #[serde(default)]
    pub structures_lost: u32,
    /// Of those, how many are rebuilt for free after a short delay.
    #[serde(default)]
    pub structures_restored: u32,
    /// What the Storage Vault kept out of reach of a successful raid, of
    /// the stockpile and the cargo aboard fleets at home together. At an
    /// outpost, what its protected floor kept.
    #[serde(default)]
    pub protected_kept: Resources,
    /// Ships docked at home, or standing at the outpost, that a lost raid
    /// destroyed.
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
