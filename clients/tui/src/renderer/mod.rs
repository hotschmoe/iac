// Renderer: ratatui-based TUI views for the IAC client.

pub mod command_center;
pub mod homeworld;
pub mod star_map;
pub mod windshield;

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    symbols::border,
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::state::ClientState;

// ── Amber color palette ────────────────────────────────────────────

pub const AMBER_FAINT: Style = Style::new().fg(Color::Rgb(30, 21, 0));
pub const AMBER_DIM: Style = Style::new().fg(Color::Rgb(77, 53, 0));
pub const AMBER: Style = Style::new().fg(Color::Rgb(153, 106, 0));
pub const AMBER_BRIGHT: Style = Style::new().fg(Color::Rgb(217, 150, 0));
pub const AMBER_FULL: Style = Style::new().fg(Color::Rgb(255, 176, 0));

// ── Semantic accents ───────────────────────────────────────────────
// The cockpit stays amber; these fire only when something demands
// attention: red = danger, green = gain, cyan = intel.

pub const RED_ALERT: Style = Style::new().fg(Color::Rgb(255, 85, 70));
pub const RED_DIM: Style = Style::new().fg(Color::Rgb(150, 55, 45));
pub const GREEN_GOOD: Style = Style::new().fg(Color::Rgb(120, 210, 110));
pub const GREEN_DIM: Style = Style::new().fg(Color::Rgb(70, 120, 65));
pub const CYAN_INTEL: Style = Style::new().fg(Color::Rgb(95, 195, 210));
pub const CYAN_DIM: Style = Style::new().fg(Color::Rgb(55, 110, 120));

// ── View dispatch ──────────────────────────────────────────────────

