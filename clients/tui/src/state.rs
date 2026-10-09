// Client-side state management: game state from server + UI state.

use std::collections::HashMap;

use iac_shared::constants::ShipClass;
use iac_shared::hex::Hex;
use iac_shared::protocol::{
    Command, EventKind, FleetState, GameEvent, HomeworldState,
    LeaderboardReply, PlayerState, SectorState, ServerMessage, WorldInfo,
};
use iac_shared::scaling::{BuildingType, DefenceKind, ResearchType};

/// UI view modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    CommandCenter,
    Windshield,
    StarMap,
    Homeworld,
}

/// Map zoom levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoomLevel {
    Close,    // node graph, ~3 hex radius
    Sector,   // hybrid hex-cell, ~8 hex radius
    Region,   // minimal dots, ~20 hex radius
}

/// Scroll direction for map navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

/// Tab within the homeworld view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HomeworldTab {
    Buildings,
    Shipyard,
    Research,
}

impl HomeworldTab {
    pub fn next(self) -> HomeworldTab {
        match self {
            HomeworldTab::Buildings => HomeworldTab::Shipyard,
            HomeworldTab::Shipyard => HomeworldTab::Research,
            HomeworldTab::Research => HomeworldTab::Buildings,
        }
    }

    pub fn item_count(self) -> usize {
        match self {
            HomeworldTab::Buildings => BuildingType::COUNT,
            HomeworldTab::Shipyard => ShipClass::ALL.len() + DefenceKind::COUNT,
            HomeworldTab::Research => ResearchType::COUNT,
        }
    }
}

/// Fleet-management overlay: pick ships of the active fleet to split off.
#[derive(Debug, Clone, Default)]
pub struct FleetPanel {
    pub cursor: usize,
    pub marked: Vec<u64>,
}

/// Navigation actions within the homeworld view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HomeworldNav {
    CursorUp,
    CursorDown,
    CursorLeft,
    CursorRight,
    TabNext,
    Select,
}

/// Ring buffer for event log entries (keeps last N events).
pub struct EventLog {
    events: [Option<GameEvent>; 100],
    head: usize,
    count: usize,
}

impl EventLog {
    pub fn new() -> Self {
        Self {
            events: std::array::from_fn(|_| None),
            head: 0,
            count: 0,
        }
    }

    pub fn push(&mut self, event: GameEvent) {
        if self.count == 100 {
            let idx = self.head;
            self.events[idx] = None;
        }

        self.events[self.head] = Some(event);
        self.head = (self.head + 1) % 100;
        if self.count < 100 {
            self.count += 1;
        }
    }
    /// Get the n-th most recent event (0 = most recent).
    pub fn get_recent(&self, n: usize) -> Option<&GameEvent> {
        if n >= self.count {
            return None;
        }
        let idx = (self.head + 100 - 1 - n) % 100;
        self.events[idx].as_ref()
    }
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.count
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

/// Full client state: game data from server + local UI state.
pub struct ClientState {
    // Game state (from server)
    pub tick: u64,
    pub player: Option<PlayerState>,
    pub fleets: Vec<FleetState>,
    pub homeworld: Option<HomeworldState>,
    pub known_sectors: HashMap<u32, SectorState>,
    pub world: Option<WorldInfo>,
    /// The last score table the server sent, shown in an overlay.
    pub leaderboard: Option<LeaderboardReply>,
    pub show_leaderboard: bool,
    pub event_log: EventLog,

    // UI state
    pub current_view: View,
    pub active_fleet_idx: usize,
    pub prev_fleet_location: Option<Hex>,
    pub map_center: Hex,
    pub map_zoom: ZoomLevel,
    pub show_sector_info: bool,
    pub show_keybinds: bool,
    pub show_tech_tree: bool,
    pub fleet_panel: Option<FleetPanel>,
    pub homeworld_tab: HomeworldTab,
    pub homeworld_cursor: usize,
    pub status_message: String,
    /// Tick when status_message was set; the footer shows it briefly.
    pub status_set_tick: u64,
    /// Which hostile fleet in the current sector is targeted by [a].
    pub target_idx: usize,
    /// Batch size for shipyard orders, adjusted with +/-.
    pub ship_build_count: u16,
    /// Arrival tick of a forecasted raid; drives the header countdown.
    pub raid_eta: Option<u64>,

