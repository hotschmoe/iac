use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use clap::Parser;
use log::{info, warn};
use tokio::time::MissedTickBehavior;

use iac_shared::constants::{DEFAULT_HOST, DEFAULT_PORT, DEFAULT_WORLD_SEED};
use iac_shared::pace::Pace;

use iac_sim::bot::{Roster, Style, DEFAULT_SESSIONS_PER_DAY, MAX_SESSIONS_PER_DAY};
use iac_sim::GameEngine;

use crate::database::{Database, Persister};
use crate::network::Network;

mod auth;
mod database;
mod network;

const TICK_PERIOD: Duration = Duration::from_secs(1);
/// A tick over this is slow enough to be worth a warning (budget is 1000 ms).
const TICK_BUDGET_WARN: Duration = Duration::from_millis(100);
/// How often the log says what a tick costs.
const LOAD_REPORT_TICKS: u32 = 300;

#[derive(Parser, Debug)]
#[command(name = "iac-server", about = "In Amber Clad - Game Server")]
struct Args {
    /// Port to listen on
    #[arg(short, long, default_value_t = DEFAULT_PORT)]
    port: u16,

    /// Address to bind (use 0.0.0.0 for LAN play)
    #[arg(long, default_value = DEFAULT_HOST)]
    host: String,

    /// Directory with the built Flutter web client to serve over HTTP
    /// (default: clients/web/build/web if it exists)
    #[arg(long)]
    web_dir: Option<PathBuf>,

    /// World seed (hex or decimal)
    #[arg(short, long)]
    seed: Option<u64>,

    /// Database path
    #[arg(short, long, default_value = "iac_world.db")]
    db: String,

    /// World pace for a NEW database: a multiplier or a preset name
    /// (persistent, fortnight, season, sprint, dev, blitz, test). Fixed once
    /// the world exists; an existing database refuses a different value.
    #[arg(long, value_parser = Pace::parse, conflicts_with = "world")]
    pace: Option<Pace>,

    /// Preset name for a new database (same as --pace <preset>)
    #[arg(long, value_parser = parse_preset)]
    world: Option<Pace>,

    /// Built-in bots the world should have; repeatable.
    /// <personality>:<attention>[:<count>], personality one of turtle,
    /// miner, raider, explorer, mixed, attention one of check-in, hybrid,
    /// hands-on, either may be `all`. Bots the world already has keep
    /// playing whether listed or not; missing ones are added while the
    /// world has a free homeworld, so list fewer than it has homes to
    /// leave room for players.
    #[arg(long = "bot", value_name = "SPEC", value_parser = parse_bots)]
    bots: Vec<BotSeats>,

    /// Look-ins per day of new check-in and hybrid bots
    #[arg(long, default_value_t = DEFAULT_SESSIONS_PER_DAY, value_parser = clap::value_parser!(u64).range(1..=MAX_SESSIONS_PER_DAY))]
    bot_sessions: u64,
}

#[derive(Debug, Clone)]
struct BotSeats(Vec<Style>);

fn parse_bots(spec: &str) -> Result<BotSeats, String> {
    Style::parse_seats(spec).map(BotSeats)
}

fn parse_preset(name: &str) -> Result<Pace, String> {
    match iac_shared::pace::preset_named(name) {
        Some(p) => Pace::new(p.pace),
        None => {
            let names: Vec<&str> = iac_shared::pace::PRESETS.iter().map(|p| p.name).collect();
            Err(format!("unknown preset '{name}' ({})", names.join(", ")))
        }
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let args = Args::parse();
    let world_seed = args.seed.unwrap_or(DEFAULT_WORLD_SEED);

    info!("====================================================");
    info!("  IN AMBER CLAD - Server v0.1.0");
    info!("====================================================");
    info!("Host:       {}", args.host);
    info!("Port:       {}", args.port);
    info!("World seed: 0x{:X}", world_seed);
    info!("Tick rate:  1 Hz");
    info!("DB path:    {}", args.db);
    info!("----------------------------------------------------");

    let db = Database::init(&args.db).expect("Failed to initialize database");
    let snapshot = match db.load_snapshot(world_seed, args.pace.or(args.world)) {
        Ok(snapshot) => snapshot,
        Err(e) => {
            eprintln!("iac-server: {e}");
            std::process::exit(2);
        }
    };
    let mut engine = GameEngine::restore(snapshot);
    engine.attach_persist(Box::new(Persister::spawn(db)));
    info!("World pace: {}", database::describe_pace(engine.pace()));
    let mut roster = seat_bots(&mut engine, &args);
    let engine = Arc::new(Mutex::new(engine));

    let web_dir = match args.web_dir.clone() {
        Some(d) if d.join("index.html").is_file() => Some(d),
        Some(d) => {
            warn!("--web-dir {} has no index.html; serving WebSocket only", d.display());
            None
        }
        None => {
            let d = PathBuf::from("clients/web/build/web");
            if d.join("index.html").is_file() {
                Some(d)
            } else {
                info!("No web client build found; WebSocket only. Build it with: scripts/play.sh (or `cd clients/web && flutter build web --release`)");
                None
            }
        }
    };
    if let Some(d) = &web_dir {
        info!("Web dir:    {} (http://{}:{}/)", d.display(), args.host, args.port);
    }

    let network = Network::init(args.port, Arc::clone(&engine));
    network.start_listening(&args.host, web_dir).await.expect("Failed to start WebSocket server");

    // Signal handling: SIGINT (ctrl-c) and SIGTERM (docker stop / systemd)
    // both trigger a final persist before exit.
    let shutdown = async {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = sigterm.recv() => {}
        }
    };
    tokio::pin!(shutdown);

    // A fixed schedule, not "work + sleep", so ticks do not drift. If a tick
    // ever overruns, Delay restarts the 1 s cadence after it rather than
    // firing a burst of catch-up ticks that would flood clients and run the
    // simulation faster than real time.
    let mut ticker = tokio::time::interval(TICK_PERIOD);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut last_start: Option<Instant> = None;
    let mut load = Load::default();

    info!("Server ready.");

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                info!("Shutdown signal received, persisting final state...");
                let mut engine = engine.lock().unwrap();
                engine.checkpoint();
                engine.persist_dirty_state();
                let saved = engine.flush_persistence();
                match saved {
                    Ok(()) => info!("Server shutdown complete."),
                    Err(e) => warn!("Final persist failed: {}", e),
                }
                break;
            }
            _ = ticker.tick() => {
                let started = Instant::now();
                if let Some(prev) = last_start {
                    log::debug!("inter-tick interval {:?}", started - prev);
                }
                last_start = Some(started);
                if let Err(e) = run_tick(&network, &engine, &mut roster, &mut load) {
                    warn!("Tick error: {}", e);
                }
            }
        }
    }
}

