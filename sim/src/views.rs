// What a player is allowed to see: the protocol's full-state and per-tick
// views of the engine, and which events reach whom.

use iac_shared::constants::{
    ResourceKind, Resources, LANE_SEARCH_HOPS, OUTPOST_MAX_LEVEL, RAID_POWER_ROLL_MAX, RAID_POWER_ROLL_MIN,
};
use iac_shared::hex::Hex;
use iac_shared::protocol::{
    BuildQueueItem, BuildingState, DefenceState, FleetState, GameEvent, GameState, HomeworldState, OutpostLevel,
    OutpostState, OutpostWork, OutpostWorkState, PlayerState, QueuedBuild, QueuedResearch, QueuedShip,
    ResearchItem, ResearchState, ShipState, ShipyardQueueItem, StandingOrders, TickUpdate, WorldInfo,
};
use iac_shared::scaling::{self, BuildingType, DefenceKind, ResearchType};

use crate::engine::{fleet_cargo_capacity, outpost_raid_range, FleetStatus, GameEngine, OutpostTask, Player};

/// The whole picture for one player, as sent on login and on request.
pub fn full_state(engine: &GameEngine, player_id: u64) -> Option<GameState> {
    let player = engine.players.get(&player_id)?;
    Some(GameState {
        tick: engine.current_tick(),
        player: player_state(engine, player),
        fleets: collect_player_fleets(engine, player_id),
        homeworld: build_homeworld_state(engine, player),
        outpost: outpost_state(engine, player),
        known_sectors: engine.known.chart(engine, player_id),
        world: WorldInfo::new(engine.pace(), engine.world.economy_version, engine.world.worldgen_version),
    })
}

/// One player's per-tick delta. `events` is the engine's drained event list
/// (see `GameEngine::drain_events`); only those that concern the player are kept.
pub fn tick_update(engine: &GameEngine, player_id: u64, events: &[GameEvent]) -> TickUpdate {
    let player_data = engine.players.get(&player_id);
    let sector_updates = engine.known.tick_updates(engine, player_id);

    let player = player_data.map(|p| player_state(engine, p));
    let homeworld_update = player_data.map(|p| build_homeworld_state(engine, p));
    let player_events: Vec<GameEvent> = player_data
        .map(|p| events.iter().filter_map(|e| event_for(e, p, engine)).collect())
        .unwrap_or_default();

    TickUpdate {
        tick: engine.current_tick(),
        player,
        fleets: collect_player_fleets(engine, player_id),
        sector_updates: if sector_updates.is_empty() { None } else { Some(sector_updates) },
        homeworld_update,
        outpost: player_data.and_then(|p| outpost_state(engine, p)),
        events: if player_events.is_empty() { None } else { Some(player_events) },
    }
}

fn outpost_state(engine: &GameEngine, player: &Player) -> Option<OutpostState> {
    let outpost = engine.outposts.get(&player.id)?;
    let pace = engine.pace();
    let ring = outpost.sector.dist_from_origin();
    let densities = engine.site_densities(outpost.sector);
    let next = outpost.job.map_or(outpost.level + 1, |job| match job.task {
        OutpostTask::Build { level } => level + 1,
        OutpostTask::Repair => outpost.level + 1,
    });
    let production = engine.outpost_production(outpost);
    let store_cap = scaling::outpost_store_cap(outpost.level, &pace);
    let repair = (outpost.breached && outpost.job.is_none())
        .then(|| scaling::outpost_repair_payment(outpost.level, outpost.store));
    Some(OutpostState {
        location: outpost.sector,
        level: outpost.level,
        max_level: OUTPOST_MAX_LEVEL,
        in_service: outpost.in_service(),
        breached: outpost.breached,
        work: outpost.job.map(|job| {
            let (kind, level) = match job.task {
                OutpostTask::Build { level } => (OutpostWork::Build, level),
                OutpostTask::Repair => (OutpostWork::Repair, outpost.level),
            };
            OutpostWorkState { kind, level, start_tick: job.start_tick, end_tick: job.end_tick }
        }),
        store: outpost.store,
        store_cap,
        protected: outpost.protected(),
        production,
        yield_lost: ResourceKind::ALL
            .into_iter()
            .filter(|&kind| production.get(kind) > 0.0 && outpost.store.get(kind) >= store_cap.get(kind))
            .collect(),
        grade: scaling::site_grade(densities),
        platform_power: if outpost.breached { 0.0 } else { scaling::outpost_platform_power(outpost.level) },
        defence_power: engine.outpost_defense_power(player.id),
        raid_power: outpost_raid_range(ring),
        raid_arrival_tick: outpost.raid.incoming.map(|raid| raid.arrival_tick),
        next_level: (next <= OUTPOST_MAX_LEVEL).then(|| OutpostLevel {
            level: next,
            cost: scaling::outpost_cost(next),
            ticks: scaling::outpost_build_ticks(next, &pace),
            store_cap: scaling::outpost_store_cap(next, &pace),
            platform_power: scaling::outpost_platform_power(next),
            production: scaling::outpost_yield_per_tick(densities, ring, next, &pace),
        }),
        upgrade_refusal: engine.outpost_upgrade(player.id).err().map(|refusal| refusal.reason),
        repair_cost: repair.map(|(payment, short)| Resources { metal: payment.metal + short, ..payment }),
        repair_waiting_for: repair
            .filter(|&(_, short)| short > 0.0)
            .map(|(_, short)| Resources { metal: short, ..Resources::default() }),
        hops_home: engine.world_gen.lane_hops(outpost.sector, player.homeworld, LANE_SEARCH_HOPS),
    })
}