    // Map intel & navigation
    /// Faint scan contacts remembered on the chart: hex key → (kind, tick).
    pub signal_pings: HashMap<u32, (iac_shared::protocol::SignalKind, u64)>,
    /// Recent scan origins for the expanding-ring flourish: (center, tick).
    pub scan_ripples: Vec<(Hex, u64)>,
    /// Plotted course for a fleet: remaining hops, dispatched as the
    /// fleet goes idle. (fleet_id, hops)
    pub nav_route: Option<(u64, Vec<Hex>)>,
    /// Last tick a route hop was sent, so we don't spam moves.
    pub nav_last_sent: u64,
}

/// How long a remembered signal ping stays on the chart (matches the
/// server's scan reveal window).
pub const SIGNAL_PING_TICKS: u64 = 120;
/// How many ticks the scan ripple animation runs.
pub const SCAN_RIPPLE_TICKS: u64 = 4;

/// How long (in ticks) an error/status message stays in the footer.
pub const STATUS_MESSAGE_TICKS: u64 = 8;
pub const MAX_SHIP_BUILD_COUNT: u16 = 99;

impl ClientState {
    pub fn new() -> Self {
        Self {
            tick: 0,
            player: None,
            fleets: Vec::new(),
            homeworld: None,
            known_sectors: HashMap::new(),
            world: None,
            leaderboard: None,
            show_leaderboard: false,
            event_log: EventLog::new(),
            current_view: View::CommandCenter,
            active_fleet_idx: 0,
            prev_fleet_location: None,
            map_center: Hex::ORIGIN,
            map_zoom: ZoomLevel::Sector,
            show_sector_info: false,
            show_keybinds: false,
            show_tech_tree: false,
            fleet_panel: None,
            homeworld_tab: HomeworldTab::Buildings,
            homeworld_cursor: 0,
            status_message: String::new(),
            status_set_tick: 0,
            target_idx: 0,
            ship_build_count: 1,
            raid_eta: None,
            signal_pings: HashMap::new(),
            scan_ripples: Vec::new(),
            nav_route: None,
            nav_last_sent: 0,
        }
    }

    /// Push an event to the log, updating any state it implies.
    fn note_event(&mut self, event: &GameEvent) {
        match &event.kind {
            EventKind::RaidIncoming(e) => self.raid_eta = Some(e.arrival_tick),
            EventKind::RaidResolved(_) => self.raid_eta = None,
            EventKind::ScanCompleted(e) => {
                self.scan_ripples.push((e.sector, event.tick));
                for contact in &e.signals {
                    self.signal_pings
                        .insert(contact.sector.to_key(), (contact.signal, event.tick));
                }
            }
            _ => {}
        }
        self.event_log.push(event.clone());
    }

    /// Drop expired chart decorations.
    fn prune_map_intel(&mut self) {
        let tick = self.tick;
        self.signal_pings.retain(|_, (_, t)| tick.saturating_sub(*t) < SIGNAL_PING_TICKS);
        self.scan_ripples.retain(|(_, t)| tick.saturating_sub(*t) <= SCAN_RIPPLE_TICKS);
    }

    /// Ticks until the forecasted raid arrives, if one is inbound.
    pub fn raid_countdown(&self) -> Option<u64> {
        let eta = self.raid_eta?;
        if self.tick >= eta {
            return None;
        }
        Some(eta - self.tick)
    }

    /// The status message, if it is still fresh enough to display.
    pub fn active_status(&self) -> Option<&str> {
        if self.status_message.is_empty() {
            return None;
        }
        if self.tick.saturating_sub(self.status_set_tick) > STATUS_MESSAGE_TICKS {
            return None;
        }
        Some(&self.status_message)
    }