pub fn view(frame: &mut Frame<'_>, state: &ClientState) {
    let area = frame.area();
    if area.width < 10 || area.height < 5 {
        return;
    }

    // Header (2 rows) + body + footer (1 row)
    let chunks = Layout::vertical([Constraint::Length(2), Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    render_header(frame, state, chunks[0]);

    if state.show_leaderboard {
        render_leaderboard(frame, state, chunks[1]);
    } else if state.show_keybinds {
        render_keybinds(frame, chunks[1]);
    } else if state.fleet_panel.is_some() {
        render_fleet_panel(frame, state, chunks[1]);
    } else if state.exchange_panel.is_some() {
        render_exchange_panel(frame, state, chunks[1]);
    } else if state.show_tech_tree && state.current_view == crate::state::View::Homeworld {
        homeworld::render_tech_tree(frame, state, chunks[1]);
    } else {
        match state.current_view {
            crate::state::View::CommandCenter => command_center::render(frame, state, chunks[1]),
            crate::state::View::Windshield => windshield::render(frame, state, chunks[1]),
            crate::state::View::StarMap => star_map::render(frame, state, chunks[1]),
            crate::state::View::Homeworld => homeworld::render(frame, state, chunks[1]),
        }
    }

    render_footer(frame, state, chunks[2]);
}

// ── Header ─────────────────────────────────────────────────────────

fn render_header(frame: &mut Frame<'_>, state: &ClientState, area: Rect) {
    let view_label = match state.current_view {
        crate::state::View::CommandCenter => "COMMAND CENTER",
        crate::state::View::Windshield => "WINDSHIELD",
        crate::state::View::StarMap => "STAR MAP",
        crate::state::View::Homeworld => "HOMEWORLD",
    };
    let player_name = state.player.as_ref().map(|p| p.name.as_str()).unwrap_or("---");

    let pace = state.world.as_ref().map(pace_label).unwrap_or_default();
    let text = format!(" IN AMBER CLAD v0.1 | TICK: {}{} | {} | {}", state.tick, pace, player_name, view_label);

    // Inbound raid: a countdown blinks in the header wherever you are.
    if let Some(remaining) = state.raid_countdown() {
        let flash = state.tick.is_multiple_of(2);
        let warn = format!("  !! RAID INBOUND T-{}s !!", remaining);
        let line = ratatui::text::Line::from(vec![
            ratatui::text::Span::styled(text, AMBER_FULL),
            ratatui::text::Span::styled(warn, if flash { RED_ALERT } else { RED_DIM }),
        ]);
        frame.render_widget(Paragraph::new(line), area);
        return;
    }

    let paragraph = Paragraph::new(text).style(AMBER_FULL);
    frame.render_widget(paragraph, area);
}

/// " x600 blitz" for the header; the pace is fixed per world.
fn pace_label(world: &iac_shared::protocol::WorldInfo) -> String {
    match &world.preset {
        Some(name) => format!(" x{} {}", world.pace, name),
        None => format!(" x{}", world.pace),
    }
}

// ── Footer ─────────────────────────────────────────────────────────

fn render_footer(frame: &mut Frame<'_>, state: &ClientState, area: Rect) {
    // A fresh error/status message takes over the footer in red — the
    // player must see why their command bounced.
    if let Some(msg) = state.active_status() {
        let paragraph = Paragraph::new(format!(" ! {}", msg)).style(RED_ALERT);
        frame.render_widget(paragraph, area);
        return;
    }

    let text: &str = if state.fleet_panel.is_some() {
        " FLEETS | [Up/Down] Ship  [Space] Mark  [s] Split  [g] Merge in  [r] Resource  [Left/Right] Amount  [l] Load  [u] Unload  [Tab] Fleet  [f] Close"
    } else if state.exchange_panel.is_some() {
        " EXCHANGE | [g] Give  [w] Want  [Left/Right] Amount  [Enter] Trade  [Tab] Fleet  [e] Close"
    } else if state.show_tech_tree && state.current_view == crate::state::View::Homeworld {
        " TECH TREE | [t] Close  [?] Keys"
    } else {
        match state.current_view {
            crate::state::View::CommandCenter => " CMD CENTER | [w] Windshield  [m] Map  [b] Base  [?] Keys",
            crate::state::View::Windshield => " WINDSHIELD | [1-6] Move  [v] Scan  [h] Harvest  [x] Board  [a] Attack  [e] Exchange  [p] Orders  [f] Fleets  [?] Keys",
            crate::state::View::StarMap => " STAR MAP | [Arrows] Crosshair  [Enter] Plot Course  [z/x] Zoom  [?] Keys",
            crate::state::View::Homeworld => " HOMEWORLD | Tab Switch  Arrows Select  Enter Build  [+/-] Count  [x/X/z] Cancel running  [c/C/Z] Cancel waiting  [t] Tree  [?] Keys",
        }
    };
    let paragraph = Paragraph::new(text).style(AMBER_DIM);
    frame.render_widget(paragraph, area);
}

// ── Keybinds overlay ───────────────────────────────────────────────

fn render_keybinds(frame: &mut Frame<'_>, area: Rect) {
    let block = Block::new()
        .title(" KEYBINDS ")
        .borders(Borders::ALL)
        .border_set(border::ROUNDED)
        .border_style(AMBER_DIM);

    let text = "
NAVIGATION
─────────────────────────────
Esc       Command Center
w         Windshield view
m         Star Map view
b         Homeworld base
Tab       Cycle fleet
q         Quit

WINDSHIELD
─────────────────────────────
1-6       Move (hex direction)
h         Harvest everything
M/C/D     Harvest metal/crystal/deut
s         Collect salvage
v         Scan (scouts reach far)
x         Board derelict site
S         Stop / abort action
n         Next target
a         Attack target
r         Recall to homeworld
i         Sector info

FLEET ORDERS
─────────────────────────────
f         Fleet panel: split ships
          off, merge fleets in
          the same sector; at
          home, load the hold
          from the stockpile
          (r, arrows, l) or
          unload it (u)
p         Cycle standing orders
          (manual > prospect >
           mine+return > salvage
           +sites > patrol home)
P         Fuel budget of orders:
          10 > 20 > 35 > 50 % of
          deuterium income > no
          bound
e         Exchange: quote and
          trade cargo at the hub

HOMEWORLD
─────────────────────────────
Tab       Cycle panel
Arrows    Navigate cards
Enter     Build/Research
+/-       Ship batch size
x/X/z     Cancel running bld/ship/res
l         Leaderboard (any view)
c/C/Z     Cancel next waiting bld/ship/res
t         Tech tree

STAR MAP
─────────────────────────────
Arrows    Move crosshair
Enter     Plot course to it
Bksp      Clear course
z / x     Zoom out / in
c         Center on fleet

[?] Close";

    let paragraph = Paragraph::new(text).style(AMBER_BRIGHT);
    let inner = block.inner(area);
    frame.render_widget(&block, area);
    frame.render_widget(paragraph, inner);
}

// ── Leaderboard ────────────────────────────────────────────────────

fn render_leaderboard(frame: &mut Frame<'_>, state: &ClientState, area: Rect) {
    use ratatui::text::Line;
    let block = titled_block(" LEADERBOARD ", AMBER_DIM);
    let inner = block.inner(area);
    frame.render_widget(&block, area);
    let Some(board) = &state.leaderboard else { return };

    let me = state.player.as_ref().map(|p| p.name.as_str());
    let row = |e: &iac_shared::protocol::LeaderboardEntry| {
        let style = if Some(e.name.as_str()) == me { AMBER_FULL } else { AMBER };
        Line::styled(
            format!(
                " {:>3}  {:<20} {}  {:>8.1}  core {:>8.1}  combat {:>6.1}  explore {:>6.1}",
                e.rank, e.name, if e.agent { "AI " } else { "   " }, e.score, e.core, e.combat, e.explore,
            ),
            style,
        )
    };
    let mut lines = vec![Line::styled(
        " rank  name                      score         core    combat   explore   (any key closes)",
        AMBER_DIM,
    )];
    lines.extend(board.entries.iter().map(row));
    if let Some(you) = &board.you {
        lines.push(Line::styled(" ...", AMBER_DIM));
        lines.push(row(you));
    }
    if !board.bots.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::styled(" BUILT-IN BOTS  part of the world, not of the ranking above", AMBER_DIM));
        lines.extend(board.bots.iter().map(row));
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

// ── Fleet panel ────────────────────────────────────────────────────

fn render_fleet_panel(frame: &mut Frame<'_>, state: &ClientState, area: Rect) {
    use ratatui::text::{Line, Span};
    let block = titled_block(" FLEETS ", AMBER_DIM);
    let inner = block.inner(area);
    frame.render_widget(&block, area);

    let mut lines: Vec<Line> = Vec::new();
    let (Some(fleet), Some(panel)) = (state.active_fleet(), state.fleet_panel.as_ref()) else {
        return;
    };

    lines.push(Line::styled(
        format!(
            "Fleet {} at [{},{}]  fuel {:.0}/{:.0}  cargo {:.0}/{:.0}",
            fleet.id, fleet.location.q, fleet.location.r, fleet.fuel, fleet.fuel_max,
            fleet.cargo.metal + fleet.cargo.crystal + fleet.cargo.deuterium, fleet.cargo_capacity,
        ),
        AMBER_FULL,
    ));
    lines.push(Line::styled(
        "Mark ships with [Space], then [s] to send them off as a new fleet. At least one stays.",
        AMBER_DIM,
    ));
    lines.push(Line::raw(""));
    for (i, ship) in fleet.ships.iter().enumerate() {
        let mark = if panel.marked.contains(&ship.id) { "[x]" } else { "[ ]" };
        let cursor = if i == panel.cursor { ">" } else { " " };
        let style = if i == panel.cursor { AMBER_FULL } else { AMBER };
        lines.push(Line::from(vec![Span::styled(
            format!(
                "{cursor} {mark} {:<9} #{:<6} hull {:.0}/{:.0}",
                ship.ship_class.label(), ship.id, ship.hull, ship.hull_max,
            ),
            style,
        )]));
    }
    lines.push(Line::raw(""));
    let partners = state.merge_candidates();
    if partners.is_empty() {
        lines.push(Line::styled("No other fleet in this sector to merge.", AMBER_DIM));
    } else {
        lines.push(Line::styled("Same sector ([g] merges the first into this fleet):", AMBER_BRIGHT));
        for f in partners {
            lines.push(Line::styled(
                format!("  fleet {}  {} ship(s)  fuel {:.0}/{:.0}", f.id, f.ships.len(), f.fuel, f.fuel_max),
                AMBER,
            ));
        }
    }
    lines.push(Line::raw(""));
    lines.extend(hold_lines(state, fleet));
    frame.render_widget(Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: false }), inner);
}

