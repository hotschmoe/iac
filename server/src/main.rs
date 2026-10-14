use std::path::PathBuf;
use std::time::{Duration, Instant};

use clap::Parser;
use log::{error, info, warn};
use tokio::time::MissedTickBehavior;

use iac_shared::constants::{DEFAULT_HOST, DEFAULT_PORT, DEFAULT_WORLD_SEED};
use iac_shared::pace::Pace;
use iac_shared::protocol::GameEvent;

use iac_sim::bot::{Roster, Style, DEFAULT_SESSIONS_PER_DAY, MAX_SESSIONS_PER_DAY};
use iac_sim::engine::AuthError;
use iac_sim::host::{Phase, Tokens, Watch};
use iac_sim::store::{self, Due, Store, Unread};
use iac_sim::{GameEngine, Host, Snapshot};

use iac_server::auth;
use iac_server::network::Network;
use iac_server::store::{Beside, Files, Writer};

/// The world as this server hosts it: accounts log in with tokens minted here.
type World = Host<Tokens<fn() -> String>>;

const TICK_PERIOD: Duration = Duration::from_secs(1);
/// A tick over this is slow enough to be worth a warning (budget is 1000 ms).
const TICK_BUDGET_WARN: Duration = Duration::from_millis(100);
/// How often the log says what a tick costs.
const LOAD_REPORT_TICKS: u32 = 300;
const DEFAULT_PLAYER_SEATS: usize = 4;
/// A killed server comes back as its last document says, so this is the most
/// it loses (`--save-every`). It is nothing a player cannot give again,
/// with one exception, below.
const DEFAULT_SAVE_SECONDS: u64 = 300;
/// A token is shown once and kept by whoever got it: an account the world
/// forgot would leave them holding a password to nothing. So once one is
/// issued the next document does not wait for its period, only this long
/// after the last document, which makes a burst of registrations one
/// document or two.
const REGISTRATION_SAVE: Duration = Duration::from_secs(2);

#[derive(Parser, Debug)]
#[command(name = "iac-server", about = "In Amber Clad - Game Server")]
struct Args {
    /// Port to listen on; 0 takes a free one from the system. The address
    /// bound is printed on stdout as `listening on <address>:<port>`.
    #[arg(short, long, default_value_t = DEFAULT_PORT)]
    port: u16,

    /// Address to bind (use 0.0.0.0 for LAN play)
    #[arg(long, default_value = DEFAULT_HOST)]
    host: String,

    /// Directory with the built Flutter web client to serve over HTTP
    /// (default: clients/web/build/web if it exists)
    #[arg(long)]
    web_dir: Option<PathBuf>,

    /// The file this world is kept in. One that does not exist yet is a
    /// new world. `<file>.lock` beside it is held while the server runs.
    #[arg(short, long, default_value = "world.iac")]
    world: PathBuf,

    /// World seed for a NEW world, in decimal. Fixed once the world exists;
    /// an existing world refuses a different value.
    #[arg(short, long)]
    seed: Option<u64>,

    /// World pace for a NEW world: a multiplier or a preset name
    /// (persistent, fortnight, season, sprint, dev, blitz, test). Fixed once
    /// the world exists; an existing world refuses a different value.
    #[arg(long, value_parser = Pace::parse)]
    pace: Option<Pace>,

    /// Seconds between two writes of the world file. A server that is
    /// killed comes back as the file says, so this is the most it loses; a
    /// server that is stopped (SIGINT, SIGTERM) writes the file and loses
    /// nothing.
    #[arg(long, value_name = "SECONDS", default_value_t = DEFAULT_SAVE_SECONDS, value_parser = clap::value_parser!(u64).range(1..))]
    save_every: u64,

    /// Built-in bots the world should have; repeatable.
    /// <personality>:<attention>[:<count>], personality one of turtle,
    /// miner, raider, explorer, mixed, attention one of check-in, hybrid,
    /// hands-on, either may be `all`. Bots the world already has keep
    /// playing whether listed or not; missing ones are added in the order
    /// listed until --player-seats stops them.
    #[arg(long = "bot", value_name = "SPEC", value_parser = parse_bots)]
    bots: Vec<BotSeats>,

    /// Home seats (a world has 24) that new bots leave free for players: a
    /// bot is added only while more than this many are free. 0 lets bots
    /// fill the world.
    #[arg(long, value_name = "N", default_value_t = DEFAULT_PLAYER_SEATS)]
    player_seats: usize,

