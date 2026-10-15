// The Dart client's wire types, generated from the Rust ones.
//
// `model` reads the types out of shared/src, `samples` proves that reading
// against serde and learns serde's defaults, `dart` writes the Dart file.
// This module holds what neither the source text nor serde can tell: the
// roots of the wire, and the Rust functions and constants the client is
// handed as columns of its enums and as constants.

pub mod dart;
pub mod model;
pub mod samples;

use std::collections::BTreeMap;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use iac_shared::Resources;
use iac_shared::constants::{
    CHASE_MIN_RATIO, DEFENCE_REBUILD_EACH_BELOW, DEFENCE_REBUILD_PCT, DEFENCE_REBUILD_SPREAD, Density, Drive,
    EXPLODE_BELOW_PCT,
    EXPLORE_DURATION_TICKS, FLEE_FUEL_MULTIPLIER, FLEE_MIN_ROUNDS, FUEL_DEUT_PER_UNIT, LANE_SEARCH_HOPS,
    MAX_FLEET_SHIPS, OUTPOST_MAX_RING, OUTPOST_MIN_RING, POLICY_CARGO_RETURN_PCT_MAX, RELAY_MIN_RING, RELAY_STORE, POLICY_CARGO_RETURN_PCT_MIN,
    POLICY_DISENGAGE_RATIO_X10_MAX, POLICY_DISENGAGE_RATIO_X10_MIN, POLICY_ENGAGE_RATIO_X10_MAX,
    POLICY_ENGAGE_RATIO_X10_MIN, POLICY_FUEL_UNBOUNDED_PCT, POLICY_HULL_READY, POLICY_MIN_FUEL_PCT_MAX,
    POLICY_RANGE_MAX, POLICY_RANGE_MIN, RAID_ROUNDS, REFINE_LOSS, ResourceKind, SCAN_BASE_RANGE, SCAN_COOLDOWN,
    SCAN_PROBE_RANGE, ShipClass, TerrainType, Zone,
};
use iac_shared::protocol::{
    BOT_NAME_PREFIX, ClientMessage, EffectKind, ErrorCode, ExchangeMeter, MAX_ODDS_SECTORS, NAME_MAX_LEN, RelaysState,
    PolicyParams, PolicyPreset, RaidLabel, ServerMessage, SignalKind, SiteRisk, StockpileCause,
};
use iac_shared::scaling::{
    BuildingType, DefenceKind, MAX_THREAT, RATIO_EVEN, RATIO_FAVOURABLE, RATIO_RISKY, RATIO_SAFE, RatioLabel,
    ResearchType, SITE_GRADE_MAX, STORAGE_NEAR_CAP_RATIO, ship_power, storage_building,
};

use model::{Body, Model};

/// Every wire type is one of these or is carried by one.
pub const ROOTS: [&str; 2] = ["ClientMessage", "ServerMessage"];