fn player_state(engine: &GameEngine, player: &Player) -> PlayerState {
    PlayerState {
        id: player.id,
        name: player.name.clone(),
        resources: player.resources,
        homeworld: player.homeworld,
        exchange: engine.exchange_meter(player),
    }
}

/// Outward hops that still leave the fuel to come back: each costs a jump
/// out and a jump home.
fn range_hops(fuel: f32, home_fuel: f32, jump_fuel: f32) -> u32 {
    if jump_fuel <= 0.0 { return 0; }
    ((fuel - home_fuel) / (2.0 * jump_fuel)).floor().max(0.0) as u32
}

fn collect_player_fleets(engine: &GameEngine, player_id: u64) -> Vec<FleetState> {
    let mut list = Vec::new();
    for fleet in engine.fleets.values() {
        if fleet.owner_id != player_id || fleet.ship_count == 0 { continue; }

        let ship_states: Vec<ShipState> = fleet.ships[0..fleet.ship_count].iter().map(|ship| {
            ShipState {
                id: ship.id,
                ship_class: ship.ship_class,
                hull: ship.hull,
                hull_max: ship.hull_max,
                shield: ship.shield,
                shield_max: ship.shield_max,
                weapon_power: ship.weapon_power,
            }
        }).collect();

        let status = match fleet.state {
            FleetStatus::Idle => iac_shared::protocol::FleetStatus::Idle,
            FleetStatus::Moving => iac_shared::protocol::FleetStatus::Moving,
            FleetStatus::Harvesting => iac_shared::protocol::FleetStatus::Harvesting,
            FleetStatus::InCombat => iac_shared::protocol::FleetStatus::InCombat,
            FleetStatus::Returning => iac_shared::protocol::FleetStatus::Returning,
            FleetStatus::Docked => iac_shared::protocol::FleetStatus::Docked,
            FleetStatus::Exploring => iac_shared::protocol::FleetStatus::Exploring,
        };

        let jump_fuel = engine.hop_fuel_cost(fleet);
        let bound_for = fleet.move_target.unwrap_or(fleet.location);
        let home_fuel = engine.route_home_cost(fleet, bound_for);
        list.push(FleetState {
            id: fleet.id,
            location: fleet.location,
            state: status,
            ships: ship_states,
            cargo: fleet.cargo,
            cargo_capacity: fleet_cargo_capacity(fleet, &engine.pace()),
            fuel: fleet.fuel,
            fuel_max: fleet.fuel_max,
            jump_fuel,
            home_fuel,
            jump_ticks: engine.hop_ticks(fleet),
            cooldown_remaining: fleet.action_cooldown,
            power: crate::engine::fleet_power(fleet),
            range_hops: range_hops(fleet.fuel, home_fuel, jump_fuel),
            cargo_blocked: !fleet.laden
                && fleet.cargo.total() > 0.0
                && engine.players.get(&player_id).is_some_and(|p| p.homeworld == fleet.location),
            laden: fleet.laden,
            at_outpost: engine.outposts.get(&player_id).is_some_and(|o| o.sector == fleet.location),
            outpost_fuel: engine.outpost_route_cost(fleet, bound_for),
            policy: engine.policies.get(&fleet.id).map(|p| StandingOrders { preset: p.preset, params: p.params }),
        });
    }
    list.sort_by_key(|f| f.id);
    list
}

