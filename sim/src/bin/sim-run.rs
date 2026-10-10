// Headless fast runner: create a world at a pace, seat a mix of bots, step
// the real engine as fast as possible, print a balance report.
//
//   cargo run -p iac-sim --release --bin sim-run -- --pace season --days 7
//   cargo run -p iac-sim --release --bin sim-run -- \
//       --pace blitz --hours 1 --seat all:hands-on --seat idle --json
//
// The report goes to stdout; timing goes to stderr.

use std::fmt::Write as _;
use std::time::Instant;

use iac_shared::constants::DEFAULT_WORLD_SEED;
use iac_shared::pace::Pace;
use iac_sim::bot::{Attention, Bot, Personality, Style, DAY, DEFAULT_SESSIONS_PER_DAY, MAX_SESSIONS_PER_DAY};
use iac_sim::headless::{Headless, PlayerSummary, ScoreBreakdown};
use iac_sim::script::IdlePlayer;
use serde_json::json;

const USAGE: &str = "usage: sim-run [--pace <preset|number>] [--seed <n>] [--seat <spec>]... [--sessions <n>]
               [--ticks <n> | --hours <n> | --days <n>] [--every <ticks>] [--ignore-cap] [--json]
  --pace      world pace (default season)
  --seed      world seed (default: the server's default)
  --seat      who plays; repeatable. <personality>:<attention>[:<count>] or idle[:<count>]
                personality  turtle | miner | raider | explorer | mixed | all
                attention    check-in | hybrid | hands-on | all
              default: all:all and idle (every style once, and a player that never acts)
  --sessions  look-ins per day of check-in and hybrid players (default 4, at most 24)
  --ticks/--hours/--days  simulated duration; a tick is one second (default --days 1)
  --every     ticks between score samples (default: a day; a tenth of a run under two days)
  --ignore-cap  bots keep flying for combat and exploration points past the score cap
                (measures what the map offers; by default they stop once the cap is banked)
  --json      print the full record as JSON instead of the tables";

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Idle,
    Bot(Style),
}

impl Kind {
    fn style(self) -> String {
        match self {
            Kind::Idle => "idle".to_string(),
            Kind::Bot(style) => style.label(),
        }
    }
}

struct Args {
    pace: Pace,
    seed: u64,
    seats: Vec<Kind>,
    sessions: u64,
    ticks: u64,
    every: Option<u64>,
    json: bool,
    ignore_cap: bool,
}

fn parse_seat(spec: &str) -> Result<Vec<Kind>, String> {
    let (kind, count) = spec.split_once(':').unwrap_or((spec, "1"));
    if kind == "idle" {
        let count = count.parse::<usize>().map_err(|_| format!("--seat {spec}: '{count}' is not a count"))?;
        return Ok(vec![Kind::Idle; count]);
    }
    let seats = Style::parse_seats(spec).map_err(|e| format!("--seat {e} (or idle[:<count>])"))?;
    Ok(seats.into_iter().map(Kind::Bot).collect())
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        pace: Pace::parse("season")?,
        seed: DEFAULT_WORLD_SEED,
        seats: Vec::new(),
        sessions: DEFAULT_SESSIONS_PER_DAY,
        ticks: DAY,
        every: None,
        json: false,
        ignore_cap: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--help" | "-h" => return Err(String::new()),
            "--json" => {
                args.json = true;
                continue;
            }
            "--ignore-cap" => {
                args.ignore_cap = true;
                continue;
            }
            _ => {}
        }
        let value = it.next().ok_or_else(|| format!("{flag} needs a value"))?;
        let number = |what: &str| value.parse::<u64>().map_err(|_| format!("{what}: '{value}' is not a whole number"));
        match flag.as_str() {
            "--pace" => args.pace = Pace::parse(&value)?,
            "--seed" => args.seed = number("--seed")?,
            "--seat" => args.seats.extend(parse_seat(&value)?),
            "--sessions" => args.sessions = number("--sessions")?,
            "--ticks" => args.ticks = number("--ticks")?,
            "--hours" => args.ticks = number("--hours")? * 3600,
            "--days" => args.ticks = number("--days")? * DAY,
            "--every" => args.every = Some(number("--every")?),
            _ => return Err(format!("unknown flag {flag}")),
        }
    }
    if !(1..=MAX_SESSIONS_PER_DAY).contains(&args.sessions) {
        return Err(format!("--sessions must be 1 to {MAX_SESSIONS_PER_DAY}"));
    }
    if args.seats.is_empty() {
        args.seats = parse_seat("all:all")?;
        args.seats.push(Kind::Idle);
    }
    Ok(args)
}

fn seat_all(world: &mut Headless, args: &Args) -> Result<(), String> {
    let mut seen: Vec<(String, usize)> = Vec::new();
    for &kind in &args.seats {
        let style = kind.style();
        let number = match seen.iter_mut().find(|(s, _)| *s == style) {
            Some((_, n)) => {
                *n += 1;
                *n
            }
            None => {
                seen.push((style.clone(), 0));
                0
            }
        };
        let name = format!("{}-{number}", style.replace('/', "-"));
        match kind {
            Kind::Idle => world.add_player(Box::new(IdlePlayer { label: name }), style)?,
            Kind::Bot(bot_style) => {
                let bot_style = Style { sessions_per_day: args.sessions, ignore_cap: args.ignore_cap, ..bot_style };
                world.add_player(Box::new(Bot::new(name, bot_style)), style)?
            }
        };
    }
    Ok(())
}

fn when(tick: u64) -> String {
    if tick.is_multiple_of(DAY) {
        format!("day {}", tick / DAY)
    } else if tick >= DAY {
        format!("day {:.2}", tick as f64 / DAY as f64)
    } else {
        format!("{} min", tick / 60)
    }
}