/// The hold of the fleet panel's fleet and what `load` would put in it.
fn hold_lines(state: &ClientState, fleet: &iac_shared::protocol::FleetState) -> Vec<ratatui::text::Line<'static>> {
    use ratatui::text::Line;
    let mut lines = vec![Line::styled(
        format!(
            "Hold {:.0}/{:.0}: {:.0} metal, {:.0} crystal, {:.0} deuterium{}",
            fleet.cargo.total(), fleet.cargo_capacity, fleet.cargo.metal, fleet.cargo.crystal, fleet.cargo.deuterium,
            if fleet.laden {
                "  LADEN: the dock leaves it aboard until the fleet has been away, or [u] unloads it"
            } else if fleet.cargo_blocked {
                "  the stockpile has no room for it yet"
            } else {
                ""
            },
        ),
        if fleet.laden { GREEN_GOOD } else { AMBER_BRIGHT },
    )];
    let Some(offer) = state.load_offer() else { return lines; };
    if !state.active_fleet_at_home() {
        lines.push(Line::styled(
            "A hold is loaded from the stockpile, and unloaded into it, at the homeworld dock only.",
            AMBER_DIM,
        ));
        return lines;
    }
    if let Some(orders) = fleet.policy {
        lines.push(Line::styled(
            format!("Under standing orders ({}), which unload at the dock: set manual with [p] to load by hand.", orders.preset.label()),
            AMBER_DIM,
        ));
        return lines;
    }
    if let Some(player) = &state.player {
        let capped = state.homeworld.as_ref().map(|h| h.storage.capped.as_slice()).unwrap_or_default();
        let part = |kind: iac_shared::constants::ResourceKind| {
            format!("{:.0} {}{}", player.resources.get(kind), kind.label(), if capped.contains(&kind) { " (FULL)" } else { "" })
        };
        let [metal, crystal, deuterium] = iac_shared::constants::ResourceKind::ALL.map(part);
        lines.push(Line::styled(format!("Stockpile {metal}, {crystal}, {deuterium}"), AMBER));
    }
    let share = crate::state::HOLD_SHARES[state.fleet_panel.as_ref().map_or(0, |p| p.share)];
    lines.push(Line::styled(
        format!(
            " [r] Load     {:<10} [<-/->] {share}% of what fits: {:.0}     [l] Load   [u] Unload",
            offer.resource.label().to_uppercase(),
            offer.amount.unwrap_or(offer.fits),
        ),
        AMBER_FULL,
    ));
    lines.push(Line::styled(
        "A loaded hold stays aboard while docked, to be flown to the hub's exchange; what the fleet brings home unloads as always.",
        AMBER_DIM,
    ));
    lines
}