fn build_homeworld_state(engine: &GameEngine, player: &Player) -> HomeworldState {
    let buildings: Vec<BuildingState> = (0..BuildingType::COUNT)
        .filter_map(BuildingType::from_usize)
        .map(|bt| BuildingState { building_type: bt, level: player.buildings.get(bt) })
        .collect();

    let research: Vec<ResearchState> = (0..ResearchType::COUNT)
        .filter_map(ResearchType::from_usize)
        .map(|rt| ResearchState { tech: rt, level: player.research.get(rt) })
        .collect();

    let pace = engine.pace();
    let build_queue: Vec<BuildQueueItem> = player.building_queue.iter().map(|q| BuildQueueItem {
        id: q.id,
        building_type: q.building_type,
        target_level: q.target_level,
        start_tick: q.start_tick,
        end_tick: q.end_tick,
    }).collect();

    let waits = crate::queue::project(player, engine.current_tick(), &pace);
    let build_pending: Vec<QueuedBuild> = player.building_pending.iter().zip(&waits.buildings).map(|(q, w)| {
        let cost = iac_shared::scaling::building_cost(q.building_type, q.target_level);
        QueuedBuild {
            id: q.id,
            building_type: q.building_type,
            target_level: q.target_level,
            reserve: q.reserve,
            cost,
            ticks: iac_shared::scaling::building_time(q.building_type, q.target_level, player.buildings.fabricator, &pace),
            waiting_for: w.short,
            waiting_on: w.reason,
            start_in: w.start_in,
        }
    }).collect();
    let research_pending: Vec<QueuedResearch> = player.research_pending.iter().zip(&waits.research).map(|(q, w)| {
        let cost = iac_shared::scaling::research_cost(q.tech, q.target_level);
        QueuedResearch {
            id: q.id,
            tech: q.tech,
            target_level: q.target_level,
            reserve: q.reserve,
            cost,
            ticks: iac_shared::scaling::research_time(q.tech, q.target_level, player.buildings.research_lab, &pace),
            waiting_for: w.short,
            waiting_on: w.reason,
            start_in: w.start_in,
        }
    }).collect();
    let shipyard_pending: Vec<QueuedShip> = player.ship_pending.iter().zip(&waits.ships).map(|(q, w)| {
        QueuedShip {
            id: q.id,
            item: q.item,
            count: q.count,
            cost: q.item.batch_cost(q.count),
            unit_cost: q.item.unit_cost(),
            ticks: q.item.unit_ticks(player.buildings.shipyard, &pace),
            waiting_on: w.reason,
            start_in: w.start_in,
        }
    }).collect();

    let shipyard_queue: Option<ShipyardQueueItem> = player.ship_queue.as_ref().map(|q| ShipyardQueueItem {
        id: q.id,
        item: q.item,
        count: q.count,
        built: q.built,
        start_tick: q.start_tick,
        end_tick: q.end_tick,
    });

    let research_active: Option<ResearchItem> = player.research_queue.as_ref().map(|q| ResearchItem {
        id: q.id,
        tech: q.tech,
        target_level: q.target_level,
        start_tick: q.start_tick,
        end_tick: q.end_tick,
    });

    let mut docked_ships: Vec<ShipState> = Vec::new();
    for fleet in engine.fleets.values() {
        if fleet.owner_id == player.id && fleet.location == player.homeworld {
            for ship in &fleet.ships[0..fleet.ship_count] {
                docked_ships.push(ShipState {
                    id: ship.id,
                    ship_class: ship.ship_class,
                    hull: ship.hull,
                    hull_max: ship.hull_max,
                    shield: ship.shield,
                    shield_max: ship.shield_max,
                    weapon_power: ship.weapon_power,
                });
            }
        }
    }

    HomeworldState {
        location: player.homeworld,
        production: player.production_per_tick(&engine.pace()),
        storage: engine.storage_state(player),
        buildings,
        research,
        build_queue,
        build_pending,
        build_slots: player.building_slots() as u8,
        queue_waiting_max: iac_shared::scaling::QUEUE_WAITING as u8,
        shipyard_queue,
        shipyard_pending,
        research_active,
        research_pending,
        docked_ships,
        defences: DefenceKind::ALL.iter().map(|&kind| DefenceState {
            kind,
            count: player.defences.count[kind as usize],
            restoring: player.defences.restoring[kind as usize],
            power: iac_shared::scaling::structure_power(kind, player.buildings.defense_grid),
        }).collect(),
        home_defence_power: engine.home_defense_power(player.id),
        next_raid_estimate_power: iac_shared::scaling::raid_power_base(
            iac_shared::scaling::econ_points(&player.buildings, &player.research),
        ) * (RAID_POWER_ROLL_MIN + RAID_POWER_ROLL_MAX) / 2.0,
        catalog: iac_shared::protocol::HomeworldCatalog::new(
            &player.buildings, &player.research, &player.defences.count, &pace,
        ),
    }
}

