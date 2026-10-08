use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use clap::Parser;
use log::{info, warn};

use iac_shared::constants::{DEFAULT_HOST, DEFAULT_PORT, DEFAULT_WORLD_SEED};

use crate::database::Database;
use crate::engine::GameEngine;
use crate::network::Network;

mod combat;
mod database;
mod engine;
mod network;

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

    let persist_interval: u64 = 30;

    info!("Server ready.");

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                info!("Shutdown signal received, persisting final state...");
                let mut engine = engine.lock().unwrap();
                engine.persist_dirty_state().unwrap_or_else(|e| {
                    warn!("Final persist failed: {}", e);
                });
                info!("Server shutdown complete.");
                break;
            }
            result = run_tick(&network, &engine, persist_interval) => {
                if let Err(e) = result {
                    warn!("Tick error: {}", e);
                }
            }
        }
    }
}

async fn run_tick(
    network: &Network,
    engine: &Arc<Mutex<GameEngine>>,
    persist_interval: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let start = std::time::Instant::now();

    // Process incoming messages from the queue
    network.process_incoming()?;

    // Run game tick
    {
        let mut engine = engine.lock().unwrap();
        engine.tick()?;
    }

    // Broadcast updates to all connected clients
    network.broadcast_updates()?;

    // Persist every N ticks
    {
        let mut engine = engine.lock().unwrap();
        if engine.current_tick().is_multiple_of(persist_interval) {
            engine.persist_dirty_state()?;
        }
    }

    // Sleep to maintain tick rate
    let elapsed = start.elapsed();
    let tick_duration = Duration::from_secs(1);
    if elapsed < tick_duration {
        tokio::time::sleep(tick_duration - elapsed).await;
    } else {
        warn!(
            "Tick {} overran by {}ms",
            engine.lock().unwrap().current_tick(),
            elapsed.as_millis() - 1000
        );
    }

    Ok(())
}