// ── Exchange panel ─────────────────────────────────────────────────

fn render_exchange_panel(frame: &mut Frame<'_>, state: &ClientState, area: Rect) {
    use iac_shared::constants::{EXCHANGE_FEE_MAX, EXCHANGE_FEE_MIN};
    use iac_shared::world::HubService;
    use ratatui::text::Line;
    let block = titled_block(" EXCHANGE ", AMBER_DIM);
    let inner = block.inner(area);
    frame.render_widget(&block, area);

    let (Some(fleet), Some(panel)) = (state.active_fleet(), state.exchange_panel.as_ref()) else {
        return;
    };
    let held = fleet.cargo.get(panel.give);
    let place = if state.exchange_at(fleet.location) {
        Line::styled("The exchange is here. It trades one resource for another and always keeps a fee.", AMBER_BRIGHT)
    } else {
        let hub = state
            .world
            .iter()
            .flat_map(|w| &w.hub)
            .find(|site| site.services.contains(&HubService::Exchange))
            .map_or("the hub".to_string(), |site| format!("the hub [{},{}]", site.location.q, site.location.r));
        Line::styled(
            format!("The exchange is at {hub}: a fleet trades what is in its hold, on the spot. This quote is for planning."),
            AMBER_BRIGHT,
        )
    };
    let mut lines = vec![
        Line::styled(
            format!(
                "Fleet {} at [{},{}]  hold {:.0}/{:.0}: {:.0} metal, {:.0} crystal, {:.0} deuterium",
                fleet.id, fleet.location.q, fleet.location.r, fleet.cargo.total(), fleet.cargo_capacity,
                fleet.cargo.metal, fleet.cargo.crystal, fleet.cargo.deuterium,
            ),
            AMBER_FULL,
        ),
        place,
        Line::styled(
            format!(
                "Par: metal 1, crystal 1.5, deuterium 2 a unit. The fee is {:.0}% of par with an empty meter and rises to {:.0}% as you sell.",
                EXCHANGE_FEE_MIN * 100.0, EXCHANGE_FEE_MAX * 100.0,
            ),
            AMBER_DIM,
        ),
        Line::raw(""),
        Line::styled(format!(" [g] Give     {:<10} {:.0} aboard", panel.give.label().to_uppercase(), held), AMBER_FULL),
        Line::styled(format!(" [w] Want     {}", panel.want.label().to_uppercase()), AMBER_FULL),
        Line::styled(
            format!(" [<-/->]      {}% of the {} aboard", crate::state::HOLD_SHARES[panel.share], panel.give.label()),
            AMBER_FULL,
        ),
        Line::raw(""),
    ];
    match &panel.quote {
        Some(quote) => {
            let t = &quote.terms;
            lines.push(Line::styled(
                format!(
                    " {:.0} {} gives {:.0} {}   (par {:.0}, fee {:.0}%)",
                    t.amount, t.give.label(), t.receive, t.want.label(), t.par, t.fee * 100.0,
                ),
                GREEN_GOOD,
            ));
            lines.push(Line::styled(
                format!(
                    " Meter {:.0} -> {:.0} of {:.0}: the fee on the next unit goes from {:.0}% to {:.0}%",
                    t.meter.pressure, t.meter_after.pressure, t.meter.span, t.meter.fee * 100.0, t.meter_after.fee * 100.0,
                ),
                AMBER,
            ));
            lines.push(Line::styled(
                match t.meter_after.clear_in {
                    Some(clear) => format!(
                        " The meter falls by {:.0} every {}; after this trade it is empty again in {}",
                        t.meter.span, ticks_label(t.meter.window_ticks), ticks_label(clear),
                    ),
                    None => " The meter does not fall: this empire has no mine output".to_string(),
                },
                AMBER,
            ));
            lines.push(Line::raw(""));
            lines.push(match &quote.refusal {
                Some(why) => Line::styled(format!(" Cannot trade now: {why}"), RED_ALERT),
                None => Line::styled(" [Enter] Trade", AMBER_FULL),
            });
        }
        None if held <= 0.0 => {
            lines.push(Line::styled(format!(" Nothing to give: the hold carries no {}.", panel.give.label()), AMBER_DIM));
        }
        None => lines.push(Line::styled(" Asking the exchange...", AMBER_DIM)),
    }
    frame.render_widget(Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: false }), inner);
}