    /// Cycle the attack target through hostiles in the current sector.
    pub fn cycle_target(&mut self) {
        let count = self.current_sector()
            .and_then(|s| s.hostiles.as_ref())
            .map(|h| h.len())
            .unwrap_or(0);
        if count > 0 {
            self.target_idx = (self.target_idx + 1) % count;
        } else {
            self.target_idx = 0;
        }
    }

    // ── Fleet management ─────────────────────────────────────────────

    pub fn toggle_fleet_panel(&mut self) {
        self.fleet_panel = match self.fleet_panel {
            Some(_) => None,
            None if self.active_fleet().is_some() => Some(FleetPanel::default()),
            None => None,
        };
        self.show_keybinds = false;
        self.show_tech_tree = false;
    }

    pub fn fleet_panel_move(&mut self, delta: i32) {
        let count = self.active_fleet().map(|f| f.ships.len()).unwrap_or(0);
        if let Some(panel) = self.fleet_panel.as_mut() && count > 0 {
            panel.cursor = (panel.cursor as i32 + delta).rem_euclid(count as i32) as usize;
        }
    }

    pub fn fleet_panel_mark(&mut self) {
        let Some(ship_id) = self.fleet_panel.as_ref()
            .and_then(|p| self.active_fleet().and_then(|f| f.ships.get(p.cursor)))
            .map(|s| s.id)
        else { return; };
        if let Some(panel) = self.fleet_panel.as_mut() {
            match panel.marked.iter().position(|id| *id == ship_id) {
                Some(i) => { panel.marked.remove(i); }
                None => panel.marked.push(ship_id),
            }
        }
    }

    /// Split the marked ships off the active fleet.
    pub fn fleet_panel_split(&mut self) -> Option<Command> {
        let fleet = self.active_fleet()?;
        let panel = self.fleet_panel.as_ref()?;
        let marked: Vec<u64> = panel.marked.iter().copied()
            .filter(|id| fleet.ships.iter().any(|s| s.id == *id))
            .collect();
        if marked.is_empty() || marked.len() >= fleet.ships.len() {
            self.status_message = "mark some ships (space), but leave at least one behind".to_string();
            self.status_set_tick = self.tick;
            return None;
        }
        let cmd = Command::Split { fleet_id: fleet.id, ship_ids: marked };
        if let Some(panel) = self.fleet_panel.as_mut() {
            panel.marked.clear();
            panel.cursor = 0;
        }
        Some(cmd)
    }

    /// Other fleets sharing the active fleet's sector, lowest id first.
    pub fn merge_candidates(&self) -> Vec<&FleetState> {
        let Some(active) = self.active_fleet() else { return Vec::new(); };
        let mut v: Vec<&FleetState> = self.fleets.iter()
            .filter(|f| f.id != active.id && f.location == active.location)
            .collect();
        v.sort_by_key(|f| f.id);
        v
    }

    /// Fold the lowest-numbered fleet sharing this sector into the active one.
    pub fn fleet_panel_merge(&mut self) -> Option<Command> {
        let active_id = self.active_fleet()?.id;
        let other = self.merge_candidates().first().map(|f| f.id);
        match other {
            Some(other_fleet_id) => Some(Command::Merge { fleet_id: active_id, other_fleet_id }),
            None => {
                self.status_message = "no other fleet in this sector to merge".to_string();
                self.status_set_tick = self.tick;
                None
            }
        }
    }

    pub fn adjust_build_count(&mut self, delta: i32) {
        let next = self.ship_build_count as i32 + delta;
        self.ship_build_count = next.clamp(1, MAX_SHIP_BUILD_COUNT as i32) as u16;
    }

    // ── Server message handling ──────────────────────────────────────