    /// Look-ins per day of new check-in and hybrid bots. Past the season's
    /// pace their day is shorter than a day of the clock: at blitz about
    /// 67 minutes
    #[arg(long, default_value_t = DEFAULT_SESSIONS_PER_DAY, value_parser = clap::value_parser!(u64).range(1..=MAX_SESSIONS_PER_DAY))]
    bot_sessions: u64,
}

#[derive(Debug, Clone)]
struct BotSeats(Vec<Style>);

fn parse_bots(spec: &str) -> Result<BotSeats, String> {
    Style::parse_seats(spec).map(BotSeats)
}

fn describe_pace(p: Pace) -> String {
    match p.preset() {
        Some(preset) => format!("{} ({})", p.value(), preset.name),
        None => p.value().to_string(),
    }
}

/// The world `--world` names: the one its file holds, or a new one. A world
/// keeps the seed and the pace it was created with, so asking an existing
/// one for another is refused, as is a file this build does not play.
fn open_world(document: Option<Vec<u8>>, args: &Args) -> Result<Snapshot, String> {
    const ELSEWHERE: &str = "start a new world with a new --world path";
    let Some(document) = document else {
        let world = Snapshot::new(args.seed.unwrap_or(DEFAULT_WORLD_SEED), args.pace.unwrap_or_default());
        info!("New world: seed {}, pace {}", world.world_seed, describe_pace(world.meta.pace));
        return Ok(world);
    };
    let (Beside {}, world): (_, Snapshot) = store::decode(&document).map_err(|unread| match unread {
        Unread::Stale(_) => format!("{}; old worlds are not migrated. To play, {ELSEWHERE}", unread.sentence("world file")),
        Unread::Damaged(_) => format!("{}. Nothing was changed; to play, {ELSEWHERE}", unread.sentence("world file")),
    })?;
    if let Some(seed) = args.seed.filter(|seed| *seed != world.world_seed) {
        return Err(format!(
            "this world was created with seed {} and cannot change; refusing --seed {seed}. \
             Omit --seed to keep it, or {ELSEWHERE}",
            world.world_seed,
        ));
    }
    if let Some(pace) = args.pace.filter(|pace| *pace != world.meta.pace) {
        return Err(format!(
            "this world was created at pace {} and cannot change; refusing --pace {}. \
             Omit --pace to keep it, or {ELSEWHERE}",
            describe_pace(world.meta.pace),
            describe_pace(pace),
        ));
    }
    info!("World loaded: {} players, {} fleets, tick {}", world.players.len(), world.fleets.len(), world.tick);
    Ok(world)
}

fn refuse(why: String) -> ! {
    eprintln!("iac-server: {why}");
    std::process::exit(2);
}

/// Copy the world out and hand the copy to the writer. The copy is all the
/// tick pays for a document.
fn store_world(host: &World, writer: &Writer) {
    let started = Instant::now();
    let world = host.engine().snapshot();
    let copied = started.elapsed();
    if copied > TICK_BUDGET_WARN {
        warn!("Copying the world out for its document took {copied:?}");
    }
    writer.hand(world, copied);
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let args = Args::parse();

    info!("====================================================");
    info!("  IN AMBER CLAD - Server v0.1.0");
    info!("====================================================");
    info!("Host:       {}", args.host);
    info!("Port:       {}", args.port);
    info!("Tick rate:  1 Hz");
    info!("World file: {} (written every {} s)", args.world.display(), args.save_every);
    info!("----------------------------------------------------");

    let mut files = Files::open(&args.world).unwrap_or_else(|why| refuse(why));
    let document = files.read().unwrap_or_else(|why| refuse(why));
    let engine = GameEngine::restore(open_world(document, &args).unwrap_or_else(|why| refuse(why)));
    info!("World seed: 0x{:X}", engine.world_gen.world_seed);
    info!("World pace: {}", describe_pace(engine.pace()));
    let mut host: World = Host::new(engine, Tokens(auth::generate_token));
    seat_bots(&mut host, &args);

    let writer = Writer::spawn(files);
    store_world(&host, &writer);
    if let Err(why) = writer.settle() {
        refuse(format!("{why}; a server that cannot store its world does not start"));
    }
    let clock = Instant::now();
    let since_start = || clock.elapsed().as_millis() as u64;
    let mut due = Due::since(since_start());
    let (period, soon) = (args.save_every * 1000, REGISTRATION_SAVE.as_millis() as u64);

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
    let network = Network::init(args.port);
    let bound = network
        .start_listening(&args.host, web_dir.clone())
        .await
        .expect("Failed to start WebSocket server");
    if let Some(d) = &web_dir {
        info!("Web dir:    {} (http://{bound}/)", d.display());
    }
    println!("listening on {bound}");

    // SIGINT (ctrl-c) and SIGTERM (docker stop / systemd) both store the
    // world before the server exits.
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
                info!("Shutdown signal received, storing the world...");
                store_world(&host, &writer);
                match writer.settle() {
                    Ok(()) => info!("Server shutdown complete."),
                    Err(why) => {
                        error!("The world as it stopped was not stored: {why}");
                        std::process::exit(1);
                    }
                }
                break;
            }
            _ = ticker.tick() => {
                let started = Instant::now();
                if let Some(prev) = last_start {
                    log::debug!("inter-tick interval {:?}", started - prev);
                }
                last_start = Some(started);
                match run_tick(&network, &mut host, &mut load, &mut due) {
                    Ok(()) if due.is_due(since_start(), period, soon) => {
                        store_world(&host, &writer);
                        due.stored(since_start());
                    }
                    Ok(()) => {}
                    Err(e) => warn!("Tick error: {}", e),
                }
            }
        }
    }
}