/// "4h16m", "12m05s", "40s".
fn ticks_label(ticks: u64) -> String {
    let (h, m, s) = (ticks / 3600, ticks % 3600 / 60, ticks % 60);
    if h > 0 {
        format!("{h}h{m:02}m")
    } else if m > 0 {
        format!("{m}m{s:02}s")
    } else {
        format!("{s}s")
    }
}

// ── Shared helpers ─────────────────────────────────────────────────

/// Create a bordered block with a title. Callers pass titles already
/// space-padded (e.g. " EVENTS ").
pub fn titled_block(title: &str, border_style: Style) -> Block<'_> {
    Block::new()
        .title(title.to_string())
        .borders(Borders::ALL)
        .border_set(border::ROUNDED)
        .border_style(border_style)
}

/// The group's name, or its fleet id when the server sent none.
fn hostile_name(label: &str, id: u64) -> String {
    if label.is_empty() { format!("fleet {id}") } else { label.to_string() }
}

/// Format a single event into a display string.
pub fn format_event(event: &iac_shared::protocol::GameEvent) -> String {
    use iac_shared::protocol::EventKind;
    match &event.kind {
        EventKind::CombatStarted(e) => {
            let whose = if e.mine { "your".to_string() } else { format!("{}'s", e.owner) };
            format!(
                " T{}: Combat at [{},{}]: {} F{} vs hostile {}\n",
                event.tick, e.sector.q, e.sector.r, whose, e.player_fleet_id, hostile_name(&e.enemy, e.enemy_fleet_id)
            )
        }
        EventKind::CombatEnded(e) => {
            let result = match (e.mine, e.player_victory) {
                (true, true) => "WON",
                (true, false) => "LOST",
                (false, true) => "won by others",
                (false, false) => "lost by others",
            };
            let mut line = format!(" T{}: Combat {} at [{},{}]", event.tick, result, e.sector.q, e.sector.r);
            if let Some(s) = e.salvage {
                line.push_str(&format!(", wreckage {}", res_short(&s)));
            }
            line.push('\n');
            line
        }
        EventKind::SectorEntered(e) => {
            format!(" T{}: Entered [{},{}]\n", event.tick, e.sector.q, e.sector.r)
        }
        EventKind::ResourceHarvested(e) => {
            format!(" T{}: F{} harvested {} over {}s\n", event.tick, e.fleet_id, res_short(&e.resources), e.ticks)
        }
        EventKind::ShipDestroyed(e) => {
            let who = e.owner.as_deref().map_or("hostile".to_string(), |o| if e.mine { "your".to_string() } else { format!("{o}'s") });
            format!(" T{}: {} {} destroyed\n", event.tick, who, e.ship_class.label())
        }
        EventKind::CombatRound(e) => {
            format!(" T{}: F{} hit F{} for {:.0} dmg ({:.0} absorbed)\n", event.tick, e.attacker_fleet_id, e.target_fleet_id, e.hull_damage, e.shield_absorbed)
        }
        EventKind::FleetDestroyed(e) => {
            if e.mine {
                format!(" T{}: !! YOUR FLEET F{} DESTROYED at [{},{}] !!\n", event.tick, e.fleet_id, e.sector.q, e.sector.r)
            } else if e.is_npc {
                format!(
                    " T{}: Hostile {} destroyed at [{},{}], wreckage {}\n",
                    event.tick, hostile_name(&e.label, e.fleet_id), e.sector.q, e.sector.r, res_short(&e.salvage)
                )
            } else {
                format!(
                    " T{}: {}'s fleet F{} destroyed at [{},{}]\n",
                    event.tick, e.owner.as_deref().unwrap_or("?"), e.fleet_id, e.sector.q, e.sector.r
                )
            }
        }
        EventKind::BuildingCompleted(e) => {
            format!(" T{}: {} upgraded to Lv{}\n", event.tick, e.building_type.label(), e.new_level)
        }
        EventKind::ResearchCompleted(e) => {
            format!(" T{}: {} {} researched\n", event.tick, e.tech.label(), e.new_level)
        }
        EventKind::ShipBuilt(e) => {
            format!(" T{}: {} built\n", event.tick, e.ship_class.label())
        }
        EventKind::SalvageCollected(e) => {
            let mut line = format!(" T{}: F{} salvaged {}", event.tick, e.fleet_id, res_short(&e.resources));
            if let Some(left) = e.remaining {
                line.push_str(&format!(", {} left behind", res_short(&left)));
            }
            line.push('\n');
            line
        }
        EventKind::SalvageDespawned(e) => {
            format!(" T{}: Wreckage at [{},{}] drifted away ({})\n", event.tick, e.sector.q, e.sector.r, res_short(&e.resources))
        }
        EventKind::FleetArrived(e) => {
            format!(" T{}: Fleet arrived at [{},{}]\n", event.tick, e.sector.q, e.sector.r)
        }
        EventKind::ScanCompleted(e) => {
            let worst = e.threats.iter().map(|t| t.threat.rating).max().unwrap_or(1);
            let mut line = format!(
                " T{}: Scan: {} sectors, {} hostile, worst T{}\n",
                event.tick, e.sectors_revealed, e.hostiles_detected, worst
            );
            for s in &e.signals {
                line.push_str(&format!(
                    "   ~ {} at [{},{}] (threat band {})\n",
                    s.signal.label(), s.sector.q, s.sector.r, s.threat_band
                ));
            }
            line
        }
        EventKind::DefenceBuilt(e) => {
            format!(" T{}: {} online\n", event.tick, e.defence.label())
        }
        EventKind::Queue(e) => {
            use iac_shared::protocol::{QueueAction, WaitReason};
            let line = match e.action {
                QueueAction::Started if e.paid.total() > 0.0 => {
                    format!("{} started, paid {}", e.item, res_short(&e.paid))
                }
                QueueAction::Started => format!("{} started", e.item),
                QueueAction::Waiting => {
                    let why = match e.waiting_on {
                        Some(WaitReason::Resources) => e.waiting_for.map_or("waiting for resources".to_string(), |r| format!("short {}", res_short(&r))),
                        Some(WaitReason::Slot) => "waiting for a free slot".to_string(),
                        Some(WaitReason::Prerequisite) => "waiting for a prerequisite".to_string(),
                        Some(WaitReason::Order) => "waiting behind an earlier order".to_string(),
                        None => "waiting".to_string(),
                    };
                    let queued = if e.paid.total() > 0.0 {
                        format!("{} ordered, paid {}", e.item, res_short(&e.paid))
                    } else {
                        format!("{} queued", e.item)
                    };
                    match e.start_in {
                        Some(t) => format!("{queued}, {why}, starts in about {t}s"),
                        None => format!("{queued}, {why}"),
                    }
                }
                QueueAction::Cancelled => {
                    if e.refunded.total() <= 0.0 {
                        format!("{} cancelled (nothing had been paid)", e.item)
                    } else {
                        format!("{} cancelled, refunded {}", e.item, res_short(&e.refunded))
                    }
                }
            };
            format!(" T{}: {}\n", event.tick, line)
        }
        EventKind::RaidIncoming(e) => {
            let bound = e.outpost.map_or_else(String::new, |at| format!(" on your outpost {at}"));
            format!(
                " T{}: !! {} RAID inbound{bound} (power {:.0}), ETA T{} !!\n",
                event.tick, e.threat.to_uppercase(), e.est_power, e.arrival_tick
            )
        }
        EventKind::RaidResolved(e) if e.outpost.is_some() => {
            let at = e.outpost.map_or_else(String::new, |at| at.to_string());
            if e.defended {
                format!(" T{}: Raid on your outpost {at} REPELLED ({:.0} vs {:.0})\n", event.tick, e.raid_power, e.defense_power)
            } else {
                format!(
                    " T{}: Raid broke your outpost {at} ({:.0} vs {:.0}) — {:.0} resources and {} ship(s) lost{}\n",
                    event.tick, e.raid_power, e.defense_power, e.resources_lost.total(), e.ships_lost,
                    if e.breached { ", BREACHED until repaired" } else { "" },
                )
            }
        }
        EventKind::OutpostCompleted(e) => {
            format!(" T{}: Outpost at {} finished level {}\n", event.tick, e.sector, e.level)
        }
        EventKind::RaidResolved(e) => {
            let structures = if e.structures_lost > 0 {
                format!(", {} structure(s) lost, {} will be rebuilt", e.structures_lost, e.structures_restored)
            } else {
                String::new()
            };
            if e.defended {
                format!(" T{}: Raid REPELLED ({:.0} vs {:.0}) — salvage in orbit{}\n", event.tick, e.raid_power, e.defense_power, structures)
            } else {
                let lost = e.resources_lost.metal + e.resources_lost.crystal + e.resources_lost.deuterium;
                format!(
                    " T{}: Raid breached defenses ({:.0} vs {:.0}) — {:.0} resources lost{}{}\n",
                    event.tick, e.raid_power, e.defense_power, lost, structures,
                    if e.ships_lost > 0 { format!(", {} docked ship(s) destroyed", e.ships_lost) } else { String::new() }
                )
            }
        }
        EventKind::SiteExplorationStarted(e) => {
            format!(
                " T{}: Boarding tier-{} derelict at [{},{}], done T{}\n",
                event.tick, e.tier, e.sector.q, e.sector.r, e.end_tick
            )
        }
        EventKind::SiteExplored(e) => {
            let mut line = format!(
                " T{}: Derelict stripped: +{:.0}M +{:.0}C +{:.0}D\n",
                event.tick, e.resources.metal, e.resources.crystal, e.resources.deuterium
            );
            if let Some(class) = e.recovered_ship {
                line.push_str(&format!("   + recovered a {} (battered)\n", class.label()));
            }
            if let Some(tech) = e.tech_cache {
                line.push_str(&format!("   + data core: {} +1\n", tech.label()));
            }
            if e.relic {
                line.push_str("   + ANCIENT RELIC\n");
            }
            if e.points > 0.0 {
                line.push_str(&format!("   + {:.2} explore points\n", e.points));
            }
            line
        }
        EventKind::ChartDelivered(e) => {
            format!(" T{}: Chart delivered: {} sectors, +{:.2} explore points\n", event.tick, e.sectors, e.points)
        }
        EventKind::Exchanged(e) => {
            let t = &e.terms;
            format!(
                " T{}: F{} traded {:.0} {} for {:.0} {} at the exchange (fee {:.0}%, meter {:.0} of {:.0})\n",
                event.tick, t.fleet_id, t.amount, t.give.label(), t.receive, t.want.label(), t.fee * 100.0,
                t.meter_after.pressure, t.meter_after.span,
            )
        }
        EventKind::SiteAmbush(e) => {
            format!(
                " T{}: !! AMBUSH — the wreck was bait! [{},{}] !!\n",
                event.tick, e.sector.q, e.sector.r
            )
        }
        EventKind::StorageNearCap(e) => {
            let eta = e.full_in_s.map(|s| format!(", full in {s}s")).unwrap_or_default();
            format!(" T{}: {} storage at {:.0}%{}\n", event.tick, e.resource.label(), e.ratio * 100.0, eta)
        }
        EventKind::StorageFull(e) => {
            format!(" T{}: {} storage FULL: production is being wasted\n", event.tick, e.resource.label())
        }
        EventKind::PolicyAction(e) => {
            format!(
                " T{}: F{} auto-{}: {}\n",
                event.tick, e.fleet_id, e.action, e.reason
            )
        }
        EventKind::Alert(e) => {
            let level = match e.level {
                iac_shared::protocol::AlertLevel::Info => "info",
                iac_shared::protocol::AlertLevel::Warning => "warning",
                iac_shared::protocol::AlertLevel::Critical => "critical",
            };
            format!(" T{}: [{}] {}\n", event.tick, level, e.message)
        }
    }
}