/// The world's bots: those it already has, and those `--bot` asks for
/// beyond them. A world with no home left seats as many as it can.
fn seat_bots(engine: &mut GameEngine, args: &Args) -> Roster {
    let mut roster = Roster::resume(engine);
    let resumed = roster.seats().len();
    let wanted: Vec<Style> = args
        .bots
        .iter()
        .flat_map(|seats| seats.0.iter())
        .map(|&style| Style { sessions_per_day: args.bot_sessions, ..style })
        .collect();
    let (added, refusal) = roster.enlist(engine, &wanted, auth::generate_token);
    if let Some(refusal) = refusal {
        warn!(
            "Seated {added} new bots, not all that --bot asks for: {refusal:?} (no free homeworld, or no free name)"
        );
    }
    if !roster.seats().is_empty() {
        info!("Bots:       {} ({resumed} resumed, {added} new)", roster.seats().len());
    }
    roster
}

/// Tick times since the last report.
#[derive(Default)]
struct Load {
    ticks: u32,
    total: Duration,
    bots: Duration,
    worst: Duration,
}

/// One simulation step. Everything here is memory-only; the database write
/// is queued to the persist thread, so the tick never waits on the disk.
fn run_tick(
    network: &Network,
    engine: &Arc<Mutex<GameEngine>>,
    roster: &mut Roster,
    load: &mut Load,
) -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();

    network.process_incoming()?;
    let t_incoming = start.elapsed();

    roster.take_turns(&mut engine.lock().unwrap());
    let t_turns = start.elapsed();

    engine.lock().unwrap().tick()?;
    let t_sim = start.elapsed();

    network.broadcast_updates()?;
    let t_broadcast = start.elapsed();

    roster.observe(&engine.lock().unwrap());
    let t_observe = start.elapsed();

    engine.lock().unwrap().persist_dirty_state();
    let t_persist = start.elapsed();

    let t_bots = (t_turns - t_incoming) + (t_observe - t_broadcast);
    log::debug!(
        "tick phases: incoming {:?} bots {:?} sim {:?} broadcast {:?} persist-handoff {:?}",
        t_incoming,
        t_bots,
        t_sim - t_turns,
        t_broadcast - t_sim,
        t_persist - t_observe
    );
    if t_persist > TICK_BUDGET_WARN {
        warn!(
            "Tick {} took {:?} (incoming {:?}, bots {:?}, sim {:?}, broadcast {:?})",
            engine.lock().unwrap().current_tick(),
            t_persist,
            t_incoming,
            t_bots,
            t_sim - t_turns,
            t_broadcast - t_sim
        );
    }

    load.ticks += 1;
    load.total += t_persist;
    load.bots += t_bots;
    load.worst = load.worst.max(t_persist);
    if load.ticks == LOAD_REPORT_TICKS {
        info!(
            "Tick {}: last {} ticks took {:?} each on average ({:?} of it the {} bots), {:?} at worst",
            engine.lock().unwrap().current_tick(),
            load.ticks,
            load.total / load.ticks,
            load.bots / load.ticks,
            roster.seats().len(),
            load.worst
        );
        *load = Load::default();
    }
    Ok(())
}