    pub fn apply_server_message(&mut self, msg: &ServerMessage) -> Result<(), String> {
        match msg {
            ServerMessage::TickUpdate(update) => {
                self.tick = update.tick;

                if let Some(player) = &update.player {
                    self.player = Some(player.clone());
                }

                self.replace_fleets(&update.fleets);

                if let Some(hw) = &update.homeworld_update {
                    self.homeworld = Some(hw.clone());
                }

                if let Some(sectors) = &update.sector_updates {
                    for sector in sectors {
                        let key = sector.location.to_key();
                        self.known_sectors.insert(key, sector.clone());
                        // Charted sectors outrank a faint ping.
                        self.signal_pings.remove(&key);
                    }
                }

                if let Some(events) = &update.events {
                    for event in events {
                        self.note_event(event);
                    }
                }
                self.prune_map_intel();
            }
            ServerMessage::FullState(state) => {
                self.tick = state.tick;
                self.player = Some(state.player.clone());
                self.homeworld = Some(state.homeworld.clone());
                self.world = Some(state.world.clone());
                self.replace_fleets(&state.fleets);

                self.known_sectors.clear();
                for sector in &state.known_sectors {
                    let key = sector.location.to_key();
                    self.known_sectors.insert(key, sector.clone());
                }

                if let Some(p) = &self.player {
                    self.map_center = p.homeworld;
                }
            }
            ServerMessage::Event(event) => {
                self.note_event(event);
            }
            ServerMessage::Leaderboard(board) => {
                self.leaderboard = Some(board.clone());
                self.show_leaderboard = true;
            }
            ServerMessage::PreviewMove(p) => {
                self.status_message = format!(
                    "F{} to [{},{}]: fuel -{:.0}, {:.0} left, {} hops home need {:.0}{}",
                    p.fleet_id, p.target.q, p.target.r, p.fuel_cost, p.fuel_after, p.hops_home, p.fuel_to_return,
                    if p.can_return { "" } else { "  CANNOT RETURN" },
                );
                self.status_set_tick = self.tick;
            }
            // Login is settled before the TUI starts (connection::login).
            ServerMessage::AuthResult(_) => {}
            ServerMessage::Error(err) => {
                // A rejected command aborts any plotted course — better to
                // stop than to blindly keep sending hops.
                if self.nav_route.is_some() {
                    self.nav_route = None;
                }
                self.status_message = err.message.clone();
                self.status_set_tick = self.tick;
                self.event_log.push(GameEvent {
                    tick: self.tick,
                    kind: EventKind::Alert(iac_shared::protocol::AlertEvent {
                        player_id: None,
                        level: iac_shared::protocol::AlertLevel::Warning,
                        message: err.message.clone(),
                        fleet_id: None,
                        sector: None,
                    }),
                });
            }
        }
        Ok(())
    }

    fn replace_fleets(&mut self, fleets: &[FleetState]) {
        let old_loc = self.active_fleet().map(|f| f.location);
        self.fleets = fleets.to_vec();
        self.active_fleet_idx = self.active_fleet_idx.min(self.fleets.len().saturating_sub(1));
        self.update_prev_fleet_location(old_loc);
    }

    fn update_prev_fleet_location(&mut self, old_loc: Option<Hex>) {
        // No previous snapshot means no movement to record.
        let Some(prev) = old_loc else { return; };
        if let Some(cur) = self.active_fleet()
            && cur.location != prev {
                self.prev_fleet_location = Some(prev);
            }
    }

    // ── Navigation helpers ───────────────────────────────────────────

    pub fn set_view(&mut self, view: View) {
        self.current_view = view;
        if view == View::Windshield
            && let Some(fleet) = self.active_fleet() {
                self.map_center = fleet.location;
            }
    }

    pub fn scroll_map(&mut self, direction: ScrollDirection) {
        let delta = match direction {
            ScrollDirection::Up => Hex::new(0, -1),
            ScrollDirection::Down => Hex::new(0, 1),
            ScrollDirection::Left => Hex::new(-1, 0),
            ScrollDirection::Right => Hex::new(1, 0),
        };
        self.map_center = self.map_center.add(delta);
    }

    pub fn set_zoom(&mut self, level: ZoomLevel) {
        self.map_zoom = level;
    }

    pub fn active_fleet(&self) -> Option<&FleetState> {
        self.fleets.get(self.active_fleet_idx)
    }