/// Style for an event line: danger red, gain green, intel cyan,
/// everything else stays amber.
pub fn event_style(event: &iac_shared::protocol::GameEvent) -> Style {
    use iac_shared::protocol::EventKind;
    match &event.kind {
        EventKind::CombatStarted(e) => if e.mine { RED_ALERT } else { AMBER_DIM },
        EventKind::CombatRound(e) => if e.mine { RED_ALERT } else { AMBER_DIM },
        EventKind::RaidIncoming(_) => RED_ALERT,
        EventKind::ShipDestroyed(e) if e.mine => RED_ALERT,
        EventKind::FleetDestroyed(e) if e.mine => RED_ALERT,
        EventKind::ShipDestroyed(e) if e.is_npc => GREEN_GOOD,
        EventKind::FleetDestroyed(e) if e.is_npc => GREEN_GOOD,
        EventKind::ShipDestroyed(_) | EventKind::FleetDestroyed(_) => AMBER_DIM,
        EventKind::CombatEnded(e) => match (e.mine, e.player_victory) {
            (true, true) => GREEN_GOOD,
            (true, false) => RED_ALERT,
            (false, _) => AMBER_DIM,
        },
        EventKind::SalvageDespawned(_) => AMBER,
        EventKind::DefenceBuilt(_) => GREEN_GOOD,
        EventKind::Queue(_) => AMBER,
        EventKind::StorageNearCap(_) => AMBER_FULL,
        EventKind::StorageFull(_) => RED_ALERT,
        EventKind::RaidResolved(e) => if e.defended { GREEN_GOOD } else { RED_ALERT },
        EventKind::ResourceHarvested(_)
        | EventKind::SalvageCollected(_)
        | EventKind::BuildingCompleted(_)
        | EventKind::ResearchCompleted(_)
        | EventKind::OutpostCompleted(_)
        | EventKind::ShipBuilt(_) => GREEN_GOOD,
        EventKind::ScanCompleted(_) => CYAN_INTEL,
        EventKind::SiteExplorationStarted(_) => CYAN_INTEL,
        EventKind::SiteExplored(_) | EventKind::ChartDelivered(_) | EventKind::Exchanged(_) => GREEN_GOOD,
        EventKind::SiteAmbush(_) => RED_ALERT,
        EventKind::PolicyAction(_) => AMBER,
        EventKind::SectorEntered(_) | EventKind::FleetArrived(_) => AMBER_BRIGHT,
        EventKind::Alert(e) => match e.level {
            iac_shared::protocol::AlertLevel::Info => AMBER,
            iac_shared::protocol::AlertLevel::Warning => AMBER_FULL,
            iac_shared::protocol::AlertLevel::Critical => RED_ALERT,
        },
    }
}