/// What serde makes of `input` read as `root` and written again.
pub fn reemit(root: &str, input: &Value) -> Result<Value, String> {
    fn through<T: Serialize + DeserializeOwned>(input: &Value) -> Result<Value, String> {
        let parsed: T = serde_json::from_value(input.clone()).map_err(|e| e.to_string())?;
        serde_json::to_value(&parsed).map_err(|e| e.to_string())
    }
    match root {
        "ClientMessage" => through::<ClientMessage>(input),
        "ServerMessage" => through::<ServerMessage>(input),
        other => panic!("{other} is not a root of the wire"),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    Text(String),
    Int(i64),
    Real(f64),
}

impl From<&str> for Cell {
    fn from(text: &str) -> Cell {
        Cell::Text(text.to_string())
    }
}

impl From<u8> for Cell {
    fn from(n: u8) -> Cell {
        Cell::Int(n.into())
    }
}

impl From<u16> for Cell {
    fn from(n: u16) -> Cell {
        Cell::Int(n.into())
    }
}

impl From<u32> for Cell {
    fn from(n: u32) -> Cell {
        Cell::Int(n.into())
    }
}

impl From<usize> for Cell {
    fn from(n: usize) -> Cell {
        Cell::Int(i64::try_from(n).unwrap())
    }
}

/// Widened exactly: a ratio read off the wire is an `f32` widened the same
/// way, so a comparison in Dart comes out as it does in Rust.
impl From<f32> for Cell {
    fn from(x: f32) -> Cell {
        Cell::Real(x.into())
    }
}

/// As it is: an amount, and what scales one, is an `f64` in Rust and in Dart.
impl From<f64> for Cell {
    fn from(x: f64) -> Cell {
        Cell::Real(x)
    }
}

/// One value for each variant of an enum, in declaration order.
pub struct Column {
    pub name: &'static str,
    pub cells: Vec<Cell>,
}

/// A constant of shared/src under its Rust name, an associated one as `Type::NAME`.
pub struct Constant {
    pub rust: &'static str,
    pub cell: Cell,
}

pub struct Extras {
    pub columns: Vec<(String, Column)>,
    pub constants: Vec<Constant>,
    /// `T::default()` of every wire struct that has one, as serde writes it.
    pub rust_defaults: BTreeMap<String, Value>,
}

macro_rules! constants {
    ($($name:path),* $(,)?) => {
        vec![$(Constant { rust: stringify!($name), cell: Cell::from($name) }),*]
    };
}

pub fn extras(model: &Model) -> Extras {
    let columns = vec![
        column(model, "label", ShipClass::label),
        column(model, "hull", |class: ShipClass| class.base_stats().hull),
        column(model, "shield", |class: ShipClass| class.base_stats().shield),
        column(model, "weapon", |class: ShipClass| class.base_stats().weapon),
        column(model, "cargo", |class: ShipClass| f64::from(class.base_stats().cargo)),
        column(model, "power", ship_power),
        column(model, "speed", |class: ShipClass| f64::from(class.base_stats().speed)),
        column(model, "fuel", |class: ShipClass| f64::from(class.base_stats().fuel)),
        column(model, "tankJumps", |class: ShipClass| f64::from(class.tank_jumps())),
        column(model, "harvest", ShipClass::harvest_power),
        column(model, "drive", |class: ShipClass| class.drive().map_or("", Drive::label)),
        column(model, "label", Drive::label),
        column(model, "speedPerLevel", Drive::speed_per_level),
        column(model, "label", Density::label),
        column(model, "label", TerrainType::label),
        column(model, "label", ResourceKind::label),
        column(model, "weight", ResourceKind::weight),
        column(model, "storage", |kind: ResourceKind| storage_building(kind).label()),
        column(model, "label", BuildingType::label),
        column(model, "label", ResearchType::label),
        column(model, "label", DefenceKind::label),
        column(model, "hull", |kind: DefenceKind| kind.base_stats().hull),
        column(model, "shield", |kind: DefenceKind| kind.base_stats().shield),
        column(model, "weapon", |kind: DefenceKind| kind.base_stats().weapon),
        column(model, "power", DefenceKind::power),
        column(model, "label", EffectKind::label),
        column(model, "label", RatioLabel::label),
        column(model, "outcome", RatioLabel::outcome),
        column(model, "label", RaidLabel::label),
        column(model, "outcome", RaidLabel::outcome),
        column(model, "phrase", StockpileCause::phrase),
        column(model, "label", PolicyPreset::label),
        column(model, "label", SiteRisk::label),
        column(model, "label", SignalKind::label),
        column(model, "code", |code: ErrorCode| code as u16),
    ];
    let constants = constants![
        BOT_NAME_PREFIX,
        MAX_ODDS_SECTORS,
        RATIO_SAFE,
        RATIO_FAVOURABLE,
        RATIO_EVEN,
        RATIO_RISKY,
        OUTPOST_MIN_RING,
        OUTPOST_MAX_RING,
        RELAY_MIN_RING,
        RELAY_STORE,
        NAME_MAX_LEN,
        Zone::INNER_RING_RADIUS,
        Zone::OUTER_RING_RADIUS,
        MAX_THREAT,
        SITE_GRADE_MAX,
        LANE_SEARCH_HOPS,
        STORAGE_NEAR_CAP_RATIO,
        SCAN_BASE_RANGE,
        SCAN_PROBE_RANGE,
        SCAN_COOLDOWN,
        EXPLORE_DURATION_TICKS,
        POLICY_MIN_FUEL_PCT_MAX,
        POLICY_CARGO_RETURN_PCT_MIN,
        POLICY_CARGO_RETURN_PCT_MAX,
        POLICY_RANGE_MIN,
        POLICY_RANGE_MAX,
        POLICY_ENGAGE_RATIO_X10_MIN,
        POLICY_ENGAGE_RATIO_X10_MAX,
        POLICY_FUEL_UNBOUNDED_PCT,
        FUEL_DEUT_PER_UNIT,
        REFINE_LOSS,
        RAID_ROUNDS,
        DEFENCE_REBUILD_PCT,
        DEFENCE_REBUILD_SPREAD,
        DEFENCE_REBUILD_EACH_BELOW,
        MAX_FLEET_SHIPS,
        EXPLODE_BELOW_PCT,
        POLICY_HULL_READY,
        POLICY_DISENGAGE_RATIO_X10_MIN,
        POLICY_DISENGAGE_RATIO_X10_MAX,
        FLEE_MIN_ROUNDS,
        FLEE_FUEL_MULTIPLIER,
        CHASE_MIN_RATIO,
    ];
    let rust_defaults = BTreeMap::from([
        rust_default::<Resources>(),
        rust_default::<PolicyParams>(),
        rust_default::<ExchangeMeter>(),
        rust_default::<RelaysState>(),
    ]);

    for def in model.types.iter().filter(|def| matches!(def.body, Body::Struct(_))) {
        assert_eq!(
            def.rust_default,
            rust_defaults.contains_key(&def.name),
            "{}: a wire struct has Default in Rust exactly when `extras` lists its default",
            def.name
        );
    }
    Extras { columns, constants, rust_defaults }
}

fn short_name<T>() -> String {
    std::any::type_name::<T>().rsplit("::").next().unwrap().to_string()
}

fn column<T: DeserializeOwned, C: Into<Cell>>(
    model: &Model,
    name: &'static str,
    read: impl Fn(T) -> C,
) -> (String, Column) {
    let owner = short_name::<T>();
    let Body::Names(variants) = &model.get(&owner).body else {
        panic!("{owner}: only an enum of unit variants takes a column");
    };
    let cells = variants
        .iter()
        .map(|v| {
            let variant: T = serde_json::from_value(json!(v.wire))
                .unwrap_or_else(|e| panic!("{owner}: serde does not read \"{}\": {e}", v.wire));
            read(variant).into()
        })
        .collect();
    (owner, Column { name, cells })
}

fn rust_default<T: Default + Serialize>() -> (String, Value) {
    (short_name::<T>(), serde_json::to_value(T::default()).unwrap())
}