    #[allow(dead_code)]
    pub fn active_fleet_mut(&mut self) -> Option<&mut FleetState> {
        self.fleets.get_mut(self.active_fleet_idx)
    }

    pub fn cycle_fleet(&mut self) {
        if !self.fleets.is_empty() {
            self.active_fleet_idx = (self.active_fleet_idx + 1) % self.fleets.len();
            self.prev_fleet_location = None;
            self.target_idx = 0;
            if let Some(fleet) = self.active_fleet() {
                self.map_center = fleet.location;
            }
        }
    }

    pub fn current_sector(&self) -> Option<&SectorState> {
        self.active_fleet()
            .map(|f| f.location.to_key())
            .and_then(|key| self.known_sectors.get(&key))
    }

    // ── Course plotting ──────────────────────────────────────────────

    /// BFS over *known* sector connections from the active fleet to
    /// `target`. The final hop may land on an unknown sector (a known
    /// sector's exit into the fog). Returns the hop list, excluding the
    /// start.
    pub fn plot_route(&self, target: Hex) -> Option<(u64, Vec<Hex>)> {
        let fleet = self.active_fleet()?;
        let start = fleet.location;
        if start == target { return None; }

        let mut prev: HashMap<u32, u32> = HashMap::new();
        let mut frontier = vec![start];
        prev.insert(start.to_key(), start.to_key());

        'search: while !frontier.is_empty() {
            let mut next = Vec::new();
            for coord in &frontier {
                // Only traverse through sectors we have charts for.
                let Some(sector) = self.known_sectors.get(&coord.to_key()) else { continue; };
                for &n in &sector.connections {
                    let key = n.to_key();
                    if prev.contains_key(&key) { continue; }
                    prev.insert(key, coord.to_key());
                    if n == target { break 'search; }
                    next.push(n);
                }
            }
            frontier = next;
        }

        prev.get(&target.to_key())?;
        let mut hops = vec![target];
        let start_key = start.to_key();
        let mut cur = target.to_key();
        while prev[&cur] != start_key {
            cur = prev[&cur];
            hops.push(Hex::from_key(cur));
        }
        hops.reverse();
        Some((fleet.id, hops))
    }

    /// The next move to dispatch for the plotted course, if the fleet is
    /// ready for one. Consumes arrived hops.
    pub fn next_route_move(&mut self) -> Option<Command> {
        let (fleet_id, ref mut hops) = *self.nav_route.as_mut()?;
        let fleet = self.fleets.iter().find(|f| f.id == fleet_id)?;

        // Drop hops we've reached.
        while hops.first() == Some(&fleet.location) {
            hops.remove(0);
        }
        if hops.is_empty() {
            self.nav_route = None;
            self.status_message = "course complete".to_string();
            self.status_set_tick = self.tick;
            return None;
        }

        let ready = matches!(
            fleet.state,
            iac_shared::protocol::FleetStatus::Idle | iac_shared::protocol::FleetStatus::Docked
        ) && fleet.cooldown_remaining == 0;
        if !ready { return None; }
        // One dispatch per tick — the fleet needs a tick to react.
        if self.nav_last_sent == self.tick { return None; }
        self.nav_last_sent = self.tick;

        Some(Command::Move { fleet_id, target: hops[0] })
    }

    /// Hexes on the active plotted course (for map highlighting).
    pub fn route_hops(&self) -> &[Hex] {
        self.nav_route.as_ref().map(|(_, h)| h.as_slice()).unwrap_or(&[])
    }

    // ── Homeworld navigation ─────────────────────────────────────────