/// True when the player has a fleet in `sector` or their homeworld or outpost is there.
fn present_at(engine: &GameEngine, player: &Player, sector: Hex) -> bool {
    player.homeworld == sector
        || engine.outposts.get(&player.id).is_some_and(|o| o.sector == sector)
        || engine.fleets.values().any(|f| f.owner_id == player.id && f.location == sector && f.ship_count > 0)
}

/// The event as `player` should see it, or None if it does not concern them.
///
/// Combat and loss events name their sector and the owning empires, so the
/// decision needs no fleet lookup (a destroyed fleet may already be gone):
/// they reach an empire that owns one of the fleets involved and anyone with
/// a fleet or their homeworld in that sector. `mine` is filled in here, per
/// recipient.
pub fn event_for(event: &GameEvent, player: &Player, engine: &GameEngine) -> Option<GameEvent> {
    use iac_shared::protocol::EventKind as K;
    let own_fleet = |fleet_id: u64| engine.fleets.get(&fleet_id).is_some_and(|f| f.owner_id == player.id);
    let named = |owner: &Option<String>| owner.as_deref() == Some(player.name.as_str());
    let in_sector = |sector: Hex| present_at(engine, player, sector);
    let mut out = event.clone();

    let relevant = match &mut out.kind {
        K::CombatRound(e) => {
            e.mine = named(&e.attacker_owner) || named(&e.target_owner);
            e.mine || in_sector(e.sector)
        }
        K::ShipDestroyed(e) => {
            e.mine = named(&e.owner);
            e.mine || in_sector(e.sector)
        }
        K::FleetDestroyed(e) => {
            e.mine = named(&e.owner);
            e.mine || in_sector(e.sector)
        }
        K::CombatStarted(e) => {
            e.mine = e.owner == player.name;
            e.mine || in_sector(e.sector)
        }
        K::CombatEnded(e) => {
            e.mine = e.owners.contains(&player.name);
            e.mine || in_sector(e.sector)
        }
        K::SalvageDespawned(e) => in_sector(e.sector),
        K::SectorEntered(e) => own_fleet(e.fleet_id),
        K::FleetArrived(e) => own_fleet(e.fleet_id),
        K::ResourceHarvested(e) => own_fleet(e.fleet_id),
        K::SalvageCollected(e) => own_fleet(e.fleet_id),
        K::BuildingCompleted(e) => e.player_id.is_none_or(|p| p == player.id),
        K::ResearchCompleted(e) => e.player_id.is_none_or(|p| p == player.id),
        K::ShipBuilt(e) => e.player_id.is_none_or(|p| p == player.id),
        K::DefenceBuilt(e) => e.player_id.is_none_or(|p| p == player.id),
        K::ScanCompleted(e) => own_fleet(e.fleet_id),
        K::RaidIncoming(e) => e.player_id == player.id,
        K::RaidResolved(e) => e.player_id == player.id,
        K::SiteExplorationStarted(e) => own_fleet(e.fleet_id),
        K::SiteExplored(e) => own_fleet(e.fleet_id),
        K::ChartDelivered(e) => own_fleet(e.fleet_id),
        K::Exchanged(e) => own_fleet(e.terms.fleet_id),
        K::Queue(e) => e.player_id == Some(player.id),
        K::SiteAmbush(e) => own_fleet(e.fleet_id),
        K::PolicyAction(e) => own_fleet(e.fleet_id),
        K::StorageNearCap(e) => e.player_id.is_none_or(|p| p == player.id),
        K::StorageFull(e) => e.player_id.is_none_or(|p| p == player.id),
        K::Alert(e) => e.player_id.is_none_or(|p| p == player.id),
        K::OutpostCompleted(e) => e.player_id == player.id,
    };
    relevant.then_some(out)
}