fn score_columns(s: &ScoreBreakdown) -> String {
    format!(
        "{:>8.1} {:>7.1} {:>7.1} {:>7.1} {:>8.1} {:>8.1} {:>8.1} {:>8.1}",
        s.econ, s.fleet, s.defence, s.core, s.combat, s.explore, s.extra, s.total
    )
}

const SCORE_HEADER: &str = "    econ   fleet defence    core   combat  explore    extra    total";

struct StyleRow {
    style: String,
    players: usize,
    score: ScoreBreakdown,
    haul: f32,
    raid_salvage: f32,
    ships_lost: f32,
}

fn styles(players: &[PlayerSummary]) -> Vec<StyleRow> {
    let mut rows: Vec<StyleRow> = Vec::new();
    for p in players {
        let at = rows.iter().position(|r| r.style == p.style).unwrap_or_else(|| {
            rows.push(StyleRow {
                style: p.style.clone(),
                players: 0,
                score: ScoreBreakdown::default(),
                haul: 0.0,
                raid_salvage: 0.0,
                ships_lost: 0.0,
            });
            rows.len() - 1
        });
        let row = &mut rows[at];
        row.players += 1;
        let (sum, s) = (&mut row.score, &p.score);
        sum.econ += s.econ;
        sum.fleet += s.fleet;
        sum.defence += s.defence;
        sum.core += s.core;
        sum.combat += s.combat;
        sum.explore += s.explore;
        sum.extra += s.extra;
        sum.total += s.total;
        row.haul += p.field.haul();
        row.raid_salvage += p.field.raid_salvage;
        row.ships_lost += p.field.ships_lost as f32;
    }
    for row in &mut rows {
        let n = row.players as f32;
        let s = &mut row.score;
        for value in [
            &mut s.econ, &mut s.fleet, &mut s.defence, &mut s.core, &mut s.combat, &mut s.explore, &mut s.extra,
            &mut s.total, &mut row.haul, &mut row.raid_salvage, &mut row.ships_lost,
        ] {
            *value /= n;
        }
    }
    rows
}

fn percent(value: f32, of: Option<f32>) -> String {
    match of {
        Some(of) if of > 0.0 => format!("{:>5.0}%", 100.0 * value / of),
        _ => "     -".to_string(),
    }
}

/// Final scores by style, with the two comparisons the balance targets ask
/// for: against the turtle with the same attention, and against the same
/// personality flown hands-on.
fn style_table(players: &[PlayerSummary]) -> String {
    let rows = styles(players);
    let total_of = |style: String| rows.iter().find(|r| r.style == style).map(|r| r.score.total);
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{:<20} {:>2} {SCORE_HEADER} {:>7} {:>7} {:>6} {:>9} {:>11}",
        "style", "n", "haul", "raidsal", "lost", "vs turtle", "vs hands-on"
    );
    for row in &rows {
        let (personality, attention) = row.style.split_once('/').unwrap_or((row.style.as_str(), ""));
        let turtle = total_of(format!("{}/{attention}", Personality::Turtle.label()));
        let hands_on = total_of(format!("{personality}/{}", Attention::HandsOn.label()));
        let _ = writeln!(
            out,
            "{:<20} {:>2} {} {:>7.1} {:>7.1} {:>6.1} {:>9} {:>11}",
            row.style,
            row.players,
            score_columns(&row.score),
            row.haul,
            row.raid_salvage,
            row.ships_lost,
            percent(row.score.total, turtle),
            percent(row.score.total, hands_on),
        );
    }
    out
}

fn sample_table(players: &[PlayerSummary]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{:<22} {:>8} {SCORE_HEADER} {:>7} {:>6} {:>6}", "player", "at", "haul", "charts", "kills");
    for p in players {
        for s in &p.samples {
            let _ = writeln!(
                out,
                "{:<22} {:>8} {} {:>7.1} {:>6} {:>6}",
                p.name,
                when(s.tick),
                score_columns(&s.score),
                s.field.haul(),
                s.field.sectors_charted,
                s.field.kills,
            );
        }
    }
    out
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
    if let Err(msg) = seat_all(&mut world, &args) {
        eprintln!("sim-run: {msg}");
        std::process::exit(2);
    }
    let every = args.every.unwrap_or(if args.ticks >= 2 * DAY { DAY } else { (args.ticks / 10).max(1) });

    let started = Instant::now();
    world.run_sampled(args.ticks, every).expect("simulation failed");
    let wall = started.elapsed().as_secs_f64();
    let tps = args.ticks as f64 / wall;

    let players = world.summaries();
    let hash = format!("{:016x}", world.engine.snapshot().hash());
    if args.json {
        let report = json!({
            "pace": args.pace.value(),
            "seed": args.seed,
            "ticks": args.ticks,
            "sample_every": every,
            "sessions_per_day": args.sessions,
            "snapshot_hash": hash,
            "players": players,
        });
        println!("{}", serde_json::to_string_pretty(&report).expect("report serializes"));
    } else {
        println!(
            "pace x{} seed {} ticks {} ({:.2} days) sessions/day {} snapshot {hash}",
            args.pace.value(),
            args.seed,
            args.ticks,
            args.ticks as f64 / DAY as f64,
            args.sessions,
        );
        println!("score = core + extra; core = econ + fleet + defence; extra = min(combat + explore, 25% of core)");
        println!("haul and raidsal: resources from the field and from repelled raids, in the points they buy\n");
        println!("{}", sample_table(&players));
        println!("{}", style_table(&players));
    }
    let turns: u64 = players.iter().map(|p| p.turns).sum();
    eprintln!(
        "{} players, {} ticks in {:.2} s: {:.0} ticks/s, {} bot turns",
        world.players(),
        args.ticks,
        wall,
        tps,
        turns,
    );
}