/// "60M 15C 9D": the nonzero parts of a resource bundle.
fn res_short(r: &iac_shared::Resources) -> String {
    let parts: Vec<String> = [(r.metal, "M"), (r.crystal, "C"), (r.deuterium, "D")]
        .iter()
        .filter(|(v, _)| *v > 0.0)
        .map(|(v, l)| format!("{v:.0}{l}"))
        .collect();
    if parts.is_empty() { "nothing".to_string() } else { parts.join(" ") }
}

/// Render the recent event log as styled lines, newest first.
/// The newest events as rows for a panel `width` columns wide and
/// `max_lines` tall. An event longer than the panel wraps onto indented
/// rows: the end of a sentence is where it says what something cost.
pub fn event_lines(state: &ClientState, max_lines: usize, width: usize) -> Vec<ratatui::text::Line<'static>> {
    let mut lines = Vec::new();
    let mut i = 0;
    while lines.len() < max_lines {
        let Some(event) = state.event_log.get_recent(i) else { break };
        let style = event_style(event);
        let text = format_event(event);
        // Multi-line events (e.g. scan signals) expand to several rows.
        for part in text.trim_end_matches('\n').split('\n') {
            for row in wrap_rows(part, width) {
                lines.push(ratatui::text::Line::styled(row, style));
            }
        }
        i += 1;
    }
    lines.truncate(max_lines);
    lines
}

fn wrap_rows(text: &str, width: usize) -> Vec<String> {
    let mut rows = Vec::new();
    let mut row = String::new();
    for (i, word) in text.split(' ').enumerate() {
        if !row.trim().is_empty() && row.chars().count() + 1 + word.chars().count() > width {
            rows.push(std::mem::replace(&mut row, format!("      {word}")));
        } else {
            if i > 0 {
                row.push(' ');
            }
            row.push_str(word);
        }
    }
    rows.push(row);
    rows
}

/// Compute queue progress.
pub fn queue_progress(tick: u64, start: u64, end: u64) -> (u64, u64) {
    let elapsed = tick.saturating_sub(start);
    let total = if end > start { end - start } else { 1 };
    (elapsed, total)
}

/// Short label for resource density.
pub fn density_short(d: iac_shared::constants::Density) -> &'static str {
    match d {
        iac_shared::constants::Density::None => "-",
        iac_shared::constants::Density::Sparse => "S",
        iac_shared::constants::Density::Moderate => "M",
        iac_shared::constants::Density::Rich => "R",
        iac_shared::constants::Density::Pristine => "P",
    }
}