    pub fn homeworld_nav(&mut self, nav: HomeworldNav) -> Option<Command> {
        let hw = self.homeworld.as_ref()?;
        let count = self.homeworld_tab.item_count();

        match nav {
            HomeworldNav::CursorUp => {
                if self.homeworld_cursor >= 2 {
                    self.homeworld_cursor -= 2;
                }
            }
            HomeworldNav::CursorDown => {
                if self.homeworld_cursor + 2 < count {
                    self.homeworld_cursor += 2;
                }
            }
            HomeworldNav::CursorLeft => {
                self.homeworld_cursor = self.homeworld_cursor.saturating_sub(1);
            }
            HomeworldNav::CursorRight => {
                if self.homeworld_cursor + 1 < count {
                    self.homeworld_cursor += 1;
                }
            }
            HomeworldNav::TabNext => {
                self.homeworld_tab = self.homeworld_tab.next();
                self.homeworld_cursor = 0;
            }
            HomeworldNav::Select => {
                let cat = &hw.catalog;
                let i = self.homeworld_cursor;
                return match self.homeworld_tab {
                    HomeworldTab::Buildings => {
                        let o = cat.buildings.get(i)?;
                        let open = o.next.is_some() && o.requires.iter().all(|r| r.met);
                        open.then_some(Command::Build { building_type: o.building_type })
                    }
                    HomeworldTab::Research => {
                        let o = cat.research.get(i)?;
                        let open = o.next.is_some() && o.requires.iter().all(|r| r.met);
                        open.then_some(Command::Research { tech: o.tech })
                    }
                    HomeworldTab::Shipyard => {
                        if let Some(o) = cat.ships.get(i) {
                            return o.requires.iter().all(|r| r.met).then_some(Command::BuildShip {
                                ship_class: o.ship_class,
                                count: self.ship_build_count,
                            });
                        }
                        let o = cat.defences.get(i - cat.ships.len())?;
                        o.requires.iter().all(|r| r.met).then_some(Command::BuildDefence {
                            kind: o.kind,
                            count: self.ship_build_count,
                        })
                    }
                };
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iac_shared::constants::Resources;
    use iac_shared::protocol::{FleetStatus, ShipState};

    fn ship(id: u64) -> ShipState {
        ShipState {
            id, ship_class: ShipClass::Scout, hull: 10.0, hull_max: 10.0,
            shield: 0.0, shield_max: 0.0, weapon_power: 1.0,
        }
    }

    fn fleet(id: u64, at: Hex, ship_ids: &[u64]) -> FleetState {
        FleetState {
            id, location: at, state: FleetStatus::Idle,
            ships: ship_ids.iter().map(|i| ship(*i)).collect(),
            cargo: Resources::default(), cargo_capacity: 20.0, fuel: 10.0, fuel_max: 10.0,
            jump_fuel: 1.0, home_fuel: 0.0, cooldown_remaining: 0, power: 0.0, range_hops: 0, cargo_blocked: false, policy: None,
        }
    }

    #[test]
    fn panel_splits_marked_ships_and_keeps_one() {
        let mut st = ClientState::new();
        st.fleets = vec![fleet(1, Hex::ORIGIN, &[11, 12, 13])];
        st.toggle_fleet_panel();
        st.fleet_panel_mark();
        st.fleet_panel_move(1);
        st.fleet_panel_mark();
        match st.fleet_panel_split() {
            Some(Command::Split { fleet_id, ship_ids }) => {
                assert_eq!(fleet_id, 1);
                assert_eq!(ship_ids, vec![11, 12]);
            }
            other => panic!("{other:?}"),
        }
        assert!(st.fleet_panel_split().is_none(), "marks were consumed");

        for _ in 0..3 { st.fleet_panel_move(1); st.fleet_panel_mark(); }
        assert!(st.fleet_panel_split().is_none(), "cannot take every ship");
    }

    #[test]
    fn panel_merges_only_fleets_in_the_same_sector() {
        let mut st = ClientState::new();
        st.fleets = vec![
            fleet(1, Hex::ORIGIN, &[11]),
            fleet(2, Hex::new(1, 0), &[21]),
            fleet(3, Hex::ORIGIN, &[31]),
        ];
        st.toggle_fleet_panel();
        match st.fleet_panel_merge() {
            Some(Command::Merge { fleet_id, other_fleet_id }) => assert_eq!((fleet_id, other_fleet_id), (1, 3)),
            other => panic!("{other:?}"),
        }
        st.fleets.retain(|f| f.id != 3);
        assert!(st.fleet_panel_merge().is_none());
    }
}