/// The bots `--bot` asks for beyond those the world already has, as far as
/// the seats kept for players allow.
fn seat_bots(host: &mut World, args: &Args) {
    let resumed = host.roster().seats().len();
    let wanted: Vec<Style> = args
        .bots
        .iter()
        .flat_map(|seats| seats.0.iter())
        .map(|&style| Style { sessions_per_day: args.bot_sessions, ..style })
        .collect();
    let (added, refusal) = host.enlist(&wanted, args.player_seats, auth::generate_token);
    let (seated, free) = (host.roster().seats().len(), host.engine().free_home_seats());
    match refusal {
        Some(AuthError::WorldFull) => warn!(
            "--bot asks for {} bots; the world has {seated} and {free} free home seats, of which --player-seats keeps {} for players. Seated {added} new ones and left the rest out",
            wanted.len(),
            args.player_seats,
        ),
        Some(refusal) => warn!("Seated {added} new bots and stopped: {refusal:?}"),
        None => {}
    }
    if seated > 0 {
        info!("Bots:       {seated} ({resumed} resumed, {added} new), {free} home seats free");
    }
}

/// Tick times since the last report.
#[derive(Default)]
struct Load {
    ticks: u32,
    total: Duration,
    bots: Duration,
    worst: Duration,
}

/// When each phase of one tick was done, counted from its start.
struct Laps {
    start: Instant,
    done: [Duration; 4],
}

impl Laps {
    fn of(&self, phase: Phase) -> Duration {
        self.done[phase as usize]
    }
}

impl Watch<Roster> for Laps {
    fn at(&mut self, phase: Phase, _engine: &GameEngine, _cast: &Roster, _events: &[GameEvent]) {
        self.done[phase as usize] = self.start.elapsed();
    }
}

/// One tick: what the sockets brought in, then the host core's step.
/// Everything here is memory only. A token issued on the way in hurries the
/// next document.
fn run_tick(network: &Network, host: &mut World, load: &mut Load, due: &mut Due) -> Result<(), Box<dyn std::error::Error>> {
    let mut laps = Laps { start: Instant::now(), done: [Duration::ZERO; 4] };

    if network.deliver(host) {
        due.hurry();
    }
    let t_incoming = laps.start.elapsed();

    host.step(&mut laps, |session, frame| network.send(session, &frame))?;
    let (t_turns, t_sim) = (laps.of(Phase::Turns), laps.of(Phase::Tick));
    let (t_broadcast, t_done) = (laps.of(Phase::Frames), laps.of(Phase::Observed));

    let t_bots = (t_turns - t_incoming) + (t_done - t_broadcast);
    log::debug!(
        "tick phases: incoming {:?} bots {:?} sim {:?} broadcast {:?}",
        t_incoming,
        t_bots,
        t_sim - t_turns,
        t_broadcast - t_sim,
    );
    if t_done > TICK_BUDGET_WARN {
        warn!(
            "Tick {} took {:?} (incoming {:?}, bots {:?}, sim {:?}, broadcast {:?})",
            host.engine().current_tick(),
            t_done,
            t_incoming,
            t_bots,
            t_sim - t_turns,
            t_broadcast - t_sim
        );
    }

    load.ticks += 1;
    load.total += t_done;
    load.bots += t_bots;
    load.worst = load.worst.max(t_done);
    if load.ticks == LOAD_REPORT_TICKS {
        info!(
            "Tick {}: last {} ticks took {:?} each on average ({:?} of it the {} bots), {:?} at worst",
            host.engine().current_tick(),
            load.ticks,
            load.total / load.ticks,
            load.bots / load.ticks,
            host.roster().seats().len(),
            load.worst
        );
        *load = Load::default();
    }
    Ok(())
}
