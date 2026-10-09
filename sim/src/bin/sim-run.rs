// Headless fast runner: create a world at a pace, seat scripted players, step
// the real engine as fast as possible, print a JSON summary per player.
//
//   cargo run -p iac-sim --release --bin sim-run -- \
//       --pace season --builders 50 --idle 2 --days 7
//
// The summary goes to stdout; timing goes to stderr.

use std::time::Instant;

use iac_shared::constants::DEFAULT_WORLD_SEED;
use iac_shared::pace::Pace;
use iac_sim::headless::Headless;
use iac_sim::script::{CheckInBuilder, IdlePlayer};
use serde_json::json;

const USAGE: &str = "usage: sim-run [--pace <preset|number>] [--seed <n>] [--builders <n>] [--idle <n>]
               (--ticks <n> | --hours <n> | --days <n>)
  --pace      world pace (default season)
  --seed      world seed (default: the server's default)
  --builders  check-in builders: cheapest affordable building every 3600 ticks (default 4)
  --idle      players that never act (default 0)
  --ticks/--hours/--days  simulated duration; a tick is one second (default --days 1)";

struct Args {
    pace: Pace,
    seed: u64,
    builders: usize,
    idle: usize,
    ticks: u64,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        pace: Pace::parse("season")?,
        seed: DEFAULT_WORLD_SEED,
        builders: 4,
        idle: 0,
        ticks: 86_400,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        if flag == "--help" || flag == "-h" {
            return Err(String::new());
        }
        let value = it.next().ok_or_else(|| format!("{flag} needs a value"))?;
        let number = |what: &str| value.parse::<u64>().map_err(|_| format!("{what}: '{value}' is not a whole number"));
        match flag.as_str() {
            "--pace" => args.pace = Pace::parse(&value)?,
            "--seed" => args.seed = number("--seed")?,
            "--builders" => args.builders = number("--builders")? as usize,
            "--idle" => args.idle = number("--idle")? as usize,
            "--ticks" => args.ticks = number("--ticks")?,
            "--hours" => args.ticks = number("--hours")? * 3600,
            "--days" => args.ticks = number("--days")? * 86_400,
            _ => return Err(format!("unknown flag {flag}")),
        }
    }
    Ok(args)
}

#[expect(clippy::disallowed_methods, reason = "the runner measures its own wall-clock speed; the engine never reads the clock")]
fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(msg) => {
            if !msg.is_empty() {
                eprintln!("sim-run: {msg}");
            }
            eprintln!("{USAGE}");
            std::process::exit(if msg.is_empty() { 0 } else { 2 });
        }
    };

    let mut world = Headless::new(args.seed, args.pace);
    for i in 0..args.builders {
        world.add_player(Box::new(CheckInBuilder::new(format!("Builder{i}")))).expect("seat builder");
    }
    for i in 0..args.idle {
        world.add_player(Box::new(IdlePlayer { label: format!("Idle{i}") })).expect("seat idle player");
    }

    let started = Instant::now();
    world.run(args.ticks).expect("simulation failed");
    let wall = started.elapsed().as_secs_f64();
    let tps = args.ticks as f64 / wall;

    let report = json!({
        "pace": args.pace.value(),
        "seed": args.seed,
        "ticks": args.ticks,
        "snapshot_hash": format!("{:016x}", world.engine.snapshot().hash()),
        "players": world.summaries(),
    });
    println!("{}", serde_json::to_string_pretty(&report).expect("report serializes"));
    eprintln!(
        "{} players, {} ticks in {:.2} s: {:.0} ticks/s ({:.0} player-ticks/s)",
        args.builders + args.idle,
        args.ticks,
        wall,
        tps,
        tps * (args.builders + args.idle) as f64,
    );
}
