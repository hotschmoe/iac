use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use clap::Parser;
use log::{info, warn};
use tokio::time::MissedTickBehavior;

use iac_shared::constants::{DEFAULT_HOST, DEFAULT_PORT, DEFAULT_WORLD_SEED};

use crate::database::Database;
use crate::engine::GameEngine;
use crate::network::Network;

mod auth;
mod combat;
mod database;
mod engine;
mod intel;
mod network;

const TICK_PERIOD: Duration = Duration::from_secs(1);
/// A tick over this is slow enough to be worth a warning (budget is 1000 ms).
const TICK_BUDGET_WARN: Duration = Duration::from_millis(100);

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
    let engine = GameEngine::init(world_seed, db).expect("Failed to initialize engine");
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

    info!("Server ready.");

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                info!("Shutdown signal received, persisting final state...");
                let mut engine = engine.lock().unwrap();
                engine.checkpoint_known_sectors();
                let saved = engine.persist_dirty_state().and_then(|_| engine.flush_persistence());
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
                if let Err(e) = run_tick(&network, &engine) {
                    warn!("Tick error: {}", e);
                }
            }
        }
    }
}

/// One simulation step. Everything here is memory-only; the database write
/// is queued to the persist thread, so the tick never waits on the disk.
fn run_tick(network: &Network, engine: &Arc<Mutex<GameEngine>>) -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();

    network.process_incoming()?;
    let t_incoming = start.elapsed();

    engine.lock().unwrap().tick()?;
    let t_sim = start.elapsed();

    network.broadcast_updates()?;
    let t_broadcast = start.elapsed();

    engine.lock().unwrap().persist_dirty_state()?;
    let t_persist = start.elapsed();

    log::debug!(
        "tick phases: incoming {:?} sim {:?} broadcast {:?} persist-handoff {:?}",
        t_incoming,
        t_sim - t_incoming,
        t_broadcast - t_sim,
        t_persist - t_broadcast
    );
    if t_persist > TICK_BUDGET_WARN {
        warn!(
            "Tick {} took {:?} (incoming {:?}, sim {:?}, broadcast {:?})",
            engine.lock().unwrap().current_tick(),
            t_persist,
            t_incoming,
            t_sim - t_incoming,
            t_broadcast - t_sim
        );
    }
    Ok(())
}
